// E:\shardvault-rs\src\studio\app.rs
// Master StudioApp: Aplikasi Desktop Utama ShardVault Studio

use eframe::egui::{self, Align, CentralPanel, Layout, RichText, SidePanel, TopBottomPanel};
use std::path::PathBuf;
use std::time::Instant;

use super::cluster_view::ChaosStudioView;
use super::data_view::DataBrowserView;
use super::model::{ActiveTab, SelectedTarget};
use super::query_view::SqlConsoleView;
use super::schema_view::SchemaInspectorView;
use super::sidebar::SidebarView;
use super::theme::{
    apply_studio_theme, COLOR_HEALTHY, COLOR_SYNCING, COLOR_TEXT_MUTED, COLOR_TEXT_SECONDARY,
};
use crate::config::EngineBuilder;
use crate::engine::DatabaseEngine;

pub struct StudioApp {
    pub data_dir_input: String,
    pub salt_input: String,
    pub engine: Option<DatabaseEngine>,
    pub selected_target: SelectedTarget,
    pub selected_table: Option<String>,
    pub active_tab: ActiveTab,

    pub sidebar: SidebarView,
    pub data_view: DataBrowserView,
    pub sql_view: SqlConsoleView,
    pub schema_view: SchemaInspectorView,
    pub chaos_view: ChaosStudioView,

    pub toast_message: Option<(String, Instant)>,
}

impl StudioApp {
    pub fn new(initial_dir: Option<PathBuf>) -> Self {
        let dir_str = initial_dir
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                if std::path::Path::new("E:/DocuTrack/data").exists() {
                    "E:/DocuTrack/data".to_string()
                } else {
                    "./data".to_string()
                }
            });

        let mut app = Self {
            data_dir_input: dir_str,
            salt_input: "docutrack_secret_salt_2026".to_string(),
            engine: None,
            selected_target: SelectedTarget::default(),
            selected_table: None,
            active_tab: ActiveTab::DataBrowser,

            sidebar: SidebarView::default(),
            data_view: DataBrowserView::default(),
            sql_view: SqlConsoleView::default(),
            schema_view: SchemaInspectorView::default(),
            chaos_view: ChaosStudioView::default(),

            toast_message: None,
        };

        // Otomatis koneksikan saat pertama menyala
        app.connect_engine();
        app
    }

    pub fn connect_engine(&mut self) {
        let path = std::path::Path::new(&self.data_dir_input);
        let config = EngineBuilder::new(path)
            .with_salt(&self.salt_input)
            .with_cache_size_mb(64)
            .with_mmap_size_mb(256)
            .build_config();

        match DatabaseEngine::with_config_and_schema(config, std::sync::Arc::new(crate::schema::EmptySchemaProvider)) {
            Ok(e) => {
                self.toast_message = Some((
                    format!("Berhasil terhubung ke cluster database di {:?}", path),
                    Instant::now(),
                ));
                self.engine = Some(e);
            }
            Err(err) => {
                self.toast_message = Some((
                    format!("Gagal menghubungkan engine: {}", err),
                    Instant::now(),
                ));
            }
        }
    }

    pub fn set_toast(&mut self, msg: String) {
        self.toast_message = Some((msg, Instant::now()));
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_studio_theme(ctx);

        let mut pending_toast = None;

        // 1. PANEL ATAS: HEADER & NAVIGASI TAB
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Logo & Judul
                ui.label(
                    RichText::new("⚡ SHARDVAULT STUDIO")
                        .size(17.0)
                        .color(COLOR_SYNCING)
                        .strong(),
                );
                ui.label(RichText::new("v1.0").size(10.0).color(COLOR_TEXT_MUTED));

                ui.separator();

                // Input Path Direktori Data
                ui.label(RichText::new("Data Dir:").size(11.0).color(COLOR_TEXT_SECONDARY));
                ui.add(egui::TextEdit::singleline(&mut self.data_dir_input).desired_width(180.0));

                ui.label(RichText::new("Salt:").size(11.0).color(COLOR_TEXT_SECONDARY));
                ui.add(egui::TextEdit::singleline(&mut self.salt_input).desired_width(120.0));

                if ui.button(RichText::new("🔗 Hubungkan").strong().color(COLOR_HEALTHY)).clicked() {
                    self.connect_engine();
                }

                ui.separator();

                // Navigasi Tab Utama
                for tab in [
                    ActiveTab::DataBrowser,
                    ActiveTab::SqlConsole,
                    ActiveTab::SchemaInspector,
                    ActiveTab::ChaosStudio,
                ] {
                    let is_active = self.active_tab == tab;
                    let text = if is_active {
                        RichText::new(format!("{}", tab)).color(COLOR_SYNCING).strong()
                    } else {
                        RichText::new(format!("{}", tab)).color(COLOR_TEXT_SECONDARY)
                    };

                    if ui.selectable_label(is_active, text).clicked() {
                        self.active_tab = tab;
                    }
                }

                // Target Aktif Indikator di Kanan
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let target_title = self.selected_target.display_title();
                    ui.label(
                        RichText::new(format!("Target: {}", target_title))
                            .size(11.0)
                            .color(COLOR_HEALTHY)
                            .strong(),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // 2. PANEL BAWAH: STATUS BAR
        TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((ref msg, instant)) = self.toast_message {
                    if instant.elapsed().as_secs() < 5 {
                        ui.label(RichText::new(format!("🔔 {}", msg)).color(COLOR_HEALTHY).size(12.0));
                    }
                } else {
                    ui.label(RichText::new("ShardVault Engine Siap.").color(COLOR_TEXT_MUTED).size(11.0));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(ref e) = self.engine {
                        let key_hex = crate::crypto::to_hex(&e.router().master_key);
                        let short_key = if key_hex.len() >= 8 { &key_hex[..8] } else { &key_hex };
                        ui.label(
                            RichText::new(format!("Master Key: {}... | WAL: NORMAL", short_key))
                                .size(11.0)
                                .color(COLOR_TEXT_MUTED),
                        );
                    }
                });
            });
        });

        // 3. PANEL KIRI: CLUSTER TREE NAVIGATOR
        SidePanel::left("left_navigator")
            .resizable(true)
            .default_width(260.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                self.sidebar.render(
                    ui,
                    &self.engine,
                    &mut self.selected_target,
                    &mut self.selected_table,
                    &mut pending_toast,
                );
            });

        // 4. PANEL TENGAH: KONTEN TAB UTAMA
        CentralPanel::default().show(ctx, |ui| {
            let engine = match self.engine {
                Some(ref e) => e,
                None => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.label(RichText::new("Cluster database belum terhubung.").size(18.0).color(COLOR_TEXT_MUTED));
                        ui.label("Periksa path folder data di panel atas dan klik 'Hubungkan'.");
                    });
                    return;
                }
            };

            match self.active_tab {
                ActiveTab::DataBrowser => {
                    self.data_view.render(
                        ui,
                        engine,
                        &self.selected_target,
                        &self.selected_table,
                        &mut pending_toast,
                    );
                }
                ActiveTab::SqlConsole => {
                    self.sql_view.render(
                        ui,
                        engine,
                        &mut self.selected_target,
                        &mut pending_toast,
                    );
                }
                ActiveTab::SchemaInspector => {
                    self.schema_view.render(
                        ui,
                        engine,
                        &self.selected_target,
                        &self.selected_table,
                        &mut pending_toast,
                    );
                }
                ActiveTab::ChaosStudio => {
                    self.chaos_view.render(ui, engine, &mut pending_toast);
                }
            }
        });

        if let Some(t) = pending_toast {
            self.set_toast(t);
        }
    }
}
