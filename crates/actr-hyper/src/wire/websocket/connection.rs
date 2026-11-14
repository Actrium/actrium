//! WebSocket transport connection implementation

use crate::transport::{
    ConnType, DataLane, NetworkError, NetworkResult, SecureCell, SecureChannelState,
    SecureDataLane, VerifiedPeerContext, WebSocketDataLane, WireHandle, WsSink,
};
use actr_protocol::PayloadType;
use async_trait::async_trait;
use futures_util::{SinkExt, Stream, StreamExt};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_util::sync::CancellationToken;

/// Stop the dispatcher when the last connection handle is dropped. The
/// dispatcher holds only the token, so it cannot keep this guard alive.
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
    #[cfg(test)]
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
        if data.len() != 5 + len {
            return Err(NetworkError::DeserializationError(
                "WebSocket message data incomplete".to_string(),
            ));
        }

        Ok(Self {
            payload_type,
            #[cfg(test)]
            data: data[5..].to_vec(),
        })
    }
}

/// WebSocket transport connection
#[derive(Clone)]
pub(crate) struct WebSocketConnection {
    /// Write end (Sink) - using Option to avoid initialization issues
    sink: WsSink,

    /// message route by table: PayloadType -> Sender (using array index, 5 fixed elements, Bytes zero-copy)
    router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>>,

    /// Lane Cache: PayloadType -> Lane (using array index, 5 fixed elements)
    lane_cache: LaneCache<5>,

    /// connection status
    connected: Arc<AtomicBool>,

    /// Secure-channel state owned by this physical WebSocket session (RFC-0563).
    /// Production relay sessions enter `Verified` only after mutual Noise
    /// authentication and key confirmation; legacy bypass is test-only.
    secure: SecureCell,

    /// Shared by connection clones; cancellation also interrupts a full lane.
    teardown: Arc<CancelOnDrop>,

    /// Cancel an in-flight virtual relay write before acquiring the sink lock.
    relay_write_cancel: Option<CancellationToken>,
}

impl WebSocketConnection {
    /// Create a disconnected fixture; production channels come from RelayClient.
    #[cfg(test)]
    pub fn new(_url: String) -> Self {
        Self {
            sink: Arc::new(Mutex::new(None)), // initial begin as None
            router: Arc::new(RwLock::new([None, None, None, None, None])),
            lane_cache: Arc::new(RwLock::new([None, None, None, None, None])),
            connected: Arc::new(AtomicBool::new(false)),
            secure: SecureCell::new(SecureChannelState::LegacyBypass),
            teardown: Arc::new(CancelOnDrop(CancellationToken::new())),
            relay_write_cancel: None,
        }
    }

    /// Install a virtual connection multiplexed through an authenticated
    /// relay. The caller must install receive lanes before accepting a peer.
    #[cfg(test)]
    pub(crate) fn from_parts(
        sink: crate::transport::lane::WsWrite,
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
    ) -> Self {
        Self::from_parts_with_state(sink, stream, SecureChannelState::LegacyBypass, None)
    }

    pub(crate) fn from_encrypted_parts(
        sink: crate::transport::lane::WsWrite,
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
        write_cancel: CancellationToken,
    ) -> Self {
        Self::from_parts_with_state(
            sink,
            stream,
            SecureChannelState::Handshaking,
            Some(write_cancel),
        )
    }

    /// Install an Actrium Actor channel without publishing application I/O.
    /// Only `publish_actrium_verified` can make this strict cell sendable.
    pub(crate) fn from_actrium_encrypted_parts(
        sink: crate::transport::lane::WsWrite,
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
        write_cancel: CancellationToken,
    ) -> Self {
        Self::from_parts_with_secure_cell(
            sink,
            stream,
            SecureCell::new_actrium(SecureChannelState::Handshaking),
            Some(write_cancel),
        )
    }

    pub(crate) fn mark_relay_verified(&self) -> bool {
        self.secure.set_state_validated(
            SecureChannelState::Handshaking,
            SecureChannelState::Verified,
        )
    }

    pub(crate) fn publish_actrium_verified(&self, context: VerifiedPeerContext) -> bool {
        self.secure
            .publish_verified(SecureChannelState::Handshaking, context)
    }

    fn from_parts_with_state(
        sink: crate::transport::lane::WsWrite,
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
        initial: SecureChannelState,
        relay_write_cancel: Option<CancellationToken>,
    ) -> Self {
        Self::from_parts_with_secure_cell(
            sink,
            stream,
            SecureCell::new(initial),
            relay_write_cancel,
        )
    }

    fn from_parts_with_secure_cell(
        sink: crate::transport::lane::WsWrite,
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
        secure: SecureCell,
        relay_write_cancel: Option<CancellationToken>,
    ) -> Self {
        let router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>> =
            Arc::new(RwLock::new([None, None, None, None, None]));
        let connected = Arc::new(AtomicBool::new(true));

        let teardown = Arc::new(CancelOnDrop(CancellationToken::new()));
        Self::spawn_dispatcher(
            stream,
            router.clone(),
            connected.clone(),
            secure.clone(),
            teardown.0.clone(),
        );

        tracing::info!("✅ Virtual WebSocket connection installed");

        Self {
            sink: Arc::new(Mutex::new(Some(sink))),
            router,
            lane_cache: Arc::new(RwLock::new([None, None, None, None, None])),
            connected,
            secure,
            teardown,
            relay_write_cancel,
        }
    }

    /// The relay adapter owns dialing; this virtual channel is already installed.
    pub async fn connect(&self) -> NetworkResult<()> {
        if self.connected.load(Ordering::Acquire) && self.secure.is_io_sendable() {
            Ok(())
        } else {
            Err(NetworkError::WebSocketClosed(
                "Relay channel is not ready".into(),
            ))
        }
    }

    /// Startmessage dispatch device (in background task)
    fn spawn_dispatcher(
        stream: impl Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Send
        + 'static,
        router: Arc<RwLock<[Option<mpsc::Sender<bytes::Bytes>>; 5]>>,
        connected: Arc<AtomicBool>,
        secure: SecureCell,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tokio::pin!(stream);
            tracing::debug!("📡 WebSocket dispatcher Start");

            loop {
                let msg_result = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    result = stream.next() => match result {
                        Some(result) => result,
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
                                let sender = router.read().await[idx].clone();
                                if let Some(tx) = sender {
                                    // convert exchange as Bytes (zero-copy)
                                    // Preserve the frame's owner, including a
                                    // relay receive-budget permit, through
                                    // the lane queue and consumer dispatch.
                                    let data = data.slice(5..);
                                    let result = tokio::select! {
                                        _ = shutdown.cancelled() => break,
                                        result = tx.send(data) => result,
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
                        tracing::info!("🔌 WebSocket Connect be pair end Close");
                        connected.store(false, Ordering::Release);
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
                        connected.store(false, Ordering::Release);
                        break;
                    }
                }
            }

            connected.store(false, Ordering::Release);
            secure.drive_terminal(SecureChannelState::Closed);
            *router.write().await = [None, None, None, None, None];

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

impl std::fmt::Debug for WebSocketConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConnection")
            .field("secure_state", &self.secure.state())
            .finish_non_exhaustive()
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
        if !self.connected.load(Ordering::Acquire) {
            return Err(NetworkError::WebSocketClosed(
                "WebSocket connection closed".to_string(),
            ));
        }

        // Create receive channel
        let (tx, rx) = mpsc::channel(100);

        // Register route
        self.register_route(payload_type, tx).await?;

        // Get shared Sink
        let sink = self.sink.clone();

        // Create WebSocketDataLane, wrapped in `SecureDataLane` so the cached
        // lane gates both outbound `send()` and the response-reader `recv()`
        // on this session's `SecureCell` (RFC-0563 review point 3).
        let lane: Arc<dyn DataLane> = Arc::new(SecureDataLane::new(
            Arc::new(WebSocketDataLane::new(sink, payload_type, rx)),
            self.secure.clone(),
        ));
        Ok(lane)
    }

    /// Close connection
    pub async fn close(&self) -> NetworkResult<()> {
        // Close admission: drive the secure session terminal first, so every
        // path into this physical close (inherent call sites bypass the trait
        // method — RFC-0563 review point on shadowed terminal driving) marks
        // the session `Closed` and invalidates all wrapped `SecureDataLane`s.
        self.secure.drive_terminal(SecureChannelState::Closed);

        self.teardown.0.cancel();
        self.connected.store(false, Ordering::Release);
        if let Some(cancel) = &self.relay_write_cancel {
            cancel.cancel();
        }

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

    fn verified_peer_context(&self) -> Option<Arc<crate::transport::VerifiedPeerContext>> {
        self.secure.verified_peer()
    }

    fn priority(&self) -> u8 {
        0
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.connect().await
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
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
