//! Request deduplication with TTL-based response caching.
//!
//! ## Design
//!
//! When a caller retries a request (e.g. after a transient send failure), the receiver
//! may process the same `request_id` twice, leading to double side-effects.
//!
//! `DedupState` prevents this by caching the response for each `request_id` for a
//! fixed TTL (default 30 s).  A second call with the same `request_id` within the TTL
//! returns the cached response without re-invoking the handler.
//! If the original request is still in-flight, duplicate callers wait for the
//! original result instead of receiving a synthetic duplicate error.
//!
//! ## Eviction
//!
//! Completed entries past `DEDUP_TTL` are evicted lazily on every
//! `check_or_mark` call. In-flight entries are NOT evicted lazily — a RETURN
//! handler may legitimately run longer than `INFLIGHT_TTL`, and evicting its
//! `InFlight` marker would let a retry re-run the handler. EXPLICIT in-flight
//! entries (a `Reply` that is never `send`-ed) are swept by the background
//! `sweep_inflight_explicit`, scoped to the `outstanding_replies` registry so
//! RETURN entries stay retained until `complete`.

use actr_framework::Bytes;
use actr_protocol::{ActorResult, ActrError};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// Default TTL for *completed* dedup entries. Must be comfortably longer than
/// the longest RpcReliable retry window. RpcReliable: 5 attempts,
/// 1s + 2s + 4s + 5s gaps = 12s of retry backoff, plus connection/recovery
/// timing. Keep enough margin so a late retry still sees the completed cache
/// instead of re-running the handler.
pub(crate) const DEDUP_TTL: Duration = Duration::from_secs(30);

/// Compute the caller reply deadline (RFC-0323): arrival now plus
/// `timeout_ms`. Returns `None` for `timeout_ms == 0` (a TELL or an unbounded
/// call — no deadline to carry). Kept in this exempt module so the
/// `std::time::Instant::now()` read stays off the audited timer-facade clock
/// in non-exempt callers (`node.rs`): this deadline is a caller-side wall-clock
/// bound surfaced via `Reply::deadline()`, not a `tokio::time`-awaited
/// deadline, so the wall clock is the correct clock to read it on.
pub(crate) fn caller_reply_deadline(timeout_ms: i64) -> Option<Instant> {
    (timeout_ms > 0).then(|| Instant::now() + Duration::from_millis(timeout_ms as u64))
}

/// Default TTL for *in-flight* dedup entries (RFC-0323 §5/§7). Independent of
/// `DEDUP_TTL`: an InFlight marker that never completes (e.g. an EXPLICIT
/// `Reply` parked in a waiter map forever) would otherwise pin the entry
/// permanently, hanging every retry for that `request_id`. After this TTL the
/// sweep completes the waiter with `ActrError::TimedOut` and frees the slot.
pub(crate) const INFLIGHT_TTL: Duration = Duration::from_secs(60);

pub(crate) type DedupWaiter = watch::Receiver<Option<ActorResult<Bytes>>>;

/// Cached outcome of a completed request.
#[derive(Clone, Debug)]
enum CachedResult {
    /// Request processed and response stored.
    Done(ActorResult<Bytes>),
    /// Request is currently in-flight (guards against concurrent duplicates).
    InFlight {
        completion_tx: watch::Sender<Option<ActorResult<Bytes>>>,
    },
}

/// Entry stored per request_id.
#[derive(Debug)]
struct Entry {
    received_at: Instant,
    result: CachedResult,
}

/// Framework-side request deduplication state.
///
/// Not `Clone` intentionally; share via `Arc<Mutex<DedupState>>`.
#[derive(Debug, Default)]
pub(crate) struct DedupState {
    entries: HashMap<String, Entry>,
    ttl: Duration,
    inflight_ttl: Duration,
}

/// Outcome of `check_or_mark`.
#[derive(Debug)]
pub(crate) enum DedupOutcome {
    /// First time we see this request_id; proceed with handling.
    Fresh,
    /// Request is already being processed; wait for the original result.
    InFlight(DedupWaiter),
    /// Request was already processed; the cached response is returned.
    Duplicate(ActorResult<Bytes>),
}

impl DedupState {
    /// Create a dedup state with the default TTLs (30 s completed, 60 s in-flight).
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: DEDUP_TTL,
            inflight_ttl: INFLIGHT_TTL,
        }
    }

    /// Check whether `request_id` is a duplicate.
    ///
    /// - If fresh: inserts an `InFlight` marker and returns `DedupOutcome::Fresh`.
    /// - If already in-flight: returns a waiter for the original result.
    /// - If already completed and still in TTL: returns the cached result.
    /// - Expired completed entries are evicted on every call. In-flight
    ///   entries are NOT evicted here (a RETURN handler may legitimately run
    ///   longer than `inflight_ttl`); EXPLICIT in-flight entries are swept
    ///   only by the background [`sweep_inflight_explicit`], scoped to the
    ///   `outstanding_replies` registry.
    pub(crate) fn check_or_mark(&mut self, request_id: &str) -> DedupOutcome {
        let now = Instant::now();
        self.evict_done_expired(now);

        match self.entries.get(request_id) {
            None => {
                let (completion_tx, _completion_rx) = watch::channel(None);
                self.entries.insert(
                    request_id.to_string(),
                    Entry {
                        received_at: now,
                        result: CachedResult::InFlight { completion_tx },
                    },
                );
                DedupOutcome::Fresh
            }
            Some(entry) => match &entry.result {
                CachedResult::InFlight { completion_tx } => {
                    DedupOutcome::InFlight(completion_tx.subscribe())
                }
                CachedResult::Done(r) => DedupOutcome::Duplicate(r.clone()),
            },
        }
    }

    /// Record the completed response for `request_id`.
    ///
    /// Call this after the handler finishes (success or error).
    pub(crate) fn complete(&mut self, request_id: &str, result: ActorResult<Bytes>) {
        if let Some(entry) = self.entries.get_mut(request_id) {
            if let CachedResult::InFlight { completion_tx } = &entry.result {
                let _ = completion_tx.send(Some(result.clone()));
            }
            entry.result = CachedResult::Done(result);
        }
    }

    /// Evict completed entries past `ttl` (RFC-0323 §5). In-flight entries
    /// are NEVER evicted here — a RETURN handler may legitimately run longer
    /// than `inflight_ttl`, and evicting its `InFlight` marker would let a
    /// retry re-run the handler (cache flip / double side-effects). EXPLICIT
    /// in-flight entries are swept separately by [`sweep_inflight_explicit`],
    /// scoped to the `outstanding_replies` registry so RETURN entries stay
    /// retained until `complete`.
    pub(crate) fn evict_done_expired(&mut self, now: Instant) {
        let ttl = self.ttl;
        self.entries.retain(|_, e| match &e.result {
            CachedResult::InFlight { .. } => true,
            CachedResult::Done(_) => now.duration_since(e.received_at) < ttl,
        });
    }

    /// Sweep EXPLICIT in-flight entries past `inflight_ttl` (RFC-0323 §5/§7).
    ///
    /// Only `request_id`s listed in `explicit_ids` — the exact set the
    /// `outstanding_replies` registry tracks — are considered; RETURN
    /// in-flight entries are retained until `complete`. Each swept entry is
    /// completed with `ActrError::TimedOut` so its parked duplicate waiter
    /// observes a consistent timeout instead of hanging forever (an EXPLICIT
    /// `Reply` that is never `send`-ed would otherwise pin its `InFlight`
    /// marker permanently). `Done` entries are left to `evict_done_expired`.
    pub(crate) fn sweep_inflight_explicit(&mut self, now: Instant, explicit_ids: &[String]) {
        let inflight_ttl = self.inflight_ttl;
        self.entries.retain(|id, e| match &mut e.result {
            CachedResult::InFlight { completion_tx } => {
                let is_explicit = explicit_ids.iter().any(|s| s == id);
                if is_explicit && now.duration_since(e.received_at) >= inflight_ttl {
                    let _ = completion_tx.send(Some(Err(ActrError::TimedOut)));
                    false
                } else {
                    true
                }
            }
            CachedResult::Done(_) => true,
        });
    }

    /// Clock-reading form of [`evict_done_expired`] for the background sweep
    /// task (keeps the `std::time::Instant::now()` read in this exempt module
    /// rather than the caller, so non-exempt callers stay off the wall clock).
    pub(crate) fn evict_done_expired_now(&mut self) {
        self.evict_done_expired(Instant::now());
    }

    /// Clock-reading form of [`sweep_inflight_explicit`] for the background
    /// sweep task.
    pub(crate) fn sweep_inflight_explicit_now(&mut self, explicit_ids: &[String]) {
        self.sweep_inflight_explicit(Instant::now(), explicit_ids);
    }

    /// Number of entries currently tracked (for monitoring / tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
#[path = "dedup_tests.rs"]
mod tests;
