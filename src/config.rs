// E:\shardvault-rs\src\config.rs
// Konfigurasi Engine & Builder Pattern

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub app_salt: String,
    pub cache_size_kb: i32,
    pub mmap_size_bytes: i64,
    pub busy_timeout_ms: u64,
    pub auto_heal_on_boot: bool,
    pub enable_integrity_guard: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            app_salt: "shardvault_default_salt".to_string(),
            cache_size_kb: -64000,          // 64 MB RAM Cache
            mmap_size_bytes: 268_435_456,   // 256 MB Memory Mapped I/O
            busy_timeout_ms: 5000,          // 5 Detik toleransi antrian lock
            auto_heal_on_boot: true,
            enable_integrity_guard: true,
        }
    }
}

pub struct EngineBuilder {
    config: EngineConfig,
}

impl EngineBuilder {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        let config = EngineConfig {
            data_dir: data_dir.as_ref().to_path_buf(),
            ..EngineConfig::default()
        };
        Self { config }
    }

    pub fn with_salt<S: Into<String>>(mut self, salt: S) -> Self {
        self.config.app_salt = salt.into();
        self
    }

    pub fn with_cache_size_mb(mut self, mb: u32) -> Self {
        self.config.cache_size_kb = -((mb as i32) * 1000);
        self
    }

    pub fn with_mmap_size_mb(mut self, mb: u32) -> Self {
        self.config.mmap_size_bytes = (mb as i64) * 1024 * 1024;
        self
    }

    pub fn with_busy_timeout_ms(mut self, ms: u64) -> Self {
        self.config.busy_timeout_ms = ms;
        self
    }

    pub fn with_auto_heal_on_boot(mut self, enable: bool) -> Self {
        self.config.auto_heal_on_boot = enable;
        self
    }

    pub fn with_integrity_guard(mut self, enable: bool) -> Self {
        self.config.enable_integrity_guard = enable;
        self
    }

    pub fn build_config(self) -> EngineConfig {
        self.config
    }
}
