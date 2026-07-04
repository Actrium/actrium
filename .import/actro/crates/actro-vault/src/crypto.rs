use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

use crate::model::{VaultCoreError, VaultField};

pub const PBKDF2_ITERATIONS: u32 = 100_000;
pub const KEY_BYTE_LEN: usize = 32;
pub const SALT_BYTE_LEN: usize = 32;
pub const NONCE_BYTE_LEN: usize = 12;
pub const TAG_BYTE_LEN: usize = 16;
pub const VERIFICATION_TOKEN: &str = "vault-verification-token";

pub type VaultKey = [u8; KEY_BYTE_LEN];

pub fn generate_salt() -> [u8; SALT_BYTE_LEN] {
    let mut salt = [0_u8; SALT_BYTE_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn generate_salt_base64() -> String {
    STANDARD.encode(generate_salt())
}

pub fn derive_key(password: &str, salt: &[u8]) -> Result<VaultKey, VaultCoreError> {
    if salt.len() != SALT_BYTE_LEN {
        return Err(VaultCoreError::InvalidSalt);
    }

    Ok(derive_key_with_iterations(
        password,
        salt,
        PBKDF2_ITERATIONS,
    ))
}

pub fn derive_key_base64(password: &str, salt_base64: &str) -> Result<String, VaultCoreError> {
    let salt = decode_base64(salt_base64)?;
    let key = derive_key(password, &salt)?;
    Ok(STANDARD.encode(key))
}

pub fn encrypt_base64(key: &[u8], plaintext: &str) -> Result<String, VaultCoreError> {
    let mut nonce = [0_u8; NONCE_BYTE_LEN];
    OsRng.fill_bytes(&mut nonce);
    encrypt_base64_with_nonce(key, &nonce, plaintext)
}

pub fn encrypt_base64_with_nonce(
    key: &[u8],
    nonce: &[u8],
    plaintext: &str,
) -> Result<String, VaultCoreError> {
    let cipher = cipher_from_key(key)?;
    if nonce.len() != NONCE_BYTE_LEN {
        return Err(VaultCoreError::InvalidNonce);
    }

    let encrypted = cipher
        .encrypt(Nonce::from_slice(nonce), plaintext.as_bytes())
        .map_err(|_| VaultCoreError::EncryptionFailed)?;

    let mut combined = Vec::with_capacity(NONCE_BYTE_LEN + encrypted.len());
    combined.extend_from_slice(nonce);
    combined.extend_from_slice(&encrypted);
    Ok(STANDARD.encode(combined))
}

pub fn decrypt_base64(key: &[u8], ciphertext_base64: &str) -> Result<String, VaultCoreError> {
    let cipher = cipher_from_key(key)?;
    let combined = decode_base64(ciphertext_base64)?;
    if combined.len() < NONCE_BYTE_LEN + TAG_BYTE_LEN {
        return Err(VaultCoreError::InvalidCiphertext);
    }

    let (nonce, ciphertext_and_tag) = combined.split_at(NONCE_BYTE_LEN);
    let decrypted = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext_and_tag)
        .map_err(|_| VaultCoreError::DecryptionFailed)?;

    String::from_utf8(decrypted).map_err(|_| VaultCoreError::InvalidUtf8)
}

pub fn create_verification_hash(
    password: &str,
    salt_base64: &str,
) -> Result<String, VaultCoreError> {
    let key = decode_key_base64(&derive_key_base64(password, salt_base64)?)?;
    encrypt_base64(&key, VERIFICATION_TOKEN)
}

pub fn verify_password(
    password: &str,
    salt_base64: &str,
    verification_hash_base64: &str,
) -> Result<bool, VaultCoreError> {
    let key = decode_key_base64(&derive_key_base64(password, salt_base64)?)?;
    match decrypt_base64(&key, verification_hash_base64) {
        Ok(value) => Ok(value == VERIFICATION_TOKEN),
        Err(VaultCoreError::DecryptionFailed | VaultCoreError::InvalidCiphertext) => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn encrypt_fields_base64(key: &[u8], fields: &[VaultField]) -> Result<String, VaultCoreError> {
    let json = serde_json::to_string(fields).map_err(|_| VaultCoreError::InvalidJson)?;
    encrypt_base64(key, &json)
}

pub fn decrypt_fields_base64(
    key: &[u8],
    ciphertext_base64: &str,
) -> Result<Vec<VaultField>, VaultCoreError> {
    let json = decrypt_base64(key, ciphertext_base64)?;
    serde_json::from_str(&json).map_err(|_| VaultCoreError::InvalidJson)
}

pub fn decode_key_base64(key_base64: &str) -> Result<VaultKey, VaultCoreError> {
    let bytes = decode_base64(key_base64)?;
    if bytes.len() != KEY_BYTE_LEN {
        return Err(VaultCoreError::InvalidKey);
    }

    let mut key = [0_u8; KEY_BYTE_LEN];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn cipher_from_key(key: &[u8]) -> Result<Aes256Gcm, VaultCoreError> {
    Aes256Gcm::new_from_slice(key).map_err(|_| VaultCoreError::InvalidKey)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, VaultCoreError> {
    STANDARD
        .decode(value)
        .map_err(|_| VaultCoreError::InvalidBase64)
}

fn derive_key_with_iterations(password: &str, salt: &[u8], iterations: u32) -> VaultKey {
    let mut key = [0_u8; KEY_BYTE_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut key);
    key
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    use super::*;

    #[test]
    fn pbkdf2_sha256_matches_known_vector() {
        let key = derive_key_with_iterations("password", b"salt", 1);
        assert_eq!(
            hex(&key),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
    }

    #[test]
    fn aes_gcm_combined_layout_matches_cryptokit() {
        let key = [7_u8; KEY_BYTE_LEN];
        let nonce = [9_u8; NONCE_BYTE_LEN];
        let plaintext = "vault-verification-token";

        let ciphertext = encrypt_base64_with_nonce(&key, &nonce, plaintext).unwrap();
        let combined = STANDARD.decode(ciphertext).unwrap();

        assert_eq!(&combined[..NONCE_BYTE_LEN], nonce);
        assert_eq!(
            combined.len(),
            NONCE_BYTE_LEN + plaintext.len() + TAG_BYTE_LEN
        );
        assert_eq!(
            decrypt_base64(&key, &STANDARD.encode(combined)).unwrap(),
            plaintext
        );
    }

    #[test]
    fn wrong_password_verification_returns_false() {
        let salt = STANDARD.encode([3_u8; SALT_BYTE_LEN]);
        let key = derive_key("correct", &[3_u8; SALT_BYTE_LEN]).unwrap();
        let verification =
            encrypt_base64_with_nonce(&key, &[4_u8; NONCE_BYTE_LEN], VERIFICATION_TOKEN).unwrap();

        assert!(verify_password("correct", &salt, &verification).unwrap());
        assert!(!verify_password("wrong", &salt, &verification).unwrap());
    }

    #[test]
    fn field_encryption_roundtrips_camel_case_json() {
        let key = [1_u8; KEY_BYTE_LEN];
        let fields = vec![VaultField {
            id: "field-1".to_string(),
            label: "Password".to_string(),
            value: "secret".to_string(),
            is_sensitive: true,
        }];

        let json = serde_json::to_string(&fields).unwrap();
        assert_eq!(
            json,
            r#"[{"id":"field-1","label":"Password","value":"secret","isSensitive":true}]"#
        );

        let encrypted = encrypt_base64_with_nonce(&key, &[2_u8; NONCE_BYTE_LEN], &json).unwrap();
        assert_eq!(decrypt_fields_base64(&key, &encrypted).unwrap(), fields);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
