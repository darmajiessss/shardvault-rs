// E:\shardvault-rs\src\engine.rs
// High-Level Facade & Public Developer Interface: DatabaseEngine

use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

use crate::config::{EngineBuilder, EngineConfig};
use crate::crypto::{decrypt_string, encrypt_string, generate_signature, verify_signature};
use crate::error::ShardVaultError;
use crate::router::ShardRouter;
use crate::schema::{EmptySchemaProvider, SchemaProvider};
use crate::types::{ShardRole, SystemTelemetry};

/// Facade Utama Pengelola Database ShardVault-RS
pub struct DatabaseEngine {
    router: ShardRouter,
}

impl DatabaseEngine {
    /// Inisialisasi engine langsung dari direktori data menggunakan pengaturan standar
    pub fn open<P: AsRef<Path>>(data_dir: P) -> Result<Self, ShardVaultError> {
        let config = EngineBuilder::new(data_dir).build_config();
        Self::with_config_and_schema(config, Arc::new(EmptySchemaProvider))
    }

    /// Inisialisasi engine dengan konfigurasi dan skema kustom
    pub fn with_config_and_schema(
        config: EngineConfig,
        schema_provider: Arc<dyn SchemaProvider>,
    ) -> Result<Self, ShardVaultError> {
        let router = ShardRouter::new(config, schema_provider)?;
        Ok(Self { router })
    }

    /// Akses router internal untuk operasi tingkat rendah
    pub fn router(&self) -> &ShardRouter {
        &self.router
    }

    // --- Operasi Transaksional Global ---

    /// Eksekusi transaksi tulis pada Global Database Primary
    pub fn write_global<F, R>(&self, f: F) -> Result<R, ShardVaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let conn_arc = self.router.get_global_conn()?;
        let conn = conn_arc.lock().unwrap();
        f(&conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "global".into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })
    }

    /// Eksekusi query baca pada Replika FastRead Global
    pub fn read_global<F, R>(&self, f: F) -> Result<R, ShardVaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let conn_arc = self.router.get_global_clone(ShardRole::FastRead)?;
        let conn = conn_arc.lock().unwrap();
        f(&conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: "global".into(),
            role: ShardRole::FastRead,
            source: e,
        })
    }

    // --- Operasi Transaksional Shard ---

    /// Eksekusi transaksi tulis pada Shard Primary spesifik
    pub fn write_shard<Q: ?Sized + AsRef<str>, F, R>(&self, shard_key: &Q, f: F) -> Result<R, ShardVaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let key_str = shard_key.as_ref();
        let conn_arc = self.router.get_shard_conn(key_str, ShardRole::PrimaryWrite)?;
        let conn = conn_arc.lock().unwrap();
        f(&conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: key_str.into(),
            role: ShardRole::PrimaryWrite,
            source: e,
        })
    }

    /// Eksekusi query pembacaan latensi rendah pada Replika FastRead Shard spesifik
    pub fn read_shard<Q: ?Sized + AsRef<str>, F, R>(&self, shard_key: &Q, f: F) -> Result<R, ShardVaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let key_str = shard_key.as_ref();
        let conn_arc = self.router.get_shard_conn(key_str, ShardRole::FastRead)?;
        let conn = conn_arc.lock().unwrap();
        f(&conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: key_str.into(),
            role: ShardRole::FastRead,
            source: e,
        })
    }

    /// Eksekusi query agregasi / laporan berat pada Replika Analytics Shard spesifik
    pub fn analytics_shard<Q: ?Sized + AsRef<str>, F, R>(&self, shard_key: &Q, f: F) -> Result<R, ShardVaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let key_str = shard_key.as_ref();
        let conn_arc = self.router.get_shard_conn(key_str, ShardRole::AnalyticsReport)?;
        let conn = conn_arc.lock().unwrap();
        f(&conn).map_err(|e| ShardVaultError::Sqlite {
            shard_key: key_str.into(),
            role: ShardRole::AnalyticsReport,
            source: e,
        })
    }

    // --- Operasi Replikasi & Pemulihan Bencana ---

    /// Ambil daftar seluruh shard aktif di cluster
    pub fn list_active_shards(&self) -> Result<Vec<String>, ShardVaultError> {
        self.router.list_active_shards()
    }

    /// Sinkronkan seluruh 3 replika (FastRead, Analytics, Standby) untuk Global Database
    pub fn sync_global_clones(&self) -> Result<(), ShardVaultError> {
        self.router.sync_global_clones()
    }

    /// Sinkronkan seluruh 3 replika (FastRead, Analytics, Standby) untuk shard spesifik
    pub fn sync_shard_clones<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<(), ShardVaultError> {
        let key_str = shard_key.as_ref();
        self.router.sync_clone_replica(key_str, ShardRole::FastRead)?;
        self.router.sync_clone_replica(key_str, ShardRole::AnalyticsReport)?;
        self.router.sync_clone_replica(key_str, ShardRole::HotStandby)?;
        Ok(())
    }

    /// Sinkronkan seluruh replika aktif di seluruh sistem (Global + Seluruh Shards)
    pub fn sync_all_clones(&self) -> Result<(), ShardVaultError> {
        self.router.sync_all_clones()
    }

    /// Introspeksi skema Global Database
    pub fn introspect_global(&self) -> Result<crate::migration::SchemaManifest, ShardVaultError> {
        let conn_arc = self.router.get_global_conn()?;
        let conn = conn_arc.lock().unwrap();
        crate::migration::introspect_schema(&conn)
    }

    /// Introspeksi skema Shard spesifik
    pub fn introspect_shard<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<crate::migration::SchemaManifest, ShardVaultError> {
        let conn_arc = self.router.get_shard_conn(shard_key, ShardRole::PrimaryWrite)?;
        let conn = conn_arc.lock().unwrap();
        crate::migration::introspect_schema(&conn)
    }

    /// Jalankan rangkaian migrasi skema ke seluruh cluster (Global + Seluruh Shard Aktif)
    pub fn run_migrations(&self, runner: &crate::migration::MigrationRunner) -> Result<Vec<crate::migration::MigrationReport>, ShardVaultError> {
        runner.run_cluster(self)
    }

    /// Promosikan Hot Standby menjadi Primary Shard Baru (Failover Seketika)
    pub fn promote_standby<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<(), ShardVaultError> {
        self.router.promote_standby_to_primary(shard_key)
    }

    // --- Primitif Kriptografi ---

    /// Enkripsi teks rahasia menggunakan AES-256-GCM dengan kunci master terikat mesin
    pub fn encrypt(&self, plaintext: &str) -> Result<String, ShardVaultError> {
        encrypt_string(plaintext, &self.router.master_key)
    }

    /// Dekripsi teks rahasia AES-256-GCM
    pub fn decrypt(&self, ciphertext: &str) -> Result<String, ShardVaultError> {
        decrypt_string(ciphertext, &self.router.master_key)
    }

    /// Hasilkan tanda tangan anti-tamper HMAC-SHA256
    pub fn sign(&self, payload: &str) -> String {
        generate_signature(payload, &self.router.master_key)
    }

    /// Verifikasi tanda tangan anti-tamper HMAC-SHA256
    pub fn verify_signature(&self, payload: &str, expected_sig: &str) -> bool {
        verify_signature(payload, expected_sig, &self.router.master_key)
    }

    /// Ekspor shard tertentu ke berkas paket portabel terenkripsi (.svp)
    pub fn export_shard_package<P: AsRef<Path>>(
        &self,
        shard_key: &str,
        dest_path: P,
        passphrase: &str,
    ) -> Result<crate::transfer::PackageMetadata, ShardVaultError> {
        crate::transfer::export_shard_to_package(self, shard_key, dest_path, passphrase)
    }

    /// Impor berkas paket portabel (.svp) ke dalam cluster sebagai shard target
    pub fn import_shard_package<P: AsRef<Path>>(
        &self,
        package_path: P,
        target_shard_key: &str,
        passphrase: &str,
    ) -> Result<crate::transfer::PackageMetadata, ShardVaultError> {
        crate::transfer::import_package_to_shard(self, package_path, target_shard_key, passphrase)
    }

    /// Buat instance SyncEngine untuk sinkronisasi terdistribusi antar node
    pub fn create_sync_engine(&self, node_id: &str) -> crate::sync::SyncEngine {
        crate::sync::SyncEngine::new(node_id.to_string())
    }

    // --- Telemetri Sistem ---

    /// Kumpulkan metrik telemetri seluruh cluster
    pub fn telemetry(&self) -> Result<SystemTelemetry, ShardVaultError> {
        self.router.get_telemetry()
    }
}
