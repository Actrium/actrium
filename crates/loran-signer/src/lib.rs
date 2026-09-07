//! Signer - Ed25519 signing key generation and management service
//!
//! The Signer service provides the following capabilities:
//! 1. Generate Ed25519 signing key pairs; the private key never leaves the Signer
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

/// Whether a signing key with this `expires_at` may still mint signatures at
/// `now` (both Unix seconds).
///
/// This is the single expiry decision shared by the gRPC and HTTP Sign
/// handlers, and it deliberately ignores the tolerance window: verifiers
/// verifier caches drop verifying keys at `expires_at` without any
/// tolerance, so a signature created at or after that instant yields a
/// credential that fails every handshake with 401. Tolerance only governs how
/// long key *material* stays
/// readable (`get_verifying_key`, storage cleanup), never minting.
///
/// `expires_at == 0` means the key never expires.
pub(crate) fn key_expired_for_signing(expires_at: u64, now: u64) -> bool {
    expires_at > 0 && expires_at <= now
}

// Re-export commonly used items
#[cfg(test)]
pub use client::{Client, ClientConfig};
pub use config::SignerServiceConfig;
pub use crypto::{KekSource, KeyEncryptor};
pub use error::SignerError;
pub use grpc_client::{GrpcClient, GrpcClientConfig};
pub use grpc_handlers::{SignerGrpcService, create_grpc_service};
// Re-export proto types from actrix-proto
pub use actrix_proto::signer::v1::signer_server::{Signer, SignerServer};
pub use handlers::{
    KEY_CLEANUP_INTERVAL, SignerState, create_router, create_signer_state, get_stats,
    register_signer_metrics, run_key_cleanup,
};
pub use storage::{KeyStorage, StorageConfig};
pub use types::{
    GenerateSigningKeyRequest, GenerateSigningKeyResponse, KeyPair, KeyRecord, SignRequest,
    SignResponse,
};

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
                key_ttl_seconds: 3600,
                sqlite: Some(SqliteConfig::default()),
                postgres: None,
            },
            kek: None,
            kek_env: None,
            kek_file: None,
            tolerance_seconds: 3600,
            ..Default::default()
        };

        // Use in-memory storage for testing (avoid filesystem dependency)
        let nonce_storage = MemoryStorage::new();
        let state = create_signer_state(
            &config,
            nonce_storage,
            "test-actrix-shared-key",
            temp_dir.path(),
        )
        .await;
        assert!(state.is_ok());
    }

    #[test]
    fn test_key_expired_for_signing_never_expires() {
        assert!(!key_expired_for_signing(0, u64::MAX));
        assert!(!key_expired_for_signing(0, 0));
    }

    #[test]
    fn test_key_expired_for_signing_boundary() {
        let now = 1_000_000u64;
        // Still valid one second before expiry, expired exactly at expiry,
        // and expired long ago (even inside any tolerance window).
        assert!(!key_expired_for_signing(now + 1, now));
        assert!(key_expired_for_signing(now, now));
        assert!(key_expired_for_signing(now - 1, now));
    }
}
