//! Lifecycle management layer (non-architectural layer)
//!
//! Responsible for Actor system lifecycle management:
//! - `node::Inner`: internal running-state struct used by `Node<S>` / `ActrRef`.

pub(crate) mod compat_lock;
pub(crate) mod credential_manager;
pub(crate) mod dedup;
mod heartbeat;
pub(crate) mod hooks;
mod incoming_guard;
mod machine_docs;
pub(crate) mod membership;
mod network_event;
pub(crate) mod node;
pub(crate) mod outstanding_replies;
mod recovery_execution;
mod recovery_policy;
mod recovery_supervisor;
pub(crate) mod reply_deliver;
pub(crate) mod session_state;

#[cfg(test)]
pub(crate) use recovery_policy::translate as test_recovery_policy;
#[cfg(test)]
pub(crate) use recovery_policy::translate::{
    Input as RecoveryInput, SignalingLostCause, SignalingOrigin,
};
pub(crate) use recovery_policy::translate::{RECOVERY_TEARDOWN_DEADLINE, SendProjection};
pub use recovery_supervisor::ConnectionFact;
// `process_network_event_batch` and `select_network_recovery_action` are the
// deprecated legacy batch path; they are still re-exported for the migration
// window, so this re-export intentionally allows the deprecation lint. Only
// the RTC recovery reducer (and its tests) reads the threshold today.
pub use incoming_guard::{
    ACTRIUM_SESSION_V1_ROUTE_PREFIX, IncomingCaller, IncomingDispatchRequest, IncomingGuard,
    IncomingGuardError, IncomingGuardErrorCode, IncomingMetadata, IncomingMetadataError,
    IncomingMetadataErrorKind, IncomingStreamChunk, SessionMethod, SessionRegistry,
    actrium_session_v1_method, route_requires_incoming_guard,
};
pub(crate) use network_event::LONG_BACKGROUND_RECONNECT_THRESHOLD_MS;
#[cfg(feature = "test-utils")]
pub(crate) use network_event::run_network_event_reconciler_with_channel_and_rtc_recovery_and_status;
#[allow(deprecated)]
pub use network_event::{
    AppLifecycleState, CleanupReason, DebounceConfig, DefaultNetworkEventProcessor,
    NetworkAvailability, NetworkEvent, NetworkEventHandle, NetworkEventProcessor,
    NetworkEventRequest, NetworkEventResult, NetworkRecoveryAction, NetworkRecoveryError,
    NetworkSnapshot, NetworkTransportFlags, ObservedOutcome, ReconnectReason,
    SignalingFactLostCause, SignalingFactOrigin, SignalingRecoveryRequestCause, SupervisorFactSink,
    SupervisorStatus, TeardownReport, process_network_event_batch, run_network_event_reconciler,
    run_network_event_reconciler_with_status, select_network_recovery_action,
};
#[cfg(feature = "test-utils")]
pub(crate) use network_event::{
    SupervisorInternalChannel, run_network_event_reconciler_with_channel,
    run_network_event_reconciler_with_channel_and_status, supervisor_internal_channel,
    supervisor_internal_channel_gated,
};
pub use node::CredentialState;
pub use session_state::{SessionPhase, SessionSnapshot, SessionState};
