// E:\shardvault-rs\src\bin\studio.rs
// Entry Point untuk Binary Desktop ShardVault Studio

use eframe::egui::ViewportBuilder;
use shardvault_rs::studio::StudioApp;
use std::env;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // Ambil path direktori data dari argumen CLI jika ada
    let args: Vec<String> = env::args().collect();
    let initial_data_dir = if args.len() > 1 && !args[1].starts_with('-') {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("ShardVault Studio — Dedicated Database Cockpit & Editor"),
        ..Default::default()
    };

    println!("========================================================");
    println!("⚡ SHARDVAULT STUDIO — Dedicated Database Cockpit & Editor");
    println!("========================================================");
    if let Some(ref p) = initial_data_dir {
        println!("Membuka direktori data: {:?}", p);
    } else {
        println!("Menggunakan default direktori data...");
    }

    eframe::run_native(
        "ShardVault Studio",
        native_options,
        Box::new(move |_cc| Ok(Box::new(StudioApp::new(initial_data_dir)))),
    )
}
