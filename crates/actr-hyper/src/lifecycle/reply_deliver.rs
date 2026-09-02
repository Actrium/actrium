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
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use actr_framework::{Deliver, ReplyOutcome};
use actr_protocol::{ActrError, ActrId};
use bytes::Bytes;
use tokio::sync::Mutex;

use crate::lifecycle::dedup::DedupState;
use crate::lifecycle::outstanding_replies::{OutstandingReplies, ReplyAttemptId};
use crate::transport::HostTransport;

const DELIVER_PENDING: u8 = 0;
const DELIVERED: u8 = 1;
const DELIVER_TIMED_OUT: u8 = 2;

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
    v020_route: crate::lifecycle::node::V020ResponseRoute,
    dedup_state: Arc<Mutex<DedupState>>,
    outstanding_replies: Weak<OutstandingReplies>,
    attempt_id: ReplyAttemptId,
    request_id: String,
    route_key: String,
    traceparent: Option<String>,
    tracestate: Option<String>,
    emit_response: bool,
    /// Runtime captured at admission so a movable Reply can be completed
    /// from a non-Tokio callback thread without losing its outcome.
    runtime_handle: Option<tokio::runtime::Handle>,
}

/// The `Deliver` impl for EXPLICIT dispatch. Cloned into a spawned task on
/// `deliver` so the sync `Deliver` call (fired inside the `Completer` mutex)
/// never blocks.
#[derive(Clone)]
pub(crate) struct ReplyDeliver {
    shared: ReplyShared,
    sink: ReplySinkClone,
    /// First-write-wins gate for `deliver` (RFC-0323 §5). The pending state is
    /// atomically claimed by either a normal delivery or the TTL timeout; the
    /// losing caller degrades to a logged no-op. This coordinates the three
    /// normal callers — the pump's EXPLICIT-Err path, a spawned `Reply::send`,
    /// and the `Reply::Drop` fallback — with the TTL sweep.
    state: Arc<AtomicU8>,
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
        v020_route: crate::lifecycle::node::V020ResponseRoute,
        dedup_state: Arc<Mutex<DedupState>>,
        outstanding_replies: Arc<OutstandingReplies>,
        attempt_id: ReplyAttemptId,
        request_id: String,
        route_key: String,
        traceparent: Option<String>,
        tracestate: Option<String>,
        sink: ReplySink,
        emit_response: bool,
    ) -> (Self, Arc<AtomicBool>) {
        let state = Arc::new(AtomicU8::new(DELIVER_PENDING));
        let delivered = Arc::new(AtomicBool::new(false));
        let runtime_handle = tokio::runtime::Handle::try_current().ok();
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
                    v020_route,
                    dedup_state,
                    outstanding_replies: Arc::downgrade(&outstanding_replies),
                    attempt_id,
                    request_id,
                    route_key,
                    traceparent,
                    tracestate,
                    emit_response,
                    runtime_handle,
                },
                sink: sink_clone,
                state,
                delivered: delivered.clone(),
            },
            delivered,
        )
    }

    /// Complete this reply with the TTL timeout through the same
    /// first-write-wins state used by `Reply::send` and `Reply::Drop`.
    ///
    /// The timeout path deliberately leaves dedup completion to the node's
    /// scoped InFlight sweep. That sweep must remove the old request entry so a
    /// same-ID retry can start; skipping a second `dedup.complete(request_id)`
    /// here prevents this old task from completing the retried entry.
    pub(crate) fn try_deliver_timeout(&self) -> bool {
        if self.try_deliver(
            ReplyOutcome {
                request_id: self.shared.request_id.clone(),
                result: Err(ActrError::TimedOut),
                emit_response: self.shared.emit_response,
            },
            false,
        ) {
            return true;
        }

        // A serial Component sweep may have won the timeout race through the
        // Deliver trait before the node-wide outstanding-reply sweep reaches
        // this entry. Treat that already-owned timeout as a win here too, so
        // the paired dedup sweep still evicts the InFlight marker.
        self.state.load(Ordering::Acquire) == DELIVER_TIMED_OUT
    }

    /// Whether no delivery task owns this reply. A TTL sweep uses this to
    /// reclaim a reply whose synchronous delivery was rolled back because no
    /// Tokio runtime was available.
    pub(crate) fn is_pending(&self) -> bool {
        self.state.load(Ordering::Acquire) == DELIVER_PENDING
    }

    fn try_deliver(&self, outcome: ReplyOutcome, complete_dedup: bool) -> bool {
        let winning_state = if complete_dedup {
            DELIVERED
        } else {
            DELIVER_TIMED_OUT
        };
        if self
            .state
            .compare_exchange(
                DELIVER_PENDING,
                winning_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            tracing::debug!(
                request_id = %self.shared.request_id,
                "explicit reply: late deliver degraded to no-op (RFC §5 first-write-wins)"
            );
            return false;
        }
        self.delivered.store(true, Ordering::Release);

        // Spawn off the current task so the sync `Deliver` call (fired inside
        // the `Completer` mutex or the pump continuation) never blocks. Prefer
        // the current runtime, but fall back to the runtime captured at
        // admission so a movable Reply can be completed off-runtime.
        let handle = tokio::runtime::Handle::try_current()
            .ok()
            .or_else(|| self.shared.runtime_handle.clone());
        match handle {
            Some(handle) => {
                let me = self.clone();
                handle.spawn(async move {
                    me.deliver_async(outcome, complete_dedup).await;
                });
                true
            }
            None => {
                self.state
                    .compare_exchange(
                        winning_state,
                        DELIVER_PENDING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .ok();
                self.delivered.store(false, Ordering::Release);
                tracing::warn!(
                    request_id = %self.shared.request_id,
                    "Deliver::deliver has no Tokio runtime; reply remains pending \
                     for the next TTL sweep"
                );
                false
            }
        }
    }

    async fn deliver_async(self, outcome: ReplyOutcome, complete_dedup: bool) {
        let ReplyOutcome {
            request_id: _,
            result,
            emit_response,
        } = outcome;

        if complete_dedup {
            // Dedup completion: for a TELL (emit_response=false) + Ok, cache
            // empty bytes (the response payload was never sent); otherwise
            // cache the real result so a retry observes the recorded outcome.
            // Always use the request ID captured when this deliver was
            // admitted, not a mutable/retried request's ID.
            let cached = match (&result, emit_response) {
                (Ok(_), false) => Ok(Bytes::new()),
                _ => result.clone(),
            };
            self.shared
                .dedup_state
                .lock()
                .await
                .complete(&self.shared.request_id, cached);
        }

        // Release the outstanding-reply registry slot (RFC-0323 §5/§7 gauge).
        // Idempotent: if a sweep already evicted this attempt, this is a no-op.
        // The attempt ID is essential when the same request ID is retried.
        if let Some(outstanding_replies) = self.shared.outstanding_replies.upgrade() {
            outstanding_replies
                .complete_attempt(self.shared.attempt_id)
                .await;
        }

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
                self.shared.v020_route.clone(),
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
        self.try_deliver(outcome, true);
    }

    fn deliver_timeout(&self, outcome: ReplyOutcome) {
        // Keep the dedup entry InFlight until the node's paired outstanding
        // reply sweep evicts it. This lets the sweep wake duplicate waiters
        // and permits a same-ID retry instead of caching TimedOut as Done.
        self.try_deliver(outcome, false);
    }
}
