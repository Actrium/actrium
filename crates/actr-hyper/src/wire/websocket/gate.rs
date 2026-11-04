//! WebSocketGate - message router for inbound WebSocket connections
//!
//! Installs virtual connections from the outbound relay client after peer authentication,
//! then routes messages by PayloadType to Mailbox or DataChunkRegistry.
//! The optional credential verifier also supports isolated authentication fixtures.
//!
//! # Design comparison with RTC inbound dispatch
//!
//! | Concern | RTC inbound dispatch | WebSocketGate |
//! |---------|-----------|---------------|
//! | Transport | WebRTC DataChannel | WebSocket (TCP) |
//! | Sender auth | loran signaling verifies credential | Local Ed25519 verification (IssuerKeyCache) |
//! | Message aggregation | Session lane pumps → `InboundRouter` | Per-connection `DataLane` reading |

use super::connection::WebSocketConnection;
use crate::inbound::DataChunkRegistry;
use crate::key_cache::IssuerKeyCache;
use crate::lifecycle::CredentialState;
use crate::lifecycle::node::PeerTypes;
use crate::transport::WsSink;
use crate::transport::correlation::PendingRequestsMap;
use crate::transport::handover::HandoverBook;
use crate::transport::secure_channel::SecureCell;
use crate::wire::SignalingKeyFetcher;
use crate::wire::webrtc::SignalingClient;
use crate::wire::webrtc::{HookCallback, HookEvent};
use crate::wire::{DirectionError, direction_for_routing};
use actr_framework::Bytes;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::transport_control;
use actr_protocol::{
    AIdCredential, ActrId, ActrType, DataChunk, Direction, IdentityClaims, PayloadType, RpcEnvelope,
};
use actr_protocol::{ActorResult, ActrError};
use actr_runtime_mailbox::{Mailbox, MessagePriority};
use ed25519_dalek::{Signature, Verifier as Ed25519Verifier};
use futures_util::SinkExt;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;

pub(crate) type InboundWsConn = (WebSocketConnection, Vec<u8>, Option<AIdCredential>);

/// WebSocket authentication context (optional)
///
/// When configured, gate will perform Ed25519 credential verification for each inbound connection:
/// - Connections that fail verification are dropped without starting lane readers
/// - Connections without credentials are treated as verification failures
pub(crate) struct WsAuthContext {
    /// Issuer signing public key cache (local hit verifies directly, miss fetches via signaling)
    pub(crate) issuer_key_cache: Arc<IssuerKeyCache>,
    /// Local ActrId (needed when requesting public key from signaling on cache miss)
    pub(crate) actor_id: ActrId,
    /// Local credential state (needed for signaling authentication on cache miss)
    pub(crate) credential_state: CredentialState,
    /// Signaling client (used to fetch public key on cache miss)
    pub(crate) signaling_client: Arc<dyn SignalingClient>,
    /// Verified peer Actor types; a verified connection records the
    /// `actor_type` from its credential claims here while it is open.
    pub(crate) peer_types: PeerTypes,
}

/// Registry type: inbound peer ActrId → (write-half, secure cell) of the
/// WebSocket connection.
///
/// Populated when a peer connects inbound (after credential verification).
/// Used by `send_response` to route server-to-client responses back over the
/// same connection the client used to send the request — avoiding the need for
/// a reverse WebRTC/WebSocket dial which would fail in test environments.
/// The cell travels with the sink so this raw-sink send path is gated on the
/// same secure-session state as the wrapped `SecureDataLane`s (RFC-0563).
type InboundSinkMap = Arc<RwLock<HashMap<ActrId, (WsSink, SecureCell)>>>;

/// WebSocketGate - receives and routes inbound WebSocket messages
pub(crate) struct WebSocketGate {
    handover: OnceLock<Arc<HandoverBook>>,
    /// Inbound connection channel (taken once and moved into background task)
    conn_rx: tokio::sync::Mutex<Option<mpsc::Receiver<InboundWsConn>>>,

    /// Pending requests map (request_id -> (caller_id, oneshot::Sender))
    /// **Shared with PeerGate** for correct Response routing
    pending_requests: PendingRequestsMap,

    /// DataChunk registry (fast-path stream message routing)
    data_chunk_registry: Arc<DataChunkRegistry>,

    /// Inbound connection authentication context
    auth_ctx: Option<Arc<WsAuthContext>>,

    /// Hook callback for WebSocket peer lifecycle events
    /// (`WebSocketConnectStart` / `Connected` / `Disconnected`).
    hook_callback: OnceLock<HookCallback>,

    /// Inbound peer sinks: peer ActrId → WsSink.
    ///
    /// Populated when a verified peer establishes an inbound connection;
    /// cleaned up on disconnect.  Used by `send_response` to route replies
    /// back over the same inbound WebSocket connection.
    inbound_sinks: InboundSinkMap,
    relay_mailbox: OnceLock<Arc<dyn Mailbox>>,
    mailbox_ready: tokio::sync::Notify,
    relay_mode: bool,
    relay_peer_types: PeerTypes,
}

impl WebSocketGate {
    /// Create WebSocketGate
    ///
    /// # Arguments
    /// - `conn_rx`: incoming virtual relay connections
    /// - `pending_requests`: pending requests map shared with PeerGate
    /// - `data_chunk_registry`: DataChunk registry
    /// - `auth_ctx`: authentication context (when configured, enforces credential verification on all inbound connections)
    pub fn new(
        conn_rx: mpsc::Receiver<InboundWsConn>,
        pending_requests: PendingRequestsMap,
        data_chunk_registry: Arc<DataChunkRegistry>,
        auth_ctx: Option<WsAuthContext>,
    ) -> Self {
        Self {
            handover: OnceLock::new(),
            conn_rx: tokio::sync::Mutex::new(Some(conn_rx)),
            pending_requests,
            data_chunk_registry,
            auth_ctx: auth_ctx.map(Arc::new),
            hook_callback: OnceLock::new(),
            inbound_sinks: Arc::new(RwLock::new(HashMap::new())),
            relay_mailbox: OnceLock::new(),
            mailbox_ready: tokio::sync::Notify::new(),
            relay_mode: false,
            relay_peer_types: PeerTypes::default(),
        }
    }

    pub(crate) fn set_handover(&self, book: Arc<HandoverBook>) {
        let _ = self.handover.set(book);
    }

    /// Authenticated relay ingress. Only the encrypted adapter attaches connections;
    /// peer identity is bound by Loran's credential authentication.
    pub(crate) fn new_relay(
        pending_requests: PendingRequestsMap,
        data_chunk_registry: Arc<DataChunkRegistry>,
        peer_types: PeerTypes,
    ) -> Self {
        let (_, receiver) = mpsc::channel(1);
        let mut gate = Self::new(receiver, pending_requests, data_chunk_registry, None);
        gate.relay_mode = true;
        gate.relay_peer_types = peer_types;
        gate
    }

    pub(crate) async fn attach_relay(
        &self,
        conn: WebSocketConnection,
        peer: ActrId,
        actor_type: ActrType,
    ) -> ActorResult<()> {
        if !self.relay_mode {
            return Err(ActrError::Internal("Relay ingress not configured".into()));
        }
        let mailbox = loop {
            let notified = self.mailbox_ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(mailbox) = self.relay_mailbox.get() {
                break mailbox.clone();
            }
            notified.await;
        };
        Self::spawn_connection_tasks(
            conn,
            peer.encode_to_vec(),
            actor_type,
            self.pending_requests.clone(),
            self.data_chunk_registry.clone(),
            mailbox,
            self.hook_callback.get().cloned(),
            self.inbound_sinks.clone(),
            false,
            self.handover.get().cloned(),
            self.relay_peer_types.clone(),
        )
        .await;
        Ok(())
    }

    pub(crate) fn relay_ready(&self, peer: ActrId) {
        if let Some(callback) = self.hook_callback.get().cloned() {
            tokio::spawn(async move {
                callback(HookEvent::WebSocketConnected { peer_id: peer }).await;
            });
        }
    }

    #[cfg(test)]
    pub(crate) async fn install_response_sink_for_test(&self, peer: ActrId, sink: WsSink) {
        self.inbound_sinks.write().await.insert(
            peer,
            (
                sink,
                SecureCell::new(crate::transport::SecureChannelState::Verified),
            ),
        );
    }

    /// Install the WebSocket peer-lifecycle hook callback.
    ///
    /// Idempotent: subsequent calls are silently ignored. Invoked once
    /// during node startup.
    pub fn set_hook_callback(&self, cb: HookCallback) {
        let _ = self.hook_callback.set(cb);
    }

    /// Send a response envelope back to a peer that connected to us inbound.
    ///
    /// Looks up the peer's write-half in `inbound_sinks`.  Returns `Ok(false)`
    /// (not an error) when there is no sendable inbound connection from
    /// `peer`, so the caller may try another transport. Returns `Ok(true)`
    /// on a successful send; an error may mean submission was attempted.
    pub async fn send_response(
        &self,
        peer: &ActrId,
        mut envelope: RpcEnvelope,
    ) -> ActorResult<bool> {
        let entry_opt = {
            let map = self.inbound_sinks.read().await;
            map.get(peer).cloned()
        };

        let (sink, secure) = match entry_opt {
            Some(entry) => entry,
            None => return Ok(false),
        };

        // Secure-session gate for this raw-sink send path: the frame below
        // bypasses the lane abstraction, so check the cell directly instead.
        // Terminal → drop the registration (the wire is gone). A live but
        // non-sendable session has not submitted bytes and can fall back.
        let state = secure.state();
        if state.is_terminal() {
            let mut sinks = self.inbound_sinks.write().await;
            if sinks
                .get(peer)
                .is_some_and(|(current, _)| Arc::ptr_eq(current, &sink))
            {
                sinks.remove(peer);
            }
            return Ok(false);
        }
        if !state.is_sendable() {
            return Ok(false);
        }

        envelope.direction = Some(Direction::Response as i32);

        // Serialize envelope
        let mut payload_buf = Vec::new();
        envelope.encode(&mut payload_buf).map_err(|e| {
            ActrError::Internal(format!("WebSocketGate: encode response failed: {e}"))
        })?;

        // Frame: [payload_type: 1 byte][len: 4 bytes BE][data: N bytes]
        let pt = PayloadType::RpcReliable as u8;
        let len = payload_buf.len() as u32;
        let mut frame = Vec::with_capacity(5 + payload_buf.len());
        frame.push(pt);
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&payload_buf);

        let mut write_started = false;
        let write = async {
            let mut sink_guard = sink.lock().await;
            match sink_guard.as_mut() {
                Some(s) => {
                    // The session may have stopped being sendable while the
                    // shared writer was busy. No bytes have been attempted.
                    if !secure.state().is_sendable() {
                        return Ok(false);
                    }
                    write_started = true;
                    s.send(WsMessage::Binary(frame.into())).await.map_err(|e| {
                        ActrError::Unavailable(format!("WebSocketGate: send_response failed: {e}"))
                    })?;
                    tracing::debug!(
                        peer = %peer,
                        request_id = %envelope.request_id,
                        "WebSocketGate: response sent via inbound connection"
                    );
                    Ok(true)
                }
                None => {
                    tracing::warn!(peer = %peer, "WebSocketGate: inbound sink gone, cannot send response");
                    Ok(false)
                }
            }
        };
        let result = tokio::select! {
            biased;
            _ = secure.wait_terminal() => None,
            result = write => Some(result),
        };
        match result {
            Some(result) => result,
            None if write_started => Err(ActrError::Unavailable(
                "WebSocket session retired during response send".into(),
            )),
            None => Ok(false),
        }
    }

    /// Handle RpcEnvelope: explicit Response wakes the waiting party,
    /// explicit Request or Tell enqueues into Mailbox (the dispatch loop
    /// suppresses the reply for Tell). Missing, Unspecified, or unknown
    /// direction values are invalid and dropped.
    async fn handle_envelope(
        envelope: RpcEnvelope,
        from_bytes: Vec<u8>,
        from_type: Option<ActrType>,
        data: Bytes,
        payload_type: PayloadType,
        pending_requests: PendingRequestsMap,
        mailbox: Arc<dyn Mailbox>,
    ) {
        let request_id = envelope.request_id.clone();

        let direction = match direction_for_routing(envelope.direction) {
            Ok(direction) => direction,
            Err(error) => {
                // Drop path only: decode the peer lazily for diagnostics.
                let peer = Self::peer_for_log(&from_bytes);
                let reason = match error {
                    DirectionError::Missing => "missing",
                    DirectionError::Unspecified => "unspecified",
                    DirectionError::Unknown => "unknown",
                };
                tracing::warn!(
                    request_id = %request_id,
                    peer = %peer,
                    route_key = %envelope.route_key,
                    direction = ?envelope.direction,
                    reason = %reason,
                    "rpc.invalid_direction_dropped: invalid RpcEnvelope.direction; dropping"
                );
                return;
            }
        };

        if matches!(direction, Direction::Request | Direction::Tell) {
            Self::enqueue_request(
                from_bytes,
                from_type,
                data,
                payload_type,
                mailbox,
                &request_id,
            )
            .await;
            return;
        }

        // Direction::Response: complete the waiting caller via the shared
        // correlation map (orphan late replies warn and drop).
        pending_requests.complete_from_envelope(envelope, || Self::peer_for_log(&from_bytes));
    }
    fn peer_for_log(from_bytes: &[u8]) -> String {
        if from_bytes.is_empty() {
            return "unavailable".to_string();
        }

        ActrId::decode(from_bytes)
            .map(|peer| peer.to_string_repr())
            .unwrap_or_else(|e| format!("decode_failed:{e}"))
    }

    /// Enqueue an explicit inbound RPC request or tell to the Mailbox.
    async fn enqueue_request(
        from_bytes: Vec<u8>,
        from_type: Option<ActrType>,
        data: Bytes,
        payload_type: PayloadType,
        mailbox: Arc<dyn Mailbox>,
        request_id: &str,
    ) {
        tracing::debug!("📥 WS Received RPC Request: request_id={}", request_id);

        let priority = match payload_type {
            PayloadType::RpcSignal => MessagePriority::High,
            _ => MessagePriority::Normal,
        };

        let from_type = from_type.map(|actor_type| actor_type.encode_to_vec());
        match mailbox
            .enqueue(from_bytes, from_type, data.to_vec(), priority)
            .await
        {
            Ok(msg_id) => {
                tracing::debug!(
                    "✅ WS RPC message enqueued: msg_id={}, priority={:?}",
                    msg_id,
                    priority
                );
            }
            Err(e) => {
                tracing::error!("❌ WS Mailbox enqueue failed: {:?}", e);
            }
        }
    }

    async fn finish_lane_task(
        active_lanes: Arc<std::sync::atomic::AtomicUsize>,
        peer_id_for_lane: Option<ActrId>,
        hook_cb_for_lane: Option<HookCallback>,
        sinks_for_lane: InboundSinkMap,
        expected_sink: WsSink,
        peer_types: PeerTypes,
    ) {
        let remaining = active_lanes
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_sub(1);
        if remaining == 0 {
            if let Some(ref peer) = peer_id_for_lane {
                let mut sinks = sinks_for_lane.write().await;
                peer_types.connection_closed(peer);
                if !sinks
                    .get(peer)
                    .is_some_and(|(sink, _)| Arc::ptr_eq(sink, &expected_sink))
                {
                    return;
                }
                sinks.remove(peer);
                tracing::debug!(peer = %peer, "WS inbound sink removed (all lanes closed)");
            }
            if let (Some(peer), Some(cb)) = (peer_id_for_lane, hook_cb_for_lane) {
                cb(HookEvent::WebSocketDisconnected { peer_id: peer }).await;
            }
        }
    }

    /// Verify AIdCredential Ed25519 signature of an inbound connection
    ///
    /// Returns the verified Actor type of the peer, `None` on failure (already logged).
    /// `source_id_bytes` is the ActrId protobuf bytes from `X-Actr-Source-ID`.
    async fn verify_credential(
        credential: &AIdCredential,
        source_id_bytes: &[u8],
        auth_ctx: &WsAuthContext,
    ) -> Option<ActrType> {
        // Get local credential (for signaling authentication on cache miss)
        let local_credential = auth_ctx.credential_state.credential().await;

        // Construct SignalingKeyFetcher adapter, wrapping signaling client as KeyFetcher
        let fetcher = SignalingKeyFetcher {
            client: auth_ctx.signaling_client.clone(),
            actor_id: auth_ctx.actor_id.clone(),
            credential: local_credential,
        };

        // Get verifying key for key_id from IssuerKeyCache (local hit or fetch from signaling)
        let verifying_key = match auth_ctx
            .issuer_key_cache
            .get_or_fetch(credential.key_id, &fetcher)
            .await
        {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    key_id = credential.key_id,
                    error = ?e,
                    "WS credential verification failed: unable to get signing key"
                );
                return None;
            }
        };

        // Ed25519 signature verification
        let sig_result =
            credential.signature[..]
                .try_into()
                .ok()
                .and_then(|sig_bytes: [u8; 64]| {
                    let signature = Signature::from_bytes(&sig_bytes);
                    verifying_key
                        .verify(&credential.claims[..], &signature)
                        .ok()
                });
        if sig_result.is_none() {
            tracing::warn!(
                key_id = credential.key_id,
                "WS AIdCredential Ed25519 verification failed"
            );
            return None;
        }

        // Decode IdentityClaims
        let claims = match IdentityClaims::decode(&credential.claims[..]) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(key_id = credential.key_id, error = ?e, "WS IdentityClaims proto decode failed");
                return None;
            }
        };

        // Issuer TTLs are configurable and claims do not carry an issued-at
        // value, so only the signed absolute expiry can be evaluated here.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if claims.expires_at <= now {
            tracing::warn!(
                key_id = credential.key_id,
                expires_at = claims.expires_at,
                "WS AIdCredential has expired"
            );
            return None;
        }

        // Verify claims.actor_id matches X-Actr-Source-ID (prevent identity claim mismatch)
        match ActrId::decode(source_id_bytes) {
            Ok(source_actor_id) => {
                let source_repr = source_actor_id.to_string_repr();
                if claims.actor_id != source_repr {
                    tracing::warn!(
                        claimed = %claims.actor_id,
                        source_id = %source_repr,
                        "WS credential actor_id does not match X-Actr-Source-ID, rejecting connection"
                    );
                    return None;
                }
                // The credential's Realm and issuer are both the source's Realm name.
                if claims.realm != source_actor_id.realm || claims.issuer != source_actor_id.realm {
                    tracing::warn!(
                        realm = %claims.realm,
                        issuer = %claims.issuer,
                        source_id = %source_repr,
                        "WS credential Realm or issuer does not match the source Realm, rejecting connection"
                    );
                    return None;
                }
                tracing::info!(
                    actor_id = %claims.actor_id,
                    "WS inbound connection identity verification passed"
                );
                Some(claims.actor_type)
            }
            Err(e) => {
                tracing::warn!(error = ?e, "WS X-Actr-Source-ID decode failed, rejecting connection");
                None
            }
        }
    }

    /// Spawn receive tasks for a single WebSocket connection
    ///
    /// Reads `PayloadType::RpcReliable`, `RpcSignal`, `StreamReliable`,
    /// `StreamLatencyFirst` -- four lanes total, spawning an independent task for each.
    ///
    /// `inbound_sinks`: shared map into which the connection's write-half is
    /// inserted keyed by the peer ActrId.  Removed when all lane tasks exit.
    ///
    /// The sink is registered synchronously (before any lane task is spawned)
    /// so that `send_response` can find it even when the first request arrives
    /// via a parallel transport (e.g. WebRTC) before the lanes are fully up.
    // Each task owns independent receive lanes and shared routing state.
    #[allow(clippy::too_many_arguments)]
    async fn spawn_connection_tasks(
        conn: WebSocketConnection,
        source_id: Vec<u8>,
        actor_type: ActrType,
        pending_requests: PendingRequestsMap,
        data_chunk_registry: Arc<DataChunkRegistry>,
        mailbox: Arc<dyn Mailbox>,
        hook_callback: Option<HookCallback>,
        inbound_sinks: InboundSinkMap,
        emit_connected: bool,
        handover: Option<Arc<HandoverBook>>,
        peer_types: PeerTypes,
    ) {
        // Fire `WebSocketConnected` for the peer once the connection is
        // accepted. Decoding the peer `ActrId` may fail if the source-id
        // header is malformed — in that case we skip hooks but still run
        // the lane readers so that the connection can fail-fast on its
        // own terms.
        let peer_id = ActrId::decode(&source_id[..]).ok();
        if let (true, Some(peer), Some(cb)) =
            (emit_connected, peer_id.clone(), hook_callback.clone())
        {
            let cb_for_connected = cb.clone();
            tokio::spawn(async move {
                cb_for_connected(HookEvent::WebSocketConnected { peer_id: peer }).await;
            });
        }

        // Register the write-half so `send_response` can route replies back.
        // Done here (synchronously, before any lane-reader is spawned) so that
        // the first RPC response after connection establishment can always find
        // the sink even when the request arrives via a different transport
        // (e.g. WebRTC) that is already in use.
        if let Some(ref peer) = peer_id {
            let sink = conn.sink();
            let secure = conn.secure_cell();
            inbound_sinks
                .write()
                .await
                .insert(peer.clone(), (sink, secure));
            // Counted until the last lane exits (`finish_lane_task`).
            peer_types.record(peer.clone(), actor_type.clone());
            peer_types.connection_opened(peer);
        }

        // Count active per-lane reader tasks. When the last one exits
        // we fire `WebSocketDisconnected` exactly once (and remove the sink).
        let active_lanes = Arc::new(std::sync::atomic::AtomicUsize::new(4));
        // Spawn per-PayloadType receive tasks
        for pt in [
            PayloadType::RpcReliable,
            PayloadType::RpcSignal,
            PayloadType::StreamReliable,
            PayloadType::StreamLatencyFirst,
        ] {
            let conn_clone = conn.clone();
            let src = source_id.clone();
            let pending = pending_requests.clone();
            let registry = data_chunk_registry.clone();
            let mb = mailbox.clone();
            let active_lanes = active_lanes.clone();
            let peer_id_for_lane = peer_id.clone();
            let hook_cb_for_lane = hook_callback.clone();
            let sinks_for_lane = inbound_sinks.clone();
            let expected_sink = conn.sink();
            let handover = handover.clone();
            // Register every route before acknowledging the relay channel.
            let lane_result = conn_clone.get_lane(pt).await;
            let peer_types_for_lane = peer_types.clone();
            let actor_type = actor_type.clone();

            tokio::spawn(async move {
                // get_lane lazily creates the mpsc channel and registers in router
                let lane = match lane_result {
                    Ok(l) => Some(l),
                    Err(e) => {
                        tracing::error!("❌ WS get_lane({:?}) failed: {:?}", pt, e);
                        None
                    }
                };

                if let Some(lane) = lane {
                    tracing::debug!("📡 WS lane reader started for {:?}", pt);

                    loop {
                        match lane.recv().await {
                            Ok(data) => {
                                let data_bytes = data.clone();

                                match pt {
                                    PayloadType::RpcReliable | PayloadType::RpcSignal => {
                                        match RpcEnvelope::decode(&data[..]) {
                                            Ok(envelope) => {
                                                if pt == PayloadType::RpcSignal
                                                    && let Some(nonce) =
                                                        transport_control::acknowledgement_nonce(
                                                            &envelope,
                                                        )
                                                {
                                                    if let (Some(book), Ok(peer)) = (
                                                        handover.as_ref(),
                                                        ActrId::decode(src.as_slice()),
                                                    ) {
                                                        book.acknowledge(&peer, nonce);
                                                    }
                                                    continue;
                                                }
                                                Self::handle_envelope(
                                                    envelope,
                                                    src.clone(),
                                                    Some(actor_type.clone()),
                                                    data_bytes,
                                                    pt,
                                                    pending.clone(),
                                                    mb.clone(),
                                                )
                                                .await;
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "❌ WS Failed to decode RpcEnvelope: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    PayloadType::StreamReliable
                                    | PayloadType::StreamLatencyFirst => {
                                        match DataChunk::decode(&data[..]) {
                                            Ok(chunk) => {
                                                tracing::debug!(
                                                    "📦 WS Received DataChunk: stream_id={}, seq={}",
                                                    chunk.stream_id,
                                                    chunk.sequence,
                                                );
                                                match ActrId::decode(&src[..]) {
                                                    Ok(sender_id) => {
                                                        if pt == PayloadType::StreamReliable
                                                            && let Some(nonce) =
                                                                transport_control::barrier_nonce(
                                                                    &chunk,
                                                                )
                                                        {
                                                            if let Some(book) = handover.as_ref() {
                                                                book.receive_barrier(
                                                                    sender_id, nonce,
                                                                )
                                                                .await;
                                                            }
                                                            continue;
                                                        }
                                                        // Per-lane task: a full StreamReliable
                                                        // queue blocks only this stream lane, not
                                                        // the RPC lanes (unlike the WebRTC single
                                                        // receive loop).
                                                        registry
                                                            .dispatch(chunk, sender_id, pt)
                                                            .await;
                                                    }
                                                    Err(e) => {
                                                        tracing::error!(
                                                            "❌ WS Failed to decode sender ActrId: {:?}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "❌ WS Failed to decode DataChunk: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    PayloadType::MediaRtp => {
                                        tracing::warn!(
                                            "⚠️ MediaRtp received in WebSocketGate (unexpected)"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::info!("🔌 WS lane {:?} closed: {:?}", pt, e);
                                break;
                            }
                        }
                    }

                    tracing::debug!("📡 WS lane reader exited for {:?}", pt);
                }

                // Last lane out fires the disconnected hook exactly once
                // and removes the inbound sink from the registry.
                Self::finish_lane_task(
                    active_lanes,
                    peer_id_for_lane,
                    hook_cb_for_lane,
                    sinks_for_lane,
                    expected_sink,
                    peer_types_for_lane,
                )
                .await;
            });
        }
    }

    /// Start the connection accept loop (called by ActrNode, can only be called once)
    ///
    /// Internally takes `conn_rx` and moves it into a background task for lock-free new connection reception.
    /// If `auth_ctx` is configured, performs credential verification for each inbound connection;
    /// drops connections on verification failure, calls `spawn_connection_tasks` on success.
    pub async fn start_receive_loop(&self, mailbox: Arc<dyn Mailbox>) -> ActorResult<()> {
        if self.relay_mode {
            self.relay_mailbox
                .set(mailbox)
                .map_err(|_| ActrError::Internal("Relay receive loop already started".into()))?;
            self.mailbox_ready.notify_waiters();
            return Ok(());
        }
        let rx = self.conn_rx.lock().await.take().ok_or_else(|| {
            ActrError::Internal("WebSocketGate: start_receive_loop already called".to_string())
        })?;

        let pending_requests = self.pending_requests.clone();
        let data_chunk_registry = self.data_chunk_registry.clone();
        let auth_ctx = self.auth_ctx.clone();
        let hook_cb = self.hook_callback.get().cloned();
        let inbound_sinks = self.inbound_sinks.clone();
        let handover = self.handover.get().cloned();

        tokio::spawn(async move {
            tracing::info!("🚀 WebSocketGate receive loop started");

            let mut rx = rx;
            while let Some((conn, source_id, credential_opt)) = rx.recv().await {
                tracing::info!(
                    "🔗 WS new inbound connection (source_id len={}, has_credential={})",
                    source_id.len(),
                    credential_opt.is_some()
                );

                // Fire `WebSocketConnectStart` as soon as we observe an
                // inbound connection, before verification — this mirrors
                // how the WebRTC path emits `WebRtcConnectStart` before
                // the selected ICE candidate pair is known.
                if let (Some(cb), Ok(peer)) = (hook_cb.clone(), ActrId::decode(&source_id[..])) {
                    let peer_clone = peer.clone();
                    tokio::spawn(async move {
                        cb(HookEvent::WebSocketConnectStart {
                            peer_id: peer_clone,
                        })
                        .await;
                    });
                }

                // Credential verification (if auth_ctx is configured)
                if let Some(ref ctx) = auth_ctx {
                    let actor_type = match credential_opt {
                        Some(ref credential) => {
                            match Self::verify_credential(credential, &source_id, ctx).await {
                                Some(actor_type) => actor_type,
                                None => {
                                    tracing::warn!(
                                        "WS inbound connection credential verification failed, dropping connection"
                                    );
                                    // Close the accepted socket and its receive
                                    // loop explicitly when authentication fails.
                                    let _ = conn.close().await;
                                    continue;
                                }
                            }
                        }
                        None => {
                            tracing::warn!(
                                "WS inbound connection missing X-Actr-Credential, rejecting connection (auth_ctx configured)"
                            );
                            let _ = conn.close().await;
                            continue;
                        }
                    };

                    Self::spawn_connection_tasks(
                        conn,
                        source_id,
                        actor_type,
                        pending_requests.clone(),
                        data_chunk_registry.clone(),
                        mailbox.clone(),
                        hook_cb.clone(),
                        inbound_sinks.clone(),
                        true,
                        handover.clone(),
                        ctx.peer_types.clone(),
                    )
                    .await;
                } else {
                    tracing::error!(
                        "WS auth_ctx not configured, rejecting connection (configuration error)"
                    );
                    let _ = conn.close().await;
                }
            }

            tracing::info!("🔌 WebSocketGate receive loop exited");
        });

        Ok(())
    }
}
#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
