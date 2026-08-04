//! Key storage backend abstract interface
//!
//! Defines the unified async interface that all storage backends must implement

use crate::error::SignerResult;
use crate::types::{KeyPair, KeyRecord};
use async_trait::async_trait;

/// Key storage backend abstract interface
///
/// All storage backends (SQLite, PostgreSQL) must implement this trait
/// Provides a unified async API for key generation, storage, querying, and management
/// Private keys never leave the storage backend; all signing operations are performed internally
#[async_trait]
pub trait KeyStorageBackend: Send + Sync {
    /// Initialize the storage backend
    ///
    /// Performs necessary initialization operations such as creating tables, indexes, etc.
    async fn init(&self) -> SignerResult<()>;

    /// Generate and store a new Ed25519 signing key pair
    ///
    /// Automatically generates an Ed25519 key pair, stores it in the backend, and returns a structure containing key_id and verifying key
    /// The private key (signing key) is only stored in the backend and not returned to the caller
    ///
    /// # Returns
    /// Key pair structure containing key_id and verifying_key (Base64-encoded 32-byte verifying public key)
    async fn generate_and_store_key(&self) -> SignerResult<KeyPair>;

    /// Query the verifying public key by key_id
    ///
    /// # Arguments
    /// * `key_id` - Key ID
    ///
    /// # Returns
    /// * `Ok(Some(verifying_key))` - Verifying public key found (Base64-encoded)
    /// * `Ok(None)` - Key does not exist
    /// * `Err(...)` - Storage error
    async fn get_public_key(&self, key_id: u32) -> SignerResult<Option<String>>;

    /// Sign a message with the specified key (Ed25519)
    ///
    /// The private key is read from storage and decrypted; signing is performed internally; the raw private key is not returned to the caller
    ///
    /// # Arguments
    /// * `key_id` - Key ID
    /// * `message` - Message bytes to sign
    ///
    /// # Returns
    /// * `Ok(signature)` - 64-byte Ed25519 signature
    /// * `Err(SignerError::NotFound)` - Key does not exist
    /// * `Err(...)` - Storage or encryption error
    async fn sign(&self, key_id: u32, message: &[u8]) -> SignerResult<Vec<u8>>;

    /// Get the complete key record (including metadata)
    ///
    /// # Arguments
    /// * `key_id` - Key ID
    ///
    /// # Returns
    /// * `Ok(Some(record))` - Key record found
    /// * `Ok(None)` - Key does not exist
    /// * `Err(...)` - Storage error
    async fn get_key_record(&self, key_id: u32) -> SignerResult<Option<KeyRecord>>;

    /// Get the total number of keys in storage
    ///
    /// # Returns
    /// Total key count (including expired and non-expired keys)
    async fn get_key_count(&self) -> SignerResult<u32>;

    /// Clean up expired keys
    ///
    /// Deletes all key records that have expired and exceeded the tolerance period
    /// (expires_at > 0 and expires_at + tolerance_seconds < current time)
    ///
    /// # Arguments
    /// * `tolerance_seconds` - Tolerance period after expiration (seconds)
    ///
    /// # Returns
    /// Number of keys cleaned up
    async fn cleanup_expired_keys(&self, tolerance_seconds: u64) -> SignerResult<u32>;
}
