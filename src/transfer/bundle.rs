// E:\shardvault-rs\src\transfer\bundle.rs
// Format Paket Portabel ShardVault (.svp) dengan Envelope Encryption & Kompresi Zstd

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io::Cursor;

use crate::error::ShardVaultError;

const MAGIC_BYTES: &[u8; 4] = b"SVPK";
const FORMAT_VERSION: u32 = 1;
const PBKDF2_ROUNDS: u32 = 100_000;

/// Metadata manifest yang menyertai berkas paket .svp
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageMetadata {
    pub shard_key: String,
    pub schema_version: u32,
    pub source_machine_salt: String,
    pub created_at_rfc3339: String,
    pub row_counts: Vec<(String, usize)>,
    pub checksum_sha256: String,
}

/// Menurunkan Key Encryption Key (KEK) dan HMAC Key dari passphrase transfer
fn derive_transfer_keys(passphrase: &str, salt: &[u8; 16]) -> ([u8; 32], [u8; 32]) {
    let mut kek = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ROUNDS, &mut kek);

    // Turunkan HMAC key dengan salt terbalik
    let mut inv_salt = *salt;
    inv_salt.reverse();
    let mut hmac_key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &inv_salt, 10_000, &mut hmac_key);

    (kek, hmac_key)
}

/// Membungkus (Pack) SQLite database snapshot menjadi format .svp terenkripsi
pub fn pack_bundle(
    metadata: &PackageMetadata,
    sqlite_bytes: &[u8],
    passphrase: &str,
) -> Result<Vec<u8>, ShardVaultError> {
    if passphrase.trim().is_empty() {
        return Err(ShardVaultError::Transfer("Passphrase transfer tidak boleh kosong".into()));
    }

    // 1. Generate salt acak 16 byte & DEK acak 32 byte
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;

    let mut dek = [0u8; 32];
    getrandom::getrandom(&mut dek).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;

    let (kek, hmac_key) = derive_transfer_keys(passphrase, &salt);

    // 2. Bungkus DEK dengan KEK via AES-256-GCM (Wrapped DEK)
    let kek_cipher = Aes256Gcm::new_from_slice(&kek).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let mut dek_nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut dek_nonce_bytes).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let dek_nonce = Nonce::from_slice(&dek_nonce_bytes);

    let wrapped_dek = kek_cipher
        .encrypt(dek_nonce, dek.as_ref())
        .map_err(|e| ShardVaultError::Crypto(format!("Gagal mengenkripsi DEK: {}", e)))?;

    // 3. Kompresi metadata dengan Zstd & enkripsi dengan DEK
    let dek_cipher = Aes256Gcm::new_from_slice(&dek).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let meta_json = serde_json::to_vec(metadata).map_err(|e| ShardVaultError::Transfer(e.to_string()))?;
    let meta_compressed = zstd::encode_all(Cursor::new(&meta_json), 3)
        .map_err(|e| ShardVaultError::Transfer(format!("Gagal kompresi metadata: {}", e)))?;

    let mut meta_nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut meta_nonce_bytes).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let encrypted_meta = dek_cipher
        .encrypt(Nonce::from_slice(&meta_nonce_bytes), meta_compressed.as_ref())
        .map_err(|e| ShardVaultError::Crypto(format!("Gagal mengenkripsi metadata: {}", e)))?;

    // 4. Kompresi payload SQLite dengan Zstd & enkripsi dengan DEK
    let payload_compressed = zstd::encode_all(Cursor::new(sqlite_bytes), 3)
        .map_err(|e| ShardVaultError::Transfer(format!("Gagal kompresi snapshot database: {}", e)))?;

    let mut payload_nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut payload_nonce_bytes).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let encrypted_payload = dek_cipher
        .encrypt(Nonce::from_slice(&payload_nonce_bytes), payload_compressed.as_ref())
        .map_err(|e| ShardVaultError::Crypto(format!("Gagal mengenkripsi payload: {}", e)))?;

    // 5. Rakit binary buffer
    let mut buffer = Vec::new();
    buffer.extend_from_slice(MAGIC_BYTES);
    buffer.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    buffer.extend_from_slice(&salt);

    // Wrapped DEK block: len (u32) + nonce (12b) + ciphertext
    buffer.extend_from_slice(&(wrapped_dek.len() as u32).to_le_bytes());
    buffer.extend_from_slice(&dek_nonce_bytes);
    buffer.extend_from_slice(&wrapped_dek);

    // Metadata block: len (u32) + nonce (12b) + ciphertext
    buffer.extend_from_slice(&(encrypted_meta.len() as u32).to_le_bytes());
    buffer.extend_from_slice(&meta_nonce_bytes);
    buffer.extend_from_slice(&encrypted_meta);

    // Payload block: len (u64) + nonce (12b) + ciphertext
    buffer.extend_from_slice(&(encrypted_payload.len() as u64).to_le_bytes());
    buffer.extend_from_slice(&payload_nonce_bytes);
    buffer.extend_from_slice(&encrypted_payload);

    // 6. Hitung tanda tangan HMAC-SHA256 atas seluruh isi paket hingga saat ini
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&hmac_key)
        .map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    mac.update(&buffer);
    let hmac_tag = mac.finalize().into_bytes();
    buffer.extend_from_slice(&hmac_tag);

    Ok(buffer)
}

/// Membuka (Unpack) berkas .svp, memvalidasi HMAC, mendekripsi payload, dan mengembalikan raw snapshot
pub fn unpack_bundle(
    package_bytes: &[u8],
    passphrase: &str,
) -> Result<(PackageMetadata, Vec<u8>), ShardVaultError> {
    // Ukuran minimal header + HMAC (4 + 4 + 16 + 4 + 12 + 48 + 4 + 12 + 16 + 8 + 12 + 16 + 32)
    if package_bytes.len() < 172 {
        return Err(ShardVaultError::Transfer("Ukuran berkas paket .svp tidak valid (terlalu kecil)".into()));
    }

    // 1. Verifikasi Magic Bytes
    if &package_bytes[0..4] != MAGIC_BYTES {
        return Err(ShardVaultError::Transfer("Bukan berkas paket ShardVault (.svp) yang valid (Magic header salah)".into()));
    }

    let version = u32::from_le_bytes(package_bytes[4..8].try_into().unwrap());
    if version != FORMAT_VERSION {
        return Err(ShardVaultError::Transfer(format!("Versi format paket tidak didukung: v{}", version)));
    }

    let mut cursor = 8;
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&package_bytes[cursor..cursor + 16]);
    cursor += 16;

    let (kek, hmac_key) = derive_transfer_keys(passphrase, &salt);

    // 2. Verifikasi HMAC Tag di akhir file sebelum memproses apa pun
    let payload_without_hmac = &package_bytes[..package_bytes.len() - 32];
    let expected_hmac = &package_bytes[package_bytes.len() - 32..];

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&hmac_key)
        .map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    mac.update(payload_without_hmac);
    if mac.verify_slice(expected_hmac).is_err() {
        return Err(ShardVaultError::Transfer("Integritas paket rusak atau passphrase salah! (HMAC verification failed)".into()));
    }

    // 3. Baca Wrapped DEK
    let wrapped_dek_len = u32::from_le_bytes(package_bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
    cursor += 4;
    let mut dek_nonce = [0u8; 12];
    dek_nonce.copy_from_slice(&package_bytes[cursor..cursor + 12]);
    cursor += 12;
    let wrapped_dek = &package_bytes[cursor..cursor + wrapped_dek_len];
    cursor += wrapped_dek_len;

    let kek_cipher = Aes256Gcm::new_from_slice(&kek).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;
    let raw_dek = kek_cipher
        .decrypt(Nonce::from_slice(&dek_nonce), wrapped_dek)
        .map_err(|_| ShardVaultError::Transfer("Gagal membuka kunci transfer! Pastikan passphrase benar.".into()))?;

    let dek_cipher = Aes256Gcm::new_from_slice(&raw_dek).map_err(|e| ShardVaultError::Crypto(e.to_string()))?;

    // 4. Baca Metadata
    let meta_len = u32::from_le_bytes(package_bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
    cursor += 4;
    let mut meta_nonce = [0u8; 12];
    meta_nonce.copy_from_slice(&package_bytes[cursor..cursor + 12]);
    cursor += 12;
    let enc_meta = &package_bytes[cursor..cursor + meta_len];
    cursor += meta_len;

    let meta_compressed = dek_cipher
        .decrypt(Nonce::from_slice(&meta_nonce), enc_meta)
        .map_err(|e| ShardVaultError::Crypto(format!("Gagal dekripsi metadata: {}", e)))?;
    let meta_json = zstd::decode_all(Cursor::new(&meta_compressed))
        .map_err(|e| ShardVaultError::Transfer(format!("Gagal dekompresi metadata: {}", e)))?;
    let metadata: PackageMetadata = serde_json::from_slice(&meta_json)
        .map_err(|e| ShardVaultError::Transfer(format!("Gagal parsing metadata: {}", e)))?;

    // 5. Baca Payload Database
    let payload_len = u64::from_le_bytes(package_bytes[cursor..cursor + 8].try_into().unwrap()) as usize;
    cursor += 8;
    let mut payload_nonce = [0u8; 12];
    payload_nonce.copy_from_slice(&package_bytes[cursor..cursor + 12]);
    cursor += 12;
    let enc_payload = &package_bytes[cursor..cursor + payload_len];

    let payload_compressed = dek_cipher
        .decrypt(Nonce::from_slice(&payload_nonce), enc_payload)
        .map_err(|e| ShardVaultError::Crypto(format!("Gagal dekripsi payload database: {}", e)))?;
    let raw_sqlite_bytes = zstd::decode_all(Cursor::new(&payload_compressed))
        .map_err(|e| ShardVaultError::Transfer(format!("Gagal dekompresi snapshot database: {}", e)))?;

    Ok((metadata, raw_sqlite_bytes))
}
