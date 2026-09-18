//! Native RTC implementation backed by webrtc-rs.
//!
//! `RtcConnectionManager` owns per-peer `RtcPeerController`s; each controller
//! drives a `WebRtcEffectRunner` and generation-owned sessions through the
//! builder, handler, runtime, poll, and back-pressure APIs.
//!
//! Per the RFC, there is **no callback adapter** here. The pieces map directly onto the target
//! component responsibilities:
//!
//! * [`factory`] — `PeerConnectionFactory`
//! * [`runtime`] — `RtcRuntimeConfig` (Runtime strategy)
//! * [`address`] — test/platform address provider seams and production wildcard resolver
//! * [`event_handler`] — `WebRtcEventHandler`
//! * [`events`] — `PeerDriverEvent` + `PeerEventMailbox`

pub(crate) mod address;
pub(crate) mod config;
pub(crate) mod data_lane;
pub(crate) mod event_handler;
pub(crate) mod events;
pub(crate) mod factory;
// Fault-injection seams (Stage 5 item 10): test / explicit debug builds only
// (RFC §"Security and privacy").
#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod fault;
pub(crate) mod inbound;
pub(crate) mod inbound_bridge;
pub(crate) mod manager;
pub(crate) mod media;
pub(crate) mod notification;
pub(crate) mod peer;
pub(crate) mod recovery;
pub(crate) mod runtime;
pub(crate) mod signaling;
pub(crate) mod stats;
pub(crate) mod status;
pub(crate) mod wire_handle;

#[cfg(test)]
#[path = "inbound_bridge_tests.rs"]
mod inbound_bridge_tests;

#[cfg(test)]
#[path = "validation_tests.rs"]
mod validation_tests;

#[cfg(test)]
#[path = "interop_tests.rs"]
mod interop_tests;

#[cfg(test)]
#[path = "wiring_tests.rs"]
mod wiring_tests;

#[cfg(test)]
#[path = "manager_tests.rs"]
mod manager_tests;

#[cfg(test)]
#[path = "notification_tests.rs"]
mod notification_tests;

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "fault_tests.rs"]
mod fault_tests;
