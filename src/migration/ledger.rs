// E:\shardvault-rs\src\migration\ledger.rs
// Pelacak Migrasi Terstruktur (_shardvault_migrations)

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::ShardVaultError;

/// Catatan riwayat migrasi yang telah dieksekusi
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationRecord {
    pub version: u32,
    pub name: String,
    pub checksum_sha256: String,
    pub applied_at_rfc3339: String,
    pub execution_time_ms: u64,
}

/// Inisialisasi tabel ledger migrasi jika belum ada
pub fn init_migration_ledger(conn: &Connection) -> Result<(), ShardVaultError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _shardvault_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            checksum_sha256 TEXT NOT NULL,
            execution_time_ms INTEGER NOT NULL
        );",
        [],
    )
    .map_err(|e| ShardVaultError::Migration(format!("Gagal membuat tabel _shardvault_migrations: {}", e)))?;

    Ok(())
}

/// Ambil seluruh daftar migrasi yang telah diterapkan terurut berdasarkan versi
pub fn get_applied_migrations(conn: &Connection) -> Result<Vec<MigrationRecord>, ShardVaultError> {
    init_migration_ledger(conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT version, name, checksum_sha256, applied_at, execution_time_ms 
             FROM _shardvault_migrations 
             ORDER BY version ASC;",
        )
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(MigrationRecord {
                version: row.get::<_, i64>(0)? as u32,
                name: row.get(1)?,
                checksum_sha256: row.get(2)?,
                applied_at_rfc3339: row.get(3)?,
                execution_time_ms: row.get::<_, i64>(4)? as u64,
            })
        })
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

    let mut records = Vec::new();
    for r in rows {
        records.push(r.map_err(|e| ShardVaultError::Migration(e.to_string()))?);
    }

    Ok(records)
}

/// Catat migrasi yang berhasil diterapkan ke dalam ledger
pub fn record_migration(conn: &Connection, record: &MigrationRecord) -> Result<(), ShardVaultError> {
    conn.execute(
        "INSERT INTO _shardvault_migrations (version, name, applied_at, checksum_sha256, execution_time_ms)
         VALUES (?1, ?2, ?3, ?4, ?5);",
        rusqlite::params![
            record.version as i64,
            record.name,
            record.applied_at_rfc3339,
            record.checksum_sha256,
            record.execution_time_ms as i64,
        ],
    )
    .map_err(|e| ShardVaultError::Migration(format!("Gagal mencatat migrasi v{}: {}", record.version, e)))?;

    Ok(())
}
