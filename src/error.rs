//! Tipe error untuk seluruh operasi crate.

use thiserror::Error;

/// Semua operasi crate ini mengembalikan tipe hasil ini.
pub type Result<T> = std::result::Result<T, Error>;

/// Kesalahan yang mungkin muncul pada operasi backup, pemulihan, dan monitor.
#[derive(Debug, Error)]
pub enum Error {
    /// Kegagalan sistem berkas.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Arsip tar/gzip rusak atau gagal dibuat.
    #[error("archive error: {0}")]
    Archive(String),

    /// Konfigurasi tidak valid atau tidak lengkap.
    #[error("configuration error: {0}")]
    Config(String),

    /// Destinasi gagal menyimpan, membaca, atau menghapus berkas.
    #[error("destination error: {0}")]
    Destination(String),

    /// Dump database gagal, termasuk binary yang tidak tersedia.
    #[error("database dump failed: {0}")]
    DatabaseDumpFailed(String),

    /// Entri arsip keluar dari direktori tujuan pemulihan.
    #[error("unsafe archive entry: {0}")]
    UnsafeArchiveEntry(String),

    /// Enkripsi atau dekripsi gagal.
    #[error("encryption error: {0}")]
    Encryption(String),

    /// Listener event mengembalikan kesalahan.
    #[error("listener error: {0}")]
    Listener(String),

    /// Backup yang diminta tidak ada pada destinasi.
    #[error("backup not found: {0}")]
    BackupNotFound(String),
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Archive(format!("manifest tidak valid: {error}"))
    }
}
