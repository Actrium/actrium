//! Serialization error types
//!
//! Defines all errors related to data serialization, deserialization, and encoding

use thiserror::Error;

/// Serialization-related errors
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Protobuf error: {message}")]
    Protobuf { message: String },

    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Invalid format: {format}")]
    InvalidFormat { format: String },
}
