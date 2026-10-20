// E:\shardvault-rs\tests\integration_tests.rs
// Test Suite Komprehensif: Multi-Tenant Sharding, Replikasi 4-Peran, Kriptografi, & Chaos Failover

use rusqlite::Connection;
use shardvault_rs::prelude::*;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

// 1. Skema Kustom untuk Pengujian (Simulasi Aplikasi POS Kasir Retail)
struct RetailSchemaProvider;

impl SchemaProvider for RetailSchemaProvider {
    fn init_global(&self, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cashiers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                pin_hash TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }

    fn init_shard(&self, _shard_key: &str, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orders (
                id TEXT PRIMARY KEY,
                customer_name TEXT NOT NULL,
                total_amount REAL NOT NULL,
                cost_price_encrypted TEXT NOT NULL,
                signature_token TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }
}

#[test]
fn test_multi_tenant_sharding_and_facade() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path())
        .with_salt("retail_secret_salt")
        .build_config();

    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider))
        .expect("Inisialisasi DatabaseEngine harus sukses");

    // Tulis ke Global
    engine
        .write_global(|conn| {
            conn.execute(
                "INSERT INTO cashiers VALUES ('c1', 'Budi Santoso', 'hash_pin_123');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // Tulis ke Shard 'jakarta'
    engine
        .write_shard("jakarta", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_jkt_1', 'Andi', 500000.0, 'enc_cost_1', 'sig_1');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // Tulis ke Shard 'surabaya'
    engine
        .write_shard("surabaya", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_sby_1', 'Citra', 750000.0, 'enc_cost_2', 'sig_2');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // Sinkronkan ke replika FastRead
    engine.sync_shard_clones("jakarta").unwrap();
    engine.sync_shard_clones("surabaya").unwrap();

    // Verifikasi Isolasi Data
    let count_jkt: i64 = engine
        .read_shard("jakarta", |conn| {
            conn.query_row("SELECT COUNT(*) FROM orders", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(count_jkt, 1);

    let count_sby: i64 = engine
        .read_shard("surabaya", |conn| {
            conn.query_row("SELECT COUNT(*) FROM orders", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(count_sby, 1);
}

#[test]
fn test_4_role_replication_synchronization() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path()).build_config();
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider)).unwrap();

    // Tulis ke Primary
    engine
        .write_shard("medan", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_mdn_1', 'Dewi', 1200000.0, 'enc_val', 'sig_val');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // Sinkronkan ke seluruh 3 replika
    engine.sync_shard_clones("medan").unwrap();

    // Verifikasi data ada di FastRead
    let read_count: i64 = engine
        .read_shard("medan", |conn| {
            conn.query_row("SELECT COUNT(*) FROM orders WHERE id='ord_mdn_1'", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(read_count, 1, "Replika FastRead harus tersinkronisasi");

    // Verifikasi data ada di AnalyticsReport
    let ana_count: i64 = engine
        .analytics_shard("medan", |conn| {
            conn.query_row("SELECT COUNT(*) FROM orders WHERE id='ord_mdn_1'", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(ana_count, 1, "Replika AnalyticsReport harus tersinkronisasi");
}

#[test]
fn test_hardware_bound_encryption_and_tamper_proofing() {
    let tmp = tempdir().unwrap();
    let engine = DatabaseEngine::open(tmp.path()).unwrap();

    let confidential_data = "Kunci Brankas: 9812-7412 | HPP Bahan: Rp 1.250.000";

    // 1. Enkripsi AES-256-GCM
    let encrypted = engine.encrypt(confidential_data).unwrap();
    assert!(encrypted.starts_with("v1:"));
    assert_ne!(encrypted, confidential_data);

    // 2. Dekripsi
    let decrypted = engine.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, confidential_data);

    // 3. HMAC-SHA256 Signatures
    let transaction_record = "trans:TRX-9981:amount:250000:cashier:Budi";
    let signature = engine.sign(transaction_record);
    assert!(!signature.is_empty());

    assert!(engine.verify_signature(transaction_record, &signature));
    assert!(!engine.verify_signature("trans:TRX-9981:amount:1000:cashier:Budi", &signature));
}

#[test]
fn test_disaster_recovery_chaos_and_hot_standby_failover() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path()).build_config();
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider)).unwrap();

    // 1. Tulis data penting ke Shard 'bandung'
    engine
        .write_shard("bandung", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_bdg_vip', 'Hartono', 99000000.0, 'enc_val', 'sig_val');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // Sinkronkan ke Hot Standby
    engine.sync_shard_clones("bandung").unwrap();

    // 2. Simulasikan Korupsi Disk pada Primary Shard (Chaos Injection)
    engine.router().simulate_shard_corruption("bandung").unwrap();

    // 3. Jalankan Failover Darurat (Standby Promotion)
    let failover_res = engine.promote_standby("bandung");
    assert!(failover_res.is_ok(), "Promosi standby harus berhasil tanpa kendala");

    // 4. Verifikasi Data 100% Utuh (Zero Data Loss)
    let restored_total: f64 = engine
        .read_shard("bandung", |conn| {
            conn.query_row("SELECT total_amount FROM orders WHERE id='ord_bdg_vip'", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(restored_total, 99000000.0, "Data VIP harus pulih seutuhnya");

    // 5. Verifikasi berkas korup dipindahkan ke karantina
    let mut quarantined = false;
    for entry in fs::read_dir(tmp.path().join("shards")).unwrap().flatten() {
        if entry.file_name().to_string_lossy().contains("bandung.db.corrupted") {
            quarantined = true;
            break;
        }
    }
    assert!(quarantined, "File korup harus berada di karantina");
}

#[test]
fn test_integrity_guard_prevents_standby_poisoning() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path()).build_config();
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider)).unwrap();

    // Tulis data awal
    engine
        .write_shard("semarang", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_smg_1', 'Lina', 150000.0, 'enc_val', 'sig_val');",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    engine.sync_shard_clones("semarang").unwrap();

    // Korupsi file Primary
    engine.router().simulate_shard_corruption("semarang").unwrap();

    // Coba lakukan sinkronisasi dari primary yang korup
    let sync_res = engine.router().sync_clone_replica("semarang", ShardRole::HotStandby);
    assert!(sync_res.is_err(), "Sinkronisasi harus dibatalkan oleh Integrity Guard");

    // Pastikan Standby tetap sehat
    let standby_path = tmp.path().join("clones").join("semarang_standby.db");
    let test_conn = Connection::open(&standby_path).unwrap();
    let status: String = test_conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap();
    assert_eq!(status, "ok", "Hot standby harus tetap sehat dan tidak teracuni!");
}

#[test]
fn test_system_telemetry_reporting() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path()).build_config();
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider)).unwrap();

    engine
        .write_shard("denpasar", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_dps_1', 'Wayan', 420000.0, 'enc', 'sig');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    let telemetry = engine.telemetry().expect("Pengambilan telemetri harus sukses");
    assert!(telemetry.wal_mode_active);
    assert!(telemetry.total_disk_bytes > 0);
    assert_eq!(telemetry.global.primary_filename, "global.db");
    assert_eq!(telemetry.global.replicas.len(), 3);
    assert!(telemetry.shards.iter().any(|s| s.shard_key == "denpasar"));
}
