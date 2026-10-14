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
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, RwLock, mpsc};
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
            PayloadType::MediaRtp,
        ] {
            let (tx, lane) = Self::make_lane(&sink, &secure, payload_type);
            routes[payload_type as usize] = Some(tx);
            lanes[payload_type as usize] = Some(lane);
        }
        let router = Arc::new(RwLock::new(routes));
        let connected = Arc::new(RwLock::new(true));
        let teardown = Arc::new(CancelOnDrop(CancellationToken::new()));

        Self::spawn_dispatcher(
            stream,
            router.clone(),
            connected.clone(),
            teardown.0.clone(),
        );

        tracing::info!("✅ WebSocketConnection created from server stream (already connected)");

        Self {
            url: String::from("<inbound>"),
            security: DirectSecurity::default(),
            local_id_hex: None,
            credential_b64: None,
            sink,
            router,
            lane_cache: Arc::new(RwLock::new(lanes)),
            connected,
            secure,
            teardown,
        }
    }

    /// establish Connect
    pub async fn connect(&self) -> NetworkResult<()> {
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
                    "direct WebSocket wss:// connection has no TLS client roots configured"
                        .into(),
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
        *self.sink.lock().await = Some(Box::new(sink));
        *self.connected.write().await = true;

        // 3. Startmessage dispatch device (in background task, not retain handle)
        let router = self.router.clone();
        let connected = self.connected.clone();
        Self::spawn_dispatcher(stream, router, connected, self.teardown.0.clone());

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

    /// Startmessage dispatch device (in background task)
    fn spawn_dispatcher<S>(
        mut stream: SplitStream<WebSocketStream<S>>,
        router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>>,
        connected: Arc<RwLock<bool>>,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            tracing::debug!("📡 WebSocket dispatcher Start");

            loop {
                let msg_result = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    msg_result = stream.next() => match msg_result {
                        Some(msg_result) => msg_result,
                        None => break,
                    },
                };
                match msg_result {
                    Ok(WsMessage::Binary(data)) => {
                        // decodemessage
                        match TransportMessage::decode(&data) {
                            Ok(transport_msg) => {
                                // Route to corresponding 's Lane (using array index reference)
                                let idx = transport_msg.payload_type as usize;
                                let router_guard = router.read().await;
                                if let Some(tx) = &router_guard[idx] {
                                    // convert exchange as Bytes (zero-copy)
                                    let data = bytes::Bytes::from(transport_msg.data);
                                    if let Err(e) = tx.send(data).await {
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
                        tracing::info!("🔌 WebSocket Connect be pair end Close");
                        *connected.write().await = false;
                        break;
                    }
                    Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {
                        // ignore center skipmessage
                    }
                    Ok(_) => {
                        tracing::debug!("⚠️ Received non-binary WebSocket message, ignoring");
                    }
                    Err(e) => {
                        tracing::error!("❌ WebSocket Error: {}", e);
                        *connected.write().await = false;
                        break;
                    }
                }
            }

            *connected.write().await = false;
            tracing::debug!("📡 WebSocket dispatcher rollback exit ");
        })
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
            Arc::new(WebSocketDataLane::new(sink.clone(), payload_type, rx)),
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

        // Close WebSocket (Send Close message)
        let mut sink_opt = self.sink.lock().await;
        if let Some(sink) = sink_opt.as_mut() {
            let _ = sink.close().await;
        }
        *sink_opt = None;

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
