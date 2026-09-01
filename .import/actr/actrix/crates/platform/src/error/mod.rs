//! Error handling module
//!
//! Error type definitions separated by concept, following the one-file-one-core-concept principle

// Submodule declarations
mod base_error;
mod config_error;
mod database_error;
mod network_error;
mod serialization_error;
mod storage_error;
mod validation_error;

// Public API exports
pub use base_error::{BaseError, Result};
pub use config_error::ConfigError;
pub use database_error::DatabaseError;
pub use network_error::NetworkError;
pub use serialization_error::SerializationError;
pub use storage_error::StorageError;
pub use validation_error::ValidationError;
