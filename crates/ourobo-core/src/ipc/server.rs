use super::{IpcCommand, IpcResponse};
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Maximum IPC message size (1 MB) to prevent memory exhaustion
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;

pub struct IpcServer {
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
}

impl IpcServer {
    pub async fn bind(path: &Path) -> crate::Result<Self> {
        #[cfg(unix)]
        {
            // Remove stale socket if it exists
            let _ = std::fs::remove_file(path);

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| crate::OuroboError::Ipc(format!("create socket dir: {e}")))?;
            }

            let listener = tokio::net::UnixListener::bind(path)
                .map_err(|e| crate::OuroboError::Ipc(format!("bind {}: {}", path.display(), e)))?;
            Ok(Self { listener })
        }
        #[cfg(windows)]
        {
            Err(crate::OuroboError::Ipc(
                "Windows named pipes are not yet supported".to_string(),
            ))
        }
    }

    pub async fn run<F, Fut>(self, handler: F) -> crate::Result<()>
    where
        F: Fn(IpcCommand) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = IpcResponse> + Send,
    {
        let handler = Arc::new(handler);

        #[cfg(unix)]
        loop {
            let (stream, _addr) = self
                .listener
                .accept()
                .await
                .map_err(|e| crate::OuroboError::Ipc(format!("accept: {e}")))?;

            let handler = handler.clone();
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut reader = BufReader::new(reader.take(MAX_MESSAGE_BYTES));
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let cmd: IpcCommand = match serde_json::from_str(line.trim()) {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    let err_resp = IpcResponse::Error {
                                        message: format!("invalid command: {e}"),
                                    };
                                    let mut resp_json = serde_json::to_string(&err_resp).unwrap();
                                    resp_json.push('\n');
                                    if writer.write_all(resp_json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                    continue;
                                }
                            };

                            let response = handler(cmd).await;
                            let mut resp_json = serde_json::to_string(&response).unwrap();
                            resp_json.push('\n');
                            if writer.write_all(resp_json.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("IPC client read error: {e}");
                            break;
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{client::IpcClient, ResponseData};

    #[tokio::test]
    async fn test_ping_pong() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");

        let server = IpcServer::bind(&sock_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let _ = server
                .run(|cmd| async move {
                    match cmd {
                        IpcCommand::Ping => IpcResponse::Ok(ResponseData::Pong),
                        _ => IpcResponse::Error {
                            message: "unexpected".to_string(),
                        },
                    }
                })
                .await;
        });

        // Give server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = IpcClient::connect(&sock_path).await.unwrap();
        let resp = client.send(IpcCommand::Ping).await.unwrap();
        assert_eq!(resp, IpcResponse::Ok(ResponseData::Pong));

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_multiple_commands() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test_multi.sock");

        let server = IpcServer::bind(&sock_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let _ = server
                .run(|cmd| async move {
                    match cmd {
                        IpcCommand::Ping => IpcResponse::Ok(ResponseData::Pong),
                        IpcCommand::Status => IpcResponse::Ok(ResponseData::DaemonStatus(
                            crate::ipc::DaemonStatus {
                                uptime_secs: 100,
                                active_watches: 1,
                                total_files_backed_up: 10,
                                last_error: None,
                            },
                        )),
                        _ => IpcResponse::Error {
                            message: "unhandled".to_string(),
                        },
                    }
                })
                .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = IpcClient::connect(&sock_path).await.unwrap();

        let resp1 = client.send(IpcCommand::Ping).await.unwrap();
        assert_eq!(resp1, IpcResponse::Ok(ResponseData::Pong));

        let resp2 = client.send(IpcCommand::Status).await.unwrap();
        match resp2 {
            IpcResponse::Ok(ResponseData::DaemonStatus(status)) => {
                assert_eq!(status.active_watches, 1);
            }
            _ => panic!("expected DaemonStatus"),
        }

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_concurrent_clients() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test_concurrent.sock");

        let server = IpcServer::bind(&sock_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let _ = server
                .run(|cmd| async move {
                    match cmd {
                        IpcCommand::Ping => IpcResponse::Ok(ResponseData::Pong),
                        _ => IpcResponse::Error {
                            message: "unhandled".to_string(),
                        },
                    }
                })
                .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path1 = sock_path.clone();
        let path2 = sock_path.clone();

        let h1 = tokio::spawn(async move {
            let mut c = IpcClient::connect(&path1).await.unwrap();
            c.send(IpcCommand::Ping).await.unwrap()
        });
        let h2 = tokio::spawn(async move {
            let mut c = IpcClient::connect(&path2).await.unwrap();
            c.send(IpcCommand::Ping).await.unwrap()
        });

        let (r1, r2) = tokio::join!(h1, h2);
        assert_eq!(r1.unwrap(), IpcResponse::Ok(ResponseData::Pong));
        assert_eq!(r2.unwrap(), IpcResponse::Ok(ResponseData::Pong));

        server_handle.abort();
    }
}
