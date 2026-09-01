//! Workload trait - Executable actor workload
//!
//! Defines the user-facing programming interface: each logical Actor
//! implements (or has generated for it) a [`Workload`] that associates a
//! [`MessageDispatcher`] and exposes a fixed set of observation hooks the
//! framework fires over the lifetime of the actor node.

use std::time::SystemTime;

use actr_protocol::{ActorResult, ActrError, ActrId};
use async_trait::async_trait;

use crate::{Context, MessageDispatcher};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Event payloads
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Peer-scoped event payload for transport hooks.
///
/// Public WebRTC peer status exposed through observation hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcPeerStatus {
    Idle,
    Connecting,
    Connected,
    Recovering,
}

/// Used by WebSocket and WebRTC hook callbacks to identify the remote peer
/// involved in the state change. For WebRTC, `relayed` reports whether the
/// selected ICE candidate pair traversed a TURN relay, and `status` reports
/// the coarse send-readiness state. For WebSocket both fields are always
/// `None` (not applicable).
#[derive(Debug, Clone)]
pub struct PeerEvent {
    /// Remote peer identity.
    pub peer: ActrId,
    /// `Some(true)` if the WebRTC connection is TURN-relayed, `Some(false)` for
    /// a direct peer-to-peer connection. Always `None` for WebSocket events.
    pub relayed: Option<bool>,
    /// Coarse WebRTC send-readiness state. Always `None` for WebSocket events.
    pub status: Option<WebRtcPeerStatus>,
}

/// Stable, FFI-safe reason a WebRTC peer is unavailable (RFC §"Error model":
/// "a Snapshot's `unavailable_reason` SHOULD use a stable enum rather than
/// exposing underlying error strings to the FFI").
///
/// `#[non_exhaustive]` per the API compatibility policy (AGENTS.md): the
/// recovery state machine grows new failure modes as additive fixes, and
/// boundary mappings (FFI / WIT) must decide how to project an unknown
/// variant instead of being source-broken by one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebRtcUnavailableReason {
    ConnectionNotReady,
    PeerConnectionBuildFailed,
    NegotiationFailed,
    IceRestartFailed,
    PeerConnectionRebuildFailed,
    UnsupportedRtcCapability,
    RtcResourceLimitExceeded,
    /// Actrix confirmed that the exact remote `ActrId` has no usable
    /// signaling session. The framework closes that endpoint but deliberately
    /// does not discover or select a replacement service instance.
    PeerUnavailable,
    RecoveryExhausted,
    Closed,
}

/// Whether the selected ICE candidate pair is direct or relayed (v0.20
/// backend). Resolved asynchronously from stats; never blocks
/// `Connected`/`sendable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebRtcNetworkPath {
    /// Stats have not resolved the selected pair yet.
    #[default]
    Unknown,
    /// Direct peer-to-peer path (no TURN relay on either side).
    Direct,
    /// TURN-relayed path.
    Relayed,
}

/// A point-in-time, FFI-safe view of one WebRTC peer's state (Stage 6 unified
/// snapshot; RFC §"RtcPeerSnapshot"). Returned by the node's
/// get/list/subscribe status APIs and carried by `on_webrtc_state_changed`.
///
/// Subscribers replay the latest value immediately on subscribe; `revision`
/// is monotonic per peer so consumers can de-duplicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRtcPeerState {
    /// Remote peer identity.
    pub peer: ActrId,
    /// Monotonic PeerConnection generation (bumps on every Hard Recovery
    /// rebuild).
    pub pc_generation: u64,
    /// Monotonic snapshot revision (per-peer dedup key).
    pub revision: u64,
    /// Coarse lifecycle status.
    pub status: WebRtcPeerStatus,
    /// Whether business sends are currently admitted.
    pub sendable: bool,
    /// The selected ICE path (S6-7a). `Unknown` until stats resolve it; never
    /// blocks `sendable`.
    pub network_path: WebRtcNetworkPath,
    /// `Some(true)` if the selected ICE pair traverses a TURN relay,
    /// `Some(false)` for direct, `None` while stats have not resolved.
    /// Derived from `network_path`; kept for one compatibility cycle — new
    /// consumers should read `network_path`.
    pub relayed: Option<bool>,
    /// Stable reason the peer is unavailable, when it is.
    pub unavailable_reason: Option<WebRtcUnavailableReason>,
}

/// Error event payload passed to [`Workload::on_error`].
///
/// Wraps the structured cause ([`ActrError`]), a coarse domain
/// classification, free-form context describing where the error happened,
/// and a wall-clock timestamp captured at the reporting site.
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    /// Underlying protocol error.
    pub source: ActrError,
    /// Coarse category to aid routing / alerting in user handlers.
    pub category: ErrorCategory,
    /// Free-form human-readable context (route key, handler name, stage, ...).
    pub context: String,
    /// Wall-clock timestamp captured when the event was emitted.
    pub timestamp: SystemTime,
}

impl ErrorEvent {
    /// Convenience constructor: stamp `timestamp` with `SystemTime::now()`.
    pub fn now(source: ActrError, category: ErrorCategory, context: impl Into<String>) -> Self {
        Self {
            source,
            category,
            context: context.into(),
            timestamp: SystemTime::now(),
        }
    }
}

/// Coarse fault-domain classification for [`ErrorEvent`].
///
/// Mirrors the top-level protocol [`actr_protocol::ErrorKind`] but is
/// specialised for the *dispatch* boundary — callers want to distinguish
/// "user code blew up" (`HandlerPanic` / `HandlerError`) from "the plumbing
/// under them failed" (`SignalingFailure` / `TransportFailure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// User handler code panicked. `source` will typically be a
    /// [`ActrError::DecodeFailure`] wrapping the panic message.
    HandlerPanic,
    /// User handler returned `Err` normally. `source` is the handler's
    /// own [`ActrError`] value.
    HandlerError,
    /// Signaling layer failure (AIS registration, reconnect, credential
    /// verification).
    SignalingFailure,
    /// Transport layer failure: WebSocket / WebRTC connection errors, lane
    /// / mpsc plumbing faults.
    TransportFailure,
    /// A `send_data_chunk` was active when the WebRTC/DataChannel path was
    /// interrupted. Delivery is uncertain; the framework has not confirmed
    /// loss or performed resume.
    DataChunkDeliveryUncertain,
}

/// Credential lifecycle event.
///
/// Fired on initial registration and any subsequent renewal (via
/// [`Workload::on_credential_renewed`]), and also used to warn when an
/// active credential is approaching its expiry (via
/// [`Workload::on_credential_expiring`]).
#[derive(Debug, Clone)]
pub struct CredentialEvent {
    /// Absolute expiry time of the credential that triggered the event.
    pub new_expiry: SystemTime,
}

/// Mailbox backpressure event.
///
/// Fires once per "mailbox pressure crossed configured threshold" incident.
/// Use [`HyperConfig::mailbox_backpressure_threshold`] (see `actr-hyper`) to
/// tune the trip level.
#[derive(Debug, Clone, Copy)]
pub struct BackpressureEvent {
    /// Current queued-message count at the time of the sample.
    pub queue_len: usize,
    /// Threshold that was crossed.
    pub threshold: usize,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Workload trait
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Workload — Executable Actor workload
///
/// Represents a complete Actor instance, composed of:
/// - an associated [`MessageDispatcher`] ([`Workload::Dispatcher`]) that
///   knows how to decode and route incoming RPC envelopes;
/// - sixteen observation hooks grouped by category (see below), each with a
///   meaningful default that emits a `tracing` record so unmodified
///   workloads produce useful operational logs out of the box.
///
/// # Design
///
/// - **Bidirectional association**: [`Workload::Dispatcher`] and
///   [`MessageDispatcher::Workload`] refer to each other so that generated
///   dispatchers can access the user-implemented workload type.
/// - **Meaningful defaults**: every hook has a `tracing`-emitting default
///   (`info` / `debug` / `warn` / `error` levels tuned per hook). Users
///   override only the hooks whose behaviour they want to extend.
/// - **Auto-implementation**: the code generator emits
///   `impl<T: Handler> Workload for Wrapper<T> { type Dispatcher = Router<T>; }`
///   which inherits every default implementation. Generated wrappers thus
///   get 16 hooks "for free".
///
/// # Hook categories
///
/// Hooks are organised into six categories. Override only the hooks you need.
///
/// ## Lifecycle (4) — fallible
/// - [`Workload::on_start`] — node started, before accepting requests
/// - [`Workload::on_ready`] — node registered and ready to accept requests
/// - [`Workload::on_stop`] — shutdown signal received
/// - [`Workload::on_error`] — framework caught a runtime error
///
/// Lifecycle hooks return [`ActorResult`]; errors propagate and abort startup
/// (for `on_start`) or are logged by the framework (for the others).
///
/// ## Signaling (3) — infallible
/// - [`Workload::on_signaling_connecting`]
/// - [`Workload::on_signaling_connected`]
/// - [`Workload::on_signaling_disconnected`]
///
/// ## Transport — WebSocket (3) — infallible
/// - [`Workload::on_websocket_connecting`]
/// - [`Workload::on_websocket_connected`]
/// - [`Workload::on_websocket_disconnected`]
///
/// ## Transport — WebRTC P2P (3) — infallible
/// - [`Workload::on_webrtc_connecting`]
/// - [`Workload::on_webrtc_connected`] — includes `relayed` info via
///   [`PeerEvent::relayed`]
/// - [`Workload::on_webrtc_disconnected`]
///
/// ## Credential (2) — infallible
/// - [`Workload::on_credential_renewed`]
/// - [`Workload::on_credential_expiring`]
///
/// ## Mailbox (1) — infallible
/// - [`Workload::on_mailbox_backpressure`]
///
/// # Code Generation Example
///
/// ```rust,ignore
/// // User-implemented Handler
/// pub struct MyEchoService { /* ... */ }
///
/// impl EchoServiceHandler for MyEchoService {
///     async fn echo<C: Context>(
///         &self,
///         req: EchoRequest,
///         ctx: &C,
///     ) -> ActorResult<EchoResponse> {
///         Ok(EchoResponse { reply: format!("Echo: {}", req.message) })
///     }
/// }
///
/// // Code-generated adapter and workload wrapper — the dispatcher is shared
/// // by every service in the DispatchSpec.
/// use generated::actr_dispatch::ActrWorkload;
/// use generated::actr_handler::ActrHandler;
///
/// let workload = ActrWorkload::new(ActrHandler::new(MyEchoService));
/// ```
// Workload trait is `?Send` on `wasm32` (browser single-threaded) and keeps
// the native default `Send` auto trait elsewhere so tokio-multi-thread-backed
// adapters on the native side continue to produce `Send` futures without
// fighting `async_trait`. Per Option U gamma-unified §3.2 the user-facing bound
// is `'static`; `MaybeSendSync` silently re-adds `Send + Sync` on native so
// the lifecycle-hook default bodies and the hyper `WorkloadAdapter`
// downstream see the `Send` futures they require.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Workload: crate::MaybeSendSync + 'static {
    /// Associated dispatcher type.
    type Dispatcher: MessageDispatcher<Workload = Self>;

    // ─────────────────────────────────────────────────────────────────────
    // Lifecycle
    // ─────────────────────────────────────────────────────────────────────

    /// Called when the node has started.
    ///
    /// Use this to initialise business resources, start timers, etc. Returning
    /// `Err` aborts node startup.
    async fn on_start<C: Context>(&self, _ctx: &C) -> ActorResult<()> {
        tracing::info!("workload on_start");
        Ok(())
    }

    /// Called when signaling is connected and registration is complete: the
    /// node is now discoverable and may serve requests.
    async fn on_ready<C: Context>(&self, _ctx: &C) -> ActorResult<()> {
        tracing::info!("workload on_ready (node ready to accept requests)");
        Ok(())
    }

    /// Called when the node receives a shutdown signal.
    ///
    /// Use this to release business resources and persist state.
    async fn on_stop<C: Context>(&self, _ctx: &C) -> ActorResult<()> {
        tracing::info!("workload on_stop");
        Ok(())
    }

    /// Called when the framework catches a runtime error.
    ///
    /// See [`ErrorEvent`] and [`ErrorCategory`] for the structure of the
    /// argument. Use this for alerting, logging, or graceful degradation.
    async fn on_error<C: Context>(&self, _ctx: &C, event: &ErrorEvent) -> ActorResult<()> {
        tracing::error!(
            category = ?event.category,
            context = %event.context,
            source = %event.source,
            "workload on_error",
        );
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────
    // Signaling
    // ─────────────────────────────────────────────────────────────────────

    /// Called when signaling connection attempt begins.
    ///
    /// `ctx` is `None` during the initial connection (before the node has
    /// obtained its identity) and `Some` for every subsequent reconnect.
    async fn on_signaling_connecting<C: Context>(&self, _ctx: Option<&C>) {
        tracing::debug!("signaling connecting");
    }

    /// Called when signaling connection is established. Actor is online.
    ///
    /// `ctx` is `None` during the initial connection and `Some` for every
    /// subsequent reconnect.
    async fn on_signaling_connected<C: Context>(&self, _ctx: Option<&C>) {
        tracing::info!("signaling connected");
    }

    /// Called when signaling connection is lost. Actor is offline.
    async fn on_signaling_disconnected<C: Context>(&self, _ctx: &C) {
        tracing::warn!("signaling disconnected");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Transport — WebSocket
    // ─────────────────────────────────────────────────────────────────────

    /// Called when a WebSocket connection attempt to a peer begins.
    async fn on_websocket_connecting<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::debug!(peer = %event.peer, "websocket connecting");
    }

    /// Called when a WebSocket connection to a peer is established.
    async fn on_websocket_connected<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::info!(peer = %event.peer, "websocket connected");
    }

    /// Called when a WebSocket connection to a peer is lost.
    async fn on_websocket_disconnected<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::warn!(peer = %event.peer, "websocket disconnected");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Transport — WebRTC P2P
    // ─────────────────────────────────────────────────────────────────────

    /// Called when a WebRTC P2P connection attempt to a peer begins.
    async fn on_webrtc_connecting<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::debug!(peer = %event.peer, "webrtc connecting");
    }

    /// Called when a WebRTC P2P connection to a peer is established.
    ///
    /// `event.relayed` carries whether the selected ICE candidate pair
    /// traverses a TURN relay (`Some(true)`) or is a direct P2P connection
    /// (`Some(false)`).
    async fn on_webrtc_connected<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::info!(
            peer = %event.peer,
            relayed = ?event.relayed,
            "webrtc connected",
        );
    }

    /// Called when a WebRTC P2P connection to a peer is lost.
    async fn on_webrtc_disconnected<C: Context>(&self, _ctx: &C, event: &PeerEvent) {
        tracing::warn!(peer = %event.peer, "webrtc disconnected");
    }

    /// Called on every revision of a peer's unified WebRTC state (Stage 6
    /// v0.20 backend). Carries the full FFI-safe snapshot — status,
    /// sendability, relay path, unavailable reason — so consumers that want
    /// more than the three coarse legacy hooks can observe everything.
    ///
    /// A workload installed late still receives the current state on the
    /// first projection pass (initial replay), and `revision` is monotonic
    /// per peer for de-duplication.
    async fn on_webrtc_state_changed<C: Context>(&self, _ctx: &C, event: &WebRtcPeerState) {
        tracing::debug!(
            peer = %event.peer.to_string_repr(),
            status = ?event.status,
            sendable = event.sendable,
            "webrtc state changed",
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Credential
    // ─────────────────────────────────────────────────────────────────────

    /// Called when the current credential is renewed (initial registration
    /// or subsequent refresh).
    async fn on_credential_renewed<C: Context>(&self, _ctx: &C, event: &CredentialEvent) {
        tracing::info!(new_expiry = ?event.new_expiry, "credential renewed");
    }

    /// Called when the active credential is approaching its expiry.
    ///
    /// The warning lead time is controlled by
    /// `HyperConfig::credential_expiry_warning` on the hyper layer.
    async fn on_credential_expiring<C: Context>(&self, _ctx: &C, event: &CredentialEvent) {
        tracing::warn!(
            new_expiry = ?event.new_expiry,
            "credential expiring soon",
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Mailbox
    // ─────────────────────────────────────────────────────────────────────

    /// Called once per incident when the persistent mailbox queue length
    /// crosses the configured backpressure threshold.
    ///
    /// Fires once per cross (rising edge); the framework resets the
    /// triggered flag once the queue falls below the threshold again.
    async fn on_mailbox_backpressure<C: Context>(&self, _ctx: &C, event: &BackpressureEvent) {
        tracing::warn!(
            queue_len = event.queue_len,
            threshold = event.threshold,
            "mailbox backpressure",
        );
    }
}
