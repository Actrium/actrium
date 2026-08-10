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
//!   background sweep pairs [`leaked_ids`] / [`complete`] on this registry
//!   with [`DedupState::sweep_inflight_explicit`] on the dedup side (which
//!   notifies the parked duplicate waiter with `TimedOut` and evicts the
//!   `InFlight` marker). The two sweeps share the same `INFLIGHT_TTL` and
//!   run on the same background tick, so a leaked reply is freed on both
//!   sides together — and only EXPLICIT entries, leaving RETURN in-flight
//!   entries untouched.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

pub(crate) struct OutstandingReplies {
    count: AtomicUsize,
    entries: Mutex<HashMap<String, Instant>>,
}

impl OutstandingReplies {
    pub(crate) fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register an outstanding EXPLICIT reply (called at `ReplyDeliver`
    /// construction in `admit_incoming`).
    pub(crate) async fn register(&self, request_id: &str) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.entries
            .lock()
            .await
            .insert(request_id.to_string(), Instant::now());
    }

    /// Mark an outstanding reply complete (called from `ReplyDeliver::deliver_async`).
    /// Idempotent: a second call (e.g. sweep racing the Deliver) is a no-op.
    pub(crate) async fn complete(&self, request_id: &str) {
        if self.entries.lock().await.remove(request_id).is_some() {
            self.count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Current outstanding count (the `outstanding_replies` gauge).
    pub(crate) fn len(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// Evict registry entries older than `ttl`, decrementing the counter for
    /// each. This is the leak-safety half of the RFC-0323 §7 sweep — the
    /// dedup-side eviction of the InFlight marker (and duplicate-waiter
    /// notification) is handled by
    /// `DedupState::sweep_inflight_explicit`, which runs on the same
    /// background tick scoped to the ids this registry exposes via
    /// [`leaked_ids`]. The node background task uses [`leaked_ids`] +
    /// [`complete`] instead, so this whole-entry retain form is kept for
    /// direct registry tests only.
    #[cfg(test)]
    pub(crate) async fn sweep(&self, ttl: Duration, now: Instant) {
        let n = {
            let mut entries = self.entries.lock().await;
            let before = entries.len();
            entries.retain(|_, admitted_at| now.duration_since(*admitted_at) < ttl);
            before - entries.len()
        };
        if n != 0 {
            self.count.fetch_sub(n, Ordering::SeqCst);
        }
    }

    /// Snapshot of request_ids whose outstanding reply has aged past `ttl`
    /// (RFC-0323 §7). The caller pairs this with
    /// `DedupState::sweep_inflight_explicit` (notify the parked duplicate
    /// waiter + evict the dedup `InFlight` marker) and [`complete`]
    /// (decrement the gauge). Does not mutate the registry — `complete`
    /// does the removal, so a `deliver_async` that completes a reply between
    /// the snapshot and the `complete` makes the latter a no-op.
    pub(crate) async fn leaked_ids(&self, ttl: Duration, now: Instant) -> Vec<String> {
        self.entries
            .lock()
            .await
            .iter()
            .filter(|(_, admitted_at)| now.duration_since(**admitted_at) >= ttl)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Clock-reading form of [`leaked_ids`] for the background sweep task
    /// (keeps the `Instant::now()` read in this exempt module).
    pub(crate) async fn leaked_ids_now(&self, ttl: Duration) -> Vec<String> {
        self.leaked_ids(ttl, Instant::now()).await
    }
}

#[cfg(test)]
#[path = "outstanding_replies_tests.rs"]
mod tests;
