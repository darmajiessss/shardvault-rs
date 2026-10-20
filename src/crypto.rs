// E:\shardvault-rs\src\crypto.rs
// Modul Kriptografi Standar Enterprise: AES-256-GCM + PBKDF2 + HMAC-SHA256

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

use crate::error::ShardVaultError;

type HmacSha256 = Hmac<Sha256>;

/// Konversi byte slice ke string heksadesimal
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Konversi string heksadesimal ke byte vector
pub fn from_hex(s: &str) -> Result<Vec<u8>, ShardVaultError> {
    if !s.len().is_multiple_of(2) {
        return Err(ShardVaultError::Crypto("Hex string length must be even".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| ShardVaultError::Crypto(format!("Invalid hex char at index {}: {}", i, e)))
        })
        .collect()
}

/// Derivasi Kunci Master 256-bit (32 byte) terikat perangkat (*Machine-Bound Master Key*)
/// Menggunakan kombinasi identifier hardware + salt aplikasi melalui PBKDF2-HMAC-SHA256 (100.000 iterasi)
pub fn derive_machine_key(salt: &str) -> [u8; 32] {
    let machine_id = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "SHARDVAULT_NODE".into());

    let cpu_count = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let raw_secret = format!("{}:{}:cpu_{}", machine_id, salt, cpu_count);
    let mut key = [0u8; 32];
    
    pbkdf2_hmac::<Sha256>(raw_secret.as_bytes(), salt.as_bytes(), 100_000, &mut key);
    key
}

/// Enkripsi data biner menggunakan AES-256-GCM (Authenticated Encryption with Associated Data)
/// Format output: "v1:{12_byte_nonce_hex}:{ciphertext_and_16_byte_tag_hex}"
pub fn encrypt_bytes(plaintext: &[u8], key: &[u8; 32]) -> Result<String, ShardVaultError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| ShardVaultError::Crypto(format!("Failed to init AES-256-GCM cipher: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| ShardVaultError::Crypto(format!("CSPRNG error generating nonce: {}", e)))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| ShardVaultError::Crypto(format!("AES-GCM encryption failed: {}", e)))?;

    Ok(format!("v1:{}:{}", to_hex(&nonce_bytes), to_hex(&ciphertext)))
}

/// Dekripsi data biner AES-256-GCM dan verifikasi integritas 16-byte Poly1305/GHASH tag
pub fn decrypt_bytes(encrypted_str: &str, key: &[u8; 32]) -> Result<Vec<u8>, ShardVaultError> {
    if !encrypted_str.starts_with("v1:") {
        return Err(ShardVaultError::Crypto("Unsupported encryption payload format (expected v1)".into()));
    }

    let parts: Vec<&str> = encrypted_str.split(':').collect();
    if parts.len() != 3 {
        return Err(ShardVaultError::Crypto("Invalid ciphertext structure (expected v1:nonce:ciphertext)".into()));
    }

    let nonce_bytes = from_hex(parts[1])?;
    if nonce_bytes.len() != 12 {
        return Err(ShardVaultError::Crypto("Invalid nonce length (must be 12 bytes)".into()));
    }

    let ciphertext_with_tag = from_hex(parts[2])?;
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| ShardVaultError::Crypto(format!("Failed to init AES-256-GCM cipher: {}", e)))?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext_with_tag.as_ref())
        .map_err(|_| ShardVaultError::Crypto("Authentication tag mismatch! Data was corrupted or tampered.".into()))?;

    Ok(plaintext)
}

/// Helper untuk enkripsi string teks langsung
pub fn encrypt_string(plaintext: &str, key: &[u8; 32]) -> Result<String, ShardVaultError> {
    encrypt_bytes(plaintext.as_bytes(), key)
}

/// Helper untuk dekripsi string teks langsung
pub fn decrypt_string(encrypted_str: &str, key: &[u8; 32]) -> Result<String, ShardVaultError> {
    let bytes = decrypt_bytes(encrypted_str, key)?;
    String::from_utf8(bytes).map_err(|e| ShardVaultError::Crypto(format!("UTF-8 decode error: {}", e)))
}

/// Menghasilkan tanda tangan HMAC-SHA256 dari string kanonikal
pub fn generate_signature(canonical_content: &str, key: &[u8; 32]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts 32-byte key");
    mac.update(canonical_content.as_bytes());
    to_hex(&mac.finalize().into_bytes())
}

/// Memverifikasi tanda tangan HMAC-SHA256 terhadap manipulasi
pub fn verify_signature(canonical_content: &str, expected_sig: &str, key: &[u8; 32]) -> bool {
    let actual_sig = generate_signature(canonical_content, key);
    actual_sig.eq_ignore_ascii_case(expected_sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let key = derive_machine_key("test_salt_123");
        let secret_text = "HPP: Rp 450.000 / Meter Kain Sutra";

        let encrypted = encrypt_string(secret_text, &key).expect("Enkripsi harus sukses");
        assert!(encrypted.starts_with("v1:"));

        let decrypted = decrypt_string(&encrypted, &key).expect("Dekripsi harus sukses");
        assert_eq!(decrypted, secret_text);
    }

    #[test]
    fn test_tamper_detection() {
        let key = derive_machine_key("test_salt_123");
        let secret_text = "Sensitive Financial Ledger";
        let encrypted = encrypt_string(secret_text, &key).unwrap();

        // Modifikasi karakter pada ciphertext
        let mut tampered = encrypted.clone();
        if tampered.ends_with('a') {
            tampered.pop();
            tampered.push('b');
        } else {
            tampered.pop();
            tampered.push('a');
        }

        let result = decrypt_string(&tampered, &key);
        assert!(result.is_err(), "Ciphertext yang dimanipulasi harus ditolak!");
    }

    #[test]
    fn test_signature_verification() {
        let key = derive_machine_key("test_salt_123");
        let payload = "invoice:INV-9901:amount:1500000:cashier:Budi";
        let sig = generate_signature(payload, &key);

        assert!(verify_signature(payload, &sig, &key));
        assert!(!verify_signature("invoice:INV-9901:amount:100:cashier:Budi", &sig, &key));
    }
}
