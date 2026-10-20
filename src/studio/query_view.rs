// E:\shardvault-rs\src\studio\query_view.rs
// Interactive SQL Console: Editor Query SQLite Interaktif dengan Pengukuran Latensi

use eframe::egui::{self, Color32, Grid, RichText, ScrollArea, Ui};
use std::time::Instant;

use super::model::{SelectedTarget, SqlExecutionResult};
use super::theme::{
    COLOR_DANGER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::engine::DatabaseEngine;
use crate::types::ShardRole;

pub struct SqlConsoleView {
    pub sql_input: String,
    pub execution_result: SqlExecutionResult,
    pub query_history: Vec<String>,
}

impl Default for SqlConsoleView {
    fn default() -> Self {
        Self {
            sql_input: "SELECT * FROM _shardvault_migrations;\n-- Tulis query SQL Anda di sini (SELECT, INSERT, UPDATE, PRAGMA)".to_string(),
            execution_result: SqlExecutionResult::default(),
            query_history: Vec::new(),
        }
    }
}

impl SqlConsoleView {
    pub fn render(
        &mut self,
        ui: &mut Ui,
        engine: &DatabaseEngine,
        selected_target: &mut SelectedTarget,
        toast: &mut Option<String>,
    ) {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("⚡ SQL CONSOLE")
                    .size(16.0)
                    .color(COLOR_TEXT_PRIMARY)
                    .strong(),
            );

            ui.label(
                RichText::new(format!("Target: {}", selected_target.display_title()))
                    .size(12.0)
                    .color(COLOR_SYNCING),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Tombol Eksekusi Utama
                let run_btn = ui.button(
                    RichText::new("▶ Jalankan Query (Ctrl+Enter)")
                        .size(13.0)
                        .color(Color32::WHITE)
                        .strong(),
                );

                if run_btn.clicked()
                    || (ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)))
                {
                    self.execute_sql(engine, selected_target);
                }

                ui.separator();

                // Quick Snippets
                if ui.button("PRAGMA integrity_check").clicked() {
                    self.sql_input = "PRAGMA integrity_check;".to_string();
                    self.execute_sql(engine, selected_target);
                }
                if ui.button("PRAGMA quick_check").clicked() {
                    self.sql_input = "PRAGMA quick_check;".to_string();
                    self.execute_sql(engine, selected_target);
                }
                if ui.button("Daftar Tabel").clicked() {
                    self.sql_input = "SELECT type, name, tbl_name FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%';".to_string();
                    self.execute_sql(engine, selected_target);
                }
            });
        });

        ui.add_space(6.0);
        ui.separator();

        // 1. Text Area SQL Input
        ui.label(RichText::new("SQL Query:").size(11.0).color(COLOR_TEXT_MUTED));
        let text_edit = egui::TextEdit::multiline(&mut self.sql_input)
            .font(egui::FontId::monospace(12.0))
            .desired_rows(6)
            .desired_width(f32::INFINITY);
        ui.add(text_edit);

        ui.add_space(6.0);

        // 2. Bar Metrik Eksekusi
        if let Some(ref err) = self.execution_result.error_message {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❌ ERROR SQL:").color(COLOR_DANGER).strong());
                    ui.label(RichText::new(err).color(COLOR_DANGER));
                });
            });
        } else if !self.execution_result.columns.is_empty() || self.execution_result.rows_affected > 0 {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("✓ Berhasil dieksekusi")
                        .color(COLOR_HEALTHY)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("⏱ Latensi: {:.2} ms", self.execution_result.elapsed_ms))
                        .color(COLOR_SYNCING),
                );
                if !self.execution_result.rows.is_empty() {
                    ui.label(
                        RichText::new(format!("📊 Baris: {}", self.execution_result.rows.len()))
                            .color(COLOR_TEXT_MUTED),
                    );
                }
                if self.execution_result.rows_affected > 0 {
                    ui.label(
                        RichText::new(format!("📝 Terpengaruh: {}", self.execution_result.rows_affected))
                            .color(COLOR_PURPLE),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("📋 Salin CSV").clicked() {
                        let mut csv = self.execution_result.columns.join(",") + "\n";
                        for row in &self.execution_result.rows {
                            csv.push_str(&row.join(","));
                            csv.push('\n');
                        }
                        ui.output_mut(|o| o.copied_text = csv);
                        *toast = Some("Hasil query disalin ke format CSV!".into());
                    }
                });
            });
        }

        ui.add_space(4.0);
        ui.separator();

        // 3. Tabel Hasil Query
        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            if self.execution_result.columns.is_empty() && self.execution_result.error_message.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        RichText::new("Belum ada query yang dieksekusi. Ketik query dan tekan 'Jalankan'.")
                            .color(COLOR_TEXT_MUTED),
                    );
                });
                return;
            }

            Grid::new("sql_result_grid")
                .striped(true)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    // Header Kolom
                    ui.label(RichText::new("#").strong().color(COLOR_TEXT_MUTED));
                    for col in &self.execution_result.columns {
                        ui.label(RichText::new(col).strong().color(COLOR_SYNCING));
                    }
                    ui.end_row();

                    // Baris Data
                    for (idx, row) in self.execution_result.rows.iter().enumerate() {
                        ui.label(RichText::new(format!("{}", idx + 1)).size(11.0).color(COLOR_TEXT_MUTED));
                        for val in row {
                            let text = if val.starts_with("v1:") {
                                RichText::new(val).color(COLOR_PURPLE).size(12.0)
                            } else if val == "NULL" {
                                RichText::new("NULL").color(COLOR_TEXT_MUTED).size(12.0)
                            } else {
                                RichText::new(val).color(COLOR_TEXT_PRIMARY).size(12.0)
                            };
                            ui.label(text);
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn execute_sql(&mut self, engine: &DatabaseEngine, selected_target: &SelectedTarget) {
        let sql = self.sql_input.trim();
        if sql.is_empty() {
            return;
        }

        // Simpan ke riwayat
        if self.query_history.last().map(|s| s.as_str()) != Some(sql) {
            self.query_history.push(sql.to_string());
        }

        let start = Instant::now();

        let conn_res = match selected_target {
            SelectedTarget::Global(role) => {
                if *role == ShardRole::PrimaryWrite {
                    engine.router().get_global_conn()
                } else {
                    engine.router().get_global_clone(*role)
                }
            }
            SelectedTarget::Shard { key, role } => engine.router().get_shard_conn(key, *role),
        };

        let conn_arc = match conn_res {
            Ok(c) => c,
            Err(e) => {
                self.execution_result = SqlExecutionResult {
                    error_message: Some(format!("Koneksi error: {}", e)),
                    ..Default::default()
                };
                return;
            }
        };

        let conn = conn_arc.lock().unwrap();

        // Cek apakah SELECT atau mutasi (INSERT/UPDATE/DELETE/PRAGMA)
        let is_select = sql.to_uppercase().starts_with("SELECT")
            || sql.to_uppercase().starts_with("PRAGMA")
            || sql.to_uppercase().starts_with("EXPLAIN");

        if is_select {
            let mut stmt = match conn.prepare(sql) {
                Ok(s) => s,
                Err(e) => {
                    self.execution_result = SqlExecutionResult {
                        error_message: Some(e.to_string()),
                        ..Default::default()
                    };
                    return;
                }
            };

            let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
            let col_count = col_names.len();

            let rows_res: Result<Vec<Vec<String>>, _> = stmt
                .query_map([], |row| {
                    let mut r = Vec::with_capacity(col_count);
                    for i in 0..col_count {
                        let val: rusqlite::types::Value = row.get(i)?;
                        let s = match val {
                            rusqlite::types::Value::Null => "NULL".to_string(),
                            rusqlite::types::Value::Integer(v) => v.to_string(),
                            rusqlite::types::Value::Real(v) => format!("{}", v),
                            rusqlite::types::Value::Text(v) => v,
                            rusqlite::types::Value::Blob(b) => format!("<BLOB {}B>", b.len()),
                        };
                        r.push(s);
                    }
                    Ok(r)
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect());

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            match rows_res {
                Ok(rows) => {
                    self.execution_result = SqlExecutionResult {
                        columns: col_names,
                        rows,
                        elapsed_ms: elapsed,
                        rows_affected: 0,
                        error_message: None,
                    };
                }
                Err(e) => {
                    self.execution_result = SqlExecutionResult {
                        elapsed_ms: elapsed,
                        error_message: Some(e.to_string()),
                        ..Default::default()
                    };
                }
            }
        } else {
            // Eksekusi mutasi
            let res = conn.execute(sql, []);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            match res {
                Ok(affected) => {
                    self.execution_result = SqlExecutionResult {
                        columns: vec!["Status".into()],
                        rows: vec![vec![format!("Query mutasi sukses. {} baris terpengaruh.", affected)]],
                        elapsed_ms: elapsed,
                        rows_affected: affected,
                        error_message: None,
                    };
                }
                Err(e) => {
                    self.execution_result = SqlExecutionResult {
                        elapsed_ms: elapsed,
                        error_message: Some(e.to_string()),
                        ..Default::default()
                    };
                }
            }
        }
    }
}
