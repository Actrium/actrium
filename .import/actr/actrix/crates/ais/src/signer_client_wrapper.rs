//! Signer client wrapper
//!
//! Provides a unified KS client interface, supporting gRPC clients (requires &mut self)

use platform::aid::AidError;
use signer::{GrpcClient, GrpcClientConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Signer client wrapper (for gRPC client)
#[derive(Clone)]
pub struct SignerClientWrapper {
    inner: Arc<RwLock<Option<GrpcClient>>>,
    grpc_config: GrpcClientConfig,
}

impl SignerClientWrapper {
    /// Create a new Signer client wrapper
    pub fn new(grpc_config: GrpcClientConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            grpc_config,
        }
    }

    /// Request a new Ed25519 signing key from KS; returns (key_id, verifying_key_bytes[32], expires_at, tolerance_secs).
    /// The private key is retained on the KS server.
    pub async fn generate_signing_key(
        &self,
    ) -> Result<(u32, [u8; 32], u64, u64), signer::SignerError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let client = GrpcClient::new(&self.grpc_config).await?;
            *guard = Some(client);
        }
        guard
            .as_mut()
            .expect("grpc client must exist after lazy init")
            .generate_signing_key()
            .await
    }

    /// Sign a message with the Ed25519 key in KS; returns a 64-byte signature.
    /// The private key never leaves the KS service.
    pub async fn sign(&self, key_id: u32, message: &[u8]) -> Result<Vec<u8>, signer::SignerError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let client = GrpcClient::new(&self.grpc_config).await?;
            *guard = Some(client);
        }
        let result = guard
            .as_mut()
            .expect("grpc client must exist after lazy init")
            .sign(key_id, message)
            .await;

        match result {
            Ok(signature) => Ok(signature),
            Err(error) if is_retryable_transport_error(&error) => {
                platform::recording::warn!(
                    "Signer gRPC transport failed for key_id={}; rebuilding the client and retrying once",
                    key_id
                );
                *guard = None;
                tokio::time::sleep(Duration::from_millis(200)).await;

                let client = GrpcClient::new(&self.grpc_config).await?;
                *guard = Some(client);
                guard
                    .as_mut()
                    .expect("grpc client must exist after reconnect")
                    .sign(key_id, message)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    /// Get the verifying public key for the specified key_id
    ///
    /// Returns (verifying_key_bytes [u8;32], expires_at, tolerance_seconds)
    pub async fn get_verifying_key(
        &self,
        key_id: u32,
    ) -> Result<([u8; 32], u64, u64), signer::SignerError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let client = GrpcClient::new(&self.grpc_config).await?;
            *guard = Some(client);
        }
        guard
            .as_mut()
            .expect("grpc client must exist after lazy init")
            .get_verifying_key(key_id)
            .await
    }

    /// Health check
    pub async fn health_check(&self) -> Result<String, signer::SignerError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let client = GrpcClient::new(&self.grpc_config).await?;
            *guard = Some(client);
        }
        guard
            .as_mut()
            .expect("grpc client must exist after lazy init")
            .health_check()
            .await
    }
}

fn is_retryable_transport_error(error: &signer::SignerError) -> bool {
    matches!(error, signer::SignerError::Internal(message) if message.contains("transport error"))
}

/// Create a Signer client wrapper from configuration
pub async fn create_signer_client(
    config: &platform::config::signer::SignerClientConfig,
    actrix_shared_key: &str,
) -> Result<SignerClientWrapper, AidError> {
    let grpc_config = signer::GrpcClientConfig {
        endpoint: config.endpoint.clone(),
        actrix_shared_key: actrix_shared_key.to_string(),
        timeout_seconds: config.timeout_seconds,
        enable_tls: config.enable_tls,
        tls_domain: config.tls_domain.clone(),
        ca_cert: config.ca_cert.clone(),
        client_cert: config.client_cert.clone(),
        client_key: config.client_key.clone(),
    };

    Ok(SignerClientWrapper::new(grpc_config))
}
