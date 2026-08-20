//! Signaling routing for the v0.20 RTC architecture (RFC §"SignalingRouter").
//!
//! `router` is the production routing component (transport-agnostic);
//! `in_memory` is a test-only in-memory signaling bus;
//! `ws_transport` adapts the production 0.14 WebSocket signaling client.

use actr_protocol::ActrId;

pub(crate) mod router;
pub(crate) mod ws_transport;

/// Shared, rebind-updatable local identity for the v0.20 signaling stack
/// (0.14 parity: `WebRtcCoordinator::set_local_id` on hard rebind).
///
/// A hard rebind adopts a freshly allocated `ActrId`; every outbound envelope
/// must stamp the *current* identity as `source`, so the transport reads this
/// cell per send instead of caching the construction-time id.
#[derive(Debug, Clone)]
pub(crate) struct SharedLocalId(std::sync::Arc<std::sync::RwLock<ActrId>>);

impl SharedLocalId {
    pub(crate) fn new(id: ActrId) -> Self {
        Self(std::sync::Arc::new(std::sync::RwLock::new(id)))
    }

    /// The current local identity (cloned; the read lock is held only for the
    /// copy, never across an `.await`).
    pub(crate) fn get(&self) -> ActrId {
        self.0.read().expect("local id lock poisoned").clone()
    }

    /// Adopt a new local identity after a hard rebind commits.
    pub(crate) fn set(&self, id: ActrId) {
        *self.0.write().expect("local id lock poisoned") = id;
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod in_memory;

#[cfg(test)]
#[path = "router_tests.rs"]
mod router_tests;
