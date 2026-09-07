//! HTTP service module
//!
//! Manages HTTP-related services

pub mod admin_api;
mod ais;
mod authority;
mod control;
mod mfr;
pub mod observability;
mod realm_descriptor;
mod signaling;

pub use ais::AisService;
pub use authority::build_authority_router;
pub use control::build_control_router;
pub use mfr::PublisherRegistryService;
pub use realm_descriptor::build_realm_descriptor_router;
pub use signaling::SignalingService;
