//! Unified error handling model
//!
//! Provides the top-level error type for the main actrix application, aggregating errors from all sub-modules and dependency crates.

use thiserror::Error;

/// Unified error enum for the main application
///
/// Aggregates error types from all sub-modules and dependency crates, providing type-safe error handling.
#[derive(Debug, Error)]
pub enum Error {
    // ========== Configuration errors ==========
    /// Configuration file related error
    #[error("Configuration error: {0}")]
    Config(#[from] Box<dyn std::error::Error>),

    // ========== Base library errors ==========
    /// Base crate aggregated error
    #[error("Base library error: {0}")]
    Base(Box<platform::error::BaseError>),

    // ========== Service errors ==========
    // Identity error type removed because AIS service has been removed.
    // TODO: The new signaling crate does not export SignalingError, temporarily disabled.
    // /// Signaling service error
    // #[error("Signaling service error: {0}")]
    // Signaling(#[from] signaling::SignalingError),
    /// STUN service error
    #[error("STUN service error: {0}")]
    Stun(#[from] stun::StunError),

    /// TURN service error
    #[error("TURN service error: {0}")]
    Turn(#[from] turn::TurnError),

    // ========== System-level errors ==========
    /// I/O operation error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Network error
    #[error("Network error: {0}")]
    Network(#[from] tokio::task::JoinError),

    // ========== Business logic errors ==========
    /// Service configuration validation failed
    #[error("Service configuration validation failed: {message}")]
    ServiceValidation { message: String },

    // ========== Generic errors ==========
    /// Anyhow error compatibility layer
    #[error("Legacy error: {0}")]
    Anyhow(#[from] anyhow::Error),

    /// Custom error message
    #[error("Application error: {message}")]
    Custom { message: String },
}

impl From<platform::error::BaseError> for Error {
    fn from(err: platform::error::BaseError) -> Self {
        Error::Base(Box::new(err))
    }
}

/// Unified Result type
///
/// Provides a unified error-handling result type for the main application.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create a custom error
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom {
            message: message.into(),
        }
    }

    /// Create a service configuration validation failure error
    pub fn service_validation(message: impl Into<String>) -> Self {
        Self::ServiceValidation {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::custom("test error");
        assert!(matches!(err, Error::Custom { .. }));
    }
}
