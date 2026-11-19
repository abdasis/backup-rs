//! Pemantauan kesehatan backup pada satu atau beberapa destinasi.

use chrono::Utc;

use crate::config::MonitorConfig;
use crate::destinations::Destination;
use crate::error::Result;

/// Masalah kesehatan backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorIssue {
    /// Destinasi tidak memiliki backup untuk nama ini.
    Missing {
        /// Indeks destinasi pada daftar monitor.
        destination: usize,
    },
    /// Backup terbaru lebih tua dari batas.
    TooOld {
        /// Umur backup terbaru dalam hari.
        age_in_days: i64,
    },
    /// Total ukuran backup melebihi batas.
    StorageTooLarge {
        /// Total ukuran dalam megabyte.
        total_size_in_megabytes: u64,
    },
}

/// Ringkasan kesehatan backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorReport {
    /// `true` bila tidak ada masalah.
    pub healthy: bool,
    /// Umur backup terbaru dalam hari, bila ada.
    pub latest_age_in_days: Option<i64>,
    /// Total ukuran arsip dalam megabyte.
    pub total_size_in_megabytes: u64,
    /// Daftar masalah yang ditemukan.
    pub issues: Vec<MonitorIssue>,
}

/// Memeriksa umur backup terbaru dan total ukuran pada daftar destinasi.
pub struct BackupMonitor<D: Destination> {
    name: String,
    destinations: Vec<D>,
    config: MonitorConfig,
}

impl<D: Destination> BackupMonitor<D> {
    /// Monitor untuk nama backup, daftar destinasi, dan ambang tertentu.
    pub fn new(name: impl Into<String>, destinations: Vec<D>, config: MonitorConfig) -> Self {
        Self {
            name: name.into(),
            destinations,
            config,
        }
    }

    /// Menjalankan pemeriksaan kesehatan.
    pub async fn check(&self) -> Result<MonitorReport> {
        let prefix = format!("{}/", self.name);
        let mut issues = Vec::new();
        let mut latest: Option<chrono::DateTime<Utc>> = None;
        let mut total_bytes = 0u64;

        for (index, destination) in self.destinations.iter().enumerate() {
            let entries: Vec<_> = destination
                .list()
                .await?
                .into_iter()
                .filter(|entry| entry.path.starts_with(&prefix))
                .collect();
            if entries.is_empty() {
                issues.push(MonitorIssue::Missing { destination: index });
                continue;
            }
            total_bytes += entries.iter().map(|entry| entry.size).sum::<u64>();
            if let Some(newest) = entries.iter().map(|entry| entry.modified).max() {
                latest = Some(latest.map_or(newest, |current| current.max(newest)));
            }
        }

        let latest_age_in_days = latest.map(|newest| (Utc::now() - newest).num_days().max(0));
        if let Some(age_in_days) = latest_age_in_days {
            if age_in_days > i64::from(self.config.maximum_age_in_days) {
                issues.push(MonitorIssue::TooOld { age_in_days });
            }
        }

        let total_size_in_megabytes = total_bytes / (1024 * 1024);
        if total_size_in_megabytes > self.config.maximum_storage_in_megabytes {
            issues.push(MonitorIssue::StorageTooLarge {
                total_size_in_megabytes,
            });
        }

        Ok(MonitorReport {
            healthy: issues.is_empty(),
            latest_age_in_days,
            total_size_in_megabytes,
            issues,
        })
    }
}
