//! Realm error type definitions
//!
//! Defines error types related to realm management.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RealmError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Realm not found")]
    NotFound,

    #[error("Realm already exists")]
    AlreadyExists,

    #[error("Key expired")]
    KeyExpired,

    #[error("Key does not exist")]
    KeyNotExist,

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<sqlx::Error> for RealmError {
    fn from(err: sqlx::Error) -> Self {
        RealmError::DatabaseError(err.to_string())
    }
}
