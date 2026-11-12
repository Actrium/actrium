//! Outbound peer relay adapter. Business identities and rotating credentials
//! are mapped here, outside the application-independent relay crates.

use super::websocket::{WebSocketConnection, WebSocketGate};
use crate::timer;
use crate::{
    lifecycle::session_state::SessionState,
    transport::{
        ConnType, DataLane, NetworkError, NetworkResult, PeerTransport, SecureChannelState,
        VerifiedPeerContext, WireHandle, WireIdentity,
    },
};
use actr_protocol::{ActrId, ActrType, PayloadType, prost::Message as ProstMessage};
use actr_relay_security::{
    ActorChannelEnrollment, ActorChannelIdentity, ActorChannelSession, IdentityKey,
    IdentityRequest, IssuedIdentity, IssuerKey, LocalIdentity, SecureSession, VerifiedActorChannel,
    signing_key_id,
};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use loran_relay_protocol::{
    ClientControl, MAX_CONTROL_BYTES, RelayDiscovery, SUBPROTOCOL, ServerControl, decode_data,
    encode_data,
};
use sha2::Digest as _;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, OnceLock, RwLock as SyncRwLock, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, RwLock, Semaphore, mpsc, oneshot, watch};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Error as WsError, Message, client::IntoClientRequest, protocol::WebSocketConfig,
    },
};
use tokio_util::sync::CancellationToken;

const MAX_FRAME: usize = 8 * 1024 * 1024 + 64;
const QUEUE_BYTES: usize = 16 * 1024 * 1024;
const OUTGOING_QUEUE_SLOTS: usize = 128;
const CLOSE_QUEUE_SLOTS: usize = 512;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_DESIRED_PEERS: usize = 1024;

fn runtime_instance_id() -> &'static str {
    static RUNTIME_INSTANCE_ID: OnceLock<String> = OnceLock::new();
    RUNTIME_INSTANCE_ID
        .get_or_init(|| uuid::Uuid::new_v4().to_string())
        .as_str()
}

fn closed() -> NetworkError {
    NetworkError::WebSocketClosed("Relay channel closed".into())
}
fn ws_closed() -> WsError {
    WsError::ConnectionClosed
}

struct Outgoing {
    message: Message,
    ack: Option<oneshot::Sender<Result<(), WsError>>>,
    _permit: Option<OwnedSemaphorePermit>,
    _slot: OwnedSemaphorePermit,
    _close: Option<CloseTicket>,
}

struct CloseTicket {
    channel: u64,
    pending: Arc<std::sync::Mutex<HashSet<u64>>>,
}

impl Drop for CloseTicket {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.channel);
    }
}

#[derive(Clone)]
struct Transport {
    tx: mpsc::Sender<Outgoing>,
    budget: Arc<Semaphore>,
    outgoing_slots: Arc<Semaphore>,
    close_slots: Arc<Semaphore>,
    pending_closes: Arc<std::sync::Mutex<HashSet<u64>>>,
    encryption_budget: Arc<Semaphore>,
    cancel: CancellationToken,
    identity: Arc<LocalIdentity>,
    /// Optional Actrium identity. `None` preserves the legacy relay handshake;
    /// `Some` selects the mutually authenticated Front/Service Actor protocol.
    actor_identity: Option<Arc<ActorChannelIdentity>>,
}

impl Transport {
    async fn submit(&self, message: Message) -> Result<(), WsError> {
        let size = message.len().max(1);
        if size > MAX_FRAME {
            return Err(WsError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "relay message too large",
            )));
        }
        let send = async {
            let permit = self
                .budget
                .clone()
                .acquire_many_owned(size as u32)
                .await
                .map_err(|_| ws_closed())?;
            let slot = self
                .outgoing_slots
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| ws_closed())?;
            let (tx, rx) = oneshot::channel();
            self.tx
                .send(Outgoing {
                    message,
                    ack: Some(tx),
                    _permit: Some(permit),
                    _slot: slot,
                    _close: None,
                })
                .await
                .map_err(|_| ws_closed())?;
            rx.await.map_err(|_| ws_closed())?
        };
        tokio::select! {
            _ = self.cancel.cancelled() => Err(ws_closed()),
            result = timer::timeout(timer::ids::RELAY_SUBMIT, IO_TIMEOUT, send) => result.unwrap_or_else(|_| Err(ws_closed())),
        }
    }

    async fn control(&self, control: ClientControl) -> NetworkResult<()> {
        self.submit(Message::Text(
            serde_json::to_string(&control)
                .expect("serializable relay control")
                .into(),
        ))
        .await
        .map_err(NetworkError::from)
    }

    fn try_close(&self, channel: u64) {
        if self.cancel.is_cancelled() {
            return;
        }
        if !self.pending_closes.lock().unwrap().insert(channel) {
            return;
        }
        let ticket = CloseTicket {
            channel,
            pending: self.pending_closes.clone(),
        };
        // Close controls have their own queue and byte reservation. A peer
        // exhausting the data budget must not tear down the shared socket.
        let Ok(slot) = self.close_slots.clone().try_acquire_owned() else {
            tracing::warn!(channel, "Relay close queue is full");
            return;
        };
        let message = serde_json::to_string(&ClientControl::Close { channel }).unwrap();
        if self
            .tx
            .try_send(Outgoing {
                message: Message::Text(message.into()),
                ack: None,
                _permit: None,
                _slot: slot,
                _close: Some(ticket),
            })
            .is_err()
        {
            tracing::debug!(
                channel,
                "Relay writer is no longer accepting close controls"
            );
        }
    }
}

#[derive(Clone)]
enum Status {
    Discovering,
    Disabled,
    Reconnecting,
    Ready(Transport),
}

struct BudgetedPayload {
    payload: Bytes,
    _permits: Vec<OwnedSemaphorePermit>,
}
impl AsRef<[u8]> for BudgetedPayload {
    fn as_ref(&self) -> &[u8] {
        &self.payload
    }
}
struct PendingRequest {
    peer: ActrId,
    reply: oneshot::Sender<NetworkResult<(u64, WebSocketConnection)>>,
}
type PendingMap = Arc<std::sync::Mutex<HashMap<u64, PendingRequest>>>;
struct PendingGuard {
    pending: PendingMap,
    request: u64,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.request);
    }
}
struct Channel {
    security: Arc<Mutex<SecureSession>>,
    receive_permits: Mutex<Vec<OwnedSemaphorePermit>>,
    connected: AtomicBool,
    actrium: bool,
    pending_request: Mutex<Option<u64>>,
    created: tokio::time::Instant,
    peer: ActrId,
    conn: WebSocketConnection,
    tx: mpsc::Sender<Message>,
    ready: watch::Sender<bool>,
    cancel: CancellationToken,
}

struct SinkState {
    transport: Transport,
    channel: u64,
    ready: watch::Receiver<bool>,
    security: Arc<Mutex<SecureSession>>,
    cancel: CancellationToken,
    write_cancel: CancellationToken,
}
impl Drop for SinkState {
    fn drop(&mut self) {
        if !self.cancel.is_cancelled() {
            self.transport.try_close(self.channel);
            self.cancel.cancel();
        }
    }
}

/// One WSS per node, shared by all peers. Only a peer-acknowledged channel is
/// exposed to WirePool; opening the relay socket alone is insufficient.
pub(crate) struct RelayClient {
    signaling_url: String,
    ais_endpoint: String,
    client_key: Option<crate::issuer_client::RealmClientKey>,
    issuers: RwLock<HashMap<u32, IssuerKey>>,
    local_identity: RwLock<Option<Arc<LocalIdentity>>>,
    actor_identity: RwLock<Option<Arc<ActorChannelIdentity>>>,
    actor_storage_root: Option<std::path::PathBuf>,
    actor_key_namespace: Option<String>,
    reissue: Option<crate::lifecycle::credential_manager::CredentialManager>,
    /// Issuers observed to never mint Actor channel certificates. Their
    /// channels run the legacy relay security until the identity expires and
    /// enrollment is retried.
    actor_legacy_issuers: RwLock<HashSet<u32>>,
    /// Credential digests already tried against the Actor-key-binding heal,
    /// so a persistent enrollment rejection cannot rotate credentials in a
    /// loop.
    heal_attempted: std::sync::Mutex<HashSet<[u8; 32]>>,
    runtime_instance_id: String,
    session: SessionState,
    gate: Arc<WebSocketGate>,
    shutdown: CancellationToken,
    status: watch::Sender<Status>,
    retry_disabled: Notify,
    channels: RwLock<HashMap<u64, Arc<Channel>>>,
    peers: RwLock<HashMap<ActrId, u64>>,
    pending: PendingMap,
    desired: Mutex<VecDeque<ActrId>>,
    transport_manager: OnceLock<Weak<PeerTransport>>,
    next_wire: AtomicU64,
    peer_locks: Mutex<HashMap<ActrId, Weak<Mutex<()>>>>,
    next_request: AtomicU64,
}

impl RelayClient {
    #[allow(clippy::too_many_arguments)] // One required setting per transport concern.
    pub(crate) fn new(
        signaling_url: String,
        ais_endpoint: String,
        client_key: Option<crate::issuer_client::RealmClientKey>,
        session: SessionState,
        gate: Arc<WebSocketGate>,
        shutdown: CancellationToken,
        actor_storage_root: Option<std::path::PathBuf>,
        actor_key_namespace: Option<String>,
        reissue: Option<crate::lifecycle::credential_manager::CredentialManager>,
    ) -> Arc<Self> {
        super::ensure_tls_provider();
        let (status, _) = watch::channel(Status::Discovering);
        Arc::new(Self {
            signaling_url,
            ais_endpoint,
            client_key,
            issuers: RwLock::new(HashMap::new()),
            local_identity: RwLock::new(None),
            actor_identity: RwLock::new(None),
            actor_storage_root,
            actor_key_namespace,
            reissue,
            actor_legacy_issuers: RwLock::new(HashSet::new()),
            heal_attempted: std::sync::Mutex::new(HashSet::new()),
            runtime_instance_id: runtime_instance_id().to_owned(),
            session,
            gate,
            shutdown,
            status,
            retry_disabled: Notify::new(),
            channels: RwLock::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            desired: Mutex::new(VecDeque::new()),
            transport_manager: OnceLock::new(),
            next_wire: AtomicU64::new(1),
            peer_locks: Mutex::new(HashMap::new()),
            next_request: AtomicU64::new(1),
        })
    }

    pub(crate) fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            client.run().await;
        })
    }

    pub(crate) fn set_transport(&self, transport: &Arc<PeerTransport>) {
        let _ = self.transport_manager.set(Arc::downgrade(transport));
    }

    pub(crate) fn wire(self: &Arc<Self>, peer: ActrId) -> Arc<dyn WireHandle> {
        let generation = self.next_wire.fetch_add(1, Ordering::Relaxed);
        Arc::new(RelayWire {
            client: self.clone(),
            peer,
            generation,
            connection: SyncRwLock::new(None),
        })
    }

    pub(crate) async fn existing_wire(
        self: &Arc<Self>,
        peer: &ActrId,
    ) -> Option<Arc<dyn WireHandle>> {
        let id = self.peers.read().await.get(peer).copied()?;
        let entry = self.channels.read().await.get(&id).cloned()?;
        if entry.cancel.is_cancelled() || !*entry.ready.borrow() {
            return None;
        }
        Some(Arc::new(RelayWire {
            client: self.clone(),
            peer: peer.clone(),
            generation: self.next_wire.fetch_add(1, Ordering::Relaxed),
            connection: SyncRwLock::new(Some(entry.conn.clone())),
        }))
    }

    async fn warm(self: Arc<Self>, cancel: CancellationToken) {
        loop {
            let peers: Vec<_> = self.desired.lock().await.iter().cloned().collect();
            let connections = futures_util::stream::iter(peers)
                .map(|peer| {
                    let client = self.clone();
                    async move {
                        let _ = client.connect_desired_peer(peer).await;
                    }
                })
                .buffer_unordered(16)
                .collect::<Vec<_>>();
            tokio::select! { _ = cancel.cancelled() => return, _ = connections => {} }
            tokio::select! { _ = cancel.cancelled() => return, _ = timer::sleep(timer::ids::RELAY_WARM_RECONNECT, Duration::from_secs(5)) => {} }
        }
    }

    async fn discover(&self) -> NetworkResult<Option<String>> {
        let mut url = url::Url::parse(&self.signaling_url)
            .map_err(|_| NetworkError::InvalidArgument("Invalid signaling URL".into()))?;
        if !matches!(url.scheme(), "ws" | "wss") {
            return Err(NetworkError::InvalidArgument(
                "Invalid signaling scheme".into(),
            ));
        }
        let secure = url.scheme() == "wss";
        url.set_scheme(if secure { "https" } else { "http" })
            .map_err(|_| NetworkError::InvalidArgument("Invalid signaling scheme".into()))?;
        url.set_path("/relay/config");
        url.set_query(None);
        url.set_fragment(None);
        let client = reqwest::Client::builder()
            .timeout(IO_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| NetworkError::ConnectionError(error.to_string()))?;
        let mut response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| NetworkError::ConnectionError(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(NetworkError::ConnectionError(
                "Relay discovery failed".into(),
            ));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| NetworkError::ConnectionError(error.to_string()))?
        {
            if body.len() + chunk.len() > MAX_CONTROL_BYTES {
                return Err(NetworkError::InvalidArgument(
                    "Relay discovery exceeds size limit".into(),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        let discovery: RelayDiscovery = serde_json::from_slice(&body)
            .map_err(|error| NetworkError::DeserializationError(error.to_string()))?;
        if discovery.version != 1 {
            return Err(NetworkError::InvalidArgument(
                "Unsupported relay discovery version".into(),
            ));
        }
        if !discovery.enabled && discovery.url.is_none() {
            return Ok(None);
        }
        let endpoint = if let Some(endpoint) = discovery.url {
            endpoint
        } else {
            url.set_scheme(if secure { "wss" } else { "ws" }).unwrap();
            url.set_path("/relay/ws");
            url.to_string()
        };
        let parsed = url::Url::parse(&endpoint)
            .map_err(|_| NetworkError::InvalidArgument("Invalid advertised relay URL".into()))?;
        if !matches!(parsed.scheme(), "ws" | "wss")
            || (secure && parsed.scheme() != "wss")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(NetworkError::InvalidArgument(
                "Invalid or insecure advertised relay URL".into(),
            ));
        }
        Ok(Some(endpoint))
    }

    async fn run(self: Arc<Self>) {
        let mut backoff_ms = 250u64;
        loop {
            if self.shutdown.is_cancelled() {
                break;
            }
            let started = tokio::time::Instant::now();
            let attempt = async {
                match self.discover().await {
                    Ok(Some(url)) => self.run_connection(url).await,
                    Ok(None) => {
                        self.status.send_replace(Status::Disabled);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            };
            let result = tokio::select! { _ = self.shutdown.cancelled() => break, result = attempt => result };
            if matches!(*self.status.borrow(), Status::Disabled) {
                self.clear().await;
                let delay = Duration::from_millis(backoff_ms);
                tokio::select! {
                    _ = self.shutdown.cancelled() => break,
                    _ = self.retry_disabled.notified() => {},
                    _ = timer::sleep(timer::ids::RELAY_RECONNECT_BACKOFF, delay) => {},
                }
                self.status.send_replace(Status::Discovering);
                backoff_ms = (backoff_ms * 2).min(30_000);
                continue;
            }
            if started.elapsed() > Duration::from_secs(10) {
                backoff_ms = 250;
            }
            self.status.send_replace(Status::Reconnecting);
            self.clear().await;
            if let Err(error) = result {
                tracing::debug!(error = %error, "Relay reconnect scheduled");
            }
            let delay =
                Duration::from_millis(backoff_ms + rand::random::<u64>() % (backoff_ms / 4 + 1));
            tokio::select! { _ = self.shutdown.cancelled() => break, _ = timer::sleep(timer::ids::RELAY_RECONNECT_BACKOFF, delay) => {} }
            backoff_ms = (backoff_ms * 2).min(30_000);
        }
        self.status.send_replace(Status::Disabled);
        self.clear().await;
    }

    async fn run_connection(self: &Arc<Self>, url: String) -> NetworkResult<()> {
        let snapshot = self.session.snapshot().await;
        let endpoint = snapshot.actor_id.to_string_repr();
        let generation = snapshot.generation;
        let initial_token = snapshot.credential.encode_to_vec();
        let expires_at = snapshot.credential_expires_at.seconds;
        let (identity, actor_identity) = self
            .certified_identity(&endpoint, &snapshot.renewal_token)
            .await?;
        let mut request = url.into_client_request()?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
        let (mut socket, response) = timer::timeout(
            timer::ids::RELAY_SOCKET_CONNECT,
            CONNECT_TIMEOUT,
            connect_async_with_config(
                request,
                Some(
                    WebSocketConfig::default()
                        .max_message_size(Some(actr_relay_security::MAX_RECORD_BYTES + 8))
                        .max_frame_size(Some(actr_relay_security::MAX_RECORD_BYTES + 8))
                        .max_write_buffer_size(QUEUE_BYTES),
                ),
                true,
            ),
        )
        .await
        .map_err(|_| closed())??;
        if response
            .headers()
            .get("sec-websocket-protocol")
            .is_none_or(|header| header != SUBPROTOCOL)
        {
            return Err(NetworkError::ConnectionError(
                "Relay subprotocol not negotiated".into(),
            ));
        }
        let hello = ClientControl::Hello {
            endpoint,
            token: STANDARD
                .encode(serde_json::to_vec(&identity.relay_credential()).map_err(|_| closed())?),
        };
        timer::timeout(
            timer::ids::RELAY_AUTHENTICATION,
            IO_TIMEOUT,
            socket.send(Message::Text(serde_json::to_string(&hello).unwrap().into())),
        )
        .await
        .map_err(|_| closed())??;
        let welcome = timer::timeout(timer::ids::RELAY_AUTHENTICATION, IO_TIMEOUT, socket.next())
            .await
            .map_err(|_| closed())?
            .ok_or_else(closed)??;
        let Message::Text(text) = welcome else {
            return Err(closed());
        };
        if !matches!(
            serde_json::from_str::<ServerControl>(&text),
            Ok(ServerControl::Welcome { .. })
        ) {
            return Err(closed());
        }
        let (mut sink, mut stream) = socket.split();
        let (tx, mut rx) = mpsc::channel::<Outgoing>(OUTGOING_QUEUE_SLOTS + CLOSE_QUEUE_SLOTS);
        let cancel = self.shutdown.child_token();
        let _cancel_on_drop = cancel.clone().drop_guard();
        let transport = Transport {
            tx,
            budget: Arc::new(Semaphore::new(QUEUE_BYTES)),
            outgoing_slots: Arc::new(Semaphore::new(OUTGOING_QUEUE_SLOTS)),
            close_slots: Arc::new(Semaphore::new(CLOSE_QUEUE_SLOTS)),
            pending_closes: Arc::new(std::sync::Mutex::new(HashSet::new())),
            encryption_budget: Arc::new(Semaphore::new(QUEUE_BYTES)),
            cancel: cancel.clone(),
            identity: identity.clone(),
            actor_identity,
        };
        let writer_cancel = cancel.clone();
        let writer = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = writer_cancel.cancelled() => break,
                    queued = rx.recv() => {
                        let Some(queued) = queued else { break; };
                        let result = timer::timeout(timer::ids::RELAY_SOCKET_WRITE, IO_TIMEOUT, sink.send(queued.message)).await.unwrap_or_else(|_| Err(ws_closed()));
                        let failed = result.is_err();
                        if let Some(ack) = queued.ack { let _ = ack.send(result); }
                        if failed { break; }
                    }
                }
            }
            writer_cancel.cancel();
            let _ = timer::timeout(
                timer::ids::RELAY_SOCKET_SHUTDOWN,
                Duration::from_secs(1),
                sink.close(),
            )
            .await;
        });
        self.status.send_replace(Status::Ready(transport.clone()));
        let warm = tokio::spawn(self.clone().warm(cancel.clone()));
        let budget = Arc::new(Semaphore::new(QUEUE_BYTES));
        let mut identity_check =
            timer::interval(timer::ids::RELAY_IDENTITY_POLL, Duration::from_secs(1));
        let result = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(()),
                _ = identity_check.tick() => {
                    let current = self.session.snapshot().await;
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                    if identity.expires_at <= now_secs() + 30 { break Ok(()); }
                    let expired: Vec<_> = self.channels.read().await.iter().filter_map(|(id, entry)| {
                        (!*entry.ready.borrow() && entry.created.elapsed() > Duration::from_secs(10)).then_some(*id)
                    }).collect();
                    for id in expired { self.close_channel(id).await; transport.try_close(id); }
                    if current.generation != generation || (expires_at.saturating_sub(now) < 30 && current.credential.encode_to_vec() != initial_token) { break Ok(()); }
                }
                message = stream.next() => match message {
                    Some(Ok(Message::Text(text))) if text.len() <= MAX_CONTROL_BYTES => {
                        match serde_json::from_str::<ServerControl>(&text) {
                            Ok(control) => if let Err(error) = self.handle_control(control, &transport).await { break Err(error); },
                            Err(_) => break Err(NetworkError::DeserializationError("Malformed relay control".into())),
                        }
                    }
                    Some(Ok(Message::Binary(frame))) => {
                        let (channel_id, payload) = match decode_data(frame, MAX_FRAME) { Ok(frame) => frame, Err(_) => break Err(closed()) };
                        if let Err(error) = self.receive_secure(channel_id, payload, &transport, &budget).await {
                            tracing::debug!(channel_id, %error, "Relay secure channel rejected");
                            self.close_channel(channel_id).await;
                            transport.try_close(channel_id);
                        }
                    }
                    Some(Ok(Message::Ping(data))) => if transport.submit(Message::Pong(data)).await.is_err() { break Err(closed()); },
                    Some(Ok(Message::Pong(_))) => {},
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break Err(closed()),
                    _ => break Err(NetworkError::DeserializationError("Unexpected relay frame".into())),
                }
            }
        };
        cancel.cancel();
        let _ = writer.await;
        let _ = warm.await;
        result
    }

    async fn install(
        &self,
        channel: u64,
        endpoint: String,
        transport: &Transport,
        actor_front: bool,
    ) -> NetworkResult<Arc<Channel>> {
        if let Some(entry) = self.channels.read().await.get(&channel).cloned() {
            if entry.peer.to_string_repr() != endpoint {
                return Err(NetworkError::DeserializationError(
                    "Relay channel identity changed".into(),
                ));
            }
            return Ok(entry);
        }
        let peer = ActrId::from_string_repr(&endpoint).map_err(|_| {
            NetworkError::DeserializationError("Invalid relay peer identity".into())
        })?;
        if self.channels.read().await.len() >= 256 {
            return Err(NetworkError::ConnectionError(
                "Relay channel capacity exceeded".into(),
            ));
        }
        if peer.to_string_repr() != endpoint || peer.realm != self.session.actor_id().await.realm {
            return Err(NetworkError::DeserializationError(
                "Relay peer identity mismatch".into(),
            ));
        }
        let (security, actrium) = if let Some(identity) = &transport.actor_identity {
            let channel_id = channel.to_string();
            let session = if actor_front {
                ActorChannelSession::new_front(identity, &endpoint, &channel_id, now_secs())
            } else {
                ActorChannelSession::new_service(identity, &endpoint, &channel_id, now_secs())
            }
            .map_err(secure_error)?;
            (Arc::new(Mutex::new(session.into_inner())), true)
        } else {
            (
                Arc::new(Mutex::new(
                    SecureSession::new(&transport.identity, &endpoint, channel, now_secs())
                        .map_err(secure_error)?,
                )),
                false,
            )
        };
        let (tx, rx) = mpsc::channel::<Message>(16);
        let (ready, ready_rx) = watch::channel(false);
        let cancel = transport.cancel.child_token();
        let write_cancel = cancel.child_token();
        let state = SinkState {
            transport: transport.clone(),
            channel,
            ready: ready_rx,
            security: security.clone(),
            cancel: cancel.clone(),
            write_cancel: write_cancel.clone(),
        };
        let sink = futures_util::sink::unfold(state, |mut state, message| async move {
            let send = async {
                while !*state.ready.borrow_and_update() {
                    state.ready.changed().await.map_err(|_| ws_closed())?;
                }
                match message {
                    Message::Binary(payload) => {
                        // Bound the full ciphertext batch before allocating it.
                        // This budget is separate from the socket queue so
                        // concurrent peers cannot deadlock their own writes.
                        let bytes = actr_relay_security::encrypted_bytes(payload.len())
                            .map_err(|_| ws_closed())?;
                        let _encrypted = timer::timeout(
                            timer::ids::RELAY_SUBMIT,
                            IO_TIMEOUT,
                            state
                                .transport
                                .encryption_budget
                                .clone()
                                .acquire_many_owned(bytes as u32),
                        )
                        .await
                        .map_err(|_| ws_closed())?
                        .map_err(|_| ws_closed())?;
                        let records = state
                            .security
                            .lock()
                            .await
                            .send(&payload, now_secs())
                            .map_err(|_| ws_closed())?;
                        for record in records {
                            if state
                                .transport
                                .submit(Message::Binary(
                                    encode_data(state.channel, &record).map_err(|_| ws_closed())?,
                                ))
                                .await
                                .is_err()
                            {
                                state.cancel.cancel();
                                return Err(ws_closed());
                            }
                        }
                        Ok(())
                    }
                    Message::Close(_) => {
                        state.transport.try_close(state.channel);
                        Ok(())
                    }
                    _ => Err(ws_closed()),
                }
            };
            tokio::select! {
                _ = state.cancel.cancelled() => return Err(ws_closed()),
                _ = state.write_cancel.cancelled() => return Err(ws_closed()),
                result = send => result?,
            }
            Ok::<_, WsError>(state)
        });
        let incoming = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
        let conn = if actrium {
            WebSocketConnection::from_actrium_encrypted_parts(
                Box::pin(sink),
                incoming,
                write_cancel,
            )
        } else {
            WebSocketConnection::from_encrypted_parts(Box::pin(sink), incoming, write_cancel)
        };
        let entry = Arc::new(Channel {
            security,
            receive_permits: Mutex::new(Vec::new()),
            connected: AtomicBool::new(false),
            actrium,
            pending_request: Mutex::new(None),
            created: tokio::time::Instant::now(),
            peer: peer.clone(),
            conn: conn.clone(),
            tx,
            ready,
            cancel,
        });
        self.channels.write().await.insert(channel, entry.clone());
        Ok(entry)
    }

    async fn handle_control(
        self: &Arc<Self>,
        control: ServerControl,
        transport: &Transport,
    ) -> NetworkResult<()> {
        match control {
            ServerControl::Incoming { channel, peer, .. } => {
                if self.install(channel, peer, transport, false).await.is_err() {
                    self.close_channel(channel).await;
                    transport.control(ClientControl::Reject { channel }).await?;
                } else {
                    transport.control(ClientControl::Accept { channel }).await?;
                }
            }
            ServerControl::Connected {
                channel,
                mut request,
                peer,
                ..
            } => {
                if let Some(request_id) = request {
                    let expected = self
                        .pending
                        .lock()
                        .unwrap()
                        .get(&request_id)
                        .map(|pending| pending.peer.clone());
                    match expected {
                        Some(expected) if expected.to_string_repr() == peer => {}
                        Some(_) => {
                            // Fail only this dial. The channel number belongs to
                            // an untrusted response and may name an unrelated
                            // live channel, so do not close or install it.
                            if let Some(pending) = self.pending.lock().unwrap().remove(&request_id)
                            {
                                let _ =
                                    pending.reply.send(Err(NetworkError::DeserializationError(
                                        "Relay response identity mismatch".into(),
                                    )));
                            }
                            return Ok(());
                        }
                        None => {
                            // A cancelled outbound dial can share an incoming
                            // channel. Keep its handshake, but never associate
                            // the expired request with that channel.
                            if self.channels.read().await.contains_key(&channel) {
                                request = None;
                            } else {
                                transport.try_close(channel);
                                return Ok(());
                            }
                        }
                    }
                }
                let entry = match self.install(channel, peer.clone(), transport, true).await {
                    Ok(entry) => entry,
                    Err(error) => {
                        // The untrusted channel id may refer to another live
                        // peer. Reject the affected dial without retiring it.
                        tracing::warn!(channel, %error, "Rejected relay channel response");
                        if let Some(request) = request
                            && let Some(pending) = self.pending.lock().unwrap().remove(&request)
                        {
                            let _ = pending.reply.send(Err(error));
                        }
                        return Ok(());
                    }
                };
                if entry.cancel.is_cancelled() || !entry.conn.is_connected() {
                    // A local close may be waiting for the old sink while a
                    // concurrent dial receives the server's accepted channel.
                    // Retire it before asking for a fresh channel; the writer
                    // preserves the Close-before-Connect control ordering.
                    let stale = {
                        let mut channels = self.channels.write().await;
                        if channels
                            .get(&channel)
                            .is_some_and(|current| Arc::ptr_eq(current, &entry))
                        {
                            channels.remove(&channel)
                        } else {
                            None
                        }
                    };
                    if let Some(stale) = stale {
                        if let Some(request) = request {
                            let mut previous = stale.pending_request.lock().await;
                            if *previous == Some(request) {
                                *previous = None;
                            }
                        }
                        stale.cancel.cancel();
                        let client = self.clone();
                        tokio::spawn(async move {
                            client.dispose_channel(channel, stale).await;
                        });
                    }
                    transport.try_close(channel);
                    if let Some(request) = request {
                        let still_pending = self.pending.lock().unwrap().contains_key(&request);
                        if still_pending {
                            transport
                                .control(ClientControl::Connect { request, peer })
                                .await?;
                        }
                    }
                    return Ok(());
                }
                if let Some(request) = request {
                    let mut pending_request = entry.pending_request.lock().await;
                    if *entry.ready.borrow() {
                        self.complete_pending(request, channel, &entry);
                    } else {
                        *pending_request = Some(request);
                    }
                }
                if !entry.connected.swap(true, Ordering::AcqRel) {
                    if let Some(first) =
                        entry.security.lock().await.start().map_err(secure_error)?
                    {
                        transport
                            .submit(Message::Binary(
                                encode_data(channel, &first).map_err(|_| closed())?,
                            ))
                            .await?;
                    }
                }
            }
            ServerControl::Closed {
                channel, request, ..
            } => {
                if let Some(request) = request {
                    if let Some(pending) = self.pending.lock().unwrap().remove(&request) {
                        let _ = pending.reply.send(Err(closed()));
                    }
                }
                self.close_channel(channel).await;
            }
            ServerControl::Error {
                request: Some(request),
                code,
            } => {
                if let Some(pending) = self.pending.lock().unwrap().remove(&request) {
                    let _ = pending
                        .reply
                        .send(Err(NetworkError::ConnectionError(format!(
                            "Relay connect rejected: {code:?}"
                        ))));
                }
            }
            ServerControl::Error { request: None, .. } => {}
            ServerControl::Welcome { .. } => {
                return Err(NetworkError::DeserializationError(
                    "Unexpected relay welcome".into(),
                ));
            }
        }
        Ok(())
    }

    async fn close_channel(&self, id: u64) {
        let entry = self.channels.write().await.remove(&id);
        if let Some(entry) = entry {
            self.dispose_channel(id, entry).await;
        }
    }

    async fn dispose_channel(&self, id: u64, entry: Arc<Channel>) {
        if let Some(request) = entry.pending_request.lock().await.take() {
            if let Some(pending) = self.pending.lock().unwrap().remove(&request) {
                let _ = pending.reply.send(Err(closed()));
            }
        }
        entry.cancel.cancel();
        entry.security.lock().await.close();
        entry.receive_permits.lock().await.clear();
        let mut peers = self.peers.write().await;
        if peers.get(&entry.peer) == Some(&id) {
            peers.remove(&entry.peer);
        }
        drop(peers);
        let _ = entry.conn.close().await;
    }

    async fn clear(&self) {
        let ids: Vec<_> = self.channels.read().await.keys().copied().collect();
        for id in ids {
            self.close_channel(id).await;
        }
        for (_, pending) in self.pending.lock().unwrap().drain() {
            let _ = pending.reply.send(Err(closed()));
        }
    }

    async fn remember_peer(&self, peer: &ActrId) -> NetworkResult<()> {
        let mut desired = self.desired.lock().await;
        if let Some(index) = desired.iter().position(|item| item == peer) {
            desired.remove(index);
        } else if desired.len() >= MAX_DESIRED_PEERS {
            let active = self.peers.read().await;
            let dialing = self.peer_locks.lock().await;
            let Some(index) = desired.iter().position(|item| {
                !active.contains_key(item)
                    && dialing
                        .get(item)
                        .is_none_or(|lock| lock.strong_count() == 0)
            }) else {
                return Err(NetworkError::ConnectionError(
                    "Relay peer capacity exhausted".into(),
                ));
            };
            desired.remove(index);
        }
        desired.push_back(peer.clone());
        Ok(())
    }

    async fn connect_peer(&self, peer: ActrId) -> NetworkResult<(u64, WebSocketConnection)> {
        self.remember_peer(&peer).await?;
        self.dial_peer(peer).await
    }

    async fn connect_desired_peer(
        &self,
        peer: ActrId,
    ) -> NetworkResult<(u64, WebSocketConnection)> {
        if !self.desired.lock().await.contains(&peer) {
            return Err(closed());
        }
        self.dial_peer(peer).await
    }

    async fn dial_peer(&self, peer: ActrId) -> NetworkResult<(u64, WebSocketConnection)> {
        let lock = {
            let mut locks = self.peer_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() != 0);
            if let Some(lock) = locks.get(&peer).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(peer.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let _peer_guard = lock.lock().await;
        if let Some(id) = self.peers.read().await.get(&peer).copied() {
            if let Some(entry) = self.channels.read().await.get(&id) {
                if !entry.cancel.is_cancelled()
                    && *entry.ready.borrow()
                    && entry.conn.is_connected()
                {
                    return Ok((id, entry.conn.clone()));
                }
            }
        }
        let mut status = self.status.subscribe();
        let transport = timer::timeout(timer::ids::RELAY_SESSION_READY, CONNECT_TIMEOUT, async {
            let mut requested_refresh = false;
            loop {
                match status.borrow_and_update().clone() {
                    Status::Ready(transport) if !transport.cancel.is_cancelled() => return Ok(transport),
                    Status::Disabled if !requested_refresh => {
                        requested_refresh = true;
                        self.retry_disabled.notify_one();
                    }
                    Status::Disabled => return Err(NetworkError::NoRoute("Relay service is not advertised".into())),
                    _ => {},
                }
                tokio::select! { _ = self.shutdown.cancelled() => return Err(closed()), result = status.changed() => result.map_err(|_| closed())? }
            }
        }).await.map_err(|_| closed())??;
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(
            request,
            PendingRequest {
                peer: peer.clone(),
                reply: tx,
            },
        );
        let _pending_guard = PendingGuard {
            pending: self.pending.clone(),
            request,
        };
        if let Err(error) = transport
            .control(ClientControl::Connect {
                request,
                peer: peer.to_string_repr(),
            })
            .await
        {
            self.pending.lock().unwrap().remove(&request);
            return Err(error);
        }
        let result = tokio::select! {
            _ = self.shutdown.cancelled() => Err(closed()),
            result = timer::timeout(timer::ids::RELAY_CHANNEL_CONNECT, CONNECT_TIMEOUT, rx) => result.map_err(|_| closed()).and_then(|result| result.map_err(|_| closed())).and_then(|result| result),
        };
        self.pending.lock().unwrap().remove(&request);
        result
    }
}

struct RelayWire {
    client: Arc<RelayClient>,
    peer: ActrId,
    generation: u64,
    connection: SyncRwLock<Option<WebSocketConnection>>,
}
impl std::fmt::Debug for RelayWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayWire")
            .field("peer", &self.peer)
            .field("generation", &self.generation)
            .finish()
    }
}

#[async_trait]
impl WireHandle for RelayWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }
    fn priority(&self) -> u8 {
        1
    }
    fn verified_peer_context(&self) -> Option<Arc<VerifiedPeerContext>> {
        self.connection
            .read()
            .unwrap()
            .as_ref()
            .and_then(|connection| connection.secure_cell().verified_peer())
    }
    async fn connect(&self) -> NetworkResult<()> {
        let (_, connection) = self.client.connect_peer(self.peer.clone()).await?;
        *self.connection.write().unwrap() = Some(connection);
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.connection
            .read()
            .unwrap()
            .as_ref()
            .map(WireHandle::is_connected)
            .unwrap_or(false)
    }
    async fn close(&self) -> NetworkResult<()> {
        let connection = self.connection.read().unwrap().clone();
        if let Some(connection) = connection {
            connection.close().await?;
            let mut current = self.connection.write().unwrap();
            if current
                .as_ref()
                .is_some_and(|other| Arc::ptr_eq(&other.sink(), &connection.sink()))
            {
                current.take();
            }
        }
        Ok(())
    }
    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        if payload_type == PayloadType::MediaRtp {
            return Err(NetworkError::InvalidArgument(
                "Native media requires WebRTC".into(),
            ));
        }
        let connection = self.connection.read().unwrap().clone().ok_or_else(closed)?;
        connection.get_lane(payload_type).await
    }
    fn identity(&self) -> Option<WireIdentity> {
        Some(WireIdentity::WebSocket {
            peer_id: self.peer.clone(),
            session_id: self.generation,
        })
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        self.connection
            .read()
            .unwrap()
            .as_ref()
            .map(WireHandle::secure_channel_state)
            .unwrap_or(SecureChannelState::Handshaking)
    }
    fn secure_epoch(&self) -> u64 {
        self.connection
            .read()
            .unwrap()
            .as_ref()
            .map(WireHandle::secure_epoch)
            .unwrap_or(0)
    }
    async fn await_secure_state_change(&self, observed: u64) {
        let connection = self.connection.read().unwrap().clone();
        if let Some(connection) = connection {
            connection.await_secure_state_change(observed).await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn secure_error(error: actr_relay_security::SecurityError) -> NetworkError {
    NetworkError::ConnectionError(error.to_string())
}

fn actrium_peer_context(
    local_actor_id: ActrId,
    peer_actor_id: ActrId,
    channel: u64,
    verified: VerifiedActorChannel,
) -> NetworkResult<VerifiedPeerContext> {
    if verified.channel_id != channel.to_string()
        || verified.peer_claims.actor_id != peer_actor_id.to_string_repr()
        || verified.peer_claims.realm != peer_actor_id.realm
        || !verified.is_valid_at(now_secs())
    {
        return Err(NetworkError::ConnectionError(
            "Verified Actor channel context mismatch".into(),
        ));
    }
    let peer_actor_type = ActrType::from_string_repr(&verified.peer_claims.actor_type)
        .map_err(|_| NetworkError::ConnectionError("Invalid verified Actor type".into()))?;
    Ok(VerifiedPeerContext {
        local_actor_id,
        peer_actor_id,
        peer_actor_type,
        peer_member: verified.peer_claims.member,
        peer_client_key: verified.peer_claims.client_key,
        peer_role: verified.peer_claims.role,
        peer_actor_public_key: verified.peer_actor_public_key.to_vec(),
        channel_id: verified.channel_id,
        channel_generation: channel,
        transcript_digest: verified.transcript_digest.to_vec(),
        local_runtime_instance_id: verified.local_runtime_instance_id,
        peer_runtime_instance_id: verified.peer_runtime_instance_id,
        peer_credential_expires_at_epoch_secs: verified.expires_at,
        current_policy_binding: verified.peer_claims.policy_digest.to_vec(),
        current_state_binding: verified.peer_claims.actor_state_digest.to_vec(),
    })
}

fn secure_http_client() -> NetworkResult<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(IO_TIMEOUT)
        .build()
        .map_err(|_| closed())
}
async fn bounded_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> NetworkResult<T> {
    bounded_json_with_status(request)
        .await
        .map_err(|_| closed())
}

/// Like [`bounded_json`] but reports the HTTP status of rejected requests so
/// callers can distinguish recoverable rejections (e.g. the 403 enrollment
/// race healed by a credential reissue) from transport failures.
async fn bounded_json_with_status<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, Option<u16>> {
    let response = request.send().await.map_err(|_| None)?;
    let status = response.status().as_u16();
    let mut response = response.error_for_status().map_err(|_| Some(status))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| None)? {
        if bytes.len() + chunk.len() > MAX_CONTROL_BYTES {
            return Err(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| None)
}

impl RelayClient {
    async fn certified_identity(
        &self,
        endpoint: &str,
        _proof: &[u8],
    ) -> NetworkResult<(Arc<LocalIdentity>, Option<Arc<ActorChannelIdentity>>)> {
        // A legacy Issuer (or a mock) never mints Actor channel certificates,
        // and the first enrollment attempt may be rejected with 403 while the
        // Actor key binding is still absent (identities registered before
        // Actor keys existed). Neither may fail the relay channel forever:
        // an unbound key is healed by reissuing the credential (which binds
        // the local Actor key Issuer-side), and a certificate-less Issuer is
        // the trust anchor deciding the legacy downgrade.
        for attempt in 0..2u8 {
            // Bind the cache to the exact credential and generation. A hard rebind
            // may retain the endpoint and actor key, but must never reuse the Actor
            // certificate issued for the previous credential generation.
            let snapshot = self.session.snapshot().await;
            let actor_binding = self.actor_storage_root.as_ref().map(|_| {
                let credential = snapshot.credential.encode_to_vec();
                let credential_digest = actr_relay_security::actor_credential_digest(&credential);
                let mut state_input = credential_digest.to_vec();
                state_input.extend_from_slice(&snapshot.generation.to_be_bytes());
                let state_digest = sha2::Sha256::digest(&state_input).into();
                (credential, credential_digest, state_digest)
            });
            if let Some(identity) = self
                .local_identity
                .read()
                .await
                .as_ref()
                .filter(|identity| {
                    identity.endpoint == endpoint && identity.expires_at > now_secs() + 30
                })
                .cloned()
            {
                let actor = if let Some((_, credential_digest, state_digest)) = &actor_binding {
                    self.actor_identity
                        .read()
                        .await
                        .as_ref()
                        .filter(|actor| {
                            actor.claims().actor_credential_digest == *credential_digest
                                && actor.claims().actor_state_digest == *state_digest
                                && actor.expires_at() > now_secs() + 30
                        })
                        .cloned()
                } else {
                    None
                };
                let legacy_issuer = self
                    .actor_legacy_issuers
                    .read()
                    .await
                    .contains(&identity.issued.issuer.key_id);
                if actor_binding.is_none() || actor.is_some() || legacy_issuer {
                    return Ok((identity, actor));
                }
            }
            let key = IdentityKey::generate().map_err(secure_error)?;
            let noise_for_actor = key.clone();
            let actor_material =
                if let (Some(storage_root), Some((credential, credential_digest, state_digest))) =
                    (&self.actor_storage_root, &actor_binding)
                {
                    let claims =
                        actr_protocol::IdentityClaims::decode(snapshot.credential.claims.as_ref())
                            .map_err(|_| closed())?;
                    let signer = crate::issuer_client::actor_signer_for_scoped(
                        storage_root,
                        &claims.realm,
                        &claims.actor_type,
                        self.actor_key_namespace.as_deref(),
                    )
                    .map_err(|_| closed())?;
                    let policy_digest = sha2::Sha256::digest(
                        format!(
                            "actrium.channel.policy.v1:{}",
                            claims.actor_type.to_string_repr()
                        )
                        .as_bytes(),
                    );
                    let enrollment = ActorChannelEnrollment {
                        policy_digest: policy_digest.into(),
                        actor_state_digest: *state_digest,
                        runtime_instance_id: self.runtime_instance_id.clone(),
                        actor_credential: credential.clone(),
                        actor_credential_digest: *credential_digest,
                    };
                    Some((signer.actor_identity_key(), enrollment))
                } else {
                    None
                };
            let client = secure_http_client()?;
            let body = serde_json::to_vec(&IdentityRequest {
                endpoint: endpoint.to_owned(),
                renewal_token: snapshot.renewal_token.to_vec(),
                public_key: key.public,
                admission_commitment: key.admission_commitment(),
                actor: actor_material
                    .as_ref()
                    .map(|(_, enrollment)| enrollment.clone()),
            })
            .map_err(|_| closed())?;
            let actor = ActrId::from_string_repr(endpoint).map_err(|_| closed())?;
            let mut request = client
                .post(self.ais_url("relay-identity")?)
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            if let Some(key) = &self.client_key {
                let proof = key
                    .proof(
                        actr_protocol::realm_membership::ProofOp::RelayIdentity,
                        &actor.realm,
                        &body,
                    )
                    .map_err(|_| {
                        NetworkError::ConnectionError("Realm client signer unavailable".into())
                    })?;
                request =
                    request.header(actr_protocol::realm_membership::CLIENT_PROOF_HEADER, proof);
            }
            let issued: IssuedIdentity = match bounded_json_with_status(request.body(body)).await {
                Ok(issued) => issued,
                Err(Some(403)) if attempt == 0 && actor_material.is_some() => {
                    self.heal_actor_key_binding(&actor).await?;
                    continue;
                }
                Err(_) => return Err(closed()),
            };
            let actor_certificate = issued.actor_certificate.clone();
            let identity = Arc::new(
                key.certify(issued, endpoint, now_secs())
                    .map_err(secure_error)?,
            );
            let actor_identity = match (actor_material, actor_certificate) {
                (Some((actor_key, _)), Some(certificate)) => Some(Arc::new(
                    ActorChannelIdentity::certify(
                        noise_for_actor,
                        actor_key,
                        certificate,
                        identity.issued.issuer.clone(),
                        &actor.realm,
                        endpoint,
                        now_secs(),
                    )
                    .map_err(secure_error)?,
                )),
                (None, _) => None,
                (Some(_), None) => {
                    // The Issuer does not issue Actor channel certificates (a
                    // mock or a deployment predating Actor channels). Degrade
                    // this issuer to the legacy relay channel instead of
                    // failing every peer connection; the marker is re-checked
                    // each identity lifetime so an upgraded Issuer is picked
                    // up without a restart.
                    tracing::warn!(
                        issuer_key_id = identity.issued.issuer.key_id,
                        "Issuer returned no Actor channel certificate; using the legacy relay channel for it"
                    );
                    self.actor_legacy_issuers
                        .write()
                        .await
                        .insert(identity.issued.issuer.key_id);
                    None
                }
            };
            if actor_identity.is_some() {
                self.actor_legacy_issuers
                    .write()
                    .await
                    .remove(&identity.issued.issuer.key_id);
            }
            {
                let mut keys = self.issuers.write().await;
                keys.retain(|_, key| key.expires_at > now_secs());
                if keys.len() >= 256 && !keys.contains_key(&identity.issued.issuer.key_id) {
                    return Err(closed());
                }
                keys.insert(
                    identity.issued.issuer.key_id,
                    identity.issued.issuer.clone(),
                );
            }
            *self.local_identity.write().await = Some(identity.clone());
            *self.actor_identity.write().await = actor_identity.clone();
            return Ok((identity, actor_identity));
        }
        Err(closed())
    }

    /// Bind the local Actor identity key Issuer-side by reissuing the
    /// credential. `/reissue` establishes the binding when the actor has none
    /// and mints a credential carrying the key claim the relay-identity
    /// enrollment requires. Attempted at most once per credential generation
    /// so a persistent rejection cannot rotate credentials in a loop.
    async fn heal_actor_key_binding(&self, actor: &ActrId) -> NetworkResult<()> {
        let Some(reissue) = &self.reissue else {
            return Err(closed());
        };
        let snapshot = self.session.snapshot().await;
        let digest =
            actr_relay_security::actor_credential_digest(&snapshot.credential.encode_to_vec());
        if !self.heal_attempted.lock().unwrap().insert(digest) {
            return Err(closed());
        }
        tracing::warn!(
            actor_id = %actor.to_string_repr(),
            "Relay Actor enrollment rejected (unbound Actor key); reissuing the credential to establish the binding"
        );
        match reissue.reissue_now().await {
            crate::lifecycle::credential_manager::ReacquireOutcome::Renewed(_) => Ok(()),
            outcome => {
                tracing::warn!(
                    outcome = ?outcome,
                    "Relay Actor key-binding heal failed"
                );
                Err(closed())
            }
        }
    }
    fn ais_url(&self, path: &str) -> NetworkResult<url::Url> {
        let mut url = url::Url::parse(&self.ais_endpoint).map_err(|_| closed())?;
        let secure = url::Url::parse(&self.signaling_url)
            .map_err(|_| closed())?
            .scheme()
            == "wss";
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || (secure && url.scheme() != "https")
        {
            return Err(NetworkError::InvalidArgument(
                "Relay encryption requires a trusted HTTPS Issuer for WSS deployments".into(),
            ));
        }
        url.set_path(&format!("{}/{path}", url.path().trim_end_matches('/')));
        Ok(url)
    }
    async fn issuer(&self, id: u32) -> NetworkResult<IssuerKey> {
        if let Some(key) = self
            .issuers
            .read()
            .await
            .get(&id)
            .filter(|key| key.expires_at > now_secs())
            .cloned()
        {
            return Ok(key);
        }
        let realm = self.session.actor_id().await.realm;
        let key: IssuerKey = bounded_json(
            secure_http_client()?.get(self.ais_url(&format!("relay-key/{realm}/{id}"))?),
        )
        .await?;
        if key.key_id != id || key.expires_at <= now_secs() {
            return Err(closed());
        }
        let mut cache = self.issuers.write().await;
        cache.retain(|_, key| key.expires_at > now_secs());
        if cache.len() >= 256 {
            return Err(closed());
        }
        cache.insert(id, key.clone());
        Ok(key)
    }
    async fn receive_secure(
        self: &Arc<Self>,
        id: u64,
        payload: Bytes,
        transport: &Transport,
        budget: &Arc<Semaphore>,
    ) -> NetworkResult<()> {
        let Some(entry) = self.channels.read().await.get(&id).cloned() else {
            return Ok(());
        };
        if !entry.connected.load(Ordering::Acquire) {
            return Err(closed());
        }
        let permit = budget
            .clone()
            .try_acquire_many_owned(payload.len().max(1) as u32)
            .map_err(|_| closed())?;
        let issuer = if let Some(id) = signing_key_id(&payload).map_err(secure_error)? {
            Some(self.issuer(id).await?)
        } else {
            None
        };
        let (received, partial) = {
            let mut session = entry.security.lock().await;
            let received = session
                .receive(&payload, issuer.as_ref(), now_secs())
                .map_err(secure_error)?;
            (received, session.buffered_bytes())
        };
        for record in received.outbound {
            transport
                .submit(Message::Binary(
                    encode_data(id, &record).map_err(|_| closed())?,
                ))
                .await?;
        }
        if received.became_ready {
            self.secure_ready(id, &entry).await?;
        }
        if let Some(application) = received.application {
            if application.len() < 5
                || application[0] > 3
                || u32::from_be_bytes(application[1..5].try_into().unwrap()) as usize
                    != application.len() - 5
            {
                return Err(closed());
            }
            let mut permits = std::mem::take(&mut *entry.receive_permits.lock().await);
            permits.push(permit);
            entry
                .tx
                .try_send(Message::Binary(Bytes::from_owner(BudgetedPayload {
                    payload: application.into(),
                    _permits: permits,
                })))
                .map_err(|_| closed())?;
        } else if partial != 0 {
            entry.receive_permits.lock().await.push(permit);
        }
        Ok(())
    }
    async fn secure_ready(self: &Arc<Self>, id: u64, entry: &Arc<Channel>) -> NetworkResult<()> {
        let (actor_type, verified) = {
            let session = entry.security.lock().await;
            let value = session.peer_actor_type().ok_or_else(closed)?;
            let actor_type =
                actr_protocol::ActrType::from_string_repr(value).map_err(|_| closed())?;
            (actor_type, session.verified_actor().cloned())
        };
        let published = if entry.actrium {
            let verified = verified.ok_or_else(closed)?;
            let context = actrium_peer_context(
                self.session.actor_id().await,
                entry.peer.clone(),
                id,
                verified,
            )?;
            entry.conn.publish_actrium_verified(context)
        } else {
            entry.conn.mark_relay_verified()
        };
        if !published {
            return Err(closed());
        }
        tokio::select! {
            _ = entry.cancel.cancelled() => return Err(closed()),
            result = self.gate.attach_relay(entry.conn.clone(), entry.peer.clone(), actor_type) => {
                result.map_err(|error| NetworkError::ConnectionError(error.to_string()))?;
            }
        }
        entry.ready.send_replace(true);
        self.gate.relay_ready(entry.peer.clone());
        let _ = self.remember_peer(&entry.peer).await;
        self.peers.write().await.insert(entry.peer.clone(), id);
        if let Some(manager) = self.transport_manager.get().and_then(Weak::upgrade) {
            if let Some(wire) = self.existing_wire(&entry.peer).await {
                manager.adopt_relay(entry.peer.clone(), wire).await;
            }
        }
        if let Some(request) = entry.pending_request.lock().await.take() {
            self.complete_pending(request, id, entry);
        }
        Ok(())
    }

    fn complete_pending(&self, request: u64, id: u64, entry: &Channel) {
        if let Some(pending) = self.pending.lock().unwrap().remove(&request) {
            let result = if pending.peer == entry.peer {
                Ok((id, entry.conn.clone()))
            } else {
                Err(NetworkError::DeserializationError(
                    "Relay response identity mismatch".into(),
                ))
            };
            let _ = pending.reply.send(result);
        }
    }
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;

#[cfg(test)]
mod identity_cache_review_tests {
    use super::*;
    use crate::inbound::DataChunkRegistry;
    use crate::issuer_client::ActorKeySigner as _;
    use crate::lifecycle::{node::PeerTypes, session_state::SessionSnapshot};
    use crate::transport::correlation::RpcCorrelation;
    use actr_relay_security::{
        ACTOR_CHANNEL_AUDIENCE, ActorChannelCertificate, ActorChannelClaims, CERTIFICATE_DOMAIN,
        Certificate, IdentityClaims as RelayIdentityClaims, signing_bytes,
    };
    use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};

    fn credential(
        claims: &actr_protocol::IdentityClaims,
        issuer: &SigningKey,
    ) -> actr_protocol::AIdCredential {
        let claims = claims.encode_to_vec();
        actr_protocol::AIdCredential {
            key_id: 1,
            signature: issuer.sign(&claims).to_bytes().to_vec().into(),
            claims: claims.into(),
        }
    }

    #[tokio::test]
    async fn actor_identity_cache_tracks_credential_and_generation() {
        let root = tempfile::tempdir().unwrap();
        let actor_type = ActrType::from_string_repr("actrium/actrium-front@1.0.0").unwrap();
        let realm = "test@example.com".to_string();
        let signer = crate::issuer_client::actor_signer_for_scoped(
            root.path(),
            &realm,
            &actor_type,
            Some("tab-cache"),
        )
        .unwrap();
        let local = ActrId::from_string_repr("2.test@example.com").unwrap();
        let endpoint = local.to_string_repr();
        let now = now_secs();
        let issuer_signing = SigningKey::from_bytes(&[7; 32]);
        let issuer_public = IssuerKey {
            key_id: 1,
            public_key: issuer_signing.verifying_key().to_bytes(),
            expires_at: now + 600,
        };
        let mut server = mockito::Server::new_async().await;
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let response_requests = requests.clone();
        let response_issuer = issuer_public.clone();
        let response_type = actor_type.clone();
        let response_signer = signer.clone();
        let response_realm = realm.clone();
        let response_issuer_signing = issuer_signing.clone();
        let mock = server
            .mock("POST", "/relay-identity")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(move |request| {
                let enrollment: IdentityRequest =
                    serde_json::from_slice(request.body().unwrap()).unwrap();
                let actor_enrollment = enrollment.actor.clone().expect("Actor enrollment");
                assert_eq!(
                    actor_enrollment.actor_credential_digest,
                    actr_relay_security::actor_credential_digest(
                        &actor_enrollment.actor_credential
                    )
                );
                let credential = actr_protocol::AIdCredential::decode(
                    actor_enrollment.actor_credential.as_slice(),
                )
                .unwrap();
                let signature = Signature::from_slice(&credential.signature).unwrap();
                response_issuer_signing
                    .verifying_key()
                    .verify(&credential.claims, &signature)
                    .unwrap();
                response_requests
                    .lock()
                    .unwrap()
                    .push(actor_enrollment.clone());
                let relay_claims = serde_json::to_vec(&RelayIdentityClaims {
                    domain: CERTIFICATE_DOMAIN.into(),
                    endpoint: enrollment.endpoint.clone(),
                    actor_type: response_type.to_string_repr(),
                    client_key: "fixture-client".into(),
                    public_key: enrollment.public_key,
                    admission_commitment: enrollment.admission_commitment,
                    expires_at: now + 120,
                })
                .unwrap();
                let signature = response_issuer_signing
                    .sign(&signing_bytes(&relay_claims))
                    .to_bytes()
                    .to_vec();
                let actor_claims = ActorChannelClaims {
                    realm: response_realm.clone(),
                    actor_id: enrollment.endpoint.clone(),
                    actor_type: response_type.to_string_repr(),
                    member: "member-1".into(),
                    client_key: "fixture-client".into(),
                    role: "owner".into(),
                    actor_public_key: response_signer.public_key(),
                    noise_public_key: enrollment.public_key,
                    actor_credential_digest: actor_enrollment.actor_credential_digest,
                    policy_digest: actor_enrollment.policy_digest,
                    actor_state_digest: actor_enrollment.actor_state_digest,
                    issuer: response_realm.clone(),
                    provider: "provider.example".into(),
                    audience: ACTOR_CHANNEL_AUDIENCE.into(),
                    expires_at: now + 120,
                    runtime_instance_id: actor_enrollment.runtime_instance_id,
                };
                let actor_certificate =
                    ActorChannelCertificate::issue(&actor_claims, 1, &response_issuer_signing)
                        .unwrap();
                serde_json::to_vec(&IssuedIdentity {
                    certificate: Certificate {
                        key_id: 1,
                        claims: relay_claims,
                        signature,
                    },
                    issuer: response_issuer.clone(),
                    actor_certificate: Some(actor_certificate),
                })
                .unwrap()
            })
            .expect(3)
            .create_async()
            .await;

        let mut claims = actr_protocol::IdentityClaims {
            realm: realm.clone(),
            actor_id: endpoint.clone(),
            expires_at: now + 600,
            protocol_version: 1,
            issuer: realm.clone(),
            provider: "provider.example".into(),
            audience: "actr.signaling".into(),
            actor_type: actor_type.clone(),
            member: "member-1".into(),
            client_key: "fixture-client".into(),
            role: actr_protocol::MemberRole::Owner as i32,
            development: Some(false),
            actor_public_key: Some(signer.public_key().to_vec().into()),
        };
        let mut snapshot = SessionSnapshot::empty_with_id(local.clone(), 1);
        snapshot.credential = credential(&claims, &issuer_signing);
        snapshot.credential_expires_at = prost_types::Timestamp {
            seconds: claims.expires_at as i64,
            nanos: 0,
        };
        snapshot.renewal_token = bytes::Bytes::from_static(&[3; 32]);
        let session = SessionState::new(snapshot);
        let gate = Arc::new(WebSocketGate::new_relay(
            Arc::new(RpcCorrelation::new()),
            Arc::new(DataChunkRegistry::new()),
            PeerTypes::default(),
        ));
        let client = RelayClient::new(
            "ws://unused".into(),
            server.url(),
            None,
            session.clone(),
            gate.clone(),
            CancellationToken::new(),
            Some(root.path().to_path_buf()),
            Some("tab-cache".into()),
            None,
        );
        let another_client = RelayClient::new(
            "ws://unused".into(),
            server.url(),
            None,
            session.clone(),
            gate,
            CancellationToken::new(),
            Some(root.path().to_path_buf()),
            Some("tab-cache".into()),
            None,
        );
        assert!(!client.runtime_instance_id.is_empty());
        assert_eq!(
            client.runtime_instance_id,
            another_client.runtime_instance_id
        );
        uuid::Uuid::parse_str(&client.runtime_instance_id).unwrap();

        let (_, first) = client
            .certified_identity(&endpoint, &[3; 32])
            .await
            .unwrap();
        let first = first.unwrap();
        let first_digest = first.claims().actor_credential_digest;
        let first_state = first.claims().actor_state_digest;

        claims.expires_at += 1;
        let mut renewed = session.snapshot().await;
        renewed.credential = credential(&claims, &issuer_signing);
        renewed.renewal_token = bytes::Bytes::from_static(&[4; 32]);
        session
            .update_credentials(
                renewed.credential.clone(),
                renewed.credential_expires_at,
                renewed.turn_credential.clone(),
                renewed.renewal_token.clone(),
                renewed.renewal_token_expires_at,
            )
            .await;
        let (_, second) = client
            .certified_identity(&endpoint, &[4; 32])
            .await
            .unwrap();
        let second = second.unwrap();
        assert_ne!(second.claims().actor_credential_digest, first_digest);
        assert_ne!(second.claims().actor_state_digest, first_state);

        let mut rebound = session.snapshot().await;
        rebound.generation = 2;
        session.commit_hard_rebind(rebound).await;
        let (_, third) = client
            .certified_identity(&endpoint, &[4; 32])
            .await
            .unwrap();
        let third = third.unwrap();
        assert_eq!(
            third.claims().actor_credential_digest,
            second.claims().actor_credential_digest
        );
        assert_ne!(
            third.claims().actor_state_digest,
            second.claims().actor_state_digest
        );

        let (_, cached) = client
            .certified_identity(&endpoint, &[4; 32])
            .await
            .unwrap();
        assert!(cached.is_some());
        assert_eq!(requests.lock().unwrap().len(), 3);
        mock.assert_async().await;
    }
}
