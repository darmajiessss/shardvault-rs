// E:\shardvault-rs\src\sync\oplog.rs
// Change Data Capture (CDC) Oplog & Generator Trigger Otomatis

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::ShardVaultError;

/// Entitas satu mutasi data di dalam tabel oplog
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OplogEntry {
    pub op_id: i64,
    pub table_name: String,
    pub row_pk: String,
    pub operation: String, // "INSERT", "UPDATE", "DELETE"
    pub hlc_timestamp: String,
    pub node_id: String,
    pub payload_json: String,
    pub created_at_utc: String,
    pub is_synced: bool,
}

/// Inisialisasi tabel _shardvault_oplog
pub fn init_oplog_table(conn: &Connection) -> Result<(), ShardVaultError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _shardvault_oplog (
            op_id INTEGER PRIMARY KEY AUTOINCREMENT,
            table_name TEXT NOT NULL,
            row_pk TEXT NOT NULL,
            operation TEXT NOT NULL,
            hlc_timestamp TEXT NOT NULL,
            node_id TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_utc TEXT NOT NULL,
            is_synced INTEGER DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_oplog_hlc ON _shardvault_oplog(hlc_timestamp);
        CREATE INDEX IF NOT EXISTS idx_oplog_synced ON _shardvault_oplog(is_synced);",
    )
    .map_err(|e| ShardVaultError::Sync(format!("Gagal membuat tabel _shardvault_oplog: {}", e)))?;

    Ok(())
}

/// Pasang trigger CDC otomatis untuk tabel tertentu agar setiap mutasi tercatat ke _shardvault_oplog
pub fn setup_table_cdc_triggers(
    conn: &Connection,
    table_name: &str,
    node_id: &str,
) -> Result<(), ShardVaultError> {
    init_oplog_table(conn)?;

    // Baca kolom-kolom tabel
    let pragma_sql = format!("PRAGMA table_info('{}');", table_name.replace('\'', "''"));
    let mut stmt = conn
        .prepare(&pragma_sql)
        .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

    let cols: Vec<(String, bool)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i32>(5)? > 0)))
        .map_err(|e| ShardVaultError::Sync(e.to_string()))?
        .flatten()
        .collect();

    if cols.is_empty() {
        return Err(ShardVaultError::Sync(format!("Tabel '{}' tidak ditemukan atau tidak memiliki kolom", table_name)));
    }

    // Tentukan Primary Key kolom (default ke kolom pertama jika tidak ada explicit PK)
    let pk_col = cols
        .iter()
        .find(|(_, is_pk)| *is_pk)
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| cols[0].0.clone());

    // Susun ekspresi json_object untuk NEW dan OLD
    let mut new_json_parts = Vec::new();
    for (c, _) in &cols {
        new_json_parts.push(format!("'{}', NEW.{}", c, c));
    }
    let new_json_expr = format!("json_object({})", new_json_parts.join(", "));

    let ins_trg_name = format!("_trg_cdc_{}_ins", table_name);
    let upd_trg_name = format!("_trg_cdc_{}_upd", table_name);
    let del_trg_name = format!("_trg_cdc_{}_del", table_name);

    let ddl = format!(
        "DROP TRIGGER IF EXISTS {ins_trg};
        DROP TRIGGER IF EXISTS {upd_trg};
        DROP TRIGGER IF EXISTS {del_trg};

        CREATE TRIGGER {ins_trg}
        AFTER INSERT ON {tbl}
        FOR EACH ROW
        BEGIN
            INSERT INTO _shardvault_oplog (table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced)
            VALUES (
                '{tbl}',
                CAST(NEW.{pk} AS TEXT),
                'INSERT',
                (CAST((strftime('%s', 'now') || '000') AS INTEGER) || '.0'),
                '{node}',
                {new_json},
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                0
            );
        END;

        CREATE TRIGGER {upd_trg}
        AFTER UPDATE ON {tbl}
        FOR EACH ROW
        BEGIN
            INSERT INTO _shardvault_oplog (table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced)
            VALUES (
                '{tbl}',
                CAST(NEW.{pk} AS TEXT),
                'UPDATE',
                (CAST((strftime('%s', 'now') || '000') AS INTEGER) || '.0'),
                '{node}',
                {new_json},
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                0
            );
        END;

        CREATE TRIGGER {del_trg}
        AFTER DELETE ON {tbl}
        FOR EACH ROW
        BEGIN
            INSERT INTO _shardvault_oplog (table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced)
            VALUES (
                '{tbl}',
                CAST(OLD.{pk} AS TEXT),
                'DELETE',
                (CAST((strftime('%s', 'now') || '000') AS INTEGER) || '.0'),
                '{node}',
                json_object('{pk}', OLD.{pk}, '_tombstone', 1),
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                0
            );
        END;",
        ins_trg = ins_trg_name,
        upd_trg = upd_trg_name,
        del_trg = del_trg_name,
        tbl = table_name,
        pk = pk_col,
        node = node_id,
        new_json = new_json_expr
    );

    conn.execute_batch(&ddl)
        .map_err(|e| ShardVaultError::Sync(format!("Gagal membuat trigger CDC untuk tabel '{}': {}", table_name, e)))?;

    Ok(())
}

/// Ambil daftar mutasi oplog yang belum disinkronkan
pub fn get_unsynced_oplog(conn: &Connection, limit: usize) -> Result<Vec<OplogEntry>, ShardVaultError> {
    init_oplog_table(conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT op_id, table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced 
             FROM _shardvault_oplog 
             WHERE is_synced = 0 
             ORDER BY op_id ASC 
             LIMIT ?1;",
        )
        .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(OplogEntry {
                op_id: r.get(0)?,
                table_name: r.get(1)?,
                row_pk: r.get(2)?,
                operation: r.get(3)?,
                hlc_timestamp: r.get(4)?,
                node_id: r.get(5)?,
                payload_json: r.get(6)?,
                created_at_utc: r.get(7)?,
                is_synced: r.get::<_, i32>(8)? != 0,
            })
        })
        .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| ShardVaultError::Sync(e.to_string()))?);
    }

    Ok(entries)
}

/// Tandai daftar op_id sebagai telah berhasil disinkronkan (is_synced = 1)
pub fn mark_oplog_synced(conn: &Connection, op_ids: &[i64]) -> Result<(), ShardVaultError> {
    if op_ids.is_empty() {
        return Ok(());
    }

    let id_strings: Vec<String> = op_ids.iter().map(|id| id.to_string()).collect();
    let query = format!(
        "UPDATE _shardvault_oplog SET is_synced = 1 WHERE op_id IN ({});",
        id_strings.join(",")
    );

    conn.execute(&query, [])
        .map_err(|e| ShardVaultError::Sync(format!("Gagal memperbarui status synced oplog: {}", e)))?;

    Ok(())
}

/// Bersihkan entri oplog lama yang sudah disinkronkan untuk menghemat disk
pub fn purge_synced_oplog(conn: &Connection, retention_days: u32) -> Result<usize, ShardVaultError> {
    init_oplog_table(conn)?;
    let query = format!(
        "DELETE FROM _shardvault_oplog WHERE is_synced = 1 AND created_at_utc < date('now', '-{} days');",
        retention_days
    );

    let deleted = conn
        .execute(&query, [])
        .map_err(|e| ShardVaultError::Sync(format!("Gagal membersihkan oplog usang: {}", e)))?;

    Ok(deleted)
}
