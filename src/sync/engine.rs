// E:\shardvault-rs\src\sync\engine.rs
// Facade Sinkronisasi Terdistribusi ShardVault

use rusqlite::Connection;
use std::sync::Arc;

use super::hlc::HybridLogicalClock;
use super::oplog::{get_unsynced_oplog, mark_oplog_synced, setup_table_cdc_triggers};
use super::protocol::{SyncPushRequest, SyncPushResponse};
use super::resolver::{ConflictOutcome, ConflictResolver};
use crate::error::ShardVaultError;

/// Engine Sinkronisasi Terdistribusi
pub struct SyncEngine {
    pub hlc: Arc<HybridLogicalClock>,
}

impl SyncEngine {
    pub fn new(node_id: String) -> Self {
        Self {
            hlc: Arc::new(HybridLogicalClock::new(node_id)),
        }
    }

    /// Pasang trigger CDC pada tabel yang diinginkan
    pub fn register_table_for_cdc(&self, conn: &Connection, table_name: &str) -> Result<(), ShardVaultError> {
        setup_table_cdc_triggers(conn, table_name, &self.hlc.node_id)
    }

    /// Ambil delta mutasi yang belum terkirim
    pub fn prepare_push_request(&self, conn: &Connection, shard_key: &str, limit: usize) -> Result<SyncPushRequest, ShardVaultError> {
        let operations = get_unsynced_oplog(conn, limit)?;
        Ok(SyncPushRequest {
            source_node_id: self.hlc.node_id.clone(),
            shard_key: shard_key.to_string(),
            operations,
        })
    }

    /// Terapkan paket mutasi yang diterima dari node lain
    pub fn process_incoming_push(&self, conn: &mut Connection, req: &SyncPushRequest) -> Result<SyncPushResponse, ShardVaultError> {
        let mut applied_count = 0;
        let mut ignored_count = 0;
        let mut latest_hlc = self.hlc.now();

        // Jalankan seluruh mutasi di dalam satu transaksi
        let tx = conn.transaction().map_err(|e| ShardVaultError::Sqlite {
            shard_key: req.shard_key.clone(),
            role: crate::types::ShardRole::PrimaryWrite,
            source: e,
        })?;

        for op in &req.operations {
            // Perbarui HLC lokal dengan timestamp yang masuk
            if let Ok(ts) = op.hlc_timestamp.parse() {
                latest_hlc = self.hlc.update(ts);
            }

            match ConflictResolver::apply_remote_mutation(&tx, op)? {
                ConflictOutcome::AppliedFresh | ConflictOutcome::AppliedRemoteWins | ConflictOutcome::AppliedFieldMerged => {
                    applied_count += 1;
                }
                ConflictOutcome::IgnoredLocalWins => {
                    ignored_count += 1;
                }
            }
        }

        tx.commit().map_err(|e| ShardVaultError::Sqlite {
            shard_key: req.shard_key.clone(),
            role: crate::types::ShardRole::PrimaryWrite,
            source: e,
        })?;

        Ok(SyncPushResponse {
            shard_key: req.shard_key.clone(),
            applied_count,
            ignored_count,
            latest_hlc: latest_hlc.to_canonical_string(),
        })
    }

    /// Konfirmasi bahwa mutasi lokal telah sukses diterima oleh peer (beri tanda is_synced = 1)
    pub fn acknowledge_push(&self, conn: &Connection, op_ids: &[i64]) -> Result<(), ShardVaultError> {
        mark_oplog_synced(conn, op_ids)
    }
}
