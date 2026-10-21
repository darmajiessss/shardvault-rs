// E:\shardvault-rs\src\sync\protocol.rs
// Format Pesan Wire & Protokol Sinkronisasi Terdistribusi

use serde::{Deserialize, Serialize};

use super::oplog::OplogEntry;

/// Permintaan pengiriman mutasi lokal ke node lain (Push)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushRequest {
    pub source_node_id: String,
    pub shard_key: String,
    pub operations: Vec<OplogEntry>,
}

/// Balasan setelah menerima push mutasi
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushResponse {
    pub shard_key: String,
    pub applied_count: usize,
    pub ignored_count: usize,
    pub latest_hlc: String,
}

/// Permintaan penarikan mutasi delta dari node lain (Pull)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullRequest {
    pub requester_node_id: String,
    pub shard_key: String,
    pub since_hlc: Option<String>,
    pub limit: usize,
}

/// Balasan atas permintaan pull mutasi
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullResponse {
    pub shard_key: String,
    pub operations: Vec<OplogEntry>,
    pub latest_hlc: String,
}
