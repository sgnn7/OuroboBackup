use super::{IpcCommand, IpcResponse};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct IpcClient {
    #[cfg(unix)]
    stream: tokio::net::UnixStream,
}

impl IpcClient {
    pub async fn connect(path: &Path) -> crate::Result<Self> {
        #[cfg(unix)]
        {
            let stream = tokio::net::UnixStream::connect(path)
                .await
                .map_err(|e| crate::OuroboError::Ipc(format!("connect to {}: {}", path.display(), e)))?;
            Ok(Self { stream })
        }
        #[cfg(windows)]
        {
            return Err(crate::OuroboError::Ipc(
                "Windows named pipes are not yet supported".to_string(),
            ))
        }
    }

    pub async fn send(&mut self, cmd: IpcCommand) -> crate::Result<IpcResponse> {
        #[cfg(unix)]
        {
            let mut json = serde_json::to_string(&cmd)?;
            json.push('\n');
            self.stream
                .write_all(json.as_bytes())
                .await
                .map_err(|e| crate::OuroboError::Ipc(format!("write: {e}")))?;

            let mut reader = BufReader::new(&mut self.stream);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| crate::OuroboError::Ipc(format!("read: {e}")))?;

            let response: IpcResponse = serde_json::from_str(line.trim())?;
            Ok(response)
        }
        #[cfg(windows)]
        {
            return Err(crate::OuroboError::Ipc(
                "Windows named pipes are not yet supported".to_string(),
            ))
        }
    }
}
