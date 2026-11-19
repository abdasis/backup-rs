//! Uji integrasi backup, restore, retensi, monitor, event, dan paritas destinasi.

#![cfg(feature = "memory")]

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use backup::{
    restore_archive, tasks::cleanup, Backup, BackupEvent, BackupMonitor, BackupProfile,
    CleanupConfig, Destination, DestinationEntry, FileSource, FnListener, LocalDestination,
    MemoryDestination, MonitorConfig, MonitorIssue, SourceConfig,
};
use chrono::{Duration, TimeZone, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use uuid::Uuid;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("backup-rs-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn profile(root: &Path, temporary: &Path, destination: &Path) -> BackupProfile {
    BackupProfile::new("app")
        .with_source(
            SourceConfig::new(temporary).with_files(
                FileSource::new()
                    .include(format!("{}/**", root.display()))
                    .with_relative_path(root.to_path_buf()),
            ),
        )
        .with_destination_directory(destination)
}

fn archive_entries(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().unwrap().to_string_lossy().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        map.insert(path, data);
    }
    map
}

fn manifest(bytes: &[u8]) -> backup::Manifest {
    let entries = archive_entries(bytes);
    serde_json::from_slice(entries.get("manifest.json").unwrap()).unwrap()
}

#[tokio::test]
async fn backup_respects_globs_and_relative_path() {
    let temp = TempDir::new();
    let root = temp.path().join("project");
    write_file(&root.join("src/main.rs"), b"fn main() {}");
    write_file(&root.join("README.md"), b"readme");
    write_file(&root.join("skip/secret.txt"), b"secret");

    let destination_directory = temp.path().join("backups");
    let destination = LocalDestination::new(&destination_directory);
    let profile = profile(&root, &temp.path().join("tmp"), &destination_directory).with_source(
        SourceConfig::new(temp.path().join("tmp")).with_files(
            FileSource::new()
                .include(format!("{}/**", root.display()))
                .exclude(format!("{}/skip/**", root.display()))
                .with_relative_path(root.clone()),
        ),
    );

    let result = Backup::new(profile, destination.clone())
        .run()
        .await
        .unwrap();
    assert!(result.path.starts_with("app/"));
    assert!(result.path.ends_with(".tar.gz"));

    let bytes = destination.get(&result.path).await.unwrap();
    let entries = archive_entries(&bytes);
    assert!(entries.contains_key("src/main.rs"));
    assert!(entries.contains_key("README.md"));
    assert!(!entries.contains_key("skip/secret.txt"));

    let manifest = manifest(&bytes);
    assert_eq!(manifest.name, "app");
    assert!(manifest.databases.is_empty());
    assert!(manifest
        .files
        .iter()
        .any(|entry| entry.path == "src/main.rs" && entry.size == 12));
    assert!(!manifest
        .files
        .iter()
        .any(|entry| entry.path.contains("skip")));
    assert!(manifest.created_at <= Utc::now());
}

#[tokio::test]
async fn restore_roundtrip_is_identical() {
    let temp = TempDir::new();
    let root = temp.path().join("project");
    write_file(&root.join("a.txt"), b"alpha");
    write_file(&root.join("nested/b.bin"), &[0u8, 1, 2, 3, 255]);

    let destination = MemoryDestination::new();
    let profile = profile(
        &root,
        &temp.path().join("tmp"),
        &temp.path().join("backups"),
    );
    let result = Backup::new(profile, &destination).run().await.unwrap();
    let bytes = destination.get(&result.path).await.unwrap();

    let restore_directory = temp.path().join("restore");
    let report = restore_archive(&bytes, &restore_directory).unwrap();
    assert_eq!(report.files.len(), 3); // dua berkas + manifest

    assert_eq!(
        std::fs::read(restore_directory.join("a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(restore_directory.join("nested/b.bin")).unwrap(),
        &[0u8, 1, 2, 3, 255]
    );
}

#[tokio::test]
async fn restore_rejects_escaping_entries() {
    let temp = TempDir::new();
    let archive = gzip(&raw_tar_with_path("../evil.txt", b"evil"));
    let target = temp.path().join("restore");

    let result = restore_archive(&archive, &target);
    assert!(matches!(result, Err(backup::Error::UnsafeArchiveEntry(_))));
}

#[tokio::test]
async fn auto_excludes_temporary_and_destination_directories() {
    let temp = TempDir::new();
    let root = temp.path().join("project");
    let temporary = root.join("tmp");
    let destination_directory = root.join("backups");
    write_file(&root.join("keep.txt"), b"keep");
    write_file(&temporary.join("scratch.txt"), b"scratch");
    write_file(&destination_directory.join("old.txt"), b"old");

    let destination = LocalDestination::new(&destination_directory);
    let profile = profile(&root, &temporary, &destination_directory);
    let result = Backup::new(profile, destination.clone())
        .run()
        .await
        .unwrap();
    let bytes = destination.get(&result.path).await.unwrap();
    let entries = archive_entries(&bytes);

    assert!(entries.contains_key("keep.txt"));
    assert!(!entries.keys().any(|path| path.starts_with("tmp/")));
    assert!(!entries.keys().any(|path| path.starts_with("backups/")));
}

#[tokio::test]
async fn retention_keeps_expected_backups() {
    let now = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
    let entries = vec![
        entry("app/newest.tar.gz", 10, now),
        entry("app/yesterday.tar.gz", 10, now - Duration::days(1)),
        entry("app/two-weeks.tar.gz", 10, now - Duration::days(14)),
        entry("app/two-months.tar.gz", 10, now - Duration::days(60)),
        entry("app/two-years.tar.gz", 10, now - Duration::days(730)),
    ];

    let base = CleanupConfig::new()
        .keep_all_backups_for_days(0)
        .keep_daily_backups_for_days(0)
        .keep_weekly_backups_for_weeks(0)
        .keep_monthly_backups_for_months(0)
        .keep_yearly_backups_for_years(0)
        .delete_oldest_backups_when_using_more_megabytes_than(10_000);

    // Hanya backup terbaru disimpan saat semua periode nol.
    let deleted = cleanup::plan_at(&entries, &base, now);
    assert_eq!(deleted.len(), 4);
    assert!(deleted
        .iter()
        .all(|entry| entry.path != "app/newest.tar.gz"));

    // keep_all mencakup semuanya.
    let keep_all = base.clone().keep_all_backups_for_days(800);
    assert!(cleanup::plan_at(&entries, &keep_all, now).is_empty());

    // daily menyimpan satu per hari.
    let daily = base.clone().keep_daily_backups_for_days(2);
    let kept: Vec<_> = kept_paths(&entries, &daily, now);
    assert!(kept.contains(&"app/newest.tar.gz".to_string()));
    assert!(kept.contains(&"app/yesterday.tar.gz".to_string()));
    assert_eq!(kept.len(), 2);

    // weekly menyimpan satu per minggu.
    let weekly = base.clone().keep_weekly_backups_for_weeks(3);
    let kept = kept_paths(&entries, &weekly, now);
    assert!(kept.contains(&"app/newest.tar.gz".to_string()));
    assert!(kept.contains(&"app/two-weeks.tar.gz".to_string()));
    assert_eq!(kept.len(), 3);

    // monthly menyimpan satu per bulan.
    let monthly = base.clone().keep_monthly_backups_for_months(2);
    let kept = kept_paths(&entries, &monthly, now);
    assert!(kept.contains(&"app/two-months.tar.gz".to_string()));
    assert_eq!(kept.len(), 2);

    // yearly menyimpan satu per tahun.
    let yearly = base.clone().keep_yearly_backups_for_years(2);
    let kept = kept_paths(&entries, &yearly, now);
    assert!(kept.contains(&"app/two-years.tar.gz".to_string()));
    assert_eq!(kept.len(), 2);
}

#[tokio::test]
async fn retention_enforces_total_size_limit() {
    let now = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
    let megabyte = 1024 * 1024;
    let entries = vec![
        entry("app/newest.tar.gz", megabyte, now),
        entry("app/middle.tar.gz", 2 * megabyte, now - Duration::days(1)),
        entry("app/oldest.tar.gz", 3 * megabyte, now - Duration::days(2)),
    ];
    let config = CleanupConfig::new()
        .keep_all_backups_for_days(30)
        .delete_oldest_backups_when_using_more_megabytes_than(2);

    let deleted = cleanup::plan_at(&entries, &config, now);
    assert_eq!(deleted.len(), 2);
    assert!(deleted
        .iter()
        .any(|entry| entry.path == "app/oldest.tar.gz"));
    assert!(deleted
        .iter()
        .any(|entry| entry.path == "app/middle.tar.gz"));
    assert!(deleted
        .iter()
        .all(|entry| entry.path != "app/newest.tar.gz"));
}

#[tokio::test]
async fn monitor_reports_health_problems() {
    let destination = MemoryDestination::new();
    let now = Utc::now();

    destination
        .insert_with_time("app/fresh.tar.gz", vec![0; 100], now)
        .unwrap();
    let monitor = BackupMonitor::new(
        "app",
        vec![destination],
        MonitorConfig::new()
            .with_maximum_age_in_days(7)
            .with_maximum_storage_in_megabytes(10),
    );
    let report = monitor.check().await.unwrap();
    assert!(report.healthy);
    assert!(report.issues.is_empty());

    let destination = MemoryDestination::new();
    destination
        .insert_with_time("app/old.tar.gz", vec![0; 100], now - Duration::days(30))
        .unwrap();
    let monitor = BackupMonitor::new(
        "app",
        vec![destination],
        MonitorConfig::new().with_maximum_age_in_days(7),
    );
    let report = monitor.check().await.unwrap();
    assert!(!report.healthy);
    assert!(report
        .issues
        .iter()
        .any(|issue| matches!(issue, MonitorIssue::TooOld { .. })));
    assert!(report.latest_age_in_days.unwrap() >= 30);

    let destination = MemoryDestination::new();
    let megabyte = 1024 * 1024;
    destination
        .insert_with_time("app/one.tar.gz", vec![0; 2 * megabyte], now)
        .unwrap();
    destination
        .insert_with_time("app/two.tar.gz", vec![0; 2 * megabyte], now)
        .unwrap();
    let monitor = BackupMonitor::new(
        "app",
        vec![destination],
        MonitorConfig::new().with_maximum_storage_in_megabytes(1),
    );
    let report = monitor.check().await.unwrap();
    assert!(!report.healthy);
    assert_eq!(report.total_size_in_megabytes, 4);
    assert!(report
        .issues
        .iter()
        .any(|issue| matches!(issue, MonitorIssue::StorageTooLarge { .. })));
}

#[cfg(feature = "database")]
#[tokio::test]
async fn sqlite_dump_is_included_in_archive() {
    use backup::DatabaseConfig;

    let temp = TempDir::new();
    let database_file = temp.path().join("app.sqlite3");
    write_file(&database_file, b"sqlite-bytes");

    let destination = MemoryDestination::new();
    let profile = BackupProfile::new("app")
        .with_source(
            SourceConfig::new(temp.path().join("tmp"))
                .with_database(DatabaseConfig::sqlite(&database_file).with_name("appdb")),
        )
        .with_destination_directory(temp.path().join("backups"));

    let result = Backup::new(profile, &destination).run().await.unwrap();
    let bytes = destination.get(&result.path).await.unwrap();
    let entries = archive_entries(&bytes);

    assert_eq!(
        entries.get("databases/appdb.sqlite").unwrap(),
        b"sqlite-bytes"
    );
    assert_eq!(manifest(&bytes).databases, vec!["appdb".to_string()]);
}

#[cfg(feature = "database")]
#[tokio::test]
async fn missing_database_binary_returns_clear_error() {
    use backup::DatabaseConfig;

    let temp = TempDir::new();
    for database in [
        DatabaseConfig::postgres("app").with_binary("/nonexistent/pg_dump"),
        DatabaseConfig::mysql("app").with_binary("/nonexistent/mysqldump"),
    ] {
        let destination = MemoryDestination::new();
        let profile = BackupProfile::new("app")
            .with_source(SourceConfig::new(temp.path().join("tmp")).with_database(database))
            .with_destination_directory(temp.path().join("backups"));

        let result = Backup::new(profile, &destination).run().await;
        match result {
            Err(backup::Error::DatabaseDumpFailed(message)) => {
                assert!(message.contains("tidak ditemukan"), "{message}");
            }
            other => panic!("expected DatabaseDumpFailed, got {other:?}"),
        }
    }
}

#[cfg(feature = "encryption")]
#[tokio::test]
async fn encrypted_backup_roundtrip_and_wrong_password() {
    use backup::{restore_encrypted, EncryptionConfig};

    let temp = TempDir::new();
    let root = temp.path().join("project");
    write_file(&root.join("secret.txt"), b"classified");

    let destination = MemoryDestination::new();
    let config = EncryptionConfig::new("correct horse").with_iterations(1_000);
    let profile = profile(
        &root,
        &temp.path().join("tmp"),
        &temp.path().join("backups"),
    )
    .with_encryption(config.clone());

    let result = Backup::new(profile, &destination).run().await.unwrap();
    let bytes = destination.get(&result.path).await.unwrap();
    assert_ne!(&bytes[..2], &[0x1f, 0x8b]);

    let restore_directory = temp.path().join("restore");
    let report = restore_encrypted(&bytes, &config, &restore_directory).unwrap();
    assert_eq!(
        report.files,
        vec!["secret.txt".to_string(), "manifest.json".to_string()]
    );
    assert_eq!(
        std::fs::read(restore_directory.join("secret.txt")).unwrap(),
        b"classified"
    );

    let wrong = EncryptionConfig::new("wrong").with_iterations(1_000);
    let result = restore_encrypted(&bytes, &wrong, &temp.path().join("wrong"));
    assert!(matches!(result, Err(backup::Error::Encryption(_))));
}

#[tokio::test]
async fn events_are_emitted_in_order() {
    let temp = TempDir::new();
    let root = temp.path().join("project");
    write_file(&root.join("a.txt"), b"a");
    let destination = MemoryDestination::new();
    let events = Arc::new(Mutex::new(Vec::new()));

    let recorder = {
        let events = Arc::clone(&events);
        FnListener::new(move |event: &BackupEvent| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        })
    };

    let profile = profile(
        &root,
        &temp.path().join("tmp"),
        &temp.path().join("backups"),
    );
    Backup::new(profile, &destination)
        .with_listener(recorder)
        .run()
        .await
        .unwrap();

    let recorded = events.lock().unwrap().clone();
    assert!(matches!(recorded[0], BackupEvent::Starting { .. }));
    assert!(recorded
        .iter()
        .any(|event| matches!(event, BackupEvent::Completed { .. })));
    assert!(matches!(
        recorded.last().unwrap(),
        BackupEvent::CleanupCompleted { .. }
    ));

    // Kegagalan memancarkan Failed sebagai event terakhir.
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = {
        let events = Arc::clone(&events);
        FnListener::new(move |event: &BackupEvent| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        })
    };
    let invalid = BackupProfile::new("app").with_source(SourceConfig::new(temp.path().join("tmp")));
    let invalid = BackupProfile {
        name: "../escape".to_string(),
        ..invalid
    };
    let result = Backup::new(invalid, &destination)
        .with_listener(recorder)
        .run()
        .await;
    assert!(result.is_err());
    let recorded = events.lock().unwrap().clone();
    assert!(matches!(recorded[0], BackupEvent::Starting { .. }));
    assert!(matches!(
        recorded.last().unwrap(),
        BackupEvent::Failed { .. }
    ));
}

#[tokio::test]
async fn memory_and_local_destinations_behave_the_same() {
    let temp = TempDir::new();
    let root = temp.path().join("project");
    write_file(&root.join("a.txt"), b"alpha");
    write_file(&root.join("nested/b.txt"), b"beta");

    let memory = MemoryDestination::new();
    let local_directory = temp.path().join("local");
    let local = LocalDestination::new(&local_directory);

    let profile = profile(
        &root,
        &temp.path().join("tmp"),
        &temp.path().join("backups"),
    );
    let memory_result = Backup::new(profile.clone(), &memory).run().await.unwrap();
    let local_result = Backup::new(profile, &local).run().await.unwrap();

    assert_eq!(memory_result.manifest.name, local_result.manifest.name);
    assert_eq!(memory_result.manifest.files, local_result.manifest.files);
    assert_eq!(
        memory_result.manifest.databases,
        local_result.manifest.databases
    );
    assert_eq!(memory_result.name, local_result.name);

    let memory_entries = memory.list().await.unwrap();
    let local_entries = local.list().await.unwrap();
    assert_eq!(memory_entries.len(), local_entries.len());
    assert_eq!(memory_entries[0].path, local_entries[0].path);

    let memory_restore = temp.path().join("restore-memory");
    let local_restore = temp.path().join("restore-local");
    restore_archive(
        &memory.get(&memory_result.path).await.unwrap(),
        &memory_restore,
    )
    .unwrap();
    restore_archive(
        &local.get(&local_result.path).await.unwrap(),
        &local_restore,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(memory_restore.join("nested/b.txt")).unwrap(),
        std::fs::read(local_restore.join("nested/b.txt")).unwrap()
    );
}

fn entry(path: &str, size: u64, modified: chrono::DateTime<Utc>) -> DestinationEntry {
    DestinationEntry {
        path: path.to_string(),
        size,
        modified,
    }
}

fn kept_paths(
    entries: &[DestinationEntry],
    config: &CleanupConfig,
    now: chrono::DateTime<Utc>,
) -> Vec<String> {
    let deleted: Vec<String> = cleanup::plan_at(entries, config, now)
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    entries
        .iter()
        .filter(|entry| !deleted.contains(&entry.path))
        .map(|entry| entry.path.clone())
        .collect()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
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
