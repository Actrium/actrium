//! PeerGate - Peer transport adapter (outbound)
//!
//! # Responsibilities
//! - Wrap PeerTransport (Protobuf serialization)
//! - Used for cross-process communication (WebRTC + WebSocket)
//! - Maintain pending_requests (Request/Response matching)
//! - Block new requests to peers being cleaned up (closing_peers)

use super::data_chunk_activity::{DataChunkActivityTracker, DataChunkRecordState};
use super::ensure_stream_payload_type;
use crate::lifecycle::SendProjection;
use crate::transport::{
    ConnectionEvent, ConnectionState, Dest, DestTransportRef, NetworkResult, PayloadTypeExt,
    PeerTransport, WireIdentity,
};
use crate::wire::webrtc::{NetworkRecoveryStatus, WebRtcCoordinator};
use actr_framework::{Bytes, MediaSample};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    ActorResult, ActrError, ActrId, Classify, ConnectionNotReadyInfo, Direction, PayloadType,
    RpcEnvelope,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast, oneshot, watch};
use tokio::time::Instant;

/// Pending requests map type: request_id -> (target_actor_id, oneshot response sender)
pub(crate) type PendingRequestsMap =
    Arc<RwLock<HashMap<String, (ActrId, oneshot::Sender<actr_protocol::ActorResult<Bytes>>)>>>;

/// Internal upper bound for a single DataChunk send operation.
///
/// DataChunk has no caller-provided request deadline like RPC envelopes, so
/// this prevents a stalled WebRTC DataChannel send from holding the mobile
/// caller forever during unrecoverable network loss.
const DATA_CHUNK_SEND_TIMEOUT: Duration = Duration::from_secs(15);
/// Total budget for fire-and-forget RPC sends, including transient retries.
///
/// `tell` does not wait for a response, so it is bounded by the transport
/// send budget rather than a caller RPC deadline (`timeout_ms` is 0 for tell).
/// In practice tell dispatches via `RpcReliable`, whose retry policy is
/// 5 attempts with a 1s→2s→4s→5s backoff (4 retries, 12s cumulative delay);
/// this timeout covers that full backoff plus a small margin for the send
/// attempts themselves, so a reliable tell can still complete all retries
/// while a stalled transport is capped instead of hanging indefinitely.
const TELL_SEND_TIMEOUT: Duration = Duration::from_secs(13);
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

impl SendAdmission {
    fn tighten(self, other: Self) -> Self {
        if self == Self::ExistingOnly || other == Self::ExistingOnly {
            Self::ExistingOnly
        } else {
            Self::Normal
        }
    }
}

/// PeerGate - Outproc transport adapter (outbound)
///
/// # Features
/// - Protobuf serialization: serialize RpcEnvelope to byte stream
/// - Defaults to PayloadType::RpcReliable for RPC messages
/// - Maintain pending_requests for Request/Response matching
/// - Support MediaTrack sending via WebRTC
/// - Block new requests to peers being cleaned up (closing_peers)
pub struct PeerGate {
    /// PeerTransport instance
    transport_manager: Arc<PeerTransport>,

    /// Pending requests: request_id -> (target_actor_id, oneshot::Sender<Bytes>)
    /// Stores both the target ActorId and response sender for efficient cleanup by peer
    pending_requests: PendingRequestsMap,

    /// WebRTC coordinator (optional, for MediaTrack support)
    webrtc_coordinator: Option<Arc<WebRtcCoordinator>>,

    #[allow(unused)]
    /// todo: Peers currently being cleaned up (block new requests) ,closed requests will be cleaned up in event listener
    closing_peers: Arc<RwLock<HashSet<ActrId>>>,

    /// Recently sent `send_data_chunk` chunks used to surface delivery
    /// uncertainty when WebRTC/DataChannel state changes mid-stream.
    active_data_chunks: Arc<RwLock<DataChunkActivityTracker>>,

    /// Live global send authority derived by the recovery supervisor.
    send_projection_rx: watch::Receiver<SendProjection>,
}

impl PeerGate {
    fn connection_not_ready_error(status: &NetworkRecoveryStatus) -> ActrError {
        ActrError::ConnectionNotReady(ConnectionNotReadyInfo::new(
            status.elapsed_ms(),
            status.timeout_ms(),
        ))
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

    async fn notify_active_data_chunks_uncertain(
        webrtc_coordinator: &WebRtcCoordinator,
        active_data_chunks: &Arc<RwLock<DataChunkActivityTracker>>,
        peer_id: &ActrId,
        session_id: u64,
        reason: &str,
    ) {
        let notices = {
            let mut tracker = active_data_chunks.write().await;
            tracker.mark_delivery_uncertain(peer_id, session_id, reason, Instant::now())
        };

        for notice in notices {
            webrtc_coordinator
                .notify_data_chunk_delivery_uncertain(
                    notice.stream_id,
                    notice.session_id,
                    notice.reason,
                )
                .await;
        }
    }

    async fn record_active_data_chunk_if_needed(
        &self,
        target: &ActrId,
        stream_id: &str,
        session_id: u64,
        now: Instant,
    ) -> bool {
        let record_state = {
            let tracker = self.active_data_chunks.read().await;
            tracker.record_state(target, stream_id, session_id, now)
        };

        if record_state != DataChunkRecordState::Fresh {
            self.active_data_chunks.write().await.record_stream(
                target,
                stream_id.to_string(),
                session_id,
                now,
            );
        }

        record_state == DataChunkRecordState::Missing
    }

    /// Create new PeerGate with a pre-allocated `pending_requests` map.
    ///
    /// Use this when the map must be shared with other components (e.g.
    /// `DefaultWireBuilder`) before `PeerGate` itself is constructed.
    pub fn with_pending_requests(
        transport_manager: Arc<PeerTransport>,
        webrtc_coordinator: Option<Arc<WebRtcCoordinator>>,
        pending_requests: PendingRequestsMap,
    ) -> Self {
        Self::with_pending_requests_and_send_projection(
            transport_manager,
            webrtc_coordinator,
            pending_requests,
            Self::normal_send_projection_receiver(),
        )
    }

    /// Production constructor with the recovery supervisor's live send
    /// projection. Kept crate-private so the projection remains an internal
    /// policy contract rather than a second public configuration surface.
    pub(crate) fn with_pending_requests_and_send_projection(
        transport_manager: Arc<PeerTransport>,
        webrtc_coordinator: Option<Arc<WebRtcCoordinator>>,
        pending_requests: PendingRequestsMap,
        send_projection_rx: watch::Receiver<SendProjection>,
    ) -> Self {
        let closing_peers = Arc::new(RwLock::new(HashSet::new()));
        let active_data_chunks = Arc::new(RwLock::new(DataChunkActivityTracker::default()));

        // Start event listener if coordinator is available
        if let Some(ref coordinator) = webrtc_coordinator {
            Self::spawn_event_listener(
                coordinator.subscribe_events(),
                Arc::clone(coordinator),
                Arc::clone(&pending_requests),
                Arc::clone(&closing_peers),
                Arc::clone(&active_data_chunks),
                Arc::clone(&transport_manager),
            );
        }

        Self {
            transport_manager,
            pending_requests,
            webrtc_coordinator,
            closing_peers,
            active_data_chunks,
            send_projection_rx,
        }
    }

    fn normal_send_projection_receiver() -> watch::Receiver<SendProjection> {
        let (_tx, rx) = watch::channel(SendProjection::Normal);
        rx
    }

    /// Create new PeerGate
    ///
    /// # Arguments
    /// - `transport_manager`: PeerTransport instance
    /// - `webrtc_coordinator`: Optional WebRTC coordinator for MediaTrack support
    #[allow(dead_code)]
    pub fn new(
        transport_manager: Arc<PeerTransport>,
        webrtc_coordinator: Option<Arc<WebRtcCoordinator>>,
    ) -> Self {
        let pending_requests = Arc::new(RwLock::new(HashMap::new()));
        Self::with_pending_requests(transport_manager, webrtc_coordinator, pending_requests)
    }

    /// Spawn event listener task to handle connection events
    ///
    /// This is the **ONLY** WebRTC event subscriber in the cleanup chain.
    /// On `ConnectionClosed` it closes only the WebRTC handle so that an
    /// outbound WebSocket connection (if present) continues to serve
    /// response-reader tasks.
    fn spawn_event_listener(
        mut event_rx: broadcast::Receiver<ConnectionEvent>,
        webrtc_coordinator: Arc<WebRtcCoordinator>,
        pending_requests: PendingRequestsMap,
        closing_peers: Arc<RwLock<HashSet<ActrId>>>,
        active_data_chunks: Arc<RwLock<DataChunkActivityTracker>>,
        transport_manager: Arc<PeerTransport>,
    ) {
        tokio::spawn(async move {
            loop {
                let event = match event_rx.recv().await {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "PeerGate event listener lagged by {} events, continuing",
                            n
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("PeerGate event listener stopped (channel closed)");
                        break;
                    }
                };
                tracing::debug!("PeerGate received connection event: {:?}", event);
                match &event {
                    ConnectionEvent::StateChanged {
                        peer_id,
                        session_id,
                        state: ConnectionState::Connected,
                        ..
                    }
                    | ConnectionEvent::DataChannelOpened {
                        peer_id,
                        session_id,
                        ..
                    } => {
                        let is_sendable = webrtc_coordinator
                            .is_peer_sendable_session(peer_id, *session_id)
                            .await;

                        if is_sendable {
                            closing_peers.write().await.remove(peer_id);
                            tracing::debug!("Peer {} is sendable again", peer_id);
                        } else {
                            tracing::debug!(
                                peer_id = ?peer_id,
                                event_session_id = session_id,
                                is_sendable = is_sendable,
                                "Ignoring recovery unblock event until current session is sendable",
                            );
                        }
                    }

                    ConnectionEvent::IceRestartCompleted {
                        peer_id,
                        session_id,
                        success: true,
                        ..
                    } => {
                        let is_sendable = webrtc_coordinator
                            .is_peer_sendable_session(peer_id, *session_id)
                            .await;

                        if is_sendable {
                            closing_peers.write().await.remove(peer_id);
                            tracing::debug!("Peer {} is sendable again", peer_id);
                        } else {
                            tracing::debug!(
                                peer_id = ?peer_id,
                                event_session_id = session_id,
                                is_sendable = is_sendable,
                                "Ignoring recovery unblock event until current session is sendable",
                            );
                        }
                    }

                    ConnectionEvent::IceRestartCompleted {
                        peer_id,
                        session_id,
                        success: false,
                        ..
                    } => {
                        if webrtc_coordinator
                            .is_active_session(peer_id, *session_id)
                            .await
                        {
                            Self::notify_active_data_chunks_uncertain(
                                &webrtc_coordinator,
                                &active_data_chunks,
                                peer_id,
                                *session_id,
                                "ice restart failed",
                            )
                            .await;
                        }
                        tracing::debug!(
                            "Peer {} ICE restart failed; coordinator teardown owns the session",
                            peer_id
                        );
                    }

                    ConnectionEvent::DataChannelClosed {
                        peer_id,
                        session_id,
                        payload_type: PayloadType::StreamReliable | PayloadType::StreamLatencyFirst,
                    } => {
                        Self::notify_active_data_chunks_uncertain(
                            &webrtc_coordinator,
                            &active_data_chunks,
                            peer_id,
                            *session_id,
                            "data channel closed",
                        )
                        .await;
                    }

                    // Clean pending requests and trigger downstream cleanup when connection is fully closed
                    ConnectionEvent::StateChanged {
                        peer_id,
                        state: ConnectionState::Closed,
                        session_id: event_session_id,
                        ..
                    }
                    | ConnectionEvent::ConnectionClosed {
                        peer_id,
                        session_id: event_session_id,
                    } => {
                        let dest = Dest::peer(peer_id.clone());

                        Self::notify_active_data_chunks_uncertain(
                            &webrtc_coordinator,
                            &active_data_chunks,
                            peer_id,
                            *event_session_id,
                            "connection closed",
                        )
                        .await;

                        // A close event may belong to a failed intermediate WebRTC attempt while
                        // the factory is still retrying. In that case the original RPC should keep
                        // waiting instead of being failed as "connection closed".
                        if transport_manager.is_connecting(&dest).await {
                            tracing::debug!(
                                "Ignoring transient close for peer {} while connection factory is still running",
                                peer_id
                            );
                            continue;
                        }

                        // Mark peer as closing (release lock immediately to avoid deadlock)
                        {
                            closing_peers.write().await.insert(peer_id.clone());
                        } // Lock released here

                        // 1. Session-guarded cleanup: only close the WebRTC wire if it
                        //    still carries the same session_id.  If the identity
                        //    mismatches (stale event from an old session that has
                        //    already been replaced), skip the close and do NOT clean
                        //    pending requests — they belong to the current wire.
                        //
                        //    We intentionally do NOT tear down the whole transport here
                        //    because that would also close any outbound WebSocket
                        //    connection in the same DestTransport.  The WebSocket
                        //    connection carries response-reader tasks that must
                        //    survive a WebRTC disconnect.  `spawn_ready_monitor`
                        //    will remove the DestTransport entry once all
                        //    connections (including WebSocket) are gone.
                        match transport_manager
                            .close_webrtc_transport_if_session(&dest, peer_id, *event_session_id)
                            .await
                        {
                            Ok(true) => {
                                tracing::info!(
                                    "Closed WebRTC connection for peer {} (session {}, WebSocket kept alive)",
                                    peer_id,
                                    event_session_id
                                );
                            }
                            Ok(false) => {
                                tracing::debug!(
                                    "Stale close event for peer {} (session {}), skipping cleanup",
                                    peer_id,
                                    event_session_id
                                );
                                closing_peers.write().await.remove(peer_id);
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to close WebRTC connection for peer {}: {}",
                                    peer_id,
                                    e
                                );
                            }
                        }

                        // 2. Clean pending requests for this peer
                        let mut pending = pending_requests.write().await;

                        // Collect request_ids that belong to this peer
                        let keys_to_remove: Vec<_> = pending
                            .iter()
                            .filter_map(|(req_id, (target, _))| {
                                if target == peer_id {
                                    Some(req_id.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let cleaned_count = keys_to_remove.len();

                        tracing::info!(
                            "Cleaned {} pending requests for peer {}",
                            cleaned_count,
                            peer_id
                        );

                        // Remove and send error to all pending requests for this peer
                        for key in keys_to_remove {
                            if let Some((_, tx)) = pending.remove(&key) {
                                let _ = tx.send(Err(ActrError::Unavailable(
                                    "Connection closed".to_string(),
                                )));
                            }
                        }
                        drop(pending); // Release lock before calling downstream

                        closing_peers.write().await.remove(peer_id);
                    }

                    _ => {} // Ignore other events
                }
            }
        });
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
        let mut pending = self.pending_requests.write().await;

        if let Some((target, tx)) = pending.remove(request_id) {
            // Wake up waiting request with result (success or error)
            let _ = tx.send(result);
            tracing::debug!("Completed request: {} (target: {})", request_id, target);
            Ok(true)
        } else {
            tracing::warn!("No pending request for: {}", request_id);
            Ok(false)
        }
    }

    /// Get pending requests count (for monitoring)
    #[cfg(feature = "test-utils")]
    pub async fn pending_count(&self) -> usize {
        self.pending_requests.read().await.len()
    }

    /// Register a pending request without sending on the wire.
    #[cfg(feature = "test-utils")]
    pub async fn register_pending_for_test(
        &self,
        request_id: &str,
        target: ActrId,
    ) -> oneshot::Receiver<ActorResult<Bytes>> {
        let (response_tx, response_rx) = oneshot::channel();
        self.pending_requests
            .write()
            .await
            .insert(request_id.to_string(), (target, response_tx));
        response_rx
    }

    /// Get pending_requests reference (for WebRtcGate to share)
    pub fn get_pending_requests(&self) -> PendingRequestsMap {
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

    async fn preflight_send(&self, target: &ActrId, dest: &Dest) -> ActorResult<SendAdmission> {
        // Admission is a pure, fail-fast read of authoritative state. It never
        // advances recovery, tears down transports, or waits for a guard: those
        // effects belong to the coordinator's event/deadline drivers.
        let admission = self.current_send_admission()?;

        if let Some(coordinator) = &self.webrtc_coordinator {
            if coordinator.cleanup_in_progress() {
                return Err(Self::send_projection_not_ready_error());
            }

            if let Some(status) = coordinator.peer_recovery_status(target).await {
                return Err(Self::connection_not_ready_error(&status));
            }
        }

        if self.closing_peers.read().await.contains(target)
            || self.transport_manager.is_closing(dest).await
        {
            return Err(Self::send_projection_not_ready_error());
        }

        // Policy may have tightened during the coordinator/session awaits.
        // Keep the strictest projection seen by this operation.
        self.tighten_with_current_projection(admission)
    }
}

impl PeerGate {
    /// Send data via transport with per-PayloadType retry on transient failures.
    ///
    /// Retry is applied only when `NetworkError::kind()` is `Transient`.
    /// Non-transient errors are returned immediately without retry.
    async fn send_with_retry(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
    ) -> ActorResult<()> {
        let policy = payload_type.retry_policy();
        self.send_with_retry_policy(dest, payload_type, data, policy, admission)
            .await
    }

    async fn send_with_retry_record_identity(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
    ) -> ActorResult<DestTransportRef> {
        let policy = payload_type.retry_policy();
        self.send_with_retry_policy_record_identity(dest, payload_type, data, policy, admission)
            .await
    }

    async fn send_with_retry_policy(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        policy: crate::transport::RetryPolicy,
        admission: SendAdmission,
    ) -> ActorResult<()> {
        self.send_with_retry_policy_record_identity(dest, payload_type, data, policy, admission)
            .await
            .map(|_| ())
    }

    async fn attempt_send(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
    ) -> NetworkResult<Option<DestTransportRef>> {
        match admission {
            SendAdmission::Normal => self
                .transport_manager
                .send_with_identity(dest, payload_type, data)
                .await
                .map(Some),
            SendAdmission::ExistingOnly => {
                self.transport_manager
                    .send_existing_with_identity(dest, payload_type, data)
                    .await
            }
        }
    }

    async fn send_once_with_admission(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        admission: SendAdmission,
    ) -> ActorResult<DestTransportRef> {
        let admission = self.tighten_with_current_projection(admission)?;
        match self.attempt_send(dest, payload_type, data, admission).await {
            Ok(Some(identity)) => Ok(identity),
            Ok(None) => Err(Self::send_projection_not_ready_error()),
            Err(error) => Err(error.into()),
        }
    }

    async fn send_with_retry_policy_record_identity(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        policy: crate::transport::RetryPolicy,
        mut admission: SendAdmission,
    ) -> ActorResult<DestTransportRef> {
        let mut delay = policy.initial_delay;
        let mut attempt = 0u32;

        loop {
            admission = self.tighten_with_current_projection(admission)?;
            match self.attempt_send(dest, payload_type, data, admission).await {
                Ok(Some(identity)) => return Ok(identity),
                // `ExistingOnly` with no committed destination is an
                // admission result, not a transient transport failure. Fail
                // immediately without entering the retry backoff.
                Ok(None) => return Err(Self::send_projection_not_ready_error()),
                Err(e) => {
                    attempt += 1;
                    let retryable = e.is_retryable();
                    let remaining = policy.max_attempts.saturating_sub(attempt);
                    if retryable && remaining > 0 {
                        tracing::warn!(
                            attempt,
                            remaining,
                            error = %e,
                            "transient send failure, retrying after {:?}",
                            delay,
                        );
                        crate::timer::sleep(crate::timer::ids::PEER_SEND_BACKOFF, delay).await;
                        // exponential backoff capped at max_delay
                        delay = (delay * 2).min(policy.max_delay);
                    } else {
                        if attempt > 1 {
                            tracing::warn!(
                                attempt,
                                error = %e,
                                retryable,
                                "send failed after {} attempt(s)",
                                attempt,
                            );
                        }
                        return Err(e.into());
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
            None => {
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
        self.send_with_retry_policy(&dest, payload_type, data, policy, admission)
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
    pub async fn send_request_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<Bytes> {
        let envelope = Self::stamp_envelope_direction(envelope, Direction::Request);

        // 0. Sender-side deadline validation (#254): reject non-positive
        // timeouts before registering pending state. This also prevents the
        // `as u64` sign wrap that would turn a negative timeout into an
        // effectively infinite one.
        let timeout = crate::transport::validate_rpc_timeout_ms(envelope.timeout_ms)?;

        // 1. Convert ActrId to Dest and fail fast during recovery before
        // registering pending_requests.
        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;

        // 2. Create oneshot channel for receiving response
        let (response_tx, response_rx) = oneshot::channel();

        // 3. Register pending request with target ActorId
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(envelope.request_id.clone(), (target.clone(), response_tx));
        }

        // 4. Serialize RpcEnvelope
        let data = Self::serialize_envelope(&envelope);

        // 5. Unified timeout: covers both retry + wait-for-response
        //    so the user-perceived latency never exceeds envelope.timeout_ms.
        let request_id = envelope.request_id.clone();
        let sent_transport = Arc::new(Mutex::new(None));
        let sent_transport_for_send = Arc::clone(&sent_transport);

        let result =
            crate::timer::timeout(crate::timer::ids::PEER_REQUEST_DEADLINE, timeout, async {
                // 5a. Send with per-PayloadType retry on transient failures
                let sent_identity = self
                    .send_with_retry_record_identity(&dest, payload_type, &data, admission)
                    .await?;
                *sent_transport_for_send.lock().await = Some(sent_identity);
                tracing::debug!("Sent request to {:?}", target);

                // 5b. Wait for response
                match response_rx.await {
                    Ok(result) => result,
                    Err(_) => Err(ActrError::Unavailable(
                        "Response channel closed".to_string(),
                    )),
                }
            })
            .await;

        match result {
            Ok(inner) => {
                if inner.is_err() {
                    // Send failed or channel closed — clean up pending request
                    self.pending_requests.write().await.remove(&request_id);
                } else {
                    tracing::debug!("Received response for request: {}", request_id);
                }
                inner
            }
            Err(_) => {
                // Deadline expired — return TimedOut (retryable) rather than
                // Unavailable so callers and language bindings can distinguish
                // "request never reached the server" from "server did not reply
                // in time".
                self.pending_requests.write().await.remove(&request_id);
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
        self.send_with_retry(
            &dest,
            PayloadType::RpcReliable,
            &data,
            SendAdmission::ExistingOnly,
        )
        .await
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

    pub(crate) async fn send_tell_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<()> {
        self.send_message_with_type_inner(target, payload_type, envelope, Some(TELL_SEND_TIMEOUT))
            .await
    }

    async fn send_message_with_type_inner(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
        send_timeout: Option<Duration>,
    ) -> ActorResult<()> {
        let envelope = Self::stamp_envelope_direction(envelope, Direction::Tell);
        let data = Self::serialize_envelope(&envelope);
        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;
        let send = self.send_with_retry(&dest, payload_type, &data, admission);
        if let Some(send_timeout) = send_timeout {
            match crate::timer::timeout(
                crate::timer::ids::PEER_REQUEST_DEADLINE,
                send_timeout,
                send,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(ActrError::TimedOut),
            }
        } else {
            send.await
        }
    }

    /// Send media sample via WebRTC native track
    ///
    /// # Parameters
    /// - `target`: Target Actor ID
    /// - `track_id`: Media track identifier
    /// - `sample`: Media sample data
    ///
    /// # Implementation Note
    /// Delegates to WebRtcCoordinator which manages WebRTC Tracks
    pub async fn send_media_sample(
        &self,
        target: &ActrId,
        track_id: &str,
        sample: MediaSample,
    ) -> ActorResult<()> {
        tracing::debug!(
            "PeerGate::send_media_sample to {:?}, track_id={}",
            target,
            track_id
        );

        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;

        // Sending a sample only uses an already-registered track, so
        // ExistingOnly remains valid. Re-read here so a projection that
        // tightened during preflight cannot admit a newly blocked send.
        self.tighten_with_current_projection(admission)?;

        // Check if WebRTC coordinator is available
        let coordinator = self.webrtc_coordinator.as_ref().ok_or_else(|| {
            ActrError::NotImplemented("MediaTrack requires WebRTC coordinator".to_string())
        })?;

        // Delegate to WebRtcCoordinator
        coordinator
            .send_media_sample(target, track_id, sample)
            .await
            .map_err(|e| ActrError::Unavailable(format!("WebRTC send failed: {e}")))?;

        tracing::debug!("Sent media sample to {:?}", target);
        Ok(())
    }

    /// Add a media track to the WebRTC connection with the target
    pub async fn add_media_track(
        &self,
        target: &ActrId,
        track_id: &str,
        codec: &str,
        media_type: &str,
    ) -> ActorResult<()> {
        tracing::debug!(
            "PeerGate::add_media_track to {:?}, track_id={}, codec={}, type={}",
            target,
            track_id,
            codec,
            media_type
        );

        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;

        // Adding a track can establish a connection and always renegotiates
        // SDP. ExistingOnly therefore cannot safely enter the coordinator.
        if self.tighten_with_current_projection(admission)? == SendAdmission::ExistingOnly {
            return Err(Self::send_projection_not_ready_error());
        }

        let coordinator = self.webrtc_coordinator.as_ref().ok_or_else(|| {
            ActrError::NotImplemented("MediaTrack requires WebRTC coordinator".to_string())
        })?;

        coordinator
            .add_dynamic_track(target, track_id.to_string(), codec, media_type)
            .await?;

        Ok(())
    }

    /// Remove a media track from the WebRTC connection with the target.
    pub async fn remove_media_track(&self, target: &ActrId, track_id: &str) -> ActorResult<()> {
        tracing::debug!(
            "PeerGate::remove_media_track to {:?}, track_id={}",
            target,
            track_id
        );

        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;

        // Removing a track triggers SDP renegotiation on an existing
        // connection. ExistingOnly permits traffic on committed sessions, but
        // must not start any new negotiation.
        if self.tighten_with_current_projection(admission)? == SendAdmission::ExistingOnly {
            return Err(Self::send_projection_not_ready_error());
        }

        let coordinator = self.webrtc_coordinator.as_ref().ok_or_else(|| {
            ActrError::NotImplemented("MediaTrack requires WebRTC coordinator".to_string())
        })?;

        coordinator.remove_dynamic_track(target, track_id).await?;
        Ok(())
    }

    /// Send DataChunk (Fast Path)
    ///
    /// # Parameters
    /// - `target`: Target Actor ID
    /// - `payload_type`: PayloadType (StreamReliable or StreamLatencyFirst)
    /// - `stream_id`: DataChunk identifier already known before serialization
    /// - `data`: Serialized DataChunk bytes
    ///
    /// # Implementation Note
    /// Sends via PeerTransport using WebRTC DataChannel or WebSocket
    pub async fn send_data_chunk(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        stream_id: &str,
        data: Bytes,
    ) -> ActorResult<()> {
        self.send_data_chunk_with_timeout(
            target,
            payload_type,
            stream_id,
            data,
            DATA_CHUNK_SEND_TIMEOUT,
        )
        .await
    }

    async fn send_data_chunk_with_timeout(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        stream_id: &str,
        data: Bytes,
        send_timeout: Duration,
    ) -> ActorResult<()> {
        tracing::debug!(
            "PeerGate::send_data_chunk to {:?}, stream_id={}, payload_type={:?}, size={} bytes",
            target,
            stream_id,
            payload_type,
            data.len()
        );

        ensure_stream_payload_type(payload_type)?;

        // // Check if target is being cleaned up
        // if self.closing_peers.read().await.contains(target) {
        //     return Err(ActrError::Unavailable(format!(
        //         "Connection to {} is closing",
        //         target.to_string_repr()
        //     )));
        // }

        // Convert ActrId to Dest
        let dest = Self::actr_id_to_dest(target);
        let admission = self.preflight_send(target, &dest).await?;

        // `ensure_stream_payload_type` above guarantees `payload_type` is a
        // stream variant, so every call tracks an active DataChunk unconditionally.
        let stream_session_id = if let Some(coordinator) = &self.webrtc_coordinator {
            coordinator.get_peer_session_id(target).await
        } else {
            None
        };

        let recorded_before_send = if let Some(session_id) = stream_session_id {
            self.record_active_data_chunk_if_needed(target, stream_id, session_id, Instant::now())
                .await
        } else {
            false
        };

        let result = match crate::timer::timeout(
            crate::timer::ids::PEER_REQUEST_DEADLINE,
            send_timeout,
            self.send_once_with_admission(&dest, payload_type, &data, admission),
        )
        .await
        {
            Ok(Ok(_identity)) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ActrError::TimedOut),
        };

        if result.is_err() {
            if recorded_before_send && let Some(session_id) = stream_session_id {
                self.active_data_chunks
                    .write()
                    .await
                    .remove_stream_session(target, stream_id, session_id);
            }
        } else if stream_session_id.is_none()
            && let Some(coordinator) = &self.webrtc_coordinator
        {
            if let Some(session_id) = coordinator.get_peer_session_id(target).await {
                self.record_active_data_chunk_if_needed(
                    target,
                    stream_id,
                    session_id,
                    Instant::now(),
                )
                .await;
            }
        }

        result
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
        let pending_requests = Arc::new(RwLock::new(HashMap::new()));
        let (projection_tx, projection_rx) = watch::channel(projection);
        let gate = PeerGate::with_pending_requests_and_send_projection(
            transport.clone(),
            None,
            pending_requests,
            projection_rx,
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
        assert!(gate.get_pending_requests().read().await.is_empty());
        assert_eq!(builds.load(Ordering::SeqCst), 0);
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

    #[tokio::test]
    async fn existing_only_allows_media_samples_but_rejects_track_negotiation() {
        let (gate, _transport, _projection_tx, builds, sends) =
            gate_with_projection(SendProjection::ExistingOnly);
        let target = test_actor_id(2);

        let sample_error = gate
            .send_media_sample(&target, "audio", test_media_sample())
            .await
            .expect_err("admitted sample should reach the missing coordinator");
        assert!(matches!(sample_error, ActrError::NotImplemented(_)));

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

    #[test]
    fn connection_not_ready_window_matches_the_recovery_deadline() {
        let now = tokio::time::Instant::now();
        let status = NetworkRecoveryStatus::new_for_test(7, "test recovery", now);
        let ActrError::ConnectionNotReady(info) = PeerGate::connection_not_ready_error(&status)
        else {
            panic!("recovery admission must return ConnectionNotReady");
        };
        assert!(
            info.retry_after_ms
                .is_some_and(|retry_after_ms| retry_after_ms <= status.timeout_ms())
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_message_with_type_times_out_before_reliable_retry_backoff_completes() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(RetryFailingWireBuilder {
                attempts: attempts.clone(),
            }),
        ));
        let gate = PeerGate::new(transport, None);
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

        assert!(matches!(err, ActrError::TimedOut), "got {err:?}");
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
    async fn send_data_chunk_timeout_is_timed_out() {
        let transport = Arc::new(PeerTransport::new(
            test_actor_id(1),
            Arc::new(PendingWireBuilder),
        ));
        let gate = PeerGate::new(transport, None);
        let target = test_actor_id(2);

        let err = gate
            .send_data_chunk_with_timeout(
                &target,
                PayloadType::StreamReliable,
                "stream-1",
                Bytes::from_static(b"chunk"),
                Duration::from_millis(250),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ActrError::TimedOut), "got {err:?}");
    }
}
