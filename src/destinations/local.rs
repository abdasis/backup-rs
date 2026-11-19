//! Destinasi pada sistem berkas lokal.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::{validate_relative, Destination, DestinationEntry};
use crate::error::{Error, Result};

/// Destinasi yang menyimpan arsip pada direktori lokal.
#[derive(Debug, Clone)]
pub struct LocalDestination {
    root: PathBuf,
}

impl LocalDestination {
    /// Destinasi pada direktori tertentu.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Direktori akar destinasi.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str) -> Result<PathBuf> {
        validate_relative(path)?;
        Ok(self.root.join(path))
    }
}

impl Destination for LocalDestination {
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<()> {
        let target = self.resolve(path)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, data).await?;
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let target = self.resolve(path)?;
        match tokio::fs::read(&target).await {
            Ok(data) => Ok(data),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::BackupNotFound(path.to_string()))
            }
            Err(error) => Err(Error::Io(error)),
        }
    }

    async fn list(&self) -> Result<Vec<DestinationEntry>> {
        let mut entries = Vec::new();
        let mut stack = vec![self.root.clone()];

        while let Some(directory) = stack.pop() {
            let mut reader = match tokio::fs::read_dir(&directory).await {
                Ok(reader) => reader,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(Error::Io(error)),
            };
            while let Some(entry) = reader.next_entry().await? {
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    stack.push(entry.path());
                } else if file_type.is_file() {
                    let metadata = entry.metadata().await?;
                    let path = entry
                        .path()
                        .strip_prefix(&self.root)
                        .map_err(|error| Error::Destination(error.to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let modified: DateTime<Utc> = metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                        .into();
                    entries.push(DestinationEntry {
                        path,
                        size: metadata.len(),
                        modified,
                    });
                }
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let target = self.resolve(path)?;
        match tokio::fs::remove_file(&target).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }

    async fn size(&self) -> Result<u64> {
        Ok(self.list().await?.iter().map(|entry| entry.size).sum())
    }
}
