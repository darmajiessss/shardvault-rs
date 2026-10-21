// E:\shardvault-rs\src\migration\runner.rs
// Universal Migration Runner: Eksekusi Migrasi Terstruktur & Propagasi Replikasi

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;

use super::ledger::{get_applied_migrations, init_migration_ledger, record_migration, MigrationRecord};
use crate::engine::DatabaseEngine;
use crate::error::ShardVaultError;

/// Target basis data untuk penerapan migrasi
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationTarget {
    /// Hanya diterapkan pada global.db
    GlobalOnly,
    /// Hanya diterapkan pada seluruh partisi shards/<key>.db
    ShardsOnly,
    /// Diterapkan pada global.db maupun seluruh partisi shards/<key>.db
    All,
}

/// Definisi deklaratif satu unit migrasi
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub target: MigrationTarget,
    pub up_sql: &'static str,
    pub down_sql: Option<&'static str>,
}

impl Migration {
    pub fn checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.up_sql.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Laporan eksekusi migrasi untuk satu target database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub target_name: String,
    pub applied_versions: Vec<u32>,
    pub total_duration_ms: u64,
}

/// Pelaksana migrasi terpusat
#[derive(Default, Clone)]
pub struct MigrationRunner {
    migrations: Vec<Migration>,
}

impl MigrationRunner {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Daftarkan migrasi baru ke dalam runner
    pub fn add(&mut self, migration: Migration) -> &mut Self {
        self.migrations.push(migration);
        self.migrations.sort_by_key(|m| m.version);
        self
    }

    /// Ambil daftar migrasi yang terdaftar
    pub fn list(&self) -> &[Migration] {
        &self.migrations
    }

    /// Eksekusi migrasi yang belum diterapkan pada satu koneksi database tertentu
    pub fn run_on_connection(
        &self,
        conn: &Connection,
        target_name: &str,
        is_global: bool,
    ) -> Result<MigrationReport, ShardVaultError> {
        init_migration_ledger(conn)?;
        let applied = get_applied_migrations(conn)?;
        let applied_map: std::collections::HashMap<u32, String> = applied
            .into_iter()
            .map(|r| (r.version, r.checksum_sha256))
            .collect();

        let mut newly_applied = Vec::new();
        let start_time = Instant::now();

        for m in &self.migrations {
            // Filter target
            match m.target {
                MigrationTarget::GlobalOnly if !is_global => continue,
                MigrationTarget::ShardsOnly if is_global => continue,
                _ => {}
            }

            let calculated_checksum = m.checksum();

            if let Some(existing_checksum) = applied_map.get(&m.version) {
                // Verifikasi bahwa isi migrasi tidak diubah setelah diterapkan
                if existing_checksum != &calculated_checksum {
                    return Err(ShardVaultError::Migration(format!(
                        "Integritas migrasi rusak pada '{}' v{}: checksum skrip telah berubah! (Tersimpan: {}, Sekarang: {})",
                        target_name, m.version, existing_checksum, calculated_checksum
                    )));
                }
                continue;
            }

            // Eksekusi migrasi di dalam transaksi atomik
            let tx_start = Instant::now();
            conn.execute_batch(m.up_sql).map_err(|e| {
                ShardVaultError::Migration(format!(
                    "Gagal mengeksekusi migrasi v{} ('{}') pada '{}': {}",
                    m.version, m.name, target_name, e
                ))
            })?;

            let duration = tx_start.elapsed().as_millis() as u64;

            let record = MigrationRecord {
                version: m.version,
                name: m.name.to_string(),
                checksum_sha256: calculated_checksum,
                applied_at_rfc3339: Utc::now().to_rfc3339(),
                execution_time_ms: duration,
            };

            record_migration(conn, &record)?;
            newly_applied.push(m.version);
        }

        Ok(MigrationReport {
            target_name: target_name.to_string(),
            applied_versions: newly_applied,
            total_duration_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Eksekusi migrasi ke seluruh cluster (Global DB + Seluruh Shard Aktif)
    /// dan otomatis mereplikasi perubahan skema ke seluruh kloning (_read, _analytics, _standby)
    pub fn run_cluster(&self, engine: &DatabaseEngine) -> Result<Vec<MigrationReport>, ShardVaultError> {
        let mut reports = Vec::new();

        // 1. Eksekusi pada Global Database
        let global_conn_arc = engine.router().get_global_conn()?;
        let global_report = {
            let conn = global_conn_arc.lock().unwrap();
            self.run_on_connection(&conn, "global", true)?
        };

        if !global_report.applied_versions.is_empty() {
            // Propagasikan DDL baru ke kloning replika global
            engine.sync_global_clones()?;
        }
        reports.push(global_report);

        // 2. Eksekusi pada Seluruh Shard Aktif
        let shards = engine.list_active_shards()?;
        for shard_key in shards {
            let shard_conn_arc = engine.router().get_shard_conn(&shard_key, crate::types::ShardRole::PrimaryWrite)?;
            let shard_report = {
                let conn = shard_conn_arc.lock().unwrap();
                self.run_on_connection(&conn, &shard_key, false)?
            };

            if !shard_report.applied_versions.is_empty() {
                // Propagasikan DDL baru ke kloning replika shard
                engine.sync_shard_clones(&shard_key)?;
            }
            reports.push(shard_report);
        }

        Ok(reports)
    }
}
