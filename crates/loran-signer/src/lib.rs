//! Signer - Ed25519 signing key generation and management service
//!
//! The Signer service provides the following capabilities:
//! 1. Keep named Ed25519 signing keys; the private key never leaves the Signer
//! 2. Expose a Sign API to perform signing operations on behalf of callers
//! 3. PSK signature verification and replay attack protection
//! 4. Multiple storage backend support: SQLite, PostgreSQL
#![deny(clippy::disallowed_macros)]

#[cfg(test)]
pub mod client;
pub mod config;
pub mod crypto;
pub mod error;
pub mod grpc_client;
pub mod grpc_handlers;
pub mod handlers;
pub mod recording;
pub mod storage;
pub mod types;

// Re-export commonly used items
#[cfg(test)]
pub use client::{Client, ClientConfig};
pub use config::SignerServiceConfig;
pub use crypto::{KekSource, KeyEncryptor};
pub use error::SignerError;
pub use grpc_client::{GrpcClient, GrpcClientConfig};
pub use grpc_handlers::{SignerGrpcService, create_grpc_service};
// Re-export proto types from loran-proto
pub use handlers::{
    SignerState, create_router, create_signer_state, get_stats, register_signer_metrics,
};
pub use loran_proto::signer::v1::signer_server::{Signer, SignerServer};
pub use storage::{KeyStorage, StorageConfig};
pub use types::{KeyPair, KeyRecord, SignRequest, SignResponse};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{SqliteConfig, StorageBackend, StorageConfig};
    use nonce_auth::storage::MemoryStorage;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_signer_service_creation() {
        let temp_dir = tempdir().unwrap();

        let config = SignerServiceConfig {
            storage: StorageConfig {
                backend: StorageBackend::Sqlite,
                sqlite: Some(SqliteConfig::default()),
                postgres: None,
            },
            kek: None,
            kek_env: None,
            kek_file: None,
            ..Default::default()
        };

        // Use in-memory storage for testing (avoid filesystem dependency)
        let nonce_storage = MemoryStorage::new();
        let state = create_signer_state(
            &config,
            nonce_storage,
            "test-loran-shared-key",
            temp_dir.path(),
        )
        .await;
        assert!(state.is_ok());
    }
}
