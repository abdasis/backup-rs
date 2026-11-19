//! Penamaan arsip backup.

use chrono::{DateTime, Utc};

/// Format stempel waktu pada nama arsip.
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d-%H-%M-%S";

/// Nama berkas arsip: `<prefix><YYYY-MM-DD-HH-MM-SS>.tar.gz`.
pub fn backup_filename(prefix: &str, at: DateTime<Utc>) -> String {
    format!("{}{}.tar.gz", prefix, at.format(TIMESTAMP_FORMAT))
}

/// Path arsip pada destinasi: `<name>/<prefix><timestamp>.tar.gz`.
pub fn backup_path(name: &str, prefix: &str, at: DateTime<Utc>) -> String {
    format!("{}/{}", name, backup_filename(prefix, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn builds_timestamped_path() {
        let at = Utc.with_ymd_and_hms(2026, 3, 5, 4, 5, 6).unwrap();
        assert_eq!(
            backup_path("app", "nightly-", at),
            "app/nightly-2026-03-05-04-05-06.tar.gz"
        );
    }
}
