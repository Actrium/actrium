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

    /// Return the key named `name`, generating and storing it if missing
    ///
    /// Concurrent calls for the same name return the same key.
    async fn ensure_named_key(&self, name: &str) -> SignerResult<KeyPair>;

    /// Retire the key named `name`: destroy its private key so it never signs
    /// again, keeping the name and verifying key. A missing name is created
    /// retired, so the name can never get a live key. Idempotent; returns the
    /// key id.
    async fn retire_named_key(&self, name: &str) -> SignerResult<u32>;

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
    /// Total key count (including retired keys)
    async fn get_key_count(&self) -> SignerResult<u32>;
}
