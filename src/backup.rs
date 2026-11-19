//! Runner siklus backup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{BackupProfile, DatabaseConfig, DatabaseDriver};
use crate::destinations::Destination;
use crate::error::{Error, Result};
use crate::event::{BackupEvent, BackupListener};
#[cfg(feature = "database")]
use crate::tasks::database_dump;
#[cfg(feature = "encryption")]
use crate::tasks::encrypt;
use crate::tasks::{cleanup, compress, file_collector, filename};

/// Manifest yang disimpan di dalam setiap arsip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Nama profil backup.
    pub name: String,
    /// Waktu arsip dibuat.
    pub created_at: DateTime<Utc>,
    /// Daftar berkas yang disertakan.
    pub files: Vec<ManifestEntry>,
    /// Nama dump database yang disertakan.
    pub databases: Vec<String>,
}

/// Satu berkas di dalam manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Path relatif di dalam arsip.
    pub path: String,
    /// Ukuran berkas dalam byte.
    pub size: u64,
}

/// Hasil satu siklus backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupResult {
    /// Nama profil backup.
    pub name: String,
    /// Path arsip pada destinasi.
    pub path: String,
    /// Ukuran arsip dalam byte.
    pub size: u64,
    /// Manifest arsip.
    pub manifest: Manifest,
}

/// Runner backup generik atas satu destinasi.
pub struct Backup<D: Destination> {
    profile: BackupProfile,
    destination: D,
    listeners: Vec<Box<dyn BackupListener>>,
}

impl<D: Destination> Backup<D> {
    /// Membuat runner untuk profil dan destinasi tertentu.
    pub fn new(profile: BackupProfile, destination: D) -> Self {
        Self {
            profile,
            destination,
            listeners: Vec::new(),
        }
    }

    /// Profil yang dipakai runner ini.
    pub fn profile(&self) -> &BackupProfile {
        &self.profile
    }

    /// Menambahkan listener event.
    pub fn with_listener<L: BackupListener + 'static>(mut self, listener: L) -> Self {
        self.listeners.push(Box::new(listener));
        self
    }

    /// Menjalankan backup, menyimpan arsip, lalu membersihkan backup lama.
    ///
    /// Event `Starting`, `Completed`, dan `CleanupCompleted` dipancarkan pada
    /// urutan itu. Kegagalan memancarkan `Failed` sebelum error dikembalikan.
    pub async fn run(&self) -> Result<BackupResult> {
        let name = self.profile.name.clone();
        self.emit(BackupEvent::Starting { name: name.clone() })?;

        match self.execute().await {
            Ok(result) => {
                self.emit(BackupEvent::Completed {
                    name: name.clone(),
                    path: result.path.clone(),
                    size: result.size,
                })?;

                let outcome = self.cleanup().await?;
                self.emit(BackupEvent::CleanupCompleted {
                    name,
                    deleted: outcome.deleted,
                    freed_bytes: outcome.freed_bytes,
                })?;
                Ok(result)
            }
            Err(error) => {
                let _ = self.emit(BackupEvent::Failed {
                    name,
                    error: error.to_string(),
                });
                Err(error)
            }
        }
    }

    async fn execute(&self) -> Result<BackupResult> {
        self.validate()?;
        let created_at = Utc::now();
        let temporary_directory = self
            .profile
            .source
            .temporary_directory
            .join(format!("backup-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&temporary_directory).await?;

        let result = self.build(created_at).await;
        let _ = tokio::fs::remove_dir_all(&temporary_directory).await;
        result
    }

    async fn build(&self, created_at: DateTime<Utc>) -> Result<BackupResult> {
        let excluded = vec![
            self.profile.source.temporary_directory.clone(),
            self.profile.destination.directory.clone(),
        ];
        let files_config = self.profile.source.files.clone();
        let collected =
            tokio::task::spawn_blocking(move || file_collector::collect(&files_config, &excluded))
                .await
                .map_err(blocking_error)??;

        let mut databases: Vec<(String, Vec<u8>)> = Vec::new();
        for database in &self.profile.source.databases {
            let path = dump_path(database);
            let data = self.dump(database).await?;
            databases.push((path, data));
        }

        let manifest = Manifest {
            name: self.profile.name.clone(),
            created_at,
            files: collected
                .iter()
                .map(|file| ManifestEntry {
                    path: file.relative.clone(),
                    size: file.size,
                })
                .collect(),
            databases: self
                .profile
                .source
                .databases
                .iter()
                .map(|database| database.name.clone())
                .collect(),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

        let files = collected;
        let modified = created_at.timestamp().max(0) as u64;
        let archive = tokio::task::spawn_blocking(move || {
            compress::create_archive(&files, &databases, &manifest_bytes, modified)
        })
        .await
        .map_err(blocking_error)??;

        let archive = self.encrypt(archive).await?;
        let path = filename::backup_path(
            &self.profile.name,
            &self.profile.filename_prefix,
            created_at,
        );
        let size = archive.len() as u64;
        self.destination.put(&path, archive).await?;

        Ok(BackupResult {
            name: self.profile.name.clone(),
            path,
            size,
            manifest,
        })
    }

    #[cfg(feature = "database")]
    async fn dump(&self, database: &DatabaseConfig) -> Result<Vec<u8>> {
        database_dump::dump(database).await
    }

    #[cfg(not(feature = "database"))]
    async fn dump(&self, database: &DatabaseConfig) -> Result<Vec<u8>> {
        Err(Error::Config(format!(
            "database '{}': fitur 'database' tidak aktif",
            database.name
        )))
    }

    #[cfg(feature = "encryption")]
    async fn encrypt(&self, archive: Vec<u8>) -> Result<Vec<u8>> {
        let Some(config) = self.profile.encryption.clone() else {
            return Ok(archive);
        };
        tokio::task::spawn_blocking(move || encrypt::encrypt(&archive, &config))
            .await
            .map_err(blocking_error)?
    }

    #[cfg(not(feature = "encryption"))]
    async fn encrypt(&self, archive: Vec<u8>) -> Result<Vec<u8>> {
        Ok(archive)
    }

    async fn cleanup(&self) -> Result<CleanupOutcome> {
        let prefix = format!("{}/", self.profile.name);
        let entries: Vec<_> = self
            .destination
            .list()
            .await?
            .into_iter()
            .filter(|entry| entry.path.starts_with(&prefix) && entry.path.ends_with(".tar.gz"))
            .collect();

        let plan = cleanup::plan(&entries, &self.profile.cleanup);
        let mut freed_bytes = 0;
        for entry in &plan {
            self.destination.delete(&entry.path).await?;
            freed_bytes += entry.size;
        }
        Ok(CleanupOutcome {
            deleted: plan.len(),
            freed_bytes,
        })
    }

    fn emit(&self, event: BackupEvent) -> Result<()> {
        for listener in &self.listeners {
            listener.handle(&event)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let name = &self.profile.name;
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
        {
            return Err(Error::Config(format!("nama backup tidak valid: '{name}'")));
        }
        if self.profile.filename_prefix.contains('/') || self.profile.filename_prefix.contains('\\')
        {
            return Err(Error::Config(
                "filename_prefix tidak boleh memuat pemisah path".to_string(),
            ));
        }
        Ok(())
    }
}

fn dump_path(database: &DatabaseConfig) -> String {
    let extension = match database.driver {
        DatabaseDriver::Sqlite => "sqlite",
        DatabaseDriver::Postgres | DatabaseDriver::Mysql => "sql",
    };
    format!("databases/{}.{}", database.name, extension)
}

fn blocking_error(error: tokio::task::JoinError) -> Error {
    Error::Archive(format!("tugas blocking gagal: {error}"))
}

struct CleanupOutcome {
    deleted: usize,
    freed_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_path_uses_driver_extension() {
        assert_eq!(
            dump_path(&DatabaseConfig::postgres("app").with_name("app")),
            "databases/app.sql"
        );
        assert_eq!(
            dump_path(&DatabaseConfig::sqlite("app.db").with_name("app")),
            "databases/app.sqlite"
        );
    }
}
