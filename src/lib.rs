//! Library backup async untuk Rust.
//!
//! Crate ini menyusun arsip `.tar.gz` berisi berkas aplikasi dan dump database,
//! menyimpannya ke destinasi lokal atau memori, menerapkan kebijakan retensi,
//! memantau kesehatan backup, dan memulihkan arsip dengan aman.
//!
//! Alur dasar:
//!
//! ```text
//! let profile = BackupProfile::new("app")
//!     .with_source(SourceConfig::new("/tmp/work").with_files(FileSource::new().include("/srv/app/**")))
//!     .with_destination_directory("/var/backups");
//!
//! let result = Backup::new(profile, LocalDestination::new("/var/backups"))
//!     .run()
//!     .await?;
//!
//! let report = restore_archive(&archive_bytes, "/tmp/restore")?;
//! ```
//!
//! Fitur opsional:
//! - `database`: dump PostgreSQL (`pg_dump`), MySQL (`mysqldump`), dan SQLite.
//! - `encryption`: AES-256-GCM dengan kunci PBKDF2-HMAC-SHA256.
//! - `memory`: destinasi in-memory untuk pengujian.
//!
//! Lisensi MIT.

// Native async fn in trait dipakai pada [`destinations::Destination`]. Trait
// ini tidak dipakai sebagai objek `dyn`, jadi tidak perlu crate `async-trait`.
#![allow(async_fn_in_trait)]
#![warn(missing_docs)]

pub mod backup;
pub mod config;
pub mod destinations;
pub mod error;
pub mod event;
pub mod monitor;
pub mod restore;
pub mod tasks;

pub use backup::{Backup, BackupResult, Manifest, ManifestEntry};
#[cfg(feature = "encryption")]
pub use config::EncryptionConfig;
pub use config::{
    BackupProfile, CleanupConfig, DatabaseConfig, DatabaseDriver, DestinationConfig, FileSource,
    MonitorConfig, SourceConfig,
};
#[cfg(feature = "memory")]
pub use destinations::MemoryDestination;
pub use destinations::{Destination, DestinationEntry, LocalDestination};
pub use error::{Error, Result};
pub use event::{BackupEvent, BackupListener, FnListener};
pub use monitor::{BackupMonitor, MonitorIssue, MonitorReport};
#[cfg(feature = "encryption")]
pub use restore::restore_encrypted;
pub use restore::{restore_archive, restore_from, RestoreReport};
