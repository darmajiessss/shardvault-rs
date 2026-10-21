// E:\shardvault-rs\src\transfer\exporter.rs
// Eksportir Basis Data ke Berkas Paket Portabel Terenkripsi (.svp)

use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::bundle::{pack_bundle, PackageMetadata};
use crate::engine::DatabaseEngine;
use crate::error::ShardVaultError;
use crate::types::ShardRole;

/// Ambil snapshot atomik yang bersih dari koneksi SQLite aktif ke memori menggunakan Online Backup API
pub fn create_atomic_snapshot(src_conn: &Connection) -> Result<Vec<u8>, ShardVaultError> {
    let temp_dir = tempfile::tempdir().map_err(|e| ShardVaultError::Io {
        path: std::env::temp_dir(),
        source: e,
    })?;
    let temp_db_path = temp_dir.path().join("snapshot.db");

    // 1. Jalankan SQLite Online Backup ke berkas sementara
    {
        let mut dst_conn = Connection::open(&temp_db_path).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "export_temp".into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })?;

        let backup = rusqlite::backup::Backup::new(src_conn, &mut dst_conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "export_temp".into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })?;

        backup
            .run_to_completion(100, Duration::from_millis(5), None)
            .map_err(|e| ShardVaultError::Sqlite {
                shard_key: "export_temp".into(),
                role: ShardRole::PrimaryWrite,
                source: e,
            })?;
    }

    // 2. Verifikasi quick_check atas berkas snapshot
    {
        let check_conn = Connection::open(&temp_db_path).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "export_temp".into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })?;
        let status: String = check_conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .unwrap_or_else(|_| "check_failed".into());
        if status != "ok" {
            return Err(ShardVaultError::Transfer(format!(
                "Snapshot export korup: {}",
                status
            )));
        }
    }

    // 3. Baca seluruh bytes berkas snapshot
    let bytes = fs::read(&temp_db_path).map_err(|e| ShardVaultError::Io {
        path: temp_db_path,
        source: e,
    })?;

    Ok(bytes)
}

/// Hitung jumlah baris per tabel dalam koneksi
fn collect_table_row_counts(conn: &Connection) -> Vec<(String, usize)> {
    let mut counts = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for table_name in rows.flatten() {
                let query = format!("SELECT COUNT(*) FROM \"{}\"", table_name.replace('"', "\"\""));
                let count: usize = conn.query_row(&query, [], |r| r.get(0)).unwrap_or(0);
                counts.push((table_name, count));
            }
        }
    }
    counts
}

/// Ekspor shard tertentu ke berkas paket .svp dengan passphrase transfer
pub fn export_shard_to_package<P: AsRef<Path>>(
    engine: &DatabaseEngine,
    shard_key: &str,
    dest_path: P,
    passphrase: &str,
) -> Result<PackageMetadata, ShardVaultError> {
    let conn_arc = engine.router().get_shard_conn(shard_key, ShardRole::PrimaryWrite)?;
    let conn = conn_arc.lock().unwrap();

    let row_counts = collect_table_row_counts(&conn);
    let snapshot_bytes = create_atomic_snapshot(&conn)?;

    let mut hasher = Sha256::new();
    hasher.update(&snapshot_bytes);
    let checksum_sha256 = format!("{:x}", hasher.finalize());

    let schema_version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _shardvault_migrations",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;

    let metadata = PackageMetadata {
        shard_key: shard_key.to_string(),
        schema_version,
        source_machine_salt: engine.router().config.app_salt.clone(),
        created_at_rfc3339: Utc::now().to_rfc3339(),
        row_counts,
        checksum_sha256,
    };

    let bundle_bytes = pack_bundle(&metadata, &snapshot_bytes, passphrase)?;

    let dest = dest_path.as_ref();
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(dest, &bundle_bytes).map_err(|e| ShardVaultError::Io {
        path: dest.to_path_buf(),
        source: e,
    })?;

    Ok(metadata)
}
