// E:\shardvault-rs\src\studio\migration_view.rs
// View GUI untuk Manajemen Migrasi & Deteksi Schema Drift

use eframe::egui::{self, RichText, ScrollArea, TextEdit, Ui};

use super::model::SelectedTarget;
use super::theme::{
    COLOR_BG_SURFACE, COLOR_BORDER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::migration::{detect_schema_drift, get_applied_migrations, Migration, MigrationRunner, MigrationTarget};

#[derive(Default)]
pub struct MigrationStudioView {
    pub ddl_input: String,
    pub migration_name_input: String,
    pub migration_version_input: String,
    pub drift_target_shard: String,
    pub drift_results: Vec<String>,
}

impl MigrationStudioView {
    pub fn render(
        &mut self,
        ui: &mut Ui,
        engine: &Option<DatabaseEngine>,
        selected_target: &SelectedTarget,
        toast: &mut Option<String>,
    ) {
        let engine = match engine {
            Some(e) => e,
            None => {
                ui.label(RichText::new("Engine belum terhubung.").color(COLOR_TEXT_MUTED));
                return;
            }
        };

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🚀 SCHEMA MIGRATION & DRIFT DETECTOR").size(16.0).color(COLOR_TEXT_PRIMARY).strong());
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Kelola versi skema database, jalankan migrasi atomik, dan deteksi deviasi struktur antar shard.")
                    .color(COLOR_TEXT_MUTED)
                    .size(12.0),
            );
            ui.add_space(10.0);

            // SECTION 1: RIWAYAT MIGRASI PADA TARGET AKTIF
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("📜 Riwayat Migrasi Diterapkan:").size(13.0).strong().color(COLOR_SYNCING));
                        ui.label(RichText::new(format!("({})", selected_target.display_title())).size(11.0).color(COLOR_TEXT_MUTED));
                    });
                    ui.add_space(6.0);

                    let migrations_res = match selected_target {
                        SelectedTarget::Global(role) => {
                            if let Ok(arc) = engine.router().get_global_clone(*role) {
                                let conn = arc.lock().unwrap();
                                get_applied_migrations(&conn)
                            } else {
                                Ok(Vec::new())
                            }
                        }
                        SelectedTarget::Shard { key, role } => {
                            if let Ok(arc) = engine.router().get_shard_conn(key, *role) {
                                let conn = arc.lock().unwrap();
                                get_applied_migrations(&conn)
                            } else {
                                Ok(Vec::new())
                            }
                        }
                    };

                    match migrations_res {
                        Ok(migrations) if !migrations.is_empty() => {
                            egui::Grid::new("migrations_grid")
                                .striped(true)
                                .min_col_width(80.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new("VER").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("NAMA MIGRASI").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("CHECKSUM SHA-256").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("DITERAPKAN PADA").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("DURASI").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.end_row();

                                    for m in migrations {
                                        ui.label(RichText::new(format!("v{}", m.version)).color(COLOR_HEALTHY).strong());
                                        ui.label(RichText::new(&m.name).color(COLOR_TEXT_PRIMARY));
                                        let short_hash = if m.checksum_sha256.len() > 16 {
                                            format!("{}...", &m.checksum_sha256[..16])
                                        } else {
                                            m.checksum_sha256.clone()
                                        };
                                        ui.label(RichText::new(short_hash).color(COLOR_TEXT_MUTED).monospace());
                                        ui.label(RichText::new(&m.applied_at_rfc3339).color(COLOR_TEXT_SECONDARY).size(11.0));
                                        ui.label(RichText::new(format!("{} ms", m.execution_time_ms)).color(COLOR_TEXT_MUTED));
                                        ui.end_row();
                                    }
                                });
                        }
                        Ok(_) => {
                            ui.label(RichText::new("Belum ada migrasi yang tercatat pada database ini.").color(COLOR_TEXT_MUTED));
                        }
                        Err(e) => {
                            ui.label(RichText::new(format!("Gagal membaca riwayat: {}", e)).color(COLOR_WARNING));
                        }
                    }
                });

            ui.add_space(12.0);

            // SECTION 2: JALANKAN MIGRASI BARU KE CLUSTER
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("⚡ Eksekusi Migrasi DDL ke Seluruh Cluster:").size(13.0).strong().color(COLOR_PURPLE));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Versi:").size(11.0).color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.migration_version_input).desired_width(60.0).hint_text("Misal: 2"));

                        ui.label(RichText::new("Nama:").size(11.0).color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.migration_name_input).desired_width(200.0).hint_text("create_table_x"));
                    });

                    ui.add_space(6.0);
                    ui.label(RichText::new("SQL DDL Script:").size(11.0).color(COLOR_TEXT_SECONDARY));
                    ui.add(
                        TextEdit::multiline(&mut self.ddl_input)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("CREATE TABLE IF NOT EXISTS coupons (code TEXT PRIMARY KEY, discount REAL);"),
                    );

                    ui.add_space(6.0);
                    if ui.button(RichText::new("🚀 Terapkan ke Seluruh Shard & Global").color(COLOR_HEALTHY).strong()).clicked() {
                        let ver = self.migration_version_input.trim().parse::<u32>().unwrap_or(1);
                        let name: &'static str = Box::leak(self.migration_name_input.trim().to_string().into_boxed_str());
                        let sql: &'static str = Box::leak(self.ddl_input.trim().to_string().into_boxed_str());

                        if sql.is_empty() {
                            *toast = Some("DDL SQL tidak boleh kosong!".into());
                        } else {
                            let mut runner = MigrationRunner::new();
                            runner.add(Migration {
                                version: ver,
                                name,
                                target: MigrationTarget::All,
                                up_sql: sql,
                                down_sql: None,
                            });

                            match engine.run_migrations(&runner) {
                                Ok(reports) => {
                                    *toast = Some(format!("Migrasi v{} sukses diterapkan ke {} target!", ver, reports.len()));
                                    self.ddl_input.clear();
                                    self.migration_name_input.clear();
                                }
                                Err(e) => {
                                    *toast = Some(format!("Migrasi gagal: {}", e));
                                }
                            }
                        }
                    }
                });

            ui.add_space(12.0);

            // SECTION 3: SCHEMA DRIFT DETECTOR
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("🔍 Deteksi Schema Drift (Perbandingan Skema):").size(13.0).strong().color(COLOR_WARNING));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Bandingkan Global vs Shard:").size(11.0).color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.drift_target_shard).desired_width(120.0).hint_text("nama_shard"));

                        if ui.button(RichText::new("🔬 Deteksi Deviasi").color(COLOR_SYNCING).strong()).clicked() {
                            let shard_name = self.drift_target_shard.trim();
                            if shard_name.is_empty() {
                                *toast = Some("Masukkan nama shard untuk dibandingkan!".into());
                            } else {
                                match (engine.introspect_global(), engine.introspect_shard(shard_name)) {
                                    (Ok(m_global), Ok(m_shard)) => {
                                        self.drift_results = detect_schema_drift(&m_global, &m_shard);
                                        if self.drift_results.is_empty() {
                                            *toast = Some("Skema 100% selaras! Tidak ada drift ditemukan.".into());
                                        } else {
                                            *toast = Some(format!("Ditemukan {} perbedaan struktur!", self.drift_results.len()));
                                        }
                                    }
                                    (Err(e), _) | (_, Err(e)) => {
                                        *toast = Some(format!("Gagal introspeksi: {}", e));
                                    }
                                }
                            }
                        }
                    });

                    if !self.drift_results.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Perbedaan Terdeteksi:").strong().color(COLOR_WARNING));
                        for diff in &self.drift_results {
                            ui.label(RichText::new(format!("• {}", diff)).color(COLOR_TEXT_SECONDARY).size(11.0));
                        }
                    }
                });
        });
    }
}
