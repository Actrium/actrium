//! Pure reducer for the per-peer control state machine (RFC §"RtcPeerController").
//!
//! `reduce(&mut RtcPeerState, RtcPeerInput) -> Vec<RtcPeerEffect>` is the single
//! place that decides phase transitions and which side effects to run. It is a
//! pure, synchronous function — blocking WebRTC operations (`build`,
//! `create_offer`, `set_remote_description`, `restart_ice`, `close`) are *not*
//! executed here; they are returned as [`RtcPeerEffect`]s for the effect shell
//! to run on effect tasks, whose completions re-enter as [`EffectCompletion`]s.
//!
//! Identity re-validation (RFC §~283, invariant #5): every completion carries
//! `peer_id`/`pc_generation`/`operation_id`; the reducer only applies a
//! completion or event whose generation still refers to a live session slot
//! (building/active/pending). Stale generations are dropped and counted, never
//! allowed to mutate newer-generation state.

use std::sync::Arc;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCIceCandidateInit, RTCIceConnectionState, RTCPeerConnectionState, RTCSdpType,
    RTCSessionDescription,
};

/// v0.20 can report DataChannel Open before the selected candidate pair has
/// reached the stats accumulator. Retry observation a small bounded number of
/// times; stats never gate sendability or recovery.
pub(super) const MAX_STATS_SAMPLE_ATTEMPTS: u8 = 3;

/// Deadline-driven Soft retries an Offerer may launch within one recovery
/// cycle before persistent failure to converge is treated as proof the
/// restart cannot repair the transport (e.g. the restart rebind silently
/// kept a dead interface, or the driver died rebinding). The same policy the
/// answerer-side retransmission threshold (`ANSWERER_HARD_ESCALATION_ATTEMPT`)
/// applies: escalate to a checked Hard rebuild rather than spin until the
/// cycle budget terminates the attempt.
pub(super) const SOFT_HARD_ESCALATION_ATTEMPT: u32 = 4;

use crate::wire::webrtc::v020::events::{PeerDriverEvent, PeerStateTick};
use crate::wire::webrtc::v020::status::{LaneKind, RtcUnavailableReason};

use super::state::{
    PendingRtcOperation, PendingRtcOperationKind, Role, RtcPeerPhase, RtcPeerState,
};

/// Candidates are naturally a small set, but both signaling and driver input
/// are adversarial/unbounded over time. Keep attempt-owned staging caches
/// finite even when candidates for future ICE generations never become usable.
const MAX_BUFFERED_ICE_CANDIDATES: usize = 256;

/// Business / manager control commands (bounded mpsc into the controller).
///
/// `Debug` is manual: the S6-2 media variants carry `Arc<dyn …>` trait objects
/// that have no `Debug` impl.
pub(crate) enum RtcPeerCommand {
    /// Ensure the peer is connected; initiates the offerer flow from `Idle`.
    Connect,
    /// Ensure the peer is connected and acknowledge the transport epoch after
    /// the Controller has reduced the command and published its snapshot.
    /// Transport callers use this boundary so the previous attempt's terminal
    /// watch replay cannot be mistaken for the new attempt's result.
    ConnectCommitted {
        respond: tokio::sync::oneshot::Sender<u64>,
    },
    /// Cancel only the exact initial transport attempt whose caller deadline
    /// elapsed. A newer retry epoch is never affected by stale cancellation.
    AbortTransportAttempt { transport_epoch: u64 },
    /// A remote SDP offer or answer arrived (from the signaling router; the
    /// Stage 2 test harness injects it directly). Boxed because
    /// `RTCSessionDescription` is large and this enum is on the hot command path.
    ApplyRemoteDescription(Box<RTCSessionDescription>),
    /// Production signaling path for an ordinary Offer/Answer, preserving the
    /// protocol's end-to-end SDP exchange identity. The legacy uncorrelated
    /// variant above remains as a compatibility/test seam.
    ApplyRemoteDescriptionCorrelated {
        desc: Box<RTCSessionDescription>,
        exchange_id: Option<String>,
    },
    /// An explicitly tagged ICE-restart offer. Unlike a fresh initial Offer
    /// with new ICE credentials, this is applied to the established answerer
    /// PeerConnection and advances only the ICE generation.
    ApplyRemoteIceRestartOffer(Box<RTCSessionDescription>),
    ApplyRemoteIceRestartOfferCorrelated {
        desc: Box<RTCSessionDescription>,
        exchange_id: Option<String>,
    },
    /// Correlated answerer request asking the offerer to start/retry the ICE
    /// restart offer flow.
    ApplyIceRestartRequest { request_id: String, attempt: u32 },
    /// The same request, with a response carrying the outcome committed by
    /// this controller's reducer. The signaling router uses this variant so
    /// it never classifies the request from a stale snapshot.
    ApplyIceRestartRequestCommitted {
        request_id: String,
        attempt: u32,
        respond: tokio::sync::oneshot::Sender<i32>,
    },
    /// Acknowledgement for a correlated answerer restart request. A negative
    /// outcome means the requester's current recovery attempt must start a
    /// fresh role negotiation instead of waiting forever for an Offer.
    ApplyIceRestartRequestAck {
        request_id: String,
        attempt: u32,
        outcome: i32,
    },
    /// A remote ICE candidate arrived.
    ApplyRemoteCandidate(RTCIceCandidateInit),
    /// The signaling router failed to commit critical SDP for one
    /// transport/PC attempt. The transport epoch and PC generation fence a
    /// delayed failure from a retired attempt; the controller owns the
    /// resulting retry/terminal policy.
    SignalingDeliveryFailed {
        transport_epoch: u64,
        pc_generation: u64,
    },
    /// The signaling server assigned this pair's role (`is_offerer`). Routed by
    /// the `SignalingRouter` from the server's `RoleAssignment` (RFC §"Role and
    /// glare handling").
    ApplyRoleAssignment { is_offerer: bool },
    /// Actrix confirmed that the exact remote endpoint had no usable
    /// signaling session when this Controller/transport/PC attempt was relayed.
    /// This is a point-in-time routing fact, not proof that the ActrId is
    /// permanently gone: a mobile runtime can reconnect with the same identity
    /// after a background suspension. For a generation-0 role probe,
    /// `negotiation_epoch` names the exact probe emission so delayed results are
    /// still observable as stale.
    TargetUnavailable {
        transport_epoch: u64,
        pc_generation: u64,
        negotiation_epoch: u64,
    },
    /// Lazily create a lane on the active session (first send on a lane other
    /// than the pre-created `RpcReliable` anchor). Data-plane request from the
    /// send path; the controller dedups against the lane registry.
    EnsureLane(LaneKind),
    /// Explicitly close this peer (terminal; drives the close barrier).
    Close,
    /// A network/app lifecycle fact from the `RecoverySupervisor` (Stage 5).
    NetworkEvent(RtcNetworkEvent),
    /// S5-6: refresh the ICE server / TURN credential configuration on the
    /// active session (RFC §"Soft Recovery": "TURN credential/config updates
    /// can take effect via the current PC"). A rejected `set_configuration()`
    /// change escalates to Hard Recovery.
    RefreshConfiguration {
        config: Box<webrtc::peer_connection::RTCConfiguration>,
        /// Present for business/API commands. The Controller retains this
        /// sender until the exact effect completion is authenticated; direct
        /// reducer tests use `None`.
        respond: Option<tokio::sync::oneshot::Sender<Result<(), RtcConfigurationRefreshError>>>,
    },
    /// S6-2: add a local media track on the active session (media topology).
    /// Executed by the controller shell, which owns the response channel and
    /// bypasses the reducer; the no-op arm below keeps direct-drive tests
    /// exhaustive. The resulting `NegotiationNeeded` driver event drives the
    /// renegotiation offer through the normal effect path.
    AddTrack {
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
        respond: tokio::sync::oneshot::Sender<
            Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, String>,
        >,
    },
    /// S6-2: remove a previously added local track by its sender.
    RemoveTrack {
        sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
        respond: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
}

impl std::fmt::Debug for RtcPeerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect => f.write_str("Connect"),
            Self::ConnectCommitted { .. } => f.write_str("ConnectCommitted(..)"),
            Self::AbortTransportAttempt { transport_epoch } => f
                .debug_struct("AbortTransportAttempt")
                .field("transport_epoch", transport_epoch)
                .finish(),
            Self::ApplyRemoteDescription(_) => f.write_str("ApplyRemoteDescription(..)"),
            Self::ApplyRemoteDescriptionCorrelated { exchange_id, .. } => f
                .debug_struct("ApplyRemoteDescriptionCorrelated")
                .field("exchange_id", exchange_id)
                .finish(),
            Self::ApplyRemoteIceRestartOffer(_) => f.write_str("ApplyRemoteIceRestartOffer(..)"),
            Self::ApplyRemoteIceRestartOfferCorrelated { exchange_id, .. } => f
                .debug_struct("ApplyRemoteIceRestartOfferCorrelated")
                .field("exchange_id", exchange_id)
                .finish(),
            Self::ApplyIceRestartRequest {
                request_id,
                attempt,
            } => f
                .debug_struct("ApplyIceRestartRequest")
                .field("request_id", request_id)
                .field("attempt", attempt)
                .finish(),
            Self::ApplyIceRestartRequestCommitted {
                request_id,
                attempt,
                ..
            } => f
                .debug_struct("ApplyIceRestartRequestCommitted")
                .field("request_id", request_id)
                .field("attempt", attempt)
                .finish(),
            Self::ApplyIceRestartRequestAck {
                request_id,
                attempt,
                outcome,
            } => f
                .debug_struct("ApplyIceRestartRequestAck")
                .field("request_id", request_id)
                .field("attempt", attempt)
                .field("outcome", outcome)
                .finish(),
            Self::ApplyRemoteCandidate(_) => f.write_str("ApplyRemoteCandidate(..)"),
            Self::SignalingDeliveryFailed {
                transport_epoch,
                pc_generation,
            } => f
                .debug_struct("SignalingDeliveryFailed")
                .field("transport_epoch", transport_epoch)
                .field("pc_generation", pc_generation)
                .finish(),
            Self::ApplyRoleAssignment { is_offerer } => f
                .debug_struct("ApplyRoleAssignment")
                .field("is_offerer", is_offerer)
                .finish(),
            Self::TargetUnavailable {
                transport_epoch,
                pc_generation,
                negotiation_epoch,
            } => f
                .debug_struct("TargetUnavailable")
                .field("transport_epoch", transport_epoch)
                .field("pc_generation", pc_generation)
                .field("negotiation_epoch", negotiation_epoch)
                .finish(),
            Self::EnsureLane(lane) => f.debug_tuple("EnsureLane").field(lane).finish(),
            Self::Close => f.write_str("Close"),
            Self::NetworkEvent(event) => f.debug_tuple("NetworkEvent").field(event).finish(),
            Self::RefreshConfiguration { .. } => f.write_str("RefreshConfiguration(..)"),
            Self::AddTrack { .. } => f.write_str("AddTrack(..)"),
            Self::RemoveTrack { .. } => f.write_str("RemoveTrack(..)"),
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub(crate) enum RtcConfigurationRefreshError {
    #[error("RTC peer has no active configuration target")]
    ConnectionNotReady,
    #[error("RTC configuration refresh was rejected: {0}")]
    Rejected(String),
    #[error("RTC configuration refresh was cancelled by peer shutdown")]
    Closed,
    #[error("RTC configuration refresh could not be submitted: {0}")]
    EffectPort(String),
    #[error("peer controller is no longer running")]
    ControllerGone,
}

/// Re-export for test ergonomics: tests that `use reducer::*` still see
/// `RtcNetworkEvent` without a separate import (the type lives in
/// `recovery.rs` now — its semantic home is the supervisor that produces it).
pub(crate) use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;

/// Readiness/liveness facts reported by a DataChannel pump (NOT the
/// `PeerConnectionEventHandler`). The `RpcReliable` open is the peer-level
/// readiness anchor that gates `Sendable`.
#[derive(Debug)]
pub(crate) enum LaneEvent {
    Opened { generation: u64, lane: LaneKind },
    Closed { generation: u64, lane: LaneKind },
}

/// Unified reducer input: every fact the controller reacts to.
#[derive(Debug)]
pub(crate) enum RtcPeerInput {
    Command(RtcPeerCommand),
    /// A structural driver event (candidate / data channel / track / etc.).
    DriverEvent(PeerDriverEvent),
    /// The coalesced latest mergeable state (PC/ICE), delivered at its newest
    /// value (O1; see `events::PeerStateTick`).
    StateTick(Box<PeerStateTick>),
    /// A DataChannel pump readiness/liveness report.
    Lane(LaneEvent),
    /// The readiness DataChannel could not be created for this generation.
    /// Without the anchor the attempt can never become Sendable, so fail the
    /// exact negotiation instead of continuing with an unusable SDP offer.
    LaneCreationFailed {
        generation: u64,
        lane: LaneKind,
    },
    /// An inline track mutation (add/remove) timed out mid-operation. webrtc
    /// 0.20.3 commits transceiver changes before later await points, so the
    /// peer connection may already hold a mutation the controller's registry
    /// does not track; the session state is untrustworthy and the generation
    /// must fail negotiation instead of serving later offers.
    TrackMutationTimedOut {
        generation: u64,
    },
    /// A blocking effect's completion, re-validated by identity before applying.
    Completion(EffectCompletion),
    /// O1: the structural-event mailbox overflowed for `generation`; treat the
    /// generation as unhealthy and hard-recover rather than silently dropping
    /// critical events (RFC §487-489).
    MailboxOverflow {
        generation: u64,
    },
    /// Exact recovery deadline emitted by the controller's policy timer. The
    /// epoch identity prevents a stale timer from escalating a newer recovery.
    RecoveryDeadlineElapsed {
        recovery_epoch: u64,
    },
    /// End-to-end recovery budget elapsed. `recovery_cycle` is stable across
    /// per-attempt epochs and Hard escalation, so retry traffic cannot re-arm
    /// this deadline.
    RecoveryBudgetElapsed {
        recovery_cycle: u64,
    },
    /// Exact role-negotiation deadline emitted by the controller. The
    /// negotiation epoch prevents a delayed timer from failing a newer role
    /// arbitration attempt.
    RoleNegotiationDeadlineElapsed {
        negotiation_epoch: u64,
    },
    /// End-to-end deadline for one never-sendable logical transport attempt.
    /// The transport epoch fences a delayed timer from terminating a retry.
    InitialConnectionDeadlineElapsed {
        transport_epoch: u64,
    },
}

/// Side effects for the effect shell to run. Blocking variants carry a
/// `pc_generation` and `operation_id` so their completions can be re-validated.
#[derive(Debug)]
pub(crate) enum RtcPeerEffect {
    BuildSession {
        generation: u64,
        operation_id: u64,
    },
    CreateOffer {
        generation: u64,
        operation_id: u64,
    },
    CreateAnswer {
        generation: u64,
        operation_id: u64,
    },
    /// Pre-create a data-channel lane on the session (the offerer's `RpcReliable`
    /// readiness anchor, before the offer so it is negotiated in the SDP).
    CreateLane {
        generation: u64,
        lane: LaneKind,
    },
    SetLocalDescription {
        generation: u64,
        operation_id: u64,
        desc: Box<RTCSessionDescription>,
    },
    SetRemoteDescription {
        generation: u64,
        operation_id: u64,
        desc: Box<RTCSessionDescription>,
    },
    AddIceCandidate {
        generation: u64,
        operation_id: u64,
        ice_generation: u64,
        candidate: RTCIceCandidateInit,
    },
    RestartIce {
        generation: u64,
        operation_id: u64,
        ice_generation: u64,
    },
    /// S5-6: refresh the ICE server / TURN credential configuration on the
    /// active session in-place (RFC §"RefreshConfiguration"). A rejected
    /// `set_configuration()` change escalates to Hard Recovery.
    RefreshConfiguration {
        generation: u64,
        operation_id: u64,
        config: Box<webrtc::peer_connection::RTCConfiguration>,
    },
    /// S6-3: sample `get_stats()` once to resolve the selected candidate pair
    /// into the snapshot's `network_path`. Emitted on entering Sendable.
    GetStats {
        generation: u64,
        operation_id: u64,
    },
    CloseSession {
        generation: u64,
        operation_id: u64,
    },
    /// Hand a local offer/answer/candidate to the signaling router (no
    /// completion; fire-and-forget).
    SendSignaling {
        generation: u64,
        message: SignalingMessage,
    },
}

impl RtcPeerEffect {
    /// Identity of a blocking effect that must produce exactly one completion.
    /// Inline/fire-and-forget effects intentionally return `None`.
    pub(crate) fn pending_identity(
        &self,
    ) -> Option<(u64, u64, Option<u64>, PendingRtcOperationKind)> {
        let identity = match self {
            Self::BuildSession {
                generation,
                operation_id,
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::BuildSession,
            ),
            Self::CreateOffer {
                generation,
                operation_id,
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::CreateOffer,
            ),
            Self::CreateAnswer {
                generation,
                operation_id,
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::CreateAnswer,
            ),
            Self::SetLocalDescription {
                generation,
                operation_id,
                ..
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::SetLocalDescription,
            ),
            Self::SetRemoteDescription {
                generation,
                operation_id,
                ..
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::SetRemoteDescription,
            ),
            Self::AddIceCandidate {
                generation,
                operation_id,
                ice_generation,
                ..
            } => (
                *generation,
                *operation_id,
                Some(*ice_generation),
                PendingRtcOperationKind::AddIceCandidate,
            ),
            Self::RestartIce {
                generation,
                operation_id,
                ice_generation,
            } => (
                *generation,
                *operation_id,
                Some(*ice_generation),
                PendingRtcOperationKind::RestartIce,
            ),
            Self::RefreshConfiguration {
                generation,
                operation_id,
                ..
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::RefreshConfiguration,
            ),
            Self::GetStats {
                generation,
                operation_id,
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::GetStats,
            ),
            Self::CloseSession {
                generation,
                operation_id,
            } => (
                *generation,
                *operation_id,
                None,
                PendingRtcOperationKind::CloseSession,
            ),
            Self::CreateLane { .. } | Self::SendSignaling { .. } => return None,
        };
        Some(identity)
    }
}

/// A signaling message destined for the remote peer (via the router). The SDP
/// payloads are boxed because `RTCSessionDescription` is large. `RoleNegotiation`
/// is sent to the signaling server (outbound); `RoleAssignment` comes back from
/// it (inbound) — both ride the same wire payload family.
#[derive(Debug, Clone)]
pub(crate) enum SignalingMessage {
    Offer {
        desc: Box<RTCSessionDescription>,
        exchange_id: Option<String>,
    },
    /// Correlated ICE-restart offer. The wire distinction is required so an
    /// answerer that no longer owns the old Session can reject it instead of
    /// silently treating it as a fresh initial offer.
    IceRestartOffer {
        desc: Box<RTCSessionDescription>,
        exchange_id: String,
    },
    IceRestartRequest {
        request_id: String,
        attempt: u32,
    },
    /// Receiver-side acknowledgement for an `IceRestartRequest`. This keeps
    /// the v0.20 wire contract aligned with the pre-542 Coordinator path.
    IceRestartRequestAck {
        request_id: String,
        attempt: u32,
        outcome: i32,
    },
    Answer {
        desc: Box<RTCSessionDescription>,
        exchange_id: Option<String>,
    },
    Candidate(RTCIceCandidateInit),
    /// Ask the signaling server to arbitrate this pair's role (RFC §"Role and
    /// glare handling"); the server replies with a `RoleAssignment` to both ends.
    RoleNegotiation,
    /// The server's role arbitration for this pair (inbound).
    RoleAssignment {
        is_offerer: bool,
    },
    IceRestartRejected {
        exchange_id: String,
        reason: i32,
    },
}

impl SignalingMessage {
    /// Stable traffic-class label for dispatch diagnostics.
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Offer { .. } => "offer",
            Self::IceRestartOffer { .. } => "ice_restart_offer",
            Self::IceRestartRequest { .. } => "ice_restart_request",
            Self::IceRestartRequestAck { .. } => "ice_restart_request_ack",
            Self::Answer { .. } => "answer",
            Self::Candidate(_) => "candidate",
            Self::RoleNegotiation => "role_negotiation",
            Self::RoleAssignment { .. } => "role_assignment",
            Self::IceRestartRejected { .. } => "ice_restart_rejected",
        }
    }
}

/// The outcome of a blocking effect, fed back into the controller. Every
/// variant carries the `pc_generation` (and `operation_id` where relevant) the
/// effect was issued with, so the reducer can re-validate identity.
pub(crate) enum EffectCompletion {
    /// The production effect runner owns and registers the freshly built
    /// Session before reporting success.  The Controller receives only the
    /// semantic outcome and never a WebRTC/session implementation object.
    BuildSession {
        generation: u64,
        operation_id: u64,
        result: Result<(), RtcError>,
    },
    CreateOffer {
        generation: u64,
        operation_id: u64,
        result: Result<Box<RTCSessionDescription>, RtcError>,
    },
    CreateAnswer {
        generation: u64,
        operation_id: u64,
        result: Result<Box<RTCSessionDescription>, RtcError>,
    },
    SetLocalDescription {
        generation: u64,
        operation_id: u64,
        result: Result<(), RtcError>,
    },
    SetRemoteDescription {
        generation: u64,
        operation_id: u64,
        result: Result<(), RtcError>,
    },
    AddIceCandidate {
        generation: u64,
        operation_id: u64,
        ice_generation: u64,
        result: Result<(), RtcError>,
    },
    RestartIce {
        generation: u64,
        operation_id: u64,
        ice_generation: u64,
        result: Result<(), RtcError>,
    },
    /// S5-6: the outcome of a `RefreshConfiguration` effect. A rejected
    /// `set_configuration()` change escalates to Hard Recovery.
    RefreshConfiguration {
        generation: u64,
        operation_id: u64,
        result: Result<(), RtcError>,
    },
    /// S6-3: the outcome of a `GetStats` effect; the report resolves the
    /// selected candidate pair into `network_path`. Best-effort: an
    /// unresolvable report leaves the previous path untouched.
    GetStats {
        generation: u64,
        operation_id: u64,
        report: webrtc::peer_connection::RTCStatsReport,
    },
    CloseSession {
        generation: u64,
        operation_id: u64,
        result: Result<(), RtcError>,
    },
}

impl EffectCompletion {
    /// The PC generation this completion belongs to.
    pub(crate) fn generation(&self) -> u64 {
        match self {
            Self::BuildSession { generation, .. }
            | Self::CreateOffer { generation, .. }
            | Self::CreateAnswer { generation, .. }
            | Self::SetLocalDescription { generation, .. }
            | Self::SetRemoteDescription { generation, .. }
            | Self::AddIceCandidate { generation, .. }
            | Self::RestartIce { generation, .. }
            | Self::RefreshConfiguration { generation, .. }
            | Self::GetStats { generation, .. }
            | Self::CloseSession { generation, .. } => *generation,
        }
    }

    /// Operation identity echoed by the effect runner.
    pub(crate) fn operation_id(&self) -> u64 {
        match self {
            Self::BuildSession { operation_id, .. }
            | Self::CreateOffer { operation_id, .. }
            | Self::CreateAnswer { operation_id, .. }
            | Self::SetLocalDescription { operation_id, .. }
            | Self::SetRemoteDescription { operation_id, .. }
            | Self::AddIceCandidate { operation_id, .. }
            | Self::RestartIce { operation_id, .. }
            | Self::RefreshConfiguration { operation_id, .. }
            | Self::GetStats { operation_id, .. }
            | Self::CloseSession { operation_id, .. } => *operation_id,
        }
    }

    pub(crate) fn ice_generation(&self) -> Option<u64> {
        match self {
            Self::AddIceCandidate { ice_generation, .. }
            | Self::RestartIce { ice_generation, .. } => Some(*ice_generation),
            _ => None,
        }
    }

    pub(crate) fn kind(&self) -> PendingRtcOperationKind {
        match self {
            Self::BuildSession { .. } => PendingRtcOperationKind::BuildSession,
            Self::CreateOffer { .. } => PendingRtcOperationKind::CreateOffer,
            Self::CreateAnswer { .. } => PendingRtcOperationKind::CreateAnswer,
            Self::SetLocalDescription { .. } => PendingRtcOperationKind::SetLocalDescription,
            Self::SetRemoteDescription { .. } => PendingRtcOperationKind::SetRemoteDescription,
            Self::AddIceCandidate { .. } => PendingRtcOperationKind::AddIceCandidate,
            Self::RestartIce { .. } => PendingRtcOperationKind::RestartIce,
            Self::RefreshConfiguration { .. } => PendingRtcOperationKind::RefreshConfiguration,
            Self::GetStats { .. } => PendingRtcOperationKind::GetStats,
            Self::CloseSession { .. } => PendingRtcOperationKind::CloseSession,
        }
    }
}

impl std::fmt::Debug for EffectCompletion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuildSession {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("BuildSession")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::CreateOffer {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("CreateOffer")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::CreateAnswer {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("CreateAnswer")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::SetLocalDescription {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("SetLocalDescription")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::SetRemoteDescription {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("SetRemoteDescription")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::AddIceCandidate {
                generation,
                operation_id,
                ice_generation,
                result,
            } => f
                .debug_struct("AddIceCandidate")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ice_generation", ice_generation)
                .field("ok", &result.is_ok())
                .finish(),
            Self::RestartIce {
                generation,
                operation_id,
                ice_generation,
                result,
            } => f
                .debug_struct("RestartIce")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ice_generation", ice_generation)
                .field("ok", &result.is_ok())
                .finish(),
            Self::RefreshConfiguration {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("RefreshConfiguration")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
            Self::GetStats {
                generation,
                operation_id,
                ..
            } => f
                .debug_struct("GetStats")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .finish(),
            Self::CloseSession {
                generation,
                operation_id,
                result,
            } => f
                .debug_struct("CloseSession")
                .field("generation", generation)
                .field("operation_id", operation_id)
                .field("ok", &result.is_ok())
                .finish(),
        }
    }
}

/// The pure reducer. Mutates `state` and returns the effects to run.
pub(crate) fn reduce(state: &mut RtcPeerState, input: RtcPeerInput) -> Vec<RtcPeerEffect> {
    let mut effects = Vec::new();
    let was_sendable = state.phase == RtcPeerPhase::Sendable;
    match input {
        RtcPeerInput::Command(command) => reduce_command(state, command, &mut effects),
        RtcPeerInput::DriverEvent(event) => reduce_driver_event(state, event, &mut effects),
        RtcPeerInput::StateTick(tick) => reduce_state_tick(state, &tick, &mut effects),
        RtcPeerInput::Lane(event) => reduce_lane_event(state, event, &mut effects),
        RtcPeerInput::LaneCreationFailed { generation, lane } => {
            if lane == LaneKind::READINESS_ANCHOR
                && state.is_live_generation(generation)
                && !state.close_requested
            {
                negotiation_failed(state, &mut effects);
            }
        }
        RtcPeerInput::TrackMutationTimedOut { generation } => {
            // The peer connection may hold a half-applied transceiver
            // mutation the registry cannot account for.
            if state.is_live_generation(generation) && !state.close_requested {
                // `ever_connected` alone is not the right lifecycle fact: a
                // make-before-break hard recovery calls
                // `begin_transport_attempt()`, which resets it while the old
                // active generation is still live. Branch on the presence of
                // a live session instead, so a transient mutation timeout on
                // that old generation still forces a replacement generation
                // rather than terminating the whole attempt.
                if state.active_generation.is_some() || state.ever_connected {
                    // Soft recovery on the same generation would serve later
                    // offers from desynchronized transceiver state; force a
                    // replacement generation, as for an unusable session.
                    start_hard_recovery(state, &mut effects);
                } else {
                    terminate_transport_attempt(
                        state,
                        RtcUnavailableReason::NegotiationFailed,
                        &mut effects,
                    );
                }
            }
        }
        RtcPeerInput::Completion(completion) => {
            if authenticate_completion(state, &completion) {
                reduce_completion(state, completion, &mut effects)
            }
        }
        RtcPeerInput::MailboxOverflow { generation } => {
            reduce_mailbox_overflow(state, generation, &mut effects)
        }
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch } => {
            if state.recovery_inflight && state.recovery_epoch == recovery_epoch {
                if (state.address_changed
                    || state.awaiting_offer
                    || state.active_generation.is_none()
                    || state.role != Role::Answerer)
                    && (state.pending_generation.is_some() || state.ice_generation == 0)
                    && matches!(
                        state.phase,
                        RtcPeerPhase::Negotiating | RtcPeerPhase::TransportReady
                    )
                {
                    // Negotiating/TransportReady are shared by Soft and Hard
                    // recovery. A pending generation, or generation-local ICE
                    // epoch zero after a break-before-make build, identifies
                    // this as a Hard replacement attempt.
                    // A Hard replacement built successfully but its signaling,
                    // SDP, transport, or readiness anchor did not converge in
                    // the attempt window. Retire the non-authoritative
                    // replacement and start the next generation. During
                    // make-before-break the old active generation remains only
                    // as a physical fallback; break-before-make has the failed
                    // replacement in the active slot, so retire that one too.
                    if let Some(generation) = state.pending_generation {
                        retire_generation(state, generation, &mut effects);
                    } else if let Some(generation) = state.active_generation {
                        retire_generation(state, generation, &mut effects);
                    }
                    clear_negotiation_for_recovery(state);
                    state.pending_pc_connected = false;
                    state.pending_rpc_reliable_open = false;
                    state.recovery_attempt_started = false;
                    start_hard_recovery(state, &mut effects);
                } else if state.address_changed {
                    start_hard_recovery(state, &mut effects);
                } else if state.role == Role::Answerer
                    && state.active_generation.is_some()
                    && !state.recovery_negotiated
                    && !state.awaiting_offer
                    && !state.address_changed
                    && !answerer_parked(state)
                {
                    // Lost request/offer: retransmit the same correlated
                    // request and move to a new attempt epoch so the controller
                    // arms one fresh per-attempt deadline. The attempt counter
                    // is observability only; the stable recovery-cycle budget
                    // is the sole bound. Do not escalate to Hard Recovery just
                    // because a request has timed out: an Answerer cannot
                    // create the restart offer and a new PC would churn while
                    // waiting for the Offerer to wake up.
                    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                    // Intermediate parked signal: one warn per retransmission
                    // attempt distinguishes a parked-but-live answerer from a
                    // silently dead one before the terminal recovery-budget
                    // cleanup exhausts.
                    tracing::warn!(
                        peer = %state.peer_id.to_string_repr(),
                        recovery_cycle = state.recovery_cycle,
                        attempt = state.local_restart_request_attempt + 1,
                        parked = false,
                        "answerer recovery attempt elapsed; retransmitting restart request"
                    );
                    send_answerer_restart_request(state, &mut effects);
                } else if state.role == Role::Answerer
                    && state.active_generation.is_some()
                    && state.recovery_negotiated
                    && state.recovery_attempt_started
                    && !state.awaiting_offer
                    && !state.address_changed
                {
                    // A restart Offer/Answer has completed while the physical
                    // readiness anchor is still unavailable. Requiring
                    // `recovery_negotiated` keeps this branch out of the
                    // pre-SDP/ICE timeout path. This is a failed or otherwise
                    // non-sendable Answerer session,
                    // not a successful recovery: keep the same request
                    // identity and ask the Offerer for the next attempt. Do
                    // not park through `start_hard_recovery`, because that
                    // path clears the request identity and turns every retry
                    // back into attempt 1.
                    state.recovery_negotiated = false;
                    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                    tracing::warn!(
                        peer = %state.peer_id.to_string_repr(),
                        recovery_cycle = state.recovery_cycle,
                        generation = state.active_generation,
                        recovery_epoch = state.recovery_epoch,
                        attempt = state.local_restart_request_attempt + 1,
                        parked = false,
                        "answerer recovery negotiated without readiness; retransmitting restart request"
                    );
                    send_answerer_restart_request(state, &mut effects);
                } else if state.role == Role::Offerer
                    && state.active_generation.is_some()
                    && matches!(
                        state.phase,
                        RtcPeerPhase::Recovering
                            | RtcPeerPhase::Negotiating
                            | RtcPeerPhase::TransportReady
                    )
                {
                    // Preserve the 0.14 Offerer recovery policy: a temporary
                    // network loss gets multiple ICE-restart attempts on the
                    // existing PeerConnection. The stable recovery-cycle timer
                    // remains the overall 60-second bound, while this new
                    // attempt epoch gives the controller a fresh per-attempt
                    // restart timer.
                    // Hard Recovery is reserved for an unusable/closed
                    // Session, the long-background lifecycle policy, or
                    // another explicit hard-failure fact instead of being the
                    // first timeout response.
                    //
                    // The Answerer follows the same stable recovery-cycle
                    // budget while it still has a live physical Session: it
                    // retransmits the correlated restart request until the
                    // 60-second budget expires. Once it must replace that
                    // Session, it parks and waits for the Offerer instead of
                    // creating a local Hard-Recovery generation.
                    //
                    // The soft ladder is bounded: retries that keep failing
                    // to converge prove the restart is not repairing the
                    // transport (e.g. the restart rebind silently kept a dead
                    // interface, or the driver died rebinding and every
                    // further restart_ice is a no-op). Escalate to a checked
                    // Hard rebuild — the build-time address validation and
                    // fresh gather are exactly the repair a wedged generation
                    // needs — rather than spin until the cycle budget
                    // terminates the attempt.
                    //
                    // Escalating ends the old generation's restart cadence:
                    // make-before-break keeps it as a passive fallback, but
                    // no further restart_ice is issued on it while the
                    // replacement builds. That break is intentional — two
                    // concurrent restart/offer flights corrupt the v0.20
                    // signaling state machine, and a generation that failed
                    // the ladder has already had its soft chances; the
                    // remaining budget is better spent on the strictly
                    // stronger rebuild.
                    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                    state.recovery_attempt_started = false;
                    state.soft_recovery_attempts = state.soft_recovery_attempts.saturating_add(1);
                    if state.soft_recovery_attempts >= SOFT_HARD_ESCALATION_ATTEMPT {
                        start_hard_recovery(state, &mut effects);
                    } else {
                        start_soft_recovery(state, &mut effects);
                    }
                } else if state.role == Role::Answerer && state.awaiting_offer {
                    // The Answerer has no local SDP with which to create a
                    // replacement. Its recovery is deliberately parked until
                    // the Offerer starts a fresh physical connection; no
                    // deadline may turn this passive wait into hard-recovery
                    // churn. BUT a parked wait must not go silent: the offerer
                    // only escalates to Hard Recovery on persistent
                    // retransmission, and an answerer that never retransmits
                    // strands both sides (the offerer stays on its healthy
                    // generation while this side exhausts its budget). Each
                    // per-attempt deadline therefore re-arms the attempt epoch
                    // and retransmits the correlated restart request, driving
                    // the offerer's attempt counter toward the escalation
                    // threshold.
                    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                    state.recovery_attempt_started = false;
                    tracing::warn!(
                        peer = %state.peer_id.to_string_repr(),
                        recovery_cycle = state.recovery_cycle,
                        attempt = state.local_restart_request_attempt + 1,
                        parked = true,
                        "answerer parked awaiting offerer; retransmitting restart request"
                    );
                    send_answerer_restart_request(state, &mut effects);
                } else {
                    start_hard_recovery(state, &mut effects);
                }
            }
        }
        RtcPeerInput::RecoveryBudgetElapsed { recovery_cycle } => {
            if state.recovery_inflight && state.recovery_cycle == recovery_cycle {
                exhaust_recovery(state, &mut effects);
            }
        }
        RtcPeerInput::RoleNegotiationDeadlineElapsed { negotiation_epoch } => {
            if state.role_negotiation_inflight && state.negotiation_epoch == negotiation_epoch {
                // The bounded wait the connection budget exposes — it must
                // fire even when the probe was never deliverable. When the
                // gate later opens, `role_probe_undelivered` lets the
                // gate-opening edge resurrect this attempt (see
                // `SignalingGenerationChanged`).
                terminate_transport_attempt(
                    state,
                    RtcUnavailableReason::NegotiationFailed,
                    &mut effects,
                );
            }
        }
        RtcPeerInput::InitialConnectionDeadlineElapsed { transport_epoch } => {
            if state.initial_connection_inflight
                && state.transport_epoch == transport_epoch
                && state.phase != RtcPeerPhase::Idle
                && !state.close_requested
            {
                terminate_transport_attempt(
                    state,
                    RtcUnavailableReason::NegotiationFailed,
                    &mut effects,
                );
            }
        }
    }
    // S6-3: on *entering* Sendable (initial connect, recovery, or the pending
    // switch), start a bounded stats sample sequence to resolve the selected
    // candidate pair into the snapshot's network_path. Best-effort: completion
    // updates the path only when the report resolves cleanly; a stale
    // generation's completion is dropped by the usual identity guard.
    if !was_sendable
        && state.phase == RtcPeerPhase::Sendable
        && let Some(generation) = state.active_generation
    {
        state.initial_connection_inflight = false;
        state.stats_sample_attempts = 1;
        let operation_id = state.alloc_operation();
        effects.push(RtcPeerEffect::GetStats {
            generation,
            operation_id,
        });
    }
    record_pending_effects(state, &effects);
    effects
}

/// Record every newly emitted blocking effect before it can leave the reducer.
/// Operation ids are peer-monotone, so replacement would indicate a reducer
/// bug and is rejected loudly in debug builds.
fn record_pending_effects(state: &mut RtcPeerState, effects: &[RtcPeerEffect]) {
    for effect in effects {
        let Some((pc_generation, operation_id, ice_generation, kind)) = effect.pending_identity()
        else {
            continue;
        };
        let previous = state.pending_operations.insert(
            operation_id,
            PendingRtcOperation {
                kind,
                pc_generation,
                ice_generation,
            },
        );
        debug_assert!(
            previous.is_none(),
            "operation id {operation_id} was reused for {kind:?}"
        );
    }
}

/// Consume a completion only when kind, PC generation, operation id, and the
/// optional ICE generation all match the operation originally emitted by this
/// peer. A malformed completion does not consume the legitimate pending entry.
fn authenticate_completion(state: &mut RtcPeerState, completion: &EffectCompletion) -> bool {
    let operation_id = completion.operation_id();
    let Some(expected) = state.pending_operations.remove(&operation_id) else {
        state.stale_completions += 1;
        return false;
    };
    let matches = expected.kind == completion.kind()
        && expected.pc_generation == completion.generation()
        && expected.ice_generation == completion.ice_generation();
    if !matches {
        state.pending_operations.insert(operation_id, expected);
        state.stale_completions += 1;
    }
    matches
}

/// Inject one remote candidate if it is ready (ufrag applies, the target
/// generation exists, and that generation's remote description has completed);
/// otherwise buffer, drop-stale, or count-stale per the ufrag disposition.
fn inject_or_buffer_candidate(
    state: &mut RtcPeerState,
    candidate: RTCIceCandidateInit,
    effects: &mut Vec<RtcPeerEffect>,
) {
    use super::state::CandidateUfragDisposition as Disp;
    match state.classify_candidate_ufrag(candidate.username_fragment.as_deref()) {
        Disp::Apply => {
            // S5-4: during make-before-break, inject into the pending
            // session (it's the one being built). Otherwise inject into
            // the active session (or buffer if none yet).
            let target = state.pending_generation.or(state.active_generation);
            if let Some(generation) = target
                && state.applied_remote_description_generation == Some(generation)
            {
                let operation_id = state.alloc_operation();
                effects.push(RtcPeerEffect::AddIceCandidate {
                    generation,
                    operation_id,
                    ice_generation: state.ice_generation,
                    candidate,
                });
            } else {
                // Matches the current ufrag, but either the Session is still
                // building or set_remote_description() has not completed.
                buffer_remote_candidate(state, candidate);
            }
        }
        Disp::DropStale => state.stale_events += 1,
        Disp::Buffer => buffer_remote_candidate(state, candidate),
    }
}

fn buffer_remote_candidate(state: &mut RtcPeerState, candidate: RTCIceCandidateInit) {
    if state.pending_candidates.len() >= MAX_BUFFERED_ICE_CANDIDATES {
        state.pending_candidates.remove(0);
        state.stale_events = state.stale_events.saturating_add(1);
    }
    state.pending_candidates.push(candidate);
}

fn buffer_local_candidate(
    state: &mut RtcPeerState,
    generation: u64,
    candidate: RTCIceCandidateInit,
) {
    if state.pending_local_candidates.len() >= MAX_BUFFERED_ICE_CANDIDATES {
        state.pending_local_candidates.remove(0);
        state.stale_events = state.stale_events.saturating_add(1);
    }
    state.pending_local_candidates.push((generation, candidate));
}

fn flush_local_candidates(
    state: &mut RtcPeerState,
    generation: u64,
    effects: &mut Vec<RtcPeerEffect>,
) {
    let pending = std::mem::take(&mut state.pending_local_candidates);
    for (candidate_generation, candidate) in pending {
        if candidate_generation == generation {
            effects.push(RtcPeerEffect::SendSignaling {
                generation,
                message: SignalingMessage::Candidate(candidate),
            });
        } else {
            state.stale_events = state.stale_events.saturating_add(1);
        }
    }
}

/// Re-classify every buffered candidate (e.g. after the ufrag is learned or the
/// active session becomes ready) (RFC §"ICE generation and candidates").
fn flush_pending_candidates(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    let buffered = std::mem::take(&mut state.pending_candidates);
    for candidate in buffered {
        inject_or_buffer_candidate(state, candidate, effects);
    }
}

/// Learn the remote ufrag from an SDP that is about to be applied.  Candidate
/// flushing is deliberately deferred until the matching SetRemoteDescription
/// completion succeeds.
fn learn_remote_sdp(state: &mut RtcPeerState, sdp: &str) {
    if let Some(ufrag) = super::state::session_ice_ufrag(sdp) {
        let changed = state.remote_ufrag.as_deref() != Some(ufrag.as_str());
        state.set_remote_ufrag(ufrag);
        if changed {
            state.applied_remote_description_generation = None;
        }
    }
}

/// Serialize `set_remote_description()` per peer and record its exact
/// operation identity.  A newer description replaces the single queued slot;
/// negotiation completion will apply the latest value before producing an
/// answer.
fn schedule_remote_description(
    state: &mut RtcPeerState,
    generation: u64,
    desc: Box<RTCSessionDescription>,
    effects: &mut Vec<RtcPeerEffect>,
) {
    if state.pending_remote_description_operation.is_some() {
        state.queued_remote_description = Some((generation, desc));
        return;
    }

    learn_remote_sdp(state, &desc.sdp);
    let operation_id = state.alloc_operation();
    state.pending_remote_description_operation = Some((generation, operation_id));
    effects.push(RtcPeerEffect::SetRemoteDescription {
        generation,
        operation_id,
        desc,
    });
}

/// Become the answerer for a remote offer: record role, build a fresh session,
/// and answer. Shared by the clean first-offer path and the
/// incoming-offer-wins teardown path.
fn become_answerer(
    state: &mut RtcPeerState,
    desc: Box<RTCSessionDescription>,
    exchange_id: Option<String>,
    effects: &mut Vec<RtcPeerEffect>,
) {
    tracing::debug!(
        peer = %state.peer_id.to_string_repr(),
        transport_epoch = state.transport_epoch,
        exchange_id = ?exchange_id,
        "v0.20 accepting remote offer as answerer"
    );
    if state.phase == RtcPeerPhase::Idle
        && state.building_generation.is_none()
        && state.active_generation.is_none()
        && state.pending_generation.is_none()
    {
        state.begin_transport_attempt();
    }
    // A fresh incoming connection supersedes every remote-description
    // operation belonging to the prior negotiation/generation.
    state.pending_remote_description_operation = None;
    state.queued_remote_description = None;
    state.applied_remote_description_generation = None;
    state.pending_remote_sdp_exchange_id = exchange_id;
    state.role = Role::Answerer;
    state.awaiting_offer = false;
    state.parked_signaling_generation = None;
    state.role_negotiation_inflight = false;
    state.phase = RtcPeerPhase::Building;
    state.unavailable_reason = None;
    // Learn the remote ufrag from the offer so subsequent candidates can be
    // tagged/validated against it.
    learn_remote_sdp(state, &desc.sdp);
    state.pending_remote_offer = Some(desc);
    state.negotiation_epoch += 1;
    let generation = state.alloc_generation();
    state.building_generation = Some(generation);
    let operation_id = state.alloc_operation();
    state.bump_revision();
    effects.push(RtcPeerEffect::BuildSession {
        generation,
        operation_id,
    });
}

/// Replace a recovery/connection attempt with a fresh ordinary remote Offer.
///
/// A changed ICE ufrag on an ordinary Offer is not another step in the old
/// recovery cycle: it proves that the remote endpoint created a new physical
/// connection flight. Retire every old live slot, abandon any in-flight build,
/// clear all attempt-scoped SDP/ICE/recovery facts, and advance the logical
/// transport epoch before `become_answerer` builds the replacement. Monotonic
/// operation/generation identities and outstanding close barriers survive.
fn supersede_transport_attempt_with_offer(
    state: &mut RtcPeerState,
    fresh_remote_ufrag: Option<&str>,
    effects: &mut Vec<RtcPeerEffect>,
) {
    // Trickle candidates can overtake the Offer on the controller/effect
    // mailboxes even though signaling transport itself is ordered. Preserve
    // only candidates explicitly tagged with the fresh Offer's ufrag; every
    // old/unknown candidate still belongs to the superseded attempt.
    let fresh_candidates = fresh_remote_ufrag.map_or_else(Vec::new, |ufrag| {
        state
            .pending_candidates
            .iter()
            .filter(|candidate| candidate.username_fragment.as_deref() == Some(ufrag))
            .cloned()
            .collect()
    });
    clear_transport_attempt_cache(state);
    state.pending_candidates = fresh_candidates;
    state.building_generation = None;
    for generation in [
        state.active_generation.take(),
        state.pending_generation.take(),
    ]
    .into_iter()
    .flatten()
    {
        if state.sessions_to_close.insert(generation) {
            schedule_close_session(state, generation, effects);
        }
    }
    state.begin_transport_attempt();
}

/// Rebase an already-building replacement onto a fresh remote ordinary Offer
/// without allocating a wasteful third PeerConnection.
///
/// The replacement Session itself is reusable, but the recovery budget and all
/// SDP/ICE correlation facts that led to it are not. Keeping the old
/// `recovery_cycle` here lets a nearly-expired timer terminate the new remote
/// flight seconds after its Offer was accepted. Clear the old attempt scope,
/// retain only the physical active/replacement slots and authenticated
/// Build/Close operations, then advance both logical identities.
fn rebase_replacement_attempt_with_offer(
    state: &mut RtcPeerState,
    fresh_remote_ufrag: Option<&str>,
) {
    let fresh_candidates = fresh_remote_ufrag.map_or_else(Vec::new, |ufrag| {
        state
            .pending_candidates
            .iter()
            .filter(|candidate| candidate.username_fragment.as_deref() == Some(ufrag))
            .cloned()
            .collect()
    });
    clear_transport_attempt_cache(state);
    state.pending_candidates = fresh_candidates;
    state.role = Role::Answerer;
    state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
    state.recovery_inflight = true;
    state.recovery_attempt_started = true;
    state.recovery_negotiated = false;
    state.phase = if state.building_generation.is_some() {
        RtcPeerPhase::Building
    } else {
        RtcPeerPhase::Negotiating
    };
    state.negotiation_epoch = state.negotiation_epoch.wrapping_add(1);
    state.begin_transport_attempt();
    state.bump_revision();
}

/// Begin (or re-drive) server-arbitrated role negotiation (RFC §"Role and
/// glare handling"; 0.14 semantics): ask the signaling server to assign this
/// pair's role before building or offering, so a simultaneous connect on both
/// ends converges to exactly one offerer. The probe is emitted unconditionally;
/// when no signaling generation is committed the controller drops it at
/// dispatch — `role_probe_undelivered` remembers that so the gate-opening edge
/// can re-drive it later (the epoch bump retires any deadline armed for a
/// previous, never-delivered probe).
fn start_role_negotiation(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    state.negotiation_epoch = state.negotiation_epoch.wrapping_add(1);
    state.role_negotiation_inflight = true;
    state.role_probe_undelivered = state.signaling_generation.is_none();
    state.phase = RtcPeerPhase::Building;
    state.unavailable_reason = None;
    state.bump_revision();
    tracing::debug!(
        peer = %state.peer_id.to_string_repr(),
        transport_epoch = state.transport_epoch,
        negotiation_epoch = state.negotiation_epoch,
        "v0.20 sending role negotiation probe"
    );
    effects.push(RtcPeerEffect::SendSignaling {
        // Role negotiation is not tied to a PC generation (none exists).
        generation: 0,
        message: SignalingMessage::RoleNegotiation,
    });
}

/// Apply an Answerer's restart request and return the outcome committed by the
/// same reducer turn. The router uses this result for its Ack, rather than
/// classifying the request from a snapshot observed before the command ran.
fn reduce_ice_restart_request(
    state: &mut RtcPeerState,
    effects: &mut Vec<RtcPeerEffect>,
    request_id: String,
    attempt: u32,
) -> i32 {
    use actr_protocol::ice_restart_request_ack::Outcome;

    let outcome = if state.role != Role::Offerer {
        Outcome::WrongRole
    } else if state.close_requested
        || state.active_generation.is_none()
            && state.building_generation.is_none()
            && state.pending_generation.is_none()
    {
        Outcome::NoActiveSession
    } else {
        Outcome::Accepted
    };

    if outcome != Outcome::Accepted {
        return outcome as i32;
    }

    // Do not consume the remote retry identity until this Controller is
    // actually able to act on it. A request can overtake role/build
    // publication; remembering it while ineligible would make the answerer's
    // retransmission look like a duplicate after the local Session becomes
    // ready.
    if state.active_generation.is_none()
        || state.building_generation.is_some()
        || state.pending_generation.is_some()
    {
        return Outcome::Accepted as i32;
    }
    let duplicate = matches!(
        state.last_remote_restart_request.as_ref(),
        Some((seen_id, seen_attempt)) if seen_id == &request_id && *seen_attempt >= attempt
    );
    if duplicate {
        return Outcome::Accepted as i32;
    }
    state.last_remote_restart_request = Some((request_id, attempt));

    // Both endpoints normally receive NetworkAvailable. The offerer may
    // therefore have already started the same soft-recovery round before the
    // answerer's first correlated wake arrives. Starting a second
    // restart_ice/offer concurrently corrupts the v0.20 signaling state
    // machine. A later attempt remains an explicit retransmission wake and is
    // allowed to restart the round.
    if attempt == 1 && state.recovery_inflight && state.recovery_attempt_started {
        return Outcome::Accepted as i32;
    }

    // Attempts below the escalation threshold drive the ordinary soft path:
    // both endpoints normally recover from a shared network blip with an ICE
    // restart on the live generation, and the answerer's live session can
    // accept the restart offer.
    //
    // Persistent retransmission is proof soft recovery cannot repair the
    // answerer — its physical session is dead (an answerer retransmits
    // indefinitely while parked awaiting an offer it can never receive on a
    // dead generation). Escalate to Hard Recovery: the replacement Session
    // allocates a fresh generation and produces the replacement Offer the
    // passive answerer needs, instead of letting it exhaust to Idle with
    // transport down. A request that overtakes a soft round already in flight
    // was returned above, so this cannot corrupt an in-progress restart; a
    // newer attempt while a replacement is already building is a no-op inside
    // start_hard_recovery.
    const ANSWERER_HARD_ESCALATION_ATTEMPT: u32 = 4;
    if attempt >= ANSWERER_HARD_ESCALATION_ATTEMPT {
        start_hard_recovery(state, effects);
    } else {
        if state.recovery_inflight {
            state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
            state.recovery_attempt_started = false;
        }
        start_soft_recovery(state, effects);
    }

    Outcome::Accepted as i32
}

fn reduce_command(
    state: &mut RtcPeerState,
    command: RtcPeerCommand,
    effects: &mut Vec<RtcPeerEffect>,
) {
    match command {
        RtcPeerCommand::Connect | RtcPeerCommand::ConnectCommitted { .. } => {
            // Role is server-arbitrated (RFC §"Role and glare handling"; 0.14
            // semantics): ask the signaling server to assign this pair's role
            // before building or offering, so a simultaneous connect on both
            // ends converges to exactly one offerer.
            if state.phase != RtcPeerPhase::Idle || state.close_requested {
                return;
            }
            state.begin_transport_attempt();
            start_role_negotiation(state, effects);
        }
        RtcPeerCommand::AbortTransportAttempt { transport_epoch } => {
            if state.initial_connection_inflight
                && state.transport_epoch == transport_epoch
                && !state.close_requested
            {
                terminate_transport_attempt(
                    state,
                    RtcUnavailableReason::NegotiationFailed,
                    effects,
                );
            }
        }
        RtcPeerCommand::ApplyRoleAssignment { is_offerer } => {
            let supersedes_inflight_attempt = is_offerer
                && !state.close_requested
                && (state.recovery_inflight
                    || (state.initial_connection_inflight
                        && (state.building_generation.is_some()
                            || state.active_generation.is_some()
                            || state.pending_generation.is_some())));
            if supersedes_inflight_attempt {
                // Restore the pre-v0.20 Coordinator contract: a fresh role
                // assignment that makes this side the Offerer supersedes the
                // current initial or recovery attempt immediately. This is
                // especially important when the dialing side was Answerer and
                // its controller is waiting for a new Offer; reusing the old
                // recovery PC would strand that retry.
                //
                // RoleAssignment itself is the authoritative rebuild command.
                // Repeated assignments deliberately allocate repeated fresh
                // generations; generation/operation fencing makes every late
                // callback from an abandoned attempt harmless.
                restart_as_offerer_after_role_assignment(state, effects);
            } else if state.role_negotiation_inflight {
                // The awaited arbitration: start the assigned role's flow.
                state.role_negotiation_inflight = false;
                state.role_probe_undelivered = false;
                state.negotiation_epoch += 1;
                if is_offerer {
                    state.role = Role::Offerer;
                    let generation = state.alloc_generation();
                    state.building_generation = Some(generation);
                    let operation_id = state.alloc_operation();
                    state.bump_revision();
                    tracing::info!(
                        peer = %state.peer_id.to_string_repr(),
                        role = "offerer",
                        generation,
                        negotiation_epoch = state.negotiation_epoch,
                        "v0.20 role arbitration resolved; building PeerConnection"
                    );
                    effects.push(RtcPeerEffect::BuildSession {
                        generation,
                        operation_id,
                    });
                } else {
                    state.role = Role::Answerer;
                    state.awaiting_offer = true;
                    state.bump_revision();
                    tracing::info!(
                        peer = %state.peer_id.to_string_repr(),
                        role = "answerer",
                        negotiation_epoch = state.negotiation_epoch,
                        "v0.20 role arbitration resolved; awaiting offer"
                    );
                }
            } else if state.role == Role::Undetermined {
                // Unsolicited assignment (the server assigns both ends when the
                // *other* peer negotiates).
                if is_offerer {
                    // 0.14 parity (`Coordinator::handle_role_assignment` spawns
                    // `start_offer_connection`): the PASSIVE offerer must drive
                    // the offer itself — the dialer lost the serial tie-break,
                    // was assigned answerer, and is now waiting for our offer.
                    // Recording the role without building deterministically
                    // deadlocks the pair.
                    state.role = Role::Offerer;
                    if state.phase == RtcPeerPhase::Idle
                        && state.building_generation.is_none()
                        && state.active_generation.is_none()
                        && state.pending_generation.is_none()
                    {
                        state.begin_transport_attempt();
                    }
                    state.phase = RtcPeerPhase::Building;
                    state.negotiation_epoch += 1;
                    let generation = state.alloc_generation();
                    state.building_generation = Some(generation);
                    let operation_id = state.alloc_operation();
                    state.bump_revision();
                    effects.push(RtcPeerEffect::BuildSession {
                        generation,
                        operation_id,
                    });
                } else {
                    // Passive answerer: record our role for the upcoming offer,
                    // without disturbing the current phase.
                    state.role = Role::Answerer;
                    state.awaiting_offer = true;
                }
            } else if is_offerer
                && state.role == Role::Offerer
                && state.ever_connected
                && state.active_generation.is_some()
                && state.building_generation.is_none()
                && state.pending_generation.is_none()
                && !state.close_requested
            {
                // The remote peer has lost its controller and negotiated the
                // pair again. The server keeps the stable role assignment, so
                // an already-established offerer receives another OFFERER
                // assignment while the replacement answerer waits for a fresh
                // offer. Rebuild the physical Session and send that offer;
                // reusing the old ICE credentials leaves the new answerer
                // paired with a stale generation.
                start_hard_recovery(state, effects);
            }
            // A role change here would invalidate in-flight SDP ops; that is
            // tracked by negotiation_epoch and generation gating on completions.
        }
        RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation,
            negotiation_epoch,
        } => {
            let exact_attempt = state.transport_epoch == transport_epoch
                && if pc_generation == 0 {
                    // Gen-0 results name one exact role probe. The
                    // gate-opening edge re-drives the probe while one is
                    // still in flight (each emission bumps
                    // `negotiation_epoch`), so `role_negotiation_inflight`
                    // alone cannot tell a delayed result for the retired
                    // probe apart from the live one's.
                    state.role_negotiation_inflight && state.negotiation_epoch == negotiation_epoch
                } else {
                    state.is_live_generation(pc_generation)
                };
            if exact_attempt && !state.close_requested {
                // `TargetUnavailable` only says that Actrix had no live
                // signaling egress for the target at one instant. Mobile
                // backgrounding routinely creates that window while retaining
                // the same ActrId and recoverable PeerConnection. Treating the
                // result as terminal destroys the remote Session and makes the
                // subsequent ICE restart impossible.
                //
                // Keep the exact attempt alive and let its existing initial or
                // recovery deadline own the terminal decision. A permanently
                // stale ActrId still converges through that bounded deadline;
                // a temporarily suspended peer can reconnect and complete the
                // current recovery attempt.
                tracing::warn!(
                    peer = %state.peer_id.to_string_repr(),
                    transport_epoch,
                    pc_generation,
                    negotiation_epoch,
                    recovery_inflight = state.recovery_inflight,
                    "target currently unavailable; preserving bounded RTC connection attempt"
                );
            } else {
                state.stale_events = state.stale_events.saturating_add(1);
            }
        }
        RtcPeerCommand::ApplyRemoteIceRestartOffer(desc) => {
            reduce_command(
                state,
                RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
                    desc,
                    exchange_id: None,
                },
                effects,
            );
        }
        RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated { desc, exchange_id } => {
            if desc.sdp_type != RTCSdpType::Offer || state.close_requested {
                state.stale_events += 1;
                return;
            }
            if state.role != Role::Offerer
                && state.active_generation.is_none()
                && state.pending_generation.is_none()
                && state.building_generation.is_none()
            {
                // A restart offer is scoped to an existing physical Session.
                // A logical Controller may survive cleanup while that Session
                // is already gone; do not reinterpret the restart SDP as a
                // first ordinary Offer. Reply on the signaling path using the
                // retired generation identity when available so the response
                // remains fenced by this Controller's lifecycle.
                // An Offerer receiving this message is a glare/stale input and
                // remains on the existing stale-offer path; only Answerer and
                // Undetermined roles can be the passive receiver that needs
                // the explicit NoActiveSession response.
                //
                // The rejection is scoped to correlated offers: a legacy
                // uncorrelated restart offer (a pre-correlation peer in a
                // rolling upgrade) has no exchange id to fence a rejection
                // with — and a peer that old may not understand
                // `IceRestartRejected` at all. A controller whose session was
                // retired keeps the pre-existing fallthrough to
                // ordinary-offer handling below, which builds the replacement
                // both ends need; a controller that never had a session drops
                // the offer as stale, since there is nothing to rebuild from
                // and the peer's restart premise is simply wrong.
                if let Some(exchange_id) = exchange_id {
                    let generation = state
                        .parked_signaling_generation
                        .unwrap_or_else(|| state.next_pc_generation.saturating_sub(1));
                    effects.push(RtcPeerEffect::SendSignaling {
                        generation,
                        message: SignalingMessage::IceRestartRejected {
                            exchange_id,
                            reason: actr_protocol::ice_restart_rejected::Reason::NoActiveSession
                                as i32,
                        },
                    });
                    state.stale_events = state.stale_events.saturating_add(1);
                    return;
                }
                if !state.ever_connected && state.parked_signaling_generation.is_none() {
                    state.stale_events = state.stale_events.saturating_add(1);
                    return;
                }
            }
            if state.role == Role::Answerer
                && state.active_generation.is_some()
                && state.building_generation.is_none()
            {
                let generation = state
                    .pending_generation
                    .or(state.active_generation)
                    .expect("guarded live answerer generation");
                let offer_ufrag = super::state::session_ice_ufrag(&desc.sdp);
                let changed = offer_ufrag.is_some()
                    && offer_ufrag.as_deref() != state.remote_ufrag.as_deref();
                if changed {
                    state.ice_generation += 1;
                }
                if !state.recovery_inflight {
                    state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
                }
                // Receiving a restart Offer is protocol progress for the
                // current Answerer recovery. Supersede the request-only
                // attempt deadline even when recovery is already in flight;
                // otherwise the timer armed when the request was sent can
                // fire while this Offer/Answer exchange is still converging
                // and emit a duplicate recovery request.
                state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                // A parked Answerer may still be reusing the active
                // PeerConnection. Receiving the correlated restart Offer is
                // the point at which it rejoins the current recovery attempt;
                // it must no longer be treated as waiting for a first Offer.
                state.awaiting_offer = false;
                state.phase = RtcPeerPhase::Recovering;
                state.recovery_inflight = true;
                state.recovery_attempt_started = true;
                state.recovery_negotiated = false;
                state.pc_connected = false;
                state.bump_revision();
                state.pending_remote_sdp_exchange_id = exchange_id;
                schedule_remote_description(state, generation, desc, effects);
            } else {
                // A restart offer without an established answerer is handled
                // like an ordinary offer; the router rejects the common
                // no-controller case before it reaches this fallback.
                reduce_command(
                    state,
                    RtcPeerCommand::ApplyRemoteDescriptionCorrelated { desc, exchange_id },
                    effects,
                );
            }
        }
        RtcPeerCommand::ApplyIceRestartRequest {
            request_id,
            attempt,
        } => {
            let _ = reduce_ice_restart_request(state, effects, request_id, attempt);
        }
        RtcPeerCommand::ApplyIceRestartRequestCommitted {
            request_id,
            attempt,
            respond,
        } => {
            let outcome = reduce_ice_restart_request(state, effects, request_id, attempt);
            let _ = respond.send(outcome);
        }
        RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id,
            attempt,
            outcome,
        } => {
            // Only the Answerer owns the request flight. An Ack for an older
            // request/attempt must not restart a newer recovery attempt.
            if state.role != Role::Answerer
                || state.close_requested
                || state.local_restart_request_id.as_deref() != Some(request_id.as_str())
                || state.local_restart_request_attempt != attempt
            {
                state.stale_events = state.stale_events.saturating_add(1);
                return;
            }

            match actr_protocol::ice_restart_request_ack::Outcome::try_from(outcome) {
                Ok(actr_protocol::ice_restart_request_ack::Outcome::Accepted) => {
                    // Acceptance only confirms that the Offerer owns the
                    // request. The Answerer still waits for the correlated
                    // ICE-restart Offer and its existing deadline remains the
                    // retransmission backstop.
                }
                Ok(
                    actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession
                    | actr_protocol::ice_restart_request_ack::Outcome::WrongRole,
                ) => {
                    // The remote endpoint cannot continue the current
                    // request-only exchange. Rebase this Controller into a
                    // fresh role-negotiated connection, matching the legacy
                    // Coordinator's Ack failure path.
                    restart_after_request_ack(state, effects, outcome);
                }
                Err(_) => {
                    state.stale_events = state.stale_events.saturating_add(1);
                }
            }
        }
        RtcPeerCommand::ApplyRemoteDescription(desc) => {
            reduce_command(
                state,
                RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
                    desc,
                    exchange_id: None,
                },
                effects,
            );
        }
        RtcPeerCommand::ApplyRemoteDescriptionCorrelated { desc, exchange_id } => {
            match desc.sdp_type {
                RTCSdpType::Offer => {
                    if state.close_requested {
                        return;
                    }
                    match state.role {
                        // An assigned/active offerer ignores a competing offer (the
                        // server already made it the offerer); it is stale.
                        Role::Offerer => state.stale_events += 1,
                        Role::Answerer | Role::Undetermined => {
                            // The wire protocol explicitly distinguishes
                            // IceRestartOffer from a normal Offer. Therefore a
                            // normal Offer whose ufrag changed is a fresh physical
                            // connection and must retire the established Session;
                            // same-ufrag offers remain ordinary renegotiation.
                            let offer_ufrag = super::state::session_ice_ufrag(&desc.sdp);
                            let is_new_connection = offer_ufrag.is_some()
                                && offer_ufrag.as_deref() != state.remote_ufrag.as_deref();
                            let has_live_session = state.active_generation.is_some()
                                || state.building_generation.is_some()
                                || state.pending_generation.is_some();

                            if state.role == Role::Answerer
                                && state.active_generation.is_some()
                                && state.building_generation.is_none()
                                && !is_new_connection
                            {
                                // Established answerer ordinary renegotiation: same
                                // ICE credentials stay on the current generation.
                                let generation = state
                                    .pending_generation
                                    .or(state.active_generation)
                                    .expect("guarded live answerer generation");
                                state.pending_remote_sdp_exchange_id = exchange_id;
                                schedule_remote_description(state, generation, desc, effects);
                            } else if state.role == Role::Answerer
                                && state.active_generation.is_some()
                                && (state.building_generation.is_some()
                                    || state.pending_generation.is_some())
                            {
                                // The Offerer may send a fresh Offer while this
                                // Answerer still has a replacement slot from an
                                // earlier connection flow. Apply the offer to
                                // that replacement if its Session already exists;
                                // otherwise hold it for the in-flight build. Never
                                // discard that replacement and allocate a third
                                // generation merely because its remote ICE
                                // credentials are new.
                                if is_new_connection {
                                    rebase_replacement_attempt_with_offer(
                                        state,
                                        offer_ufrag.as_deref(),
                                    );
                                }
                                if let Some(generation) = state.pending_generation {
                                    state.pending_remote_sdp_exchange_id = exchange_id;
                                    schedule_remote_description(state, generation, desc, effects);
                                } else {
                                    learn_remote_sdp(state, &desc.sdp);
                                    state.pending_remote_sdp_exchange_id = exchange_id;
                                    state.pending_remote_offer = Some(desc);
                                }
                            } else if is_new_connection && has_live_session {
                                // A fresh physical connection supersedes the old
                                // logical recovery attempt. In particular, its old
                                // total-recovery timer and SDP/ICE caches must not
                                // survive and kill or contaminate this generation.
                                supersede_transport_attempt_with_offer(
                                    state,
                                    offer_ufrag.as_deref(),
                                    effects,
                                );
                                become_answerer(state, desc, exchange_id, effects);
                            } else {
                                // First offer / duplicate on a clean base: Idle
                                // (passive), undetermined (offer wins a role
                                // negotiation), or an assigned answerer awaiting the
                                // offer. Never while already building/connected.
                                let can_answer = !has_live_session
                                    && (state.phase == RtcPeerPhase::Idle
                                        || state.role == Role::Undetermined
                                        || (state.role == Role::Answerer && state.awaiting_offer));
                                if !can_answer {
                                    state.stale_events += 1;
                                    return;
                                }
                                become_answerer(state, desc, exchange_id, effects);
                            }
                        }
                    }
                }
                RTCSdpType::Answer => {
                    // A remote answer is only meaningful to an offerer with a live
                    // session; otherwise it is stale. S5-4: during make-before-
                    // break, the answer targets the pending session (not the old
                    // active one).
                    if state.role != Role::Offerer || state.close_requested {
                        state.stale_events += 1;
                        return;
                    }
                    let target = state.pending_generation.or(state.active_generation);
                    let Some(generation) = target else {
                        state.stale_events += 1;
                        return;
                    };
                    match (
                        state.pending_local_sdp_exchange_id.as_deref(),
                        exchange_id.as_deref(),
                    ) {
                        (Some(expected), Some(received)) if expected != received => {
                            state.stale_events += 1;
                            tracing::warn!(
                                peer = %state.peer_id.to_string_repr(),
                                %expected,
                                %received,
                                "discarding stale SDP answer exchange"
                            );
                            return;
                        }
                        (None, Some(received)) => {
                            state.stale_events += 1;
                            tracing::warn!(
                                peer = %state.peer_id.to_string_repr(),
                                %received,
                                "discarding SDP answer with no pending local exchange"
                            );
                            return;
                        }
                        (None, None) => {
                            state.stale_events += 1;
                            tracing::warn!(
                                peer = %state.peer_id.to_string_repr(),
                                "discarding uncorrelated SDP answer with no pending local exchange"
                            );
                            return;
                        }
                        // A missing id remains accepted for rolling compatibility
                        // with older native peers only while one local Offer is
                        // actually outstanding. New v0.20 peers always send it.
                        (Some(_), None) | (Some(_), Some(_)) => {
                            state.pending_local_sdp_exchange_id = None;
                        }
                    }
                    tracing::debug!(
                        peer = %state.peer_id.to_string_repr(),
                        generation,
                        exchange_id = ?exchange_id,
                        "v0.20 remote SDP answer accepted; applying"
                    );
                    schedule_remote_description(state, generation, desc, effects);
                }
                _ => {
                    // Pranswer / rollback are not used by Actr's signaling protocol.
                }
            }
        }
        RtcPeerCommand::ApplyRemoteCandidate(candidate) => {
            // Tag candidates by (pc_generation, ufrag) and inject those matching
            // the current remote ufrag into the live active generation; buffer
            // or drop-stale the rest (RFC §"ICE generation and candidates",
            // invariant #6).
            inject_or_buffer_candidate(state, candidate, effects);
        }
        RtcPeerCommand::SignalingDeliveryFailed {
            transport_epoch,
            pc_generation,
        } => {
            if state.transport_epoch == transport_epoch
                && state.is_live_generation(pc_generation)
                && !state.close_requested
            {
                negotiation_failed(state, effects);
            }
        }
        RtcPeerCommand::EnsureLane(lane) => {
            // S5-4: during make-before-break, target the pending session (it
            // will become active after the switch). Otherwise target the
            // active session.
            let target = state.pending_generation.or(state.active_generation);
            if let Some(generation) = target {
                effects.push(RtcPeerEffect::CreateLane { generation, lane });
            }
        }
        RtcPeerCommand::Close => reduce_close(state, effects),
        RtcPeerCommand::NetworkEvent(event) => {
            reduce_network_event(state, event, effects);
        }
        RtcPeerCommand::RefreshConfiguration { config, .. } => {
            // S5-6: TURN config refresh on the active session. Only applicable
            // when there is a live session; on failure, the completion handler
            // escalates to Hard Recovery (RFC §"RefreshConfiguration").
            if state.close_requested {
                return;
            }
            if let Some(generation) = state.active_generation {
                let operation_id = state.alloc_operation();
                effects.push(RtcPeerEffect::RefreshConfiguration {
                    generation,
                    operation_id,
                    config,
                });
            }
        }
        // S6-2: media topology commands are executed by the controller shell
        // (which owns the response channel) and never reach the reducer in
        // production; this no-op arm keeps direct-drive reducer tests
        // exhaustive. Renegotiation flows from the subsequent NegotiationNeeded
        // driver event.
        RtcPeerCommand::AddTrack { .. } | RtcPeerCommand::RemoveTrack { .. } => {}
    }
}

/// Supersede an initial or recovery attempt after a new Offerer role
/// assignment. Existing Sessions close asynchronously while the replacement
/// build starts immediately; waiting for close would let a stalled PC block the
/// retry that is intended to replace it.
fn restart_as_offerer_after_role_assignment(
    state: &mut RtcPeerState,
    effects: &mut Vec<RtcPeerEffect>,
) {
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        transport_epoch = state.transport_epoch,
        recovery_cycle = state.recovery_cycle,
        recovery_epoch = state.recovery_epoch,
        role = ?state.role,
        recovery_inflight = state.recovery_inflight,
        "v0.20 RoleAssignment supersedes current attempt; rebuilding as Offerer"
    );
    clear_transport_attempt_cache(state);
    state.role_probe_undelivered = false;
    state.building_generation = None;
    for generation in [
        state.active_generation.take(),
        state.pending_generation.take(),
    ]
    .into_iter()
    .flatten()
    {
        if state.sessions_to_close.insert(generation) {
            schedule_close_session(state, generation, effects);
        }
    }

    state.begin_transport_attempt();
    state.role = Role::Offerer;
    state.awaiting_offer = false;
    state.role_negotiation_inflight = false;
    state.negotiation_epoch = state.negotiation_epoch.wrapping_add(1);
    state.phase = RtcPeerPhase::Building;

    let generation = state.alloc_generation();
    state.building_generation = Some(generation);
    let operation_id = state.alloc_operation();
    state.bump_revision();
    effects.push(RtcPeerEffect::BuildSession {
        generation,
        operation_id,
    });
}

/// Recreate the logical connection after the remote endpoint reports that it
/// cannot continue the current Answerer restart request. This is the v0.20
/// equivalent of the legacy `IceRestartRequestAck(NoActiveSession)` path:
/// retire the current physical attempt, then let server role arbitration choose
/// the next Offerer instead of waiting for an Offer that cannot arrive.
fn restart_after_request_ack(
    state: &mut RtcPeerState,
    effects: &mut Vec<RtcPeerEffect>,
    outcome: i32,
) {
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        outcome,
        recovery_cycle = state.recovery_cycle,
        "v0.20 restart request acknowledgement requires fresh role negotiation"
    );
    terminate_transport_attempt(state, RtcUnavailableReason::NegotiationFailed, effects);
    state.begin_transport_attempt();
    start_role_negotiation(state, effects);
}

/// S5-2/S5-3/S5-5: the supervisor broadcasts facts; the controller's reducer
/// decides the recovery action. Network events drive the Soft/Hard decision:
///
/// * `NetworkUnavailable`: mark recovering (don't restart yet — wait for
///   availability to return).
/// * `NetworkAvailable`: if recovering and no explicit hard-recovery request is
///   pending, start Soft Recovery (ICE restart). If `address_changed` was set
///   by an explicit force-reconnect or long-background policy, start Hard
///   Recovery instead.
/// * `LocalAddressChanged`: start Soft Recovery on the existing
///   PeerConnection. v0.20.3 rebinds ICE transports and refreshes local
///   candidates when the ICE generation restarts. The soft ladder is
///   bounded: a restart whose rebind fails (the driver dies, so the next
///   `restart_ice` errors) escalates to Hard Recovery immediately, and
///   retries that keep failing to converge escalate at
///   `SOFT_HARD_ESCALATION_ATTEMPT`. A build still holding the pre-change
///   address snapshot is superseded by a fresh build instead.
/// * `AppBackground`: if the peer is `Sendable`, enter `Recovering`
///   (sendable=false) because the platform may suspend the process and the
///   connection must be re-established on foreground. No restart is issued
///   yet — recovery resumes on `AppForeground`.
/// * `AppForeground`: if recovering and no explicit hard-recovery request is
///   pending, start Soft Recovery. If `address_changed`, the Offerer starts
///   Hard Recovery and the Answerer waits for its next Offer.
/// * `ForceReconnect`: set `address_changed = true` (force Hard for the
///   Offerer) and wait for the supervisor-owned signaling recovery gate before
///   rebuilding.
fn recovery_gate_open(state: &RtcPeerState) -> bool {
    state.network_available
        && state.app_foreground
        && state.signaling_generation.is_some()
        && !state.close_requested
}

fn defer_recovery_until_gate(state: &mut RtcPeerState, hard: bool) {
    // Existing-recovery semantics (main 0.14's `BeginRecoveryOutcome::Existing`):
    // a recovery round already in flight keeps its exchange identity, its
    // deadline, and its negotiation state. Late lifecycle/platform edges
    // (AppBackground, gate withdrawal) coalesce into the in-flight round
    // instead of wiping it — clearing the negotiation here opened a second
    // restart exchange whenever a background/foreground pair overlapped an
    // in-flight one (p08, rapid-restart dedup).
    if state.recovery_inflight && state.recovery_attempt_started {
        state.pc_connected = false; // S5-5: sendable still expires while gated
        state.phase = RtcPeerPhase::Recovering;
        if hard {
            // An explicit hard-recovery request must still steer the *next* round hard.
            state.address_changed = true;
        }
        state.bump_revision();
        return;
    }
    if !state.recovery_inflight {
        tracing::debug!(
            peer = %state.peer_id.to_string_repr(),
            hard,
            "v0.20 recovery deferred until the signaling gate opens"
        );
        state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
        state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
        state.soft_recovery_attempts = 0;
    }
    clear_negotiation_for_recovery(state);
    state.recovery_inflight = true;
    state.recovery_attempt_started = false;
    state.recovery_negotiated = false;
    state.phase = RtcPeerPhase::Recovering;
    if hard {
        state.address_changed = true;
    } else {
        state.pc_connected = false;
    }
    state.bump_revision();
}

fn maybe_start_deferred_recovery(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    if !state.recovery_inflight || state.recovery_attempt_started || !recovery_gate_open(state) {
        return;
    }
    if state.role == Role::Answerer && state.awaiting_offer {
        return;
    }
    if state.address_changed {
        start_hard_recovery(state, effects);
    } else {
        start_soft_recovery(state, effects);
    }
}

fn reduce_network_event(
    state: &mut RtcPeerState,
    event: RtcNetworkEvent,
    effects: &mut Vec<RtcPeerEffect>,
) {
    state.network_events_received += 1;
    state.bump_revision();
    if state.close_requested {
        return; // teardown owns the transition
    }
    match event {
        RtcNetworkEvent::NetworkUnavailable => {
            state.network_available = false;
            if state.phase == RtcPeerPhase::Sendable || state.recovery_inflight {
                // Only an explicit hard-recovery request escalates to Hard. A
                // soft restart's own negotiation sets `building_generation`/
                // `pending_generation` while the BuildSession is in flight;
                // reading those as "must rebuild" converted every temporary
                // loss that overlapped an in-flight restart exchange into a
                // hard rebuild (pc_generation bump) — the pair-contract
                // "temporary loss uses soft ICE recovery" contract and the
                // rapid-restart dedup both regress on that.
                defer_recovery_until_gate(state, state.address_changed);
            }
        }
        RtcNetworkEvent::NetworkAvailable => {
            state.network_available = true;
            maybe_start_deferred_recovery(state, effects);
        }
        RtcNetworkEvent::LocalAddressChanged => {
            // The v0.20.3 backend rebinds ICE transports and re-gathers local
            // candidates during ICE restart. Treat an address snapshot change
            // as an in-place recovery opportunity; an explicit ForceReconnect
            // or long-background policy still sets `address_changed` and keeps
            // the Hard Recovery path below.
            if state.active_generation.is_some() && state.recovery_attempt_started {
                if state.recovery_negotiated {
                    if state.role == Role::Answerer {
                        // The Answerer must keep its request path alive so an
                        // Offerer that has not noticed the handover can still
                        // be woken. However, an attempt that already has an
                        // Offer/Answer must not emit a new request merely
                        // because its address edge arrived: the Offerer may
                        // still be executing the same ICE restart. Preserve
                        // the current attempt and let its deadline or the
                        // pending-edge convergence path send the next request.
                        state.address_edge_pending = true;
                    } else {
                        // SDP/ICE negotiation for the current attempt has
                        // already completed, but the physical path is not
                        // ready yet. A newly observed interface set must
                        // supersede that attempt immediately; waiting for its
                        // restart timer leaves the PeerConnection bound to the
                        // old interface set during a mobile handover.
                        state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
                        state.recovery_attempt_started = false;
                        state.address_edge_pending = false;
                        start_soft_recovery(state, effects);
                    }
                } else {
                    // The SDP/ICE exchange is still in flight. Keep the
                    // current exchange serialized, then publish one more
                    // restart when it reaches a physical ready state.
                    state.address_edge_pending = true;
                }
            } else if state.active_generation.is_some() {
                if recovery_gate_open(state) {
                    start_soft_recovery(state, effects);
                } else {
                    defer_recovery_until_gate(state, false);
                }
            } else if state.building_generation.is_some() {
                // The build in flight is consuming the pre-change address
                // snapshot, and a same-generation ICE restart cannot repair a
                // Session that has not been born yet. Supersede the build so
                // the replacement resolves the provider's current set.
                state.recovery_attempt_started = false;
                if recovery_gate_open(state) {
                    start_hard_recovery(state, effects);
                } else {
                    defer_recovery_until_gate(state, true);
                }
            } else if state.phase == RtcPeerPhase::Sendable || state.recovery_inflight {
                defer_recovery_until_gate(state, false);
            }
            // With no generation at all the edge needs no recording: the
            // eventual initial build resolves the provider's current set.
        }
        RtcNetworkEvent::AppBackground => {
            state.app_foreground = false;
            // S5-5: the platform may suspend the process while backgrounded.
            // Enter Recovering so sendable expires immediately; the connection
            // must be re-established on foreground. No restart is issued yet
            // (the process could be suspended at any moment).
            if state.phase == RtcPeerPhase::Sendable || state.recovery_inflight {
                // See NetworkUnavailable: in-flight soft-restart negotiation
                // must not read as an address change.
                defer_recovery_until_gate(state, state.address_changed);
            }
        }
        RtcNetworkEvent::AppForeground {
            background_duration_ms,
        } => {
            state.app_foreground = true;
            if state.phase == RtcPeerPhase::Recovering {
                if background_duration_ms
                    >= crate::lifecycle::LONG_BACKGROUND_RECONNECT_THRESHOLD_MS
                {
                    state.address_changed = true;
                }
                maybe_start_deferred_recovery(state, effects);
            }
        }
        RtcNetworkEvent::ForceReconnect => {
            state.address_changed = true;
            if state.active_generation.is_some() {
                if recovery_gate_open(state) {
                    start_hard_recovery(state, effects);
                } else {
                    defer_recovery_until_gate(state, true);
                }
            } else if state.recovery_inflight || state.building_generation.is_some() {
                defer_recovery_until_gate(state, true);
            } else if state.phase != RtcPeerPhase::Idle {
                // No active session yet; steer any subsequent failure to Hard.
                state.phase = RtcPeerPhase::Recovering;
                state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
                state.recovery_inflight = true;
            }
        }
        RtcNetworkEvent::SignalingGenerationChanged { generation } => {
            // The supervisor gate in the controller's `SendSignaling` dispatch
            // drops outbound signaling while no generation is committed —
            // including the very first Connect role probe, which had no other
            // re-drive (the deadline then failed the attempt as
            // NegotiationFailed). Re-send the probe on the gate-opening edge;
            // the epoch bump retires the deadline armed for the dropped
            // attempt so the re-send gets a fresh full window.
            let gate_opening = state.signaling_generation.is_none() && generation.is_some();
            if state.signaling_generation != generation {
                state.signaling_generation = generation;
                state.bump_revision();
            }
            if generation.is_none() && state.recovery_inflight {
                // See NetworkUnavailable: in-flight soft-restart negotiation
                // must not read as an address change.
                defer_recovery_until_gate(state, state.address_changed);
            } else if generation.is_some() {
                maybe_start_deferred_recovery(state, effects);
                if gate_opening
                    && !state.close_requested
                    && (state.role_negotiation_inflight || state.role_probe_undelivered)
                {
                    // A committed generation is the first moment a probe
                    // dropped by the gate could have run. Re-drive it — and
                    // if the role deadline already failed the attempt while
                    // its probe was undelivered, resurrect it as a fresh
                    // transport attempt.
                    if state.role_probe_undelivered && !state.role_negotiation_inflight {
                        state.begin_transport_attempt();
                    }
                    start_role_negotiation(state, effects);
                }
            }
        }
    }
}

/// S5-2: start Soft Recovery — keep the current PeerConnection and increase
/// the ICE generation. The offerer issues `restart_ice()` and follows with a
/// restart `CreateOffer` after the `RestartIce` completion; the answerer
/// performs no local restart and sends the restart request directly, its
/// local restart being implicit in applying the offerer's restart offer
/// (RFC §"Soft Recovery" steps 2/4). Soft Recovery does not increase the PC
/// generation.
fn clear_negotiation_for_recovery(state: &mut RtcPeerState) {
    retain_lifecycle_operations(state);
    state.pending_remote_offer = None;
    state.pending_remote_description_operation = None;
    state.queued_remote_description = None;
    state.applied_remote_description_generation = None;
    state.pending_local_desc = None;
    state.announced_local_description_generation = None;
    state.pending_local_candidates.clear();
    state.pending_local_sdp_exchange_id = None;
    state.pending_remote_sdp_exchange_id = None;
    state.pending_local_restart_exchange_id = None;
    state.local_restart_request_id = None;
    state.local_restart_request_attempt = 0;
    state.renegotiation_inflight = false;
    state.renegotiation_pending = false;
    state.pending_candidates.clear();
}

/// Whether this peer is an Answerer parked awaiting the stable Offerer's
/// replacement Offer. Centralizing the (role, recovery, awaiting) coupling
/// keeps future refactors from quietly re-enabling Answerer-side restart
/// churn: a parked answerer must never issue RestartIce/BuildSession.
fn answerer_parked(state: &RtcPeerState) -> bool {
    state.role == Role::Answerer && state.recovery_inflight && state.awaiting_offer
}

fn start_soft_recovery(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    if state.recovery_attempt_started {
        return;
    }
    if answerer_parked(state) {
        return;
    }
    if !recovery_gate_open(state) {
        defer_recovery_until_gate(state, false);
        return;
    }
    clear_negotiation_for_recovery(state);
    if !state.recovery_inflight {
        state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
        state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
        state.local_restart_request_id = None;
        state.local_restart_request_attempt = 0;
        state.soft_recovery_attempts = 0;
    }
    state.recovery_attempt_started = true;
    // This round's rebind resolves the current address set, consuming any
    // remembered edge.
    state.address_edge_pending = false;
    state.ice_generation += 1;
    state.recovery_inflight = true;
    state.recovery_negotiated = false;
    state.pc_connected = false;
    state.phase = RtcPeerPhase::Recovering;
    state.bump_revision();
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        role = ?state.role,
        recovery_cycle = state.recovery_cycle,
        recovery_epoch = state.recovery_epoch,
        ice_generation = state.ice_generation,
        generation = state.active_generation,
        "v0.20 soft recovery started"
    );
    if state.active_generation.is_none() {
        return;
    }
    match state.role {
        Role::Offerer | Role::Undetermined => {
            let operation_id = state.alloc_operation();
            effects.push(RtcPeerEffect::RestartIce {
                generation: state.active_generation.expect("guarded live generation"),
                operation_id,
                ice_generation: state.ice_generation,
            });
        }
        Role::Answerer => {
            // The answerer performs no local restart_ice() here. Upstream rtc
            // 0.20 applies the implicit local ICE restart (ufrag roll, stale
            // candidate discard, socket rebind per webrtc-rs/webrtc#868) inside
            // set_remote_description when it detects the offerer's changed
            // credentials — the answerer's local state can therefore only roll
            // atomically with an incoming restart offer, never out from under
            // an answer it just sent. A local restart_ice() racing that offer
            // would instead re-roll credentials after the answer, wedge the
            // round (the offerer de-dupes the attempt-1 request), and escalate
            // both ends to a Hard rebuild at the attempt deadline.
            send_answerer_restart_request(state, effects);
        }
    }
}

fn send_answerer_restart_request(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    let Some(generation) = state
        .active_generation
        .or(state.parked_signaling_generation)
    else {
        return;
    };
    if state.local_restart_request_id.is_none() {
        state.local_restart_request_id = Some(format!(
            "v020-request-{}-{}-{}",
            generation, state.ice_generation, state.recovery_epoch
        ));
    }
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("restart request id initialized");
    state.local_restart_request_attempt = state.local_restart_request_attempt.saturating_add(1);
    tracing::debug!(
        peer = %state.peer_id.to_string_repr(),
        generation,
        %request_id,
        attempt = state.local_restart_request_attempt,
        "v0.20 answerer sending ICE restart request"
    );
    effects.push(RtcPeerEffect::SendSignaling {
        generation,
        message: SignalingMessage::IceRestartRequest {
            request_id,
            attempt: state.local_restart_request_attempt,
        },
    });
}

/// Retire a physical Session generation while leaving the logical transport
/// attempt alive. A building generation has not necessarily been registered
/// yet; clearing its slot makes a later successful completion follow the
/// orphan-close rule in `reduce_build_session`.
fn retire_generation(state: &mut RtcPeerState, generation: u64, effects: &mut Vec<RtcPeerEffect>) {
    let mut has_session = false;
    let mut changed = false;
    if state.active_generation == Some(generation) {
        if state.role == Role::Answerer {
            state.parked_signaling_generation = Some(generation);
        }
        state.active_generation = None;
        has_session = true;
        changed = true;
    }
    if state.pending_generation == Some(generation) {
        state.pending_generation = None;
        has_session = true;
        changed = true;
    }
    if state.building_generation == Some(generation) {
        state.building_generation = None;
        changed = true;
    }
    if has_session && state.sessions_to_close.insert(generation) {
        schedule_close_session(state, generation, effects);
    }
    if changed {
        tracing::debug!(
            peer = %state.peer_id.to_string_repr(),
            generation,
            "v0.20 session generation retired"
        );
        state.bump_revision();
    }
}

/// Drop every fact/cache scoped to the logical transport attempt. Monotonic
/// identities, diagnostics, outstanding close operations, and revision floors
/// deliberately survive so late completions remain harmless and observable.
fn retain_lifecycle_operations(state: &mut RtcPeerState) {
    // CloseSession effects remain authenticated until the physical close
    // barrier reports completion. An unfinished BuildSession must also retain
    // its completion identity so a late success can be recognized and closed
    // as an orphan. Every other operation belongs to the retired attempt.
    state.pending_operations.retain(|_, operation| {
        matches!(
            operation.kind,
            PendingRtcOperationKind::BuildSession | PendingRtcOperationKind::CloseSession
        )
    });
}

fn clear_transport_attempt_cache(state: &mut RtcPeerState) {
    retain_lifecycle_operations(state);
    state.role = Role::Undetermined;
    state.role_negotiation_inflight = false;
    state.awaiting_offer = false;
    state.initial_connection_inflight = false;
    state.ice_generation = 0;
    state.recovery_inflight = false;
    state.recovery_negotiated = false;
    state.recovery_attempt_started = false;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    state.pending_pc_connected = false;
    state.pending_rpc_reliable_open = false;
    state.network_path = crate::wire::webrtc::v020::status::NetworkPath::Unknown;
    state.stats_sample_attempts = 0;
    state.address_changed = false;
    state.address_edge_pending = false;
    state.pending_remote_offer = None;
    state.pending_remote_description_operation = None;
    state.queued_remote_description = None;
    state.applied_remote_description_generation = None;
    state.pending_local_desc = None;
    state.announced_local_description_generation = None;
    state.pending_local_candidates.clear();
    state.pending_local_sdp_exchange_id = None;
    state.pending_remote_sdp_exchange_id = None;
    state.pending_local_restart_exchange_id = None;
    state.local_restart_request_id = None;
    state.local_restart_request_attempt = 0;
    state.last_remote_restart_request = None;
    state.parked_signaling_generation = None;
    state.renegotiation_inflight = false;
    state.renegotiation_pending = false;
    state.local_ufrag = None;
    state.remote_ufrag = None;
    state.known_remote_ufrags.clear();
    state.pending_candidates.clear();
}

/// Converge a failed logical attempt to terminal `Idle`. Every registered
/// Session is explicitly retired; an unfinished build becomes an orphan whose
/// late successful completion is closed by the normal completion guard.
fn terminate_transport_attempt(
    state: &mut RtcPeerState,
    reason: RtcUnavailableReason,
    effects: &mut Vec<RtcPeerEffect>,
) {
    clear_transport_attempt_cache(state);

    state.building_generation = None;
    for generation in [
        state.active_generation.take(),
        state.pending_generation.take(),
    ]
    .into_iter()
    .flatten()
    {
        if state.sessions_to_close.insert(generation) {
            schedule_close_session(state, generation, effects);
        }
    }
    // A successful completion for an abandoned build is authenticated but no
    // longer live, so `reduce_build_session` schedules its orphan close.
    state.phase = RtcPeerPhase::Idle;
    state.unavailable_reason = Some(reason);
    tracing::warn!(
        peer = %state.peer_id.to_string_repr(),
        reason = ?reason,
        transport_epoch = state.transport_epoch,
        "v0.20 transport attempt terminated"
    );
    state.bump_revision();
}

/// S5-3/S5-4: start Offerer-side Hard Recovery — increase the PC generation and
/// build a replacement (RFC §"Hard Recovery" process: steps 2/3/4). When there
/// is an active session, uses make-before-break: the old session stays alive
/// while the replacement is built as `pending_generation`. When the pending
/// session is fully ready (connected + RpcReliable open), `atomic_switch` fires
/// (old → retiring, pending → active). When there is no active session (initial
/// build failure, orphaned), falls back to break-before-make. An Answerer has no
/// local Offer to negotiate a replacement, so it is parked by
/// `wait_for_offerer_offer` instead.
///
/// Offerer triggers: irrecoverable Soft Recovery failure, mailbox overflow,
/// `ForceReconnect`, driver exit, non-converging status,
/// socket/bind plan unusable, or app lifecycle policy (RFC §"Hard Recovery is
/// suitable for"). The same physical failures on an Answerer park the logical
/// attempt and wait for an Offerer-created Offer.
fn start_hard_recovery(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    if state.recovery_attempt_started
        && (state.building_generation.is_some() || state.pending_generation.is_some())
    {
        return;
    }
    if state.role == Role::Answerer {
        wait_for_offerer_offer(state);
        return;
    }
    if !recovery_gate_open(state) {
        defer_recovery_until_gate(state, true);
        return;
    }
    if !state.recovery_inflight {
        state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
    }
    // A replacement Session is a new recovery identity and therefore owns a
    // fresh, non-extendable deadline.
    state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
    state.recovery_attempt_started = true;
    state.recovery_inflight = true;
    state.recovery_negotiated = false;
    // The address-changed flag is consumed: Hard Recovery is in progress and
    // the new session will use the current local address.
    state.address_changed = false;
    state.address_edge_pending = false;

    // The old active generation can remain physically alive during
    // make-before-break, but its in-flight negotiation state no longer owns
    // this recovery flight. Invalidate SDP/ICE completions, correlation ids,
    // candidates, and renegotiation latches while retaining only Build/Close
    // completions needed for orphan cleanup and the close barrier.
    clear_negotiation_for_recovery(state);
    // Hard Recovery starts a new physical Session, so request identities from
    // the retired Session must not suppress wakes for the replacement. Soft
    // retries deliberately retain this field for duplicate-attempt fencing.
    state.last_remote_restart_request = None;

    // S5-4: make-before-break when there is an active session to retire. The
    // old session stays alive (its events are routed separately) while the
    // replacement is built. When there is no active session, fall back to
    // break-before-make (clear active state, nothing to keep alive).
    let make_before_break = state.active_generation.is_some();
    if !make_before_break {
        state.pc_connected = false;
        state.rpc_reliable_open = false;
    }
    // (If make-before-break: keep old active state; the pending session's
    // readiness is tracked by pending_pc_connected / pending_rpc_reliable_open.)

    // An in-flight build (if any) is abandoned; its completion will be dropped
    // as stale or closed as an orphan by `reduce_build_session`.
    state.building_generation = None;

    // RFC step 3: allocate a new PC generation (monotonic, never reused).
    let new_generation = state.alloc_generation();
    state.building_generation = Some(new_generation);
    state.ice_generation = 0; // new PC, fresh ICE generation epoch
    // Leaving the soft ladder: a later soft round after this rebuild starts
    // a fresh attempt count.
    state.soft_recovery_attempts = 0;
    state.phase = RtcPeerPhase::Building;
    state.unavailable_reason = None;

    // Clear ufrags/candidates from the old PC — they do not apply to the new
    // PeerConnection's ICE layer.
    state.local_ufrag = None;
    state.remote_ufrag = None;
    state.known_remote_ufrags.clear();
    state.role_negotiation_inflight = false;
    state.awaiting_offer = false;
    // S5-4: reset pending readiness facts for the new replacement.
    state.pending_pc_connected = false;
    state.pending_rpc_reliable_open = false;
    state.bump_revision();
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        role = ?state.role,
        recovery_cycle = state.recovery_cycle,
        recovery_epoch = state.recovery_epoch,
        new_generation,
        make_before_break,
        "v0.20 hard recovery started; building replacement PeerConnection"
    );

    // RFC step 4: create the replacement Session (pending if make-before-break).
    let operation_id = state.alloc_operation();
    effects.push(RtcPeerEffect::BuildSession {
        generation: new_generation,
        operation_id,
    });
}

/// Park an Answerer while waiting for the stable Offerer to initiate the next
/// physical connection.
///
/// `preserve_request` is used for a terminal `Failed` callback. A request may
/// already have been emitted by `Disconnected` or an explicit path edge; in
/// that case parking must not clear its identity or advance its recovery epoch.
/// The regular Hard-Recovery path passes `false`, because it starts a fresh
/// request exchange for the parked generation.
fn park_answerer_recovery(state: &mut RtcPeerState, preserve_request: bool) {
    // Hard recovery can be requested because the local address or lifecycle
    // changed while the active PeerConnection was still physically connected.
    // In that case the existing RpcReliable DataChannel remains the readiness
    // anchor for a same-generation ICE restart and will not emit another
    // `OnOpen`. A terminal/failed Session revokes this fact before calling this
    // helper; an active generation therefore tells us that the retained fact is
    // still eligible for reuse.
    let retain_active_lane = state.active_generation.is_some();
    let newly_parked_retired_session = !state.awaiting_offer
        && state.active_generation.is_none()
        && state.parked_signaling_generation.is_some();
    let existing_request = state.local_restart_request_id.clone();
    let existing_attempt = state.local_restart_request_attempt;
    let preserve_exchange = preserve_request
        && state.recovery_inflight
        && !newly_parked_retired_session
        && existing_request.is_some();
    state.parked_signaling_generation = state
        .active_generation
        .or(state.parked_signaling_generation);
    if !state.recovery_inflight || newly_parked_retired_session {
        state.recovery_cycle = state.recovery_cycle.wrapping_add(1);
    }
    if !preserve_exchange {
        state.recovery_epoch = state.recovery_epoch.wrapping_add(1);
    }
    clear_negotiation_for_recovery(state);
    if preserve_exchange {
        state.local_restart_request_id = existing_request;
        state.local_restart_request_attempt = existing_attempt;
    }
    state.recovery_inflight = true;
    state.recovery_attempt_started = false;
    state.recovery_negotiated = false;
    state.address_changed = false;
    state.pc_connected = false;
    if !retain_active_lane {
        state.rpc_reliable_open = false;
    }
    state.awaiting_offer = true;
    state.phase = RtcPeerPhase::Recovering;
    state.bump_revision();
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        role = ?state.role,
        recovery_cycle = state.recovery_cycle,
        recovery_epoch = state.recovery_epoch,
        generation = state.active_generation,
        newly_parked_retired_session,
        "v0.20 answerer waiting for offerer to initiate recovery"
    );
}

/// Compatibility wrapper for paths that intentionally begin a new parked
/// request exchange (Hard Recovery, failed replacement build, and a closed
/// Answerer). The terminal `Failed` path uses the preserving variant below.
fn wait_for_offerer_offer(state: &mut RtcPeerState) {
    park_answerer_recovery(state, false);
}

/// Exhaust one complete recovery cycle without terminating the Controller.
/// Every existing Session is retired, an in-flight build is orphaned (its late
/// successful completion is explicitly closed by `reduce_build_session`), and
/// business observers receive terminal Idle + RecoveryExhausted. A later
/// explicit Connect starts a fresh cycle and generation.
fn exhaust_recovery(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    tracing::warn!(
        peer = %state.peer_id.to_string_repr(),
        recovery_cycle = state.recovery_cycle,
        "v0.20 recovery cycle exhausted; terminating the transport attempt"
    );
    terminate_transport_attempt(state, RtcUnavailableReason::RecoveryExhausted, effects);
}

/// Explicit close: request close, move to `Closing`, and close every *existing*
/// session. An in-flight build has no session yet; its completion is handled by
/// the close barrier in `reduce_completion` (it closes the orphan instead of
/// activating it). The controller leaves `Closing` only once every session's
/// close has completed (see `CloseSession` handling).
fn reduce_close(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    if state.phase == RtcPeerPhase::Idle
        && state.building_generation.is_none()
        && state.active_generation.is_none()
        && state.pending_generation.is_none()
        && state.sessions_to_close.is_empty()
        && !has_outstanding_build(state)
    {
        // An explicit close of a freshly-created/otherwise idle controller is
        // still terminal.  The controller task must exit even when another
        // internal handle clone (for example the manager's Closing tombstone)
        // keeps the command channel alive.
        // A graceful close is as much a logical-transport terminal state as a
        // failure: stamping the reason lets the controller's retirement edge
        // publish `peer_transport_retired`, so the outbound DestTransport is
        // reclaimed instead of lingering as a stale `Connected` entry that a
        // later explicit connect would silently reuse (mp06).
        state.unavailable_reason = Some(RtcUnavailableReason::Closed);
        state.terminal_close_done = true;
        return;
    }
    state.close_requested = true;
    retain_lifecycle_operations(state);
    state.pending_remote_description_operation = None;
    state.queued_remote_description = None;
    state.applied_remote_description_generation = None;
    state.pending_local_desc = None;
    state.announced_local_description_generation = None;
    state.pending_local_candidates.clear();
    state.pending_local_sdp_exchange_id = None;
    state.pending_remote_sdp_exchange_id = None;
    state.pending_local_restart_exchange_id = None;
    state.role_negotiation_inflight = false;
    state.awaiting_offer = false;
    state.phase = RtcPeerPhase::Closing;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    state.unavailable_reason = None;
    state.bump_revision();

    for generation in [state.active_generation, state.pending_generation]
        .into_iter()
        .flatten()
    {
        if state.sessions_to_close.insert(generation) {
            schedule_close_session(state, generation, effects);
        }
    }

    // `Building` also covers role arbitration before a PC build exists.  In
    // that state there is no asynchronous close completion to drive the
    // barrier, so finish it synchronously when no generation is outstanding.
    maybe_finish_close(state);
}

/// Issue an explicit generation close with its own operation identity. Close is
/// asynchronous just like SDP/ICE operations; sharing only a generation is not
/// sufficient when an orphan close overlaps controller teardown.
fn schedule_close_session(
    state: &mut RtcPeerState,
    generation: u64,
    effects: &mut Vec<RtcPeerEffect>,
) {
    let operation_id = state.alloc_operation();
    effects.push(RtcPeerEffect::CloseSession {
        generation,
        operation_id,
    });
}

fn reduce_state_tick(
    state: &mut RtcPeerState,
    tick: &PeerStateTick,
    effects: &mut Vec<RtcPeerEffect>,
) {
    // The coalesced state arrives at its latest value; apply it idempotently.
    // Stale generations never change newer state (invariant #5).
    if !state.is_live_generation(tick.generation) {
        state.stale_events += 1;
        return;
    }
    if state.close_requested {
        return;
    }

    // S5-4: route pending session state ticks separately (make-before-break).
    // The pending session's connected/open state is tracked by
    // pending_pc_connected / pending_rpc_reliable_open, not the active
    // session's fields.
    if Some(tick.generation) == state.pending_generation {
        reduce_pending_state_tick(state, tick, effects);
        return;
    }

    // S5-4: ignore old active session's state ticks during make-before-break.
    // Non-terminal facts must not change recovery progress. A terminal
    // Failed/Closed fact still owns physical cleanup: retire the dead old
    // generation while allowing the already-pending replacement to continue.
    if Some(tick.generation) == state.active_generation && state.pending_generation.is_some() {
        let terminal = matches!(
            tick.pc_state,
            Some(RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed)
        ) || matches!(
            tick.ice_state,
            Some(RTCIceConnectionState::Failed | RTCIceConnectionState::Closed)
        );
        if terminal {
            state.pc_connected = false;
            state.rpc_reliable_open = false;
            retire_generation(state, tick.generation, effects);
        }
        return;
    }

    let failed = matches!(tick.pc_state, Some(RTCPeerConnectionState::Failed))
        || matches!(tick.ice_state, Some(RTCIceConnectionState::Failed));
    let disconnected = matches!(tick.pc_state, Some(RTCPeerConnectionState::Disconnected))
        || matches!(tick.ice_state, Some(RTCIceConnectionState::Disconnected));
    let closed = matches!(tick.pc_state, Some(RTCPeerConnectionState::Closed))
        || matches!(tick.ice_state, Some(RTCIceConnectionState::Closed));
    // PC and ICE callbacks are independently coalesced into one latest-value
    // tick. During a transition it is valid to observe a terminal/degraded PC
    // state alongside the preceding ICE Connected value (or vice versa). That
    // stale positive fact must not hide Failed/Disconnected/Closed.
    let connected = !failed
        && !disconnected
        && !closed
        && (matches!(tick.pc_state, Some(RTCPeerConnectionState::Connected))
            || matches!(
                tick.ice_state,
                Some(RTCIceConnectionState::Connected | RTCIceConnectionState::Completed)
            ));

    if connected {
        // A coalesced Connected callback from the pre-restart ICE state can
        // arrive after NetworkUnavailable/RestartIce.  It is not evidence that
        // the current recovery exchange succeeded.  Only callbacks observed
        // after the matching offer/answer completion may restore readiness.
        if state.recovery_inflight && !state.recovery_negotiated {
            return;
        }
        let was_connected = state.pc_connected;
        state.pc_connected = true;
        state.ever_connected = true;
        if state.phase == RtcPeerPhase::Negotiating {
            state.phase = RtcPeerPhase::TransportReady;
        }
        // S5-2: Recovering → TransportReady on re-connect after ICE restart.
        if state.phase == RtcPeerPhase::Recovering {
            state.phase = RtcPeerPhase::TransportReady;
        }
        // Transport-ready + anchor already open ⇒ sendable.
        if state.phase == RtcPeerPhase::TransportReady && state.rpc_reliable_open {
            state.phase = RtcPeerPhase::Sendable;
            state.recovery_inflight = false;
            state.recovery_negotiated = true;
            state.recovery_attempt_started = false;
            state.local_restart_request_id = None;
            state.local_restart_request_attempt = 0;
            state.soft_recovery_attempts = 0;
            state.parked_signaling_generation = None;
            state.address_changed = false; // recovery succeeded
            if state.address_edge_pending {
                // An address edge was coalesced while this round was in
                // flight; the round converged on the pre-edge set. Publish
                // one more restart so the link moves onto the current set
                // instead of stranding on the superseded interface.
                start_soft_recovery(state, effects);
            }
        }
        if !was_connected || state.phase == RtcPeerPhase::Sendable {
            state.bump_revision();
        }
    } else if failed {
        if state.close_requested {
            return; // teardown already owns the transition
        }
        state.pc_connected = false;
        if state.ever_connected {
            // S5-2/S5-3: a previously-connected peer failing transitions to
            // Recovering. Only the Offerer can choose between Soft Recovery
            // and a new Hard-Recovery PC. An Answerer cannot restart its
            // physical PC locally, but it immediately wakes the Offerer with a
            // request-only recovery effect and then waits for the Offer.
            if state.role == Role::Answerer {
                // The physical Session has failed, so any readiness fact from
                // its DataChannel is stale. Retire the failed generation before
                // parking: keeping it live lets the data-plane pump continue
                // accepting requests while the response path correctly refuses
                // to send on the same non-sendable generation.
                tracing::warn!(
                    peer = %state.peer_id.to_string_repr(),
                    generation = tick.generation,
                    phase = ?state.phase,
                    pc_connected = state.pc_connected,
                    rpc_reliable_open = state.rpc_reliable_open,
                    recovery_inflight = state.recovery_inflight,
                    "v0.20 Answerer physical session failed; retiring generation before recovery park"
                );
                let recovery_cycle_before_failure = state.recovery_cycle;
                let recovery_was_inflight = state.recovery_inflight;
                let recovery_wake_was_sent = state.recovery_attempt_started
                    && state.local_restart_request_id.is_some()
                    && state.local_restart_request_attempt > 0;
                let was_already_parked = state.awaiting_offer && state.recovery_inflight;
                let preserved_request_id = state.local_restart_request_id.clone();
                let preserved_attempt = state.local_restart_request_attempt;
                state.rpc_reliable_open = false;
                retire_generation(state, tick.generation, effects);
                wait_for_offerer_offer(state);
                // Retiring a dead Session changes the physical generation, not
                // the logical recovery budget. If a soft recovery was already
                // in flight, keep its original cycle so this failure cannot
                // extend the end-to-end deadline while the Answerer is parked.
                if recovery_was_inflight {
                    state.recovery_cycle = recovery_cycle_before_failure;
                }
                // A Failed fact can be the first recovery signal, so the
                // passive side must still wake the Offerer. If a recovery wake
                // was already in flight, retain its identity and advance the
                // attempt rather than creating a second logical recovery.
                if let Some(request_id) = preserved_request_id {
                    state.local_restart_request_id = Some(request_id);
                    state.local_restart_request_attempt = preserved_attempt;
                }
                state.recovery_attempt_started = recovery_wake_was_sent;
                // A Failed edge arriving during an unparked soft recovery is
                // a new immediate wake and advances the existing request. A
                // duplicate Failed edge after parking remains idempotent; the
                // attempt timer owns later retransmissions.
                if !recovery_wake_was_sent || !was_already_parked {
                    state.recovery_attempt_started = true;
                    send_answerer_restart_request(state, effects);
                }
            } else if !state.recovery_inflight {
                state.phase = RtcPeerPhase::Recovering;
                if !state.address_changed {
                    start_soft_recovery(state, effects);
                } else {
                    // S5-3: an explicit hard-recovery request → Offerer Hard
                    // Recovery.
                    start_hard_recovery(state, effects);
                }
            } else if state.address_changed {
                // S5-3: Soft Recovery was in flight but an explicit
                // hard-recovery request arrived — escalate the Offerer to
                // Hard Recovery. An Answerer remains parked until the
                // Offerer sends a fresh Offer.
                start_hard_recovery(state, effects);
            }
        } else {
            // Initial connection failed.
            terminate_transport_attempt(state, RtcUnavailableReason::NegotiationFailed, effects);
            return;
        }
        state.bump_revision();
    } else if disconnected {
        if state.close_requested {
            return;
        }
        state.pc_connected = false;
        if state.phase == RtcPeerPhase::Sendable || state.phase == RtcPeerPhase::TransportReady {
            state.phase = RtcPeerPhase::Recovering;
            if !state.address_changed {
                // S5-2: temporary network loss starts Soft Recovery.
                start_soft_recovery(state, effects);
            } else {
                // S5-3: explicit hard recovery → Offerer Hard Recovery; an
                // Answerer waits for the Offerer to initiate a new Offer.
                start_hard_recovery(state, effects);
            }
        }
    } else if closed && !state.close_requested {
        // A physically closed established Session cannot Soft Recover. Retire
        // that generation and let the Offerer rebuild under the same logical
        // transport flight; an Answerer waits for the resulting fresh Offer.
        // A close during the initial attempt is terminal and must release both
        // the Session and the transport-layer cache entry.
        state.pc_connected = false;
        state.rpc_reliable_open = false;
        if state.ever_connected {
            retire_generation(state, tick.generation, effects);
            // A replacement may already be building because recovery started
            // before the old Session reported Closed. Keep that replacement;
            // abandoning it would create an unnecessary extra generation and
            // could strand its late successful completion.
            if state.building_generation.is_none() && state.pending_generation.is_none() {
                state.recovery_attempt_started = false;
                start_hard_recovery(state, effects);
            }
        } else {
            terminate_transport_attempt(state, RtcUnavailableReason::Closed, effects);
        }
    }
}

fn reduce_driver_event(
    state: &mut RtcPeerState,
    event: PeerDriverEvent,
    effects: &mut Vec<RtcPeerEffect>,
) {
    let generation = event.generation();
    if !state.is_live_generation(generation) {
        state.stale_events += 1;
        return;
    }
    if state.close_requested {
        return;
    }
    // A lazily-created lane/media mid-connection triggers `on_negotiation_needed`
    // (RFC §"DataChannel APIs": renegotiation is serialized by the Controller
    // effect). Only the offerer renegotiates, and only once the connection is
    // established (the initial offer/answer is driven by the connect flow, not by
    // this event — re-firing it would loop). Duplicate events while one is in
    // flight are merged (RFC §"DataChannel APIs": "merge duplicate
    // on_negotiation_needed() events").
    //
    // BREAKING (v0.20, RFC-0504 §"Deliberate regressions"): an answerer-side
    // `NegotiationNeeded` (the answerer added/removed a local media track) is
    // deliberately dropped — answerer-initiated media renegotiation is NOT
    // supported. The 0.14 stack re-offered from either role
    // (`Coordinator::renegotiate_connection` had no role check); v0.20 only
    // renegotiates from the offerer. An answerer that needs to publish media
    // must ask the offerer (out of band) to add the track, or reconnect with
    // swapped roles. The answerer CAN still apply an incoming offerer re-offer
    // (see `answerer_applies_same_ufrag_offer_as_renegotiation`).
    if let PeerDriverEvent::NegotiationNeeded { generation } = event {
        if state.role != Role::Offerer || !state.sendable() {
            tracing::debug!(
                peer = %state.peer_id.to_string_repr(),
                generation,
                role = ?state.role,
                "v0.20 negotiation-needed ignored: only a sendable offerer renegotiates"
            );
            return;
        }
        if state.renegotiation_inflight {
            state.renegotiation_pending = true;
            return;
        }
        state.renegotiation_inflight = true;
        let operation_id = state.alloc_operation();
        tracing::debug!(
            peer = %state.peer_id.to_string_repr(),
            generation,
            operation_id,
            "v0.20 renegotiation started"
        );
        effects.push(RtcPeerEffect::CreateOffer {
            generation,
            operation_id,
        });
        return;
    }

    if let PeerDriverEvent::DriverExited { generation, .. } = &event {
        if state.ever_connected {
            retire_generation(state, *generation, effects);
            state.pc_connected = false;
            state.rpc_reliable_open = false;
            if state.building_generation.is_none() && state.pending_generation.is_none() {
                state.recovery_attempt_started = false;
                start_hard_recovery(state, effects);
            }
        } else {
            terminate_transport_attempt(state, RtcUnavailableReason::Closed, effects);
        }
        return;
    }

    // DataChannel/Track registration and readiness are data-plane concerns
    // (Stage 4/6); the reducer's readiness comes from LaneEvent, so only a local
    // candidate drives a control effect here (forward to the remote via
    // signaling). Other structural events are observed (and stale-checked above)
    // but drive no control transition.
    if let PeerDriverEvent::LocalIceCandidate { candidate, .. } = event
        && let Ok(mut init) = candidate.to_json()
    {
        // During make-before-break the old active Session remains physically
        // live only as a retirement fallback. Its late candidates must never
        // be stamped with the replacement's scalar ufrag or sent into the new
        // signaling exchange.
        let signaling_generation = state
            .pending_generation
            .or(state.building_generation)
            .or(state.active_generation);
        if signaling_generation != Some(generation) {
            state.stale_events = state.stale_events.saturating_add(1);
            return;
        }
        // Stamp the local ufrag so the remote can tag this candidate to our ICE
        // generation (RTCIceCandidate::to_json does not populate it; mirrors
        // 0.14 serialize_local_ice_candidate).
        if init.username_fragment.is_none() {
            init.username_fragment = state.local_ufrag.clone();
        }
        if state.announced_local_description_generation == Some(generation) {
            effects.push(RtcPeerEffect::SendSignaling {
                generation,
                message: SignalingMessage::Candidate(init),
            });
        } else {
            buffer_local_candidate(state, generation, init);
        }
    }
}

fn reduce_lane_event(state: &mut RtcPeerState, event: LaneEvent, effects: &mut Vec<RtcPeerEffect>) {
    match event {
        LaneEvent::Opened { generation, lane } => {
            // S5-4: route pending generation's lane events to pending fields.
            if Some(generation) == state.pending_generation {
                if lane == LaneKind::READINESS_ANCHOR {
                    state.pending_rpc_reliable_open = true;
                    // Check if the pending session is ready for atomic switch.
                    if state.pending_pc_connected {
                        atomic_switch(state, effects);
                    } else {
                        state.bump_revision();
                    }
                }
                return;
            }
            // S5-4: ignore old active session's lane events during
            // make-before-break (the old session is retiring).
            if Some(generation) == state.active_generation && state.pending_generation.is_some() {
                return;
            }
            if Some(generation) != state.active_generation {
                state.stale_events += 1;
                return;
            }
            if lane == LaneKind::READINESS_ANCHOR {
                state.rpc_reliable_open = true;
                if state.phase == RtcPeerPhase::TransportReady && state.pc_connected {
                    state.phase = RtcPeerPhase::Sendable;
                    state.recovery_inflight = false;
                    state.recovery_negotiated = true;
                    state.recovery_attempt_started = false;
                    state.local_restart_request_id = None;
                    state.local_restart_request_attempt = 0;
                    state.soft_recovery_attempts = 0;
                    state.parked_signaling_generation = None;
                }
                state.bump_revision();
            }
        }
        LaneEvent::Closed { generation, lane } => {
            // S5-4: ignore pending and old-active lane-closed during
            // make-before-break (neither affects the recovery).
            if Some(generation) == state.pending_generation {
                // Exception: the pending session's READINESS anchor. The
                // controller already removed its lane handle, so a closed
                // anchor leaves the pending session permanently unusable —
                // a later connected tick with stale pending readiness would
                // atomic-switch this dead generation into the active slot
                // and tear down the working connection. Retire the pending
                // generation and clear its readiness atomically instead;
                // the active generation (if any) stays and the recovery
                // driver re-attempts a replacement.
                if lane == LaneKind::READINESS_ANCHOR {
                    retire_generation(state, generation, effects);
                    state.pending_pc_connected = false;
                    state.pending_rpc_reliable_open = false;
                }
                return;
            }
            if Some(generation) == state.active_generation && state.pending_generation.is_some() {
                return;
            }
            if Some(generation) != state.active_generation {
                state.stale_events += 1;
                return;
            }
            if lane == LaneKind::READINESS_ANCHOR {
                state.rpc_reliable_open = false;
                if state.phase == RtcPeerPhase::Sendable {
                    // The readiness anchor is a physical Session resource.  A
                    // closed DataChannel cannot be reopened by ICE restart, so
                    // this is not a healthy-Session Soft Recovery case.  Revoke
                    // sendability immediately and build a fresh PC generation
                    // (RFC: channel close -> Recovering; unhealthy Session ->
                    // Hard Recovery).
                    start_hard_recovery(state, effects);
                }
            }
        }
    }
}

/// S5-4: atomic switch — pending → active, old active → retiring (RFC §"Hard
/// Recovery" steps 7/8/9). Called when the pending session is fully ready
/// (connected + RpcReliable open). The old active session is closed explicitly;
/// its events become stale. The pending session's readiness facts are copied to
/// the active fields. The peer becomes Sendable.
fn atomic_switch(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    // RFC step 7: atomic switching of active generation.
    let retired_generation = state.active_generation.take();
    if let Some(old) = retired_generation {
        // RFC step 9: mark old for retiring and close explicitly.
        state.sessions_to_close.insert(old);
        schedule_close_session(state, old, effects);
    }
    // Pending → active.
    let new_gen = state
        .pending_generation
        .take()
        .expect("atomic_switch called with pending_generation set");
    state.active_generation = Some(new_gen);
    // Copy pending readiness to active fields.
    state.pc_connected = state.pending_pc_connected;
    state.rpc_reliable_open = state.pending_rpc_reliable_open;
    state.pending_pc_connected = false;
    state.pending_rpc_reliable_open = false;
    // RFC step 8: publish Connected.
    state.recovery_inflight = false;
    state.recovery_negotiated = true;
    state.recovery_attempt_started = false;
    state.local_restart_request_id = None;
    state.local_restart_request_attempt = 0;
    state.parked_signaling_generation = None;
    state.phase = RtcPeerPhase::Sendable;
    state.bump_revision();
    tracing::info!(
        peer = %state.peer_id.to_string_repr(),
        retired_generation = ?retired_generation,
        generation = new_gen,
        "v0.20 atomic generation switch: replacement active"
    );
    // Buffered candidates matching the new active ufrag can now be injected.
    flush_pending_candidates(state, effects);
}

/// S5-4: handle state ticks for the pending (make-before-break) session. The
/// pending session's connected state is tracked separately; when both
/// connected and anchor-open are true, `atomic_switch` fires.
fn reduce_pending_state_tick(
    state: &mut RtcPeerState,
    tick: &PeerStateTick,
    effects: &mut Vec<RtcPeerEffect>,
) {
    let failed_or_closed = matches!(
        tick.pc_state,
        Some(RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed)
    ) || matches!(
        tick.ice_state,
        Some(RTCIceConnectionState::Failed | RTCIceConnectionState::Closed)
    );
    if failed_or_closed {
        retire_generation(state, tick.generation, effects);
        state.pending_pc_connected = false;
        state.pending_rpc_reliable_open = false;
        state.recovery_attempt_started = false;
        start_hard_recovery(state, effects);
        return;
    }

    let connected = matches!(tick.pc_state, Some(RTCPeerConnectionState::Connected))
        || matches!(
            tick.ice_state,
            Some(RTCIceConnectionState::Connected | RTCIceConnectionState::Completed)
        );

    if connected {
        state.pending_pc_connected = true;
        // Negotiating → TransportReady for the pending session.
        if state.phase == RtcPeerPhase::Negotiating {
            state.phase = RtcPeerPhase::TransportReady;
        }
        // Check if the pending session is ready for atomic switch.
        if state.pending_rpc_reliable_open {
            atomic_switch(state, effects);
        } else {
            state.bump_revision();
        }
    }
    // A disconnected pending session remains under the current attempt
    // deadline; a terminal Failed/Closed session above is explicitly retired
    // and replaced immediately.
}

fn reduce_completion(
    state: &mut RtcPeerState,
    completion: EffectCompletion,
    effects: &mut Vec<RtcPeerEffect>,
) {
    match completion {
        EffectCompletion::BuildSession {
            generation, result, ..
        } => reduce_build_session(state, generation, result, effects),
        EffectCompletion::CreateOffer {
            generation, result, ..
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            match result {
                Ok(desc) => {
                    let operation_id = state.alloc_operation();
                    // Learn the local ufrag from our own offer/answer so outgoing
                    // candidates can be stamped with it.
                    if let Some(ufrag) = super::state::session_ice_ufrag(&desc.sdp) {
                        state.local_ufrag = Some(ufrag);
                    }
                    state.announced_local_description_generation = None;
                    state.pending_local_desc = Some(desc.clone());
                    effects.push(RtcPeerEffect::SetLocalDescription {
                        generation,
                        operation_id,
                        desc,
                    });
                }
                Err(_) => {
                    state.pending_local_restart_exchange_id = None;
                    negotiation_failed(state, effects);
                }
            }
        }
        EffectCompletion::CreateAnswer {
            generation, result, ..
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            match result {
                Ok(desc) => {
                    let operation_id = state.alloc_operation();
                    if let Some(ufrag) = super::state::session_ice_ufrag(&desc.sdp) {
                        state.local_ufrag = Some(ufrag);
                    }
                    state.announced_local_description_generation = None;
                    state.pending_local_desc = Some(desc.clone());
                    effects.push(RtcPeerEffect::SetLocalDescription {
                        generation,
                        operation_id,
                        desc,
                    });
                }
                Err(_) => negotiation_failed(state, effects),
            }
        }
        EffectCompletion::SetLocalDescription {
            generation,
            operation_id,
            result,
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            match result {
                Ok(()) => {
                    // Local description applied; hand it to the signaling router.
                    if let Some(desc) = state.pending_local_desc.take() {
                        let completes_recovery_answer = state.recovery_inflight
                            && state.role == Role::Answerer
                            && desc.sdp_type == RTCSdpType::Answer;
                        let message = match desc.sdp_type {
                            RTCSdpType::Offer => {
                                if let Some(exchange_id) =
                                    state.pending_local_restart_exchange_id.take()
                                {
                                    state.pending_local_sdp_exchange_id = Some(exchange_id.clone());
                                    SignalingMessage::IceRestartOffer { desc, exchange_id }
                                } else {
                                    let exchange_id =
                                        format!("v020-sdp-{generation}-{operation_id}");
                                    state.pending_local_sdp_exchange_id = Some(exchange_id.clone());
                                    SignalingMessage::Offer {
                                        desc,
                                        exchange_id: Some(exchange_id),
                                    }
                                }
                            }
                            _ => SignalingMessage::Answer {
                                desc,
                                exchange_id: state.pending_remote_sdp_exchange_id.take(),
                            },
                        };
                        let (sdp_kind, announced_exchange_id) = match &message {
                            SignalingMessage::Offer { exchange_id, .. } => {
                                ("offer", exchange_id.clone())
                            }
                            SignalingMessage::IceRestartOffer { exchange_id, .. } => {
                                ("ice-restart-offer", Some(exchange_id.clone()))
                            }
                            SignalingMessage::Answer { exchange_id, .. } => {
                                ("answer", exchange_id.clone())
                            }
                            _ => ("sdp", None),
                        };
                        tracing::debug!(
                            peer = %state.peer_id.to_string_repr(),
                            generation,
                            sdp_kind,
                            exchange_id = ?announced_exchange_id,
                            "v0.20 local description announced via signaling"
                        );
                        effects.push(RtcPeerEffect::SendSignaling {
                            generation,
                            message,
                        });
                        state.announced_local_description_generation = Some(generation);
                        // Preserve signaling FIFO: the SDP effect above must be
                        // submitted before any candidate that WebRTC emitted
                        // while set_local_description() was still in flight.
                        flush_local_candidates(state, generation, effects);
                        if completes_recovery_answer {
                            state.recovery_negotiated = true;
                        }
                    }
                    // The offer was sent: this renegotiation round completes. If
                    // another negotiation-needed arrived while in flight, kick off
                    // the next offer (RFC: merge duplicate on_negotiation_needed).
                    if state.renegotiation_inflight {
                        state.renegotiation_inflight = false;
                        if state.renegotiation_pending {
                            state.renegotiation_pending = false;
                            state.renegotiation_inflight = true;
                            let operation_id = state.alloc_operation();
                            effects.push(RtcPeerEffect::CreateOffer {
                                generation,
                                operation_id,
                            });
                        }
                    }
                }
                Err(_) => {
                    state.pending_local_candidates.clear();
                    state.announced_local_description_generation = None;
                    state.pending_local_sdp_exchange_id = None;
                    state.pending_local_restart_exchange_id = None;
                    negotiation_failed(state, effects);
                }
            }
        }
        EffectCompletion::SetRemoteDescription {
            generation,
            operation_id,
            result,
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            if state.pending_remote_description_operation != Some((generation, operation_id)) {
                state.stale_completions += 1;
                return;
            }
            state.pending_remote_description_operation = None;
            match result {
                Ok(()) => {
                    state.applied_remote_description_generation = Some(generation);
                    if state.recovery_inflight && state.role == Role::Offerer {
                        // The restart/replacement answer is now applied. A
                        // subsequent Connected callback belongs to this
                        // recovery epoch and may restore sendability.
                        state.recovery_negotiated = true;
                    }
                    // WebRTC now has a remote description for this generation;
                    // matching trickle candidates can safely be injected.
                    flush_pending_candidates(state, effects);

                    // A newer remote description arrived while this operation
                    // was running. Apply it next; only the final description in
                    // the serialized chain may advance CreateAnswer.
                    if let Some((queued_generation, desc)) = state.queued_remote_description.take()
                    {
                        if state.is_live_generation(queued_generation) {
                            schedule_remote_description(state, queued_generation, desc, effects);
                        } else {
                            state.stale_events += 1;
                        }
                    } else if state.role == Role::Answerer {
                        // The answerer follows a remote offer with an answer;
                        // the offerer is done applying the answer and awaits
                        // transport.
                        let operation_id = state.alloc_operation();
                        effects.push(RtcPeerEffect::CreateAnswer {
                            generation,
                            operation_id,
                        });
                    }
                }
                Err(_) => {
                    state.queued_remote_description = None;
                    negotiation_failed(state, effects);
                }
            }
        }
        EffectCompletion::AddIceCandidate { generation, .. } => {
            // Best-effort; only staleness matters.
            if !state.is_live_generation(generation) {
                state.stale_completions += 1;
            }
        }
        EffectCompletion::RestartIce {
            generation, result, ..
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            match result {
                Ok(()) => {
                    // S5-2: Soft Recovery step 5 — the offerer creates a
                    // restart Offer (with a new ICE ufrag from restart_ice).
                    // The answerer never reaches this branch: its soft
                    // recovery sends the restart request directly without a
                    // local restart_ice() effect (see start_soft_recovery),
                    // and its local restart happens implicitly when the
                    // remote restart offer is applied.
                    if state.role == Role::Offerer {
                        let operation_id = state.alloc_operation();
                        state.pending_local_restart_exchange_id = Some(format!(
                            "v020-{}-{}-{}",
                            generation, state.ice_generation, operation_id
                        ));
                        effects.push(RtcPeerEffect::CreateOffer {
                            generation,
                            operation_id,
                        });
                    } else if state.role == Role::Answerer {
                        // A RestartIce completion on an answerer means the
                        // effect was dispatched before this role's recovery
                        // switched to request-only (e.g. role resolved after
                        // dispatch). The request may already have been sent;
                        // re-sending the same correlated attempt is idempotent
                        // on the offerer (duplicate attempt is dropped), so
                        // stay silent here and let the attempt deadline own
                        // retransmission.
                        tracing::debug!(
                            peer = %state.peer_id.to_string_repr(),
                            generation,
                            "answerer RestartIce completion ignored; request-only recovery owns signaling"
                        );
                    }
                }
                Err(_) => {
                    // S5-3: Soft Recovery failed — escalate to Hard Recovery
                    // (PC generation + 1). The old generation is retired and
                    // a replacement is built.
                    start_hard_recovery(state, effects);
                }
            }
        }
        EffectCompletion::RefreshConfiguration {
            generation, result, ..
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            if result.is_err() {
                // S5-6: the configuration change was rejected by the
                // PeerConnection (e.g. incompatible ICE transport policy,
                // invalid TURN credential). Escalate to Hard Recovery — the
                // current session cannot apply the new configuration.
                start_hard_recovery(state, effects);
            }
            // On success, the config was applied in-place; no renegotiation
            // is needed (set_configuration only updates ICE servers/TURN).
        }
        EffectCompletion::GetStats {
            generation, report, ..
        } => {
            if !completion_applies(state, generation) {
                return;
            }
            // S6-3: resolve the selected pair into network_path. An
            // inconsistent report keeps the previous value (RFC: stats are
            // "maintained when data is missing or reports are inconsistent").
            if let Some((local, remote)) =
                crate::wire::webrtc::v020::stats::selected_pair_candidate_types(&report)
            {
                state.stats_sample_attempts = 0;
                let path = crate::wire::webrtc::v020::stats::network_path_from_candidate_types(
                    local, remote,
                );
                if path != state.network_path {
                    state.network_path = path;
                    state.bump_revision();
                }
            } else if state.phase == RtcPeerPhase::Sendable
                && state.active_generation == Some(generation)
                && state.stats_sample_attempts < MAX_STATS_SAMPLE_ATTEMPTS
            {
                state.stats_sample_attempts += 1;
                let operation_id = state.alloc_operation();
                effects.push(RtcPeerEffect::GetStats {
                    generation,
                    operation_id,
                });
            } else {
                state.stats_sample_attempts = 0;
            }
        }
        EffectCompletion::CloseSession { generation, .. } => {
            state.sessions_to_close.remove(&generation);
            if state.active_generation == Some(generation) {
                state.active_generation = None;
            }
            if state.pending_generation == Some(generation) {
                state.pending_generation = None;
            }
            // Close barrier: leave Closing only once every session's close has
            // completed and no build is still in flight. The convergence body
            // lives in `maybe_finish_close` (single source: it also stamps the
            // graceful-close reason for the retirement edge).
            maybe_finish_close(state);
        }
    }
}

/// Handle a `BuildSession` completion separately because it carries the close
/// barrier's "orphan session" rule: a build that finishes while closing (late
/// success) or for a superseded generation must be closed, never activated.
/// The physical Session has already been registered in the effect runner; the
/// reducer sees only whether that registration/build succeeded.
fn reduce_build_session(
    state: &mut RtcPeerState,
    generation: u64,
    result: Result<(), RtcError>,
    effects: &mut Vec<RtcPeerEffect>,
) {
    // Late completion during close. Only the completion for the genuinely
    // in-flight build is treated as the orphan to close; a completion for any
    // other (stale/never-allocated) generation is ignored and must not clear the
    // real build or close a non-existent session.
    if state.close_requested {
        let expected_build = state.building_generation == Some(generation);
        let abandoned_terminal_build = state.building_generation.is_none()
            && generation < state.next_pc_generation
            && !state.is_live_generation(generation);
        if expected_build || abandoned_terminal_build {
            state.building_generation = None;
            match result {
                Ok(()) => {
                    // Late success while closing → close the freshly built session.
                    state.sessions_to_close.insert(generation);
                    schedule_close_session(state, generation, effects);
                }
                Err(_) => {
                    // Late failure while closing → nothing to close; the barrier
                    // may now be satisfied.
                    maybe_finish_close(state);
                }
            }
        } else {
            state.stale_completions += 1;
        }
        return;
    }

    // Superseded build (a newer build replaced this generation): close the
    // orphan — but only if this generation was actually built (allocated, and
    // not currently live in another slot). A completion for a never-allocated
    // generation, or for one that is already active/pending, is merely stale and
    // must not close a live or non-existent session.
    if state.building_generation != Some(generation) {
        state.stale_completions += 1;
        if result.is_ok()
            && generation < state.next_pc_generation
            && !state.is_live_generation(generation)
        {
            schedule_close_session(state, generation, effects);
        }
        return;
    }

    // The expected build completion.
    state.building_generation = None;
    match result {
        Ok(()) => {
            // S5-4: make-before-break — if there is still an active (old)
            // session, the replacement is pending (not active). The atomic
            // switch (pending → active, old → retiring) fires when the
            // pending session reaches full readiness (connected + anchor
            // open). Without an active session (initial connection or
            // break-before-make), the new session is active immediately.
            let make_before_break = state.active_generation.is_some();
            if make_before_break {
                state.pending_generation = Some(generation);
            } else {
                state.active_generation = Some(generation);
            }
            state.phase = RtcPeerPhase::Negotiating;
            state.bump_revision();
            match state.role {
                Role::Offerer => {
                    // Pre-create the RpcReliable readiness anchor before the
                    // offer so it is negotiated in the SDP (RFC §"Initial
                    // connection": "Offerer pre-creates `RpcReliable`
                    // DataChannel").
                    effects.push(RtcPeerEffect::CreateLane {
                        generation,
                        lane: LaneKind::READINESS_ANCHOR,
                    });
                    let operation_id = state.alloc_operation();
                    effects.push(RtcPeerEffect::CreateOffer {
                        generation,
                        operation_id,
                    });
                }
                Role::Answerer => {
                    if let Some(offer) = state.pending_remote_offer.take() {
                        // Build completion only makes the Session addressable;
                        // candidates remain buffered until the exact remote-SDP
                        // operation completes successfully.
                        schedule_remote_description(state, generation, offer, effects);
                    }
                }
                Role::Undetermined => {}
            }
        }
        Err(_) => {
            // S5-3/S5-4: a build failure after an established connection must
            // stay in Recovering, not Idle. If this was a make-before-break
            // replacement (old active still alive), retire the old session —
            // it was already failing.
            if state.ever_connected {
                if let Some(old) = state.active_generation.take() {
                    state.sessions_to_close.insert(old);
                    schedule_close_session(state, old, effects);
                }
                state.pc_connected = false;
                state.rpc_reliable_open = false;
                state.pending_pc_connected = false;
                state.pending_rpc_reliable_open = false;
                if state.role == Role::Answerer {
                    // A failed Answerer build cannot be repaired by creating
                    // another local PC. Park the logical recovery attempt and
                    // wait for the Offerer to send a fresh ordinary Offer.
                    wait_for_offerer_offer(state);
                } else {
                    state.phase = RtcPeerPhase::Recovering;
                    state.recovery_inflight = true;
                }
            } else {
                // Initial build failed.
                terminate_transport_attempt(
                    state,
                    RtcUnavailableReason::PeerConnectionBuildFailed,
                    effects,
                );
                return;
            }
            state.bump_revision();
        }
    }
}

/// O1: the structural mailbox overflowed for `generation`. The handler dropped
/// critical events, so the generation's health can no longer be trusted —
/// hard-recover (RFC: "the Controller SHOULD mark that generation unhealthy and
/// perform Hard Recovery, not silently lose critical events and continue to
/// claim the connection healthy").
fn reduce_mailbox_overflow(
    state: &mut RtcPeerState,
    generation: u64,
    effects: &mut Vec<RtcPeerEffect>,
) {
    if state.close_requested || state.phase == RtcPeerPhase::Closing {
        return;
    }

    let is_active = Some(generation) == state.active_generation;
    let is_pending = Some(generation) == state.pending_generation;
    let is_building = Some(generation) == state.building_generation;
    if !is_active && !is_pending && !is_building {
        return;
    }

    // S5-9: count and log the overflow so it is observable via the snapshot.
    state.mailbox_overflows += 1;
    tracing::warn!(
        peer = %state.peer_id.to_string_repr(),
        generation,
        mailbox_overflows = state.mailbox_overflows,
        "structural mailbox overflow; hard-recovering generation"
    );

    if is_pending || is_building {
        // The replacement lost a structural fact and can no longer be trusted.
        // Retire/abandon only that generation, then allocate its successor
        // inside the same bounded recovery cycle. A late successful build is
        // recognized by its retained operation identity and closed as orphan.
        retire_generation(state, generation, effects);
        state.pending_pc_connected = false;
        state.pending_rpc_reliable_open = false;
        state.recovery_attempt_started = false;
        start_hard_recovery(state, effects);
    } else if state.building_generation.is_some() || state.pending_generation.is_some() {
        // An old active generation can overflow while its replacement is
        // already progressing. Remove only the old generation; restarting the
        // replacement would let an obsolete callback storm cause churn.
        state.pc_connected = false;
        state.rpc_reliable_open = false;
        retire_generation(state, generation, effects);
    } else {
        // No replacement exists yet: start the shared Hard Recovery path.
        start_hard_recovery(state, effects);
    }
}

/// Whether a blocking completion still applies to the current state. A
/// completion is stale if its generation is no longer live or a close has been
/// requested; stale completions are counted and dropped (invariant #5).
fn completion_applies(state: &mut RtcPeerState, generation: u64) -> bool {
    if state.close_requested || !state.is_live_generation(generation) {
        state.stale_completions += 1;
        false
    } else if state.active_generation == Some(generation)
        && (state.building_generation.is_some() || state.pending_generation.is_some())
    {
        // S5-4: make-before-break — the old active session is being retired
        // while a replacement is built or pending. Its negotiation and ICE
        // completions are stale (in-flight operations were abandoned when
        // Hard Recovery started).
        state.stale_completions += 1;
        false
    } else {
        true
    }
}

/// A negotiation-step failure: a previously-connected peer recovers, an initial
/// attempt reports `Idle` + unavailable reason.
fn negotiation_failed(state: &mut RtcPeerState, effects: &mut Vec<RtcPeerEffect>) {
    if !state.ever_connected {
        terminate_transport_attempt(state, RtcUnavailableReason::NegotiationFailed, effects);
        return;
    }

    if let Some(failed_replacement) = state.pending_generation {
        // A pending replacement whose SDP operation failed can never become
        // authoritative. Retire it, keep the old active PC only as a physical
        // fallback, and let the exact attempt deadline launch the next Hard
        // generation. `address_changed` is the reducer's existing "must Hard"
        // fact and prevents an answerer restart-request retry on that deadline.
        retire_generation(state, failed_replacement, effects);
        clear_negotiation_for_recovery(state);
        state.pending_pc_connected = false;
        state.pending_rpc_reliable_open = false;
        state.address_changed = true;
        state.phase = RtcPeerPhase::Recovering;
        state.recovery_inflight = true;
        state.recovery_attempt_started = true;
        state.bump_revision();
        return;
    }

    if !state.recovery_inflight {
        // A normal established-session renegotiation failure has no recovery
        // timer yet. Start the regular Soft path immediately instead of
        // leaving the peer non-sendable until the 60-second total budget.
        state.phase = RtcPeerPhase::Recovering;
        state.recovery_attempt_started = false;
        start_soft_recovery(state, effects);
        return;
    }

    // A Soft attempt already owns an exact per-attempt timer; retain that
    // identity so its deadline escalates once without starting a second ICE
    // restart concurrently.
    state.phase = RtcPeerPhase::Recovering;
    state.bump_revision();
}

/// After a close-related completion, finish the close if nothing is left.
fn maybe_finish_close(state: &mut RtcPeerState) {
    if state.close_requested
        && state.sessions_to_close.is_empty()
        && state.building_generation.is_none()
        && state.active_generation.is_none()
        && state.pending_generation.is_none()
        && !has_outstanding_build(state)
    {
        state.phase = RtcPeerPhase::Idle;
        state.close_requested = false;
        // Terminal graceful close: stamp the reason so the controller's
        // retirement edge publishes `peer_transport_retired` and the outbound
        // DestTransport for this peer is reclaimed (see `reduce_close`).
        state.unavailable_reason = Some(RtcUnavailableReason::Closed);
        state.terminal_close_done = true;
        state.bump_revision();
    }
}

fn has_outstanding_build(state: &RtcPeerState) -> bool {
    state
        .pending_operations
        .values()
        .any(|operation| operation.kind == PendingRtcOperationKind::BuildSession)
}
