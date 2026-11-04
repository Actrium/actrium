//! Unified SDK facade for Loran.
//!
//! This crate organizes exports into explicit layers:
//! - `control`: stable control-plane API facade.
//! - `testing`: internal integration-test oriented facade (feature-gated).

pub mod control;
#[cfg(feature = "testing")]
pub mod testing;

// Keep a simple default surface for runtime consumers.
pub use control::*;
