//! Validation error types
//!
//! Defines all errors related to data validation, permission checks, and rule verification

use thiserror::Error;

/// Validation-related errors
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid input: {field}")]
    InvalidInput { field: String },

    #[error("Value out of range: {field} = {value}")]
    OutOfRange { field: String, value: String },

    #[error("Required field missing: {field}")]
    Required { field: String },

    #[error("Invalid format: {field}")]
    InvalidFormat { field: String },

    #[error("Authentication failed: {reason}")]
    Authentication { reason: String },

    #[error("Authorization failed: {reason}")]
    Authorization { reason: String },

    #[error("Rate limit exceeded: {limit}")]
    RateLimit { limit: String },
}
