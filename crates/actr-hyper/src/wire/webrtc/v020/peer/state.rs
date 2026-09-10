//! Single-writer state for one logical RTC peer (RFC §"RtcPeerController" and
//! §"State ownership and invariants").
//!
//! The `RtcPeerController` is the *only* writer of this state; handlers, pumps,
//! and effect tasks never touch it directly (invariant #8). All transitions are
//! decided by the pure reducer (`super::reducer::reduce`), which mutates this
//! state and returns effects for the effect shell to run.

use std::collections::{BTreeSet, HashMap};

use actr_framework::WebRtcPeerStatus;
use actr_protocol::ActrId;

use webrtc::peer_connection::RTCSessionDescription;

use crate::wire::webrtc::v020::status::{NetworkPath, RtcPeerSnapshot, RtcUnavailableReason};

/// Internal lifecycle phase (RFC §"Peer lifecycle"). Mapped to the public
/// `WebRtcPeerStatus` by [`RtcPeerState::public_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RtcPeerPhase {
    Idle,
    Building,
    Negotiating,
    TransportReady,
    Sendable,
    Recovering,
    Closing,
}

/// Offerer/Answerer role for the current negotiation (RFC §"Role and glare handling").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Undetermined,
    Offerer,
    Answerer,
}

/// Stable identity of one blocking RTC effect.  The reducer records this when
/// it emits an effect and consumes it only after an exact completion match.
/// This prevents two operations on the same PC generation from authenticating
/// each other merely because their generation is equal (RFC §RtcPeerController).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingRtcOperationKind {
    BuildSession,
    CreateOffer,
    CreateAnswer,
    SetLocalDescription,
    SetRemoteDescription,
    AddIceCandidate,
    RestartIce,
    RefreshConfiguration,
    GetStats,
    CloseSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingRtcOperation {
    pub(crate) kind: PendingRtcOperationKind,
    pub(crate) pc_generation: u64,
    /// Present for operations whose result is tied to one ICE generation.
    pub(crate) ice_generation: Option<u64>,
}

/// The mutable, single-writer control state of one logical RTC peer.
///
/// Generation counters are monotonic and never reused; `Option<u64>` generation
/// slots refer to the *current* building/active/pending sessions. Old-generation
/// events and completions are detected by comparing against these slots and are
/// dropped as stale (invariant #5).
#[derive(Debug)]
pub(crate) struct RtcPeerState {
    pub(crate) peer_id: ActrId,
    pub(crate) phase: RtcPeerPhase,
    pub(crate) role: Role,

    // Role negotiation (server-arbitrated, RFC §"Role and glare handling").
    /// A `RoleNegotiation` is in flight; the controller is waiting for the
    /// server's `RoleAssignment` before building/offering.
    pub(crate) role_negotiation_inflight: bool,
    /// The in-flight (or deadline-failed) role probe was emitted while no
    /// signaling generation was committed, so the controller dropped it at
    /// dispatch. The gate-opening edge uses this to re-drive the negotiation —
    /// including resurrecting an attempt the role deadline already failed,
    /// which it must: the deadline is the bounded failure the connection
    /// budget exposes even when signaling never became usable.
    pub(crate) role_probe_undelivered: bool,
    /// The server assigned the answerer role and the offer has not arrived yet.
    pub(crate) awaiting_offer: bool,
    /// Monotonic negotiation epoch, bumped whenever role or generation changes;
    /// stale SDP completions from an older epoch are discarded (RFC §"Role and
    /// glare handling").
    pub(crate) negotiation_epoch: u64,

    /// Monotonic logical transport-attempt epoch. A fresh attempt from
    /// terminal `Idle`, or a fresh remote Offer that supersedes an in-flight
    /// recovery, increments it. Soft/Hard Recovery deliberately keeps it so
    /// the cached logical transport can replace PC generations in place.
    pub(crate) transport_epoch: u64,
    /// A fresh logical transport has not reached business sendability yet.
    /// This remains true after the physical PC reports Connected so the
    /// attempt deadline still covers the readiness DataChannel.
    pub(crate) initial_connection_inflight: bool,

    /// Next PC generation to assign (monotonic, never reused). Generations
    /// start at 1; `None`/0 means "no session".
    pub(crate) next_pc_generation: u64,
    /// Last issued operation id (monotonic). Every blocking effect carries one
    /// so a stale completion can be told apart from the current operation.
    pub(crate) operation_id: u64,
    /// Exact identities of blocking effects whose completions are still
    /// outstanding.  Entries are keyed by the peer-monotone operation id.
    pub(crate) pending_operations: HashMap<u64, PendingRtcOperation>,

    /// Generation currently being built (Building phase).
    pub(crate) building_generation: Option<u64>,
    /// Generation of the active session (the only one that may be sendable).
    pub(crate) active_generation: Option<u64>,
    /// Generation of a pending (hard-recovery) session (Stage 5; declared for
    /// the one-active-generation invariant).
    pub(crate) pending_generation: Option<u64>,
    /// A retired physical generation that an Answerer may still name on a
    /// recovery wake while waiting for the Offerer to create a replacement.
    /// This is a signaling identity only; it is never considered live by
    /// `is_live_generation` and cannot receive RTC events or data-plane work.
    pub(crate) parked_signaling_generation: Option<u64>,
    /// Current ICE generation within the active PC generation.
    pub(crate) ice_generation: u64,

    // Readiness facts driving phase transitions (derived from driver events).
    pub(crate) pc_connected: bool,
    pub(crate) rpc_reliable_open: bool,
    pub(crate) recovery_inflight: bool,
    /// The current recovery epoch has completed its offer/answer exchange.
    /// Connected callbacks from the pre-restart ICE state cannot restore
    /// sendability while this is false.
    pub(crate) recovery_negotiated: bool,
    /// Monotone identity of the current recovery deadline. It advances when a
    /// new recovery epoch/session replacement begins or when an accepted remote
    /// restart Offer supersedes the request-only attempt deadline, so duplicate
    /// network facts cannot extend an in-flight deadline.
    pub(crate) recovery_epoch: u64,
    /// Identity of the end-to-end recovery budget. Unlike `recovery_epoch`,
    /// this advances only when entering a new logical recovery cycle; Soft
    /// retries and Hard escalation cannot extend its deadline.
    pub(crate) recovery_cycle: u64,
    /// A Soft/Hard recovery operation has been launched for the current
    /// recovering epoch. Network-event storms are facts, not a request to run
    /// the same restart repeatedly; this latch is cleared only on convergence
    /// or a terminal attempt failure.
    pub(crate) recovery_attempt_started: bool,
    /// The current soft round was started by a local recovery signal the
    /// offerer observed itself (NetworkAvailable / AppForeground /
    /// SignalingGenerationChanged through `maybe_start_deferred_recovery`), as
    /// opposed to a driver-side fact (the peer's PC death). Only in the former
    /// case may the answerer's first correlated restart wake (attempt=1) be
    /// de-duped: the offerer is already running the same round, so the wake is
    /// redundant. When the round came from the peer's death (a one-sided
    /// outage), the wake is the offerer's only signal that the answerer is
    /// back and must resend the offer immediately (issue #227).
    pub(crate) recovery_from_network_available: bool,
    /// Offerer-side count of deadline-driven Soft retries within the current
    /// recovery cycle. Retries that keep failing to converge prove the
    /// restart is not repairing the transport (e.g. the restart rebind
    /// silently kept a dead interface), so the ladder escalates to a checked
    /// Hard rebuild at the threshold instead of spinning until the cycle
    /// budget terminates the attempt.
    pub(crate) soft_recovery_attempts: u32,
    /// A `LocalAddressChanged` edge arrived while a soft round was already in
    /// flight and was coalesced by the attempt latch. When the in-flight
    /// round converges (on the pre-edge address set), one more restart is
    /// published so the link moves onto the current set instead of stranding
    /// on the superseded interface. Cleared when a new soft round launches
    /// (its rebind resolves the current set) and on Hard rebuild/terminate.
    pub(crate) address_edge_pending: bool,
    pub(crate) ever_connected: bool,

    // Close/lifecycle.
    pub(crate) close_requested: bool,
    /// Set when an explicit Close's close-barrier has fully completed (terminal
    /// `Idle`). The controller exits its run loop on this, so a terminal Close
    /// converges the task even if a stale handle clone is still alive (N1).
    pub(crate) terminal_close_done: bool,
    /// Generations whose `close()` has been issued but whose close completion
    /// has not yet arrived. The close barrier holds the controller in `Closing`
    /// until this set drains, so a close-during-build cannot leave a live
    /// session behind.
    pub(crate) sessions_to_close: BTreeSet<u64>,

    // Snapshot bookkeeping (published via `to_snapshot`).
    pub(crate) revision: u64,
    pub(crate) network_path: NetworkPath,
    /// Bounded sampling attempts for the current entry into Sendable. v0.20 may
    /// publish DataChannel Open before its selected-pair stats entry becomes
    /// visible, so one observation-only retry is not sufficient under load.
    pub(crate) stats_sample_attempts: u8,
    pub(crate) unavailable_reason: Option<RtcUnavailableReason>,

    // Observability counters (RFC metrics: stale driver event/completion).
    pub(crate) stale_events: u64,
    pub(crate) stale_completions: u64,
    /// Signaling messages dropped because the shared outbound signaling queue
    /// was full (N2 backpressure). One peer's signaling storm must never stall
    /// the node's other controllers, so sends are non-blocking and counted.
    pub(crate) signaling_dropped: u64,

    /// O1: total number of mailbox overflow events handled by this controller.
    /// Depending on the exact generation, an overflow retires an obsolete
    /// session or replaces the affected current attempt. Exposed in the
    /// snapshot for observability (RFC: "Handler mailbox depth and overflow
    /// [must be observable]").
    pub(crate) mailbox_overflows: u64,

    /// Network/App lifecycle events received from the `RecoverySupervisor`
    /// (RFC observability: "Controller recovery latency" and event-count
    /// metrics). Incremented on every `NetworkEvent` command, regardless of
    /// whether the reducer acted on it.
    pub(crate) network_events_received: u64,

    /// Global recovery gate projected by the lifecycle supervisor. Peer
    /// recovery may be prepared while any gate is closed, but ICE restart,
    /// SDP creation and signaling sends start only when all three are ready.
    pub(crate) network_available: bool,
    pub(crate) app_foreground: bool,
    pub(crate) signaling_generation: Option<u64>,

    /// S5-5: set by an explicit `ForceReconnect` or long-background policy.
    /// It forces Hard Recovery (PC generation + 1) instead of Soft Recovery.
    /// A `LocalAddressChanged` fact itself starts Soft Recovery on the current
    /// PeerConnection; a failed restart can still escalate through the normal
    /// hard-recovery paths. Cleared when the connection reaches `Sendable`
    /// again (recovery succeeded).
    pub(crate) address_changed: bool,

    /// S5-4: make-before-break pending session readiness facts. During Hard
    /// Recovery with an active old session, the replacement session is built
    /// as `pending_generation`. Its connected/open state is tracked here,
    /// separate from the active session's. When both are true, the atomic
    /// switch fires (pending → active, old → retiring).
    pub(crate) pending_pc_connected: bool,
    pub(crate) pending_rpc_reliable_open: bool,

    /// Answerer: the remote offer held while the session is being built.
    pub(crate) pending_remote_offer: Option<Box<RTCSessionDescription>>,

    /// The remote-description operation currently executing.  The exact
    /// `(pc_generation, operation_id)` identity is required before its
    /// completion may unlock ICE candidates or advance the answer chain.
    pub(crate) pending_remote_description_operation: Option<(u64, u64)>,

    /// At most one newer remote description waiting behind the in-flight
    /// operation.  Keeping this in the single-writer state serializes
    /// `set_remote_description()` calls instead of racing effect tasks.
    pub(crate) queued_remote_description: Option<(u64, Box<RTCSessionDescription>)>,

    /// Generation whose current remote description has successfully been
    /// applied.  A matching ufrag alone is insufficient to call
    /// `add_ice_candidate()`; WebRTC requires the remote description first.
    pub(crate) applied_remote_description_generation: Option<u64>,

    /// The local description just applied, held so its
    /// `SetLocalDescription`-completion can hand it to the signaling router.
    pub(crate) pending_local_desc: Option<Box<RTCSessionDescription>>,

    /// Generation whose latest local SDP has already been queued ahead of its
    /// trickle candidates. WebRTC may publish candidates while
    /// `set_local_description()` is still running; sending them before the
    /// initial Offer would make an unknown-peer router drop every candidate.
    pub(crate) announced_local_description_generation: Option<u64>,

    /// Local trickle candidates waiting for their generation's Offer/Answer to
    /// enter the ordered signaling queue first.
    pub(crate) pending_local_candidates: Vec<(u64, webrtc::peer_connection::RTCIceCandidateInit)>,

    /// Correlation identity of the ordinary/renegotiation/restart Offer whose
    /// Answer is currently expected. A late Answer from an older PC generation
    /// must not be applied to the current generation merely because its SDP
    /// type is `Answer`.
    pub(crate) pending_local_sdp_exchange_id: Option<String>,

    /// Correlation identity received with the latest accepted remote Offer.
    /// The answerer echoes it on the matching Answer after the serialized SDP
    /// chain completes.
    pub(crate) pending_remote_sdp_exchange_id: Option<String>,

    /// Correlation identity for a local ICE-restart offer. It is attached to
    /// the wire only after SetLocalDescription succeeds, and lets an answerer
    /// with no active Session reject the restart explicitly.
    pub(crate) pending_local_restart_exchange_id: Option<String>,

    /// Answerer-to-offerer ICE restart request identity. The id is stable for
    /// one recovery epoch and the attempt increases on retransmission.
    pub(crate) local_restart_request_id: Option<String>,
    pub(crate) local_restart_request_attempt: u32,
    /// Highest correlated request observed from the remote answerer, used to
    /// suppress duplicate retries without suppressing a newer attempt.
    pub(crate) last_remote_restart_request: Option<(String, u32)>,

    // Renegotiation (lazily-added lanes/media mid-connection; RFC §"DataChannel
    // APIs": "Track add/remove serializes renegotiation through the Controller
    // effect").
    /// A renegotiation is in flight (an offer is being created/sent); further
    /// `on_negotiation_needed` events while in flight are marked pending and
    /// merged.
    pub(crate) renegotiation_inflight: bool,
    /// A negotiation-needed event arrived while a renegotiation was in flight;
    /// another offer is due once the current one completes.
    pub(crate) renegotiation_pending: bool,

    // ICE generation / candidate tagging (RFC §"ICE generation and candidates").
    /// The local ICE ufrag, learned from our own local SDP. Outgoing candidates
    /// are stamped with it so the remote can tag them to this ICE generation
    /// (mirrors 0.14 `serialize_local_ice_candidate`).
    pub(crate) local_ufrag: Option<String>,
    /// The current remote ICE ufrag, learned from the remote SDP. Incoming
    /// candidates are only injected if they match it.
    pub(crate) remote_ufrag: Option<String>,
    /// Recently seen remote ufrags (current + prior ICE generations), capped so
    /// candidates from a known-old generation are dropped as stale while truly
    /// unknown ones are buffered (mirrors 0.14 `known_remote_ufrags`).
    pub(crate) known_remote_ufrags: std::collections::VecDeque<String>,
    /// Remote candidates buffered because their ufrag is not yet known (no SDP
    /// yet, or a possible future ICE generation). Re-classified when SDP arrives.
    pub(crate) pending_candidates: Vec<webrtc::peer_connection::RTCIceCandidateInit>,
}

impl RtcPeerState {
    pub(crate) fn new(peer_id: ActrId) -> Self {
        Self::new_with_next_generation(peer_id, 1)
    }

    /// Construct a controller state with a manager-owned generation floor.
    /// Recreating a controller for the same logical peer must never reuse a PC
    /// generation that belonged to the retired controller.
    pub(crate) fn new_with_next_generation(peer_id: ActrId, next_pc_generation: u64) -> Self {
        Self::new_with_lifecycle_floors(peer_id, next_pc_generation, 0)
    }

    /// Construct state with Manager-owned generation and public-revision
    /// floors. The revision floor keeps one logical peer's Hook stream
    /// strictly monotone when its Controller/watch channel is recreated.
    pub(crate) fn new_with_lifecycle_floors(
        peer_id: ActrId,
        next_pc_generation: u64,
        next_revision: u64,
    ) -> Self {
        Self {
            peer_id,
            phase: RtcPeerPhase::Idle,
            role: Role::Undetermined,
            role_negotiation_inflight: false,
            role_probe_undelivered: false,
            awaiting_offer: false,
            negotiation_epoch: 0,
            transport_epoch: 0,
            initial_connection_inflight: false,
            next_pc_generation: next_pc_generation.max(1),
            operation_id: 0,
            pending_operations: HashMap::new(),
            building_generation: None,
            active_generation: None,
            pending_generation: None,
            parked_signaling_generation: None,
            ice_generation: 0,
            pc_connected: false,
            rpc_reliable_open: false,
            recovery_inflight: false,
            recovery_negotiated: true,
            recovery_epoch: 0,
            recovery_cycle: 0,
            recovery_attempt_started: false,
            recovery_from_network_available: false,
            soft_recovery_attempts: 0,
            address_edge_pending: false,
            ever_connected: false,
            close_requested: false,
            terminal_close_done: false,
            sessions_to_close: BTreeSet::new(),
            revision: next_revision,
            network_path: NetworkPath::Unknown,
            stats_sample_attempts: 0,
            unavailable_reason: None,
            stale_events: 0,
            stale_completions: 0,
            signaling_dropped: 0,
            mailbox_overflows: 0,
            network_events_received: 0,
            network_available: true,
            app_foreground: true,
            // Standalone reducer/test state starts with a synthetic committed
            // generation. Production controller construction immediately
            // overwrites this from the lifecycle supervisor's watch.
            signaling_generation: Some(0),
            address_changed: false,
            pending_pc_connected: false,
            pending_rpc_reliable_open: false,
            pending_remote_offer: None,
            pending_remote_description_operation: None,
            queued_remote_description: None,
            applied_remote_description_generation: None,
            pending_local_desc: None,
            announced_local_description_generation: None,
            pending_local_candidates: Vec::new(),
            pending_local_sdp_exchange_id: None,
            pending_remote_sdp_exchange_id: None,
            pending_local_restart_exchange_id: None,
            local_restart_request_id: None,
            local_restart_request_attempt: 0,
            last_remote_restart_request: None,
            renegotiation_inflight: false,
            renegotiation_pending: false,
            local_ufrag: None,
            remote_ufrag: None,
            known_remote_ufrags: std::collections::VecDeque::new(),
            pending_candidates: Vec::new(),
        }
    }

    /// Cap on remembered remote ufrags (ICE generations), mirroring 0.14
    /// `MAX_KNOWN_REMOTE_ICE_GENERATIONS`.
    pub(crate) const MAX_KNOWN_REMOTE_UFRAGS: usize = 8;

    /// Record the remote ufrag from a freshly applied remote SDP: make it the
    /// current ufrag and remember it (dedup, capped) for stale classification.
    pub(crate) fn set_remote_ufrag(&mut self, ufrag: String) {
        self.remote_ufrag = Some(ufrag.clone());
        self.known_remote_ufrags.retain(|u| u != &ufrag);
        self.known_remote_ufrags.push_front(ufrag);
        self.known_remote_ufrags
            .truncate(Self::MAX_KNOWN_REMOTE_UFRAGS);
    }

    /// Classify a remote candidate's ufrag against the current/known remote
    /// ufrags (mirrors 0.14 `classify_remote_candidate_ufrag`).
    pub(crate) fn classify_candidate_ufrag(
        &self,
        candidate_ufrag: Option<&str>,
    ) -> CandidateUfragDisposition {
        let Some(candidate_ufrag) = candidate_ufrag else {
            // No ufrag on the candidate: cannot validate; buffer until SDP.
            return CandidateUfragDisposition::Buffer;
        };
        if Some(candidate_ufrag) == self.remote_ufrag.as_deref() {
            CandidateUfragDisposition::Apply
        } else if self
            .known_remote_ufrags
            .iter()
            .any(|u| u == candidate_ufrag)
        {
            CandidateUfragDisposition::DropStale
        } else if self.remote_ufrag.is_none() {
            // No SDP yet: we cannot tell a future generation from a stale one.
            CandidateUfragDisposition::Buffer
        } else {
            // Unknown ufrag with an established current one: likely a future ICE
            // generation (restart) that arrived before its SDP — buffer it.
            CandidateUfragDisposition::Buffer
        }
    }

    /// Bump the snapshot revision. The reducer calls this on every
    /// snapshot-relevant change so subscribers only see monotone revisions.
    pub(crate) fn bump_revision(&mut self) {
        self.revision += 1;
    }

    /// Allocate the next PC generation (monotonic, never reused).
    pub(crate) fn alloc_generation(&mut self) -> u64 {
        let generation = self.next_pc_generation;
        self.next_pc_generation += 1;
        generation
    }

    /// Allocate the next operation id (monotonic).
    pub(crate) fn alloc_operation(&mut self) -> u64 {
        self.operation_id += 1;
        self.operation_id
    }

    /// Start a fresh logical connection flight without reusing epoch zero.
    pub(crate) fn begin_transport_attempt(&mut self) {
        self.transport_epoch = self.transport_epoch.wrapping_add(1).max(1);
        self.initial_connection_inflight = true;
        self.ever_connected = false;
        self.unavailable_reason = None;
        self.parked_signaling_generation = None;
    }

    /// Whether the current attempt has converged to terminal unavailable
    /// `Idle` with no generation that could still recover in place.
    pub(crate) fn terminally_unavailable(&self) -> bool {
        self.phase == RtcPeerPhase::Idle
            && self.unavailable_reason.is_some()
            && self.building_generation.is_none()
            && self.active_generation.is_none()
            && self.pending_generation.is_none()
    }

    /// Whether `generation` refers to a currently live session slot (building /
    /// active / pending). Anything else is a stale generation (invariant #5).
    pub(crate) fn is_live_generation(&self, generation: u64) -> bool {
        Some(generation) == self.building_generation
            || Some(generation) == self.active_generation
            || Some(generation) == self.pending_generation
    }

    /// Business sendability (RFC §"Sendability and public state"):
    /// an active generation that is PC-connected with the RpcReliable anchor
    /// open, not recovering, and not closing.
    pub(crate) fn sendable(&self) -> bool {
        self.active_generation.is_some()
            && self.pc_connected
            && self.rpc_reliable_open
            && !self.recovery_inflight
            && !self.close_requested
    }

    /// Public status derived from the internal phase (RFC mapping table).
    pub(crate) fn public_status(&self) -> WebRtcPeerStatus {
        // A Hard Recovery uses the internal Building/Negotiating phases for
        // the pending Session, but the logical peer must remain publicly
        // Recovering until the atomic switch completes (RFC Hard Recovery
        // steps 1-8).  Initial connection has `recovery_inflight == false`
        // and therefore still projects as Connecting.
        if self.recovery_inflight
            && !matches!(self.phase, RtcPeerPhase::Idle | RtcPeerPhase::Closing)
        {
            return WebRtcPeerStatus::Recovering;
        }
        match self.phase {
            RtcPeerPhase::Idle | RtcPeerPhase::Closing => WebRtcPeerStatus::Idle,
            RtcPeerPhase::Building | RtcPeerPhase::Negotiating | RtcPeerPhase::TransportReady => {
                WebRtcPeerStatus::Connecting
            }
            RtcPeerPhase::Sendable => WebRtcPeerStatus::Connected,
            RtcPeerPhase::Recovering => WebRtcPeerStatus::Recovering,
        }
    }

    /// Build the authoritative snapshot for subscribers. `revision` is bumped
    /// by the reducer on every snapshot-relevant change, so subscribers only
    /// ever see monotone revisions.
    pub(crate) fn to_snapshot(&self) -> RtcPeerSnapshot {
        RtcPeerSnapshot {
            peer: self.peer_id.clone(),
            role: self.role,
            transport_epoch: self.transport_epoch,
            // During Hard Recovery the replacement identity is authoritative
            // even before it becomes sendable. This lets observers distinguish
            // recovery progress while the send path remains gated by
            // `sendable=false`; after the atomic switch the same generation is
            // active, so the value never rolls back.
            pc_generation: self
                .pending_generation
                .or(self.building_generation)
                .or(self.active_generation)
                .unwrap_or(0),
            ice_generation: self.ice_generation,
            revision: self.revision,
            status: self.public_status(),
            sendable: self.sendable(),
            network_path: self.network_path,
            unavailable_reason: self.unavailable_reason,
            signaling_dropped: self.signaling_dropped,
            mailbox_overflows: self.mailbox_overflows,
        }
    }
}

/// Disposition of a remote candidate by ufrag (RFC §"ICE generation and
/// candidates"; mirrors 0.14 `RemoteCandidateDisposition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateUfragDisposition {
    /// Matches the current remote ufrag: inject into the active PeerConnection.
    Apply,
    /// Matches a known-old ICE generation: drop and count as stale.
    DropStale,
    /// Unknown (no SDP yet, or a possible future generation): buffer.
    Buffer,
}

/// Extract the ICE ufrag from an SDP blob. Returns `Some` when all
/// `a=ice-ufrag:` lines agree (the common single-bundle case), `None` when
/// there is no ufrag or media sections disagree (ambiguous — RFC 8839 §5.4).
pub(crate) fn session_ice_ufrag(sdp: &str) -> Option<String> {
    let mut ufrags = sdp
        .lines()
        .filter_map(|line| line.strip_prefix("a=ice-ufrag:"))
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let first = ufrags.next()?;
    if ufrags.all(|u| u == first) {
        Some(first.to_string())
    } else {
        None
    }
}
