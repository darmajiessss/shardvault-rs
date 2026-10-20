// E:\shardvault-rs\src\schema.rs
// Trait SchemaProvider & Pengaturan PRAGMA Performa Tinggi

use rusqlite::Connection;

use crate::config::EngineConfig;
use crate::error::ShardVaultError;

/// Trait untuk mendefinisikan skema aplikasi eksternal (Global & Shard)
pub trait SchemaProvider: Send + Sync {
    /// Inisialisasi skema tabel untuk database global (katalog, pengguna, otentikasi)
    fn init_global(&self, conn: &Connection) -> Result<(), rusqlite::Error>;

    /// Inisialisasi skema tabel untuk database shard spesifik tenant/partisi
    fn init_shard(&self, shard_key: &str, conn: &Connection) -> Result<(), rusqlite::Error>;
}

/// Penyedia skema kosong bawaan (hanya membuat tabel internal engine)
#[derive(Debug, Default, Clone)]
pub struct EmptySchemaProvider;

impl SchemaProvider for EmptySchemaProvider {
    fn init_global(&self, _conn: &Connection) -> Result<(), rusqlite::Error> {
        Ok(())
    }

    fn init_shard(&self, _shard_key: &str, _conn: &Connection) -> Result<(), rusqlite::Error> {
        Ok(())
    }
}

/// Terapkan PRAGMA performa enterprise pada setiap koneksi baru
pub fn apply_pragma_settings(conn: &Connection, config: &EngineConfig) -> Result<(), ShardVaultError> {
    // Mode WAL untuk konkurensi zero-lock
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA cache_size = {};
        PRAGMA mmap_size = {};
        PRAGMA temp_store = MEMORY;
        PRAGMA busy_timeout = {};
        PRAGMA foreign_keys = ON;
        "#,
        config.cache_size_kb,
        config.mmap_size_bytes,
        config.busy_timeout_ms,
    ))
    .map_err(|e| ShardVaultError::Sqlite {
        shard_key: "pragma".into(),
        role: crate::types::ShardRole::PrimaryWrite,
        source: e,
    })?;

    Ok(())
}

/// Inisialisasi tabel metadata internal ShardVault pada database Global
pub fn init_internal_global_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _shardvault_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            checksum_sha256 TEXT NOT NULL,
            execution_time_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS _shardvault_shards (
            shard_key TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'ACTIVE',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            last_synced_at TEXT,
            metadata_json TEXT
        );

        CREATE TABLE IF NOT EXISTS _shardvault_audit_signatures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target_table TEXT NOT NULL,
            target_row_id TEXT NOT NULL,
            record_hash_sha256 TEXT NOT NULL,
            signature_hmac TEXT NOT NULL,
            signed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );
        "#,
    )?;
    Ok(())
}

/// Inisialisasi tabel metadata internal ShardVault pada setiap Shard
pub fn init_internal_shard_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _shardvault_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            checksum_sha256 TEXT NOT NULL,
            execution_time_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS _shardvault_audit_signatures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target_table TEXT NOT NULL,
            target_row_id TEXT NOT NULL,
            record_hash_sha256 TEXT NOT NULL,
            signature_hmac TEXT NOT NULL,
            signed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );
        "#,
    )?;
    Ok(())
}
