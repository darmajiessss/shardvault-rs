# ⚡ ShardVault-RS

> **Ultra-Fast, Self-Healing, Hardware-Bound Multi-Shard SQLite Engine & Studio for Edge, Desktop, and Distributed Local Applications.**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-edition%202021-orange.svg)](https://www.rust-lang.org/)
[![SQLite](https://img.shields.io/badge/sqlite-WAL%20Mode-003B57.svg)](https://sqlite.org/)
[![GUI](https://img.shields.io/badge/studio-egui%20%2F%20eframe-2b5c8f.svg)](#-shardvault-studio-desktop-editor)
[![Status](https://img.shields.io/badge/tests-12%2F12%20passed%20(100%25)-brightgreen.svg)](#-testing--chaos-verification)

---

## 🌟 Overview

Traditional SQLite is fast and easy to embed, but runs into hard limits when applications scale:
1. **Single Monolithic File**: One bloated `.db` file holds all tables; if it corrupts, the whole system goes down.
2. **Write Contention**: Heavy FTS5 search or bulk reporting can cause `database is locked` (`SQLITE_BUSY`) errors for cashier / critical write transactions.
3. **No Automatic Disaster Recovery**: If disk pages corrupt, standard SQLite crashes without automated hot-standby failover.
4. **Tenant Data Leakage**: Mixing multiple branches or tenants in single tables with `tenant_id` filters creates data leakage risks.

**ShardVault-RS** solves this by abstracting a battle-tested multi-shard SQLite architecture into an elegant, reusable Rust engine with its own native desktop studio.

---

## ✨ Key Features

- **Dynamic Horizontal Sharding (`K: ShardKey`)**: Automatically isolates data into separate physical SQLite files per tenant, branch, store, or time partition.
- **4-Role Replication Topology**:
  - `PrimaryWrite`: Transaction-optimized with WAL mode (Zero-Lock write path).
  - `FastRead`: High-throughput, read-only replica for fast queries and FTS5 search.
  - `AnalyticsReport`: Dedicated replica for heavy aggregate queries and PDF/CSV export.
  - `HotStandby`: Mirror replica ready for instant disaster recovery failover.
- **Online Backup Replication with Integrity Guard**: Synchronizes replicas on-the-fly via SQLite's native Online Backup API (`sqlite3_backup_*`), protected by a pre-sync `PRAGMA quick_check` guard to prevent corrupted primaries from poisoning healthy standbys.
- **Instant Chaos Self-Healing (<50ms RTO)**: Automated startup quarantine (`.corrupted.<timestamp>`) and zero-downtime hot standby promotion, fully immune to Windows memory-mapped file lock conflicts (**OS Error 1224**).
- **Structure & Universal Migration Engine (`shardvault::migration`)**:
  - Declarative versioned migrations across Global DB and all active Shards in atomic transactions.
  - Runtime DDL introspection (`PRAGMA table_info`, `sqlite_master`) and SHA-256 schema hash comparison to detect *Schema Drift*.
  - Immediate auto-propagation of structural DDL changes to all replica clones (`_read`, `_analytics`, `_standby`).
- **Secure Database Transfer Package (`.svp` - `shardvault::transfer`)**:
  - Encrypted, portable database bundles combining SQLite snapshots with **Envelope Encryption** (DEK/KEK via PBKDF2 + AES-256-GCM).
  - High-ratio **Zstandard (`zstd`)** compression and **HMAC-SHA256** digital integrity verification.
  - Seamless cross-machine re-keying to adopt hardware-bound encryption keys on new nodes.
- **Distributed Data Synchronization (`shardvault::sync`)**:
  - **Hybrid Logical Clock (HLC)** generator (Kulkarni et al. algorithm) guaranteeing strict monotonic causality across physical clock skews.
  - **Change Data Capture (CDC)** via automated SQLite triggers recording mutational streams to `_shardvault_oplog` with **Tombstone** deletion protocol.
  - **Conflict Resolution Engine**: Deterministic Row-Level & Field-Level Last-Write-Wins (LWW) merging for edge-to-cloud and multi-master topologies.
- **Hardware-Bound Zero-Trust Security**:
  - **AES-256-GCM** authenticated column encryption with random 12-byte nonces.
  - **PBKDF2-HMAC-SHA256** key derivation bound to machine hardware identifiers.
  - **HMAC-SHA256** tamper-evident integrity chains for out-of-band edit detection.
- **Desktop Database Cockpit ([`ShardVault Studio`](#shardvault-studio-desktop-editor))**: Native GPU-accelerated desktop database browser, live AES decryptor, multi-target SQL console, migration manager, `.svp` transfer wizard, and CDC oplog synchronization hub.

---

## 🚀 Quick Start (Using in Any Rust Project)

### 1. Add ShardVault-RS to your `Cargo.toml`
```toml
[dependencies]
shardvault-rs = { path = "path/to/shardvault-rs" }
rusqlite = { version = "0.31", features = ["bundled"] }
```

### 2. Define Your Application Schema & Query
```rust
use rusqlite::Connection;
use shardvault_rs::prelude::*;
use std::path::Path;
use std::sync::Arc;

// 1. Define custom tables for Global and Shard databases
struct RetailSchema;

impl SchemaProvider for RetailSchema {
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
                customer TEXT NOT NULL,
                total REAL NOT NULL,
                cost_enc TEXT NOT NULL,
                sig TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Build the Engine
    let config = EngineBuilder::new(Path::new("./data"))
        .with_salt("my_app_secret_salt")
        .with_cache_size_mb(64)
        .with_mmap_size_mb(256)
        .build_config();

    let db = DatabaseEngine::with_config_and_schema(config, Arc::new(RetailSchema))?;

    // 3. Write Transaction to a Specific Shard (e.g., "jakarta")
    let secret_cost = db.encrypt("HPP: Rp 450.000")?;
    let token = db.sign("ord_1:Budi:500000");

    db.write_shard("jakarta", |conn| {
        conn.execute(
            "INSERT INTO orders VALUES ('ord_1', 'Budi', 500000.0, ?1, ?2);",
            rusqlite::params![secret_cost, token],
        )?;
        Ok(())
    })?;

    // 4. Synchronize 3 Replicas (FastRead, Analytics, Standby)
    db.sync_shard_clones("jakarta")?;

    // 5. Query from FastRead Replica (Non-blocking)
    let total: f64 = db.read_shard("jakarta", |conn| {
        conn.query_row("SELECT SUM(total) FROM orders;", [], |r| r.get(0))
    })?;
    println!("Total Sales: Rp {:.2}", total);

    // 6. Decrypt Sensitive Field
    let cost_decrypted = db.decrypt(&secret_cost)?;
    println!("Decrypted Field: {}", cost_decrypted);

    Ok(())
}
```

---

## 🖥️ ShardVault Studio (Desktop Editor)

`shardvault-studio` is a native, GPU-accelerated desktop cockpit built with `egui`/`eframe`.

```powershell
# Launch ShardVault Studio
cargo run --bin shardvault-studio -- "./data"
```

### Studio Capabilities:
1. **Cluster Explorer Tree**: Browse Global DB and Tenant Shards with 4 replica nodes per database and real-time row counts.
2. **Table Data Browser & Live Decryptor**:
   - Filter and view rows.
   - **🔓 One-Click Decrypt Toggle**: Real-time decryption of AES-256-GCM columns (`v1:...`) using the machine master key.
   - **🛡 Tamper Detection**: Instant visual flags (`⚠ TAMPER`) if rows were altered externally.
   - **Row Detail Inspector**: Inspect complex JSON fields in a dedicated drawer.
3. **Interactive SQL Console**:
   - Multi-target selector: run queries on Global or any Shard, directed to `PrimaryWrite`, `FastRead`, or `AnalyticsReport`.
   - Monospace editor with `Ctrl + Enter` execution.
   - Precision latency measurement (in milliseconds) and CSV export.
4. **Schema DDL Inspector**: Inspect `CREATE TABLE` DDL, column definitions, and active indexes.
5. **Replication & Chaos Disaster Hub**:
   - Live cluster telemetry (disk usage, active shards, WAL status).
   - One-click online clone replication.
   - **⚡ Chaos Simulation**: Injects corrupt bytes into primary to test automated healing.
   - **🚀 Emergency Hot Standby Promotion**: Zero-downtime failover with zero data loss.
6. **🚀 Schema Migrations & Drift Detector**:
   - Inspect applied migrations and SHA-256 checksums from `_shardvault_migrations`.
   - Execute cluster-wide DDL migrations with automated clone synchronization.
   - Compare schemas between Global and Shards to detect schema deviations.
7. **📦 Database Transfer Wizard (.svp)**:
   - Export any shard to a portable, password-protected `.svp` package with Zstd compression.
   - Import `.svp` packages onto new nodes with automated hardware re-keying.
8. **🔄 Distributed Sync Center & Oplog Monitor**:
   - Live stream of mutational CDC events from `_shardvault_oplog`.
   - Real-time Hybrid Logical Clock (HLC) monitor.
   - One-click trigger CDC setup, manual sync acknowledgement, and log compaction.

---

## 📂 Storage Architecture & Layout

```
<DATA_DIR>/
├── global.db                      # Primary: Global Metadata, Users, Audit Logs
├── shards/                        # Active Primary Shards (Write-Optimized)
│   ├── store_jakarta.db
│   ├── store_surabaya.db
│   └── ...
├── clones/                        # Workload-Specific Replicas
│   ├── global_read.db
│   ├── global_analytics.db
│   ├── global_standby.db
│   ├── store_jakarta_read.db
│   ├── store_jakarta_analytics.db
│   ├── store_jakarta_standby.db
│   └── ...
└── backups/                       # Snapshot Manifests & Pre-Migration Backups
```

---

## 🧪 Testing & Chaos Verification

All tests run fully self-contained using isolated temporary directories:

```powershell
# Run all unit and integration tests (including chaos crash & failover tests)
cargo test

# Run Clippy quality gate (Zero Warnings Policy)
cargo clippy -- -D warnings
```

---

## 📖 In-Depth Documentation

For detailed step-by-step instructions on incorporating ShardVault into your projects:
- **[How to Use ShardVault in Your Project](docs/HOW_TO_USE.md)**
- **[Panduan Lengkap: Sharding & Replikasi Multi-Clone](docs/TUTORIAL_SHARDING_DAN_CLONE.md)**

---

## 📄 License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.
