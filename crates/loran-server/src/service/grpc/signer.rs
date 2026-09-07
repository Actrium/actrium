//! Signer gRPC router construction.
//!
//! Builds the Signer gRPC service router served on its own dedicated listener
//! (SGN-02), separate from the shared primary HTTP/HTTPS listener.

use anyhow::Result;
use axum::Router;
use platform::{
    config::ActrixConfig, monitoring::ServiceCounters, storage::nonce::SqliteNonceStorage,
};
use signer::{KeyEncryptor, KeyStorage, create_grpc_service};
use std::sync::Arc;

/// Build the Signer gRPC router served on the dedicated Signer listener.
///
/// Primary route for tonic clients:
/// `/signer.v1.Signer/<Method>`
///
/// When `counters` is provided, an axum middleware layer records every
/// request into the shared `ServiceCounters`.
pub async fn build_signer_grpc_router(
    config: &ActrixConfig,
    counters: Option<Arc<ServiceCounters>>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Router> {
    let signer_service_config = config
        .services
        .signer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Signer service configuration not found"))?;

    // Create the nonce storage instance (used for replay protection).
    // `sqlite_path` is the directory; `nonce.db` is appended internally.
    let nonce_storage = SqliteNonceStorage::new_async(&config.sqlite_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create nonce storage: {e}"))?;

    // Create the key encryptor.
    let encryptor = match signer_service_config.get_kek_source() {
        Some(kek_source) => {
            platform::recording::info!("KEK configured, enabling private key encryption");
            KeyEncryptor::from_kek_source(&kek_source)
                .map_err(|e| anyhow::anyhow!("Failed to create key encryptor: {e}"))?
        }
        None => {
            platform::recording::info!(
                "No KEK configured, private keys will be stored in plaintext"
            );
            KeyEncryptor::no_encryption()
        }
    };

    // Create the Signer storage.
    let storage = KeyStorage::from_config(
        &signer_service_config.storage,
        encryptor,
        &config.sqlite_path,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create Signer storage: {e}"))?;

    // SGN-04/07: drive background pruning of expired signer keys and nonce
    // rows on the gRPC path (the HTTP path has its own lazy, per-request
    // trigger; the gRPC path had none, so both tables grew unbounded). Clone
    // the stores before they are moved into the gRPC service; both loops exit
    // when `cancel` fires on reload/shutdown.
    tokio::spawn(signer::run_key_cleanup(
        storage.clone(),
        signer_service_config.tolerance_seconds,
        cancel.clone().cancelled_owned(),
    ));
    tokio::spawn(platform::storage::nonce::run_nonce_cleanup(
        Arc::new(nonce_storage.clone()),
        cancel.cancelled_owned(),
    ));

    // Create the gRPC service (this moves the stores).
    let grpc_service = create_grpc_service(
        storage,
        nonce_storage,
        config.actrix_shared_key.clone(),
        signer_service_config.tolerance_seconds,
    );

    platform::recording::info!("Signer gRPC service router built for the dedicated listener");

    let mut router = Router::new().route_service("/signer.v1.Signer/{*grpc_method}", grpc_service);

    // Wrap with a metrics middleware when counters are provided
    if let Some(ctr) = counters {
        router = router.layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let ctr = ctr.clone();
                async move {
                    let start = std::time::Instant::now();
                    let response = next.run(req).await;
                    let success = response.status().is_success();
                    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                    ctr.record_request(success, latency_ms).await;
                    response
                }
            },
        ));
    }

    Ok(router)
}
