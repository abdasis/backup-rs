//! Abstraksi destinasi penyimpanan arsip.

use chrono::{DateTime, Utc};

use crate::error::{Error, Result};

pub mod local;
#[cfg(feature = "memory")]
pub mod memory;

pub use local::LocalDestination;
#[cfg(feature = "memory")]
pub use memory::MemoryDestination;

/// Satu arsip pada destinasi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationEntry {
    /// Path relatif terhadap akar destinasi.
    pub path: String,
    /// Ukuran berkas dalam byte.
    pub size: u64,
    /// Waktu modifikasi terakhir.
    pub modified: DateTime<Utc>,
}

/// Penyimpanan arsip backup.
///
/// Memakai async fn in trait bawaan Rust. Trait ini tidak dipakai sebagai
/// objek `dyn`; komponen backup dan monitor generik atas implementasinya.
pub trait Destination: Send + Sync {
    /// Menyimpan data pada path relatif.
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<()>;

    /// Membaca data pada path relatif.
    async fn get(&self, path: &str) -> Result<Vec<u8>>;

    /// Mendaftar seluruh arsip beserta ukuran dan waktu modifikasi.
    async fn list(&self) -> Result<Vec<DestinationEntry>>;

    /// Menghapus arsip pada path relatif. Tidak error bila tidak ada.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Total ukuran seluruh arsip dalam byte.
    async fn size(&self) -> Result<u64>;
}

impl<D: Destination> Destination for &D {
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<()> {
        (**self).put(path, data).await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        (**self).get(path).await
    }

    async fn list(&self) -> Result<Vec<DestinationEntry>> {
        (**self).list().await
    }

    async fn delete(&self, path: &str) -> Result<()> {
        (**self).delete(path).await
    }

    async fn size(&self) -> Result<u64> {
        (**self).size().await
    }
}

/// Memvalidasi path relatif agar tidak keluar dari akar destinasi.
pub(crate) fn validate_relative(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::Destination("path kosong".to_string()));
    }
    let candidate = std::path::Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(Error::Destination(format!(
            "path tidak boleh absolut atau memuat '..': '{path}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escaping_paths() {
        assert!(validate_relative("app/2026.tar.gz").is_ok());
        assert!(validate_relative("../evil.tar.gz").is_err());
        assert!(validate_relative("/etc/passwd").is_err());
    }
}
