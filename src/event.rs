//! Event siklus backup dan listener.

use crate::error::Result;

/// Event yang dipancarkan selama siklus backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupEvent {
    /// Backup mulai dijalankan.
    Starting {
        /// Nama profil backup.
        name: String,
    },
    /// Backup selesai dan arsip tersimpan.
    Completed {
        /// Nama profil backup.
        name: String,
        /// Path arsip pada destinasi.
        path: String,
        /// Ukuran arsip dalam byte.
        size: u64,
    },
    /// Backup gagal.
    Failed {
        /// Nama profil backup.
        name: String,
        /// Pesan kesalahan.
        error: String,
    },
    /// Pembersihan backup lama selesai.
    CleanupCompleted {
        /// Nama profil backup.
        name: String,
        /// Jumlah arsip yang dihapus.
        deleted: usize,
        /// Total byte yang dibebaskan.
        freed_bytes: u64,
    },
}

/// Pendengar event siklus backup.
///
/// Listener dipanggil secara sinkron agar bisa disimpan sebagai
/// `Box<dyn BackupListener>`; pekerjaan berat sebaiknya diteruskan ke kanal
/// atau antrean di dalam listener.
pub trait BackupListener: Send + Sync {
    /// Menangani satu event. Kesalahan menghentikan siklus backup.
    fn handle(&self, event: &BackupEvent) -> Result<()>;
}

/// Listener dari closure sederhana.
///
/// ```ignore
/// use backup::{BackupEvent, FnListener};
///
/// let listener = FnListener::new(|event: &BackupEvent| {
///     println!("{event:?}");
///     Ok(())
/// });
/// ```
pub struct FnListener<F> {
    handler: F,
}

impl<F> FnListener<F> {
    /// Membungkus closure menjadi listener.
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

impl<F> BackupListener for FnListener<F>
where
    F: Fn(&BackupEvent) -> Result<()> + Send + Sync,
{
    fn handle(&self, event: &BackupEvent) -> Result<()> {
        (self.handler)(event)
    }
}
