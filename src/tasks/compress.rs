//! Pembuatan arsip tar.gz di memori.

use std::fs::File;

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Builder, Header};

use crate::error::{Error, Result};
use crate::tasks::file_collector::CollectedFile;

/// Nama entri manifest di dalam arsip.
pub const MANIFEST_PATH: &str = "manifest.json";

/// Membuat arsip tar.gz dari daftar berkas, dump database, dan manifest.
///
/// Fungsi ini memblokir; panggil lewat `spawn_blocking` dari konteks async.
pub fn create_archive(
    files: &[CollectedFile],
    databases: &[(String, Vec<u8>)],
    manifest: &[u8],
    modified: u64,
) -> Result<Vec<u8>> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);

    for file in files {
        let mut source = File::open(&file.absolute).map_err(|error| {
            Error::Archive(format!(
                "gagal membuka '{}': {error}",
                file.absolute.display()
            ))
        })?;
        let mut header = Header::new_gnu();
        header.set_size(file.size);
        header.set_mode(0o644);
        header.set_mtime(modified);
        header.set_cksum();
        builder
            .append_data(&mut header, &file.relative, &mut source)
            .map_err(|error| {
                Error::Archive(format!("gagal menambahkan '{}': {error}", file.relative))
            })?;
    }

    for (path, data) in databases {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(modified);
        header.set_cksum();
        builder
            .append_data(&mut header, path, data.as_slice())
            .map_err(|error| Error::Archive(format!("gagal menambahkan '{path}': {error}")))?;
    }

    let mut header = Header::new_gnu();
    header.set_size(manifest.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(modified);
    header.set_cksum();
    builder
        .append_data(&mut header, MANIFEST_PATH, manifest)
        .map_err(|error| Error::Archive(format!("gagal menambahkan manifest: {error}")))?;

    builder
        .finish()
        .map_err(|error| Error::Archive(format!("gagal menutup arsip: {error}")))?;
    let encoder = builder
        .into_inner()
        .map_err(|error| Error::Archive(format!("gagal menutup arsip: {error}")))?;
    encoder
        .finish()
        .map_err(|error| Error::Archive(format!("gagal mengompresi arsip: {error}")))
}
