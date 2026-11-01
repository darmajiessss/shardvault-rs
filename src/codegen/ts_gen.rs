// E:\shardvault-rs\src\codegen\ts_gen.rs
// Generator TypeScript Interfaces & DrawDB / Blueprint JSON

use super::ast::SchemaBlueprint;
use super::rust_gen::to_pascal_case;

/// Menghasilkan definisi TypeScript Interfaces (`export interface ...`) untuk Frontend / Tauri / API
pub fn generate_typescript_code(blueprint: &SchemaBlueprint) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// ============================================================================\n\
         // Auto-Generated TypeScript Interfaces\n\
         // Project : {} (v{})\n\
         // Engine  : ShardVault Studio Schema Generator\n\
         // ============================================================================\n\n",
        blueprint.project_name, blueprint.version
    ));

    for tbl in &blueprint.tables {
        let iface_name = to_pascal_case(&tbl.name);
        if !tbl.comment.is_empty() {
            out.push_str(&format!("/** {} (Scope: {}) */\n", tbl.comment, tbl.scope));
        } else {
            out.push_str(&format!("/** Table `{}` (Scope: {}) */\n", tbl.name, tbl.scope));
        }
        out.push_str(&format!("export interface {} {{\n", iface_name));
        for col in &tbl.columns {
            let ts_type = col.col_type.to_ts_type();
            let nullable = !col.not_null && !col.is_primary_key;
            let opt_suffix = if nullable { "?:" } else { ":" };
            let null_union = if nullable { " | null" } else { "" };
            let mut badges = Vec::new();
            if col.is_primary_key {
                badges.push("PK");
            }
            if col.encrypted_aes {
                badges.push("AES-256-GCM");
            }
            if col.include_in_hmac {
                badges.push("HMAC");
            }
            let comment_suffix = if badges.is_empty() {
                String::new()
            } else {
                format!(" // [{}]", badges.join(", "))
            };
            out.push_str(&format!(
                "  {}{}{} {}{};\n",
                col.name, opt_suffix, "", ts_type, null_union
            ));
            if !comment_suffix.is_empty() {
                // Sisipkan komentar di akhir baris sebelum newline
                out.pop(); // hapus '\n'
                out.push_str(&comment_suffix);
                out.push('\n');
            }
        }
        out.push_str("}\n\n");
    }

    out
}

/// Ekspor `SchemaBlueprint` menjadi JSON berformat rapi (`.ddb.json` / `.schema.json`)
pub fn export_blueprint_json(blueprint: &SchemaBlueprint) -> String {
    serde_json::to_string_pretty(blueprint).unwrap_or_else(|_| "{}".to_string())
}
