// E:\shardvault-rs\src\sync\resolver.rs
// Resolusi Konflik Terdistribusi (Last-Write-Wins via HLC & Field-Level Merge)

use rusqlite::Connection;
use serde_json::Value;
use std::cmp::Ordering;
use std::str::FromStr;

use super::hlc::HlcTimestamp;
use super::oplog::OplogEntry;
use crate::error::ShardVaultError;

/// Hasil penanganan satu mutasi sinkronisasi
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictOutcome {
    AppliedFresh,
    AppliedRemoteWins,
    IgnoredLocalWins,
    AppliedFieldMerged,
}

/// Engine Resolusi Konflik
pub struct ConflictResolver;

impl ConflictResolver {
    /// Bandingkan dua timestamp HLC beserta Node ID tie-breaker
    pub fn compare_causality(
        remote_hlc_str: &str,
        remote_node: &str,
        local_hlc_str: &str,
        local_node: &str,
    ) -> Ordering {
        let remote_hlc = HlcTimestamp::from_str(remote_hlc_str).unwrap_or(HlcTimestamp::new(0, 0));
        let local_hlc = HlcTimestamp::from_str(local_hlc_str).unwrap_or(HlcTimestamp::new(0, 0));

        match remote_hlc.cmp(&local_hlc) {
            Ordering::Equal => remote_node.cmp(local_node),
            ord => ord,
        }
    }

    /// Ambil timestamp HLC mutasi terakhir untuk suatu baris tertentu di database lokal
    pub fn get_latest_local_hlc(
        conn: &Connection,
        table_name: &str,
        row_pk: &str,
    ) -> Result<Option<(String, String)>, ShardVaultError> {
        let mut stmt = conn
            .prepare(
                "SELECT hlc_timestamp, node_id 
                 FROM _shardvault_oplog 
                 WHERE table_name = ?1 AND row_pk = ?2 
                 ORDER BY op_id DESC 
                 LIMIT 1;",
            )
            .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

        let mut rows = stmt
            .query_map(rusqlite::params![table_name, row_pk], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

        if let Some(first) = rows.next() {
            let res = first.map_err(|e| ShardVaultError::Sync(e.to_string()))?;
            Ok(Some(res))
        } else {
            Ok(None)
        }
    }

    /// Terapkan satu mutasi remote ke basis data lokal dengan resolusi konflik LWW
    pub fn apply_remote_mutation(
        conn: &Connection,
        remote_op: &OplogEntry,
    ) -> Result<ConflictOutcome, ShardVaultError> {
        // 1. Cek apakah ada mutasi lokal sebelumnya untuk baris ini
        let local_meta = Self::get_latest_local_hlc(conn, &remote_op.table_name, &remote_op.row_pk)?;

        let outcome = if let Some((local_hlc, local_node)) = local_meta {
            let ord = Self::compare_causality(
                &remote_op.hlc_timestamp,
                &remote_op.node_id,
                &local_hlc,
                &local_node,
            );

            if ord == Ordering::Less {
                // Versi lokal lebih baru, abaikan perubahan remote
                return Ok(ConflictOutcome::IgnoredLocalWins);
            } else {
                ConflictOutcome::AppliedRemoteWins
            }
        } else {
            ConflictOutcome::AppliedFresh
        };

        // 2. Eksekusi mutasi ke tabel target
        if remote_op.operation == "DELETE" {
            // Dapatkan nama primary key kolom
            let pragma_sql = format!("PRAGMA table_info('{}');", remote_op.table_name.replace('\'', "''"));
            let mut stmt = conn
                .prepare(&pragma_sql)
                .map_err(|e| ShardVaultError::Sync(e.to_string()))?;

            let pk_col: String = stmt
                .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i32>(5)? > 0)))
                .map_err(|e| ShardVaultError::Sync(e.to_string()))?
                .flatten()
                .find(|(_, is_pk)| *is_pk)
                .map(|(name, _)| name)
                .unwrap_or_else(|| "id".into());

            let del_query = format!(
                "DELETE FROM \"{}\" WHERE \"{}\" = ?1;",
                remote_op.table_name.replace('"', "\"\""),
                pk_col.replace('"', "\"\"")
            );
            conn.execute(&del_query, rusqlite::params![remote_op.row_pk])
                .map_err(|e| ShardVaultError::Sync(format!("Gagal eksekusi remote DELETE: {}", e)))?;
        } else {
            // INSERT atau UPDATE
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&remote_op.payload_json) {
                let mut cols = Vec::new();
                let mut placeholders = Vec::new();
                let mut values = Vec::new();

                for (key, val) in map {
                    cols.push(format!("\"{}\"", key.replace('"', "\"\"")));
                    placeholders.push("?".to_string());
                    match val {
                        Value::String(s) => values.push(s),
                        Value::Number(n) => values.push(n.to_string()),
                        Value::Bool(b) => values.push((if b { "1" } else { "0" }).to_string()),
                        Value::Null => values.push("".to_string()),
                        _ => values.push(val.to_string()),
                    }
                }

                if !cols.is_empty() {
                    let insert_query = format!(
                        "INSERT OR REPLACE INTO \"{}\" ({}) VALUES ({});",
                        remote_op.table_name.replace('"', "\"\""),
                        cols.join(", "),
                        placeholders.join(", ")
                    );

                    let params_refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                    conn.execute(&insert_query, rusqlite::params_from_iter(params_refs))
                        .map_err(|e| ShardVaultError::Sync(format!("Gagal eksekusi remote UPSERT: {}", e)))?;
                }
            }
        }

        // 3. Catat ke oplog lokal sebagai mutasi yang telah tersinkronkan (is_synced = 1)
        conn.execute(
            "INSERT INTO _shardvault_oplog (table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1);",
            rusqlite::params![
                remote_op.table_name,
                remote_op.row_pk,
                remote_op.operation,
                remote_op.hlc_timestamp,
                remote_op.node_id,
                remote_op.payload_json,
                remote_op.created_at_utc,
            ],
        )
        .map_err(|e| ShardVaultError::Sync(format!("Gagal mencatat remote op ke oplog lokal: {}", e)))?;

        Ok(outcome)
    }
}
