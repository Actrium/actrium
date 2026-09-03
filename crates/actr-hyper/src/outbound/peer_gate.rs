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
use actr_framework::{Bytes, MediaSample};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    ActorResult, ActrError, ActrId, Classify, ConnectionNotReadyInfo, Direction, PayloadType,
    RpcEnvelope,
};
use std::collections::HashMap;
use std::sync::Arc;
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
    /// a peer whose controller exists but is not sendable (recovery window)
    /// is rejected; a peer with NO controller yet is admitted so the
    /// transport dials on demand (0.14 parity: no recovery entry -> admit),
    /// which is also what keeps WebSocket-only destinations sendable.
    async fn preflight_send(&self, target: &ActrId, dest: &Dest) -> ActorResult<SendAdmission> {
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
            {
                return Err(Self::send_projection_not_ready_error());
            }
        }

        // Policy may have tightened during the snapshot await. Keep the
        // strictest projection seen by this operation.
        self.tighten_with_current_projection(admission)
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
        let admission = self.tighten_with_current_projection(admission)?;
        let attempt = self
            .attempt_send(dest, payload_type, data, admission, record, progress_sink)
            .await;
        match attempt.result {
            Ok(Some(identity)) => Ok(identity),
            Ok(None) => Err(Self::send_projection_not_ready_error()),
            Err(error) if attempt.progress == DeliveryProgress::Unknown => {
                Err(Self::delivery_uncertain_error(operation_id, error))
            }
            Err(error) => Err(error),
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
        let admission = self.preflight_send(target, &dest).await?;

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
        let sent_transport = Arc::new(Mutex::new(None));
        let sent_transport_for_send = Arc::clone(&sent_transport);
        let mut send_started = false;

        let result = crate::timer::timeout_at(
            crate::timer::ids::PEER_REQUEST_DEADLINE,
            deadline.instant(),
            async {
                let admission = self.preflight_send(target, &dest).await?;
                deadline.ensure_remaining()?;

                // The RAII guard removes pending state on send failure,
                // response-channel closure, timeout, or caller cancellation.
                let pending = self
                    .pending_requests
                    .register(request_id.clone(), target.clone());
                let data = Self::serialize_envelope(&envelope);
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

        // Same cancellation-classification contract as the data-chunk path:
        // a deadline that fires mid-write must surface terminal
        // `DeliveryUncertain`, not a retryable `TimedOut`.
        let progress_cell = new_delivery_progress_cell();
        match crate::timer::timeout_at(
            crate::timer::ids::OUTBOUND_TELL_DEADLINE,
            deadline.instant(),
            async {
                let data = Self::serialize_envelope(&envelope);
                let admission = self.preflight_send(target, &dest).await?;
                deadline.ensure_remaining()?;
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
        let admission = self.preflight_send(target, &dest).await?;
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
        let admission = self.preflight_send(target, &dest).await?;
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
        let admission = self.preflight_send(target, &dest).await?;
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
        let admission = self.preflight_send(target, &dest).await?;
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
                let admission = self.preflight_send(target, &dest).await?;
                deadline.ensure_remaining()?;
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
            Ok(Err(e)) => Err(e),
            Err(_) => match observed_delivery_progress(&progress_cell) {
                None | Some(DeliveryProgress::NotAttempted) => Err(ActrError::TimedOut),
                Some(DeliveryProgress::SubmittedLocally) | Some(DeliveryProgress::Unknown) => {
                    Err(Self::delivery_uncertain_error(
                        Some(operation_id),
                        "data chunk send deadline elapsed after transport submission",
                    ))
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
    async fn blocked_projection_rejects_before_pending_registration_or_build() {
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
            .expect_err("blocked projection must reject new outbound work");

        assert!(matches!(error, ActrError::ConnectionNotReady(_)));
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
            .preflight_send(&target, &dest)
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
            .preflight_send(&target, &dest)
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
}
