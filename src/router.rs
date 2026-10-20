// E:\shardvault-rs\src\router.rs
// Generic ShardRouter: Pengelola Multi-Shard, Replikasi 4-Peran, Self-Healing, & Windows Lock Immunity

use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::config::EngineConfig;
use crate::crypto::derive_machine_key;
use crate::error::ShardVaultError;
use crate::schema::{apply_pragma_settings, init_internal_global_tables, init_internal_shard_tables, SchemaProvider};
use crate::types::{GlobalTelemetry, ReplicaTelemetry, ShardRole, ShardTelemetry, SystemTelemetry};

pub type ShardConnectionMap = HashMap<(String, ShardRole), Arc<Mutex<Connection>>>;

pub struct ShardRouter {
    pub config: EngineConfig,
    pub master_key: [u8; 32],
    schema_provider: Arc<dyn SchemaProvider>,
    global_conn: Arc<Mutex<Connection>>,
    global_clones: Arc<Mutex<HashMap<ShardRole, Arc<Mutex<Connection>>>>>,
    shards: Arc<Mutex<ShardConnectionMap>>,
}

impl ShardRouter {
    /// Inisialisasi ShardRouter baru dengan konfigurasi dan skema kustom
    pub fn new(config: EngineConfig, schema_provider: Arc<dyn SchemaProvider>) -> Result<Self, ShardVaultError> {
        let master_key = derive_machine_key(&config.app_salt);

        // Buat struktur direktori jika belum ada
        let shards_dir = config.data_dir.join("shards");
        let clones_dir = config.data_dir.join("clones");
        let backups_dir = config.data_dir.join("backups");

        fs::create_dir_all(&shards_dir).map_err(|e| ShardVaultError::Io {
            path: shards_dir.clone(),
            source: e,
        })?;
        fs::create_dir_all(&clones_dir).map_err(|e| ShardVaultError::Io {
            path: clones_dir.clone(),
            source: e,
        })?;
        fs::create_dir_all(&backups_dir).map_err(|e| ShardVaultError::Io {
            path: backups_dir.clone(),
            source: e,
        })?;

        // 1. Inisialisasi global.db dengan proteksi Self-Healing Failover pada saat boot
        let global_path = config.data_dir.join("global.db");
        let global_standby_path = clones_dir.join("global_standby.db");

        let open_and_verify = |path: &Path| -> Result<Connection, ShardVaultError> {
            let conn = Connection::open(path).map_err(|e| ShardVaultError::Sqlite {
                shard_key: "global".into(),
                role: ShardRole::PrimaryWrite,
                source: e,
            })?;
            let status: String = conn
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .map_err(|e| ShardVaultError::Sqlite {
                    shard_key: "global".into(),
                    role: ShardRole::PrimaryWrite,
                    source: e,
                })?;
            if status != "ok" {
                return Err(ShardVaultError::CorruptPrimary {
                    shard_key: "global".into(),
                    reason: status,
                });
            }
            apply_pragma_settings(&conn, &config)?;
            init_internal_global_tables(&conn).map_err(|e| ShardVaultError::Sqlite {
                shard_key: "global".into(),
                role: ShardRole::PrimaryWrite,
                source: e,
            })?;
            schema_provider.init_global(&conn).map_err(|e| ShardVaultError::Sqlite {
                shard_key: "global".into(),
                role: ShardRole::PrimaryWrite,
                source: e,
            })?;
            Ok(conn)
        };

        let primary_res = open_and_verify(&global_path);
        let global_conn = match primary_res {
            Ok(conn) => Arc::new(Mutex::new(conn)),
            Err(err) => {
                if config.auto_heal_on_boot && global_standby_path.exists() {
                    eprintln!("[SHARDVAULT HEAL] Global primary corrupt: {}. Memulihkan dari hot standby...", err);
                    let ts = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let corrupt_dest = config.data_dir.join(format!("global.db.corrupted.{}", ts));
                    let _ = fs::rename(&global_path, &corrupt_dest);
                    fs::copy(&global_standby_path, &global_path).map_err(|e| ShardVaultError::Io {
                        path: global_path.clone(),
                        source: e,
                    })?;
                    let healed = open_and_verify(&global_path)?;
                    Arc::new(Mutex::new(healed))
                } else {
                    return Err(err);
                }
            }
        };

        // 2. Inisialisasi Replika Kloning Global (Read, Analytics, Standby)
        let global_clones_map = Arc::new(Mutex::new(HashMap::new()));
        let router = Self {
            config: config.clone(),
            master_key,
            schema_provider,
            global_conn,
            global_clones: global_clones_map,
            shards: Arc::new(Mutex::new(HashMap::new())),
        };

        for role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
            let clone_path = clones_dir.join(format!("global{}.db", role.suffix()));
            if !clone_path.exists() {
                let conn = Connection::open(&clone_path).map_err(|e| ShardVaultError::Sqlite {
                    shard_key: "global".into(),
                    role,
                    source: e,
                })?;
                apply_pragma_settings(&conn, &config)?;
                router.global_clones.lock().unwrap().insert(role, Arc::new(Mutex::new(conn)));
                let _ = router.sync_global_clone_replica(role);
            } else {
                let conn = Connection::open(&clone_path).map_err(|e| ShardVaultError::Sqlite {
                    shard_key: "global".into(),
                    role,
                    source: e,
                })?;
                apply_pragma_settings(&conn, &config)?;
                router.global_clones.lock().unwrap().insert(role, Arc::new(Mutex::new(conn)));
            }
        }

        Ok(router)
    }

    /// Ambil koneksi thread-safe ke Global Database Primary
    pub fn get_global_conn(&self) -> Result<Arc<Mutex<Connection>>, ShardVaultError> {
        Ok(Arc::clone(&self.global_conn))
    }

    /// Ambil koneksi thread-safe ke Replika Kloning Global sesuai perannya
    pub fn get_global_clone(&self, role: ShardRole) -> Result<Arc<Mutex<Connection>>, ShardVaultError> {
        if role == ShardRole::PrimaryWrite {
            return self.get_global_conn();
        }
        let map = self.global_clones.lock().unwrap();
        map.get(&role)
            .cloned()
            .ok_or_else(|| ShardVaultError::Other(format!("Global clone {:?} tidak ditemukan", role)))
    }

    /// Ambil atau buka koneksi thread-safe ke Shard Tenant/Partisi tertentu
    pub fn get_shard_conn<Q: ?Sized + AsRef<str>>(
        &self,
        shard_key: &Q,
        role: ShardRole,
    ) -> Result<Arc<Mutex<Connection>>, ShardVaultError> {
        let key_str = shard_key.as_ref().to_lowercase();
        let cache_key = (key_str.clone(), role);

        let mut shards_lock = self.shards.lock().unwrap();
        if let Some(conn) = shards_lock.get(&cache_key) {
            return Ok(Arc::clone(conn));
        }

        // Buka koneksi baru
        let folder = role.folder();
        let filename = format!("{}{}.db", key_str, role.suffix());
        let db_path = self.config.data_dir.join(folder).join(&filename);

        // Jika primary rusak saat dibuka, jalankan auto-heal jika diaktifkan
        if role == ShardRole::PrimaryWrite && db_path.exists() && self.config.auto_heal_on_boot {
            if let Ok(test_conn) = Connection::open(&db_path) {
                let status: Result<String, _> = test_conn.query_row("PRAGMA quick_check", [], |r| r.get(0));
                if let Ok(ref s) = status {
                    if s != "ok" {
                        drop(test_conn);
                        eprintln!("[SHARDVAULT HEAL] Shard '{}' corrupt ({}). Memulihkan...", key_str, s);
                        let standby_path = self.config.data_dir.join("clones").join(format!("{}_standby.db", key_str));
                        if standby_path.exists() {
                            let ts = SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let corrupt_dest = self.config.data_dir.join("shards").join(format!("{}.db.corrupted.{}", key_str, ts));
                            let _ = fs::rename(&db_path, &corrupt_dest);
                            let _ = fs::copy(&standby_path, &db_path);
                        }
                    }
                }
            }
        }

        let is_new = !db_path.exists();
        let conn = Connection::open(&db_path).map_err(|e| ShardVaultError::Sqlite {
            shard_key: key_str.clone(),
            role,
            source: e,
        })?;

        apply_pragma_settings(&conn, &self.config)?;

        if role == ShardRole::PrimaryWrite {
            init_internal_shard_tables(&conn).map_err(|e| ShardVaultError::Sqlite {
                shard_key: key_str.clone(),
                role,
                source: e,
            })?;
            self.schema_provider.init_shard(&key_str, &conn).map_err(|e| ShardVaultError::Sqlite {
                shard_key: key_str.clone(),
                role,
                source: e,
            })?;

            // Catat ke katalog global
            if is_new {
                if let Ok(global) = self.global_conn.lock() {
                    let _ = global.execute(
                        "INSERT OR IGNORE INTO _shardvault_shards (shard_key, display_name) VALUES (?1, ?2)",
                        rusqlite::params![key_str, format!("Shard {}", key_str)],
                    );
                }
            }
        }

        let conn_arc = Arc::new(Mutex::new(conn));
        shards_lock.insert(cache_key, Arc::clone(&conn_arc));

        // Jika ini shard baru dan role adalah PrimaryWrite, persiapkan 3 replikanya seketika
        if is_new && role == ShardRole::PrimaryWrite {
            drop(shards_lock);
            for sub_role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
                let _ = self.get_shard_conn(&key_str, sub_role);
                let _ = self.sync_clone_replica(&key_str, sub_role);
            }
            return self.get_shard_conn(&key_str, role);
        }

        Ok(conn_arc)
    }

    /// Sinkronisasi replika kloning spesifik shard menggunakan SQLite Online Backup API
    /// Dilengkapi INTEGRITY GUARD agar primary corrupt tidak meracuni standby
    pub fn sync_clone_replica<Q: ?Sized + AsRef<str>>(
        &self,
        shard_key: &Q,
        target_role: ShardRole,
    ) -> Result<(), ShardVaultError> {
        let key_str = shard_key.as_ref().to_lowercase();
        if target_role == ShardRole::PrimaryWrite {
            return Ok(());
        }

        let primary_arc = self.get_shard_conn(&key_str, ShardRole::PrimaryWrite)?;
        let target_arc = self.get_shard_conn(&key_str, target_role)?;

        let src_lock = primary_arc.lock().unwrap();

        // INTEGRITY GUARD
        if self.config.enable_integrity_guard {
            let status: String = src_lock
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .unwrap_or_else(|_| "query_failed".into());
            if status != "ok" {
                return Err(ShardVaultError::PoisoningPrevented {
                    shard_key: key_str,
                    reason: status,
                });
            }
        }

        let mut dst_lock = target_arc.lock().unwrap();
        let backup = rusqlite::backup::Backup::new(&src_lock, &mut dst_lock).map_err(|e| ShardVaultError::Sqlite {
            shard_key: key_str.clone(),
            role: target_role,
            source: e,
        })?;

        backup
            .run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|e| ShardVaultError::Sqlite {
                shard_key: key_str,
                role: target_role,
                source: e,
            })?;

        Ok(())
    }

    /// Sinkronisasi replika kloning global dengan INTEGRITY GUARD
    pub fn sync_global_clone_replica(&self, target_role: ShardRole) -> Result<(), ShardVaultError> {
        if target_role == ShardRole::PrimaryWrite {
            return Ok(());
        }

        let primary_arc = self.get_global_conn()?;
        let target_arc = self.get_global_clone(target_role)?;

        let src_lock = primary_arc.lock().unwrap();

        if self.config.enable_integrity_guard {
            let status: String = src_lock
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .unwrap_or_else(|_| "query_failed".into());
            if status != "ok" {
                return Err(ShardVaultError::PoisoningPrevented {
                    shard_key: "global".into(),
                    reason: status,
                });
            }
        }

        let mut dst_lock = target_arc.lock().unwrap();
        let backup = rusqlite::backup::Backup::new(&src_lock, &mut dst_lock).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "global".into(),
            role: target_role,
            source: e,
        })?;

        backup
            .run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|e| ShardVaultError::Sqlite {
                shard_key: "global".into(),
                role: target_role,
                source: e,
            })?;

        Ok(())
    }

    /// Sinkronkan seluruh replika aktif di seluruh sistem
    pub fn sync_all_clones(&self) -> Result<(), ShardVaultError> {
        for role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
            let _ = self.sync_global_clone_replica(role);
        }

        let shard_keys: Vec<String> = {
            let shards_lock = self.shards.lock().unwrap();
            let mut set = std::collections::HashSet::new();
            for (k, _) in shards_lock.keys() {
                set.insert(k.clone());
            }
            set.into_iter().collect()
        };

        for key in shard_keys {
            for role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
                let _ = self.sync_clone_replica(&key, role);
            }
        }

        Ok(())
    }

    /// Promosikan Hot Standby menjadi Primary Shard Baru (Failover Seketika)
    /// Dilengkapi proteksi Windows OS Error 1224 (eviction mmap handle sebelum file swap)
    pub fn promote_standby_to_primary<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<(), ShardVaultError> {
        let key_str = shard_key.as_ref().to_lowercase();
        let primary_path = self.config.data_dir.join("shards").join(format!("{}.db", key_str));
        let standby_path = self.config.data_dir.join("clones").join(format!("{}_standby.db", key_str));

        if !standby_path.exists() {
            return Err(ShardVaultError::StandbyUnavailable {
                shard_key: key_str,
                path: standby_path,
            });
        }

        // 1. Evakuasi handle aktif ke in-memory untuk melepaskan OS memory-mapped lock
        {
            let mut shards_lock = self.shards.lock().unwrap();
            if let Some(conn_arc) = shards_lock.get(&(key_str.clone(), ShardRole::PrimaryWrite)) {
                if let Ok(mut c) = conn_arc.lock() {
                    let _ = std::mem::replace(&mut *c, Connection::open_in_memory().unwrap());
                }
            }
            if let Some(conn_arc) = shards_lock.get(&(key_str.clone(), ShardRole::HotStandby)) {
                if let Ok(mut c) = conn_arc.lock() {
                    let _ = std::mem::replace(&mut *c, Connection::open_in_memory().unwrap());
                }
            }
            shards_lock.remove(&(key_str.clone(), ShardRole::PrimaryWrite));
            shards_lock.remove(&(key_str.clone(), ShardRole::HotStandby));
        }

        // 2. Karantina berkas primer lama
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let corrupt_dest = self.config.data_dir.join("shards").join(format!("{}.db.corrupted.{}", key_str, ts));
        if primary_path.exists() {
            let _ = fs::rename(&primary_path, &corrupt_dest);
        }

        // 3. Promosikan Standby ke Primary
        fs::copy(&standby_path, &primary_path).map_err(|e| ShardVaultError::Io {
            path: primary_path.clone(),
            source: e,
        })?;

        // 4. Buka kembali koneksi baru & sinkronkan ulang replika
        let _ = self.get_shard_conn(&key_str, ShardRole::PrimaryWrite)?;
        let _ = self.get_shard_conn(&key_str, ShardRole::HotStandby)?;
        self.sync_clone_replica(&key_str, ShardRole::HotStandby)?;
        self.sync_clone_replica(&key_str, ShardRole::FastRead)?;
        self.sync_clone_replica(&key_str, ShardRole::AnalyticsReport)?;

        Ok(())
    }

    /// Simulasi korupsi disk pada Shard Primary untuk keperluan Chaos Testing & Disaster Recovery Drill
    pub fn simulate_shard_corruption<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<(), ShardVaultError> {
        let key_str = shard_key.as_ref().to_lowercase();

        // 1. Evakuasi koneksi aktif dari cache
        {
            let mut shards_lock = self.shards.lock().unwrap();
            if let Some(conn_arc) = shards_lock.get(&(key_str.clone(), ShardRole::PrimaryWrite)) {
                if let Ok(mut c) = conn_arc.lock() {
                    let _ = std::mem::replace(&mut *c, Connection::open_in_memory().unwrap());
                }
            }
            shards_lock.remove(&(key_str.clone(), ShardRole::PrimaryWrite));
        }

        // 2. Tulis byte korupsi ke file primer
        let primary_path = self.config.data_dir.join("shards").join(format!("{}.db", key_str));
        if !primary_path.exists() {
            return Err(ShardVaultError::Other(format!("File shard primer {:?} tidak ditemukan", primary_path)));
        }

        use std::io::{Seek, SeekFrom, Write};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&primary_path)
            .map_err(|e| ShardVaultError::Io {
                path: primary_path.clone(),
                source: e,
            })?;

        file.seek(SeekFrom::Start(100)).map_err(|e| ShardVaultError::Io {
            path: primary_path.clone(),
            source: e,
        })?;
        file.write_all(b"CORRUPTED_CHAOS_TEST_BAD_BYTES_1234567890").map_err(|e| ShardVaultError::Io {
            path: primary_path.clone(),
            source: e,
        })?;
        file.flush().map_err(|e| ShardVaultError::Io {
            path: primary_path,
            source: e,
        })?;

        Ok(())
    }

    /// Kumpulkan telemetri status operasional sistem secara real-time
    pub fn get_telemetry(&self) -> Result<SystemTelemetry, ShardVaultError> {
        let global_path = self.config.data_dir.join("global.db");
        let global_size = fs::metadata(&global_path).map(|m| m.len()).unwrap_or(0);

        let mut global_replicas = Vec::new();
        for role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
            let path = self.config.data_dir.join("clones").join(format!("global{}.db", role.suffix()));
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            global_replicas.push(ReplicaTelemetry {
                role,
                filename: format!("global{}.db", role.suffix()),
                size_bytes: size,
                exists: path.exists(),
            });
        }

        // Tabel di global
        let mut global_counts = Vec::new();
        if let Ok(conn) = self.global_conn.lock() {
            if let Ok(count) = conn.query_row("SELECT COUNT(*) FROM _shardvault_shards", [], |r| r.get(0)) {
                global_counts.push(("_shardvault_shards".into(), count));
            }
        }

        let global_telemetry = GlobalTelemetry {
            primary_filename: "global.db".into(),
            primary_size_bytes: global_size,
            table_counts: global_counts,
            replicas: global_replicas,
        };

        // Kumpulkan data shard
        let mut shards_telemetry = Vec::new();
        let mut total_disk = global_size;
        for r in &global_telemetry.replicas {
            total_disk += r.size_bytes;
        }

        let shards_dir = self.config.data_dir.join("shards");
        if let Ok(entries) = fs::read_dir(shards_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    let file_stem = path.file_stem().unwrap().to_str().unwrap().to_string();
                    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    total_disk += size;

                    let is_corrupt = if let Ok(conn) = Connection::open(&path) {
                        let status: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap_or_else(|_| "err".into());
                        status != "ok"
                    } else {
                        true
                    };

                    let mut shard_replicas = Vec::new();
                    for role in [ShardRole::FastRead, ShardRole::AnalyticsReport, ShardRole::HotStandby] {
                        let rep_path = self.config.data_dir.join("clones").join(format!("{}{}.db", file_stem, role.suffix()));
                        let rep_size = fs::metadata(&rep_path).map(|m| m.len()).unwrap_or(0);
                        total_disk += rep_size;
                        shard_replicas.push(ReplicaTelemetry {
                            role,
                            filename: format!("{}{}.db", file_stem, role.suffix()),
                            size_bytes: rep_size,
                            exists: rep_path.exists(),
                        });
                    }

                    shards_telemetry.push(ShardTelemetry {
                        shard_key: file_stem.clone(),
                        primary_filename: format!("{}.db", file_stem),
                        primary_size_bytes: size,
                        table_counts: vec![],
                        replicas: shard_replicas,
                        is_corrupt,
                    });
                }
            }
        }

        Ok(SystemTelemetry {
            wal_mode_active: true,
            global: global_telemetry,
            shards: shards_telemetry,
            total_disk_bytes: total_disk,
            last_updated_rfc3339: chrono::Utc::now().to_rfc3339(),
        })
    }
}
