use super::*;
use crate::backend::BackupBackend;
use async_trait::async_trait;
use std::path::Path;

pub struct CopyOnChange;

#[async_trait]
impl BackupStrategy for CopyOnChange {
    async fn handle_event(
        &self,
        event: &FileEvent,
        watch_source_root: &Path,
        backend: &dyn BackupBackend,
    ) -> crate::Result<BackupResult> {
        let path = event.path();
        let relative = path
            .strip_prefix(watch_source_root)
            .map_err(|_| {
                crate::OuroboError::Backend(format!(
                    "{} is not under watch root {}",
                    path.display(),
                    watch_source_root.display()
                ))
            })?
            .to_path_buf();

        match event {
            FileEvent::Created(_) | FileEvent::Modified(_) => {
                backend.copy_file(path, &relative).await?;
                Ok(BackupResult {
                    source: path.to_path_buf(),
                    dest_relative: relative,
                    action: BackupAction::Copied,
                })
            }
            FileEvent::Deleted(_) => {
                backend.delete_file(&relative).await?;
                Ok(BackupResult {
                    source: path.to_path_buf(),
                    dest_relative: relative,
                    action: BackupAction::Deleted,
                })
            }
        }
    }

    fn name(&self) -> &str {
        "copy-on-change"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockBackupBackend;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_modified_file_is_copied() {
        let mut mock = MockBackupBackend::new();
        mock.expect_copy_file()
            .withf(|src, dest| {
                src == Path::new("/watch/root/sub/file.txt")
                    && dest == Path::new("sub/file.txt")
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let strategy = CopyOnChange;
        let event = FileEvent::Modified(PathBuf::from("/watch/root/sub/file.txt"));
        let result = strategy
            .handle_event(&event, Path::new("/watch/root"), &mock)
            .await
            .unwrap();

        assert_eq!(result.action, BackupAction::Copied);
        assert_eq!(result.dest_relative, PathBuf::from("sub/file.txt"));
    }

    #[tokio::test]
    async fn test_created_file_is_copied() {
        let mut mock = MockBackupBackend::new();
        mock.expect_copy_file()
            .times(1)
            .returning(|_, _| Ok(()));

        let strategy = CopyOnChange;
        let event = FileEvent::Created(PathBuf::from("/watch/root/new.txt"));
        let result = strategy
            .handle_event(&event, Path::new("/watch/root"), &mock)
            .await
            .unwrap();

        assert_eq!(result.action, BackupAction::Copied);
        assert_eq!(result.dest_relative, PathBuf::from("new.txt"));
    }

    #[tokio::test]
    async fn test_deleted_file_is_deleted() {
        let mut mock = MockBackupBackend::new();
        mock.expect_delete_file()
            .withf(|dest| dest == Path::new("gone.txt"))
            .times(1)
            .returning(|_| Ok(()));

        let strategy = CopyOnChange;
        let event = FileEvent::Deleted(PathBuf::from("/watch/root/gone.txt"));
        let result = strategy
            .handle_event(&event, Path::new("/watch/root"), &mock)
            .await
            .unwrap();

        assert_eq!(result.action, BackupAction::Deleted);
    }

    #[tokio::test]
    async fn test_relative_path_computed_correctly() {
        let mut mock = MockBackupBackend::new();
        mock.expect_copy_file()
            .withf(|_, dest| dest == Path::new("a/b/c/deep.txt"))
            .times(1)
            .returning(|_, _| Ok(()));

        let strategy = CopyOnChange;
        let event = FileEvent::Modified(PathBuf::from("/root/a/b/c/deep.txt"));
        let result = strategy
            .handle_event(&event, Path::new("/root"), &mock)
            .await
            .unwrap();

        assert_eq!(result.dest_relative, PathBuf::from("a/b/c/deep.txt"));
    }

    #[tokio::test]
    async fn test_path_outside_root_returns_error() {
        let mock = MockBackupBackend::new();
        let strategy = CopyOnChange;
        let event = FileEvent::Modified(PathBuf::from("/other/path/file.txt"));
        let result = strategy
            .handle_event(&event, Path::new("/watch/root"), &mock)
            .await;

        assert!(result.is_err());
    }
}
