# backup-rs

Library backup async untuk Rust: mengarsipkan berkas aplikasi dan dump
database ke `.tar.gz`, menyimpan ke destinasi lokal atau memori, menerapkan
kebijakan retensi, memantau kesehatan backup, dan memulihkan arsip dengan
aman. Tanpa `unsafe`, tanpa proc-macro, dan tanpa `anyhow` di library.

## Fitur

- Arsip `.tar.gz` berisi berkas sumber, dump database, dan `manifest.json`.
- Pemilihan berkas dengan pola glob include/exclude, `follow_links`,
  `ignore_unreadable`, dan `relative_path`.
- Auto-exclude direktori sementara, direktori destinasi lokal, dan arsip yang
  sedang dibuat.
- Retensi: keep all, daily, weekly, monthly, yearly, serta batas total ukuran.
  Backup terbaru tidak pernah dihapus.
- Monitor umur backup terbaru dan total ukuran penyimpanan.
- Event siklus `Starting`, `Completed`, `Failed`, dan `CleanupCompleted`.
- Restore aman: menolak path absolut, `..`, symlink, dan hard link.
- Destinasi `LocalDestination` dan `MemoryDestination`.
- Enkripsi opsional AES-256-GCM dengan kunci PBKDF2-HMAC-SHA256 (fitur
  `encryption`).
- Dump PostgreSQL (`pg_dump`), MySQL (`mysqldump`), dan SQLite (fitur
  `database`, aktif secara default).

## Instalasi

```toml
[dependencies]
backup-rs = "0.1"
```

Nama paket `backup-rs`, nama library tetap `backup`, sehingga
`use backup::...` dipakai di kode.

Fitur: `database` (default), `memory` (default), `encryption`.

## Pakai cepat

```rust
use backup::{
    restore_archive, Backup, BackupProfile, FileSource, LocalDestination, SourceConfig,
};

# async fn run() -> backup::Result<()> {
let profile = BackupProfile::new("app")
    .with_source(
        SourceConfig::new("/tmp/backup-work").with_files(
            FileSource::new()
                .include("/srv/app/**")
                .exclude("/srv/app/vendor/**")
                .with_relative_path("/srv/app"),
        ),
    )
    .with_destination_directory("/var/backups");

let destination = LocalDestination::new("/var/backups");
let result = Backup::new(profile, destination.clone()).run().await?;

let archive = destination.get(&result.path).await?;
restore_archive(&archive, "/tmp/restore")?;
# Ok(())
# }
```

Contoh lengkap ada di `examples/basic.rs`.

## Retensi

`CleanupConfig` memakai nilai default: keep all 7 hari, daily 16 hari, weekly
8 minggu, monthly 4 bulan, yearly 2 tahun, dan batas 5000 MB. Backup terbaru
selalu dipertahankan.

```rust
use backup::CleanupConfig;

let cleanup = CleanupConfig::new()
    .keep_all_backups_for_days(7)
    .keep_daily_backups_for_days(16)
    .keep_weekly_backups_for_weeks(8)
    .keep_monthly_backups_for_months(4)
    .keep_yearly_backups_for_years(2)
    .delete_oldest_backups_when_using_more_megabytes_than(5000);
```

Retensi juga bisa dihitung tanpa menjalankan backup melalui
`backup::tasks::cleanup::plan_at`.

## Monitor

```rust
use backup::{BackupMonitor, LocalDestination, MonitorConfig};

# async fn run() -> backup::Result<()> {
let monitor = BackupMonitor::new(
    "app",
    vec![LocalDestination::new("/var/backups")],
    MonitorConfig::new()
        .with_maximum_age_in_days(7)
        .with_maximum_storage_in_megabytes(5000),
);
let report = monitor.check().await?;
if !report.healthy {
    eprintln!("{:?}", report.issues);
}
# Ok(())
# }
```

## Restore

`restore_archive` mengekstrak arsip ke direktori tujuan. `restore_from`
mengambil arsip dari destinasi lebih dulu. Semua entri divalidasi; entri yang
keluar dari direktori tujuan menghasilkan `Error::UnsafeArchiveEntry`.

## Database

```rust
use backup::{DatabaseConfig, SourceConfig};

let source = SourceConfig::new("/tmp/backup-work")
    .with_database(
        DatabaseConfig::postgres("app")
            .with_host("127.0.0.1")
            .with_port(5432)
            .with_user("backup")
            .with_password_env("PGPASSWORD"),
    )
    .with_database(DatabaseConfig::mysql("app").with_password_env("MYSQL_PWD"))
    .with_database(DatabaseConfig::sqlite("/srv/app/database.sqlite"));
```

Password selalu diambil dari variabel lingkungan yang disebut pada
`password_env`; crate tidak menyimpan atau menebak credential. Bila binary
dump tidak tersedia atau keluar dengan status bukan nol, hasilnya
`Error::DatabaseDumpFailed`. SQLite cukup menyalin berkas.

## Enkripsi

```rust
use backup::EncryptionConfig;

let profile = profile.with_encryption(
    EncryptionConfig::new("passphrase").with_iterations(600_000),
);
```

Arsip diawali header berisi penanda, salt acak 16 byte, dan nonce acak 12
byte. Kunci 256-bit diturunkan dengan PBKDF2-HMAC-SHA256; default 600.000
iterasi mengikuti rekomendasi OWASP dan dapat diubah lewat `with_iterations`.
Password salah
menghasilkan `Error::Encryption`. Pemulihan memakai
`restore_encrypted(&archive, &config, "/tmp/restore")`.

## Format arsip

Arsip memakai tar.gz, bukan zip. Nama berkas pada destinasi:
`<name>/<filename_prefix><YYYY-MM-DD-HH-MM-SS>.tar.gz`. Isi arsip:

```text
manifest.json
<path relatif berkas>
databases/<nama>.sql
databases/<nama>.sqlite
```

Manifest memuat nama backup, timestamp pembuatan, daftar entri beserta ukuran,
dan daftar database.

## Batasan

- Format arsip tar.gz. Tidak ada dukungan zip.
- Tidak ada notifikasi pihak ketiga (email, Slack, webhook) bawaan.
- Tidak ada storage cloud (S3, GCS, Dropbox) bawaan; implementasikan trait
  `Destination` sendiri bila perlu.
- Tidak ada scheduler. Panggil `Backup::run` dari cron, systemd timer, atau
  task runner aplikasi.
- Arsip dibangun sepenuhnya di memori sebelum dikirim ke destinasi; untuk
  dataset sangat besar, proses perlu memori sebesar arsip terkompresi.
- Monitor hanya memeriksa destinasi yang diberikan; tidak menyimpan riwayat
  pemeriksaan.
- Symlink tidak diikuti saat restore dan seluruh entri symlink ditolak.

## Lisensi

Dirilis dengan lisensi MIT. Lihat berkas [LICENSE](LICENSE).
