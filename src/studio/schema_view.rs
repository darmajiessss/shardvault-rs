// E:\shardvault-rs\src\studio\schema_view.rs
// Schema & DDL Inspector: Peninjau Struktur Tabel, Indeks, & Riwayat Migrasi

use eframe::egui::{self, Grid, RichText, ScrollArea, Ui};

use super::model::{fetch_columns, fetch_table_ddl, fetch_table_indexes, fetch_tables, SelectedTarget};
use super::theme::{
    COLOR_DANGER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::types::ShardRole;

#[derive(Default)]
pub struct SchemaInspectorView {
    pub selected_table_inspect: Option<String>,
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
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("📐 SCHEMA & DDL INSPECTOR")
                    .size(16.0)
                    .color(COLOR_TEXT_PRIMARY)
                    .strong(),
            );

            ui.label(
                RichText::new(format!("Basis Data: {}", selected_target.display_title()))
                    .size(12.0)
                    .color(COLOR_SYNCING),
            );
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        let conn_arc_res = match selected_target {
            SelectedTarget::Global(role) => {
                if *role == ShardRole::PrimaryWrite {
                    engine.router().get_global_conn()
                } else {
                    engine.router().get_global_clone(*role)
                }
            }
            SelectedTarget::Shard { key, role } => engine.router().get_shard_conn(key, *role),
        };

        let conn_arc = match conn_arc_res {
            Ok(c) => c,
            Err(e) => {
                ui.label(RichText::new(format!("Gagal membuka koneksi database: {}", e)).color(COLOR_DANGER));
                return;
            }
        };

        let conn = conn_arc.lock().unwrap();
        let tables = fetch_tables(&conn);

        if tables.is_empty() {
            ui.label(RichText::new("Tidak ada tabel ditemukan pada database ini.").color(COLOR_TEXT_MUTED));
            return;
        }

        // Tentukan tabel mana yang diinspeksi
        if self.selected_table_inspect.is_none() {
            if let Some(ref st) = selected_table {
                self.selected_table_inspect = Some(st.clone());
            } else if let Some(first) = tables.first() {
                self.selected_table_inspect = Some(first.name.clone());
            }
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Pilih Tabel:").strong().color(COLOR_TEXT_SECONDARY));
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
            // 1. DDL CREATE TABLE
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📄 Skema DDL: {}", inspect_tbl))
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

            // 2. Kolom-Kolom Tabel
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

            // 3. Indeks
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
