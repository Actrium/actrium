//! Signer service data type definitions

use nonce_auth::NonceCredential;
use serde::{Deserialize, Serialize};

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
    /// Expiration time (Unix timestamp)
    pub expires_at: u64,
    /// Tolerance period (seconds)
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
    /// Expiration time (Unix timestamp)
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
        format!("sign:{}", self.key_id)
    }
}
