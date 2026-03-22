use anyhow::Result;
use ourobo_core::backend::local::LocalFsBackend;
use ourobo_core::config::{AppConfig, TargetConfig};
use ourobo_core::engine::BackupEngine;
use ourobo_core::ipc::server::IpcServer;
use ourobo_core::ipc::{
    DaemonStatus, IpcCommand, IpcResponse, ResponseData, WatchStatus,
};
use ourobo_core::strategy::copy_on_change::CopyOnChange;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

struct DaemonState {
    engine: BackupEngine,
    config: AppConfig,
    config_path: PathBuf,
}

impl DaemonState {
    fn save_config(&self) -> std::result::Result<(), String> {
        self.config
            .save(&self.config_path)
            .map_err(|e| format!("failed to save config: {e}"))
    }
}

pub async fn run(config: AppConfig, config_path: PathBuf) -> Result<()> {
    let state = Arc::new(Mutex::new(DaemonState {
        engine: BackupEngine::new(Arc::new(CopyOnChange)),
        config,
        config_path,
    }));
    let start_time = std::time::Instant::now();

    // Add configured watches
    {
        let mut st = state.lock().await;
        let watches: Vec<_> = st.config.watches.clone();
        let debounce = st.config.daemon.debounce_ms;
        for watch in &watches {
            if !watch.enabled {
                continue;
            }
            let backend: Arc<dyn ourobo_core::backend::BackupBackend> = match &watch.target {
                TargetConfig::Local { path } => Arc::new(LocalFsBackend::new(path.clone())),
                TargetConfig::Smb { .. } => {
                    tracing::warn!("SMB backend not yet implemented, skipping watch {}", watch.id);
                    continue;
                }
            };
            match st.engine.add_watch(watch.clone(), backend, debounce) {
                Ok(()) => tracing::info!("watching: {} ({})", watch.label, watch.source.display()),
                Err(e) => tracing::error!("failed to add watch {}: {e}", watch.id),
            }
        }
    }

    let ipc_path = state.lock().await.config.daemon.ipc_path.clone();
    let server = IpcServer::bind(&ipc_path).await?;
    tracing::info!("daemon listening on {}", ipc_path.display());

    let state_for_handler = state.clone();
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_signal = shutdown_notify.clone();

    let server_handle = tokio::spawn(async move {
        server
            .run(move |cmd| {
                let state = state_for_handler.clone();
                let uptime = start_time.elapsed().as_secs();
                let shutdown = shutdown_signal.clone();
                async move {
                    match cmd {
                        IpcCommand::Ping => IpcResponse::Ok(ResponseData::Pong),

                        IpcCommand::Status => {
                            let st = state.lock().await;
                            let watches = st.engine.list_watches();
                            let total: u64 = watches
                                .iter()
                                .map(|(_, s)| s.files_backed_up.load(Ordering::Relaxed))
                                .sum();
                            IpcResponse::Ok(ResponseData::DaemonStatus(DaemonStatus {
                                uptime_secs: uptime,
                                active_watches: st.engine.watch_count(),
                                total_files_backed_up: total,
                                last_error: None,
                            }))
                        }

                        IpcCommand::ListWatches => {
                            let st = state.lock().await;
                            let watches: Vec<WatchStatus> = st.engine
                                .list_watches()
                                .into_iter()
                                .map(|(config, stats)| WatchStatus {
                                    config: config.clone(),
                                    files_backed_up: stats.files_backed_up.load(Ordering::Relaxed),
                                    last_backup: None,
                                    last_error: None,
                                    is_watching: true,
                                })
                                .collect();
                            IpcResponse::Ok(ResponseData::WatchList(watches))
                        }

                        IpcCommand::AddWatch(watch_config) => {
                            let mut st = state.lock().await;
                            let id = watch_config.id.clone();
                            let backend: Arc<dyn ourobo_core::backend::BackupBackend> =
                                match &watch_config.target {
                                    TargetConfig::Local { path } => {
                                        Arc::new(LocalFsBackend::new(path.clone()))
                                    }
                                    TargetConfig::Smb { .. } => {
                                        return IpcResponse::Error {
                                            message: "SMB backend not yet implemented".to_string(),
                                        };
                                    }
                                };
                            let debounce = st.config.daemon.debounce_ms;
                            match st.engine.add_watch(watch_config.clone(), backend, debounce) {
                                Ok(()) => {
                                    // Deduplicate: replace if ID already exists (e.g. disabled watch)
                                    st.config.watches.retain(|w| w.id != watch_config.id);
                                    st.config.watches.push(watch_config);
                                    if let Err(e) = st.save_config() {
                                        // Roll back in-memory state
                                        st.config.watches.retain(|w| w.id != id);
                                        let _ = st.engine.remove_watch(&id);
                                        return IpcResponse::Error { message: e };
                                    }
                                    IpcResponse::Ok(ResponseData::WatchAdded { id })
                                }
                                Err(e) => IpcResponse::Error {
                                    message: e.to_string(),
                                },
                            }
                        }

                        IpcCommand::RemoveWatch { id } => {
                            let mut st = state.lock().await;
                            // Save the watch config before removing, for rollback
                            let removed_watch = st.config.watches
                                .iter()
                                .find(|w| w.id == id)
                                .cloned();
                            match st.engine.remove_watch(&id) {
                                Ok(()) => {
                                    st.config.watches.retain(|w| w.id != id);
                                    if let Err(e) = st.save_config() {
                                        // Roll back: re-add to config (engine watch is gone,
                                        // but it will be restored on next daemon restart)
                                        if let Some(watch) = removed_watch {
                                            st.config.watches.push(watch);
                                        }
                                        return IpcResponse::Error { message: e };
                                    }
                                    IpcResponse::Ok(ResponseData::WatchRemoved { id })
                                }
                                Err(e) => IpcResponse::Error {
                                    message: e.to_string(),
                                },
                            }
                        }

                        IpcCommand::SetWatchEnabled { id: _, enabled: _ } => {
                            IpcResponse::Error {
                                message: "enable/disable not yet implemented".to_string(),
                            }
                        }

                        IpcCommand::TriggerBackup { id: _ } => {
                            IpcResponse::Error {
                                message: "manual backup trigger not yet implemented".to_string(),
                            }
                        }

                        IpcCommand::ReloadConfig => {
                            IpcResponse::Error {
                                message: "config reload not yet implemented".to_string(),
                            }
                        }

                        IpcCommand::Shutdown => {
                            tracing::info!("shutdown requested");
                            shutdown.notify_one();
                            IpcResponse::Ok(ResponseData::ShuttingDown)
                        }
                    }
                }
            })
            .await
    });

    // Wait for SIGINT, SIGTERM, or IPC shutdown command
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).map_err(|e| anyhow::anyhow!("SIGTERM handler: {e}"))?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
            _ = shutdown_notify.notified() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = shutdown_notify.notified() => {},
        }
    }
    tracing::info!("received shutdown signal, cleaning up");

    server_handle.abort();
    match server_handle.await {
        Ok(Ok(())) => tracing::debug!("server task finished"),
        Ok(Err(e)) => tracing::error!("server task error: {e}"),
        Err(e) if e.is_cancelled() => tracing::debug!("server task cancelled"),
        Err(e) => tracing::error!("server task panicked: {e}"),
    }

    // Clean up socket file
    match std::fs::remove_file(&ipc_path) {
        Ok(()) => tracing::debug!("removed socket {}", ipc_path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("socket already removed");
        }
        Err(e) => tracing::warn!("failed to remove socket {}: {e}", ipc_path.display()),
    }

    tracing::info!("daemon stopped");
    Ok(())
}
