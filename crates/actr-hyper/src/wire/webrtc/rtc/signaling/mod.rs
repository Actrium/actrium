//! Signaling routing for the RTC architecture (RFC §"SignalingRouter").
//!
//! `router` is the production routing component (transport-agnostic);
//! `in_memory` is a test-only in-memory signaling bus;
//! `ws_transport` adapts the production WebSocket signaling client.

use actr_protocol::ActrId;
#[cfg(feature = "opentelemetry")]
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::wire::webrtc::rtc::peer::reducer::SignalingMessage;

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
    /// Independent dispatch span carried across the router and transport queues.
    /// Its OTel parent preserves causality without retaining the business span.
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
        let span = Self::dispatch_span(&peer, &message, span);
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
        let span = Self::dispatch_span(&peer, &message, span);
        Self {
            peer,
            message,
            attempt: None,
            span,
        }
    }

    fn dispatch_span(
        peer: &ActrId,
        message: &SignalingMessage,
        parent: tracing::Span,
    ) -> tracing::Span {
        // A tracing parent would keep the business span open while signaling
        // waits in the queue or transport. Retain only its OTel identity.
        let span = tracing::info_span!(
            parent: None,
            "signaling.dispatch",
            peer = %peer.to_string_repr(),
            message_kind = message.kind_name(),
        );
        #[cfg(feature = "opentelemetry")]
        let _ = span.set_parent(parent.context());
        drop(parent);
        span
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

/// Shared, rebind-updatable local identity for the RTC signaling stack
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

impl From<ActrId> for SharedLocalId {
    fn from(id: ActrId) -> Self {
        Self::new(id)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod in_memory;

#[cfg(test)]
#[path = "router_tests.rs"]
mod router_tests;

#[cfg(test)]
mod tracing_lifetime_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[cfg(feature = "opentelemetry")]
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::{Layer, Registry};

    #[derive(Clone)]
    struct ClosedSpans(Arc<Mutex<Vec<&'static str>>>);

    impl<S> Layer<S> for ClosedSpans
    where
        S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        fn on_close(&self, id: tracing::span::Id, context: Context<'_, S>) {
            self.0
                .lock()
                .unwrap()
                .push(context.span(&id).unwrap().name());
        }
    }

    fn check_dispatch_lifetime(tracked: bool) {
        let closed = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Registry::default().with(ClosedSpans(closed.clone()));
        #[cfg(feature = "opentelemetry")]
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        #[cfg(feature = "opentelemetry")]
        let subscriber = {
            use opentelemetry::trace::TracerProvider;
            subscriber.with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")))
        };
        let closed_names = || closed.lock().unwrap().clone();
        tracing::subscriber::with_default(subscriber, || {
            let peer = ActrId {
                realm: "test@example.com".to_string(),
                serial_number: 1,
            };
            let attempt = tracing::info_span!("webrtc.recover");
            let dispatch = attempt.in_scope(|| {
                if tracked {
                    SignalingDispatch::tracked(peer, 1, 2, 3, 4, SignalingMessage::RoleNegotiation)
                } else {
                    SignalingDispatch::untracked(peer, SignalingMessage::RoleNegotiation)
                }
            });
            #[cfg(feature = "opentelemetry")]
            {
                use opentelemetry::trace::TraceContextExt;
                let parent = attempt.context();
                let child = dispatch.span.context();
                assert!(parent.span().span_context().is_valid());
                assert_eq!(
                    parent.span().span_context().trace_id(),
                    child.span().span_context().trace_id()
                );
                assert_ne!(
                    parent.span().span_context().span_id(),
                    child.span().span_context().span_id()
                );
            }
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            tx.try_send(dispatch).unwrap();
            // An attempt must close even while its signaling is still queued.
            drop(attempt);
            assert_eq!(closed_names(), vec!["webrtc.recover"]);
            let dispatch = rx.try_recv().unwrap();
            // Model the transport's child span outliving the dispatch handle.
            let transport = dispatch
                .span
                .in_scope(|| tracing::info_span!("signaling.transport_send"));
            drop(dispatch);
            assert_eq!(closed_names(), vec!["webrtc.recover"]);
            drop(transport);
            assert_eq!(
                closed_names(),
                vec![
                    "webrtc.recover",
                    "signaling.transport_send",
                    "signaling.dispatch"
                ]
            );
        });
    }

    #[test]
    fn tracked_dispatch_does_not_keep_attempt_open() {
        check_dispatch_lifetime(true);
    }

    #[test]
    fn untracked_dispatch_does_not_keep_attempt_open() {
        check_dispatch_lifetime(false);
    }
}
