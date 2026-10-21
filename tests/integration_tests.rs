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

#[test]
fn test_schema_introspection_and_migration() {
    let tmp = tempdir().unwrap();
    let config = EngineBuilder::new(tmp.path()).build_config();
    let engine = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchemaProvider)).unwrap();

    // Pastikan shard 'makassar' dibuat
    engine
        .write_shard("makassar", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_mks_1', 'Daeng', 250000.0, 'enc_1', 'sig_1');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // 1. Introspeksi skema awal
    let manifest_before = engine.introspect_shard("makassar").unwrap();
    assert!(manifest_before.tables.iter().any(|t| t.name == "orders"));
    assert!(!manifest_before.tables.iter().any(|t| t.name == "loyalty_points"));

    // 2. Buat MigrationRunner dan tambahkan migrasi v1
    let mut runner = MigrationRunner::new();
    runner.add(Migration {
        version: 1,
        name: "create_loyalty_and_alter_orders",
        target: MigrationTarget::ShardsOnly,
        up_sql: "CREATE TABLE loyalty_points (user_id TEXT PRIMARY KEY, points INTEGER NOT NULL);
                 ALTER TABLE orders ADD COLUMN discount_percent REAL DEFAULT 0.0;",
        down_sql: None,
    });

    // 3. Jalankan migrasi pada seluruh cluster
    let reports = engine.run_migrations(&runner).unwrap();
    assert!(!reports.is_empty());
    assert!(reports.iter().any(|r| r.target_name == "makassar" && r.applied_versions.contains(&1)));

    // 4. Introspeksi skema sesudah migrasi
    let manifest_after = engine.introspect_shard("makassar").unwrap();
    assert!(manifest_after.tables.iter().any(|t| t.name == "loyalty_points"));
    let orders_table = manifest_after.tables.iter().find(|t| t.name == "orders").unwrap();
    assert!(orders_table.columns.iter().any(|c| c.name == "discount_percent"));

    // Verifikasi bahwa replika FastRead juga otomatis menerima DDL migrasi
    let count: i64 = engine
        .read_shard("makassar", |conn| {
            conn.query_row("SELECT COUNT(*) FROM loyalty_points;", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(count, 0);

    // 5. Verifikasi deteksi drift
    let drift = detect_schema_drift(&manifest_before, &manifest_after);
    assert!(!drift.is_empty(), "Harus terdeteksi perbedaan skema");
}

#[test]
fn test_database_transfer_package_roundtrip_and_tamper_rejection() {
    let tmp_src = tempdir().unwrap();
    let config_src = EngineBuilder::new(tmp_src.path())
        .with_salt("salt_source_machine")
        .build_config();
    let engine_src = DatabaseEngine::with_config_and_schema(config_src, Arc::new(RetailSchemaProvider)).unwrap();

    // Isi data di shard 'medan'
    engine_src
        .write_shard("medan", |conn| {
            conn.execute(
                "INSERT INTO orders VALUES ('ord_mdn_1', 'Rahmat', 300000.0, 'enc_cost', 'sig_token');",
                [],
            )?;
            conn.execute(
                "INSERT INTO orders VALUES ('ord_mdn_2', 'Dewi', 150000.0, 'enc_cost_2', 'sig_token_2');",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    let package_file = tmp_src.path().join("medan_export.svp");
    let transfer_password = "SuperSecretTransferPass2026!";

    // 1. Ekspor shard ke paket .svp
    let meta = engine_src
        .export_shard_package("medan", &package_file, transfer_password)
        .expect("Ekspor shard ke paket .svp harus sukses");

    assert_eq!(meta.shard_key, "medan");
    assert!(package_file.exists());

    // 2. Simulasi Mesin Kedua yang Berbeda (Target Node)
    let tmp_dst = tempdir().unwrap();
    let config_dst = EngineBuilder::new(tmp_dst.path())
        .with_salt("salt_destination_machine_different")
        .build_config();
    let engine_dst = DatabaseEngine::with_config_and_schema(config_dst, Arc::new(RetailSchemaProvider)).unwrap();

    // 3. Impor paket .svp ke mesin kedua sebagai shard 'medan_imported'
    let imported_meta = engine_dst
        .import_shard_package(&package_file, "medan_imported", transfer_password)
        .expect("Impor paket .svp harus sukses");

    assert_eq!(imported_meta.checksum_sha256, meta.checksum_sha256);

    // Verifikasi data terbaca utuh di mesin target melalui FastRead
    let count: i64 = engine_dst
        .read_shard("medan_imported", |conn| {
            conn.query_row("SELECT COUNT(*) FROM orders;", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(count, 2);

    // 4. Uji Proteksi Tampering: Ubah 1 byte di tengah file paket .svp
    let mut tampered_bytes = fs::read(&package_file).unwrap();
    let mid = tampered_bytes.len() / 2;
    tampered_bytes[mid] ^= 0xFF; // Flip bit
    let tampered_file = tmp_src.path().join("medan_tampered.svp");
    fs::write(&tampered_file, &tampered_bytes).unwrap();

    let tamper_res = engine_dst.import_shard_package(&tampered_file, "medan_corrupt", transfer_password);
    assert!(tamper_res.is_err(), "Paket yang dimanipulasi harus ditolak oleh HMAC!");
}

#[test]
fn test_distributed_sync_hlc_cdc_and_conflict_resolution() {
    let tmp_a = tempdir().unwrap();
    let tmp_b = tempdir().unwrap();

    let engine_a = DatabaseEngine::open(tmp_a.path()).unwrap();
    let engine_b = DatabaseEngine::open(tmp_b.path()).unwrap();

    let sync_a = engine_a.create_sync_engine("node_jakarta");
    let sync_b = engine_b.create_sync_engine("node_surabaya");

    // Buat tabel produk di kedua node
    for (eng, sync_engine) in [(&engine_a, &sync_a), (&engine_b, &sync_b)] {
        eng.write_shard("catalog", |conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    price REAL NOT NULL,
                    stock INTEGER NOT NULL
                );",
                [],
            )?;
            sync_engine
                .register_table_for_cdc(conn, "products")
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok(())
        })
        .unwrap();
    }

    // Node A menambahkan produk P1
    engine_a
        .write_shard("catalog", |conn| {
            conn.execute(
                "INSERT INTO products VALUES ('P1', 'Kemeja Katun', 120000.0, 50);",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    // 1. Node A menyiapkan Push Request
    let conn_a_arc = engine_a.router().get_shard_conn("catalog", ShardRole::PrimaryWrite).unwrap();
    let push_req = {
        let conn_a = conn_a_arc.lock().unwrap();
        sync_a.prepare_push_request(&conn_a, "catalog", 100).unwrap()
    };
    assert_eq!(push_req.operations.len(), 1);
    assert_eq!(push_req.operations[0].operation, "INSERT");

    // 2. Node B menerima dan memproses Push dari Node A
    let conn_b_arc = engine_b.router().get_shard_conn("catalog", ShardRole::PrimaryWrite).unwrap();
    {
        let mut conn_b = conn_b_arc.lock().unwrap();
        let push_res = sync_b.process_incoming_push(&mut conn_b, &push_req).unwrap();
        assert_eq!(push_res.applied_count, 1);
    }
    engine_b.sync_shard_clones("catalog").unwrap();

    // Node A menandai mutasi sebagai tersinkronkan
    {
        let conn_a = conn_a_arc.lock().unwrap();
        let op_ids: Vec<i64> = push_req.operations.iter().map(|o| o.op_id).collect();
        sync_a.acknowledge_push(&conn_a, &op_ids).unwrap();
    }

    // Verifikasi Node B memiliki produk P1
    let stock_b: i64 = engine_b
        .read_shard("catalog", |conn| {
            conn.query_row("SELECT stock FROM products WHERE id = 'P1';", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(stock_b, 50);

    // 3. Simulasi Konflik Last-Write-Wins (LWW) saat kedua node offline
    // Node A mengubah stok menjadi 40
    let ts_a = sync_a.hlc.now();
    let op_a = OplogEntry {
        op_id: 901,
        table_name: "products".into(),
        row_pk: "P1".into(),
        operation: "UPDATE".into(),
        hlc_timestamp: ts_a.to_canonical_string(),
        node_id: "node_jakarta".into(),
        payload_json: r#"{"id":"P1","name":"Kemeja Katun","price":120000.0,"stock":40}"#.into(),
        created_at_utc: "2026-10-21T10:00:00Z".into(),
        is_synced: false,
    };

    // Node B menghasilkan perubahan berikutnya dengan timestamp HLC lebih tinggi
    let mut ts_b = ts_a;
    ts_b.c += 5; // Memastikan HLC B lebih tinggi secara kausal
    let op_b = OplogEntry {
        op_id: 902,
        table_name: "products".into(),
        row_pk: "P1".into(),
        operation: "UPDATE".into(),
        hlc_timestamp: ts_b.to_canonical_string(),
        node_id: "node_surabaya".into(),
        payload_json: r#"{"id":"P1","name":"Kemeja Katun","price":120000.0,"stock":35}"#.into(),
        created_at_utc: "2026-10-21T10:00:05Z".into(),
        is_synced: false,
    };

    // Terapkan op_a di Node A
    {
        let mut conn_a = conn_a_arc.lock().unwrap();
        let push_a = SyncPushRequest {
            source_node_id: "node_jakarta".into(),
            shard_key: "catalog".into(),
            operations: vec![op_a.clone()],
        };
        let _ = sync_a.process_incoming_push(&mut conn_a, &push_a).unwrap();
    }

    // Sekarang coba terapkan op_a (lebih lama) ke Node B yang sudah memiliki op_b (lebih baru)
    {
        let mut conn_b = conn_b_arc.lock().unwrap();
        let push_b = SyncPushRequest {
            source_node_id: "node_surabaya".into(),
            shard_key: "catalog".into(),
            operations: vec![op_b],
        };
        let _ = sync_b.process_incoming_push(&mut conn_b, &push_b).unwrap();

        // Coba kirimkan op_a ke Node B: Harusnya diabaikan karena local op_b menang!
        let push_stale_a = SyncPushRequest {
            source_node_id: "node_jakarta".into(),
            shard_key: "catalog".into(),
            operations: vec![op_a],
        };
        let res_stale = sync_b.process_incoming_push(&mut conn_b, &push_stale_a).unwrap();
        assert_eq!(res_stale.ignored_count, 1, "Mutasi usang harus diabaikan oleh LWW!");
    }

    // Stok akhir di Node B harus tetap 35
    engine_b.sync_shard_clones("catalog").unwrap();
    let final_stock_b: i64 = engine_b
        .read_shard("catalog", |conn| {
            conn.query_row("SELECT stock FROM products WHERE id = 'P1';", [], |r| r.get(0))
        })
        .unwrap();
    assert_eq!(final_stock_b, 35);
}
