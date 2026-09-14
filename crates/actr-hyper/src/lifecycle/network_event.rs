//! Network Event Handling Architecture
//!
//! This module defines the network event handling infrastructure.
//!
//! # Architecture Overview
//!
//! ```text
//!        ┌─────────────────────────────────────────────┐
//!        │ (FFI Path - Implemented)  (Actor Path - TODO)
//!        ▼                                             ▼
//! ┌──────────────────────────┐      ┌──────────────────────────┐
//! │ NetworkEventHandle       │      │ Direct Proto Message     │
//! │ • Platform FFI calls     │      │ • Actor call/tell        │
//! │ • Send via channel       │      │ • Send to actor mailbox  │
//! │ • Await result           │      │ • No handle needed       │
//! └────────┬─────────────────┘      └──────┬───────────────────┘
//!          │                               │
//!          └───────────────┬───────────────┘
//!                          │ Both trigger
//!                          ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │  ActrNode::network_event_loop()                         │
//! │  • Receive event from channel (FFI path)                │
//! │  • Or handle message directly (Actor path - TODO)       │
//! │  • Delegate to NetworkEventProcessor                    │
//! │  • Send result back via channel                         │
//! └──────────────────────┬──────────────────────────────────┘
//!                        │ Delegate
//!                        ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │  NetworkEventProcessor (Trait)                          │
//! │                                                          │
//! │  DefaultNetworkEventProcessor:                          │
//! │  • feed normalized network/app inputs into policy FSM   │
//! │  • execute one recovery action                          │
//! │    └─► Offline / Probe / Restore / Cleanup / Reconnect  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Components
//!
//! - **NetworkEvent**: Unified mobile network/app/command events
//! - **NetworkEventResult**: Processing result with success/error/duration
//! - **NetworkEventProcessor**: Trait for custom event handling logic
//! - **DefaultNetworkEventProcessor**: Default implementation with signaling + WebRTC recovery
//!
//! # Usage Patterns
//!
//! ## 1. Platform FFI Call (Primary, Implemented)
//! ```ignore
//! // Platform layer calls NetworkEventHandle via FFI
//! let network_handle = system.create_network_event_handle();
//! let result = network_handle.handle_network_path_changed(snapshot).await?;
//! if result.success {
//!     println!("Processed in {}ms", result.duration_ms);
//! }
//! ```
//!
//! ## 2. Actor Proto Message (Optional, TODO)
//! ```ignore
//! // TODO: actors send proto message directly (not yet implemented)
//! actor_ref.call(NetworkPathChangedMessage { snapshot }, 30_000).await?;
//! ```
//!
//! **Key Differences:**
//! - FFI path: Uses NetworkEventHandle + channel (implemented)
//! - Actor path: Direct proto message to mailbox (TODO, future enhancement)

use actr_protocol::Classify as _;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
// Recovery bookkeeping (debounce windows, last successful connect, reconciler
// latency) reads the same clock the supervisor arms its timers on, so a paused
// clock advances both sides of every recovery decision together.
use tokio::time::Instant;

use crate::transport::{NetworkError, PeerTransport};
use crate::wire::webrtc::SignalingClient;
use actr_framework::WebRtcPeerStatus;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::recovery_policy::diagnosis::{AbortCause, EffectDiagnosis, EffectKind, EffectOutcome};
use super::recovery_policy::translate as tp;
use super::recovery_supervisor::{
    RecoverySupervisor, StartedEffect, TimerOp, event_to_input, network_action_of,
};
use super::session_state::{SessionPhase, SessionState};

const NETWORK_EVENT_RESULT_TIMEOUT: Duration = Duration::from_secs(5);
// Mobile network handovers can delay the signaling Pong briefly. Keep the
// probe bounded, but give the existing WebSocket time to survive the switch
// before rebuilding it and dropping in-flight signaling requests.
const SIGNALING_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const LONG_BACKGROUND_RECONNECT_THRESHOLD_MS: u64 = 30_000;
/// Upper bound accepted for `background_duration_ms` (30 days, in ms).
///
/// Bindings derive the duration from wall-clock timestamps, so a clock jump
/// while the app is suspended can report absurd values. Anything above this
/// bound is clamped (with a warning) before it reaches recovery decisions.
/// Kept as a defensive limit even once bindings move to a suspend-aware
/// monotonic source.
pub(super) const MAX_BACKGROUND_DURATION_MS: u64 = 2_592_000_000;
static NEXT_NETWORK_EVENT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_NETWORK_RECOVERY_ACTION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_NETWORK_SOURCE_EPOCH: AtomicU64 = AtomicU64::new(1);

/// Mobile network path snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetworkSnapshot {
    pub sequence: u64,
    pub availability: NetworkAvailability,
    pub transport: NetworkTransportFlags,
    pub is_expensive: bool,
    pub is_constrained: bool,
}

impl NetworkSnapshot {
    pub fn is_offline(&self) -> bool {
        matches!(self.availability, NetworkAvailability::Unavailable)
    }

    pub fn should_restore(&self) -> bool {
        matches!(self.availability, NetworkAvailability::Available)
    }
}

/// Whether the platform currently has a usable network path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkAvailability {
    Unknown,
    Available,
    Unavailable,
}

/// Active network transport flags. Multiple flags can be true at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NetworkTransportFlags {
    pub wifi: bool,
    pub cellular: bool,
    pub ethernet: bool,
    pub vpn: bool,
    pub other: bool,
}

/// App lifecycle state relevant to connection recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppLifecycleState {
    Background,
    Foreground { background_duration_ms: u64 },
}

impl AppLifecycleState {
    /// Clamp `background_duration_ms` to [`MAX_BACKGROUND_DURATION_MS`] with a
    /// warning. Values within the bound are returned unchanged.
    fn sanitized(self) -> Self {
        match self {
            Self::Foreground {
                background_duration_ms,
            } if background_duration_ms > MAX_BACKGROUND_DURATION_MS => {
                tracing::warn!(
                    background_duration_ms,
                    clamped_ms = MAX_BACKGROUND_DURATION_MS,
                    "background duration exceeds sanity bound; clamping (likely a wall-clock jump in the binding layer)"
                );
                Self::Foreground {
                    background_duration_ms: MAX_BACKGROUND_DURATION_MS,
                }
            }
            other => other,
        }
    }
}

/// Reason for a cleanup-only operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanupReason {
    AppTerminating,
    UserLogout,
    StaleConnectionSuspected,
    ManualReset,
}

/// Reason for a forced cleanup + restore operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReconnectReason {
    NetworkPathChanged,
    LongBackground,
    ProbeFailed,
    ManualReconnect,
    StaleConnectionSuspected,
}

/// Network event type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NetworkEvent {
    /// Full mobile network path changed.
    NetworkPathChanged { snapshot: NetworkSnapshot },

    /// App lifecycle changed.
    AppLifecycleChanged { state: AppLifecycleState },

    /// Proactively clean up all connections
    ///
    /// Used when the caller explicitly wants teardown without reconnection,
    /// such as user logout, app termination, or a manual reset. Merely entering
    /// the background is represented by `AppLifecycleChanged::Background` and
    /// does not tear down a healthy connection.
    CleanupConnections { reason: CleanupReason },

    /// Proactively clean up and restore connections.
    ForceReconnect { reason: ReconnectReason },
}

/// Final action selected from one network-event decision cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRecoveryAction {
    Noop,
    Offline,
    Probe,
    Restore,
    CleanupOnly,
    ForceReconnect,
}

/// Typed failure reported by a network recovery effect.
///
/// This is deliberately smaller than [`NetworkError`]: an effect only reports
/// observations that the recovery policy can classify without inspecting an
/// opaque error string. In particular, generic transport failures remain
/// availability-family `PathUnreachable` unless the effect has independently
/// verified a stronger fact.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NetworkRecoveryError {
    #[error("path unreachable: {stage}")]
    PathUnreachable { stage: String },
    #[error("operation timed out: {stage}")]
    Timeout { stage: String },
    #[error("resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
    #[error("remote overloaded; retry no earlier than {retry_after:?}")]
    Overloaded { retry_after: Duration },
    #[error("transport impaired: {}", scopes.join(", "))]
    TransportImpaired { scopes: Vec<String> },
    #[error("authentication rejected: {kind}")]
    AuthRejected { kind: String },
    #[error("configuration rejected: {detail}")]
    ConfigRejected { detail: String },
    #[error("recovery invariant violated: {detail}")]
    InvariantViolation { detail: String },
}

impl NetworkRecoveryError {
    fn from_opaque(stage: &str, detail: impl std::fmt::Display) -> Self {
        Self::PathUnreachable {
            stage: format!("{stage}: {detail}"),
        }
    }

    fn from_network_error(stage: &str, error: NetworkError) -> Self {
        let detail = format!("{stage}: {error}");
        if error.auth_verdict().is_some() {
            return Self::AuthRejected { kind: detail };
        }
        if let Some(retry_after) = error.retry_after() {
            return Self::Overloaded { retry_after };
        }
        if matches!(&error, NetworkError::Other(_)) && error.is_retryable() {
            return Self::PathUnreachable { stage: detail };
        }
        match error {
            NetworkError::TimeoutError(_) => Self::Timeout { stage: detail },
            NetworkError::ResourceExhaustedError(_) => Self::ResourceExhausted { resource: detail },
            NetworkError::AuthenticationError(_)
            | NetworkError::CredentialExpired(_)
            | NetworkError::PermissionError(_) => Self::AuthRejected { kind: detail },
            NetworkError::ProtocolError(_)
            | NetworkError::ConfigurationError(_)
            | NetworkError::ServiceDiscoveryError(_)
            | NetworkError::NotImplemented(_)
            | NetworkError::NoRoute(_)
            | NetworkError::InvalidOperation(_)
            | NetworkError::InvalidArgument(_)
            | NetworkError::UrlParseError(_) => Self::ConfigRejected { detail },
            NetworkError::SerializationError(_)
            | NetworkError::DeserializationError(_)
            | NetworkError::BroadcastError(_)
            | NetworkError::JsonError(_)
            | NetworkError::Other(_) => Self::InvariantViolation { detail },
            NetworkError::ConnectionError(_)
            | NetworkError::SignalingReplyDropped
            | NetworkError::SignalingError(_)
            | NetworkError::WebRtcError(_)
            | NetworkError::NetworkUnreachableError(_)
            | NetworkError::NatTraversalError(_)
            | NetworkError::DataChannelError(_)
            | NetworkError::DataChannelClosed(_)
            | NetworkError::DataChannelNotOpen(_)
            | NetworkError::IceError(_)
            | NetworkError::DtlsError(_)
            | NetworkError::StunTurnError(_)
            | NetworkError::WebSocketError(_)
            | NetworkError::WebSocketClosed(_)
            | NetworkError::ConnectionNotFound(_)
            | NetworkError::ConnectionClosed(_)
            | NetworkError::PeerConnectionClosed(_)
            | NetworkError::SecureChannelNotReady(_)
            | NetworkError::ChannelClosed(_)
            | NetworkError::SendError(_)
            | NetworkError::ChannelNotFound(_)
            | NetworkError::IoError(_) => Self::PathUnreachable { stage: detail },
        }
    }

    fn into_diagnosis(self) -> EffectDiagnosis {
        match self {
            Self::PathUnreachable { stage } => EffectDiagnosis::PathUnreachable { stage },
            Self::Timeout { stage } => EffectDiagnosis::Timeout { stage },
            Self::ResourceExhausted { resource } => EffectDiagnosis::ResourceExhausted { resource },
            Self::Overloaded { retry_after } => EffectDiagnosis::Overloaded { retry_after },
            Self::TransportImpaired { scopes } => EffectDiagnosis::TransportImpaired { scopes },
            Self::AuthRejected { kind } => EffectDiagnosis::AuthRejected { kind },
            Self::ConfigRejected { detail } => EffectDiagnosis::ConfigRejected { detail },
            Self::InvariantViolation { detail } => EffectDiagnosis::InvariantViolation { detail },
        }
    }
}

/// The structured result of a bounded teardown effect (cleanup or confirmed
/// offline disconnect), per the RFC-0400 bounded-completion contract.
///
/// A teardown never reports a plain success/failure boolean: it reports whether
/// the logical teardown goal was reached, whether its overall deadline aborted
/// remaining steps, and the residual diagnostics accumulated from best-effort
/// physical steps. Remote-notification failures are recorded as residuals and
/// never block local completion.
#[derive(Debug, Clone)]
pub struct TeardownReport {
    /// Whether the logical teardown goal (local resources released) was reached.
    pub reached_goal: bool,
    /// Whether the overall teardown deadline aborted or detached remaining steps.
    pub deadline_reached: bool,
    /// One residual per best-effort step that left an error behind.
    pub residuals: Vec<String>,
}

impl TeardownReport {
    /// The teardown met its full contract with no residuals.
    pub fn succeeded() -> Self {
        Self {
            reached_goal: true,
            deadline_reached: false,
            residuals: Vec::new(),
        }
    }

    /// The teardown reached its goal but left the given residual diagnostics.
    pub fn completed_with_residuals(residuals: Vec<String>) -> Self {
        Self {
            reached_goal: true,
            deadline_reached: false,
            residuals,
        }
    }

    /// The overall deadline aborted remaining steps before the goal was reached.
    pub fn abandoned(residuals: Vec<String>) -> Self {
        Self {
            reached_goal: false,
            deadline_reached: true,
            residuals,
        }
    }
}

/// Accept facts only from the newest attached platform-monitor incarnation.
///
/// Network snapshots also carry an incarnation-local sequence that the policy
/// reducer validates. Lifecycle facts have no such sequence, so their source
/// epoch must be fenced before translation; otherwise a delayed callback from
/// a retired monitor can overwrite the authoritative phase reported by its
/// replacement. Explicit cleanup/reconnect commands are user intent rather
/// than monitor observations and remain valid from any live handle.
fn accept_monitor_source_epoch(
    current_epoch: &mut Option<u64>,
    event: &NetworkEvent,
    source_epoch: u64,
) -> bool {
    if !matches!(
        event,
        NetworkEvent::NetworkPathChanged { .. } | NetworkEvent::AppLifecycleChanged { .. }
    ) {
        return true;
    }

    match *current_epoch {
        Some(current) if source_epoch < current => false,
        Some(current) if source_epoch == current => true,
        _ => {
            *current_epoch = Some(source_epoch);
            true
        }
    }
}

/// Network event processing result
#[derive(Debug, Clone)]
pub struct NetworkEventResult {
    /// Event type
    pub event: NetworkEvent,

    /// Whether processing succeeded
    pub success: bool,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Processing duration (milliseconds).
    ///
    /// Measured on the Tokio clock, the same one the recovery timers are armed
    /// on. Under a paused clock this reports *virtual* elapsed time — normally
    /// 0, or exactly what the test advanced — not wall-clock latency. Treat it
    /// as an observation of the supervisor's own time line, not as a
    /// wall-clock performance metric.
    pub duration_ms: u64,
}

impl NetworkEventResult {
    pub fn success(event: NetworkEvent, duration_ms: u64) -> Self {
        Self {
            event,
            success: true,
            error: None,
            duration_ms,
        }
    }

    pub fn failure(event: NetworkEvent, error: String, duration_ms: u64) -> Self {
        Self {
            event,
            success: false,
            error: Some(error),
            duration_ms,
        }
    }
}

/// Network event processor trait
///
/// Defines the processing logic for network events; can be custom-implemented by users
#[async_trait::async_trait]
pub trait NetworkEventProcessor: Send + Sync {
    /// Perform synchronous, non-blocking preparation as soon as an event is
    /// dequeued, before the reconciler optionally waits through offline grace.
    fn prepare_network_event(&self, _event: &NetworkEvent) {}

    /// Invalidate commit rights for every in-flight signaling connection
    /// attempt. The supervisor shell calls this synchronously before cancelling
    /// a superseded recovery effect.
    fn invalidate_signaling_connection_attempts(&self) {}

    /// Process network available event
    ///
    /// # Returns
    /// - `Ok(())`: processing succeeded
    /// - `Err(String)`: processing failed, contains error message
    async fn process_network_available(&self) -> Result<(), String>;

    /// Process network lost event
    ///
    /// # Returns
    /// - `Ok(())`: processing succeeded
    /// - `Err(String)`: processing failed, contains error message
    async fn process_network_lost(&self) -> Result<(), String>;

    /// Process network type changed event
    ///
    /// # Returns
    /// - `Ok(())`: processing succeeded
    /// - `Err(String)`: processing failed, contains error message
    async fn process_network_type_changed(
        &self,
        is_wifi: bool,
        is_cellular: bool,
    ) -> Result<(), String>;

    /// Proactively clean up all connections
    ///
    /// This method proactively cleans up all network connections. Applicable scenarios:
    /// - User actively logging out
    /// - App about to exit
    /// - Need to reset network state
    ///
    /// # FFI Binding Note
    ///
    /// This method is specifically designed for FFI bindings, allowing upper-layer
    /// platform code (Swift/Kotlin) to proactively manage connection lifecycle
    /// through the unified `NetworkEventProcessor` interface.
    ///
    /// # Difference from Event Response
    ///
    /// - `process_network_lost()`: passively responds to network disconnection events
    /// - `cleanup_connections()`: proactively cleans up connections (independent of network events)
    ///
    /// # Returns
    /// - `Ok(())`: cleanup succeeded
    /// - `Err(String)`: cleanup failed, contains error message
    async fn cleanup_connections(&self) -> Result<(), String>;

    /// Recovery-aware cleanup for automatic rebuilds.
    ///
    /// [`NetworkRecoveryAction::ForceReconnect`] tears the stack down only to
    /// rebuild it immediately. The runtime implementation routes the signaling
    /// teardown through `disconnect_for_recovery()`, which arms the
    /// idempotent-retry mark for the replacement generation: RouteCandidates
    /// and GetSigningKey waiters dropped by the rebuild replay once after the
    /// replacement commits instead of failing fast. Manual teardown keeps
    /// [`Self::cleanup_connections`] and its fail-fast contract.
    async fn cleanup_connections_for_recovery(&self) -> Result<(), String> {
        self.cleanup_connections().await
    }

    /// Probe existing connectivity without forcing cleanup.
    async fn probe_connectivity(&self) -> Result<(), String> {
        Ok(())
    }

    /// Proactively clean up and restore connections.
    async fn force_reconnect(&self) -> Result<(), String> {
        self.cleanup_connections_for_recovery().await?;
        self.process_network_available().await
    }

    /// Process the final action selected by the connection policy FSM.
    ///
    /// Custom processors can rely on the default mapping. The default runtime
    /// processor overrides this to coordinate signaling and WebRTC recovery.
    async fn process_network_recovery_action(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        match action {
            NetworkRecoveryAction::Noop => Ok(()),
            NetworkRecoveryAction::Offline => self.process_network_lost().await,
            NetworkRecoveryAction::Probe => self.probe_connectivity().await,
            NetworkRecoveryAction::Restore => self.process_network_available().await,
            NetworkRecoveryAction::CleanupOnly => self.cleanup_connections().await,
            NetworkRecoveryAction::ForceReconnect => self.force_reconnect().await,
        }
    }

    /// Execute a policy-selected recovery action while preserving a typed
    /// diagnosis for the supervisor.
    ///
    /// The compatibility default wraps legacy `String` errors as
    /// `PathUnreachable`; the runtime processor overrides this method so native
    /// [`NetworkError`] variants reach the policy without string parsing.
    #[doc(hidden)]
    async fn process_network_recovery_effect(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), NetworkRecoveryError> {
        self.process_network_recovery_action(action)
            .await
            .map_err(|detail| NetworkRecoveryError::from_opaque("custom recovery effect", detail))
    }

    /// Execute an effect under the supervisor action that owns its commit
    /// rights. Custom processors can ignore the identity; the built-in
    /// signaling client uses it to tag the committed socket generation. The
    /// returned [`EffectReport`] carries effect-synthesized terminal facts
    /// (per-peer coverage residuals); custom processors keep the legacy
    /// success/failure contract.
    #[doc(hidden)]
    async fn process_network_recovery_effect_for_action(
        &self,
        action: NetworkRecoveryAction,
        _action_id: u64,
    ) -> Result<EffectReport, NetworkRecoveryError> {
        self.process_network_recovery_effect(action)
            .await
            .map(|()| EffectReport::PURE)
    }

    /// Run a bounded teardown effect (cleanup or confirmed offline disconnect).
    ///
    /// Implementations must synchronously invalidate the scoped generations,
    /// commit rights, and new-work admission before any physical step, bound the
    /// remaining physical steps by `budget`, and never let a remote-notification
    /// failure block local completion. The default delegates to the unbounded
    /// action so custom processors keep working; the runtime processor overrides
    /// it with the real bounded-completion contract.
    async fn run_bounded_teardown(
        &self,
        action: NetworkRecoveryAction,
        _budget: Duration,
    ) -> TeardownReport {
        match self.process_network_recovery_action(action).await {
            Ok(()) => TeardownReport::succeeded(),
            Err(e) => TeardownReport::completed_with_residuals(vec![e]),
        }
    }
}

/// Optional legacy time-throttling configuration for direct processor calls.
///
/// The persistent connection supervisor performs structural duplicate
/// suppression from snapshot sequence and route state, so the production
/// default is zero. A non-zero value is retained only as an explicit
/// compatibility/performance knob for callers that invoke processor methods
/// directly.
#[derive(Debug, Clone)]
pub struct DebounceConfig {
    /// Debounce time window (duplicate events within this window are ignored)
    pub window: Duration,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            // Correctness and duplicate suppression belong to the persistent
            // supervisor. Time-based throttling is legacy opt-in only.
            window: Duration::ZERO,
        }
    }
}

/// Debounce state tracking
#[derive(Debug)]
struct DebounceState {
    last_available: tokio::sync::Mutex<Option<Instant>>,
    last_lost: tokio::sync::Mutex<Option<Instant>>,
    last_type_changed: tokio::sync::Mutex<Option<Instant>>,
}

impl DebounceState {
    fn new() -> Self {
        Self {
            last_available: tokio::sync::Mutex::new(None),
            last_lost: tokio::sync::Mutex::new(None),
            last_type_changed: tokio::sync::Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebounceEvent {
    Available,
    Lost,
    TypeChanged,
}

#[derive(Debug)]
struct SignalingRecoveryState {
    connect_lock: tokio::sync::Mutex<()>,
    last_successful_connect: tokio::sync::Mutex<Option<Instant>>,
}

impl SignalingRecoveryState {
    fn new() -> Self {
        Self {
            connect_lock: tokio::sync::Mutex::new(()),
            last_successful_connect: tokio::sync::Mutex::new(None),
        }
    }
}

/// Default network event processor implementation
pub struct DefaultNetworkEventProcessor {
    signaling_client: Arc<dyn SignalingClient>,
    peer_transport: Option<Arc<PeerTransport>>,
    rtc_manager: Option<Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>>,
    debounce_config: DebounceConfig,
    debounce_state: Option<Arc<DebounceState>>,
    recovery_state: Arc<SignalingRecoveryState>,
    session_state: Option<SessionState>,
}

/// Which signaling teardown contract a cleanup sequence uses.
#[derive(Clone, Copy)]
enum SignalingTeardown {
    /// Ordinary disconnect: dropped idempotent waiters fail fast. Manual,
    /// offline, and bounded teardown keep this contract.
    Manual,
    /// Recovery disconnect: dropped idempotent waiters replay once after the
    /// replacement generation commits. Automatic rebuilds that are immediately
    /// followed by a reconnect (ForceReconnect) use this, so an in-flight
    /// RouteCandidates/GetSigningKey request survives the rebuild it
    /// overlapped with.
    Recovery,
}

impl DefaultNetworkEventProcessor {
    pub fn new(signaling_client: Arc<dyn SignalingClient>) -> Self {
        Self::new_with_debounce_and_peer_transport(
            signaling_client,
            DebounceConfig::default(),
            None,
        )
    }

    pub fn new_with_debounce(
        signaling_client: Arc<dyn SignalingClient>,
        debounce_config: DebounceConfig,
    ) -> Self {
        Self::new_with_debounce_and_peer_transport(signaling_client, debounce_config, None)
    }

    pub(crate) fn new_with_peer_transport(
        signaling_client: Arc<dyn SignalingClient>,
        peer_transport: Option<Arc<PeerTransport>>,
    ) -> Self {
        Self::new_with_debounce_and_peer_transport(
            signaling_client,
            DebounceConfig::default(),
            peer_transport,
        )
    }

    pub(crate) fn new_with_debounce_and_peer_transport(
        signaling_client: Arc<dyn SignalingClient>,
        debounce_config: DebounceConfig,
        peer_transport: Option<Arc<PeerTransport>>,
    ) -> Self {
        let debounce_state =
            (!debounce_config.window.is_zero()).then(|| Arc::new(DebounceState::new()));
        Self {
            signaling_client,
            peer_transport,
            rtc_manager: None,
            debounce_config,
            debounce_state,
            recovery_state: Arc::new(SignalingRecoveryState::new()),
            session_state: None,
        }
    }

    pub(crate) fn with_session_state(mut self, session_state: SessionState) -> Self {
        self.session_state = Some(session_state);
        self
    }

    pub(crate) fn with_rtc_manager(
        mut self,
        manager: Option<Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>>,
    ) -> Self {
        self.rtc_manager = manager;
        self
    }

    /// Check whether an event should be filtered by debounce
    ///
    /// # Returns
    /// - `true`: the event should be processed
    /// - `false`: the event is within the debounce window and should be ignored
    async fn should_process_event(&self, event: DebounceEvent) -> bool {
        if self.debounce_config.window.is_zero() {
            return true;
        }

        let debounce_state = self
            .debounce_state
            .as_ref()
            .expect("nonzero debounce window must have state");
        let now = Instant::now();

        match event {
            DebounceEvent::Available => {
                let mut last = debounce_state.last_available.lock().await;
                if let Some(last_time) = *last {
                    if now.duration_since(last_time) < self.debounce_config.window {
                        tracing::debug!(
                            "⏸️  Debouncing Network Available event (last event was {:?} ago)",
                            now.duration_since(last_time)
                        );
                        return false;
                    }
                }
                *last = Some(now);
                true
            }
            DebounceEvent::Lost => {
                let mut last = debounce_state.last_lost.lock().await;
                if let Some(last_time) = *last {
                    if now.duration_since(last_time) < self.debounce_config.window {
                        tracing::debug!(
                            "⏸️  Debouncing Network Lost event (last event was {:?} ago)",
                            now.duration_since(last_time)
                        );
                        return false;
                    }
                }
                *last = Some(now);
                true
            }
            DebounceEvent::TypeChanged => {
                let mut last = debounce_state.last_type_changed.lock().await;
                if let Some(last_time) = *last {
                    if now.duration_since(last_time) < self.debounce_config.window {
                        tracing::debug!(
                            "⏸️  Debouncing Network TypeChanged event (last event was {:?} ago)",
                            now.duration_since(last_time)
                        );
                        return false;
                    }
                }
                *last = Some(now);
                true
            }
        }
    }

    async fn ensure_signaling_healthy_once_typed(
        &self,
        reason: &str,
        action_id: Option<u64>,
    ) -> Result<(), NetworkRecoveryError> {
        let _guard = self.recovery_state.connect_lock.lock().await;

        if !self.signaling_client.is_connected() {
            tracing::info!(reason = reason, "🔄 Connecting signaling");
            let connect_result = match action_id {
                Some(action_id) => {
                    self.signaling_client
                        .connect_once_for_recovery(action_id)
                        .await
                }
                // Only the deprecated direct-call/batch adapter reaches this
                // branch. It still performs one explicit attempt, while
                // reconciled effects always carry a supervisor action id.
                None => self.signaling_client.connect().await,
            };
            connect_result.map_err(|e| {
                let error = NetworkRecoveryError::from_network_error("WebSocket connect failed", e);
                tracing::error!("❌ {}", error);
                error
            })?;

            *self.recovery_state.last_successful_connect.lock().await = Some(Instant::now());
            tracing::info!(reason = reason, "✅ Signaling connected");
            return Ok(());
        }

        tracing::debug!(
            reason = reason,
            timeout_ms = SIGNALING_PROBE_TIMEOUT.as_millis() as u64,
            "🔎 Probing existing signaling WebSocket"
        );

        match self
            .signaling_client
            .probe_alive(SIGNALING_PROBE_TIMEOUT)
            .await
        {
            Ok(()) => {
                tracing::debug!(
                    reason = reason,
                    "✅ Signaling probe succeeded; keeping existing WebSocket"
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    reason = reason,
                    "⚠️ Signaling probe failed; rebuilding WebSocket: {}",
                    e
                );

                if let Err(disconnect_err) = self.signaling_client.disconnect_for_recovery().await {
                    tracing::warn!(
                        reason = reason,
                        "⚠️ Failed to disconnect unhealthy signaling before rebuild: {}",
                        disconnect_err
                    );
                }

                tracing::info!(reason = reason, "🔄 Rebuilding signaling: connecting");
                let connect_result = match action_id {
                    Some(action_id) => {
                        self.signaling_client
                            .connect_once_for_recovery(action_id)
                            .await
                    }
                    // See the matching initial-connect branch above: only
                    // deprecated direct/batch callers have no action id.
                    None => self.signaling_client.connect().await,
                };
                connect_result.map_err(|connect_err| {
                    let error = NetworkRecoveryError::from_network_error(
                        "WebSocket rebuild failed",
                        connect_err,
                    );
                    tracing::error!("❌ {}", error);
                    error
                })?;

                *self.recovery_state.last_successful_connect.lock().await = Some(Instant::now());
                tracing::info!(reason = reason, "✅ Signaling rebuilt");
                Ok(())
            }
        }
    }

    async fn restore_signaling_and_webrtc_typed(
        &self,
        reason: &str,
        action_id: Option<u64>,
    ) -> Result<(), NetworkRecoveryError> {
        self.ensure_signaling_healthy_once_typed(reason, action_id)
            .await?;

        if let Some(session) = &self.session_state
            && session.phase().await == SessionPhase::Rebinding
        {
            session.set_active().await;
        }

        // RTC recovery is deliberately not started here. The global
        // supervisor first commits the signaling generation; the RTC
        // controllers observe that committed generation through their
        // recovery gate and only then launch ICE restart / SDP effects.
        Ok(())
    }

    async fn restore_signaling_and_webrtc_from_network_event_typed(
        &self,
        reason: &str,
        action_id: Option<u64>,
    ) -> Result<(), NetworkRecoveryError> {
        self.restore_signaling_and_webrtc_typed(reason, action_id)
            .await
    }

    async fn restore_signaling_and_webrtc_from_network_event(
        &self,
        reason: &str,
    ) -> Result<(), String> {
        self.restore_signaling_and_webrtc_from_network_event_typed(reason, None)
            .await
            .map_err(|error| error.to_string())
    }

    async fn probe_connectivity_typed(&self) -> Result<(), NetworkRecoveryError> {
        self.signaling_client
            .probe_alive(SIGNALING_PROBE_TIMEOUT)
            .await
            .map_err(|error| {
                NetworkRecoveryError::from_network_error("Signaling probe failed", error)
            })
    }

    /// Legacy self-healing probe: on failure, disconnect the unhealthy socket
    /// and restore in the same call.
    ///
    /// Only the deprecated direct-call/batch surface may use this: those
    /// callers have no supervisor to translate a typed probe outcome, so the
    /// pre-RFC probe-then-restore contract is preserved for them. The
    /// supervisor-driven effect path reports the typed outcome instead
    /// ([`Self::probe_signaling_liveness_typed`]) and lets `translate()`
    /// derive the successor action.
    async fn probe_or_restore_typed(&self, reason: &str) -> Result<(), NetworkRecoveryError> {
        match self.probe_connectivity_typed().await {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    reason = reason,
                    "Connectivity probe failed; restoring connections: {}",
                    e
                );
                if let Err(disconnect_err) = self.signaling_client.disconnect_for_recovery().await {
                    tracing::warn!(
                        reason = reason,
                        "Failed to disconnect unhealthy signaling before restore: {}",
                        disconnect_err
                    );
                }
                self.restore_signaling_and_webrtc_from_network_event_typed(reason, None)
                    .await
            }
        }
    }

    /// The supervisor-driven Probe effect: establish whether the live signaling
    /// generation's channel answers, and report the observation.
    ///
    /// Per RFC-0400 the effect never chooses its successor action. A failed
    /// liveness check is the conclusive `TransportImpaired` observation — the
    /// probe's entire contract is to determine channel liveness, so failing it
    /// (timeout, ping error, or no socket at all) is a verified statement that
    /// the signaling channel is dead, not an inconclusive availability error.
    /// Classification rules `Probe + TransportImpaired -> Escalate { Restore }`,
    /// so the rebuild is derived centrally by `translate()` within one probe
    /// timeout instead of inline here.
    async fn probe_signaling_liveness_typed(&self) -> Result<(), NetworkRecoveryError> {
        self.signaling_client
            .probe_alive(SIGNALING_PROBE_TIMEOUT)
            .await
            .map_err(|error| {
                let error = NetworkRecoveryError::TransportImpaired {
                    scopes: vec![format!("signaling: {error}")],
                };
                tracing::warn!(
                    error = %error,
                    consequence = "policy escalates the probe obligation to Restore",
                    "network_event.probe.liveness_failed"
                );
                error
            })
    }

    async fn process_network_available_typed(&self) -> Result<(), NetworkRecoveryError> {
        let should_process = self.should_process_event(DebounceEvent::Available).await;
        if !should_process && self.signaling_client.is_connected() {
            return Ok(());
        }

        tracing::info!("📱 Processing: Network available");

        self.restore_signaling_and_webrtc_from_network_event_typed("NetworkAvailable", None)
            .await
    }

    async fn force_reconnect_typed(
        &self,
        action_id: Option<u64>,
    ) -> Result<(), NetworkRecoveryError> {
        // Recovery-aware cleanup: ForceReconnect always rebuilds signaling
        // right after the teardown, so idempotent waiters dropped here must
        // replay on the replacement instead of failing fast.
        self.cleanup_connections_for_recovery()
            .await
            .map_err(|error| NetworkRecoveryError::from_opaque("ForceReconnect cleanup", error))?;
        // cleanup_connections disconnected signaling and drained the old
        // inbound queue before close_all suppressed the retired peers. A new
        // signaling generation must accept passive RoleAssignment/Offer again;
        // otherwise an endpoint that remains the answerer can never rebuild.
        if let Some(manager) = self.rtc_manager.as_ref() {
            manager.reopen_all_inbound_admission();
        }
        self.restore_signaling_and_webrtc_from_network_event_typed("ForceReconnect", action_id)
            .await
    }

    async fn process_offline(&self) -> Result<(), String> {
        tracing::info!("📱 Processing: Network offline");

        tracing::info!("🔌 Disconnecting WebSocket...");
        let _ = self.signaling_client.disconnect().await;

        Ok(())
    }

    /// The confirmed-offline disconnect steps, accumulating step residuals in
    /// order instead of collapsing them to a single boolean.
    async fn process_offline_collect(&self) -> Vec<String> {
        let mut residuals = Vec::new();
        tracing::info!("📱 Processing: Network offline (bounded)");

        if let Err(e) = self.signaling_client.disconnect().await {
            residuals.push(format!("offline signaling disconnect failed: {e}"));
        }

        residuals
    }

    /// The full cleanup step sequence, accumulating one residual per best-effort
    /// step that leaves an error behind. The logical teardown goal — releasing
    /// local signaling, coordinator, and transport resources — is always
    /// attempted to completion; residuals are diagnostics, not early returns.
    async fn cleanup_connections_collect(
        &self,
        signaling_teardown: SignalingTeardown,
    ) -> Vec<String> {
        let mut residuals: Vec<String> = Vec::new();

        tracing::info!("🧹 Cleaning up all connections (bounded)...");

        // Step 1: Stop old signaling ingress before reopening any peer
        // lifecycle. Disconnect resets the inbound queue, so delayed Offer,
        // RoleAssignment, and ICE messages cannot repopulate state after drain.
        tracing::info!("🔌 Disconnecting WebSocket before peer cleanup...");
        let signaling_disconnect = match signaling_teardown {
            SignalingTeardown::Manual => self.signaling_client.disconnect().await,
            SignalingTeardown::Recovery => self.signaling_client.disconnect_for_recovery().await,
        };
        match signaling_disconnect {
            Ok(_) => tracing::info!("✅ WebSocket disconnected successfully"),
            Err(e) => {
                let err_msg = format!("Failed to disconnect WebSocket before cleanup: {e}");
                tracing::warn!("⚠️  {}", err_msg);
                residuals.push(err_msg);
            }
        }

        // Step 2: Drain every RTC peer controller, including inbound-only
        // controllers that have no PeerTransport entry.
        if let Some(manager) = self.rtc_manager.as_ref() {
            manager.close_all().await;
        }

        // Step 4: Cancel PeerTransport singleflight and close any remaining
        // established transport handles after the coordinator sessions are gone.
        if let Some(ref peer_transport) = self.peer_transport {
            tracing::info!("🔻 Closing all PeerTransport connections...");
            if let Err(e) = peer_transport.close_all().await {
                let err_msg = format!("Failed to close peer transports: {}", e);
                tracing::warn!("⚠️  {}", err_msg);
                residuals.push(err_msg);
            } else {
                tracing::info!("✅ All PeerTransport connections closed");
            }
        }

        // Step 4: A cancelled PeerTransport creator may have crossed the first
        // drain before observing its token. Repeat the bounded manager drain
        // after creators have been cancelled.
        if let Some(manager) = self.rtc_manager.as_ref() {
            manager.close_all().await;
        }

        residuals
    }

    /// Synchronously close signaling commit rights and new-work admission, then
    /// run the cleanup steps under the obligation's remaining `budget`. On
    /// budget expiry the remaining steps are detached and the teardown is
    /// `Abandoned`; otherwise it completes (with residuals if any step failed).
    async fn bounded_cleanup(&self, budget: Duration) -> TeardownReport {
        // Step 0: before any physical step, invalidate the scoped signaling
        // generation and suppress new attempts so no later commit can race the
        // teardown across its commit boundary.
        self.signaling_client.invalidate_generation();

        match crate::timer::timeout(
            crate::timer::ids::RECOVERY_CLEANUP_TEARDOWN,
            budget,
            self.cleanup_connections_collect(SignalingTeardown::Manual),
        )
        .await
        {
            Ok(residuals) => TeardownReport {
                reached_goal: true,
                deadline_reached: false,
                residuals,
            },
            Err(_) => {
                tracing::warn!(
                    budget_ms = budget.as_millis() as u64,
                    "network_event.cleanup.deadline_abandoned"
                );
                TeardownReport::abandoned(vec![format!(
                    "cleanup teardown abandoned remaining steps after {}ms budget",
                    budget.as_millis()
                )])
            }
        }
    }

    /// The confirmed-offline disconnect under the obligation's remaining budget.
    async fn bounded_offline(&self, budget: Duration) -> TeardownReport {
        self.signaling_client.invalidate_generation();

        match crate::timer::timeout(
            crate::timer::ids::RECOVERY_OFFLINE_TEARDOWN,
            budget,
            self.process_offline_collect(),
        )
        .await
        {
            Ok(residuals) => TeardownReport {
                reached_goal: true,
                deadline_reached: false,
                residuals,
            },
            Err(_) => {
                tracing::warn!(
                    budget_ms = budget.as_millis() as u64,
                    "network_event.offline.deadline_abandoned"
                );
                TeardownReport::abandoned(vec![format!(
                    "offline disconnect abandoned remaining steps after {}ms budget",
                    budget.as_millis()
                )])
            }
        }
    }

    async fn execute_recovery_effect(
        &self,
        action: NetworkRecoveryAction,
        supervisor_action_id: Option<u64>,
    ) -> Result<EffectReport, NetworkRecoveryError> {
        if action == NetworkRecoveryAction::Noop {
            return Ok(EffectReport::PURE);
        }

        let action_id = supervisor_action_id
            .unwrap_or_else(|| NEXT_NETWORK_RECOVERY_ACTION_ID.fetch_add(1, Ordering::Relaxed));
        tracing::info!(
            action_id,
            action = ?action,
            "network_event.execution.action.start"
        );
        // A supervisor-driven Restore is a peer-scoped recovery effect
        // (RFC-0400 per-peer coverage contract): its target set — every peer
        // holding a session that is not currently covered by a success fact —
        // is captured at dispatch, before the shared prerequisites run, so
        // per-peer terminal facts can be validated against the frozen tuples
        // at synthesis time.
        let captured_targets =
            if action == NetworkRecoveryAction::Restore && supervisor_action_id.is_some() {
                self.capture_peer_recovery_targets().await
            } else {
                Vec::new()
            };
        let result = match action {
            NetworkRecoveryAction::Noop => Ok(()),
            NetworkRecoveryAction::Offline => self
                .process_offline()
                .await
                .map_err(|error| NetworkRecoveryError::from_opaque("Offline", error)),
            NetworkRecoveryAction::Probe => self.probe_signaling_liveness_typed().await,
            NetworkRecoveryAction::Restore => {
                self.restore_signaling_and_webrtc_from_network_event_typed(
                    "SupervisorRestore",
                    supervisor_action_id,
                )
                .await
            }
            NetworkRecoveryAction::CleanupOnly => self
                .cleanup_connections()
                .await
                .map_err(|error| NetworkRecoveryError::from_opaque("CleanupOnly", error)),
            NetworkRecoveryAction::ForceReconnect => {
                self.force_reconnect_typed(supervisor_action_id).await
            }
        };
        let report = match (&result, captured_targets.is_empty()) {
            // Shared prerequisites failed: the shared-scope diagnosis is the
            // effect's failure; per-peer coverage is not synthesized (the
            // failure verdict applies and the obligation is retained).
            (Err(_), _) => EffectReport::PURE,
            // Not a peer-scoped effect dispatch (no captured targets): keep
            // the legacy success contract.
            (Ok(()), true) => EffectReport::PURE,
            // Shared prerequisites completed: synthesize the terminal
            // outcome from per-peer terminal facts, handoffs, and
            // out-of-scope records. Dispatch alone never yields a
            // success-class outcome for an uncovered target (RFC-0400
            // invariant 33).
            (Ok(()), false) => EffectReport {
                residuals: self
                    .synthesize_peer_coverage(captured_targets, action_id)
                    .await,
            },
        };
        tracing::info!(
            action_id,
            action = ?action,
            success = result.is_ok(),
            residual_count = report.residuals.len(),
            "network_event.execution.action.completed"
        );
        result.map(|()| report)
    }

    /// Capture the Restore effect's target set at dispatch: one identity
    /// tuple per peer that holds a PC session (generation > 0) and is not
    /// currently covered by a valid success fact, excluding peers already
    /// leaving (`close_peer` in flight). Peers without a session have no
    /// identity tuple to recover and are not targets.
    async fn capture_peer_recovery_targets(
        &self,
    ) -> Vec<crate::wire::webrtc::rtc::recovery::PeerRecoveryTarget> {
        let Some(manager) = self.rtc_manager.as_ref() else {
            return Vec::new();
        };
        let mut targets = Vec::new();
        for snapshot in manager.list_snapshots().await {
            if snapshot.pc_generation == 0
                || manager.is_closing(&snapshot.peer).await
                || (snapshot.sendable && snapshot.status == WebRtcPeerStatus::Connected)
            {
                continue;
            }
            targets.push(crate::wire::webrtc::rtc::recovery::PeerRecoveryTarget::new(
                snapshot.peer.clone(),
                snapshot.pc_generation,
                snapshot.ice_generation,
            ));
        }
        if !targets.is_empty() {
            tracing::debug!(
                targets = ?targets
                    .iter()
                    .map(|t| {
                        format!(
                            "{}#pc{}#ice{}",
                            t.peer.to_string_repr(),
                            t.pc_generation,
                            t.ice_generation
                        )
                    })
                    .collect::<Vec<_>>(),
                "network_event.recovery.targets_captured"
            );
        }
        targets
    }

    /// Synthesize the peer-scoped coverage outcome once the effect's shared
    /// prerequisites have completed (RFC-0400 per-peer coverage contract).
    ///
    /// Every captured target must be covered by an identity-validated
    /// terminal fact, or named in a recorded residual:
    /// - a current snapshot that is sendable on the captured session (or a
    ///   same-or-newer ICE generation within it, or a newer session) covers
    ///   the target — a fact bearing an older ICE generation covers nothing;
    /// - a still-unfinished or peer-scoped-retryable target is handed off:
    ///   the receiving resource owner records a recovery obligation carrying
    ///   the target's current identity tuple and acknowledges the record, and
    ///   the completion carries one handoff residual naming the tuple and
    ///   the owner;
    /// - a peer that conclusively left (controller gone or closing) is named
    ///   in one out-of-scope residual.
    async fn synthesize_peer_coverage(
        &self,
        captured_targets: Vec<crate::wire::webrtc::rtc::recovery::PeerRecoveryTarget>,
        action_id: u64,
    ) -> Vec<EffectDiagnosis> {
        use crate::wire::webrtc::rtc::recovery::PeerRecoveryTarget;

        let Some(manager) = self.rtc_manager.as_ref() else {
            return Vec::new();
        };
        let mut residuals = Vec::new();
        for target in captured_targets {
            let snapshot = manager.get_snapshot(&target.peer).await;
            let out_of_scope = manager.is_closing(&target.peer).await
                || snapshot.as_ref().is_none_or(|s| s.pc_generation == 0);
            if out_of_scope {
                tracing::info!(
                    action_id,
                    peer = %target.peer.to_string_repr(),
                    "network_event.recovery.target_out_of_scope"
                );
                residuals.push(EffectDiagnosis::TransportImpaired {
                    scopes: vec![format!(
                        "peer={}:out_of_scope:peer_left_session",
                        target.peer.to_string_repr()
                    )],
                });
                continue;
            }
            let snapshot = snapshot.expect("out-of-scope peers excluded above");
            let success_fact_valid = snapshot.sendable
                && snapshot.status == WebRtcPeerStatus::Connected
                && (snapshot.pc_generation > target.pc_generation
                    || (snapshot.pc_generation == target.pc_generation
                        && snapshot.ice_generation >= target.ice_generation));
            if success_fact_valid {
                tracing::debug!(
                    action_id,
                    peer = %target.peer.to_string_repr(),
                    "network_event.recovery.target_covered_by_success_fact"
                );
                continue;
            }
            // Stale-fact guard: a snapshot naming an older session
            // generation than the captured tuple covers nothing; the handoff
            // records the tuple the receiving owner will actually track.
            let handoff = PeerRecoveryTarget::new(
                target.peer.clone(),
                snapshot.pc_generation.max(target.pc_generation),
                if snapshot.pc_generation >= target.pc_generation {
                    snapshot.ice_generation
                } else {
                    target.ice_generation
                },
            );
            // The handoff exists only once the receiving owner has recorded
            // the obligation; the record call is the acknowledgement.
            manager.record_peer_recovery_obligation(handoff.clone());
            tracing::info!(
                action_id,
                peer = %target.peer.to_string_repr(),
                pc_generation = handoff.pc_generation,
                ice_generation = handoff.ice_generation,
                owner = "RtcConnectionManager",
                "network_event.recovery.target_handed_off"
            );
            residuals.push(EffectDiagnosis::TransportImpaired {
                scopes: vec![format!(
                    "peer={}:handoff:session={}:ice={}:owner=RtcConnectionManager",
                    handoff.peer.to_string_repr(),
                    handoff.pc_generation,
                    handoff.ice_generation
                )],
            });
        }
        residuals
    }
}

#[async_trait::async_trait]
impl NetworkEventProcessor for DefaultNetworkEventProcessor {
    fn invalidate_signaling_connection_attempts(&self) {
        self.signaling_client.invalidate_generation();
    }

    /// Process network available event
    async fn process_network_available(&self) -> Result<(), String> {
        self.process_network_available_typed()
            .await
            .map_err(|error| error.to_string())
    }

    /// Process network lost event
    async fn process_network_lost(&self) -> Result<(), String> {
        // Debounce check
        if !self.should_process_event(DebounceEvent::Lost).await {
            return Ok(());
        }

        self.process_offline().await
    }

    /// Process network type changed event
    async fn process_network_type_changed(
        &self,
        is_wifi: bool,
        is_cellular: bool,
    ) -> Result<(), String> {
        // Debounce check
        let should_process = self.should_process_event(DebounceEvent::TypeChanged).await;
        if !should_process && self.signaling_client.is_connected() {
            return Ok(());
        }

        tracing::info!(
            "📱 Processing: Network type changed (WiFi={}, Cellular={})",
            is_wifi,
            is_cellular
        );

        self.restore_signaling_and_webrtc_from_network_event("NetworkTypeChanged")
            .await
    }

    /// Proactively clean up all connections
    ///
    /// Differs from `process_network_lost()`:
    /// - No debounce check (proactive calls always execute)
    /// - Intended for app lifecycle management, not network event response
    async fn cleanup_connections(&self) -> Result<(), String> {
        let residuals = self
            .cleanup_connections_collect(SignalingTeardown::Manual)
            .await;
        if let Some(err) = residuals.into_iter().next() {
            tracing::warn!(
                error = %err,
                "Connection cleanup released remaining resources but did not fully quiesce"
            );
            Err(err)
        } else {
            tracing::info!("✅ Connection cleanup completed");
            Ok(())
        }
    }

    async fn cleanup_connections_for_recovery(&self) -> Result<(), String> {
        let residuals = self
            .cleanup_connections_collect(SignalingTeardown::Recovery)
            .await;
        if let Some(err) = residuals.into_iter().next() {
            tracing::warn!(
                error = %err,
                "Recovery cleanup released remaining resources but did not fully quiesce"
            );
            Err(err)
        } else {
            tracing::info!("✅ Recovery cleanup completed");
            Ok(())
        }
    }

    async fn run_bounded_teardown(
        &self,
        action: NetworkRecoveryAction,
        budget: Duration,
    ) -> TeardownReport {
        match action {
            NetworkRecoveryAction::CleanupOnly => self.bounded_cleanup(budget).await,
            NetworkRecoveryAction::Offline => self.bounded_offline(budget).await,
            // Not a teardown action; keep the coarse mapping for safety.
            other => match self.process_network_recovery_action(other).await {
                Ok(()) => TeardownReport::succeeded(),
                Err(e) => TeardownReport::completed_with_residuals(vec![e]),
            },
        }
    }

    async fn probe_connectivity(&self) -> Result<(), String> {
        self.probe_connectivity_typed()
            .await
            .map_err(|error| error.to_string())
    }

    async fn force_reconnect(&self) -> Result<(), String> {
        self.force_reconnect_typed(None)
            .await
            .map_err(|error| error.to_string())
    }

    async fn process_network_recovery_action(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        match action {
            // The direct-call/batch surface has no supervisor to translate a
            // typed probe outcome into a successor action, so it keeps the
            // pre-RFC self-healing probe-then-restore contract.
            NetworkRecoveryAction::Probe => self
                .probe_or_restore_typed("Probe")
                .await
                .map_err(|error| error.to_string()),
            other => self
                .process_network_recovery_effect(other)
                .await
                .map_err(|error| error.to_string()),
        }
    }

    async fn process_network_recovery_effect(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), NetworkRecoveryError> {
        // The single-flight execution machine (`view.execution`) is owned by the
        // supervisor, which is its only writer. The effect task performs I/O and
        // reports a typed outcome; it holds no policy state of its own. Driving a
        // second execution tracker here would poison it whenever the supervisor
        // cancels the effect after `begin` but before `complete` — a cancelled
        // future's `complete` never runs — permanently wedging every later
        // recovery action (RFC-0400 "MUST NOT let a cancellation poison
        // single-flight ownership", invariant 8).
        self.execute_recovery_effect(action, None).await.map(|_| ())
    }

    async fn process_network_recovery_effect_for_action(
        &self,
        action: NetworkRecoveryAction,
        action_id: u64,
    ) -> Result<EffectReport, NetworkRecoveryError> {
        self.execute_recovery_effect(action, Some(action_id)).await
    }
}

/// Select one recovery action from a batch of events with the legacy
/// synchronous selector.
///
/// Deprecated: this is a second policy source that does not go through the RFC
/// `translate` engine and has no timer owner. Prefer the responsive reconciler
/// ([`run_network_event_reconciler`]), which routes every decision through the
/// pure translation function. Retained only for the migration window.
#[deprecated(note = "second policy source without a timer owner; use \
            run_network_event_reconciler, which drives the RFC translate engine")]
#[allow(deprecated)]
pub fn select_network_recovery_action(events: &[NetworkEvent]) -> NetworkRecoveryAction {
    super::recovery_supervisor::legacy_select_action(events)
}

/// Process a batch of events by selecting one action and running it once.
///
/// Deprecated: prefer the responsive reconciler
/// ([`run_network_event_reconciler`]); this legacy batch path bypasses the RFC
/// `translate` engine and its single-flight/timer ownership. Retained only for
/// the migration window.
#[deprecated(note = "legacy batch path bypassing the RFC translate engine; use \
            run_network_event_reconciler")]
#[allow(deprecated)]
pub async fn process_network_event_batch(
    events: Vec<NetworkEvent>,
    processor: Arc<dyn NetworkEventProcessor>,
) -> Vec<NetworkEventResult> {
    if events.is_empty() {
        return Vec::new();
    }

    let action = super::recovery_supervisor::legacy_select_action(&events);
    process_network_event_batch_with_action(events, action, processor).await
}

async fn process_network_event_batch_with_action(
    events: Vec<NetworkEvent>,
    action: NetworkRecoveryAction,
    processor: Arc<dyn NetworkEventProcessor>,
) -> Vec<NetworkEventResult> {
    let start = Instant::now();

    tracing::info!(
        event_count = events.len(),
        action = ?action,
        "network_event.action.start"
    );

    let result = processor.process_network_recovery_action(action).await;

    let duration_ms = start.elapsed().as_millis() as u64;
    match &result {
        Ok(()) => tracing::info!(
            event_count = events.len(),
            action = ?action,
            duration_ms,
            "network_event.action.completed"
        ),
        Err(e) => tracing::warn!(
            event_count = events.len(),
            action = ?action,
            duration_ms,
            error = %e,
            "network_event.action.completed"
        ),
    }

    events
        .into_iter()
        .map(|event| match &result {
            Ok(()) => NetworkEventResult::success(event, duration_ms),
            Err(e) => NetworkEventResult::failure(event, e.clone(), duration_ms),
        })
        .collect()
}

/// Opaque request envelope consumed by the network-event reconciler.
///
/// Callers create the channel carrying this type, but production requests
/// must be emitted through [`NetworkEventHandle`]. The handle owns the
/// supervisor ordering metadata and ingress timestamp; exposing those fields
/// for external struct literals would let callers violate those invariants.
#[non_exhaustive]
pub struct NetworkEventRequest {
    event: NetworkEvent,
    result_tx: oneshot::Sender<NetworkEventResult>,
    /// The path-monitor incarnation epoch stamped by the originating handle;
    /// `(source_epoch, sequence)` lexicographic order decides snapshot acceptance.
    source_epoch: u64,
    /// Monotonic supervisor-ingress time captured before the request is sent.
    ///
    /// This stays out of [`NetworkSnapshot`] so the platform FFI schema does
    /// not need to expose a runtime-specific clock representation.
    observed_at: Instant,
}

#[cfg(any(test, feature = "test-utils"))]
impl NetworkEventRequest {
    /// Construct a request with explicit supervisor metadata for deterministic
    /// tests and benchmarks that exercise epoch ordering or ingress timing.
    #[doc(hidden)]
    pub fn new_for_test(
        event: NetworkEvent,
        result_tx: oneshot::Sender<NetworkEventResult>,
        source_epoch: u64,
        observed_at: Instant,
    ) -> Self {
        Self {
            event,
            result_tx,
            source_epoch,
            observed_at,
        }
    }

    /// Consume a captured request in tests that exercise a custom event loop.
    #[doc(hidden)]
    pub fn into_parts_for_test(self) -> (NetworkEvent, oneshot::Sender<NetworkEventResult>) {
        (self.event, self.result_tx)
    }
}

/// The terminal facts an effect contributes to its `EffectCompleted`
/// synthesis, beyond the plain success/failure vocabulary.
///
/// A peer-scoped Restore effect (RFC-0400 "Per-peer coverage for
/// peer-scoped recovery", invariant 33) populates `residuals` when shared
/// prerequisites completed but not every captured target peer is covered by a
/// valid terminal fact: one residual per handed-off or out-of-scope target.
/// The shell folds a non-empty report into `EffectOutcome::CompletedWithResiduals`.
#[doc(hidden)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectReport {
    pub(crate) residuals: Vec<EffectDiagnosis>,
}

impl EffectReport {
    /// A report with no residuals (full coverage).
    pub(crate) const PURE: EffectReport = EffectReport {
        residuals: Vec::new(),
    };
}

/// A minimal observation of supervisor progress for tests and callers.
///
/// It exposes the causal `policy_revision`, the identity of the most recently
/// started action, and the most recent effect outcome — the final result a
/// caller would otherwise infer by racing an effect.
#[derive(Debug, Clone, Default)]
pub struct SupervisorStatus {
    pub policy_revision: u64,
    pub last_action_id: Option<u64>,
    pub last_outcome: Option<ObservedOutcome>,
}

/// The observable class of the most recent effect completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedOutcome {
    Succeeded,
    CompletedWithResiduals,
    Abandoned,
    Failed,
    Cancelled,
    Aborted,
}

fn observed_outcome(outcome: &EffectOutcome) -> ObservedOutcome {
    match outcome {
        EffectOutcome::Succeeded => ObservedOutcome::Succeeded,
        EffectOutcome::CompletedWithResiduals { .. } => ObservedOutcome::CompletedWithResiduals,
        EffectOutcome::Abandoned { .. } => ObservedOutcome::Abandoned,
        EffectOutcome::Failed { .. } => ObservedOutcome::Failed,
        EffectOutcome::Cancelled => ObservedOutcome::Cancelled,
        EffectOutcome::Aborted { .. } => ObservedOutcome::Aborted,
    }
}

/// The shell's handle on the single in-flight effect: its identity and the
/// cancellation token the supervisor toggles on a preemption request.
struct RunningEffect {
    action_id: u64,
    token: CancellationToken,
}

impl Drop for RunningEffect {
    fn drop(&mut self) {
        // Keep detached effect work under the reconciler's ownership even if
        // the reconciler task itself is aborted before its normal epilogue.
        self.token.cancel();
    }
}

/// A narrow, cloneable handle for resource owners (signaling, session) to feed
/// normalized facts into the running supervisor without going through the
/// public [`NetworkEvent`] surface.
///
/// Facts share the supervisor's internal input channel, so they are processed
/// in the same one-input-at-a-time policy order as timer expiries and effect
/// completions. Delivery is non-blocking and lossless while the supervisor is
/// alive: the internal queue is unbounded because resource owners may emit
/// while holding a transport lock and therefore cannot await capacity.
#[derive(Clone)]
pub struct SupervisorFactSink {
    tx: mpsc::UnboundedSender<tp::Input>,
}

/// The origin of a normalized signaling-committed fact, as seen at the public
/// resource-owner boundary. Mirrors the supervisor's internal origin type but
/// keeps the recovery-policy internals out of the wire layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalingFactOrigin {
    /// Produced by the running lifecycle effect (carries its `action_id`).
    CurrentEffect { action_id: u64 },
    /// External news from the signaling resource owner itself.
    External,
}

/// Why a live signaling generation was lost. Diagnostic only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalingFactLostCause {
    /// A lifecycle disconnect or explicit invalidation invalidated the socket.
    Disconnected,
    /// A newer generation superseded this one.
    Superseded,
    /// The remote or transport reset the connection.
    RemoteReset,
}

/// Why a resource owner observed that signaling connectivity is required.
///
/// This is diagnostic only: the supervisor coalesces equivalent requests and
/// never lets one bypass a backoff deadline, lifecycle gate, or terminal park.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalingRecoveryRequestCause {
    HeartbeatObservedDisconnected,
    InitialConnectDeferred,
    CredentialRebound,
}

impl SupervisorFactSink {
    pub(crate) fn new(tx: mpsc::UnboundedSender<tp::Input>) -> Self {
        Self { tx }
    }

    fn emit(&self, input: tp::Input) {
        if self.tx.send(input).is_err() {
            tracing::debug!("network_event.fact_sink.closed");
        }
    }

    /// Record that a signaling generation was authoritatively committed (its
    /// generation validated and its socket published under the commit facade).
    pub fn signaling_generation_committed(&self, generation: u64, origin: SignalingFactOrigin) {
        let origin = match origin {
            SignalingFactOrigin::CurrentEffect { action_id } => {
                tp::SignalingOrigin::CurrentEffect { action_id }
            }
            SignalingFactOrigin::External => tp::SignalingOrigin::External,
        };
        self.emit(tp::Input::SignalingGenerationCommitted { generation, origin });
    }

    /// Record that a live signaling generation was lost at a disconnect or
    /// invalidation point.
    pub fn signaling_generation_lost(&self, generation: u64, cause: SignalingFactLostCause) {
        let cause = match cause {
            SignalingFactLostCause::Disconnected => tp::SignalingLostCause::Disconnected,
            SignalingFactLostCause::Superseded => tp::SignalingLostCause::Superseded,
            SignalingFactLostCause::RemoteReset => tp::SignalingLostCause::RemoteReset,
        };
        self.emit(tp::Input::SignalingGenerationLost { generation, cause });
    }

    /// Record demand for a signaling connection without directly driving the
    /// socket. Repeated demand is coalesced by the recovery reducer.
    pub fn signaling_connection_required(&self, cause: SignalingRecoveryRequestCause) {
        let cause = match cause {
            SignalingRecoveryRequestCause::HeartbeatObservedDisconnected => {
                tp::SignalingConnectionRequiredCause::HeartbeatObservedDisconnected
            }
            SignalingRecoveryRequestCause::InitialConnectDeferred => {
                tp::SignalingConnectionRequiredCause::InitialConnectDeferred
            }
            SignalingRecoveryRequestCause::CredentialRebound => {
                tp::SignalingConnectionRequiredCause::CredentialRebound
            }
        };
        self.emit(tp::Input::SignalingConnectionRequired { cause });
    }

    /// Configure the supervisor-owned retry window and backoff compatibility
    /// policy before signaling resource facts begin arriving.
    #[doc(hidden)]
    pub(crate) fn signaling_recovery_configured(
        &self,
        max_attempts: u32,
        initial_delay: Duration,
        max_delay: Duration,
        backoff_multiplier: f64,
    ) {
        // The internal reducer input derives `Eq`; retain the public floating
        // point value losslessly as bits rather than making the ordered fact
        // model approximate it.
        self.emit(tp::Input::SignalingRecoveryConfigChanged {
            max_attempts,
            initial_delay,
            max_delay,
            backoff_multiplier_bits: backoff_multiplier.to_bits(),
        });
    }

    /// Record that the membership authority published newer credential
    /// material. Revision validation makes duplicate publications inert.
    pub fn signaling_credential_published(&self, revision: u64) {
        self.emit(tp::Input::SignalingCredentialPublished { revision });
    }

    /// Record that a new session generation was authoritatively committed.
    pub fn session_activated(&self, session_generation: u64) {
        self.emit(tp::Input::SessionActivated { session_generation });
    }
}

/// An opaque handle on the supervisor's internal input channel, so the node can
/// hold and hand it to [`run_network_event_reconciler_with_channel`] without
/// naming the crate-private input type.
pub struct SupervisorInternalChannel {
    tx: mpsc::UnboundedSender<tp::Input>,
    rx: mpsc::UnboundedReceiver<tp::Input>,
    send_projection_tx: watch::Sender<tp::SendProjection>,
    profile: tp::LifecycleProfile,
    clock_origin: Instant,
}

impl SupervisorInternalChannel {
    /// Subscribe to the supervisor's outbound-send admission projection.
    ///
    /// The initial policy is `Normal`; every accepted reducer input publishes
    /// the newly derived projection before any corresponding effect is started.
    pub(crate) fn subscribe_send_projection(&self) -> watch::Receiver<tp::SendProjection> {
        self.send_projection_tx.subscribe()
    }

    /// Switch the lifecycle profile in place (#568): the channel is created
    /// eagerly with `Ungated` at node construction and every node — including
    /// headless runtimes that never create a public `NetworkEventHandle` —
    /// keeps the same supervisor input queue; `create_network_event_handle`
    /// only retargets the profile.
    ///
    /// The retarget always precedes any supervisor translation: handle
    /// creation borrows the node mutably while `start()` consumes it, and the
    /// reconciler that snapshots this profile only spawns during `start()`.
    /// Facts queued earlier — such as the signaling retry config emitted when
    /// the fact sink is installed at construction — are therefore always
    /// normalized against the final profile, so the mobile backoff cap cannot
    /// be skipped by an early `SignalingRecoveryConfigChanged`.
    pub(crate) fn set_profile(&mut self, profile: tp::LifecycleProfile) {
        self.profile = profile;
    }
}

#[derive(Debug, Clone)]
struct ReconcilerConfig {
    profile: tp::LifecycleProfile,
    clock_origin: Instant,
    rtc_recovery: Option<Arc<crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RtcPathObservation {
    semantic_path: tp::SemanticPath,
    /// Transport identity includes VPN because entering/leaving a VPN changes
    /// the effective route/interface class. Billing and data-saver properties
    /// can change without requiring an ICE rebind; the lifecycle policy keeps
    /// those fields in its own signaling probe fingerprint.
    transport_fingerprint: u8,
}

fn rtc_path_observation(snapshot: &NetworkSnapshot) -> RtcPathObservation {
    let transport = snapshot.transport;
    let mut transport_fingerprint = 0u8;
    transport_fingerprint |= transport.wifi as u8;
    transport_fingerprint |= (transport.cellular as u8) << 1;
    transport_fingerprint |= (transport.ethernet as u8) << 2;
    transport_fingerprint |= (transport.vpn as u8) << 3;
    transport_fingerprint |= (transport.other as u8) << 4;

    RtcPathObservation {
        semantic_path: match snapshot.availability {
            NetworkAvailability::Unknown => tp::SemanticPath::Unknown,
            NetworkAvailability::Available => tp::SemanticPath::Online,
            NetworkAvailability::Unavailable => tp::SemanticPath::Offline,
        },
        transport_fingerprint,
    }
}

fn publish_rtc_network_fact(
    feed: &crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor,
    event: &NetworkEvent,
    previous_path: Option<RtcPathObservation>,
    path_change_pending: &mut bool,
    current_path: &mut Option<RtcPathObservation>,
) {
    use crate::wire::webrtc::rtc::recovery::RtcNetworkEvent;

    if let NetworkEvent::NetworkPathChanged { snapshot } = event {
        let observation = rtc_path_observation(snapshot);
        let address_changed = match observation.semantic_path {
            tp::SemanticPath::Unknown => {
                // Android can report a new default-network handle as
                // Online -> Unknown -> Online. Keep the path edge alive until
                // the new network has a usable capability snapshot.
                if previous_path
                    .is_some_and(|previous| previous.semantic_path == tp::SemanticPath::Online)
                {
                    *path_change_pending = true;
                }
                false
            }
            tp::SemanticPath::Offline => {
                // Availability recovery below owns Offline -> Online. Do not
                // turn the same transition into a second local-address edge.
                *path_change_pending = false;
                false
            }
            tp::SemanticPath::Online => {
                let changed_after_unknown = *path_change_pending;
                let changed_transport = previous_path.is_some_and(|previous| {
                    previous.semantic_path == tp::SemanticPath::Online
                        && previous.transport_fingerprint != observation.transport_fingerprint
                });
                *path_change_pending = false;
                changed_after_unknown || changed_transport
            }
        };
        *current_path = Some(observation);

        // Preserve the existing availability projection. A material path
        // change additionally becomes a RTC fact so an already-sendable
        // PeerConnection does not wait for a delayed Disconnected/Failed edge.
        if snapshot.is_offline() {
            feed.publish(RtcNetworkEvent::NetworkUnavailable);
        } else if snapshot.should_restore() {
            feed.publish(RtcNetworkEvent::NetworkAvailable);
        }
        if address_changed {
            feed.publish(RtcNetworkEvent::LocalAddressChanged);
        }
        return;
    }

    let fact = match event {
        NetworkEvent::AppLifecycleChanged {
            state: AppLifecycleState::Background,
        } => Some(RtcNetworkEvent::AppBackground),
        NetworkEvent::AppLifecycleChanged {
            state:
                AppLifecycleState::Foreground {
                    background_duration_ms,
                },
        } => Some(RtcNetworkEvent::AppForeground {
            background_duration_ms: *background_duration_ms,
        }),
        NetworkEvent::ForceReconnect { .. } => Some(RtcNetworkEvent::ForceReconnect),
        _ => None,
    };
    if let Some(fact) = fact {
        feed.publish(fact);
    }
}

fn sync_rtc_signaling_generation(
    feed: &crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor,
    supervisor: &RecoverySupervisor,
) {
    let view = supervisor.view();
    let recovery_gate_open = view.recovery_mode == tp::RecoveryModeState::Active
        && view.app_phase != tp::AppPhaseState::Background
        && matches!(
            view.network_path,
            tp::NetworkPathState::Unknown | tp::NetworkPathState::Online
        )
        && view.recovery_intent == tp::RecoveryIntentState::Idle
        && view.cleanup_work == tp::CleanupWorkState::Idle
        && view.offline_work == tp::OfflineWorkState::Idle
        && view.execution == tp::ExecutionState::Idle;
    feed.set_signaling_generation(
        recovery_gate_open
            .then_some(view.live_signaling_generation)
            .flatten(),
    );
}

/// Create the supervisor's internal input channel plus a [`SupervisorFactSink`]
/// over its sender. The node hands the sink to resource owners and passes the
/// channel into [`run_network_event_reconciler_with_channel`]. Rust and
/// headless callers default to an ungated lifecycle profile.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn supervisor_internal_channel() -> (SupervisorFactSink, SupervisorInternalChannel) {
    supervisor_internal_channel_with_profile(tp::LifecycleProfile::Ungated)
}

/// Test-support constructor matching the lifecycle-gated mobile binding path.
#[cfg(feature = "test-utils")]
pub(crate) fn supervisor_internal_channel_gated() -> (SupervisorFactSink, SupervisorInternalChannel)
{
    supervisor_internal_channel_with_profile(tp::LifecycleProfile::Gated)
}

/// Create the supervisor channel with an explicit constructor-time lifecycle
/// profile. Mobile bindings use `Gated`; Rust and headless deployments use an
/// `Ungated` profile.
pub(crate) fn supervisor_internal_channel_with_profile(
    profile: tp::LifecycleProfile,
) -> (SupervisorFactSink, SupervisorInternalChannel) {
    let (tx, rx) = mpsc::unbounded_channel::<tp::Input>();
    let (send_projection_tx, _send_projection_rx) = watch::channel(tp::SendProjection::Normal);
    let clock_origin = Instant::now();
    (
        SupervisorFactSink::new(tx.clone()),
        SupervisorInternalChannel {
            tx,
            rx,
            send_projection_tx,
            profile,
            clock_origin,
        },
    )
}

/// Test-support variant of [`run_network_event_reconciler_with_channel`] that
/// additionally exposes the observable status stream, so tests can await the
/// processed policy state (revision / action id / outcome) deterministically
/// instead of polling effect-side counters.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) async fn run_network_event_reconciler_with_channel_and_status(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    channel: SupervisorInternalChannel,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
    status_tx: watch::Sender<SupervisorStatus>,
) {
    let SupervisorInternalChannel {
        tx,
        rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    reconcile_loop(
        event_rx,
        tx,
        rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: None,
        },
    )
    .await;
}

pub async fn run_network_event_reconciler(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
) {
    let clock_origin = Instant::now();
    let (internal_tx, internal_rx) = mpsc::unbounded_channel::<tp::Input>();
    let (status_tx, _status_rx) = watch::channel(SupervisorStatus::default());
    let (send_projection_tx, _send_projection_rx) = watch::channel(tp::SendProjection::Normal);
    reconcile_loop(
        event_rx,
        internal_tx,
        internal_rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile: tp::LifecycleProfile::Ungated,
            clock_origin,
            rtc_recovery: None,
        },
    )
    .await;
}

/// The responsive reconciler with an observable status stream.
///
/// The supervisor stays responsive while an effect runs: effects execute in
/// separate tasks and report versioned completions back over the ordered
/// internal channel, timers are armed as absolute deadlines that deliver their
/// own expiry inputs, and event acceptance is decoupled from effect completion.
pub async fn run_network_event_reconciler_with_status(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
    status_tx: watch::Sender<SupervisorStatus>,
) {
    let clock_origin = Instant::now();
    let (internal_tx, internal_rx) = mpsc::unbounded_channel::<tp::Input>();
    let (send_projection_tx, _send_projection_rx) = watch::channel(tp::SendProjection::Normal);
    reconcile_loop(
        event_rx,
        internal_tx,
        internal_rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile: tp::LifecycleProfile::Ungated,
            clock_origin,
            rtc_recovery: None,
        },
    )
    .await;
}

/// The reconciler entry point used by the node. The caller supplies the internal
/// input channel (see [`supervisor_internal_channel`]) so a [`SupervisorFactSink`]
/// over the same sender can feed normalized signaling and session facts into the
/// supervisor's policy-ordered input queue.
pub(crate) async fn run_network_event_reconciler_with_channel(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    channel: SupervisorInternalChannel,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
) {
    let (status_tx, _status_rx) = watch::channel(SupervisorStatus::default());
    let SupervisorInternalChannel {
        tx,
        rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    reconcile_loop(
        event_rx,
        tx,
        rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: None,
        },
    )
    .await;
}

/// Production entry point for the RTC backend. The lifecycle supervisor
/// remains the sole owner of signaling recovery; accepted network/App facts
/// and its committed signaling generation are projected to peer controllers.
pub(crate) async fn run_network_event_reconciler_with_channel_and_rtc_recovery(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    channel: SupervisorInternalChannel,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
    rtc_recovery: Arc<crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor>,
) {
    let (status_tx, _status_rx) = watch::channel(SupervisorStatus::default());
    let SupervisorInternalChannel {
        tx,
        rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    reconcile_loop(
        event_rx,
        tx,
        rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: Some(rtc_recovery),
        },
    )
    .await;
}

/// Test-support variant retaining the supervisor status stream while using the
/// production RTC signaling-generation projection.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) async fn run_network_event_reconciler_with_channel_and_rtc_recovery_and_status(
    event_rx: mpsc::Receiver<NetworkEventRequest>,
    channel: SupervisorInternalChannel,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
    rtc_recovery: Arc<crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor>,
    status_tx: watch::Sender<SupervisorStatus>,
) {
    let SupervisorInternalChannel {
        tx,
        rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    reconcile_loop(
        event_rx,
        tx,
        rx,
        processor,
        shutdown_token,
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: Some(rtc_recovery),
        },
    )
    .await;
}

// Spawn entry for the reconciler task: every channel and watch sender is
// distinct runtime wiring the loop must own, so the argument count is intrinsic.
#[allow(clippy::too_many_arguments)]
async fn reconcile_loop(
    mut event_rx: mpsc::Receiver<NetworkEventRequest>,
    internal_tx: mpsc::UnboundedSender<tp::Input>,
    mut internal_rx: mpsc::UnboundedReceiver<tp::Input>,
    processor: Arc<dyn NetworkEventProcessor>,
    shutdown_token: CancellationToken,
    status_tx: watch::Sender<SupervisorStatus>,
    send_projection_tx: watch::Sender<tp::SendProjection>,
    config: ReconcilerConfig,
) {
    let ReconcilerConfig {
        profile,
        clock_origin,
        rtc_recovery,
    } = config;
    tracing::info!(?profile, "🔄 Network event reconciler started");

    let start = clock_origin;
    let mut supervisor = RecoverySupervisor::new(profile);

    if let Some(feed) = &rtc_recovery {
        sync_rtc_signaling_generation(feed, &supervisor);
    }

    let mut timers: HashMap<tp::TimerId, tokio::task::AbortHandle> = HashMap::new();
    let mut effect: Option<RunningEffect> = None;
    let mut last_action_id: Option<u64> = None;
    let mut last_outcome: Option<ObservedOutcome> = None;
    let mut current_monitor_source_epoch: Option<u64> = None;
    let mut rtc_path_observation: Option<RtcPathObservation> = None;
    let mut rtc_path_change_pending = false;

    // Arm the gated bootstrap deadline (no-op under `Ungated`).
    if let Some(op) = supervisor.bootstrap_arm(start.elapsed()) {
        arm_timer(&mut timers, start, &internal_tx, op);
    }

    // Whether the external shutdown token has already been lowered into a
    // `ShutdownRequested` reducer input. External cancellation must not exit
    // the loop directly: that would skip the cleanup obligations and the
    // bounded shutdown deadline the RFC ties to `ShutdownRequested`.
    let mut external_shutdown = false;

    loop {
        tokio::select! {
            biased;
            _ = shutdown_token.cancelled(), if !external_shutdown => {
                external_shutdown = true;
                tracing::info!(
                    consequence = "running bounded teardown before exit",
                    "🛑 Network event reconciler shutdown requested"
                );
                let now = start.elapsed();
                let terminate = handle_supervisor_input(
                    &mut supervisor, tp::Input::ShutdownRequested, now, start, &internal_tx,
                    &mut timers, &mut effect, &processor, &status_tx, &mut last_action_id,
                    &mut last_outcome, &send_projection_tx,
                );
                if let Some(feed) = &rtc_recovery {
                    sync_rtc_signaling_generation(feed, &supervisor);
                }
                if terminate || shutdown_teardown_discharged(&supervisor) {
                    break;
                }
            }
            Some(input) = internal_rx.recv() => {
                let now = start.elapsed();
                let terminate = handle_supervisor_input(
                    &mut supervisor, input, now, start, &internal_tx, &mut timers,
                    &mut effect, &processor, &status_tx, &mut last_action_id, &mut last_outcome,
                    &send_projection_tx,
                );
                if let Some(feed) = &rtc_recovery {
                    sync_rtc_signaling_generation(feed, &supervisor);
                }
                if terminate {
                    tracing::info!("🛑 Supervisor ended at shutdown deadline");
                    break;
                }
                if external_shutdown && shutdown_teardown_discharged(&supervisor) {
                    break;
                }
            }
            Some(request) = event_rx.recv() => {
                let event = request.event.clone();
                let epoch = request.source_epoch;
                let wait_started = Instant::now();
                tracing::debug!(event = ?event, "network_event.reconciler.received");
                if !accept_monitor_source_epoch(
                    &mut current_monitor_source_epoch,
                    &event,
                    epoch,
                ) {
                    tracing::debug!(
                        event = ?event,
                        source_epoch = epoch,
                        current_source_epoch = ?current_monitor_source_epoch,
                        "network_event.reconciler.stale_monitor_incarnation"
                    );
                    let duration_ms = wait_started.elapsed().as_millis() as u64;
                    if request
                        .result_tx
                        .send(NetworkEventResult::success(event, duration_ms))
                        .is_err()
                    {
                        tracing::debug!(
                            "Network event caller dropped before receiving stale-fact acceptance"
                        );
                    }
                    continue;
                }
                processor.prepare_network_event(&event);
                let now = start.elapsed();
                let observed_at = request
                    .observed_at
                    .checked_duration_since(start)
                    .unwrap_or(Duration::ZERO);
                let input = event_to_input(&event, epoch, observed_at);
                let previous_revision = supervisor.view().policy_revision;
                let previous_rtc_path = rtc_path_observation;
                let terminate = handle_supervisor_input(
                    &mut supervisor, input, now, start, &internal_tx, &mut timers,
                    &mut effect, &processor, &status_tx, &mut last_action_id, &mut last_outcome,
                    &send_projection_tx,
                );
                if let Some(feed) = &rtc_recovery {
                    // Close/open the signaling recovery gate before publishing
                    // the network edge. Controllers synchronously reconcile
                    // the watch snapshot before reducing that edge, so
                    // NetworkAvailable cannot race ahead of supervisor-owned
                    // probe/reconnect work.
                    sync_rtc_signaling_generation(feed, &supervisor);
                    if supervisor.view().policy_revision != previous_revision {
                        publish_rtc_network_fact(
                            feed,
                            &event,
                            previous_rtc_path,
                            &mut rtc_path_change_pending,
                            &mut rtc_path_observation,
                        );
                    }
                }
                // Acceptance: reply as soon as the fact is accepted and one
                // synchronous reconcile round completes. The result does not
                // wait for effect completion (RFC-0400 invariant 11).
                let duration_ms = wait_started.elapsed().as_millis() as u64;
                if request.result_tx.send(NetworkEventResult::success(event, duration_ms)).is_err() {
                    tracing::debug!("Network event caller dropped before receiving acceptance");
                }
                if terminate {
                    tracing::info!("🛑 Supervisor ended at shutdown deadline");
                    break;
                }
                if external_shutdown && shutdown_teardown_discharged(&supervisor) {
                    break;
                }
            }
            else => break,
        }
    }

    if external_shutdown {
        tracing::info!(
            recovery_mode = ?supervisor.view().recovery_mode,
            cleanup_work = ?supervisor.view().cleanup_work,
            offline_work = ?supervisor.view().offline_work,
            "🛑 Network event reconciler exiting after bounded teardown"
        );
    }

    for (_, handle) in timers.drain() {
        handle.abort();
    }
    if let Some(running) = effect.take() {
        running.token.cancel();
    }
}

/// Whether an externally requested shutdown has discharged every teardown
/// obligation, so the reconcile loop may exit before the shutdown overall
/// deadline. This is loop-lifecycle bookkeeping, not policy: the obligations,
/// their bounded budgets, and the unconditional deadline were all derived by
/// `translate()` from the injected `ShutdownRequested`; the shell merely stops
/// hosting a supervisor that is `Terminating` with nothing left to tear down.
fn shutdown_teardown_discharged(supervisor: &RecoverySupervisor) -> bool {
    let view = supervisor.view();
    view.recovery_mode == tp::RecoveryModeState::Terminating
        && view.cleanup_work == tp::CleanupWorkState::Idle
        && view.offline_work == tp::OfflineWorkState::Idle
        && view.execution == tp::ExecutionState::Idle
}

fn arm_timer(
    timers: &mut HashMap<tp::TimerId, tokio::task::AbortHandle>,
    start: Instant,
    internal_tx: &mpsc::UnboundedSender<tp::Input>,
    op: TimerOp,
) {
    if let TimerOp::Arm { key, at, fire } = op {
        if let Some(existing) = timers.remove(&key) {
            existing.abort();
        }
        let definition = recovery_timer_definition(key);
        let itx = internal_tx.clone();
        let target = start + at;
        let handle = tokio::spawn(async move {
            crate::timer::sleep_until(definition, target).await;
            let _ = itx.send(fire);
        });
        timers.insert(key, handle.abort_handle());
    }
}

fn recovery_timer_definition(key: tp::TimerId) -> crate::timer::TimerDefinition {
    match key {
        tp::TimerId::OfflineCandidate => crate::timer::ids::RECOVERY_OFFLINE_CANDIDATE,
        tp::TimerId::ShutdownOverall => crate::timer::ids::RECOVERY_SHUTDOWN_OVERALL,
        tp::TimerId::TeardownOverall(tp::TeardownDomain::Cleanup) => {
            crate::timer::ids::RECOVERY_CLEANUP_OBLIGATION
        }
        tp::TimerId::TeardownOverall(tp::TeardownDomain::OfflineDisconnect) => {
            crate::timer::ids::RECOVERY_OFFLINE_OBLIGATION
        }
        tp::TimerId::BootstrapPhase => crate::timer::ids::RECOVERY_BOOTSTRAP_PHASE,
        tp::TimerId::FailureBackoff(tp::RetryDomain::Recovery) => {
            crate::timer::ids::RECOVERY_RETRY_BACKOFF
        }
        tp::TimerId::FailureBackoff(tp::RetryDomain::Cleanup) => {
            crate::timer::ids::RECOVERY_CLEANUP_RETRY_BACKOFF
        }
        tp::TimerId::FailureBackoff(tp::RetryDomain::Offline) => {
            crate::timer::ids::RECOVERY_OFFLINE_RETRY_BACKOFF
        }
    }
}

/// Apply one supervisor input and drive its side effects. Returns `true` when
/// the supervisor has ended unconditionally (shutdown overall deadline) and the
/// reconcile loop must stop.
#[allow(clippy::too_many_arguments)]
#[must_use]
fn handle_supervisor_input(
    supervisor: &mut RecoverySupervisor,
    input: tp::Input,
    now: Duration,
    start: Instant,
    internal_tx: &mpsc::UnboundedSender<tp::Input>,
    timers: &mut HashMap<tp::TimerId, tokio::task::AbortHandle>,
    effect: &mut Option<RunningEffect>,
    processor: &Arc<dyn NetworkEventProcessor>,
    status_tx: &watch::Sender<SupervisorStatus>,
    last_action_id: &mut Option<u64>,
    last_outcome: &mut Option<ObservedOutcome>,
    send_projection_tx: &watch::Sender<tp::SendProjection>,
) -> bool {
    let previous_signaling_reconnect_state = supervisor.signaling_reconnect_state();
    if let tp::Input::EffectCompleted { outcome, .. } = &input {
        *last_outcome = Some(observed_outcome(outcome));
    }
    if let Some((kind, diagnosis)) =
        matching_running_signaling_failure(&previous_signaling_reconnect_state, &input)
    {
        tracing::warn!(
            effect_kind = ?kind,
            connection_error = ?diagnosis.tag(),
            diagnosis_family = ?diagnosis.family(),
            "network_event.signaling.retry_failed"
        );
    }

    let outcome = supervisor.accept(input, now);
    let terminate = outcome.terminate;
    let send_projection = supervisor.send_policy();
    let signaling_reconnect_state = supervisor.signaling_reconnect_state();
    log_signaling_reconnect_transition(
        &previous_signaling_reconnect_state,
        &signaling_reconnect_state,
        now,
    );
    send_projection_tx.send_if_modified(|current| {
        if *current == send_projection {
            false
        } else {
            *current = send_projection;
            true
        }
    });

    tracing::debug!(
        advanced = outcome.advanced,
        revision = supervisor.view().policy_revision,
        send_policy = ?send_projection,
        pending_action = ?supervisor.composite_action(),
        signaling_reconnect_state = ?signaling_reconnect_state,
        "network_event.supervisor.reconciled"
    );

    for op in outcome.timer_ops {
        match op {
            TimerOp::Cancel { key } => {
                if let Some(handle) = timers.remove(&key) {
                    handle.abort();
                }
            }
            arm @ TimerOp::Arm { .. } => arm_timer(timers, start, internal_tx, arm),
        }
    }

    // Lower the policy-derived signaling resource fence to the processor
    // before requesting effect cancellation. A preemption fence must
    // revoke the old effect's commit rights before cooperative cancellation.
    for signal in &outcome.signals {
        match signal {
            tp::SignalingDirective::InvalidateConnectionAttempts => {
                tracing::debug!("network_event.signaling.invalidate_connection_attempts");
                processor.invalidate_signaling_connection_attempts();
            }
        }
    }

    if outcome.cancel_effect
        && let Some(running) = effect.as_ref()
    {
        tracing::debug!(
            action_id = running.action_id,
            "network_event.effect.cancel_requested"
        );
        running.token.cancel();
    }

    for record in &outcome.status {
        match record {
            tp::StatusRecord::RecoveryRejected { mode, reason } => {
                tracing::info!(?mode, ?reason, "network_event.supervisor.recovery_rejected")
            }
            tp::StatusRecord::BackgroundObservationMissing => {
                // The binding layer broke its reporting contract: a foreground
                // transition carried no suspend-aware background duration. The
                // supervisor cannot rule out a long suspension and is forced
                // into a conservative full connection rebuild.
                tracing::error!(
                    consequence = "forced long-background reconnect",
                    "network_event.supervisor.background_observation_missing"
                )
            }
            tp::StatusRecord::BootstrapDeadlineElapsed => {
                tracing::error!("network_event.supervisor.bootstrap_deadline_elapsed")
            }
            tp::StatusRecord::ShutdownAbandon => {
                tracing::warn!("network_event.supervisor.shutdown_abandon")
            }
        }
    }

    // A terminating supervisor starts no further work; its remaining teardown
    // obligations were detached with `Abandoned` residuals in translation.
    if !terminate {
        if let Some(started) = supervisor.maybe_start_effect(now) {
            *last_action_id = Some(started.action_id);
            let token = CancellationToken::new();
            tracing::info!(
                action_id = started.action_id,
                kind = ?started.kind,
                policy_revision = started.captured_revision,
                signaling_reconnect_state = ?supervisor.signaling_reconnect_state(),
                "network_event.effect.started"
            );
            spawn_effect(
                started,
                token.clone(),
                processor.clone(),
                internal_tx.clone(),
            );
            *effect = Some(RunningEffect {
                action_id: started.action_id,
                token,
            });
        } else if supervisor.view().execution == tp::ExecutionState::Idle {
            *effect = None;
        }
    }

    let _ = status_tx.send(SupervisorStatus {
        policy_revision: supervisor.view().policy_revision,
        last_action_id: *last_action_id,
        last_outcome: *last_outcome,
    });

    terminate
}

fn matching_running_signaling_failure<'a>(
    state: &tp::SignalingReconnectState,
    input: &'a tp::Input,
) -> Option<(EffectKind, &'a EffectDiagnosis)> {
    let tp::SignalingReconnectState::Running {
        action_id: running_action_id,
        ..
    } = state
    else {
        return None;
    };
    let tp::Input::EffectCompleted {
        action_id,
        kind,
        outcome: EffectOutcome::Failed { diagnosis },
        ..
    } = input
    else {
        return None;
    };
    (*action_id == *running_action_id).then_some((*kind, diagnosis))
}

fn log_signaling_reconnect_transition(
    previous: &tp::SignalingReconnectState,
    current: &tp::SignalingReconnectState,
    now: Duration,
) {
    if previous == current {
        return;
    }

    match current {
        tp::SignalingReconnectState::BackingOff {
            attempt,
            deadline,
            delay_kind,
            ..
        } => tracing::warn!(
            retry_attempt = *attempt,
            // A zero delay here can mean the deadline already lapsed; say so
            // explicitly instead of letting a saturated subtraction imply
            // "immediately" for an overdue retry.
            retry_overdue = *deadline <= now,
            retry_after_ms = deadline.saturating_sub(now).as_millis() as u64,
            next_retry_at_ms = deadline.as_millis() as u64,
            clock = "supervisor_monotonic",
            delay_kind = ?delay_kind,
            "network_event.signaling.retry_scheduled"
        ),
        tp::SignalingReconnectState::Ready {
            attempt, strength, ..
        } => tracing::info!(
            retry_attempt = *attempt,
            strength = ?strength,
            "network_event.signaling.retry_ready"
        ),
        tp::SignalingReconnectState::Running {
            attempt,
            action_id,
            strength,
            ..
        } => tracing::info!(
            retry_attempt = *attempt,
            action_id = *action_id,
            strength = ?strength,
            "network_event.signaling.retry_running"
        ),
        tp::SignalingReconnectState::Idle => {
            tracing::info!("network_event.signaling.retry_idle")
        }
        tp::SignalingReconnectState::Gated {
            strength, causes, ..
        } => tracing::info!(
            strength = ?strength,
            gate_causes = ?causes,
            "network_event.signaling.retry_gated"
        ),
        tp::SignalingReconnectState::Parked {
            strength, causes, ..
        } => tracing::warn!(
            strength = ?strength,
            park_causes = ?causes,
            "network_event.signaling.retry_parked"
        ),
    }
}

fn spawn_effect(
    started: StartedEffect,
    token: CancellationToken,
    processor: Arc<dyn NetworkEventProcessor>,
    internal_tx: mpsc::UnboundedSender<tp::Input>,
) {
    let action_id = started.action_id;
    let kind = started.kind;
    let policy_revision = started.captured_revision;
    let net_action = network_action_of(started.action);
    let teardown_budget = started.teardown_budget;

    // Outer join monitor: every termination path — normal return, cancellation,
    // or panic/abort — reports exactly one terminal completion.
    tokio::spawn(async move {
        let inner = tokio::spawn(async move {
            match teardown_budget {
                // Teardown kinds run under the bounded-completion contract and
                // report a structured residual outcome, not a plain success.
                Some(budget) => tokio::select! {
                    biased;
                    _ = token.cancelled() => EffectOutcome::Cancelled,
                    report = processor.run_bounded_teardown(net_action, budget) =>
                        teardown_outcome(report),
                },
                None => tokio::select! {
                    biased;
                    _ = token.cancelled() => EffectOutcome::Cancelled,
                    res = processor.process_network_recovery_effect_for_action(
                        net_action,
                        action_id,
                    ) => match res {
                        Ok(report) if report.residuals.is_empty() => EffectOutcome::Succeeded,
                        // A peer-scoped recovery effect that completed its
                        // shared prerequisites but could not cover every
                        // captured target: success-class with one residual per
                        // handed-off or out-of-scope target (RFC-0400
                        // per-peer coverage contract, invariant 33).
                        Ok(report) => EffectOutcome::CompletedWithResiduals {
                            residuals: report.residuals,
                        },
                        Err(error) => EffectOutcome::Failed {
                            diagnosis: error.into_diagnosis(),
                        },
                    }
                },
            }
        });
        let outcome = match inner.await {
            Ok(outcome) => outcome,
            Err(join_err) => EffectOutcome::Aborted {
                cause: if join_err.is_cancelled() {
                    AbortCause::SupervisorCancellation
                } else {
                    AbortCause::PanicOrContractViolation
                },
            },
        };
        let _ = internal_tx.send(tp::Input::EffectCompleted {
            action_id,
            kind,
            policy_revision,
            outcome,
        });
    });
}

/// Map a bounded [`TeardownReport`] onto the effect completion vocabulary.
///
/// Reaching the logical teardown goal is success-class: no residuals →
/// `Succeeded`, residuals → `CompletedWithResiduals`. A deadline that aborted
/// remaining steps before the goal is `Abandoned` (still success-class, so the
/// obligation is extinguished). Otherwise the teardown reports `Failed` so the
/// policy retries inside the obligation deadline. Residual strings are recorded
/// as availability-family `PathUnreachable` diagnostics, which is inside the
/// producible set for teardown kinds.
fn teardown_outcome(report: TeardownReport) -> EffectOutcome {
    let residuals: Vec<EffectDiagnosis> = report
        .residuals
        .iter()
        .map(|stage| EffectDiagnosis::PathUnreachable {
            stage: stage.clone(),
        })
        .collect();

    if report.reached_goal {
        if residuals.is_empty() {
            EffectOutcome::Succeeded
        } else {
            EffectOutcome::CompletedWithResiduals { residuals }
        }
    } else if report.deadline_reached {
        EffectOutcome::Abandoned { residuals }
    } else {
        let stage = report
            .residuals
            .into_iter()
            .next()
            .unwrap_or_else(|| "teardown did not reach its goal".to_string());
        EffectOutcome::Failed {
            diagnosis: EffectDiagnosis::PathUnreachable { stage },
        }
    }
}

/// Network Event Handle
///
/// Lightweight handle for sending network events and receiving processing results.
/// Created before `ActrNode::start()` to bridge platform network events.
pub struct NetworkEventHandle {
    /// Event sender (to ActrNode)
    event_tx: mpsc::Sender<NetworkEventRequest>,
    result_timeout: Duration,
    /// The path-monitor incarnation epoch assigned when this handle is created.
    ///
    /// One handle is one authoritative monitor incarnation; every snapshot it
    /// forwards carries this epoch, and clones share it. A restarted monitor
    /// gets a fresh handle and therefore a newer epoch, so a stale replay from
    /// the previous incarnation is rejected by `(epoch, sequence)` ordering
    /// without any change to the existing sequence semantics callers rely on.
    source_epoch: u64,
    /// Optional fact sink so the upper layer can report a `SessionActivated`
    /// generation commit through this handle when the session/credential layer
    /// is not itself wired to the supervisor.
    fact_sink: Option<SupervisorFactSink>,
}

impl NetworkEventHandle {
    /// Create a new NetworkEventHandle
    pub fn new(event_tx: mpsc::Sender<NetworkEventRequest>) -> Self {
        Self::new_with_result_timeout(event_tx, NETWORK_EVENT_RESULT_TIMEOUT)
    }

    /// Create a new NetworkEventHandle with a custom result timeout.
    ///
    /// Production bindings use [`NetworkEventHandle::new`]. Tests can use this
    /// constructor to verify bounded waiting without sleeping for the full
    /// binding timeout.
    pub fn new_with_result_timeout(
        event_tx: mpsc::Sender<NetworkEventRequest>,
        result_timeout: Duration,
    ) -> Self {
        Self {
            event_tx,
            result_timeout,
            source_epoch: NEXT_NETWORK_SOURCE_EPOCH.fetch_add(1, Ordering::Relaxed),
            fact_sink: None,
        }
    }

    /// Create a new NetworkEventHandle carrying a supervisor fact sink so the
    /// upper layer can report `SessionActivated` through
    /// [`NetworkEventHandle::notify_session_activated`].
    pub(crate) fn new_with_fact_sink(
        event_tx: mpsc::Sender<NetworkEventRequest>,
        fact_sink: SupervisorFactSink,
    ) -> Self {
        Self {
            event_tx,
            result_timeout: NETWORK_EVENT_RESULT_TIMEOUT,
            source_epoch: NEXT_NETWORK_SOURCE_EPOCH.fetch_add(1, Ordering::Relaxed),
            fact_sink: Some(fact_sink),
        }
    }

    /// Create a handle for a newly attached or restarted path monitor.
    ///
    /// The returned handle shares the same supervisor channel and fact sink,
    /// but receives a fresh `source_epoch`; ordinary [`Clone`] keeps the current
    /// epoch and is therefore only for sharing one monitor incarnation.
    pub fn new_monitor_incarnation(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            result_timeout: self.result_timeout,
            source_epoch: NEXT_NETWORK_SOURCE_EPOCH.fetch_add(1, Ordering::Relaxed),
            fact_sink: self.fact_sink.clone(),
        }
    }

    /// Return the path-monitor incarnation carried by this handle.
    ///
    /// This diagnostic identity lets binding tests prove that a newly attached
    /// monitor received a fresh ordering domain instead of an ordinary clone.
    #[doc(hidden)]
    pub fn source_epoch(&self) -> u64 {
        self.source_epoch
    }

    /// Report that a new session generation was authoritatively committed.
    ///
    /// This is the explicit path for an upper layer whose session/credential
    /// owner is not itself wired to the supervisor; the runtime session state
    /// reports the same fact directly at its hard-rebind commit point. Delivery
    /// is non-blocking and best-effort.
    pub fn notify_session_activated(&self, session_generation: u64) {
        match &self.fact_sink {
            Some(sink) => sink.session_activated(session_generation),
            None => tracing::debug!(
                session_generation,
                "network_event.handle.session_activated_no_sink"
            ),
        }
    }

    /// Handle full network path changes.
    pub async fn handle_network_path_changed(
        &self,
        snapshot: NetworkSnapshot,
    ) -> Result<NetworkEventResult, String> {
        self.send_event_and_await_result(NetworkEvent::NetworkPathChanged { snapshot })
            .await
    }

    /// Handle app lifecycle changes.
    ///
    /// The reported background duration is sanitized here, at the single
    /// ingest point, so every downstream consumer sees a bounded value.
    pub async fn handle_app_lifecycle_changed(
        &self,
        state: AppLifecycleState,
    ) -> Result<NetworkEventResult, String> {
        self.send_event_and_await_result(NetworkEvent::AppLifecycleChanged {
            state: state.sanitized(),
        })
        .await
    }

    /// Proactively clean up all connections with a reason. This never reconnects.
    pub async fn cleanup_connections(
        &self,
        reason: CleanupReason,
    ) -> Result<NetworkEventResult, String> {
        self.send_event_and_await_result(NetworkEvent::CleanupConnections { reason })
            .await
    }

    /// Force cleanup and reconnect.
    pub async fn force_reconnect(
        &self,
        reason: ReconnectReason,
    ) -> Result<NetworkEventResult, String> {
        self.send_event_and_await_result(NetworkEvent::ForceReconnect { reason })
            .await
    }

    /// Send event and await result (internal helper)
    async fn send_event_and_await_result(
        &self,
        event: NetworkEvent,
    ) -> Result<NetworkEventResult, String> {
        let event_request_id = NEXT_NETWORK_EVENT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();
        let (result_tx, result_rx) = oneshot::channel();
        let request = NetworkEventRequest {
            event: event.clone(),
            result_tx,
            source_epoch: self.source_epoch,
            observed_at: Instant::now(),
        };

        tracing::info!(
            event_request_id,
            event = ?event,
            result_timeout_ms = self.result_timeout.as_millis() as u64,
            "network_event.handle.enqueue"
        );

        if let Err(e) = self.event_tx.send(request).await {
            let err = format!("Failed to send network event: {}", e);
            tracing::warn!(
                event_request_id,
                event = ?event,
                error = %err,
                "network_event.handle.enqueue_failed"
            );
            return Err(err);
        }

        let result = match crate::timer::timeout(
            crate::timer::ids::NETWORK_EVENT_ACCEPTANCE,
            self.result_timeout,
            result_rx,
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err("Failed to receive network event result".to_string()),
            Err(_) => Err(format!(
                "Timed out waiting for network event result after {}ms",
                self.result_timeout.as_millis()
            )),
        };

        let wait_ms = start.elapsed().as_millis() as u64;
        match &result {
            Ok(result) if result.success => tracing::info!(
                event_request_id,
                event = ?event,
                result_event = ?result.event,
                duration_ms = result.duration_ms,
                wait_ms,
                "network_event.handle.result_received"
            ),
            Ok(result) => tracing::warn!(
                event_request_id,
                event = ?event,
                result_event = ?result.event,
                duration_ms = result.duration_ms,
                wait_ms,
                error = ?result.error,
                "network_event.handle.result_received"
            ),
            Err(e) => tracing::warn!(
                event_request_id,
                event = ?event,
                wait_ms,
                error = %e,
                "network_event.handle.result_failed"
            ),
        }

        result
    }
}

impl Clone for NetworkEventHandle {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            result_timeout: self.result_timeout,
            source_epoch: self.source_epoch,
            fact_sink: self.fact_sink.clone(),
        }
    }
}

#[cfg(test)]
#[path = "network_event_tests.rs"]
mod tests;
