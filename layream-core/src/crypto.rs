use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use sha2::{Digest, Sha256};

use crate::error::LayreamError;

const ZERO_IV: [u8; 12] = [0u8; 12];

fn derive_key(password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>, LayreamError> {
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| LayreamError::AesDecrypt)?;
    let nonce = Nonce::from_slice(&ZERO_IV);
    cipher.decrypt(nonce, data).map_err(|_| LayreamError::AesDecrypt)
}

pub fn encrypt(data: &[u8], password: &str) -> Result<Vec<u8>, LayreamError> {
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| LayreamError::AesEncrypt)?;
    let nonce = Nonce::from_slice(&ZERO_IV);
    cipher.encrypt(nonce, data).map_err(|_| LayreamError::AesEncrypt)
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
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_fails() {
        let encrypted = encrypt(b"secret", "correct").unwrap();
        assert!(decrypt(&encrypted, "wrong").is_err());
    }

    #[test]
    fn sha256_known_value() {
        let hash = sha256_hex(b"risupreset");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
