//! HTTP service module
//!
//! Manages HTTP-related services

pub mod admin_api;
mod ais;
mod control;
mod mfr;
pub mod observability;
mod signaling;

pub use ais::AisService;
pub use control::build_control_router;
pub use mfr::MfrService;
pub use signaling::SignalingService;
