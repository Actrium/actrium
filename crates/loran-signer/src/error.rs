//! Signer service error definitions

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

/// Signer service error type
#[derive(Error, Debug)]
pub enum SignerError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Encryption/decryption error
    #[error("Crypto error: {0}")]
    Crypto(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Replay attack detected
    #[error("Replay attack detected: {0}")]
    ReplayAttack(String),

    /// Invalid request parameters
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Key not found
    #[error("Key not found: key_id={0}")]
    KeyNotFound(u32),

    /// The key exists but has passed its `expires_at`, so signing is refused
    ///
    /// Verifier caches drop verifying keys at
    /// `expires_at` without any tolerance, so a signature minted afterwards
    /// would produce a credential that fails every handshake. The tolerance
    /// window only keeps key material
    /// around for reads; it never extends the signing window.
    #[error("Signing key expired: key_id={0}")]
    KeyExpired(u32),

    /// The named key was retired: its private key is gone and it never signs
    #[error("Signing key retired: key_id={0}")]
    KeyRetired(u32),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Base64 encode/decode error
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),

    /// JSON serialize/deserialize error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP client error
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    /// Nonce auth errors
    #[error("Nonce auth error: {0}")]
    NonceAuth(#[from] nonce_auth::NonceError),
}

impl IntoResponse for SignerError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            SignerError::Authentication(_) => (
                StatusCode::UNAUTHORIZED,
                "Authentication failed".to_string(),
            ),
            SignerError::ReplayAttack(_) => (StatusCode::FORBIDDEN, "Request rejected".to_string()),
            SignerError::NonceAuth(_) => (
                StatusCode::UNAUTHORIZED,
                "Authentication failed".to_string(),
            ),
            SignerError::InvalidRequest(_) => (
                StatusCode::BAD_REQUEST,
                "Invalid request parameters".to_string(),
            ),
            SignerError::KeyNotFound(_) | SignerError::NotFound(_) => {
                // Do not leak the concrete key_id in production
                (StatusCode::NOT_FOUND, "Resource not found".to_string())
            }
            SignerError::KeyExpired(_) => (StatusCode::GONE, "Signing key expired".to_string()),
            SignerError::KeyRetired(_) => (StatusCode::GONE, "Signing key retired".to_string()),
            SignerError::Database(_) | SignerError::Internal(_) | SignerError::Crypto(_) => {
                // Never expose internal error details to the client
                crate::recording::error!("Internal error: {:?}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            _ => {
                crate::recording::error!("Unexpected error: {:?}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": error_message,
            "code": status.as_u16()
        }));

        (status, body).into_response()
    }
}

/// Maps a `SignerError` to a `tonic::Status` for the gRPC surface.
///
/// The client-facing message is always a fixed, opaque string; every concrete
/// detail (sqlx table names, file paths, nonce failure kind, key ids) is kept
/// server-side in the recording log only. In particular, all authentication
/// failures — invalid signature, replayed nonce, and timestamp-out-of-window —
/// collapse into a single `unauthenticated("authentication failed")` so a caller
/// cannot use the response to distinguish them and build a signing oracle.
impl From<SignerError> for tonic::Status {
    fn from(err: SignerError) -> Self {
        match &err {
            SignerError::Authentication(_)
            | SignerError::NonceAuth(_)
            | SignerError::ReplayAttack(_) => {
                crate::recording::warn!("gRPC authentication rejected: {err:?}");
                tonic::Status::unauthenticated("authentication failed")
            }
            SignerError::InvalidRequest(_) => {
                crate::recording::debug!("gRPC invalid request: {err:?}");
                tonic::Status::invalid_argument("invalid request")
            }
            SignerError::KeyNotFound(_) | SignerError::NotFound(_) => {
                crate::recording::debug!("gRPC resource not found: {err:?}");
                tonic::Status::not_found("not found")
            }
            SignerError::KeyRetired(_) => {
                crate::recording::warn!("gRPC sign refused: {err:?}");
                tonic::Status::failed_precondition("key retired")
            }
            _ => {
                crate::recording::error!("gRPC internal error: {err:?}");
                tonic::Status::internal("internal error")
            }
        }
    }
}

/// Signer result type alias
pub type SignerResult<T> = Result<T, SignerError>;

// ECIES error handling - simplified for now due to ecies crate error-type churn.
// More detailed error conversions can be added later as needed.
