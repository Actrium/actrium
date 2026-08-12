//! Signer storage module
//!
//! Provides multiple storage backend support: SQLite, PostgreSQL
//!
//! # Design
//!
//! - `KeyStorageBackend` trait defines the unified async interface
//! - `KeyStorage` enum encapsulates different backend implementations
//! - `StorageConfig` configures and initializes the backend
//! - Private keys never leave the storage backend; signing is performed internally

use std::path::Path;

pub mod backend;
pub mod config;

// SQLite is always available (uses sqlx)
pub mod sqlite;

#[cfg(feature = "backend-postgres")]
pub mod postgres;

use crate::crypto::KeyEncryptor;
use crate::error::{SignerError, SignerResult};
use crate::types::{KeyPair, KeyRecord};

pub use backend::KeyStorageBackend;
pub use config::{PostgresConfig, SqliteConfig, StorageBackend, StorageConfig};

use sqlite::SqliteBackend;

#[cfg(feature = "backend-postgres")]
use postgres::PostgresBackend;

/// Unified key storage interface
///
/// Benefits of using an enum instead of a trait object:
/// - Zero-cost abstraction (no virtual function calls)
/// - Can be Clone
/// - Compile-time type checking
#[derive(Clone, Debug)]
pub enum KeyStorage {
    /// SQLite storage backend (always available)
    Sqlite(Box<SqliteBackend>),

    /// PostgreSQL storage backend
    #[cfg(feature = "backend-postgres")]
    Postgres(Box<PostgresBackend>),
}

impl KeyStorage {
    /// Create a storage instance from configuration
    ///
    /// # Arguments
    /// * `config` - Storage configuration
    /// * `encryptor` - Key encryptor
    /// * `db_path` - Database file storage directory path (required when backend = "sqlite", from ActrixConfig.sqlite_path)
    ///
    /// # Returns
    /// Initialized storage instance
    ///
    /// # Errors
    /// - Missing configuration for the corresponding backend
    /// - Backend initialization failed
    /// - Backend feature not enabled (feature flag)
    pub async fn from_config<P: AsRef<Path>>(
        config: &StorageConfig,
        encryptor: KeyEncryptor,
        db_path: P,
    ) -> SignerResult<Self> {
        match config.backend {
            StorageBackend::Sqlite => {
                let cfg = config
                    .sqlite
                    .as_ref()
                    .ok_or_else(|| SignerError::Config("Missing SQLite config".into()))?;
                let backend =
                    SqliteBackend::new(cfg, config.key_ttl_seconds, encryptor, db_path.as_ref())
                        .await?;
                Ok(Self::Sqlite(Box::new(backend)))
            }

            #[cfg(feature = "backend-postgres")]
            StorageBackend::Postgres => {
                let cfg = config
                    .postgres
                    .as_ref()
                    .ok_or_else(|| SignerError::Config("Missing PostgreSQL config".into()))?;
                let backend = PostgresBackend::new(cfg, config.key_ttl_seconds, encryptor).await?;
                Ok(Self::Postgres(Box::new(backend)))
            }

            #[cfg(not(feature = "backend-postgres"))]
            StorageBackend::Postgres => Err(SignerError::Config(
                "PostgreSQL backend not enabled. Compile with --features backend-postgres".into(),
            )),
        }
    }

    /// Generate and store a new Ed25519 signing key pair
    pub async fn generate_and_store_key(&self) -> SignerResult<KeyPair> {
        match self {
            Self::Sqlite(b) => b.generate_and_store_key().await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.generate_and_store_key().await,
        }
    }

    /// Query the verifying public key by key_id
    pub async fn get_public_key(&self, key_id: u32) -> SignerResult<Option<String>> {
        match self {
            Self::Sqlite(b) => b.get_public_key(key_id).await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.get_public_key(key_id).await,
        }
    }

    /// Sign a message with the specified key (Ed25519)
    ///
    /// Private key does not leave the storage backend; signing is performed internally
    pub async fn sign(&self, key_id: u32, message: &[u8]) -> SignerResult<Vec<u8>> {
        match self {
            Self::Sqlite(b) => b.sign(key_id, message).await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.sign(key_id, message).await,
        }
    }

    /// Get the complete key record
    pub async fn get_key_record(&self, key_id: u32) -> SignerResult<Option<KeyRecord>> {
        match self {
            Self::Sqlite(b) => b.get_key_record(key_id).await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.get_key_record(key_id).await,
        }
    }

    /// Get the total key count
    pub async fn get_key_count(&self) -> SignerResult<u32> {
        match self {
            Self::Sqlite(b) => b.get_key_count().await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.get_key_count().await,
        }
    }

    /// Clean up expired keys (only delete keys beyond the tolerance period)
    pub async fn cleanup_expired_keys(&self, tolerance_seconds: u64) -> SignerResult<u32> {
        match self {
            Self::Sqlite(b) => b.cleanup_expired_keys(tolerance_seconds).await,

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(b) => b.cleanup_expired_keys(tolerance_seconds).await,
        }
    }

    /// Get the backend type name
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "SQLite",

            #[cfg(feature = "backend-postgres")]
            Self::Postgres(_) => "Postgres",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_storage_from_config_sqlite() {
        let temp_dir = tempdir().unwrap();
        let config = StorageConfig {
            backend: StorageBackend::Sqlite,
            key_ttl_seconds: 3600,
            sqlite: Some(SqliteConfig {}),
            postgres: None,
        };

        let storage = KeyStorage::from_config(
            &config,
            crate::crypto::KeyEncryptor::no_encryption(),
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(storage.backend_name(), "SQLite");

        // Test basic operations
        let key_pair = storage.generate_and_store_key().await.unwrap();
        assert!(key_pair.key_id > 0);

        let public_key = storage.get_public_key(key_pair.key_id).await.unwrap();
        assert_eq!(public_key, Some(key_pair.verifying_key));
    }

    #[tokio::test]
    async fn test_missing_backend_config() {
        let config = StorageConfig {
            backend: StorageBackend::Sqlite,
            key_ttl_seconds: 3600,
            sqlite: None, // missing configuration
            postgres: None,
        };

        let temp_dir = tempdir().unwrap();
        let result = KeyStorage::from_config(
            &config,
            crate::crypto::KeyEncryptor::no_encryption(),
            temp_dir.path(),
        )
        .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing SQLite config")
        );
    }
}
