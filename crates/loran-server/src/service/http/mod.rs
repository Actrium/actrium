//! HTTP service module
//!
//! Manages HTTP-related services

pub mod admin_api;
mod authority;
mod control;
mod issuer;
pub mod observability;
mod publisher_registry;
mod signaling;

pub use authority::{build_authority_router, build_realm_profiles};
pub use control::build_control_router;
pub use issuer::IssuerService;
pub use publisher_registry::PublisherRegistryService;
pub use signaling::SignalingService;
