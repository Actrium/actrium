//! Signer key encryption module
//!
//! Uses AES-256-GCM to encrypt and protect stored private keys

// Allow deprecated generic-array::from_slice until aes-gcm upgrades
#![allow(deprecated)]

use crate::error::{SignerError, SignerResult};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use base64::prelude::*;
use rand::RngCore;

/// KEK (Key Encryption Key) source
#[derive(Debug, Clone)]
pub enum KekSource {
    /// Read KEK directly from configuration file
    Direct(String),
    /// Read KEK from environment variable
    Environment(String),
    /// Read KEK from file path
    File(String),
}

/// Key encryptor
///
/// Encrypts/decrypts private keys using AES-256-GCM
/// Encryption format: base64(nonce[12] || ciphertext || tag[16])
#[derive(Clone)]
pub struct KeyEncryptor {
    cipher: Option<Aes256Gcm>,
}

impl std::fmt::Debug for KeyEncryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyEncryptor")
            .field("encryption_enabled", &self.is_enabled())
            .finish()
    }
}

impl KeyEncryptor {
    /// Create a no-encryption encryptor (compatibility mode)
    pub fn no_encryption() -> Self {
        crate::recording::debug!("Creating KeyEncryptor in no-encryption mode");
        Self { cipher: None }
    }

    /// Create an encryptor from a KEK source
    pub fn from_kek_source(source: &KekSource) -> SignerResult<Self> {
        let kek = match source {
            KekSource::Direct(key) => {
                crate::recording::debug!("Loading KEK from direct configuration");
                key.clone()
            }
            KekSource::Environment(env_var) => {
                crate::recording::debug!("Loading KEK from environment variable: {}", env_var);
                std::env::var(env_var).map_err(|e| {
                    SignerError::Config(format!(
                        "Failed to read KEK from environment variable {env_var}: {e}"
                    ))
                })?
            }
            KekSource::File(path) => {
                crate::recording::debug!("Loading KEK from file: {}", path);
                std::fs::read_to_string(path).map_err(|e| {
                    SignerError::Config(format!("Failed to read KEK from file {path}: {e}"))
                })?
            }
        };

        Self::from_kek(&kek)
    }

    /// Create an encryptor from a KEK string
    ///
    /// KEK can be:
    /// - 64-character hex string (32 bytes)
    /// - 44-character Base64 string (32 bytes)
    pub fn from_kek(kek: &str) -> SignerResult<Self> {
        let kek = kek.trim();

        // Try to parse as hex
        let key_bytes = if kek.len() == 64 {
            hex::decode(kek)
                .map_err(|e| SignerError::Config(format!("Invalid KEK hex format: {e}")))?
        } else if kek.len() == 44 || kek.len() == 43 {
            // Base64-encoded 32-byte key
            BASE64_STANDARD
                .decode(kek)
                .map_err(|e| SignerError::Config(format!("Invalid KEK base64 format: {e}")))?
        } else {
            return Err(SignerError::Config(format!(
                "Invalid KEK length: expected 64 hex chars or 44 base64 chars, got {}",
                kek.len()
            )));
        };

        if key_bytes.len() != 32 {
            return Err(SignerError::Config(format!(
                "Invalid KEK size: expected 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| SignerError::Crypto(format!("Failed to create cipher: {e}")))?;

        crate::recording::info!("KEK loaded successfully");
        Ok(Self {
            cipher: Some(cipher),
        })
    }

    /// Encrypt a private key
    ///
    /// If no KEK is configured, returns the original private key as-is (backward compatibility)
    /// Otherwise, encrypts using AES-256-GCM
    ///
    /// Encryption format: base64(nonce[12] || ciphertext || tag[16])
    pub fn encrypt(&self, secret_key: &str) -> SignerResult<String> {
        let cipher = match &self.cipher {
            Some(c) => c,
            None => {
                // No-encryption mode, return plaintext directly
                return Ok(secret_key.to_string());
            }
        };

        // Generate random nonce (12 bytes)
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, secret_key.as_bytes())
            .map_err(|e| SignerError::Crypto(format!("Encryption failed: {e}")))?;

        // Combine: nonce || ciphertext (includes tag)
        let mut encrypted = Vec::with_capacity(12 + ciphertext.len());
        encrypted.extend_from_slice(&nonce_bytes);
        encrypted.extend_from_slice(&ciphertext);

        // Base64 encode
        Ok(BASE64_STANDARD.encode(&encrypted))
    }

    /// Decrypt a private key
    ///
    /// If no KEK is configured, returns the original data as-is (backward compatibility)
    /// Otherwise, attempts Base64 decoding and AES-256-GCM decryption
    pub fn decrypt(&self, encrypted_key: &str) -> SignerResult<String> {
        let cipher = match &self.cipher {
            Some(c) => c,
            None => {
                // No-encryption mode, return plaintext directly
                return Ok(encrypted_key.to_string());
            }
        };

        // Base64 decode
        let encrypted_bytes = BASE64_STANDARD
            .decode(encrypted_key)
            .map_err(|e| SignerError::Crypto(format!("Invalid encrypted key format: {e}")))?;

        if encrypted_bytes.len() < 12 + 16 {
            return Err(SignerError::Crypto(format!(
                "Invalid encrypted key size: expected at least 28 bytes, got {}",
                encrypted_bytes.len()
            )));
        }

        // Split nonce and ciphertext
        let (nonce_bytes, ciphertext) = encrypted_bytes.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| SignerError::Crypto(format!("Decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| SignerError::Crypto(format!("Invalid UTF-8 after decryption: {e}")))
    }

    /// Whether encryption is enabled
    pub fn is_enabled(&self) -> bool {
        self.cipher.is_some()
    }

    /// Generate a new KEK (for initialization)
    ///
    /// Returns a 32-byte random key in hex format
    pub fn generate_kek() -> String {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        hex::encode(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_encryption_mode() {
        let encryptor = KeyEncryptor::no_encryption();
        assert!(!encryptor.is_enabled());

        let original = "secret-key-data";
        let encrypted = encryptor.encrypt(original).unwrap();
        assert_eq!(encrypted, original);

        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_encryption_decryption_hex_kek() {
        let kek = KeyEncryptor::generate_kek();
        let encryptor = KeyEncryptor::from_kek(&kek).unwrap();
        assert!(encryptor.is_enabled());

        let original = "my-secret-private-key-base64-encoded";
        let encrypted = encryptor.encrypt(original).unwrap();

        // After encryption, should differ from original
        assert_ne!(encrypted, original);

        // After decryption, should match original
        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_encryption_decryption_base64_kek() {
        // Generate a 32-byte key and encode as Base64
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        let kek_b64 = BASE64_STANDARD.encode(key);

        let encryptor = KeyEncryptor::from_kek(&kek_b64).unwrap();
        assert!(encryptor.is_enabled());

        let original = "another-secret-key";
        let encrypted = encryptor.encrypt(original).unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_invalid_kek_length() {
        let result = KeyEncryptor::from_kek("too-short");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid KEK length")
        );
    }

    #[test]
    fn test_invalid_kek_hex() {
        let invalid_hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let result = KeyEncryptor::from_kek(invalid_hex);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_with_wrong_kek() {
        let kek1 = KeyEncryptor::generate_kek();
        let encryptor1 = KeyEncryptor::from_kek(&kek1).unwrap();

        let original = "secret-data";
        let encrypted = encryptor1.encrypt(original).unwrap();

        // Attempt to decrypt with a different KEK
        let kek2 = KeyEncryptor::generate_kek();
        let encryptor2 = KeyEncryptor::from_kek(&kek2).unwrap();

        let result = encryptor2.decrypt(&encrypted);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );
    }

    #[test]
    fn test_decrypt_invalid_base64() {
        let kek = KeyEncryptor::generate_kek();
        let encryptor = KeyEncryptor::from_kek(&kek).unwrap();

        let result = encryptor.decrypt("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_too_short() {
        let kek = KeyEncryptor::generate_kek();
        let encryptor = KeyEncryptor::from_kek(&kek).unwrap();

        // Create encrypted data that is too short (fewer than 28 bytes)
        let short_data = BASE64_STANDARD.encode([0u8; 10]);
        let result = encryptor.decrypt(&short_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid encrypted key size")
        );
    }

    #[test]
    fn test_kek_from_environment() {
        let kek = KeyEncryptor::generate_kek();
        unsafe {
            std::env::set_var("TEST_KEK_ENV", &kek);
        }

        let encryptor =
            KeyEncryptor::from_kek_source(&KekSource::Environment("TEST_KEK_ENV".to_string()))
                .unwrap();
        assert!(encryptor.is_enabled());

        unsafe {
            std::env::remove_var("TEST_KEK_ENV");
        }
    }

    #[test]
    fn test_kek_from_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let kek = KeyEncryptor::generate_kek();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(kek.as_bytes()).unwrap();
        file.flush().unwrap();

        let encryptor = KeyEncryptor::from_kek_source(&KekSource::File(
            file.path().to_string_lossy().to_string(),
        ))
        .unwrap();
        assert!(encryptor.is_enabled());
    }

    #[test]
    fn test_generate_kek_format() {
        let kek = KeyEncryptor::generate_kek();
        // Should be 64 hex characters
        assert_eq!(kek.len(), 64);
        // Should successfully create an encryptor
        let encryptor = KeyEncryptor::from_kek(&kek).unwrap();
        assert!(encryptor.is_enabled());
    }
}
