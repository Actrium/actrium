//! Actor Identity management module
//!
//! Provides Actor Identity Token verification based on aid proto (issuing has been moved to the ais crate)

pub mod credential;
pub mod identity_claims;
pub mod key_cache;

pub use credential::{AIdCredential, AIdCredentialValidator, AidError};
pub use identity_claims::IdentityClaims;
pub use key_cache::KeyCache;
