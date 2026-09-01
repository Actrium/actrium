//! Movable reply handle for `ReplyMode::Explicit` methods (RFC-0323).
//!
//! `Reply<T>` decouples the callee's dispatch completion from reply delivery.
//! An EXPLICIT handler receives a `Reply<T>` alongside the request and may:
//!
//! 1. call `reply.send(result)` synchronously before returning, or
//! 2. move the handle into a `tokio::spawn` task and return immediately,
//!    releasing its dispatch slot / conflict key before the reply is produced.
//!
//! ## Type-level contract
//!
//! - `send(self)` consumes the handle → a second `send` on the same instance is
//!   impossible at compile time.
//! - `Reply<T>: Send + 'static` → movable across tasks and storable in a waiter
//!   map.
//! - `Drop` fallback: dropping an un-`send` handle sends
//!   `ActrError::Internal("reply dropped without response")` so the caller never
//!   white-waits to a timeout. `send` is synchronous enqueue precisely because
//!   `Drop` cannot await.
//!
//! ## Idempotent completer (first-write-wins)
//!
//! The §2 spawn pattern creates a double-reply race: the handle has been moved
//! into a spawned task (not yet `send`) *and* the handler returns `Err`, so the
//! framework's error path also tries to deliver. Generated dispatchers retain a
//! hidden [`ReplyDispatchGuard`] while awaiting the handler. This prevents the
//! handler argument's `Drop` from replacing a real handler error with the generic
//! drop fallback; the guard submits that error first. The shared `Once`-style
//! completion state then makes all later writers logged no-ops.
//!
//! ## Pump contract (hyper side)
//!
//! The hyper message pump drives an EXPLICIT dispatch through three moments:
//!
//! | moment | what happens | dedup | mailbox ACK |
//! |---|---|---|---|
//! | admission | `admit_incoming` builds the `ReplyDeliver`, registers it on the ctx, and registers an `outstanding_replies` gauge slot | `InFlight` marker set | — |
//! | dispatch-completion (handler return) | the dispatcher guard submits a real handler `Err` before the generic Drop fallback; on `Ok` the continuation returns `Deferred` and **ACKs the durable mailbox now** | unchanged unless an error completed it | **ACK** (§4: ack-at-return) |
//! | reply (`reply.send` / `Drop`) | `Deliver::deliver` spawns a task that sends the RESPONSE and completes dedup, then releases the gauge slot | `Done` | — |
//!
//! **Ack-at-return trade (§4):** the durable ACK fires at dispatch-completion,
//! *before* the reply is produced — the opposite of `ReplyMode::Return`, where
//! the pump ACKs at reply time. Acking earlier lets the pump keep admitting
//! work while the reply is still pending; acking at reply time would pin the
//! durable message for the entire reply latency, contradicting the decoupling
//! goal (and §4 explicitly rejects "ack-at-reply"). The reply half (RESPONSE
//! envelope + dedup completion) still runs from the `Deliver`, so reply
//! delivery remains first-write-wins.
//!
//! **Budget C (§5):** an EXPLICIT dispatch that completes with `Deferred`
//! has released its dispatch slot / conflict key but *not* its
//! `outstanding_replies` slot — that slot is released by `deliver_async`. The
//! `max_pending_replies` cap pauses dequeue when outstanding replies pile up,
//! and the in-flight TTL sweep frees slots leaked by a `Reply` that is never
//! `send`-ed.
//!
//! **Deadlock rule (§5):** a `ReplyMode::Return` handler MUST NOT
//! synchronously call back into itself (it would block on its own dispatch
//! slot, which it still holds). Cross-actor call-backs and self-call-backs
//! that can recurse require `ReplyMode::Explicit` so the handler can move the
//! `Reply` aside and return the slot first.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use actr_protocol::{ActorResult, ActrError};
use bytes::Bytes;

/// Caller deadline for an EXPLICIT reply (RFC-0323): arrival time plus
/// `envelope.timeout_ms`. An alias for `std::time::Instant` so downstream
/// crates (hyper) can reference the deadline type without spelling
/// `std::time::Instant` in files audited by the timer-facade drift test.
pub type ReplyDeadline = Instant;

/// What the runtime does with a completed reply: send the RESPONSE envelope and
/// complete the in-flight dedup entry. The runtime supplies an implementation;
/// tests inject a capturing one.
///
/// The payload carries either the encoded response bytes (Ok) or the structured
/// error (Err), plus a flag distinguishing the TELL empty-variant (which
/// completes dedup with empty bytes but emits no RESPONSE).
pub trait Deliver: Send + Sync + 'static {
    fn deliver(&self, outcome: ReplyOutcome);

    /// Deliver a timeout without completing the dedup entry as a normal
    /// response. Implementations with a first-write-wins timeout path may
    /// override this; simple test sinks can use the normal delivery behavior.
    fn deliver_timeout(&self, outcome: ReplyOutcome) {
        self.deliver(outcome);
    }
}

/// What a `Reply` hands to the runtime when it completes.
#[derive(Debug, Clone)]
pub struct ReplyOutcome {
    pub request_id: String,
    /// `Ok(bytes)` → a real RESPONSE payload (encoded message).
    /// `Err(ActrError)` → an error RESPONSE.
    /// For the TELL empty-variant this is `Ok(empty)` and `emit_response` is
    /// `false`: dedup completes with empty bytes, no envelope goes out.
    pub result: ActorResult<Bytes>,
    /// Whether to emit a RESPONSE envelope on the wire. TELL into an EXPLICIT
    /// method sets this to `false` (fire-and-forget semantics chosen by the
    /// caller, the callee code is unaware).
    pub emit_response: bool,
}

/// Shared, idempotent completion state — first writer wins.
///
/// `Mutex<Option<ReplyOutcome>>` holds `Some` once the first `send`/`Drop`/framework
/// error path lands. `OnceLock` would suffice for the "exactly one wins" check but
/// we need to deliver the outcome to the [`Deliver`] impl exactly once, and a
/// caller that constructs the outcome before taking the lock must not be able to
/// observe a half-delivered state.
struct Completer {
    outcome: Mutex<Option<ReplyOutcome>>,
    deliver: Arc<dyn Deliver>,
    owners: AtomicUsize,
}

impl Completer {
    fn new(deliver: Arc<dyn Deliver>) -> Self {
        Self {
            outcome: Mutex::new(None),
            deliver,
            owners: AtomicUsize::new(1),
        }
    }

    fn add_owner(&self) {
        self.owners.fetch_add(1, Ordering::Relaxed);
    }

    /// Release one Reply/guard owner. Exactly one concurrent drop observes
    /// `true`, avoiding the racy last-owner check that `Arc::strong_count`
    /// would provide.
    fn release_owner(&self) -> bool {
        self.owners.fetch_sub(1, Ordering::AcqRel) == 1
    }

    /// Attempt to deliver `outcome`. Returns `true` if this call won (i.e. it was
    /// the first writer), `false` if a previous completion already landed.
    ///
    /// On a win, `Deliver::deliver` is invoked synchronously inside the lock
    /// guard's scope so that the outcome is observed as delivered before any
    /// re-entrant duplicate can run.
    fn try_complete(&self, outcome: ReplyOutcome) -> bool {
        let mut slot = self.outcome.lock().expect("completer mutex poisoned");
        if slot.is_some() {
            return false;
        }
        // First writer wins. Deliver before releasing the slot so a concurrent
        // second writer observes the win and degrades to a no-op.
        (self.deliver).deliver(outcome.clone());
        *slot = Some(outcome);
        true
    }

    fn is_completed(&self) -> bool {
        self.outcome
            .lock()
            .expect("completer mutex poisoned")
            .is_some()
    }
}

/// A movable, droppable reply handle for an EXPLICIT RPC method (RFC-0323).
///
/// Constructed by the runtime's dispatch path (or a test via [`Reply::new`]) and
/// handed to the handler. The handler either calls [`send`](Self::send) or moves
/// the handle into a spawned task. Dropping an un-`send` handle triggers the
/// `Drop` fallback.
///
/// `T` is the response message type; `Reply<T>: Send + 'static` so it can cross
/// task boundaries and live in a waiter map.
pub struct Reply<T> {
    request_id: String,
    deadline: Option<Instant>,
    completer: Arc<Completer>,
    /// Whether completing this handle should emit a RESPONSE envelope on the
    /// wire. `true` for normal EXPLICIT calls; `false` for the TELL
    /// empty-variant (complete dedup with empty bytes, send nothing).
    emit_response: bool,
    // `fn() -> T` keeps the handle `Send + Sync` regardless of `T` (the
    // handle never stores a `T`; it only tags the response type for the
    // handler). This replaces an earlier `unsafe impl Send/Sync` with the
    // same soundness — a function-pointer `PhantomData` is always `Send +
    // Sync`, so no `unsafe` is needed.
    _marker: PhantomData<fn() -> T>,
}

/// Generated-dispatch companion for a [`Reply<T>`].
///
/// The dispatcher keeps this alive while the handler owns the public Reply.
/// Only the final owner runs the Drop fallback, so a handler's returned error
/// can be delivered before the generic "dropped without response" outcome.
/// This type is public solely because generated code lives downstream.
#[doc(hidden)]
pub struct ReplyDispatchGuard {
    request_id: String,
    completer: Arc<Completer>,
    emit_response: bool,
}

impl<T: prost::Message> Reply<T> {
    /// Construct a `Reply` backed by a [`Deliver`] impl. The runtime builds one
    /// per inbound EXPLICIT request; tests pass a capturing `Deliver`.
    ///
    /// - `request_id`: correlates the eventual RESPONSE with the request.
    /// - `deadline`: arrival + `timeout_ms`; `None` for TELL. Sending after the
    ///   deadline is still valid — the caller discards the orphan RESPONSE.
    /// - `deliver`: where the winning outcome goes (transport + dedup complete).
    /// - `emit_response`: `true` for normal calls, `false` for the TELL
    ///   empty-variant.
    pub fn new(
        request_id: String,
        deadline: Option<Instant>,
        deliver: Arc<dyn Deliver>,
        emit_response: bool,
    ) -> Self {
        Self {
            request_id,
            deadline,
            completer: Arc::new(Completer::new(deliver)),
            emit_response,
            _marker: PhantomData,
        }
    }

    /// Enqueue the reply synchronously and consume the handle.
    ///
    /// First writer wins: sends the RESPONSE envelope and completes dedup. A
    /// concurrent `send` / framework error path / `Drop` that already landed
    /// makes this call a logged no-op. Calling `send` after `send` on the *same*
    /// instance is a compile error (`self` is consumed); the race only arises
    /// across distinct clones of the completer (spawned task vs. error path).
    pub fn send(self, result: ActorResult<T>) {
        let result_bytes: ActorResult<Bytes> = match result {
            Ok(msg) => Ok(msg.encode_to_vec().into()),
            Err(e) => Err(e),
        };
        self.completer.try_complete(ReplyOutcome {
            request_id: self.request_id.clone(),
            result: result_bytes,
            emit_response: self.emit_response,
        });
    }

    /// The caller's deadline: arrival time plus `timeout_ms`. `None` for TELL.
    ///
    /// This is an *upper bound* on the caller's real deadline (it excludes
    /// in-transit time); sending after it is still valid, the caller just
    /// discards the orphan RESPONSE.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Whether this handle has already been completed (by `send`, the framework
    /// error path, or a `Drop` twin). Primarily for diagnostics and tests.
    pub fn is_completed(&self) -> bool {
        self.completer.is_completed()
    }

    /// Retain the completion state while a generated dispatcher awaits the
    /// handler. Business code should not call this directly.
    #[doc(hidden)]
    pub fn dispatch_guard(&self) -> ReplyDispatchGuard {
        self.completer.add_owner();
        ReplyDispatchGuard {
            request_id: self.request_id.clone(),
            completer: Arc::clone(&self.completer),
            emit_response: self.emit_response,
        }
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        // `send` consumed `self` by value, so reaching `Drop` here means the
        // handle was dropped without `send` (e.g. a spawned task panicked before
        // sending, or a waiter-map entry was evicted). The caller must never
        // white-wait to a timeout because the callee forgot to reply.
        //
        // First-write-wins: if `send` or the framework error path already
        // completed, this is a no-op.
        if !self.completer.release_owner() || self.completer.is_completed() {
            return;
        }
        let outcome = if self.emit_response {
            ReplyOutcome {
                request_id: self.request_id.clone(),
                result: Err(ActrError::Internal(
                    "reply dropped without response".to_string(),
                )),
                emit_response: true,
            }
        } else {
            // TELL empty-variant: complete dedup with empty bytes, send nothing.
            ReplyOutcome {
                request_id: self.request_id.clone(),
                result: Ok(Bytes::new()),
                emit_response: false,
            }
        };
        let _ = self.completer.try_complete(outcome);
    }
}

impl ReplyDispatchGuard {
    /// Complete the reply with the handler's actual error. Consumes the guard
    /// so its Drop observes the completed state and remains a no-op.
    #[doc(hidden)]
    pub fn complete_error(self, error: ActrError) {
        self.completer.try_complete(ReplyOutcome {
            request_id: self.request_id.clone(),
            result: Err(error),
            emit_response: self.emit_response,
        });
    }
}

impl Drop for ReplyDispatchGuard {
    fn drop(&mut self) {
        if !self.completer.release_owner() || self.completer.is_completed() {
            return;
        }
        let outcome = if self.emit_response {
            ReplyOutcome {
                request_id: self.request_id.clone(),
                result: Err(ActrError::Internal(
                    "reply dropped without response".to_string(),
                )),
                emit_response: true,
            }
        } else {
            ReplyOutcome {
                request_id: self.request_id.clone(),
                result: Ok(Bytes::new()),
                emit_response: false,
            }
        };
        let _ = self.completer.try_complete(outcome);
    }
}

#[cfg(test)]
#[path = "reply_tests.rs"]
mod tests;
