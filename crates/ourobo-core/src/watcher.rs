use crate::strategy::FileEvent;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct FileWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    pub fn start(
        path: PathBuf,
        debounce_ms: u64,
    ) -> crate::Result<(Self, mpsc::UnboundedReceiver<FileEvent>)> {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut debouncer = new_debouncer(
            Duration::from_millis(debounce_ms),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                match events {
                    Err(e) => {
                        eprintln!("watcher error: {e}");
                    }
                    Ok(events) => {
                        for event in events {
                            let file_event = match event.kind {
                                DebouncedEventKind::Any => {
                                    if event.path.exists() {
                                        if event.path.is_file() {
                                            FileEvent::Modified(event.path)
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        FileEvent::Deleted(event.path)
                                    }
                                }
                                _ => continue,
                            };
                            let _ = tx.send(file_event);
                        }
                    }
                }
            },
        )
        .map_err(|e| crate::OuroboError::Watch {
            path: path.clone(),
            message: e.to_string(),
        })?;

        debouncer
            .watcher()
            .watch(&path, notify::RecursiveMode::Recursive)
            .map_err(|e| crate::OuroboError::Watch {
                path: path.clone(),
                message: e.to_string(),
            })?;

        Ok((Self { _debouncer: debouncer }, rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Canonicalize to handle macOS /var -> /private/var symlink
    fn canon(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().canonicalize().unwrap()
    }

    #[tokio::test]
    async fn test_watcher_detects_file_creation() {
        let dir = tempfile::tempdir().unwrap();
        let base = canon(&dir);
        let (_watcher, mut rx) = FileWatcher::start(base.clone(), 100).unwrap();

        let file_path = base.join("test.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel closed");

        assert_eq!(event.path(), &file_path);
    }

    #[tokio::test]
    async fn test_watcher_detects_file_modification() {
        let dir = tempfile::tempdir().unwrap();
        let base = canon(&dir);
        let file_path = base.join("existing.txt");
        std::fs::write(&file_path, b"original").unwrap();

        let (_watcher, mut rx) = FileWatcher::start(base, 100).unwrap();

        std::fs::write(&file_path, b"modified").unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        match event {
            FileEvent::Modified(p) => assert_eq!(p, file_path),
            other => panic!("expected Modified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_watcher_detects_file_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let base = canon(&dir);
        let file_path = base.join("to_delete.txt");
        std::fs::write(&file_path, b"bye").unwrap();

        let (_watcher, mut rx) = FileWatcher::start(base, 100).unwrap();

        std::fs::remove_file(&file_path).unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        match event {
            FileEvent::Deleted(p) => assert_eq!(p, file_path),
            other => panic!("expected Deleted, got {:?}", other),
        }
    }
}
