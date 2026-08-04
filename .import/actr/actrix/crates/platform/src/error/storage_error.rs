//! Storage error types
//!
//! Defines all errors related to filesystem and storage backends

use thiserror::Error;

/// Storage-related errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Disk full: {path}")]
    DiskFull { path: String },

    #[error("Corruption detected: {path}")]
    Corruption { path: String },

    #[error("Storage backend error: {backend}")]
    Backend { backend: String },
}
