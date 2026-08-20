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
use webrtc::data_channel::DataChannel;

use crate::wire::webrtc::v020::status::LaneKind;

/// A cheap, cloneable handle to one live DataChannel on one PC generation.
#[derive(Clone)]
pub(crate) struct LaneHandle {
    generation: u64,
    lane: LaneKind,
    channel: Arc<dyn DataChannel>,
    cancellation: CancellationToken,
}

impl LaneHandle {
    pub(crate) fn new(
        generation: u64,
        lane: LaneKind,
        channel: Arc<dyn DataChannel>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            generation,
            lane,
            channel,
            cancellation,
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn lane(&self) -> LaneKind {
        self.lane
    }

    pub(crate) fn channel(&self) -> &Arc<dyn DataChannel> {
        &self.channel
    }

    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
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
/// A `watch` version counter is bumped on every insert so lazy-create waiters
/// (`RtcPeerControllerHandle::lane_or_ensure`) can sleep until a lane appears
/// instead of busy-polling the map (N1).
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

    /// The current handle for `(generation, lane)`, if present and live.
    pub(crate) fn get(&self, generation: u64, lane: LaneKind) -> Option<LaneHandle> {
        self.lanes
            .read()
            .expect("lane registry lock")
            .get(&(generation, lane))
            .cloned()
            .filter(|h| h.is_live())
    }

    /// Remove the handle for `(generation, lane)` (lane closed).
    pub(crate) fn remove(&self, generation: u64, lane: LaneKind) {
        self.lanes
            .write()
            .expect("lane registry lock")
            .remove(&(generation, lane));
    }

    /// Remove every handle belonging to a generation (session closing).
    pub(crate) fn remove_generation(&self, generation: u64) {
        self.lanes
            .write()
            .expect("lane registry lock")
            .retain(|(g, _), _| *g != generation);
    }

    /// Remove and return every handle belonging to a generation, so the caller
    /// can explicitly close each `DataChannel` before closing the PeerConnection
    /// (RFC: "Shutdown explicitly calls each `DataChannel::close()`").
    pub(crate) fn drain_generation(&self, generation: u64) -> Vec<LaneHandle> {
        let mut lanes = self.lanes.write().expect("lane registry lock");
        let keys: Vec<(u64, LaneKind)> = lanes
            .keys()
            .filter(|(g, _)| *g == generation)
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|key| lanes.remove(&key))
            .collect()
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
}
