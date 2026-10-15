//! WebSocket transport connection implementation

use super::security::DirectSecurity;
use crate::transport::{
    ConnType, DataLane, NetworkError, NetworkResult, SecureCell, SecureChannelState,
    SecureDataLane, WebSocketDataLane, WireHandle, WsSink,
};
use actr_protocol::PayloadType;
use async_trait::async_trait;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, RwLock, mpsc, watch};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request as WsRequest;
use tokio_tungstenite::tungstenite::http::Uri as WsUri;
use tokio_tungstenite::{Connector, WebSocketStream, connect_async_tls_with_config};
use tokio_util::sync::CancellationToken;

/// Cancels the dispatcher task when the last `WebSocketConnection` handle is
/// dropped, so a connection dropped without `close()` (e.g. a credential
/// rejection in the gate) cannot leave the socket and read loop alive.
struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

const PING_INTERVAL: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
const PEER_BACKPRESSURE_TIMEOUT: Duration = Duration::from_secs(120);
type ProbeNonce = [u8; 16];
// Bound one continuous local receive stall independently of successful writes.
// A peer may still ACK TCP data while its application is unresponsive. Even a
// healthy session is retired if its consumer cannot make progress for 2 minutes.
const RECEIVE_BACKPRESSURE_TIMEOUT: Duration = Duration::from_secs(120);
// A peer whose bounded application queue is full cannot read a normal Ping.
// Repeated control frames prove it is still alive without buffering more data.
// This is a lease refreshed by each frame, never a permanent watchdog disable.
const RECEIVE_BACKPRESSURE_PING: &[u8] = b"actr:receive-backpressure:v1";

/// WebSocket transmitting messagesprotocol
///
/// Forin single WebSocket Connect for multiple route reuse different Type'smessage.
///
/// ## Message format
///
/// ```text
/// [payload_type: 1 byte][data_len: 4 bytes][data: N bytes]
/// ```
/// Type alias for lane cache array (PayloadType index → cached DataLane)
type LaneCache<const N: usize> = Arc<RwLock<[Option<Arc<dyn DataLane>>; N]>>;

#[derive(Debug, Clone)]
struct TransportMessage {
    payload_type: PayloadType,
    data: Vec<u8>,
}

impl TransportMessage {
    /// frombytes stream decode
    fn decode(data: &[u8]) -> NetworkResult<Self> {
        if data.len() < 5 {
            return Err(NetworkError::DeserializationError(
                "WebSocket message too short".to_string(),
            ));
        }

        // Parse payload_type (must match proto enum values)
        let payload_type_raw = data[0];
        let payload_type = match payload_type_raw {
            0 => PayloadType::RpcReliable,
            1 => PayloadType::RpcSignal,
            2 => PayloadType::StreamReliable,
            3 => PayloadType::StreamLatencyFirst,
            4 => PayloadType::MediaRtp,
            _ => {
                return Err(NetworkError::DeserializationError(format!(
                    "Invalid payload_type: {payload_type_raw}"
                )));
            }
        };

        // Parse length
        let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;

        // Parse data
        if data.len() < 5 + len {
            return Err(NetworkError::DeserializationError(
                "WebSocket message data incomplete".to_string(),
            ));
        }

        let msg_data = data[5..5 + len].to_vec();

        Ok(Self {
            payload_type,
            data: msg_data,
        })
    }
}

/// WebSocket transport connection
#[derive(Clone)]
pub(crate) struct WebSocketConnection {
    /// URL
    url: String,
    security: DirectSecurity,
    /// Local node identity (hex-encoded protobuf ActrId bytes), sent as X-Actr-Source-ID in handshake request for direct-connect mode
    local_id_hex: Option<String>,
    /// Local AIdCredential (base64-encoded), sent with X-Actr-Credential header during handshake for peer verification
    credential_b64: Option<String>,
    /// Write end (Sink) - using Option to avoid initialization issues
    sink: WsSink,
    connect_gate: Arc<Mutex<()>>,
    /// Retain each stall's start even when watch coalesces clear/full updates.
    receive_stall: watch::Sender<Option<Instant>>,
    peer_backpressure: watch::Sender<Option<Instant>>,

    /// message route by table: PayloadType -> Sender (using array index, 5 fixed elements, Bytes zero-copy)
    router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>>,

    /// Lane Cache: PayloadType -> Lane (using array index, 5 fixed elements)
    lane_cache: LaneCache<5>,

    /// connection status
    connected: Arc<RwLock<bool>>,

    /// Secure-channel state owned by this physical WebSocket session (RFC-0563).
    /// Phase 2 constructs `LegacyBypass`; phase 3 drives the real handshake.
    secure: SecureCell,

    /// Shared teardown signal: `close()` cancels it directly, and the guard
    /// cancels it when the last handle drops. The dispatcher holds only the
    /// token, so its own lifetime never keeps the connection open.
    teardown: Arc<CancelOnDrop>,
}

impl WebSocketConnection {
    /// Connectto WebSocket service device
    ///
    /// # Arguments
    /// - `url`: WebSocket URL (ws:// or wss://)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = WebSocketConnection::new("ws://localhost:8080");
    /// conn.connect().await?;
    /// ```
    pub fn new(url: String) -> Self {
        Self {
            url: url.clone(),
            security: DirectSecurity::default(),
            local_id_hex: None,
            credential_b64: None,
            connect_gate: Arc::new(Mutex::new(())),
            receive_stall: watch::channel(None).0,
            peer_backpressure: watch::channel(None).0,
            sink: Arc::new(Mutex::new(None)), // initial begin as None
            router: Arc::new(RwLock::new([None, None, None, None, None])),
            lane_cache: Arc::new(RwLock::new([None, None, None, None, None])),
            connected: Arc::new(RwLock::new(false)),
            secure: SecureCell::new(SecureChannelState::LegacyBypass),
            teardown: Arc::new(CancelOnDrop(CancellationToken::new())),
        }
    }

    pub(crate) fn with_security(mut self, security: DirectSecurity) -> Self {
        self.security = security;
        self
    }

    /// Set local node identity; automatically appends X-Actr-Source-ID header during handshake (used in direct-connect mode)
    pub fn with_local_id(mut self, id_hex: String) -> Self {
        self.local_id_hex = Some(id_hex);
        self
    }

    /// Set local node AIdCredential (base64-encoded), sent with X-Actr-Credential header during handshake
    pub fn with_credential_b64(mut self, credential_b64: String) -> Self {
        self.credential_b64 = Some(credential_b64);
        self
    }

    /// Create connection from an inbound WebSocket stream with completed
    /// handshake (used for direct-connect ingress).
    ///
    /// Unlike `new()` + `connect()`, this method is for already-accepted
    /// inbound connections where the handshake has been completed by
    /// `WebSocketServer`, entering Ready state directly.
    ///
    /// The generic stream accepts either server TLS or explicit loopback development TCP.
    pub fn from_server_stream<S>(ws_stream: WebSocketStream<S>) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (sink, stream) = ws_stream.split();

        let sink: WsSink = Arc::new(Mutex::new(Some(Box::new(sink))));
        let secure = SecureCell::new(SecureChannelState::LegacyBypass);
        let mut routes = [None, None, None, None, None];
        let mut lanes = [None, None, None, None, None];
        // The peer can send its first frame as soon as the upgrade completes,
        // before credential verification and the gate's lane readers finish.
        // Register bounded receive queues before starting the dispatcher.
        for payload_type in [
            PayloadType::RpcReliable,
            PayloadType::RpcSignal,
            PayloadType::StreamReliable,
            PayloadType::StreamLatencyFirst,
        ] {
            let (tx, lane) = Self::make_lane(&sink, &secure, payload_type);
            routes[payload_type as usize] = Some(tx);
            lanes[payload_type as usize] = Some(lane);
        }
        let router = Arc::new(RwLock::new(routes));
        let connected = Arc::new(RwLock::new(true));
        let teardown = Arc::new(CancelOnDrop(CancellationToken::new()));

        tracing::info!("✅ WebSocketConnection created from server stream (already connected)");

        let connection = Self {
            url: String::from("<inbound>"),
            security: DirectSecurity::default(),
            local_id_hex: None,
            credential_b64: None,
            connect_gate: Arc::new(Mutex::new(())),
            receive_stall: watch::channel(None).0,
            peer_backpressure: watch::channel(None).0,
            sink,
            router,
            lane_cache: Arc::new(RwLock::new(lanes)),
            connected,
            secure,
            teardown,
        };
        connection.spawn_session(stream);
        connection
    }

    /// establish Connect
    pub async fn connect(&self) -> NetworkResult<()> {
        // One object owns one physical session. Redial uses a fresh object,
        // so an old watchdog can never retire a replacement socket.
        let _connect = self.connect_gate.lock().await;
        if self.secure.state().is_terminal() {
            return Err(NetworkError::WebSocketClosed(
                "WebSocket session retired".into(),
            ));
        }
        if *self.connected.read().await {
            return Ok(());
        }
        self.security.validate_url(&self.url)?;
        // Dial the same normalized authority that the policy validated. In
        // particular, never pass alternate numeric hosts to a DNS resolver.
        let url = actr_config::Url::parse(&self.url)
            .map_err(|_| NetworkError::InvalidArgument("invalid direct WebSocket URL".into()))?;
        let is_tls = url.scheme() == "wss";
        let url = url.to_string();
        let connector = match self.security.client.clone() {
            Some(client) => Some(Connector::Rustls(client)),
            None if is_tls => {
                // Fail closed: never substitute a default public-roots policy
                // for the operator's. The node's `DirectSecurity` must be
                // attached via `with_security`.
                return Err(NetworkError::ConfigurationError(
                    "direct WebSocket wss:// connection has no TLS client roots configured".into(),
                ));
            }
            None => None,
        };
        // 1. Establish WebSocket connection (direct-connect mode carries X-Actr-Source-ID header)
        let (ws_stream, _) = if let Some(ref hex_id) = self.local_id_hex {
            // tungstenite does not auto-complete WebSocket upgrade headers; all must be specified manually.
            // Missing any of (Host/Connection/Upgrade/Sec-WebSocket-Version/Sec-WebSocket-Key)
            // will cause the handshake to fail.
            let uri: WsUri = url
                .parse()
                .map_err(|e| NetworkError::ConnectionError(format!("Invalid WS URI: {e}")))?;
            let host = uri
                .host()
                .ok_or_else(|| NetworkError::ConnectionError("WS URL missing host".to_string()))?;
            let host_header = match uri.port_u16() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
            let mut builder = WsRequest::builder()
                .uri(url.as_str())
                .header("Host", host_header)
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", generate_key())
                .header("X-Actr-Source-ID", hex_id);

            if let Some(ref cred_b64) = self.credential_b64 {
                builder = builder.header("X-Actr-Credential", cred_b64.as_str());
            }

            let request = builder.body(()).map_err(|e| {
                NetworkError::ConnectionError(format!("WS request build failed: {e}"))
            })?;
            connect_async_tls_with_config(request, None, false, connector).await
        } else {
            connect_async_tls_with_config(&url, None, false, connector).await
        }
        .map_err(|e| Self::classify_connect_error(e, &url))?;
        let (sink, stream) = ws_stream.split();

        // 2. update new sink
        {
            let mut slot = self.sink.lock().await;
            if self.secure.state().is_terminal() {
                return Err(NetworkError::WebSocketClosed(
                    "WebSocket closed during connect".into(),
                ));
            }
            *slot = Some(Box::new(sink));
        }
        *self.connected.write().await = true;

        // 3. Startmessage dispatch device (in background task, not retain handle)
        self.spawn_session(stream);

        tracing::info!("✅ WebSocketConnection already Connect: {}", self.url);

        Ok(())
    }

    /// Preserve the shared tungstenite classification (closed-like eviction,
    /// typed HTTP statuses) while keeping the direct-transport context that
    /// distinguishes these failures from signaling handshake errors.
    fn classify_connect_error(
        err: tokio_tungstenite::tungstenite::Error,
        url: &str,
    ) -> NetworkError {
        match NetworkError::from(err) {
            NetworkError::WebSocketClosed(m) => {
                NetworkError::WebSocketClosed(format!("direct WebSocket {url}: {m}"))
            }
            NetworkError::WebSocketError(m) => {
                NetworkError::WebSocketError(format!("direct WebSocket {url}: {m}"))
            }
            // Typed handshake failures (401/403/503) keep their verdict
            // semantics untouched.
            typed => typed,
        }
    }

    /// Run liveness and receive work without retaining the connection's drop guard.
    fn spawn_session<S>(&self, stream: SplitStream<WebSocketStream<S>>)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let session = WebSocketSession {
            sink: self.sink.clone(),
            router: self.router.clone(),
            lane_cache: self.lane_cache.clone(),
            connected: self.connected.clone(),
            secure: self.secure.clone(),
            receive_stall: self.receive_stall.clone(),
            peer_backpressure: self.peer_backpressure.clone(),
        };
        let shutdown = self.teardown.0.clone();
        tokio::spawn(async move {
            let (pong_tx, pong_rx) = watch::channel(None);
            let graceful = tokio::select! {
                _ = shutdown.cancelled() => true,
                graceful = session.dispatch(stream, pong_tx) => graceful,
                _ = session.probe(pong_rx) => false,
                _ = session.secure.wait_terminal() => session.secure.state() == SecureChannelState::Closed,
            };
            session.secure.drive_terminal(if graceful {
                SecureChannelState::Closed
            } else {
                SecureChannelState::Failed
            });
            *session.connected.write().await = false;
            // Terminal state cancels application writers. Orderly shutdown
            // attempts the Close frame, but never waits indefinitely to flush.
            retire_sink(
                &session.sink,
                session.secure.state() == SecureChannelState::Closed,
            )
            .await;
            *session.router.write().await = [None, None, None, None, None];
            *session.lane_cache.write().await = [None, None, None, None, None];
        });
    }

    /// Register PayloadType route by
    async fn register_route(
        &self,
        payload_type: PayloadType,
        tx: mpsc::Sender<bytes::Bytes>,
    ) -> NetworkResult<()> {
        let mut router = self.router.write().await;
        let idx = payload_type as usize;
        router[idx] = Some(tx);
        tracing::debug!("✅ Register WebSocket route by : {:?}", payload_type);
        Ok(())
    }
}

impl WebSocketConnection {
    /// Return a clone of the shared write sink for this connection.
    ///
    /// Used by `WebSocketGate` to send responses back over an inbound
    /// connection without going through the lane/router abstraction.
    pub(crate) fn sink(&self) -> WsSink {
        self.sink.clone()
    }

    /// Return the session's secure cell so direct-sink senders
    /// (`WebSocketGate::send_response`) can gate on the same state as the
    /// wrapped `SecureDataLane`s.
    pub(crate) fn secure_cell(&self) -> SecureCell {
        self.secure.clone()
    }

    /// Get or create DataLane (with caching)
    pub async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        self.get_lane_internal(payload_type).await
    }

    /// Internal implementation of get_lane
    async fn get_lane_internal(
        &self,
        payload_type: PayloadType,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        let idx = payload_type as usize;

        // 1. Check cache
        {
            let cache = self.lane_cache.read().await;
            if let Some(lane) = &cache[idx] {
                tracing::debug!("Reuse cached DataLane: {:?}", payload_type);
                return Ok(Arc::clone(lane));
            }
        }

        // 2. Create new DataLane
        let lane = self.create_lane_internal(payload_type).await?;

        // 3. Cache
        {
            let mut cache = self.lane_cache.write().await;
            cache[idx] = Some(Arc::clone(&lane));
        }

        tracing::info!(
            "WebSocketConnection created new DataLane: {:?}",
            payload_type
        );

        Ok(lane)
    }

    /// Internal: Create DataLane (without cache)
    async fn create_lane_internal(
        &self,
        payload_type: PayloadType,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        if payload_type == PayloadType::MediaRtp {
            return Err(NetworkError::NoRoute(
                "MediaRtp is not supported by direct WebSocket connections".into(),
            ));
        }
        // Check connection status
        if !*self.connected.read().await {
            return Err(NetworkError::WebSocketClosed(
                "WebSocket connection closed".to_string(),
            ));
        }

        let (tx, lane) = Self::make_lane(&self.sink, &self.secure, payload_type);
        self.register_route(payload_type, tx).await?;
        Ok(lane)
    }

    // Keep cached lanes gated by the physical session's shared secure state.
    fn make_lane(
        sink: &WsSink,
        secure: &SecureCell,
        payload_type: PayloadType,
    ) -> (mpsc::Sender<bytes::Bytes>, Arc<dyn DataLane>) {
        let (tx, rx) = mpsc::channel(100);
        let lane = Arc::new(SecureDataLane::new(
            Arc::new(
                WebSocketDataLane::new(sink.clone(), payload_type, rx).with_session(secure.clone()),
            ),
            secure.clone(),
        ));
        (tx, lane)
    }

    /// Close connection
    pub async fn close(&self) -> NetworkResult<()> {
        // Close admission: drive the secure session terminal first, so every
        // path into this physical close (inherent call sites bypass the trait
        // method — RFC-0563 review point on shadowed terminal driving) marks
        // the session `Closed` and invalidates all wrapped `SecureDataLane`s.
        self.secure.drive_terminal(SecureChannelState::Closed);

        // Stop the dispatcher promptly instead of waiting for the peer's
        // Close echo; a non-cooperative peer must not keep the socket alive.
        self.teardown.0.cancel();

        *self.connected.write().await = false;

        retire_sink(
            &self.sink,
            self.secure.state() == SecureChannelState::Closed,
        )
        .await;

        // clear blank route by table
        let mut router = self.router.write().await;
        *router = [None, None, None, None, None];

        // clear blank Lane Cache
        let mut cache = self.lane_cache.write().await;
        *cache = [None, None, None, None, None];

        tracing::info!("WebSocketConnection closed");
        Ok(())
    }
}

#[async_trait]
impl WireHandle for WebSocketConnection {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        0
    }

    async fn connect(&self) -> NetworkResult<()> {
        // TODO(phase3): drive `LegacyBypass → New → Handshaking → Verified`
        // once the authenticated handshake lands. For now connect() returns
        // only when transport-ready (`LegacyBypass` is sendable).
        //
        // A single failed attempt does NOT drive terminal `Failed`: the
        // `WirePool` retries this same object, and a terminal state would make
        // a later successful attempt permanently unsendable. Failure is
        // declared only at retry exhaustion (`mark_secure_failed`).
        self.connect().await
    }

    fn is_connected(&self) -> bool {
        *self.connected.blocking_read()
    }

    async fn close(&self) -> NetworkResult<()> {
        // The inherent `Self::close` owns terminal driving (see above).
        Self::close(self).await
    }

    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        self.get_lane_internal(payload_type).await
    }

    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure.state()
    }

    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }

    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }

    fn mark_secure_failed(&self) {
        self.secure.drive_terminal(SecureChannelState::Failed);
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;

impl std::fmt::Debug for WebSocketConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never format the reusable credential or credential-bearing request headers.
        f.debug_struct("WebSocketConnection")
            .field("secure", &self.secure)
            .finish_non_exhaustive()
    }
}

/// Shared I/O state held by session tasks, excluding credentials and the drop guard.
struct WebSocketSession {
    sink: WsSink,
    router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>>,
    lane_cache: LaneCache<5>,
    connected: Arc<RwLock<bool>>,
    secure: SecureCell,
    receive_stall: watch::Sender<Option<Instant>>,
    peer_backpressure: watch::Sender<Option<Instant>>,
}

impl WebSocketSession {
    async fn probe(&self, mut pong_rx: watch::Receiver<Option<ProbeNonce>>) {
        let mut receive_stall = self.receive_stall.subscribe();
        let mut peer_backpressure = self.peer_backpressure.subscribe();
        let initial_pressure = *peer_backpressure.borrow_and_update();
        let mut peer_deadline = initial_pressure.map(|seen| seen + PEER_BACKPRESSURE_TIMEOUT);
        let mut peer_lease = initial_pressure.map(|seen| seen + PING_INTERVAL + PROBE_TIMEOUT);
        loop {
            if receive_stall.borrow_and_update().is_some() {
                let wait = self.wait_for_receive_capacity(&mut receive_stall);
                let result = if let Some(deadline) = peer_deadline {
                    crate::timer::timeout_at(
                        crate::timer::ids::WEBSOCKET_PEER_PRESSURE_DEADLINE,
                        deadline,
                        wait,
                    )
                    .await
                    .unwrap_or(Err(()))
                } else {
                    wait.await
                };
                if result.is_err() {
                    tracing::warn!("direct WebSocket receive pressure expired; retiring session");
                    return;
                }
                if peer_deadline.is_some() {
                    peer_lease = Some(Instant::now() + PING_INTERVAL + PROBE_TIMEOUT);
                }
                continue;
            }
            let nonce = *uuid::Uuid::new_v4().as_bytes();
            let cycle_deadline = Instant::now() + PING_INTERVAL + PROBE_TIMEOUT;
            let cycle = async {
                crate::timer::sleep(crate::timer::ids::WEBSOCKET_PING_SCHEDULE, PING_INTERVAL)
                    .await;
                {
                    let mut guard = self.sink.lock().await;
                    let sink = guard.as_mut().ok_or(())?;
                    sink.send(WsMessage::Ping(nonce.to_vec().into()))
                        .await
                        .map_err(|_| ())?;
                }
                pong_rx
                    .wait_for(|value| *value == Some(nonce))
                    .await
                    .map_err(|_| ())?;
                Ok::<(), ()>(())
            };
            tokio::pin!(cycle);
            loop {
                let (timer, deadline) = match peer_deadline {
                    Some(deadline) => (
                        crate::timer::ids::WEBSOCKET_PEER_PRESSURE_DEADLINE,
                        deadline.min(peer_lease.unwrap_or(deadline)),
                    ),
                    None => (
                        crate::timer::ids::WEBSOCKET_PROBE_CYCLE_DEADLINE,
                        cycle_deadline,
                    ),
                };
                tokio::select! {
                    biased;
                    // A stream of pressure frames cannot starve or renew this deadline.
                    _ = crate::timer::sleep_until(timer, deadline) => {
                        tracing::warn!("direct WebSocket liveness deadline expired; retiring session");
                        return;
                    },
                    result = &mut cycle => {
                        if result.is_err() { return; }
                        // Only the unpredictable challenge response proves the
                        // peer read this connection; pressure assertions do not.
                        peer_deadline = None;
                        peer_lease = None;
                        break;
                    },
                    result = receive_stall.changed() => {
                        if result.is_err() { return; }
                        break;
                    },
                    result = peer_backpressure.changed() => {
                        if result.is_err() { return; }
                        if peer_deadline.is_none() {
                            peer_deadline = Some(Instant::now() + PEER_BACKPRESSURE_TIMEOUT);
                        }
                        peer_lease = Some(Instant::now() + PING_INTERVAL + PROBE_TIMEOUT);
                        // Keep the current probe alive so a recovered peer can
                        // answer it; repeated heartbeats never restart the cycle.
                    },
                }
            }
        }
    }

    async fn wait_for_receive_capacity(
        &self,
        receive_stall: &mut watch::Receiver<Option<Instant>>,
    ) -> Result<(), ()> {
        loop {
            let Some(started) = *receive_stall.borrow_and_update() else {
                return Ok(());
            };
            // A successful receive followed immediately by another full queue
            // can coalesce into one watch update. Its timestamp still identifies
            // the new stall, so progress cannot inherit the previous deadline.
            let heartbeat = async {
                self.send_pressure_heartbeat().await?;
                crate::timer::sleep(crate::timer::ids::WEBSOCKET_PING_SCHEDULE, PING_INTERVAL)
                    .await;
                Ok::<(), ()>(())
            };
            tokio::select! {
                biased;
                result = receive_stall.changed() => result.map_err(|_| ())?,
                // Every heartbeat uses the same absolute deadline until the
                // receive queue accepts a message. Writes cannot renew it.
                result = crate::timer::timeout_at(
                    crate::timer::ids::WEBSOCKET_RECEIVE_BACKPRESSURE_DEADLINE,
                    started + RECEIVE_BACKPRESSURE_TIMEOUT,
                    heartbeat,
                ) => result.map_err(|_| ())??,
            }
        }
    }

    async fn send_pressure_heartbeat(&self) -> Result<(), ()> {
        let write = async {
            let mut guard = self.sink.lock().await;
            let sink = guard.as_mut().ok_or(())?;
            sink.send(WsMessage::Ping(RECEIVE_BACKPRESSURE_PING.to_vec().into()))
                .await
                .map_err(|_| ())
        };
        crate::timer::timeout(
            crate::timer::ids::WEBSOCKET_PROBE_DEADLINE,
            PROBE_TIMEOUT,
            write,
        )
        .await
        .map_err(|_| ())?
    }

    async fn dispatch<S>(
        &self,
        mut stream: SplitStream<WebSocketStream<S>>,
        pong_tx: watch::Sender<Option<ProbeNonce>>,
    ) -> bool
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        while let Some(msg_result) = stream.next().await {
            match msg_result {
                Ok(WsMessage::Binary(data)) => {
                    // decodemessage
                    match TransportMessage::decode(&data) {
                        Ok(transport_msg) => {
                            // Media uses its dedicated transport, not a WebSocket
                            // data lane. Never enqueue frames for an unread route.
                            if transport_msg.payload_type == PayloadType::MediaRtp {
                                tracing::debug!("dropping unsupported WebSocket MediaRtp frame");
                                continue;
                            }
                            // Route to corresponding 's Lane (using array index reference)
                            let idx = transport_msg.payload_type as usize;
                            let tx = self.router.read().await[idx].clone();
                            if let Some(tx) = tx {
                                // convert exchange as Bytes (zero-copy)
                                let data = bytes::Bytes::from(transport_msg.data);
                                let result = match tx.try_send(data) {
                                    Ok(()) => Ok(()),
                                    Err(mpsc::error::TrySendError::Full(data)) => {
                                        self.receive_stall.send_replace(Some(Instant::now()));
                                        let result = tx.send(data).await;
                                        self.receive_stall.send_replace(None);
                                        result
                                    }
                                    Err(mpsc::error::TrySendError::Closed(data)) => {
                                        Err(mpsc::error::SendError(data))
                                    }
                                };
                                if let Err(e) = result {
                                    tracing::warn!(
                                        "❌ WebSocket message route by failure (type={:?}): {}",
                                        transport_msg.payload_type,
                                        e
                                    );
                                }
                            } else {
                                tracing::warn!(
                                    "⚠️ WebSocket received not RegisterType'smessage: {:?}",
                                    transport_msg.payload_type
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("❌ WebSocket message decodefailure: {}", e);
                        }
                    }
                }
                Ok(WsMessage::Close(_)) => {
                    tracing::info!("WebSocket peer initiated orderly close");
                    return true;
                }
                Ok(WsMessage::Pong(payload)) => {
                    if let Ok(bytes) = ProbeNonce::try_from(payload.as_ref()) {
                        pong_tx.send_replace(Some(bytes));
                    }
                }
                // tungstenite queues Pong automatically; the next read
                // flushes it, including when no application response exists.
                Ok(WsMessage::Ping(payload)) => {
                    if payload.as_ref() == RECEIVE_BACKPRESSURE_PING {
                        self.peer_backpressure.send_replace(Some(Instant::now()));
                    }
                }
                Ok(_) => {
                    tracing::debug!("⚠️ Received non-binary WebSocket message, ignoring");
                }
                Err(e) => {
                    tracing::error!("❌ WebSocket Error: {}", e);
                    break;
                }
            }
        }

        tracing::debug!("📡 WebSocket dispatcher exited");
        false
    }
}

/// Only one closer takes ownership of the sink; cancellation drops that owned
/// socket even if an earlier frame or the Close frame cannot be flushed.
async fn retire_sink(sink: &WsSink, graceful: bool) {
    let close = async {
        let socket = sink.lock().await.take();
        if let Some(mut socket) = socket {
            if graceful {
                let _ = socket.close().await;
            }
        }
    };
    let _ = crate::timer::timeout(
        crate::timer::ids::WEBSOCKET_CLOSE_DEADLINE,
        CLOSE_TIMEOUT,
        close,
    )
    .await;
}
