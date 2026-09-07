//! Signer service data type definitions

use nonce_auth::NonceCredential;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SIGN_REQUEST_AUTH_VERSION: &str = "actrix-signer-v1";

/// Build the canonical nonce-auth payload for a signing request.
///
/// Binding the message digest prevents an authenticated request from being
/// modified into a request to sign different bytes.
pub fn sign_request_payload(key_id: u32, message: &[u8]) -> String {
    let message_digest = Sha256::digest(message);
    format!(
        "{SIGN_REQUEST_AUTH_VERSION}:sign:{key_id}:sha256:{}",
        hex::encode(message_digest)
    )
}

/// Key pair structure (internal use, stores public key reference)
#[derive(Debug, Clone)]
pub struct KeyPair {
    /// Key ID
    pub key_id: u32,
    /// Verifying public key (Base64-encoded Ed25519 verifying key, 32 bytes)
    pub verifying_key: String,
}

/// Generate signing key request
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateSigningKeyRequest {
    /// nonce-auth credential
    pub credential: NonceCredential,
}

/// Generate signing key response
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateSigningKeyResponse {
    /// Generated key ID
    pub key_id: u32,
    /// Verifying public key (Base64-encoded Ed25519 verifying key, 32 bytes)
    pub verifying_key: String,
    /// Expiry time (Unix timestamp)
    pub expires_at: u64,
    /// Tolerance window (seconds)
    pub tolerance_seconds: u64,
}

/// Sign request
#[derive(Debug, Serialize, Deserialize)]
pub struct SignRequest {
    /// Key ID to use
    pub key_id: u32,
    /// Message to sign (raw bytes, base64-encoded for HTTP transport)
    pub message: Vec<u8>,
    /// nonce-auth credential
    pub credential: NonceCredential,
}

/// Sign response
#[derive(Debug, Serialize, Deserialize)]
pub struct SignResponse {
    /// Ed25519 signature (64 bytes)
    pub signature: Vec<u8>,
}

/// Key record stored in the database
#[derive(Debug, Clone, PartialEq)]
pub struct KeyRecord {
    /// Key ID
    pub key_id: u32,
    /// Verifying public key (Base64-encoded Ed25519 verifying key)
    pub public_key: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Expiry time (Unix timestamp)
    pub expires_at: u64,
}

impl GenerateSigningKeyRequest {
    /// Get the request data used for verification
    pub fn request_payload(&self) -> String {
        "generate_signing_key".to_string()
    }
}

impl SignRequest {
    /// Get the request data used for verification
    pub fn request_payload(&self) -> String {
        sign_request_payload(self.key_id, &self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::sign_request_payload;

    #[test]
    fn sign_auth_payload_binds_key_and_message() {
        let first = sign_request_payload(7, b"first");
        let second = sign_request_payload(7, b"second");
        let other_key = sign_request_payload(8, b"first");

        assert_ne!(first, second);
        assert_ne!(first, other_key);
        assert_eq!(first, sign_request_payload(7, b"first"));
        assert!(!first.contains("first"));
    }
}
