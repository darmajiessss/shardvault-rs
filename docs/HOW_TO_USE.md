# Complete Guide: How to Use ShardVault-RS in Your Project

This document provides a comprehensive, step-by-step tutorial on integrating **ShardVault-RS** into any new or existing Rust application (Point of Sale, Desktop ERP, IoT Edge Gateway, or Multi-Tenant SaaS).

> 💡 *Untuk panduan mendalam tentang penambahan partisi sharding & multi-clone replika dalam Bahasa Indonesia, baca [Tutorial Sharding & Replikasi Multi-Clone](TUTORIAL_SHARDING_DAN_CLONE.md).*

---

## Table of Contents
1. [Installation & Setup](#1-installation--setup)
2. [Configuring the Engine](#2-configuring-the-engine)
3. [Defining Application Schemas](#3-defining-application-schemas)
4. [Writing Data (Transactions)](#4-writing-data-transactions)
5. [Reading Data (High-Throughput Replicas)](#5-reading-data-high-throughput-replicas)
6. [Heavy Analytics & Reporting](#6-heavy-analytics--reporting)
7. [Cryptographic Security (AES-256-GCM & HMAC)](#7-cryptographic-security-aes-256-gcm--hmac)
8. [Replication Management](#8-replication-management)
9. [Disaster Recovery & Chaos Testing](#9-disaster-recovery--chaos-testing)
10. [Managing with ShardVault Studio GUI](#10-managing-with-shardvault-studio-gui)

---

## 1. Installation & Setup

In your project directory, create a new Rust project if you haven't already:
```powershell
cargo new my_store_app
cd my_store_app
```

Add `shardvault-rs` and `rusqlite` to your `Cargo.toml`:
```toml
[dependencies]
shardvault-rs = { path = "E:/shardvault-rs" }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1.0", features = ["derive"] }
```

---

## 2. Configuring the Engine

Use `EngineBuilder` to configure enterprise SQLite parameters:

```rust
use shardvault_rs::prelude::*;
use std::path::Path;

fn setup_database() -> Result<DatabaseEngine, ShardVaultError> {
    let config = EngineBuilder::new(Path::new("./data"))
        .with_salt("my_app_production_secret_salt")
        .with_cache_size_mb(64)       // 64 MB RAM Cache per connection
        .with_mmap_size_mb(256)      // 256 MB Memory-mapped I/O
        .with_busy_timeout_ms(5000)  // 5 seconds lock queue tolerance
        .with_auto_heal_on_boot(true)// Automatic corruption quarantine & failover
        .with_integrity_guard(true)  // Prevent syncing corrupt pages to standbys
        .build_config();

    // Initialize DatabaseEngine
    DatabaseEngine::with_config_and_schema(config, Arc::new(MySchemaProvider))
}
```

---

## 3. Defining Application Schemas

ShardVault divides schemas into two categories:
- **Global Schema**: Master data common to the entire system (users, auth, tenant registry).
- **Shard Schema**: Localized transactional tables isolated per tenant or branch.

Implement the `SchemaProvider` trait:

```rust
use rusqlite::Connection;
use shardvault_rs::prelude::*;

pub struct MySchemaProvider;

impl SchemaProvider for MySchemaProvider {
    fn init_global(&self, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS system_users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );",
            [],
        )?;
        Ok(())
    }

    fn init_shard(&self, shard_key: &str, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orders (
                id TEXT PRIMARY KEY,
                invoice_no TEXT UNIQUE NOT NULL,
                customer_name TEXT NOT NULL,
                total_amount REAL NOT NULL,
                cost_price_encrypted TEXT NOT NULL,
                anti_tamper_sig TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_orders_invoice ON orders(invoice_no);",
            [],
        )?;
        Ok(())
    }
}
```

---

## 4. Writing Data (Transactions)

Mutations are executed directly on the **`PrimaryWrite`** database in WAL mode:

### Writing to Global DB:
```rust
db.write_global(|conn| {
    conn.execute(
        "INSERT OR IGNORE INTO system_users (id, username, role) VALUES (?1, ?2, ?3)",
        rusqlite::params!["usr_01", "cashier_jkt", "CASHIER"],
    )?;
    Ok(())
})?;
```

### Writing to a Specific Shard (e.g., `"jakarta"`):
```rust
db.write_shard("jakarta", |conn| {
    conn.execute(
        "INSERT INTO orders (id, invoice_no, customer_name, total_amount, cost_price_encrypted, anti_tamper_sig)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "ord_01",
            "INV-2026-001",
            "Budi Santoso",
            150000.0,
            encrypted_cost,
            signature_token
        ],
    )?;
    Ok(())
})?;
```

---

## 5. Reading Data (High-Throughput Replicas)

To prevent search queries from blocking cashiers or write transactions, execute reads on the **`FastRead`** replica:

```rust
// Reads from clones/jakarta_read.db
let sales_total: f64 = db.read_shard("jakarta", |conn| {
    conn.query_row(
        "SELECT SUM(total_amount) FROM orders WHERE customer_name = ?1",
        ["Budi Santoso"],
        |r| r.get(0),
    )
})?;
println!("Total Sales: Rp {:.2}", sales_total);
```

---

## 6. Heavy Analytics & Reporting

For heavy PDF batch exports, monthly reports, or full-table scans, query the **`AnalyticsReport`** replica so the read and write paths remain unaffected:

```rust
// Reads from clones/jakarta_analytics.db
db.analytics_shard("jakarta", |conn| {
    let mut stmt = conn.prepare("SELECT invoice_no, total_amount FROM orders ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })?;
    
    for row in rows {
        let (inv, amt) = row?;
        println!("Exporting {} : {}", inv, amt);
    }
    Ok(())
})?;
```

---

## 7. Cryptographic Security (AES-256-GCM & HMAC)

Sensitive columns (cost prices, tokens, PII) can be encrypted with the machine-bound master key:

```rust
// 1. Column-Level Encryption (AES-256-GCM)
let sensitive_data = "Cost Price: Rp 45.000 / item";
let ciphertext = db.encrypt(sensitive_data)?;
// Stored as "v1:{nonce}:{ciphertext+tag}"

// 2. Decryption
let decrypted = db.decrypt(&ciphertext)?;
assert_eq!(decrypted, sensitive_data);

// 3. HMAC-SHA256 Anti-Tamper Signatures
let canonical_row = "INV-2026-001:150000:Budi Santoso";
let signature = db.sign(canonical_row);

// 4. Verification
let is_authentic = db.verify_signature(canonical_row, &signature);
if !is_authentic {
    eprintln!("WARNING: Row data has been tampered with!");
}
```

---

## 8. Replication Management

Replicas can be synchronized on-demand or on a debounced schedule:

```rust
// Synchronize all 3 replicas (FastRead, Analytics, HotStandby) for shard "jakarta"
db.sync_shard_clones("jakarta")?;

// Synchronize all active shards across the entire cluster
db.router().sync_all_clones()?;
```

---

## 9. Disaster Recovery & Chaos Testing

### Emergency Failover (Hot Standby Promotion):
If the primary file is corrupted or lost, promote the hot standby to primary in $<50$ ms with zero data loss:

```rust
// Promotes clones/jakarta_standby.db to shards/jakarta.db
db.promote_standby("jakarta")?;
```

### Chaos Crash Injection (For Testing):
To test your application's resilience under disk failure:
```rust
db.router().simulate_shard_corruption("jakarta")?;
```

---

## 10. Managing with ShardVault Studio GUI

You can manage any project's database using the bundled native desktop editor:

```powershell
cd E:\shardvault-rs
cargo run --bin shardvault-studio -- "path/to/your/project/data"
```

Enjoy building resilient, high-speed applications with **ShardVault-RS**!
