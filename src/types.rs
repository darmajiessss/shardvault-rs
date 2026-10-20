// E:\shardvault-rs\src\types.rs
// Tipe Data Inti, Enum Peran Shard, dan Telemetri ShardVault-RS

use serde::{Deserialize, Serialize};
use std::fmt;

/// Peran Replikasi Khusus Beban Kerja
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShardRole {
    /// Shard Utama untuk transaksi mutasi (INSERT, UPDATE, DELETE) dengan WAL mode
    PrimaryWrite,
    /// Replika terisolasi untuk query pembacaan latensi rendah, pencarian, & FTS
    FastRead,
    /// Replika terisolasi untuk query agregasi berat, ekspor laporan, dan batch PDF
    AnalyticsReport,
    /// Hot Standby pasif siap dipromosikan seketika untuk pemulihan bencana (HA)
    HotStandby,
}

impl ShardRole {
    pub fn suffix(&self) -> &'static str {
        match self {
            ShardRole::PrimaryWrite => "",
            ShardRole::FastRead => "_read",
            ShardRole::AnalyticsReport => "_analytics",
            ShardRole::HotStandby => "_standby",
        }
    }

    pub fn folder(&self) -> &'static str {
        match self {
            ShardRole::PrimaryWrite => "shards",
            ShardRole::FastRead | ShardRole::AnalyticsReport | ShardRole::HotStandby => "clones",
        }
    }
}

impl fmt::Display for ShardRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShardRole::PrimaryWrite => write!(f, "PRIMARY_WRITE"),
            ShardRole::FastRead => write!(f, "FAST_READ"),
            ShardRole::AnalyticsReport => write!(f, "ANALYTICS_REPORT"),
            ShardRole::HotStandby => write!(f, "HOT_STANDBY"),
        }
    }
}

/// Trait ShardKey untuk identifikasi partisi/tenant generik
pub trait ShardKey: AsRef<str> + fmt::Display + Send + Sync + 'static {}

// Implementasi blanket untuk tipe string umum
impl ShardKey for String {}
impl ShardKey for &'static str {}

/// Telemetri untuk berkas kloning replika
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaTelemetry {
    pub role: ShardRole,
    pub filename: String,
    pub size_bytes: u64,
    pub exists: bool,
}

/// Telemetri untuk database global
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalTelemetry {
    pub primary_filename: String,
    pub primary_size_bytes: u64,
    pub table_counts: Vec<(String, usize)>,
    pub replicas: Vec<ReplicaTelemetry>,
}

/// Telemetri untuk satu shard data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardTelemetry {
    pub shard_key: String,
    pub primary_filename: String,
    pub primary_size_bytes: u64,
    pub table_counts: Vec<(String, usize)>,
    pub replicas: Vec<ReplicaTelemetry>,
    pub is_corrupt: bool,
}

/// Telemetri menyeluruh seluruh cluster SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTelemetry {
    pub wal_mode_active: bool,
    pub global: GlobalTelemetry,
    pub shards: Vec<ShardTelemetry>,
    pub total_disk_bytes: u64,
    pub last_updated_rfc3339: String,
}
