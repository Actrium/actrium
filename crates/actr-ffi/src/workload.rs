//! Dynamic Workload implementation for callback interfaces
//!
//! The FFI-exposed workload API is shaped as a **multi-observer**:
//!
//! - [`WorkloadLifecycleBridge`] is mandatory: the foreign-language side
//!   supplies a single implementation that handles lifecycle hooks
//!   (`on_start` / `on_ready` / `on_stop` / `on_error`) plus `dispatch`
//!   (message routing).
//! - [`SignalingObserverBridge`], [`WebSocketObserverBridge`],
//!   [`WebRtcObserverBridge`], [`CredentialObserverBridge`] and
//!   [`MailboxObserverBridge`] are **optional** per-category observers.
//!   The foreign-language wrapper library may install only the observers
//!   it cares about; categories left as `None` fall back to the Rust
//!   framework's tracing defaults.
//!
//! This replaces the previous monolithic `WorkloadBridge` (lifecycle +
//! dispatch in one trait with no observation hooks).

use crate::ActrResult;
use crate::context::ContextBridge;
use actr_framework::{
    BackpressureEvent, Bytes, Context, CredentialEvent, ErrorCategory, ErrorEvent,
    MessageDispatcher, PeerEvent, ReplyOutcome, WebRtcPeerState, WebRtcPeerStatus, Workload,
};
use actr_protocol::{ActorResult, ActrError, ReplyMode, RpcEnvelope};
use async_trait::async_trait;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// FFI-safe records mirroring framework event types
// ─────────────────────────────────────────────────────────────────────────────

/// RPC Envelope exposed to FFI
///
/// Contains the route key, payload, request ID, and request deadline for an RPC message.
#[derive(uniffi::Record, Clone)]
pub struct RpcEnvelopeBridge {
    /// Route key for the RPC method (e.g., "echo.EchoService.Echo")
    pub route_key: String,
    /// Request payload bytes (protobuf encoded)
    pub payload: Vec<u8>,
    /// Request ID for correlation
    pub request_id: String,
    /// Request deadline in milliseconds. Responses and tells carry zero.
    pub timeout_ms: i64,
}

impl From<RpcEnvelope> for RpcEnvelopeBridge {
    fn from(envelope: RpcEnvelope) -> Self {
        Self {
            route_key: envelope.route_key,
            payload: envelope.payload.map(|p| p.to_vec()).unwrap_or_default(),
            request_id: envelope.request_id,
            timeout_ms: envelope.timeout_ms,
        }
    }
}

/// Peer-scoped event payload (WebSocket / WebRTC).
#[derive(uniffi::Record, Clone)]
pub struct PeerEventBridge {
    /// Remote peer identity.
    pub peer: crate::types::ActrId,
    /// `Some(true)` for WebRTC TURN-relayed, `Some(false)` for direct P2P,
    /// `None` for WebSocket (not applicable).
    pub relayed: Option<bool>,
    /// Coarse WebRTC send-readiness state. `None` for WebSocket and old
    /// compatibility paths where the status is not available.
    pub status: Option<WebRtcPeerStatusBridge>,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebRtcPeerStatusBridge {
    Idle,
    Connecting,
    Connected,
    Recovering,
}

impl From<WebRtcPeerStatus> for WebRtcPeerStatusBridge {
    fn from(status: WebRtcPeerStatus) -> Self {
        match status {
            WebRtcPeerStatus::Idle => WebRtcPeerStatusBridge::Idle,
            WebRtcPeerStatus::Connecting => WebRtcPeerStatusBridge::Connecting,
            WebRtcPeerStatus::Connected => WebRtcPeerStatusBridge::Connected,
            WebRtcPeerStatus::Recovering => WebRtcPeerStatusBridge::Recovering,
        }
    }
}

impl From<&PeerEvent> for PeerEventBridge {
    fn from(ev: &PeerEvent) -> Self {
        Self {
            peer: ev.peer.clone().into(),
            relayed: ev.relayed,
            status: ev.status.map(Into::into),
        }
    }
}

/// Stable, FFI-safe reason a WebRTC peer is unavailable. Mirrors
/// `actr_framework::WebRtcUnavailableReason`.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebRtcUnavailableReasonBridge {
    ConnectionNotReady,
    PeerConnectionBuildFailed,
    NegotiationFailed,
    IceRestartFailed,
    PeerConnectionRebuildFailed,
    UnsupportedRtcCapability,
    RtcResourceLimitExceeded,
    RecoveryExhausted,
    Closed,
    PeerUnavailable,
}

impl From<actr_framework::WebRtcUnavailableReason> for WebRtcUnavailableReasonBridge {
    fn from(reason: actr_framework::WebRtcUnavailableReason) -> Self {
        use actr_framework::WebRtcUnavailableReason as R;
        match reason {
            R::ConnectionNotReady => Self::ConnectionNotReady,
            R::PeerConnectionBuildFailed => Self::PeerConnectionBuildFailed,
            R::NegotiationFailed => Self::NegotiationFailed,
            R::IceRestartFailed => Self::IceRestartFailed,
            R::PeerConnectionRebuildFailed => Self::PeerConnectionRebuildFailed,
            R::UnsupportedRtcCapability => Self::UnsupportedRtcCapability,
            R::RtcResourceLimitExceeded => Self::RtcResourceLimitExceeded,
            R::PeerUnavailable => Self::PeerUnavailable,
            R::RecoveryExhausted => Self::RecoveryExhausted,
            R::Closed => Self::Closed,
            // The source enum is non_exhaustive; project an unknown reason
            // onto the generic capability bucket to keep the bridge additive.
            _ => Self::UnsupportedRtcCapability,
        }
    }
}

/// Whether the selected ICE candidate pair is direct or relayed. Mirrors
/// `actr_framework::WebRtcNetworkPath`.
#[derive(uniffi::Enum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebRtcNetworkPathBridge {
    /// Stats have not resolved the selected pair yet.
    #[default]
    Unknown,
    /// Direct peer-to-peer path.
    Direct,
    /// TURN-relayed path.
    Relayed,
}

impl From<actr_framework::WebRtcNetworkPath> for WebRtcNetworkPathBridge {
    fn from(path: actr_framework::WebRtcNetworkPath) -> Self {
        match path {
            actr_framework::WebRtcNetworkPath::Unknown => Self::Unknown,
            actr_framework::WebRtcNetworkPath::Direct => Self::Direct,
            actr_framework::WebRtcNetworkPath::Relayed => Self::Relayed,
        }
    }
}

/// FFI-shaped unified WebRTC peer state. Carried by
/// `WebRtcObserverBridge::on_state_changed` and the `ActrRefWrapper` status
/// query/list/subscribe APIs. `revision` is monotonic per peer (dedup key).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct WebRtcPeerStateBridge {
    /// Remote peer identity.
    pub peer: crate::types::ActrId,
    /// Monotonic PeerConnection generation (bumps on Hard Recovery rebuilds).
    pub pc_generation: u64,
    /// Monotonic snapshot revision (per-peer dedup key).
    pub revision: u64,
    /// Coarse lifecycle status.
    pub status: WebRtcPeerStatusBridge,
    /// Whether business sends are currently admitted.
    pub sendable: bool,
    /// The selected ICE path (`Unknown` until stats resolve it).
    pub network_path: WebRtcNetworkPathBridge,
    /// `Some(true)` TURN-relayed, `Some(false)` direct, `None` unresolved.
    /// Derived from `network_path`; kept for one compatibility cycle — new
    /// consumers should read `network_path`.
    pub relayed: Option<bool>,
    /// Stable reason the peer is unavailable, when it is.
    pub unavailable_reason: Option<WebRtcUnavailableReasonBridge>,
}

impl From<actr_framework::WebRtcPeerState> for WebRtcPeerStateBridge {
    fn from(state: actr_framework::WebRtcPeerState) -> Self {
        Self {
            peer: state.peer.into(),
            pc_generation: state.pc_generation,
            revision: state.revision,
            status: state.status.into(),
            sendable: state.sendable,
            network_path: state.network_path.into(),
            relayed: state.relayed,
            unavailable_reason: state.unavailable_reason.map(Into::into),
        }
    }
}

/// Coarse error-event classification mirror of
/// [`actr_framework::ErrorCategory`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCategoryBridge {
    HandlerPanic,
    HandlerError,
    SignalingFailure,
    TransportFailure,
    DataChunkDeliveryUncertain,
}

impl From<ErrorCategory> for ErrorCategoryBridge {
    fn from(c: ErrorCategory) -> Self {
        match c {
            ErrorCategory::HandlerPanic => ErrorCategoryBridge::HandlerPanic,
            ErrorCategory::HandlerError => ErrorCategoryBridge::HandlerError,
            ErrorCategory::SignalingFailure => ErrorCategoryBridge::SignalingFailure,
            ErrorCategory::TransportFailure => ErrorCategoryBridge::TransportFailure,
            ErrorCategory::DataChunkDeliveryUncertain => {
                ErrorCategoryBridge::DataChunkDeliveryUncertain
            }
        }
    }
}

/// FFI-shaped error event.
///
/// `source` is the `Display` of the underlying [`actr_protocol::ActrError`]
/// (the enum itself cannot cross UniFFI unchanged), and `timestamp_ms` is
/// the wall-clock time encoded as milliseconds since the UNIX epoch.
#[derive(uniffi::Record, Clone)]
pub struct ErrorEventBridge {
    /// Stringified underlying error (see [`actr_protocol::ActrError`]).
    pub source: String,
    /// Error-domain classification.
    pub category: ErrorCategoryBridge,
    /// Free-form context (route key, handler name, stage).
    pub context: String,
    /// Wall-clock timestamp (milliseconds since UNIX epoch).
    pub timestamp_ms: i64,
}

impl From<&ErrorEvent> for ErrorEventBridge {
    fn from(ev: &ErrorEvent) -> Self {
        let timestamp_ms = ev
            .timestamp
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            source: ev.source.to_string(),
            category: ev.category.into(),
            context: ev.context.clone(),
            timestamp_ms,
        }
    }
}

/// Credential renewal / warning event.
#[derive(uniffi::Record, Clone)]
pub struct CredentialEventBridge {
    /// New credential expiry as milliseconds since UNIX epoch.
    pub new_expiry_ms: i64,
}

impl From<&CredentialEvent> for CredentialEventBridge {
    fn from(ev: &CredentialEvent) -> Self {
        let ms = ev
            .new_expiry
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self { new_expiry_ms: ms }
    }
}

/// Mailbox backpressure event.
#[derive(uniffi::Record, Clone, Copy)]
pub struct BackpressureEventBridge {
    pub queue_len: u64,
    pub threshold: u64,
}

impl From<&BackpressureEvent> for BackpressureEventBridge {
    fn from(ev: &BackpressureEvent) -> Self {
        Self {
            queue_len: ev.queue_len as u64,
            threshold: ev.threshold as u64,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Callback interfaces
// ─────────────────────────────────────────────────────────────────────────────

/// Required lifecycle + dispatch bridge.
///
/// The foreign-language code supplies exactly one implementation of this
/// interface. It handles the four fallible lifecycle hooks and the core
/// `dispatch` entry point.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait WorkloadLifecycleBridge: Send + Sync + 'static {
    /// Called when the node has started.
    async fn on_start(&self, ctx: Arc<ContextBridge>) -> ActrResult<()>;

    /// Called when the node is ready to accept requests.
    async fn on_ready(&self, ctx: Arc<ContextBridge>) -> ActrResult<()>;

    /// Called when the node receives a shutdown signal.
    async fn on_stop(&self, ctx: Arc<ContextBridge>) -> ActrResult<()>;

    /// Called when the framework catches a runtime error.
    async fn on_error(&self, ctx: Arc<ContextBridge>, event: ErrorEventBridge) -> ActrResult<()>;

    /// Dispatch an incoming RPC message and return the response bytes.
    async fn dispatch(
        &self,
        ctx: Arc<ContextBridge>,
        envelope: RpcEnvelopeBridge,
    ) -> ActrResult<Vec<u8>>;
}

/// Optional observer for signaling-layer events.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait SignalingObserverBridge: Send + Sync + 'static {
    async fn on_connecting(&self, ctx: Option<Arc<ContextBridge>>);
    async fn on_connected(&self, ctx: Option<Arc<ContextBridge>>);
    async fn on_disconnected(&self, ctx: Arc<ContextBridge>);
}

/// Optional observer for WebSocket peer events.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait WebSocketObserverBridge: Send + Sync + 'static {
    async fn on_connecting(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
    async fn on_connected(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
    async fn on_disconnected(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
}

/// Optional observer for WebRTC P2P peer events.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait WebRtcObserverBridge: Send + Sync + 'static {
    async fn on_connecting(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
    async fn on_connected(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
    async fn on_disconnected(&self, ctx: Arc<ContextBridge>, event: PeerEventBridge);
    /// Unified per-revision state hook. Fires on
    /// every snapshot revision; a late-registered workload receives the
    /// current state on the first projection pass.
    async fn on_state_changed(&self, ctx: Arc<ContextBridge>, state: WebRtcPeerStateBridge);
}

/// Optional observer for credential lifecycle events.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait CredentialObserverBridge: Send + Sync + 'static {
    async fn on_renewed(&self, ctx: Arc<ContextBridge>, event: CredentialEventBridge);
    async fn on_expiring(&self, ctx: Arc<ContextBridge>, event: CredentialEventBridge);
}

/// Optional observer for mailbox-backpressure events.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait MailboxObserverBridge: Send + Sync + 'static {
    async fn on_backpressure(&self, ctx: Arc<ContextBridge>, event: BackpressureEventBridge);
}

// ─────────────────────────────────────────────────────────────────────────────
// DynamicWorkload — multi-observer wrapper
// ─────────────────────────────────────────────────────────────────────────────

/// Dynamic workload composed of one mandatory [`WorkloadLifecycleBridge`]
/// and up to five optional category observers.
///
/// Categories left as `None` fall back to the framework's built-in tracing
/// defaults when the hook fires.
#[derive(uniffi::Object)]
pub struct DynamicWorkload {
    lifecycle: Arc<dyn WorkloadLifecycleBridge>,
    #[allow(dead_code)]
    signaling: Option<Arc<dyn SignalingObserverBridge>>,
    #[allow(dead_code)]
    websocket: Option<Arc<dyn WebSocketObserverBridge>>,
    #[allow(dead_code)]
    webrtc: Option<Arc<dyn WebRtcObserverBridge>>,
    #[allow(dead_code)]
    credential: Option<Arc<dyn CredentialObserverBridge>>,
    #[allow(dead_code)]
    mailbox: Option<Arc<dyn MailboxObserverBridge>>,
}

impl Clone for DynamicWorkload {
    fn clone(&self) -> Self {
        Self {
            lifecycle: self.lifecycle.clone(),
            signaling: self.signaling.clone(),
            websocket: self.websocket.clone(),
            webrtc: self.webrtc.clone(),
            credential: self.credential.clone(),
            mailbox: self.mailbox.clone(),
        }
    }
}

#[uniffi::export]
impl DynamicWorkload {
    /// Construct a `DynamicWorkload` from a mandatory lifecycle bridge and
    /// a variadic set of optional per-category observers.
    #[uniffi::constructor]
    pub fn new(
        lifecycle: Box<dyn WorkloadLifecycleBridge>,
        signaling: Option<Box<dyn SignalingObserverBridge>>,
        websocket: Option<Box<dyn WebSocketObserverBridge>>,
        webrtc: Option<Box<dyn WebRtcObserverBridge>>,
        credential: Option<Box<dyn CredentialObserverBridge>>,
        mailbox: Option<Box<dyn MailboxObserverBridge>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            lifecycle: Arc::from(lifecycle),
            signaling: signaling.map(Arc::from),
            websocket: websocket.map(Arc::from),
            webrtc: webrtc.map(Arc::from),
            credential: credential.map(Arc::from),
            mailbox: mailbox.map(Arc::from),
        })
    }
}

/// Runtime-only observers for package-backed hosts.
///
/// Unlike [`DynamicWorkload`], this object does not represent actor business
/// logic and does not need lifecycle or dispatch callbacks. Mobile shells use
/// it to observe transport readiness for UI state and intent retry decisions
/// while the package guest continues to own actor dispatch.
#[derive(uniffi::Object)]
pub struct RuntimeObservers {
    signaling: Option<Arc<dyn SignalingObserverBridge>>,
    websocket: Option<Arc<dyn WebSocketObserverBridge>>,
    webrtc: Option<Arc<dyn WebRtcObserverBridge>>,
    credential: Option<Arc<dyn CredentialObserverBridge>>,
    mailbox: Option<Arc<dyn MailboxObserverBridge>>,
}

impl Clone for RuntimeObservers {
    fn clone(&self) -> Self {
        Self {
            signaling: self.signaling.clone(),
            websocket: self.websocket.clone(),
            webrtc: self.webrtc.clone(),
            credential: self.credential.clone(),
            mailbox: self.mailbox.clone(),
        }
    }
}

#[uniffi::export]
impl RuntimeObservers {
    /// Construct host-side observers for a package-backed runtime.
    #[uniffi::constructor]
    pub fn new(
        signaling: Option<Box<dyn SignalingObserverBridge>>,
        websocket: Option<Box<dyn WebSocketObserverBridge>>,
        webrtc: Option<Box<dyn WebRtcObserverBridge>>,
        credential: Option<Box<dyn CredentialObserverBridge>>,
        mailbox: Option<Box<dyn MailboxObserverBridge>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            signaling: signaling.map(Arc::from),
            websocket: websocket.map(Arc::from),
            webrtc: webrtc.map(Arc::from),
            credential: credential.map(Arc::from),
            mailbox: mailbox.map(Arc::from),
        })
    }
}

#[async_trait]
impl Workload for RuntimeObservers {
    type Dispatcher = RuntimeObserverDispatcher;

    // ── Signaling observers ──────────────────────────────────────────────

    async fn on_signaling_connecting<C: Context>(&self, ctx: Option<&C>) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let ctx_bridge = ctx.and_then(|c| ContextBridge::try_from_context(c).ok());
        obs.on_connecting(ctx_bridge).await;
    }

    async fn on_signaling_connected<C: Context>(&self, ctx: Option<&C>) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let ctx_bridge = ctx.and_then(|c| ContextBridge::try_from_context(c).ok());
        obs.on_connected(ctx_bridge).await;
    }

    async fn on_signaling_disconnected<C: Context>(&self, ctx: &C) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge).await;
    }

    // ── WebSocket observers ──────────────────────────────────────────────

    async fn on_websocket_connecting<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connecting(ctx_bridge, event.into()).await;
    }

    async fn on_websocket_connected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connected(ctx_bridge, event.into()).await;
    }

    async fn on_websocket_disconnected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge, event.into()).await;
    }

    // ── WebRTC observers ─────────────────────────────────────────────────

    async fn on_webrtc_connecting<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connecting(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_connected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connected(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_disconnected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_state_changed<C: Context>(&self, ctx: &C, event: &WebRtcPeerState) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_state_changed(ctx_bridge, event.clone().into()).await;
    }

    // ── Credential observers ─────────────────────────────────────────────

    async fn on_credential_renewed<C: Context>(&self, ctx: &C, event: &CredentialEvent) {
        let Some(obs) = self.credential.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_renewed(ctx_bridge, event.into()).await;
    }

    async fn on_credential_expiring<C: Context>(&self, ctx: &C, event: &CredentialEvent) {
        let Some(obs) = self.credential.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_expiring(ctx_bridge, event.into()).await;
    }

    // ── Mailbox observers ────────────────────────────────────────────────

    async fn on_mailbox_backpressure<C: Context>(&self, ctx: &C, event: &BackpressureEvent) {
        let Some(obs) = self.mailbox.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_backpressure(ctx_bridge, event.into()).await;
    }
}

#[async_trait]
impl Workload for DynamicWorkload {
    type Dispatcher = DynamicDispatcher;

    async fn on_start<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        let ctx_bridge =
            ContextBridge::try_from_context(ctx).map_err(actr_protocol::ActrError::from)?;
        self.lifecycle
            .on_start(ctx_bridge)
            .await
            .map_err(actr_protocol::ActrError::from)
    }

    async fn on_ready<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        let ctx_bridge =
            ContextBridge::try_from_context(ctx).map_err(actr_protocol::ActrError::from)?;
        self.lifecycle
            .on_ready(ctx_bridge)
            .await
            .map_err(actr_protocol::ActrError::from)
    }

    async fn on_stop<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        let ctx_bridge =
            ContextBridge::try_from_context(ctx).map_err(actr_protocol::ActrError::from)?;
        self.lifecycle
            .on_stop(ctx_bridge)
            .await
            .map_err(actr_protocol::ActrError::from)
    }

    async fn on_error<C: Context>(&self, ctx: &C, event: &ErrorEvent) -> ActorResult<()> {
        let ctx_bridge =
            ContextBridge::try_from_context(ctx).map_err(actr_protocol::ActrError::from)?;
        let ev_bridge: ErrorEventBridge = event.into();
        self.lifecycle
            .on_error(ctx_bridge, ev_bridge)
            .await
            .map_err(actr_protocol::ActrError::from)
    }

    // ── Signaling observers ──────────────────────────────────────────────

    async fn on_signaling_connecting<C: Context>(&self, ctx: Option<&C>) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let ctx_bridge = ctx.and_then(|c| ContextBridge::try_from_context(c).ok());
        obs.on_connecting(ctx_bridge).await;
    }

    async fn on_signaling_connected<C: Context>(&self, ctx: Option<&C>) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let ctx_bridge = ctx.and_then(|c| ContextBridge::try_from_context(c).ok());
        obs.on_connected(ctx_bridge).await;
    }

    async fn on_signaling_disconnected<C: Context>(&self, ctx: &C) {
        let Some(obs) = self.signaling.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge).await;
    }

    // ── WebSocket observers ──────────────────────────────────────────────

    async fn on_websocket_connecting<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connecting(ctx_bridge, event.into()).await;
    }

    async fn on_websocket_connected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connected(ctx_bridge, event.into()).await;
    }

    async fn on_websocket_disconnected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.websocket.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge, event.into()).await;
    }

    // ── WebRTC observers ─────────────────────────────────────────────────

    async fn on_webrtc_connecting<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connecting(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_connected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_connected(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_disconnected<C: Context>(&self, ctx: &C, event: &PeerEvent) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_disconnected(ctx_bridge, event.into()).await;
    }

    async fn on_webrtc_state_changed<C: Context>(&self, ctx: &C, event: &WebRtcPeerState) {
        let Some(obs) = self.webrtc.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_state_changed(ctx_bridge, event.clone().into()).await;
    }

    // ── Credential observers ─────────────────────────────────────────────

    async fn on_credential_renewed<C: Context>(&self, ctx: &C, event: &CredentialEvent) {
        let Some(obs) = self.credential.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_renewed(ctx_bridge, event.into()).await;
    }

    async fn on_credential_expiring<C: Context>(&self, ctx: &C, event: &CredentialEvent) {
        let Some(obs) = self.credential.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_expiring(ctx_bridge, event.into()).await;
    }

    // ── Mailbox observers ────────────────────────────────────────────────

    async fn on_mailbox_backpressure<C: Context>(&self, ctx: &C, event: &BackpressureEvent) {
        let Some(obs) = self.mailbox.clone() else {
            return;
        };
        let Ok(ctx_bridge) = ContextBridge::try_from_context(ctx) else {
            return;
        };
        obs.on_backpressure(ctx_bridge, event.into()).await;
    }
}

/// Dynamic dispatcher that routes messages to the callback interface.
///
/// All message-handling logic must live in the user's implementation of
/// [`WorkloadLifecycleBridge::dispatch`] on the foreign-language side.
pub struct DynamicDispatcher;

#[async_trait]
impl MessageDispatcher for DynamicDispatcher {
    type Workload = DynamicWorkload;

    /// Foreign linked workloads use the same explicit delivery boundary as
    /// generated WASM workloads. Generated RETURN routes call `reply_raw`
    /// after obtaining their response; the unclaimed fallback below keeps
    /// older generated dispatchers usable while they are regenerated.
    fn reply_mode_for_route(_route_key: &str) -> ReplyMode {
        ReplyMode::Explicit
    }

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        let ctx_bridge =
            ContextBridge::try_from_context(ctx).map_err(actr_protocol::ActrError::from)?;
        let envelope_bridge: RpcEnvelopeBridge = envelope.into();
        let response = workload
            .lifecycle
            .dispatch(Arc::clone(&ctx_bridge), envelope_bridge)
            .await
            .map_err(actr_protocol::ActrError::from)?;
        if let Some(deliver) = ctx.reply_deliver() {
            // New generated RETURN routes deliver through ContextBridge so
            // the bridge records the reply as claimed. Generated EXPLICIT
            // routes claim it before invoking user code, allowing them to
            // defer an empty response. If neither happened, this is an older
            // generated dispatcher returning its response through the legacy
            // path; deliver it even when protobuf encoded it as zero bytes.
            if !ctx_bridge.reply_was_claimed() {
                deliver.deliver(ReplyOutcome {
                    request_id: ctx.request_id().to_string(),
                    result: Ok(Bytes::from(response)),
                    emit_response: ctx.expects_response(),
                });
            }
            return Ok(Bytes::new());
        }
        Ok(Bytes::from(response))
    }
}

/// Dispatcher placeholder for [`RuntimeObservers`].
///
/// Runtime-only observers are never used for inbound actor dispatch; the
/// package guest remains the dispatcher. Returning `NotImplemented` keeps the
/// type compliant with `actr_framework::Workload` if this path is accidentally
/// invoked.
pub struct RuntimeObserverDispatcher;

#[async_trait]
impl MessageDispatcher for RuntimeObserverDispatcher {
    type Workload = RuntimeObservers;

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        _envelope: RpcEnvelope,
        _ctx: &C,
    ) -> ActorResult<Bytes> {
        Err(ActrError::NotImplemented(
            "runtime observers do not dispatch actor messages".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_framework::{ErrorCategory, ErrorEvent, Workload};
    use actr_mock_loran::MockLoranServer;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use tokio::sync::Notify;
    use tokio::time::{Duration, timeout};

    #[derive(Clone)]
    struct LifecycleRecorder {
        on_start: Arc<AtomicUsize>,
        on_ready: Arc<AtomicUsize>,
        on_stop: Arc<AtomicUsize>,
        on_error: Arc<AtomicUsize>,
        dispatches: Arc<Mutex<Vec<RpcEnvelopeBridge>>>,
        dispatch_response: Arc<Mutex<Result<Vec<u8>, String>>>,
        notify: Arc<Notify>,
    }

    impl Default for LifecycleRecorder {
        fn default() -> Self {
            Self {
                on_start: Arc::new(AtomicUsize::new(0)),
                on_ready: Arc::new(AtomicUsize::new(0)),
                on_stop: Arc::new(AtomicUsize::new(0)),
                on_error: Arc::new(AtomicUsize::new(0)),
                dispatches: Arc::new(Mutex::new(Vec::new())),
                dispatch_response: Arc::new(Mutex::new(Ok(Vec::new()))),
                notify: Arc::new(Notify::new()),
            }
        }
    }

    impl LifecycleRecorder {
        fn with_response(response: impl Into<Vec<u8>>) -> Self {
            Self {
                dispatch_response: Arc::new(Mutex::new(Ok(response.into()))),
                ..Self::default()
            }
        }

        fn with_unknown_route(route: impl Into<String>) -> Self {
            Self {
                dispatch_response: Arc::new(Mutex::new(Err(route.into()))),
                ..Self::default()
            }
        }
    }

    #[async_trait::async_trait]
    impl WorkloadLifecycleBridge for LifecycleRecorder {
        async fn on_start(&self, _ctx: Arc<ContextBridge>) -> ActrResult<()> {
            self.on_start.fetch_add(1, Ordering::Relaxed);
            self.notify.notify_waiters();
            Ok(())
        }

        async fn on_ready(&self, _ctx: Arc<ContextBridge>) -> ActrResult<()> {
            self.on_ready.fetch_add(1, Ordering::Relaxed);
            self.notify.notify_waiters();
            Ok(())
        }

        async fn on_stop(&self, _ctx: Arc<ContextBridge>) -> ActrResult<()> {
            self.on_stop.fetch_add(1, Ordering::Relaxed);
            self.notify.notify_waiters();
            Ok(())
        }

        async fn on_error(
            &self,
            _ctx: Arc<ContextBridge>,
            _event: ErrorEventBridge,
        ) -> ActrResult<()> {
            self.on_error.fetch_add(1, Ordering::Relaxed);
            self.notify.notify_waiters();
            Ok(())
        }

        async fn dispatch(
            &self,
            _ctx: Arc<ContextBridge>,
            envelope: RpcEnvelopeBridge,
        ) -> ActrResult<Vec<u8>> {
            self.dispatches.lock().push(envelope);
            self.notify.notify_waiters();
            self.dispatch_response
                .lock()
                .clone()
                .map_err(|msg| crate::ActrError::UnknownRoute { msg })
        }
    }

    #[tokio::test]
    async fn linked_workload_forwards_lifecycle_callbacks() {
        let mut server = MockLoranServer::start()
            .await
            .expect("mock loran server should start");
        let temp = tempdir().expect("temp dir");
        let config_path = temp.path().join("actr.toml");
        let data_dir = temp.path().display().to_string().replace('\\', "/");
        std::fs::write(
            &config_path,
            format!(
                "edition = 1\n\
                 [signaling]\n\
                 url = \"{}\"\n\
                 [issuer_endpoint]\n\
                 url = \"{}/issuer\"\n\
                 [deployment]\n\
                 realm = \"test@example.com\"\n\
                 [hyper]\n\
                 data_dir = \"{}\"\n\
                 [hyper.trust]\n\
                 kind = \"dev_only\"\n",
                server.ws_url(),
                server.http_url(),
                data_dir,
            ),
        )
        .expect("write actr.toml");

        let lifecycle = LifecycleRecorder::default();
        let workload =
            DynamicWorkload::new(Box::new(lifecycle.clone()), None, None, None, None, None);
        let node = crate::runtime::ActrNode::new_from_linked_workload(
            config_path.display().to_string(),
            crate::types::ActrType {
                publisher: "acme".to_string(),
                name: "LifecycleProbe".to_string(),
                version: "0.1.0".to_string(),
            },
            workload.clone(),
            None,
        )
        .await
        .expect("linked workload node should be created");

        let actr_ref = node
            .start()
            .await
            .expect("linked workload node should start");

        wait_for_count(&lifecycle.notify, &lifecycle.on_start, 1, "on_start").await;

        let ctx = actr_ref.app_context_for_test().await;
        <DynamicWorkload as Workload>::on_ready(workload.as_ref(), &ctx)
            .await
            .expect("on_ready forwarding should succeed");
        wait_for_count(&lifecycle.notify, &lifecycle.on_ready, 1, "on_ready").await;

        let error = ErrorEvent::now(
            actr_protocol::ActrError::Internal("boom".to_string()),
            ErrorCategory::HandlerError,
            "test error",
        );
        <DynamicWorkload as Workload>::on_error(workload.as_ref(), &ctx, &error)
            .await
            .expect("on_error forwarding should succeed");
        wait_for_count(&lifecycle.notify, &lifecycle.on_error, 1, "on_error").await;

        actr_ref.shutdown();
        actr_ref.wait_for_shutdown().await;
        wait_for_count(&lifecycle.notify, &lifecycle.on_stop, 1, "on_stop").await;

        server.shutdown().await;
    }

    #[tokio::test]
    async fn linked_workload_dispatches_local_call_to_foreign_lifecycle_bridge() {
        let mut server = MockLoranServer::start()
            .await
            .expect("mock loran server should start");
        let temp = tempdir().expect("temp dir");
        let config_path = write_test_config(temp.path(), &server);

        let lifecycle = LifecycleRecorder::with_response(b"swift-local:hello".to_vec());
        let node = linked_workload_node(&config_path, lifecycle.clone())
            .await
            .expect("linked workload node should be created");
        let actr_ref = node
            .start()
            .await
            .expect("linked workload node should start");
        wait_for_count(&lifecycle.notify, &lifecycle.on_start, 1, "on_start").await;

        let response = actr_ref
            .call(
                "echoapp.LocalEchoService.Send".to_string(),
                crate::types::PayloadType::RpcReliable,
                b"hello".to_vec(),
                30_000,
            )
            .await
            .expect("local call should dispatch through the foreign bridge");

        assert_eq!(response, b"swift-local:hello");

        // Stale generated dispatchers return their protobuf bytes instead of
        // claiming the ContextBridge reply boundary. An Empty/default message
        // legitimately encodes to zero bytes and must still be delivered.
        *lifecycle.dispatch_response.lock() = Ok(Vec::new());
        let empty_response = actr_ref
            .call(
                "echoapp.LocalEchoService.Send".to_string(),
                crate::types::PayloadType::RpcReliable,
                b"empty".to_vec(),
                5_000,
            )
            .await
            .expect("zero-byte foreign response should use the migration fallback");
        assert!(empty_response.is_empty());

        {
            let dispatches = lifecycle.dispatches.lock();
            assert_eq!(dispatches.len(), 2);
            assert_eq!(dispatches[0].route_key, "echoapp.LocalEchoService.Send");
            assert_eq!(dispatches[0].payload, b"hello");
            assert!(!dispatches[0].request_id.is_empty());
            // Dispatch inherits the remaining deadline budget, not a fresh timeout.
            assert!((1..=30_000).contains(&dispatches[0].timeout_ms));
            assert_eq!(dispatches[1].payload, b"empty");
            assert!((1..=5_000).contains(&dispatches[1].timeout_ms));
        }

        actr_ref.shutdown();
        actr_ref.wait_for_shutdown().await;
        wait_for_count(&lifecycle.notify, &lifecycle.on_stop, 1, "on_stop").await;

        server.shutdown().await;
    }

    #[tokio::test]
    async fn linked_workload_propagates_foreign_dispatch_errors() {
        let mut server = MockLoranServer::start()
            .await
            .expect("mock loran server should start");
        let temp = tempdir().expect("temp dir");
        let config_path = write_test_config(temp.path(), &server);

        let lifecycle = LifecycleRecorder::with_unknown_route("echoapp.UnknownService.Send");
        let node = linked_workload_node(&config_path, lifecycle.clone())
            .await
            .expect("linked workload node should be created");
        let actr_ref = node
            .start()
            .await
            .expect("linked workload node should start");
        wait_for_count(&lifecycle.notify, &lifecycle.on_start, 1, "on_start").await;

        let err = actr_ref
            .call(
                "echoapp.UnknownService.Send".to_string(),
                crate::types::PayloadType::RpcReliable,
                b"hello".to_vec(),
                30_000,
            )
            .await
            .expect_err("foreign dispatch errors should cross the FFI runtime boundary");

        assert!(
            matches!(err, crate::ActrError::UnknownRoute { ref msg } if msg.contains("echoapp.UnknownService.Send")),
            "unexpected dispatch error: {err:?}"
        );
        {
            let dispatches = lifecycle.dispatches.lock();
            assert_eq!(dispatches.len(), 1);
            assert_eq!(dispatches[0].route_key, "echoapp.UnknownService.Send");
        }

        actr_ref.shutdown();
        actr_ref.wait_for_shutdown().await;
        wait_for_count(&lifecycle.notify, &lifecycle.on_stop, 1, "on_stop").await;

        server.shutdown().await;
    }

    async fn wait_for_count(notify: &Notify, counter: &AtomicUsize, expected: usize, label: &str) {
        timeout(Duration::from_secs(5), async {
            loop {
                if counter.load(Ordering::Relaxed) >= expected {
                    break;
                }
                notify.notified().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{label} callback was not observed"));
    }

    fn write_test_config(dir: &std::path::Path, server: &MockLoranServer) -> std::path::PathBuf {
        let config_path = dir.join("actr.toml");
        let data_dir = dir.display().to_string().replace('\\', "/");
        std::fs::write(
            &config_path,
            format!(
                "edition = 1\n\
                 [signaling]\n\
                 url = \"{}\"\n\
                 [issuer_endpoint]\n\
                 url = \"{}/issuer\"\n\
                 [deployment]\n\
                 realm = \"test@example.com\"\n\
                 [hyper]\n\
                 data_dir = \"{}\"\n\
                 [hyper.trust]\n\
                 kind = \"dev_only\"\n",
                server.ws_url(),
                server.http_url(),
                data_dir,
            ),
        )
        .expect("write actr.toml");
        config_path
    }

    async fn linked_workload_node(
        config_path: &std::path::Path,
        lifecycle: LifecycleRecorder,
    ) -> ActrResult<Arc<crate::runtime::ActrNode>> {
        let workload = DynamicWorkload::new(Box::new(lifecycle), None, None, None, None, None);
        crate::runtime::ActrNode::new_from_linked_workload(
            config_path.display().to_string(),
            crate::types::ActrType {
                publisher: "acme".to_string(),
                name: "LifecycleProbe".to_string(),
                version: "0.1.0".to_string(),
            },
            workload,
            None,
        )
        .await
    }
}
