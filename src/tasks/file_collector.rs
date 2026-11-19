//! Pengumpulan berkas sumber sesuai pola include/exclude.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use crate::config::FileSource;
use crate::error::{Error, Result};

/// Satu berkas yang akan masuk ke arsip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedFile {
    /// Path absolut berkas di sistem.
    pub absolute: PathBuf,
    /// Path relatif di dalam arsip.
    pub relative: String,
    /// Ukuran berkas dalam byte.
    pub size: u64,
}

/// Mengumpulkan berkas dari sumber, menghormati include/exclude dan direktori
/// yang dikecualikan.
pub fn collect(source: &FileSource, excluded_dirs: &[PathBuf]) -> Result<Vec<CollectedFile>> {
    let excluded: Vec<PathBuf> = excluded_dirs.iter().map(|path| absolute(path)).collect();
    let naming_root = source.relative_path.as_deref().map(absolute);
    let mut seen = BTreeSet::new();
    let mut collected = Vec::new();

    for pattern in &source.include {
        let (base, relative_pattern) = split_pattern(pattern);
        let base = absolute(&base);
        let include_set = glob_set(std::slice::from_ref(&relative_pattern))?;
        let exclude_set = glob_set(
            &source
                .exclude
                .iter()
                .map(|pattern| strip_base(pattern, &base))
                .collect::<Vec<_>>(),
        )?;

        let entries: Box<dyn Iterator<Item = walkdir::Result<walkdir::DirEntry>>> =
            if base.is_file() {
                Box::new(WalkDir::new(&base).max_depth(0).into_iter())
            } else {
                Box::new(
                    WalkDir::new(&base)
                        .follow_links(source.follow_links)
                        .into_iter(),
                )
            };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_error) if source.ignore_unreadable => continue,
                Err(error) => return Err(Error::Io(std::io::Error::other(error))),
            };
            if !entry.file_type().is_file() {
                continue;
            }

            let path = absolute(entry.path());
            if excluded.iter().any(|directory| path.starts_with(directory)) {
                continue;
            }
            if !seen.insert(path.clone()) {
                continue;
            }

            let relative_to_base = path
                .strip_prefix(&base)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| Path::new(entry.file_name()).to_path_buf())
                .to_string_lossy()
                .replace('\\', "/");
            if !include_set.is_match(&relative_to_base) {
                continue;
            }
            if exclude_set.is_match(&relative_to_base) {
                continue;
            }

            let size = match entry.metadata() {
                Ok(metadata) => metadata.len(),
                Err(_) if source.ignore_unreadable => continue,
                Err(error) => return Err(Error::Io(std::io::Error::other(error))),
            };

            let root = naming_root.as_deref().unwrap_or(&base);
            let relative = path
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| Path::new(entry.file_name()).to_path_buf())
                .to_string_lossy()
                .replace('\\', "/");

            collected.push(CollectedFile {
                absolute: path,
                relative,
                size,
            });
        }
    }

    collected.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(collected)
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Memisahkan pola glob menjadi direktori dasar dan pola relatif terhadapnya.
fn split_pattern(pattern: &str) -> (PathBuf, String) {
    let pattern = pattern.replace('\\', "/");
    let meta = pattern
        .char_indices()
        .find(|(_, character)| matches!(character, '*' | '?' | '[' | '{'))
        .map(|(index, _)| index);

    let Some(meta) = meta else {
        let path = PathBuf::from(&pattern);
        if path.is_file() {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "**".to_string());
            let base = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            return (base, name);
        }
        return (path, "**".to_string());
    };

    let prefix = &pattern[..meta];
    let base = match prefix.rfind('/') {
        Some(index) => &prefix[..index],
        None => ".",
    };
    let relative = pattern[base.len()..].trim_start_matches('/').to_string();
    let relative = if relative.is_empty() {
        "**".to_string()
    } else {
        relative
    };
    (PathBuf::from(base), relative)
}

/// Memotong awalan direktori dasar dari pola exclude agar cocok dengan path
/// relatif hasil penelusuran.
fn strip_base(pattern: &str, base: &Path) -> String {
    let pattern = pattern.replace('\\', "/");
    let base = base.to_string_lossy().replace('\\', "/");
    pattern
        .strip_prefix(&base)
        .map(|rest| rest.trim_start_matches('/').to_string())
        .filter(|rest| !rest.is_empty())
        .unwrap_or(pattern)
}

fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            Error::Config(format!("pola glob tidak valid '{pattern}': {error}"))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|error| Error::Config(format!("pola glob tidak valid: {error}")))
}
