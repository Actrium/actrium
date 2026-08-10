//! Reply delivery for `ReplyMode::Explicit` methods (RFC-0323).
//!
//! When an inbound RPC is `ReplyMode::Explicit`, the pump constructs a
//! [`ReplyDeliver`] from the inbound envelope + transport handles and stashes
//! it on the [`crate::context::RuntimeContext`]. The generated EXPLICIT
//! dispatch arm reads it via [`actr_framework::Context::reply_deliver`] and
//! builds a `Reply<T>` handle from it.
//!
//! When the handler calls `reply.send(result)` (or drops the handle), the
//! [`Deliver::deliver`] method fires. It spawns a task that sends the RESPONSE
//! envelope and completes the dedup entry — the same reply half the inline
//! RETURN path runs synchronously, just deferred to whenever the handler
//! produces its reply. The durable mailbox ACK is NOT done here: per RFC-0323
//! §4 the pump ACKs at dispatch-completion (handler return), before the reply.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use actr_framework::{Deliver, ReplyOutcome};
use actr_protocol::ActrId;
use bytes::Bytes;
use tokio::sync::Mutex;

use crate::lifecycle::dedup::DedupState;
use crate::lifecycle::outstanding_replies::OutstandingReplies;
use crate::transport::HostTransport;

/// Path-specific reply routing state, captured by the receive loop and passed
/// to [`Inner::admit_incoming`](crate::lifecycle::Inner::admit_incoming) so the
/// `ReplyDeliver` can reach the right transport + ACK the right message.
pub(crate) enum ReplySink {
    /// Mailbox (State Path): reply via transport gates + ACK the durable message.
    Mailbox {
        caller: Result<ActrId, String>,
        msg_id: uuid::Uuid,
        span: tracing::Span,
    },
    /// In-proc (Shell → Guest): reply via `HostTransport`, no ACK.
    Inproc {
        response_tx: Arc<HostTransport>,
        span: tracing::Span,
    },
}

/// Cloned reply state shared between the RETURN and EXPLICIT paths. Built in
/// `admit_incoming` from `Inner` + the inbound envelope.
#[derive(Clone)]
struct ReplyShared {
    ws_gate: Option<Arc<crate::wire::websocket::WebSocketGate>>,
    webrtc_gate: Option<Arc<crate::wire::webrtc::gate::WebRtcGate>>,
    dedup_state: Arc<Mutex<DedupState>>,
    outstanding_replies: Arc<OutstandingReplies>,
    request_id: String,
    route_key: String,
    traceparent: Option<String>,
    tracestate: Option<String>,
}

/// The `Deliver` impl for EXPLICIT dispatch. Cloned into a spawned task on
/// `deliver` so the sync `Deliver` call (fired inside the `Completer` mutex)
/// never blocks.
#[derive(Clone)]
pub(crate) struct ReplyDeliver {
    shared: ReplyShared,
    sink: ReplySinkClone,
    /// First-write-wins gate for `deliver` (RFC-0323 §5). CAS'd from `false`
    /// to `true` inside [`Deliver::deliver`]; the losing caller degrades to a
    /// logged no-op. This coordinates the three callers that can reach
    /// `deliver` — the pump's EXPLICIT-Err path, a spawned `Reply::send`, and
    /// the `Reply::Drop` fallback — so only one emits a RESPONSE and completes
    /// the dedup entry, preventing both wire double-send and the dedup
    /// `Done(Err)`→`Done(Ok)` cache flip.
    delivered: Arc<AtomicBool>,
}

// `ReplySink` is not `Clone` (it carries a `tracing::Span` which is Clone, but
// the enum also has `Result<ActrId, String>` and `Arc<HostTransport>` which are
// Clone). We split the clonable fields out so `ReplyDeliver` can derive
// `Clone` without requiring `ReplySink` to be `Clone` in the trait sense.
#[derive(Clone)]
struct ReplySinkClone {
    caller: Option<Result<ActrId, String>>,
    msg_id: Option<uuid::Uuid>,
    response_tx: Option<Arc<HostTransport>>,
    span: tracing::Span,
}

impl ReplyDeliver {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ws_gate: Option<Arc<crate::wire::websocket::WebSocketGate>>,
        webrtc_gate: Option<Arc<crate::wire::webrtc::gate::WebRtcGate>>,
        dedup_state: Arc<Mutex<DedupState>>,
        outstanding_replies: Arc<OutstandingReplies>,
        request_id: String,
        route_key: String,
        traceparent: Option<String>,
        tracestate: Option<String>,
        sink: ReplySink,
    ) -> (Self, Arc<AtomicBool>) {
        let delivered = Arc::new(AtomicBool::new(false));
        let sink_clone = match sink {
            ReplySink::Mailbox {
                caller,
                msg_id,
                span,
            } => ReplySinkClone {
                caller: Some(caller),
                msg_id: Some(msg_id),
                response_tx: None,
                span,
            },
            ReplySink::Inproc { response_tx, span } => ReplySinkClone {
                caller: None,
                msg_id: None,
                response_tx: Some(response_tx),
                span,
            },
        };
        (
            Self {
                shared: ReplyShared {
                    ws_gate,
                    webrtc_gate,
                    dedup_state,
                    outstanding_replies,
                    request_id,
                    route_key,
                    traceparent,
                    tracestate,
                },
                sink: sink_clone,
                delivered: delivered.clone(),
            },
            delivered,
        )
    }

    async fn deliver_async(self, outcome: ReplyOutcome) {
        let ReplyOutcome {
            request_id,
            result,
            emit_response,
        } = outcome;

        // Dedup completion: for a TELL (emit_response=false) + Ok, cache empty
        // bytes (the response payload was never sent); otherwise cache the
        // real result so a retry observes the recorded outcome.
        let cached = match (&result, emit_response) {
            (Ok(_), false) => Ok(Bytes::new()),
            _ => result.clone(),
        };
        self.shared
            .dedup_state
            .lock()
            .await
            .complete(&request_id, cached);

        // Release the outstanding-reply registry slot (RFC-0323 §5/§7 gauge).
        // Idempotent: if a sweep already evicted it, this is a no-op.
        self.shared.outstanding_replies.complete(&request_id).await;

        // Send the RESPONSE envelope (if emit_response). The mailbox ACK is
        // NOT done here: per RFC-0323 §4 the EXPLICIT pump ACKs at
        // dispatch-completion (handler return), which already ran before this
        // spawned task fires. Only the reply half runs here.
        if let Some(_msg_id) = &self.sink.msg_id {
            // Mailbox path
            let caller = self
                .sink
                .caller
                .clone()
                .unwrap_or(Err("<unknown>".to_string()));
            crate::lifecycle::node::mailbox_send_reply(
                self.shared.ws_gate.clone(),
                self.shared.webrtc_gate.clone(),
                caller,
                self.shared.request_id.clone(),
                self.shared.route_key.clone(),
                self.shared.traceparent.clone(),
                self.shared.tracestate.clone(),
                emit_response,
                result,
                self.sink.span.clone(),
            )
            .await;
        } else if let Some(response_tx) = &self.sink.response_tx {
            // In-proc path
            crate::lifecycle::node::inproc_send_reply(
                response_tx.clone(),
                self.shared.route_key.clone(),
                self.shared.request_id.clone(),
                self.shared.traceparent.clone(),
                self.shared.tracestate.clone(),
                emit_response,
                result,
                self.sink.span.clone(),
            )
            .await;
        }
    }
}

impl Deliver for ReplyDeliver {
    fn deliver(&self, outcome: ReplyOutcome) {
        // RFC-0323 §5 first-write-wins: the first caller to reach `deliver`
        // (the pump's EXPLICIT-Err path, a spawned `Reply::send`, or the
        // `Reply::Drop` fallback) wins; every later caller degrades to a
        // logged no-op. This prevents the double-RESPONSE / dedup cache-flip
        // race where a late `reply.send(Ok)` would overwrite the pump's
        // `Done(Err)` and emit a second wire reply.
        if self
            .delivered
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            tracing::debug!(
                request_id = %self.shared.request_id,
                "explicit reply: late deliver degraded to no-op (RFC §5 first-write-wins)"
            );
            return;
        }
        // Spawn off the current task so the sync `Deliver` call (fired inside
        // the `Completer` mutex or the pump continuation) never blocks. Use
        // `Handle::try_current` instead of bare `tokio::spawn` so a `deliver`
        // fired off-runtime (e.g. a drop on a foreign thread) degrades to a
        // logged miss instead of panicking and poisoning the Completer mutex.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let me = self.clone();
                handle.spawn(async move {
                    me.deliver_async(outcome).await;
                });
            }
            Err(err) => {
                tracing::warn!(
                    request_id = %self.shared.request_id,
                    error = %err,
                    "Deliver::deliver off-runtime: reply dropped; \
                     InFlight TTL sweep will free the dedup slot + outstanding gauge"
                );
            }
        }
    }
}
