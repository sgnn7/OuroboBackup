use super::{BackupBackend, RemoteFileMeta};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn full_path(&self, relative: &Path) -> crate::Result<PathBuf> {
        // Reject absolute paths and path traversal attempts
        if relative.is_absolute() || relative.components().any(|c| c == std::path::Component::ParentDir) {
            return Err(crate::OuroboError::Backend(
                format!("path traversal denied: {}", relative.display())
            ));
        }
        Ok(self.root.join(relative))
    }
}

#[async_trait]
impl BackupBackend for LocalFsBackend {
    async fn copy_file(&self, source: &Path, dest_relative: &Path) -> crate::Result<()> {
        let dest = self.full_path(dest_relative)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(source, &dest).await?;
        Ok(())
    }

    async fn file_meta(&self, dest_relative: &Path) -> crate::Result<RemoteFileMeta> {
        let dest = self.full_path(dest_relative)?;
        match tokio::fs::metadata(&dest).await {
            Ok(meta) => Ok(RemoteFileMeta {
                size: meta.len(),
                modified: meta.modified().ok().map(chrono::DateTime::from),
                exists: true,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RemoteFileMeta {
                size: 0,
                modified: None,
                exists: false,
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn create_dir_all(&self, dest_relative: &Path) -> crate::Result<()> {
        tokio::fs::create_dir_all(self.full_path(dest_relative)?).await?;
        Ok(())
    }

    async fn delete_file(&self, dest_relative: &Path) -> crate::Result<()> {
        tokio::fs::remove_file(self.full_path(dest_relative)?).await?;
        Ok(())
    }

    fn name(&self) -> &str {
        "local-fs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup() -> (TempDir, TempDir, LocalFsBackend) {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let backend = LocalFsBackend::new(dest_dir.path().to_path_buf());
        (src_dir, dest_dir, backend)
    }

    fn create_source_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[tokio::test]
    async fn test_copy_file_creates_dest() {
        let (src_dir, dest_dir, backend) = setup();
        let src_file = create_source_file(&src_dir, "hello.txt", b"hello world");

        backend
            .copy_file(&src_file, Path::new("hello.txt"))
            .await
            .unwrap();

        let dest_content = std::fs::read(dest_dir.path().join("hello.txt")).unwrap();
        assert_eq!(dest_content, b"hello world");
    }

    #[tokio::test]
    async fn test_copy_file_creates_parent_dirs() {
        let (src_dir, dest_dir, backend) = setup();
        let src_file = create_source_file(&src_dir, "file.txt", b"data");

        backend
            .copy_file(&src_file, Path::new("sub/dir/file.txt"))
            .await
            .unwrap();

        let dest_content = std::fs::read(dest_dir.path().join("sub/dir/file.txt")).unwrap();
        assert_eq!(dest_content, b"data");
    }

    #[tokio::test]
    async fn test_file_meta_existing() {
        let (src_dir, _dest_dir, backend) = setup();
        let src_file = create_source_file(&src_dir, "meta.txt", b"some content");

        backend
            .copy_file(&src_file, Path::new("meta.txt"))
            .await
            .unwrap();

        let meta = backend.file_meta(Path::new("meta.txt")).await.unwrap();
        assert!(meta.exists);
        assert_eq!(meta.size, 12); // "some content" = 12 bytes
        assert!(meta.modified.is_some());
    }

    #[tokio::test]
    async fn test_file_meta_nonexistent() {
        let (_src_dir, _dest_dir, backend) = setup();

        let meta = backend.file_meta(Path::new("nope.txt")).await.unwrap();
        assert!(!meta.exists);
        assert_eq!(meta.size, 0);
        assert!(meta.modified.is_none());
    }

    #[tokio::test]
    async fn test_delete_file() {
        let (src_dir, dest_dir, backend) = setup();
        let src_file = create_source_file(&src_dir, "del.txt", b"delete me");

        backend
            .copy_file(&src_file, Path::new("del.txt"))
            .await
            .unwrap();
        assert!(dest_dir.path().join("del.txt").exists());

        backend.delete_file(Path::new("del.txt")).await.unwrap();
        assert!(!dest_dir.path().join("del.txt").exists());
    }

    #[tokio::test]
    async fn test_create_dir_all() {
        let (_src_dir, dest_dir, backend) = setup();

        backend
            .create_dir_all(Path::new("a/b/c"))
            .await
            .unwrap();
        assert!(dest_dir.path().join("a/b/c").is_dir());
    }
}
