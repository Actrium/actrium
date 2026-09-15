//! `LaneHandle` and the `ActiveLaneRegistry` (RFC §"DataChannel model" and
//! §"RtcPeerControllerHandle").
//!
//! The controller installs/replaces/cancels lane handles; the send path reads
//! them (data plane). A `LaneHandle` is the cheapest possible handle to a live
//! DataChannel — it carries exactly what `SendPermit` needs to re-validate a
//! send against the current generation and cancellation, never message payloads
//! or control state.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::wire::webrtc::v020::status::LaneKind;

use super::adapter::LaneDataChannel;

/// Lifecycle of a DataChannel lane in the send-path registry.
///
/// A lane is published before its pump observes `OnOpen`, because the
/// controller must retain the channel while the DCEP handshake is in flight.
/// The send path therefore cannot treat registry presence as send readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneReadiness {
    Pending,
    Ready,
    Closed,
}

/// Result of waiting for one lane's handshake/lifecycle edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneReadinessWait {
    Ready,
    Closed,
    Cancelled,
}

/// Shared, replayable readiness state for one concrete DataChannel.
///
/// The sender lives in an `Arc` shared by the registry and all cloned
/// `LaneHandle`s. A watch channel gives waiters both a level-triggered state
/// and a wake edge, so a waiter cannot miss an `OnOpen` that races its first
/// check.
struct LaneReadinessState {
    tx: watch::Sender<LaneReadiness>,
}

impl LaneReadinessState {
    fn new() -> Arc<Self> {
        let (tx, _) = watch::channel(LaneReadiness::Pending);
        Arc::new(Self { tx })
    }

    fn current(&self) -> LaneReadiness {
        *self.tx.borrow()
    }

    fn set(&self, readiness: LaneReadiness) {
        if self.current() != readiness {
            self.tx.send_replace(readiness);
        }
    }

    async fn wait(&self, cancellation: &CancellationToken) -> LaneReadinessWait {
        let mut changes = self.tx.subscribe();
        loop {
            match *changes.borrow_and_update() {
                LaneReadiness::Ready => return LaneReadinessWait::Ready,
                LaneReadiness::Closed => return LaneReadinessWait::Closed,
                LaneReadiness::Pending => {}
            }

            tokio::select! {
                _ = cancellation.cancelled() => return LaneReadinessWait::Cancelled,
                changed = changes.changed() => {
                    if changed.is_err() {
                        return LaneReadinessWait::Closed;
                    }
                }
            }
        }
    }
}

/// A cheap, cloneable handle to one live DataChannel on one PC generation.
#[derive(Clone)]
pub(crate) struct LaneHandle {
    generation: u64,
    lane: LaneKind,
    channel: Arc<dyn LaneDataChannel>,
    cancellation: CancellationToken,
    readiness: Arc<LaneReadinessState>,
}

impl LaneHandle {
    pub(crate) fn new(
        generation: u64,
        lane: LaneKind,
        channel: Arc<dyn LaneDataChannel>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            generation,
            lane,
            channel,
            cancellation,
            readiness: LaneReadinessState::new(),
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn lane(&self) -> LaneKind {
        self.lane
    }

    pub(crate) fn channel(&self) -> &Arc<dyn LaneDataChannel> {
        &self.channel
    }

    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub(crate) fn readiness(&self) -> LaneReadiness {
        self.readiness.current()
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.readiness() == LaneReadiness::Ready
    }

    pub(crate) async fn wait_ready(&self) -> LaneReadinessWait {
        self.readiness.wait(&self.cancellation).await
    }

    fn mark_ready(&self) {
        self.readiness.set(LaneReadiness::Ready);
    }

    fn mark_closed(&self) {
        self.readiness.set(LaneReadiness::Closed);
    }

    /// Whether this handle still refers to a live, uncancelled generation.
    pub(crate) fn is_live(&self) -> bool {
        !self.cancellation.is_cancelled()
    }
}

/// The shared registry of active lane handles, keyed by `(generation, lane)`.
///
/// Written only by the controller (invariant #8-adjacent: the controller owns
/// lane lifecycle), read by the handle's send path. Cheap to clone (Arc inside).
///
/// A `watch` version counter is bumped on every registry lifecycle change so
/// lazy-create waiters (`RtcPeerControllerHandle::lane_or_ensure`) can sleep
/// until a lane appears or disappears instead of busy-polling the map (N1).
#[derive(Clone)]
pub(crate) struct ActiveLaneRegistry {
    lanes: Arc<RwLock<HashMap<(u64, LaneKind), LaneHandle>>>,
    version_tx: watch::Sender<u64>,
}

impl Default for ActiveLaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveLaneRegistry {
    pub(crate) fn new() -> Self {
        let (version_tx, _) = watch::channel(0u64);
        Self {
            lanes: Arc::new(RwLock::new(HashMap::new())),
            version_tx,
        }
    }

    /// Subscribe to lane insertions. The receiver's `changed()` resolves after
    /// the next `insert` (N1: lets lazy-create waiters sleep, not spin).
    pub(crate) fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    /// Install or replace the handle for `(generation, lane)`.
    pub(crate) fn insert(&self, handle: LaneHandle) {
        self.lanes
            .write()
            .expect("lane registry lock")
            .insert((handle.generation(), handle.lane()), handle);
        // Wake lazy-create waiters. The value itself is meaningless; only the
        // change edge matters.
        self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
    }

    /// Mark the exact generation/lane entry ready after its pump observes the
    /// real DataChannel `OnOpen` event.
    pub(crate) fn mark_ready(&self, generation: u64, lane: LaneKind) -> bool {
        let handle = self
            .lanes
            .read()
            .expect("lane registry lock")
            .get(&(generation, lane))
            .cloned()
            .filter(|h| h.is_live());
        let Some(handle) = handle else {
            return false;
        };
        handle.mark_ready();
        self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
        true
    }

    /// The current handle for `(generation, lane)`, if present and live.
    pub(crate) fn get(&self, generation: u64, lane: LaneKind) -> Option<LaneHandle> {
        self.lanes
            .read()
            .expect("lane registry lock")
            .get(&(generation, lane))
            .cloned()
            .filter(|h| h.is_live())
    }

    /// The current handle only when its DataChannel handshake has completed.
    pub(crate) fn get_ready(&self, generation: u64, lane: LaneKind) -> Option<LaneHandle> {
        self.get(generation, lane).filter(|h| h.is_ready())
    }

    /// Remove the handle for `(generation, lane)` (lane closed).
    pub(crate) fn remove(&self, generation: u64, lane: LaneKind) {
        let removed = self
            .lanes
            .write()
            .expect("lane registry lock")
            .remove(&(generation, lane));
        if let Some(handle) = removed {
            handle.mark_closed();
            self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
        }
    }

    /// Remove every handle belonging to a generation (session closing).
    pub(crate) fn remove_generation(&self, generation: u64) {
        let removed = {
            let mut lanes = self.lanes.write().expect("lane registry lock");
            let keys: Vec<(u64, LaneKind)> = lanes
                .keys()
                .filter(|(g, _)| *g == generation)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| lanes.remove(&key))
                .collect::<Vec<_>>()
        };
        if !removed.is_empty() {
            for handle in removed {
                handle.mark_closed();
            }
            self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
        }
    }

    /// Remove and return every handle belonging to a generation, so the caller
    /// can explicitly close each `DataChannel` before closing the PeerConnection
    /// (RFC: "Shutdown explicitly calls each `DataChannel::close()`").
    pub(crate) fn drain_generation(&self, generation: u64) -> Vec<LaneHandle> {
        let removed = {
            let mut lanes = self.lanes.write().expect("lane registry lock");
            let keys: Vec<(u64, LaneKind)> = lanes
                .keys()
                .filter(|(g, _)| *g == generation)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| lanes.remove(&key))
                .collect::<Vec<_>>()
        };
        if !removed.is_empty() {
            for handle in &removed {
                handle.mark_closed();
            }
            self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
        }
        removed
    }

    /// Number of live lanes (S5-11 observation / diagnostics).
    pub(crate) fn len(&self) -> usize {
        self.lanes.read().expect("lane registry lock").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::webrtc::v020::peer::effects::test_support::fake_data_channel;

    fn handle(generation: u64, lane: LaneKind) -> LaneHandle {
        LaneHandle::new(
            generation,
            lane,
            fake_data_channel(lane.wire_label()),
            CancellationToken::new(),
        )
    }

    /// N1: an `insert` must wake a waiter sleeping on `subscribe_changes()`
    /// (the send path's lazy-create wait must not busy-poll the map).
    #[tokio::test]
    async fn insert_wakes_change_subscribers() {
        let registry = ActiveLaneRegistry::new();
        let mut changes = registry.subscribe_changes();

        // No lane yet: the waiter must be pending (proves changed() slept).
        assert!(registry.get(1, LaneKind::RpcSignal).is_none());

        registry.insert(handle(1, LaneKind::RpcSignal));

        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(5),
            changes.changed(),
        )
        .await
        .expect("insert did not wake the change subscriber")
        .expect("change watch closed unexpectedly");
        assert!(registry.get(1, LaneKind::RpcSignal).is_some());
    }

    /// A subscriber that registers after an insert sees the next change, not a
    /// spurious immediate wake for the pre-subscription insert.
    #[tokio::test]
    async fn late_subscriber_waits_for_next_insert() {
        let registry = ActiveLaneRegistry::new();
        registry.insert(handle(1, LaneKind::RpcSignal));

        let mut changes = registry.subscribe_changes();
        // The pre-subscription insert is already "seen": no pending change, so
        // this wait must expire rather than resolve.
        let expired = crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_millis(100),
            changes.changed(),
        )
        .await;
        assert!(
            expired.is_err(),
            "pre-subscription insert must not wake a late subscriber"
        );

        registry.insert(handle(1, LaneKind::StreamReliable));
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(5),
            changes.changed(),
        )
        .await
        .expect("second insert did not wake the subscriber")
        .expect("change watch closed unexpectedly");
    }

    #[tokio::test]
    async fn lane_stays_pending_until_the_controller_marks_it_ready() {
        let registry = ActiveLaneRegistry::new();
        let handle = handle(1, LaneKind::StreamLatencyFirst);
        registry.insert(handle.clone());

        assert_eq!(handle.readiness(), LaneReadiness::Pending);
        assert!(
            registry
                .get_ready(1, LaneKind::StreamLatencyFirst)
                .is_none()
        );

        let waiter = tokio::spawn({
            let handle = handle.clone();
            async move { handle.wait_ready().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished(), "pending lane waiter completed early");

        assert!(registry.mark_ready(1, LaneKind::StreamLatencyFirst));
        assert_eq!(
            waiter.await.expect("readiness waiter panicked"),
            LaneReadinessWait::Ready
        );
        assert_eq!(handle.readiness(), LaneReadiness::Ready);
        assert!(
            registry
                .get_ready(1, LaneKind::StreamLatencyFirst)
                .is_some()
        );
    }

    #[tokio::test]
    async fn closing_one_generation_wakes_only_that_generation_and_never_opens_the_other() {
        let registry = ActiveLaneRegistry::new();
        let old = handle(1, LaneKind::StreamLatencyFirst);
        let current = handle(2, LaneKind::StreamLatencyFirst);
        registry.insert(old.clone());
        registry.insert(current.clone());

        let old_waiter = tokio::spawn({
            let old = old.clone();
            async move { old.wait_ready().await }
        });
        let current_waiter = tokio::spawn({
            let current = current.clone();
            async move { current.wait_ready().await }
        });
        tokio::task::yield_now().await;

        registry.remove_generation(1);
        assert_eq!(
            old_waiter.await.expect("old readiness waiter panicked"),
            LaneReadinessWait::Closed
        );
        assert_eq!(current.readiness(), LaneReadiness::Pending);
        assert!(
            !current_waiter.is_finished(),
            "new generation was opened by stale close"
        );

        assert!(registry.mark_ready(2, LaneKind::StreamLatencyFirst));
        assert_eq!(
            current_waiter
                .await
                .expect("current readiness waiter panicked"),
            LaneReadinessWait::Ready
        );
    }
}
