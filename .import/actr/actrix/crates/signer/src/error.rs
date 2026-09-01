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
#[non_exhaustive]
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

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Base64 encoding/decoding error
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),

    /// JSON serialization/deserialization error
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
                // Do not leak specific key_id in production
                (StatusCode::NOT_FOUND, "Resource not found".to_string())
            }
            SignerError::Database(_) | SignerError::Internal(_) | SignerError::Crypto(_) => {
                // Do not expose internal error details to the client
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

/// Signer result type alias
pub type SignerResult<T> = Result<T, SignerError>;

// ECIES error handling - temporarily simplified due to error type changes in the ecies library
// More detailed error conversion can be added later as needed
