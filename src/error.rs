// E:\shardvault-rs\src\error.rs
// Definisi Error Terstruktur untuk ShardVault-RS Engine

use std::path::PathBuf;
use thiserror::Error;

use crate::types::ShardRole;

#[derive(Debug, Error)]
pub enum ShardVaultError {
    #[error("I/O failure at path {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("SQLite database error on shard '{shard_key}' ({role:?}): {source}")]
    Sqlite {
        shard_key: String,
        role: ShardRole,
        #[source]
        source: rusqlite::Error,
    },

    #[error("Cryptographic operation error: {0}")]
    Crypto(String),

    #[error("Cryptographic tampering detected on row '{row_id}' in table '{table}'")]
    TamperDetected {
        table: String,
        row_id: String,
    },

    #[error("Corrupted database detected on primary shard '{shard_key}': {reason}")]
    CorruptPrimary {
        shard_key: String,
        reason: String,
    },

    #[error("Hot standby replica is not available for shard '{shard_key}' at path {path:?}")]
    StandbyUnavailable {
        shard_key: String,
        path: PathBuf,
    },

    #[error("Integrity check failed: source shard is corrupted. Sync aborted to protect standby replica.")]
    PoisoningPrevented {
        shard_key: String,
        reason: String,
    },

    #[error("Configuration or initialization error: {0}")]
    Config(String),

    #[error("Schema migration error: {0}")]
    Migration(String),

    #[error("Database transfer package error: {0}")]
    Transfer(String),

    #[error("Distributed synchronization error: {0}")]
    Sync(String),

    #[error("General engine error: {0}")]
    Other(String),
}

impl From<rusqlite::Error> for ShardVaultError {
    fn from(err: rusqlite::Error) -> Self {
        ShardVaultError::Sqlite {
            shard_key: "unknown".to_string(),
            role: ShardRole::PrimaryWrite,
            source: err,
        }
    }
}
