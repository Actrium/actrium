//! Actor Identity management module
//!
//! Provides Actor Identity Token verification based on aid proto (issuing has been moved to the issuer crate)

pub mod credential;
pub mod key_cache;

pub use actr_protocol::IdentityClaims;
pub use credential::{AIdCredential, AIdCredentialValidator, AidError};
pub use key_cache::KeyCache;
