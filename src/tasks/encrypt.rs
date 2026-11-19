//! Enkripsi arsip dengan AES-256-GCM dan kunci turunan PBKDF2-HMAC-SHA256.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

use crate::config::EncryptionConfig;
use crate::error::{Error, Result};

/// Penanda arsip terenkripsi pada awal berkas.
pub const MAGIC: &[u8; 8] = b"BKRSENC1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Memeriksa apakah data diawali penanda arsip terenkripsi.
pub fn is_encrypted(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

/// Mengenkripsi arsip. Header berisi magic, salt acak, dan nonce acak.
pub fn encrypt(plain: &[u8], config: &EncryptionConfig) -> Result<Vec<u8>> {
    let (salt, nonce) = random_header();
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(
        config.password.as_bytes(),
        &salt,
        config.iterations.max(1),
        &mut key,
    );
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| Error::Encryption("kunci tidak valid".to_string()))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|_| Error::Encryption("gagal mengenkripsi arsip".to_string()))?;

    let mut output = Vec::with_capacity(MAGIC.len() + SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Mendekripsi arsip. Password salah atau data rusak menghasilkan
/// [`Error::Encryption`].
pub fn decrypt(data: &[u8], config: &EncryptionConfig) -> Result<Vec<u8>> {
    let header_len = MAGIC.len() + SALT_LEN + NONCE_LEN;
    if data.len() < header_len || !is_encrypted(data) {
        return Err(Error::Encryption(
            "arsip tidak memiliki header enkripsi yang valid".to_string(),
        ));
    }
    let salt = &data[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let nonce = &data[MAGIC.len() + SALT_LEN..header_len];
    let ciphertext = &data[header_len..];

    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(
        config.password.as_bytes(),
        salt,
        config.iterations.max(1),
        &mut key,
    );
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| Error::Encryption("kunci tidak valid".to_string()))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| Error::Encryption("password salah atau arsip rusak".to_string()))
}

fn random_header() -> ([u8; SALT_LEN], [u8; NONCE_LEN]) {
    let first = *uuid::Uuid::new_v4().as_bytes();
    let second = *uuid::Uuid::new_v4().as_bytes();
    let mut entropy = [0u8; 32];
    entropy[..16].copy_from_slice(&first);
    entropy[16..].copy_from_slice(&second);

    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&entropy[..SALT_LEN]);
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&entropy[SALT_LEN..SALT_LEN + NONCE_LEN]);
    (salt, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip_and_wrong_password() {
        let config = EncryptionConfig::new("correct horse").with_iterations(1_000);
        let plain = b"data rahasia";

        let encrypted = encrypt(plain, &config).unwrap();
        assert!(is_encrypted(&encrypted));
        assert_ne!(&encrypted[..2], &[0x1f, 0x8b]);
        assert_eq!(decrypt(&encrypted, &config).unwrap(), plain);

        let wrong = EncryptionConfig::new("wrong").with_iterations(1_000);
        assert!(matches!(
            decrypt(&encrypted, &wrong),
            Err(Error::Encryption(_))
        ));
    }
}
