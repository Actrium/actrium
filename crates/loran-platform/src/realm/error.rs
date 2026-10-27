//! Realm error type definitions
//!
//! Defines error types related to realm management.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RealmError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Realm not found")]
    NotFound,

    #[error("Realm already exists; Realm names are never reused")]
    AlreadyExists,

    #[error("Realm is retired")]
    Retired,

    #[error("Realm quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Key expired")]
    KeyExpired,

    #[error("Key does not exist")]
    KeyNotExist,

    #[error("Parse error: {0}")]
    ParseError(String),

    /// A membership check inside a Realm write failed.
    #[error(transparent)]
    Membership(#[from] super::membership::MembershipError),
}

impl From<sqlx::Error> for RealmError {
    fn from(err: sqlx::Error) -> Self {
        RealmError::DatabaseError(err.to_string())
    }
}
