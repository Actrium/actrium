//! Top-level error enum
//!
//! Aggregates all submodule error types, providing a unified error handling interface

use super::{
    ConfigError, DatabaseError, NetworkError, SerializationError, StorageError, ValidationError,
};
use thiserror::Error;

/// Top-level error enum, aggregating all sub-crate error types
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum BaseError {
    // ========== Identity management errors ==========
    /// AId Token-related errors
    #[error("AId error: {0}")]
    Aid(#[from] crate::aid::credential::AidError),

    /// Realm management errors
    #[error("Realm error: {0}")]
    Realm(#[from] crate::realm::RealmError),

    // ========== Service layer errors ==========
    /// Identity service error
    #[error("Identity service error: {message}")]
    IdentityService { message: String },

    /// Signaling service error
    #[error("Signaling service error: {message}")]
    SignalingService { message: String },

    /// Control service error
    #[error("Admin service error: {message}")]
    ControlService { message: String },

    /// TURN service error
    #[error("TURN service error: {message}")]
    TurnService { message: String },

    /// STUN service error
    #[error("STUN service error: {message}")]
    StunService { message: String },

    // ========== Infrastructure errors ==========
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Network error
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    // ========== General errors ==========
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] SerializationError),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// General error (for errors that don't fit other categories)
    #[error("General error: {message}")]
    General { message: String },

    /// Internal error (usually indicates a programming error)
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl BaseError {
    /// Create a general error
    pub fn general(message: impl Into<String>) -> Self {
        Self::General {
            message: message.into(),
        }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Create an identity service error
    pub fn identity_service(message: impl Into<String>) -> Self {
        Self::IdentityService {
            message: message.into(),
        }
    }

    /// Create a signaling service error
    pub fn signaling_service(message: impl Into<String>) -> Self {
        Self::SignalingService {
            message: message.into(),
        }
    }

    /// Create a control service error
    pub fn control_service(message: impl Into<String>) -> Self {
        Self::ControlService {
            message: message.into(),
        }
    }

    /// Create a TURN service error
    pub fn turn_service(message: impl Into<String>) -> Self {
        Self::TurnService {
            message: message.into(),
        }
    }

    /// Create a STUN service error
    pub fn stun_service(message: impl Into<String>) -> Self {
        Self::StunService {
            message: message.into(),
        }
    }
}

/// Unified Result type
pub type Result<T> = std::result::Result<T, BaseError>;
