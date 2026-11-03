//! Signer HTTP handlers

use crate::{
    crypto::KeyEncryptor,
    error::SignerError,
    storage::KeyStorage,
    types::{SignRequest, SignResponse},
};
use axum::{
    Router,
    extract::{Json, Path, State},
    routing::{get, post},
};
use lazy_static::lazy_static;
use nonce_auth::{CredentialVerifier, NonceError, storage::NonceStorage};
use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, Opts};
use std::sync::Arc;
use std::time::Instant;

lazy_static! {
    /// Signer service metrics
    static ref SIGNER_REQUEST_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("loran_request_duration_seconds", "HTTP request duration in seconds")
            .namespace("loran")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        &["service", "method", "path", "status"]
    ).unwrap();

    static ref SIGNER_REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_requests_total", "Total number of HTTP requests")
            .namespace("loran"),
        &["service", "method", "path", "status"]
    ).unwrap();

    static ref SIGNER_AUTH_FAILURES: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_auth_failures_total", "Total number of authentication failures")
            .namespace("loran"),
        &["service", "reason"]
    ).unwrap();
}

/// Register Signer metrics to the global registry
pub fn register_signer_metrics(registry: &prometheus::Registry) -> Result<(), prometheus::Error> {
    registry.register(Box::new(SIGNER_REQUEST_DURATION.clone()))?;
    registry.register(Box::new(SIGNER_REQUESTS_TOTAL.clone()))?;
    registry.register(Box::new(SIGNER_AUTH_FAILURES.clone()))?;
    Ok(())
}

/// Signer service state
#[derive(Clone)]
pub struct SignerState {
    pub storage: KeyStorage,
    pub nonce_storage: Arc<dyn NonceStorage + Send + Sync>,
    pub psk: String,
}

impl SignerState {
    pub fn new<N: NonceStorage + Send + Sync + 'static>(
        storage: KeyStorage,
        nonce_storage: N,
        psk: String,
    ) -> Self {
        Self {
            storage,
            nonce_storage: Arc::new(nonce_storage),
            psk,
        }
    }

    pub async fn verify_credential(
        &self,
        credential: &nonce_auth::NonceCredential,
        request_payload: &str,
    ) -> Result<(), SignerError> {
        let verify_result = CredentialVerifier::new(self.nonce_storage.clone())
            .with_secret(self.psk.as_bytes())
            .verify(credential, request_payload.as_bytes())
            .await;

        verify_result.map_err(|e| match e {
            NonceError::DuplicateNonce => {
                SignerError::ReplayAttack("Nonce already used".to_string())
            }
            NonceError::TimestampOutOfWindow => {
                SignerError::Authentication("Request timestamp out of range".to_string())
            }
            NonceError::InvalidSignature => {
                SignerError::Authentication("Invalid signature".to_string())
            }
            _ => SignerError::Internal(format!("Authentication error: {e}")),
        })?;

        Ok(())
    }
}

/// Create SignerState from Signer config
///
/// Note: Nonce storage must be provided by the caller to avoid circular dependencies
/// between signer and platform crates.
pub async fn create_signer_state<N: NonceStorage + Send + Sync + 'static>(
    service_config: &crate::config::SignerServiceConfig,
    nonce_storage: N,
    loran_shared_key: &str,
    sqlite_path: &std::path::Path,
) -> Result<SignerState, SignerError> {
    crate::recording::info!("Initializing Signer state from SignerServiceConfig");

    // Create the key encryptor
    let encryptor = match service_config.get_kek_source() {
        Some(kek_source) => {
            crate::recording::info!("KEK configured, enabling private key encryption");
            KeyEncryptor::from_kek_source(&kek_source)?
        }
        None => {
            crate::recording::info!("No KEK configured, private keys will be stored in plaintext");
            KeyEncryptor::no_encryption()
        }
    };

    // Create the storage instance from config (async)
    let key_storage =
        KeyStorage::from_config(&service_config.storage, encryptor, sqlite_path).await?;

    Ok(SignerState::new(
        key_storage,
        nonce_storage,
        loran_shared_key.to_string(),
    ))
}

/// Create the router for the Signer service
pub fn create_router(state: SignerState) -> Router {
    Router::new()
        .route("/sign/{key_id}", post(sign_handler))
        .route("/health", get(health_check_handler))
        .with_state(state)
}

/// Get service statistics
pub async fn get_stats(state: &SignerState) -> Result<ServiceStats, SignerError> {
    let key_count = state.storage.get_key_count().await?;
    Ok(ServiceStats { key_count })
}

/// Service statistics
#[derive(Debug, Clone)]
pub struct ServiceStats {
    pub key_count: u32,
}

async fn sign_handler(
    State(app_state): State<SignerState>,
    Path(key_id): Path<u32>,
    Json(request): Json<SignRequest>,
) -> Result<Json<SignResponse>, SignerError> {
    let start_time = Instant::now();
    crate::recording::info!("Received sign request for key_id: {}", key_id);

    if request.key_id != key_id {
        return Err(SignerError::InvalidRequest(
            "path key_id does not match request key_id".to_string(),
        ));
    }

    // Verify the credential
    let request_data = request.request_payload();
    let verify_result = app_state
        .verify_credential(&request.credential, &request_data)
        .await;

    if let Err(ref e) = verify_result {
        let reason = match e {
            SignerError::ReplayAttack(_) => "replay_attack",
            SignerError::Authentication(_) => "invalid_signature",
            _ => "unknown",
        };
        SIGNER_AUTH_FAILURES
            .with_label_values(&["signer", reason])
            .inc();

        let duration = start_time.elapsed().as_secs_f64();
        SIGNER_REQUEST_DURATION
            .with_label_values(&["signer", "POST", "/sign", "401"])
            .observe(duration);
        SIGNER_REQUESTS_TOTAL
            .with_label_values(&["signer", "POST", "/sign", "401"])
            .inc();

        return verify_result.map(|_| unreachable!());
    }
    verify_result?;

    // Sign inside the backend; the private key never leaves the Signer
    let signature = app_state
        .storage
        .sign(key_id, &request.message)
        .await
        .map_err(|e| match e {
            SignerError::NotFound(_) => SignerError::KeyNotFound(key_id),
            other => other,
        })?;

    let response = SignResponse { signature };

    let duration = start_time.elapsed().as_secs_f64();
    SIGNER_REQUEST_DURATION
        .with_label_values(&["signer", "POST", "/sign", "200"])
        .observe(duration);
    SIGNER_REQUESTS_TOTAL
        .with_label_values(&["signer", "POST", "/sign", "200"])
        .inc();

    crate::recording::info!("Signed message for key_id: {}", key_id);
    Ok(Json(response))
}

async fn health_check_handler(
    State(app_state): State<SignerState>,
) -> Result<Json<serde_json::Value>, SignerError> {
    crate::recording::debug!("Health check requested");

    let key_count = app_state.storage.get_key_count().await?;

    let response = serde_json::json!({
        "status": "healthy",
        "service": "signer",
        "backend": app_state.storage.backend_name(),
        "key_count": key_count,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nonce_auth::{CredentialBuilder, NonceCredential, storage::MemoryStorage};
    use serde_json;
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn test_config() -> crate::config::SignerServiceConfig {
        crate::config::SignerServiceConfig {
            storage: crate::storage::StorageConfig {
                backend: crate::storage::StorageBackend::Sqlite,
                sqlite: Some(crate::storage::SqliteConfig::default()),
                postgres: None,
            },
            ..Default::default()
        }
    }

    async fn create_test_app() -> (Router, SignerState, String, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let psk = "test-psk".to_string();
        let app_state =
            create_signer_state(&test_config(), MemoryStorage::new(), &psk, temp_dir.path())
                .await
                .unwrap();
        (create_router(app_state.clone()), app_state, psk, temp_dir)
    }

    fn create_credential_for_request(psk: &str, request_data: &str) -> NonceCredential {
        CredentialBuilder::new(psk.as_bytes())
            .sign(request_data.as_bytes())
            .unwrap()
    }

    #[tokio::test]
    async fn test_service_creation_from_config() {
        let temp_dir = tempdir().unwrap();
        let state = create_signer_state(
            &test_config(),
            MemoryStorage::new(),
            "test-shared-key-123",
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(state.psk, "test-shared-key-123");
    }

    #[tokio::test]
    async fn test_service_stats() {
        let (_app, state, _psk, _temp_dir) = create_test_app().await;
        let stats = get_stats(&state).await.unwrap();
        assert_eq!(stats.key_count, 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let (app, _state, _psk, _temp_dir) = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn post_sign(app: &Router, credential: NonceCredential, key_id: u32) -> StatusCode {
        let request = SignRequest {
            key_id,
            message: b"probe".to_vec(),
            credential,
        };
        let request_body = serde_json::to_value(request).unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sign/{key_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        response.status()
    }

    fn sign_credential(psk: &str, key_id: u32) -> NonceCredential {
        create_credential_for_request(psk, &crate::types::sign_request_payload(key_id, b"probe"))
    }

    #[tokio::test]
    async fn test_sign_with_valid_key_succeeds() {
        let (app, state, psk, _temp_dir) = create_test_app().await;
        let key = state.storage.ensure_named_key("test").await.unwrap();

        let status = post_sign(&app, sign_credential(&psk, key.key_id), key.key_id).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_invalid_signature() {
        let (app, state, psk, _temp_dir) = create_test_app().await;
        let key = state.storage.ensure_named_key("test").await.unwrap();

        let credential = create_credential_for_request(&psk, "invalid-data");
        let status = post_sign(&app, credential, key.key_id).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_sign_refuses_a_retired_key() {
        let (app, state, psk, _temp_dir) = create_test_app().await;
        let key = state.storage.ensure_named_key("test").await.unwrap();
        state.storage.retire_named_key("test").await.unwrap();

        let status = post_sign(&app, sign_credential(&psk, key.key_id), key.key_id).await;
        assert_eq!(status, StatusCode::GONE);
    }
}
