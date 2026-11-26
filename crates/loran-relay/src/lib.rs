//! Multiplexed WebSocket relay. Identity, authentication and authorization are
//! supplied by the embedding application; payloads are never interpreted.
//!
//! TLS protects each hop. End-to-end encryption, when required, belongs to the
//! endpoints' payload protocol. This crate does not claim to provide it.

mod registry;

use async_trait::async_trait;
use axum::{
    Router,
    extract::{
        State,
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::StreamExt;
pub use loran_relay_protocol::RelayLimits;
use loran_relay_protocol::{
    ClientControl, MAX_CONTROL_BYTES, MAX_METADATA_BYTES, SUBPROTOCOL, valid_endpoint,
    valid_identifier,
};
use registry::{Peer, Registry};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

/// Authenticated routing identity. The namespace cannot be selected by a
/// frame: it is a result of external authentication.
#[derive(Clone)]
pub struct Principal {
    pub namespace: String,
    pub endpoint: String,
    /// Public, authenticated handshake material delivered to the other peer.
    /// Never place a private authentication secret here.
    pub metadata: String,
    /// Opaque authenticated data used by the embedding authorization policy.
    /// Unlike `metadata`, this is never sent to the peer.
    pub authorization_context: String,
    pub expires_at: Option<SystemTime>,
}

#[derive(Debug, thiserror::Error)]
#[error("relay authentication rejected")]
pub struct AuthError;

#[async_trait]
pub trait Authenticator: Send + Sync + 'static {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError>;

    /// Check whether an existing principal remains authorized.
    /// This synchronous callback must not block or perform network operations.
    fn active(&self, _principal: &Principal) -> bool {
        true
    }

    /// Additional peer policy. Namespace equality is always enforced first.
    /// This synchronous callback must not block or perform network operations.
    fn authorize(&self, _source: &Principal, _target: &Principal) -> bool {
        true
    }

    /// Authorize a new peer pairing without holding the relay registry lock.
    async fn authorize_pair(&self, source: &Principal, target: &Principal) -> bool {
        self.authorize(source, target)
    }
}

/// Optional integration with an embedding application's metrics. Callbacks
/// must be nonblocking and must not retain credentials or payload contents.
pub trait Observer: Send + Sync + 'static {
    fn connected(&self) {}
    fn disconnected(&self) {}
    fn authentication_failed(&self) {}
    fn received_frame(&self) {}
}
impl Observer for () {}

struct ConnectionObservation(Arc<dyn Observer>);
impl Drop for ConnectionObservation {
    fn drop(&mut self) {
        self.0.disconnected();
    }
}

pub(crate) struct Queued {
    message: Message,
    _bytes: OwnedSemaphorePermit,
}

/// A router can be mounted on an existing HTTP/TLS listener or served alone.
pub struct Relay {
    limits: RelayLimits,
    auth: Arc<dyn Authenticator>,
    registry: Mutex<Registry>,
    slots: Arc<Semaphore>,
    shutdown: CancellationToken,
    observer: Arc<dyn Observer>,
}

impl Relay {
    pub fn new(
        limits: RelayLimits,
        auth: Arc<dyn Authenticator>,
        shutdown: CancellationToken,
    ) -> Result<Arc<Self>, &'static str> {
        Self::with_observer(limits, auth, shutdown, Arc::new(()))
    }

    pub fn with_observer(
        limits: RelayLimits,
        auth: Arc<dyn Authenticator>,
        shutdown: CancellationToken,
        observer: Arc<dyn Observer>,
    ) -> Result<Arc<Self>, &'static str> {
        limits.validate()?;
        if limits.max_connections > Semaphore::MAX_PERMITS
            || limits.queue_messages > Semaphore::MAX_PERMITS
            || limits.queue_bytes as usize > Semaphore::MAX_PERMITS
        {
            return Err("relay queue or connection limit exceeds semaphore capacity");
        }
        Ok(Arc::new(Self {
            slots: Arc::new(Semaphore::new(limits.max_connections)),
            limits,
            auth,
            registry: Mutex::new(Registry::default()),
            shutdown,
            observer,
        }))
    }

    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/ws", get(upgrade))
            .with_state(self.clone())
    }

    pub fn connection_count(&self) -> usize {
        self.registry.lock().unwrap().peers.len()
    }

    /// Revoke active sessions without stopping the listener. External policy
    /// must separately prevent credentials from authenticating again.
    pub fn disconnect_all(&self) {
        for peer in self.registry.lock().unwrap().peers.values() {
            peer.cancel.cancel();
        }
    }

    /// Disconnect only sessions whose principals no longer satisfy the
    /// configured authorization policy.
    pub fn disconnect_inactive(&self) -> usize {
        let registry = self.registry.lock().unwrap();
        let mut disconnected = 0;
        for peer in registry.peers.values() {
            if !self.auth.active(&peer.principal) && !peer.cancel.is_cancelled() {
                peer.cancel.cancel();
                disconnected += 1;
            }
        }
        disconnected
    }

    async fn serve(self: Arc<Self>, mut socket: WebSocket, _slot: OwnedSemaphorePermit) {
        let deadline = Duration::from_secs(self.limits.authentication_timeout_secs);
        let authenticate = async {
            let Some(Ok(Message::Text(text))) = socket.next().await else {
                return Err(AuthError);
            };
            if text.len() > MAX_CONTROL_BYTES {
                return Err(AuthError);
            }
            let Ok(ClientControl::Hello { endpoint, token }) = serde_json::from_str(&text) else {
                return Err(AuthError);
            };
            if !valid_endpoint(&endpoint) {
                return Err(AuthError);
            }
            let principal = self.auth.authenticate(&endpoint, &token).await?;
            if principal.endpoint != endpoint
                || principal.namespace.is_empty()
                || principal.namespace.len() > 512
                || principal.metadata.len() > MAX_METADATA_BYTES
                // Escaping can make a public metadata string much larger on
                // the wire. Leave room for endpoint and control fields.
                || serde_json::to_string(&principal.metadata)
                    .map_or(true, |encoded| encoded.len() > MAX_CONTROL_BYTES - 4096)
                || principal
                    .expires_at
                    .is_some_and(|expires| expires <= SystemTime::now())
            {
                return Err(AuthError);
            }
            Ok(principal)
        };
        let principal = tokio::select! {
            _ = self.shutdown.cancelled() => { close(&mut socket, 1001).await; return; }
            result = tokio::time::timeout(deadline, authenticate) => match result {
                Ok(Ok(principal)) => principal,
                _ => { self.observer.authentication_failed(); close(&mut socket, 1008).await; return; }
            }
        };
        self.observer.connected();
        let _observation = ConnectionObservation(self.observer.clone());
        let expiry = principal.expires_at;
        let policy_principal = principal.clone();
        let (tx, mut rx) = mpsc::channel(self.limits.queue_messages);
        let cancel = self.shutdown.child_token();
        let budget = Arc::new(Semaphore::new(self.limits.queue_bytes as usize));
        let key = (principal.namespace.clone(), principal.endpoint.clone());
        let session = self.registry.lock().unwrap().register(Peer {
            principal,
            session: 0,
            tx,
            budget,
            cancel: cancel.clone(),
            channels: Default::default(),
        });
        let expires_in = expiry
            .map(|time| time.duration_since(SystemTime::now()).unwrap_or_default())
            .unwrap_or(Duration::from_secs(100 * 365 * 24 * 3600));
        let expiration = tokio::time::sleep(expires_in);
        tokio::pin!(expiration);
        let mut heartbeat =
            tokio::time::interval(Duration::from_secs(self.limits.heartbeat_interval_secs));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        let mut maintenance = tokio::time::interval(Duration::from_secs(1));
        maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_pong = tokio::time::Instant::now();
        let mut close_code = 1001;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = &mut expiration => { close_code = 1008; break; }
                message = rx.recv() => {
                    let Some(queued) = message else { break; };
                    if !matches!(tokio::time::timeout(Duration::from_secs(self.limits.write_timeout_secs), socket.send(queued.message)).await, Ok(Ok(()))) { break; }
                }
                incoming = socket.next() => {
                    if !self.auth.active(&policy_principal) { close_code = 1008; break; }
                    match incoming {
                        Some(Ok(Message::Text(text))) if text.len() <= MAX_CONTROL_BYTES => {
                            self.observer.received_frame();
                            let Ok(control) = serde_json::from_str::<ClientControl>(&text) else { close_code = 1002; break; };
                            match control {
                                ClientControl::Connect { request, peer } if valid_identifier(request) && valid_endpoint(&peer) => {
                                    let pair = {
                                        let registry = self.registry.lock().unwrap();
                                        if !registry.current(&key, session) { break; }
                                        registry.admission_pair(&key, &peer)
                                    };
                                    let target_session = match pair {
                                        Some((source, target, target_session)) => {
                                            let authorized = tokio::select! {
                                                _ = cancel.cancelled() => break,
                                                result = tokio::time::timeout(
                                                    Duration::from_secs(self.limits.connect_timeout_secs),
                                                    self.auth.authorize_pair(&source, &target),
                                                ) => result.unwrap_or(false),
                                            };
                                            authorized.then_some(target_session)
                                        }
                                        None => None,
                                    };
                                    let mut registry = self.registry.lock().unwrap();
                                    if !registry.current(&key, session) { break; }
                                    registry.connect(&key, request, &peer, &self.limits, self.auth.as_ref(), target_session);
                                }
                                ClientControl::Accept { channel } if valid_identifier(channel) => {
                                    let pair = {
                                        let registry = self.registry.lock().unwrap();
                                        if !registry.current(&key, session) { break; }
                                        registry.pending_pair(&key, channel)
                                    };
                                    let (expected, authorized) = match pair {
                                        Some((source, target, source_session, target_session)) => {
                                            let authorized = tokio::select! {
                                                _ = cancel.cancelled() => break,
                                                result = tokio::time::timeout(
                                                    Duration::from_secs(self.limits.connect_timeout_secs),
                                                    self.auth.authorize_pair(&source, &target),
                                                ) => result.unwrap_or(false),
                                            };
                                            (Some((source_session, target_session)), authorized)
                                        }
                                        None => (None, false),
                                    };
                                    let mut registry = self.registry.lock().unwrap();
                                    if !registry.current(&key, session) { break; }
                                    registry.accept(&key, channel, expected, authorized, self.auth.as_ref());
                                }
                                ClientControl::Reject { channel } if valid_identifier(channel) => {
                                    let mut registry = self.registry.lock().unwrap();
                                    if !registry.current(&key, session) { break; }
                                    registry.reject(&key, channel);
                                }
                                ClientControl::Close { channel } if valid_identifier(channel) => {
                                    let mut registry = self.registry.lock().unwrap();
                                    if !registry.current(&key, session) { break; }
                                    registry.close_channel(&key, channel);
                                }
                                _ => { close_code = 1002; break; }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            self.observer.received_frame();
                            if loran_relay_protocol::decode_data(data.clone(), self.limits.max_frame_bytes).is_err() { close_code = 1002; break; }
                            let registry = self.registry.lock().unwrap();
                            if !registry.current(&key, session) { break; }
                            registry.forward(&key, data, self.auth.as_ref());
                        }
                        Some(Ok(Message::Pong(_))) => last_pong = tokio::time::Instant::now(),
                        Some(Ok(Message::Ping(data))) => {
                            if !matches!(tokio::time::timeout(Duration::from_secs(self.limits.write_timeout_secs), socket.send(Message::Pong(data))).await, Ok(Ok(()))) { break; }
                        }
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        _ => { close_code = 1002; break; }
                    }
                }
                _ = heartbeat.tick() => {
                    if last_pong.elapsed() >= Duration::from_secs(self.limits.heartbeat_timeout_secs) { break; }
                    if !matches!(tokio::time::timeout(Duration::from_secs(self.limits.write_timeout_secs), socket.send(Message::Ping(bytes::Bytes::new()))).await, Ok(Ok(()))) { break; }
                }
                _ = maintenance.tick() => {
                    if !self.auth.active(&policy_principal) { close_code = 1008; break; }
                    self.registry.lock().unwrap().expire_pending(&key, Duration::from_secs(self.limits.connect_timeout_secs));
                }
            }
        }
        self.registry.lock().unwrap().remove(&key, session);
        close(&mut socket, close_code).await;
    }
}

async fn close(socket: &mut WebSocket, code: u16) {
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        socket.send(Message::Close(Some(CloseFrame {
            code,
            reason: "".into(),
        }))),
    )
    .await;
}

async fn upgrade(
    State(relay): State<Arc<Relay>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let offered = headers
        .get("sec-websocket-protocol")
        .and_then(|header| header.to_str().ok());
    if !offered.is_some_and(|header| header.split(',').any(|value| value.trim() == SUBPROTOCOL)) {
        return (StatusCode::BAD_REQUEST, "relay subprotocol required").into_response();
    }
    if let Some(origin) = headers.get("origin") {
        if !relay.limits.allowed_origins.is_empty()
            && !origin.to_str().ok().is_some_and(|value| {
                relay
                    .limits
                    .allowed_origins
                    .iter()
                    .any(|allowed| allowed == value)
            })
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let Ok(slot) = relay.slots.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if relay.shutdown.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.protocols([SUBPROTOCOL])
        .max_frame_size(relay.limits.max_frame_bytes)
        .max_message_size(relay.limits.max_frame_bytes)
        .on_upgrade(move |socket| relay.serve(socket, slot))
}
