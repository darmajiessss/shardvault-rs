// E:\shardvault-rs\src\transfer\importer.rs
// Importir Berkas Paket Portabel (.svp) ke dalam Cluster ShardVault

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use super::bundle::{unpack_bundle, PackageMetadata};
use crate::engine::DatabaseEngine;
use crate::error::ShardVaultError;
use crate::types::ShardRole;

/// Impor berkas paket .svp ke dalam cluster sebagai shard target
pub fn import_package_to_shard<P: AsRef<Path>>(
    engine: &DatabaseEngine,
    package_path: P,
    target_shard_key: &str,
    passphrase: &str,
) -> Result<PackageMetadata, ShardVaultError> {
    let pkg_path = package_path.as_ref();
    let package_bytes = fs::read(pkg_path).map_err(|e| ShardVaultError::Io {
        path: pkg_path.to_path_buf(),
        source: e,
    })?;

    // 1. Buka kemasan paket dengan validasi integritas HMAC & dekripsi payload
    let (metadata, raw_sqlite_bytes) = unpack_bundle(&package_bytes, passphrase)?;

    // 2. Verifikasi SHA-256 checksum snapshot
    let mut hasher = Sha256::new();
    hasher.update(&raw_sqlite_bytes);
    let calculated_checksum = format!("{:x}", hasher.finalize());

    if calculated_checksum != metadata.checksum_sha256 {
        return Err(ShardVaultError::Transfer(format!(
            "Checksum snapshot database tidak cocok! (Manifest: {}, Didekripsi: {})",
            metadata.checksum_sha256, calculated_checksum
        )));
    }

    // 3. Tentukan path tujuan file primary shard
    let shards_dir = engine.router().config.data_dir.join("shards");
    fs::create_dir_all(&shards_dir).map_err(|e| ShardVaultError::Io {
        path: shards_dir.clone(),
        source: e,
    })?;

    let dest_db_path = shards_dir.join(format!("{}.db", target_shard_key.to_lowercase()));

    // Jika file sudah ada, backup dulu untuk keamanan
    if dest_db_path.exists() {
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup_path = shards_dir.join(format!("{}.db.pre_import_backup.{}", target_shard_key.to_lowercase(), ts));
        let _ = fs::rename(&dest_db_path, &backup_path);
    }

    // 4. Tulis berkas snapshot ke disk
    fs::write(&dest_db_path, &raw_sqlite_bytes).map_err(|e| ShardVaultError::Io {
        path: dest_db_path.clone(),
        source: e,
    })?;

    // 5. Verifikasi quick_check atas database yang baru ditulis
    {
        let conn = Connection::open(&dest_db_path).map_err(|e| ShardVaultError::Sqlite {
            shard_key: target_shard_key.into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })?;
        let status: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .unwrap_or_else(|_| "corrupt".into());
        if status != "ok" {
            return Err(ShardVaultError::Transfer(format!(
                "Hasil impor database tidak lolos quick_check: {}",
                status
            )));
        }
    }

    // 6. Daftarkan shard ke katalog global jika belum ada
    if let Ok(global_conn_arc) = engine.router().get_global_conn() {
        if let Ok(global) = global_conn_arc.lock() {
            let _ = global.execute(
                "INSERT OR IGNORE INTO _shardvault_shards (shard_key, display_name) VALUES (?1, ?2);",
                rusqlite::params![
                    target_shard_key.to_lowercase(),
                    format!("Imported Shard {}", target_shard_key)
                ],
            );
        }
    }

    // 7. Sinkronkan seluruh 3 replika clone agar instan siap digunakan
    engine.sync_shard_clones(target_shard_key)?;

    Ok(metadata)
}
