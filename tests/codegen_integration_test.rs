// E:\shardvault-rs\tests\codegen_integration_test.rs
// Pengujian Integrasi End-to-End: DrawDB Schema Blueprint, Foreign Key Builder,
// Issue Linter, Smart 12-Step Rebuild Migration, & Multi-Target Code Generator

use shardvault_rs::prelude::*;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_drawdb_blueprint_foreign_keys_and_issue_linter() {
    let bp = SchemaBlueprint::template_pos_retail();
    assert_eq!(bp.tables.len(), 4);
    assert_eq!(bp.subject_areas.len(), 2);
    assert_eq!(bp.notes.len(), 1);

    // Template bawaan harus bebas dari Error
    let issues = bp.validate_issues();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == IssueSeverity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Template POS tidak boleh memiliki error skema: {:?}",
        errors
    );

    // Uji deteksi tipe Foreign Key yang tidak cocok (mismatch)
    let mut bad_bp = bp.clone();
    // Ubah kolom category_id pada products menjadi TEXT, sementara categories.id adalah INTEGER
    let prod_tbl = bad_bp
        .tables
        .iter_mut()
        .find(|t| t.name == "products")
        .unwrap();
    let cat_fk_col = prod_tbl
        .columns
        .iter_mut()
        .find(|c| c.name == "category_id")
        .unwrap();
    cat_fk_col.col_type = SqliteAffinity::Text;

    let bad_issues = bad_bp.validate_issues();
    assert!(
        bad_issues.iter().any(|i| i.severity == IssueSeverity::Error
            && i.message.contains("Tipe data Foreign Key tidak cocok")),
        "Linter harus mendeteksi ketidakcocokan tipe data Foreign Key!"
    );
}

#[test]
fn test_add_foreign_key_triggers_12_step_safe_rebuild_preserving_data() {
    let dir = tempdir().unwrap();
    let config = EngineBuilder::new(dir.path())
        .with_salt("test_codegen_salt")
        .build_config();
    let engine =
        DatabaseEngine::with_config_and_schema(config, Arc::new(EmptySchemaProvider)).unwrap();

    // 1. Buat skema awal TANPA Foreign Key pada Global DB dan isi data
    {
        let conn_arc = engine.router().get_global_conn().unwrap();
        let conn = conn_arc.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE departments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                dept_name TEXT NOT NULL
            );
            CREATE TABLE employees (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                dept_id INTEGER NOT NULL,
                full_name TEXT NOT NULL
            );
            INSERT INTO departments (id, dept_name) VALUES (10, 'Cryptography R&D');
            INSERT INTO employees (id, dept_id, full_name) VALUES (1, 10, 'Raden Siti');
            INSERT INTO employees (id, dept_id, full_name) VALUES (2, 10, 'Budi Santoso');",
        )
        .unwrap();
    }

    // 2. Reverse-engineer skema aktif ke dalam SchemaBlueprint
    let manifest_before = {
        let conn_arc = engine.router().get_global_conn().unwrap();
        let conn = conn_arc.lock().unwrap();
        introspect_schema(&conn).unwrap()
    };
    let mut bp = SchemaBlueprint::from_live_manifest(&manifest_before, TableScope::Global);

    // 3. Tambahkan Foreign Key baru (employees.dept_id -> departments.id ON DELETE CASCADE)
    //    serta kolom baru `salary_grade` pada tabel `employees`
    {
        let emp_tbl = bp
            .tables
            .iter_mut()
            .find(|t| t.name == "employees")
            .unwrap();
        emp_tbl.foreign_keys.push(ForeignKeyBlueprint::new(
            "employees",
            "dept_id",
            "departments",
            "id",
            ForeignKeyAction::Cascade,
        ));
        let mut salary_col = ColumnBlueprint::new("salary_grade", SqliteAffinity::Text);
        salary_col.default_expr = Some("'L4'".to_string());
        emp_tbl.columns.push(salary_col);
    }

    // 4. Jalankan Smart Diff -> harus mendeteksi 12-Step Safe Table Rebuild karena penambahan Foreign Key
    let diff_plan =
        diff_blueprint_against_manifest(&bp, &manifest_before, Some(TableScope::Global), false);
    assert!(
        diff_plan.requires_rebuild,
        "Penambahan Foreign Key ke tabel eksisting wajib memicu 12-Step Safe Rebuild"
    );
    assert!(diff_plan
        .items
        .iter()
        .any(|i| i.kind == DiffActionKind::RebuildTable12Step && i.table_name == "employees"));

    // 5. Terapkan migrasi hasil Smart Diff menggunakan MigrationRunner
    let leaked_sql: &'static str = Box::leak(diff_plan.migration_sql.clone().into_boxed_str());
    let mut runner = MigrationRunner::new();
    runner.add(Migration {
        version: 1,
        name: "add_fk_employees_departments",
        target: MigrationTarget::GlobalOnly,
        up_sql: leaked_sql,
        down_sql: None,
    });
    let reports = engine.run_migrations(&runner).unwrap();
    assert!(!reports.is_empty());
    assert_eq!(reports[0].applied_versions, vec![1]);

    // 6. Verifikasi seluruh baris data lama tetap utuh DAN Foreign Key kini terdaftar di SQLite!
    {
        let conn_arc = engine.router().get_global_conn().unwrap();
        let conn = conn_arc.lock().unwrap();

        let row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM employees;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            row_count, 2,
            "Seluruh baris data lama harus tetap utuh setelah 12-Step Rebuild!"
        );

        let grade: String = conn
            .query_row(
                "SELECT salary_grade FROM employees WHERE id = 1;",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(grade, "L4");

        let manifest_after = introspect_schema(&conn).unwrap();
        let emp_after = manifest_after
            .tables
            .iter()
            .find(|t| t.name == "employees")
            .unwrap();
        assert_eq!(
            emp_after.foreign_keys.len(),
            1,
            "Foreign Key baru harus terdaftar secara fisik di SQLite!"
        );
        assert_eq!(emp_after.foreign_keys[0].target_table, "departments");
        assert_eq!(emp_after.foreign_keys[0].from_column, "dept_id");
        assert_eq!(emp_after.foreign_keys[0].to_column, "id");
        assert_eq!(emp_after.foreign_keys[0].on_delete, "CASCADE");
    }
}

#[test]
fn test_multi_target_code_and_dialect_generators() {
    let bp = SchemaBlueprint::template_pos_retail();

    // 1. SQLite, PostgreSQL, & MySQL DDL
    let sqlite_ddl = compile_blueprint_ddl(&bp, SqlDialect::Sqlite, None);
    assert!(sqlite_ddl.contains("FOREIGN KEY (\"category_id\") REFERENCES \"categories\"(\"id\")"));
    assert!(sqlite_ddl.contains("CREATE INDEX IF NOT EXISTS \"idx_products_category_id_fk\""));

    let pg_ddl = compile_blueprint_ddl(&bp, SqlDialect::Postgres, None);
    assert!(pg_ddl.contains("BIGSERIAL PRIMARY KEY"));

    let mysql_ddl = compile_blueprint_ddl(&bp, SqlDialect::Mysql, None);
    assert!(mysql_ddl.contains("AUTO_INCREMENT"));

    // 2. Rust Codegen (SchemaProvider + AES-256-GCM + HMAC Repo)
    let rust_code = generate_rust_code(&bp);
    assert!(rust_code.contains("impl SchemaProvider for RetailPosShardvaultSchemaProvider"));
    assert!(rust_code.contains("pub struct Products"));
    assert!(rust_code.contains("pub fn encrypt_fields"));
    assert!(rust_code.contains("pub fn decrypt_fields"));
    assert!(rust_code.contains("pub fn compute_row_hmac"));

    // 3. TypeScript Interfaces & Blueprint JSON
    let ts_code = generate_typescript_code(&bp);
    assert!(ts_code.contains("export interface Products"));
    assert!(ts_code.contains("cost_price_enc: string; // [AES-256-GCM, HMAC]"));

    let json_str = export_blueprint_json(&bp);
    let roundtrip: SchemaBlueprint = serde_json::from_str(&json_str).unwrap();
    assert_eq!(roundtrip.tables.len(), bp.tables.len());

    // 4. Reverse-Engineer langsung dari skrip SQL DDL mentah (DrawDB Import from SQL)
    let imported_from_sql =
        SchemaBlueprint::from_sql_ddl_script(&sqlite_ddl, TableScope::Shard).unwrap();
    assert_eq!(imported_from_sql.tables.len(), 4);
    let imp_prod = imported_from_sql
        .tables
        .iter()
        .find(|t| t.name == "products")
        .unwrap();
    assert_eq!(imp_prod.foreign_keys.len(), 1);
    assert_eq!(imp_prod.foreign_keys[0].ref_table, "categories");
}

#[test]
fn test_friendly_field_types_and_modular_presets_and_wiring() {
    // 1. Uji FriendlyFieldType (tipe data ramah orang awam)
    let cur_col = FriendlyFieldType::Currency.to_column_blueprint("harga_jual");
    assert_eq!(cur_col.name, "harga_jual");
    assert_eq!(cur_col.col_type, SqliteAffinity::Integer);
    assert_eq!(cur_col.default_expr, Some("0".to_string()));
    assert_eq!(FriendlyFieldType::from_column_blueprint(&cur_col), FriendlyFieldType::Currency);

    let sec_col = FriendlyFieldType::SecretText.to_column_blueprint("pin_rahasia");
    assert!(sec_col.encrypted_aes);
    assert_eq!(FriendlyFieldType::from_column_blueprint(&sec_col), FriendlyFieldType::SecretText);

    // 2. Uji ModularTablePreset (Template blok tabel siap pakai)
    let catalog_tables = ModularTablePreset::CatalogModule.instantiate(50.0, 50.0);
    assert_eq!(catalog_tables.len(), 2);
    assert!(catalog_tables.iter().any(|t| t.name == "kategori"));
    assert!(catalog_tables.iter().any(|t| t.name == "produk"));

    let produk_tbl = catalog_tables.iter().find(|t| t.name == "produk").unwrap();
    assert_eq!(produk_tbl.foreign_keys.len(), 1);
    assert_eq!(produk_tbl.foreign_keys[0].ref_table, "kategori");
    assert_eq!(produk_tbl.foreign_keys[0].from_column, "kategori_id");

    // 3. Uji Port-to-Card Automatic Wiring (connect_tables_automatically)
    let mut bp = SchemaBlueprint::new_empty("ModularWiringTest");
    let mut parent = TableBlueprint::new("suppliers", TableScope::Global, 10.0, 10.0);
    parent.columns.push(ColumnBlueprint::primary_key_int("id"));
    parent.columns.push(ColumnBlueprint::new("supplier_name", SqliteAffinity::Text));
    bp.tables.push(parent);

    let mut child = TableBlueprint::new("purchase_orders", TableScope::Shard, 300.0, 10.0);
    child.columns.push(ColumnBlueprint::primary_key_int("id"));
    bp.tables.push(child);

    // Hubungkan suppliers (idx 0) ke purchase_orders (idx 1)
    let wired = bp.connect_tables_automatically(0, 1);
    assert!(wired.is_ok(), "Wajib berhasil menghubungkan parent ke child secara otomatis: {:?}", wired);
    let child_after = &bp.tables[1];
    assert!(child_after.columns.iter().any(|c| c.name == "supplier_id"));
    assert_eq!(child_after.foreign_keys.len(), 1);
    assert_eq!(child_after.foreign_keys[0].from_column, "supplier_id");
    assert_eq!(child_after.foreign_keys[0].ref_table, "suppliers");
    assert_eq!(child_after.foreign_keys[0].ref_column, "id");
    assert!(child_after.foreign_keys[0].auto_index_fk);

    // 4. Uji Merge Blueprint (Drag & Drop modular schema files / templates)
    let mut other_bp = SchemaBlueprint::new_empty("OtherSubsystem");
    let other_tbl = TableBlueprint::new("warehouses", TableScope::Global, 0.0, 0.0);
    other_bp.tables.push(other_tbl);
    bp.merge_blueprint(other_bp, 100.0, 100.0);

    assert_eq!(bp.tables.len(), 3);
    let merged_tbl = bp.tables.iter().find(|t| t.name == "warehouses").unwrap();
    assert_eq!(merged_tbl.x, 100.0);
    assert_eq!(merged_tbl.y, 100.0);
}

#[test]
fn test_schema_canvas_scrolling_and_auto_centering() {
    use shardvault_rs::studio::SchemaInspectorView;

    let mut view = SchemaInspectorView::default();
    assert_eq!(view.blueprint.tables.len(), 4);
    assert_eq!(view.blueprint.subject_areas.len(), 2);

    // 1. Uji kalkulasi batas luar (bounding box) seluruh item skema
    let (min_x, max_x, min_y, max_y) = view.compute_schema_bounds();
    assert!(min_x < max_x, "min_x harus lebih kecil dari max_x: {} vs {}", min_x, max_x);
    assert!(min_y < max_y, "min_y harus lebih kecil dari max_y: {} vs {}", min_y, max_y);
    assert!(max_y >= 595.0, "max_y harus mencakup area domain bawah (>595px), aktual: {}", max_y);

    // 2. Uji fitur Pusatkan Semua Item (Auto-Centering & Fit View)
    let canvas_size = eframe::egui::Vec2::new(1200.0, 800.0);
    view.center_all_items(canvas_size);

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    assert_eq!(view.canvas_pan.x + center_x, 600.0, "Titik tengah skema harus tepat di tengah kanvas X");
    assert_eq!(view.canvas_pan.y + center_y, 400.0, "Titik tengah skema harus tepat di tengah kanvas Y");

    // 3. Uji geser / scroll kanvas vertikal dan horizontal
    let pan_before = view.canvas_pan;
    view.canvas_pan.y -= 160.0; // scroll ke bawah
    assert_eq!(view.canvas_pan.y, pan_before.y - 160.0);

    view.canvas_pan.x += 160.0; // scroll ke kanan
    assert_eq!(view.canvas_pan.x, pan_before.x + 160.0);
}
