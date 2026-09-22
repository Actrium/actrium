//! Actor Identity Token management module
//!
//! Provides Actor Identity Token verification (issuing has been moved to the issuer crate)

pub mod error;
pub mod validator;
pub mod verifier;

pub use actr_protocol::AIdCredential;
pub use error::AidError;
pub use validator::AIdCredentialValidator;
pub use verifier::AIdCredentialVerifier;
