// E:\shardvault-rs\src\codegen\diff_gen.rs
// Smart Schema Diff Engine & Generator Migrasi (Fast ALTER TABLE + SQLite 12-Step Safe Rebuild)

use serde::{Deserialize, Serialize};

use super::ast::{ForeignKeyAction, SchemaBlueprint, SqliteAffinity, TableBlueprint, TableScope};
use super::ddl_gen::{compile_table_ddl, quote_ident, SqlDialect};
use crate::migration::ddl::{SchemaManifest, TableSchema};

/// Jenis perubahan terdeteksi oleh Smart Schema Differ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffActionKind {
    CreateTable,
    AlterAddColumn,
    RebuildTable12Step,
    DropColumn,
    DropTable,
}

/// Rincian satu aksi perubahan skema
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffActionItem {
    pub kind: DiffActionKind,
    pub table_name: String,
    pub description: String,
}

/// Hasil komparasi lengkap antara `SchemaBlueprint` (Visual Editor) dan `SchemaManifest` (Database Aktif)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDiffPlan {
    pub items: Vec<DiffActionItem>,
    pub migration_sql: String,
    pub requires_rebuild: bool,
}

impl SchemaDiffPlan {
    pub fn is_up_to_date(&self) -> bool {
        self.items.is_empty()
    }
}

/// Mengecek apakah perubahan antara tabel di database (`live`) dan `blueprint` membutuhkan SQLite 12-Step Table Rebuild
fn needs_table_rebuild(live: &TableSchema, bp: &TableBlueprint) -> Option<String> {
    // 1. Cek perbedaan jumlah atau isi Foreign Key
    if live.foreign_keys.len() != bp.foreign_keys.len() {
        return Some(format!(
            "Jumlah Foreign Key berubah ({} -> {})",
            live.foreign_keys.len(),
            bp.foreign_keys.len()
        ));
    }

    for bp_fk in &bp.foreign_keys {
        let matched = live.foreign_keys.iter().any(|lfk| {
            lfk.from_column.eq_ignore_ascii_case(&bp_fk.from_column)
                && lfk.target_table.eq_ignore_ascii_case(&bp_fk.ref_table)
                && lfk.to_column.eq_ignore_ascii_case(&bp_fk.ref_column)
                && ForeignKeyAction::from_sql_str(&lfk.on_delete) == bp_fk.on_delete
                && ForeignKeyAction::from_sql_str(&lfk.on_update) == bp_fk.on_update
        });
        if !matched {
            return Some(format!(
                "Definisi Foreign Key '{}' ({} -> {}.{}) ditambahkan atau diubah",
                bp_fk.name, bp_fk.from_column, bp_fk.ref_table, bp_fk.ref_column
            ));
        }
    }

    // 2. Cek perubahan tipe data, Primary Key, atau NOT NULL pada kolom yang sudah ada
    for bp_col in &bp.columns {
        if let Some(live_col) = live
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&bp_col.name))
        {
            let live_affinity = SqliteAffinity::from_sql_type(&live_col.data_type);
            if live_affinity != bp_col.col_type {
                return Some(format!(
                    "Tipe kolom '{}.{}' berubah ({} -> {})",
                    bp.name, bp_col.name, live_affinity, bp_col.col_type
                ));
            }
            let live_pk = live_col.pk_order > 0;
            if live_pk != bp_col.is_primary_key {
                return Some(format!(
                    "Status Primary Key kolom '{}.{}' berubah",
                    bp.name, bp_col.name
                ));
            }
            let live_nn = live_col.not_null || live_pk;
            let bp_nn = bp_col.not_null || bp_col.is_primary_key;
            if live_nn != bp_nn {
                return Some(format!(
                    "Konstrain NOT NULL kolom '{}.{}' berubah ({} -> {})",
                    bp.name, bp_col.name, live_nn, bp_nn
                ));
            }
        } else {
            // Kolom baru yang NOT NULL tetapi tanpa DEFAULT membutuhkan rebuild atau fallback value
            if bp_col.not_null && bp_col.default_expr.is_none() && !bp_col.is_primary_key {
                return Some(format!(
                    "Kolom baru '{}.{}' memiliki NOT NULL tanpa DEFAULT",
                    bp.name, bp_col.name
                ));
            }
        }
    }

    None
}

/// Menghasilkan skrip SQLite 12-Step Safe Table Rebuild untuk tabel yang dimodifikasi
fn generate_12_step_rebuild_sql(live: &TableSchema, bp: &TableBlueprint) -> String {
    let mut sql = String::new();
    let temp_name = format!("_sv_rebuild_{}", bp.name);

    sql.push_str(&format!(
        "-- [12-STEP SAFE TABLE REBUILD] Tabel '{}' (Mempertahankan seluruh data eksisting)\n",
        bp.name
    ));

    // Hapus trigger CDC sementara jika ada
    sql.push_str(&format!(
        "DROP TRIGGER IF EXISTS \"_sv_cdc_{}_insert\";\n\
         DROP TRIGGER IF EXISTS \"_sv_cdc_{}_update\";\n\
         DROP TRIGGER IF EXISTS \"_sv_cdc_{}_delete\";\n",
        bp.name, bp.name, bp.name
    ));

    // Buat tabel sementara tanpa index terlebih dahulu
    let mut temp_bp = bp.clone();
    temp_bp.name = temp_name.clone();
    temp_bp.indexes.clear();
    for fk in &mut temp_bp.foreign_keys {
        fk.auto_index_fk = false;
    }
    sql.push_str(&compile_table_ddl(&temp_bp, SqlDialect::Sqlite));

    // Susun daftar kolom untuk menyalin data lama ke tabel baru
    let mut insert_cols = Vec::new();
    let mut select_exprs = Vec::new();

    for bp_col in &bp.columns {
        let col_q = quote_ident(&bp_col.name, SqlDialect::Sqlite);
        if live
            .columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&bp_col.name))
        {
            insert_cols.push(col_q.clone());
            select_exprs.push(col_q);
        } else if bp_col.not_null {
            // Kolom baru yang NOT NULL: gunakan default_expr atau nilai fallback aman
            insert_cols.push(col_q);
            let fallback = bp_col.default_expr.clone().unwrap_or_else(|| {
                match bp_col.col_type {
                    SqliteAffinity::Integer | SqliteAffinity::Real | SqliteAffinity::Numeric => {
                        "0".to_string()
                    }
                    SqliteAffinity::Text => "''".to_string(),
                    SqliteAffinity::Blob => "X''".to_string(),
                }
            });
            select_exprs.push(fallback);
        }
    }

    if !insert_cols.is_empty() {
        let temp_q = quote_ident(&temp_name, SqlDialect::Sqlite);
        let orig_q = quote_ident(&bp.name, SqlDialect::Sqlite);
        sql.push_str(&format!(
            "INSERT INTO {} ({}) SELECT {} FROM {};\n",
            temp_q,
            insert_cols.join(", "),
            select_exprs.join(", "),
            orig_q
        ));
    }

    let temp_q = quote_ident(&temp_name, SqlDialect::Sqlite);
    let orig_q = quote_ident(&bp.name, SqlDialect::Sqlite);
    sql.push_str(&format!("DROP TABLE {};\n", orig_q));
    sql.push_str(&format!("ALTER TABLE {} RENAME TO {};\n", temp_q, orig_q));

    // Buat kembali seluruh indeks dan Auto-Index FK pada tabel hasil rename
    for fk in &bp.foreign_keys {
        if fk.auto_index_fk {
            let idx_name = format!("idx_{}_{}_fk", bp.name, fk.from_column);
            let idx_q = quote_ident(&idx_name, SqlDialect::Sqlite);
            let col_q = quote_ident(&fk.from_column, SqlDialect::Sqlite);
            sql.push_str(&format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({});\n",
                idx_q, orig_q, col_q
            ));
        }
    }

    for idx in &bp.indexes {
        if idx.columns.is_empty() {
            continue;
        }
        let idx_q = quote_ident(&idx.name, SqlDialect::Sqlite);
        let cols_q: Vec<String> = idx
            .columns
            .iter()
            .map(|c| quote_ident(c, SqlDialect::Sqlite))
            .collect();
        let unique_kw = if idx.unique { "UNIQUE " } else { "" };
        sql.push_str(&format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({});\n",
            unique_kw,
            idx_q,
            orig_q,
            cols_q.join(", ")
        ));
    }

    sql
}

/// Bandingkan `SchemaBlueprint` terhadap `SchemaManifest` database aktif dan hasilkan rencana migrasi SQL
pub fn diff_blueprint_against_manifest(
    blueprint: &SchemaBlueprint,
    manifest: &SchemaManifest,
    scope_filter: Option<TableScope>,
    allow_destructive_drops: bool,
) -> SchemaDiffPlan {
    let mut items = Vec::new();
    let mut sql_blocks = Vec::new();
    let mut requires_rebuild = false;

    let target_tables: Vec<&TableBlueprint> = blueprint
        .tables
        .iter()
        .filter(|t| match scope_filter {
            Some(s) => t.scope == s || t.scope == TableScope::Both,
            None => true,
        })
        .collect();

    // 1. Cek tabel baru atau modifikasi pada tabel yang sudah ada
    for bp_tbl in &target_tables {
        match manifest
            .tables
            .iter()
            .find(|lt| lt.name.eq_ignore_ascii_case(&bp_tbl.name))
        {
            None => {
                // Tabel belum ada di DB -> CREATE TABLE
                items.push(DiffActionItem {
                    kind: DiffActionKind::CreateTable,
                    table_name: bp_tbl.name.clone(),
                    description: format!(
                        "Buat tabel baru '{}' ({} kolom, {} FK)",
                        bp_tbl.name,
                        bp_tbl.columns.len(),
                        bp_tbl.foreign_keys.len()
                    ),
                });
                sql_blocks.push(compile_table_ddl(bp_tbl, SqlDialect::Sqlite));
            }
            Some(live_tbl) => {
                // Cek apakah membutuhkan 12-Step Rebuild (misal tambah/ubah Foreign Key, ubah tipe, ubah PK/NOT NULL)
                if let Some(reason) = needs_table_rebuild(live_tbl, bp_tbl) {
                    requires_rebuild = true;
                    items.push(DiffActionItem {
                        kind: DiffActionKind::RebuildTable12Step,
                        table_name: bp_tbl.name.clone(),
                        description: format!("12-Step Safe Rebuild '{}': {}", bp_tbl.name, reason),
                    });
                    sql_blocks.push(generate_12_step_rebuild_sql(live_tbl, bp_tbl));
                } else {
                    // Fast Path: ALTER TABLE ADD COLUMN / DROP COLUMN
                    let tbl_q = quote_ident(&bp_tbl.name, SqlDialect::Sqlite);
                    for bp_col in &bp_tbl.columns {
                        let exists = live_tbl
                            .columns
                            .iter()
                            .any(|c| c.name.eq_ignore_ascii_case(&bp_col.name));
                        if !exists {
                            items.push(DiffActionItem {
                                kind: DiffActionKind::AlterAddColumn,
                                table_name: bp_tbl.name.clone(),
                                description: format!(
                                    "Tambah kolom '{}.{}' ({})",
                                    bp_tbl.name, bp_col.name, bp_col.col_type
                                ),
                            });
                            let col_q = quote_ident(&bp_col.name, SqlDialect::Sqlite);
                            let mut alter_stmt = format!(
                                "ALTER TABLE {} ADD COLUMN {} {}",
                                tbl_q,
                                col_q,
                                bp_col.col_type.to_sqlite_str()
                            );
                            if bp_col.not_null {
                                alter_stmt.push_str(" NOT NULL");
                            }
                            if let Some(ref def_val) = bp_col.default_expr {
                                if !def_val.trim().is_empty() {
                                    alter_stmt.push_str(&format!(" DEFAULT {}", def_val.trim()));
                                }
                            }
                            alter_stmt.push_str(";\n");
                            sql_blocks.push(alter_stmt);
                        }
                    }

                    // Cek kolom yang dihapus
                    for live_col in &live_tbl.columns {
                        let still_exists = bp_tbl
                            .columns
                            .iter()
                            .any(|c| c.name.eq_ignore_ascii_case(&live_col.name));
                        if !still_exists {
                            items.push(DiffActionItem {
                                kind: DiffActionKind::DropColumn,
                                table_name: bp_tbl.name.clone(),
                                description: format!(
                                    "Hapus kolom '{}.{}'",
                                    bp_tbl.name, live_col.name
                                ),
                            });
                            let col_q = quote_ident(&live_col.name, SqlDialect::Sqlite);
                            if allow_destructive_drops {
                                sql_blocks.push(format!(
                                    "ALTER TABLE {} DROP COLUMN {};\n",
                                    tbl_q, col_q
                                ));
                            } else {
                                sql_blocks.push(format!(
                                    "-- [SAFE MODE: Aktifkan Destructive Drops untuk mengeksekusi]\n-- ALTER TABLE {} DROP COLUMN {};\n",
                                    tbl_q, col_q
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Cek tabel yang ada di DB tetapi dihapus dari Blueprint (kecuali tabel sistem _shardvault_*)
    for live_tbl in &manifest.tables {
        if live_tbl.name.starts_with("_shardvault_") {
            continue;
        }
        let in_blueprint = target_tables
            .iter()
            .any(|t| t.name.eq_ignore_ascii_case(&live_tbl.name));
        if !in_blueprint {
            items.push(DiffActionItem {
                kind: DiffActionKind::DropTable,
                table_name: live_tbl.name.clone(),
                description: format!("Hapus tabel '{}' dari database", live_tbl.name),
            });
            let tbl_q = quote_ident(&live_tbl.name, SqlDialect::Sqlite);
            if allow_destructive_drops {
                sql_blocks.push(format!("DROP TABLE IF EXISTS {};\n", tbl_q));
            } else {
                sql_blocks.push(format!(
                    "-- [SAFE MODE: Aktifkan Destructive Drops untuk mengeksekusi]\n-- DROP TABLE IF EXISTS {};\n",
                    tbl_q
                ));
            }
        }
    }

    let migration_sql = if sql_blocks.is_empty() {
        "-- Skema database sudah 100% sinkron dengan Schema Blueprint.".to_string()
    } else {
        format!(
            "-- Smart Diff Migration Script (Project: {} v{})\n\n{}",
            blueprint.project_name,
            blueprint.version,
            sql_blocks.join("\n")
        )
    };

    SchemaDiffPlan {
        items,
        migration_sql,
        requires_rebuild,
    }
}
