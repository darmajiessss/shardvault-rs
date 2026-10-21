// E:\shardvault-rs\src\migration\ddl.rs
// Introspeksi Skema SQLite, Schema Manifest, & Deteksi Schema Drift

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ShardVaultError;

/// Representasi kolom dalam tabel SQLite
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColumnInfo {
    pub cid: i32,
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub pk_order: i32,
}

/// Representasi konstrain Foreign Key
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForeignKeyInfo {
    pub id: i32,
    pub seq: i32,
    pub target_table: String,
    pub from_column: String,
    pub to_column: String,
    pub on_update: String,
    pub on_delete: String,
}

/// Representasi Indeks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub origin: String,
    pub columns: Vec<String>,
}

/// Representasi lengkap satu tabel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableSchema {
    pub name: String,
    pub sql: String,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub indexes: Vec<IndexInfo>,
}

/// Manifest Skema Lengkap untuk mendeteksi keselarasan (drift) antar database/shard
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaManifest {
    pub hash_sha256: String,
    pub tables: Vec<TableSchema>,
    pub views: Vec<String>,
    pub triggers: Vec<String>,
}

/// Introspeksi skema koneksi SQLite menjadi SchemaManifest terstruktur
pub fn introspect_schema(conn: &Connection) -> Result<SchemaManifest, ShardVaultError> {
    // 1. Ambil daftar tabel (lewati tabel sistem sqlite_)
    let mut stmt = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master 
             WHERE type='table' AND name NOT LIKE 'sqlite_%' 
             ORDER BY name ASC;",
        )
        .map_err(|e| ShardVaultError::Migration(format!("Gagal membaca tabel: {}", e)))?;

    let table_rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| ShardVaultError::Migration(format!("Query map tabel gagal: {}", e)))?;

    let mut tables = Vec::new();
    for row in table_rows {
        let (tbl_name, sql_opt) = row.map_err(|e| ShardVaultError::Migration(e.to_string()))?;
        let table_sql = sql_opt.unwrap_or_default();

        // Ambil info kolom via pragma_table_info
        let pragma_cols_sql = format!("PRAGMA table_info('{}');", tbl_name.replace('\'', "''"));
        let mut col_stmt = conn
            .prepare(&pragma_cols_sql)
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let col_rows = col_stmt
            .query_map([], |c_row| {
                Ok(ColumnInfo {
                    cid: c_row.get(0)?,
                    name: c_row.get(1)?,
                    data_type: c_row.get(2)?,
                    not_null: c_row.get::<_, i32>(3)? != 0,
                    default_value: c_row.get(4)?,
                    pk_order: c_row.get(5)?,
                })
            })
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let mut columns = Vec::new();
        for c in col_rows {
            columns.push(c.map_err(|e| ShardVaultError::Migration(e.to_string()))?);
        }

        // Ambil foreign keys
        let pragma_fk_sql = format!("PRAGMA foreign_key_list('{}');", tbl_name.replace('\'', "''"));
        let mut fk_stmt = conn
            .prepare(&pragma_fk_sql)
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let fk_rows = fk_stmt
            .query_map([], |fk_row| {
                Ok(ForeignKeyInfo {
                    id: fk_row.get(0)?,
                    seq: fk_row.get(1)?,
                    target_table: fk_row.get(2)?,
                    from_column: fk_row.get(3)?,
                    to_column: fk_row.get(4)?,
                    on_update: fk_row.get(5)?,
                    on_delete: fk_row.get(6)?,
                })
            })
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let mut foreign_keys = Vec::new();
        for fk in fk_rows {
            foreign_keys.push(fk.map_err(|e| ShardVaultError::Migration(e.to_string()))?);
        }

        // Ambil indexes
        let pragma_idx_sql = format!("PRAGMA index_list('{}');", tbl_name.replace('\'', "''"));
        let mut idx_stmt = conn
            .prepare(&pragma_idx_sql)
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let idx_rows = idx_stmt
            .query_map([], |i_row| {
                Ok((
                    i_row.get::<_, String>(1)?,
                    i_row.get::<_, i32>(2)? != 0,
                    i_row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

        let mut indexes = Vec::new();
        for idx in idx_rows {
            let (idx_name, unique, origin) = idx.map_err(|e| ShardVaultError::Migration(e.to_string()))?;
            // Ambil kolom di dalam index
            let pragma_info_sql = format!("PRAGMA index_info('{}');", idx_name.replace('\'', "''"));
            let mut info_stmt = conn
                .prepare(&pragma_info_sql)
                .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

            let info_rows = info_stmt
                .query_map([], |r| r.get::<_, String>(2))
                .map_err(|e| ShardVaultError::Migration(e.to_string()))?;

            let mut idx_columns = Vec::new();
            for cn in info_rows.flatten() {
                idx_columns.push(cn);
            }

            indexes.push(IndexInfo {
                name: idx_name,
                unique,
                origin,
                columns: idx_columns,
            });
        }

        tables.push(TableSchema {
            name: tbl_name,
            sql: table_sql,
            columns,
            foreign_keys,
            indexes,
        });
    }

    // 2. Ambil Views
    let mut v_stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='view' ORDER BY name ASC;")
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;
    let view_rows = v_stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;
    let mut views = Vec::new();
    for v in view_rows {
        views.push(v.map_err(|e| ShardVaultError::Migration(e.to_string()))?);
    }

    // 3. Ambil Triggers
    let mut trg_stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name NOT LIKE 'sqlite_%' ORDER BY name ASC;")
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;
    let trg_rows = trg_stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| ShardVaultError::Migration(e.to_string()))?;
    let mut triggers = Vec::new();
    for t in trg_rows {
        triggers.push(t.map_err(|e| ShardVaultError::Migration(e.to_string()))?);
    }

    // 4. Hitung hash kanonikal
    let mut hasher = Sha256::new();
    for tbl in &tables {
        hasher.update(tbl.name.as_bytes());
        for col in &tbl.columns {
            hasher.update(col.name.as_bytes());
            hasher.update(col.data_type.as_bytes());
            hasher.update([col.not_null as u8, col.pk_order as u8]);
        }
    }
    for v in &views {
        hasher.update(v.as_bytes());
    }
    for t in &triggers {
        hasher.update(t.as_bytes());
    }
    let hash_sha256 = format!("{:x}", hasher.finalize());

    Ok(SchemaManifest {
        hash_sha256,
        tables,
        views,
        triggers,
    })
}

/// Deteksi perbedaan (drift) antara dua schema manifest
pub fn detect_schema_drift(source: &SchemaManifest, target: &SchemaManifest) -> Vec<String> {
    let mut differences = Vec::new();

    if source.hash_sha256 == target.hash_sha256 {
        return differences;
    }

    // Cek tabel yang hilang di target
    for src_tbl in &source.tables {
        if let Some(tgt_tbl) = target.tables.iter().find(|t| t.name == src_tbl.name) {
            // Cek perbedaan kolom
            for src_col in &src_tbl.columns {
                if let Some(tgt_col) = tgt_tbl.columns.iter().find(|c| c.name == src_col.name) {
                    if src_col.data_type.to_uppercase() != tgt_col.data_type.to_uppercase() {
                        differences.push(format!(
                            "Tabel '{}' kolom '{}' tipe berbeda: {} vs {}",
                            src_tbl.name, src_col.name, src_col.data_type, tgt_col.data_type
                        ));
                    }
                } else {
                    differences.push(format!(
                        "Tabel '{}' kekurangan kolom '{}' di target",
                        src_tbl.name, src_col.name
                    ));
                }
            }
        } else {
            differences.push(format!("Tabel '{}' tidak ditemukan di target", src_tbl.name));
        }
    }

    // Cek tabel yang berlebih di target
    for tgt_tbl in &target.tables {
        if !source.tables.iter().any(|t| t.name == tgt_tbl.name) {
            differences.push(format!("Target memiliki tabel tambahan '{}'", tgt_tbl.name));
        }
    }

    differences
}
