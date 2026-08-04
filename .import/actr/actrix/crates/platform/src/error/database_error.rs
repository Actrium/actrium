//! Database error types
//!
//! Defines all errors related to database connections, queries, and transactions

use thiserror::Error;

/// Database-related errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Query failed: {query}")]
    QueryFailed { query: String },

    #[error("Transaction failed: {message}")]
    TransactionFailed { message: String },

    #[error("Migration failed: {version}")]
    MigrationFailed { version: String },

    #[error("Database not found: {name}")]
    NotFound { name: String },

    #[error("Constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },

    #[error("SQLite error: {0}")]
    Sqlite(#[from] sqlx::Error),
}
