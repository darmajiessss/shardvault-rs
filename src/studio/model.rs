// E:\shardvault-rs\src\studio\model.rs
// Struktur Data & Model Status untuk ShardVault Studio

use rusqlite::Connection;
use std::fmt;

use crate::types::ShardRole;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedTarget {
    Global(ShardRole),
    Shard {
        key: String,
        role: ShardRole,
    },
}

impl SelectedTarget {
    pub fn display_title(&self) -> String {
        match self {
            SelectedTarget::Global(role) => format!("global.db [{}]", role),
            SelectedTarget::Shard { key, role } => format!("{}.db [{}]", key, role),
        }
    }

    pub fn shard_key(&self) -> Option<&str> {
        match self {
            SelectedTarget::Global(_) => None,
            SelectedTarget::Shard { key, .. } => Some(key.as_str()),
        }
    }

    pub fn role(&self) -> ShardRole {
        match self {
            SelectedTarget::Global(role) => *role,
            SelectedTarget::Shard { role, .. } => *role,
        }
    }
}

impl Default for SelectedTarget {
    fn default() -> Self {
        SelectedTarget::Global(ShardRole::PrimaryWrite)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveTab {
    DataBrowser,
    SqlConsole,
    SchemaInspector,
    ChaosStudio,
    Migrations,
    TransferHub,
    SyncCenter,
}

impl fmt::Display for ActiveTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveTab::DataBrowser => write!(f, "📊 Data Browser"),
            ActiveTab::SqlConsole => write!(f, "⚡ SQL Console"),
            ActiveTab::SchemaInspector => write!(f, "📐 Schema & DDL"),
            ActiveTab::ChaosStudio => write!(f, "🛡 Replication & Chaos"),
            ActiveTab::Migrations => write!(f, "🚀 Migrations"),
            ActiveTab::TransferHub => write!(f, "📦 Transfer (.svp)"),
            ActiveTab::SyncCenter => write!(f, "🔄 Sync & Oplog"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableMetadata {
    pub name: String,
    pub row_count: usize,
}

#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    pub cid: i32,
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub is_pk: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SqlExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub elapsed_ms: f64,
    pub rows_affected: usize,
    pub error_message: Option<String>,
}

/// Helper untuk mengambil daftar tabel dan jumlah baris dari koneksi SQLite aktif
pub fn fetch_tables(conn: &Connection) -> Vec<TableMetadata> {
    let mut stmt = match conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let table_names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let mut result = Vec::new();
    for name in table_names {
        let count_query = format!("SELECT COUNT(*) FROM \"{}\"", name);
        let count: usize = conn.query_row(&count_query, [], |r| r.get(0)).unwrap_or(0);
        result.push(TableMetadata {
            name,
            row_count: count,
        });
    }

    result
}

/// Helper untuk mengambil metadata kolom sebuah tabel
pub fn fetch_columns(conn: &Connection, table_name: &str) -> Vec<ColumnMetadata> {
    let pragma_query = format!("PRAGMA table_info(\"{}\")", table_name);
    let mut stmt = match conn.prepare(&pragma_query) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let cols = stmt
        .query_map([], |r| {
            Ok(ColumnMetadata {
                cid: r.get(0)?,
                name: r.get(1)?,
                data_type: r.get(2)?,
                not_null: r.get(3)?,
                is_pk: r.get(5)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    cols
}

/// Helper untuk mengambil DDL CREATE TABLE dari SQLite master
pub fn fetch_table_ddl(conn: &Connection, table_name: &str) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?1",
        [table_name],
        |r| r.get(0),
    )
    .unwrap_or_else(|_| "-- DDL tidak ditemukan".into())
}

/// Helper untuk mengambil indeks-indeks sebuah tabel
pub fn fetch_table_indexes(conn: &Connection, table_name: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name = ?1 AND sql IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([table_name], |r| r.get(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}
