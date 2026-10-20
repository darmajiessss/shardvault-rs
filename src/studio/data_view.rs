// E:\shardvault-rs\src\studio\data_view.rs
// Table Data Browser: Penjelajah & Editor Data Tabel dengan Live AES Decryptor

use eframe::egui::{self, Grid, RichText, ScrollArea, Ui};

use super::model::{fetch_columns, SelectedTarget};
use super::theme::{
    COLOR_DANGER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::types::ShardRole;

pub struct DataBrowserView {
    pub auto_decrypt: bool,
    pub search_query: String,
    pub page: usize,
    pub page_size: usize,
    pub selected_cell_content: Option<(String, String)>, // (ColumnName, Value)
}

impl Default for DataBrowserView {
    fn default() -> Self {
        Self {
            auto_decrypt: true,
            search_query: String::new(),
            page: 0,
            page_size: 50,
            selected_cell_content: None,
        }
    }
}

impl DataBrowserView {
    pub fn render(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &SelectedTarget,
        selected_table: &Option<String>,
        toast: &mut Option<String>,
    ) {
        let table_name = match selected_table {
            Some(t) => t.as_str(),
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(
                        RichText::new("Silakan pilih tabel pada panel samping untuk melihat data.")
                            .size(16.0)
                            .color(COLOR_TEXT_MUTED),
                    );
                });
                return;
            }
        };

        // Ambil koneksi sesuai target terpilih
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

        // 1. Toolbar Atas
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(format!("⊞ Tabel: {}", table_name))
                    .size(16.0)
                    .color(COLOR_TEXT_PRIMARY)
                    .strong(),
            );

            ui.label(
                RichText::new(format!("({})", selected_target.display_title()))
                    .size(12.0)
                    .color(COLOR_TEXT_MUTED),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Toggle Live Decryptor
                let decrypt_btn = if self.auto_decrypt {
                    RichText::new("🔓 Dekripsi AES-256 Aktif").color(COLOR_HEALTHY).strong()
                } else {
                    RichText::new("🔒 Ciphertext Mentah").color(COLOR_TEXT_MUTED)
                };
                if ui.button(decrypt_btn).clicked() {
                    self.auto_decrypt = !self.auto_decrypt;
                    if self.auto_decrypt {
                        *toast = Some("Auto-dekripsi kolom terenkripsi diaktifkan".into());
                    } else {
                        *toast = Some("Menampilkan ciphertext mentah dari SQLite".into());
                    }
                }

                ui.separator();

                // Search Filter Box
                ui.label(RichText::new("🔍").size(14.0));
                ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Cari di tabel..."));
            });
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // 2. Ambil Kolom & Baris Data
        let columns = fetch_columns(&conn, table_name);
        if columns.is_empty() {
            ui.label(RichText::new("Tabel tidak memiliki kolom yang terdefinisi.").color(COLOR_TEXT_MUTED));
            return;
        }

        let offset = self.page * self.page_size;
        let query_str = format!(
            "SELECT * FROM \"{}\" LIMIT {} OFFSET {}",
            table_name, self.page_size, offset
        );

        let mut stmt = match conn.prepare(&query_str) {
            Ok(s) => s,
            Err(e) => {
                ui.label(RichText::new(format!("Gagal menyiapkan query: {}", e)).color(COLOR_DANGER));
                return;
            }
        };

        let col_count = columns.len();
        let rows_res: Result<Vec<Vec<String>>, _> = stmt
            .query_map([], |row| {
                let mut row_vals = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let val: rusqlite::types::Value = row.get(i)?;
                    let str_val = match val {
                        rusqlite::types::Value::Null => "NULL".to_string(),
                        rusqlite::types::Value::Integer(v) => v.to_string(),
                        rusqlite::types::Value::Real(v) => format!("{:.2}", v),
                        rusqlite::types::Value::Text(v) => v,
                        rusqlite::types::Value::Blob(b) => format!("<BLOB {}B>", b.len()),
                    };
                    row_vals.push(str_val);
                }
                Ok(row_vals)
            })
            .map(|mapped| mapped.filter_map(|r| r.ok()).collect());

        let rows = rows_res.unwrap_or_default();

        // Filter pencarian
        let filtered_rows: Vec<&Vec<String>> = if self.search_query.trim().is_empty() {
            rows.iter().collect()
        } else {
            let q = self.search_query.to_lowercase();
            rows.iter()
                .filter(|r| r.iter().any(|cell| cell.to_lowercase().contains(&q)))
                .collect()
        };

        // 3. Grid Tabel Data
        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            Grid::new("data_browser_grid")
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    // Header Baris
                    ui.label(RichText::new("#").strong().color(COLOR_TEXT_MUTED));
                    for col in &columns {
                        let header_text = format!("{} ({})", col.name, col.data_type);
                        let header_color = if col.is_pk { COLOR_WARNING } else { COLOR_TEXT_PRIMARY };
                        ui.label(RichText::new(header_text).strong().color(header_color));
                    }
                    ui.end_row();

                    // Baris Data
                    for (row_idx, row_data) in filtered_rows.iter().enumerate() {
                        let abs_idx = offset + row_idx + 1;
                        ui.label(RichText::new(format!("{}", abs_idx)).size(11.0).color(COLOR_TEXT_MUTED));

                        for (col_idx, raw_val) in row_data.iter().enumerate() {
                            let col_name = &columns[col_idx].name;

                            // Cek apakah cell ini berupa AES-256-GCM encrypted format (v1:nonce:ciphertext)
                            let (display_val, cell_color, _is_encrypted) = if raw_val.starts_with("v1:") && raw_val.contains(':') {
                                if self.auto_decrypt {
                                    match engine.decrypt(raw_val) {
                                        Ok(decrypted) => (format!("🔓 {}", decrypted), COLOR_HEALTHY, true),
                                        Err(_) => (format!("⚠ TAMPER: {}", raw_val), COLOR_DANGER, true),
                                    }
                                } else {
                                    (format!("🔒 {}", raw_val), COLOR_PURPLE, true)
                                }
                            } else if raw_val == "NULL" {
                                ("NULL".to_string(), COLOR_TEXT_MUTED, false)
                            } else {
                                (raw_val.clone(), COLOR_TEXT_PRIMARY, false)
                            };

                            let truncated = if display_val.len() > 50 {
                                format!("{}...", &display_val[..47])
                            } else {
                                display_val.clone()
                            };

                            let cell_btn = ui.selectable_label(
                                false,
                                RichText::new(truncated).color(cell_color).size(12.0),
                            );

                            if cell_btn.clicked() {
                                self.selected_cell_content = Some((col_name.clone(), display_val));
                            }
                        }
                        ui.end_row();
                    }
                });
        });

        // 4. Drawer Inspector untuk Cell yang Diklik
        let mut close_inspector = false;
        if let Some((ref col_name, ref val)) = self.selected_cell_content {
            ui.add_space(8.0);
            ui.separator();
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("🔍 Detail Nilai Kolom: [{}]", col_name))
                            .strong()
                            .color(COLOR_SYNCING),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Tutup").clicked() {
                            close_inspector = true;
                        }
                        if ui.button("📋 Salin").clicked() {
                            ui.output_mut(|o| o.copied_text = val.clone());
                            *toast = Some("Nilai berhasil disalin ke clipboard!".into());
                        }
                    });
                });
                ui.add_space(4.0);
                ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut val.as_str())
                            .font(egui::FontId::monospace(12.0))
                            .desired_width(f32::INFINITY),
                    );
                });
            });
        }
        if close_inspector {
            self.selected_cell_content = None;
        }
    }
}
