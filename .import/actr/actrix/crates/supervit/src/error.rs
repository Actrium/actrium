//! Error types for supervit

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SupervitError>;

#[derive(Debug, Error)]
pub enum SupervitError {
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("gRPC status error: {0}")]
    Status(#[from] tonic::Status),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Metrics collection error: {0}")]
    Metrics(String),

    #[error("Invalid node ID: {0}")]
    InvalidNodeId(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("nonce-auth error: {0}")]
    NonceAuth(#[from] ::nonce_auth::NonceError),

    #[error("Internal error: {0}")]
    Internal(String),
}
