use crate::backend::BackupBackend;
use crate::config::WatchConfig;
use crate::strategy::BackupStrategy;
use crate::watcher::FileWatcher;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

fn build_exclude_set(patterns: &[String]) -> crate::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| {
            crate::OuroboError::Config(format!("invalid exclude pattern \"{pattern}\": {e}"))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| crate::OuroboError::Config(format!("failed to build exclude set: {e}")))
}

fn is_excluded(path: &Path, watch_root: &Path, exclude_set: &GlobSet) -> bool {
    if exclude_set.is_empty() {
        return false;
    }
    // Match against the relative path and the filename
    match path.strip_prefix(watch_root) {
        Ok(relative) => {
            if exclude_set.is_match(relative) {
                return true;
            }
        }
        Err(_) => {
            tracing::debug!(
                "event path {} not under watch root {}, skipping relative exclude check",
                path.display(),
                watch_root.display()
            );
        }
    }
    if let Some(name) = path.file_name() {
        if exclude_set.is_match(name) {
            return true;
        }
    }
    false
}

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
        let exclude_set = build_exclude_set(&config.exclude)?;

        let (watcher, mut rx) = FileWatcher::start(source.clone(), debounce_ms)?;

        tokio::spawn(async move {
            let _watcher = watcher; // keep alive
            let mut cancel_rx = cancel_rx;

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Some(file_event) => {
                                if is_excluded(file_event.path(), &source, &exclude_set) {
                                    tracing::trace!("excluded: {}", file_event.path().display());
                                    continue;
                                }
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
    use std::path::PathBuf;

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

    #[test]
    fn test_build_exclude_set() {
        let set = build_exclude_set(&[
            "*.tmp".to_string(),
            ".DS_Store".to_string(),
        ])
        .unwrap();
        assert!(set.is_match("foo.tmp"));
        assert!(set.is_match(".DS_Store"));
        assert!(!set.is_match("file.txt"));
    }

    #[test]
    fn test_build_exclude_set_empty() {
        let set = build_exclude_set(&[]).unwrap();
        assert!(!set.is_match("anything"));
    }

    #[test]
    fn test_build_exclude_set_invalid_pattern() {
        let result = build_exclude_set(&["[invalid".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_excluded_by_filename() {
        let set = build_exclude_set(&["*.tmp".to_string(), ".DS_Store".to_string()]).unwrap();
        let root = PathBuf::from("/watch");

        assert!(is_excluded(Path::new("/watch/foo.tmp"), &root, &set));
        assert!(is_excluded(Path::new("/watch/sub/.DS_Store"), &root, &set));
        assert!(!is_excluded(Path::new("/watch/file.txt"), &root, &set));
    }

    #[test]
    fn test_is_excluded_by_relative_path() {
        let set = build_exclude_set(&["target/**".to_string()]).unwrap();
        let root = PathBuf::from("/project");

        assert!(is_excluded(
            Path::new("/project/target/debug/bin"),
            &root,
            &set
        ));
        assert!(!is_excluded(Path::new("/project/src/main.rs"), &root, &set));
    }

    #[test]
    fn test_is_excluded_empty_set() {
        let set = build_exclude_set(&[]).unwrap();
        assert!(!is_excluded(
            Path::new("/watch/anything"),
            Path::new("/watch"),
            &set
        ));
    }
}
