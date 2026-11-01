// E:\shardvault-rs\src\codegen\rust_gen.rs
// Generator Kode Rust: SchemaProvider, Domain Structs, & AES-256-GCM / HMAC-SHA256 Repository

use super::ast::{SchemaBlueprint, TableBlueprint, TableScope};
use super::ddl_gen::{compile_table_ddl, SqlDialect};

/// Konversi snake_case ke PascalCase untuk nama struct Rust
pub fn to_pascal_case(input: &str) -> String {
    let mut out = String::new();
    for part in input.split(|c: char| !c.is_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_lowercase());
        }
    }
    if out.is_empty() {
        "GeneratedRecord".to_string()
    } else {
        out
    }
}

/// Konversi nama ke identifier snake_case yang aman untuk field Rust
pub fn to_snake_ident(input: &str) -> String {
    let mut s: String = input
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if s.is_empty() {
        s = "field".to_string();
    }
    if matches!(
        s.as_str(),
        "type" | "ref" | "mod" | "fn" | "let" | "match" | "struct" | "enum" | "use" | "crate"
    ) {
        format!("r#{}", s)
    } else {
        s
    }
}

fn generate_table_struct_and_repo(table: &TableBlueprint) -> String {
    let struct_name = to_pascal_case(&table.name);
    let mut out = String::new();

    out.push_str(&format!(
        "// ----------------------------------------------------------------------------\n\
         // Domain Model & Cryptographic Repository: `{}` (Scope: {})\n\
         // ----------------------------------------------------------------------------\n",
        table.name, table.scope
    ));
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {} {{\n", struct_name));

    for col in &table.columns {
        let field_name = to_snake_ident(&col.name);
        let base_type = col.col_type.to_rust_type();
        let full_type = if col.not_null || col.is_primary_key {
            base_type.to_string()
        } else {
            format!("Option<{}>", base_type)
        };
        if !col.comment.is_empty() {
            out.push_str(&format!("    /// {}\n", col.comment));
        }
        out.push_str(&format!("    pub {}: {},\n", field_name, full_type));
    }
    out.push_str("}\n\n");

    out.push_str(&format!("impl {} {{\n", struct_name));

    // 1. Jika ada kolom AES-256-GCM Encrypted
    let aes_cols: Vec<&str> = table
        .columns
        .iter()
        .filter(|c| c.encrypted_aes)
        .map(|c| c.name.as_str())
        .collect();
    if !aes_cols.is_empty() {
        out.push_str(
            "    /// Enkripsi kolom sensitif menggunakan AES-256-GCM sebelum disimpan ke SQLite\n\
             \x20   pub fn encrypt_fields(&mut self, master_key: &[u8; 32]) -> Result<(), ShardVaultError> {\n",
        );
        for c_name in &aes_cols {
            let f_name = to_snake_ident(c_name);
            let col = table.columns.iter().find(|c| c.name == *c_name).unwrap();
            if col.not_null || col.is_primary_key {
                out.push_str(&format!(
                    "        if !self.{f}.starts_with(\"v1:\") {{\n\
                     \x20           self.{f} = encrypt_string(&self.{f}, master_key)?;\n\
                     \x20       }}\n",
                    f = f_name
                ));
            } else {
                out.push_str(&format!(
                    "        if let Some(ref val) = self.{f} {{\n\
                     \x20           if !val.starts_with(\"v1:\") {{\n\
                     \x20               self.{f} = Some(encrypt_string(val, master_key)?);\n\
                     \x20           }}\n\
                     \x20       }}\n",
                    f = f_name
                ));
            }
        }
        out.push_str("        Ok(())\n    }\n\n");

        out.push_str(
            "    /// Dekripsi kolom sensitif AES-256-GCM saat dibaca dari SQLite\n\
             \x20   pub fn decrypt_fields(&mut self, master_key: &[u8; 32]) -> Result<(), ShardVaultError> {\n",
        );
        for c_name in &aes_cols {
            let f_name = to_snake_ident(c_name);
            let col = table.columns.iter().find(|c| c.name == *c_name).unwrap();
            if col.not_null || col.is_primary_key {
                out.push_str(&format!(
                    "        if self.{f}.starts_with(\"v1:\") {{\n\
                     \x20           self.{f} = decrypt_string(&self.{f}, master_key)?;\n\
                     \x20       }}\n",
                    f = f_name
                ));
            } else {
                out.push_str(&format!(
                    "        if let Some(ref val) = self.{f} {{\n\
                     \x20           if val.starts_with(\"v1:\") {{\n\
                     \x20               self.{f} = Some(decrypt_string(val, master_key)?);\n\
                     \x20           }}\n\
                     \x20       }}\n",
                    f = f_name
                ));
            }
        }
        out.push_str("        Ok(())\n    }\n\n");
    }

    // 2. Jika ada kolom HMAC-SHA256
    let hmac_cols: Vec<&str> = table
        .columns
        .iter()
        .filter(|c| c.include_in_hmac && c.name != "signature")
        .map(|c| c.name.as_str())
        .collect();
    if !hmac_cols.is_empty() {
        let fmt_placeholders: Vec<String> = hmac_cols.iter().map(|c| format!("{}:{{:?}}", c)).collect();
        let fmt_args: Vec<String> = hmac_cols
            .iter()
            .map(|c| format!("self.{}", to_snake_ident(c)))
            .collect();
        out.push_str(&format!(
            "    /// Hitung tanda tangan integritas baris (HMAC-SHA256)\n\
             \x20   pub fn compute_row_hmac(&self, master_key: &[u8; 32]) -> String {{\n\
             \x20       let canonical = format!(\"{}\", {});\n\
             \x20       generate_signature(&canonical, master_key)\n\
             \x20   }}\n\n",
            fmt_placeholders.join("|"),
            fmt_args.join(", ")
        ));
    }

    // 3. Helper INSERT / UPSERT ke SQLite
    let col_names: Vec<String> = table.columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let placeholders: Vec<String> = (1..=table.columns.len()).map(|i| format!("?{}", i)).collect();
    let param_fields: Vec<String> = table
        .columns
        .iter()
        .map(|c| format!("&self.{}", to_snake_ident(&c.name)))
        .collect();

    out.push_str(&format!(
        "    /// Simpan atau perbarui baris pada tabel `{tbl}`\n\
         \x20   pub fn upsert(&self, conn: &Connection) -> Result<(), ShardVaultError> {{\n\
         \x20       conn.execute(\n\
         \x20           \"INSERT OR REPLACE INTO \\\"{tbl}\\\" ({cols}) VALUES ({phs})\",\n\
         \x20           params![{args}],\n\
         \x20       )?;\n\
         \x20       Ok(())\n\
         \x20   }}\n",
        tbl = table.name,
        cols = col_names.join(", "),
        phs = placeholders.join(", "),
        args = param_fields.join(", ")
    ));

    out.push_str("}\n\n");
    out
}

/// Menghasilkan kode sumber Rust lengkap (`SchemaProvider` + Domain Structs + Cryptographic Repo)
pub fn generate_rust_code(blueprint: &SchemaBlueprint) -> String {
    let provider_name = format!("{}SchemaProvider", to_pascal_case(&blueprint.project_name));
    let mut out = String::new();

    out.push_str(&format!(
        "// ============================================================================\n\
         // Auto-Generated Rust Schema & Cryptographic Repository\n\
         // Project : {} (v{})\n\
         // Engine  : ShardVault-RS\n\
         // ============================================================================\n\n\
         use serde::{{Deserialize, Serialize}};\n\
         use shardvault_rs::prelude::*;\n\
         use shardvault_rs::sync::oplog::{{init_oplog_schema, setup_table_cdc_triggers}};\n\n",
        blueprint.project_name, blueprint.version
    ));

    // 1. Implementasi SchemaProvider
    out.push_str(&format!(
        "pub struct {prov};\n\n\
         impl SchemaProvider for {prov} {{\n\
         \x20   fn init_global_schema(&self, conn: &Connection) -> Result<(), ShardVaultError> {{\n\
         \x20       conn.execute_batch(r#\"\n\
         PRAGMA foreign_keys = ON;\n",
        prov = provider_name
    ));

    for tbl in &blueprint.tables {
        if tbl.scope == TableScope::Global || tbl.scope == TableScope::Both {
            out.push_str(&compile_table_ddl(tbl, SqlDialect::Sqlite));
            out.push('\n');
        }
    }
    out.push_str("        \"#)?;\n");

    for tbl in &blueprint.tables {
        if (tbl.scope == TableScope::Global || tbl.scope == TableScope::Both) && tbl.enable_cdc {
            if let Some(pk) = tbl.pk_column() {
                let col_refs: Vec<String> =
                    tbl.columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
                out.push_str(&format!(
                    "        init_oplog_schema(conn)?;\n\
                     \x20       setup_table_cdc_triggers(conn, \"{}\", \"{}\", &[{}])?;\n",
                    tbl.name,
                    pk.name,
                    col_refs.join(", ")
                ));
            }
        }
    }
    out.push_str("        Ok(())\n    }\n\n");

    out.push_str(
        "    fn init_shard_schema(&self, conn: &Connection) -> Result<(), ShardVaultError> {\n\
         \x20       conn.execute_batch(r#\"\n\
         PRAGMA foreign_keys = ON;\n",
    );
    for tbl in &blueprint.tables {
        if tbl.scope == TableScope::Shard || tbl.scope == TableScope::Both {
            out.push_str(&compile_table_ddl(tbl, SqlDialect::Sqlite));
            out.push('\n');
        }
    }
    out.push_str("        \"#)?;\n");

    for tbl in &blueprint.tables {
        if (tbl.scope == TableScope::Shard || tbl.scope == TableScope::Both) && tbl.enable_cdc {
            if let Some(pk) = tbl.pk_column() {
                let col_refs: Vec<String> =
                    tbl.columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
                out.push_str(&format!(
                    "        init_oplog_schema(conn)?;\n\
                     \x20       setup_table_cdc_triggers(conn, \"{}\", \"{}\", &[{}])?;\n",
                    tbl.name,
                    pk.name,
                    col_refs.join(", ")
                ));
            }
        }
    }
    out.push_str("        Ok(())\n    }\n}\n\n");

    // 2. Domain Structs & Repositories
    for tbl in &blueprint.tables {
        out.push_str(&generate_table_struct_and_repo(tbl));
    }

    out
}
