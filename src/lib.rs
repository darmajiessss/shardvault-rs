// E:\shardvault-rs\src\lib.rs
//! # ShardVault-RS
//!
//! Ultra-Fast, Self-Healing, Hardware-Bound Multi-Shard SQLite Engine
//! Dirancang untuk aplikasi Desktop (Rust/egui/Tauri), POS Offline-First, Edge Gateways, dan Multi-Tenant SaaS.

pub mod config;
pub mod crypto;
pub mod engine;
pub mod error;
pub mod migration;
pub mod router;
pub mod schema;
pub mod studio;
pub mod sync;
pub mod transfer;
pub mod types;

pub mod prelude {
    pub use crate::config::{EngineBuilder, EngineConfig};
    pub use crate::crypto::{
        decrypt_string, derive_machine_key, encrypt_string, generate_signature, verify_signature,
    };
    pub use crate::engine::DatabaseEngine;
    pub use crate::error::ShardVaultError;
    pub use crate::migration::{
        detect_schema_drift, introspect_schema, ColumnInfo, Migration, MigrationRecord, MigrationReport,
        MigrationRunner, MigrationTarget, SchemaManifest, TableSchema,
    };
    pub use crate::router::ShardRouter;
    pub use crate::schema::{EmptySchemaProvider, SchemaProvider};
    pub use crate::sync::{
        ConflictOutcome, ConflictResolver, HlcTimestamp, HybridLogicalClock, OplogEntry, SyncEngine,
        SyncPullRequest, SyncPullResponse, SyncPushRequest, SyncPushResponse,
    };
    pub use crate::transfer::{export_shard_to_package, import_package_to_shard, pack_bundle, unpack_bundle, PackageMetadata};
    pub use crate::types::{
        GlobalTelemetry, ReplicaTelemetry, ShardKey, ShardRole, ShardTelemetry, SystemTelemetry,
    };
    pub use rusqlite::{params, Connection};
}

pub use engine::DatabaseEngine;
pub use error::ShardVaultError;
pub use router::ShardRouter;
