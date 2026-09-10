//! signaling clientImplementation
//!
//! Based on protobuf Definition'ssignalingprotocol, using SignalingEnvelope conclude construct

#[cfg(feature = "opentelemetry")]
use super::trace;
use crate::lifecycle::membership::{MembershipHandle, MembershipResolution};
use crate::lifecycle::{
    CredentialState, SignalingFactLostCause, SignalingFactOrigin, SignalingRecoveryRequestCause,
    SupervisorFactSink,
};
use crate::transport::{NetworkError, NetworkResult};
#[cfg(feature = "opentelemetry")]
use crate::wire::webrtc::trace::extract_trace_context;
use actr_framework::WebRtcPeerStatus;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    AIdCredential, ActrId, ActrToSignaling, GetSigningKeyRequest, PeerToSignaling, Ping, Pong,
    RegisterRequest, RegisterResponse, RouteCandidatesRequest, RouteCandidatesResponse,
    SIGNALING_ENVELOPE_VERSION, ServiceAvailabilityState, SignalingEnvelope, UnregisterRequest,
    UnregisterResponse, actr_to_signaling, peer_to_signaling, signaling_envelope,
    signaling_to_actr,
};
use async_trait::async_trait;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Semaphore, broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_with_config};
#[cfg(feature = "opentelemetry")]
use tracing_opentelemetry::OpenTelemetrySpanExt;
use url::Url;

/// WebSocket sink type alias for the split write half of a signaling connection
type WsSink = Arc<
    tokio::sync::Mutex<
        Option<
            futures_util::stream::SplitSink<
                WebSocketStream<MaybeTlsStream<TcpStream>>,
                tokio_tungstenite::tungstenite::Message,
            >,
        >,
    >,
>;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Constants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Default timeout in seconds for waiting for signaling request/response RPCs.
const RESPONSE_TIMEOUT_SECS: u64 = 5;
// WebSocket-level keepalive to detect silent half-open connections
const PING_INTERVAL_SECS: u64 = 5;
const PONG_TIMEOUT_SECS: u64 = 10;
const SIGNALING_SEND_TIMEOUT_SECS: u64 = 5;
#[cfg(test)]
const CONCURRENT_CONNECT_WAIT_TIMEOUT_SECS: u64 = 5;
const DISCONNECT_LOCK_TIMEOUT_SECS: u64 = 5;
const NO_IDEMPOTENT_RETRY_GENERATION: u64 = u64::MAX;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// configurationType
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// signalingconfiguration
#[derive(Debug, Clone)]
pub struct SignalingConfig {
    /// signaling server URL
    pub server_url: Url,

    /// Connecttimeout temporal duration (seconds)
    pub connection_timeout: u64,

    /// center skipinterval (seconds)
    pub heartbeat_interval: u64,

    /// reconnection configuration
    pub reconnect_config: ReconnectConfig,

    /// acknowledge verify configuration
    pub auth_config: Option<AuthConfig>,

    /// WebRTC role preference: "answer" if this node has advanced config
    pub webrtc_role: Option<String>,

    /// Bounded inbound envelope queue settings (issue #351).
    ///
    /// Defaults to a safe bounded configuration; see [`InboundQueueConfig`].
    pub inbound_queue: InboundQueueConfig,
}

/// reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Whether compatibility callers may publish automatic reconnect demand.
    /// The client never starts a reconnect loop itself; the supervisor decides
    /// whether that demand may turn into a connection action.
    pub enabled: bool,

    /// Maximum attempts in one supervisor-owned retry window. When the window
    /// is exhausted, recovery remains pending through a cooldown and begins a
    /// fresh window afterward.
    pub max_attempts: u32,

    /// Initial supervisor-owned reconnection backoff delay (seconds).
    pub initial_delay: u64,

    /// Maximum supervisor-owned backoff delay and the basis for the cooldown
    /// duration (seconds).
    pub max_delay: u64,

    /// Supervisor-owned backoff multiplier factor.
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 10,
            initial_delay: 1,
            max_delay: 60,
            backoff_multiplier: 2.0,
        }
    }
}

/// Default bound on queued unmatched/push envelopes (message count).
pub const DEFAULT_INBOUND_MAX_MESSAGES: usize = 256;
/// Default bound on queued unmatched/push envelope bytes (byte budget).
pub const DEFAULT_INBOUND_MAX_BYTES: usize = 1 << 20; // 1 MiB
/// Default maximum size of a single inbound signaling frame. Larger frames are
/// rejected before protobuf decode/allocation.
pub const DEFAULT_INBOUND_MAX_FRAME_BYTES: usize = 256 << 10; // 256 KiB
/// Default deadline (milliseconds) the receiver waits for inbound queue
/// capacity before failing closed and reconnecting.
pub const DEFAULT_INBOUND_BACKPRESSURE_TIMEOUT_MS: u64 = 2_000;

/// Bounded, byte-budgeted inbound envelope queue settings (issue #351).
///
/// The signaling client demultiplexes direct RPC replies by `reply_for`
/// (never queued) and forwards unmatched/push envelopes (ActrRelay, presence)
/// into a single bounded queue consumed by `receive_envelope`. These settings
/// bound that queue by both message count and encoded bytes, cap the size of a
/// single inbound frame, and bound how long the receiver applies backpressure
/// before failing closed (closing the socket and reconnecting) rather than
/// silently dropping signaling messages.
#[derive(Debug, Clone, Copy)]
pub struct InboundQueueConfig {
    /// Maximum number of queued unmatched/push envelopes (message-count bound).
    pub max_messages: usize,
    /// Maximum total encoded bytes held in the inbound queue (byte budget).
    pub max_bytes: usize,
    /// Maximum size of a single inbound signaling frame in bytes. Frames larger
    /// than this are rejected before protobuf decode/allocation.
    pub max_frame_bytes: usize,
    /// How long the receiver waits for inbound queue capacity before treating
    /// the consumer as stalled and failing closed (reconnect). This applies
    /// TCP/WebSocket backpressure instead of dropping messages.
    pub backpressure_timeout: Duration,
}

impl Default for InboundQueueConfig {
    fn default() -> Self {
        Self {
            max_messages: DEFAULT_INBOUND_MAX_MESSAGES,
            max_bytes: DEFAULT_INBOUND_MAX_BYTES,
            max_frame_bytes: DEFAULT_INBOUND_MAX_FRAME_BYTES,
            backpressure_timeout: Duration::from_millis(DEFAULT_INBOUND_BACKPRESSURE_TIMEOUT_MS),
        }
    }
}

/// acknowledge verify configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// acknowledge verify Type
    pub auth_type: AuthType,

    /// acknowledge verify credential data
    pub credentials: HashMap<String, String>,
}

/// acknowledge verify Type
#[derive(Debug, Clone)]
pub enum AuthType {
    /// no acknowledge verify
    None,
    /// Bearer Token
    BearerToken,
    /// API Key
    ApiKey,
    /// JWT
    Jwt,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Client interface and implementation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// signaling client connect port
///
/// # interior mutability
/// allMethodusing `&self` and non `&mut self`, with for conveniencein Arc in shared.
/// Implementation class needs interior mutability (like Mutex) to manage WebSocket connection status.
#[async_trait]
pub trait SignalingClient: Send + Sync {
    /// Connecttosignaling server
    async fn connect(&self) -> NetworkResult<()>;

    /// Perform one supervisor-owned recovery attempt.
    ///
    /// Implementations may report an authentication verdict to their
    /// credential authority, but must not sleep or start another socket
    /// attempt. The supervisor owns every subsequent retry deadline.
    async fn connect_once_for_recovery(&self, _action_id: u64) -> NetworkResult<()> {
        self.connect().await
    }

    /// Report that signaling connectivity is required. This method is
    /// synchronous so heartbeat and resource-loss paths can emit a fact while
    /// holding no async recovery ownership.
    fn request_signaling_recovery(&self, _cause: SignalingRecoveryRequestCause) {}

    /// DisconnectConnect
    async fn disconnect(&self) -> NetworkResult<()>;

    /// Disconnect a stale signaling socket as part of an automatic recovery.
    /// Implementations may preserve the fact that idempotent requests
    /// (discovery, key fetch) are eligible for one retry after the
    /// replacement connects.
    async fn disconnect_for_recovery(&self) -> NetworkResult<()> {
        self.disconnect().await
    }

    /// Probe whether the existing signaling WebSocket is truly alive.
    ///
    /// The default implementation only checks local state. WebSocket-backed
    /// clients override this with an active Ping/Pong probe to catch half-open
    /// sockets before network recovery decides whether to reconnect.
    async fn probe_alive(&self, _timeout: Duration) -> NetworkResult<()> {
        if self.is_connected() {
            Ok(())
        } else {
            Err(NetworkError::ConnectionError(
                "Signaling client is not connected".to_string(),
            ))
        }
    }

    /// Deprecated: Registration now happens via AIS HTTP; this WS path is no longer used.
    /// Kept for backward compatibility; will be removed in a future release.
    async fn send_register_request(
        &self,
        request: RegisterRequest,
    ) -> NetworkResult<RegisterResponse>;

    /// Send UnregisterRequest to signaling server (Actr → Signaling flow)
    ///
    /// This is used when an Actor is shutting down gracefully and wants to
    /// proactively notify the signaling server that it is no longer available.
    async fn send_unregister_request(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        reason: Option<String>,
    ) -> NetworkResult<UnregisterResponse>;

    /// Send center skip (Registerafter stream process, using ActrToSignaling)
    /// Returns Pong response if received, error if timeout or no response
    async fn send_heartbeat(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        availability: ServiceAvailabilityState,
        power_reserve: f32,
        mailbox_backlog: f32,
    ) -> NetworkResult<Pong>;

    /// Send RouteCandidatesRequest (requires authenticated Actor session)
    async fn send_route_candidates_request(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        request: RouteCandidatesRequest,
    ) -> NetworkResult<RouteCandidatesResponse>;

    /// Query AIS Ed25519 signing public key via signaling
    ///
    /// Returns `(key_id, pubkey_bytes)` where pubkey_bytes is the 32-byte raw public key.
    /// Typically called by AisKeyCache on cache miss; should not be used directly in hot paths.
    async fn get_signing_key(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        key_id: u32,
    ) -> NetworkResult<(u32, Vec<u8>)>;

    /// Sendsignalingsignal seal (pass usage Method)
    async fn send_envelope(&self, envelope: SignalingEnvelope) -> NetworkResult<()>;

    /// Receivesignalingsignal seal
    async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>>;

    /// Receive an envelope together with its library-only receive span. Legacy
    /// implementations inherit a current-span fallback; the WebSocket client
    /// preserves the span created by its bounded receive queue.
    async fn receive_envelope_with_span(
        &self,
    ) -> NetworkResult<Option<(SignalingEnvelope, tracing::Span)>> {
        self.receive_envelope()
            .await
            .map(|envelope| envelope.map(|envelope| (envelope, tracing::Span::current())))
    }

    /// Check connection status
    fn is_connected(&self) -> bool;

    /// GetConnect statistics info
    fn get_stats(&self) -> SignalingStats;
    /// Subscribe to signaling events (state transitions).
    fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent>;

    /// Set actor ID and credential state for reconnect URL parameters.
    async fn set_actor_id(&self, actor_id: ActrId);
    async fn set_credential_state(&self, credential_state: CredentialState);

    /// Clear stored actor ID and credential state.
    ///
    /// After calling this, `connect()` will produce a clean WebSocket URL
    /// without identity query parameters, so the signaling server treats
    /// the connection as brand-new rather than a reconnect of the old actor.
    /// This is required before re-registration when the credential has expired.
    async fn clear_identity(&self);

    /// Set a lifecycle hook callback for signaling state changes.
    ///
    /// State-transition paths normally await the ordered enqueue operation.
    /// Envelope send failures enqueue from a detached task because their
    /// callers may hold a peer signaling commit gate that re-entrant cleanup
    /// also needs.
    /// Default implementation is a no-op for clients that don't support hooks.
    fn set_hook_callback(&self, _cb: HookCallback) {}

    /// Attach the supervisor fact sink so authoritative signaling generation
    /// transitions (`SignalingGenerationCommitted` / `SignalingGenerationLost`)
    /// are reported to the connection supervisor as normalized facts.
    ///
    /// Default is a no-op for clients that do not drive lifecycle recovery.
    fn set_supervisor_fact_sink(&self, _sink: SupervisorFactSink) {}

    /// Return the generation of the currently connected signaling socket in a
    /// test harness.
    ///
    /// Production recovery must consume the normalized facts published through
    /// [`Self::set_supervisor_fact_sink`] instead of polling this value. The
    /// probe exists only for integration harnesses that need to assert the
    /// exact generation carried by a manually injected resource fact.
    #[cfg(any(test, feature = "test-utils"))]
    fn live_generation_for_test(&self) -> Option<u64> {
        None
    }

    /// Test-only probe that exercises the concrete client's production fact
    /// emission path through the sink installed above.
    #[cfg(test)]
    fn emit_test_signaling_committed(&self, _generation: u64, _origin: SignalingFactOrigin) {}

    /// Synchronously invalidate the current signaling generation: bump the
    /// connection generation so no in-flight attempt can still commit, and
    /// suppress new automatic attempts. A bounded teardown calls this before any
    /// physical step so a later connect cannot race the teardown across its
    /// commit boundary. It must not await or perform I/O; implementations may
    /// enter a bounded synchronous commit critical section. Default is a no-op.
    fn invalidate_generation(&self) {}

    /// Install the membership authority wiring (self-healing credential
    /// lifecycle). Default is a no-op for clients that don't participate.
    ///
    /// When installed, the client builds handshake URLs from the controller's
    /// credential watch and routes typed auth verdicts through its handle.
    async fn install_membership(&self, _handle: MembershipHandle) {}
}

/// High-level signaling connection state (kept for quick boolean checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
}

/// Signaling state transition events.
///
/// Unlike `ConnectionState` (which is a snapshot), these represent discrete
/// transitions and are delivered via `broadcast` so every subscriber sees
/// every event, even if the same state occurs twice in a row.
#[derive(Debug, Clone)]
pub enum SignalingEvent {
    /// About to start a connection attempt (includes retry count).
    ConnectStart { attempt: u32 },
    /// Connection successfully established.
    Connected,
    /// Connection lost.
    Disconnected { reason: DisconnectReason },
}

/// Reason why the signaling connection was lost.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// WebSocket stream ended (receiver task exited normally).
    StreamEnded,
    /// No Pong received within the timeout window.
    PongTimeout,
    /// Failed to send a WebSocket Ping frame.
    PingSendFailed,
    /// Failed to send a signaling envelope on the active WebSocket.
    SendFailed,
    /// Timed out waiting for the signaling sink or sending an envelope.
    SendTimeout,
    /// Credential expired (heartbeat 401).
    CredentialExpired,
    /// Explicit disconnect() call or external trigger.
    Manual,
    /// Connection attempt failed with an error.
    ConnectionFailed(String),
    /// The inbound signaling queue stayed saturated past its backpressure
    /// deadline, so the client failed closed to force a reconnect rather than
    /// silently drop signaling messages.
    BackpressureSaturated,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Hook callback for ordered lifecycle notification
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Events that trigger workload lifecycle hooks.
///
/// Used by `HookCallback` to enqueue workload hooks in emission order at the
/// point where the state change occurs.
#[derive(Clone, Debug)]
pub enum HookEvent {
    // ── Signaling ──
    SignalingConnectStart {
        attempt: u32,
    },
    SignalingConnected,
    SignalingDisconnected,
    // ── WebRTC ──
    WebRtcConnectStart {
        peer_id: ActrId,
    },
    WebRtcConnected {
        peer_id: ActrId,
        relayed: bool,
    },
    WebRtcDisconnected {
        peer_id: ActrId,
        status: WebRtcPeerStatus,
    },
    /// S6-5: a revision of the peer's unified state (v0.20 backend). Carries
    /// the full FFI-safe snapshot; fires on every snapshot revision advance
    /// (initial replay included for late-installed workloads).
    WebRtcStateChanged {
        state: actr_framework::WebRtcPeerState,
    },
    /// A delivery-uncertainty event for outbound data. `operation_id`
    /// occupies the same caller-visible reconciliation namespace as the
    /// `DeliveryUncertain` error, in one of two provably disjoint forms:
    /// chunk-scoped emitters pass
    /// [`data_chunk_operation_id`](crate::outbound::data_chunk_operation_id)
    /// (`{stream_id}#{sequence}`, tail `#` + decimal sequence); stream-scoped
    /// emitters (a session close invalidating every outstanding chunk at
    /// once, where no single sequence is authoritative) pass
    /// [`data_stream_operation_id`](crate::outbound::data_stream_operation_id)
    /// (`{stream_id}#stream`, non-numeric tail). A stream id may itself
    /// contain `#`, so emitters must never hand-compose a bare stream id —
    /// SDK reconciliation maps keyed by operation id rely on the two scopes
    /// never colliding.
    DataChunkDeliveryUncertain {
        operation_id: String,
        /// `None` means the session is unknown or not applicable: the
        /// synchronous send classification happens before or independently of
        /// session selection (the failure may even travel over WebSocket).
        /// Emitters that can name the WebRTC session invalidated by a
        /// disconnect pass `Some(session_id)` so reconciliation can group the
        /// uncertainty by session.
        session_id: Option<u64>,
        reason: String,
    },
    // ── WebSocket ──
    WebSocketConnectStart {
        peer_id: ActrId,
    },
    WebSocketConnected {
        peer_id: ActrId,
    },
    WebSocketDisconnected {
        peer_id: ActrId,
    },
    // ── Credential ──
    CredentialRenewed {
        new_expiry: std::time::SystemTime,
    },
    CredentialExpiring {
        new_expiry: std::time::SystemTime,
    },
    // ── Mailbox ──
    MailboxBackpressure {
        queue_len: usize,
        threshold: usize,
    },
}

/// Callback closure used when a hook event occurs.
///
/// Set once via `set_hook_callback()`. State-change paths normally await the
/// enqueue operation; observer execution is handled by the callback's FIFO
/// dispatcher.
pub type HookCallback =
    Arc<dyn Fn(HookEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Debug, Clone, Copy)]
enum ConnectIntent {
    Explicit { generation: u64 },
    Recovery { generation: u64, action_id: u64 },
}

impl ConnectIntent {
    fn generation(self) -> u64 {
        match self {
            Self::Explicit { generation } | Self::Recovery { generation, .. } => generation,
        }
    }

    fn fact_origin(self) -> SignalingFactOrigin {
        match self {
            Self::Recovery { action_id, .. } => SignalingFactOrigin::CurrentEffect { action_id },
            Self::Explicit { .. } => SignalingFactOrigin::External,
        }
    }

    fn action_id(self) -> Option<u64> {
        match self {
            Self::Recovery { action_id, .. } => Some(action_id),
            Self::Explicit { .. } => None,
        }
    }
}

/// Authoritative signaling socket-resource state. The boolean fields retained
/// by the client are read-optimized projections of this state, not independent
/// lifecycle machines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalingSocketState {
    Disconnected {
        last_generation: Option<u64>,
    },
    Connecting {
        expected_generation: u64,
        action_id: Option<u64>,
    },
    Connected {
        generation: u64,
        action_id: Option<u64>,
    },
    Disconnecting {
        generation: u64,
    },
}

/// Cancellation-safe ownership of the signaling single-flight slot.
///
/// Futures that own a connection attempt can be dropped by timeout, shutdown,
/// or task abortion at any `.await`. Releasing the atomic flag from `Drop`
/// prevents one cancelled future from permanently blocking every later
/// connection attempt.
struct ConnectionAttemptGuard<'a> {
    connecting: &'a AtomicBool,
    completion: &'a tokio::sync::Notify,
    deadline: &'a Mutex<Option<tokio::time::Instant>>,
    commit_gate: &'a Mutex<()>,
    socket_state: &'a Mutex<SignalingSocketState>,
    intent: ConnectIntent,
}

impl Drop for ConnectionAttemptGuard<'_> {
    fn drop(&mut self) {
        *self
            .deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        {
            // Clear the typed resource state before releasing the single-flight
            // flag. Otherwise a same-generation explicit follower could become
            // leader and publish a new `Connecting` state that this old guard
            // would then mistake for its own.
            let _commit_guard = self
                .commit_gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut state = self
                .socket_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if matches!(
                *state,
                SignalingSocketState::Connecting {
                    expected_generation,
                    action_id,
                } if expected_generation == self.intent.generation()
                    && action_id == self.intent.action_id()
            ) {
                *state = SignalingSocketState::Disconnected {
                    last_generation: Some(self.intent.generation()),
                };
            }
        }
        self.connecting.store(false, Ordering::Release);
        self.completion.notify_waiters();
    }
}

/// Outcome of routing a reconnect error through the membership authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthVerdictAction {
    /// A newer credential was published after the auth verdict was reported;
    /// its revision fact will wake the supervisor-owned recovery record.
    Reacquired,
    /// AIS was temporarily unavailable. Return a retryable result so the
    /// supervisor can arm its identity-bearing backoff.
    RetrySameCredential,
    /// The realm denied membership. Recovery needs a new external trigger.
    Denied,
    /// The credential owner shut down while the action was waiting.
    ShutdownWhileParked,
    /// The error was not an auth verdict (or no controller is wired); the caller
    /// reports the original typed error to the supervisor.
    NotAVerdict,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Bounded inbound envelope queue (issue #351)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Why an unmatched/push envelope could not be queued for the consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InboundEnqueueError {
    /// The queue stayed full past the backpressure deadline; the caller must
    /// fail closed (close the socket and reconnect) instead of dropping the
    /// message.
    Saturated,
    /// The inbound channel was closed (reset/disconnect); the receiver should
    /// stop reading.
    Closed,
}

/// An unmatched/push envelope together with the byte-budget permit that
/// reserved its queue capacity.
///
/// The permit is released (returning the reserved bytes to the budget) when
/// this value is dropped — after the consumer dequeues and processes the
/// envelope, or when the queue is drained/reset without delivery.
struct QueuedInbound {
    envelope: SignalingEnvelope,
    span: tracing::Span,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Bounded, byte-budgeted producer handle for the inbound unmatched/push queue.
///
/// A fresh queue is built per session (see [`WebSocketSignalingClient::reset_inbound_channel`])
/// and cloned into the per-connection receiver task. Sending applies
/// backpressure: when the queue is full it awaits capacity up to
/// `backpressure_timeout`, then reports [`InboundEnqueueError::Saturated`] so the
/// receiver can fail closed instead of silently dropping signaling messages.
#[derive(Clone)]
struct InboundEgress {
    tx: mpsc::Sender<QueuedInbound>,
    byte_budget: Arc<Semaphore>,
    config: InboundQueueConfig,
    stats: Arc<AtomicSignalingStats>,
}

impl InboundEgress {
    /// Build a fresh bounded queue and its consuming receiver.
    fn new(config: InboundQueueConfig, stats: Arc<AtomicSignalingStats>) -> (Self, InboundQueueRx) {
        let (tx, rx) = mpsc::channel(config.max_messages.max(1));
        let egress = Self {
            tx,
            byte_budget: Arc::new(Semaphore::new(config.max_bytes)),
            config,
            stats,
        };
        (egress, rx)
    }

    /// Reserve byte budget and a message slot, then enqueue the envelope.
    ///
    /// Returns `Saturated` if capacity is not granted within
    /// `backpressure_timeout`; `Closed` if the channel went away.
    #[cfg(test)]
    async fn send(
        &self,
        envelope: SignalingEnvelope,
        byte_len: usize,
    ) -> Result<(), InboundEnqueueError> {
        self.send_with_span(envelope, byte_len, tracing::Span::current())
            .await
    }

    async fn send_with_span(
        &self,
        envelope: SignalingEnvelope,
        byte_len: usize,
        span: tracing::Span,
    ) -> Result<(), InboundEnqueueError> {
        let reserve = async {
            // `byte_len` is bounded by `max_frame_bytes` (checked before decode),
            // which is <= `max_bytes`, so it always fits in a u32 permit count.
            let permits = u32::try_from(byte_len).unwrap_or(u32::MAX);
            let permit = self
                .byte_budget
                .clone()
                .acquire_many_owned(permits)
                .await
                .map_err(|_| InboundEnqueueError::Closed)?;
            self.tx
                .send(QueuedInbound {
                    envelope,
                    span,
                    _permit: permit,
                })
                .await
                .map_err(|_| InboundEnqueueError::Closed)
        };

        match crate::timer::timeout(
            crate::timer::ids::SIGNALING_INBOUND_BACKPRESSURE,
            self.config.backpressure_timeout,
            reserve,
        )
        .await
        {
            Ok(result) => {
                if result.is_ok() {
                    self.stats.inbound_enqueued.fetch_add(1, Ordering::Relaxed);
                }
                result
            }
            Err(_) => {
                self.stats.inbound_saturated.fetch_add(1, Ordering::Relaxed);
                Err(InboundEnqueueError::Saturated)
            }
        }
    }

    /// Current number of queued envelopes (observability gauge).
    #[cfg(any(test, feature = "test-utils"))]
    fn depth(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }

    /// Current number of queued encoded bytes (observability gauge).
    #[cfg(any(test, feature = "test-utils"))]
    fn queued_bytes(&self) -> usize {
        self.config
            .max_bytes
            .saturating_sub(self.byte_budget.available_permits())
    }
}

/// Receiving half of the bounded inbound queue.
type InboundQueueRx = mpsc::Receiver<QueuedInbound>;

/// WebSocket signaling clientImplementation
pub struct WebSocketSignalingClient {
    config: SignalingConfig,
    actor_id: tokio::sync::Mutex<Option<ActrId>>,
    credential_state: tokio::sync::Mutex<Option<CredentialState>>,
    /// WebSocket write end (using Mutex Implementation interior mutability)
    ws_sink: WsSink,
    /// WebSocket read end (using Mutex Implementation interior mutability)
    ws_stream: tokio::sync::Mutex<
        Option<futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    >,
    /// connection status
    connected: Arc<AtomicBool>,
    /// Connection in progress flag (prevents concurrent connect attempts)
    connecting: Arc<AtomicBool>,
    /// Typed socket lifecycle protected by the same synchronous facade as
    /// generation validation and socket publication.
    socket_state: Arc<Mutex<SignalingSocketState>>,
    /// statistics info
    stats: Arc<AtomicSignalingStats>,
    /// Envelope count number device
    envelope_counter: tokio::sync::Mutex<u64>,
    /// Pending reply waiters (reply_for -> oneshot)
    pending_replies: Arc<tokio::sync::Mutex<HashMap<String, oneshot::Sender<SignalingEnvelope>>>>,
    /// Pending WebSocket Pong waiters (ping payload -> oneshot)
    pending_pongs: Arc<tokio::sync::Mutex<HashMap<Vec<u8>, oneshot::Sender<()>>>>,
    /// Monotonic probe payload counter.
    probe_counter: AtomicU64,
    /// Inbound envelope queue for unmatched messages (ActrRelay / push).
    ///
    /// The receiving half is consumed by the single serialized
    /// `receive_envelope` caller; the bounded producing half is cloned into the
    /// per-connection receiver task and applies backpressure when full.
    inbound_rx: Arc<tokio::sync::Mutex<InboundQueueRx>>,
    inbound_egress: tokio::sync::Mutex<InboundEgress>,
    /// Background receive task handle to allow graceful shutdown
    receiver_task: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Background ping task to detect half-open connections
    ping_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Connection state broadcast channel (event-driven)
    event_tx: broadcast::Sender<SignalingEvent>,
    /// Incremented whenever the current connection observes inbound traffic.
    ///
    /// The ping task measures silence in completed active-time ticks instead
    /// of comparing `std::time::Instant` values, whose suspend behavior is not
    /// portable (Windows counts sleep).
    liveness_generation: Arc<AtomicU64>,
    /// Incremented by lifecycle disconnects to invalidate every in-flight
    /// connection attempt, whether explicit or automatic. Shared with the
    /// background receiver/ping tasks so spontaneous-loss handlers can arm
    /// the idempotent-retry mark for the live generation.
    connection_generation: Arc<AtomicU64>,
    /// Connection generation whose dropped idempotent request waiters
    /// (RouteCandidates, GetSigningKey) may be retried once after an
    /// automatic recovery. Manual teardown clears this mark. Shared with the
    /// background tasks that publish spontaneous disconnect transitions.
    idempotent_retry_generation: Arc<AtomicU64>,
    /// Serializes the generation linearization point with connection state
    /// publication. No `.await` is allowed while this gate is held.
    connection_commit_gate: Arc<Mutex<()>>,
    /// Wakes connection waiters when lifecycle invalidates their generation.
    ///
    /// Connection invalidation and attempt completion have separate channels,
    /// so cancellation cannot consume a completion notification.
    connection_generation_notify: tokio::sync::Notify,
    /// Wakes tasks waiting for the current single-flight connection attempt.
    connection_attempt_notify: tokio::sync::Notify,
    /// Absolute deadline owned by the active single-flight connection attempt.
    /// Followers observe this exact value instead of inventing a shorter wait.
    connection_attempt_deadline: Mutex<Option<tokio::time::Instant>>,
    /// Hook callback for ordered lifecycle notification (set once, lock-free read)
    hook_callback: OnceLock<HookCallback>,
    /// Supervisor fact sink for normalized signaling generation facts (set once,
    /// lock-free read). The cell is shared with the background receiver and
    /// liveness tasks so late lifecycle wiring is visible to an already-open
    /// socket as well as to sockets opened afterwards.
    supervisor_fact_sink: Arc<OnceLock<SupervisorFactSink>>,
    /// Deterministic two-phase barrier for the generation/publication race.
    #[cfg(any(test, feature = "test-utils"))]
    connection_publish_barrier: OnceLock<Arc<tokio::sync::Barrier>>,
    /// Membership authority wiring (set once when the membership controller is
    /// enabled). When present:
    /// - `build_url_with_identity` reads the credential from the `watch`
    ///   receiver instead of the stored `credential_state`, so the handshake URL
    ///   always carries the latest published credential.
    /// - A supervisor recovery effect submits a tracked on-demand credential
    ///   request. Publication emits a material fact; transient AIS failure is
    ///   returned for supervisor-owned backoff.
    membership: OnceLock<MembershipHandle>,
}

impl WebSocketSignalingClient {
    /// Create new WebSocket signaling client
    pub fn new(config: SignalingConfig) -> Self {
        let stats = Arc::new(AtomicSignalingStats::default());
        let (inbound_egress, inbound_rx) = InboundEgress::new(config.inbound_queue, stats.clone());
        let (event_tx, _event_rx) = broadcast::channel(64);
        Self {
            config,
            actor_id: tokio::sync::Mutex::new(None),
            credential_state: tokio::sync::Mutex::new(None),
            ws_sink: Arc::new(tokio::sync::Mutex::new(None)),
            ws_stream: tokio::sync::Mutex::new(None),
            connected: Arc::new(AtomicBool::new(false)),
            connecting: Arc::new(AtomicBool::new(false)),
            socket_state: Arc::new(Mutex::new(SignalingSocketState::Disconnected {
                last_generation: None,
            })),
            stats,
            envelope_counter: tokio::sync::Mutex::new(0),
            pending_replies: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            pending_pongs: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            probe_counter: AtomicU64::new(0),
            inbound_rx: Arc::new(tokio::sync::Mutex::new(inbound_rx)),
            inbound_egress: tokio::sync::Mutex::new(inbound_egress),
            receiver_task: Arc::new(tokio::sync::Mutex::new(None)),
            ping_task: tokio::sync::Mutex::new(None),
            event_tx,
            liveness_generation: Arc::new(AtomicU64::new(0)),
            connection_generation: Arc::new(AtomicU64::new(0)),
            idempotent_retry_generation: Arc::new(AtomicU64::new(NO_IDEMPOTENT_RETRY_GENERATION)),
            connection_commit_gate: Arc::new(Mutex::new(())),
            connection_generation_notify: tokio::sync::Notify::new(),
            connection_attempt_notify: tokio::sync::Notify::new(),
            connection_attempt_deadline: Mutex::new(None),
            hook_callback: OnceLock::new(),
            supervisor_fact_sink: Arc::new(OnceLock::new()),
            #[cfg(any(test, feature = "test-utils"))]
            connection_publish_barrier: OnceLock::new(),
            membership: OnceLock::new(),
        }
    }

    /// Install a deterministic two-phase gate immediately before signaling
    /// publication. Integration tests use the first rendezvous to observe the
    /// commit boundary and the second to release it after invalidation.
    #[cfg(feature = "test-utils")]
    pub fn set_connection_publish_barrier_for_test(&self, barrier: Arc<tokio::sync::Barrier>) {
        self.connection_publish_barrier
            .set(barrier)
            .expect("signaling publication barrier should only be installed once");
    }

    fn lock_connection_commit(&self) -> MutexGuard<'_, ()> {
        self.connection_commit_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Emit a `SignalingGenerationCommitted` fact if a supervisor fact sink is
    /// wired. Best-effort and non-blocking.
    fn emit_signaling_committed(&self, generation: u64, origin: SignalingFactOrigin) {
        if let Some(sink) = self.supervisor_fact_sink.get() {
            sink.signaling_generation_committed(generation, origin);
        }
    }

    /// Emit a `SignalingGenerationLost` fact if a supervisor fact sink is wired.
    /// Best-effort and non-blocking.
    fn emit_signaling_lost(&self, generation: u64, cause: SignalingFactLostCause) {
        if let Some(sink) = self.supervisor_fact_sink.get() {
            sink.signaling_generation_lost(generation, cause);
        }
    }

    /// Arm the idempotent-retry mark for the currently live generation.
    ///
    /// The commit gate serializes the load/store with connection
    /// invalidation, so a concurrent disconnect cannot interleave a
    /// generation bump between them and leave a stale mark that silently
    /// denies the retry.
    fn mark_idempotent_retryable(&self) {
        let _commit_guard = self.lock_connection_commit();
        let generation = self.connection_generation.load(Ordering::Acquire);
        self.idempotent_retry_generation
            .store(generation, Ordering::Release);
    }

    /// Invalidate in-flight connection attempts and arm the retry mark for
    /// the post-invalidation generation as one linearized step.
    fn invalidate_marking_idempotent_retry(&self) {
        {
            let _commit_guard = self.lock_connection_commit();
            let bumped = self.connection_generation.fetch_add(1, Ordering::AcqRel) + 1;
            self.idempotent_retry_generation
                .store(bumped, Ordering::Release);
        }
        self.connection_generation_notify.notify_waiters();
    }

    fn clear_idempotent_retryable(&self) {
        self.idempotent_retry_generation
            .store(NO_IDEMPOTENT_RETRY_GENERATION, Ordering::Release);
    }

    fn idempotent_retry_allowed(&self) -> bool {
        let generation = self.connection_generation.load(Ordering::Acquire);
        self.idempotent_retry_generation.load(Ordering::Acquire) == generation
            && generation != NO_IDEMPOTENT_RETRY_GENERATION
    }

    /// Install the membership wiring. Idempotent; only the first call binds.
    ///
    /// After this the client reads its credential from the `watch` receiver and
    /// reports auth verdicts through the `mpsc` sender instead of retrying a dead
    /// credential.
    pub(crate) fn install_membership(&self, handle: MembershipHandle) {
        let _ = self.membership.set(handle);
    }

    /// Invoke the hook callback and await its completion.
    /// No-op if no callback has been set yet.
    async fn invoke_hook(&self, event: HookEvent) {
        if let Some(cb) = self.hook_callback.get() {
            cb(event).await;
        }
    }

    async fn publish_disconnected_transition(
        was_connected: bool,
        stats: &Arc<AtomicSignalingStats>,
        event_tx: &broadcast::Sender<SignalingEvent>,
        hook_callback: Option<HookCallback>,
        reason: DisconnectReason,
    ) -> bool {
        if !was_connected {
            return false;
        }

        stats.disconnections.fetch_add(1, Ordering::Relaxed);

        if let Some(cb) = hook_callback {
            cb(HookEvent::SignalingDisconnected).await;
        }

        let _ = event_tx.send(SignalingEvent::Disconnected { reason });

        true
    }

    /// Invalidate an unusable signaling socket after an envelope send failure.
    ///
    /// `generation` is the connection generation observed when the send began;
    /// by the time the failure is processed here, a recovery may already have
    /// invalidated that generation and committed a replacement, so every
    /// destructive step is fenced on the failed generation still being the
    /// published `Connected` state.
    ///
    /// The envelope outcome is indeterminate once the send has failed or timed
    /// out, so callers must not retry it in place. Stop the old socket tasks,
    /// publish one disconnected transition, and let the supervisor derive the
    /// one recovery obligation for that generation.
    async fn handle_envelope_send_failure(&self, generation: u64, reason: DisconnectReason) {
        self.stats.errors.fetch_add(1, Ordering::Relaxed);

        // Invalidate the failed socket only while its generation is still the
        // published `Connected` state. A recovery that already invalidated
        // this generation or committed a replacement owns the socket; swapping
        // the shared liveness flag or overwriting the state then would tear
        // down the replacement connection instead.
        let was_connected = {
            let _commit_guard = self.lock_connection_commit();
            let still_live = {
                let state = &*self
                    .socket_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                matches!(
                    state,
                    SignalingSocketState::Connected {
                        generation: live,
                        ..
                    } if *live == generation
                )
            };
            if !still_live {
                false
            } else {
                let was_connected = self.connected.swap(false, Ordering::AcqRel);
                if was_connected {
                    *self
                        .socket_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        SignalingSocketState::Disconnected {
                            last_generation: Some(generation),
                        };
                }
                was_connected
            }
        };
        if !was_connected {
            return;
        }

        // This is an automatic transport failure rather than an explicit
        // teardown. Idempotent requests (RouteCandidates, GetSigningKey) may
        // be replayed once after the replacement signaling generation commits.
        self.mark_idempotent_retryable();

        // The envelope send failed on a previously live socket; its generation
        // is lost. Report it before the local teardown proceeds. A recovery
        // that already superseded this generation rejects the stale fact and
        // stays on its own path.
        self.emit_signaling_lost(generation, SignalingFactLostCause::RemoteReset);

        // Generation-fenced teardown: only complete the old socket's cleanup
        // (task aborts, sink, pending waiters) while this path still owns the
        // `Disconnected` publication. A recovery that already invalidated the
        // generation or committed a replacement owns the teardown, and
        // publishing another Disconnected transition would spuriously fail
        // waiters parked on the replacement.
        let owns_teardown = self.teardown_failed_generation_slots(generation).await;

        self.stats.disconnections.fetch_add(1, Ordering::Relaxed);
        if !owns_teardown {
            tracing::debug!(
                generation,
                ?reason,
                "Signaling send-failure teardown skipped: a newer teardown already owns this socket"
            );
            return;
        }

        let _ = self.event_tx.send(SignalingEvent::Disconnected { reason });
        // send_envelope() may be called while a peer signaling commit gate is
        // held. Run the externally supplied hook after this method returns so
        // a re-entrant cleanup from the hook cannot wait on that same gate.
        if let Some(cb) = self.hook_callback.get().cloned() {
            tokio::spawn(async move {
                cb(HookEvent::SignalingDisconnected).await;
            });
        }
    }

    /// Complete the failed generation's slot teardown while its `Disconnected`
    /// publication is still the live socket state.
    ///
    /// A replacement connection publishes `Connected` under the commit gate
    /// before storing task handles or serving sends from its sink, and a
    /// recovery disconnect overwrites the state before finishing its own
    /// teardown, so while our `Disconnected` publication is live every handle,
    /// sink byte, and pending waiter still registered belongs to the failed
    /// generation. Once any other writer replaces the state, that path owns
    /// this socket's cleanup; touching the slots then could abort the
    /// replacement's live tasks or drop its waiters. Returns whether this
    /// call kept ownership and therefore owns the disconnect publication.
    async fn teardown_failed_generation_slots(&self, generation: u64) -> bool {
        // Slot locks are taken without awaiting: no `.await` may run while
        // the commit gate is held. Every legitimate holder performs only
        // synchronous work, so a few yield-and-revalidate retries absorb
        // transient contention. Persistent contention (for example a stalled
        // send holding the sink for its bounded timeout) leaves the old tasks
        // to self-terminate on the next failed ping or stream end, and the
        // recovery disconnect finishes the teardown.
        for _ in 0..4 {
            let uncontended = {
                let _commit_guard = self.lock_connection_commit();
                let state = &*self
                    .socket_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !matches!(
                    state,
                    SignalingSocketState::Disconnected {
                        last_generation: Some(published)
                    } if *published == generation
                ) {
                    return false;
                }

                let mut uncontended = true;

                if let Ok(mut task_guard) = self.ping_task.try_lock() {
                    if let Some(handle) = task_guard.take() {
                        handle.abort();
                    }
                } else {
                    uncontended = false;
                }
                if let Ok(mut task_guard) = self.receiver_task.try_lock() {
                    if let Some(handle) = task_guard.take() {
                        handle.abort();
                    }
                } else {
                    uncontended = false;
                }

                // The timed-out operation has normally released the sink lock
                // already. Avoid extending the public five-second send bound
                // if another operation still owns it; the next successful
                // connect will replace the old sink.
                if let Ok(mut sink_guard) = self.ws_sink.try_lock() {
                    sink_guard.take();
                } else {
                    uncontended = false;
                }

                if let Ok(mut pending) = self.pending_replies.try_lock() {
                    let dropped = pending.len();
                    pending.clear();
                    if dropped > 0 {
                        self.stats
                            .reply_waiters_dropped
                            .fetch_add(dropped as u64, Ordering::Relaxed);
                        tracing::debug!(
                            dropped,
                            "Dropping pending signaling reply waiters after send failure"
                        );
                    }
                } else {
                    uncontended = false;
                }
                if let Ok(mut pongs) = self.pending_pongs.try_lock() {
                    pongs.clear();
                } else {
                    uncontended = false;
                }

                uncontended
            };
            if uncontended {
                return true;
            }
            tokio::task::yield_now().await;
        }
        tracing::warn!(
            generation,
            "Signaling send-failure teardown deferred: slot locks stayed contended; \
             old tasks self-terminate and recovery finishes the cleanup"
        );
        true
    }

    /// Handle a reconnect error against the membership authority.
    ///
    /// If the error carries a typed [`crate::transport::AuthVerdict`] and the
    /// membership controller is wired, submit one on-demand request. A
    /// successful publication emits a revisioned material fact; all retry
    /// pacing remains in the supervisor.
    async fn handle_auth_verdict(&self, err: &NetworkError) -> AuthVerdictAction {
        let Some(verdict) = err.auth_verdict() else {
            return AuthVerdictAction::NotAVerdict;
        };
        let Some(handle) = self.membership.get() else {
            // No controller wired (flag off): preserve the original typed error
            // for supervisor classification.
            return AuthVerdictAction::NotAVerdict;
        };

        let mut rx = handle.credential_rx();
        let stale_revision = rx.borrow().revision;

        tracing::warn!(
            ?verdict,
            stale_revision,
            "🔒 Signaling auth verdict; requesting on-demand credential recovery"
        );

        match handle.resolve(verdict, stale_revision).await {
            MembershipResolution::Published | MembershipResolution::Superseded => {
                // Publish happens before the resolution reply. Keep the watch
                // fence for the superseded/racing case.
                if rx.borrow().revision <= stale_revision
                    && rx
                        .wait_for(|published| published.revision > stale_revision)
                        .await
                        .is_err()
                {
                    return AuthVerdictAction::ShutdownWhileParked;
                }
                AuthVerdictAction::Reacquired
            }
            MembershipResolution::Deferred => AuthVerdictAction::RetrySameCredential,
            MembershipResolution::Denied => AuthVerdictAction::Denied,
            MembershipResolution::Shutdown => AuthVerdictAction::ShutdownWhileParked,
        }
    }

    /// Test-only convenience constructor: create, connect, and return a client.
    ///
    /// The returned client has no `actor_id` / `credential_state` bound, so the
    /// signaling URL carries no identity query parameters — mock-actrix will
    /// not bind the WebSocket to any registry entry. Use this only for tests
    /// that explicitly exercise the unbound path; integration tests that need
    /// peer-to-peer relay should use [`Self::connect_to_with_identity`].
    #[cfg(feature = "test-utils")]
    pub async fn connect_to(url: &str) -> NetworkResult<Arc<Self>> {
        let config = SignalingConfig {
            server_url: url.parse()?,
            connection_timeout: 5,
            heartbeat_interval: 30,
            reconnect_config: ReconnectConfig::default(),
            auth_config: None,
            webrtc_role: None,
            inbound_queue: InboundQueueConfig::default(),
        };

        let client = Arc::new(Self::new(config));
        client.connect().await?;
        Ok(client)
    }

    /// Test-only constructor that pins identity *before* the WebSocket
    /// handshake so mock-actrix can bind the connection to the actor on
    /// register (`?actor_id=…` query parameter). Required by integration
    /// tests that rely on peer-to-peer signaling relay — without this binding
    /// mock-actrix drops outbound relays for "unbound target".
    #[cfg(feature = "test-utils")]
    pub async fn connect_to_with_identity(
        url: &str,
        actor_id: ActrId,
        credential_state: CredentialState,
    ) -> NetworkResult<Arc<Self>> {
        let config = SignalingConfig {
            server_url: url.parse()?,
            connection_timeout: 5,
            heartbeat_interval: 30,
            reconnect_config: ReconnectConfig::default(),
            auth_config: None,
            webrtc_role: None,
            inbound_queue: InboundQueueConfig::default(),
        };

        let client = Arc::new(Self::new(config));
        client.set_actor_id(actor_id).await;
        client.set_credential_state(credential_state).await;
        client.connect().await?;
        Ok(client)
    }

    /// alive integrate down a envelope ID
    async fn next_envelope_id(&self) -> String {
        let mut counter = self.envelope_counter.lock().await;
        *counter += 1;
        format!("env-{}", *counter)
    }

    /// Create SignalingEnvelope
    async fn create_envelope(&self, flow: signaling_envelope::Flow) -> SignalingEnvelope {
        SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.next_envelope_id().await,
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(flow),
        }
    }

    /// Reset inbound channel for a fresh session (useful after disconnects).
    ///
    /// Builds a brand-new bounded queue and byte budget and swaps both halves
    /// in. Dropping the old receiver releases every still-queued envelope, whose
    /// permits return their reserved bytes to the old (now dropped) budget, so a
    /// reset never leaves orphaned queued messages or leaked byte reservations.
    async fn reset_inbound_channel(&self) {
        self.drop_pending_replies("inbound channel reset").await;
        self.drop_pending_pongs("inbound channel reset").await;

        let (egress, rx) = InboundEgress::new(self.config.inbound_queue, self.stats.clone());
        *self.inbound_egress.lock().await = egress;
        *self.inbound_rx.lock().await = rx;
    }

    /// Current number of unmatched/push envelopes queued for the consumer
    /// (observability gauge, issue #351).
    ///
    /// Available wherever the concrete client type is nameable (tests and the
    /// `test-utils` feature); production observability flows through the
    /// counters in [`SignalingStats`].
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn inbound_queue_depth(&self) -> usize {
        self.inbound_egress.lock().await.depth()
    }

    /// Current number of encoded bytes held in the inbound queue
    /// (observability gauge, issue #351). See [`Self::inbound_queue_depth`].
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn inbound_queue_bytes(&self) -> usize {
        self.inbound_egress.lock().await.queued_bytes()
    }

    async fn drop_pending_replies(&self, reason: &'static str) {
        let dropped = {
            let mut pending = self.pending_replies.lock().await;
            let dropped = pending.len();
            pending.clear();
            dropped
        };

        if dropped > 0 {
            self.stats
                .reply_waiters_dropped
                .fetch_add(dropped as u64, Ordering::Relaxed);
            tracing::debug!(reason, dropped, "Dropping pending signaling reply waiters");
        }
    }

    async fn drop_pending_pongs(&self, reason: &'static str) {
        let dropped = {
            let mut pending = self.pending_pongs.lock().await;
            let dropped = pending.len();
            pending.clear();
            dropped
        };

        if dropped > 0 {
            tracing::debug!(reason, dropped, "Dropping pending signaling pong waiters");
        }
    }

    /// Build signaling URL with actor identity and Ed25519 credential params for authentication.
    ///
    /// Passes `actor_id`, `key_id`, `claims` (base64), `signature` (base64) as URL query params
    /// so the signaling server can validate the credential before upgrading the WebSocket.
    async fn build_url_with_identity(&self) -> Url {
        let mut url = self.config.server_url.clone();

        // When the membership controller is wired, the credential owner is the
        // single source of truth: read the latest published credential + actor
        // id straight from the `watch`. This is what makes the reconnect
        // self-heal — after the owner mints a fresh credential and wakes us, the
        // very next handshake URL carries it, never the stale one.
        if let Some(handle) = self.membership.get() {
            let credential_rx = handle.credential_rx();
            let published = credential_rx.borrow().clone();
            let actor_str = actr_protocol::ActrId::to_string_repr(&published.actor_id);
            url.query_pairs_mut().append_pair("actor_id", &actor_str);

            let cred = &published.credential;
            let claims_b64 = base64::engine::general_purpose::STANDARD.encode(&cred.claims);
            let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&cred.signature);
            url.query_pairs_mut()
                .append_pair("key_id", &cred.key_id.to_string())
                .append_pair("claims", &claims_b64)
                .append_pair("signature", &sig_b64);

            if let Some(role) = &self.config.webrtc_role {
                url.query_pairs_mut().append_pair("webrtc_role", role);
            }
            return url;
        }

        // Legacy path (membership controller disabled): stored actor id +
        // credential_state.
        let actor_id_opt = self.actor_id.lock().await.clone();
        if let Some(actor_id) = actor_id_opt {
            let actor_str = actr_protocol::ActrId::to_string_repr(&actor_id);
            url.query_pairs_mut().append_pair("actor_id", &actor_str);
        }

        // Pass Ed25519 credential in URL for initial WS auth
        let cred_state_opt = self.credential_state.lock().await.clone();
        if let Some(cred_state) = cred_state_opt {
            let cred = cred_state.credential().await;
            let claims_b64 = base64::engine::general_purpose::STANDARD.encode(&cred.claims);
            let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&cred.signature);
            url.query_pairs_mut()
                .append_pair("key_id", &cred.key_id.to_string())
                .append_pair("claims", &claims_b64)
                .append_pair("signature", &sig_b64);
        }

        // Add WebRTC role preference if configured
        if let Some(role) = &self.config.webrtc_role {
            url.query_pairs_mut().append_pair("webrtc_role", role);
        }

        url
    }

    /// Test-only accessor for the handshake URL the next connect/reconnect
    /// will use. Mirrors `build_url_with_identity` so tests can assert which
    /// credential claims are carried without inspecting the wire.
    ///
    /// This exists to guard the fix for #423: after a soft renewal updates the
    /// shared `CredentialState`, the very next handshake URL must carry the
    /// renewed credential, never the stale initial one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_handshake_url(&self) -> Url {
        self.build_url_with_identity().await
    }

    fn redact_signaling_url_for_log(url: &Url) -> String {
        let mut redacted = url.clone();
        let pairs: Vec<(String, String)> = redacted
            .query_pairs()
            .map(|(key, value)| {
                let redacted_value = match key.to_ascii_lowercase().as_str() {
                    "claims" | "signature" | "token" | "authorization" | "bearer"
                    | "access_token" | "api_key" => "REDACTED".to_string(),
                    _ => value.into_owned(),
                };
                (key.into_owned(), redacted_value)
            })
            .collect();

        redacted.set_query(None);
        if !pairs.is_empty() {
            let mut query = redacted.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }

        redacted.to_string()
    }

    fn connection_generation_changed(&self, generation: u64) -> bool {
        self.connection_generation.load(Ordering::Acquire) != generation
    }

    fn connect_intent_cancelled(&self, intent: ConnectIntent) -> bool {
        match intent {
            ConnectIntent::Explicit { generation } | ConnectIntent::Recovery { generation, .. } => {
                self.connection_generation_changed(generation)
            }
        }
    }

    fn invalidate_connection_attempts(&self) {
        {
            // The same gate covers the final generation check and socket
            // publication. Whichever side acquires it first defines whether the
            // candidate connection committed before or after invalidation.
            let _commit_guard = self.lock_connection_commit();
            self.connection_generation.fetch_add(1, Ordering::AcqRel);
        }
        self.connection_generation_notify.notify_waiters();
    }

    /// Establish a single signaling WebSocket connection attempt, honoring connection_timeout.
    ///
    /// This does not perform any retry logic; callers that want retries should wrap this.
    fn connection_attempt_deadline(&self) -> Option<tokio::time::Instant> {
        (self.config.connection_timeout > 0).then(|| {
            tokio::time::Instant::now()
                + std::time::Duration::from_secs(self.config.connection_timeout)
        })
    }

    async fn establish_connection_once_with_intent(
        &self,
        intent: ConnectIntent,
        deadline: Option<tokio::time::Instant>,
    ) -> NetworkResult<()> {
        // Guard: Check if already connected (handles rare TOCTOU scenarios)
        if self.connected.load(Ordering::Acquire) {
            tracing::debug!("Connection already established, skipping establish_connection_once()");
            return Ok(());
        }

        let url = self.build_url_with_identity().await;
        let timeout_secs = self.config.connection_timeout;
        tracing::debug!(
            "Establishing connection to URL: {}",
            Self::redact_signaling_url_for_log(&url)
        );
        // After disconnection, data written to the buffer will continue to be sent once the network recovers
        // Cap frame/message sizes at the transport boundary so a peer cannot
        // force tungstenite to allocate a many-megabyte buffer before the
        // post-read size check rejects it (issue #351). Tungstenite defaults
        // are 16 MiB frame / 64 MiB message; both are bounded to
        // `max_frame_bytes` here. The post-read check in `start_receiver`
        // remains as defense in depth.
        let max_frame_bytes = self.config.inbound_queue.max_frame_bytes;
        let config = WebSocketConfig::default()
            .write_buffer_size(0)
            .max_frame_size(Some(max_frame_bytes))
            .max_message_size(Some(max_frame_bytes));
        // One attempt is bounded by its flight deadline and interrupted by
        // lifecycle generation invalidation. The retained Notify futures are
        // enabled before each state check, closing the check/subscribe race.
        let connect = async {
            let result = if let Some(deadline) = deadline {
                crate::timer::timeout_at(
                    crate::timer::ids::SIGNALING_CONNECT,
                    deadline,
                    connect_async_with_config(url.as_str(), Some(config), false),
                )
                .await
                .map_err(|_| {
                    NetworkError::ConnectionError(format!(
                        "Signaling connect timeout after {}s",
                        timeout_secs
                    ))
                })?
            } else {
                connect_async_with_config(url.as_str(), Some(config), false).await
            };
            result.map_err(NetworkError::from)
        };
        tokio::pin!(connect);

        let connect_result = loop {
            let generation_changed = self.connection_generation_notify.notified();
            tokio::pin!(generation_changed);
            generation_changed.as_mut().enable();

            if self.connect_intent_cancelled(intent) {
                return Err(NetworkError::ConnectionError(
                    "Signaling connect was cancelled by lifecycle invalidation".to_string(),
                ));
            }

            tokio::select! {
                biased;
                _ = &mut generation_changed => {
                    if self.connect_intent_cancelled(intent) {
                        return Err(NetworkError::ConnectionError(
                            "Signaling connect was cancelled by lifecycle invalidation".to_string(),
                        ));
                    }
                }
                result = &mut connect => break result?,
            }
        };

        let (ws_stream, _) = connect_result;

        // Split read/write halves and initialize client state
        let (sink, stream) = ws_stream.split();

        if self.connect_intent_cancelled(intent) {
            return self.discard_cancelled_connection(intent, sink).await;
        }

        #[cfg(any(test, feature = "test-utils"))]
        if let Some(barrier) = self.connection_publish_barrier.get() {
            barrier.wait().await;
            barrier.wait().await;
        }

        // Acquire every asynchronously-locked publication slot before entering
        // the synchronous commit gate. Invalidation never waits for these
        // locks, so there is no inverse lock ordering.
        let mut sink_guard = self.ws_sink.lock().await;
        let mut stream_guard = self.ws_stream.lock().await;
        let generation = intent.generation();
        let mut candidate_sink = Some(sink);
        let mut candidate_stream = Some(stream);
        let committed = {
            let _commit_guard = self.lock_connection_commit();
            if self.connect_intent_cancelled(intent) {
                false
            } else {
                *sink_guard = candidate_sink.take();
                *stream_guard = candidate_stream.take();
                self.connected.store(true, Ordering::Release);
                *self
                    .socket_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    SignalingSocketState::Connected {
                        generation,
                        action_id: intent.action_id(),
                    };
                self.liveness_generation.fetch_add(1, Ordering::Release);
                self.stats.connections.fetch_add(1, Ordering::Relaxed);
                self.emit_signaling_committed(generation, intent.fact_origin());
                true
            }
        };
        drop(stream_guard);
        drop(sink_guard);

        if !committed {
            return self
                .discard_cancelled_connection(
                    intent,
                    candidate_sink.expect("uncommitted signaling sink must remain local"),
                )
                .await;
        }

        // Enqueue the hook before broadcasting to other subscribers.
        self.invoke_hook(HookEvent::SignalingConnected).await;
        let _ = self.event_tx.send(SignalingEvent::Connected);

        Ok(())
    }

    async fn discard_cancelled_connection(
        &self,
        intent: ConnectIntent,
        mut sink: futures_util::stream::SplitSink<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            tokio_tungstenite::tungstenite::Message,
        >,
    ) -> NetworkResult<()> {
        tracing::debug!(
            intent = ?intent,
            "Discarding completed signaling connection after lifecycle generation changed"
        );
        if let Err(e) = sink.close().await {
            tracing::warn!(
                "Signaling socket close failed after lifecycle cancellation: {}",
                e
            );
        }
        Err(NetworkError::ConnectionError(
            "Signaling connection was cancelled by explicit disconnect".to_string(),
        ))
    }

    /// Send envelope and wait for response with timeout and error handling.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            name = "signaling.request",
            level = "debug",
            skip_all,
            fields(envelope_id = %envelope.envelope_id)
        )
    )]
    async fn send_envelope_and_wait_response(
        &self,
        envelope: SignalingEnvelope,
    ) -> NetworkResult<SignalingEnvelope> {
        let reply_for = envelope.envelope_id.clone();

        // Subscribe before registering the waiter: every loss path arms the
        // idempotent-retry mark before publishing `Disconnected`, so a waiter
        // woken by that event observes a mark consistent with the cleanup
        // that dropped (or is about to drop) its entry.
        let mut events = self.event_tx.subscribe();

        // Register waiter before sending
        let (tx, mut rx) = oneshot::channel();
        self.pending_replies
            .lock()
            .await
            .insert(reply_for.clone(), tx);

        if let Err(e) = self.send_envelope(envelope).await {
            // Cleanup waiter on immediate send failure to avoid leaks. A
            // missing entry means an automatic cleanup already dropped it
            // after arming the retry mark: surface the typed error so
            // idempotent callers can retry instead of the raw socket error.
            if self
                .pending_replies
                .lock()
                .await
                .remove(&reply_for)
                .is_none()
            {
                return Err(NetworkError::SignalingReplyDropped);
            }
            return Err(e);
        }

        // The server only ever replies on the WebSocket the request arrived
        // on, so a Disconnected event observed while waiting proves the
        // response is unreachable: fail immediately with the retryable typed
        // error instead of parking until the response timeout.
        let wait = async {
            loop {
                tokio::select! {
                    biased;
                    response = &mut rx => {
                        return response.map_err(|_| NetworkError::SignalingReplyDropped);
                    }
                    event = events.recv() => {
                        match event {
                            Ok(SignalingEvent::Disconnected { .. }) => {
                                self.pending_replies.lock().await.remove(&reply_for);
                                return Err(NetworkError::SignalingReplyDropped);
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => {
                                return Err(NetworkError::ConnectionClosed(
                                    "Signaling event channel closed while waiting for response"
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        };

        let result = crate::timer::timeout(
            crate::timer::ids::SIGNALING_REQUEST_RESPONSE,
            std::time::Duration::from_secs(RESPONSE_TIMEOUT_SECS),
            wait,
        )
        .await;
        // Clean up waiter on timeout
        if result.is_err() {
            self.pending_replies.lock().await.remove(&reply_for);
        }

        let response_envelope = result.map_err(|_| {
            NetworkError::ConnectionError("Timed out waiting for signaling response".to_string())
        })??;

        Ok(response_envelope)
    }

    /// Spawn background receiver to demux envelopes by reply_for.
    async fn start_receiver(&self) {
        let mut stream_guard = self.ws_stream.lock().await;
        if stream_guard.is_none() {
            return;
        }

        let mut stream = stream_guard.take().expect("stream exists");
        let pending = self.pending_replies.clone();
        let inbound_egress = { self.inbound_egress.lock().await.clone() };
        let max_frame_bytes = self.config.inbound_queue.max_frame_bytes;
        let stats = self.stats.clone();
        let connected = self.connected.clone();
        let socket_state = self.socket_state.clone();
        let connection_commit_gate = self.connection_commit_gate.clone();
        let event_tx = self.event_tx.clone();
        let liveness_generation = self.liveness_generation.clone();
        let pending_pongs = self.pending_pongs.clone();
        let hook_callback = self.hook_callback.get().cloned();
        // Keep the shared fact-sink cell so a supervisor installed after the
        // socket handshake still receives a spontaneous stream-end fact.
        let fact_sink = Arc::clone(&self.supervisor_fact_sink);
        let connection_generation = Arc::clone(&self.connection_generation);
        let idempotent_retry_generation = Arc::clone(&self.idempotent_retry_generation);
        let socket_generation = self.connection_generation.load(Ordering::Acquire);
        let handle = tokio::spawn(async move {
            // Why the receive loop terminated; published to subscribers so a
            // backpressure fail-closed is distinguishable from a clean end.
            let mut disconnect_reason = DisconnectReason::StreamEnded;
            'recv_loop: while let Some(msg) = stream.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                        // Any inbound traffic counts as liveness
                        liveness_generation.fetch_add(1, Ordering::Release);

                        // Reject oversized frames before protobuf
                        // decode/allocation so a malicious or buggy peer cannot
                        // force a large allocation (issue #351).
                        if data.len() > max_frame_bytes {
                            stats.inbound_oversized.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!(
                                "Rejecting oversized signaling frame: {} bytes (max {})",
                                data.len(),
                                max_frame_bytes
                            );
                            continue;
                        }

                        let frame_len = data.len();
                        match SignalingEnvelope::decode(&data[..]) {
                            Ok(envelope) => {
                                let span = {
                                    let span = tracing::info_span!("signaling.receive_envelope", envelope_id = %envelope.envelope_id);
                                    #[cfg(feature = "opentelemetry")]
                                    let _ = span.set_parent(extract_trace_context(&envelope));
                                    span
                                };

                                stats.messages_received.fetch_add(1, Ordering::Relaxed);
                                // Log only non-payload identifiers; never format the envelope
                                // (which carries message content) — see issue #484.
                                tracing::debug!(
                                    envelope_id = %envelope.envelope_id,
                                    reply_for = ?envelope.reply_for,
                                    "Received signaling envelope"
                                );
                                if let Some(reply_for) = envelope.reply_for.clone() {
                                    if let Some(sender) = pending.lock().await.remove(&reply_for) {
                                        if let Err(e) = span.in_scope(|| sender.send(envelope)) {
                                            stats.errors.fetch_add(1, Ordering::Relaxed);
                                            tracing::warn!(
                                                "Failed to send reply envelope to waiter: {e:?}",
                                            );
                                        }
                                        continue;
                                    }
                                }
                                tracing::debug!(
                                    "Unmatched or push message -> forward to inbound channel"
                                );
                                // Unmatched or push message -> forward to the bounded
                                // inbound queue. When the queue is full this applies
                                // TCP/WebSocket backpressure by awaiting capacity, then
                                // fails closed (reconnect) rather than dropping the
                                // signaling message (issue #351).
                                match inbound_egress
                                    .send_with_span(envelope, frame_len, span)
                                    .await
                                {
                                    Ok(()) => {}
                                    Err(InboundEnqueueError::Saturated) => {
                                        stats
                                            .inbound_backpressure_closes
                                            .fetch_add(1, Ordering::Relaxed);
                                        tracing::warn!(
                                            "Inbound signaling queue saturated past backpressure deadline; failing closed to reconnect"
                                        );
                                        disconnect_reason = DisconnectReason::BackpressureSaturated;
                                        break 'recv_loop;
                                    }
                                    Err(InboundEnqueueError::Closed) => {
                                        tracing::debug!(
                                            "Inbound channel closed; stopping receiver task"
                                        );
                                        break 'recv_loop;
                                    }
                                }
                            }
                            Err(e) => {
                                stats.errors.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!("Failed to decode SignalingEnvelope: {e}");
                            }
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Pong(payload)) => {
                        tracing::debug!("Received pong");
                        liveness_generation.fetch_add(1, Ordering::Release);
                        if let Some(sender) = pending_pongs.lock().await.remove(&payload.to_vec()) {
                            let _ = sender.send(());
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Ping(_)) => {
                        tracing::debug!("Received ping");
                        liveness_generation.fetch_add(1, Ordering::Release);
                    }
                    Ok(other) => {
                        // Log only the frame variant, not its content — a `Text`
                        // frame's payload is user data and must not be exported
                        // via OTLP even at `info`/default filter (issue #484).
                        let kind = match &other {
                            tokio_tungstenite::tungstenite::Message::Text(_) => "Text",
                            tokio_tungstenite::tungstenite::Message::Close(_) => "Close",
                            tokio_tungstenite::tungstenite::Message::Frame(_) => "Frame",
                            tokio_tungstenite::tungstenite::Message::Ping(_) => "Ping",
                            tokio_tungstenite::tungstenite::Message::Pong(_) => "Pong",
                            tokio_tungstenite::tungstenite::Message::Binary(_) => "Binary",
                        };
                        tracing::warn!("Received non-binary frame ({kind}), ignoring");
                    }
                    Err(e) => {
                        stats.errors.fetch_add(1, Ordering::Relaxed);
                        tracing::error!("Signaling receive error: {e}");
                        break 'recv_loop;
                    }
                }
            }

            tracing::warn!("Stream terminated");
            // If explicit disconnect already marked the client disconnected,
            // do not publish a second loss fact for the intentional close. The
            // disconnect path publishes its own Manual event and supervisor fact.
            let was_connected = {
                let _commit_guard = connection_commit_gate
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let was_connected = connected.swap(false, Ordering::AcqRel);
                if was_connected {
                    *socket_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        SignalingSocketState::Disconnected {
                            last_generation: Some(socket_generation),
                        };
                    // A spontaneous stream end is an automatic loss: arm the
                    // idempotent-retry mark for the live generation before
                    // publishing the transition, so waiters woken by the
                    // event observe a mark consistent with what dropped them.
                    idempotent_retry_generation.store(
                        connection_generation.load(Ordering::Acquire),
                        Ordering::Release,
                    );
                }
                was_connected
            };
            Self::publish_disconnected_transition(
                was_connected,
                &stats,
                &event_tx,
                hook_callback,
                disconnect_reason,
            )
            .await;
            // A spontaneous stream end lost the live generation with no lifecycle
            // command; report it so the supervisor can derive recovery.
            if was_connected && let Some(sink) = fact_sink.get() {
                sink.signaling_generation_lost(
                    socket_generation,
                    SignalingFactLostCause::RemoteReset,
                );
            }
            pending_pongs.lock().await.clear();
        });

        *self.receiver_task.lock().await = Some(handle);
    }

    /// Spawn background ping task to detect half-open connections where writes do not fail but peer is gone.
    /// fixme: merge to heartbeat task
    async fn start_ping_task(&self) {
        let mut existing = self.ping_task.lock().await;
        if let Some(handle) = existing.as_ref() {
            if handle.is_finished() {
                existing.take();
            } else {
                return;
            }
        }

        let sink = self.ws_sink.clone();
        let connected = self.connected.clone();
        let socket_state = self.socket_state.clone();
        let connection_commit_gate = self.connection_commit_gate.clone();
        let stats = self.stats.clone();
        let event_tx = self.event_tx.clone();
        let liveness_generation = self.liveness_generation.clone();
        let receiver_task_clone = Arc::clone(&self.receiver_task);
        let hook_callback = self.hook_callback.get().cloned();
        // Resolve the sink when a liveness failure is observed rather than
        // snapshotting it at task creation. Test and embedding harnesses may
        // finish lifecycle wiring after the initial signaling handshake.
        let fact_sink = Arc::clone(&self.supervisor_fact_sink);
        let connection_generation = Arc::clone(&self.connection_generation);
        let idempotent_retry_generation = Arc::clone(&self.idempotent_retry_generation);
        let socket_generation = self.connection_generation.load(Ordering::Acquire);

        let handle = tokio::spawn(async move {
            let mut watchdog = PongWatchdog::new(liveness_generation.load(Ordering::Acquire));
            loop {
                crate::timer::sleep(
                    crate::timer::ids::SIGNALING_PING_SCHEDULE,
                    std::time::Duration::from_secs(PING_INTERVAL_SECS),
                )
                .await;

                if !connected.load(Ordering::Acquire) {
                    break;
                }

                // Send ping; mark disconnect on failure.
                let mut disconnect_reason = None;
                {
                    let mut sink_guard = sink.lock().await;
                    if let Some(sink) = sink_guard.as_mut() {
                        match crate::timer::timeout(
                            crate::timer::ids::SIGNALING_SEND,
                            std::time::Duration::from_secs(SIGNALING_SEND_TIMEOUT_SECS),
                            sink.send(tokio_tungstenite::tungstenite::Message::Ping(
                                Vec::new().into(),
                            )),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                tracing::warn!("Signaling ping send failed: {e}");
                                disconnect_reason = Some(DisconnectReason::PingSendFailed);
                            }
                            Err(_) => {
                                tracing::warn!("Signaling ping send timed out");
                                disconnect_reason = Some(DisconnectReason::PingSendFailed);
                            }
                        }
                    } else {
                        tracing::warn!("Signaling not connected");
                        disconnect_reason = Some(DisconnectReason::PingSendFailed);
                    }
                }

                if let Some(reason) = disconnect_reason {
                    let was_connected = {
                        let _commit_guard = connection_commit_gate
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let was_connected = connected.swap(false, Ordering::AcqRel);
                        if was_connected {
                            *socket_state
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                SignalingSocketState::Disconnected {
                                    last_generation: Some(socket_generation),
                                };
                            // A ping-detected loss is an automatic failure:
                            // arm the idempotent-retry mark for the live
                            // generation before publishing the transition.
                            idempotent_retry_generation.store(
                                connection_generation.load(Ordering::Acquire),
                                Ordering::Release,
                            );
                        }
                        was_connected
                    };
                    Self::publish_disconnected_transition(
                        was_connected,
                        &stats,
                        &event_tx,
                        hook_callback.clone(),
                        reason,
                    )
                    .await;
                    if was_connected && let Some(sink) = fact_sink.get() {
                        sink.signaling_generation_lost(
                            socket_generation,
                            SignalingFactLostCause::RemoteReset,
                        );
                    }
                    break;
                }

                // Count one active scheduling interval without liveness. A
                // delayed wake after device suspend still contributes only
                // this one tick, so sleep time cannot expire the watchdog.
                let silent_active_secs = watchdog.tick(
                    liveness_generation.load(Ordering::Acquire),
                    PING_INTERVAL_SECS,
                );
                if silent_active_secs > PONG_TIMEOUT_SECS {
                    tracing::warn!(
                        "Signaling pong timeout (last seen {}s ago), marking disconnected",
                        silent_active_secs
                    );
                    if let Some(handle) = receiver_task_clone.lock().await.take() {
                        handle.abort();
                    }
                    let was_connected = {
                        let _commit_guard = connection_commit_gate
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let was_connected = connected.swap(false, Ordering::AcqRel);
                        if was_connected {
                            *socket_state
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                SignalingSocketState::Disconnected {
                                    last_generation: Some(socket_generation),
                                };
                            // A pong timeout is an automatic loss: arm the
                            // idempotent-retry mark for the live generation
                            // before publishing the transition.
                            idempotent_retry_generation.store(
                                connection_generation.load(Ordering::Acquire),
                                Ordering::Release,
                            );
                        }
                        was_connected
                    };
                    Self::publish_disconnected_transition(
                        was_connected,
                        &stats,
                        &event_tx,
                        hook_callback.clone(),
                        DisconnectReason::PongTimeout,
                    )
                    .await;
                    if was_connected && let Some(sink) = fact_sink.get() {
                        sink.signaling_generation_lost(
                            socket_generation,
                            SignalingFactLostCause::RemoteReset,
                        );
                    }
                    break;
                }
            }
        });

        *existing = Some(handle);
    }

    async fn disconnect_internal(&self, invalidate_attempts: bool) -> NetworkResult<()> {
        self.disconnect_internal_with_retry_mark(invalidate_attempts, false)
            .await
    }

    async fn disconnect_internal_with_retry_mark(
        &self,
        invalidate_attempts: bool,
        mark_idempotent_retry: bool,
    ) -> NetworkResult<()> {
        let teardown_deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(DISCONNECT_LOCK_TIMEOUT_SECS);
        // Capture the live generation before invalidation bumps it, so the
        // `SignalingGenerationLost` fact reports the generation that was live.
        let generation_before = self.connection_generation.load(Ordering::Acquire);
        if invalidate_attempts {
            if mark_idempotent_retry {
                self.invalidate_marking_idempotent_retry();
            } else {
                self.invalidate_connection_attempts();
                self.clear_idempotent_retryable();
            }
        } else if mark_idempotent_retry {
            self.mark_idempotent_retryable();
        } else {
            self.clear_idempotent_retryable();
        }

        {
            let _commit_guard = self.lock_connection_commit();
            if self.connected.load(Ordering::Acquire) {
                *self
                    .socket_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    SignalingSocketState::Disconnecting {
                        generation: generation_before,
                    };
            }
        }

        self.drop_pending_replies("signaling disconnect").await;
        self.drop_pending_pongs("signaling disconnect").await;
        let was_connected = {
            let _commit_guard = self.lock_connection_commit();
            let was_connected = self.connected.swap(false, Ordering::AcqRel);
            *self
                .socket_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                SignalingSocketState::Disconnected {
                    last_generation: was_connected.then_some(generation_before),
                };
            was_connected
        };

        // Stop background tasks before taking the WebSocket sink/stream locks.
        // A ping or receiver task can be inside a socket operation while holding
        // one of those locks; aborting first keeps disconnect from waiting on
        // the task it is about to shut down.
        let ping_handle = match crate::timer::timeout_at(
            crate::timer::ids::SIGNALING_DISCONNECT_LOCK,
            teardown_deadline,
            self.ping_task.lock(),
        )
        .await
        {
            Ok(mut task_guard) => task_guard.take(),
            Err(_) => {
                tracing::warn!("Timed out waiting for signaling ping task lock during disconnect");
                None
            }
        };
        if let Some(handle) = ping_handle {
            handle.abort();
        }

        let receiver_handle = match crate::timer::timeout_at(
            crate::timer::ids::SIGNALING_DISCONNECT_LOCK,
            teardown_deadline,
            self.receiver_task.lock(),
        )
        .await
        {
            Ok(mut task_guard) => task_guard.take(),
            Err(_) => {
                tracing::warn!(
                    "Timed out waiting for signaling receiver task lock during disconnect"
                );
                None
            }
        };
        if let Some(handle) = receiver_handle {
            handle.abort();
        }

        // Fetch and close the sink without holding the mutex during the close
        // await. The lock itself is bounded because a stalled send can hold it
        // on broken mobile network transitions.
        let sink = match crate::timer::timeout_at(
            crate::timer::ids::SIGNALING_DISCONNECT_LOCK,
            teardown_deadline,
            self.ws_sink.lock(),
        )
        .await
        {
            Ok(mut sink_guard) => sink_guard.take(),
            Err(_) => {
                tracing::warn!(
                    "Timed out waiting for signaling WebSocket sink lock during disconnect"
                );
                None
            }
        };

        if let Some(mut sink) = sink {
            match crate::timer::timeout_at(
                crate::timer::ids::SIGNALING_DISCONNECT_CLOSE,
                teardown_deadline,
                sink.close(),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::warn!("Signaling WebSocket close failed during disconnect: {}", e);
                }
                Err(_) => {
                    tracing::warn!(
                        "Signaling WebSocket close timed out during disconnect; continuing cleanup"
                    );
                }
            }
        }

        match crate::timer::timeout_at(
            crate::timer::ids::SIGNALING_DISCONNECT_LOCK,
            teardown_deadline,
            self.ws_stream.lock(),
        )
        .await
        {
            Ok(mut stream_guard) => {
                stream_guard.take();
            }
            Err(_) => {
                tracing::warn!(
                    "Timed out waiting for signaling WebSocket stream lock during disconnect"
                );
            }
        }

        self.reset_inbound_channel().await;

        // Enqueue the hook before broadcasting to other subscribers.
        Self::publish_disconnected_transition(
            was_connected,
            &self.stats,
            &self.event_tx,
            self.hook_callback.get().cloned(),
            DisconnectReason::Manual,
        )
        .await;

        // A live socket went down at an explicit disconnect/invalidation point;
        // report the lost generation to the supervisor.
        if was_connected {
            self.emit_signaling_lost(generation_before, SignalingFactLostCause::Disconnected);
        }

        Ok(())
    }

    async fn connect_once_for_explicit(&self, generation: u64) -> NetworkResult<()> {
        self.connect_once_with_intent(ConnectIntent::Explicit { generation })
            .await
    }

    async fn connect_once_with_intent(&self, intent: ConnectIntent) -> NetworkResult<()> {
        let generation = intent.generation();
        loop {
            if self.connection_generation_changed(generation) {
                return Err(NetworkError::ConnectionError(
                    "Signaling connect was cancelled by explicit disconnect".to_string(),
                ));
            }

            if self.connected.load(Ordering::Acquire) {
                tracing::debug!("Already connected, skipping signaling connect attempt");
                return Ok(());
            }

            match self
                .connecting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    let _commit_guard = self.lock_connection_commit();
                    *self
                        .socket_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        SignalingSocketState::Connecting {
                            expected_generation: generation,
                            action_id: intent.action_id(),
                        };
                    break;
                }
                Err(_) => {
                    if self.connected.load(Ordering::Acquire) {
                        tracing::debug!("Already connected, skipping signaling connect attempt");
                        return Ok(());
                    }

                    tracing::debug!(
                        "Another connection attempt in progress, waiting for state change..."
                    );
                    match self.wait_for_connection_result(generation).await {
                        Ok(()) => return Ok(()),
                        Err(e)
                            if !self.connection_generation_changed(generation)
                                && !self.connected.load(Ordering::Acquire)
                                && !self.connecting.load(Ordering::Acquire) =>
                        {
                            tracing::debug!(
                                "Concurrent signaling connection failed; explicit connect will retry immediately: {e}"
                            );
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        let attempt_deadline = self.connection_attempt_deadline();
        *self
            .connection_attempt_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = attempt_deadline;
        let attempt_guard = ConnectionAttemptGuard {
            connecting: &self.connecting,
            completion: &self.connection_attempt_notify,
            deadline: &self.connection_attempt_deadline,
            commit_gate: &self.connection_commit_gate,
            socket_state: &self.socket_state,
            intent,
        };

        if self.connection_generation_changed(generation) {
            return Err(NetworkError::ConnectionError(
                "Signaling connect was cancelled by explicit disconnect".to_string(),
            ));
        }

        if self.connected.load(Ordering::Acquire) {
            tracing::debug!("Connection completed by another task while acquiring lock");
            return Ok(());
        }

        self.invoke_hook(HookEvent::SignalingConnectStart { attempt: 1 })
            .await;

        tracing::debug!(
            generation,
            "Acquired connection lock, establishing one explicit signaling attempt..."
        );

        let result = self
            .establish_connection_once_with_intent(intent, attempt_deadline)
            .await;

        match result {
            Ok(()) => {
                if self.connection_generation_changed(generation) {
                    let cleanup_result = self.disconnect_internal(false).await;
                    cleanup_result?;
                    return Err(NetworkError::ConnectionError(
                        "Signaling connect was cancelled by explicit disconnect".to_string(),
                    ));
                }

                self.start_receiver().await;
                self.start_ping_task().await;

                if self.connection_generation_changed(generation) {
                    let cleanup_result = self.disconnect_internal(false).await;
                    cleanup_result?;
                    return Err(NetworkError::ConnectionError(
                        "Signaling connect was cancelled by explicit disconnect".to_string(),
                    ));
                }

                drop(attempt_guard);
                Ok(())
            }
            Err(e) => {
                if !self.connection_generation_changed(generation) {
                    let _ = self.event_tx.send(SignalingEvent::Disconnected {
                        reason: DisconnectReason::ConnectionFailed(e.to_string()),
                    });
                    tracing::error!("Connection attempt failed: {e}");
                } else {
                    tracing::debug!(
                        generation,
                        "Explicit signaling attempt ended after lifecycle cancellation"
                    );
                }
                Err(e)
            }
        }
    }

    /// Wait for ongoing connection attempt to complete (used when another task is connecting).
    ///
    /// Uses the broadcast channel to wait for a Connected event without recursion.
    async fn wait_for_connection_result(&self, generation: u64) -> NetworkResult<()> {
        let mut event_rx = self.event_tx.subscribe();
        let deadline = self
            .connection_attempt_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .or_else(|| self.connection_attempt_deadline());

        loop {
            let attempt_finished = self.connection_attempt_notify.notified();
            tokio::pin!(attempt_finished);
            attempt_finished.as_mut().enable();

            if self.connection_generation_changed(generation) {
                return Err(NetworkError::ConnectionError(
                    "Signaling connect wait was cancelled by explicit disconnect".to_string(),
                ));
            }
            if self.connected.load(Ordering::Acquire) {
                return Ok(());
            }
            if !self.connecting.load(Ordering::Acquire) {
                return Err(NetworkError::ConnectionError(
                    "Concurrent signaling connection attempt ended".to_string(),
                ));
            }

            tokio::select! {
                _ = &mut attempt_finished => continue,
                _ = self.connection_generation_notify.notified() => {
                    if self.connection_generation_changed(generation) {
                        return Err(NetworkError::ConnectionError(
                            "Signaling connect wait was cancelled by explicit disconnect".to_string(),
                        ));
                    }
                }
                _ = async {
                    match deadline {
                        Some(deadline) => crate::timer::sleep_until(
                            crate::timer::ids::SIGNALING_CONNECTED_WAIT,
                            deadline,
                        ).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    // Final check before giving up
                    if self.connected.load(Ordering::Acquire) {
                        tracing::debug!("Connection succeeded just before timeout");
                        return Ok(());
                    }
                    return Err(NetworkError::ConnectionError(
                        "Timeout waiting for concurrent connection attempt".to_string(),
                    ));
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(SignalingEvent::Connected) => {
                            tracing::debug!("Connection established by another task");
                            return Ok(());
                        }
                        Ok(SignalingEvent::Disconnected { reason }) => {
                            return Err(NetworkError::ConnectionError(format!(
                                "Concurrent signaling connection failed: {reason:?}"
                            )));
                        }
                        Ok(_) => continue, // ConnectStart — keep waiting
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Event receiver lagged by {n} events");
                            // Check current state after lag
                            if self.connected.load(Ordering::Acquire) {
                                return Ok(());
                            }
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            return Err(NetworkError::ConnectionError(
                                "Event channel closed while waiting for connection".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    async fn wait_for_signaling_reconnect(
        &self,
        events: &mut broadcast::Receiver<SignalingEvent>,
    ) -> NetworkResult<()> {
        let wait = async {
            loop {
                if self.is_connected() {
                    return Ok(());
                }
                if !self.idempotent_retry_allowed() {
                    return Err(NetworkError::SignalingReplyDropped);
                }

                match events.recv().await {
                    Ok(SignalingEvent::Connected) => {
                        if self.is_connected() {
                            return Ok(());
                        }
                    }
                    Ok(SignalingEvent::ConnectStart { .. })
                    | Ok(SignalingEvent::Disconnected { .. }) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(NetworkError::ConnectionClosed(
                            "Signaling event channel closed while waiting for reconnect"
                                .to_string(),
                        ));
                    }
                }
            }
        };

        crate::timer::timeout(
            crate::timer::ids::SIGNALING_CONNECTED_WAIT,
            Duration::from_secs(RESPONSE_TIMEOUT_SECS),
            wait,
        )
        .await
        .map_err(|_| {
            NetworkError::TimeoutError(format!(
                "Timed out waiting for signaling reconnect after {}s",
                RESPONSE_TIMEOUT_SECS
            ))
        })?
    }

    /// Send an idempotent signaling request, replaying it once when an
    /// automatic signaling rebuild drops the response waiter.
    ///
    /// `build_flow` constructs a fresh flow for every attempt so a late
    /// response for a superseded generation cannot satisfy the new request;
    /// `interpret` converts the response envelope into the caller's result.
    /// Only read-only requests belong here: writes that lack an idempotency
    /// contract must call [`Self::send_envelope_and_wait_response`] directly.
    async fn send_idempotent_request_with_retry<T>(
        &self,
        request_kind: &'static str,
        build_flow: impl Fn() -> signaling_envelope::Flow,
        interpret: impl Fn(SignalingEnvelope) -> NetworkResult<T>,
    ) -> NetworkResult<T> {
        // Subscribe before sending so the short disconnect/reconnect sequence
        // in a mobile handover cannot race past the replacement Connected
        // event while this request is waiting for its response.
        let mut events = self.event_tx.subscribe();
        let mut retried = false;

        loop {
            let envelope = self.create_envelope(build_flow()).await;
            let response_envelope = match self.send_envelope_and_wait_response(envelope).await {
                Ok(response) => response,
                Err(error)
                    if !retried
                        && matches!(error, NetworkError::SignalingReplyDropped)
                        && self.idempotent_retry_allowed() =>
                {
                    retried = true;
                    self.stats.request_retries.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        request = request_kind,
                        retry_attempt = 1,
                        "Signaling response waiter dropped; waiting for reconnect before retry"
                    );
                    self.wait_for_signaling_reconnect(&mut events).await?;
                    tracing::info!(
                        request = request_kind,
                        retry_attempt = 1,
                        "Retrying signaling request after reconnect"
                    );
                    continue;
                }
                Err(error) => return Err(error),
            };

            return interpret(response_envelope);
        }
    }
}

#[async_trait]
impl SignalingClient for WebSocketSignalingClient {
    async fn connect(&self) -> NetworkResult<()> {
        if self.connected.load(Ordering::Acquire) {
            tracing::debug!("Already connected, skipping connect()");
            return Ok(());
        }

        let generation = self.connection_generation.load(Ordering::Acquire);
        self.connect_once_for_explicit(generation).await
    }

    async fn connect_once_for_recovery(&self, action_id: u64) -> NetworkResult<()> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        match self
            .connect_once_with_intent(ConnectIntent::Recovery {
                generation,
                action_id,
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => match self.handle_auth_verdict(&error).await {
                AuthVerdictAction::Reacquired => Err(NetworkError::ConnectionError(
                    "signaling credential changed during recovery".to_string(),
                )),
                AuthVerdictAction::RetrySameCredential => Err(NetworkError::ConnectionError(
                    "credential recovery deferred; retry signaling later".to_string(),
                )),
                AuthVerdictAction::Denied
                | AuthVerdictAction::ShutdownWhileParked
                | AuthVerdictAction::NotAVerdict => Err(error),
            },
        }
    }

    fn request_signaling_recovery(&self, cause: SignalingRecoveryRequestCause) {
        if let Some(sink) = self.supervisor_fact_sink.get() {
            sink.signaling_connection_required(cause);
        }
    }

    async fn disconnect(&self) -> NetworkResult<()> {
        self.disconnect_internal(true).await
    }

    async fn disconnect_for_recovery(&self) -> NetworkResult<()> {
        self.disconnect_internal_with_retry_mark(true, true).await
    }

    async fn probe_alive(&self, timeout: Duration) -> NetworkResult<()> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(NetworkError::ConnectionError(
                "Signaling client is not connected".to_string(),
            ));
        }

        let probe_deadline = tokio::time::Instant::now() + timeout;
        let probe_id = self.probe_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let payload =
            format!("actr-signaling-probe-{probe_id}-{}", current_unix_secs()).into_bytes();
        let (tx, rx) = oneshot::channel();
        self.pending_pongs.lock().await.insert(payload.clone(), tx);

        let send_deadline = probe_deadline.min(
            tokio::time::Instant::now()
                + std::time::Duration::from_secs(SIGNALING_SEND_TIMEOUT_SECS),
        );
        let ping_payload = payload.clone();
        let send_result =
            crate::timer::timeout_at(crate::timer::ids::SIGNALING_SEND, send_deadline, async {
                let mut sink_guard = self.ws_sink.lock().await;
                match sink_guard.as_mut() {
                    Some(sink) => sink
                        .send(tokio_tungstenite::tungstenite::Message::Ping(
                            ping_payload.into(),
                        ))
                        .await
                        .map_err(|e| {
                            NetworkError::ConnectionError(format!(
                                "Signaling probe ping failed: {e}"
                            ))
                        }),
                    None => Err(NetworkError::ConnectionError(
                        "Signaling probe failed: WebSocket sink is not available".to_string(),
                    )),
                }
            })
            .await
            .unwrap_or_else(|_| {
                Err(NetworkError::TimeoutError(format!(
                    "Timed out sending signaling probe ping after {}ms",
                    timeout.as_millis()
                )))
            });

        if let Err(e) = send_result {
            self.pending_pongs.lock().await.remove(&payload);
            let generation = self.connection_generation.load(Ordering::Acquire);
            let was_connected = {
                let _commit_guard = self.lock_connection_commit();
                let was_connected = self.connected.swap(false, Ordering::AcqRel);
                if was_connected {
                    *self
                        .socket_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        SignalingSocketState::Disconnected {
                            last_generation: Some(generation),
                        };
                }
                was_connected
            };
            Self::publish_disconnected_transition(
                was_connected,
                &self.stats,
                &self.event_tx,
                self.hook_callback.get().cloned(),
                DisconnectReason::PingSendFailed,
            )
            .await;
            return Err(e);
        }

        match crate::timer::timeout_at(crate::timer::ids::SIGNALING_PROBE, probe_deadline, rx).await
        {
            Ok(Ok(())) => {
                self.liveness_generation.fetch_add(1, Ordering::Release);
                Ok(())
            }
            Ok(Err(_)) => {
                self.pending_pongs.lock().await.remove(&payload);
                Err(NetworkError::ConnectionError(
                    "Signaling probe pong waiter dropped".to_string(),
                ))
            }
            Err(_) => {
                self.pending_pongs.lock().await.remove(&payload);
                Err(NetworkError::TimeoutError(format!(
                    "Timed out waiting for signaling probe pong after {}ms",
                    timeout.as_millis()
                )))
            }
        }
    }

    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(name = "signaling.register", skip_all)
    )]
    async fn send_register_request(
        &self,
        request: RegisterRequest,
    ) -> NetworkResult<RegisterResponse> {
        // Create PeerToSignaling stream process (Register front)
        let flow = signaling_envelope::Flow::PeerToServer(PeerToSignaling {
            payload: Some(peer_to_signaling::Payload::RegisterRequest(request)),
        });

        let envelope = self.create_envelope(flow).await;
        let response_envelope = self.send_envelope_and_wait_response(envelope).await?;

        if let Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) = response_envelope.flow
        {
            if let Some(signaling_to_actr::Payload::RegisterResponse(response)) =
                server_to_actr.payload
            {
                return Ok(response);
            }
        }

        Err(NetworkError::ConnectionError(
            "Invalid registration response".to_string(),
        ))
    }

    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            name = "signaling.unregister",
            skip_all,
            fields(actor_id = %actor_id)
        )
    )]
    async fn send_unregister_request(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        reason: Option<String>,
    ) -> NetworkResult<UnregisterResponse> {
        // Build UnregisterRequest payload
        let request = UnregisterRequest {
            actr_id: actor_id.clone(),
            reason,
        };

        // Wrap into ActrToSignaling flow
        let flow = signaling_envelope::Flow::ActrToServer(ActrToSignaling {
            source: actor_id,
            credential,
            payload: Some(actr_to_signaling::Payload::UnregisterRequest(request)),
        });

        // Send envelope (fire-and-forget)
        let envelope = self.create_envelope(flow).await;
        self.send_envelope(envelope).await?;

        // Do not wait for UnregisterResponse here because the signaling stream
        // is also consumed by the v0.20 signaling router. Waiting could race with that loop
        // and lead to spurious timeouts. Treat Unregister as best-effort.
        // not wait for the response , because the signaling stream have multi customers use it, fixme: should wait for the response
        Ok(UnregisterResponse {
            result: Some(actr_protocol::unregister_response::Result::Success(
                actr_protocol::unregister_response::UnregisterOk {},
            )),
        })
    }

    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            name = "signaling.heartbeat",
            level = "debug",
            skip_all,
            fields(actor_id = %actor_id)
        )
    )]
    async fn send_heartbeat(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        availability: ServiceAvailabilityState,
        power_reserve: f32,
        mailbox_backlog: f32,
    ) -> NetworkResult<Pong> {
        let ping = Ping {
            availability: availability as i32,
            power_reserve,
            mailbox_backlog,
            sticky_client_ids: vec![], // TODO: Implement sticky session tracking
        };

        let flow = signaling_envelope::Flow::ActrToServer(ActrToSignaling {
            source: actor_id,
            credential,
            payload: Some(actr_to_signaling::Payload::Ping(ping)),
        });

        let envelope = self.create_envelope(flow).await;
        let reply_for = envelope.envelope_id.clone();

        // Register waiter before sending
        let (tx, rx) = oneshot::channel();
        self.pending_replies
            .lock()
            .await
            .insert(reply_for.clone(), tx);

        if let Err(e) = self.send_envelope(envelope).await {
            // Cleanup waiter on immediate send failure to avoid leaks.
            self.pending_replies.lock().await.remove(&reply_for);
            return Err(e);
        }

        // Wait for response
        let response_envelope = rx.await.map_err(|_| {
            NetworkError::ConnectionError(
                "Receiver dropped while waiting for heartbeat response".to_string(),
            )
        })?;

        // Extract Pong from response, or handle Error response
        if let Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) = response_envelope.flow
        {
            match server_to_actr.payload {
                Some(signaling_to_actr::Payload::Pong(pong)) => {
                    return Ok(pong);
                }
                Some(signaling_to_actr::Payload::Error(err)) => {
                    // A credential rejection (401) is refresh-eligible: surface
                    // it as CredentialExpired so the heartbeat path triggers a
                    // re-acquire.
                    if err.code == 401 {
                        return Err(NetworkError::CredentialExpired(err.message));
                    }
                    // A realm denial (403) is terminal. Carry the typed
                    // RealmDenied verdict — the same one a handshake 403 produces
                    // — so the membership authority stops recovery instead of the
                    // heartbeat layer downgrading it to a generic auth error and
                    // retrying the dead credential forever.
                    if err.code == 403 {
                        return Err(NetworkError::signaling_realm_denied(format!(
                            "heartbeat rejected by realm: {} ({})",
                            err.message, err.code
                        )));
                    }
                    return Err(NetworkError::AuthenticationError(format!(
                        "{} ({})",
                        err.message, err.code
                    )));
                }
                _ => {}
            }
        }

        Err(NetworkError::ConnectionError(
            "Received response but not a Pong message".to_string(),
        ))
    }

    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(name = "signaling.route_candidates", skip_all)
    )]
    async fn send_route_candidates_request(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        request: RouteCandidatesRequest,
    ) -> NetworkResult<RouteCandidatesResponse> {
        // RouteCandidates is a read-only, idempotent query.
        self.send_idempotent_request_with_retry(
            "RouteCandidates",
            move || {
                signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                    source: actor_id.clone(),
                    credential: credential.clone(),
                    payload: Some(actr_to_signaling::Payload::RouteCandidatesRequest(
                        request.clone(),
                    )),
                })
            },
            |response_envelope| {
                if let Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) =
                    response_envelope.flow
                {
                    match server_to_actr.payload {
                        Some(signaling_to_actr::Payload::RouteCandidatesResponse(response)) => {
                            return Ok(response);
                        }
                        Some(signaling_to_actr::Payload::Error(err)) => {
                            return Err(NetworkError::ServiceDiscoveryError(format!(
                                "{} ({})",
                                err.message, err.code
                            )));
                        }
                        _ => {}
                    }
                }

                Err(NetworkError::ConnectionError(
                    "Invalid route candidates response".to_string(),
                ))
            },
        )
        .await
    }

    /// GetSigningKey is a read-only, idempotent query, so it shares the
    /// single automatic replay of dropped waiters.
    async fn get_signing_key(
        &self,
        actor_id: ActrId,
        credential: AIdCredential,
        key_id: u32,
    ) -> NetworkResult<(u32, Vec<u8>)> {
        self.send_idempotent_request_with_retry(
            "GetSigningKey",
            move || {
                signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                    source: actor_id.clone(),
                    credential: credential.clone(),
                    payload: Some(actr_to_signaling::Payload::GetSigningKeyRequest(
                        GetSigningKeyRequest { key_id },
                    )),
                })
            },
            |response_envelope| {
                if let Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) =
                    response_envelope.flow
                {
                    match server_to_actr.payload {
                        Some(signaling_to_actr::Payload::GetSigningKeyResponse(resp)) => {
                            return Ok((resp.key_id, resp.pubkey.to_vec()));
                        }
                        Some(signaling_to_actr::Payload::Error(err)) => {
                            return Err(NetworkError::ConnectionError(format!(
                                "get_signing_key failed: {} ({})",
                                err.message, err.code
                            )));
                        }
                        _ => {}
                    }
                }

                Err(NetworkError::ConnectionError(
                    "get_signing_key: invalid response".to_string(),
                ))
            },
        )
        .await
    }

    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            name = "signaling.send",
            level = "debug",
            skip_all,
            fields(envelope_id = %envelope.envelope_id)
        )
    )]
    async fn send_envelope(&self, envelope: SignalingEnvelope) -> NetworkResult<()> {
        #[cfg(feature = "opentelemetry")]
        let envelope = {
            let mut envelope = envelope;
            trace::inject_span_context(&tracing::Span::current(), &mut envelope);
            envelope
        };

        // Check connection state first to avoid sending on stale/closed connections
        // This prevents "Broken pipe" errors when ws_sink exists but connection is dead
        if !self.is_connected() {
            return Err(NetworkError::ConnectionError(
                "Cannot send: WebSocket not connected".to_string(),
            ));
        }

        // Snapshot the generation the send runs on: a recovery that rebuilds
        // signaling while this send is failing must keep its replacement, so
        // the send-failure teardown is fenced to the generation observed
        // here rather than to whichever generation is live when the failure
        // is finally processed.
        let socket_generation = self.connection_generation.load(Ordering::Acquire);

        // Bound the complete commit, including contention on the shared sink
        // mutex. Timing out only `sink.send()` still permits an unbounded wait
        // before the send future is ever created.
        let mut buf = Vec::new();
        envelope.encode(&mut buf)?;
        let msg = tokio_tungstenite::tungstenite::Message::Binary(buf.into());
        let send_result = crate::timer::timeout(
            crate::timer::ids::SIGNALING_SEND,
            std::time::Duration::from_secs(SIGNALING_SEND_TIMEOUT_SECS),
            async {
                let mut sink_guard = self.ws_sink.lock().await;
                if !self.connected.load(Ordering::Acquire) {
                    return Err(NetworkError::ConnectionError(
                        "Cannot send: WebSocket disconnected while waiting for sink".to_string(),
                    ));
                }
                match sink_guard.as_mut() {
                    Some(sink) => sink.send(msg).await.map_err(NetworkError::from),
                    None => Err(NetworkError::ConnectionError("Not connected".to_string())),
                }
            },
        )
        .await;

        match send_result {
            Ok(Ok(())) => {
                if !self.connected.load(Ordering::Acquire) {
                    self.stats.errors.fetch_add(1, Ordering::Relaxed);
                    return Err(NetworkError::ConnectionError(
                        "Signaling send completed after the WebSocket was invalidated".to_string(),
                    ));
                }
                self.stats.messages_sent.fetch_add(1, Ordering::Relaxed);
                tracing::debug!("Stats: {:?}", self.stats.snapshot());
                Ok(())
            }
            Ok(Err(err)) => {
                tracing::warn!("Signaling envelope send failed: {err}");
                self.handle_envelope_send_failure(socket_generation, DisconnectReason::SendFailed)
                    .await;
                Err(err)
            }
            Err(_) => {
                tracing::warn!(
                    "Signaling WebSocket sink lock/send timed out after {}s",
                    SIGNALING_SEND_TIMEOUT_SECS
                );
                self.handle_envelope_send_failure(socket_generation, DisconnectReason::SendTimeout)
                    .await;
                Err(NetworkError::ConnectionError(
                    "Signaling WebSocket sink lock/send timed out".to_string(),
                ))
            }
        }
    }

    async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>> {
        self.receive_envelope_with_span()
            .await
            .map(|envelope| envelope.map(|(envelope, _span)| envelope))
    }

    async fn receive_envelope_with_span(
        &self,
    ) -> NetworkResult<Option<(SignalingEnvelope, tracing::Span)>> {
        let mut rx = self.inbound_rx.lock().await;
        match rx.recv().await {
            Some(queued) => {
                // Destructuring drops the byte-budget permit here, returning the
                // reserved bytes to the budget now that the envelope is handed
                // to the consumer.
                let QueuedInbound { envelope, span, .. } = queued;
                Ok(Some((envelope, span)))
            }
            None => {
                tracing::error!("Inbound channel closed");
                Err(NetworkError::ConnectionError(
                    "Inbound channel closed".to_string(),
                ))
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn get_stats(&self) -> SignalingStats {
        self.stats.snapshot()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent> {
        self.event_tx.subscribe()
    }

    async fn set_actor_id(&self, actor_id: ActrId) {
        *self.actor_id.lock().await = Some(actor_id);
    }

    async fn set_credential_state(&self, credential_state: CredentialState) {
        *self.credential_state.lock().await = Some(credential_state);
    }

    async fn clear_identity(&self) {
        *self.actor_id.lock().await = None;
        *self.credential_state.lock().await = None;
    }

    fn set_hook_callback(&self, cb: HookCallback) {
        let _ = self.hook_callback.set(cb);
    }

    fn set_supervisor_fact_sink(&self, sink: SupervisorFactSink) {
        // A production node installs its sink before its first handshake, but
        // embedders and VNet harnesses may finish lifecycle wiring after a
        // socket is already live. Seed that sink from the same commit-fenced
        // resource state that emitted the original transition; otherwise a
        // later `SignalingGenerationLost` fact is correctly rejected as stale
        // because the supervisor never learned the generation was live.
        // Holding this gate keeps a new connection commit from publishing a
        // resource fact between the configuration transfer and the seed. The
        // configuration itself is sent before making the sink visible, so the
        // supervisor never executes a loss with its default retry policy just
        // because lifecycle wiring raced an already-running socket.
        let _commit_guard = self.lock_connection_commit();
        if self.supervisor_fact_sink.get().is_some() {
            return;
        }

        // Preserve the public reconnect knobs after moving retry ownership to
        // the supervisor. `enabled` remains the compatibility-shim gate for
        // callers that explicitly request automatic recovery; the window and
        // curve values below are now interpreted by the one policy owner.
        let reconnect = &self.config.reconnect_config;
        sink.signaling_recovery_configured(
            reconnect.max_attempts,
            Duration::from_secs(reconnect.initial_delay),
            Duration::from_secs(reconnect.max_delay),
            reconnect.backoff_multiplier,
        );
        if self.supervisor_fact_sink.set(sink.clone()).is_err() {
            return;
        }

        let live_generation = match *self
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            SignalingSocketState::Connected { generation, .. } => Some(generation),
            _ => None,
        };
        if let Some(generation) = live_generation {
            sink.signaling_generation_committed(generation, SignalingFactOrigin::External);
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn live_generation_for_test(&self) -> Option<u64> {
        let _commit_guard = self.lock_connection_commit();
        match *self
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            SignalingSocketState::Connected { generation, .. } => Some(generation),
            _ => None,
        }
    }

    #[cfg(test)]
    fn emit_test_signaling_committed(&self, generation: u64, origin: SignalingFactOrigin) {
        self.emit_signaling_committed(generation, origin);
    }

    fn invalidate_generation(&self) {
        // Bump the connection generation so no in-flight connect can still
        // commit. The teardown-crossing
        // `SignalingGenerationLost` is delivered by the disconnect that follows
        // (and, decisively, by the supervisor dropping the teardown-scoped
        // generation on teardown completion), so this synchronous close only
        // invalidates commit rights; the supervisor remains the sole owner of
        // new-work admission.
        self.invalidate_connection_attempts();
    }

    async fn install_membership(&self, handle: MembershipHandle) {
        WebSocketSignalingClient::install_membership(self, handle);
    }
}

/// signaling statistics info
#[derive(Debug)]
pub(crate) struct AtomicSignalingStats {
    /// Connect attempts
    pub connections: AtomicU64,

    /// DisconnectConnect attempts
    pub disconnections: AtomicU64,

    /// Send'smessage number
    pub messages_sent: AtomicU64,

    /// Receive'smessage number
    pub messages_received: AtomicU64,

    /// Send's center skip number
    /// TODO: Wire heartbeat counters when heartbeat send/receive paths are instrumented; currently never incremented.
    pub heartbeats_sent: AtomicU64,

    /// Receive's center skip number
    /// TODO: Wire heartbeat counters when heartbeat send/receive paths are instrumented; currently never incremented.
    pub heartbeats_received: AtomicU64,

    /// Error attempts
    pub errors: AtomicU64,

    /// Unmatched/push envelopes successfully queued for the consumer (issue #351).
    pub inbound_enqueued: AtomicU64,

    /// Inbound frames rejected for exceeding `max_frame_bytes` before decode (issue #351).
    pub inbound_oversized: AtomicU64,

    /// Times the inbound queue stayed full past the backpressure deadline (issue #351).
    pub inbound_saturated: AtomicU64,

    /// Connections torn down because inbound backpressure forced a fail-closed (issue #351).
    pub inbound_backpressure_closes: AtomicU64,

    /// Response waiters dropped by a signaling rebuild or send failure.
    pub reply_waiters_dropped: AtomicU64,

    /// Idempotent requests replayed once after a signaling reconnect.
    pub request_retries: AtomicU64,
}

impl Default for AtomicSignalingStats {
    fn default() -> Self {
        Self {
            connections: AtomicU64::new(0),
            disconnections: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            heartbeats_sent: AtomicU64::new(0),
            heartbeats_received: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            inbound_enqueued: AtomicU64::new(0),
            inbound_oversized: AtomicU64::new(0),
            inbound_saturated: AtomicU64::new(0),
            inbound_backpressure_closes: AtomicU64::new(0),
            reply_waiters_dropped: AtomicU64::new(0),
            request_retries: AtomicU64::new(0),
        }
    }
}

/// Snapshot of statistics for serialization and reading
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SignalingStats {
    /// Connect attempts
    pub connections: u64,

    /// DisconnectConnect attempts
    pub disconnections: u64,

    /// Send'smessage number
    pub messages_sent: u64,

    /// Receive'smessage number
    pub messages_received: u64,

    /// Send's center skip number
    pub heartbeats_sent: u64,

    /// Receive's center skip number
    pub heartbeats_received: u64,

    /// Error attempts
    pub errors: u64,

    /// Unmatched/push envelopes successfully queued for the consumer (issue #351).
    pub inbound_enqueued: u64,

    /// Inbound frames rejected for exceeding `max_frame_bytes` before decode (issue #351).
    pub inbound_oversized: u64,

    /// Times the inbound queue stayed full past the backpressure deadline (issue #351).
    pub inbound_saturated: u64,

    /// Connections torn down because inbound backpressure forced a fail-closed (issue #351).
    pub inbound_backpressure_closes: u64,

    /// Response waiters dropped by a signaling rebuild or send failure.
    #[serde(default)]
    pub reply_waiters_dropped: u64,

    /// Idempotent requests replayed once after a signaling reconnect.
    #[serde(default)]
    pub request_retries: u64,
}

impl AtomicSignalingStats {
    /// Create a snapshot of current statistics
    pub fn snapshot(&self) -> SignalingStats {
        SignalingStats {
            connections: self.connections.load(Ordering::Relaxed),
            disconnections: self.disconnections.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            heartbeats_sent: self.heartbeats_sent.load(Ordering::Relaxed),
            heartbeats_received: self.heartbeats_received.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            inbound_enqueued: self.inbound_enqueued.load(Ordering::Relaxed),
            inbound_oversized: self.inbound_oversized.load(Ordering::Relaxed),
            inbound_saturated: self.inbound_saturated.load(Ordering::Relaxed),
            inbound_backpressure_closes: self.inbound_backpressure_closes.load(Ordering::Relaxed),
            reply_waiters_dropped: self.reply_waiters_dropped.load(Ordering::Relaxed),
            request_retries: self.request_retries.load(Ordering::Relaxed),
        }
    }
}

fn current_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug)]
struct PongWatchdog {
    observed_generation: u64,
    silent_active_secs: u64,
}

impl PongWatchdog {
    fn new(observed_generation: u64) -> Self {
        Self {
            observed_generation,
            silent_active_secs: 0,
        }
    }

    /// Advance after one completed ping interval.
    ///
    /// The caller invokes this once per scheduled wake, so a wake delayed by
    /// suspend or scheduler throttling counts as one interval rather than the
    /// wall duration of the delay. Late timeout is allowed; early timeout
    /// after resume is not.
    fn tick(&mut self, current_generation: u64, interval_secs: u64) -> u64 {
        if current_generation != self.observed_generation {
            self.observed_generation = current_generation;
            self.silent_active_secs = 0;
        } else {
            self.silent_active_secs = self.silent_active_secs.saturating_add(interval_secs);
        }
        self.silent_active_secs
    }
}

#[cfg(test)]
#[path = "signaling_tests.rs"]
mod tests;
