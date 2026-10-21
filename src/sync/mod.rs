// E:\shardvault-rs\src\sync\mod.rs
// Modul Sinkronisasi Terdistribusi (CDC Oplog, HLC, & Conflict Resolution)

pub mod engine;
pub mod hlc;
pub mod oplog;
pub mod protocol;
pub mod resolver;

pub use engine::SyncEngine;
pub use hlc::{HlcTimestamp, HybridLogicalClock};
pub use oplog::{get_unsynced_oplog, init_oplog_table, mark_oplog_synced, purge_synced_oplog, setup_table_cdc_triggers, OplogEntry};
pub use protocol::{SyncPullRequest, SyncPullResponse, SyncPushRequest, SyncPushResponse};
pub use resolver::{ConflictOutcome, ConflictResolver};
