// E:\shardvault-rs\src\codegen\ddl_gen.rs
// Forward-Engineering Multi-Dialect SQL DDL Compiler (SQLite, PostgreSQL, MySQL)

use serde::{Deserialize, Serialize};

use super::ast::{SchemaBlueprint, SqliteAffinity, TableBlueprint, TableScope};

/// Pilihan Dialek SQL untuk Generator DDL ala DrawDB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
    Mysql,
}

impl SqlDialect {
    pub fn all() -> &'static [SqlDialect] {
        &[SqlDialect::Sqlite, SqlDialect::Postgres, SqlDialect::Mysql]
    }
}

impl std::fmt::Display for SqlDialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqlDialect::Sqlite => write!(f, "SQLite 3 (ShardVault Native)"),
            SqlDialect::Postgres => write!(f, "PostgreSQL"),
            SqlDialect::Mysql => write!(f, "MySQL / MariaDB"),
        }
    }
}

/// Mengutip identifier SQL sesuai dialek
pub fn quote_ident(name: &str, dialect: SqlDialect) -> String {
    match dialect {
        SqlDialect::Sqlite | SqlDialect::Postgres => {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
        SqlDialect::Mysql => {
            format!("`{}`", name.replace('`', "``"))
        }
    }
}

/// Kompilasi satu `TableBlueprint` menjadi perintah `CREATE TABLE` + `CREATE INDEX`
pub fn compile_table_ddl(table: &TableBlueprint, dialect: SqlDialect) -> String {
    let mut out = String::new();

    if !table.comment.trim().is_empty() {
        out.push_str(&format!("-- Tabel: {} ({})\n", table.name, table.comment));
    } else {
        out.push_str(&format!("-- Tabel: {} [Scope: {}]\n", table.name, table.scope));
    }

    let tbl_q = quote_ident(&table.name, dialect);
    out.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n", tbl_q));

    let pk_cols: Vec<&str> = table
        .columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.as_str())
        .collect();
    let composite_pk = pk_cols.len() > 1;

    let mut line_items: Vec<String> = Vec::new();

    for col in &table.columns {
        let col_q = quote_ident(&col.name, dialect);
        let type_str = match dialect {
            SqlDialect::Sqlite => col.col_type.to_sqlite_str(),
            SqlDialect::Postgres => col.col_type.to_postgres_str(col.auto_increment),
            SqlDialect::Mysql => col.col_type.to_mysql_str(col.is_primary_key || col.unique),
        };

        let mut col_def = format!("    {} {}", col_q, type_str);

        if col.is_primary_key && !composite_pk {
            col_def.push_str(" PRIMARY KEY");
            if col.auto_increment && col.col_type == SqliteAffinity::Integer {
                match dialect {
                    SqlDialect::Sqlite => col_def.push_str(" AUTOINCREMENT"),
                    SqlDialect::Mysql => col_def.push_str(" AUTO_INCREMENT"),
                    SqlDialect::Postgres => {} // BIGSERIAL sudah menangani auto-increment
                }
            }
        } else {
            if col.not_null {
                col_def.push_str(" NOT NULL");
            }
            if col.unique {
                col_def.push_str(" UNIQUE");
            }
        }

        if let Some(ref def_val) = col.default_expr {
            let trimmed = def_val.trim();
            if !trimmed.is_empty() {
                col_def.push_str(&format!(" DEFAULT {}", trimmed));
            }
        }

        if let Some(ref chk) = col.check_expr {
            let trimmed = chk.trim();
            if !trimmed.is_empty() {
                col_def.push_str(&format!(" CHECK ({})", trimmed));
            }
        }

        line_items.push(col_def);
    }

    if composite_pk {
        let pk_list: Vec<String> = pk_cols.iter().map(|c| quote_ident(c, dialect)).collect();
        line_items.push(format!("    PRIMARY KEY ({})", pk_list.join(", ")));
    }

    // Konstrain Foreign Key
    for fk in &table.foreign_keys {
        let fk_name_q = quote_ident(&fk.name, dialect);
        let from_q = quote_ident(&fk.from_column, dialect);
        let ref_tbl_q = quote_ident(&fk.ref_table, dialect);
        let ref_col_q = quote_ident(&fk.ref_column, dialect);
        line_items.push(format!(
            "    CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {} ON UPDATE {}",
            fk_name_q,
            from_q,
            ref_tbl_q,
            ref_col_q,
            fk.on_delete.to_sql_str(),
            fk.on_update.to_sql_str()
        ));
    }

    out.push_str(&line_items.join(",\n"));
    out.push('\n');

    if dialect == SqlDialect::Sqlite && table.without_rowid {
        out.push_str(") WITHOUT ROWID;\n");
    } else {
        out.push_str(");\n");
    }

    // Auto-Index untuk Foreign Key (Best Practice SQLite untuk mencegah table lock)
    for fk in &table.foreign_keys {
        if fk.auto_index_fk {
            let idx_name = format!("idx_{}_{}_fk", table.name, fk.from_column);
            let already_defined = table.indexes.iter().any(|i| i.name == idx_name);
            if !already_defined {
                let idx_q = quote_ident(&idx_name, dialect);
                let col_q = quote_ident(&fk.from_column, dialect);
                out.push_str(&format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({});\n",
                    idx_q, tbl_q, col_q
                ));
            }
        }
    }

    // Indeks eksplisit
    for idx in &table.indexes {
        if idx.columns.is_empty() {
            continue;
        }
        let idx_q = quote_ident(&idx.name, dialect);
        let cols_q: Vec<String> = idx.columns.iter().map(|c| quote_ident(c, dialect)).collect();
        let unique_kw = if idx.unique { "UNIQUE " } else { "" };
        out.push_str(&format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({});\n",
            unique_kw,
            idx_q,
            tbl_q,
            cols_q.join(", ")
        ));
    }

    out
}

/// Kompilasi seluruh `SchemaBlueprint` menjadi skrip DDL lengkap
pub fn compile_blueprint_ddl(
    blueprint: &SchemaBlueprint,
    dialect: SqlDialect,
    scope_filter: Option<TableScope>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "-- ============================================================================\n\
         -- Generated by ShardVault Studio Schema Designer & Generator\n\
         -- Project : {} (v{})\n\
         -- Dialect : {}\n\
         -- ============================================================================\n\n",
        blueprint.project_name, blueprint.version, dialect
    ));

    if dialect == SqlDialect::Sqlite {
        out.push_str("PRAGMA foreign_keys = ON;\n\n");
    }

    for tbl in &blueprint.tables {
        if let Some(filter) = scope_filter {
            if tbl.scope != filter && tbl.scope != TableScope::Both {
                continue;
            }
        }
        out.push_str(&compile_table_ddl(tbl, dialect));
        out.push('\n');
    }

    out
}
