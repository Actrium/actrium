//! WireBuilder - Wire layer component builder
//!
//! Provides default Wire component builder implementation, supporting:
//! - WebRTC P2P connections (through the RTC manager)
//! - WebSocket transport connections
//! - CancellationToken for terminating in-progress connection creation

use super::Dest; // Re-exported from actr-framework
use super::error::{NetworkError, NetworkResult};
use super::lane::DataLane;
use super::peer_transport::WireBuilder;
use super::wire_handle::{WireHandle, WireIdentity};
use super::wire_pool::ConnType;
use crate::lifecycle::CredentialState;
use crate::lifecycle::session_state::SessionState;
use crate::outbound::PendingRequestsMap;
use crate::wire::websocket::WebSocketConnection;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, Direction, PayloadType, RpcEnvelope};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Default Wire builder configuration
pub struct DefaultWireBuilderConfig {
    /// Local node identity as hex-encoded protobuf `ActrId` bytes, sent in the `X-Actr-Source-ID` header during outbound WebSocket handshakes.
    pub local_id_hex: String,

    /// Enable WebSocket
    pub enable_websocket: bool,

    /// Shared map of discovered WebSocket direct-connect URLs, keyed by ActrId.
    ///
    /// Populated by discovery flow after receiving ws_address info
    /// from the signaling server.  When a connection to an ActrId is needed and this map
    /// contains an entry for it, the stored URL is used instead of the url_template.
    pub discovered_ws_addresses: Arc<RwLock<HashMap<ActrId, String>>>,

    /// Optional local credential state. During outbound WebSocket handshakes the current credential is base64-encoded and sent in the `X-Actr-Credential` header so the peer can verify the Ed25519 signature.
    pub credential_state: Option<CredentialState>,

    /// Unified session state. When present, outbound WebSocket handshakes read
    /// source identity and credential from the current snapshot.
    pub session_state: Option<SessionState>,

    /// Shared pending-requests map.
    ///
    /// When set, outbound WebSocket connections spawn reader tasks that deliver
    /// server responses (and error envelopes) to the waiting `send_request` calls.
    /// Must be the same map as `PeerGate.pending_requests`.
    pub(crate) pending_requests: Option<PendingRequestsMap>,
}

impl Default for DefaultWireBuilderConfig {
    fn default() -> Self {
        Self {
            local_id_hex: String::new(),
            enable_websocket: true,
            discovered_ws_addresses: Arc::new(RwLock::new(HashMap::new())),
            credential_state: None,
            session_state: None,
            pending_requests: None,
        }
    }
}

/// Default builder for wire-layer connections.
///
/// Creates WebRTC and/or WebSocket wire handles from configuration and supports attempting multiple connection types during the same creation pass.
pub struct DefaultWireBuilder {
    /// RTC manager used to create peer wire handles when present.
    rtc_manager: Option<Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>>,

    /// Local node identity hex string used as `X-Actr-Source-ID` in outbound WebSocket handshakes.
    local_id_hex: String,

    /// Unified session state used for dynamic direct WebSocket identity.
    session_state: Option<SessionState>,

    /// Shared map of discovered WebSocket URLs (from signaling discovery)
    discovered_ws_addresses: Arc<RwLock<HashMap<ActrId, String>>>,

    /// Local credential state used to provide `X-Actr-Credential` during outbound WebSocket handshakes.
    credential_state: Option<CredentialState>,

    /// Pending requests map (shared with PeerGate) for outbound WebSocket response routing.
    pending_requests: Option<PendingRequestsMap>,

    /// Monotonic generation counter for outbound WebSocket wire identities.
    ///
    /// Each newly built `ClientWebSocketHandle` draws the next value so that a
    /// stale close event for a replaced wire can never match the fresh wire's
    /// `WireIdentity::WebSocket`. Strictly increasing within the builder, which
    /// is sufficient for the compare-and-swap in `mark_connection_closed_if_same`.
    ws_session_counter: Arc<AtomicU64>,

    /// Builder configuration.
    config: DefaultWireBuilderConfig,
}

impl DefaultWireBuilder {
    /// Create a new wire builder.
    ///
    /// # Arguments
    /// - `config`: builder configuration
    pub fn new(config: DefaultWireBuilderConfig) -> Self {
        Self {
            rtc_manager: None,
            local_id_hex: config.local_id_hex.clone(),
            session_state: config.session_state.clone(),
            discovered_ws_addresses: config.discovered_ws_addresses.clone(),
            credential_state: config.credential_state.clone(),
            pending_requests: config.pending_requests.clone(),
            ws_session_counter: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// Set the RTC manager used to create and adopt peer connections.
    pub(crate) fn with_rtc_manager(
        mut self,
        manager: Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>,
    ) -> Self {
        self.rtc_manager = Some(manager);
        self
    }

    /// Look up the direct WebSocket URL for the target node, sourced only from service discovery.
    async fn resolve_websocket_url(&self, dest: &Dest) -> Option<String> {
        if let Dest::Peer(actor_id) = dest {
            let map = self.discovered_ws_addresses.read().await;
            if let Some(url) = map.get(actor_id) {
                tracing::debug!(
                    "🔎 [Factory] Using discovered WebSocket URL for {}: {}",
                    actor_id,
                    url
                );
                return Some(url.clone());
            }
        }
        None
    }
}

// ── ClientWebSocketHandle ─────────────────────────────────────────────────────

/// `WireHandle` wrapper around an outbound `WebSocketConnection` that, after
/// a successful `connect()`, spawns lane-reader tasks routing incoming
/// `RpcEnvelope` responses back to the shared `pending_requests` map.
///
/// Without this, the server-side `WebSocketGate.send_response()` would write
/// the response onto the TCP connection but nobody on the client side would
/// consume it from the dispatcher lanes and deliver it to the waiting
/// `send_request_with_type` future.
struct ClientWebSocketHandle {
    inner: WebSocketConnection,
    pending_requests: PendingRequestsMap,
    /// Peer identity for `WireIdentity::WebSocket`.
    peer_id: ActrId,
    /// Monotonic generation distinguishing this wire session from any prior or
    /// replacement wire to the same peer.
    session_id: u64,
}

impl std::fmt::Debug for ClientWebSocketHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientWebSocketHandle")
            .field("peer_id", &self.peer_id)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl ClientWebSocketHandle {
    fn new(
        inner: WebSocketConnection,
        pending_requests: PendingRequestsMap,
        peer_id: ActrId,
        session_id: u64,
    ) -> Self {
        Self {
            inner,
            pending_requests,
            peer_id,
            session_id,
        }
    }

    /// Spawn a task that reads `RpcReliable` and `RpcSignal` lanes on the
    /// outbound connection and routes every received envelope to
    /// `pending_requests`.
    ///
    /// Responses are identified by `request_id`: if a matching entry exists
    /// in `pending_requests`, the oneshot sender is fired.  Unknown
    /// `request_id` values are dropped with a debug log (they arrive before
    /// the sender registers, which should not happen in practice).
    fn spawn_response_readers(&self) {
        for pt in [PayloadType::RpcReliable, PayloadType::RpcSignal] {
            let conn = self.inner.clone();
            let pending = self.pending_requests.clone();

            tokio::spawn(async move {
                let lane = match conn.get_lane(pt).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("ClientWebSocketHandle: get_lane({pt:?}) failed: {e:?}");
                        return;
                    }
                };

                tracing::debug!("ClientWebSocketHandle: response reader started for {pt:?}");

                loop {
                    let data = match lane.recv().await {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::info!("ClientWebSocketHandle: lane {pt:?} closed: {e:?}");
                            break;
                        }
                    };

                    match RpcEnvelope::decode(&data[..]) {
                        Ok(envelope) => {
                            if !ClientWebSocketHandle::is_response_envelope(&envelope, pt) {
                                continue;
                            }

                            let request_id = envelope.request_id.clone();
                            let outcome = pending.complete_from_envelope(envelope, || {
                                "outbound-websocket".to_string()
                            });
                            if outcome == crate::transport::correlation::CompleteOutcome::Orphan {
                                tracing::debug!(
                                    request_id = %request_id,
                                    "ClientWebSocketHandle: no pending request for incoming envelope, dropping"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "ClientWebSocketHandle: RpcEnvelope decode failed: {e:?}"
                            );
                        }
                    }
                }

                tracing::debug!("ClientWebSocketHandle: response reader exited for {pt:?}");
            });
        }
    }

    fn is_response_envelope(envelope: &RpcEnvelope, payload_type: PayloadType) -> bool {
        match envelope.direction {
            Some(raw) => match Direction::try_from(raw) {
                Ok(Direction::Response) => true,
                Ok(Direction::Request | Direction::Tell) => {
                    tracing::warn!(
                        request_id = %envelope.request_id,
                        route_key = %envelope.route_key,
                        payload_type = ?payload_type,
                        direction = raw,
                        "ClientWebSocketHandle: request/tell-labeled envelope received on response reader, dropping"
                    );
                    false
                }
                Ok(Direction::Unspecified) => {
                    tracing::warn!(
                        request_id = %envelope.request_id,
                        route_key = %envelope.route_key,
                        payload_type = ?payload_type,
                        direction = raw,
                        "ClientWebSocketHandle: RpcEnvelope.direction is Unspecified on response reader, dropping"
                    );
                    false
                }
                Err(_) => {
                    tracing::warn!(
                        request_id = %envelope.request_id,
                        route_key = %envelope.route_key,
                        payload_type = ?payload_type,
                        direction = raw,
                        "ClientWebSocketHandle: unknown RpcEnvelope.direction on response reader, dropping"
                    );
                    false
                }
            },
            None => {
                tracing::warn!(
                    request_id = %envelope.request_id,
                    route_key = %envelope.route_key,
                    payload_type = ?payload_type,
                    "ClientWebSocketHandle: missing RpcEnvelope.direction on response reader, dropping"
                );
                false
            }
        }
    }
}

#[async_trait]
impl WireHandle for ClientWebSocketHandle {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        self.inner.priority()
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.inner.connect().await?;
        self.spawn_response_readers();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    async fn close(&self) -> NetworkResult<()> {
        self.inner.close().await
    }

    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        self.inner.get_lane(payload_type).await
    }

    fn identity(&self) -> Option<WireIdentity> {
        Some(WireIdentity::WebSocket {
            peer_id: self.peer_id.clone(),
            session_id: self.session_id,
        })
    }
}

#[async_trait]
impl WireBuilder for DefaultWireBuilder {
    #[cfg_attr(feature = "opentelemetry", tracing::instrument(skip_all))]
    async fn create_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        // Delegate to method with no cancel token
        self.create_connections_with_cancel(dest, None).await
    }

    async fn existing_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        // RTC controllers own the physical inbound/outbound PeerConnection
        // independently of PeerTransport. ExistingOnly response traffic may
        // materialize a local DestTransport around that already-sendable
        // Session; it must not dial or negotiate a new connection.
        if let Some(manager) = self.rtc_manager.as_ref()
            && let Some(peer_id) = dest.as_peer_id()
            && let Some(controller) = manager.get_controller(peer_id).await
            && controller.snapshot().sendable
        {
            let handle = crate::wire::webrtc::rtc::wire_handle::RtcWireHandle::from_existing(
                manager.clone(),
                peer_id.clone(),
                controller,
            );
            return Ok(vec![Arc::new(handle) as Arc<dyn WireHandle>]);
        }
        Ok(Vec::new())
    }

    #[cfg_attr(feature = "opentelemetry", tracing::instrument(skip_all))]
    async fn create_connections_with_cancel(
        &self,
        dest: &Dest,
        cancel_token: Option<CancellationToken>,
    ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        let mut connections: Vec<Arc<dyn WireHandle>> = Vec::new();

        // Helper to check cancellation
        let check_cancelled = |token: &Option<CancellationToken>| -> NetworkResult<()> {
            if let Some(t) = token {
                if t.is_cancelled() {
                    return Err(NetworkError::ConnectionClosed(
                        "Connection creation cancelled".to_string(),
                    ));
                }
            }
            Ok(())
        };

        // 1. Check whether the operation was already cancelled.
        check_cancelled(&cancel_token)?;

        // 2. Try to establish a WebSocket connection.
        // The URL comes from service discovery (`discovered_ws_addresses`). If nothing was discovered, skip WebSocket for this attempt.
        if self.config.enable_websocket {
            check_cancelled(&cancel_token)?;

            if let Some(url) = self.resolve_websocket_url(dest).await {
                tracing::debug!("🏭 [Factory] Create WebSocket Connect: {}", url);
                let (local_id_hex, session_credential) =
                    if let Some(session_state) = &self.session_state {
                        let snapshot = session_state.snapshot().await;
                        (
                            hex::encode(snapshot.actor_id.encode_to_vec()),
                            Some(snapshot.credential),
                        )
                    } else {
                        (self.local_id_hex.clone(), None)
                    };
                let mut ws_conn = WebSocketConnection::new(url).with_local_id(local_id_hex);

                // Attach the local credential so the peer `WebSocketGate` can verify the Ed25519 signature.
                if let Some(credential) = session_credential {
                    let cred_bytes = credential.encode_to_vec();
                    use base64::Engine as _;
                    let cred_b64 = base64::engine::general_purpose::STANDARD.encode(&cred_bytes);
                    ws_conn = ws_conn.with_credential_b64(cred_b64);
                } else if let Some(ref cred_state) = self.credential_state {
                    let credential = cred_state.credential().await;
                    let cred_bytes = credential.encode_to_vec();
                    use base64::Engine as _;
                    let cred_b64 = base64::engine::general_purpose::STANDARD.encode(&cred_bytes);
                    ws_conn = ws_conn.with_credential_b64(cred_b64);
                }

                // Wrap the connection so that, after `connect()`, response reader
                // tasks are spawned to deliver server responses to `pending_requests`.
                if let Some(ref pending) = self.pending_requests {
                    let peer_id = dest.as_peer_id().cloned().ok_or_else(|| {
                        NetworkError::InvalidArgument(
                            "ClientWebSocketHandle requires a peer destination".to_string(),
                        )
                    })?;
                    // The session id makes the wire's identity unique so
                    // passive-close self-heal can distinguish the current wire
                    // from a replaced one.
                    let session_id = self.ws_session_counter.fetch_add(1, Ordering::Relaxed);
                    connections.push(Arc::new(ClientWebSocketHandle::new(
                        ws_conn,
                        pending.clone(),
                        peer_id,
                        session_id,
                    )) as Arc<dyn WireHandle>);
                } else {
                    connections.push(Arc::new(ws_conn) as Arc<dyn WireHandle>);
                }
            } else {
                tracing::debug!(
                    "🔎 [Factory] No WebSocket URL available for {:?}, skipping WS connection",
                    dest
                );
            }
        }

        // 3. Check cancellation before trying WebRTC.
        check_cancelled(&cancel_token)?;

        // 4. Attempt to create a WebRTC connection.
        if let Some(ref rtc_manager) = self.rtc_manager {
            if let Some(target_id) = dest.as_peer_id() {
                tracing::debug!("[Factory] Creating WebRTC connection to: {:?}", dest);
                check_cancelled(&cancel_token)?;
                let handle = crate::wire::webrtc::rtc::wire_handle::RtcWireHandle::new(
                    rtc_manager.clone(),
                    target_id.clone(),
                );
                // Hand the unconnected candidate to WirePool. The pool is
                // the connection retry owner (three attempts by default);
                // pre-connecting here consumed only one attempt and then
                // discarded the handle before the pool could retry with a
                // fresh Controller transport/PC generation.
                connections.push(Arc::new(handle) as Arc<dyn WireHandle>);
            }
            return Ok(connections);
        }

        tracing::info!(
            "✨ [Factory] Finished creating {} connections for {:?}",
            connections.len(),
            dest,
        );

        Ok(connections)
    }
}

#[cfg(test)]
#[path = "wire_builder_tests.rs"]
mod tests;
