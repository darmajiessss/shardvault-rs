# 📖 Tutorial Lengkap: Sharding & Replikasi Multi-Clone di ShardVault-RS

Panduan teknis mendalam mengenai cara menambah partisi database (**Sharding**), mengelola replika data (**Clones**), membuat peran kloning kustom (**Custom Roles**), serta strategi sinkronisasi non-blocking pada engine **ShardVault-RS**.

> 📦 *Untuk panduan migrasi skema, ekspor/impor paket portabel `.svp`, dan sinkronisasi data terdistribusi (CDC + HLC), baca [Tutorial Structure, Transfer, & Sync](TUTORIAL_TRANSFER_DAN_SYNC.md).*

---

## 📑 Daftar Isi
1. [Konsep Fundamental: Sharding & Multi-Clone](#1-konsep-fundamental-sharding--multi-clone)
2. [Menambah Sharding Baru (Horizontal Partitioning)](#2-menambah-sharding-baru-horizontal-partitioning)
   - [Pola Penamaan Shard (Shard Key)](#pola-penamaan-shard-shard-key)
   - [Mekanisme On-Demand Auto-Provisioning](#mekanisme-on-demand-auto-provisioning)
   - [Contoh Kode Lengkap Pembuatan Shard](#contoh-kode-lengkap-pembuatan-shard)
3. [Mengelola & Menggunakan Replikasi Clone](#3-mengelola--menggunakan-replikasi-clone)
   - [Mengenal 4 Role Bawaan Engine](#mengenal-4-role-bawaan-engine)
   - [Mengarahkan Query ke Clone yang Tepat](#mengarahkan-query-ke-clone-yang-tepat)
   - [Strategi Sinkronisasi Data (Online Backup API)](#strategi-sinkronisasi-data-online-backup-api)
   - [Integrity Guard (Pencegahan Keracunan Data)](#integrity-guard-pencegahan-keracunan-data)
4. [Tingkat Lanjut: Menambah Role Clone Kustom](#4-tingkat-lanjut-menambah-role-clone-kustom)
   - [Studi Kasus: Menambah Clone Audit & FTS](#studi-kasus-menambah-clone-audit--fts)
   - [Langkah 1: Modifikasi Enum ShardRole](#langkah-1-modifikasi-enum-shardrole)
   - [Langkah 2: Daftarkan ke Router Engine](#langkah-2-daftarkan-ke-router-engine)
   - [Langkah 3: Menggunakan Role Baru dalam Kode](#langkah-3-menggunakan-role-baru-dalam-kode)
5. [Disaster Recovery & Hot Standby Promotion](#5-disaster-recovery--hot-standby-promotion)
6. [Manajemen Visual via ShardVault Studio (GUI)](#6-manajemen-visual-via-shardvault-studio-gui)
7. [FAQ & Best Practices](#7-faq--best-practices)

---

## 1. Konsep Fundamental: Sharding & Multi-Clone

### Mengapa Sharding?
Pada SQLite konvensional, seluruh data aplikasi ditumpuk ke dalam satu berkas `.db` monolitik. Ketika volume data meningkat:
* Ukuran file membengkak hingga puluhan gigabyte.
* Jika satu halaman disk (*page*) rusak, seluruh sistem bisa mati total.
* Risiko kebocoran data antar-tenant sangat tinggi jika pengembang lupa menambahkan klausa `WHERE tenant_id = ...`.

**ShardVault-RS** membagi data menjadi partisi fisik independen (**Shards**):
* Setiap cabang, tenant, atau periode waktu memiliki berkas SQLite terisolasi sendiri di folder `shards/`.
* Kerusakan atau pemeliharaan pada satu shard **sama sekali tidak mempengaruhi** shard lainnya.

### Mengapa Multi-Clone?
SQLite hanya mengizinkan **satu penulis (single writer)** pada satu waktu. Jika kasir sedang menyimpan transaksi, query laporan analitik berat (misal `SUM()`, `JOIN` 5 tabel, atau ekspor CSV bulanan) dapat menahan kunci database dan memicu galat `SQLITE_BUSY` (`database is locked`).

ShardVault memecahkan masalah ini dengan mereplikasi setiap shard menjadi **4 berkas independen**:
```
DATA_DIR/
├── shards/
│   └── store_jakarta.db            <-- [PrimaryWrite] Jalur tulis kasir (Zero-Lock WAL)
└── clones/
    ├── store_jakarta_read.db       <-- [FastRead] Query kasir & pencarian FTS5
    ├── store_jakarta_analytics.db  <-- [AnalyticsReport] Ekspor laporan & PDF agregat
    └── store_jakarta_standby.db    <-- [HotStandby] Siap dipromosikan seketika saat crash
```

---

## 2. Menambah Sharding Baru (Horizontal Partitioning)

### Pola Penamaan Shard (Shard Key)
Shard Key adalah pengidentifikasi unik untuk suatu partisi. Gunakan string huruf kecil dan garis bawah (`snake_case`):
1. **Multi-Cabang / Multi-Outlet**: `"cabang_jakarta"`, `"cabang_surabaya"`, `"cabang_bali"`.
2. **Multi-Tenant / Pelanggan SaaS**: `"tenant_001"`, `"tenant_alpha"`.
3. **Partisi Waktu (Time-based Archival)**: `"transaksi_2026_q1"`, `"transaksi_2026_q2"`.

### Mekanisme On-Demand Auto-Provisioning
Di ShardVault, Anda **tidak perlu menjalankan skrip setup manual** untuk membuat shard baru. Engine bekerja secara *lazy-loaded on demand*:

```
Aplikasi Memanggil write_shard("cabang_bandung")
                       │
                       ▼
Apakah berkas shards/cabang_bandung.db sudah ada?
      ├── SUDAH ──► Langsung eksekusi transaksi
      └── BELUM ──► 1. Buat file fisik shards/cabang_bandung.db
                    2. Terapkan PRAGMA WAL & tuning memori
                    3. Eksekusi SchemaProvider::init_shard("cabang_bandung", conn)
                    4. Daftarkan nama shard ke katalog global
                    5. Otomatis buat 3 clone di folder clones/:
                       - cabang_bandung_read.db
                       - cabang_bandung_analytics.db
                       - cabang_bandung_standby.db
                    6. Eksekusi initial backup sync ke ketiga clone
```

### Contoh Kode Lengkap Pembuatan Shard

```rust
use rusqlite::Connection;
use shardvault_rs::prelude::*;
use std::path::Path;
use std::sync::Arc;

// 1. Definisikan Skema Database Anda
struct StoreSchema;

impl SchemaProvider for StoreSchema {
    fn init_global(&self, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS global_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
            [],
        )?;
        Ok(())
    }

    fn init_shard(&self, _shard_key: &str, conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS inventory (
                sku TEXT PRIMARY KEY,
                nama_barang TEXT NOT NULL,
                stok INTEGER NOT NULL,
                harga REAL NOT NULL
            );",
            [],
        )?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Inisialisasi DatabaseEngine
    let config = EngineBuilder::new(Path::new("./data_retail"))
        .with_salt("secret_retail_salt_2026")
        .build_config();

    let db = DatabaseEngine::with_config_and_schema(config, Arc::new(StoreSchema))?;

    // 3. Menambah Shard Baru 1: "cabang_jakarta"
    println!("Membuat Shard Jakarta...");
    db.write_shard("cabang_jakarta", |conn| {
        conn.execute(
            "INSERT INTO inventory VALUES ('BRG-01', 'Kopi Arabika 1kg', 50, 120000.0);",
            [],
        )?;
        Ok(())
    })?;

    // 4. Menambah Shard Baru 2: "cabang_surabaya"
    println!("Membuat Shard Surabaya...");
    db.write_shard("cabang_surabaya", |conn| {
        conn.execute(
            "INSERT INTO inventory VALUES ('BRG-02', 'Teh Hitam 500g', 80, 45000.0);",
            [],
        )?;
        Ok(())
    })?;

    // Verifikasi: Kedua shard memiliki berkas fisik dan replika terisolasi masing-masing
    println!("Shard Jakarta dan Surabaya berhasil dibuat beserta replikanya!");

    Ok(())
}
```

---

## 3. Mengelola & Menggunakan Replikasi Clone

### Mengenal 4 Role Bawaan Engine

| Role Enum | Folder & Nama Berkas | Mode Operasi | Tujuan Beban Kerja |
|---|---|---|---|
| `ShardRole::PrimaryWrite` | `shards/<key>.db` | Read / Write (WAL) | Transaksi mutasi (`INSERT`, `UPDATE`, `DELETE`) berkecepatan tinggi. |
| `ShardRole::FastRead` | `clones/<key>_read.db` | Read-Only | Query katalog, pengecekan stok kasir, pencarian instan. |
| `ShardRole::AnalyticsReport` | `clones/<key>_analytics.db` | Read-Only | Agregasi data berat, laporan laba rugi, ekspor CSV / PDF. |
| `ShardRole::HotStandby` | `clones/<key>_standby.db` | Mirror Pasif | Cadangan real-time yang siap mengambil alih saat primary crash. |

---

### Mengarahkan Query ke Clone yang Tepat

#### Skenario 1: Pembacaan Standar Kasir (Default ke `FastRead`)
Gunakan fungsi `read_shard`:
```rust
let stok: i64 = db.read_shard("cabang_jakarta", |conn| {
    conn.query_row(
        "SELECT stok FROM inventory WHERE sku = 'BRG-01';",
        [],
        |row| row.get(0),
    )
})?;
println!("Stok di FastRead Replica: {}", stok);
```

#### Skenario 2: Laporan Analitik Berat (Diarahkan ke `AnalyticsReport`)
Gunakan fungsi `read_shard_with_role`:
```rust
let total_aset: f64 = db.read_shard_with_role(
    "cabang_jakarta",
    ShardRole::AnalyticsReport,
    |conn| {
        conn.query_row(
            "SELECT SUM(stok * harga) FROM inventory;",
            [],
            |row| row.get(0),
        )
    },
)?;
println!("Total Nilai Aset (Analytics Clone): Rp {:.2}", total_aset);
```

> **Keuntungan:** Meskipun query analitik ini memproses jutaan baris selama 10 detik, kasir yang melakukan `INSERT` di `PrimaryWrite` **tidak akan pernah terkena `SQLITE_BUSY`**.

---

### Strategi Sinkronisasi Data (Online Backup API)

Setelah transaksi selesai di Primary, data di replika clone perlu disegarkan. ShardVault menggunakan **SQLite Online Backup API** berkecepatan tingkat memori.

#### 1. Sinkronisasi Seluruh Clone Shard
Menyinkronkan `FastRead`, `AnalyticsReport`, dan `HotStandby` sekaligus:
```rust
// Cukup satu baris kode:
db.sync_shard_clones("cabang_jakarta")?;
```

#### 2. Sinkronisasi Spesifik Satu Clone Saja
Jika Anda hanya ingin memperbarui `FastRead` agar cepat, tanpa membuang I/O untuk `AnalyticsReport`:
```rust
db.router().sync_clone_replica("cabang_jakarta", ShardRole::FastRead)?;
```

#### 3. Pola Eksekusi yang Direkomendasikan di Aplikasi Produksi
* **Kasir / POS**: Panggil `sync_shard_clones` setelah transaksi penting selesai disimpan (misal cetak struk).
* **High-Throughput Ingestion**: Gunakan background timer (setiap 5 detik atau 10 detik) untuk menjalankan sync berkala agar tidak membebani loop penulisan data.

---

### Integrity Guard (Pencegahan Keracunan Data)

Salah satu kelemahan replikasi berkas biasa (seperti `fs::copy`) adalah: jika berkas primary tiba-tiba rusak akibat listrik padam atau disk failure, perintah copy biasa akan menyalin byte rusak tersebut ke file cadangan sehingga cadangan ikut rusak.

Di ShardVault-RS, setiap proses sync dilindungi **Integrity Guard**:
```
db.sync_shard_clones("cabang_jakarta")
                   │
                   ▼
       [PRAGMA quick_check]
        pada PrimaryWrite
         /              \
     [OK]            [CORRUPT / ERROR]
      │                      │
Jalankan Backup API    BATALKAN SINKRONISASI!
ke Clones Replikasi    Return Err(PoisoningPrevented)
                       Standby tetap sehat & aman 100%!
```

---

## 4. Tingkat Lanjut: Menambah Role Clone Kustom

Jika arsitektur aplikasi Anda memerlukan replika khusus kelima atau keenam (misalnya clone khusus mesin pencari Full-Text Search atau clone khusus audit log audit internal), Anda dapat memperluas engine ShardVault dalam 3 langkah mudah.

### Studi Kasus: Menambah Clone Audit (`AuditSync`)
Tujuan: Membuat replika `clones/<key>_audit.db` yang otomatis disinkronkan untuk konsumsi tim auditor.

#### Langkah 1: Modifikasi Enum ShardRole di [`src/types.rs`](file:///E:/shardvault-rs/src/types.rs)

Buka `src/types.rs` dan tambahkan varian baru:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShardRole {
    PrimaryWrite,
    FastRead,
    AnalyticsReport,
    HotStandby,
    AuditSync, // <-- 1. Tambah varian baru di sini
}

impl ShardRole {
    pub fn suffix(&self) -> &'static str {
        match self {
            ShardRole::PrimaryWrite => "",
            ShardRole::FastRead => "_read",
            ShardRole::AnalyticsReport => "_analytics",
            ShardRole::HotStandby => "_standby",
            ShardRole::AuditSync => "_audit", // <-- 2. Nama file menjadi: <key>_audit.db
        }
    }

    pub fn folder(&self) -> &'static str {
        match self {
            ShardRole::PrimaryWrite => "shards",
            ShardRole::FastRead 
            | ShardRole::AnalyticsReport 
            | ShardRole::HotStandby 
            | ShardRole::AuditSync => "clones", // <-- 3. Masuk folder clones/
        }
    }
}
```

#### Langkah 2: Daftarkan ke Router Engine di [`src/router.rs`](file:///E:/shardvault-rs/src/router.rs)

Buka `src/router.rs` dan cari blok `auto-provisioning` (sekitar baris 244):

```rust
// Daftarkan AuditSync agar otomatis dibuat saat ada shard baru:
if is_new && role == ShardRole::PrimaryWrite {
    drop(shards_lock);
    for sub_role in [
        ShardRole::FastRead, 
        ShardRole::AnalyticsReport, 
        ShardRole::HotStandby,
        ShardRole::AuditSync, // <-- Tambahkan di sini
    ] {
        let _ = self.get_shard_conn(&key_str, sub_role);
        let _ = self.sync_clone_replica(&key_str, sub_role);
    }
    return self.get_shard_conn(&key_str, role);
}
```

Dan di fungsi `sync_shard_clones` di [`src/engine.rs`](file:///E:/shardvault-rs/src/engine.rs):
```rust
pub fn sync_shard_clones<Q: ?Sized + AsRef<str>>(&self, shard_key: &Q) -> Result<(), ShardVaultError> {
    for role in [
        ShardRole::FastRead, 
        ShardRole::AnalyticsReport, 
        ShardRole::HotStandby,
        ShardRole::AuditSync, // <-- Tambahkan di sini
    ] {
        self.router.sync_clone_replica(shard_key, role)?;
    }
    Ok(())
}
```

#### Langkah 3: Menggunakan Role Baru dalam Kode

```rust
// Baca data langsung dari clone audit khusus:
let log_count: i64 = db.read_shard_with_role("cabang_jakarta", ShardRole::AuditSync, |conn| {
    conn.query_row("SELECT COUNT(*) FROM inventory;", [], |r| r.get(0))
})?;
println!("Audit Sync Clone Item Count: {}", log_count);
```

---

## 5. Disaster Recovery & Hot Standby Promotion

Jika berkas `PrimaryWrite` mengalami kerusakan fatal (misal sektor disk bad sector):

### 1. Auto-Heal Saat Startup (Zero-Config)
Secara default, `EngineConfig.auto_heal_on_boot = true`. Saat aplikasi Anda dinyalakan:
1. Router mendeteksi status `PRAGMA quick_check != "ok"`.
2. Router mengarantina berkas rusak ke format `shards/<key>.db.corrupted.<timestamp>`.
3. Router langsung memulihkan data dari `clones/<key>_standby.db` menjadi primary baru.
4. Waktu pemulihan (*Recovery Time Objective / RTO*) rata-rata **di bawah 50 milidetik**.

### 2. Manual Promotion (Darurat / Run-time)
Anda juga bisa memicu failover secara programatik:
```rust
db.router().promote_standby_to_primary("cabang_jakarta")?;
println!("HotStandby berhasil dipromosikan menjadi Primary!");
```

---

## 6. Manajemen Visual via ShardVault Studio (GUI)

Jika Anda ingin melihat dan mengelola shard serta clone secara visual:

```powershell
# Jalankan Studio mengarah ke folder data Anda:
cd E:\shardvault-rs
.\start-studio.bat "path/ke/data_retail"
```

### Navigasi Studio:
1. **Cluster Explorer Tree (Panel Kiri)**:
   * Menampilkan daftar semua Shard yang aktif.
   * Di bawah setiap shard, terdapat 4 node replika (`PrimaryWrite`, `FastRead`, `AnalyticsReport`, `HotStandby`) beserta jumlah baris data secara langsung.
2. **Tab Replication & Chaos Hub (Panel Atas)**:
   * **Tombol "⚡ Sync Clones"**: Menjalankan sinkronisasi instan ke seluruh replika.
   * **Uji Chaos Inoculation**: Menguji keandalan sistem dengan menyuntikkan byte korup untuk melihat auto-heal beraksi.
   * **Tombol Emergency Failover**: Melakukan promosi paksa dari standby menjadi primary.
3. **Tab SQL Console**:
   * Memungkinkan Anda memilih target query secara spesifik: apakah ingin mengetes query pada `PrimaryWrite`, `FastRead`, atau `AnalyticsReport`.

---

## 7. FAQ & Best Practices

#### Q: Kapan waktu terbaik untuk memanggil `sync_shard_clones`?
* **Aplikasi POS Kasir**: Panggil tepat setelah transaksi pembayaran berhasil disimpan (`INSERT`).
* **Sistem Logging / IoT Berkecepatan Tinggi**: Panggil secara periodik (misal setiap 3-5 detik via background thread) agar I/O disk tidak terbebani secara konstan.

#### Q: Apakah memiliki banyak Shard dan Clone akan membuat aplikasi boros RAM?
* **Tidak**. Koneksi SQLite diatur secara *lazy*. Berkas hanya dibuka saat pertama kali diakses. ShardVault juga mengatur `PRAGMA cache_size = -64000` (64 MB) dan `mmap_size = 256 MB` yang dikelola secara efisien oleh kernel OS.

#### Q: Apakah aman membaca dari `FastRead` saat `PrimaryWrite` sedang menulis?
* **Sangat aman 100%**. Karena `FastRead` dan `PrimaryWrite` adalah dua berkas fisik yang berbeda, keduanya memiliki lock file OS yang terpisah. Pembacaan di `FastRead` tidak akan pernah terkunci oleh penulisan di `PrimaryWrite`.
