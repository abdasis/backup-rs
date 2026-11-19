//! Dump database ke memori untuk dimasukkan ke arsip.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::config::{DatabaseConfig, DatabaseDriver};
use crate::error::{Error, Result};

/// Menjalankan dump database dan mengembalikan isi dump.
///
/// PostgreSQL memakai `pg_dump`, MySQL memakai `mysqldump`, SQLite menyalin
/// berkas. Password diambil dari variabel lingkungan yang disebut pada
/// konfigurasi; tidak pernah dari teks konfigurasi.
pub async fn dump(config: &DatabaseConfig) -> Result<Vec<u8>> {
    match config.driver {
        DatabaseDriver::Sqlite => dump_sqlite(config).await,
        DatabaseDriver::Postgres => dump_postgres(config).await,
        DatabaseDriver::Mysql => dump_mysql(config).await,
    }
}

async fn dump_sqlite(config: &DatabaseConfig) -> Result<Vec<u8>> {
    let file = config.sqlite_file.as_ref().ok_or_else(|| {
        Error::Config(format!(
            "database '{}': sqlite_file wajib diisi",
            config.name
        ))
    })?;
    tokio::fs::read(file).await.map_err(|error| {
        Error::DatabaseDumpFailed(format!(
            "gagal membaca SQLite '{}': {error}",
            file.display()
        ))
    })
}

async fn dump_postgres(config: &DatabaseConfig) -> Result<Vec<u8>> {
    let mut command = Command::new(&config.binary);
    command
        .arg("--no-password")
        .arg("--host")
        .arg(&config.host)
        .arg("--port")
        .arg(config.port.to_string())
        .arg("--dbname")
        .arg(&config.database);
    if let Some(user) = &config.user {
        command.arg("--username").arg(user);
    }
    apply_password(&mut command, config)?;
    run_command(command, &config.binary).await
}

async fn dump_mysql(config: &DatabaseConfig) -> Result<Vec<u8>> {
    let mut command = Command::new(&config.binary);
    command
        .arg("--host")
        .arg(&config.host)
        .arg("--port")
        .arg(config.port.to_string());
    if let Some(user) = &config.user {
        command.arg("--user").arg(user);
    }
    command.arg(&config.database);
    apply_password(&mut command, config)?;
    run_command(command, &config.binary).await
}

fn apply_password(command: &mut Command, config: &DatabaseConfig) -> Result<()> {
    let Some(variable) = &config.password_env else {
        return Ok(());
    };
    let password = std::env::var(variable).map_err(|_| {
        Error::Config(format!(
            "database '{}': variabel lingkungan '{variable}' tidak diset",
            config.name
        ))
    })?;
    match config.driver {
        DatabaseDriver::Postgres => {
            command.env("PGPASSWORD", password);
        }
        DatabaseDriver::Mysql => {
            command.env("MYSQL_PWD", password);
        }
        DatabaseDriver::Sqlite => {}
    }
    Ok(())
}

async fn run_command(mut command: Command, binary: &Path) -> Result<Vec<u8>> {
    let output = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::DatabaseDumpFailed(format!("binary '{}' tidak ditemukan", binary.display()))
            } else {
                Error::DatabaseDumpFailed(format!(
                    "gagal menjalankan '{}': {error}",
                    binary.display()
                ))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::DatabaseDumpFailed(format!(
            "'{}' keluar dengan status {}: {}",
            binary.display(),
            output.status,
            stderr.trim()
        )));
    }
    Ok(output.stdout)
}
