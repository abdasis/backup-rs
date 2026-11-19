//! Perencanaan retensi backup lama.

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Utc};

use crate::config::CleanupConfig;
use crate::destinations::DestinationEntry;

/// Memilih entri backup yang harus dihapus menurut [`CleanupConfig`].
///
/// Backup terbaru selalu dipertahankan. Waktu acuan diambil dari
/// `DateTime::now` agar bisa diuji dengan timestamp sintetis.
pub fn plan(entries: &[DestinationEntry], config: &CleanupConfig) -> Vec<DestinationEntry> {
    let now = Utc::now();
    plan_at(entries, config, now)
}

/// Seperti [`plan`], tetapi memakai waktu acuan eksplisit.
pub fn plan_at(
    entries: &[DestinationEntry],
    config: &CleanupConfig,
    now: DateTime<Utc>,
) -> Vec<DestinationEntry> {
    if entries.len() < 2 {
        return Vec::new();
    }

    let mut sorted: Vec<&DestinationEntry> = entries.iter().collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.modified));

    let mut keep: HashSet<&str> = HashSet::new();
    keep.insert(sorted[0].path.as_str());

    let all_cutoff = now - chrono::Duration::days(i64::from(config.keep_all_backups_for_days));
    for entry in &sorted {
        if entry.modified >= all_cutoff {
            keep.insert(entry.path.as_str());
        }
    }

    keep_newest_per_bucket(
        &sorted,
        config.keep_daily_backups_for_days,
        &mut keep,
        |entry| i64::from(entry.modified.date_naive().num_days_from_ce()),
    );
    keep_newest_per_bucket(
        &sorted,
        config.keep_weekly_backups_for_weeks,
        &mut keep,
        |entry| {
            let week = entry.modified.iso_week();
            i64::from(week.year()) * 100 + i64::from(week.week())
        },
    );
    keep_newest_per_bucket(
        &sorted,
        config.keep_monthly_backups_for_months,
        &mut keep,
        |entry| i64::from(entry.modified.year()) * 100 + i64::from(entry.modified.month()),
    );
    keep_newest_per_bucket(
        &sorted,
        config.keep_yearly_backups_for_years,
        &mut keep,
        |entry| i64::from(entry.modified.year()),
    );

    enforce_size_limit(&sorted, config, &mut keep);

    sorted
        .iter()
        .filter(|entry| !keep.contains(entry.path.as_str()))
        .map(|entry| (*entry).clone())
        .collect()
}

fn keep_newest_per_bucket<'a, F>(
    sorted: &[&'a DestinationEntry],
    limit: u32,
    keep: &mut HashSet<&'a str>,
    bucket_of: F,
) where
    F: Fn(&DestinationEntry) -> i64,
{
    if limit == 0 {
        return;
    }
    let mut buckets: HashSet<i64> = HashSet::new();
    for entry in sorted {
        let bucket = bucket_of(entry);
        if buckets.contains(&bucket) {
            continue;
        }
        if buckets.len() >= limit as usize {
            break;
        }
        buckets.insert(bucket);
        keep.insert(entry.path.as_str());
    }
}

fn enforce_size_limit(
    sorted: &[&DestinationEntry],
    config: &CleanupConfig,
    keep: &mut HashSet<&str>,
) {
    let limit = config.delete_oldest_backups_when_using_more_megabytes_than * 1024 * 1024;
    let mut total: u64 = sorted
        .iter()
        .filter(|entry| keep.contains(entry.path.as_str()))
        .map(|entry| entry.size)
        .sum();

    // `sorted` terurut dari terbaru; hapus dari ujung tertua, sisakan minimal
    // satu backup (terbaru) berapa pun batasnya.
    for entry in sorted.iter().rev() {
        if total <= limit || keep.len() <= 1 {
            break;
        }
        if keep.remove(entry.path.as_str()) {
            total = total.saturating_sub(entry.size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(path: &str, size: u64, at: DateTime<Utc>) -> DestinationEntry {
        DestinationEntry {
            path: path.to_string(),
            size,
            modified: at,
        }
    }

    fn at(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn newest_backup_is_never_deleted() {
        let entries = vec![
            entry("a/new.tar.gz", 10, at(2026, 1, 1)),
            entry("a/old.tar.gz", 10, at(2020, 1, 1)),
        ];
        let config = CleanupConfig::new()
            .keep_all_backups_for_days(0)
            .keep_daily_backups_for_days(1)
            .keep_weekly_backups_for_weeks(0)
            .keep_monthly_backups_for_months(0)
            .keep_yearly_backups_for_years(0)
            .delete_oldest_backups_when_using_more_megabytes_than(0);
        let plan = plan_at(&entries, &config, at(2026, 6, 1));
        assert!(plan.iter().all(|entry| entry.path != "a/new.tar.gz"));
    }
}
