//! `RtcConnectionManager` — the process-wide directory of peer controllers
//! (RFC §"RtcConnectionManager").
//!
//! The manager owns the shared `SessionFactory` and the signaling channel, and
//! a registry of `ActrId → RtcPeerControllerHandle`. It creates/finds/removes
//! controllers (one live controller per remote `ActrId`, invariant #1), creates
//! an answerer controller on an unknown peer's offer, and offers
//! query/subscribe/close-peer/close-all.
//!
//! It MUST NOT store any single-peer role/SDP/ICE/DataChannel/recovering/sendable
//! state (RFC §225-230) — that lives exclusively in each controller. And it MUST
//! NOT hold the `peers` lock across an `.await` (invariant #9): all lock scopes
//! here do synchronous work only; channel sends and task joins happen after the
//! lock is released.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use actr_protocol::ActrId;
use futures_util::future::join_all;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use webrtc::peer_connection::RTCSessionDescription;

use crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor;
use crate::wire::webrtc::v020::signaling::{RelayAttemptIdentity, SignalingDispatch};
use crate::wire::webrtc::v020::status::{PeerTransportRetirement, RtcPeerSnapshot};

use super::peer::controller::RtcPeerController;
use super::peer::effects::SessionFactory;
use super::peer::handle::{ControllerGone, RtcPeerControllerHandle};
use super::peer::reducer::{LaneEvent, SignalingMessage};

/// Overall deadline for `close_all`: all peers are closed concurrently and
/// the entire operation must converge within this bound (RFC §"Shutdown":
/// "timeout cannot be accumulated in Peer order").
const CLOSE_ALL_OVERALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// A failed peer must not retain a closing tombstone indefinitely. Normal
/// controller/session teardown gets one bounded window; after it expires the
/// manager aborts the controller task (through `ControllerTask::drop`) and
/// explicitly drains the retained Session registry.
const CLOSE_PEER_OVERALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Final explicit Session cleanup after the normal single-peer close deadline.
/// This is deliberately short: the registry tombstone must be retired so a new
/// controller generation can be admitted even if a lower-layer close regresses.
const CLOSE_PEER_FALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// A credential rotation must not let one unresponsive controller prevent the
/// remaining PeerConnections from receiving the new TURN configuration.
const CONFIGURATION_REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// One registered peer: its handle plus the plumbing needed to feed it and to
/// converge it on close.
struct PeerEntry {
    handle: RtcPeerControllerHandle,
    lane_tx: mpsc::Sender<LaneEvent>,
    task: ControllerTask,
}

/// A controller task must never become detached. Normal teardown awaits it;
/// the close-all deadline's exceptional drop path aborts it explicitly.
struct ControllerTask(Option<JoinHandle<()>>);

impl ControllerTask {
    fn new(task: JoinHandle<()>) -> Self {
        Self(Some(task))
    }

    async fn join(mut self) {
        if let Some(task) = self.0.take() {
            let _ = task.await;
        }
    }
}

impl Drop for ControllerTask {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

impl PeerEntry {
    fn snapshot_generation_floor(&self) -> u64 {
        self.handle
            .snapshot()
            .pc_generation
            .saturating_add(1)
            .max(1)
    }
}

/// Registry tombstone retained while a controller is physically converging.
/// A concurrent ensure waits for `done`, preserving one live controller per
/// logical peer without holding the registry lock across an await.
struct ClosingPeer {
    handle: RtcPeerControllerHandle,
    marker: Arc<CloseMarker>,
}

struct CloseMarker {
    done: CancellationToken,
}

impl CloseMarker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            done: CancellationToken::new(),
        })
    }
}

/// Drop-time safety net for `close_all`'s Closing tombstones. When the
/// normal or deadline-fallback path finishes, `finished` is set and Drop is a
/// no-op. When the outer future is cancelled first, Drop re-runs the
/// idempotent `finish_close` for every marker on a detached task so
/// `ensure_controller` cannot wait on a tombstone forever. `finish_close` is
/// marker-matched and registry-removing, so re-running it for an already
/// retired peer is a no-op.
struct TombstoneCleanupGuard {
    manager: Arc<RtcConnectionManager>,
    markers: Vec<(ActrId, Arc<CloseMarker>)>,
    finished: bool,
}

impl Drop for TombstoneCleanupGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let manager = Arc::clone(&self.manager);
        let markers = std::mem::take(&mut self.markers);
        // close_all only ever runs inside a tokio runtime, and a future
        // dropped by an outer timeout is dropped on a runtime worker, so
        // spawning here is safe.
        tokio::spawn(async move {
            for (peer, marker) in markers {
                manager.finish_close(&peer, &marker).await;
            }
        });
    }
}

enum PeerSlot {
    Active(PeerEntry),
    Closing(ClosingPeer),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControllerAdmission {
    /// An explicit local operation may reopen a peer that was closed earlier.
    Local,
    /// Inbound signaling must respect the node-wide close tombstone.
    Inbound,
}

pub(crate) struct RtcConnectionManager {
    factory: Arc<dyn SessionFactory>,
    signaling_tx: mpsc::Sender<SignalingDispatch>,
    /// The shared inbound data-plane router handed to every controller (message
    /// payloads bypass all controllers). Business-level, one per node.
    inbound: std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
    /// One validated resource policy shared by all controllers and sessions.
    buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig,
    /// The global network/App lifecycle broadcast hub (S5-1). Holds a
    /// `broadcast::Sender`; each controller subscribes at spawn time and
    /// selects on the receiver in its event loop.
    supervisor: Arc<RtcRecoverySupervisor>,
    /// Router-side per-peer signaling dispatch locks; installed once right
    /// after `SignalingRouter::new` at the wiring site. Peer teardown
    /// releases entries so remote-controlled identities cannot grow the
    /// router-lifetime map without bound.
    signaling_dispatch_chains:
        std::sync::Mutex<Option<crate::wire::webrtc::v020::signaling::router::PeerDispatchChains>>,
    /// Ledger of resource-scoped per-peer recovery obligations handed off by
    /// peer-scoped Restore effects (RFC-0400 per-peer coverage contract,
    /// invariant 33). This manager is the receiving resource owner.
    recovery_obligations: Arc<crate::wire::webrtc::v020::recovery::PeerRecoveryObligationLog>,
    /// The node-level media-frame registry (the stable business surface).
    /// Each controller's sessions dispatch received
    /// track RTP into it. `None` (tests) drops media dispatch.
    media_registry: Option<Arc<crate::inbound::MediaFrameRegistry>>,
    /// Raw Handler-fact observer used only by the integration evidence
    /// harness. It never participates in Controller decisions.
    #[cfg(feature = "test-utils")]
    driver_event_observer:
        Option<crate::wire::webrtc::v020::peer::controller::NodeDriverEventObserver>,
    /// S6-6b: bumped whenever the peer registry changes (controller created or
    /// removed), so the hook projection re-sweeps peer ids on the edge instead
    /// of polling on an interval.
    peer_set_version: watch::Sender<u64>,
    /// Tombstone generation floor per logical peer. A removed/recreated
    /// controller starts above its retired generation so stale callbacks and
    /// signaling can never alias the new Session.
    next_pc_generations: std::sync::Mutex<HashMap<ActrId, u64>>,
    /// Unit-test seam: when set, every spawned controller runs on this policy
    /// clock instead of the system clock, so controller-side deadlines can be
    /// frozen while caller-side tokio deadlines still fire.
    #[cfg(test)]
    policy_clock_for_test:
        Option<std::sync::Arc<dyn crate::wire::webrtc::v020::peer::policy_clock::PolicyClock>>,
    /// Public revision floor per logical peer. Controller recreation within
    /// one manager lifetime must not make mobile/business observers see a
    /// revision reset even though the underlying watch channel is replaced.
    next_revisions: std::sync::Mutex<HashMap<ActrId, u64>>,
    /// Peers closed by node-wide `close_all` policy. Late offers/role
    /// assignments must not recreate a controller behind terminal cleanup; an
    /// explicit local `connect` clears this admission gate. A per-peer close is
    /// deliberately different: a fresh ordinary Offer remains the production
    /// recovery path when the remote side still owns the logical connection.
    inbound_suppressed_peers: std::sync::Mutex<HashSet<ActrId>>,
    /// Explicit teardown edge consumed by transport/request owners. The PC
    /// generation makes delayed cleanup safe against a replacement transport.
    peer_closing: tokio::sync::broadcast::Sender<(ActrId, u64)>,
    /// Controller-owned hard-recovery edge. Already-sent requests must not
    /// retain their full RPC deadline after the generation that carried them
    /// has been superseded by a replacement build. Soft recovery deliberately
    /// emits no edge: committed reliable work keeps the v0.14 behavior and may
    /// complete when the same Session becomes usable again.
    peer_generation_invalidated: tokio::sync::broadcast::Sender<(ActrId, u64)>,
    /// Every Controller-owned sendability edge. PeerGate uses the paired
    /// falling/rising events to bound response waits after a remote-side
    /// recovery without discarding work during a short Soft Recovery.
    peer_sendability_changed: tokio::sync::broadcast::Sender<(ActrId, bool)>,
    /// Terminal logical-transport edge. Unlike generation invalidation, this
    /// retires the matching `DestTransport` connection flight so a later send
    /// cannot reuse a wire whose physical Sessions have already closed.
    peer_transport_retired: tokio::sync::broadcast::Sender<PeerTransportRetirement>,
    /// Ordered Controller snapshot commits for the workload Hook projection.
    /// The public subscribe API remains a latest-value watch; Hooks consume
    /// this event stream so a rapid non-sendable→sendable transition cannot
    /// hide the business-admission falling edge.
    peer_snapshot_events: tokio::sync::broadcast::Sender<RtcPeerSnapshot>,
    /// Global controller-publication epoch. `close_all()` advances it before
    /// draining the registry, so a creator admitted by an older epoch cannot
    /// publish a controller after the close-all linearization point.
    lifecycle_epoch: AtomicU64,
    /// Unique identity source for Controller lifetimes within this Manager.
    /// Combined with the Controller's transport epoch, it protects terminal
    /// cache cleanup from delayed events after controller recreation.
    next_controller_lifecycle_id: AtomicU64,
    /// Deterministic pre-publication rendezvous used by integration tests to
    /// place a creator on the exact side of the close-all epoch boundary.
    #[cfg(feature = "test-utils")]
    peer_publish_barriers: std::sync::Mutex<HashMap<ActrId, Arc<tokio::sync::Barrier>>>,
    peers: RwLock<HashMap<ActrId, PeerSlot>>,
}

impl RtcConnectionManager {
    pub(crate) fn new(
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
    ) -> Self {
        Self::with_inbound(
            factory,
            signaling_tx,
            std::sync::Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::discard()),
        )
    }

    /// Create a manager with an explicit inbound router (business/tests).
    pub(crate) fn with_inbound(
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        inbound: std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
    ) -> Self {
        Self::with_inbound_config(
            factory,
            signaling_tx,
            inbound,
            crate::wire::webrtc::v020::config::RtcBufferConfig::default(),
        )
    }

    pub(crate) fn with_inbound_config(
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        inbound: std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
        buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig,
    ) -> Self {
        buffer_config
            .validate()
            .expect("RtcConnectionManager requires validated finite resource limits");
        let (peer_closing, _) = tokio::sync::broadcast::channel(256);
        let (peer_generation_invalidated, _) = tokio::sync::broadcast::channel(256);
        let (peer_sendability_changed, _) = tokio::sync::broadcast::channel(256);
        let (peer_transport_retired, _) = tokio::sync::broadcast::channel(256);
        let (peer_snapshot_events, _) = tokio::sync::broadcast::channel(4096);
        Self {
            factory,
            signaling_tx,
            inbound,
            buffer_config: buffer_config.clone(),
            supervisor: Arc::new(RtcRecoverySupervisor::new()),
            recovery_obligations: Arc::new(
                crate::wire::webrtc::v020::recovery::PeerRecoveryObligationLog::new(),
            ),
            media_registry: None,
            #[cfg(feature = "test-utils")]
            driver_event_observer: None,
            signaling_dispatch_chains: std::sync::Mutex::new(None),
            peer_set_version: watch::channel(0u64).0,
            next_pc_generations: std::sync::Mutex::new(HashMap::new()),
            #[cfg(test)]
            policy_clock_for_test: None,
            next_revisions: std::sync::Mutex::new(HashMap::new()),
            inbound_suppressed_peers: std::sync::Mutex::new(HashSet::new()),
            peer_closing,
            peer_generation_invalidated,
            peer_sendability_changed,
            peer_transport_retired,
            peer_snapshot_events,
            lifecycle_epoch: AtomicU64::new(0),
            next_controller_lifecycle_id: AtomicU64::new(1),
            #[cfg(feature = "test-utils")]
            peer_publish_barriers: std::sync::Mutex::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
        }
    }

    /// Attach the node media-frame registry (Stage 6). Call before the manager
    /// is shared (it takes `self` by value); controllers spawned afterwards
    /// dispatch received track media into it.
    pub(crate) fn with_media_registry(
        mut self,
        registry: Arc<crate::inbound::MediaFrameRegistry>,
    ) -> Self {
        self.media_registry = Some(registry);
        self
    }

    /// Unit-test seam: spawn controllers on an injected policy clock so
    /// controller-side deadlines can be frozen while caller-side tokio
    /// deadlines still fire.
    #[cfg(test)]
    pub(crate) fn with_policy_clock_for_test(
        mut self,
        clock: std::sync::Arc<dyn crate::wire::webrtc::v020::peer::policy_clock::PolicyClock>,
    ) -> Self {
        self.policy_clock_for_test = Some(clock);
        self
    }

    #[cfg(feature = "test-utils")]
    pub(crate) fn with_driver_event_observer_for_test(
        mut self,
        observer: crate::wire::webrtc::v020::peer::controller::NodeDriverEventObserver,
    ) -> Self {
        self.driver_event_observer = Some(observer);
        self
    }

    /// The shared inbound router (for the send path / tests to read inbound).
    pub(crate) fn inbound(
        &self,
    ) -> &std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter> {
        &self.inbound
    }

    /// The global recovery supervisor (S5-1). Callers (platform network
    /// monitors, app lifecycle hooks) publish `RtcNetworkEvent`s here; every
    /// controller receives them via its broadcast receiver.
    pub(crate) fn supervisor(&self) -> &Arc<RtcRecoverySupervisor> {
        &self.supervisor
    }

    /// The manager-owned ledger of handed-off per-peer recovery obligations.
    pub(crate) fn recovery_obligation_log(
        &self,
    ) -> &Arc<crate::wire::webrtc::v020::recovery::PeerRecoveryObligationLog> {
        &self.recovery_obligations
    }

    /// Record one resource-scoped recovery obligation handed off by a
    /// peer-scoped Restore effect and acknowledge the record (RFC-0400
    /// per-peer coverage contract). The record call is the acknowledgement:
    /// once it returns, the obligation is durable in this owner.
    pub(crate) fn record_peer_recovery_obligation(
        &self,
        target: crate::wire::webrtc::v020::recovery::PeerRecoveryTarget,
    ) {
        self.recovery_obligations.record(target);
    }

    /// S6-6b: subscribe to peer-registry changes (a version edge fires on every
    /// controller create/remove). The hook projection re-sweeps `peer_ids()`
    /// on each edge rather than polling snapshots on an interval.
    pub(crate) fn subscribe_peers(&self) -> watch::Receiver<u64> {
        self.peer_set_version.subscribe()
    }

    /// The current peer ids (paired with `subscribe_peers` re-sweeps).
    pub(crate) async fn peer_ids(&self) -> Vec<ActrId> {
        self.peers.read().await.keys().cloned().collect()
    }

    /// Subscribe to explicit peer-teardown edges. A lagging consumer must
    /// conservatively release pending work rather than retain a timeout tail.
    pub(crate) fn subscribe_peer_closing(&self) -> tokio::sync::broadcast::Receiver<(ActrId, u64)> {
        self.peer_closing.subscribe()
    }

    /// Subscribe to hard-recovery generation invalidation edges.
    pub(crate) fn subscribe_peer_generation_invalidated(
        &self,
    ) -> tokio::sync::broadcast::Receiver<(ActrId, u64)> {
        self.peer_generation_invalidated.subscribe()
    }

    /// Subscribe to authoritative Controller sendability edges. Both sides of
    /// the edge are emitted so consumers can cancel recovery hysteresis when
    /// the same logical peer becomes usable again.
    pub(crate) fn subscribe_peer_sendability_changed(
        &self,
    ) -> tokio::sync::broadcast::Receiver<(ActrId, bool)> {
        self.peer_sendability_changed.subscribe()
    }

    /// Subscribe to terminal logical-transport retirement edges.
    pub(crate) fn subscribe_peer_transport_retired(
        &self,
    ) -> tokio::sync::broadcast::Receiver<PeerTransportRetirement> {
        self.peer_transport_retired.subscribe()
    }

    #[cfg(test)]
    pub(crate) fn publish_peer_transport_retired_for_test(
        &self,
        retirement: PeerTransportRetirement,
    ) {
        let _ = self.peer_transport_retired.send(retirement);
    }

    /// Subscribe to ordered snapshot commits for edge-sensitive workload Hook
    /// projection. Late subscribers separately replay each Controller's
    /// current snapshot before consuming this stream.
    pub(crate) fn subscribe_peer_snapshot_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<RtcPeerSnapshot> {
        self.peer_snapshot_events.subscribe()
    }

    /// Find or create the controller for `peer` (idempotent, invariant #1).
    ///
    /// The lock is held only for synchronous map access and the (synchronous)
    /// controller spawn; no `.await` happens while the lock is held.
    async fn ensure_controller_for_epoch(
        &self,
        peer: ActrId,
        expected_epoch: u64,
        admission: ControllerAdmission,
    ) -> Option<RtcPeerControllerHandle> {
        loop {
            if self.lifecycle_epoch.load(Ordering::Acquire) != expected_epoch {
                return None;
            }
            if admission == ControllerAdmission::Inbound
                && self
                    .inbound_suppressed_peers
                    .lock()
                    .expect("RTC inbound-admission lock poisoned")
                    .contains(&peer)
            {
                return None;
            }
            // Fast path: copy the close marker out before awaiting it.
            let closing = {
                let peers = self.peers.read().await;
                match peers.get(&peer) {
                    Some(PeerSlot::Active(entry)) => return Some(entry.handle.clone()),
                    Some(PeerSlot::Closing(closing)) => Some(closing.marker.clone()),
                    None => None,
                }
            };
            if let Some(marker) = closing {
                tracing::debug!(
                    peer = %peer.to_string_repr(),
                    "v0.20 waiting for a closing peer entry to converge before re-creating the controller"
                );
                marker.done.cancelled().await;
                continue;
            }

            // Slow path: double-check under the write lock. If close won the
            // race, release the lock before waiting for convergence.
            let mut peers = self.peers.write().await;
            // `close_all` advances the epoch before it waits for this same
            // lock. Checking while holding the lock makes publication and the
            // close-all linearization point totally ordered.
            if self.lifecycle_epoch.load(Ordering::Acquire) != expected_epoch {
                return None;
            }
            // Order inbound publication against close_all while the registry
            // write lock is held. If close-all marks the peer first, this path
            // cannot recreate it; if publication wins, close-all subsequently
            // observes and removes the newly published controller.
            if admission == ControllerAdmission::Inbound
                && self
                    .inbound_suppressed_peers
                    .lock()
                    .expect("RTC inbound-admission lock poisoned")
                    .contains(&peer)
            {
                return None;
            }
            match peers.get(&peer) {
                Some(PeerSlot::Active(entry)) => return Some(entry.handle.clone()),
                Some(PeerSlot::Closing(closing)) => {
                    let marker = closing.marker.clone();
                    drop(peers);
                    marker.done.cancelled().await;
                    continue;
                }
                None => {}
            }
            let next_pc_generation = self
                .next_pc_generations
                .lock()
                .expect("RTC generation floor lock poisoned")
                .get(&peer)
                .copied()
                .unwrap_or(1);
            let next_revision = self
                .next_revisions
                .lock()
                .expect("RTC revision floor lock poisoned")
                .get(&peer)
                .copied()
                .unwrap_or(0);
            let controller_lifecycle_id = self
                .next_controller_lifecycle_id
                .fetch_add(1, Ordering::Relaxed);
            #[cfg(test)]
            let policy_clock = self
                .policy_clock_for_test
                .clone()
                .unwrap_or_else(super::peer::policy_clock::system_policy_clock);
            #[cfg(not(test))]
            let policy_clock = super::peer::policy_clock::system_policy_clock();
            let (handle, lane_tx, task) = RtcPeerController::spawn_with_lifecycle_floors(
                peer.clone(),
                next_pc_generation,
                next_revision,
                self.factory.clone(),
                self.signaling_tx.clone(),
                self.supervisor.subscribe(),
                self.supervisor.subscribe_signaling_generation(),
                super::peer::controller::SharedNodeDeps {
                    inbound: self.inbound.clone(),
                    media_registry: self.media_registry.clone(),
                    buffer_config: self.buffer_config.clone(),
                    #[cfg(feature = "test-utils")]
                    driver_event_observer: self.driver_event_observer.clone(),
                    controller_lifecycle_id,
                    peer_generation_invalidated: Some(self.peer_generation_invalidated.clone()),
                    peer_sendability_changed: Some(self.peer_sendability_changed.clone()),
                    peer_transport_retired: Some(self.peer_transport_retired.clone()),
                    peer_snapshot_events: Some(self.peer_snapshot_events.clone()),
                    recovery_obligations: Some(self.recovery_obligations.clone()),
                },
                policy_clock,
            );
            tracing::info!(
                peer = %peer.to_string_repr(),
                controller_lifecycle_id,
                pc_generation_floor = next_pc_generation,
                inbound = admission == ControllerAdmission::Inbound,
                "v0.20 created peer controller"
            );
            peers.insert(
                peer,
                PeerSlot::Active(PeerEntry {
                    handle: handle.clone(),
                    lane_tx,
                    task: ControllerTask::new(task),
                }),
            );
            self.peer_set_version
                .send_modify(|v| *v = v.wrapping_add(1));
            return Some(handle);
        }
    }

    pub(crate) async fn ensure_controller(&self, peer: ActrId) -> RtcPeerControllerHandle {
        self.reopen_inbound_admission(&peer);
        loop {
            let epoch = self.lifecycle_epoch.load(Ordering::Acquire);
            if let Some(handle) = self
                .ensure_controller_for_epoch(peer.clone(), epoch, ControllerAdmission::Local)
                .await
            {
                return handle;
            }
            tokio::task::yield_now().await;
        }
    }

    #[cfg(feature = "test-utils")]
    pub(crate) fn set_peer_publish_barrier_for_test(
        &self,
        peer: ActrId,
        barrier: Arc<tokio::sync::Barrier>,
    ) {
        self.peer_publish_barriers
            .lock()
            .expect("RTC publish-barrier lock poisoned")
            .insert(peer, barrier);
    }

    #[cfg(feature = "test-utils")]
    async fn wait_at_peer_publish_barrier_for_test(&self, peer: &ActrId) {
        let barrier = self
            .peer_publish_barriers
            .lock()
            .expect("RTC publish-barrier lock poisoned")
            .remove(peer);
        if let Some(barrier) = barrier {
            barrier.wait().await;
            barrier.wait().await;
        }
    }

    /// An existing controller, if present.
    pub(crate) async fn get_controller(&self, peer: &ActrId) -> Option<RtcPeerControllerHandle> {
        match self.peers.read().await.get(peer) {
            Some(PeerSlot::Active(entry)) => Some(entry.handle.clone()),
            Some(PeerSlot::Closing(_)) | None => None,
        }
    }

    /// Registry lifecycle admission used by PeerGate. This does not derive or
    /// publish RTC status; it only prevents a new business send from dialing a
    /// replacement while the old controller is still being closed.
    pub(crate) async fn is_closing(&self, peer: &ActrId) -> bool {
        matches!(
            self.peers.read().await.get(peer),
            Some(PeerSlot::Closing(_))
        )
    }

    /// Initiate a connection to `peer`: ensure the controller exists, then ask
    /// it to enter server-owned role arbitration. The caller is not necessarily
    /// assigned offerer (the send happens after the lock is out of scope).
    pub(crate) async fn connect(&self, peer: ActrId) {
        let _ = self.connect_transport_attempt(peer).await;
    }

    /// Start or join the peer's current transport attempt and return only after
    /// the Controller has committed the Connect command. The returned epoch is
    /// the attempt identity transport code must use when interpreting terminal
    /// snapshots; enqueue completion alone is not an attempt boundary.
    pub(crate) async fn connect_transport_attempt(
        &self,
        peer: ActrId,
    ) -> Result<Option<(RtcPeerControllerHandle, u64)>, ControllerGone> {
        tracing::debug!(
            peer = %peer.to_string_repr(),
            "v0.20 starting or joining a peer transport attempt"
        );
        self.reopen_inbound_admission(&peer);
        let epoch = self.lifecycle_epoch.load(Ordering::Acquire);
        #[cfg(feature = "test-utils")]
        self.wait_at_peer_publish_barrier_for_test(&peer).await;
        let Some(handle) = self
            .ensure_controller_for_epoch(peer, epoch, ControllerAdmission::Local)
            .await
        else {
            return Ok(None);
        };
        let transport_epoch = handle.connect().await?;
        Ok(Some((handle, transport_epoch)))
    }

    /// Handle an incoming offer, creating an answerer controller for an unknown
    /// peer (RFC §"Initial connection" passive flow). The offer is applied after
    /// the controller is ensured; no lock is held across the send.
    pub(crate) async fn handle_incoming_offer(&self, peer: ActrId, offer: RTCSessionDescription) {
        self.handle_incoming_offer_correlated(peer, offer, None)
            .await;
    }

    pub(crate) async fn handle_incoming_offer_correlated(
        &self,
        peer: ActrId,
        offer: RTCSessionDescription,
        exchange_id: Option<String>,
    ) {
        let epoch = self.lifecycle_epoch.load(Ordering::Acquire);
        #[cfg(feature = "test-utils")]
        self.wait_at_peer_publish_barrier_for_test(&peer).await;
        if let Some(handle) = self
            .ensure_controller_for_epoch(peer.clone(), epoch, ControllerAdmission::Inbound)
            .await
        {
            let _ = handle
                .apply_remote_description_correlated(offer, exchange_id)
                .await;
        } else {
            tracing::debug!(
                peer = %peer.to_string_repr(),
                "dropping offer for an RTC peer closed by node-wide cleanup"
            );
        }
    }

    /// Apply a server role assignment under the same publication epoch as the
    /// inbound signaling dispatch. This preserves passive-offerer creation but
    /// prevents a router already racing `close_all()` from republishing later.
    pub(crate) async fn handle_role_assignment(&self, peer: ActrId, is_offerer: bool) {
        let epoch = self.lifecycle_epoch.load(Ordering::Acquire);
        #[cfg(feature = "test-utils")]
        self.wait_at_peer_publish_barrier_for_test(&peer).await;
        if let Some(handle) = self
            .ensure_controller_for_epoch(peer.clone(), epoch, ControllerAdmission::Inbound)
            .await
        {
            let _ = handle.apply_role_assignment(is_offerer).await;
        } else {
            tracing::debug!(
                peer = %peer.to_string_repr(),
                "dropping role assignment for an RTC peer closed by node-wide cleanup"
            );
        }
    }

    fn reopen_inbound_admission(&self, peer: &ActrId) {
        self.inbound_suppressed_peers
            .lock()
            .expect("RTC inbound-admission lock poisoned")
            .remove(peer);
    }

    /// Re-open passive controller creation after node-wide cleanup has fenced
    /// and drained the old signaling generation. The lifecycle owner calls
    /// this before installing a fresh signaling generation, so messages from
    /// the retired socket cannot recreate peers while legitimate Offers on the
    /// replacement socket remain admissible.
    pub(crate) fn reopen_all_inbound_admission(&self) {
        self.inbound_suppressed_peers
            .lock()
            .expect("RTC inbound-admission lock poisoned")
            .clear();
    }

    /// Non-blocking signaling response used by the router for protocol-level
    /// rejects. The router is the single inbound consumer and must not head-of-
    /// line block unrelated peers on a full outbound queue.
    pub(crate) fn try_send_signaling(&self, peer: ActrId, message: SignalingMessage) -> bool {
        self.signaling_tx
            .try_send(SignalingDispatch::untracked(peer, message))
            .is_ok()
    }

    /// Apply an Actrix point-in-time fact that the exact target endpoint had no
    /// usable signaling session. The echoed identity fences delayed results
    /// from older Controller lifetimes and transport/PC attempts; the
    /// Controller keeps a matching attempt under its existing bounded deadline
    /// because temporary mobile suspension is indistinguishable from permanent
    /// identity loss at this layer.
    pub(crate) async fn handle_target_unavailable(
        &self,
        peer: ActrId,
        attempt: RelayAttemptIdentity,
    ) {
        let Some(handle) = self.get_controller(&peer).await else {
            return;
        };
        if handle.controller_lifecycle_id() != attempt.controller_lifecycle_id {
            tracing::debug!(
                peer = %peer.to_string_repr(),
                expected_controller_lifecycle_id = handle.controller_lifecycle_id(),
                received_controller_lifecycle_id = attempt.controller_lifecycle_id,
                "ignoring target-unavailable result from a retired RTC Controller"
            );
            return;
        }
        let _ = handle
            .target_unavailable(
                attempt.transport_epoch,
                attempt.pc_generation,
                attempt.negotiation_epoch,
            )
            .await;
    }

    /// The transport failed to commit critical SDP for one dispatch (issue
    /// #609). The echoed attempt identity fences delayed failures from older
    /// Controller lifetimes and transport/PC attempts exactly like
    /// `handle_target_unavailable`, so a stale failure cannot fire
    /// `negotiation_failed` on a live generation.
    pub(crate) async fn handle_signaling_delivery_failed(
        &self,
        peer: ActrId,
        attempt: RelayAttemptIdentity,
        message: SignalingMessage,
    ) {
        let critical_sdp = matches!(
            message,
            SignalingMessage::Offer { .. }
                | SignalingMessage::Answer { .. }
                | SignalingMessage::IceRestartOffer { .. }
        );
        let Some(handle) = self.get_controller(&peer).await else {
            return;
        };
        if !critical_sdp || handle.controller_lifecycle_id() != attempt.controller_lifecycle_id {
            return;
        }
        let _ = handle
            .signaling_delivery_failed(attempt.transport_epoch, attempt.pc_generation)
            .await;
    }

    /// The latest snapshot for a peer, if it has a controller.
    pub(crate) async fn get_snapshot(&self, peer: &ActrId) -> Option<RtcPeerSnapshot> {
        match self.peers.read().await.get(peer) {
            Some(PeerSlot::Active(entry)) => Some(entry.handle.snapshot()),
            Some(PeerSlot::Closing(closing)) => Some(closing.handle.snapshot()),
            None => None,
        }
    }

    /// Snapshots for all peers that currently have a controller.
    pub(crate) async fn list_snapshots(&self) -> Vec<RtcPeerSnapshot> {
        self.peers
            .read()
            .await
            .values()
            .map(|slot| match slot {
                PeerSlot::Active(entry) => entry.handle.snapshot(),
                PeerSlot::Closing(closing) => closing.handle.snapshot(),
            })
            .collect()
    }

    /// Reconstruct terminal transport-retirement identities from authoritative
    /// Controller state. Consumers use this after broadcast lag so an event
    /// storm cannot leave a terminal `DestTransport` cached indefinitely.
    pub(crate) async fn list_terminal_transport_retirements(&self) -> Vec<PeerTransportRetirement> {
        self.peers
            .read()
            .await
            .values()
            .filter_map(|slot| {
                let handle = match slot {
                    PeerSlot::Active(entry) => &entry.handle,
                    PeerSlot::Closing(closing) => &closing.handle,
                };
                let snapshot = handle.snapshot();
                let reason = snapshot.unavailable_reason?;
                (snapshot.status == actr_framework::WebRtcPeerStatus::Idle
                    && !snapshot.sendable
                    && snapshot.transport_epoch > 0)
                    .then(|| PeerTransportRetirement {
                        lifecycle: crate::transport::WebRtcTransportLifecycle {
                            peer_id: snapshot.peer,
                            controller_lifecycle_id: handle.controller_lifecycle_id(),
                            transport_epoch: snapshot.transport_epoch,
                        },
                        reason,
                    })
            })
            .collect()
    }

    /// Apply a newly issued TURN credential to every active peer through the
    /// v0.20-native `set_configuration()` path.
    ///
    /// Controller handles are copied while the registry is read, then awaited
    /// without holding the manager lock. A controller that is currently idle
    /// or closing rejects the refresh; its next Factory build will still read
    /// the same latest credential from the shared provider.
    pub(crate) async fn refresh_all_configurations(
        &self,
        configuration: webrtc::peer_connection::RTCConfiguration,
    ) -> usize {
        let handles = self
            .peers
            .read()
            .await
            .iter()
            .filter_map(|(peer, slot)| match slot {
                PeerSlot::Active(entry) => Some((peer.clone(), entry.handle.clone())),
                PeerSlot::Closing(_) => None,
            })
            .collect::<Vec<_>>();

        let refreshes = handles.into_iter().map(|(peer, handle)| {
            let configuration = configuration.clone();
            async move {
                let result = crate::timer::timeout(
                    crate::timer::ids::RTC_CONFIGURATION_REFRESH,
                    CONFIGURATION_REFRESH_TIMEOUT,
                    handle.refresh_configuration(configuration),
                )
                .await;
                (peer, result)
            }
        });

        join_all(refreshes)
            .await
            .into_iter()
            .filter_map(|(peer, result)| match result {
                Ok(Ok(())) => Some(()),
                Ok(Err(error)) => {
                    tracing::error!(
                        peer = %peer.to_string_repr(),
                        error = %error,
                        "v0.20 PeerConnection TURN configuration refresh failed"
                    );
                    None
                }
                Err(_) => {
                    tracing::error!(
                        peer = %peer.to_string_repr(),
                        timeout_ms = CONFIGURATION_REFRESH_TIMEOUT.as_millis(),
                        "v0.20 PeerConnection TURN configuration refresh timed out"
                    );
                    None
                }
            })
            .count()
    }

    /// Subscribe to a peer's snapshot updates, if it has a controller.
    pub(crate) async fn subscribe(
        &self,
        peer: &ActrId,
    ) -> Option<watch::Receiver<RtcPeerSnapshot>> {
        match self.peers.read().await.get(peer) {
            Some(PeerSlot::Active(entry)) => Some(entry.handle.subscribe()),
            Some(PeerSlot::Closing(closing)) => Some(closing.handle.subscribe()),
            None => None,
        }
    }

    /// Subscribe to the Controller-maintained public/FFI state with immediate
    /// replay and no per-subscriber forwarding task.
    pub(crate) async fn subscribe_public(
        &self,
        peer: &ActrId,
    ) -> Option<watch::Receiver<actr_framework::WebRtcPeerState>> {
        match self.peers.read().await.get(peer) {
            Some(PeerSlot::Active(entry)) => Some(entry.handle.subscribe_public()),
            Some(PeerSlot::Closing(closing)) => Some(closing.handle.subscribe_public()),
            None => None,
        }
    }

    /// Unified recovery/resource/teardown observation for the T2-T3 and
    /// system/platform harnesses (S5-11, RFC Stage 5 exit: "system/platform
    /// harnesses can access unified Snapshot/Hook/resource observation").
    ///
    /// One call aggregates, per peer: the authoritative [`RtcPeerSnapshot`]
    /// (recovery facts — status, generation, mailbox overflows, dropped
    /// signaling) and the lane count; plus the Actr-wide totals. The lock is
    /// held only for synchronous reads (invariant #9).
    pub(crate) async fn observe(&self) -> RtcNodeObservation {
        let peers = self.peers.read().await;
        let observations = peers
            .values()
            .map(|slot| {
                let handle = match slot {
                    PeerSlot::Active(entry) => &entry.handle,
                    PeerSlot::Closing(closing) => &closing.handle,
                };
                let snapshot = handle.snapshot();
                RtcPeerObservation {
                    live_lanes: handle.lanes().len(),
                    // A controller that reached terminal Idle (no live session)
                    // is converged; close_peer/close_all remove it from the
                    // registry entirely, so its absence is the final signal.
                    converged: matches!(snapshot.status, actr_framework::WebRtcPeerStatus::Idle)
                        && !snapshot.sendable,
                    snapshot,
                }
            })
            .collect::<Vec<_>>();
        RtcNodeObservation {
            peer_count: observations.len(),
            inbound_dropped_messages: self.inbound.dropped_messages(),
            peers: observations,
        }
    }

    /// Close one peer: remove its controller and converge its task. Returns
    /// whether the peer existed. The handle close and task join happen after the
    /// lock is released (invariant #9). The command/lane senders are dropped
    /// before joining so the controller's event loop observes the closed channel
    /// and exits — otherwise the join would wait forever.
    /// Install the router-side per-peer dispatch locks. Called at the node or
    /// test-harness wiring site immediately after `SignalingRouter::new`.
    pub(crate) fn set_signaling_dispatch_chains(
        &self,
        locks: crate::wire::webrtc::v020::signaling::router::PeerDispatchChains,
    ) {
        *self
            .signaling_dispatch_chains
            .lock()
            .expect("RTC dispatch chain registry poisoned") = Some(locks);
    }

    /// Forget the closed peer's router-side dispatch-chain entry. Unconditional
    /// by design: the chain's completion watcher only orders dispatches and
    /// grants no exclusivity, so an in-flight dispatch still completes on its
    /// own after the entry is gone.
    fn release_signaling_dispatch_chain(&self, peer: &ActrId) {
        if let Some(chains) = self
            .signaling_dispatch_chains
            .lock()
            .expect("RTC dispatch chain registry poisoned")
            .as_ref()
        {
            chains.release(peer);
        }
    }

    pub(crate) async fn close_peer(self: &Arc<Self>, peer: &ActrId) -> bool {
        enum CloseAction {
            Start(Box<PeerEntry>, Arc<CloseMarker>),
            Wait(Arc<CloseMarker>),
            Missing,
        }

        let action = {
            let mut peers = self.peers.write().await;
            match peers.remove(peer) {
                Some(PeerSlot::Active(entry)) => {
                    let next = entry.snapshot_generation_floor();
                    self.next_pc_generations
                        .lock()
                        .expect("RTC generation floor lock poisoned")
                        .entry(peer.clone())
                        .and_modify(|floor| *floor = (*floor).max(next))
                        .or_insert(next);
                    let marker = CloseMarker::new();
                    peers.insert(
                        peer.clone(),
                        PeerSlot::Closing(ClosingPeer {
                            handle: entry.handle.clone(),
                            marker: marker.clone(),
                        }),
                    );
                    CloseAction::Start(Box::new(entry), marker)
                }
                Some(PeerSlot::Closing(closing)) => {
                    let marker = closing.marker.clone();
                    peers.insert(peer.clone(), PeerSlot::Closing(closing));
                    CloseAction::Wait(marker)
                }
                None => CloseAction::Missing,
            }
        };
        // Router-side dispatch entry for this peer is releasable now that the
        // controller is leaving the registry; an in-flight dispatch keeps its
        // lock until a later teardown observes it idle.
        self.release_signaling_dispatch_chain(peer);

        match action {
            CloseAction::Missing => false,
            CloseAction::Wait(marker) => {
                marker.done.cancelled().await;
                true
            }
            CloseAction::Start(entry, marker) => {
                // Like close_all, close_peer may itself be wrapped in an
                // application/lifecycle timeout. If that outer future is
                // cancelled after the Closing slot is published, retire the
                // marker from Drop so ensure_controller cannot wait forever.
                let mut cleanup_guard = TombstoneCleanupGuard {
                    manager: Arc::clone(self),
                    markers: vec![(peer.clone(), marker.clone())],
                    finished: false,
                };
                let entry = *entry;
                // Request owners must wake before the physical close barrier
                // finishes; otherwise a sent request retains its full RPC
                // deadline despite the peer becoming terminally unavailable.
                let closing_generation = entry.handle.snapshot().pc_generation;
                let _ = self.peer_closing.send((peer.clone(), closing_generation));
                let cleanup_handle = entry.handle.clone();
                if crate::timer::timeout(
                    crate::timer::ids::RTC_CLOSE_PEER_DEADLINE,
                    CLOSE_PEER_OVERALL_TIMEOUT,
                    Self::close_entry(entry),
                )
                .await
                .is_err()
                {
                    // Cancelling `close_entry` drops its `ControllerTask`, whose
                    // Drop implementation aborts the stuck controller. The
                    // retained handle still owns the effect port, so cleanup is
                    // explicit and resumable instead of relying on async Drop.
                    tracing::warn!(
                        peer = %peer.to_string_repr(),
                        "peer close deadline expired; aborting controller and forcing Session cleanup"
                    );
                    if crate::timer::timeout(
                        crate::timer::ids::RTC_CLOSE_PEER_FALLBACK,
                        CLOSE_PEER_FALLBACK_TIMEOUT,
                        cleanup_handle.force_close_sessions(),
                    )
                    .await
                    .is_err()
                    {
                        tracing::warn!(
                            peer = %peer.to_string_repr(),
                            "forced peer Session cleanup timed out; retiring closing tombstone"
                        );
                    }
                    cleanup_handle.mark_secure_closed();
                }
                self.finish_close(peer, &marker).await;
                cleanup_guard.finished = true;
                true
            }
        }
    }

    async fn close_entry(entry: PeerEntry) {
        let PeerEntry {
            handle,
            lane_tx,
            task,
        } = entry;
        let _ = handle.close().await;
        handle.mark_secure_closed();
        drop(handle);
        drop(lane_tx);
        task.join().await;
    }

    async fn finish_close(&self, peer: &ActrId, marker: &Arc<CloseMarker>) {
        let retired_revision = {
            let mut peers = self.peers.write().await;
            match peers.get(peer) {
                Some(PeerSlot::Closing(closing)) if Arc::ptr_eq(&closing.marker, marker) => {
                    let next = closing.handle.snapshot().revision.saturating_add(1);
                    peers.remove(peer);
                    Some(next)
                }
                _ => None,
            }
        };
        if let Some(next_revision) = retired_revision {
            self.next_revisions
                .lock()
                .expect("RTC revision floor lock poisoned")
                .entry(peer.clone())
                .and_modify(|floor| *floor = (*floor).max(next_revision))
                .or_insert(next_revision);
            self.peer_set_version
                .send_modify(|v| *v = v.wrapping_add(1));
            // The peer is conclusively out of scope for every handed-off
            // recovery obligation (RFC-0400 extinguishment path).
            self.recovery_obligations.observe_departure(peer);
        }
        marker.done.cancel();
    }

    /// Close every peer and converge all controller tasks. Drains the registry
    /// under the lock, then closes all peers **concurrently** under an overall
    /// deadline (RFC: "close_all() MUST first copy the Handle in the lock and
    /// release it. After locking, it is bounded and concurrently closed
    /// according to the overall deadline, and timeout cannot be accumulated
    /// in Peer order").
    ///
    /// Each peer's close is spawned as a separate task; the overall deadline
    /// bounds the entire operation. If it expires, remaining Controller tasks
    /// are aborted and the Manager uses their retained effect ports for one
    /// final bounded explicit Session close. Drop is only the last-resort
    /// cover if the WebRTC close future itself cannot converge.
    pub(crate) async fn close_all(self: &Arc<Self>) {
        // The owner's peer lifetime ends here: every open handed-off recovery
        // obligation is extinguished by owner-lifetime-end (RFC-0400
        // extinguishment path), recorded before the drain begins.
        self.recovery_obligations.observe_owner_end();
        // Establish the publication fence even when the registry is currently
        // empty: a creator may already have captured the previous epoch and be
        // waiting immediately before its commit boundary.
        self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
        enum CloseAllJob {
            Start {
                peer: ActrId,
                entry: Box<PeerEntry>,
                marker: Arc<CloseMarker>,
            },
            Wait(Arc<CloseMarker>),
        }
        enum CloseAllDone {
            Started(ActrId, Arc<CloseMarker>),
            Waited,
        }

        let jobs = {
            let mut peers = self.peers.write().await;
            let previous = std::mem::take(&mut *peers);
            self.inbound_suppressed_peers
                .lock()
                .expect("RTC inbound-admission lock poisoned")
                .extend(previous.keys().cloned());
            let mut floors = self
                .next_pc_generations
                .lock()
                .expect("RTC generation floor lock poisoned");
            let mut jobs = Vec::with_capacity(previous.len());
            for (peer, slot) in previous {
                match slot {
                    PeerSlot::Active(entry) => {
                        let next = entry.snapshot_generation_floor();
                        floors
                            .entry(peer.clone())
                            .and_modify(|floor| *floor = (*floor).max(next))
                            .or_insert(next);
                        self.release_signaling_dispatch_chain(&peer);
                        let marker = CloseMarker::new();
                        peers.insert(
                            peer.clone(),
                            PeerSlot::Closing(ClosingPeer {
                                handle: entry.handle.clone(),
                                marker: marker.clone(),
                            }),
                        );
                        jobs.push(CloseAllJob::Start {
                            peer,
                            entry: Box::new(entry),
                            marker,
                        });
                    }
                    PeerSlot::Closing(closing) => {
                        let marker = closing.marker.clone();
                        peers.insert(peer, PeerSlot::Closing(closing));
                        jobs.push(CloseAllJob::Wait(marker));
                    }
                }
            }
            jobs
        };
        if jobs.is_empty() {
            return;
        }
        let started_markers = jobs
            .iter()
            .filter_map(|job| match job {
                CloseAllJob::Start { peer, marker, .. } => Some((peer.clone(), marker.clone())),
                CloseAllJob::Wait(_) => None,
            })
            .collect::<Vec<_>>();

        // Cancellation safety: an outer lifecycle timeout can drop this whole
        // future (including the deadline fallback below) at any await point,
        // which previously left Closing tombstones in the registry forever —
        // `ensure_controller` waits on their markers indefinitely. The guard
        // re-runs the (idempotent) tombstone cleanup on drop when the normal
        // paths did not finish it first.
        let mut cleanup_guard = TombstoneCleanupGuard {
            manager: Arc::clone(self),
            markers: started_markers.clone(),
            finished: false,
        };

        // Spawn each peer close as a separate task so they run concurrently.
        let mut join_set = tokio::task::JoinSet::new();
        for job in jobs {
            match job {
                CloseAllJob::Start {
                    peer,
                    entry,
                    marker,
                } => {
                    let closing_generation = entry.handle.snapshot().pc_generation;
                    let _ = self.peer_closing.send((peer.clone(), closing_generation));
                    join_set.spawn(async move {
                        Self::close_entry(*entry).await;
                        CloseAllDone::Started(peer, marker)
                    });
                }
                CloseAllJob::Wait(marker) => {
                    join_set.spawn(async move {
                        marker.done.cancelled().await;
                        CloseAllDone::Waited
                    });
                }
            }
        }

        // Bound the entire close-all by an overall deadline. If it expires,
        // abort remaining tasks (their Drop impls clean up sessions).
        let result = crate::timer::timeout(
            crate::timer::ids::RTC_CLOSE_ALL_DEADLINE,
            CLOSE_ALL_OVERALL_TIMEOUT,
            async {
                while let Some(joined) = join_set.join_next().await {
                    if let Ok(CloseAllDone::Started(peer, marker)) = joined {
                        self.finish_close(&peer, &marker).await;
                    }
                }
            },
        )
        .await;
        if result.is_ok() {
            cleanup_guard.finished = true;
        }
        if result.is_err() {
            tracing::warn!(
                "close_all deadline expired; aborting {} remaining controller tasks",
                join_set.len()
            );
            join_set.abort_all();
            // First prove the aborted outer/controller tasks have dropped all
            // in-flight use of their runner. Then explicitly close every
            // physical Session through the retained effect port. Keep both
            // phases under one bounded fallback window so peer count cannot
            // multiply the close-all deadline.
            let _ = crate::timer::timeout(
                crate::timer::ids::RTC_CLOSE_ALL_DEADLINE,
                std::time::Duration::from_secs(2),
                async {
                    while join_set.join_next().await.is_some() {}

                    let cleanup_handles = {
                        let peers = self.peers.read().await;
                        started_markers
                            .iter()
                            .filter_map(|(peer, marker)| match peers.get(peer) {
                                Some(PeerSlot::Closing(closing))
                                    if Arc::ptr_eq(&closing.marker, marker) =>
                                {
                                    Some(closing.handle.clone())
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    let mut cleanup = tokio::task::JoinSet::new();
                    for handle in cleanup_handles {
                        cleanup.spawn(async move {
                            handle.force_close_sessions().await;
                            handle.mark_secure_closed();
                        });
                    }
                    while cleanup.join_next().await.is_some() {}
                },
            )
            .await;
            // Retire registry tombstones and wake ensure/close waiters after
            // the bounded explicit cleanup attempt.
            for (peer, marker) in started_markers {
                self.finish_close(&peer, &marker).await;
            }
            cleanup_guard.finished = true;
        }
    }

    /// Number of live controllers (test/diagnostic).
    #[cfg(test)]
    pub(crate) async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }
}

/// One peer's unified observation (S5-11): recovery snapshot + resource usage
/// + teardown signal, as asserted by the T2-T3 and system/platform harnesses.
#[derive(Debug, Clone)]
pub(crate) struct RtcPeerObservation {
    /// The authoritative controller snapshot (recovery facts).
    pub(crate) snapshot: RtcPeerSnapshot,
    /// Live lane handles registered for the active generation.
    pub(crate) live_lanes: usize,
    /// Terminal Idle and not sendable: the controller has no live session
    /// work. (`close_peer`/`close_all` additionally remove the peer from the
    /// registry, so absence is the final teardown signal.)
    pub(crate) converged: bool,
}

/// Node-wide unified observation (S5-11).
#[derive(Debug, Clone)]
pub(crate) struct RtcNodeObservation {
    /// Per-peer observations, one per registered controller.
    pub(crate) peers: Vec<RtcPeerObservation>,
    /// Registered controllers (a fully torn-down peer is absent).
    pub(crate) peer_count: usize,
    /// Messages rejected by inbound frame/reassembly/dispatch byte or count
    /// limits. This makes receive-side overload observable to T2/T3 harnesses.
    pub(crate) inbound_dropped_messages: u64,
}
