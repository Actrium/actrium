//! HTTP service module
//!
//! Manages HTTP-related services

pub mod admin_api;
mod ais;
mod authority;
mod control;
mod mfr;
pub mod observability;
mod signaling;

pub use ais::AisService;
pub use authority::build_authority_router;
pub use control::build_control_router;
pub use mfr::MfrService;
pub use signaling::SignalingService;
