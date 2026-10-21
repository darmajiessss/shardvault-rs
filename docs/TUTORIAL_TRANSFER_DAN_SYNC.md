# 📖 Tutorial Lengkap: Database Structure, Secure Transfer (.svp), & Data Synchronization

Panduan teknis mendalam mengenai cara mengelola evolusi struktur basis data (**Schema & Migrations**), memindahkan basis data secara aman antar-mesin (**Secure Transfer .svp**), serta menyinkronkan data antar-node (**Distributed Synchronization**) pada engine **ShardVault-RS**.

---

## 📑 Daftar Isi
1. [Kajian Konsep & Latar Belakang Arsitektur](#1-kajian-konsep--latar-belakang-arsitektur)
2. [Evolusi Skema & Universal Migration Engine](#2-evolusi-skema--universal-migration-engine)
   - [Mekanisme Pelacak `_shardvault_migrations`](#mekanisme-pelacak-_shardvault_migrations)
   - [Eksekusi Migrasi Atomik Multi-Shard](#eksekusi-migrasi-atomik-multi-shard)
   - [Introspeksi Skema & Deteksi Schema Drift](#introspeksi-skema--deteksi-schema-drift)
3. [Secure Database Transfer Package (.svp)](#3-secure-database-transfer-package-svp)
   - [Tantangan Hardware-Bound Key Antar Komputer](#tantangan-hardware-bound-key-antar-komputer)
   - [Mekanisme Envelope Encryption (DEK & KEK)](#mekanisme-envelope-encryption-dek--kek)
   - [Kompresi Zstandard & Tanda Tangan Digital HMAC](#kompresi-zstandard--tanda-tangan-digital-hmac)
   - [Panduan Kode Ekspor & Impor](#panduan-kode-ekspor--impor)
4. [Distributed Data Synchronization Engine](#4-distributed-data-synchronization-engine)
   - [Mengapa Wall-Clock / SystemTime Gagal di Sistem Terdistribusi](#mengapa-wall-clock--systemtime-gagal-di-sistem-terdistribusi)
   - [Algoritma Hybrid Logical Clock (HLC - Kulkarni et al.)](#algoritma-hybrid-logical-clock-hlc---kulkarni-et-al)
   - [Change Data Capture (CDC) via Triggers & Tombstone Protocol](#change-data-capture-cdc-via-triggers--tombstone-protocol)
   - [Resolusi Konflik Last-Write-Wins (LWW) & Field-Level Merge](#resolusi-konflik-last-write-wins-lww--field-level-merge)
   - [Alur Protokol Push, Pull, & Acknowledge](#alur-protokol-push-pull--acknowledge)
5. [Pengoperasian Visual via ShardVault Studio (GUI)](#5-pengoperasian-visual-via-shardvault-studio-gui)
6. [FAQ & Penanganan Skenario Ekstrem (Chaos Cases)](#6-faq--penanganan-skenario-ekstrem-chaos-cases)

---

## 1. Kajian Konsep & Latar Belakang Arsitektur

Pada aplikasi lokal terdistribusi (seperti Point-of-Sale multi-cabang, software desktop offline-first, atau edge device), tiga kebutuhan krusial sering kali menjadi titik kegagalan utama arsitektur SQLite:
1. **Perubahan Skema (Schema Evolution):** Bagaimana menambahkan kolom baru atau tabel baru ke puluhan partisi shard secara serempak tanpa merusak replika kloning?
2. **Pemindahan Data Lintas Mesin (Transfer):** Bagaimana mengekspor basis data lokal yang terenkripsi oleh kunci hardware mesin A agar dapat dibuka dan diadopsi dengan aman di mesin B?
3. **Sinkronisasi Data Multi-Master (Sync):** Ketika cabang A dan cabang B sama-sama memodifikasi data saat koneksi internet terputus, bagaimana menggabungkan data tersebut tanpa menimpa data yang lebih baru secara keliru?

ShardVault-RS menghadirkan solusi komprehensif melalui tiga modul inti: `shardvault::migration`, `shardvault::transfer`, dan `shardvault::sync`.

---

## 2. Evolusi Skema & Universal Migration Engine

### Mekanisme Pelacak `_shardvault_migrations`
Setiap basis data (`global.db` maupun setiap `shards/<key>.db`) secara otomatis memiliki tabel internal:
```sql
CREATE TABLE IF NOT EXISTS _shardvault_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    checksum_sha256 TEXT NOT NULL,
    execution_time_ms INTEGER NOT NULL
);
```

### Eksekusi Migrasi Atomik Multi-Shard
Pengembang mendefinisikan perubahan skema secara deklaratif. Runner akan:
1. Memeriksa migrasi yang belum diterapkan.
2. Memverifikasi bahwa skrip migrasi yang telah dieksekusi sebelumnya tidak pernah diubah (integritas SHA-256).
3. Mengeksekusi DDL baru di dalam transaksi ACID.
4. **Secara instan menyinkronkan DDL ke seluruh 3 replika kloning** (`FastRead`, `AnalyticsReport`, `HotStandby`).

```rust
use shardvault_rs::prelude::*;

// 1. Buat MigrationRunner
let mut runner = MigrationRunner::new();

// 2. Tambahkan migrasi v1
runner.add(Migration {
    version: 1,
    name: "create_loyalty_and_alter_orders",
    target: MigrationTarget::ShardsOnly, // Target: GlobalOnly, ShardsOnly, atau All
    up_sql: "CREATE TABLE loyalty_points (user_id TEXT PRIMARY KEY, points INTEGER NOT NULL);
             ALTER TABLE orders ADD COLUMN discount_percent REAL DEFAULT 0.0;",
    down_sql: None,
});

// 3. Eksekusi ke seluruh cluster
let reports = db.run_migrations(&runner)?;
for r in reports {
    println!("Database '{}': Migrasi v{:?} sukses diterapkan ({} ms)", 
        r.target_name, r.applied_versions, r.total_duration_ms);
}
```

### Introspeksi Skema & Deteksi Schema Drift
ShardVault mampu membaca katalog relasional SQLite (`sqlite_master`, `PRAGMA table_info`, `PRAGMA foreign_key_list`, `PRAGMA index_list`) dan menghasilkan `SchemaManifest` dengan hash kanonikal SHA-256.

```rust
let manifest_a = db.introspect_shard("cabang_jakarta")?;
let manifest_b = db.introspect_shard("cabang_surabaya")?;

// Deteksi perbedaan struktur (kolom hilang, tipe tidak cocok, dsb.)
let drift = detect_schema_drift(&manifest_a, &manifest_b);
if drift.is_empty() {
    println!("Kedua cabang memiliki skema 100% identik!");
} else {
    for d in drift {
        eprintln!("Deviasi terdeteksi: {}", d);
    }
}
```

---

## 3. Secure Database Transfer Package (.svp)

### Tantangan Hardware-Bound Key Antar Komputer
Enkripsi kolom ShardVault terikat pada spesifikasi perangkat keras komputer lokal (Machine GUID & CPU Core Count via PBKDF2-HMAC-SHA256). Jika file `.db` disalin langsung ke komputer lain, komputer tujuan tidak akan bisa mendekripsi kolom privat tersebut.

### Mekanisme Envelope Encryption (DEK & KEK)
ShardVault menggunakan standar **Envelope Encryption**:
```
[Passphrase Pengguna] ──(PBKDF2 100.000 iterasi)──► [KEK: Key Encryption Key]
                                                              │
                                                Enkripsi AES-256-GCM
                                                              │
                                                              ▼
[DEK Acak 32-byte] ──────────Enkripsi Snapshot SQLite────► [Wrapped DEK]
```
1. Mesin sumber membangkitkan kunci acak 32-byte (**DEK**).
2. Data snapshot SQLite dikompresi dengan **Zstandard (`zstd`)**, lalu dienkripsi menggunakan DEK.
3. DEK dienkripsi menggunakan **KEK** yang diturunkan dari Passphrase Transfer.
4. Seluruh paket diverifikasi oleh tanda tangan **HMAC-SHA256**.

### Panduan Kode Ekspor & Impor

#### 1. Ekspor Shard ke Berkas `.svp`:
```rust
let meta = db.export_shard_package(
    "cabang_jakarta",
    "./backups/jakarta_export.svp",
    "PassphraseKuatTransfer2026!"
)?;
println!("Berhasil diekspor: Shard '{}' (SHA-256: {})", meta.shard_key, meta.checksum_sha256);
```

#### 2. Impor Berkas `.svp` di Mesin Tujuan:
```rust
let imported = db.import_shard_package(
    "./backups/jakarta_export.svp",
    "cabang_jakarta_imported",
    "PassphraseKuatTransfer2026!"
)?;
println!("Berhasil diimpor ke mesin baru! 3 kloning replika otomatis siap.");
```

> **Keamanan:** Jika berkas `.svp` diubah 1 byte saja saat transit di jaringan atau flashdisk, proses impor akan **langsung ditolak oleh verifikasi HMAC** sebelum data sempat ditulis ke disk.

---

## 4. Distributed Data Synchronization Engine

### Mengapa Wall-Clock / SystemTime Gagal di Sistem Terdistribusi
Menggunakan jam sistem (`SystemTime::now()` atau `DATETIME('now')`) untuk menentukan pemenang konflik (*Last-Write-Wins*) berisiko tinggi karena:
* Sinkronisasi NTP dapat memundurkan waktu sistem (*clock skew*).
* Baterai CMOS perangkat edge yang lemah dapat menyebabkan deviasi waktu hingga berjam-jam.
* Dua peristiwa yang saling bergantung (*causal*) bisa memiliki timestamp terbalik.

### Algoritma Hybrid Logical Clock (HLC - Kulkarni et al.)
ShardVault mengimplementasikan **HLC** murni:
Tuple HLC: $(l, c)$
- $l$: Waktu fisik (milidetik) yang dibatasi secara ketat terhadap jam sistem.
- $c$: Penghitung logis untuk menjaga kausalitas jika terjadi beberapa mutasi dalam milidetik yang sama atau ketika menerima pesan dari node lain yang memiliki waktu lebih maju.

HLC menjamin sifat **Strict Monotonicity**: waktu tidak pernah berjalan mundur.

---

### Change Data Capture (CDC) via Triggers & Tombstone Protocol
ShardVault memasang trigger `AFTER INSERT`, `AFTER UPDATE`, dan `AFTER DELETE` secara otomatis pada tabel aplikasi untuk mencatat mutasi ke `_shardvault_oplog`:

#### Tabel Oplog Internal:
```sql
CREATE TABLE IF NOT EXISTS _shardvault_oplog (
    op_id INTEGER PRIMARY KEY AUTOINCREMENT,
    table_name TEXT NOT NULL,
    row_pk TEXT NOT NULL,
    operation TEXT NOT NULL,        -- 'INSERT', 'UPDATE', 'DELETE'
    hlc_timestamp TEXT NOT NULL,    -- '<physical_ms>.<logical_counter>'
    node_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    is_synced INTEGER DEFAULT 0
);
```

#### Protokol Tombstone:
Ketika sebuah baris dihapus, operasi tersebut **tidak hilang begitu saja**, melainkan dicatat sebagai Tombstone:
```json
{"id": "P1", "_tombstone": 1}
```
Ini memastikan bahwa node lain yang tersambung belakangan mengetahui bahwa baris tersebut telah dihapus, alih-alih menganggapnya belum terdaftar.

---

### Resolusi Konflik Last-Write-Wins (LWW) & Field-Level Merge

Ketika Node A (Cabang) dan Node B (HQ) sama-sama mengedit baris yang sama saat offline:
1. **Pemeriksaan Kausalitas HLC:**
   $$\text{HLC}_{\text{remote}} \text{ vs } \text{HLC}_{\text{local}}$$
2. Jika $\text{HLC}_{\text{remote}} > \text{HLC}_{\text{local}}$, data remote diterapkan (*AppliedRemoteWins*).
3. Jika $\text{HLC}_{\text{local}} > \text{HLC}_{\text{remote}}$, data remote diabaikan (*IgnoredLocalWins*).
4. Jika HLC identik (seri), string `node_id` digunakan sebagai *tie-breaker* deterministik.

---

### Alur Protokol Push, Pull, & Acknowledge

```rust
// 1. Inisialisasi SyncEngine dengan nama node
let sync_node = db.create_sync_engine("node_cabang_jakarta");

// 2. Pasang trigger CDC pada tabel bisnis
db.write_shard("transaksi", |conn| {
    sync_node.register_table_for_cdc(conn, "orders")?;
    Ok(())
})?;

// 3. Node Pengirim: Siapkan paket mutasi lokal
let conn_arc = db.router().get_shard_conn("transaksi", ShardRole::PrimaryWrite)?;
let push_req = {
    let conn = conn_arc.lock().unwrap();
    sync_node.prepare_push_request(&conn, "transaksi", 100)?
};

// 4. Node Penerima (HQ): Terapkan paket mutasi dengan resolusi konflik LWW
let mut target_conn = target_conn_arc.lock().unwrap();
let push_res = sync_hq.process_incoming_push(&mut target_conn, &push_req)?;
println!("Mutasi diterapkan: {}, Diabaikan: {}", push_res.applied_count, push_res.ignored_count);

// 5. Node Pengirim: Beri tanda bahwa mutasi telah terkirim
{
    let conn = conn_arc.lock().unwrap();
    let op_ids: Vec<i64> = push_req.operations.iter().map(|o| o.op_id).collect();
    sync_node.acknowledge_push(&conn, &op_ids)?;
}
```

---

## 5. Pengoperasian Visual via ShardVault Studio (GUI)

Jalankan GUI editor:
```powershell
cd E:\shardvault-rs
.\start-studio.bat
```

### Tab Baru yang Tersedia:
1. **Tab `🚀 Migrations`**:
   * Menampilkan daftar migrasi yang sudah aktif lengkap dengan checksum SHA-256 dan durasi eksekusi.
   * Editor DDL interaktif untuk menyebarkan tabel atau kolom baru ke seluruh shard aktif.
   * Panel perbandingan skema untuk mendeteksi *Schema Drift*.
2. **Tab `📦 Transfer (.svp)`**:
   * Form pembuatan paket `.svp` terenkripsi dengan proteksi kata sandi.
   * Form impor untuk mengunggah dan mendekripsi paket `.svp` ke dalam cluster.
3. **Tab `🔄 Sync & Oplog`**:
   * Tabel aliran data CDC secara langsung (`_shardvault_oplog`).
   * Indikator status jam HLC real-time.
   * Tombol satu-klik untuk memasang trigger CDC, konfirmasi sync, dan pembersihan log lama (> 14 hari).

---

## 6. FAQ & Penanganan Skenario Ekstrem (Chaos Cases)

#### Q: Apa yang terjadi jika jam sistem komputer diubah secara manual ke masa lalu?
* **A:** Algoritma HLC menjamin nilai $l$ dan $c$ tidak akan pernah turun. Jika jam fisik melompat mundur, komponen $c$ akan bertambah secara logis sehingga seluruh transaksi berikutnya tetap memiliki timestamp yang lebih baru.

#### Q: Seberapa efisien ukuran kompresi berkas `.svp`?
* **A:** Berkat integrasi Zstandard (`zstd` level 3), berkas database SQLite yang berukuran 50 MB umumnya berkurang hingga 8–15 MB tergantung tipe data teks/indeks di dalamnya.

#### Q: Apakah sinkronisasi mengunci penulisan kasir?
* **A:** Tidak. Trigger CDC berjalan di dalam transaksi penulisan asli di `PrimaryWrite` tanpa lock tambahan, dan operasi query baca dilakukan secara terisolasi pada replika `FastRead`.
