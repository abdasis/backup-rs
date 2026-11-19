//! Contoh alur backup, monitor, dan restore.
//!
//! Jalankan dengan `cargo run --example basic`.

use backup::{
    restore_archive, Backup, BackupMonitor, BackupProfile, CleanupConfig, Destination, FileSource,
    LocalDestination, MonitorConfig, SourceConfig,
};

#[tokio::main]
async fn main() -> backup::Result<()> {
    let base = std::env::temp_dir().join(format!("backup-rs-example-{}", uuid::Uuid::new_v4()));
    let working = base.join("source");
    let destination_directory = base.join("backups");
    let restore_directory = base.join("restore");
    let temporary_directory = base.join("tmp");

    tokio::fs::create_dir_all(working.join("src")).await?;
    tokio::fs::write(working.join("src/main.rs"), b"fn main() {}").await?;
    tokio::fs::write(working.join("README.md"), b"contoh").await?;

    let profile = BackupProfile::new("app")
        .with_source(
            SourceConfig::new(temporary_directory).with_files(
                FileSource::new()
                    .include(format!("{}/**", working.display()))
                    .exclude(format!("{}/README.md", working.display()))
                    .with_relative_path(working.clone()),
            ),
        )
        .with_destination_directory(&destination_directory)
        .with_cleanup(
            CleanupConfig::new()
                .keep_all_backups_for_days(7)
                .keep_daily_backups_for_days(16),
        );

    let destination = LocalDestination::new(&destination_directory);
    let result = Backup::new(profile, destination.clone()).run().await?;
    println!("arsip: {} ({} byte)", result.path, result.size);

    let archive = destination.get(&result.path).await?;
    let report = restore_archive(&archive, &restore_directory)?;
    println!("dipulihkan: {:?} ({} byte)", report.files, report.bytes);

    let monitor = BackupMonitor::new(
        "app",
        vec![destination],
        MonitorConfig::new()
            .with_maximum_age_in_days(1)
            .with_maximum_storage_in_megabytes(100),
    );
    let report = monitor.check().await?;
    println!("sehat={} issues={:?}", report.healthy, report.issues);

    let _ = tokio::fs::remove_dir_all(&base).await;
    Ok(())
}
