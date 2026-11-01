# Panduan Lengkap: DrawDB Visual Schema Designer, Foreign Key Builder, & Multi-Target Code Generator

Dokumen ini menjelaskan cara menggunakan fitur **Visual Database Schema Designer & Multi-Target Code Generator** (terinspirasi dari **[drawdb-io/drawdb](https://github.com/drawdb-io/drawdb)**) yang terintegrasi penuh di dalam **ShardVault-RS** (`shardvault::codegen`) dan **ShardVault Studio GUI**.

---

## 1. Ikhtisar Arsitektur & Fitur Utama

Berbeda dengan editor diagram ER biasa, Schema Designer pada ShardVault menggabungkan kemudahan visual **DrawDB** dengan fitur keamanan dan replikasi terdistribusi **ShardVault**:

1. **Interactive ER Diagram Canvas (`🕸️ ER Diagram`)**:
   - Drag-and-drop kartu tabel di atas kanvas ber-grid.
   - Visualisasi garis relasi **Foreign Key** lengkap dengan badge **Cardinality** (`N:1`, `1:N`, `1:1`) dan aksi referensial (`CASCADE`, `RESTRICT`, `SET NULL`).
   - Pengelompokan domain menggunakan **Subject Areas** (kotak area berwarna) dan dokumentasi menggunakan **Sticky Notes**.
2. **First-Class Foreign Key & Table Designer (`🛠️ Tables & Foreign Keys`)**:
   - Tambah/ubah/urutkan kolom, pilih tipe data (`INTEGER`, `TEXT`, `REAL`, `BLOB`, `NUMERIC`), atur `PRIMARY KEY`, `AUTOINCREMENT`, `NOT NULL`, `UNIQUE`, `DEFAULT`, dan `CHECK`.
   - Tandai kolom dengan flag keamanan ShardVault:
     - `🔒 AES-256`: Otomatis mengenkripsi/mendekripsi kolom dengan AES-256-GCM pada kode Rust yang di-generate.
     - `🛡️ HMAC`: Menyertakan kolom dalam tanda tangan integritas baris (`HMAC-SHA256`).
   - Pembuat **Foreign Key** visual dengan fitur **Auto-Index FK** (otomatis membuat indeks pada kolom anak untuk mencegah *full-table lock* di SQLite saat `DELETE`/`UPDATE`).
3. **DrawDB Schema Issue Detector (`⚠️ Issues Linter`)**:
   - Memvalidasi skema secara *real-time*: mendeteksi tabel tanpa Primary Key, ketidakcocokan tipe data antara kolom Foreign Key dan kolom induk, referensi tabel/kolom yang hilang, serta peringatan jika Foreign Key melintasi batas fisik `Global` vs `Shard`.
4. **Smart Diff & SQLite 12-Step Safe Table Rebuild (`🔄 Smart Diff & Deploy`)**:
   - Membandingkan rancangan visual (`SchemaBlueprint`) terhadap database SQLite yang sedang aktif.
   - Karena SQLite tidak mengizinkan `ALTER TABLE ... ADD CONSTRAINT FOREIGN KEY` secara langsung, ShardVault secara otomatis menyusun skrip transaksional **12-Step Safe Table Rebuild** saat Anda menambahkan Foreign Key atau mengubah konstrain kolom pada tabel yang sudah berisi data—**100% tanpa kehilangan data**!
5. **Multi-Target Code & SQL Generator (`💻 Code & SQL Generator`)**:
   - **Rust (`.rs`)**: Menghasilkan implementasi `SchemaProvider`, *struct* domain (`Serialize`, `Deserialize`), serta metode CRUD kriptografis (`encrypt_fields`, `decrypt_fields`, `compute_row_hmac`, `upsert`).
   - **Multi-Dialect SQL DDL (`.sql`)**: Menghasilkan skrip DDL untuk **SQLite 3**, **PostgreSQL**, dan **MySQL / MariaDB**.
   - **TypeScript Interfaces (`.ts`)**: Menghasilkan `export interface` untuk integrasi frontend/Tauri.
   - **DrawDB Blueprint JSON (`.ddb.json`)**: Menyimpan dan memuat ulang cetak biru diagram ER.

---

## 2. Cara Menggunakan melalui ShardVault Studio (GUI)

Jalankan **ShardVault Studio**:
```powershell
cd E:\shardvault-rs
.\start-studio.bat
```

Buka tab **`📐 Schema & DrawDB Studio`** di bagian atas aplikasi:

### Langkah A: Memulai dari Database Aktif, Skrip SQL, atau Template
- Klik **`📥 Import dari Live DB`** untuk melakukan *reverse-engineering* seluruh tabel, kolom, indeks, dan Foreign Key dari database yang sedang dipilih di sidebar.
- Atau pilih **`🛒 Template: POS & Retail`** / **`🏢 Template: SaaS Vault`** untuk memulai dari arsitektur referensi.
- Anda juga dapat mengimpor file `.sql` mentah melalui tab **`💻 Code & SQL Generator`** -> pilih **`🗄️ SQLite 3 DDL (.sql)`** -> klik **`📂 Import SQL DDL dari Path`**.

### Langkah B: Menambahkan Tabel & Foreign Key Baru
1. Buka sub-tab **`🛠️ Tables & Foreign Keys`**.
2. Pilih tabel anak (misal `orders` atau `products`) di daftar atas.
3. Pada panel **`🔗 Foreign Keys & Relationships Builder`**:
   - Pilih **Kolom Lokal (Child)** (misal `category_id`).
   - Pilih **Tabel Induk (Parent)** (misal `categories`) dan **Kolom Induk** (misal `id`).
   - Pilih **Cardinality** (`N:1`, `1:N`, atau `1:1`) dan aturan **ON DELETE** (`CASCADE`, `RESTRICT`, `SET NULL`).
   - Pastikan **`Auto-Index`** dicentang, lalu klik **`➕ Tambah Foreign Key`**.
4. Kembali ke sub-tab **`🕸️ ER Diagram (DrawDB)`** untuk melihat garis relasi Foreign Key terhubung secara otomatis!

### Langkah C: Menerapkan Perubahan ke Cluster (Smart Diff & 12-Step Rebuild)
1. Buka sub-tab **`🔄 Smart Diff & Deploy`**.
2. Studio akan menampilkan daftar perubahan antara rancangan Anda dan database aktif (`[+ CREATE TABLE]`, `[+ ADD COLUMN]`, atau `[🔄 12-STEP REBUILD]`).
3. Klik **`⚡ Jalankan Migrasi Sekarang`** untuk mengeksekusi migrasi secara atomik ke `GlobalOnly`, `ShardsOnly`, atau `All` (termasuk propagasi otomatis ke seluruh replika `_read`, `_analytics`, dan `_standby`).

---

## 3. Cara Menggunakan melalui Kode Rust (`shardvault::codegen`)

Anda juga dapat membangun skema, menambahkan Foreign Key, memvalidasi isu, dan men-generate kode secara terprogram di Rust:

```rust
use shardvault_rs::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Buat Blueprint Skema atau Reverse-Engineer dari Skrip SQL
    let mut bp = SchemaBlueprint::from_sql_ddl_script(
        "CREATE TABLE categories (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
         CREATE TABLE products (id INTEGER PRIMARY KEY AUTOINCREMENT, category_id INTEGER NOT NULL, title TEXT NOT NULL);",
        TableScope::Global,
    )?;

    // 2. Tambahkan Foreign Key (products.category_id -> categories.id ON DELETE RESTRICT)
    if let Some(prod_tbl) = bp.tables.iter_mut().find(|t| t.name == "products") {
        prod_tbl.foreign_keys.push(ForeignKeyBlueprint::new(
            "products",
            "category_id",
            "categories",
            "id",
            ForeignKeyAction::Restrict,
        ));
    }

    // 3. Validasi dengan DrawDB Linter
    let issues = bp.validate_issues();
    for issue in &issues {
        println!("[{:?}] {}: {}", issue.severity, issue.target, issue.message);
    }

    // 4. Generate DDL (SQLite, PostgreSQL, MySQL) & Kode Rust / TypeScript
    let sqlite_sql = compile_blueprint_ddl(&bp, SqlDialect::Sqlite, None);
    let postgres_sql = compile_blueprint_ddl(&bp, SqlDialect::Postgres, None);
    let rust_repo_code = generate_rust_code(&bp);
    let ts_interfaces = generate_typescript_code(&bp);

    println!("Generated SQLite DDL:\n{}", sqlite_sql);
    Ok(())
}
```
