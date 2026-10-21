// E:\shardvault-rs\src\studio\sync_view.rs
// View GUI untuk Monitor CDC Oplog, HLC Telemetry, & Sinkronisasi Terdistribusi

use eframe::egui::{self, RichText, ScrollArea, TextEdit, Ui};

use super::model::SelectedTarget;
use super::theme::{
    COLOR_BG_SURFACE, COLOR_BORDER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING,
    COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::sync::{mark_oplog_synced, purge_synced_oplog, setup_table_cdc_triggers, OplogEntry};

#[derive(Default)]
pub struct SyncStudioView {
    pub cdc_table_input: String,
    pub node_id_input: String,
}

impl SyncStudioView {
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

        if self.node_id_input.is_empty() {
            self.node_id_input = "node_local_studio".to_string();
        }

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🔄 DISTRIBUTED SYNC & CDC OPLOG").size(16.0).color(COLOR_TEXT_PRIMARY).strong());
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Monitor mutasi Change Data Capture (CDC), generator Hybrid Logical Clock (HLC), dan resolusi konflik Last-Write-Wins.")
                    .color(COLOR_TEXT_MUTED)
                    .size(12.0),
            );
            ui.add_space(10.0);

            // SECTION 1: STATUS HLC & KONTROL CDC TRIGGER
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚙️ Konfigurasi CDC & Node HLC:").size(13.0).strong().color(COLOR_SYNCING));
                        ui.label(RichText::new(format!("({})", selected_target.display_title())).size(11.0).color(COLOR_TEXT_MUTED));
                    });
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Node Identifier:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.node_id_input).desired_width(160.0));

                        let sync_eng = engine.create_sync_engine(&self.node_id_input);
                        let current_hlc = sync_eng.hlc.now();
                        ui.label(RichText::new(format!("HLC Sekarang: {}", current_hlc)).monospace().color(COLOR_SYNCING).strong());
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Pasang CDC pada Tabel:").color(COLOR_TEXT_SECONDARY));
                        ui.add(TextEdit::singleline(&mut self.cdc_table_input).desired_width(160.0).hint_text("nama_tabel"));

                        if ui.button(RichText::new("⚡ Pasang Triggers").color(COLOR_HEALTHY).strong()).clicked() {
                            let tbl = self.cdc_table_input.trim();
                            if tbl.is_empty() {
                                *toast = Some("Masukkan nama tabel yang akan dipasangi trigger!".into());
                            } else {
                                let res = match selected_target {
                                    SelectedTarget::Global(role) => {
                                        if let Ok(arc) = engine.router().get_global_clone(*role) {
                                            let conn = arc.lock().unwrap();
                                            setup_table_cdc_triggers(&conn, tbl, &self.node_id_input)
                                        } else {
                                            Ok(())
                                        }
                                    }
                                    SelectedTarget::Shard { key, role } => {
                                        if let Ok(arc) = engine.router().get_shard_conn(key, *role) {
                                            let conn = arc.lock().unwrap();
                                            setup_table_cdc_triggers(&conn, tbl, &self.node_id_input)
                                        } else {
                                            Ok(())
                                        }
                                    }
                                };

                                match res {
                                    Ok(_) => *toast = Some(format!("CDC triggers aktif pada tabel '{}'!", tbl)),
                                    Err(e) => *toast = Some(format!("Gagal pasang trigger: {}", e)),
                                }
                            }
                        }
                    });
                });

            ui.add_space(12.0);

            // SECTION 2: STREAM MUTASI _SHARDVAULT_OPLOG
            egui::Frame::none()
                .fill(COLOR_BG_SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("📋 Aliran Log Mutasi (_shardvault_oplog):").size(13.0).strong().color(COLOR_PURPLE));
                    });
                    ui.add_space(6.0);

                    // Ambil 30 entri terbaru dari oplog
                    let oplog_entries_res: Result<Vec<OplogEntry>, _> = match selected_target {
                        SelectedTarget::Global(role) => {
                            if let Ok(arc) = engine.router().get_global_clone(*role) {
                                let conn = arc.lock().unwrap();
                                let stmt = conn.prepare("SELECT op_id, table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced FROM _shardvault_oplog ORDER BY op_id DESC LIMIT 30;");
                                match stmt {
                                    Ok(mut s) => {
                                        let rows = s.query_map([], |r| {
                                            Ok(OplogEntry {
                                                op_id: r.get(0)?,
                                                table_name: r.get(1)?,
                                                row_pk: r.get(2)?,
                                                operation: r.get(3)?,
                                                hlc_timestamp: r.get(4)?,
                                                node_id: r.get(5)?,
                                                payload_json: r.get(6)?,
                                                created_at_utc: r.get(7)?,
                                                is_synced: r.get::<_, i32>(8)? != 0,
                                            })
                                        });
                                        match rows {
                                            Ok(r_iter) => Ok(r_iter.flatten().collect()),
                                            Err(e) => Err(e.to_string()),
                                        }
                                    }
                                    Err(e) => Err(e.to_string()),
                                }
                            } else {
                                Ok(Vec::new())
                            }
                        }
                        SelectedTarget::Shard { key, role } => {
                            if let Ok(arc) = engine.router().get_shard_conn(key, *role) {
                                let conn = arc.lock().unwrap();
                                let stmt = conn.prepare("SELECT op_id, table_name, row_pk, operation, hlc_timestamp, node_id, payload_json, created_at_utc, is_synced FROM _shardvault_oplog ORDER BY op_id DESC LIMIT 30;");
                                match stmt {
                                    Ok(mut s) => {
                                        let rows = s.query_map([], |r| {
                                            Ok(OplogEntry {
                                                op_id: r.get(0)?,
                                                table_name: r.get(1)?,
                                                row_pk: r.get(2)?,
                                                operation: r.get(3)?,
                                                hlc_timestamp: r.get(4)?,
                                                node_id: r.get(5)?,
                                                payload_json: r.get(6)?,
                                                created_at_utc: r.get(7)?,
                                                is_synced: r.get::<_, i32>(8)? != 0,
                                            })
                                        });
                                        match rows {
                                            Ok(r_iter) => Ok(r_iter.flatten().collect()),
                                            Err(e) => Err(e.to_string()),
                                        }
                                    }
                                    Err(e) => Err(e.to_string()),
                                }
                            } else {
                                Ok(Vec::new())
                            }
                        }
                    };

                    match oplog_entries_res {
                        Ok(entries) if !entries.is_empty() => {
                            egui::Grid::new("oplog_grid")
                                .striped(true)
                                .min_col_width(60.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new("ID").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("TABEL").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("PK").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("OPERASI").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("HLC").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("NODE").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("STATUS").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.label(RichText::new("PAYLOAD PREVIEW").strong().color(COLOR_TEXT_SECONDARY));
                                    ui.end_row();

                                    for op in &entries {
                                        ui.label(RichText::new(format!("#{}", op.op_id)).color(COLOR_TEXT_MUTED));
                                        ui.label(RichText::new(&op.table_name).color(COLOR_TEXT_PRIMARY));
                                        ui.label(RichText::new(&op.row_pk).color(COLOR_SYNCING));
                                        
                                        let op_color = match op.operation.as_str() {
                                            "INSERT" => COLOR_HEALTHY,
                                            "UPDATE" => COLOR_SYNCING,
                                            "DELETE" => COLOR_WARNING,
                                            _ => COLOR_TEXT_PRIMARY,
                                        };
                                        ui.label(RichText::new(&op.operation).color(op_color).strong());
                                        ui.label(RichText::new(&op.hlc_timestamp).monospace().size(11.0).color(COLOR_TEXT_SECONDARY));
                                        ui.label(RichText::new(&op.node_id).color(COLOR_TEXT_MUTED).size(11.0));

                                        if op.is_synced {
                                            ui.label(RichText::new("✓ SYNCED").color(COLOR_HEALTHY).size(10.0));
                                        } else {
                                            ui.label(RichText::new("⏳ PENDING").color(COLOR_WARNING).size(10.0));
                                        }

                                        let preview = if op.payload_json.len() > 30 {
                                            format!("{}...", &op.payload_json[..30])
                                        } else {
                                            op.payload_json.clone()
                                        };
                                        ui.label(RichText::new(preview).color(COLOR_TEXT_MUTED).size(10.0));
                                        ui.end_row();
                                    }
                                });

                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("✓ Tandai Seluruh Oplog Sebagai Tersinkronkan").color(COLOR_HEALTHY)).clicked() {
                                    let ids: Vec<i64> = entries.iter().map(|e| e.op_id).collect();
                                    let _ = match selected_target {
                                        SelectedTarget::Global(role) => {
                                            if let Ok(arc) = engine.router().get_global_clone(*role) {
                                                let conn = arc.lock().unwrap();
                                                mark_oplog_synced(&conn, &ids)
                                            } else {
                                                Ok(())
                                            }
                                        }
                                        SelectedTarget::Shard { key, role } => {
                                            if let Ok(arc) = engine.router().get_shard_conn(key, *role) {
                                                let conn = arc.lock().unwrap();
                                                mark_oplog_synced(&conn, &ids)
                                            } else {
                                                Ok(())
                                            }
                                        }
                                    };
                                    *toast = Some("Seluruh oplog ditandai tersinkronkan!".into());
                                }

                                if ui.button(RichText::new("🧹 Bersihkan Oplog Tersinkronkan (> 14 hari)").color(COLOR_TEXT_MUTED)).clicked() {
                                    let _ = match selected_target {
                                        SelectedTarget::Global(role) => {
                                            if let Ok(arc) = engine.router().get_global_clone(*role) {
                                                let conn = arc.lock().unwrap();
                                                purge_synced_oplog(&conn, 14)
                                            } else {
                                                Ok(0)
                                            }
                                        }
                                        SelectedTarget::Shard { key, role } => {
                                            if let Ok(arc) = engine.router().get_shard_conn(key, *role) {
                                                let conn = arc.lock().unwrap();
                                                purge_synced_oplog(&conn, 14)
                                            } else {
                                                Ok(0)
                                            }
                                        }
                                    };
                                    *toast = Some("Pembersihan oplog lama selesai.".into());
                                }
                            });
                        }
                        Ok(_) => {
                            ui.label(RichText::new("Belum ada catatan mutasi CDC di dalam oplog.").color(COLOR_TEXT_MUTED));
                        }
                        Err(e) => {
                            ui.label(RichText::new(format!("Tabel _shardvault_oplog belum diinisialisasi atau kosong: {}", e)).color(COLOR_TEXT_MUTED));
                        }
                    }
                });
        });
    }
}
