pub mod local;

use async_trait::async_trait;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RemoteFileMeta {
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
    pub exists: bool,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BackupBackend: Send + Sync {
    async fn copy_file(&self, source: &Path, dest_relative: &Path) -> crate::Result<()>;

    async fn file_meta(&self, dest_relative: &Path) -> crate::Result<RemoteFileMeta>;

    async fn create_dir_all(&self, dest_relative: &Path) -> crate::Result<()>;

    async fn delete_file(&self, dest_relative: &Path) -> crate::Result<()>;

    fn name(&self) -> &str;
}
