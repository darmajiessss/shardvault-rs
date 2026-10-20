// E:\shardvault-rs\examples\pos_store_demo.rs
//! Contoh Aplikasi Retail POS Multi-Cabang Menggunakan ShardVault-RS Engine

use rusqlite::Connection;
use shardvault_rs::prelude::*;
use std::sync::Arc;

struct PosStoreSchema;

impl SchemaProvider for PosStoreSchema {
    fn init_global(&self, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS system_users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                role TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }

    fn init_shard(&self, _shard_key: &str, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS branch_transactions (
                id TEXT PRIMARY KEY,
                invoice_number TEXT UNIQUE NOT NULL,
                total_amount REAL NOT NULL,
                encrypted_margin TEXT NOT NULL,
                signature_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== DEMO APLIKASI POS MULTI-CABANG DENGAN SHARDVAULT-RS ===");

    let temp_data_dir = tempfile::tempdir()?;
    let config = EngineBuilder::new(temp_data_dir.path())
        .with_salt("pos_super_secret_salt")
        .with_cache_size_mb(32)
        .with_mmap_size_mb(128)
        .build_config();

    // 1. Buka Engine
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(PosStoreSchema))?;
    println!("[OK] ShardVault-RS Engine berhasil diinisialisasi.");

    // 2. Transaksi Global: Daftarkan Kasir
    engine.write_global(|conn| {
        conn.execute(
            "INSERT INTO system_users VALUES ('usr_1', 'kasir_jakarta_01', 'CASHIER');",
            [],
        )?;
        Ok(())
    })?;
    println!("[OK] Pengguna global berhasil didaftarkan di global.db.");

    // 3. Transaksi Shard: Catat Penjualan di Cabang Jakarta
    let margin_secret = "Margin: 42.5% | Supplier: Pabrik Gorden Bandung";
    let encrypted_margin = engine.encrypt(margin_secret)?;
    let invoice_payload = "INV-JKT-001:500000:kasir_jakarta_01";
    let invoice_sig = engine.sign(invoice_payload);

    engine.write_shard("jakarta", |conn| {
        conn.execute(
            "INSERT INTO branch_transactions VALUES ('trx_1', 'INV-JKT-001', 500000.0, ?1, ?2, '2026-10-20T12:00:00Z');",
            rusqlite::params![encrypted_margin, invoice_sig],
        )?;
        Ok(())
    })?;
    println!("[OK] Transaksi berhasil dicatat di shard jakarta.db (PrimaryWrite).");

    // 4. Sinkronkan ke seluruh replika (FastRead, AnalyticsReport, HotStandby)
    engine.sync_shard_clones("jakarta")?;
    println!("[OK] Replikasi 3-kloning berhasil disinkronkan secara online.");

    // 5. Query cepat dari Replika FastRead
    let total_sales: f64 = engine.read_shard("jakarta", |conn| {
        conn.query_row("SELECT SUM(total_amount) FROM branch_transactions", [], |r| r.get(0))
    })?;
    println!("[OK] Total penjualan terbaca dari FastRead: Rp {:.2}", total_sales);

    // 6. Dekripsi Data Sensitif
    let decrypted = engine.decrypt(&encrypted_margin)?;
    println!("[OK] Margin finansial terenkripsi berhasil didekripsi: '{}'", decrypted);

    // 7. Ambil Telemetri Cluster
    let telemetry = engine.telemetry()?;
    println!(
        "[OK] Telemetri Cluster: Total Disk = {} bytes, Jumlah Shard Aktif = {}",
        telemetry.total_disk_bytes,
        telemetry.shards.len()
    );

    println!("\n=== SEMUA FITUR SHARDVAULT-RS BERJALAN SEMPURNA! ===");
    Ok(())
}
