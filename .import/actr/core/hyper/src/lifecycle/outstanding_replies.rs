//! Outstanding EXPLICIT reply tracking (RFC-0323 §5/§7).
//!
//! When an EXPLICIT dispatch completes (handler returns `Ok`), the reply is
//! deferred to a later `Reply::send` / `Drop` on the `ReplyDeliver`. Until then
//! the reply is "outstanding". This module counts those pending replies and
//! backs a registry used by the TTL sweep.
//!
//! Two purposes:
//! - **Gauge**: `len()` exposes the count for backpressure decisions
//!   (`max_pending_replies`) and the `on_mailbox_backpressure` hook.
//! - **Leak safety**: a `Reply` that is never `send`-ed (e.g. parked in a
//!   waiter map that is never drained) would otherwise pin the counter
//!   forever and, once `max_pending_replies` is reached, stall the pump. The
//!   background sweep calls the exact registered [`ReplyDeliver`] through its
//!   first-write-wins gate, then pairs the timed-out IDs with
//!   [`DedupState::sweep_inflight_explicit`] (which notifies parked duplicate
//!   waiters with `TimedOut` and evicts the `InFlight` marker). The two sweeps
//!   share the same `INFLIGHT_TTL` and run on the same background tick, so a
//!   leaked reply is freed on both sides together — and only EXPLICIT entries,
//!   leaving RETURN in-flight entries untouched.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::lifecycle::reply_deliver::ReplyDeliver;

pub(crate) type ReplyAttemptId = u64;

struct OutstandingEntry {
    request_id: String,
    admitted_at: Instant,
    deadline: Option<Instant>,
    /// The exact reply writer for this admission. Keeping it here lets the
    /// TTL sweep race the handler's Reply::send through the same
    /// first-write-wins gate instead of manufacturing a second completion
    /// path.
    deliver: Option<ReplyDeliver>,
}

pub(crate) struct OutstandingReplies {
    count: AtomicUsize,
    next_attempt_id: AtomicU64,
    entries: Mutex<HashMap<ReplyAttemptId, OutstandingEntry>>,
}

impl OutstandingReplies {
    pub(crate) fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            next_attempt_id: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate a process-local identity for one explicit reply admission.
    /// The identity is deliberately independent of `request_id`: callers may
    /// retry with the same request ID after the old attempt has timed out.
    pub(crate) fn allocate_attempt_id(&self) -> ReplyAttemptId {
        self.next_attempt_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register an outstanding EXPLICIT reply without a deliver. This legacy
    /// form is retained for direct registry users; the node uses
    /// [`register_with_deliver`] so the TTL sweep can complete the exact
    /// `ReplyDeliver` that belongs to the entry.
    #[cfg(test)]
    pub(crate) async fn register(&self, request_id: &str) {
        let attempt_id = self.allocate_attempt_id();
        self.entries.lock().await.insert(
            attempt_id,
            OutstandingEntry {
                request_id: request_id.to_string(),
                admitted_at: Instant::now(),
                deadline: None,
                deliver: None,
            },
        );
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    /// Register the exact `ReplyDeliver` for one explicit reply admission.
    pub(crate) async fn register_with_deliver(
        &self,
        request_id: &str,
        attempt_id: ReplyAttemptId,
        deliver: ReplyDeliver,
        deadline: Option<Instant>,
    ) {
        self.entries.lock().await.insert(
            attempt_id,
            OutstandingEntry {
                request_id: request_id.to_string(),
                admitted_at: Instant::now(),
                deadline,
                deliver: Some(deliver),
            },
        );
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    /// Mark the exact outstanding reply attempt complete. Idempotent: a
    /// second call (e.g. the deliver task racing the TTL sweep) is a no-op.
    pub(crate) async fn complete_attempt(&self, attempt_id: ReplyAttemptId) {
        if self.entries.lock().await.remove(&attempt_id).is_some() {
            self.count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Mark an outstanding reply complete by request ID for legacy registry
    /// users. Runtime delivery uses [`complete_attempt`] so an old attempt can
    /// never remove a newer retry with the same request ID.
    #[cfg(test)]
    pub(crate) async fn complete(&self, request_id: &str) {
        let removed = {
            let mut entries = self.entries.lock().await;
            let attempt_id = entries
                .iter()
                .find(|(_, entry)| entry.request_id == request_id)
                .map(|(attempt_id, _)| *attempt_id);
            attempt_id.and_then(|id| entries.remove(&id))
        };
        if removed.is_some() {
            self.count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Current outstanding count (the `outstanding_replies` gauge).
    pub(crate) fn len(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// Evict registry entries older than `ttl`, decrementing the counter for
    /// each. This test-only legacy form does not deliver a timeout; production
    /// uses [`sweep_expired`] so the registered `ReplyDeliver` owns the
    /// first-write-wins decision.
    #[cfg(test)]
    pub(crate) async fn sweep(&self, ttl: Duration, now: Instant) {
        let n = {
            let mut entries = self.entries.lock().await;
            let before = entries.len();
            entries.retain(|_, entry| now.duration_since(entry.admitted_at) < ttl);
            before - entries.len()
        };
        if n != 0 {
            self.count.fetch_sub(n, Ordering::SeqCst);
        }
    }

    /// Expire aged explicit replies through their own `ReplyDeliver`.
    ///
    /// The deliver's atomic first-write-wins gate is the arbitration point
    /// between a late `Reply::send` and the TTL timeout. Entries whose normal
    /// reply already won remain registered until that delivery task completes;
    /// only an entry whose deliver successfully wins the timeout race is
    /// removed and returned for the dedup-side InFlight eviction.
    pub(crate) async fn sweep_expired(&self, ttl: Duration, now: Instant) -> Vec<String> {
        let mut expired_ids = Vec::new();
        let mut remove = Vec::new();
        {
            let mut entries = self.entries.lock().await;
            for (attempt_id, entry) in entries.iter() {
                let expires_at = entry.deadline.unwrap_or(entry.admitted_at + ttl);
                if now < expires_at {
                    continue;
                }

                let timeout_won = entry
                    .deliver
                    .as_ref()
                    .map(|deliver| deliver.deliver_timeout())
                    .unwrap_or(true);
                if timeout_won || entry.deliver.as_ref().is_some_and(ReplyDeliver::is_pending) {
                    remove.push(*attempt_id);
                    expired_ids.push(entry.request_id.clone());
                }
            }
            for attempt_id in remove {
                entries.remove(&attempt_id);
            }
        }
        if !expired_ids.is_empty() {
            self.count.fetch_sub(expired_ids.len(), Ordering::SeqCst);
        }
        expired_ids
    }

    /// Clock-reading form of [`sweep_expired`] for the background sweep task.
    pub(crate) async fn sweep_expired_now(&self, ttl: Duration) -> Vec<String> {
        self.sweep_expired(ttl, Instant::now()).await
    }
}

#[cfg(test)]
#[path = "outstanding_replies_tests.rs"]
mod tests;
