pub mod copy_on_change;

use crate::backend::BackupBackend;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

impl FileEvent {
    pub fn path(&self) -> &Path {
        match self {
            FileEvent::Created(p) | FileEvent::Modified(p) | FileEvent::Deleted(p) => p,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupResult {
    pub source: PathBuf,
    pub dest_relative: PathBuf,
    pub action: BackupAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackupAction {
    Copied,
    Deleted,
    Skipped { reason: String },
}

#[async_trait]
pub trait BackupStrategy: Send + Sync {
    async fn handle_event(
        &self,
        event: &FileEvent,
        watch_source_root: &Path,
        backend: &dyn BackupBackend,
    ) -> crate::Result<BackupResult>;

    fn name(&self) -> &str;
}
