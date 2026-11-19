//! Destinasi in-memory untuk pengujian dan paritas perilaku.

use std::collections::BTreeMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};

use super::{validate_relative, Destination, DestinationEntry};
use crate::error::{Error, Result};

struct MemoryEntry {
    data: Vec<u8>,
    modified: DateTime<Utc>,
}

/// Destinasi yang menyimpan arsip di memori.
#[derive(Default)]
pub struct MemoryDestination {
    inner: RwLock<BTreeMap<String, MemoryEntry>>,
}

impl MemoryDestination {
    /// Destinasi kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Menyisipkan arsip dengan timestamp tertentu, dipakai untuk pengujian
    /// retensi dan monitor.
    pub fn insert_with_time(
        &self,
        path: &str,
        data: Vec<u8>,
        modified: DateTime<Utc>,
    ) -> Result<()> {
        validate_relative(path)?;
        self.write()?
            .insert(path.to_string(), MemoryEntry { data, modified });
        Ok(())
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, BTreeMap<String, MemoryEntry>>> {
        self.inner
            .read()
            .map_err(|_| Error::Destination("memory destination lock poisoned".to_string()))
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, BTreeMap<String, MemoryEntry>>> {
        self.inner
            .write()
            .map_err(|_| Error::Destination("memory destination lock poisoned".to_string()))
    }
}

impl Destination for MemoryDestination {
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<()> {
        validate_relative(path)?;
        self.write()?.insert(
            path.to_string(),
            MemoryEntry {
                data,
                modified: Utc::now(),
            },
        );
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        validate_relative(path)?;
        self.read()?
            .get(path)
            .map(|entry| entry.data.clone())
            .ok_or_else(|| Error::BackupNotFound(path.to_string()))
    }

    async fn list(&self) -> Result<Vec<DestinationEntry>> {
        Ok(self
            .read()?
            .iter()
            .map(|(path, entry)| DestinationEntry {
                path: path.clone(),
                size: entry.data.len() as u64,
                modified: entry.modified,
            })
            .collect())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        validate_relative(path)?;
        self.write()?.remove(path);
        Ok(())
    }

    async fn size(&self) -> Result<u64> {
        Ok(self
            .read()?
            .values()
            .map(|entry| entry.data.len() as u64)
            .sum())
    }
}
