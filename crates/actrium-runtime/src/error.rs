use actrium_core::session::{AttachErrorCode, SessionStatus};
use thiserror::Error;

/// A stable protocol error plus a diagnostic that is safe to show to a host.
#[derive(Debug, Clone, Error)]
#[error("{code:?}: {message}")]
pub struct SessionError {
    pub code: AttachErrorCode,
    pub message: String,
    /// Safe occupancy metadata. Secret, nonce, and proof material are never included.
    pub occupied: Option<SessionStatus>,
}

impl SessionError {
    pub fn new(code: AttachErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            occupied: None,
        }
    }

    pub fn occupied(status: SessionStatus) -> Self {
        Self {
            code: AttachErrorCode::SessionOccupied,
            message: "the Service already has an active exclusive Front".into(),
            occupied: Some(status),
        }
    }

    pub fn expired(message: impl Into<String>) -> Self {
        Self::new(AttachErrorCode::SessionExpired, message)
    }

    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(AttachErrorCode::PermissionDenied, message)
    }

    pub fn epoch() -> Self {
        Self::new(
            AttachErrorCode::SessionEpochMismatch,
            "the session or lease epoch is stale",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid ServiceConfig: {0}")]
pub struct ConfigError(pub String);
