//! Pemulihan arsip ke direktori tujuan.

use std::fs::File;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;

use crate::destinations::Destination;
use crate::error::{Error, Result};

#[cfg(feature = "encryption")]
use crate::config::EncryptionConfig;

/// Ringkasan hasil pemulihan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// Path relatif berkas yang diekstrak.
    pub files: Vec<String>,
    /// Total byte yang ditulis.
    pub bytes: u64,
}

/// Mengekstrak arsip tar.gz ke direktori tujuan.
///
/// Entri dengan path absolut, komponen `..`, symbolic link, hard link, atau
/// tipe khusus ditolak dengan [`Error::UnsafeArchiveEntry`].
pub fn restore_archive(archive: &[u8], target_dir: &Path) -> Result<RestoreReport> {
    std::fs::create_dir_all(target_dir)?;
    let root = std::path::absolute(target_dir)?;
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = Archive::new(decoder);
    let mut files = Vec::new();
    let mut bytes = 0;

    for entry in archive.entries().map_err(archive_error)? {
        let mut entry = entry.map_err(archive_error)?;
        let kind = entry.header().entry_type();
        let path = entry.path().map_err(archive_error)?.into_owned();

        if kind.is_symlink() || kind.is_hard_link() {
            return Err(Error::UnsafeArchiveEntry(format!(
                "symlink atau hard link ditolak: {}",
                path.display()
            )));
        }

        let destination = safe_path(&root, &path)?;

        if kind.is_dir() {
            std::fs::create_dir_all(&destination)?;
            continue;
        }
        if !kind.is_file() {
            return Err(Error::UnsafeArchiveEntry(format!(
                "tipe entri tidak didukung: {}",
                path.display()
            )));
        }

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&destination)?;
        bytes += std::io::copy(&mut entry, &mut output)?;
        files.push(path.to_string_lossy().replace('\\', "/"));
    }

    Ok(RestoreReport { files, bytes })
}

/// Mengambil arsip dari destinasi lalu memulihkannya ke direktori tujuan.
pub async fn restore_from<D: Destination>(
    destination: &D,
    path: &str,
    target_dir: &Path,
) -> Result<RestoreReport> {
    let archive = destination.get(path).await?;
    let target = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || restore_archive(&archive, &target))
        .await
        .map_err(|error| Error::Archive(format!("tugas blocking gagal: {error}")))?
}

/// Mendekripsi arsip lalu memulihkannya ke direktori tujuan.
#[cfg(feature = "encryption")]
pub fn restore_encrypted(
    archive: &[u8],
    config: &EncryptionConfig,
    target_dir: &Path,
) -> Result<RestoreReport> {
    let archive = crate::tasks::encrypt::decrypt(archive, config)?;
    restore_archive(&archive, target_dir)
}

fn safe_path(root: &Path, path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Error::UnsafeArchiveEntry(format!(
            "path keluar dari direktori tujuan: {}",
            path.display()
        )));
    }
    let destination = root.join(path);
    if !destination.starts_with(root) {
        return Err(Error::UnsafeArchiveEntry(format!(
            "path keluar dari direktori tujuan: {}",
            path.display()
        )));
    }
    Ok(destination)
}

fn archive_error(error: std::io::Error) -> Error {
    Error::Archive(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_directory_entries() {
        let root = Path::new("/tmp/restore-root");
        assert!(safe_path(root, Path::new("../evil")).is_err());
        assert!(safe_path(root, Path::new("/etc/passwd")).is_err());
        assert!(safe_path(root, Path::new("safe/file.txt")).is_ok());
    }

    #[test]
    fn rejects_traversal_inside_tar() {
        // Header tar mentah dipakai karena `tar::Builder` menolak menulis path
        // berisi `..`; arsip jahat bisa saja dibuat tools lain.
        let tar = raw_tar_with_path("../evil.txt", b"evil");
        let archive = gzip(&tar);

        let target = std::env::temp_dir().join(format!("backup-restore-{}", uuid::Uuid::new_v4()));
        let result = restore_archive(&archive, &target);
        let _ = std::fs::remove_dir_all(&target);
        assert!(matches!(result, Err(Error::UnsafeArchiveEntry(_))));
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn raw_tar_with_path(path: &str, data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        header[..path.len()].copy_from_slice(path.as_bytes());
        write_octal(&mut header, 100, 8, 0o644);
        write_octal(&mut header, 108, 8, 0);
        write_octal(&mut header, 116, 8, 0);
        write_octal(&mut header, 124, 12, data.len() as u64);
        write_octal(&mut header, 136, 12, 0);
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].fill(b' ');
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());

        let mut output = Vec::new();
        output.extend_from_slice(&header);
        output.extend_from_slice(data);
        let padding = (512 - data.len() % 512) % 512;
        output.extend(std::iter::repeat_n(0u8, padding));
        output.extend(std::iter::repeat_n(0u8, 1024));
        output
    }

    fn write_octal(header: &mut [u8; 512], offset: usize, width: usize, value: u64) {
        let digits = width - 1;
        let text = format!("{value:0digits$o}");
        header[offset..offset + digits].copy_from_slice(text.as_bytes());
        header[offset + digits] = 0;
    }
}
