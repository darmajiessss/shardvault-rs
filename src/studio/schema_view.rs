// E:\shardvault-rs\src\studio\schema_view.rs
// DrawDB-Powered Visual Schema Designer, Interactive ER Canvas, Foreign Key Builder,
// Smart 12-Step Diff Deployer, & Multi-Target Code Generator

use eframe::egui::{
    self, Align2, Color32, ComboBox, FontId, Grid, Pos2, Rect, RichText, Rounding, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};

use super::model::{
    fetch_columns, fetch_table_ddl, fetch_table_indexes, fetch_tables, SelectedTarget,
};
use super::theme::{
    COLOR_BG_SURFACE, COLOR_BORDER, COLOR_DANGER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING,
    COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::codegen::{
    compile_blueprint_ddl, diff_blueprint_against_manifest, export_blueprint_json,
    generate_rust_code, generate_typescript_code, Cardinality, ColumnBlueprint, DiffActionKind,
    ForeignKeyAction, ForeignKeyBlueprint, FriendlyFieldType, IndexBlueprint, IssueSeverity,
    ModularTablePreset, NoteBlueprint, SchemaBlueprint, SqlDialect, SqliteAffinity,
    SubjectAreaBlueprint, TableBlueprint, TableScope,
};
use crate::engine::DatabaseEngine;
use crate::migration::{introspect_schema, Migration, MigrationRunner, MigrationTarget};
use crate::types::ShardRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaStudioMode {
    ErCanvas,
    TableAndFkDesigner,
    IssuesLinter,
    SmartDiffDeploy,
    CodeGenerator,
    LiveDbInspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodegenOutputFormat {
    SqliteDdl,
    PostgresDdl,
    MysqlDdl,
    RustSchemaAndRepo,
    TypeScriptInterfaces,
    BlueprintJson,
}

pub struct SchemaInspectorView {
    pub mode: SchemaStudioMode,
    pub blueprint: SchemaBlueprint,
    pub selected_bp_table_idx: usize,

    // ER Canvas state
    pub canvas_pan: Vec2,

    // Form tambah tabel cepat
    pub new_table_name: String,
    pub new_table_scope: TableScope,

    // Form tambah kolom cepat
    pub new_col_name: String,
    pub new_col_type: SqliteAffinity,

    // Form tambah Foreign Key cepat
    pub new_fk_from_col: String,
    pub new_fk_ref_table: String,
    pub new_fk_ref_col: String,
    pub new_fk_cardinality: Cardinality,
    pub new_fk_on_delete: ForeignKeyAction,
    pub new_fk_auto_index: bool,

    // Form tambah Index cepat
    pub new_idx_name: String,
    pub new_idx_cols_csv: String,
    pub new_idx_unique: bool,

    // Smart Diff & Deploy state
    pub allow_destructive_drops: bool,
    pub deploy_migration_version: u32,
    pub deploy_migration_name: String,
    pub deploy_target: MigrationTarget,

    // Code Generator state
    pub codegen_format: CodegenOutputFormat,
    pub export_file_path: String,

    // Live DB Inspector state
    pub selected_table_inspect: Option<String>,

    // Modular Drag & Drop & Quick Inspector state
    pub active_wire_drag: Option<usize>,
    pub show_palette_panel: bool,
    pub show_inspector_panel: bool,
    pub quick_field_name: String,
    pub quick_field_type: FriendlyFieldType,
}

impl Default for SchemaInspectorView {
    fn default() -> Self {
        Self {
            mode: SchemaStudioMode::ErCanvas,
            blueprint: SchemaBlueprint::template_pos_retail(),
            selected_bp_table_idx: 0,
            canvas_pan: Vec2::ZERO,
            new_table_name: "new_table".to_string(),
            new_table_scope: TableScope::Shard,
            new_col_name: "new_column".to_string(),
            new_col_type: SqliteAffinity::Text,
            new_fk_from_col: String::new(),
            new_fk_ref_table: String::new(),
            new_fk_ref_col: "id".to_string(),
            new_fk_cardinality: Cardinality::ManyToOne,
            new_fk_on_delete: ForeignKeyAction::Cascade,
            new_fk_auto_index: true,
            new_idx_name: String::new(),
            new_idx_cols_csv: String::new(),
            new_idx_unique: false,
            allow_destructive_drops: false,
            deploy_migration_version: 20261101,
            deploy_migration_name: "schema_designer_sync".to_string(),
            deploy_target: MigrationTarget::All,
            codegen_format: CodegenOutputFormat::RustSchemaAndRepo,
            export_file_path: "./generated_schema.rs".to_string(),
            selected_table_inspect: None,
            active_wire_drag: None,
            show_palette_panel: true,
            show_inspector_panel: true,
            quick_field_name: "kolom_baru".to_string(),
            quick_field_type: FriendlyFieldType::Text,
        }
    }
}

impl SchemaInspectorView {
    pub fn render(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        selected_table: &Option<String>,
        toast: &mut Option<String>,
    ) {
        // 1. HEADER & SUB-TAB NAVIGATION (DrawDB Studio Bar)
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("📐 DRAWDB SCHEMA DESIGNER & CODE GENERATOR")
                    .size(16.0)
                    .color(COLOR_TEXT_PRIMARY)
                    .strong(),
            );

            ui.separator();

            let issues = self.blueprint.validate_issues();
            let err_count = issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Error)
                .count();
            let warn_count = issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();

            let issues_label = if err_count > 0 {
                format!("⚠ Issues ({} Err, {} Warn)", err_count, warn_count)
            } else if warn_count > 0 {
                format!("⚠ Issues ({} Warn)", warn_count)
            } else {
                "✅ Issues (0)".to_string()
            };

            let tabs = [
                (SchemaStudioMode::ErCanvas, "📐 ER Diagram (DrawDB)"),
                (
                    SchemaStudioMode::TableAndFkDesigner,
                    "🔧 Tables & Foreign Keys",
                ),
                (SchemaStudioMode::IssuesLinter, issues_label.as_str()),
                (SchemaStudioMode::SmartDiffDeploy, "🔄 Smart Diff & Deploy"),
                (SchemaStudioMode::CodeGenerator, "💻 Code & SQL Generator"),
                (SchemaStudioMode::LiveDbInspector, "🔍 Live DB Inspector"),
            ];

            for (m, label) in tabs {
                let is_sel = self.mode == m;
                let color = if is_sel {
                    COLOR_SYNCING
                } else if m == SchemaStudioMode::IssuesLinter && err_count > 0 {
                    COLOR_DANGER
                } else {
                    COLOR_TEXT_SECONDARY
                };
                if ui
                    .selectable_label(is_sel, RichText::new(label).color(color).strong())
                    .clicked()
                {
                    self.mode = m;
                }
            }
        });

        ui.add_space(4.0);

        // Bar Aksi Cepat Blueprint (Reverse-Engineer, Template, Auto-Layout)
        ui.horizontal(|ui| {
            ui.label(RichText::new("Project:").color(COLOR_TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut self.blueprint.project_name).desired_width(150.0));

            if ui
                .button(RichText::new("📥 Import dari Live DB").color(COLOR_HEALTHY))
                .on_hover_text("Reverse-engineer tabel, kolom, dan Foreign Key dari koneksi database aktif")
                .clicked()
            {
                if let Some(conn_arc) = self.open_target_conn(engine, selected_target, toast) {
                    let conn = conn_arc.lock().unwrap();
                    match introspect_schema(&conn) {
                        Ok(manifest) => {
                            let default_scope = match selected_target {
                                SelectedTarget::Global(_) => TableScope::Global,
                                SelectedTarget::Shard { .. } => TableScope::Shard,
                            };
                            self.blueprint = SchemaBlueprint::from_live_manifest(&manifest, default_scope);
                            self.selected_bp_table_idx = 0;
                            *toast = Some(format!(
                                "Berhasil mengimpor {} tabel dari {} ke DrawDB Editor!",
                                self.blueprint.tables.len(),
                                selected_target.display_title()
                            ));
                        }
                        Err(e) => {
                            *toast = Some(format!("Gagal introspeksi DB: {}", e));
                        }
                    }
                }
            }

            if ui.button("📦 Template: POS & Retail").clicked() {
                self.blueprint = SchemaBlueprint::template_pos_retail();
                self.selected_bp_table_idx = 0;
                *toast = Some("Template POS & Retail dimuat!".into());
            }

            if ui.button("🔒 Template: SaaS Vault").clicked() {
                self.blueprint = SchemaBlueprint::template_saas_multitenant();
                self.selected_bp_table_idx = 0;
                *toast = Some("Template Multi-Tenant SaaS Vault dimuat!".into());
            }

            if ui.button("✨ Auto-Layout Grid").clicked() {
                for (idx, tbl) in self.blueprint.tables.iter_mut().enumerate() {
                    let col = idx % 2;
                    let row = idx / 2;
                    tbl.x = 50.0 + (col as f32) * 400.0;
                    tbl.y = 70.0 + (row as f32) * 290.0;
                }
                self.canvas_pan = Vec2::ZERO;
                *toast = Some("Tata letak diagram ER dirapikan!".into());
            }

            if ui.button("🔄 Reset Kosong").clicked() {
                self.blueprint = SchemaBlueprint::new_empty("New_ShardVault_Schema");
                self.selected_bp_table_idx = 0;
            }
        });

        ui.separator();

        match self.mode {
            SchemaStudioMode::ErCanvas => self.render_er_canvas(ui, engine, selected_target, toast),
            SchemaStudioMode::TableAndFkDesigner => self.render_table_and_fk_designer(ui, toast),
            SchemaStudioMode::IssuesLinter => self.render_issues_linter(ui),
            SchemaStudioMode::SmartDiffDeploy => {
                self.render_smart_diff_deploy(ui, engine, selected_target, toast)
            }
            SchemaStudioMode::CodeGenerator => self.render_code_generator(ui, toast),
            SchemaStudioMode::LiveDbInspector => {
                self.render_live_db_inspector(ui, engine, selected_target, selected_table, toast)
            }
        }
    }

    fn open_target_conn(
        &self,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        toast: &mut Option<String>,
    ) -> Option<std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>> {
        let res = match selected_target {
            SelectedTarget::Global(role) => {
                if *role == ShardRole::PrimaryWrite {
                    engine.router().get_global_conn()
                } else {
                    engine.router().get_global_clone(*role)
                }
            }
            SelectedTarget::Shard { key, role } => engine.router().get_shard_conn(key, *role),
        };
        match res {
            Ok(c) => Some(c),
            Err(e) => {
                *toast = Some(format!("Gagal membuka koneksi: {}", e));
                None
            }
        }
    }

    // ========================================================================
    // MODE 1: DRAWDB INTERACTIVE ER DIAGRAM CANVAS (MODULAR & RAMAH AWAM)
    // ========================================================================
    fn render_er_canvas(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        toast: &mut Option<String>,
    ) {
        // 1. Dukungan Drag & Drop File Langsung dari Windows Explorer (.ddb.json atau .sql)
        let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() {
            for file in dropped_files {
                if let Some(path) = file.path {
                    let path_str = path.to_string_lossy().to_string();
                    if path_str.ends_with(".json") || path_str.ends_with(".ddb.json") {
                        if let Ok(raw) = std::fs::read_to_string(&path) {
                            if let Ok(other_bp) = serde_json::from_str::<SchemaBlueprint>(&raw) {
                                self.blueprint.merge_blueprint(
                                    other_bp,
                                    60.0 - self.canvas_pan.x,
                                    60.0 - self.canvas_pan.y,
                                );
                                *toast = Some(format!(
                                    "Modul skema '{}' berhasil digabungkan ke kanvas!",
                                    path.file_name().unwrap_or_default().to_string_lossy()
                                ));
                            }
                        }
                    } else if path_str.ends_with(".sql") {
                        if let Ok(raw) = std::fs::read_to_string(&path) {
                            if let Ok(other_bp) =
                                SchemaBlueprint::from_sql_ddl_script(&raw, TableScope::Shard)
                            {
                                self.blueprint.merge_blueprint(
                                    other_bp,
                                    60.0 - self.canvas_pan.x,
                                    60.0 - self.canvas_pan.y,
                                );
                                *toast = Some(format!(
                                    "Skrip SQL '{}' berhasil diimpor ke kanvas!",
                                    path.file_name().unwrap_or_default().to_string_lossy()
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 2. Toolbar Atas Kanvas (Tombol Simpan 1-Klik, Reset Layar, Toggle Panel)
        ui.horizontal(|ui| {
            let pal_text = if self.show_palette_panel {
                "◀ Sembunyikan Palet"
            } else {
                "▶ Buka Palet Blok"
            };
            if ui.selectable_label(self.show_palette_panel, pal_text).clicked() {
                self.show_palette_panel = !self.show_palette_panel;
            }

            ui.separator();

            // Tombol Hijau 1-Klik: Simpan & Terapkan ke Database Aktif
            if ui
                .button(
                    RichText::new("💾 Simpan & Terapkan ke Database")
                        .size(12.5)
                        .color(COLOR_HEALTHY)
                        .strong(),
                )
                .on_hover_text("Bandingkan perubahan visual dengan database dan simpan langsung secara aman tanpa kehilangan data lama")
                .clicked()
            {
                if let Some(conn_arc) = self.open_target_conn(engine, selected_target, toast) {
                    let manifest_res = {
                        let conn = conn_arc.lock().unwrap();
                        introspect_schema(&conn)
                    };
                    match manifest_res {
                        Ok(manifest) => {
                            let scope_filter = match selected_target {
                                SelectedTarget::Global(_) => Some(TableScope::Global),
                                SelectedTarget::Shard { .. } => Some(TableScope::Shard),
                            };
                            let diff_plan = diff_blueprint_against_manifest(
                                &self.blueprint,
                                &manifest,
                                scope_filter,
                                self.allow_destructive_drops,
                            );
                            if diff_plan.is_up_to_date() {
                                *toast = Some("Database sudah 100% selaras dengan skema kanvas!".into());
                            } else {
                                let leaked_name: &'static str = Box::leak(
                                    format!("studio_auto_sync_{}", self.deploy_migration_version)
                                        .into_boxed_str(),
                                );
                                let leaked_sql: &'static str =
                                    Box::leak(diff_plan.migration_sql.clone().into_boxed_str());
                                let mut runner = MigrationRunner::new();
                                runner.add(Migration {
                                    version: self.deploy_migration_version,
                                    name: leaked_name,
                                    target: self.deploy_target,
                                    up_sql: leaked_sql,
                                    down_sql: None,
                                });
                                match engine.run_migrations(&runner) {
                                    Ok(reports) => {
                                        self.deploy_migration_version += 1;
                                        *toast = Some(format!(
                                            "Berhasil! Perubahan skema disimpan ke {} database! (12-Step Rebuild: {})",
                                            reports.len(),
                                            if diff_plan.requires_rebuild { "Ya" } else { "Tidak" }
                                        ));
                                    }
                                    Err(e) => {
                                        *toast = Some(format!("Gagal menerapkan ke database: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => *toast = Some(format!("Gagal introspeksi database: {}", e)),
                    }
                }
            }

            ui.separator();

            if ui.button("🎯 Reset Posisi Layar").clicked() {
                self.canvas_pan = Vec2::ZERO;
            }
            if ui.button("📝 + Catatan").clicked() {
                self.blueprint.notes.push(NoteBlueprint {
                    title: "Catatan Baru".to_string(),
                    content: "Tuliskan keterangan di sini.".to_string(),
                    color_hex: "#f59e0b".to_string(),
                    x: 120.0 - self.canvas_pan.x,
                    y: 120.0 - self.canvas_pan.y,
                });
            }
            if ui.button("📦 + Area Domain").clicked() {
                self.blueprint.subject_areas.push(SubjectAreaBlueprint {
                    name: "Area Baru".to_string(),
                    color_hex: "#0284c7".to_string(),
                    x: 40.0 - self.canvas_pan.x,
                    y: 40.0 - self.canvas_pan.y,
                    width: 460.0,
                    height: 320.0,
                });
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let insp_text = if self.show_inspector_panel {
                    "Tutup Pengaturan ▶"
                } else {
                    "◀ Buka Pengaturan"
                };
                if ui
                    .selectable_label(self.show_inspector_panel, insp_text)
                    .clicked()
                {
                    self.show_inspector_panel = !self.show_inspector_panel;
                }
            });
        });

        ui.add_space(4.0);

        let total_width = ui.available_width();
        let total_height = ui.available_height().max(520.0);

        let mut canvas_width = total_width;
        if self.show_palette_panel {
            canvas_width -= 222.0;
        }
        if self.show_inspector_panel {
            canvas_width -= 275.0;
        }
        canvas_width = canvas_width.max(300.0);

        // 3. Tata Letak 3-Kolom: [Palet Kiri] [Kanvas Tengah] [Pengaturan Kanan]
        ui.horizontal(|ui| {
            // ================================================================
            // KOLOM KIRI: PALET BLOK DATABASE (MODUL & PRESET RAMAH AWAM)
            // ================================================================
            if self.show_palette_panel {
                ui.group(|ui| {
                    ui.set_width(210.0);
                    ui.set_height(total_height);
                    ScrollArea::vertical()
                        .id_salt("palette_scroll_area")
                        .show(ui, |ui| {
                            ui.heading(
                                RichText::new("📦 BLOK DATABASE")
                                    .size(13.0)
                                    .color(COLOR_SYNCING)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new("Klik blok di bawah untuk menyusun database semudah bermain lego!")
                                    .size(10.5)
                                    .color(COLOR_TEXT_MUTED),
                            );
                            ui.add_space(6.0);

                            ui.label(
                                RichText::new("🧱 Modul Tabel Siap Pakai")
                                    .size(11.5)
                                    .strong()
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                            for preset in ModularTablePreset::all() {
                                if ui
                                    .button(RichText::new(preset.display_name()).size(11.0))
                                    .clicked()
                                {
                                    let center_x = 80.0 - self.canvas_pan.x;
                                    let center_y = 80.0 - self.canvas_pan.y;
                                    let new_tables = preset.instantiate(center_x, center_y);
                                    let count = new_tables.len();
                                    for tbl in new_tables {
                                        self.blueprint.tables.push(tbl);
                                    }
                                    self.selected_bp_table_idx =
                                        self.blueprint.tables.len().saturating_sub(1);
                                    *toast = Some(format!(
                                        "{} tabel dari '{}' ditambahkan ke kanvas!",
                                        count,
                                        preset.display_name()
                                    ));
                                }
                            }

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(4.0);

                            ui.label(
                                RichText::new("🧩 Tambah Kolom Cepat")
                                    .size(11.5)
                                    .strong()
                                    .color(COLOR_HEALTHY),
                            );
                            let sel_tbl_name = self
                                .blueprint
                                .tables
                                .get(self.selected_bp_table_idx)
                                .map(|t| t.name.as_str())
                                .unwrap_or("<pilih tabel>");
                            ui.label(
                                RichText::new(format!("Ke tabel aktif: {}", sel_tbl_name))
                                    .size(10.5)
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                            ui.add_space(4.0);

                            for field_type in FriendlyFieldType::all() {
                                if ui
                                    .button(RichText::new(field_type.display_name()).size(11.0))
                                    .clicked()
                                {
                                    if let Some(tbl) =
                                        self.blueprint.tables.get_mut(self.selected_bp_table_idx)
                                    {
                                        let c_name = format!("kolom_{}", tbl.columns.len() + 1);
                                        tbl.columns.push(field_type.to_column_blueprint(&c_name));
                                        *toast = Some(format!(
                                            "Kolom '{}' ({}) ditambahkan ke tabel '{}'!",
                                            c_name,
                                            field_type.display_name(),
                                            tbl.name
                                        ));
                                    } else {
                                        *toast = Some(
                                            "Pilih salah satu kartu tabel di kanvas terlebih dahulu!"
                                                .into(),
                                        );
                                    }
                                }
                            }

                            if ui
                                .button(RichText::new("⏱ + Waktu Buat & Ubah").size(11.0))
                                .clicked()
                            {
                                if let Some(tbl) =
                                    self.blueprint.tables.get_mut(self.selected_bp_table_idx)
                                {
                                    tbl.columns.push(
                                        FriendlyFieldType::Timestamp
                                            .to_column_blueprint("created_at"),
                                    );
                                    tbl.columns.push(
                                        FriendlyFieldType::Text.to_column_blueprint("updated_at"),
                                    );
                                    *toast = Some(format!(
                                        "Audit timestamps ditambahkan ke tabel '{}'!",
                                        tbl.name
                                    ));
                                }
                            }
                        });
                });
            }

            // ================================================================
            // KOLOM TENGAH: KANVAS ER DIAGRAM INTERAKTIF DENGAN WIRING
            // ================================================================
            ui.vertical(|ui| {
                ui.set_width(canvas_width);
                ui.set_height(total_height);

                let (canvas_rect, canvas_response) =
                    ui.allocate_exact_size(Vec2::new(canvas_width, total_height), Sense::click_and_drag());

                if canvas_response.dragged_by(egui::PointerButton::Secondary)
                    || canvas_response.dragged_by(egui::PointerButton::Middle)
                {
                    self.canvas_pan += canvas_response.drag_delta();
                }

                let painter = ui.painter_at(canvas_rect);

                // Grid Latar Belakang
                painter.rect_filled(canvas_rect, Rounding::same(6.0), Color32::from_rgb(11, 17, 32));
                let grid_step = 28.0_f32;
                let offset_x = self.canvas_pan.x.rem_euclid(grid_step);
                let offset_y = self.canvas_pan.y.rem_euclid(grid_step);

                let mut gx = canvas_rect.left() + offset_x;
                while gx < canvas_rect.right() {
                    let mut gy = canvas_rect.top() + offset_y;
                    while gy < canvas_rect.bottom() {
                        painter.circle_filled(
                            Pos2::new(gx, gy),
                            1.0,
                            Color32::from_rgba_unmultiplied(71, 85, 105, 90),
                        );
                        gy += grid_step;
                    }
                    gx += grid_step;
                }

                let origin = canvas_rect.min.to_vec2() + self.canvas_pan;

                // Gambar Subject Areas (Group Drag & Resize)
                for (a_idx, area) in self.blueprint.subject_areas.iter_mut().enumerate() {
                    let area_rect = Rect::from_min_size(
                        Pos2::new(origin.x + area.x, origin.y + area.y),
                        Vec2::new(area.width, area.height),
                    );
                    let id = ui.make_persistent_id(format!("area_card_{}_{}", a_idx, area.name));
                    let resp = ui.interact(area_rect, id, Sense::click_and_drag());
                    if resp.dragged() {
                        let delta = resp.drag_delta();
                        let prev_x = area.x;
                        let prev_y = area.y;
                        area.x += delta.x;
                        area.y += delta.y;
                        // Group Drag: geser tabel-tabel di dalam Subject Area ini
                        for tbl in &mut self.blueprint.tables {
                            if tbl.x >= prev_x
                                && tbl.x <= prev_x + area.width
                                && tbl.y >= prev_y
                                && tbl.y <= prev_y + area.height
                            {
                                tbl.x += delta.x;
                                tbl.y += delta.y;
                            }
                        }
                    }

                    painter.rect_filled(
                        area_rect,
                        Rounding::same(8.0),
                        Color32::from_rgba_unmultiplied(30, 58, 138, 38),
                    );
                    painter.rect_stroke(
                        area_rect,
                        Rounding::same(8.0),
                        Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 120)),
                    );
                    painter.text(
                        Pos2::new(area_rect.left() + 10.0, area_rect.top() + 8.0),
                        Align2::LEFT_TOP,
                        &area.name,
                        FontId::proportional(13.0),
                        COLOR_SYNCING,
                    );

                    // Resize handle di pojok kanan bawah
                    let resize_rect = Rect::from_min_size(
                        Pos2::new(area_rect.right() - 16.0, area_rect.bottom() - 16.0),
                        Vec2::new(16.0, 16.0),
                    );
                    let resize_id = ui.make_persistent_id(format!("area_resize_{}", a_idx));
                    let resize_resp = ui.interact(resize_rect, resize_id, Sense::drag());
                    if resize_resp.dragged() {
                        area.width = (area.width + resize_resp.drag_delta().x).max(200.0);
                        area.height = (area.height + resize_resp.drag_delta().y).max(140.0);
                    }
                    painter.text(
                        resize_rect.center(),
                        Align2::CENTER_CENTER,
                        "◢",
                        FontId::monospace(12.0),
                        COLOR_SYNCING,
                    );
                }

                // Gambar Kartu Tabel Interaktif
                let card_width = 250.0_f32;
                let header_height = 32.0_f32;
                let row_height = 20.0_f32;

                let mut clicked_table_idx = None;
                for (idx, tbl) in self.blueprint.tables.iter_mut().enumerate() {
                    let card_height =
                        header_height + (tbl.columns.len().max(1) as f32 * row_height) + 8.0;
                    let card_rect = Rect::from_min_size(
                        Pos2::new(origin.x + tbl.x, origin.y + tbl.y),
                        Vec2::new(card_width, card_height),
                    );

                    let id = ui.make_persistent_id(format!("er_table_card_{}_{}", idx, tbl.name));
                    let resp = ui.interact(card_rect, id, Sense::click_and_drag());
                    if resp.dragged() && self.active_wire_drag.is_none() {
                        tbl.x += resp.drag_delta().x;
                        tbl.y += resp.drag_delta().y;
                        clicked_table_idx = Some(idx);
                    }
                    if resp.clicked() {
                        clicked_table_idx = Some(idx);
                        self.show_inspector_panel = true;
                    }

                    let is_selected = self.selected_bp_table_idx == idx;
                    let header_color = match tbl.scope {
                        TableScope::Global => Color32::from_rgb(2, 132, 199), // Sky Blue
                        TableScope::Shard => Color32::from_rgb(126, 34, 206), // Purple
                        TableScope::Both => Color32::from_rgb(5, 150, 105),   // Emerald
                    };

                    // Body Kartu
                    painter.rect_filled(card_rect, Rounding::same(6.0), COLOR_BG_SURFACE);

                    // Header Kartu
                    let header_rect =
                        Rect::from_min_size(card_rect.min, Vec2::new(card_width, header_height));
                    painter.rect_filled(header_rect, Rounding::same(6.0), header_color);

                    let border_stroke = if is_selected {
                        Stroke::new(2.2_f32, COLOR_HEALTHY)
                    } else {
                        Stroke::new(1.0_f32, COLOR_BORDER)
                    };
                    painter.rect_stroke(card_rect, Rounding::same(6.0), border_stroke);

                    // Judul Tabel (Bersih tanpa karakter unicode yang rapuh)
                    let cdc_badge = if tbl.enable_cdc { " [CDC]" } else { "" };
                    painter.text(
                        Pos2::new(header_rect.left() + 10.0, header_rect.center().y),
                        Align2::LEFT_CENTER,
                        format!("{}{}", tbl.name, cdc_badge),
                        FontId::proportional(13.0),
                        Color32::WHITE,
                    );

                    // Tombol Port "● Hubungkan" (Tarik garis relasi)
                    let wire_btn_rect = Rect::from_min_size(
                        Pos2::new(header_rect.right() - 86.0, header_rect.top() + 5.0),
                        Vec2::new(78.0, 22.0),
                    );
                    let wire_id = ui.make_persistent_id(format!("wire_btn_{}", idx));
                    let wire_resp = ui.interact(wire_btn_rect, wire_id, Sense::click_and_drag());
                    painter.rect_filled(
                        wire_btn_rect,
                        Rounding::same(3.0),
                        Color32::from_rgba_unmultiplied(15, 23, 42, 210),
                    );
                    painter.text(
                        wire_btn_rect.center(),
                        Align2::CENTER_CENTER,
                        "● Hubungkan",
                        FontId::monospace(10.0),
                        COLOR_WARNING,
                    );
                    if wire_resp.drag_started() {
                        self.active_wire_drag = Some(idx);
                    }

                    // Daftar Kolom dengan Badge Visual Anti-Slop (PK / FK / AES)
                    for (c_idx, col) in tbl.columns.iter().enumerate() {
                        let row_y =
                            card_rect.top() + header_height + (c_idx as f32 * row_height) + 4.0;
                        let is_fk = tbl
                            .foreign_keys
                            .iter()
                            .any(|f| f.from_column.eq_ignore_ascii_case(&col.name));

                        // Gambar Badge Kolom (PK / FK) ala DrawDB
                        let mut text_x = card_rect.left() + 8.0;
                        if col.is_primary_key {
                            let pk_rect = Rect::from_min_size(
                                Pos2::new(text_x, row_y + 3.0),
                                Vec2::new(20.0, 14.0),
                            );
                            painter.rect_filled(
                                pk_rect,
                                Rounding::same(3.0),
                                Color32::from_rgb(245, 158, 11),
                            );
                            painter.text(
                                pk_rect.center(),
                                Align2::CENTER_CENTER,
                                "PK",
                                FontId::monospace(9.0),
                                Color32::from_rgb(15, 23, 42),
                            );
                            text_x += 24.0;
                        }
                        if is_fk {
                            let fk_rect = Rect::from_min_size(
                                Pos2::new(text_x, row_y + 3.0),
                                Vec2::new(20.0, 14.0),
                            );
                            painter.rect_filled(
                                fk_rect,
                                Rounding::same(3.0),
                                Color32::from_rgb(59, 130, 246),
                            );
                            painter.text(
                                fk_rect.center(),
                                Align2::CENTER_CENTER,
                                "FK",
                                FontId::monospace(9.0),
                                Color32::WHITE,
                            );
                            text_x += 24.0;
                        }

                        let name_color = if col.is_primary_key {
                            COLOR_WARNING
                        } else if is_fk {
                            COLOR_SYNCING
                        } else if col.encrypted_aes {
                            COLOR_HEALTHY
                        } else {
                            COLOR_TEXT_PRIMARY
                        };

                        painter.text(
                            Pos2::new(text_x, row_y + row_height * 0.5),
                            Align2::LEFT_CENTER,
                            &col.name,
                            FontId::monospace(11.5),
                            name_color,
                        );

                        let nn_mark = if col.not_null { "*" } else { "?" };
                        let sec_tag = if col.encrypted_aes { "AES " } else { "" };
                        painter.text(
                            Pos2::new(card_rect.right() - 8.0, row_y + row_height * 0.5),
                            Align2::RIGHT_CENTER,
                            format!("{}{}{}", sec_tag, col.col_type, nn_mark),
                            FontId::monospace(10.5),
                            COLOR_PURPLE,
                        );
                    }
                }

                if let Some(sel) = clicked_table_idx {
                    self.selected_bp_table_idx = sel;
                }

                // Gambar Garis Relasi Foreign Key
                for child_tbl in &self.blueprint.tables {
                    let child_pos = Pos2::new(origin.x + child_tbl.x, origin.y + child_tbl.y);

                    for fk in &child_tbl.foreign_keys {
                        if let Some(parent_tbl) = self
                            .blueprint
                            .tables
                            .iter()
                            .find(|t| t.name.eq_ignore_ascii_case(&fk.ref_table))
                        {
                            let parent_pos =
                                Pos2::new(origin.x + parent_tbl.x, origin.y + parent_tbl.y);

                            let child_col_idx = child_tbl
                                .columns
                                .iter()
                                .position(|c| c.name.eq_ignore_ascii_case(&fk.from_column))
                                .unwrap_or(0);
                            let parent_col_idx = parent_tbl
                                .columns
                                .iter()
                                .position(|c| c.name.eq_ignore_ascii_case(&fk.ref_column))
                                .unwrap_or(0);

                            let child_y = child_pos.y
                                + header_height
                                + (child_col_idx as f32 + 0.5) * row_height
                                + 4.0;
                            let parent_y = parent_pos.y
                                + header_height
                                + (parent_col_idx as f32 + 0.5) * row_height
                                + 4.0;

                            let (start_pt, end_pt) = if child_pos.x + card_width < parent_pos.x {
                                (
                                    Pos2::new(child_pos.x + card_width, child_y),
                                    Pos2::new(parent_pos.x, parent_y),
                                )
                            } else if parent_pos.x + card_width < child_pos.x {
                                (
                                    Pos2::new(child_pos.x, child_y),
                                    Pos2::new(parent_pos.x + card_width, parent_y),
                                )
                            } else {
                                (
                                    Pos2::new(child_pos.x + card_width, child_y),
                                    Pos2::new(parent_pos.x + card_width, parent_y),
                                )
                            };

                            let mid_x = (start_pt.x + end_pt.x) * 0.5;
                            let p1 = Pos2::new(mid_x, start_pt.y);
                            let p2 = Pos2::new(mid_x, end_pt.y);

                            let line_color = match fk.on_delete {
                                ForeignKeyAction::Cascade => COLOR_HEALTHY,
                                ForeignKeyAction::Restrict => COLOR_WARNING,
                                _ => COLOR_SYNCING,
                            };

                            let stroke = Stroke::new(2.0_f32, line_color);
                            painter.line_segment([start_pt, p1], stroke);
                            painter.line_segment([p1, p2], stroke);
                            painter.line_segment([p2, end_pt], stroke);

                            painter.circle_filled(start_pt, 4.0, line_color);
                            painter.circle_filled(end_pt, 4.0, COLOR_WARNING);

                            let badge_label =
                                format!("{} ({})", fk.cardinality.short_badge(), fk.on_delete);
                            let badge_width = (badge_label.len() as f32 * 6.4 + 14.0).max(96.0);
                            let mid_pt = Pos2::new(mid_x, (start_pt.y + end_pt.y) * 0.5);
                            let badge_rect =
                                Rect::from_center_size(mid_pt, Vec2::new(badge_width, 20.0));
                            painter.rect_filled(badge_rect, Rounding::same(4.0), COLOR_BG_SURFACE);
                            painter.rect_stroke(
                                badge_rect,
                                Rounding::same(4.0),
                                Stroke::new(1.2_f32, line_color),
                            );
                            painter.text(
                                mid_pt,
                                Align2::CENTER_CENTER,
                                badge_label,
                                FontId::monospace(10.0),
                                COLOR_TEXT_PRIMARY,
                            );
                        }
                    }
                }

                // Render Garis Aktif Saat User Sedang Menarik Garis "● Hubungkan"
                if let Some(src_idx) = self.active_wire_drag {
                    if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        if let Some(src_tbl) = self.blueprint.tables.get(src_idx) {
                            let src_pt = Pos2::new(
                                origin.x + src_tbl.x + card_width,
                                origin.y + src_tbl.y + header_height * 0.5,
                            );
                            let mid_x = (src_pt.x + mouse_pos.x) * 0.5;
                            let p1 = Pos2::new(mid_x, src_pt.y);
                            let p2 = Pos2::new(mid_x, mouse_pos.y);
                            painter.line_segment([src_pt, p1], Stroke::new(2.5_f32, COLOR_SYNCING));
                            painter.line_segment([p1, p2], Stroke::new(2.5_f32, COLOR_SYNCING));
                            painter.line_segment([p2, mouse_pos], Stroke::new(2.5_f32, COLOR_SYNCING));
                            painter.circle_filled(src_pt, 5.0, COLOR_SYNCING);
                            painter.circle_filled(mouse_pos, 5.0, COLOR_HEALTHY);
                            painter.text(
                                Pos2::new(mouse_pos.x + 10.0, mouse_pos.y - 12.0),
                                Align2::LEFT_BOTTOM,
                                "Lepaskan di kartu tabel tujuan untuk menghubungkan!",
                                FontId::proportional(11.5),
                                COLOR_HEALTHY,
                            );
                        }
                    }

                    if ui.input(|i| i.pointer.any_released()) {
                        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                            for (dst_idx, dst_tbl) in self.blueprint.tables.iter().enumerate() {
                                if dst_idx != src_idx {
                                    let dst_card_height = header_height
                                        + (dst_tbl.columns.len().max(1) as f32 * row_height)
                                        + 8.0;
                                    let dst_rect = Rect::from_min_size(
                                        Pos2::new(origin.x + dst_tbl.x, origin.y + dst_tbl.y),
                                        Vec2::new(card_width, dst_card_height),
                                    );
                                    if dst_rect.contains(mouse_pos) {
                                        match self
                                            .blueprint
                                            .connect_tables_automatically(src_idx, dst_idx)
                                        {
                                            Ok(msg) => *toast = Some(msg),
                                            Err(e) => {
                                                *toast = Some(format!("Gagal menghubungkan: {}", e))
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        self.active_wire_drag = None;
                    }
                }

                // Gambar Sticky Notes Interaktif
                for (n_idx, note) in self.blueprint.notes.iter_mut().enumerate() {
                    let note_rect = Rect::from_min_size(
                        Pos2::new(origin.x + note.x, origin.y + note.y),
                        Vec2::new(285.0, 95.0),
                    );
                    let id = ui.make_persistent_id(format!("er_sticky_note_{}", n_idx));
                    let resp = ui.interact(note_rect, id, Sense::drag());
                    if resp.dragged() {
                        note.x += resp.drag_delta().x;
                        note.y += resp.drag_delta().y;
                    }

                    painter.rect_filled(
                        note_rect,
                        Rounding::same(6.0),
                        Color32::from_rgba_unmultiplied(69, 26, 3, 225),
                    );
                    painter.rect_stroke(
                        note_rect,
                        Rounding::same(6.0),
                        Stroke::new(1.2_f32, COLOR_WARNING),
                    );
                    painter.text(
                        Pos2::new(note_rect.left() + 8.0, note_rect.top() + 8.0),
                        Align2::LEFT_TOP,
                        &note.title,
                        FontId::proportional(12.0),
                        COLOR_WARNING,
                    );
                    painter.text(
                        Pos2::new(note_rect.left() + 8.0, note_rect.top() + 28.0),
                        Align2::LEFT_TOP,
                        &note.content,
                        FontId::proportional(11.0),
                        COLOR_TEXT_PRIMARY,
                    );
                }
            });

            // ================================================================
            // KOLOM KANAN: QUICK INSPECTOR PENGATURAN TABEL (RAMAH AWAM)
            // ================================================================
            if self.show_inspector_panel {
                ui.group(|ui| {
                    ui.set_width(265.0);
                    ui.set_height(total_height);
                    ScrollArea::vertical()
                        .id_salt("inspector_scroll_area")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.heading(
                                    RichText::new("✏️ PENGATURAN TABEL")
                                        .size(13.0)
                                        .color(COLOR_HEALTHY)
                                        .strong(),
                                );
                            });
                            ui.add_space(4.0);

                            if let Some(tbl) =
                                self.blueprint.tables.get_mut(self.selected_bp_table_idx)
                            {
                                ui.label(
                                    RichText::new("Nama Tabel:")
                                        .size(11.0)
                                        .color(COLOR_TEXT_SECONDARY),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut tbl.name).desired_width(240.0),
                                );

                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Jenis Data:")
                                        .size(11.0)
                                        .color(COLOR_TEXT_SECONDARY),
                                );
                                ComboBox::from_id_salt("inspector_scope_cb")
                                    .selected_text(match tbl.scope {
                                        TableScope::Global => "🌐 Master Pusat (Global)",
                                        TableScope::Shard => "📁 Transaksi Cabang (Shard)",
                                        TableScope::Both => "🔄 Keduanya (Both)",
                                    })
                                    .width(240.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut tbl.scope,
                                            TableScope::Global,
                                            "🌐 Master Pusat (Global)",
                                        );
                                        ui.selectable_value(
                                            &mut tbl.scope,
                                            TableScope::Shard,
                                            "📁 Transaksi Cabang (Shard)",
                                        );
                                        ui.selectable_value(
                                            &mut tbl.scope,
                                            TableScope::Both,
                                            "🔄 Keduanya (Both)",
                                        );
                                    });

                                ui.checkbox(&mut tbl.enable_cdc, "⚡ Otomatis Rekam Mutasi (CDC)");

                                ui.add_space(6.0);
                                ui.separator();
                                ui.label(
                                    RichText::new("📋 Kolom Tabel:")
                                        .size(11.5)
                                        .strong()
                                        .color(COLOR_SYNCING),
                                );

                                let mut del_col = None;
                                for (c_idx, col) in tbl.columns.iter_mut().enumerate() {
                                    ui.group(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut col.name)
                                                    .desired_width(110.0),
                                            );
                                            let mut friendly =
                                                FriendlyFieldType::from_column_blueprint(col);
                                            let prev_friendly = friendly;
                                            ComboBox::from_id_salt(format!("col_friendly_{}", c_idx))
                                                .selected_text(friendly.display_name())
                                                .width(90.0)
                                                .show_ui(ui, |ui| {
                                                    for ft in FriendlyFieldType::all() {
                                                        ui.selectable_value(
                                                            &mut friendly,
                                                            *ft,
                                                            ft.display_name(),
                                                        );
                                                    }
                                                });
                                            if friendly != prev_friendly {
                                                let updated =
                                                    friendly.to_column_blueprint(&col.name);
                                                col.col_type = updated.col_type;
                                                col.default_expr = updated.default_expr;
                                                col.encrypted_aes = updated.encrypted_aes;
                                                col.include_in_hmac = updated.include_in_hmac;
                                            }

                                            if !col.is_primary_key && ui.small_button("✖").clicked()
                                            {
                                                del_col = Some(c_idx);
                                            }
                                        });

                                        ui.horizontal(|ui| {
                                            if col.is_primary_key {
                                                ui.label(
                                                    RichText::new("🔑 Kunci Utama (ID)")
                                                        .size(10.0)
                                                        .color(COLOR_WARNING),
                                                );
                                            } else {
                                                ui.checkbox(&mut col.encrypted_aes, "🔒 Rahasia");
                                                ui.checkbox(&mut col.not_null, "Wajib");
                                            }
                                        });
                                    });
                                }
                                if let Some(i) = del_col {
                                    tbl.columns.remove(i);
                                }

                                ui.add_space(6.0);
                                ui.separator();
                                ui.label(
                                    RichText::new("🔗 Hubungan Antar Tabel (Relasi):")
                                        .size(11.5)
                                        .strong()
                                        .color(COLOR_PURPLE),
                                );
                                if tbl.foreign_keys.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "Belum ada relasi.\n💡 Tarik tombol '● Hubungkan' pada kartu tabel ke tabel lain untuk membuat relasi!",
                                        )
                                        .size(10.5)
                                        .color(COLOR_TEXT_MUTED),
                                    );
                                } else {
                                    let mut del_fk = None;
                                    for (f_idx, fk) in tbl.foreign_keys.iter_mut().enumerate() {
                                        ui.group(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "➔ Ke Tabel: {}",
                                                    fk.ref_table
                                                ))
                                                .strong()
                                                .color(COLOR_WARNING),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "Kolom: {} ➔ {}",
                                                    fk.from_column, fk.ref_column
                                                ))
                                                .size(10.5),
                                            );
                                            ui.horizontal(|ui| {
                                                ComboBox::from_id_salt(format!(
                                                    "fk_ondel_insp_{}",
                                                    f_idx
                                                ))
                                                .selected_text(match fk.on_delete {
                                                    ForeignKeyAction::Cascade => {
                                                        "Ikut Hapus (Cascade)"
                                                    }
                                                    _ => "Tolak / Lindungi (Restrict)",
                                                })
                                                .width(180.0)
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        &mut fk.on_delete,
                                                        ForeignKeyAction::Cascade,
                                                        "Ikut Hapus (Cascade)",
                                                    );
                                                    ui.selectable_value(
                                                        &mut fk.on_delete,
                                                        ForeignKeyAction::Restrict,
                                                        "Tolak / Lindungi (Restrict)",
                                                    );
                                                });
                                                if ui.small_button("✖").clicked() {
                                                    del_fk = Some(f_idx);
                                                }
                                            });
                                        });
                                    }
                                    if let Some(f) = del_fk {
                                        tbl.foreign_keys.remove(f);
                                    }
                                }

                                ui.add_space(8.0);
                                ui.separator();
                                if ui
                                    .button(
                                        RichText::new("✖ Hapus Tabel Ini")
                                            .color(COLOR_DANGER)
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    self.blueprint.tables.remove(self.selected_bp_table_idx);
                                    self.selected_bp_table_idx = 0;
                                    *toast = Some("Tabel berhasil dihapus!".into());
                                }
                            } else {
                                ui.label(
                                    RichText::new(
                                        "Klik salah satu kartu tabel di kanvas untuk melihat dan mengedit pengaturannya di sini.",
                                    )
                                    .color(COLOR_TEXT_MUTED),
                                );
                            }
                        });
                });
            }
        });
    }

    // ========================================================================
    // MODE 2: VISUAL TABLE, COLUMN & FOREIGN KEY DESIGNER
    // ========================================================================
    fn render_table_and_fk_designer(&mut self, ui: &mut Ui, toast: &mut Option<String>) {
        // Bar Pilih & Buat Tabel
        ui.horizontal(|ui| {
            ui.label(RichText::new("Daftar Tabel Blueprint:").strong());
            for (idx, tbl) in self.blueprint.tables.iter().enumerate() {
                let is_sel = self.selected_bp_table_idx == idx;
                let label = format!("📋 {} [{}]", tbl.name, tbl.scope);
                if ui
                    .selectable_label(
                        is_sel,
                        RichText::new(label).color(if is_sel {
                            COLOR_SYNCING
                        } else {
                            COLOR_TEXT_SECONDARY
                        }),
                    )
                    .clicked()
                {
                    self.selected_bp_table_idx = idx;
                }
            }

            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.new_table_name).desired_width(110.0));
            ComboBox::from_id_salt("new_tbl_scope_cb")
                .selected_text(format!("{}", self.new_table_scope))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.new_table_scope, TableScope::Global, "Global");
                    ui.selectable_value(&mut self.new_table_scope, TableScope::Shard, "Shard");
                    ui.selectable_value(&mut self.new_table_scope, TableScope::Both, "Both");
                });

            if ui
                .button(RichText::new("➕ Buat Tabel").color(COLOR_HEALTHY).strong())
                .clicked()
            {
                let clean = self.new_table_name.trim().to_string();
                if !clean.is_empty() {
                    let count = self.blueprint.tables.len();
                    self.blueprint.tables.push(TableBlueprint::new(
                        &clean,
                        self.new_table_scope,
                        60.0 + (count as f32 * 40.0),
                        80.0 + (count as f32 * 30.0),
                    ));
                    self.selected_bp_table_idx = self.blueprint.tables.len() - 1;
                    *toast = Some(format!("Tabel '{}' berhasil dibuat!", clean));
                }
            }
        });

        ui.add_space(6.0);

        if self.blueprint.tables.is_empty() {
            ui.label(
                RichText::new("Belum ada tabel di Blueprint. Buat tabel baru atau klik 'Import dari Live DB'.")
                    .color(COLOR_TEXT_MUTED),
            );
            return;
        }

        if self.selected_bp_table_idx >= self.blueprint.tables.len() {
            self.selected_bp_table_idx = 0;
        }

        // Ambil daftar semua nama tabel dan kolomnya untuk dropdown Foreign Key
        let all_table_columns: Vec<(String, Vec<String>)> = self
            .blueprint
            .tables
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    t.columns.iter().map(|c| c.name.clone()).collect(),
                )
            })
            .collect();

        let mut delete_current_table = false;

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            let tbl = &mut self.blueprint.tables[self.selected_bp_table_idx];

            // 1. Properti Tabel
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚙ Properti Tabel:").strong().color(COLOR_SYNCING));
                    ui.label("Nama:");
                    ui.add(egui::TextEdit::singleline(&mut tbl.name).desired_width(140.0));

                    ui.label("Scope:");
                    ComboBox::from_id_salt("edit_tbl_scope_cb")
                        .selected_text(format!("{}", tbl.scope))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut tbl.scope, TableScope::Global, "Global");
                            ui.selectable_value(&mut tbl.scope, TableScope::Shard, "Shard");
                            ui.selectable_value(&mut tbl.scope, TableScope::Both, "Both");
                        });

                    ui.checkbox(&mut tbl.enable_cdc, "⚡ Enable HLC Auto-CDC Triggers");
                    ui.checkbox(&mut tbl.without_rowid, "WITHOUT ROWID");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new("✖ Hapus Tabel").color(COLOR_DANGER))
                            .clicked()
                        {
                            delete_current_table = true;
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.label("Deskripsi / Komentar:");
                    ui.add(egui::TextEdit::singleline(&mut tbl.comment).desired_width(420.0));
                });
            });

            ui.add_space(8.0);

            // 2. Editor Kolom & Flag Keamanan (AES-256-GCM & HMAC)
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("📋 Kolom, Konstrain & Flag Kriptografi (AES-256 / HMAC)")
                            .strong()
                            .color(COLOR_HEALTHY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new("➕ Tambah Kolom").color(COLOR_HEALTHY))
                            .clicked()
                        {
                            let c_name = if self.new_col_name.trim().is_empty() {
                                format!("col_{}", tbl.columns.len() + 1)
                            } else {
                                self.new_col_name.trim().to_string()
                            };
                            tbl.columns
                                .push(ColumnBlueprint::new(&c_name, self.new_col_type));
                        }
                        ComboBox::from_id_salt("new_col_type_cb")
                            .selected_text(format!("{}", self.new_col_type))
                            .show_ui(ui, |ui| {
                                for aff in SqliteAffinity::all() {
                                    ui.selectable_value(
                                        &mut self.new_col_type,
                                        *aff,
                                        aff.to_sqlite_str(),
                                    );
                                }
                            });
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_col_name).desired_width(120.0),
                        );
                        ui.label("Kolom Baru:");
                    });
                });

                ui.add_space(4.0);

                let mut remove_col_idx = None;
                let mut move_up_idx = None;
                let mut move_down_idx = None;
                let col_len = tbl.columns.len();

                Grid::new("blueprint_columns_editor_grid")
                    .striped(true)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Urutan").color(COLOR_TEXT_MUTED));
                        ui.label(RichText::new("Nama Kolom").strong());
                        ui.label(RichText::new("Tipe Data").strong().color(COLOR_PURPLE));
                        ui.label(RichText::new("🔑 PK").strong().color(COLOR_WARNING));
                        ui.label(RichText::new("AutoInc").strong());
                        ui.label(RichText::new("NOT NULL").strong());
                        ui.label(RichText::new("UNIQUE").strong());
                        ui.label(RichText::new("🔒 AES-256").strong().color(COLOR_HEALTHY));
                        ui.label(RichText::new("🛡 HMAC").strong().color(COLOR_SYNCING));
                        ui.label(RichText::new("DEFAULT").strong());
                        ui.label(RichText::new("CHECK (...)").strong());
                        ui.label(RichText::new("Aksi").color(COLOR_DANGER));
                        ui.end_row();

                        for (c_idx, col) in tbl.columns.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.small_button("⬆").clicked() && c_idx > 0 {
                                    move_up_idx = Some(c_idx);
                                }
                                if ui.small_button("⬇").clicked() && c_idx + 1 < col_len {
                                    move_down_idx = Some(c_idx);
                                }
                            });

                            ui.add(egui::TextEdit::singleline(&mut col.name).desired_width(120.0));

                            ComboBox::from_id_salt(format!("col_type_cb_{}", c_idx))
                                .selected_text(col.col_type.to_sqlite_str())
                                .width(95.0)
                                .show_ui(ui, |ui| {
                                    for aff in SqliteAffinity::all() {
                                        ui.selectable_value(
                                            &mut col.col_type,
                                            *aff,
                                            aff.to_sqlite_str(),
                                        );
                                    }
                                });

                            if ui.checkbox(&mut col.is_primary_key, "").changed()
                                && col.is_primary_key
                            {
                                col.not_null = true;
                            }
                            ui.checkbox(&mut col.auto_increment, "");
                            ui.checkbox(&mut col.not_null, "");
                            ui.checkbox(&mut col.unique, "");
                            ui.checkbox(&mut col.encrypted_aes, "");
                            ui.checkbox(&mut col.include_in_hmac, "");

                            let mut def_str = col.default_expr.clone().unwrap_or_default();
                            if ui
                                .add(egui::TextEdit::singleline(&mut def_str).desired_width(80.0))
                                .changed()
                            {
                                col.default_expr = if def_str.trim().is_empty() {
                                    None
                                } else {
                                    Some(def_str)
                                };
                            }

                            let mut chk_str = col.check_expr.clone().unwrap_or_default();
                            if ui
                                .add(egui::TextEdit::singleline(&mut chk_str).desired_width(90.0))
                                .changed()
                            {
                                col.check_expr = if chk_str.trim().is_empty() {
                                    None
                                } else {
                                    Some(chk_str)
                                };
                            }

                            if ui.small_button("✖").clicked() {
                                remove_col_idx = Some(c_idx);
                            }
                            ui.end_row();
                        }
                    });

                if let Some(i) = move_up_idx {
                    tbl.columns.swap(i, i - 1);
                }
                if let Some(i) = move_down_idx {
                    tbl.columns.swap(i, i + 1);
                }
                if let Some(i) = remove_col_idx {
                    tbl.columns.remove(i);
                }
            });

            ui.add_space(10.0);

            // 3. FIRST-CLASS FOREIGN KEY (RELATIONSHIP) BUILDER ALA DRAWDB
            ui.group(|ui| {
                ui.label(
                    RichText::new("🔗 Foreign Keys & Relationships Builder (DrawDB Cardinality + Referential Actions)")
                        .strong()
                        .color(COLOR_SYNCING),
                );
                ui.add_space(4.0);

                // Inisialisasi default pilihan dropdown jika kosong
                if self.new_fk_from_col.is_empty() {
                    if let Some(first_c) = tbl.columns.first() {
                        self.new_fk_from_col = first_c.name.clone();
                    }
                }
                if self.new_fk_ref_table.is_empty() {
                    if let Some((first_t, _)) = all_table_columns.first() {
                        self.new_fk_ref_table = first_t.clone();
                    }
                }

                // Bar Tambah Foreign Key Baru
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Kolom Lokal (Child):").color(COLOR_TEXT_SECONDARY));
                    ComboBox::from_id_salt("new_fk_from_col_cb")
                        .selected_text(&self.new_fk_from_col)
                        .show_ui(ui, |ui| {
                            for c in &tbl.columns {
                                ui.selectable_value(
                                    &mut self.new_fk_from_col,
                                    c.name.clone(),
                                    &c.name,
                                );
                            }
                        });

                    ui.label(RichText::new("➡ Tabel Induk (Parent):").color(COLOR_TEXT_SECONDARY));
                    ComboBox::from_id_salt("new_fk_ref_tbl_cb")
                        .selected_text(&self.new_fk_ref_table)
                        .show_ui(ui, |ui| {
                            for (t_name, t_cols) in &all_table_columns {
                                if ui
                                    .selectable_value(
                                        &mut self.new_fk_ref_table,
                                        t_name.clone(),
                                        t_name,
                                    )
                                    .clicked()
                                {
                                    if let Some(first_pc) = t_cols.first() {
                                        self.new_fk_ref_col = first_pc.clone();
                                    }
                                }
                            }
                        });

                    ui.label(RichText::new("Kolom Induk:").color(COLOR_TEXT_SECONDARY));
                    let parent_cols: Vec<String> = all_table_columns
                        .iter()
                        .find(|(tn, _)| tn == &self.new_fk_ref_table)
                        .map(|(_, cols)| cols.clone())
                        .unwrap_or_default();

                    ComboBox::from_id_salt("new_fk_ref_col_cb")
                        .selected_text(&self.new_fk_ref_col)
                        .show_ui(ui, |ui| {
                            for pc in &parent_cols {
                                ui.selectable_value(&mut self.new_fk_ref_col, pc.clone(), pc);
                            }
                        });

                    ComboBox::from_id_salt("new_fk_card_cb")
                        .selected_text(self.new_fk_cardinality.short_badge())
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for card in Cardinality::all() {
                                ui.selectable_value(
                                    &mut self.new_fk_cardinality,
                                    *card,
                                    format!("{}", card),
                                );
                            }
                        });

                    ui.label("ON DELETE:");
                    ComboBox::from_id_salt("new_fk_del_cb")
                        .selected_text(self.new_fk_on_delete.to_sql_str())
                        .width(95.0)
                        .show_ui(ui, |ui| {
                            for act in ForeignKeyAction::all() {
                                ui.selectable_value(
                                    &mut self.new_fk_on_delete,
                                    *act,
                                    act.to_sql_str(),
                                );
                            }
                        });

                    ui.checkbox(&mut self.new_fk_auto_index, "Auto-Index");

                    if ui
                        .button(
                            RichText::new("➕ Tambah Foreign Key")
                                .color(COLOR_HEALTHY)
                                .strong(),
                        )
                        .clicked()
                        && !self.new_fk_from_col.is_empty()
                        && !self.new_fk_ref_table.is_empty()
                        && !self.new_fk_ref_col.is_empty()
                    {
                        let mut fk = ForeignKeyBlueprint::new(
                            &tbl.name,
                            &self.new_fk_from_col,
                            &self.new_fk_ref_table,
                            &self.new_fk_ref_col,
                            self.new_fk_on_delete,
                        );
                        fk.cardinality = self.new_fk_cardinality;
                        fk.auto_index_fk = self.new_fk_auto_index;
                        tbl.foreign_keys.push(fk);
                        *toast = Some(format!(
                            "Foreign Key {}.{} -> {}.{} berhasil ditambahkan!",
                            tbl.name,
                            self.new_fk_from_col,
                            self.new_fk_ref_table,
                            self.new_fk_ref_col
                        ));
                    }
                });

                ui.add_space(6.0);

                if tbl.foreign_keys.is_empty() {
                    ui.label(
                        RichText::new("Belum ada Foreign Key pada tabel ini. Gunakan form di atas untuk menambahkan relasi.")
                            .color(COLOR_TEXT_MUTED),
                    );
                } else {
                    let mut remove_fk_idx = None;
                    let local_col_names: Vec<String> =
                        tbl.columns.iter().map(|c| c.name.clone()).collect();

                    Grid::new("blueprint_fk_editor_grid")
                        .striped(true)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Nama Constraint").strong());
                            ui.label(RichText::new("Kolom Lokal").strong().color(COLOR_SYNCING));
                            ui.label(RichText::new("Tabel Induk").strong().color(COLOR_WARNING));
                            ui.label(RichText::new("Kolom Induk").strong().color(COLOR_WARNING));
                            ui.label(RichText::new("Cardinality").strong());
                            ui.label(RichText::new("ON DELETE").strong());
                            ui.label(RichText::new("ON UPDATE").strong());
                            ui.label(RichText::new("Auto-Index").strong());
                            ui.label(RichText::new("Hapus").color(COLOR_DANGER));
                            ui.end_row();

                            for (fk_idx, fk) in tbl.foreign_keys.iter_mut().enumerate() {
                                ui.add(
                                    egui::TextEdit::singleline(&mut fk.name).desired_width(140.0),
                                );

                                ComboBox::from_id_salt(format!("fk_from_cb_{}", fk_idx))
                                    .selected_text(&fk.from_column)
                                    .show_ui(ui, |ui| {
                                        for lc in &local_col_names {
                                            ui.selectable_value(
                                                &mut fk.from_column,
                                                lc.clone(),
                                                lc,
                                            );
                                        }
                                    });

                                ComboBox::from_id_salt(format!("fk_reftbl_cb_{}", fk_idx))
                                    .selected_text(&fk.ref_table)
                                    .show_ui(ui, |ui| {
                                        for (tn, _) in &all_table_columns {
                                            ui.selectable_value(&mut fk.ref_table, tn.clone(), tn);
                                        }
                                    });

                                let ref_cols: Vec<String> = all_table_columns
                                    .iter()
                                    .find(|(tn, _)| tn == &fk.ref_table)
                                    .map(|(_, cols)| cols.clone())
                                    .unwrap_or_default();

                                ComboBox::from_id_salt(format!("fk_refcol_cb_{}", fk_idx))
                                    .selected_text(&fk.ref_column)
                                    .show_ui(ui, |ui| {
                                        for rc in &ref_cols {
                                            ui.selectable_value(
                                                &mut fk.ref_column,
                                                rc.clone(),
                                                rc,
                                            );
                                        }
                                    });

                                ComboBox::from_id_salt(format!("fk_card_cb_{}", fk_idx))
                                    .selected_text(fk.cardinality.short_badge())
                                    .show_ui(ui, |ui| {
                                        for card in Cardinality::all() {
                                            ui.selectable_value(
                                                &mut fk.cardinality,
                                                *card,
                                                format!("{}", card),
                                            );
                                        }
                                    });

                                ComboBox::from_id_salt(format!("fk_ondel_cb_{}", fk_idx))
                                    .selected_text(fk.on_delete.to_sql_str())
                                    .show_ui(ui, |ui| {
                                        for act in ForeignKeyAction::all() {
                                            ui.selectable_value(
                                                &mut fk.on_delete,
                                                *act,
                                                act.to_sql_str(),
                                            );
                                        }
                                    });

                                ComboBox::from_id_salt(format!("fk_onupd_cb_{}", fk_idx))
                                    .selected_text(fk.on_update.to_sql_str())
                                    .show_ui(ui, |ui| {
                                        for act in ForeignKeyAction::all() {
                                            ui.selectable_value(
                                                &mut fk.on_update,
                                                *act,
                                                act.to_sql_str(),
                                            );
                                        }
                                    });

                                ui.checkbox(&mut fk.auto_index_fk, "");

                                if ui.small_button("✖").clicked() {
                                    remove_fk_idx = Some(fk_idx);
                                }
                                ui.end_row();
                            }
                        });

                    if let Some(i) = remove_fk_idx {
                        tbl.foreign_keys.remove(i);
                    }
                }
            });

            ui.add_space(10.0);

            // 4. Editor Indeks Tambahan (Composite / Unique Indexes)
            ui.group(|ui| {
                ui.label(
                    RichText::new("⚡ Composite & Custom Indexes")
                        .strong()
                        .color(COLOR_PURPLE),
                );
                ui.horizontal(|ui| {
                    ui.label("Nama Indeks:");
                    ui.add(egui::TextEdit::singleline(&mut self.new_idx_name).desired_width(140.0));
                    ui.label("Kolom (pisahkan koma):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_idx_cols_csv).desired_width(160.0),
                    );
                    ui.checkbox(&mut self.new_idx_unique, "UNIQUE");
                    if ui.button("➕ Tambah Indeks").clicked() {
                        let cols: Vec<String> = self
                            .new_idx_cols_csv
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !cols.is_empty() {
                            let idx_name = if self.new_idx_name.trim().is_empty() {
                                format!("idx_{}_{}", tbl.name, cols.join("_"))
                            } else {
                                self.new_idx_name.trim().to_string()
                            };
                            tbl.indexes.push(IndexBlueprint {
                                name: idx_name,
                                columns: cols,
                                unique: self.new_idx_unique,
                            });
                            self.new_idx_name.clear();
                            self.new_idx_cols_csv.clear();
                        }
                    }
                });

                let mut remove_idx = None;
                for (i_idx, idx_bp) in tbl.indexes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let u_badge = if idx_bp.unique { "[UNIQUE] " } else { "" };
                        ui.label(
                            RichText::new(format!(
                                "• {}{} ({})",
                                u_badge,
                                idx_bp.name,
                                idx_bp.columns.join(", ")
                            ))
                            .monospace(),
                        );
                        if ui.small_button("✖").clicked() {
                            remove_idx = Some(i_idx);
                        }
                    });
                }
                if let Some(i) = remove_idx {
                    tbl.indexes.remove(i);
                }
            });
        });

        if delete_current_table && !self.blueprint.tables.is_empty() {
            let removed = self.blueprint.tables.remove(self.selected_bp_table_idx);
            self.selected_bp_table_idx = 0;
            *toast = Some(format!("Tabel '{}' dihapus dari Blueprint.", removed.name));
        }
    }

    // ========================================================================
    // MODE 3: DRAWDB ISSUES & SCHEMA LINTER
    // ========================================================================
    fn render_issues_linter(&mut self, ui: &mut Ui) {
        let issues = self.blueprint.validate_issues();
        ui.label(
            RichText::new("🛡 DrawDB Schema Issue Detector & Relational Integrity Validator")
                .size(14.0)
                .strong()
                .color(COLOR_SYNCING),
        );
        ui.label(
            RichText::new(
                "Memeriksa konsistensi Primary Key, kecocokan tipe data Foreign Key, cakupan Global vs Shard, dan kelengkapan indeks.",
            )
            .size(11.5)
            .color(COLOR_TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        if issues.is_empty() {
            ui.group(|ui| {
                ui.label(
                    RichText::new("✅ Skema 100% Valid! Tidak ditemukan masalah pada Tabel, Kolom, maupun Foreign Key.")
                        .color(COLOR_HEALTHY)
                        .strong(),
                );
            });
            return;
        }

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for issue in issues {
                let (badge, color) = match issue.severity {
                    IssueSeverity::Error => ("❌ ERROR", COLOR_DANGER),
                    IssueSeverity::Warning => ("⚠ WARNING", COLOR_WARNING),
                    IssueSeverity::Info => ("ℹ INFO", COLOR_SYNCING),
                };
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(badge).color(color).strong());
                        ui.label(
                            RichText::new(format!("[{}]", issue.target))
                                .monospace()
                                .color(COLOR_TEXT_PRIMARY)
                                .strong(),
                        );
                        ui.label(RichText::new(&issue.message).color(COLOR_TEXT_SECONDARY));
                    });
                });
                ui.add_space(4.0);
            }
        });
    }

    // ========================================================================
    // MODE 4: SMART DIFF & 12-STEP REBUILD CLUSTER DEPLOYER
    // ========================================================================
    fn render_smart_diff_deploy(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        toast: &mut Option<String>,
    ) {
        let conn_arc = match self.open_target_conn(engine, selected_target, toast) {
            Some(c) => c,
            None => return,
        };

        let manifest = {
            let conn = conn_arc.lock().unwrap();
            match introspect_schema(&conn) {
                Ok(m) => m,
                Err(e) => {
                    ui.label(
                        RichText::new(format!("Gagal mengintrospeksi database aktif: {}", e))
                            .color(COLOR_DANGER),
                    );
                    return;
                }
            }
        };

        let scope_filter = match selected_target {
            SelectedTarget::Global(_) => Some(TableScope::Global),
            SelectedTarget::Shard { .. } => Some(TableScope::Shard),
        };

        let diff_plan = diff_blueprint_against_manifest(
            &self.blueprint,
            &manifest,
            scope_filter,
            self.allow_destructive_drops,
        );

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "🔄 Smart Diff: Blueprint vs Live DB ({})",
                    selected_target.display_title()
                ))
                .size(14.0)
                .strong()
                .color(COLOR_SYNCING),
            );
            ui.checkbox(
                &mut self.allow_destructive_drops,
                "Izinkan Destructive Drops (DROP TABLE / DROP COLUMN)",
            );
        });

        ui.add_space(6.0);

        if diff_plan.requires_rebuild {
            ui.group(|ui| {
                ui.label(
                    RichText::new("🛡 SQLite 12-Step Safe Table Rebuild Aktif!")
                        .color(COLOR_WARNING)
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "Terdeteksi penambahan/perubahan Foreign Key atau konstrain kolom pada tabel yang sudah ada. \
                         ShardVault otomatis membangkitkan prosedur 12-Step Table Rebuild transaksional agar seluruh data lama tetap aman!",
                    )
                    .size(11.5)
                    .color(COLOR_TEXT_SECONDARY),
                );
            });
            ui.add_space(6.0);
        }

        // Bar Eksekusi Migrasi Langsung ke Cluster
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🚀 Deploy ke Cluster:").strong().color(COLOR_HEALTHY));
                ui.label("Versi:");
                ui.add(egui::DragValue::new(&mut self.deploy_migration_version).speed(1.0));
                ui.label("Nama Migrasi:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.deploy_migration_name)
                        .desired_width(160.0),
                );

                ui.label("Target:");
                ComboBox::from_id_salt("deploy_mig_target_cb")
                    .selected_text(format!("{:?}", self.deploy_target))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.deploy_target,
                            MigrationTarget::GlobalOnly,
                            "GlobalOnly",
                        );
                        ui.selectable_value(
                            &mut self.deploy_target,
                            MigrationTarget::ShardsOnly,
                            "ShardsOnly",
                        );
                        ui.selectable_value(&mut self.deploy_target, MigrationTarget::All, "All");
                    });

                if ui
                    .button(
                        RichText::new("⚡ Jalankan Migrasi Sekarang")
                            .color(COLOR_HEALTHY)
                            .strong(),
                    )
                    .clicked()
                {
                    if diff_plan.is_up_to_date() {
                        *toast = Some("Skema sudah sinkron, tidak ada migrasi yang perlu dijalankan.".into());
                    } else {
                        let leaked_name: &'static str =
                            Box::leak(self.deploy_migration_name.trim().to_string().into_boxed_str());
                        let leaked_sql: &'static str =
                            Box::leak(diff_plan.migration_sql.clone().into_boxed_str());

                        let mut runner = MigrationRunner::new();
                        runner.add(Migration {
                            version: self.deploy_migration_version,
                            name: leaked_name,
                            target: self.deploy_target,
                            up_sql: leaked_sql,
                            down_sql: None,
                        });
                        match engine.run_migrations(&runner) {
                            Ok(reports) => {
                                self.deploy_migration_version += 1;
                                *toast = Some(format!(
                                    "Migrasi sukses diterapkan ke {} target database!",
                                    reports.len()
                                ));
                            }
                            Err(e) => {
                                *toast = Some(format!("Migrasi gagal: {}", e));
                            }
                        }
                    }
                }
            });
        });

        ui.add_space(6.0);

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.group(|ui| {
                ui.label(
                    RichText::new(format!("📋 Ringkasan Perubahan ({})", diff_plan.items.len()))
                        .strong()
                        .color(COLOR_SYNCING),
                );
                if diff_plan.items.is_empty() {
                    ui.label(
                        RichText::new("✅ Tidak ada perbedaan antara Blueprint dan database aktif.")
                            .color(COLOR_HEALTHY),
                    );
                } else {
                    for item in &diff_plan.items {
                        let (badge, color) = match item.kind {
                            DiffActionKind::CreateTable => ("[+ CREATE TABLE]", COLOR_HEALTHY),
                            DiffActionKind::AlterAddColumn => ("[+ ADD COLUMN]", COLOR_SYNCING),
                            DiffActionKind::RebuildTable12Step => {
                                ("[🔄 12-STEP REBUILD]", COLOR_WARNING)
                            }
                            DiffActionKind::DropColumn => ("[- DROP COLUMN]", COLOR_DANGER),
                            DiffActionKind::DropTable => ("[- DROP TABLE]", COLOR_DANGER),
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(badge).color(color).monospace().strong());
                            ui.label(RichText::new(&item.description).color(COLOR_TEXT_PRIMARY));
                        });
                    }
                }
            });

            ui.add_space(8.0);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("📜 Generated Migration SQL")
                            .strong()
                            .color(COLOR_PURPLE),
                    );
                    if ui.button("📋 Salin SQL Migrasi").clicked() {
                        ui.output_mut(|o| o.copied_text = diff_plan.migration_sql.clone());
                        *toast = Some("SQL Migrasi disalin ke clipboard!".into());
                    }
                });
                let mut sql_preview = diff_plan.migration_sql.as_str();
                ui.add(
                    egui::TextEdit::multiline(&mut sql_preview)
                        .font(FontId::monospace(12.0))
                        .desired_rows(16)
                        .desired_width(f32::INFINITY),
                );
            });
        });
    }

    // ========================================================================
    // MODE 5: MULTI-TARGET CODE & SQL GENERATOR
    // ========================================================================
    fn render_code_generator(&mut self, ui: &mut Ui, toast: &mut Option<String>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Target Generator:").strong());
            let formats = [
                (
                    CodegenOutputFormat::RustSchemaAndRepo,
                    "⚙ Rust (SchemaProvider + AES/HMAC Repo)",
                    "./generated_schema.rs",
                ),
                (
                    CodegenOutputFormat::SqliteDdl,
                    "📦 SQLite 3 DDL (.sql)",
                    "./schema_sqlite.sql",
                ),
                (
                    CodegenOutputFormat::PostgresDdl,
                    "📄 PostgreSQL DDL (.sql)",
                    "./schema_postgres.sql",
                ),
                (
                    CodegenOutputFormat::MysqlDdl,
                    "📄 MySQL / MariaDB DDL (.sql)",
                    "./schema_mysql.sql",
                ),
                (
                    CodegenOutputFormat::TypeScriptInterfaces,
                    "💻 TypeScript Interfaces (.ts)",
                    "./schema_types.ts",
                ),
                (
                    CodegenOutputFormat::BlueprintJson,
                    "📐 DrawDB Blueprint (.ddb.json)",
                    "./project_blueprint.ddb.json",
                ),
            ];

            for (fmt, label, default_path) in formats {
                let is_sel = self.codegen_format == fmt;
                if ui
                    .selectable_label(
                        is_sel,
                        RichText::new(label).color(if is_sel {
                            COLOR_HEALTHY
                        } else {
                            COLOR_TEXT_SECONDARY
                        }),
                    )
                    .clicked()
                {
                    self.codegen_format = fmt;
                    self.export_file_path = default_path.to_string();
                }
            }
        });

        ui.add_space(6.0);

        let generated_code = match self.codegen_format {
            CodegenOutputFormat::SqliteDdl => {
                compile_blueprint_ddl(&self.blueprint, SqlDialect::Sqlite, None)
            }
            CodegenOutputFormat::PostgresDdl => {
                compile_blueprint_ddl(&self.blueprint, SqlDialect::Postgres, None)
            }
            CodegenOutputFormat::MysqlDdl => {
                compile_blueprint_ddl(&self.blueprint, SqlDialect::Mysql, None)
            }
            CodegenOutputFormat::RustSchemaAndRepo => generate_rust_code(&self.blueprint),
            CodegenOutputFormat::TypeScriptInterfaces => generate_typescript_code(&self.blueprint),
            CodegenOutputFormat::BlueprintJson => export_blueprint_json(&self.blueprint),
        };

        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("📋 Salin ke Clipboard").color(COLOR_SYNCING).strong())
                .clicked()
            {
                ui.output_mut(|o| o.copied_text = generated_code.clone());
                *toast = Some("Kode hasil generator berhasil disalin ke clipboard!".into());
            }

            ui.separator();
            ui.label("Path File:");
            ui.add(egui::TextEdit::singleline(&mut self.export_file_path).desired_width(240.0));

            if ui
                .button(RichText::new("💾 Simpan ke Disk").color(COLOR_HEALTHY).strong())
                .clicked()
            {
                match std::fs::write(&self.export_file_path, &generated_code) {
                    Ok(_) => {
                        *toast = Some(format!(
                            "Berhasil mengekspor kode ke '{}'",
                            self.export_file_path
                        ));
                    }
                    Err(e) => {
                        *toast = Some(format!("Gagal menyimpan file: {}", e));
                    }
                }
            }

            if self.codegen_format == CodegenOutputFormat::BlueprintJson
                && ui.button("📂 Muat Blueprint JSON dari Path").clicked()
            {
                match std::fs::read_to_string(&self.export_file_path) {
                    Ok(raw) => match serde_json::from_str::<SchemaBlueprint>(&raw) {
                        Ok(bp) => {
                            self.blueprint = bp;
                            self.selected_bp_table_idx = 0;
                            *toast = Some("Blueprint JSON berhasil dimuat ke Editor!".into());
                        }
                        Err(e) => *toast = Some(format!("Format JSON tidak valid: {}", e)),
                    },
                    Err(e) => *toast = Some(format!("Gagal membaca file: {}", e)),
                }
            }

            if self.codegen_format == CodegenOutputFormat::SqliteDdl
                && ui.button("📂 Import SQL DDL dari Path").clicked()
            {
                match std::fs::read_to_string(&self.export_file_path) {
                    Ok(raw_sql) => {
                        match SchemaBlueprint::from_sql_ddl_script(&raw_sql, TableScope::Shard) {
                            Ok(bp) => {
                                self.blueprint = bp;
                                self.selected_bp_table_idx = 0;
                                *toast = Some(format!(
                                    "Berhasil mengimpor {} tabel dari skrip SQL DDL!",
                                    self.blueprint.tables.len()
                                ));
                            }
                            Err(e) => *toast = Some(format!("Gagal mem-parsing SQL DDL: {}", e)),
                        }
                    }
                    Err(e) => *toast = Some(format!("Gagal membaca file SQL: {}", e)),
                }
            }
        });

        ui.add_space(6.0);

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            let mut preview = generated_code.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut preview)
                    .font(FontId::monospace(12.0))
                    .desired_rows(26)
                    .desired_width(f32::INFINITY),
            );
        });
    }

    // ========================================================================
    // MODE 6: LIVE DATABASE DDL INSPECTOR (READ-ONLY)
    // ========================================================================
    fn render_live_db_inspector(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        selected_table: &Option<String>,
        toast: &mut Option<String>,
    ) {
        let conn_arc = match self.open_target_conn(engine, selected_target, toast) {
            Some(c) => c,
            None => return,
        };

        let conn = conn_arc.lock().unwrap();
        let tables = fetch_tables(&conn);

        if tables.is_empty() {
            ui.label(
                RichText::new("Tidak ada tabel ditemukan pada database ini.")
                    .color(COLOR_TEXT_MUTED),
            );
            return;
        }

        if self.selected_table_inspect.is_none() {
            if let Some(ref st) = selected_table {
                self.selected_table_inspect = Some(st.clone());
            } else if let Some(first) = tables.first() {
                self.selected_table_inspect = Some(first.name.clone());
            }
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Pilih Tabel Fisik:").strong().color(COLOR_TEXT_SECONDARY));
            for t in &tables {
                let is_sel = self.selected_table_inspect.as_deref() == Some(&t.name);
                let text = if is_sel {
                    RichText::new(&t.name).color(COLOR_SYNCING).strong()
                } else {
                    RichText::new(&t.name).color(COLOR_TEXT_MUTED)
                };
                if ui.selectable_label(is_sel, text).clicked() {
                    self.selected_table_inspect = Some(t.name.clone());
                }
            }
        });

        ui.add_space(8.0);

        let inspect_tbl = match self.selected_table_inspect {
            Some(ref t) => t.as_str(),
            None => return,
        };

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📄 Skema DDL Fisik: {}", inspect_tbl))
                            .strong()
                            .color(COLOR_HEALTHY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ddl = fetch_table_ddl(&conn, inspect_tbl);
                        if ui.button("📋 Salin DDL").clicked() {
                            ui.output_mut(|o| o.copied_text = ddl);
                            *toast = Some("DDL disalin ke clipboard!".into());
                        }
                    });
                });
                ui.add_space(4.0);
                let ddl = fetch_table_ddl(&conn, inspect_tbl);
                ui.add(
                    egui::TextEdit::multiline(&mut ddl.as_str())
                        .font(egui::FontId::monospace(12.0))
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
            });

            ui.add_space(8.0);

            ui.group(|ui| {
                ui.label(
                    RichText::new("📋 Definisi Kolom & Tipe Data")
                        .strong()
                        .color(COLOR_SYNCING),
                );
                ui.add_space(4.0);

                let columns = fetch_columns(&conn, inspect_tbl);
                Grid::new("columns_inspect_grid")
                    .striped(true)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("CID").strong().color(COLOR_TEXT_MUTED));
                        ui.label(RichText::new("Nama Kolom").strong().color(COLOR_TEXT_PRIMARY));
                        ui.label(RichText::new("Tipe Data").strong().color(COLOR_SYNCING));
                        ui.label(RichText::new("NOT NULL?").strong().color(COLOR_TEXT_MUTED));
                        ui.label(RichText::new("Primary Key?").strong().color(COLOR_WARNING));
                        ui.end_row();

                        for c in columns {
                            ui.label(format!("{}", c.cid));
                            ui.label(RichText::new(&c.name).strong());
                            ui.label(RichText::new(&c.data_type).color(COLOR_PURPLE));
                            ui.label(if c.not_null { "Ya" } else { "Tidak" });
                            ui.label(if c.is_pk { "⭐ PRIMARY KEY" } else { "-" });
                            ui.end_row();
                        }
                    });
            });

            ui.add_space(8.0);

            let indexes = fetch_table_indexes(&conn, inspect_tbl);
            if !indexes.is_empty() {
                ui.group(|ui| {
                    ui.label(
                        RichText::new(format!("⚡ Indeks Terpasang ({})", indexes.len()))
                            .strong()
                            .color(COLOR_PURPLE),
                    );
                    ui.add_space(4.0);
                    for idx_sql in indexes {
                        ui.label(
                            RichText::new(format!("• {}", idx_sql))
                                .monospace()
                                .size(11.0)
                                .color(COLOR_TEXT_SECONDARY),
                        );
                    }
                });
            }
        });
    }
}
