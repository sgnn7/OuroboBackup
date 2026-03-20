use crate::backend::BackupBackend;
use crate::config::WatchConfig;
use crate::strategy::BackupStrategy;
use crate::watcher::FileWatcher;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct WatchStats {
    pub files_backed_up: AtomicU64,
    pub last_backup: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    pub last_error: Mutex<Option<String>>,
}

impl WatchStats {
    fn new() -> Self {
        Self {
            files_backed_up: AtomicU64::new(0),
            last_backup: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }
}

pub struct WatchHandle {
    pub config: WatchConfig,
    pub stats: Arc<WatchStats>,
    cancel: tokio::sync::oneshot::Sender<()>,
}

pub struct BackupEngine {
    watches: HashMap<String, WatchHandle>,
    strategy: Arc<dyn BackupStrategy>,
}

impl BackupEngine {
    pub fn new(strategy: Arc<dyn BackupStrategy>) -> Self {
        Self {
            watches: HashMap::new(),
            strategy,
        }
    }

    pub fn add_watch(
        &mut self,
        config: WatchConfig,
        backend: Arc<dyn BackupBackend>,
        debounce_ms: u64,
    ) -> crate::Result<()> {
        if self.watches.contains_key(&config.id) {
            return Err(crate::OuroboError::DuplicateWatch(config.id.clone()));
        }

        let stats = Arc::new(WatchStats::new());
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

        let source = config.source.clone();
        let strategy = self.strategy.clone();
        let task_stats = stats.clone();

        let (watcher, mut rx) = FileWatcher::start(source.clone(), debounce_ms)?;

        tokio::spawn(async move {
            let _watcher = watcher; // keep alive
            let mut cancel_rx = cancel_rx;

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Some(file_event) => {
                                match strategy.handle_event(&file_event, &source, backend.as_ref()).await {
                                    Ok(result) => {
                                        if result.action == crate::strategy::BackupAction::Copied
                                            || result.action == crate::strategy::BackupAction::Deleted
                                        {
                                            task_stats.files_backed_up.fetch_add(1, Ordering::Relaxed);
                                            *task_stats.last_backup.lock().await = Some(chrono::Utc::now());
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("backup error: {e}");
                                        *task_stats.last_error.lock().await = Some(e.to_string());
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    _ = &mut cancel_rx => {
                        tracing::info!("watch cancelled for {}", source.display());
                        break;
                    }
                }
            }
        });

        self.watches.insert(
            config.id.clone(),
            WatchHandle {
                config,
                stats,
                cancel: cancel_tx,
            },
        );

        Ok(())
    }

    pub fn remove_watch(&mut self, id: &str) -> crate::Result<()> {
        match self.watches.remove(id) {
            Some(handle) => {
                // Dropping cancel_tx signals the task to stop
                let _ = handle.cancel.send(());
                Ok(())
            }
            None => Err(crate::OuroboError::WatchNotFound(id.to_string())),
        }
    }

    pub fn list_watches(&self) -> Vec<(&WatchConfig, &Arc<WatchStats>)> {
        self.watches
            .values()
            .map(|h| (&h.config, &h.stats))
            .collect()
    }

    pub fn has_watch(&self, id: &str) -> bool {
        self.watches.contains_key(id)
    }

    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::local::LocalFsBackend;
    use crate::strategy::copy_on_change::CopyOnChange;

    fn make_engine() -> BackupEngine {
        BackupEngine::new(Arc::new(CopyOnChange))
    }

    fn make_watch_config(id: &str, source: &std::path::Path) -> WatchConfig {
        WatchConfig {
            id: id.to_string(),
            label: format!("Test {id}"),
            source: source.to_path_buf(),
            target: crate::config::TargetConfig::Local {
                path: std::path::PathBuf::from("/tmp/unused"),
            },
            exclude: vec![],
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_add_and_list_watch() {
        let mut engine = make_engine();
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalFsBackend::new(dest_dir.path().to_path_buf()));

        let config = make_watch_config("w1", src_dir.path());
        engine.add_watch(config, backend, 100).unwrap();

        assert_eq!(engine.watch_count(), 1);
        assert!(engine.has_watch("w1"));
    }

    #[tokio::test]
    async fn test_remove_watch() {
        let mut engine = make_engine();
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalFsBackend::new(dest_dir.path().to_path_buf()));

        let config = make_watch_config("w1", src_dir.path());
        engine.add_watch(config, backend, 100).unwrap();
        assert_eq!(engine.watch_count(), 1);

        engine.remove_watch("w1").unwrap();
        assert_eq!(engine.watch_count(), 0);
        assert!(!engine.has_watch("w1"));
    }

    #[tokio::test]
    async fn test_remove_nonexistent_watch_errors() {
        let mut engine = make_engine();
        let result = engine.remove_watch("nope");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_duplicate_id_errors() {
        let mut engine = make_engine();
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalFsBackend::new(dest_dir.path().to_path_buf()));

        let config1 = make_watch_config("dup", src_dir.path());
        engine.add_watch(config1, backend.clone(), 100).unwrap();

        let config2 = make_watch_config("dup", src_dir.path());
        let result = engine.add_watch(config2, backend, 100);
        assert!(result.is_err());
    }
}
