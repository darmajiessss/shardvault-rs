// E:\shardvault-rs\src\studio\sidebar.rs
// Cluster Tree Navigator: Panel Samping Penjelajah Global & Multi-Shard

use eframe::egui::{CollapsingHeader, RichText, ScrollArea, Ui};

use super::model::{fetch_tables, SelectedTarget, TableMetadata};
use super::theme::{
    COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::types::ShardRole;

#[derive(Default)]
pub struct SidebarView {
    pub new_shard_input: String,
    pub is_creating_shard: bool,
}

impl SidebarView {
    pub fn render(
        &mut self,
        ui: &mut Ui,
        engine: &Option<DatabaseEngine>,
        selected_target: &mut SelectedTarget,
        selected_table: &mut Option<String>,
        toast: &mut Option<String>,
    ) {
        ui.vertical(|ui| {
            // Header Navigator
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("📁 CLUSTER EXPLORER").size(14.0).color(COLOR_TEXT_PRIMARY).strong());
            });
            ui.add_space(6.0);
            ui.separator();

            let engine = match engine {
                Some(e) => e,
                None => {
                    ui.label(RichText::new("Engine belum terhubung.").color(COLOR_TEXT_MUTED));
                    return;
                }
            };

            ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                // 1. SECTION: GLOBAL DATABASE CLUSTER
                let global_header = CollapsingHeader::new(
                    RichText::new("🌐 GLOBAL CLUSTER")
                        .size(13.0)
                        .strong()
                        .color(COLOR_SYNCING),
                )
                .default_open(true);

                global_header.show(ui, |ui| {
                    for role in [
                        ShardRole::PrimaryWrite,
                        ShardRole::FastRead,
                        ShardRole::AnalyticsReport,
                        ShardRole::HotStandby,
                    ] {
                        let target = SelectedTarget::Global(role);
                        let is_active = *selected_target == target;

                        let role_color = match role {
                            ShardRole::PrimaryWrite => COLOR_HEALTHY,
                            ShardRole::FastRead => COLOR_SYNCING,
                            ShardRole::AnalyticsReport => COLOR_PURPLE,
                            ShardRole::HotStandby => COLOR_WARNING,
                        };

                        let label_text = format!("📄 global{} [{}]", role.suffix(), role);
                        let text = if is_active {
                            RichText::new(label_text).color(role_color).strong()
                        } else {
                            RichText::new(label_text).color(COLOR_TEXT_SECONDARY)
                        };

                        if ui.selectable_label(is_active, text).clicked() {
                            *selected_target = target.clone();
                        }

                        // Jika node ini yang dipilih, tampilkan daftar tabelnya di bawah
                        if is_active {
                            ui.indent("global_tables", |ui| {
                                let tables: Vec<TableMetadata> = match role {
                                    ShardRole::PrimaryWrite => {
                                        engine.write_global(|conn| Ok(fetch_tables(conn))).unwrap_or_default()
                                    }
                                    _ => {
                                        if let Ok(arc) = engine.router().get_global_clone(role) {
                                            let conn = arc.lock().unwrap();
                                            fetch_tables(&conn)
                                        } else {
                                            vec![]
                                        }
                                    }
                                };

                                if tables.is_empty() {
                                    ui.label(RichText::new("  (Tidak ada tabel)").size(11.0).color(COLOR_TEXT_MUTED));
                                } else {
                                    for t in tables {
                                        let is_tbl_active = selected_table.as_deref() == Some(&t.name);
                                        let tbl_label = format!("  ⊞ {} ({})", t.name, t.row_count);
                                        let item = if is_tbl_active {
                                            RichText::new(tbl_label).color(COLOR_TEXT_PRIMARY).strong()
                                        } else {
                                            RichText::new(tbl_label).color(COLOR_TEXT_MUTED)
                                        };

                                        if ui.selectable_label(is_tbl_active, item).clicked() {
                                            *selected_table = Some(t.name.clone());
                                        }
                                    }
                                }
                            });
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // 2. SECTION: TENANT SHARDS CLUSTER
                let shards_header = CollapsingHeader::new(
                    RichText::new("🏢 TENANT SHARDS")
                        .size(13.0)
                        .strong()
                        .color(COLOR_HEALTHY),
                )
                .default_open(true);

                shards_header.show(ui, |ui| {
                    let telemetry = engine.telemetry().ok();
                    let shard_keys: Vec<String> = telemetry
                        .map(|t| t.shards.into_iter().map(|s| s.shard_key).collect())
                        .unwrap_or_default();

                    if shard_keys.is_empty() {
                        ui.label(RichText::new("(Belum ada shard aktif)").color(COLOR_TEXT_MUTED).size(12.0));
                    }

                    for shard_key in shard_keys {
                        let is_shard_expanded = selected_target.shard_key() == Some(&shard_key);
                        let header_label = format!("📦 shard: {}", shard_key);

                        CollapsingHeader::new(
                            RichText::new(header_label)
                                .size(12.0)
                                .strong()
                                .color(if is_shard_expanded { COLOR_HEALTHY } else { COLOR_TEXT_PRIMARY }),
                        )
                        .default_open(is_shard_expanded)
                        .show(ui, |ui| {
                            for role in [
                                ShardRole::PrimaryWrite,
                                ShardRole::FastRead,
                                ShardRole::AnalyticsReport,
                                ShardRole::HotStandby,
                            ] {
                                let target = SelectedTarget::Shard {
                                    key: shard_key.clone(),
                                    role,
                                };
                                let is_active = *selected_target == target;

                                let role_color = match role {
                                    ShardRole::PrimaryWrite => COLOR_HEALTHY,
                                    ShardRole::FastRead => COLOR_SYNCING,
                                    ShardRole::AnalyticsReport => COLOR_PURPLE,
                                    ShardRole::HotStandby => COLOR_WARNING,
                                };

                                let label_text = format!("📄 {}{} [{}]", shard_key, role.suffix(), role);
                                let text = if is_active {
                                    RichText::new(label_text).color(role_color).strong()
                                } else {
                                    RichText::new(label_text).color(COLOR_TEXT_SECONDARY)
                                };

                                if ui.selectable_label(is_active, text).clicked() {
                                    *selected_target = target.clone();
                                }

                                if is_active {
                                    ui.indent(format!("{}_tables", shard_key), |ui| {
                                        let tables = if let Ok(arc) = engine.router().get_shard_conn(&shard_key, role) {
                                            let conn = arc.lock().unwrap();
                                            fetch_tables(&conn)
                                        } else {
                                            vec![]
                                        };

                                        if tables.is_empty() {
                                            ui.label(RichText::new("  (Tidak ada tabel)").size(11.0).color(COLOR_TEXT_MUTED));
                                        } else {
                                            for t in tables {
                                                let is_tbl_active = selected_table.as_deref() == Some(&t.name);
                                                let tbl_label = format!("  ⊞ {} ({})", t.name, t.row_count);
                                                let item = if is_tbl_active {
                                                    RichText::new(tbl_label).color(COLOR_TEXT_PRIMARY).strong()
                                                } else {
                                                    RichText::new(tbl_label).color(COLOR_TEXT_MUTED)
                                                };

                                                if ui.selectable_label(is_tbl_active, item).clicked() {
                                                    *selected_table = Some(t.name.clone());
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        });
                    }
                });

                ui.add_space(12.0);
                ui.separator();

                // 3. ACTIONS FOOTER: TAMBAH SHARD & SINKRONISASI
                ui.add_space(4.0);
                if self.is_creating_shard {
                    ui.group(|ui| {
                        ui.label(RichText::new("Nama Shard Baru:").size(11.0));
                        ui.text_edit_singleline(&mut self.new_shard_input);
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("Buat").strong().color(COLOR_HEALTHY)).clicked() {
                                let key = self.new_shard_input.trim().to_lowercase();
                                if !key.is_empty() {
                                    let _ = engine.write_shard(&key, |_conn| Ok(()));
                                    let _ = engine.sync_shard_clones(&key);
                                    *toast = Some(format!("Shard '{}' berhasil dibuat dengan 3 replikanya!", key));
                                    self.new_shard_input.clear();
                                    self.is_creating_shard = false;
                                }
                            }
                            if ui.button("Batal").clicked() {
                                self.is_creating_shard = false;
                            }
                        });
                    });
                } else {
                    if ui.button(RichText::new("➕ Shard Baru").size(12.0)).clicked() {
                        self.is_creating_shard = true;
                    }
                }

                ui.add_space(6.0);
                if ui.button(RichText::new("⟳ Sinkronkan Semua Replika").size(11.0).color(COLOR_SYNCING)).clicked() {
                    match engine.router().sync_all_clones() {
                        Ok(_) => *toast = Some("Seluruh replika kloning berhasil disinkronkan!".into()),
                        Err(e) => *toast = Some(format!("Gagal sinkronisasi: {}", e)),
                    }
                }
            });
        });
    }
}
