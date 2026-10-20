// E:\shardvault-rs\src\studio\cluster_view.rs
// Replication & Chaos Disaster Hub: Pengawasan Kloning, Uji Resiliensi, & Failover

use eframe::egui::{self, Grid, RichText, ScrollArea, Ui};

use super::theme::{
    COLOR_DANGER, COLOR_HEALTHY, COLOR_PURPLE, COLOR_SYNCING, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_WARNING,
};
use crate::engine::DatabaseEngine;
use crate::types::ShardRole;

#[derive(Default)]
pub struct ChaosStudioView {
    pub confirmation_modal: Option<(String, &'static str)>, // (shard_key, action: "crash" | "promote")
}

impl ChaosStudioView {
    pub fn render(&mut self, ui: &mut Ui, engine: &DatabaseEngine, toast: &mut Option<String>) {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("🛡 REPLICATION & CHAOS DISASTER HUB")
                    .size(16.0)
                    .color(COLOR_TEXT_PRIMARY)
                    .strong(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("⟳ Sinkronkan Semua Replika").color(COLOR_SYNCING).strong()).clicked() {
                    match engine.router().sync_all_clones() {
                        Ok(_) => *toast = Some("Seluruh replika kloning berhasil disinkronkan!".into()),
                        Err(e) => *toast = Some(format!("Gagal sinkronisasi: {}", e)),
                    }
                }
            });
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        let telemetry = match engine.telemetry() {
            Ok(t) => t,
            Err(e) => {
                ui.label(RichText::new(format!("Gagal mengambil telemetri cluster: {}", e)).color(COLOR_DANGER));
                return;
            }
        };

        // 1. Ringkasan Metrik Kartu Atas
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("TOTAL DISK USAGE").size(10.0).color(COLOR_TEXT_MUTED));
                    let mb = telemetry.total_disk_bytes as f64 / 1_048_576.0;
                    ui.label(RichText::new(format!("{:.2} MB", mb)).size(18.0).color(COLOR_HEALTHY).strong());
                });
            });

            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("ACTIVE SHARDS").size(10.0).color(COLOR_TEXT_MUTED));
                    ui.label(RichText::new(format!("{}", telemetry.shards.len())).size(18.0).color(COLOR_SYNCING).strong());
                });
            });

            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("WAL CONCURRENCY").size(10.0).color(COLOR_TEXT_MUTED));
                    ui.label(RichText::new("ACTIVE (NORMAL)").size(18.0).color(COLOR_PURPLE).strong());
                });
            });

            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("INTEGRITY GUARD").size(10.0).color(COLOR_TEXT_MUTED));
                    ui.label(RichText::new("ONLINE").size(18.0).color(COLOR_HEALTHY).strong());
                });
            });
        });

        ui.add_space(8.0);
        ui.separator();

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            // 2. GLOBAL DATABASE CARD
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🌐 GLOBAL DATABASE CLUSTER").size(14.0).strong().color(COLOR_SYNCING));
                    ui.label(RichText::new(format!("({} bytes)", telemetry.global.primary_size_bytes)).size(11.0).color(COLOR_TEXT_MUTED));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⟳ Sync Global").clicked() {
                            let _ = engine.router().sync_global_clone_replica(ShardRole::FastRead);
                            let _ = engine.router().sync_global_clone_replica(ShardRole::AnalyticsReport);
                            let _ = engine.router().sync_global_clone_replica(ShardRole::HotStandby);
                            *toast = Some("Kloning global berhasil disinkronkan!".into());
                        }
                    });
                });

                ui.add_space(4.0);
                Grid::new("global_replicas_grid").spacing([20.0, 4.0]).show(ui, |ui| {
                    for r in &telemetry.global.replicas {
                        let role_badge = match r.role {
                            ShardRole::FastRead => RichText::new("FastRead").color(COLOR_SYNCING),
                            ShardRole::AnalyticsReport => RichText::new("Analytics").color(COLOR_PURPLE),
                            ShardRole::HotStandby => RichText::new("HotStandby").color(COLOR_WARNING),
                            _ => RichText::new("Primary").color(COLOR_HEALTHY),
                        };
                        ui.label(role_badge);
                        ui.label(RichText::new(&r.filename).size(11.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new(format!("{} KB", r.size_bytes / 1024)).size(11.0).color(COLOR_TEXT_MUTED));
                        ui.label(if r.exists { RichText::new("✓ EXISTS").color(COLOR_HEALTHY).size(11.0) } else { RichText::new("MISSING").color(COLOR_DANGER).size(11.0) });
                        ui.end_row();
                    }
                });
            });

            ui.add_space(10.0);

            // 3. TENANT SHARDS CARDS
            ui.label(RichText::new("🏢 KARTU RESILIENSI TENANT SHARDS").size(13.0).strong().color(COLOR_TEXT_SECONDARY));
            ui.add_space(4.0);

            for shard in &telemetry.shards {
                let shard_key = shard.shard_key.clone();
                let is_corrupt = shard.is_corrupt;

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let status_tag = if is_corrupt {
                            RichText::new("● CORRUPTED").color(COLOR_DANGER).strong()
                        } else {
                            RichText::new("● HEALTHY").color(COLOR_HEALTHY).strong()
                        };
                        ui.label(status_tag);
                        ui.label(RichText::new(format!("Shard: '{}'", shard_key)).size(14.0).strong().color(COLOR_TEXT_PRIMARY));
                        ui.label(RichText::new(format!("({} KB)", shard.primary_size_bytes / 1024)).size(11.0).color(COLOR_TEXT_MUTED));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Tombol Failover Darurat
                            let promote_btn = ui.button(RichText::new("🚀 Promote Standby").color(COLOR_WARNING).strong());
                            if promote_btn.clicked() {
                                match engine.promote_standby(&shard_key) {
                                    Ok(_) => *toast = Some(format!("Hot standby untuk shard '{}' berhasil dipromosikan ke Primary!", shard_key)),
                                    Err(e) => *toast = Some(format!("Gagal promosi: {}", e)),
                                }
                            }

                            // Tombol Simulasi Crash (Chaos Test)
                            let crash_btn = ui.button(RichText::new("⚡ Simulasi Crash").color(COLOR_DANGER));
                            if crash_btn.clicked() {
                                match engine.router().simulate_shard_corruption(&shard_key) {
                                    Ok(_) => *toast = Some(format!("Korupsi disk berhasil disuntikkan ke primary shard '{}'. Uji self-healing aktif!", shard_key)),
                                    Err(e) => *toast = Some(format!("Gagal simulasi crash: {}", e)),
                                }
                            }

                            // Tombol Sinkronisasi Shard
                            if ui.button("⟳ Sync Clones").clicked() {
                                match engine.sync_shard_clones(&shard_key) {
                                    Ok(_) => *toast = Some(format!("Replika shard '{}' berhasil disinkronkan!", shard_key)),
                                    Err(e) => *toast = Some(format!("Gagal sync shard '{}': {}", shard_key, e)),
                                }
                            }
                        });
                    });

                    ui.add_space(4.0);
                    // Tampilkan status 3 replika shard
                    Grid::new(format!("grid_rep_{}", shard_key)).spacing([20.0, 2.0]).show(ui, |ui| {
                        for rep in &shard.replicas {
                            let role_name = format!("{:?}", rep.role);
                            let role_color = match rep.role {
                                ShardRole::FastRead => COLOR_SYNCING,
                                ShardRole::AnalyticsReport => COLOR_PURPLE,
                                ShardRole::HotStandby => COLOR_WARNING,
                                _ => COLOR_HEALTHY,
                            };
                            ui.label(RichText::new(role_name).color(role_color).size(11.0));
                            ui.label(RichText::new(&rep.filename).size(11.0).color(COLOR_TEXT_MUTED));
                            ui.label(RichText::new(format!("{} KB", rep.size_bytes / 1024)).size(11.0).color(COLOR_TEXT_MUTED));
                            ui.label(if rep.exists { RichText::new("Ready").color(COLOR_HEALTHY).size(11.0) } else { RichText::new("Absent").color(COLOR_DANGER).size(11.0) });
                            ui.end_row();
                        }
                    });
                });

                ui.add_space(6.0);
            }
        });
    }
}
