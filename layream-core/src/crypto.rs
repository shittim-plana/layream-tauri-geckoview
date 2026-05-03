use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::LayreamError;

const NONCE_LEN: usize = 12;

fn derive_key(password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

pub fn encrypt(data: &[u8], password: &str) -> Result<Vec<u8>, LayreamError> {
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| LayreamError::AesEncrypt)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let encrypted = cipher.encrypt(nonce, data).map_err(|_| LayreamError::AesEncrypt)?;
    let mut result = nonce_bytes.to_vec();
    result.extend(encrypted);
    Ok(result)
}

pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>, LayreamError> {
    if data.len() < NONCE_LEN {
        return Err(LayreamError::AesDecrypt);
    }
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| LayreamError::AesDecrypt)?;

    // Try new format first: nonce (12 bytes) || ciphertext
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    if let Ok(plaintext) = cipher.decrypt(nonce, ciphertext) {
        return Ok(plaintext);
    }

    // Fall back to legacy zero-nonce format for existing .risup files and old token stores
    let zero_nonce = Nonce::from_slice(&[0u8; NONCE_LEN]);
    cipher
        .decrypt(zero_nonce, data)
        .map_err(|_| LayreamError::AesDecrypt)
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let plaintext = b"hello layream";
        let password = "risupreset";
        let encrypted = encrypt(plaintext, password).unwrap();
        // Ciphertext must be longer than plaintext by at least NONCE_LEN + GCM tag (16)
        assert!(encrypted.len() >= plaintext.len() + NONCE_LEN + 16);
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn unique_nonces() {
        let encrypted1 = encrypt(b"same data", "same key").unwrap();
        let encrypted2 = encrypt(b"same data", "same key").unwrap();
        // The prepended nonces (first 12 bytes) should differ
        assert_ne!(&encrypted1[..NONCE_LEN], &encrypted2[..NONCE_LEN]);
    }

    #[test]
    fn wrong_password_fails() {
        let encrypted = encrypt(b"secret", "correct").unwrap();
        assert!(decrypt(&encrypted, "wrong").is_err());
    }

    #[test]
    fn data_too_short_fails() {
        assert!(decrypt(&[0u8; 11], "password").is_err());
        assert!(decrypt(&[], "password").is_err());
    }

    #[test]
    fn sha256_known_value() {
        let hash = sha256_hex(b"risupreset");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
