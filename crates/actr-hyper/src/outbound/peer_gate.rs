//! PeerGate - Peer transport adapter (outbound)
//!
//! # Responsibilities
//! - Wrap PeerTransport (Protobuf serialization)
//! - Used for cross-process communication (WebRTC + WebSocket)
//! - Maintain pending_requests (Request/Response matching)

use super::ensure_stream_payload_type;
use crate::lifecycle::SendProjection;
#[cfg(any(test, feature = "test-utils"))]
use crate::transport::DATA_CHUNK_SEND_TIMEOUT;
use crate::transport::correlation::PendingRequestsMap;
#[cfg(any(test, feature = "test-utils"))]
use crate::transport::correlation::{CompleteOutcome, RpcCorrelation};
use crate::transport::{
    DeliveryProgress, DeliveryProgressCell, Dest, DestTransportRef, PayloadTypeExt,
    PeerSendAttempt, PeerTransport, RpcDeadline, SentTransportSlot, WireIdentity,
    new_delivery_progress_cell, observed_delivery_progress,
};
use actr_framework::Bytes;
#[cfg(any(feature = "webrtc-v020", test))]
use actr_framework::MediaSample;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    ActorResult, ActrError, ActrId, Classify, ConnectionNotReadyInfo, Direction, PayloadType,
    RpcEnvelope,
};
// The bounded readiness-waiter accounting uses HashMap unconditionally; the
// v0.20 cleanup tasks (also consumers on their builds) share this import.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
#[cfg(any(test, feature = "test-utils", feature = "webrtc-v020"))]
use std::time::Duration;
#[cfg(any(test, feature = "test-utils"))]
use tokio::sync::oneshot;
use tokio::sync::{Mutex, watch};

/// Build a stable caller-correlatable identity for one DataChunk. Stream IDs
/// identify the logical stream; the sequence suffix distinguishes chunks that
/// may be uncertain independently on the same stream.
pub(crate) fn data_chunk_operation_id(stream_id: &str, sequence: u64) -> String {
    format!("{stream_id}#{sequence}")
}

/// Build the caller-visible identity for a whole-stream delivery-uncertainty
/// event (a WebRTC session close invalidating every outstanding chunk on the
/// stream at once, where no single sequence is authoritative).
///
/// Chunk keys end in `#` followed by the decimal sequence
/// ([`data_chunk_operation_id`]); this key ends in the non-numeric marker
/// `#stream`. Because a chunk sequence is always decimal, no chunk key can
/// end in `#stream`, and this key never ends in `#` + digits — the two
/// scopes are disjoint by construction even when a stream id itself contains
/// `#` (e.g. stream `"s#4"` stream-scoped key is `"s#4#stream"`, which
/// cannot collide with the chunk key `"s#4"` of stream `"s"`).
///
/// Contract for stream-scoped uncertain-delivery emitters; exercised by the
/// hook tests today (a production stream-wide emitter adopts it on landing).
#[allow(dead_code)]
pub(crate) fn data_stream_operation_id(stream_id: &str) -> String {
    format!("{stream_id}#stream")
}

/// Minimum remaining budget a one-shot send (Tell/DataChunk) must retain
/// after route readiness before committing to a transport write. This is a
/// small absolute slice, deliberately not proportional to the caller budget:
/// a route that clears within this slice of the deadline is refused pre-send
/// (retryable `TimedOut`) instead of committing bytes whose mid-write expiry
/// would surface as terminal `DeliveryUncertain`.
const MIN_ONE_SHOT_SEND_SLICE: Duration = Duration::from_millis(25);

/// Preserve in-flight reliable RPCs across a brief sendability dip, but do not
/// let a remote-side recovery leave response waits parked until their much
/// longer business deadline.
#[cfg(feature = "webrtc-v020")]
const PENDING_RECOVERY_GRACE: Duration = Duration::from_secs(5);

/// Per-operation send authority captured at outbound admission.
///
/// Once an operation observes `ExistingOnly`, later policy relaxation must
/// not upgrade it back to `Normal`; every retry retains the strictest
/// projection observed since admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendAdmission {
    ExistingOnly,
    Normal,
}

/// One peer-gate attempt with the route's delivery-progress fact preserved.
struct GateSendAttempt {
    result: ActorResult<Option<DestTransportRef>>,
    progress: DeliveryProgress,
}

/// Hard limits for readiness waiters.  These are deliberately conservative:
/// waiting is bounded work, not an offline payload queue.  Each class retains
/// a minimum share so a burst of one operation type cannot consume the entire
/// global budget.
const MAX_GLOBAL_WAITERS: usize = 256;
const MAX_PEER_WAITERS: usize = 64;
const MAX_GLOBAL_WAITER_BYTES: usize = 16 * 1024 * 1024;
const MAX_PEER_WAITER_BYTES: usize = 4 * 1024 * 1024;
const MIN_RPC_WAITERS: usize = 32;
const MIN_STREAM_WAITERS: usize = 32;
const MAX_RPC_WAITERS: usize = MAX_GLOBAL_WAITERS - MIN_STREAM_WAITERS;
const MAX_STREAM_WAITERS: usize = MAX_GLOBAL_WAITERS - MIN_RPC_WAITERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaiterClass {
    Rpc,
    Stream,
}

impl WaiterClass {
    fn for_payload(payload_type: PayloadType) -> Option<Self> {
        match payload_type {
            PayloadType::RpcReliable => Some(Self::Rpc),
            PayloadType::StreamReliable => Some(Self::Stream),
            _ => None,
        }
    }
}

#[derive(Default)]
struct PeerWaiterUsage {
    waiters: usize,
    bytes: usize,
}

#[derive(Default)]
struct WaiterUsage {
    global_waiters: usize,
    global_bytes: usize,
    rpc_waiters: usize,
    stream_waiters: usize,
    peers: HashMap<ActrId, PeerWaiterUsage>,
}

/// Synchronous accounting for readiness waiters.  The lock is held only while
/// checking/incrementing counters; it is never held across an `.await`.
struct WaiterBudget {
    usage: StdMutex<WaiterUsage>,
}

impl Default for WaiterBudget {
    fn default() -> Self {
        Self {
            usage: StdMutex::new(WaiterUsage::default()),
        }
    }
}

impl WaiterBudget {
    fn try_acquire(
        self: &Arc<Self>,
        peer: &ActrId,
        class: WaiterClass,
        payload_bytes: usize,
    ) -> Option<WaiterGuard> {
        let mut usage = self
            .usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (peer_waiters, peer_bytes) = usage
            .peers
            .get(peer)
            .map(|peer_usage| (peer_usage.waiters, peer_usage.bytes))
            .unwrap_or_default();
        let class_waiters = match class {
            WaiterClass::Rpc => usage.rpc_waiters,
            WaiterClass::Stream => usage.stream_waiters,
        };
        let class_limit = match class {
            WaiterClass::Rpc => MAX_RPC_WAITERS,
            WaiterClass::Stream => MAX_STREAM_WAITERS,
        };
        let fits = usage.global_waiters < MAX_GLOBAL_WAITERS
            && class_waiters < class_limit
            && peer_waiters < MAX_PEER_WAITERS
            && usage
                .global_bytes
                .checked_add(payload_bytes)
                .is_some_and(|bytes| bytes <= MAX_GLOBAL_WAITER_BYTES)
            && peer_bytes
                .checked_add(payload_bytes)
                .is_some_and(|bytes| bytes <= MAX_PEER_WAITER_BYTES);
        if !fits {
            return None;
        }

        usage.global_waiters += 1;
        usage.global_bytes += payload_bytes;
        match class {
            WaiterClass::Rpc => usage.rpc_waiters += 1,
            WaiterClass::Stream => usage.stream_waiters += 1,
        }
        // `peer_usage` points into the map, so reacquire it after the counter
        // updates (which may have reborrowed `usage`).
        let peer_usage = usage.peers.entry(peer.clone()).or_default();
        peer_usage.waiters += 1;
        peer_usage.bytes += payload_bytes;

        Some(WaiterGuard {
            budget: Arc::clone(self),
            peer: peer.clone(),
            class,
            payload_bytes,
        })
    }

    #[cfg(test)]
    fn snapshot(&self) -> (usize, usize, usize, usize) {
        let usage = self
            .usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            usage.global_waiters,
            usage.global_bytes,
            usage.rpc_waiters,
            usage.stream_waiters,
        )
    }
}

struct WaiterGuard {
    budget: Arc<WaiterBudget>,
    peer: ActrId,
    class: WaiterClass,
    payload_bytes: usize,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        let mut usage = self
            .budget
            .usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        usage.global_waiters = usage.global_waiters.saturating_sub(1);
        usage.global_bytes = usage.global_bytes.saturating_sub(self.payload_bytes);
        match self.class {
            WaiterClass::Rpc => usage.rpc_waiters = usage.rpc_waiters.saturating_sub(1),
            WaiterClass::Stream => usage.stream_waiters = usage.stream_waiters.saturating_sub(1),
        }
        let remove_peer = if let Some(peer_usage) = usage.peers.get_mut(&self.peer) {
            peer_usage.waiters = peer_usage.waiters.saturating_sub(1);
            peer_usage.bytes = peer_usage.bytes.saturating_sub(self.payload_bytes);
            peer_usage.waiters == 0
        } else {
            false
        };
        if remove_peer {
            usage.peers.remove(&self.peer);
        }
    }
}

impl SendAdmission {
    fn tighten(self, other: Self) -> Self {
        if self == Self::ExistingOnly || other == Self::ExistingOnly {
            Self::ExistingOnly
        } else {
            Self::Normal
        }
    }
}

/// A `SessionFactory` that never builds — the gate test seam's manager is
/// always empty, so a build attempt is a test bug.
#[cfg(all(feature = "webrtc-v020", feature = "test-utils"))]
struct NoopSessionFactory;

#[cfg(all(feature = "webrtc-v020", feature = "test-utils"))]
#[async_trait::async_trait]
impl crate::wire::webrtc::v020::peer::effects::SessionFactory for NoopSessionFactory {
    async fn build(
        &self,
        _handler: Arc<crate::wire::webrtc::v020::event_handler::WebRtcEventHandler>,
    ) -> Result<Arc<dyn crate::wire::webrtc::v020::peer::effects::SessionOps>, webrtc::error::Error>
    {
        Err(webrtc::error::Error::Other(
            "NoopSessionFactory never builds (gate test seam)".to_string(),
        ))
    }
}

/// PeerGate - Outproc transport adapter (outbound)
///
/// # Features
/// - Protobuf serialization: serialize RpcEnvelope to byte stream
/// - Defaults to PayloadType::RpcReliable for RPC messages
/// - Maintain pending_requests for Request/Response matching
/// - Support MediaTrack sending via WebRTC
///
/// The v0.20 `RtcConnectionManager`'s `RtcPeerSnapshot.sendable` is the only
/// RTC preflight (RFC §"Send path and SendPermit": "PeerGate MUST delete its
/// own recovering-peer collection; the Controller Snapshot and LaneHandle are
/// the only preflight status sources"). There is no recovering/closing-peer
/// tracking here.
pub struct PeerGate {
    /// PeerTransport instance
    transport_manager: Arc<PeerTransport>,

    /// Pending requests: request_id -> (target_actor_id, response sender).
    /// RAII registration - a dropped caller future removes its own entry.
    pending_requests: PendingRequestsMap,

    /// The v0.20 connection manager: the RTC preflight authority.
    #[cfg(feature = "webrtc-v020")]
    v020_manager: Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,

    /// Live global send authority derived by the recovery supervisor.
    send_projection_rx: watch::Receiver<SendProjection>,

    /// Bounded readiness wait accounting.  Waiting never owns an unbounded
    /// payload queue; the guard is released as soon as the wait ends.
    waiter_budget: Arc<WaiterBudget>,

    /// Optional workload hook so a synchronous `DeliveryUncertain` returned
    /// to a caller is also observable through the
    /// `DataChunkDeliveryUncertain` lifecycle hook (the two channels must not
    /// disagree). Installed once by the node when it wires the workload hook
    /// observer.
    hook_callback: std::sync::OnceLock<crate::wire::webrtc::HookCallback>,
}

impl PeerGate {
    /// Create a PeerGate: the v0.20 `RtcConnectionManager`'s snapshot is the
    /// only RTC preflight (RFC item 5). There is no recovering/closing-peer
    /// tracking — the gate never populates or consults any.
    ///
    /// `send_projection_rx` is the recovery supervisor's live projection;
    /// nodes without a network-event handle pass `None` for the always-Normal
    /// default.
    #[cfg(feature = "webrtc-v020")]
    pub(crate) fn with_v020_manager(
        transport_manager: Arc<PeerTransport>,
        v020_manager: Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        pending_requests: PendingRequestsMap,
        send_projection_rx: Option<watch::Receiver<SendProjection>>,
    ) -> Self {
        Self::spawn_pending_close_cleanup(&v020_manager, &transport_manager, &pending_requests);
        Self::spawn_terminal_transport_cleanup(
            &v020_manager,
            &transport_manager,
            &pending_requests,
        );
        Self::spawn_pending_invalidated_generation_cleanup(&v020_manager, &pending_requests);
        Self::spawn_pending_sendability_cleanup(&v020_manager, &pending_requests);
        Self {
            transport_manager,
            pending_requests,
            v020_manager,
            send_projection_rx: send_projection_rx
                .unwrap_or_else(Self::normal_send_projection_receiver),
            waiter_budget: Arc::new(WaiterBudget::default()),
            hook_callback: std::sync::OnceLock::new(),
        }
    }

    /// Create a PeerGate for configurations without the `webrtc-v020`
    /// backend: WebSocket lanes carry peer traffic and there is no RTC
    /// preflight authority, so admission comes from the send projection
    /// alone. The delivery-uncertainty hook stays installable — a WebSocket
    /// write can still classify as `DeliveryUncertain`.
    #[cfg(not(feature = "webrtc-v020"))]
    pub(crate) fn new(
        transport_manager: Arc<PeerTransport>,
        pending_requests: PendingRequestsMap,
        send_projection_rx: Option<watch::Receiver<SendProjection>>,
    ) -> Self {
        Self {
            transport_manager,
            pending_requests,
            send_projection_rx: send_projection_rx
                .unwrap_or_else(Self::normal_send_projection_receiver),
            waiter_budget: Arc::new(WaiterBudget::default()),
            hook_callback: std::sync::OnceLock::new(),
        }
    }

    /// Install the workload hook callback used to mirror synchronous
    /// delivery-uncertainty failures to hook observers. Installing twice is a
    /// caller bug: the first callback is retained and the ignored
    /// re-installation is logged so the lost update stays observable.
    pub fn set_hook_callback(&self, callback: crate::wire::webrtc::HookCallback) {
        if self.hook_callback.set(callback).is_err() {
            tracing::warn!(
                "workload hook callback already installed; first callback retained, ignoring re-installation"
            );
        }
    }

    /// Mirror a synchronous `DeliveryUncertain` to hook observers. The send
    /// path cannot name a session (the failure may precede session selection
    /// or travel over WebSocket), so `session_id` is reported as `None` and
    /// the reason carries the caller-visible cause. The event uses the exact
    /// caller-visible operation id (`stream_id#sequence`) so both channels
    /// reconcile on one key.
    fn emit_data_chunk_uncertain_hook(&self, operation_id: &str, cause: &str) {
        if let Some(callback) = self.hook_callback.get() {
            // The dispatcher enqueues synchronously inside the closure; the
            // returned future is a no-op handle and may be dropped.
            drop(callback(
                crate::wire::webrtc::HookEvent::DataChunkDeliveryUncertain {
                    operation_id: operation_id.to_string(),
                    session_id: None,
                    reason: format!("synchronous send classification: {cause}"),
                },
            ));
        }
    }

    /// Release pending work and retire the exact cached connection flight when
    /// a Controller reaches terminal unavailable `Idle` (for example
    /// `RecoveryExhausted`). Generation invalidation deliberately keeps the
    /// logical transport; this terminal edge removes it so the next send dials
    /// a fresh transport instead of reusing a closed ready wire.
    #[cfg(feature = "webrtc-v020")]
    fn spawn_terminal_transport_cleanup(
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        transport_manager: &Arc<PeerTransport>,
        pending_requests: &PendingRequestsMap,
    ) {
        let mut retired = manager.subscribe_peer_transport_retired();
        let manager = Arc::downgrade(manager);
        let transports = Arc::downgrade(transport_manager);
        let pending = Arc::downgrade(pending_requests);
        tokio::spawn(async move {
            loop {
                match retired.recv().await {
                    Ok(retirement) => {
                        let peer = retirement.lifecycle.peer_id.clone();
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(
                            |target| target == &peer,
                            Self::send_projection_not_ready_error,
                        );

                        if let Some(transports) = transports.upgrade() {
                            let dest = Dest::peer(peer.clone());
                            match transports
                                .close_transport_if_webrtc_lifecycle(&dest, &retirement.lifecycle)
                                .await
                            {
                                Ok(true) => tracing::info!(
                                    peer = ?peer,
                                    reason = ?retirement.reason,
                                    transport_epoch = retirement.lifecycle.transport_epoch,
                                    "retired terminal WebRTC DestTransport"
                                ),
                                Ok(false) => tracing::debug!(
                                    peer = ?peer,
                                    reason = ?retirement.reason,
                                    transport_epoch = retirement.lifecycle.transport_epoch,
                                    "terminal transport already absent or replaced"
                                ),
                                Err(error) => tracing::warn!(
                                    peer = ?peer,
                                    reason = ?retirement.reason,
                                    %error,
                                    "failed to retire terminal WebRTC DestTransport"
                                ),
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "terminal-transport cleanup lagged; releasing all pending RPCs"
                        );
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(|_| true, Self::send_projection_not_ready_error);
                        let (Some(manager), Some(transports)) =
                            (manager.upgrade(), transports.upgrade())
                        else {
                            break;
                        };
                        for retirement in manager.list_terminal_transport_retirements().await {
                            let dest = Dest::peer(retirement.lifecycle.peer_id.clone());
                            if let Err(error) = transports
                                .close_transport_if_webrtc_lifecycle(&dest, &retirement.lifecycle)
                                .await
                            {
                                tracing::warn!(
                                    peer = ?retirement.lifecycle.peer_id,
                                    %error,
                                    "failed terminal-transport reconciliation after lag"
                                );
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Release already-sent RPCs when the manager starts explicit peer
    /// teardown. Snapshot preflight governs new sends; this edge governs the
    /// response wait that remains after bytes have entered the DataChannel.
    #[cfg(feature = "webrtc-v020")]
    fn spawn_pending_close_cleanup(
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        transport_manager: &Arc<PeerTransport>,
        pending_requests: &PendingRequestsMap,
    ) {
        let mut closing = manager.subscribe_peer_closing();
        let transports = Arc::downgrade(transport_manager);
        let pending = Arc::downgrade(pending_requests);
        tokio::spawn(async move {
            loop {
                match closing.recv().await {
                    Ok((peer, pc_generation)) => {
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(
                            |target| target == &peer,
                            Self::send_projection_not_ready_error,
                        );
                        if let Some(transports) = transports.upgrade() {
                            let dest = Dest::peer(peer.clone());
                            let close_result = if pc_generation == 0 {
                                transports.close_transport(&dest).await.map(|_| true)
                            } else {
                                transports
                                    .close_transport_if_webrtc_session(&dest, &peer, pc_generation)
                                    .await
                            };
                            if let Err(error) = close_result {
                                tracing::warn!(
                                    %error,
                                    "failed to evict DestTransport after peer close"
                                );
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "peer-close cleanup lagged; conservatively releasing all pending RPCs"
                        );
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(|_| true, Self::send_projection_not_ready_error);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Release response waits when the Controller starts replacing the Session
    /// that carried them. Unlike explicit close cleanup, this deliberately
    /// keeps the DestTransport: hard recovery replaces its generation in place
    /// and the generation-aware v0.20 lanes become usable after convergence.
    /// A soft-recovery sendability dip does not retire a Session, so reliable
    /// work already accepted by the lane remains eligible to complete.
    #[cfg(feature = "webrtc-v020")]
    fn spawn_pending_invalidated_generation_cleanup(
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        pending_requests: &PendingRequestsMap,
    ) {
        let mut invalidated = manager.subscribe_peer_generation_invalidated();
        let pending = Arc::downgrade(pending_requests);
        tokio::spawn(async move {
            loop {
                match invalidated.recv().await {
                    Ok((peer, _generation)) => {
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(
                            |target| target == &peer,
                            Self::send_projection_not_ready_error,
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "generation-invalidation cleanup lagged; releasing all pending RPCs"
                        );
                        let Some(pending) = pending.upgrade() else {
                            break;
                        };
                        pending.fail_where(|_| true, Self::send_projection_not_ready_error);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Bound pending response waits when this endpoint only learns about a
    /// remote-side recovery through its own Controller callbacks. A paired
    /// rising edge cancels the grace period, preserving the v0.14 behavior for
    /// short Soft Recovery. If the peer stays non-sendable, only correlation
    /// waiters are released; the DestTransport remains available for the
    /// Controller's generation-aware recovery path.
    #[cfg(feature = "webrtc-v020")]
    fn spawn_pending_sendability_cleanup(
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        pending_requests: &PendingRequestsMap,
    ) {
        let mut sendability = manager.subscribe_peer_sendability_changed();
        let manager = Arc::downgrade(manager);
        let pending = Arc::downgrade(pending_requests);
        // Grace-task registry shared with the spawned tasks: each task
        // self-removes on completion (guarded by its registration id, so a
        // replaced newer task is never evicted by an older one's cleanup).
        // Without self-cleanup a peer whose last observed edge was
        // non-sendable and which is then closed/removed never sees the rising
        // edge that was the only removal path, leaking its entry forever.
        let grace_tasks = Arc::new(std::sync::Mutex::new(HashMap::<
            ActrId,
            (u64, tokio::task::JoinHandle<()>),
        >::new()));
        let grace_seq = std::sync::atomic::AtomicU64::new(1);
        tokio::spawn(async move {
            loop {
                match sendability.recv().await {
                    Ok((peer, true)) => {
                        if let Some((_, task)) =
                            grace_tasks.lock().expect("grace task map").remove(&peer)
                        {
                            task.abort();
                        }
                    }
                    Ok((peer, false)) => {
                        let task_manager = manager.clone();
                        let task_pending = pending.clone();
                        let task_peer = peer.clone();
                        let task_map = grace_tasks.clone();
                        let task_id = grace_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let task = tokio::spawn(async move {
                            // Core grace logic: wait out the grace period, then
                            // release the peer's pending waiters unless the peer
                            // recovered. Early exits fall through to the shared
                            // self-cleanup below.
                            crate::timer::sleep(
                                crate::timer::ids::PEER_PENDING_RECOVERY_GRACE,
                                PENDING_RECOVERY_GRACE,
                            )
                            .await;
                            if let Some(manager) = task_manager.upgrade()
                                && manager
                                    .get_snapshot(&task_peer)
                                    .await
                                    .is_some_and(|snapshot| snapshot.sendable)
                            {
                            } else if let Some(pending) = task_pending.upgrade() {
                                pending.fail_where(
                                    |target| target == &task_peer,
                                    Self::send_projection_not_ready_error,
                                );
                            }
                            // Self-cleanup on every exit path: drop this task's
                            // registration unless a newer grace task for the
                            // same peer already replaced it.
                            let mut map = task_map.lock().expect("grace task map");
                            if map
                                .get(&task_peer)
                                .is_some_and(|(current_id, _)| *current_id == task_id)
                            {
                                map.remove(&task_peer);
                            }
                        });
                        let mut map = grace_tasks.lock().expect("grace task map");
                        if let Some((_, stale)) = map.insert(peer, (task_id, task)) {
                            stale.abort();
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "sendability cleanup lagged; cancelling stale recovery grace tasks"
                        );
                        for (_, (_, task)) in grace_tasks.lock().expect("grace task map").drain() {
                            task.abort();
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        for (_, (_, task)) in grace_tasks.lock().expect("grace task map").drain() {
                            task.abort();
                        }
                        break;
                    }
                }
            }
        });
    }

    fn normal_send_projection_receiver() -> watch::Receiver<SendProjection> {
        let (_tx, rx) = watch::channel(SendProjection::Normal);
        rx
    }

    fn send_projection_not_ready_error() -> ActrError {
        ActrError::ConnectionNotReady(ConnectionNotReadyInfo::without_retry_hint())
    }

    fn current_send_admission(&self) -> ActorResult<SendAdmission> {
        match *self.send_projection_rx.borrow() {
            SendProjection::Blocked => Err(Self::send_projection_not_ready_error()),
            SendProjection::ExistingOnly => Ok(SendAdmission::ExistingOnly),
            SendProjection::Normal => Ok(SendAdmission::Normal),
        }
    }

    fn tighten_with_current_projection(
        &self,
        admission: SendAdmission,
    ) -> ActorResult<SendAdmission> {
        Ok(admission.tighten(self.current_send_admission()?))
    }

    /// An empty v0.20 manager for gate tests: no peer ever has a controller,
    /// so the snapshot preflight admits and the injected transport drives the
    /// behavior under test.
    #[cfg(all(feature = "webrtc-v020", feature = "test-utils"))]
    fn empty_manager_for_test() -> Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager> {
        let (signaling_tx, _drop_rx) = tokio::sync::mpsc::channel(1);
        Arc::new(
            crate::wire::webrtc::v020::manager::RtcConnectionManager::new(
                Arc::new(NoopSessionFactory),
                signaling_tx,
            ),
        )
    }

    /// Test-support constructor (test-utils): a gate over an EMPTY v0.20
    /// manager (see [`Self::empty_manager_for_test`]) with the always-Normal
    /// projection default. Lets integration tests build a gate without naming
    /// the crate-private manager/factory/projection types.
    #[cfg(all(feature = "webrtc-v020", feature = "test-utils"))]
    pub fn for_test(transport_manager: Arc<PeerTransport>) -> Self {
        Self::with_v020_manager(
            transport_manager,
            Self::empty_manager_for_test(),
            Arc::new(RpcCorrelation::new()),
            None,
        )
    }

    /// Handle response message (called by MessageDispatcher)
    ///
    /// # Arguments
    /// - `request_id`: Request ID
    /// - `result`: Response data (Ok) or error (Err)
    ///
    /// # Returns
    /// - `Ok(true)`: Successfully woke up waiting request
    /// - `Ok(false)`: No corresponding pending request found
    #[cfg(feature = "test-utils")]
    pub async fn handle_response(
        &self,
        request_id: &str,
        result: actr_protocol::ActorResult<Bytes>,
    ) -> ActorResult<bool> {
        Ok(matches!(
            self.pending_requests.complete(request_id, result),
            CompleteOutcome::Completed
        ))
    }

    /// Get pending requests count (for monitoring)
    #[cfg(feature = "test-utils")]
    pub async fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Register a pending request without sending on the wire.
    #[cfg(feature = "test-utils")]
    pub async fn register_pending_for_test(
        &self,
        request_id: &str,
        target: ActrId,
    ) -> oneshot::Receiver<ActorResult<Bytes>> {
        self.pending_requests
            .register(request_id.to_string(), target)
            .into_receiver()
    }

    /// Get pending_requests reference (for WebRtcGate to share)
    pub(crate) fn get_pending_requests(&self) -> PendingRequestsMap {
        self.pending_requests.clone()
    }

    /// Convert ActrId to Dest
    fn actr_id_to_dest(actor_id: &ActrId) -> Dest {
        Dest::peer(actor_id.clone())
    }

    /// Serialize RpcEnvelope to bytes
    fn serialize_envelope(envelope: &RpcEnvelope) -> Vec<u8> {
        envelope.encode_to_vec()
    }

    fn stamp_envelope_direction(mut envelope: RpcEnvelope, direction: Direction) -> RpcEnvelope {
        envelope.direction = Some(direction as i32);
        envelope
    }

    /// Outbound admission. The global send projection is read before any
    /// await so globally blocked work fails fast.
    ///
    /// The v0.20 Controller Snapshot is the only RTC preflight (RFC item 5):
    /// a peer whose controller exists but is not sendable is withheld unless
    /// this payload has an already-ready fallback route; a peer with NO
    /// controller yet is admitted so the transport dials on demand (0.14
    /// parity: no recovery entry -> admit), which also keeps WebSocket-only
    /// destinations sendable.
    async fn preflight_send(
        &self,
        target: &ActrId,
        dest: &Dest,
        payload_type: PayloadType,
    ) -> ActorResult<SendAdmission> {
        let admission = self.current_send_admission()?;

        #[cfg(feature = "webrtc-v020")]
        {
            if self.v020_manager.is_closing(target).await {
                return Err(Self::send_projection_not_ready_error());
            }
            if self.v020_send_blocked(target, dest).await
                // Re-evaluate once before rejecting. The first pass awaits
                // both the controller snapshot and the transport lock, so a
                // readiness edge or a concurrent materialization can land in
                // that window; turning either into a rejection would be a
                // false negative.
                && self.v020_send_blocked(target, dest).await
                && (!matches!(
                    payload_type,
                    PayloadType::RpcReliable | PayloadType::StreamReliable
                ) || !self
                    .transport_manager
                    .has_ready_fallback_connection_for_payload(dest, payload_type)
                    .await)
            {
                return Err(Self::send_projection_not_ready_error());
            }
        }

        // Policy may have tightened during the snapshot await. Keep the
        // strictest projection seen by this operation.
        self.tighten_with_current_projection(admission)
    }

    /// Wait for a currently waitable route without creating a second
    /// readiness authority.  The caller already owns the operation deadline;
    /// this helper only wakes on authoritative projection, Controller, or
    /// destination ready-set changes and re-runs `preflight_send` after each
    /// wake.  Call/Tell/StreamReliable use it; latency-first and media paths
    /// retain their existing fail-fast behavior.
    async fn wait_until_route_ready(
        &self,
        target: &ActrId,
        dest: &Dest,
        payload_type: PayloadType,
        deadline: RpcDeadline,
        payload_bytes: usize,
    ) -> ActorResult<SendAdmission> {
        let waitable = matches!(
            payload_type,
            PayloadType::RpcReliable | PayloadType::StreamReliable
        );
        if !waitable {
            return self.preflight_send(target, dest, payload_type).await;
        }

        let waiter_class = WaiterClass::for_payload(payload_type)
            .expect("waitable route payloads must have a waiter class");
        let mut projection = self.send_projection_rx.clone();
        #[cfg(feature = "webrtc-v020")]
        let mut snapshots = self.v020_manager.subscribe_peer_snapshot_events();
        // Subscribe to the materialization revision BEFORE resolving the
        // destination ready-set: if the destination is absent or still
        // `Connecting` there is no ready-set to watch, and without this
        // revision subscription a later commit by another path would never
        // wake this waiter — it would expire at the outer deadline despite a
        // usable route.
        let mut materialization = self.transport_manager.watch_transports_revision();
        let mut ready = self.transport_manager.watch_ready_for(dest).await;
        // A destination that does not exist yet has no ready-set to watch, so
        // its eventual materialization (including a WebSocket fallback
        // committed by another path) must come through the manager's
        // transports-map revision — bumped inside every lifecycle transition
        // (Connecting insert, Connected commit, removal, teardown) —
        // otherwise the waiter below parks until the operation deadline.
        let mut dest_state = self.transport_manager.watch_transports_revision();
        let mut waiter = None;

        loop {
            match self.preflight_send(target, dest, payload_type).await {
                Ok(admission) => return Ok(admission),
                Err(error @ ActrError::ConnectionNotReady(_)) => {
                    #[cfg(feature = "webrtc-v020")]
                    if self.v020_manager.is_closing(target).await {
                        return Err(error);
                    }

                    let existing_only = *projection.borrow() == SendProjection::ExistingOnly;
                    if existing_only
                        && !self
                            .transport_manager
                            .has_outbound_materialization(dest)
                            .await
                    {
                        return Err(error);
                    }
                    if waiter.is_none() {
                        waiter = Some(
                            self.waiter_budget
                                .try_acquire(target, waiter_class, payload_bytes)
                                .ok_or_else(|| {
                                    ActrError::Unavailable(
                                        "outbound readiness waiter capacity exhausted".to_string(),
                                    )
                                })?,
                        );
                    }
                    deadline.ensure_remaining()?;

                    // A destination can be materialized while this waiter is
                    // asleep. Discover its ready-set before choosing the next
                    // wake source so a WebSocket fallback is not missed.
                    if ready.is_none() {
                        ready = self.transport_manager.watch_ready_for(dest).await;
                    }

                    let ready_changed = async {
                        match ready.as_mut() {
                            Some(watcher) => watcher.changed().await.map(|_| ()),
                            None => {
                                std::future::pending::<
                                    Result<(), tokio::sync::watch::error::RecvError>,
                                >()
                                .await
                            }
                        }
                    };

                    #[cfg(feature = "webrtc-v020")]
                    let snapshot_changed = async {
                        // Any snapshot revision may unblock the wait: the
                        // readiness projection is destination-wide, so there
                        // is no per-peer filtering to apply — both arms wake.
                        match snapshots.recv().await {
                            Ok(_) => Ok(()),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => Ok(()),
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => Err(()),
                        }
                    };
                    #[cfg(not(feature = "webrtc-v020"))]
                    let snapshot_changed = async { std::future::pending::<Result<(), ()>>().await };

                    tokio::select! {
                        biased;
                        changed = projection.changed() => {
                            if changed.is_err() {
                                return Err(error);
                            }
                        },
                        event = snapshot_changed => {
                            if event.is_err() {
                                return Err(error);
                            }
                        },
                        changed = dest_state.changed() => {
                            // Destination materialization (or fallback
                            // commit): the loop re-runs preflight and
                            // re-acquires the ready-set watch above.
                            if changed.is_err() {
                                return Err(error);
                            }
                        },
                        changed = ready_changed => {
                            if changed.is_err() {
                                ready = None;
                            }
                        },
                        // Destination materialization itself: a fresh
                        // destination has no ready-set to watch, so the map
                        // revision is the only wake source for a later
                        // `DestTransport` commit (e.g. another path dialing
                        // the WebSocket fallback). Cheap re-run of the loop.
                        changed = materialization.changed() => {
                            if changed.is_err() {
                                return Err(error);
                            }
                            if ready.is_none() {
                                ready = self.transport_manager.watch_ready_for(dest).await;
                            }
                        },
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Whether the v0.20 controller projection currently withholds business
    /// sends to `target`.
    #[cfg(feature = "webrtc-v020")]
    async fn v020_send_blocked(&self, target: &ActrId, dest: &Dest) -> bool {
        let Some(snapshot) = self.v020_manager.get_snapshot(target).await else {
            // No controller yet: the transport dials on demand (0.14 parity).
            return false;
        };
        if snapshot.sendable {
            return false;
        }
        // `Idle` is a terminal *attempt* state, not a permanent peer
        // tombstone. A later business send must be allowed to dial again
        // after an initial build/role-negotiation failure. The manager's
        // closing tombstone still fences explicit teardown, while Recovering
        // remains fail-fast as required by the RFC sendability contract.
        if snapshot.status == actr_framework::WebRtcPeerStatus::Idle {
            return false;
        }
        // The request that created the destination flight is allowed to
        // bootstrap an initially Idle peer. Concurrent requests can observe
        // the newly-created Controller as Connecting while the same flight is
        // still short of a ready wire — including after it has been committed
        // as a `DestTransport`, which happens before the wire can carry
        // traffic. They must join that bounded materialization rather than
        // fail merely because they lost the creation race. This is transport
        // admission, not a second RTC state authority: inbound-only
        // Connecting peers without this exact destination materialization are
        // still rejected.
        if snapshot.status == actr_framework::WebRtcPeerStatus::Connecting
            && self
                .transport_manager
                .has_outbound_materialization(dest)
                .await
        {
            return false;
        }
        true
    }
}

impl PeerGate {
    /// Send data via transport with per-PayloadType retry on transient failures.
    ///
    /// Retry is applied only when `NetworkError::kind()` is `Transient`.
    /// Non-transient errors are returned immediately without retry.
    /// Retry-policy send without a caller-correlatable operation id. Only
    /// in-crate tests drive opaque-byte sends; every production path threads a
    /// stable operation identity (request id or stream id).
    #[cfg(test)]
    async fn send_with_retry(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
    ) -> ActorResult<()> {
        let policy = payload_type.retry_policy();
        self.send_with_retry_policy(dest, payload_type, data, policy, admission, None)
            .await
    }

    async fn send_with_retry_record_identity_for_operation(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
        record: Option<&SentTransportSlot>,
        operation_id: &str,
    ) -> ActorResult<DestTransportRef> {
        let policy = payload_type.retry_policy();
        self.send_with_retry_policy_record_identity(
            dest,
            payload_type,
            data,
            policy,
            admission,
            record,
            Some(operation_id),
        )
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn send_with_retry_policy(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        policy: crate::transport::RetryPolicy,
        admission: SendAdmission,
        operation_id: Option<&str>,
    ) -> ActorResult<()> {
        self.send_with_retry_policy_record_identity(
            dest,
            payload_type,
            data,
            policy,
            admission,
            None,
            operation_id,
        )
        .await
        .map(|_| ())
    }

    async fn attempt_send(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
        record: Option<&SentTransportSlot>,
        progress_sink: Option<&DeliveryProgressCell>,
    ) -> GateSendAttempt {
        match admission {
            SendAdmission::Normal => {
                let PeerSendAttempt { result, progress } = self
                    .transport_manager
                    .send_with_progress(dest, payload_type, data, record, progress_sink)
                    .await;
                GateSendAttempt {
                    result: result.map(Some).map_err(ActrError::from),
                    progress,
                }
            }
            SendAdmission::ExistingOnly => {
                let attempt = self
                    .transport_manager
                    .send_existing_with_progress(dest, payload_type, data, record, progress_sink)
                    .await;
                GateSendAttempt {
                    result: attempt.result.map_err(ActrError::from),
                    progress: attempt.progress,
                }
            }
        }
    }

    fn delivery_uncertain_error(
        operation_id: Option<&str>,
        error: impl std::fmt::Display,
    ) -> ActrError {
        ActrError::DeliveryUncertain {
            operation_id: operation_id.unwrap_or("unknown").to_string(),
            cause: error.to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)] // Keeps send context explicit at this internal boundary.
    async fn send_once_with_admission(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
        record: Option<&SentTransportSlot>,
        operation_id: Option<&str>,
        progress_sink: Option<&DeliveryProgressCell>,
    ) -> ActorResult<DestTransportRef> {
        // Re-attempting a proven `NotAttempted` failure cannot duplicate
        // delivery (nothing entered a transport), so a single immediate
        // re-attempt is safe even for one-shot sends: it recovers pre-send
        // races (a lane invalidated between readiness and submission) without
        // weakening the at-most-once contract. Ambiguous (`Unknown`) attempts
        // still surface `DeliveryUncertain` with no replay. Worst case this
        // boundary submits a payload to a physical transport at most twice
        // per call (initial attempt + one safe reattempt); outer retry
        // policies multiply that bound by their own attempt counts.
        const MAX_SAFE_REATTEMPTS: u32 = 1;
        let mut reattempts = 0u32;
        loop {
            let admission = self.tighten_with_current_projection(admission)?;
            let attempt = self
                .attempt_send(dest, payload_type, data, admission, record, progress_sink)
                .await;
            match attempt.result {
                Ok(Some(identity)) => return Ok(identity),
                Ok(None) => return Err(Self::send_projection_not_ready_error()),
                Err(error) if attempt.progress == DeliveryProgress::Unknown => {
                    return Err(Self::delivery_uncertain_error(operation_id, error));
                }
                Err(error)
                    if attempt.progress == DeliveryProgress::NotAttempted
                        && error.is_retryable()
                        && reattempts < MAX_SAFE_REATTEMPTS =>
                {
                    reattempts += 1;
                    tracing::debug!(
                        "re-attempting proven pre-send failure once ({reattempts}/{MAX_SAFE_REATTEMPTS}): {error}"
                    );
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Retry only completed, proven `NotAttempted` failures. The retry loop
    /// intentionally has no progress sink: ambiguous (`Unknown`) results
    /// return before backoff, while timeout-sensitive one-way sends use the
    /// progress-aware `send_once_with_admission` path below. This keeps a
    /// retry from overwriting an earlier delivery fact with a later attempt.
    #[allow(clippy::too_many_arguments)] // Keeps retry context explicit at this internal boundary.
    async fn send_with_retry_policy_record_identity(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        policy: crate::transport::RetryPolicy,
        mut admission: SendAdmission,
        record: Option<&SentTransportSlot>,
        operation_id: Option<&str>,
    ) -> ActorResult<DestTransportRef> {
        let mut delay = policy.initial_delay;
        let mut attempt_count = 0u32;

        loop {
            admission = self.tighten_with_current_projection(admission)?;
            let send_attempt = self
                .attempt_send(dest, payload_type, data, admission, record, None)
                .await;
            match send_attempt.result {
                Ok(Some(identity)) => return Ok(identity),
                // `ExistingOnly` with no committed destination is an
                // admission result, not a transient transport failure. Fail
                // immediately without entering the retry backoff.
                Ok(None) => return Err(Self::send_projection_not_ready_error()),
                Err(e) if send_attempt.progress == DeliveryProgress::Unknown => {
                    return Err(Self::delivery_uncertain_error(operation_id, e));
                }
                Err(e) => {
                    attempt_count += 1;
                    let retryable = e.is_retryable();
                    let remaining = policy.max_attempts.saturating_sub(attempt_count);
                    if retryable && remaining > 0 {
                        tracing::warn!(
                            attempt = attempt_count,
                            remaining,
                            error = %e,
                            "transient send failure, retrying after {:?}",
                            delay,
                        );
                        crate::timer::sleep(crate::timer::ids::PEER_SEND_BACKOFF, delay).await;
                        // exponential backoff capped at max_delay
                        delay = (delay * 2).min(policy.max_delay);
                    } else {
                        if attempt_count > 1 {
                            tracing::warn!(
                                attempt = attempt_count,
                                error = %e,
                                retryable,
                                "send failed after {} attempt(s)",
                                attempt_count,
                            );
                        }
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn close_timed_out_request_transport(
        transport_manager: Arc<PeerTransport>,
        dest: Dest,
        target: ActrId,
        sent_transport: Option<DestTransportRef>,
    ) {
        let Some(sent_transport) = sent_transport else {
            tracing::warn!(
                peer = ?target,
                "Request timed out before a sent transport was recorded; closing transport"
            );
            if let Err(e) = transport_manager.close_transport(&dest).await {
                tracing::warn!(
                    peer = ?target,
                    "Failed to close transport after request timeout: {}",
                    e
                );
            }
            return;
        };

        match sent_transport.wire_identity.clone() {
            Some(WireIdentity::WebRtc {
                peer_id,
                session_id,
            }) => {
                tracing::warn!(
                    peer = ?target,
                    session_id,
                    "Request timed out after send; closing stale transport if WebRTC session still matches"
                );
                match transport_manager
                    .close_transport_if_webrtc_session(&dest, &peer_id, session_id)
                    .await
                {
                    Ok(true) => tracing::debug!(
                        peer = ?target,
                        session_id,
                        "Closed timed-out request transport for matching WebRTC session"
                    ),
                    Ok(false) => tracing::debug!(
                        peer = ?target,
                        session_id,
                        "Skipped timed-out request transport close because WebRTC session was replaced"
                    ),
                    Err(e) => tracing::warn!(
                        peer = ?target,
                        session_id,
                        "Failed to close stale transport after request timeout: {}",
                        e
                    ),
                }
            }
            Some(WireIdentity::WebSocket { .. }) | None => {
                tracing::warn!(
                    peer = ?target,
                    "Request timed out after send without wire identity; closing transport if instance still matches"
                );
                match transport_manager
                    .close_transport_if_current(&dest, &sent_transport)
                    .await
                {
                    Ok(true) => tracing::debug!(
                        peer = ?target,
                        "Closed timed-out request transport for matching instance"
                    ),
                    Ok(false) => tracing::debug!(
                        peer = ?target,
                        "Skipped timed-out request transport close because instance was replaced"
                    ),
                    Err(e) => tracing::warn!(
                        peer = ?target,
                        "Failed to close stale transport after request timeout: {}",
                        e
                    ),
                }
            }
        }
    }

    #[cfg(feature = "test-utils")]
    pub async fn send_serialized_with_zero_retry_delay_for_test(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        data: &[u8],
    ) -> ActorResult<()> {
        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest, payload_type).await?;

        let mut policy = payload_type.retry_policy();
        policy.initial_delay = std::time::Duration::ZERO;
        policy.max_delay = std::time::Duration::ZERO;
        self.send_with_retry_policy(&dest, payload_type, data, policy, admission, None)
            .await
    }

    /// Send request and wait for response (with specified PayloadType).
    ///
    /// This is primarily used by language bindings / non-generic RPC paths.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            skip_all,
            name = "PeerGate.send_request",
            fields(
                request_id = %envelope.request_id,
                payload_type = ?payload_type,
                target = %target
            )
        )
    )]
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn send_request_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<Bytes> {
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        self.send_request_with_type_until(target, payload_type, envelope, deadline)
            .await
    }

    /// Send a request under an absolute deadline created by the API entry
    /// point.  Preflight, pending registration, serialization, transport
    /// selection/retry, and response waiting all share this one budget.
    pub(crate) async fn send_request_with_type_until(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
        deadline: RpcDeadline,
    ) -> ActorResult<Bytes> {
        deadline.ensure_remaining()?;
        let envelope = Self::stamp_envelope_direction(envelope, Direction::Request);
        let dest = Self::actr_id_to_dest(target);
        let request_id = envelope.request_id.clone();
        let data = Self::serialize_envelope(&envelope);
        // Account the serialized envelope (metadata, correlation fields and
        // framing included), consistent with how stream chunks account their
        // serialized bytes — otherwise retained memory can exceed the
        // configured waiter byte ceilings.
        let payload_bytes = data.len();
        let sent_transport = Arc::new(Mutex::new(None));
        let sent_transport_for_send = Arc::clone(&sent_transport);
        let mut send_started = false;

        let result = crate::timer::timeout_at(
            crate::timer::ids::PEER_REQUEST_DEADLINE,
            deadline.instant(),
            async {
                let admission = self
                    .wait_until_route_ready(target, &dest, payload_type, deadline, payload_bytes)
                    .await?;
                // No proportional admission floor here: a floor derived from
                // the total budget would reject usable residual budgets solely
                // because route readiness consumed more than half of it (a
                // 100 ms call whose route clears at 55 ms would refuse with
                // 45 ms still usable, and retrying with a bigger budget
                // changes the caller's timing semantics). The remaining
                // budget is still bounded by `ensure_remaining` at
                // registration and by the request deadline itself: a request
                // committed with a thin slice simply races its own deadline,
                // and the timeout arm tears down exactly the transport it was
                // submitted to.

                // The RAII guard removes pending state on send failure,
                // response-channel closure, timeout, or caller cancellation.
                let pending = self
                    .pending_requests
                    .register(request_id.clone(), target.clone());
                deadline.ensure_remaining()?;
                send_started = true;

                self.send_with_retry_record_identity_for_operation(
                    &dest,
                    payload_type,
                    &data,
                    admission,
                    Some(&sent_transport_for_send),
                    &request_id,
                )
                .await?;
                tracing::debug!("Sent request to {:?}", target);

                // The outer timeout owns the combined send + response budget.
                pending.recv().await
            },
        )
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                // Deadline expired — return TimedOut (retryable) rather than
                // Unavailable so callers and language bindings can distinguish
                // "request never reached the server" from "server did not reply
                // in time".
                //
                // Deliberate asymmetry vs Tell/DataChunk: a request that
                // timed out after submission stays retryable because this
                // arm force-closes the transport it was submitted to
                // (`close_timed_out_request_transport`) — a caller retry
                // cannot double-deliver on the stale wire, and the pending
                // response correlation is reclaimed rather than left
                // ambiguous. One-way Tell/DataChunk sends have no such
                // correlation handle to reclaim and no dedup on the far
                // side, so those surfaces are terminal `DeliveryUncertain`
                // after an ambiguous submission instead.
                if send_started {
                    let transport_manager = self.transport_manager.clone();
                    let stale_dest = dest.clone();
                    let stale_target = target.clone();
                    let sent_transport = sent_transport.lock().await.clone();
                    tokio::spawn(async move {
                        Self::close_timed_out_request_transport(
                            transport_manager,
                            stale_dest,
                            stale_target,
                            sent_transport,
                        )
                        .await;
                    });
                }
                Err(ActrError::TimedOut)
            }
        }
    }

    /// Send request and wait for response (bidirectional communication)
    #[cfg(feature = "test-utils")]
    pub async fn send_request(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<Bytes> {
        self.send_request_with_type(target, PayloadType::RpcReliable, envelope)
            .await
    }

    /// Send one-way message (no response expected)
    #[cfg(feature = "test-utils")]
    pub async fn send_message(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<()> {
        self.send_message_with_type(target, PayloadType::RpcReliable, envelope)
            .await
    }

    /// Send an RPC response without rejecting it during the WebRTC recovery window.
    ///
    /// A response is correlated to an already-pending request at the caller. If the
    /// existing transport cannot accept it, the caller's RPC deadline remains the
    /// recovery boundary and the caller may issue a fresh request. This path still
    /// uses the reliable retry policy but never starts a new destination
    /// negotiation: it is already-committed work and therefore uses the
    /// `ExistingOnly` transport path while bypassing request preflight.
    #[cfg(feature = "test-utils")]
    pub async fn send_response(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<()> {
        let envelope = Self::stamp_envelope_direction(envelope, Direction::Response);
        let data = Self::serialize_envelope(&envelope);
        let dest = Self::actr_id_to_dest(target);
        self.send_with_retry_record_identity_for_operation(
            &dest,
            PayloadType::RpcReliable,
            &data,
            SendAdmission::ExistingOnly,
            None,
            &envelope.request_id,
        )
        .await
        .map(|_| ())
    }

    /// Send one-way message with specified PayloadType.
    ///
    /// One-way traffic is stamped `Direction::Tell`: the receiver dispatches
    /// the envelope but MUST NOT reply, and no pending entry is registered
    /// on this side.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            skip_all,
            name = "PeerGate.send_message",
            fields(
                payload_type = ?payload_type,
                target = %target
            )
        )
    )]
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn send_message_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<()> {
        self.send_message_with_type_inner(target, payload_type, envelope, None)
            .await
    }

    /// Send a Tell under the absolute budget created by the runtime context.
    /// The route may reselect a lane before submission, but the payload itself
    /// is submitted at most once for this logical operation.
    pub(crate) async fn send_tell_with_type_until(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
        deadline: RpcDeadline,
    ) -> ActorResult<()> {
        deadline.ensure_remaining()?;
        let envelope = Self::stamp_envelope_direction(envelope, Direction::Tell);
        let dest = Self::actr_id_to_dest(target);
        // An empty request id carries no correlation value; report the
        // documented placeholder instead of an empty operation id.
        let operation_id = if envelope.request_id.is_empty() {
            None
        } else {
            Some(envelope.request_id.clone())
        };
        let data = Self::serialize_envelope(&envelope);
        // Same serialized-bytes accounting as the request path.
        let payload_bytes = data.len();

        // Same cancellation-classification contract as the data-chunk path:
        // a deadline that fires mid-write must surface terminal
        // `DeliveryUncertain`, not a retryable `TimedOut`.
        let progress_cell = new_delivery_progress_cell();
        match crate::timer::timeout_at(
            crate::timer::ids::OUTBOUND_TELL_DEADLINE,
            deadline.instant(),
            async {
                let admission = self
                    .wait_until_route_ready(target, &dest, payload_type, deadline, payload_bytes)
                    .await?;
                deadline.ensure_remaining()?;
                // One-shot admission floor: committing bytes with less than
                // the minimal slice left makes a mid-write expiry (terminal
                // `DeliveryUncertain`) likelier than a safe pre-send refusal.
                deadline.ensure_min_remaining(MIN_ONE_SHOT_SEND_SLICE)?;
                self.send_once_with_admission(
                    &dest,
                    payload_type,
                    &data,
                    admission,
                    None,
                    operation_id.as_deref(),
                    Some(&progress_cell),
                )
                .await
                .map(|_| ())
            },
        )
        .await
        {
            Ok(result) => result,
            Err(_) => match observed_delivery_progress(&progress_cell) {
                None | Some(DeliveryProgress::NotAttempted) => Err(ActrError::TimedOut),
                Some(DeliveryProgress::SubmittedLocally) | Some(DeliveryProgress::Unknown) => {
                    Err(Self::delivery_uncertain_error(
                        operation_id.as_deref(),
                        "tell send deadline elapsed after transport submission",
                    ))
                }
            },
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn send_message_with_type_inner(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
        send_timeout: Option<Duration>,
    ) -> ActorResult<()> {
        // Explicit caller deadlines use the one-shot, progress-aware Tell
        // path so cancellation after local submission is terminal
        // `DeliveryUncertain`, never a retryable `TimedOut`. The no-deadline
        // compatibility entry point retains the historical per-payload retry
        // policy used by the low-level RpcSignal tests and callers.
        if let Some(send_timeout) = send_timeout {
            let deadline = RpcDeadline::from_duration(send_timeout);
            return self
                .send_tell_with_type_until(target, payload_type, envelope, deadline)
                .await;
        }

        let envelope = Self::stamp_envelope_direction(envelope, Direction::Tell);
        let data = Self::serialize_envelope(&envelope);
        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest, payload_type).await?;
        self.send_with_retry_record_identity_for_operation(
            &dest,
            payload_type,
            &data,
            admission,
            None,
            &envelope.request_id,
        )
        .await
        .map(|_| ())
    }

    /// S6-10: v0.20 `send_media_sample` — the peer's outbound track writer
    /// (data plane). Admission mirrors the 0.14 path (ExistingOnly allowed:
    /// sending uses an already-registered track).
    #[cfg(feature = "webrtc-v020")]
    async fn send_media_sample_v020(
        &self,
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        target: &ActrId,
        track_id: &str,
        sample: MediaSample,
    ) -> ActorResult<()> {
        let dest = Self::actr_id_to_dest(target);
        let admission = self
            .preflight_send(target, &dest, PayloadType::MediaRtp)
            .await?;
        self.tighten_with_current_projection(admission)?;

        let Some(handle) = manager.get_controller(target).await else {
            return Err(Self::send_projection_not_ready_error());
        };
        handle
            .send_media_sample(track_id, &sample)
            .await
            .map_err(|e| ActrError::Unavailable(format!("WebRTC send failed: {e}")))
    }

    /// S6-10: v0.20 `add_media_track` — ensure the peer connection (a track
    /// add may establish it, 0.14 parity), then register the track on the
    /// active session. Renegotiation rides the controller's effect path.
    #[cfg(feature = "webrtc-v020")]
    async fn add_media_track_v020(
        &self,
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        target: &ActrId,
        track_id: &str,
        codec: &str,
        media_type: &str,
    ) -> ActorResult<()> {
        let dest = Self::actr_id_to_dest(target);
        let admission = self
            .preflight_send(target, &dest, PayloadType::MediaRtp)
            .await?;
        if self.tighten_with_current_projection(admission)? == SendAdmission::ExistingOnly {
            return Err(Self::send_projection_not_ready_error());
        }

        let handle = manager.ensure_controller(target.clone()).await;
        handle
            .connect()
            .await
            .map_err(|_| Self::send_projection_not_ready_error())?;

        // The track needs an active session; wait (bounded) for the peer to
        // reach sendable, watching the snapshot rather than polling.
        let mut rx = handle.subscribe();
        let ready = crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(10),
            async move {
                loop {
                    if rx.borrow().sendable {
                        return;
                    }
                    if rx.changed().await.is_err() {
                        return;
                    }
                }
            },
        )
        .await;
        if ready.is_err() || !handle.snapshot().sendable {
            return Err(Self::send_projection_not_ready_error());
        }

        handle
            .add_media_track(track_id, codec, media_type)
            .await
            .map_err(|e| ActrError::Unavailable(format!("WebRTC add track failed: {e}")))
    }

    /// S6-10: v0.20 `remove_media_track` — unregister the writer and remove
    /// the track from the active session (renegotiates).
    #[cfg(feature = "webrtc-v020")]
    async fn remove_media_track_v020(
        &self,
        manager: &Arc<crate::wire::webrtc::v020::manager::RtcConnectionManager>,
        target: &ActrId,
        track_id: &str,
    ) -> ActorResult<()> {
        let dest = Self::actr_id_to_dest(target);
        let admission = self
            .preflight_send(target, &dest, PayloadType::MediaRtp)
            .await?;
        if self.tighten_with_current_projection(admission)? == SendAdmission::ExistingOnly {
            return Err(Self::send_projection_not_ready_error());
        }

        let Some(handle) = manager.get_controller(target).await else {
            return Err(ActrError::Unavailable(format!(
                "track not found: {track_id} (no connection to peer)"
            )));
        };
        handle
            .remove_media_track(track_id)
            .await
            .map_err(|e| ActrError::Unavailable(format!("WebRTC remove track failed: {e}")))
    }

    /// Send a media sample via the peer's v0.20 outbound track writer (data
    /// plane). Admission mirrors 0.14 (ExistingOnly allowed: sending uses an
    /// already-registered track).
    #[cfg(feature = "webrtc-v020")]
    pub async fn send_media_sample(
        &self,
        target: &ActrId,
        track_id: &str,
        sample: MediaSample,
    ) -> ActorResult<()> {
        let manager = self.v020_manager.clone();
        self.send_media_sample_v020(&manager, target, track_id, sample)
            .await
    }

    /// Add a media track on the v0.20 peer connection (the controller may
    /// establish the connection first) and renegotiate. Renegotiation is
    /// offerer-only (RFC-0504 §"Deliberate regressions").
    #[cfg(feature = "webrtc-v020")]
    pub async fn add_media_track(
        &self,
        target: &ActrId,
        track_id: &str,
        codec: &str,
        media_type: &str,
    ) -> ActorResult<()> {
        let manager = self.v020_manager.clone();
        self.add_media_track_v020(&manager, target, track_id, codec, media_type)
            .await
    }

    /// Remove a media track from the v0.20 peer connection (renegotiates).
    #[cfg(feature = "webrtc-v020")]
    pub async fn remove_media_track(&self, target: &ActrId, track_id: &str) -> ActorResult<()> {
        let manager = self.v020_manager.clone();
        self.remove_media_track_v020(&manager, target, track_id)
            .await
    }

    /// Send DataChunk (Fast Path)
    ///
    /// # Parameters
    /// - `target`: Target Actor ID
    /// - `payload_type`: PayloadType (StreamReliable or StreamLatencyFirst)
    /// - `stream_id`: DataChunk identifier already known before serialization
    /// - `data`: Serialized DataChunk bytes
    /// - `operation_id`: caller-derived identity for this logical send
    ///   (`stream_id#sequence` from the still-decoded `DataChunk`), threaded
    ///   through unchanged so one send can never surface under two
    ///   reconciliation keys
    ///
    /// # Implementation Note
    /// Sends via PeerTransport using WebRTC DataChannel or WebSocket
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn send_data_chunk(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        stream_id: &str,
        data: Bytes,
        operation_id: &str,
    ) -> ActorResult<()> {
        self.send_data_chunk_with_timeout(
            target,
            payload_type,
            stream_id,
            data,
            DATA_CHUNK_SEND_TIMEOUT,
            operation_id,
        )
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn send_data_chunk_with_timeout(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        stream_id: &str,
        data: Bytes,
        send_timeout: Duration,
        operation_id: &str,
    ) -> ActorResult<()> {
        let deadline = RpcDeadline::from_duration(send_timeout);
        self.send_data_chunk_until(
            target,
            payload_type,
            stream_id,
            data,
            deadline,
            operation_id,
        )
        .await
    }

    pub(crate) async fn send_data_chunk_until(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        stream_id: &str,
        data: Bytes,
        deadline: RpcDeadline,
        operation_id: &str,
    ) -> ActorResult<()> {
        tracing::debug!(
            "PeerGate::send_data_chunk to {:?}, stream_id={}, payload_type={:?}, size={} bytes",
            target,
            stream_id,
            payload_type,
            data.len()
        );

        ensure_stream_payload_type(payload_type)?;

        // Convert ActrId to Dest
        let dest = Self::actr_id_to_dest(target);

        // Delivery uncertainty on a session close mid-flight is surfaced by
        // the v0.20 lane send itself (RtcSendError::DataChunkDeliveryUncertain
        // mapped at the transport boundary); no separate tracker lives here.
        //
        // Reclaim semantics: unlike the retired 0.14 path (which recorded the
        // chosen transport in a sent-slot and force-closed it on deadline
        // expiry), the v0.20 architecture makes the Controller's recovery the
        // canonical reclaim mechanism — the recovery deadline, ICE/DTLS state
        // monitoring, and terminal transport cleanup retire an unhealthy
        // session. A chunk-send timeout therefore only fails this send; it
        // deliberately does not tear down the transport (a timeout is not
        // proof the transport is bad — the remote may simply be slow).
        //
        // The progress cell survives cancellation of the send future: the
        // transport records the delivery fact the moment it exists, so the
        // timeout arm below can tell "cancelled before any transport write"
        // (retryable `TimedOut`) apart from "cancelled with bytes possibly in
        // flight" (terminal `DeliveryUncertain` — the data-chunk path has no
        // dedup, so an automatic replay could deliver the payload twice).
        let progress_cell = new_delivery_progress_cell();
        match crate::timer::timeout_at(
            crate::timer::ids::OUTBOUND_DATA_CHUNK_DEADLINE,
            deadline.instant(),
            async {
                let admission = self
                    .wait_until_route_ready(target, &dest, payload_type, deadline, data.len())
                    .await?;
                deadline.ensure_remaining()?;
                // One-shot admission floor: committing bytes with less than
                // the minimal slice left makes a mid-write expiry (terminal
                // `DeliveryUncertain`) likelier than a safe pre-send refusal.
                deadline.ensure_min_remaining(MIN_ONE_SHOT_SEND_SLICE)?;
                self.send_once_with_admission(
                    &dest,
                    payload_type,
                    &data,
                    admission,
                    None,
                    Some(operation_id),
                    Some(&progress_cell),
                )
                .await
            },
        )
        .await
        {
            Ok(Ok(_identity)) => Ok(()),
            Ok(Err(e)) => {
                if let ActrError::DeliveryUncertain { cause, .. } = &e {
                    self.emit_data_chunk_uncertain_hook(operation_id, cause);
                }
                Err(e)
            }
            Err(_) => match observed_delivery_progress(&progress_cell) {
                None | Some(DeliveryProgress::NotAttempted) => Err(ActrError::TimedOut),
                Some(DeliveryProgress::SubmittedLocally) | Some(DeliveryProgress::Unknown) => {
                    let cause = "data chunk send deadline elapsed after transport submission";
                    self.emit_data_chunk_uncertain_hook(operation_id, cause);
                    Err(Self::delivery_uncertain_error(Some(operation_id), cause))
                }
            },
        }
    }
}

impl Drop for PeerGate {
    fn drop(&mut self) {
        tracing::debug!("PeerGate dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::timeout;
    use crate::transport::{
        ConnType, DataLane, NetworkError, NetworkResult, WireBuilder, WireHandle,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_actor_id(serial_number: u64) -> ActrId {
        ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number,
            r#type: actr_protocol::ActrType {
                manufacturer: "acme".to_string(),
                name: "peer-gate-test".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    fn envelope_with_direction(direction: Option<i32>) -> RpcEnvelope {
        RpcEnvelope {
            request_id: "req-direction".to_string(),
            route_key: "pkg.Service.Method".to_string(),
            direction,
            ..Default::default()
        }
    }

    #[test]
    fn data_chunk_operation_id_distinguishes_sequences() {
        assert_ne!(
            data_chunk_operation_id("stream-1", 4),
            data_chunk_operation_id("stream-1", 5)
        );
        assert_eq!(data_chunk_operation_id("stream-1", 4), "stream-1#4");
    }

    #[test]
    fn data_stream_operation_id_never_collides_with_chunk_keys() {
        // Stream ids are unconstrained protobuf strings and may contain `#`.
        // The stream-scoped key must therefore stay disjoint from every
        // chunk-scoped key by construction, not by convention.
        assert_eq!(data_stream_operation_id("s"), "s#stream");
        assert_ne!(
            data_stream_operation_id("s#4"),
            data_chunk_operation_id("s", 4),
            "a stream literally named `s#4` must not collide with stream `s` sequence 4"
        );
        // Exhaustive over the digit-tail rule: a chunk key always ends in
        // `#` + decimal digits, a stream key always ends in `#stream`.
        for (stream, sequence) in [("s", 4u64), ("s#4", 5), ("a#b#c", 123)] {
            let chunk_key = data_chunk_operation_id(stream, sequence);
            assert!(
                chunk_key.rsplit_once('#').is_some_and(|(_, seq)| {
                    !seq.is_empty() && seq.bytes().all(|b| b.is_ascii_digit())
                }),
                "chunk keys end in #digits: {chunk_key}"
            );
            assert_ne!(
                chunk_key,
                data_stream_operation_id(stream),
                "scopes must stay disjoint for stream {stream:?}"
            );
        }
    }

    #[test]
    fn stamp_envelope_direction_overwrites_missing_and_mismatched_values() {
        let request =
            PeerGate::stamp_envelope_direction(envelope_with_direction(None), Direction::Request);
        assert_eq!(request.direction, Some(Direction::Request as i32));

        let response = PeerGate::stamp_envelope_direction(
            envelope_with_direction(Some(Direction::Request as i32)),
            Direction::Response,
        );
        assert_eq!(response.direction, Some(Direction::Response as i32));

        // One-way message path: stamps Tell even over a stale Request label.
        let tell = PeerGate::stamp_envelope_direction(
            envelope_with_direction(Some(Direction::Request as i32)),
            Direction::Tell,
        );
        assert_eq!(tell.direction, Some(Direction::Tell as i32));
    }

    #[derive(Debug)]
    struct RetryFailingLane {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl DataLane for RetryFailingLane {
        async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(NetworkError::ConnectionError(
                "send still unavailable".into(),
            ))
        }

        fn lane_type(&self) -> &'static str {
            "retry-failing"
        }
    }

    #[derive(Debug)]
    struct ReadyWebSocketWire {
        lane: Arc<RetryFailingLane>,
    }

    #[async_trait]
    impl WireHandle for ReadyWebSocketWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebSocket
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn close(&self) -> NetworkResult<()> {
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            Ok(self.lane.clone())
        }
    }

    struct RetryFailingWireBuilder {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WireBuilder for RetryFailingWireBuilder {
        async fn create_connections(
            &self,
            _dest: &Dest,
        ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            Ok(vec![Arc::new(ReadyWebSocketWire {
                lane: Arc::new(RetryFailingLane {
                    attempts: self.attempts.clone(),
                }),
            })])
        }
    }

    #[derive(Debug)]
    struct SuccessfulLane {
        sends: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl DataLane for SuccessfulLane {
        async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn lane_type(&self) -> &'static str {
            "successful"
        }
    }

    #[derive(Debug)]
    struct ReadySuccessfulWire {
        sends: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WireHandle for ReadySuccessfulWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebSocket
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn close(&self) -> NetworkResult<()> {
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            Ok(Arc::new(SuccessfulLane {
                sends: self.sends.clone(),
            }))
        }
    }

    struct CountingSuccessfulWireBuilder {
        builds: Arc<AtomicUsize>,
        sends: Arc<AtomicUsize>,
    }

    #[derive(Debug)]
    struct LifecycleWebRtcWire {
        lifecycle: crate::transport::WebRtcTransportLifecycle,
        close_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WireHandle for LifecycleWebRtcWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebRTC
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn close(&self) -> NetworkResult<()> {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            Ok(Arc::new(SuccessfulLane {
                sends: Arc::new(AtomicUsize::new(0)),
            }))
        }

        fn webrtc_transport_lifecycle(&self) -> Option<crate::transport::WebRtcTransportLifecycle> {
            Some(self.lifecycle.clone())
        }
    }

    struct LifecycleWireBuilder {
        wire: Arc<LifecycleWebRtcWire>,
        builds: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WireBuilder for LifecycleWireBuilder {
        async fn create_connections(
            &self,
            _dest: &Dest,
        ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.wire.clone()])
        }
    }

    #[async_trait]
    impl WireBuilder for CountingSuccessfulWireBuilder {
        async fn create_connections(
            &self,
            _dest: &Dest,
        ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Arc::new(ReadySuccessfulWire {
                sends: self.sends.clone(),
            })])
        }
    }

    fn gate_with_projection(
        projection: SendProjection,
    ) -> (
        PeerGate,
        Arc<PeerTransport>,
        watch::Sender<SendProjection>,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
    ) {
        let builds = Arc::new(AtomicUsize::new(0));
        let sends = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(CountingSuccessfulWireBuilder {
                builds: builds.clone(),
                sends: sends.clone(),
            }),
        ));
        let pending_requests: PendingRequestsMap = Arc::new(RpcCorrelation::new());
        let (projection_tx, projection_rx) = watch::channel(projection);
        let gate = PeerGate::with_v020_manager(
            transport.clone(),
            PeerGate::empty_manager_for_test(),
            pending_requests,
            Some(projection_rx),
        );
        (gate, transport, projection_tx, builds, sends)
    }

    #[tokio::test]
    async fn blocked_projection_times_out_before_pending_registration_or_build() {
        let (gate, _transport, _projection_tx, builds, _sends) =
            gate_with_projection(SendProjection::Blocked);
        let target = test_actor_id(2);
        let envelope = RpcEnvelope {
            request_id: "blocked-request".to_string(),
            timeout_ms: 1_000,
            ..Default::default()
        };

        let error = gate
            .send_request_with_type(&target, PayloadType::RpcReliable, envelope)
            .await
            .expect_err("blocked projection must time out before admitting work");

        assert!(matches!(error, ActrError::TimedOut));
        assert!(gate.get_pending_requests().len() == 0);
        assert_eq!(builds.load(Ordering::SeqCst), 0);
    }

    #[cfg(feature = "webrtc-v020")]
    #[tokio::test(start_paused = true)]
    async fn request_deadline_covers_preflight_wait() {
        let (gate, transport, _projection_tx, builds, sends) =
            gate_with_projection(SendProjection::Normal);
        let gate = Arc::new(gate);
        let target = test_actor_id(2);

        // Keep a healthy committed transport in the map. Expiring before the
        // send phase must not run the transport-reclaim path against it.
        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("preparing a healthy transport should succeed");
        assert_eq!(transport.dest_count().await, 1);

        // Hold the manager peers-map write lock so the preflight `is_closing`
        // read blocks. A caller deadline must bound this wait even though no
        // pending request or transport send has started yet.
        let preflight_guard = gate.v020_manager.hold_peers_write_for_test().await;
        let request_gate = Arc::clone(&gate);
        let request = tokio::spawn(async move {
            request_gate
                .send_request_with_type(
                    &target,
                    PayloadType::RpcReliable,
                    RpcEnvelope {
                        request_id: "preflight-deadline".to_string(),
                        timeout_ms: 100,
                        ..Default::default()
                    },
                )
                .await
        });

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert!(
            request.is_finished(),
            "preflight lock wait must be covered by the request deadline"
        );

        let error = request
            .await
            .expect("request task should not panic")
            .expect_err("blocked preflight should time out");
        assert!(matches!(error, ActrError::TimedOut), "got {error:?}");
        assert!(gate.get_pending_requests().len() == 0);
        tokio::task::yield_now().await;
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            transport.dest_count().await,
            1,
            "pre-send timeout must not close a healthy transport"
        );

        drop(preflight_guard);
    }

    #[tokio::test]
    async fn existing_only_without_transport_fails_without_build_or_retry() {
        let (gate, _transport, _projection_tx, builds, sends) =
            gate_with_projection(SendProjection::ExistingOnly);

        let error = gate
            .send_message_with_type(
                &test_actor_id(2),
                PayloadType::RpcReliable,
                RpcEnvelope::default(),
            )
            .await
            .expect_err("existing-only admission requires a reusable physical connection");

        assert!(matches!(error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);

        let chunk_error = gate
            .send_data_chunk_with_timeout(
                &test_actor_id(2),
                PayloadType::StreamReliable,
                "existing-only-stream",
                Bytes::from_static(b"chunk"),
                Duration::from_secs(1),
                "existing-only-stream",
            )
            .await
            .expect_err("data-chunk send must use the same no-create admission");
        assert!(matches!(chunk_error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    fn test_media_sample() -> MediaSample {
        MediaSample {
            data: Bytes::from_static(b"frame"),
            timestamp: 0,
            codec: "OPUS".to_string(),
            media_type: actr_framework::MediaType::Audio,
        }
    }

    #[tokio::test]
    async fn blocked_projection_rejects_media_operations_before_coordinator_access() {
        let (gate, _transport, _projection_tx, builds, sends) =
            gate_with_projection(SendProjection::Blocked);
        let target = test_actor_id(2);

        let sample_error = gate
            .send_media_sample(&target, "audio", test_media_sample())
            .await
            .expect_err("blocked projection must reject media samples");
        assert!(matches!(sample_error, ActrError::ConnectionNotReady(_)));

        let track_error = gate
            .add_media_track(&target, "audio", "OPUS", "audio")
            .await
            .expect_err("blocked projection must reject media track negotiation");
        assert!(matches!(track_error, ActrError::ConnectionNotReady(_)));

        let remove_error = gate
            .remove_media_track(&target, "audio")
            .await
            .expect_err("blocked projection must reject media track renegotiation");
        assert!(matches!(remove_error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    /// ExistingOnly admission: media work on a peer with no live session
    /// fails fast with ConnectionNotReady (samples pass the projection check
    /// but find no controller; topology changes are rejected at the tighten
    /// step) and nothing reaches the transport.
    #[tokio::test]
    async fn existing_only_media_ops_fail_fast_without_a_session() {
        let (gate, _transport, _projection_tx, builds, sends) =
            gate_with_projection(SendProjection::ExistingOnly);
        let target = test_actor_id(2);

        let sample_error = gate
            .send_media_sample(&target, "audio", test_media_sample())
            .await
            .expect_err("a sample with no live session must fail fast");
        assert!(matches!(sample_error, ActrError::ConnectionNotReady(_)));

        let track_error = gate
            .add_media_track(&target, "audio", "OPUS", "audio")
            .await
            .expect_err("existing-only projection must not start track negotiation");
        assert!(matches!(track_error, ActrError::ConnectionNotReady(_)));

        let remove_error = gate
            .remove_media_track(&target, "audio")
            .await
            .expect_err("existing-only projection must not start track renegotiation");
        assert!(matches!(remove_error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn normal_projection_builds_but_existing_only_reuses_the_commit() {
        let (gate, _transport, projection_tx, builds, sends) =
            gate_with_projection(SendProjection::Normal);
        let target = test_actor_id(2);

        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("normal projection should establish and send");
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(sends.load(Ordering::SeqCst), 1);

        projection_tx.send_replace(SendProjection::ExistingOnly);
        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("existing-only projection should use the committed transport");

        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(sends.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn recovery_exhaustion_event_retires_cached_transport_and_pending_rpc() {
        let target = test_actor_id(2);
        let lifecycle = crate::transport::WebRtcTransportLifecycle {
            peer_id: target.clone(),
            controller_lifecycle_id: 11,
            transport_epoch: 4,
        };
        let close_count = Arc::new(AtomicUsize::new(0));
        let builds = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(LifecycleWireBuilder {
                wire: Arc::new(LifecycleWebRtcWire {
                    lifecycle: lifecycle.clone(),
                    close_count: close_count.clone(),
                }),
                builds: builds.clone(),
            }),
        ));
        let manager = PeerGate::empty_manager_for_test();
        let pending_requests: PendingRequestsMap = Arc::new(RpcCorrelation::new());
        let gate =
            PeerGate::with_v020_manager(transport.clone(), manager.clone(), pending_requests, None);

        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("seed transport send");
        let dest = Dest::peer(target.clone());
        assert!(transport.has_dest(&dest).await);
        let pending = gate
            .register_pending_for_test("terminal-pending", target.clone())
            .await;

        manager.publish_peer_transport_retired_for_test(
            crate::wire::webrtc::v020::status::PeerTransportRetirement {
                lifecycle: lifecycle.clone(),
                reason: actr_framework::WebRtcUnavailableReason::RecoveryExhausted,
            },
        );

        let pending_result = timeout(
            crate::timer::ids::PEER_REQUEST_DEADLINE,
            Duration::from_secs(1),
            pending,
        )
        .await
        .expect("terminal cleanup must wake pending RPC")
        .expect("pending sender must resolve");
        assert!(matches!(
            pending_result,
            Err(ActrError::ConnectionNotReady(_))
        ));
        timeout(
            crate::timer::ids::PEER_TRANSPORT_HEALTH,
            Duration::from_secs(1),
            async {
                while transport.has_dest(&dest).await || transport.is_closing(&dest).await {
                    tokio::task::yield_now().await;
                }
            },
        )
        .await
        .expect("terminal cleanup must evict the cached destination");
        assert_eq!(
            close_count.load(Ordering::SeqCst),
            0,
            "terminal event cleanup must not recursively close its Controller wire"
        );

        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("next send must build a fresh destination flight");
        assert_eq!(builds.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn policy_tightening_between_preflight_and_send_prevents_build() {
        let (gate, _transport, projection_tx, builds, sends) =
            gate_with_projection(SendProjection::Normal);
        let target = test_actor_id(2);
        let dest = PeerGate::actr_id_to_dest(&target);
        let admission = gate
            .preflight_send(&target, &dest, PayloadType::RpcReliable)
            .await
            .expect("normal preflight should admit");

        projection_tx.send_replace(SendProjection::ExistingOnly);
        let error = gate
            .send_with_retry(&dest, PayloadType::RpcReliable, b"payload", admission)
            .await
            .expect_err("send must re-read a tighter projection");

        assert!(matches!(error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn admitted_existing_only_operation_cannot_upgrade_after_policy_relaxes() {
        let (gate, _transport, projection_tx, builds, sends) =
            gate_with_projection(SendProjection::ExistingOnly);
        let target = test_actor_id(2);
        let dest = PeerGate::actr_id_to_dest(&target);
        let admission = gate
            .preflight_send(&target, &dest, PayloadType::RpcReliable)
            .await
            .expect("existing-only preflight should admit committed-work attempts");

        projection_tx.send_replace(SendProjection::Normal);
        let error = gate
            .send_with_retry(&dest, PayloadType::RpcReliable, b"payload", admission)
            .await
            .expect_err("an admitted operation must retain its strictest projection");

        assert!(matches!(error, ActrError::ConnectionNotReady(_)));
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn request_admits_usable_residual_budget_after_route_wait() {
        let (gate, _transport, projection_tx, _builds, sends) =
            gate_with_projection(SendProjection::Blocked);
        let target = test_actor_id(2);

        // The projection clears only after most of the 200ms budget is gone.
        // There is deliberately no proportional admission floor for requests:
        // the ~20ms residual budget is usable, the request is committed, and
        // the deadline arm tears down exactly the transport it was submitted
        // to when the response does not arrive in time.
        tokio::spawn(async move {
            crate::timer::sleep(
                crate::timer::ids::PEER_TEST_EVENT_DELAY,
                Duration::from_millis(180),
            )
            .await;
            projection_tx.send_replace(SendProjection::Normal);
        });

        let envelope = RpcEnvelope {
            request_id: "admission-floor".to_string(),
            timeout_ms: 200,
            ..Default::default()
        };
        let error = gate
            .send_request_with_type_until(
                &target,
                PayloadType::RpcReliable,
                envelope,
                RpcDeadline::from_duration(Duration::from_millis(200)),
            )
            .await
            .expect_err("the response wait still races the residual budget");

        assert!(matches!(error, ActrError::TimedOut), "got {error:?}");
        assert_eq!(
            sends.load(Ordering::SeqCst),
            1,
            "the usable residual budget must commit the request — refusing it would reject usable budgets solely because readiness consumed half of the total"
        );
        assert_eq!(
            gate.pending_count().await,
            0,
            "no pending entry may be registered below the admission floor"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reliable_route_wait_rechecks_projection_after_wake() {
        let (gate, _transport, projection_tx, _builds, _sends) =
            gate_with_projection(SendProjection::Blocked);
        let gate = Arc::new(gate);
        let target = test_actor_id(2);
        let dest = PeerGate::actr_id_to_dest(&target);
        let waiter = {
            let gate = Arc::clone(&gate);
            let deadline = RpcDeadline::from_duration(Duration::from_secs(1));
            tokio::spawn(async move {
                gate.wait_until_route_ready(&target, &dest, PayloadType::RpcReliable, deadline, 0)
                    .await
            })
        };

        tokio::task::yield_now().await;
        projection_tx.send_replace(SendProjection::Normal);
        let admission = waiter
            .await
            .expect("route waiter must not panic")
            .expect("normal projection should admit the route");
        assert_eq!(admission, SendAdmission::Normal);
    }

    #[test]
    fn waiter_budget_enforces_peer_cap_and_releases_raii() {
        let budget = Arc::new(WaiterBudget::default());
        let peer = test_actor_id(7);
        let mut guards = Vec::new();
        for _ in 0..MAX_PEER_WAITERS {
            guards.push(
                budget
                    .try_acquire(&peer, WaiterClass::Rpc, 1)
                    .expect("peer waiter cap should admit its configured limit"),
            );
        }
        assert!(budget.try_acquire(&peer, WaiterClass::Rpc, 1).is_none());
        assert_eq!(budget.snapshot().0, MAX_PEER_WAITERS);

        drop(guards);
        assert_eq!(budget.snapshot(), (0, 0, 0, 0));
        assert!(budget.try_acquire(&peer, WaiterClass::Rpc, 1).is_some());
    }

    #[test]
    fn waiter_budget_enforces_global_cap() {
        let budget = Arc::new(WaiterBudget::default());
        let mut guards = Vec::with_capacity(MAX_GLOBAL_WAITERS);
        // Mix classes so neither per-class cap fires before the global one:
        // 224 Rpc (the class cap) + 32 Stream = 256 = the global cap.
        for serial in 0..MAX_RPC_WAITERS {
            let peer = test_actor_id(serial as u64);
            guards.push(
                budget
                    .try_acquire(&peer, WaiterClass::Rpc, 1)
                    .unwrap_or_else(|| panic!("global cap should admit rpc waiter {serial}")),
            );
        }
        for serial in 0..MIN_STREAM_WAITERS {
            let peer = test_actor_id((serial + 10_000) as u64);
            guards.push(
                budget
                    .try_acquire(&peer, WaiterClass::Stream, 1)
                    .unwrap_or_else(|| panic!("global cap should admit stream waiter {serial}")),
            );
        }
        assert!(
            budget
                .try_acquire(&test_actor_id(9_999), WaiterClass::Stream, 1)
                .is_none(),
            "the 257th waiter must be rejected by the global cap"
        );
        drop(guards);
        assert_eq!(budget.snapshot().0, 0);
    }

    #[test]
    fn waiter_budget_reserves_per_class_capacity() {
        let budget = Arc::new(WaiterBudget::default());
        let mut rpc_guards = Vec::with_capacity(MAX_RPC_WAITERS);
        // MAX_RPC_WAITERS = MAX_GLOBAL_WAITERS - MIN_STREAM_WAITERS: filling
        // the Rpc class leaves exactly the reserved Stream slots.
        for serial in 0..MAX_RPC_WAITERS {
            rpc_guards.push(
                budget
                    .try_acquire(&test_actor_id(serial as u64), WaiterClass::Rpc, 1)
                    .unwrap_or_else(|| panic!("rpc class cap should admit waiter {serial}")),
            );
        }
        assert!(
            budget
                .try_acquire(&test_actor_id(9_999), WaiterClass::Rpc, 1)
                .is_none(),
            "an exhausted Rpc class must reject a further Rpc waiter"
        );
        let reserved_stream = budget
            .try_acquire(&test_actor_id(8_888), WaiterClass::Stream, 1)
            .expect("the reserved Stream capacity must remain available");
        drop(rpc_guards);
        drop(reserved_stream);
        assert_eq!(budget.snapshot(), (0, 0, 0, 0));
    }

    #[test]
    fn waiter_budget_enforces_byte_ceilings() {
        let budget = Arc::new(WaiterBudget::default());
        let peer = test_actor_id(7);

        // A single payload larger than the per-peer ceiling is refused.
        assert!(
            budget
                .try_acquire(&peer, WaiterClass::Stream, MAX_PEER_WAITER_BYTES + 1)
                .is_none(),
            "an over-ceiling payload must never be retained"
        );

        // Two payloads whose sum exceeds the per-peer ceiling are refused.
        let first = budget
            .try_acquire(&peer, WaiterClass::Stream, MAX_PEER_WAITER_BYTES - 1)
            .expect("a payload just under the per-peer ceiling fits");
        assert!(
            budget.try_acquire(&peer, WaiterClass::Stream, 2).is_none(),
            "the per-peer byte ceiling must reject the second retained payload"
        );
        drop(first);

        // Distinct peers sharing the global byte ceiling.
        let mut guards = Vec::new();
        for serial in 0..(MAX_GLOBAL_WAITER_BYTES / MAX_PEER_WAITER_BYTES) {
            let peer = test_actor_id(serial as u64);
            guards.push(
                budget
                    .try_acquire(&peer, WaiterClass::Stream, MAX_PEER_WAITER_BYTES)
                    .unwrap_or_else(|| panic!("global byte ceiling should admit peer {serial}")),
            );
        }
        assert!(
            budget
                .try_acquire(&test_actor_id(9_999), WaiterClass::Stream, 1)
                .is_none(),
            "the global byte ceiling must reject further retained payloads"
        );
        drop(guards);
        assert_eq!(budget.snapshot(), (0, 0, 0, 0));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn route_waiter_exhaustion_fails_fast_from_public_send_path() {
        let (gate, _transport, projection_tx, _builds, sends) =
            gate_with_projection(SendProjection::Blocked);
        let gate = Arc::new(gate);
        let target = test_actor_id(3);

        let mut requests = Vec::new();
        for _ in 0..MAX_PEER_WAITERS {
            let gate = Arc::clone(&gate);
            let target = target.clone();
            requests.push(tokio::spawn(async move {
                gate.send_tell_with_type_until(
                    &target,
                    PayloadType::RpcReliable,
                    RpcEnvelope {
                        request_id: "exhaustion".to_string(),
                        ..RpcEnvelope::default()
                    },
                    RpcDeadline::from_duration(Duration::from_secs(5)),
                )
                .await
            }));
        }

        // Wait until every waiter is registered before overflowing.
        let register_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while gate.waiter_budget.snapshot().0 < MAX_PEER_WAITERS {
            assert!(
                tokio::time::Instant::now() < register_deadline,
                "waiters never registered"
            );
            crate::timer::sleep(
                crate::timer::ids::PEER_TEST_EVENT_DELAY,
                Duration::from_millis(5),
            )
            .await;
        }

        let overflow = gate
            .send_tell_with_type_until(
                &target,
                PayloadType::RpcReliable,
                RpcEnvelope {
                    request_id: "overflow".to_string(),
                    ..RpcEnvelope::default()
                },
                RpcDeadline::from_duration(Duration::from_secs(5)),
            )
            .await;
        match overflow {
            Err(ActrError::Unavailable(message)) => assert!(
                message.contains("outbound readiness waiter capacity exhausted"),
                "unexpected exhaustion diagnostic: {message}"
            ),
            other => panic!("the overflowing waiter must fail fast, got {other:?}"),
        }

        // Release: clearing the projection admits every parked waiter and
        // the RAII guards return the budget.
        projection_tx.send_replace(SendProjection::Normal);
        for request in requests {
            request
                .await
                .expect("waiter task must not panic")
                .expect("admitted waiter should submit its tell");
        }
        assert_eq!(sends.load(Ordering::SeqCst), MAX_PEER_WAITERS);
        assert_eq!(
            gate.waiter_budget.snapshot(),
            (0, 0, 0, 0),
            "all waiter budget must be released after completion"
        );
    }

    #[tokio::test]
    async fn route_waiter_wakes_on_destination_materialization_signal() {
        let (gate, transport, _projection_tx, _builds, _sends) =
            gate_with_projection(SendProjection::Normal);
        let target = test_actor_id(4);
        let dest = PeerGate::actr_id_to_dest(&target);
        let _gate = gate;

        // The parked-waiter scenario the wake source exists for: the
        // destination does not exist yet, so `watch_ready_for` yields no
        // ready-set and only the transports-map revision can report its
        // materialization (e.g. a WebSocket fallback committed elsewhere).
        // The revision is destination-agnostic by design — every subscriber
        // re-runs its own preflight and re-acquires its own ready-set — and
        // it fires on EVERY lifecycle transition, not just successful
        // Connected commits, so non-committed changes (flight insert,
        // removal, teardown) also wake parked waiters instead of leaving
        // them stranded until their operation deadline.
        assert!(transport.watch_ready_for(&dest).await.is_none());
        let mut revision = transport.watch_transports_revision();
        let baseline = *revision.borrow_and_update();

        transport
            .get_or_create_transport(&dest)
            .await
            .expect("destination transport should materialize");

        revision
            .changed()
            .await
            .expect("destination materialization must be observable");
        assert!(
            *revision.borrow_and_update() >= baseline + 2,
            "the Connecting insert and the Connected commit each bump the revision"
        );
        assert!(
            transport.watch_ready_for(&dest).await.is_some(),
            "after the wake the waiter can acquire the ready-set it lacked"
        );

        // Non-committed lifecycle transitions wake too: closing the
        // destination removes it and bumps the revision again, so a waiter
        // parked on a vanished route re-runs instead of stranding.
        transport
            .close_transport(&dest)
            .await
            .expect("close must succeed");
        revision
            .changed()
            .await
            .expect("destination removal must be observable");
        assert!(transport.watch_ready_for(&dest).await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tell_waits_for_route_then_submits_once() {
        let (gate, _transport, projection_tx, _builds, sends) =
            gate_with_projection(SendProjection::Blocked);
        let gate = Arc::new(gate);
        let target = test_actor_id(2);
        let request = {
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                gate.send_tell_with_type_until(
                    &target,
                    PayloadType::RpcReliable,
                    RpcEnvelope {
                        request_id: "waited-tell".to_string(),
                        ..RpcEnvelope::default()
                    },
                    RpcDeadline::from_duration(Duration::from_secs(1)),
                )
                .await
            })
        };

        tokio::task::yield_now().await;
        assert!(
            !request.is_finished(),
            "Tell should wait while sends are blocked"
        );
        projection_tx.send_replace(SendProjection::Normal);

        request
            .await
            .expect("Tell task must not panic")
            .expect("Tell should submit after the route becomes ready");
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stream_reliable_waits_for_route_before_sending() {
        let (gate, _transport, projection_tx, _builds, sends) =
            gate_with_projection(SendProjection::Blocked);
        let gate = Arc::new(gate);
        let target = test_actor_id(2);
        let request = {
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                gate.send_data_chunk_until(
                    &target,
                    PayloadType::StreamReliable,
                    "waited-stream",
                    Bytes::from_static(b"chunk"),
                    RpcDeadline::from_duration(Duration::from_secs(1)),
                    "waited-stream-operation",
                )
                .await
            })
        };

        tokio::task::yield_now().await;
        assert!(
            !request.is_finished(),
            "StreamReliable should wait while blocked"
        );
        projection_tx.send_replace(SendProjection::Normal);

        request
            .await
            .expect("DataStream task must not panic")
            .expect("StreamReliable should send after route readiness");
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_message_with_type_reports_uncertain_before_reliable_retry_backoff() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(RetryFailingWireBuilder {
                attempts: attempts.clone(),
            }),
        ));
        let gate = PeerGate::for_test(transport);
        let target = test_actor_id(2);

        let err = gate
            .send_message_with_type_inner(
                &target,
                PayloadType::RpcReliable,
                envelope_with_direction(None),
                Some(Duration::from_millis(250)),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            &err,
            ActrError::DeliveryUncertain { operation_id, .. } if operation_id == "req-direction"
        ));
        assert!(!err.is_retryable());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tell_submits_at_most_once_when_delivery_is_uncertain() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(RetryFailingWireBuilder {
                attempts: attempts.clone(),
            }),
        ));
        let gate = PeerGate::for_test(transport);
        let target = test_actor_id(2);
        let envelope = RpcEnvelope {
            request_id: "tell-once".to_string(),
            ..envelope_with_direction(None)
        };

        let error = gate
            .send_tell_with_type_until(
                &target,
                PayloadType::RpcReliable,
                envelope,
                RpcDeadline::from_duration(Duration::from_secs(1)),
            )
            .await
            .expect_err("an uncertain send must surface an error");

        assert!(matches!(
            error,
            ActrError::DeliveryUncertain { operation_id, .. } if operation_id == "tell-once"
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[derive(Debug)]
    struct PendingLane;

    #[async_trait]
    impl DataLane for PendingLane {
        async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
            std::future::pending::<NetworkResult<()>>().await
        }

        fn lane_type(&self) -> &'static str {
            "pending"
        }
    }

    #[derive(Debug)]
    struct PendingWebSocketWire;

    #[async_trait]
    impl WireHandle for PendingWebSocketWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebSocket
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn close(&self) -> NetworkResult<()> {
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            Ok(Arc::new(PendingLane))
        }
    }

    struct PendingWireBuilder;

    #[async_trait]
    impl WireBuilder for PendingWireBuilder {
        async fn create_connections(
            &self,
            _dest: &Dest,
        ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            Ok(vec![Arc::new(PendingWebSocketWire)])
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_data_chunk_timeout_mid_write_is_delivery_uncertain() {
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(PendingWireBuilder),
        ));
        let gate = PeerGate::for_test(transport);
        let target = test_actor_id(2);
        let chunk = actr_protocol::DataChunk {
            stream_id: "stream-1".to_string(),
            sequence: 4,
            payload: Bytes::from_static(b"chunk"),
            metadata: vec![],
            timestamp_ms: None,
        };

        let err = gate
            .send_data_chunk_with_timeout(
                &target,
                PayloadType::StreamReliable,
                "stream-1",
                Bytes::from(chunk.encode_to_vec()),
                Duration::from_millis(250),
                "stream-1#4",
            )
            .await
            .unwrap_err();

        // The deadline cancelled the send while a lane write was in flight.
        // Bytes may already have entered the transport and the data-chunk
        // path has no dedup, so the failure must be terminal and correlate to
        // the caller-visible stream + sequence identity — never a retryable
        // TimedOut that an auto-retry would turn into a duplicate delivery.
        assert!(
            matches!(
                &err,
                ActrError::DeliveryUncertain { operation_id, .. } if operation_id == "stream-1#4"
            ),
            "got {err:?}"
        );
        assert!(!err.is_retryable());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tell_timeout_mid_write_is_delivery_uncertain() {
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(PendingWireBuilder),
        ));
        let gate = PeerGate::for_test(transport);
        let target = test_actor_id(2);
        let envelope = RpcEnvelope {
            request_id: "tell-timeout".to_string(),
            ..envelope_with_direction(None)
        };

        let err = gate
            .send_tell_with_type_until(
                &target,
                PayloadType::RpcReliable,
                envelope,
                RpcDeadline::from_duration(Duration::from_millis(250)),
            )
            .await
            .unwrap_err();

        // Tell is one-shot: once the lane write was entered, a deadline must
        // not be reported as a retryable timeout that could replay the
        // payload. The operation ID remains caller-correlatable.
        assert!(
            matches!(
                &err,
                ActrError::DeliveryUncertain { operation_id, .. }
                    if operation_id == "tell-timeout"
            ),
            "got {err:?}"
        );
        assert!(!err.is_retryable());
    }

    #[derive(Debug)]
    struct LanelessPendingWire;

    #[async_trait]
    impl WireHandle for LanelessPendingWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebSocket
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn close(&self) -> NetworkResult<()> {
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            std::future::pending().await
        }
    }

    struct LanelessPendingWireBuilder;

    #[async_trait]
    impl WireBuilder for LanelessPendingWireBuilder {
        async fn create_connections(
            &self,
            _dest: &Dest,
        ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            Ok(vec![Arc::new(LanelessPendingWire)])
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_data_chunk_timeout_before_write_is_timed_out() {
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(LanelessPendingWireBuilder),
        ));
        let gate = PeerGate::for_test(transport);
        let target = test_actor_id(2);

        let err = gate
            .send_data_chunk_with_timeout(
                &target,
                PayloadType::StreamReliable,
                "stream-1",
                Bytes::from_static(b"chunk"),
                Duration::from_millis(250),
                "stream-1",
            )
            .await
            .unwrap_err();

        // The deadline fired before any lane write was approached (lane
        // acquisition never resolved), so nothing can have entered a
        // transport: the caller may safely retry.
        assert!(matches!(err, ActrError::TimedOut), "got {err:?}");
    }
    /// #390: dropping the `send_request` future (Kotlin coroutine cancel =
    /// Rust future drop) must synchronously remove its pending entry instead
    /// of leaking it until the timeout deadline.
    #[tokio::test]
    async fn dropping_send_request_future_removes_pending_entry() {
        let target = test_actor_id(2);
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(CountingSuccessfulWireBuilder {
                builds: Arc::new(AtomicUsize::new(0)),
                sends: Arc::new(AtomicUsize::new(0)),
            }),
        ));
        let pending_requests: PendingRequestsMap = Arc::new(RpcCorrelation::new());
        let (_projection_tx, projection_rx) = watch::channel(SendProjection::Normal);
        let gate = Arc::new(PeerGate::with_v020_manager(
            transport,
            PeerGate::empty_manager_for_test(),
            pending_requests.clone(),
            Some(projection_rx),
        ));

        let envelope = RpcEnvelope {
            request_id: "dropped-caller".to_string(),
            timeout_ms: 30_000,
            ..Default::default()
        };

        // The wire send succeeds but no response ever arrives; the caller
        // abandons the wait while the request is pending.
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let gate_for_task = gate.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = cancel_rx => {}
                res = gate_for_task.send_request_with_type(
                    &target,
                    PayloadType::RpcReliable,
                    envelope,
                ) => {
                    panic!("no response exists; the request must not complete, got {res:?}")
                }
            }
        });

        let register_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while pending_requests.len() == 0 {
            assert!(
                tokio::time::Instant::now() < register_deadline,
                "request must register before the caller cancels"
            );
            crate::timer::sleep(
                crate::timer::ids::PEER_REQUEST_DEADLINE,
                Duration::from_millis(10),
            )
            .await;
        }

        cancel_tx.send(()).expect("task still running");
        crate::timer::timeout(
            crate::timer::ids::PEER_REQUEST_DEADLINE,
            Duration::from_secs(5),
            task,
        )
        .await
        .expect("select task must finish after cancel")
        .expect("task must not panic");

        assert_eq!(
            pending_requests.len(),
            0,
            "PendingRpc RAII must drop the entry with the request future"
        );
    }

    #[cfg(feature = "webrtc-v020")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn non_sendable_controller_with_ready_websocket_fallback_admits() {
        use crate::transport::ConnType;

        // A controller created against the no-op session factory never
        // connects, so its snapshot stays non-sendable. Together with a
        // destination whose WebSocket lane is ready, the RTC preflight must
        // still admit a reliable send — the peer is reachable through the
        // fallback wire, so the send is delivered instead of rejected.
        let ws_sends = Arc::new(AtomicUsize::new(0));
        let target = test_actor_id(2);

        #[derive(Debug)]
        struct ReadyWsLane {
            sends: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl DataLane for ReadyWsLane {
            async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
                self.sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            fn lane_type(&self) -> &'static str {
                "ready-ws-test"
            }
        }

        #[derive(Debug)]
        struct ReadyWsWire {
            lane: Arc<ReadyWsLane>,
        }

        #[async_trait]
        impl WireHandle for ReadyWsWire {
            fn connection_type(&self) -> ConnType {
                ConnType::WebSocket
            }

            fn priority(&self) -> u8 {
                1
            }

            async fn connect(&self) -> NetworkResult<()> {
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }

            async fn close(&self) -> NetworkResult<()> {
                Ok(())
            }

            async fn get_lane(&self, _t: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
                Ok(self.lane.clone())
            }
        }

        #[derive(Debug)]
        struct ReadyWsWireBuilder {
            wire: Arc<ReadyWsWire>,
        }

        #[async_trait]
        impl WireBuilder for ReadyWsWireBuilder {
            async fn create_connections(
                &self,
                _dest: &Dest,
            ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
                Ok(vec![self.wire.clone()])
            }
        }

        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(ReadyWsWireBuilder {
                wire: Arc::new(ReadyWsWire {
                    lane: Arc::new(ReadyWsLane {
                        sends: ws_sends.clone(),
                    }),
                }),
            }),
        ));
        let manager = PeerGate::empty_manager_for_test();
        let pending_requests: PendingRequestsMap = Arc::new(RpcCorrelation::new());
        let gate =
            PeerGate::with_v020_manager(transport.clone(), manager.clone(), pending_requests, None);

        // Force a non-sendable controller for the target: it exists, but the
        // no-op factory never completes a session, so `sendable` is false.
        let _controller = manager.ensure_controller(target.clone()).await;
        let snapshot = manager.get_snapshot(&target).await;
        assert!(
            snapshot.as_ref().is_some_and(|s| !s.sendable),
            "controller snapshot must be non-sendable in this fixture: {snapshot:?}"
        );

        // Seed the destination transport so the ready WebSocket wire is
        // materialized for the dest before the reliable send is issued.
        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("seed send materializes the WebSocket destination");
        let dest = Dest::peer(target.clone());
        assert!(transport.has_outbound_materialization(&dest).await);

        gate.send_message_with_type(&target, PayloadType::RpcReliable, RpcEnvelope::default())
            .await
            .expect("reliable send must be admitted over the ready WebSocket fallback");

        assert_eq!(
            ws_sends.load(Ordering::SeqCst),
            2,
            "both sends must ride the ready WebSocket lane despite the non-sendable controller"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transports_revision_wakes_on_every_materialization_transition() {
        use crate::transport::ConnType;

        // Wake source for readiness waiters that found no destination
        // ready-set (`wait_until_route_ready` parks only when the RTC
        // preflight blocks AND no fallback exists): every transports-map
        // transition — Connecting flight insert, Connected commit, removal —
        // must bump the revision while the map lock is held, so a waiter
        // parked before the transition re-runs its lookup afterwards instead
        // of expiring at the outer deadline. (Driving the full park-then-
        // materialize interleave end to end requires a live RTC session; the
        // revision contract is pinned here and consumed by the select arm in
        // wait_until_route_ready.)
        #[derive(Debug)]
        struct RevWsLane;

        #[derive(Debug)]
        struct RevWsWire;

        #[derive(Debug)]
        struct RevWsWireBuilder;

        #[async_trait]
        impl DataLane for RevWsLane {
            async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
                Ok(())
            }
            fn lane_type(&self) -> &'static str {
                "rev-ws-test"
            }
        }

        #[async_trait]
        impl WireHandle for RevWsWire {
            fn connection_type(&self) -> ConnType {
                ConnType::WebSocket
            }
            fn priority(&self) -> u8 {
                1
            }
            async fn connect(&self) -> NetworkResult<()> {
                Ok(())
            }
            fn is_connected(&self) -> bool {
                true
            }
            async fn close(&self) -> NetworkResult<()> {
                Ok(())
            }
            async fn get_lane(&self, _t: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
                Ok(Arc::new(RevWsLane))
            }
        }

        #[async_trait]
        impl WireBuilder for RevWsWireBuilder {
            async fn create_connections(
                &self,
                _d: &Dest,
            ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
                Ok(vec![Arc::new(RevWsWire)])
            }
        }

        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(RevWsWireBuilder),
        ));
        let mut revision = transport.watch_transports_revision();
        assert_eq!(*revision.borrow_and_update(), 0);

        // Materialize: inserting the Connecting flight and committing the
        // Connected DestTransport each bump the revision.
        let dest = Dest::peer(test_actor_id(2));
        let committed = transport
            .get_or_create_transport(&dest)
            .await
            .expect("destination must materialize");
        assert!(Arc::ptr_eq(&committed, &committed)); // keep the transport alive
        let after_commit = *revision.borrow_and_update();
        assert!(
            after_commit >= 2,
            "flight insert + Connected commit must each bump the revision: {after_commit}"
        );

        // Closing removes the destination and bumps again, so waiters also
        // re-run after a route disappears.
        transport
            .close_transport(&dest)
            .await
            .expect("close must succeed");
        assert!(
            *revision.borrow_and_update() > after_commit,
            "removal must bump the revision"
        );
    }
    #[cfg(feature = "webrtc-v020")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn at_most_once_survives_an_unsafe_reattempt_after_unknown() {
        // Adversarial pin for the safe-reattempt boundary: a lane may report
        // an honest `NotAttempted` failure (which earns the single bounded
        // reattempt), but the moment an attempt classifies as `Unknown` no
        // further wire write may happen — the third scripted outcome (`Ok`)
        // would be reached and delivered if the boundary regressed into an
        // unsafe replay.
        use crate::transport::SendAttemptResult;
        use std::collections::VecDeque;

        #[derive(Debug)]
        struct AdversarialLane {
            writes: Arc<AtomicUsize>,
            outcomes: tokio::sync::Mutex<VecDeque<SendAttemptResult>>,
        }

        #[async_trait]
        impl DataLane for AdversarialLane {
            async fn send_with_progress(&self, _data: bytes::Bytes) -> SendAttemptResult {
                self.writes.fetch_add(1, Ordering::SeqCst);
                self.outcomes
                    .lock()
                    .await
                    .pop_front()
                    .expect("script must cover every attempted write")
            }

            fn lane_type(&self) -> &'static str {
                "adversarial-at-most-once"
            }
        }

        #[derive(Debug)]
        struct AdversarialWire {
            lane: Arc<AdversarialLane>,
        }

        #[async_trait]
        impl WireHandle for AdversarialWire {
            fn connection_type(&self) -> crate::transport::ConnType {
                crate::transport::ConnType::WebSocket
            }
            fn priority(&self) -> u8 {
                1
            }
            async fn connect(&self) -> NetworkResult<()> {
                Ok(())
            }
            fn is_connected(&self) -> bool {
                true
            }
            async fn close(&self) -> NetworkResult<()> {
                Ok(())
            }
            async fn get_lane(&self, _t: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
                Ok(self.lane.clone())
            }
        }

        #[derive(Debug)]
        struct AdversarialWireBuilder {
            wire: Arc<AdversarialWire>,
        }

        #[async_trait]
        impl WireBuilder for AdversarialWireBuilder {
            async fn create_connections(
                &self,
                _d: &Dest,
            ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
                Ok(vec![self.wire.clone()])
            }
        }

        let writes = Arc::new(AtomicUsize::new(0));
        let outcomes = VecDeque::from(vec![
            // First write: proven pre-send closure — eligible for the single
            // bounded reattempt.
            SendAttemptResult::failed(
                NetworkError::DataChannelClosed("closed before write".into()),
                DeliveryProgress::NotAttempted,
            ),
            // Second write: an ambiguous failure. Any further write here
            // would be an unsafe replay.
            SendAttemptResult::failed(
                NetworkError::SendError("ambiguous partial write".into()),
                DeliveryProgress::Unknown,
            ),
            // Third outcome: only reached if the boundary regresses.
            SendAttemptResult::submitted(),
        ]);

        let gate = PeerGate::for_test(Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(AdversarialWireBuilder {
                wire: Arc::new(AdversarialWire {
                    lane: Arc::new(AdversarialLane {
                        writes: writes.clone(),
                        outcomes: tokio::sync::Mutex::new(outcomes),
                    }),
                }),
            }),
        )));

        let err = gate
            .send_tell_with_type_until(
                &test_actor_id(2),
                PayloadType::RpcReliable,
                RpcEnvelope::default(),
                RpcDeadline::from_duration(Duration::from_secs(1)),
            )
            .await
            .expect_err("the ambiguous attempt must surface");

        assert!(
            matches!(&err, ActrError::DeliveryUncertain { .. }),
            "got {err:?}"
        );
        assert!(!err.is_retryable());
        assert_eq!(
            writes.load(Ordering::SeqCst),
            2,
            "exactly two wire writes: initial + one safe reattempt; the Unknown attempt must never be replayed"
        );
    }
}
