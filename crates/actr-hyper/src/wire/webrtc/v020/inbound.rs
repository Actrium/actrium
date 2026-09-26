//! `InboundRouter` — the inbound data-plane router for the v0.20 architecture
//! (RFC §"Control plane and data plane": "DataChannel.poll → DataChannelPump →
//! InboundRouter → workload").
//!
//! The pump delivers each `OnMessage` payload straight here, by lane. The
//! payload **bypasses the Controller entirely** — it never enters the peer
//! command mailbox (data plane, invariant #11). The router is a thin, pluggable
//! seam: production wires it to the workload's inbound path; tests install a
//! recorder.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use actr_protocol::ActrId;

use crate::wire::webrtc::v020::status::LaneKind;

/// One inbound data message, tagged with the peer, PC generation, and lane it
/// arrived on. The payload is frozen bytes (no copy past the channel's buffer).
#[derive(Debug, Clone)]
pub(crate) struct InboundMessage {
    pub(crate) peer: ActrId,
    pub(crate) generation: u64,
    pub(crate) lane: LaneKind,
    pub(crate) payload: bytes::Bytes,
}

/// The business inbound sink. Called on a lane pump task (not the Controller),
/// so it must be cheap and non-blocking; production routes into a bounded
/// workload mailbox.
pub(crate) type InboundHandler = std::sync::Arc<dyn Fn(InboundMessage, InboundLease) + Send + Sync>;

#[derive(Debug, Default)]
struct InboundBudgetState {
    messages: usize,
    bytes: usize,
}

struct InboundBudget {
    state: std::sync::Mutex<InboundBudgetState>,
    max_messages: usize,
    max_bytes: usize,
    max_message_bytes: usize,
    dropped_messages: AtomicU64,
}

/// Capacity lease for one inbound message. The business bridge retains this
/// value until asynchronous dispatch finishes, so the bound covers queued and
/// in-flight work rather than only the synchronous callback invocation.
pub(crate) struct InboundLease {
    budget: Arc<InboundBudget>,
    bytes: usize,
}

impl Drop for InboundLease {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().expect("RTC inbound budget lock");
        state.messages = state.messages.saturating_sub(1);
        state.bytes = state.bytes.saturating_sub(self.bytes);
    }
}

/// Routes inbound data messages to the business sink, by lane.
pub(crate) struct InboundRouter {
    handler: InboundHandler,
    budget: Arc<InboundBudget>,
}

impl InboundRouter {
    pub(crate) fn new(handler: InboundHandler) -> Self {
        Self::new_with_config(handler, super::config::RtcBufferConfig::default())
    }

    pub(crate) fn new_with_config(
        handler: InboundHandler,
        config: super::config::RtcBufferConfig,
    ) -> Self {
        config
            .validate()
            .expect("InboundRouter requires validated finite RTC resource limits");
        Self {
            handler,
            budget: Arc::new(InboundBudget {
                state: std::sync::Mutex::new(InboundBudgetState::default()),
                max_messages: config.max_inbound_dispatch_messages,
                max_bytes: config.max_inbound_dispatch_bytes,
                max_message_bytes: config.max_inbound_message_bytes,
                dropped_messages: AtomicU64::new(0),
            }),
        }
    }

    /// A router that drops messages (used when no business sink is wired yet,
    /// e.g. control-only tests).
    pub(crate) fn discard() -> Self {
        Self::new(std::sync::Arc::new(|_msg, _lease| {}))
    }

    /// Try to deliver one inbound message to the business sink. This method is
    /// non-blocking: oversize or over-budget work is dropped and counted.
    pub(crate) fn route(&self, message: InboundMessage) -> bool {
        let bytes = message.payload.len();
        let lease = {
            let mut state = self.budget.state.lock().expect("RTC inbound budget lock");
            let within_message = bytes <= self.budget.max_message_bytes;
            let within_count = state.messages < self.budget.max_messages;
            let within_bytes = state
                .bytes
                .checked_add(bytes)
                .is_some_and(|next| next <= self.budget.max_bytes);
            if !within_message || !within_count || !within_bytes {
                self.budget.dropped_messages.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            state.messages += 1;
            state.bytes += bytes;
            InboundLease {
                budget: self.budget.clone(),
                bytes,
            }
        };
        (self.handler)(message, lease);
        true
    }

    pub(crate) fn dropped_messages(&self) -> u64 {
        self.budget.dropped_messages.load(Ordering::Relaxed)
    }

    /// Record a data-plane drop that happens before dispatch admission (for
    /// example malformed or over-budget fragment reassembly).
    pub(crate) fn record_drop(&self) {
        self.budget.dropped_messages.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn inflight(&self) -> (usize, usize) {
        let state = self.budget.state.lock().expect("RTC inbound budget lock");
        (state.messages, state.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(bytes: usize) -> InboundMessage {
        InboundMessage {
            peer: ActrId {
                realm: "test@example.com".to_string(),
                serial_number: 1,
            },
            generation: 1,
            lane: LaneKind::RpcReliable,
            payload: bytes::Bytes::from(vec![0; bytes]),
        }
    }

    #[test]
    fn lease_bounds_queued_and_inflight_messages_by_count_and_bytes() {
        let held = Arc::new(std::sync::Mutex::new(Vec::new()));
        let held_for_handler = held.clone();
        let config = super::super::config::RtcBufferConfig {
            max_inbound_message_bytes: 70_000,
            max_reassembly_bytes: 70_000,
            max_inbound_dispatch_messages: 2,
            max_inbound_dispatch_bytes: 100_000,
            ..super::super::config::RtcBufferConfig::default()
        };
        let router = InboundRouter::new_with_config(
            Arc::new(move |_message, lease| {
                held_for_handler.lock().unwrap().push(lease);
            }),
            config,
        );

        assert!(router.route(message(40_000)));
        assert!(router.route(message(40_000)));
        assert!(!router.route(message(1)), "count bound must reject");
        assert_eq!(router.inflight(), (2, 80_000));
        held.lock().unwrap().pop();
        assert!(router.route(message(20_000)));
        assert!(!router.route(message(20_001)), "byte bound must reject");
        assert_eq!(router.dropped_messages(), 2);
    }

    #[test]
    fn oversize_single_message_is_rejected_without_calling_handler() {
        let calls = Arc::new(AtomicU64::new(0));
        let calls_for_handler = calls.clone();
        let config = super::super::config::RtcBufferConfig {
            max_inbound_message_bytes: 70_000,
            max_reassembly_bytes: 70_000,
            ..super::super::config::RtcBufferConfig::default()
        };
        let router = InboundRouter::new_with_config(
            Arc::new(move |_message, _lease| {
                calls_for_handler.fetch_add(1, Ordering::Relaxed);
            }),
            config,
        );
        assert!(!router.route(message(70_001)));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
}
