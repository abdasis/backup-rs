//! Konfigurasi profil backup, sumber, destinasi, retensi, dan monitor.

use std::path::PathBuf;

/// Profil lengkap satu jenis backup.
#[derive(Debug, Clone)]
pub struct BackupProfile {
    /// Nama backup; juga menjadi nama direktori pada destinasi.
    pub name: String,
    /// Awalan nama berkas arsip.
    pub filename_prefix: String,
    /// Sumber berkas dan database.
    pub source: SourceConfig,
    /// Lokasi arsip disimpan.
    pub destination: DestinationConfig,
    /// Kebijakan retensi.
    pub cleanup: CleanupConfig,
    /// Konfigurasi monitor opsional.
    pub monitor: Option<MonitorConfig>,
    /// Konfigurasi enkripsi opsional.
    #[cfg(feature = "encryption")]
    pub encryption: Option<EncryptionConfig>,
}

impl Default for BackupProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            filename_prefix: String::new(),
            source: SourceConfig::default(),
            destination: DestinationConfig::default(),
            cleanup: CleanupConfig::default(),
            monitor: None,
            #[cfg(feature = "encryption")]
            encryption: None,
        }
    }
}

impl BackupProfile {
    /// Profil dengan nama backup tertentu dan nilai default lainnya.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Mengganti konfigurasi sumber.
    pub fn with_source(mut self, source: SourceConfig) -> Self {
        self.source = source;
        self
    }

    /// Mengganti direktori destinasi.
    pub fn with_destination_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.destination = DestinationConfig::new(directory);
        self
    }

    /// Mengganti konfigurasi destinasi.
    pub fn with_destination(mut self, destination: DestinationConfig) -> Self {
        self.destination = destination;
        self
    }

    /// Mengganti kebijakan retensi.
    pub fn with_cleanup(mut self, cleanup: CleanupConfig) -> Self {
        self.cleanup = cleanup;
        self
    }

    /// Menambahkan konfigurasi monitor.
    pub fn with_monitor(mut self, monitor: MonitorConfig) -> Self {
        self.monitor = Some(monitor);
        self
    }

    /// Mengganti awalan nama berkas arsip.
    pub fn with_filename_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.filename_prefix = prefix.into();
        self
    }

    /// Mengaktifkan enkripsi arsip.
    #[cfg(feature = "encryption")]
    pub fn with_encryption(mut self, encryption: EncryptionConfig) -> Self {
        self.encryption = Some(encryption);
        self
    }
}

/// Sumber data yang dicadangkan.
#[derive(Debug, Clone)]
pub struct SourceConfig {
    /// Berkas dan direktori yang disertakan.
    pub files: FileSource,
    /// Database yang didump.
    pub databases: Vec<DatabaseConfig>,
    /// Direktori kerja sementara; otomatis dikecualikan dari arsip.
    pub temporary_directory: PathBuf,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            files: FileSource::default(),
            databases: Vec::new(),
            temporary_directory: std::env::temp_dir().join("backup-rs"),
        }
    }
}

impl SourceConfig {
    /// Sumber dengan direktori sementara tertentu.
    pub fn new(temporary_directory: impl Into<PathBuf>) -> Self {
        Self {
            temporary_directory: temporary_directory.into(),
            ..Self::default()
        }
    }

    /// Mengganti konfigurasi berkas.
    pub fn with_files(mut self, files: FileSource) -> Self {
        self.files = files;
        self
    }

    /// Menambahkan satu database.
    pub fn with_database(mut self, database: DatabaseConfig) -> Self {
        self.databases.push(database);
        self
    }
}

/// Pemilihan berkas yang dicadangkan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSource {
    /// Pola glob yang disertakan. Kosong berarti tidak ada berkas yang diarsipkan.
    pub include: Vec<String>,
    /// Pola glob yang dikecualikan.
    pub exclude: Vec<String>,
    /// Ikuti symbolic link saat menelusuri direktori.
    pub follow_links: bool,
    /// Lewati berkas yang tidak bisa dibaca alih-alih gagal.
    pub ignore_unreadable: bool,
    /// Awalan path yang dipotong dari nama entri di arsip.
    pub relative_path: Option<PathBuf>,
}

impl Default for FileSource {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            follow_links: false,
            ignore_unreadable: true,
            relative_path: None,
        }
    }
}

impl FileSource {
    /// Konfigurasi berkas kosong dengan nilai default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Menambahkan pola include.
    pub fn include(mut self, pattern: impl Into<String>) -> Self {
        self.include.push(pattern.into());
        self
    }

    /// Menambahkan pola exclude.
    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.exclude.push(pattern.into());
        self
    }

    /// Mengatur perilaku mengikuti symbolic link.
    pub fn with_follow_links(mut self, follow: bool) -> Self {
        self.follow_links = follow;
        self
    }

    /// Mengatur perilaku terhadap berkas yang tidak terbaca.
    pub fn with_ignore_unreadable(mut self, ignore: bool) -> Self {
        self.ignore_unreadable = ignore;
        self
    }

    /// Mengatur awalan path yang dipotong dari nama entri.
    pub fn with_relative_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.relative_path = Some(path.into());
        self
    }
}

/// Lokasi direktori destinasi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationConfig {
    /// Direktori tujuan penyimpanan arsip.
    pub directory: PathBuf,
}

impl Default for DestinationConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("backups"),
        }
    }
}

impl DestinationConfig {
    /// Destinasi pada direktori tertentu.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
}

/// Kebijakan retensi backup.
///
/// Backup terbaru tidak pernah dihapus, berapa pun batas yang diatur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupConfig {
    /// Simpan semua backup selama sekian hari terakhir.
    pub keep_all_backups_for_days: u32,
    /// Simpan satu backup per hari selama sekian hari.
    pub keep_daily_backups_for_days: u32,
    /// Simpan satu backup per minggu selama sekian minggu.
    pub keep_weekly_backups_for_weeks: u32,
    /// Simpan satu backup per bulan selama sekian bulan.
    pub keep_monthly_backups_for_months: u32,
    /// Simpan satu backup per tahun selama sekian tahun.
    pub keep_yearly_backups_for_years: u32,
    /// Hapus backup tertua saat total ukuran melebihi batas megabyte ini.
    pub delete_oldest_backups_when_using_more_megabytes_than: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            keep_all_backups_for_days: 7,
            keep_daily_backups_for_days: 16,
            keep_weekly_backups_for_weeks: 8,
            keep_monthly_backups_for_months: 4,
            keep_yearly_backups_for_years: 2,
            delete_oldest_backups_when_using_more_megabytes_than: 5000,
        }
    }
}

impl CleanupConfig {
    /// Kebijakan default dengan semua nilai bawaan.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mengatur berapa hari semua backup disimpan.
    pub fn keep_all_backups_for_days(mut self, days: u32) -> Self {
        self.keep_all_backups_for_days = days;
        self
    }

    /// Mengatur berapa hari backup harian disimpan.
    pub fn keep_daily_backups_for_days(mut self, days: u32) -> Self {
        self.keep_daily_backups_for_days = days;
        self
    }

    /// Mengatur berapa minggu backup mingguan disimpan.
    pub fn keep_weekly_backups_for_weeks(mut self, weeks: u32) -> Self {
        self.keep_weekly_backups_for_weeks = weeks;
        self
    }

    /// Mengatur berapa bulan backup bulanan disimpan.
    pub fn keep_monthly_backups_for_months(mut self, months: u32) -> Self {
        self.keep_monthly_backups_for_months = months;
        self
    }

    /// Mengatur berapa tahun backup tahunan disimpan.
    pub fn keep_yearly_backups_for_years(mut self, years: u32) -> Self {
        self.keep_yearly_backups_for_years = years;
        self
    }

    /// Mengatur batas total ukuran dalam megabyte.
    pub fn delete_oldest_backups_when_using_more_megabytes_than(mut self, megabytes: u64) -> Self {
        self.delete_oldest_backups_when_using_more_megabytes_than = megabytes;
        self
    }
}

/// Ambang kesehatan backup untuk monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    /// Umur maksimum backup terbaru dalam hari.
    pub maximum_age_in_days: u32,
    /// Batas total ukuran backup dalam megabyte.
    pub maximum_storage_in_megabytes: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            maximum_age_in_days: 7,
            maximum_storage_in_megabytes: 5000,
        }
    }
}

impl MonitorConfig {
    /// Ambang dengan nilai default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mengatur umur maksimum backup terbaru.
    pub fn with_maximum_age_in_days(mut self, days: u32) -> Self {
        self.maximum_age_in_days = days;
        self
    }

    /// Mengatur batas total ukuran backup.
    pub fn with_maximum_storage_in_megabytes(mut self, megabytes: u64) -> Self {
        self.maximum_storage_in_megabytes = megabytes;
        self
    }
}

/// Driver database yang didukung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatabaseDriver {
    /// PostgreSQL lewat `pg_dump`.
    #[default]
    Postgres,
    /// MySQL lewat `mysqldump`.
    Mysql,
    /// SQLite dengan menyalin berkas database.
    Sqlite,
}

/// Konfigurasi satu database yang didump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    /// Nama dump; menjadi nama berkas di dalam arsip.
    pub name: String,
    /// Driver database.
    pub driver: DatabaseDriver,
    /// Path binary dump; default `pg_dump` atau `mysqldump` sesuai driver.
    pub binary: PathBuf,
    /// Host database.
    pub host: String,
    /// Port database.
    pub port: u16,
    /// Nama pengguna database.
    pub user: Option<String>,
    /// Nama variabel lingkungan yang menyimpan password.
    pub password_env: Option<String>,
    /// Nama database.
    pub database: String,
    /// Berkas SQLite; hanya dipakai driver [`DatabaseDriver::Sqlite`].
    pub sqlite_file: Option<PathBuf>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            driver: DatabaseDriver::Postgres,
            binary: PathBuf::from("pg_dump"),
            host: "127.0.0.1".to_string(),
            port: 5432,
            user: None,
            password_env: None,
            database: String::new(),
            sqlite_file: None,
        }
    }
}

impl DatabaseConfig {
    /// Database PostgreSQL.
    pub fn postgres(database: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            ..Self::default()
        }
    }

    /// Database MySQL.
    pub fn mysql(database: impl Into<String>) -> Self {
        Self {
            driver: DatabaseDriver::Mysql,
            binary: PathBuf::from("mysqldump"),
            port: 3306,
            database: database.into(),
            ..Self::default()
        }
    }

    /// Database SQLite dari berkas tertentu.
    pub fn sqlite(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sqlite".to_string());
        Self {
            name,
            driver: DatabaseDriver::Sqlite,
            sqlite_file: Some(file),
            database: String::new(),
            ..Self::default()
        }
    }

    /// Mengganti nama dump.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Mengganti path binary dump.
    pub fn with_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Mengganti host.
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Mengganti port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Mengganti nama pengguna.
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Mengatur nama variabel lingkungan berisi password.
    pub fn with_password_env(mut self, variable: impl Into<String>) -> Self {
        self.password_env = Some(variable.into());
        self
    }
}

/// Konfigurasi enkripsi arsip.
///
/// Kunci AES-256-GCM diturunkan dari password dengan PBKDF2-HMAC-SHA256 dan
/// salt acak yang disimpan di header arsip.
#[cfg(feature = "encryption")]
#[derive(Clone)]
pub struct EncryptionConfig {
    /// Password yang dipakai menurunkan kunci.
    pub password: String,
    /// Jumlah iterasi PBKDF2.
    pub iterations: u32,
}

#[cfg(feature = "encryption")]
impl std::fmt::Debug for EncryptionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionConfig")
            .field("password", &"<redacted>")
            .field("iterations", &self.iterations)
            .finish()
    }
}

#[cfg(feature = "encryption")]
impl EncryptionConfig {
    /// Jumlah iterasi PBKDF2 default.
    pub const DEFAULT_ITERATIONS: u32 = 600_000;

    /// Konfigurasi enkripsi dengan password tertentu.
    pub fn new(password: impl Into<String>) -> Self {
        Self {
            password: password.into(),
            iterations: Self::DEFAULT_ITERATIONS,
        }
    }

    /// Mengganti jumlah iterasi PBKDF2.
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }
}
