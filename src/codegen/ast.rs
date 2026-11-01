// E:\shardvault-rs\src\codegen\ast.rs
// DrawDB + ShardVault Semantic Schema Blueprint AST, Reverse-Engineering, Templates & Issue Linter

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::ShardVaultError;
use crate::migration::ddl::{introspect_schema, SchemaManifest};

/// Cakupan penempatan tabel di dalam arsitektur ShardVault
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableScope {
    /// Hanya dibuat pada database `global_master.db`
    Global,
    /// Dibuat pada setiap database tenant/shard (`shard_{key}.db`)
    Shard,
    /// Dibuat baik di Global maupun di setiap Shard
    Both,
}

impl std::fmt::Display for TableScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableScope::Global => write!(f, "Global"),
            TableScope::Shard => write!(f, "Shard"),
            TableScope::Both => write!(f, "Both (Global + Shard)"),
        }
    }
}

/// Tipe data dasar (SQLite Affinity) yang dapat dikompilasi lintas dialek SQL & bahasa pemrograman
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqliteAffinity {
    Integer,
    Text,
    Real,
    Blob,
    Numeric,
}

impl SqliteAffinity {
    pub fn all() -> &'static [SqliteAffinity] {
        &[
            SqliteAffinity::Integer,
            SqliteAffinity::Text,
            SqliteAffinity::Real,
            SqliteAffinity::Blob,
            SqliteAffinity::Numeric,
        ]
    }

    pub fn from_sql_type(raw: &str) -> Self {
        let up = raw.trim().to_uppercase();
        if up.contains("INT") {
            SqliteAffinity::Integer
        } else if up.contains("CHAR") || up.contains("CLOB") || up.contains("TEXT") || up.is_empty() {
            SqliteAffinity::Text
        } else if up.contains("BLOB") {
            SqliteAffinity::Blob
        } else if up.contains("REAL") || up.contains("FLOA") || up.contains("DOUB") {
            SqliteAffinity::Real
        } else {
            SqliteAffinity::Numeric
        }
    }

    pub fn to_sqlite_str(self) -> &'static str {
        match self {
            SqliteAffinity::Integer => "INTEGER",
            SqliteAffinity::Text => "TEXT",
            SqliteAffinity::Real => "REAL",
            SqliteAffinity::Blob => "BLOB",
            SqliteAffinity::Numeric => "NUMERIC",
        }
    }

    pub fn to_postgres_str(self, is_auto_inc: bool) -> &'static str {
        if is_auto_inc && self == SqliteAffinity::Integer {
            return "BIGSERIAL";
        }
        match self {
            SqliteAffinity::Integer => "BIGINT",
            SqliteAffinity::Text => "TEXT",
            SqliteAffinity::Real => "DOUBLE PRECISION",
            SqliteAffinity::Blob => "BYTEA",
            SqliteAffinity::Numeric => "NUMERIC(18, 4)",
        }
    }

    pub fn to_mysql_str(self, is_pk_or_indexed: bool) -> &'static str {
        match self {
            SqliteAffinity::Integer => "BIGINT",
            SqliteAffinity::Text => {
                if is_pk_or_indexed {
                    "VARCHAR(255)"
                } else {
                    "TEXT"
                }
            }
            SqliteAffinity::Real => "DOUBLE",
            SqliteAffinity::Blob => "LONGBLOB",
            SqliteAffinity::Numeric => "DECIMAL(18, 4)",
        }
    }

    pub fn to_rust_type(self) -> &'static str {
        match self {
            SqliteAffinity::Integer => "i64",
            SqliteAffinity::Text => "String",
            SqliteAffinity::Real => "f64",
            SqliteAffinity::Blob => "Vec<u8>",
            SqliteAffinity::Numeric => "f64",
        }
    }

    pub fn to_ts_type(self) -> &'static str {
        match self {
            SqliteAffinity::Integer | SqliteAffinity::Real | SqliteAffinity::Numeric => "number",
            SqliteAffinity::Text => "string",
            SqliteAffinity::Blob => "Uint8Array",
        }
    }
}

impl std::fmt::Display for SqliteAffinity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sqlite_str())
    }
}

/// Kardinalitas relasi antar tabel ala DrawDB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
}

impl Cardinality {
    pub fn all() -> &'static [Cardinality] {
        &[
            Cardinality::ManyToOne,
            Cardinality::OneToMany,
            Cardinality::OneToOne,
        ]
    }

    pub fn short_badge(self) -> &'static str {
        match self {
            Cardinality::OneToOne => "1 : 1",
            Cardinality::OneToMany => "1 : N",
            Cardinality::ManyToOne => "N : 1",
        }
    }
}

impl std::fmt::Display for Cardinality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cardinality::OneToOne => write!(f, "One-to-One (1:1)"),
            Cardinality::OneToMany => write!(f, "One-to-Many (1:N)"),
            Cardinality::ManyToOne => write!(f, "Many-to-One (N:1)"),
        }
    }
}

/// Aksi referensial Foreign Key (`ON DELETE` / `ON UPDATE`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForeignKeyAction {
    Cascade,
    Restrict,
    SetNull,
    SetDefault,
    NoAction,
}

impl ForeignKeyAction {
    pub fn all() -> &'static [ForeignKeyAction] {
        &[
            ForeignKeyAction::Cascade,
            ForeignKeyAction::Restrict,
            ForeignKeyAction::SetNull,
            ForeignKeyAction::SetDefault,
            ForeignKeyAction::NoAction,
        ]
    }

    pub fn from_sql_str(raw: &str) -> Self {
        match raw.trim().to_uppercase().as_str() {
            "CASCADE" => ForeignKeyAction::Cascade,
            "RESTRICT" => ForeignKeyAction::Restrict,
            "SET NULL" => ForeignKeyAction::SetNull,
            "SET DEFAULT" => ForeignKeyAction::SetDefault,
            _ => ForeignKeyAction::NoAction,
        }
    }

    pub fn to_sql_str(self) -> &'static str {
        match self {
            ForeignKeyAction::Cascade => "CASCADE",
            ForeignKeyAction::Restrict => "RESTRICT",
            ForeignKeyAction::SetNull => "SET NULL",
            ForeignKeyAction::SetDefault => "SET DEFAULT",
            ForeignKeyAction::NoAction => "NO ACTION",
        }
    }
}

impl std::fmt::Display for ForeignKeyAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sql_str())
    }
}

/// Definisi kolom pada Schema Blueprint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnBlueprint {
    pub name: String,
    pub col_type: SqliteAffinity,
    pub is_primary_key: bool,
    pub auto_increment: bool,
    pub not_null: bool,
    pub unique: bool,
    pub default_expr: Option<String>,
    pub check_expr: Option<String>,
    pub comment: String,
    /// Enkripsi otomatis menggunakan AES-256-GCM pada kode repository yang di-generate
    pub encrypted_aes: bool,
    /// Sertakan nilai kolom ini dalam perhitungan HMAC-SHA256 row integrity signature
    pub include_in_hmac: bool,
}

impl ColumnBlueprint {
    pub fn new(name: &str, col_type: SqliteAffinity) -> Self {
        Self {
            name: name.to_string(),
            col_type,
            is_primary_key: false,
            auto_increment: false,
            not_null: true,
            unique: false,
            default_expr: None,
            check_expr: None,
            comment: String::new(),
            encrypted_aes: false,
            include_in_hmac: false,
        }
    }

    pub fn primary_key_int(name: &str) -> Self {
        Self {
            name: name.to_string(),
            col_type: SqliteAffinity::Integer,
            is_primary_key: true,
            auto_increment: true,
            not_null: true,
            unique: true,
            default_expr: None,
            check_expr: None,
            comment: "Primary Key ID".to_string(),
            encrypted_aes: false,
            include_in_hmac: true,
        }
    }

    pub fn primary_key_text(name: &str) -> Self {
        Self {
            name: name.to_string(),
            col_type: SqliteAffinity::Text,
            is_primary_key: true,
            auto_increment: false,
            not_null: true,
            unique: true,
            default_expr: None,
            check_expr: None,
            comment: "Primary Key UUID/Kode".to_string(),
            encrypted_aes: false,
            include_in_hmac: true,
        }
    }
}

/// Definisi relasi Foreign Key antar tabel (DrawDB Relationship)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyBlueprint {
    pub name: String,
    pub from_column: String,
    pub ref_table: String,
    pub ref_column: String,
    pub cardinality: Cardinality,
    pub on_delete: ForeignKeyAction,
    pub on_update: ForeignKeyAction,
    /// Otomatis buat indeks pada kolom FK anak untuk mencegah full-table lock di SQLite
    pub auto_index_fk: bool,
}

impl ForeignKeyBlueprint {
    pub fn new(
        table_name: &str,
        from_column: &str,
        ref_table: &str,
        ref_column: &str,
        on_delete: ForeignKeyAction,
    ) -> Self {
        Self {
            name: format!("fk_{}_{}_{}", table_name, from_column, ref_table),
            from_column: from_column.to_string(),
            ref_table: ref_table.to_string(),
            ref_column: ref_column.to_string(),
            cardinality: Cardinality::ManyToOne,
            on_delete,
            on_update: ForeignKeyAction::Cascade,
            auto_index_fk: true,
        }
    }
}

/// Definisi Indeks tambahan pada tabel
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexBlueprint {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

/// Subject Area (Bounded Context Grouping Box) ala DrawDB pada kanvas ER
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectAreaBlueprint {
    pub name: String,
    pub color_hex: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Catatan Dokumentasi (Sticky Note) ala DrawDB pada kanvas ER
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteBlueprint {
    pub title: String,
    pub content: String,
    pub color_hex: String,
    pub x: f32,
    pub y: f32,
}

/// Definisi lengkap satu Tabel di dalam Blueprint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableBlueprint {
    pub name: String,
    pub scope: TableScope,
    pub comment: String,
    pub color_hex: String,
    pub x: f32,
    pub y: f32,
    pub enable_cdc: bool,
    pub without_rowid: bool,
    pub columns: Vec<ColumnBlueprint>,
    pub foreign_keys: Vec<ForeignKeyBlueprint>,
    pub indexes: Vec<IndexBlueprint>,
}

impl TableBlueprint {
    pub fn new(name: &str, scope: TableScope, x: f32, y: f32) -> Self {
        let color_hex = match scope {
            TableScope::Global => "#38bdf8".to_string(), // Sky Blue
            TableScope::Shard => "#a855f7".to_string(),  // Purple
            TableScope::Both => "#10b981".to_string(),   // Emerald
        };
        Self {
            name: name.to_string(),
            scope,
            comment: String::new(),
            color_hex,
            x,
            y,
            enable_cdc: scope == TableScope::Shard,
            without_rowid: false,
            columns: vec![ColumnBlueprint::primary_key_int("id")],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }
    }

    pub fn pk_column(&self) -> Option<&ColumnBlueprint> {
        self.columns.iter().find(|c| c.is_primary_key)
    }
}

/// Tingkat keparahan temuan pada DrawDB Schema Linter (`Issues` Panel)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

/// Temuan validasi skema (DrawDB Issue Detection)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaIssue {
    pub severity: IssueSeverity,
    pub target: String,
    pub message: String,
}

/// Dokumen utama Schema Blueprint (kompatibel untuk ekspor/impor JSON & generator multi-target)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaBlueprint {
    pub project_name: String,
    pub version: u32,
    pub tables: Vec<TableBlueprint>,
    pub subject_areas: Vec<SubjectAreaBlueprint>,
    pub notes: Vec<NoteBlueprint>,
}

impl Default for SchemaBlueprint {
    fn default() -> Self {
        Self::template_pos_retail()
    }
}

impl SchemaBlueprint {
    pub fn new_empty(project_name: &str) -> Self {
        Self {
            project_name: project_name.to_string(),
            version: 1,
            tables: Vec::new(),
            subject_areas: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Validasi komprehensif skema ala DrawDB "Issues" Detector
    pub fn validate_issues(&self) -> Vec<SchemaIssue> {
        let mut issues = Vec::new();
        let mut table_names = HashSet::new();

        if self.tables.is_empty() {
            issues.push(SchemaIssue {
                severity: IssueSeverity::Info,
                target: "Schema".to_string(),
                message: "Belum ada tabel yang didefinisikan di dalam skema.".to_string(),
            });
            return issues;
        }

        for tbl in &self.tables {
            if tbl.name.trim().is_empty() {
                issues.push(SchemaIssue {
                    severity: IssueSeverity::Error,
                    target: "<unnamed_table>".to_string(),
                    message: "Terdapat tabel dengan nama kosong.".to_string(),
                });
            }
            if !table_names.insert(tbl.name.to_lowercase()) {
                issues.push(SchemaIssue {
                    severity: IssueSeverity::Error,
                    target: tbl.name.clone(),
                    message: format!("Nama tabel '{}' duplikat.", tbl.name),
                });
            }

            if tbl.columns.is_empty() {
                issues.push(SchemaIssue {
                    severity: IssueSeverity::Error,
                    target: tbl.name.clone(),
                    message: format!("Tabel '{}' tidak memiliki kolom sama sekali.", tbl.name),
                });
            }

            // Cek Primary Key
            let pk_count = tbl.columns.iter().filter(|c| c.is_primary_key).count();
            if pk_count == 0 {
                issues.push(SchemaIssue {
                    severity: IssueSeverity::Warning,
                    target: tbl.name.clone(),
                    message: format!(
                        "Tabel '{}' tidak memiliki Primary Key (wajib jika menggunakan CDC/Sync).",
                        tbl.name
                    ),
                });
            }

            // Cek nama kolom duplikat
            let mut col_names = HashSet::new();
            for col in &tbl.columns {
                if col.name.trim().is_empty() {
                    issues.push(SchemaIssue {
                        severity: IssueSeverity::Error,
                        target: tbl.name.clone(),
                        message: format!("Tabel '{}' memiliki kolom tanpa nama.", tbl.name),
                    });
                }
                if !col_names.insert(col.name.to_lowercase()) {
                    issues.push(SchemaIssue {
                        severity: IssueSeverity::Error,
                        target: format!("{}.{}", tbl.name, col.name),
                        message: format!(
                            "Kolom '{}' didefinisikan lebih dari satu kali pada tabel '{}'.",
                            col.name, tbl.name
                        ),
                    });
                }
                if col.encrypted_aes && col.col_type != SqliteAffinity::Text {
                    issues.push(SchemaIssue {
                        severity: IssueSeverity::Warning,
                        target: format!("{}.{}", tbl.name, col.name),
                        message: format!(
                            "Kolom '{}' ditandai AES-256 Encrypted tetapi bertipe {} (disarankan TEXT untuk menyimpan format ciphertext 'v1:nonce:tag').",
                            col.name, col.col_type
                        ),
                    });
                }
            }

            // Cek Foreign Keys
            for fk in &tbl.foreign_keys {
                let local_col = tbl.columns.iter().find(|c| c.name == fk.from_column);
                if local_col.is_none() {
                    issues.push(SchemaIssue {
                        severity: IssueSeverity::Error,
                        target: format!("{}.{}", tbl.name, fk.from_column),
                        message: format!(
                            "Foreign Key '{}' merujuk kolom lokal '{}' yang tidak ada di tabel '{}'.",
                            fk.name, fk.from_column, tbl.name
                        ),
                    });
                }

                let parent_tbl = self.tables.iter().find(|t| t.name == fk.ref_table);
                match parent_tbl {
                    None => {
                        issues.push(SchemaIssue {
                            severity: IssueSeverity::Error,
                            target: format!("{} -> {}", tbl.name, fk.ref_table),
                            message: format!(
                                "Foreign Key '{}' merujuk tabel induk '{}' yang tidak ditemukan di Blueprint.",
                                fk.name, fk.ref_table
                            ),
                        });
                    }
                    Some(pt) => {
                        // Cek lintas cakupan fisik Global vs Shard
                        if (tbl.scope == TableScope::Global && pt.scope == TableScope::Shard)
                            || (tbl.scope == TableScope::Shard && pt.scope == TableScope::Global)
                        {
                            issues.push(SchemaIssue {
                                severity: IssueSeverity::Warning,
                                target: format!("{} -> {}", tbl.name, pt.name),
                                message: format!(
                                    "Relasi FK antara '{}' ({}) dan '{}' ({}) melintasi batas file database fisik (Global vs Shard).",
                                    tbl.name, tbl.scope, pt.name, pt.scope
                                ),
                            });
                        }

                        let parent_col = pt.columns.iter().find(|c| c.name == fk.ref_column);
                        match parent_col {
                            None => {
                                issues.push(SchemaIssue {
                                    severity: IssueSeverity::Error,
                                    target: format!("{}.{}", pt.name, fk.ref_column),
                                    message: format!(
                                        "Foreign Key '{}' merujuk kolom '{}' yang tidak ada pada tabel induk '{}'.",
                                        fk.name, fk.ref_column, pt.name
                                    ),
                                });
                            }
                            Some(pc) => {
                                if let Some(lc) = local_col {
                                    if lc.col_type != pc.col_type {
                                        issues.push(SchemaIssue {
                                            severity: IssueSeverity::Error,
                                            target: format!("{}.{} -> {}.{}", tbl.name, lc.name, pt.name, pc.name),
                                            message: format!(
                                                "Tipe data Foreign Key tidak cocok: '{}.{}' ({}) vs '{}.{}' ({}).",
                                                tbl.name, lc.name, lc.col_type, pt.name, pc.name, pc.col_type
                                            ),
                                        });
                                    }
                                }
                                if !pc.is_primary_key && !pc.unique {
                                    issues.push(SchemaIssue {
                                        severity: IssueSeverity::Warning,
                                        target: format!("{}.{}", pt.name, pc.name),
                                        message: format!(
                                            "Kolom induk '{}.{}' yang dirujuk Foreign Key sebaiknya merupakan PRIMARY KEY atau UNIQUE.",
                                            pt.name, pc.name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }

                // Cek apakah kolom FK memiliki indeks
                let has_explicit_idx = tbl
                    .indexes
                    .iter()
                    .any(|idx| idx.columns.first().map(|c| c == &fk.from_column).unwrap_or(false));
                if !fk.auto_index_fk && !has_explicit_idx {
                    issues.push(SchemaIssue {
                        severity: IssueSeverity::Info,
                        target: format!("{}.{}", tbl.name, fk.from_column),
                        message: format!(
                            "Kolom FK '{}.{}' belum diindeks. Aktifkan 'Auto-Index FK' untuk mencegah table-lock saat DELETE/UPDATE.",
                            tbl.name, fk.from_column
                        ),
                    });
                }
            }
        }

        issues
    }

    /// Reverse-Engineer langsung dari skrip SQL DDL mentah (`CREATE TABLE ...`) ala DrawDB "Import from SQL"
    /// Menggunakan parser native SQLite in-memory untuk akurasi sintaks 100%
    pub fn from_sql_ddl_script(
        sql_ddl: &str,
        default_scope: TableScope,
    ) -> Result<Self, ShardVaultError> {
        let mem_conn = rusqlite::Connection::open_in_memory()?;
        // Matikan sementara penegakan FK saat parsing urutan CREATE TABLE bebas
        mem_conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        mem_conn.execute_batch(sql_ddl).map_err(|e| {
            ShardVaultError::Migration(format!("Gagal mem-parsing skrip SQL DDL: {}", e))
        })?;
        let manifest = introspect_schema(&mem_conn)?;
        let mut bp = Self::from_live_manifest(&manifest, default_scope);
        bp.project_name = "Imported_SQL_Schema".to_string();
        Ok(bp)
    }

    /// Reverse-Engineer dari `SchemaManifest` database SQLite yang sedang aktif
    pub fn from_live_manifest(manifest: &SchemaManifest, default_scope: TableScope) -> Self {
        let mut tables = Vec::new();
        let cols_per_row = 3usize;

        for (idx, live_tbl) in manifest.tables.iter().enumerate() {
            // Lewati tabel internal _shardvault_* agar kanvas bersih
            if live_tbl.name.starts_with("_shardvault_") {
                continue;
            }

            let grid_col = idx % cols_per_row;
            let grid_row = idx / cols_per_row;
            let x = 50.0 + (grid_col as f32) * 410.0;
            let y = 60.0 + (grid_row as f32) * 260.0;

            let upper_sql = live_tbl.sql.to_uppercase();
            let is_autoinc = upper_sql.contains("AUTOINCREMENT");
            let without_rowid = upper_sql.contains("WITHOUT ROWID");

            // Deteksi kolom yang memiliki single-column UNIQUE index
            let unique_single_cols: HashSet<String> = live_tbl
                .indexes
                .iter()
                .filter(|i| i.unique && i.columns.len() == 1)
                .filter_map(|i| i.columns.first().cloned())
                .collect();

            let mut columns = Vec::new();
            for c in &live_tbl.columns {
                let is_pk = c.pk_order > 0;
                let col_type = SqliteAffinity::from_sql_type(&c.data_type);
                let encrypted_aes = c.name.ends_with("_enc") || c.name.contains("secret") || c.name.contains("cipher");
                let include_in_hmac = is_pk || c.name != "signature";

                columns.push(ColumnBlueprint {
                    name: c.name.clone(),
                    col_type,
                    is_primary_key: is_pk,
                    auto_increment: is_pk && is_autoinc && col_type == SqliteAffinity::Integer,
                    not_null: c.not_null || is_pk,
                    unique: is_pk || unique_single_cols.contains(&c.name),
                    default_expr: c.default_value.clone(),
                    check_expr: None,
                    comment: String::new(),
                    encrypted_aes,
                    include_in_hmac,
                });
            }

            let mut foreign_keys = Vec::new();
            for fk in &live_tbl.foreign_keys {
                let is_one_to_one = columns
                    .iter()
                    .any(|c| c.name == fk.from_column && (c.unique || c.is_primary_key));
                foreign_keys.push(ForeignKeyBlueprint {
                    name: format!("fk_{}_{}_{}", live_tbl.name, fk.from_column, fk.target_table),
                    from_column: fk.from_column.clone(),
                    ref_table: fk.target_table.clone(),
                    ref_column: fk.to_column.clone(),
                    cardinality: if is_one_to_one {
                        Cardinality::OneToOne
                    } else {
                        Cardinality::ManyToOne
                    },
                    on_delete: ForeignKeyAction::from_sql_str(&fk.on_delete),
                    on_update: ForeignKeyAction::from_sql_str(&fk.on_update),
                    auto_index_fk: true,
                });
            }

            let mut indexes = Vec::new();
            for idx_info in &live_tbl.indexes {
                if idx_info.name.starts_with("sqlite_autoindex_") {
                    continue;
                }
                indexes.push(IndexBlueprint {
                    name: idx_info.name.clone(),
                    columns: idx_info.columns.clone(),
                    unique: idx_info.unique,
                });
            }

            let has_cdc = manifest
                .triggers
                .iter()
                .any(|t| t.contains(&format!("_sv_cdc_{}_", live_tbl.name)));

            let mut tbl_bp = TableBlueprint::new(&live_tbl.name, default_scope, x, y);
            tbl_bp.enable_cdc = has_cdc;
            tbl_bp.without_rowid = without_rowid;
            tbl_bp.columns = columns;
            tbl_bp.foreign_keys = foreign_keys;
            tbl_bp.indexes = indexes;
            tables.push(tbl_bp);
        }

        Self {
            project_name: "LiveClusterSchema".to_string(),
            version: 1,
            tables,
            subject_areas: Vec::new(),
            notes: vec![NoteBlueprint {
                title: "Live Reverse-Engineered Schema".to_string(),
                content: "Skema ini diimpor langsung dari database aktif via introspect_schema().".to_string(),
                color_hex: "#f59e0b".to_string(),
                x: 40.0,
                y: 560.0,
            }],
        }
    }

    /// Template Bawaan 1: POS & Retail Inventory (DrawDB Starter Template)
    pub fn template_pos_retail() -> Self {
        let mut categories = TableBlueprint::new("categories", TableScope::Global, 50.0, 70.0);
        categories.comment = "Master kategori produk global".to_string();
        categories.columns = vec![
            ColumnBlueprint::primary_key_int("id"),
            ColumnBlueprint {
                name: "code".to_string(),
                col_type: SqliteAffinity::Text,
                is_primary_key: false,
                auto_increment: false,
                not_null: true,
                unique: true,
                default_expr: None,
                check_expr: None,
                comment: "Kode unik kategori".to_string(),
                encrypted_aes: false,
                include_in_hmac: true,
            },
            ColumnBlueprint::new("name", SqliteAffinity::Text),
        ];

        let mut products = TableBlueprint::new("products", TableScope::Global, 450.0, 70.0);
        products.comment = "Katalog produk beserta HPP terenkripsi AES-256-GCM".to_string();
        let mut hpp_col = ColumnBlueprint::new("cost_price_enc", SqliteAffinity::Text);
        hpp_col.encrypted_aes = true;
        hpp_col.include_in_hmac = true;
        hpp_col.comment = "Harga Pokok Penjualan (AES-256-GCM)".to_string();

        let mut price_col = ColumnBlueprint::new("sell_price", SqliteAffinity::Integer);
        price_col.default_expr = Some("0".to_string());
        price_col.check_expr = Some("sell_price >= 0".to_string());
        price_col.include_in_hmac = true;

        products.columns = vec![
            ColumnBlueprint::primary_key_int("id"),
            ColumnBlueprint::new("category_id", SqliteAffinity::Integer),
            ColumnBlueprint::new("sku", SqliteAffinity::Text),
            ColumnBlueprint::new("title", SqliteAffinity::Text),
            hpp_col,
            price_col,
            ColumnBlueprint::new("signature", SqliteAffinity::Text),
        ];
        products.foreign_keys.push(ForeignKeyBlueprint::new(
            "products",
            "category_id",
            "categories",
            "id",
            ForeignKeyAction::Restrict,
        ));

        let mut orders = TableBlueprint::new("orders", TableScope::Shard, 50.0, 360.0);
        orders.comment = "Transaksi penjualan per cabang/periode shard".to_string();
        let mut status_col = ColumnBlueprint::new("status", SqliteAffinity::Text);
        status_col.default_expr = Some("'PAID'".to_string());
        orders.columns = vec![
            ColumnBlueprint::primary_key_text("invoice_id"),
            ColumnBlueprint::new("cashier_name", SqliteAffinity::Text),
            ColumnBlueprint::new("total_amount", SqliteAffinity::Integer),
            status_col,
            ColumnBlueprint::new("signature", SqliteAffinity::Text),
        ];

        let mut order_items = TableBlueprint::new("order_items", TableScope::Shard, 450.0, 360.0);
        order_items.comment = "Detail item belanja yang merujuk ke orders".to_string();
        order_items.columns = vec![
            ColumnBlueprint::primary_key_int("id"),
            ColumnBlueprint::new("invoice_id", SqliteAffinity::Text),
            ColumnBlueprint::new("product_sku", SqliteAffinity::Text),
            ColumnBlueprint::new("qty", SqliteAffinity::Integer),
            ColumnBlueprint::new("subtotal", SqliteAffinity::Integer),
        ];
        order_items.foreign_keys.push(ForeignKeyBlueprint::new(
            "order_items",
            "invoice_id",
            "orders",
            "invoice_id",
            ForeignKeyAction::Cascade,
        ));

        Self {
            project_name: "Retail_POS_ShardVault".to_string(),
            version: 1,
            tables: vec![categories, products, orders, order_items],
            subject_areas: vec![
                SubjectAreaBlueprint {
                    name: "Global Master Catalog".to_string(),
                    color_hex: "#0284c7".to_string(),
                    x: 30.0,
                    y: 35.0,
                    width: 700.0,
                    height: 270.0,
                },
                SubjectAreaBlueprint {
                    name: "Sharded Branch Transactions (Auto-CDC)".to_string(),
                    color_hex: "#7e22ce".to_string(),
                    x: 30.0,
                    y: 325.0,
                    width: 700.0,
                    height: 270.0,
                },
            ],
            notes: vec![NoteBlueprint {
                title: "🔐 Keamanan HPP & Integritas Struk".to_string(),
                content: "Kolom cost_price_enc dienkripsi AES-256-GCM.\nSetiap baris orders memiliki HMAC-SHA256.".to_string(),
                color_hex: "#10b981".to_string(),
                x: 755.0,
                y: 70.0,
            }],
        }
    }

    /// Template Bawaan 2: Multi-Tenant SaaS & Encrypted Vault
    pub fn template_saas_multitenant() -> Self {
        let mut tenants = TableBlueprint::new("tenants", TableScope::Global, 50.0, 70.0);
        tenants.columns = vec![
            ColumnBlueprint::primary_key_text("tenant_id"),
            ColumnBlueprint::new("organization_name", SqliteAffinity::Text),
            ColumnBlueprint::new("plan_tier", SqliteAffinity::Text),
        ];

        let mut users = TableBlueprint::new("tenant_users", TableScope::Global, 450.0, 70.0);
        let mut api_secret = ColumnBlueprint::new("api_secret_enc", SqliteAffinity::Text);
        api_secret.encrypted_aes = true;
        api_secret.include_in_hmac = true;
        users.columns = vec![
            ColumnBlueprint::primary_key_int("id"),
            ColumnBlueprint::new("tenant_id", SqliteAffinity::Text),
            ColumnBlueprint::new("email", SqliteAffinity::Text),
            api_secret,
        ];
        users.foreign_keys.push(ForeignKeyBlueprint::new(
            "tenant_users",
            "tenant_id",
            "tenants",
            "tenant_id",
            ForeignKeyAction::Cascade,
        ));

        let mut documents = TableBlueprint::new("vault_documents", TableScope::Shard, 50.0, 350.0);
        let mut payload_enc = ColumnBlueprint::new("payload_enc", SqliteAffinity::Text);
        payload_enc.encrypted_aes = true;
        payload_enc.include_in_hmac = true;
        documents.columns = vec![
            ColumnBlueprint::primary_key_text("doc_id"),
            ColumnBlueprint::new("title", SqliteAffinity::Text),
            payload_enc,
            ColumnBlueprint::new("signature", SqliteAffinity::Text),
        ];

        let mut doc_versions = TableBlueprint::new("document_versions", TableScope::Shard, 450.0, 350.0);
        doc_versions.columns = vec![
            ColumnBlueprint::primary_key_int("version_id"),
            ColumnBlueprint::new("doc_id", SqliteAffinity::Text),
            ColumnBlueprint::new("checksum_sha256", SqliteAffinity::Text),
        ];
        doc_versions.foreign_keys.push(ForeignKeyBlueprint::new(
            "document_versions",
            "doc_id",
            "vault_documents",
            "doc_id",
            ForeignKeyAction::Cascade,
        ));

        Self {
            project_name: "SaaS_Encrypted_Vault".to_string(),
            version: 1,
            tables: vec![tenants, users, documents, doc_versions],
            subject_areas: vec![
                SubjectAreaBlueprint {
                    name: "Global Control Plane".to_string(),
                    color_hex: "#0284c7".to_string(),
                    x: 30.0,
                    y: 35.0,
                    width: 700.0,
                    height: 260.0,
                },
                SubjectAreaBlueprint {
                    name: "Tenant Isolated Vault Shard".to_string(),
                    color_hex: "#7e22ce".to_string(),
                    x: 30.0,
                    y: 315.0,
                    width: 700.0,
                    height: 260.0,
                },
            ],
            notes: vec![NoteBlueprint {
                title: "📌 Isolasi Per Tenant".to_string(),
                content: "Setiap tenant memiliki file shard terpisah\ndengan replika otomatis.".to_string(),
                color_hex: "#38bdf8".to_string(),
                x: 755.0,
                y: 70.0,
            }],
        }
    }

    /// Otomatis menghubungkan dua tabel saat ditarik garis dari tabel induk ke tabel anak.
    /// Jika kolom penghubung belum ada di tabel anak, kolom akan otomatis dibuat dengan tipe yang sesuai!
    pub fn connect_tables_automatically(
        &mut self,
        parent_idx: usize,
        child_idx: usize,
    ) -> Result<String, ShardVaultError> {
        if parent_idx >= self.tables.len() || child_idx >= self.tables.len() {
            return Err(ShardVaultError::Migration("Indeks tabel tidak valid".into()));
        }
        if parent_idx == child_idx {
            return Err(ShardVaultError::Migration(
                "Tidak dapat menghubungkan tabel ke dirinya sendiri".into(),
            ));
        }

        let parent_name = self.tables[parent_idx].name.clone();
        let (parent_pk_name, parent_pk_type) = {
            let parent_tbl = &self.tables[parent_idx];
            let pk_col = parent_tbl
                .pk_column()
                .cloned()
                .unwrap_or_else(|| ColumnBlueprint::primary_key_int("id"));
            (pk_col.name, pk_col.col_type)
        };

        let child_tbl = &mut self.tables[child_idx];
        let child_name = child_tbl.name.clone();

        // Tentukan nama kolom FK yang ramah pemula, e.g. "kategori_id" jika parent_name = "kategori"
        let singular_parent = if parent_name.ends_with('s') && parent_name.len() > 1 {
            &parent_name[..parent_name.len() - 1]
        } else {
            &parent_name
        };
        let suggested_fk_col = format!("{}_{}", singular_parent, parent_pk_name);

        // Cari apakah child sudah punya kolom yang cocok (bukan primary key milik child sendiri)
        let fk_col_name = if let Some(existing_col) = child_tbl.columns.iter().find(|c| {
            !c.is_primary_key
                && (c.name.eq_ignore_ascii_case(&suggested_fk_col)
                    || c.name.eq_ignore_ascii_case(&format!("{}_id", parent_name)))
        }) {
            existing_col.name.clone()
        } else {
            // Buat kolom baru otomatis di child
            let new_col = ColumnBlueprint {
                name: suggested_fk_col.clone(),
                col_type: parent_pk_type,
                is_primary_key: false,
                auto_increment: false,
                not_null: true,
                unique: false,
                default_expr: None,
                check_expr: None,
                comment: format!("Relasi ke {}.{}", parent_name, parent_pk_name),
                encrypted_aes: false,
                include_in_hmac: true,
            };
            child_tbl.columns.push(new_col);
            suggested_fk_col
        };

        // Cek apakah relasi FK ini sudah terdaftar
        let exists = child_tbl.foreign_keys.iter().any(|f| {
            f.from_column.eq_ignore_ascii_case(&fk_col_name)
                && f.ref_table.eq_ignore_ascii_case(&parent_name)
        });

        if !exists {
            let on_del = if child_name.contains("rincian")
                || child_name.contains("detail")
                || child_name.contains("item")
            {
                ForeignKeyAction::Cascade
            } else {
                ForeignKeyAction::Restrict
            };
            let mut fk = ForeignKeyBlueprint::new(
                &child_name,
                &fk_col_name,
                &parent_name,
                &parent_pk_name,
                on_del,
            );
            fk.auto_index_fk = true;
            child_tbl.foreign_keys.push(fk);
        }

        Ok(format!(
            "Tabel '{}' berhasil dihubungkan ke '{}' via kolom '{}'!",
            child_name, parent_name, fk_col_name
        ))
    }

    /// Gabungkan blueprint lain (misal dari drop file .ddb.json atau template) ke kanvas saat ini tanpa menimpa yang lama
    pub fn merge_blueprint(&mut self, other: SchemaBlueprint, offset_x: f32, offset_y: f32) {
        for mut tbl in other.tables {
            let orig_name = tbl.name.clone();
            let mut candidate_name = orig_name.clone();
            let mut counter = 2;
            while self
                .tables
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(&candidate_name))
            {
                candidate_name = format!("{}_{}", orig_name, counter);
                counter += 1;
            }
            tbl.name = candidate_name;
            tbl.x += offset_x;
            tbl.y += offset_y;
            self.tables.push(tbl);
        }
        for mut area in other.subject_areas {
            area.x += offset_x;
            area.y += offset_y;
            self.subject_areas.push(area);
        }
        for mut note in other.notes {
            note.x += offset_x;
            note.y += offset_y;
            self.notes.push(note);
        }
    }
}

/// Tipe data ramah orang awam (seperti di Airtable / Notion)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FriendlyFieldType {
    /// Teks umum / Nama
    Text,
    /// Uang / Harga / Jumlah (Integer, default 0)
    Currency,
    /// Angka Desimal / Persen (Real, default 0.0)
    Decimal,
    /// Tanggal & Jam Otomatis (Text, default CURRENT_TIMESTAMP)
    Timestamp,
    /// Status Ya / Tidak (Integer, default 1)
    Boolean,
    /// Teks Rahasia (AES-256-GCM + HMAC)
    SecretText,
}

impl FriendlyFieldType {
    pub fn all() -> &'static [FriendlyFieldType] {
        &[
            FriendlyFieldType::Text,
            FriendlyFieldType::Currency,
            FriendlyFieldType::Decimal,
            FriendlyFieldType::Timestamp,
            FriendlyFieldType::Boolean,
            FriendlyFieldType::SecretText,
        ]
    }

    pub fn display_name(self) -> &'static str {
        match self {
            FriendlyFieldType::Text => "📝 Teks / Nama",
            FriendlyFieldType::Currency => "💰 Uang / Harga",
            FriendlyFieldType::Decimal => "📊 Angka Desimal",
            FriendlyFieldType::Timestamp => "📅 Tanggal Otomatis",
            FriendlyFieldType::Boolean => "✅ Status Ya/Tidak",
            FriendlyFieldType::SecretText => "🔒 Teks Rahasia (AES)",
        }
    }

    pub fn to_column_blueprint(self, name: &str) -> ColumnBlueprint {
        match self {
            FriendlyFieldType::Text => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Text);
                col.comment = "Teks / String".to_string();
                col
            }
            FriendlyFieldType::Currency => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Integer);
                col.default_expr = Some("0".to_string());
                col.comment = "Nominal Uang / Rupiah".to_string();
                col
            }
            FriendlyFieldType::Decimal => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Real);
                col.default_expr = Some("0.0".to_string());
                col.comment = "Angka Desimal / Persen".to_string();
                col
            }
            FriendlyFieldType::Timestamp => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Text);
                col.default_expr = Some("CURRENT_TIMESTAMP".to_string());
                col.comment = "Waktu Pencatatan".to_string();
                col
            }
            FriendlyFieldType::Boolean => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Integer);
                col.default_expr = Some("1".to_string());
                col.comment = "Status Ya (1) / Tidak (0)".to_string();
                col
            }
            FriendlyFieldType::SecretText => {
                let mut col = ColumnBlueprint::new(name, SqliteAffinity::Text);
                col.encrypted_aes = true;
                col.include_in_hmac = true;
                col.comment = "Teks Rahasia Terenkripsi AES-256-GCM".to_string();
                col
            }
        }
    }

    pub fn from_column_blueprint(col: &ColumnBlueprint) -> Self {
        if col.encrypted_aes {
            FriendlyFieldType::SecretText
        } else if col.col_type == SqliteAffinity::Real {
            FriendlyFieldType::Decimal
        } else if col.default_expr.as_deref() == Some("CURRENT_TIMESTAMP")
            || col.comment.to_lowercase().contains("waktu")
            || col.name.ends_with("_at")
        {
            FriendlyFieldType::Timestamp
        } else if col.col_type == SqliteAffinity::Integer {
            if col.comment.to_lowercase().contains("ya")
                || col.name.starts_with("is_")
                || col.name.starts_with("has_")
                || col.default_expr.as_deref() == Some("1")
            {
                FriendlyFieldType::Boolean
            } else {
                FriendlyFieldType::Currency
            }
        } else {
            FriendlyFieldType::Text
        }
    }
}

/// Paket modul tabel siap pakai untuk pemula
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModularTablePreset {
    BlankTable,
    CatalogModule,
    InvoiceModule,
    CustomerModule,
}

impl ModularTablePreset {
    pub fn all() -> &'static [ModularTablePreset] {
        &[
            ModularTablePreset::BlankTable,
            ModularTablePreset::CatalogModule,
            ModularTablePreset::InvoiceModule,
            ModularTablePreset::CustomerModule,
        ]
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ModularTablePreset::BlankTable => "➕ Tabel Kosong Baru",
            ModularTablePreset::CatalogModule => "📦 Paket Produk & Kategori",
            ModularTablePreset::InvoiceModule => "🧾 Paket Nota & Rincian",
            ModularTablePreset::CustomerModule => "👥 Paket Pelanggan",
        }
    }

    pub fn instantiate(self, base_x: f32, base_y: f32) -> Vec<TableBlueprint> {
        match self {
            ModularTablePreset::BlankTable => {
                vec![TableBlueprint::new("tabel_baru", TableScope::Shard, base_x, base_y)]
            }
            ModularTablePreset::CatalogModule => {
                let mut categories = TableBlueprint::new("kategori", TableScope::Global, base_x, base_y);
                categories.comment = "Kategori Produk".to_string();
                categories.columns.push(ColumnBlueprint::new("nama_kategori", SqliteAffinity::Text));

                let mut products = TableBlueprint::new("produk", TableScope::Global, base_x + 400.0, base_y);
                products.comment = "Daftar Produk & Harga".to_string();
                products.columns.push(ColumnBlueprint::new("kategori_id", SqliteAffinity::Integer));
                products.columns.push(ColumnBlueprint::new("nama_produk", SqliteAffinity::Text));
                let mut price = ColumnBlueprint::new("harga_jual", SqliteAffinity::Integer);
                price.default_expr = Some("0".to_string());
                products.columns.push(price);
                products.foreign_keys.push(ForeignKeyBlueprint::new(
                    "produk",
                    "kategori_id",
                    "kategori",
                    "id",
                    ForeignKeyAction::Restrict,
                ));
                vec![categories, products]
            }
            ModularTablePreset::InvoiceModule => {
                let mut invoices = TableBlueprint::new("nota_pesanan", TableScope::Shard, base_x, base_y);
                invoices.comment = "Nota Transaksi Penjualan".to_string();
                invoices.columns = vec![
                    ColumnBlueprint::primary_key_text("nomor_nota"),
                    ColumnBlueprint::new("nama_kasir", SqliteAffinity::Text),
                    ColumnBlueprint {
                        name: "total_bayar".to_string(),
                        col_type: SqliteAffinity::Integer,
                        is_primary_key: false,
                        auto_increment: false,
                        not_null: true,
                        unique: false,
                        default_expr: Some("0".to_string()),
                        check_expr: None,
                        comment: "Total Pembayaran".to_string(),
                        encrypted_aes: false,
                        include_in_hmac: true,
                    },
                    ColumnBlueprint::new("signature", SqliteAffinity::Text),
                ];

                let mut items = TableBlueprint::new("rincian_nota", TableScope::Shard, base_x + 400.0, base_y);
                items.comment = "Item Barang dalam Nota".to_string();
                items.columns = vec![
                    ColumnBlueprint::primary_key_int("id"),
                    ColumnBlueprint::new("nomor_nota", SqliteAffinity::Text),
                    ColumnBlueprint::new("nama_barang", SqliteAffinity::Text),
                    ColumnBlueprint::new("jumlah", SqliteAffinity::Integer),
                    ColumnBlueprint::new("subtotal", SqliteAffinity::Integer),
                ];
                items.foreign_keys.push(ForeignKeyBlueprint::new(
                    "rincian_nota",
                    "nomor_nota",
                    "nota_pesanan",
                    "nomor_nota",
                    ForeignKeyAction::Cascade,
                ));
                vec![invoices, items]
            }
            ModularTablePreset::CustomerModule => {
                let mut customers = TableBlueprint::new("pelanggan", TableScope::Global, base_x, base_y);
                customers.comment = "Data Pelanggan & Member".to_string();
                customers.columns.push(ColumnBlueprint::new("nama_lengkap", SqliteAffinity::Text));
                customers.columns.push(ColumnBlueprint::new("no_telepon", SqliteAffinity::Text));
                let mut secret_pin = ColumnBlueprint::new("pin_rahasia_enc", SqliteAffinity::Text);
                secret_pin.encrypted_aes = true;
                secret_pin.include_in_hmac = true;
                secret_pin.comment = "PIN Terenkripsi AES-256".to_string();
                customers.columns.push(secret_pin);
                vec![customers]
            }
        }
    }
}
