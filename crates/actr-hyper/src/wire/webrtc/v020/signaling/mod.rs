//! Signaling routing for the v0.20 RTC architecture (RFC §"SignalingRouter").
//!
//! `router` is the production routing component (transport-agnostic);
//! `in_memory` is a test-only in-memory signaling bus;
//! `ws_transport` adapts the production 0.14 WebSocket signaling client.

use actr_protocol::ActrId;

use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

pub(crate) mod router;
pub(crate) mod ws_transport;

/// Exact sender-side RTC identity attached to a recovery/negotiation-critical
/// relay. Actrix echoes it without interpretation so a delayed negative
/// delivery result cannot affect a newer Controller or transport attempt.
/// `negotiation_epoch` discriminates generation-0 role probes: the signaling
/// gate-opening edge re-drives a probe while one is still in flight, and each
/// emission bumps the epoch, so only the exact probe observes its point-in-time
/// delivery disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RelayAttemptIdentity {
    pub(crate) controller_lifecycle_id: u64,
    pub(crate) transport_epoch: u64,
    pub(crate) pc_generation: u64,
    pub(crate) negotiation_epoch: u64,
}

/// One Controller-originated signaling dispatch.
#[derive(Debug, Clone)]
pub(crate) struct SignalingDispatch {
    pub(crate) peer: ActrId,
    pub(crate) message: SignalingMessage,
    pub(crate) attempt: Option<RelayAttemptIdentity>,
    /// Library-only context for preserving the originating business span
    /// across the router and transport queues. This never crosses the wire.
    pub(crate) span: tracing::Span,
}

impl SignalingDispatch {
    pub(crate) fn tracked(
        peer: ActrId,
        controller_lifecycle_id: u64,
        transport_epoch: u64,
        pc_generation: u64,
        negotiation_epoch: u64,
        message: SignalingMessage,
    ) -> Self {
        Self::tracked_with_span(
            peer,
            controller_lifecycle_id,
            transport_epoch,
            pc_generation,
            negotiation_epoch,
            message,
            tracing::Span::current(),
        )
    }

    pub(crate) fn tracked_with_span(
        peer: ActrId,
        controller_lifecycle_id: u64,
        transport_epoch: u64,
        pc_generation: u64,
        negotiation_epoch: u64,
        message: SignalingMessage,
        span: tracing::Span,
    ) -> Self {
        Self {
            peer,
            message,
            attempt: Some(RelayAttemptIdentity {
                controller_lifecycle_id,
                transport_epoch,
                pc_generation,
                negotiation_epoch,
            }),
            span,
        }
    }

    /// Protocol responses emitted outside a live Controller do not request a
    /// delivery result and therefore carry no attempt identity.
    pub(crate) fn untracked(peer: ActrId, message: SignalingMessage) -> Self {
        Self::untracked_with_span(peer, message, tracing::Span::current())
    }

    pub(crate) fn untracked_with_span(
        peer: ActrId,
        message: SignalingMessage,
        span: tracing::Span,
    ) -> Self {
        Self {
            peer,
            message,
            attempt: None,
            span,
        }
    }

    pub(crate) fn requests_delivery_result(&self) -> bool {
        self.attempt.is_some()
            && matches!(
                self.message,
                SignalingMessage::Offer { .. }
                    | SignalingMessage::Answer { .. }
                    | SignalingMessage::IceRestartOffer { .. }
                    | SignalingMessage::IceRestartRequest { .. }
                    | SignalingMessage::RoleNegotiation
            )
    }
}

/// Items delivered by a signaling transport to the router.
#[derive(Debug)]
pub(crate) enum SignalingInbound {
    PeerMessage {
        from: ActrId,
        message: SignalingMessage,
        /// Receive-side span recovered from the signaling envelope queue.
        span: tracing::Span,
    },
    TargetUnavailable {
        target: ActrId,
        attempt: RelayAttemptIdentity,
    },
}

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
