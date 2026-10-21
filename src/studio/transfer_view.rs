// E:\shardvault-rs\src\studio\transfer_view.rs
// View GUI untuk Ekspor & Impor Paket Portabel Terenkripsi (.svp)

use eframe::egui::{self, RichText, ScrollArea, TextEdit, Ui};

use super::model::SelectedTarget;
use super::theme::{
    COLOR_BG_SURFACE, COLOR_BORDER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY,
};
use crate::engine::DatabaseEngine;
use crate::transfer::PackageMetadata;

#[derive(Default)]
pub struct TransferStudioView {
    pub export_shard_key: String,
    pub export_dest_path: String,
    pub export_passphrase: String,
    pub last_export_meta: Option<PackageMetadata>,

    pub import_source_path: String,
    pub import_target_shard_key: String,
    pub import_passphrase: String,
    pub last_import_meta: Option<PackageMetadata>,
}

impl TransferStudioView {
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

        // Jika export_shard_key masih kosong, isi dengan selected shard jika ada
        if self.export_shard_key.is_empty() {
            if let Some(key) = selected_target.shard_key() {
                self.export_shard_key = key.to_string();
                self.export_dest_path = format!("./backups/{}_export.svp", key);
            }
        }

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("📦 DATABASE TRANSFER HUB (.SVP)").size(16.0).color(COLOR_TEXT_PRIMARY).strong());
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Ekspor dan impor database terenkripsi portabel (.svp) dengan kompresi Zstandard dan tanda tangan HMAC-SHA256.")
                    .color(COLOR_TEXT_MUTED)
                    .size(12.0),
            );
            ui.add_space(10.0);

            // SECTION 1: WIZARD EKSPOR PAKET
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("📤 Ekspor Shard ke Berkas Paket (.svp):").size(13.0).strong().color(COLOR_SYNCING));
                    ui.add_space(6.0);

                    egui::Grid::new("export_grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label(RichText::new("Nama Shard Sumber:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.export_shard_key).desired_width(240.0));
                        ui.end_row();

                        ui.label(RichText::new("Path Berkas Tujuan (.svp):").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.export_dest_path).desired_width(320.0));
                        ui.end_row();

                        ui.label(RichText::new("Passphrase Transfer:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.export_passphrase).password(true).desired_width(240.0));
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    if ui.button(RichText::new("📦 Buat Paket Terenkripsi (.svp)").color(COLOR_HEALTHY).strong()).clicked() {
                        let key = self.export_shard_key.trim();
                        let dest = self.export_dest_path.trim();
                        let pass = self.export_passphrase.trim();

                        if key.is_empty() || dest.is_empty() || pass.is_empty() {
                            *toast = Some("Lengkapi seluruh kolom ekspor!".into());
                        } else {
                            match engine.export_shard_package(key, dest, pass) {
                                Ok(meta) => {
                                    *toast = Some(format!("Ekspor berhasil ke '{}'!", dest));
                                    self.last_export_meta = Some(meta);
                                }
                                Err(e) => {
                                    *toast = Some(format!("Ekspor gagal: {}", e));
                                }
                            }
                        }
                    }

                    if let Some(ref m) = self.last_export_meta {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(RichText::new(format!("✓ Berhasil diekspor: Shard '{}' (v{})", m.shard_key, m.schema_version)).color(COLOR_HEALTHY));
                        ui.label(RichText::new(format!("SHA-256: {}", m.checksum_sha256)).size(10.0).monospace().color(COLOR_TEXT_MUTED));
                        ui.label(RichText::new(format!("Tabel tercakup: {:?}", m.row_counts)).size(11.0).color(COLOR_TEXT_SECONDARY));
                    }
                });

            ui.add_space(12.0);

            // SECTION 2: WIZARD IMPOR PAKET
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("📥 Impor Berkas Paket (.svp) ke Cluster:").size(13.0).strong().color(COLOR_PURPLE));
                    ui.add_space(6.0);

                    egui::Grid::new("import_grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label(RichText::new("Path Berkas Sumber (.svp):").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.import_source_path).desired_width(320.0).hint_text("./backups/sample.svp"));
                        ui.end_row();

                        ui.label(RichText::new("Nama Shard Target:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.import_target_shard_key).desired_width(240.0).hint_text("nama_shard_baru"));
                        ui.end_row();

                        ui.label(RichText::new("Passphrase Transfer:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.import_passphrase).password(true).desired_width(240.0));
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    if ui.button(RichText::new("📥 Verifikasi HMAC & Impor ke Shard").color(COLOR_SYNCING).strong()).clicked() {
                        let src = self.import_source_path.trim();
                        let target_key = self.import_target_shard_key.trim();
                        let pass = self.import_passphrase.trim();

                        if src.is_empty() || target_key.is_empty() || pass.is_empty() {
                            *toast = Some("Lengkapi seluruh kolom impor!".into());
                        } else {
                            match engine.import_shard_package(src, target_key, pass) {
                                Ok(meta) => {
                                    *toast = Some(format!("Impor sukses! Shard '{}' kini aktif beserta 3 kloningnya.", target_key));
                                    self.last_import_meta = Some(meta);
                                }
                                Err(e) => {
                                    *toast = Some(format!("Impor gagal: {}", e));
                                }
                            }
                        }
                    }

                    if let Some(ref m) = self.last_import_meta {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(RichText::new(format!("✓ Sukses diimpor sebagai '{}'", self.import_target_shard_key)).color(COLOR_HEALTHY));
                        ui.label(RichText::new(format!("Asal Shard: '{}' (v{})", m.shard_key, m.schema_version)).size(11.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new(format!("Integritas Terverifikasi: SHA-256 {}", m.checksum_sha256)).size(10.0).monospace().color(COLOR_TEXT_MUTED));
                    }
                });
        });
    }
}
