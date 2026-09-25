//! Signer client wrapper
//!
//! Provides a unified Signer client interface, supporting gRPC clients (requires &mut self)

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

    /// Run one Signer RPC against the cached client, optionally rebuilding the
    /// client once after a retryable transport failure.
    ///
    /// A cached gRPC channel can go stale when the Signer restarts or the connection
    /// breaks; without a rebuild every later call keeps failing on the dead
    /// channel. Every wrapper method — not just `sign` — goes through this
    /// path so a broken channel can never permanently wedge key rotation.
    ///
    /// Non-idempotent operations must disable replay. In particular, the Signer may
    /// persist a generated key before its response is lost; replaying that
    /// request would create an orphaned key.
    ///
    /// If the rebuild itself fails the slot stays `None`, so the next call
    /// retries lazy initialization instead of caching the failure forever.
    async fn call_with_rebuild<T>(
        &self,
        op_name: &str,
        retry_on_transport_error: bool,
        mut op: impl for<'c> FnMut(
            &'c mut GrpcClient,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, signer::SignerError>> + Send + 'c>,
        >,
    ) -> Result<T, signer::SignerError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            *guard = Some(GrpcClient::new(&self.grpc_config).await?);
        }

        let result = op(guard
            .as_mut()
            .expect("grpc client must exist after lazy init"))
        .await;

        match result {
            Ok(value) => Ok(value),
            Err(error) if is_retryable_transport_error(&error) => {
                if retry_on_transport_error {
                    platform::recording::warn!(
                        "Signer gRPC transport failed during {}; rebuilding the client and retrying once",
                        op_name
                    );
                } else {
                    platform::recording::warn!(
                        "Signer gRPC transport failed during {}; clearing the cached client without replaying the request",
                        op_name
                    );
                }
                *guard = None;
                if !retry_on_transport_error {
                    return Err(error);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;

                *guard = Some(GrpcClient::new(&self.grpc_config).await?);
                op(guard
                    .as_mut()
                    .expect("grpc client must exist after reconnect"))
                .await
            }
            Err(error) => Err(error),
        }
    }

    /// Sign a message with the Ed25519 key in the Signer; returns a 64-byte signature.
    /// The private key never leaves the Signer service.
    pub async fn sign(&self, key_id: u32, message: &[u8]) -> Result<Vec<u8>, signer::SignerError> {
        // Each attempt owns its copy of the message so the retry closure
        // stays FnMut and the boxed future borrows nothing from the closure.
        self.call_with_rebuild("sign", true, move |client| {
            let payload = message.to_vec();
            Box::pin(async move { client.sign(key_id, &payload).await })
        })
        .await
    }

    /// Return the key named `name`, generating it if missing;
    /// returns (key_id, verifying_key_bytes[32]). Idempotent, so it retries.
    pub async fn ensure_named_key(
        &self,
        name: &str,
    ) -> Result<(u32, [u8; 32]), signer::SignerError> {
        self.call_with_rebuild("ensure_named_key", true, move |client| {
            let name = name.to_string();
            Box::pin(async move { client.ensure_named_key(&name).await })
        })
        .await
    }

    /// Retire the key named `name` so it never signs again; returns its
    /// key id. Idempotent, so it retries.
    pub async fn retire_named_key(&self, name: &str) -> Result<u32, signer::SignerError> {
        self.call_with_rebuild("retire_named_key", true, move |client| {
            let name = name.to_string();
            Box::pin(async move { client.retire_named_key(&name).await })
        })
        .await
    }

    /// Health check
    pub async fn health_check(&self) -> Result<String, signer::SignerError> {
        self.call_with_rebuild("health_check", true, |client| {
            Box::pin(client.health_check())
        })
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
