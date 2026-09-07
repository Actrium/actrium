//! `RtcPeerController` — the single-writer control task for one remote peer
//! (RFC §"RtcPeerController").
//!
//! The controller is a Tokio task with a reducer/effect shell: it selects over
//! the command mailbox, the (coalesced + structural) driver-event mailbox, the
//! lane-event channel, and the effect-completion channel; feeds each input to
//! the pure [`reduce`] function; publishes the snapshot when it changes; and
//! submits the returned effects to its [`RtcPeerEffectPort`]. It is the *only*
//! writer of the peer's control state (invariant #8) and the *only* producer of
//! the peer's snapshot (invariant #7).
//!
//! RFC §"RtcPeerEffectPort": "The Controller only saves the lifecycle/identity
//! of RtcSessionSlot and does not directly save the PC related to WebRTC
//! implementation object." The controller never holds concrete sessions or
//! `Arc<dyn DataChannel>` objects — the [`WebRtcEffectRunner`] owns the session
//! registry, and the controller only installs `LaneHandle` tokens into the
//! send-path registry.

use std::sync::Arc;

use actr_protocol::ActrId;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::wire::webrtc::v020::events::{
    DataChannelDescriptor, PEER_EVENT_MAILBOX_CAPACITY, PeerDriverEvent, TrackDescriptor,
    peer_event_mailbox_with_capacity_and_bytes,
};
use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;
use crate::wire::webrtc::v020::signaling::SignalingDispatch;
use crate::wire::webrtc::v020::status::{LaneKind, PeerTransportRetirement, RtcPeerSnapshot};

use super::effect_port::{
    ResourceRegistration, RtcEffectCompletionSender, RtcPeerEffectPort, TaggedRtcEffectCompletion,
    TaggedRtcPeerEffect, WebRtcEffectRunner,
};
use super::effects::SessionFactory;
use super::handle::RtcPeerControllerHandle;
use super::reducer::{
    LaneEvent, RtcConfigurationRefreshError, RtcPeerCommand, RtcPeerEffect, RtcPeerInput,
    SignalingMessage, reduce,
};
use super::session::SessionResourceId;
use super::state::{PendingRtcOperationKind, RtcPeerState};

#[cfg(feature = "test-utils")]
pub(crate) type NodeDriverEventObserver = Arc<dyn Fn(&ActrId, &PeerDriverEvent) + Send + Sync>;

/// Bounded capacity of the per-peer command mailbox.
const COMMAND_CAPACITY: usize = 64;
/// Bounded capacity of the per-peer effect-completion channel.
const COMPLETION_CAPACITY: usize = 128;
/// Bounded capacity of the per-peer lane-event channel.
const LANE_CAPACITY: usize = 64;
/// Per-attempt ICE restart readiness budget, preserving the 0.14 policy. The
/// reducer receives only the elapsed fact; it never reads wall-clock time.
const ICE_RESTART_ATTEMPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
/// Server-arbitrated role negotiation keeps the historical short failure
/// budget, distinct from the longer initial ICE/DataChannel readiness budget.
const ROLE_NEGOTIATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const CONTROLLER_INLINE_OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// One complete initial role/build/SDP/ICE/readiness attempt. It expires just
/// before `V020WireHandle`'s 10-second wait so the caller observes a terminal
/// attempt and WirePool can start its configured retry with a fresh PC.
const INITIAL_CONNECTION_ATTEMPT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_millis(9_500);

#[derive(Clone, Copy)]
struct ControllerLifecycleFloors {
    next_pc_generation: u64,
    next_revision: u64,
}

/// One critical signaling message retained under signaling-queue backpressure,
/// fenced by the transport attempt that produced it (and, for SDP, by the PC
/// generation that announced it). `negotiation_epoch` preserves the emission
/// epoch of a retained generation-0 role probe so a delayed delivery result
/// cannot be mistaken for a newer probe's.
struct RetainedSignaling {
    transport_epoch: u64,
    generation: u64,
    negotiation_epoch: u64,
    message: SignalingMessage,
}

#[derive(Clone)]
enum PendingDiscoveredResource {
    DataChannel {
        generation: u64,
        resource_id: SessionResourceId,
        descriptor: DataChannelDescriptor,
    },
    Track {
        generation: u64,
        resource_id: SessionResourceId,
        descriptor: TrackDescriptor,
    },
}

/// The controller's single-writer control task.
pub(crate) struct RtcPeerController {
    state: RtcPeerState,
    /// The effect port: owns the session registry and executes blocking effects.
    /// The controller submits effects and receives completions; it never holds
    /// sessions or WebRTC implementation objects directly.
    effect_port: Arc<dyn RtcPeerEffectPort>,
    /// Shared registry of active lane handles (installed/cancelled here, read by
    /// the handle's send path). Shared with the handle via the same `Arc`.
    lanes: crate::wire::webrtc::v020::peer::lane_handle::ActiveLaneRegistry,
    /// The inbound data-plane router (message payloads bypass the Controller).
    inbound: std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
    event_rx: crate::wire::webrtc::v020::events::PeerEventMailboxReceiver,
    command_rx: mpsc::Receiver<RtcPeerCommand>,
    lane_rx: mpsc::Receiver<LaneEvent>,
    /// Sender clone handed to DataChannel pumps so they report readiness/liveness
    /// back into `lane_rx` (the controller keeps one; the caller gets another).
    lane_tx: mpsc::Sender<LaneEvent>,
    completion_tx: RtcEffectCompletionSender,
    completion_rx: mpsc::Receiver<TaggedRtcEffectCompletion>,
    /// Tokens discovered before `build()` installs the physical Session. Raw
    /// resources stay in the generation-owned runner registry; this list holds
    /// only immutable descriptors/tokens and is retried after completions.
    pending_resources: Vec<PendingDiscoveredResource>,
    /// Reducer facts produced by inline controller-owned work. They are
    /// consumed before selecting another external event, avoiding recursive
    /// async `apply()` while still letting a failed readiness lane stop the
    /// remaining effects from that reducer turn.
    internal_inputs: std::collections::VecDeque<RtcPeerInput>,
    /// Critical signaling (role probe / offer / answer) retained when the
    /// signaling queue was full, retried on every event-loop turn. The
    /// reducer already marked the attempt announced, so dropping the message
    /// would strand negotiation. The originating transport attempt fences a
    /// retained message from leaking into a later WirePool retry. Only the
    /// newest message is retained: a newer offer/answer supersedes any
    /// retained one.
    retry_signaling: Option<RetainedSignaling>,
    /// Wake channel: a drained signaling queue re-delivers a retained
    /// critical message without waiting for an unrelated event-loop turn
    /// (queue capacity release itself arms the wake).
    signaling_retry_wake_tx: mpsc::UnboundedSender<()>,
    signaling_retry_wake_rx: mpsc::UnboundedReceiver<()>,
    signaling_retry_wake_armed: bool,
    /// API acknowledgements keyed by the exact RefreshConfiguration operation.
    /// Enqueueing the command is not success; the caller resolves only after
    /// the generation/operation-authenticated `set_configuration()` result.
    pending_configuration_refreshes: std::collections::HashMap<
        u64,
        (
            u64,
            tokio::sync::oneshot::Sender<Result<(), RtcConfigurationRefreshError>>,
        ),
    >,
    snapshot_tx: watch::Sender<RtcPeerSnapshot>,
    /// Public/FFI projection is maintained by the same single writer as the
    /// internal snapshot, so subscriptions need no per-subscriber tasks and
    /// can never observe a different revision.
    public_snapshot_tx: watch::Sender<actr_framework::WebRtcPeerState>,
    signaling_tx: mpsc::Sender<SignalingDispatch>,
    /// Network/App lifecycle events from the `RtcRecoverySupervisor`
    /// (S5-1). The controller selects on this receiver and feeds events to
    /// its reducer as `NetworkEvent` commands; the reducer (not the
    /// supervisor) decides the recovery action.
    network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
    /// Authoritative signaling generation projected by the global lifecycle
    /// supervisor. Recovery effects remain pending while this is `None`.
    signaling_generation_rx: watch::Receiver<Option<u64>>,
    /// Explicit scheduler boundary. The reducer receives only elapsed facts;
    /// production uses audited sleeps and T1 uses a per-fixture manual clock.
    policy_clock: Arc<dyn super::policy_clock::PolicyClock>,
    /// Node-level shared dependencies (inbound router, throttle,
    /// media registry) handed down from the manager at spawn.
    deps: SharedNodeDeps,
}

/// Node-level shared dependencies handed to every controller and its effect
/// runner (one instance per node, cloned per controller). Bundled so
/// `spawn`/runner construction stay under the argument-count lint as Stage 6
/// adds the media registry.
#[derive(Clone)]
pub(crate) struct SharedNodeDeps {
    /// The shared inbound data-plane router (payloads bypass controllers).
    pub(crate) inbound: std::sync::Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
    /// The node media-frame registry (Stage 6); `None` drops media dispatch.
    pub(crate) media_registry: Option<Arc<crate::inbound::MediaFrameRegistry>>,
    /// Validated receive/reassembly/resource limits shared with every Session.
    pub(crate) buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig,
    /// Test-only observation of raw Handler facts before Controller reduction.
    /// Production managers leave this unset.
    #[cfg(feature = "test-utils")]
    pub(crate) driver_event_observer: Option<NodeDriverEventObserver>,
    /// Stable identity of this Controller lifetime within its Manager.
    pub(crate) controller_lifecycle_id: u64,
    /// Manager-owned hard-recovery sink used to release business work that was
    /// committed to the generation being superseded. Soft recovery keeps the
    /// same Session and therefore emits no edge.
    pub(crate) peer_generation_invalidated: Option<tokio::sync::broadcast::Sender<(ActrId, u64)>>,
    /// Manager-owned sink for every authoritative sendability edge. PeerGate
    /// pairs falling and rising edges to apply recovery hysteresis to pending
    /// response waits.
    pub(crate) peer_sendability_changed: Option<tokio::sync::broadcast::Sender<(ActrId, bool)>>,
    /// Manager-owned sink for terminal connection-flight retirement. This is
    /// distinct from generation invalidation because recovery keeps the
    /// logical `DestTransport`, while terminal `Idle` must evict it.
    pub(crate) peer_transport_retired:
        Option<tokio::sync::broadcast::Sender<PeerTransportRetirement>>,
    /// Manager-owned ordered snapshot commit stream for workload Hooks. The
    /// latest-value watch remains the query/subscription surface.
    pub(crate) peer_snapshot_events:
        Option<tokio::sync::broadcast::Sender<crate::wire::webrtc::v020::status::RtcPeerSnapshot>>,
    /// Manager-owned ledger of handed-off per-peer recovery obligations
    /// (RFC-0400 per-peer coverage contract). The controller reports each
    /// authoritative snapshot so terminal per-peer facts extinguish open
    /// obligations at their publication point.
    pub(crate) recovery_obligations:
        Option<std::sync::Arc<crate::wire::webrtc::v020::recovery::PeerRecoveryObligationLog>>,
}

impl RtcPeerController {
    /// Spawn the controller task for `peer_id`. Returns the handle (for the
    /// manager/business), the lane-event sender (for DataChannel pumps; tests
    /// inject readiness directly), and the task join handle.
    pub(crate) fn spawn(
        peer_id: ActrId,
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
        signaling_generation_rx: watch::Receiver<Option<u64>>,
        deps: SharedNodeDeps,
    ) -> (
        RtcPeerControllerHandle,
        mpsc::Sender<LaneEvent>,
        JoinHandle<()>,
    ) {
        Self::spawn_with_next_generation(
            peer_id,
            1,
            factory,
            signaling_tx,
            network_rx,
            signaling_generation_rx,
            deps,
        )
    }

    /// Manager entry point that preserves a monotone PC-generation floor when
    /// a logical peer controller is removed and later recreated.
    pub(crate) fn spawn_with_next_generation(
        peer_id: ActrId,
        next_pc_generation: u64,
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
        signaling_generation_rx: watch::Receiver<Option<u64>>,
        deps: SharedNodeDeps,
    ) -> (
        RtcPeerControllerHandle,
        mpsc::Sender<LaneEvent>,
        JoinHandle<()>,
    ) {
        Self::spawn_with_lifecycle_floors(
            peer_id,
            next_pc_generation,
            0,
            factory,
            signaling_tx,
            network_rx,
            signaling_generation_rx,
            deps,
            super::policy_clock::system_policy_clock(),
        )
    }

    /// Manager entry point that also preserves the public Hook revision floor
    /// when a logical peer Controller/watch channel is recreated.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_with_lifecycle_floors(
        peer_id: ActrId,
        next_pc_generation: u64,
        next_revision: u64,
        factory: Arc<dyn SessionFactory>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
        signaling_generation_rx: watch::Receiver<Option<u64>>,
        deps: SharedNodeDeps,
        policy_clock: Arc<dyn super::policy_clock::PolicyClock>,
    ) -> (
        RtcPeerControllerHandle,
        mpsc::Sender<LaneEvent>,
        JoinHandle<()>,
    ) {
        let factory_for_port = factory;
        let (handle, lane_tx, _event_mailbox, task) = Self::spawn_with_port_factory(
            peer_id,
            ControllerLifecycleFloors {
                next_pc_generation,
                next_revision,
            },
            signaling_tx,
            network_rx,
            signaling_generation_rx,
            deps,
            policy_clock,
            move |peer_id, event_mailbox, lane_tx, deps| {
                Arc::new(WebRtcEffectRunner::new(
                    peer_id,
                    factory_for_port,
                    event_mailbox,
                    lane_tx,
                    deps,
                ))
            },
        );
        (handle, lane_tx, task)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_with_port_factory(
        peer_id: ActrId,
        floors: ControllerLifecycleFloors,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
        signaling_generation_rx: watch::Receiver<Option<u64>>,
        deps: SharedNodeDeps,
        policy_clock: Arc<dyn super::policy_clock::PolicyClock>,
        make_effect_port: impl FnOnce(
            ActrId,
            crate::wire::webrtc::v020::events::PeerEventMailbox,
            mpsc::Sender<LaneEvent>,
            SharedNodeDeps,
        ) -> Arc<dyn RtcPeerEffectPort>,
    ) -> (
        RtcPeerControllerHandle,
        mpsc::Sender<LaneEvent>,
        crate::wire::webrtc::v020::events::PeerEventMailbox,
        JoinHandle<()>,
    ) {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let mut state = RtcPeerState::new_with_lifecycle_floors(
            peer_id.clone(),
            floors.next_pc_generation,
            floors.next_revision,
        );
        state.signaling_generation = *signaling_generation_rx.borrow();
        let initial_snapshot = state.to_snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot.clone());
        let (public_snapshot_tx, public_snapshot_rx) = watch::channel(initial_snapshot.to_public());
        let (completion_tx, completion_rx) = mpsc::channel(COMPLETION_CAPACITY);
        let (event_mailbox, event_rx) = peer_event_mailbox_with_capacity_and_bytes(
            PEER_EVENT_MAILBOX_CAPACITY,
            deps.buffer_config.max_structural_event_bytes,
        );
        #[cfg(feature = "test-utils")]
        let event_mailbox = match &deps.driver_event_observer {
            Some(observer) => {
                let observer = observer.clone();
                let observed_peer = peer_id.clone();
                event_mailbox.with_observer(Arc::new(move |event| {
                    observer(&observed_peer, event);
                }))
            }
            None => event_mailbox,
        };
        let (lane_tx, lane_rx) = mpsc::channel(LANE_CAPACITY);
        let (signaling_retry_wake_tx, signaling_retry_wake_rx) = mpsc::unbounded_channel();
        let lanes = crate::wire::webrtc::v020::peer::lane_handle::ActiveLaneRegistry::new();

        let effect_port = make_effect_port(
            peer_id.clone(),
            event_mailbox.clone(),
            lane_tx.clone(),
            deps.clone(),
        );

        let controller = Self {
            state,
            effect_port: effect_port.clone(),
            lanes: lanes.clone(),
            inbound: deps.inbound.clone(),
            event_rx,
            command_rx,
            lane_rx,
            lane_tx: lane_tx.clone(),
            completion_tx,
            completion_rx,
            pending_resources: Vec::new(),
            internal_inputs: std::collections::VecDeque::new(),
            retry_signaling: None,
            signaling_retry_wake_tx,
            signaling_retry_wake_rx,
            signaling_retry_wake_armed: false,
            pending_configuration_refreshes: std::collections::HashMap::new(),
            snapshot_tx,
            public_snapshot_tx,
            signaling_tx,
            network_rx,
            signaling_generation_rx,
            policy_clock,
            deps: deps.clone(),
        };
        let task = tokio::spawn(controller.run());
        let handle = RtcPeerControllerHandle::new(
            peer_id,
            deps.controller_lifecycle_id,
            command_tx,
            snapshot_rx,
            public_snapshot_rx,
            #[cfg(feature = "test-utils")]
            event_mailbox.clone(),
            lanes,
            effect_port,
            deps.buffer_config.max_inbound_message_bytes,
        );
        (handle, lane_tx, event_mailbox, task)
    }

    /// T1 seam: run the real Controller task with a recording effect sink and
    /// an explicit policy clock, without constructing a SessionFactory or
    /// PeerConnection. The returned mailbox is the controlled fact injector.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_with_effect_port(
        peer_id: ActrId,
        next_pc_generation: u64,
        effect_port: Arc<dyn RtcPeerEffectPort>,
        signaling_tx: mpsc::Sender<SignalingDispatch>,
        network_rx: tokio::sync::broadcast::Receiver<RtcNetworkEvent>,
        signaling_generation_rx: watch::Receiver<Option<u64>>,
        deps: SharedNodeDeps,
        policy_clock: Arc<dyn super::policy_clock::PolicyClock>,
    ) -> (
        RtcPeerControllerHandle,
        mpsc::Sender<LaneEvent>,
        crate::wire::webrtc::v020::events::PeerEventMailbox,
        JoinHandle<()>,
    ) {
        Self::spawn_with_port_factory(
            peer_id,
            ControllerLifecycleFloors {
                next_pc_generation,
                next_revision: 0,
            },
            signaling_tx,
            network_rx,
            signaling_generation_rx,
            deps,
            policy_clock,
            move |_, _, _, _| effect_port,
        )
    }

    async fn run(mut self) {
        tracing::debug!(
            peer = %self.state.peer_id.to_string_repr(),
            controller_lifecycle_id = self.deps.controller_lifecycle_id,
            "v0.20 peer controller event loop started"
        );
        let mut state_rx = self.event_rx.state();
        let mut last_state_seq = 0;
        let mut generation_overflow_counts = std::collections::HashMap::<u64, u64>::new();
        let mut recovery_timer: Option<(u64, super::policy_clock::PolicySleep)> = None;
        let mut recovery_budget_timer: Option<(u64, super::policy_clock::PolicySleep)> = None;
        let mut role_negotiation_timer: Option<(u64, super::policy_clock::PolicySleep)> = None;
        let mut initial_connection_timer: Option<(u64, super::policy_clock::PolicySleep)> = None;
        let mut signaling_generation_watch_open = true;
        'run: loop {
            // Retry critical SDP retained during signaling-queue backpressure
            // before waiting on new inputs: every loop turn (any event or
            // timer wakeup) is a retry opportunity, and the try_send keeps
            // the loop non-blocking.
            self.flush_retry_signaling();

            if let Some(input) = self.internal_inputs.pop_front() {
                if self.apply(input).await {
                    break 'run;
                }
                continue;
            }

            // One non-extendable end-to-end budget covers every Soft retry and
            // Hard rebuild in this logical recovery cycle. Only convergence or
            // exhaustion clears it; per-attempt epoch changes never re-arm it.
            let recovery_cycle = self
                .state
                .recovery_inflight
                .then_some(self.state.recovery_cycle);
            match (
                recovery_cycle,
                recovery_budget_timer.as_ref().map(|(cycle, _)| *cycle),
            ) {
                (Some(cycle), Some(armed)) if cycle == armed => {}
                (Some(cycle), _) => {
                    recovery_budget_timer = Some((
                        cycle,
                        self.policy_clock.sleep(
                            crate::timer::ids::WEBRTC_PEER_RECOVERY_DEADLINE,
                            crate::wire::webrtc::v020::recovery::NETWORK_RECOVERY_TIMEOUT,
                        ),
                    ));
                }
                (None, _) => recovery_budget_timer = None,
            }

            // Arm one non-extendable timer for the exact recovery-attempt
            // epoch. It bounds a Soft restart while an active PC exists,
            // supplies the retry delay after a transient Hard replacement
            // build failure leaves no live PC, and retransmits a signaling wake
            // while an Answerer is parked after retiring its physical session.
            // While a replacement is actually building/pending, its effects own
            // convergence and no timer is armed. Duplicate network facts
            // retain the epoch and cannot move the deadline.
            let recovery_attempt_epoch = (self.state.recovery_inflight
                && (self.state.recovery_attempt_started
                    || (self.state.role == super::state::Role::Answerer
                        && self.state.awaiting_offer
                        && (self.state.active_generation.is_some()
                            || self.state.parked_signaling_generation.is_some())))
                && self.state.building_generation.is_none()
                && matches!(
                    self.state.phase,
                    super::state::RtcPeerPhase::Recovering
                        | super::state::RtcPeerPhase::Negotiating
                        | super::state::RtcPeerPhase::TransportReady
                ))
            .then_some(self.state.recovery_epoch);
            match (
                recovery_attempt_epoch,
                recovery_timer.as_ref().map(|(epoch, _)| *epoch),
            ) {
                (Some(epoch), Some(armed)) if epoch == armed => {}
                (Some(epoch), _) => {
                    recovery_timer = Some((
                        epoch,
                        self.policy_clock.sleep(
                            crate::timer::ids::WEBRTC_ICE_RESTART_WAIT,
                            ICE_RESTART_ATTEMPT_TIMEOUT,
                        ),
                    ));
                }
                (None, _) => recovery_timer = None,
            }

            // Role arbitration has its own exact epoch and short deadline.
            // Re-arming is allowed only when a genuinely new negotiation
            // epoch starts; unrelated controller inputs cannot extend it.
            let role_negotiation_epoch = self
                .state
                .role_negotiation_inflight
                .then_some(self.state.negotiation_epoch);
            match (
                role_negotiation_epoch,
                role_negotiation_timer.as_ref().map(|(epoch, _)| *epoch),
            ) {
                (Some(epoch), Some(armed)) if epoch == armed => {}
                (Some(epoch), _) => {
                    role_negotiation_timer = Some((
                        epoch,
                        self.policy_clock.sleep(
                            crate::timer::ids::RTC_ROLE_NEGOTIATION_WAIT,
                            ROLE_NEGOTIATION_TIMEOUT,
                        ),
                    ));
                }
                (None, _) => role_negotiation_timer = None,
            }

            // One exact timer covers the full first-connection pipeline,
            // including the gap after PC Connected while the readiness anchor
            // is still closed. A WirePool retry advances transport_epoch, so a
            // delayed timer cannot terminate the new attempt.
            let initial_transport_epoch = self
                .state
                .initial_connection_inflight
                .then_some(self.state.transport_epoch);
            match (
                initial_transport_epoch,
                initial_connection_timer
                    .as_ref()
                    .map(|(transport_epoch, _)| *transport_epoch),
            ) {
                (Some(transport_epoch), Some(armed)) if transport_epoch == armed => {}
                (Some(transport_epoch), _) => {
                    initial_connection_timer = Some((
                        transport_epoch,
                        self.policy_clock.sleep(
                            crate::timer::ids::RTC_INITIAL_CONNECTION_ATTEMPT,
                            INITIAL_CONNECTION_ATTEMPT_TIMEOUT,
                        ),
                    ));
                }
                (None, _) => initial_connection_timer = None,
            }

            tokio::select! {
                command = self.command_rx.recv() => {
                    match command {
                        Some(command) => {
                            if self.apply(RtcPeerInput::Command(command)).await {
                                break 'run;
                            }
                        }
                        // All handles dropped: the manager owns teardown; stop.
                        None => break,
                    }
                }
                event = self.event_rx.recv_structural() => {
                    // A `None` (structural sender gone) is ignored: keep serving
                    // the other channels.
                    if let Some(event) = event
                        && self.apply(RtcPeerInput::DriverEvent(event)).await
                    {
                        break;
                    }
                }
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break; // mailbox sender gone
                    }
                    let book = state_rx.borrow_and_update().clone();
                    generation_overflow_counts
                        .retain(|generation, _| book.contains_generation(*generation));
                    let ticks = book.ticks_after(last_state_seq);
                    last_state_seq = book.sequence();
                    for tick in ticks {
                        let seen_overflow = generation_overflow_counts
                            .entry(tick.generation)
                            .or_default();
                        if tick.overflow_count > *seen_overflow {
                            *seen_overflow = tick.overflow_count;
                            if self
                                .apply(RtcPeerInput::MailboxOverflow {
                                    generation: tick.generation,
                                })
                                .await
                            {
                                break 'run;
                            }
                        }
                        if tick.has_driver_state()
                            && self.apply(RtcPeerInput::StateTick(Box::new(tick))).await
                        {
                            break 'run;
                        }
                    }
                }
                lane = self.lane_rx.recv() => {
                    // A `None` (lane sender gone) is ignored: keep serving the
                    // other channels.
                    if let Some(lane) = lane
                        && self.apply(RtcPeerInput::Lane(lane)).await
                    {
                        break;
                    }
                }
                completion = self.completion_rx.recv() => {
                    // The completion sender is held by `self`, so `None` is
                    // unreachable in practice; ignore it defensively.
                    if let Some(completion) = completion {
                        match completion.into_completion_for(&self.state.peer_id) {
                            Ok(completion) => {
                                if self.apply(RtcPeerInput::Completion(completion)).await {
                                    break;
                                }
                            }
                            Err(error) => {
                                self.state.stale_completions += 1;
                                tracing::warn!(
                                    peer = %self.state.peer_id.to_string_repr(),
                                    %error,
                                    "discarding unauthenticated RTC effect completion"
                                );
                            }
                        }
                    }
                }
                wake = self.signaling_retry_wake_rx.recv() => {
                    self.signaling_retry_wake_armed = false;
                    if wake.is_none() {
                        break;
                    }
                }
                network = self.network_rx.recv() => {
                    match network {
                        Ok(event) => {
                            // Reconcile the latest signaling projection before
                            // reducing a network edge. The two facts travel on
                            // independent channels, so select ordering alone
                            // must not let NetworkAvailable start ICE against
                            // a generation the lifecycle supervisor has
                            // already gated off.
                            let signaling_generation =
                                *self.signaling_generation_rx.borrow_and_update();
                            if self.state.signaling_generation != signaling_generation
                                && self
                                    .apply(RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
                                        RtcNetworkEvent::SignalingGenerationChanged {
                                            generation: signaling_generation,
                                        },
                                    )))
                                    .await
                            {
                                break 'run;
                            }
                            if self
                                .apply(RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(event)))
                                .await
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // At least one network/lifecycle transition was
                            // lost. Conservatively expire sendability before
                            // consuming the retained tail of the broadcast
                            // queue. The next retained NetworkAvailable /
                            // AppForeground fact can then start Soft Recovery;
                            // a retained address-change / force-reconnect fact
                            // still escalates to Hard Recovery. Silently
                            // skipping here can leave a Sendable controller on
                            // a stale transport when the evicted edge was
                            // NetworkUnavailable.
                            tracing::warn!(
                                peer = %self.state.peer_id.to_string_repr(),
                                lagged = n,
                                "network event receiver lagged; expiring sendability before replaying retained events"
                            );
                            if self
                                .apply(RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
                                    RtcNetworkEvent::NetworkUnavailable,
                                )))
                                .await
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                changed = self.signaling_generation_rx.changed(), if signaling_generation_watch_open => {
                    let generation = if changed.is_ok() {
                        *self.signaling_generation_rx.borrow_and_update()
                    } else {
                        signaling_generation_watch_open = false;
                        None
                    };
                    if self
                        .apply(RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
                            RtcNetworkEvent::SignalingGenerationChanged { generation },
                        )))
                        .await
                    {
                        break;
                    }
                }
                _ = async {
                    match recovery_timer.as_mut() {
                        Some((_, timer)) => timer.as_mut().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    let recovery_epoch = recovery_timer
                        .take()
                        .expect("guarded recovery timer")
                        .0;
                    tracing::warn!(
                        peer = %self.state.peer_id.to_string_repr(),
                        recovery_epoch,
                        "v0.20 recovery attempt deadline elapsed; escalating the attempt"
                    );
                    if self
                        .apply(RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch })
                        .await
                    {
                        break;
                    }
                }
                _ = async {
                    match recovery_budget_timer.as_mut() {
                        Some((_, timer)) => timer.as_mut().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    let recovery_cycle = recovery_budget_timer
                        .take()
                        .expect("guarded recovery budget timer")
                        .0;
                    tracing::warn!(
                        peer = %self.state.peer_id.to_string_repr(),
                        recovery_cycle,
                        "v0.20 recovery budget deadline elapsed; resolving the recovery cycle"
                    );
                    if self
                        .apply(RtcPeerInput::RecoveryBudgetElapsed { recovery_cycle })
                        .await
                    {
                        break;
                    }
                }
                _ = async {
                    match role_negotiation_timer.as_mut() {
                        Some((_, timer)) => timer.as_mut().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    let negotiation_epoch = role_negotiation_timer
                        .take()
                        .expect("guarded role-negotiation timer")
                        .0;
                    tracing::warn!(
                        peer = %self.state.peer_id.to_string_repr(),
                        negotiation_epoch,
                        "v0.20 role negotiation deadline elapsed without an assignment"
                    );
                    if self
                        .apply(RtcPeerInput::RoleNegotiationDeadlineElapsed {
                            negotiation_epoch,
                        })
                        .await
                    {
                        break;
                    }
                }
                _ = async {
                    match initial_connection_timer.as_mut() {
                        Some((_, timer)) => timer.as_mut().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    let transport_epoch = initial_connection_timer
                        .take()
                        .expect("guarded initial-connection timer")
                        .0;
                    tracing::warn!(
                        peer = %self.state.peer_id.to_string_repr(),
                        transport_epoch,
                        "v0.20 initial connection deadline elapsed; terminating the attempt"
                    );
                    if self
                        .apply(RtcPeerInput::InitialConnectionDeadlineElapsed {
                            transport_epoch,
                        })
                        .await
                    {
                        break;
                    }
                }
            }
        }

        // Final teardown: close every remaining session via the effect port
        // (invariant #12: every PeerConnection is closed by its session).
        for (_, (_, respond)) in self.pending_configuration_refreshes.drain() {
            let _ = respond.send(Err(RtcConfigurationRefreshError::Closed));
        }
        self.effect_port.close_all_sessions().await;
    }

    /// Feed one input to the reducer, publish the snapshot if it changed, and
    /// submit the returned effects to the effect port.
    ///
    /// Returns `true` when the controller should exit: an explicit Close's
    /// close-barrier has fully completed (terminal `Idle`). This converges the
    /// task on a terminal Close even if a stale handle clone is still holding
    /// the command channel open (N1) — otherwise only a command-channel close
    /// (every handle dropped) would stop the loop.
    async fn apply(&mut self, input: RtcPeerInput) -> bool {
        let completed_refresh = match &input {
            RtcPeerInput::Completion(super::reducer::EffectCompletion::RefreshConfiguration {
                generation,
                operation_id,
                result,
            }) => {
                let exact_pending =
                    self.state
                        .pending_operations
                        .get(operation_id)
                        .is_some_and(|pending| {
                            pending.kind == PendingRtcOperationKind::RefreshConfiguration
                                && pending.pc_generation == *generation
                                && pending.ice_generation.is_none()
                        });
                let exact_ack = self
                    .pending_configuration_refreshes
                    .get(operation_id)
                    .is_some_and(|(expected_generation, _)| *expected_generation == *generation);
                (exact_pending && exact_ack).then(|| {
                    let result = if self.state.close_requested {
                        Err(RtcConfigurationRefreshError::Closed)
                    } else {
                        result.as_ref().map(|_| ()).map_err(|error| {
                            RtcConfigurationRefreshError::Rejected(error.to_string())
                        })
                    };
                    (*operation_id, result)
                })
            }
            _ => None,
        };

        // A lane's pump reporting Closed removes that lane from the send-path
        // registry (the reducer independently transitions state; this is the
        // data-plane mirror so the send path stops handing out the dead lane).
        if let RtcPeerInput::Lane(LaneEvent::Closed { generation, lane }) = &input {
            self.lanes.remove(*generation, *lane);
        }

        let discovered = match &input {
            RtcPeerInput::DriverEvent(PeerDriverEvent::DataChannelDiscovered {
                generation,
                resource_id,
                descriptor,
            }) => Some(PendingDiscoveredResource::DataChannel {
                generation: *generation,
                resource_id: *resource_id,
                descriptor: descriptor.clone(),
            }),
            RtcPeerInput::DriverEvent(PeerDriverEvent::TrackDiscovered {
                generation,
                resource_id,
                descriptor,
            }) => Some(PendingDiscoveredResource::Track {
                generation: *generation,
                resource_id: *resource_id,
                descriptor: descriptor.clone(),
            }),
            _ => None,
        };
        if let Some(discovered) = discovered
            && let Some(pending) = self.process_discovered_resource(discovered).await
        {
            self.pending_resources.push(pending);
        }

        // S6-2: media topology commands execute inline on the controller's
        // event loop (serialized with every other effect) and respond via the
        // command's oneshot, bypassing the reducer entirely (it has a no-op
        // arm only so direct-drive tests stay exhaustive). The resulting
        // NegotiationNeeded driver event drives the renegotiation.
        let mut connect_respond = None;
        let mut refresh_respond = None;
        let input = match input {
            RtcPeerInput::Command(RtcPeerCommand::ConnectCommitted { respond }) => {
                connect_respond = Some(respond);
                RtcPeerInput::Command(RtcPeerCommand::Connect)
            }
            RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration { config, respond }) => {
                refresh_respond = respond;
                RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
                    config,
                    respond: None,
                })
            }
            RtcPeerInput::Command(RtcPeerCommand::AddTrack { track, respond }) => {
                let mut timed_out_generation = None;
                let result = match self.state.active_generation {
                    Some(generation) if !self.state.close_requested => match crate::timer::timeout(
                        crate::timer::ids::RTC_CONTROLLER_INLINE_OPERATION,
                        CONTROLLER_INLINE_OPERATION_TIMEOUT,
                        self.effect_port.add_track(generation, track),
                    )
                    .await
                    {
                        Ok(Some(result)) => result.map_err(|e| e.to_string()),
                        Ok(None) => Err("connection not ready".to_string()),
                        Err(_) => {
                            timed_out_generation = Some(generation);
                            Err(format!("add_track timed out (generation {generation})"))
                        }
                    },
                    _ => Err("connection not ready".to_string()),
                };
                let _ = respond.send(result);
                if let Some(generation) = timed_out_generation {
                    // webrtc 0.20.3 may have committed the transceiver change
                    // before the cancelled await point, leaving the PC holding
                    // a track the local registry does not track. The session
                    // state is untrustworthy: fail the negotiation so the
                    // generation is retired rather than serving later offers.
                    self.internal_inputs
                        .push_back(RtcPeerInput::TrackMutationTimedOut { generation });
                }
                return self.state.terminal_close_done;
            }
            RtcPeerInput::Command(RtcPeerCommand::RemoveTrack { sender, respond }) => {
                let mut timed_out_generation = None;
                let result = match self.state.active_generation {
                    Some(generation) if !self.state.close_requested => match crate::timer::timeout(
                        crate::timer::ids::RTC_CONTROLLER_INLINE_OPERATION,
                        CONTROLLER_INLINE_OPERATION_TIMEOUT,
                        self.effect_port.remove_track(generation, sender),
                    )
                    .await
                    {
                        Ok(Some(result)) => result.map_err(|e| e.to_string()),
                        Ok(None) => Err("connection not ready".to_string()),
                        Err(_) => {
                            timed_out_generation = Some(generation);
                            Err(format!("remove_track timed out (generation {generation})"))
                        }
                    },
                    _ => Err("connection not ready".to_string()),
                };
                let _ = respond.send(result);
                if let Some(generation) = timed_out_generation {
                    // Same partial-mutation hazard as AddTrack: the PC may
                    // already reflect the removal while the registry does not.
                    self.internal_inputs
                        .push_back(RtcPeerInput::TrackMutationTimedOut { generation });
                }
                return self.state.terminal_close_done;
            }
            other => other,
        };

        let before = self.state.revision;
        let was_sendable = self.state.sendable();
        let phase_before = self.state.phase;
        let was_terminally_unavailable = self.state.terminally_unavailable();
        let active_generation_before = self.state.active_generation;
        let effects = reduce(&mut self.state, input);
        let is_sendable = self.state.sendable();
        if self.state.phase != phase_before {
            tracing::debug!(
                peer = %self.state.peer_id.to_string_repr(),
                phase_before = ?phase_before,
                phase = ?self.state.phase,
                transport_epoch = self.state.transport_epoch,
                active_generation = self.state.active_generation,
                "v0.20 peer phase transition"
            );
        }
        let builds_session = effects
            .iter()
            .any(|effect| matches!(effect, RtcPeerEffect::BuildSession { .. }));
        let invalidates_active_generation = builds_session
            && active_generation_before.is_some()
            && (self.state.recovery_inflight
                || self.state.active_generation != active_generation_before);
        if let Some(respond) = refresh_respond {
            if let Some((generation, operation_id)) = effects.iter().find_map(|effect| match effect
            {
                RtcPeerEffect::RefreshConfiguration {
                    generation,
                    operation_id,
                    ..
                } => Some((*generation, *operation_id)),
                _ => None,
            }) {
                self.pending_configuration_refreshes
                    .insert(operation_id, (generation, respond));
            } else {
                let _ = respond.send(Err(RtcConfigurationRefreshError::ConnectionNotReady));
            }
        }
        if self.state.revision > before {
            let snapshot = self.state.to_snapshot();
            let _ = self.snapshot_tx.send(snapshot.clone());
            let _ = self.public_snapshot_tx.send(snapshot.to_public());
            if let Some(recovery_obligations) = &self.deps.recovery_obligations {
                recovery_obligations.observe_snapshot(&snapshot);
            }
            if let Some(peer_snapshot_events) = &self.deps.peer_snapshot_events {
                let _ = peer_snapshot_events.send(snapshot);
            }
        }
        if let Some(respond) = connect_respond {
            // This acknowledgement is deliberately after snapshot publication:
            // a caller subscribing after it resolves can no longer observe the
            // previous transport attempt as the current Connect result.
            tracing::debug!(
                peer = %self.state.peer_id.to_string_repr(),
                transport_epoch = self.state.transport_epoch,
                "v0.20 connect committed"
            );
            let _ = respond.send(self.state.transport_epoch);
        }
        if invalidates_active_generation
            && let Some(generation) = active_generation_before
            && let Some(peer_generation_invalidated) = &self.deps.peer_generation_invalidated
        {
            let _ = peer_generation_invalidated.send((self.state.peer_id.clone(), generation));
        }
        if was_sendable != is_sendable {
            tracing::info!(
                peer = %self.state.peer_id.to_string_repr(),
                sendable = is_sendable,
                transport_epoch = self.state.transport_epoch,
                generation = self.state.active_generation,
                "v0.20 peer sendability changed"
            );
            if let Some(peer_sendability_changed) = &self.deps.peer_sendability_changed {
                let _ = peer_sendability_changed.send((self.state.peer_id.clone(), is_sendable));
            }
        }
        if !was_terminally_unavailable
            && self.state.terminally_unavailable()
            && let Some(reason) = self.state.unavailable_reason
            && let Some(peer_transport_retired) = &self.deps.peer_transport_retired
        {
            let _ = peer_transport_retired.send(PeerTransportRetirement {
                lifecycle: crate::transport::WebRtcTransportLifecycle {
                    peer_id: self.state.peer_id.clone(),
                    controller_lifecycle_id: self.deps.controller_lifecycle_id,
                    transport_epoch: self.state.transport_epoch,
                },
                reason,
            });
        }
        for effect in effects {
            if !self.execute(effect).await {
                break;
            }
        }
        if let Some((operation_id, result)) = completed_refresh
            && let Some((_, respond)) = self.pending_configuration_refreshes.remove(&operation_id)
        {
            let _ = respond.send(result);
        }
        self.retry_pending_resources().await;
        self.state.terminal_close_done
    }

    async fn process_discovered_resource(
        &mut self,
        resource: PendingDiscoveredResource,
    ) -> Option<PendingDiscoveredResource> {
        let (generation, resource_id) = match &resource {
            PendingDiscoveredResource::DataChannel {
                generation,
                resource_id,
                ..
            }
            | PendingDiscoveredResource::Track {
                generation,
                resource_id,
                ..
            } => (*generation, *resource_id),
        };

        // Building/pending replacement resources are authoritative during
        // make-before-break. The old active Session remains physically live
        // only until the switch; accepting its late DataChannel/Track tokens
        // would repopulate lane/media caches after that generation was already
        // invalidated.
        let resource_generation = self
            .state
            .pending_generation
            .or(self.state.building_generation)
            .or(self.state.active_generation);
        if resource_generation != Some(generation) {
            self.discard_remote_resource_bounded(generation, resource_id)
                .await;
            return None;
        }

        match &resource {
            PendingDiscoveredResource::DataChannel { descriptor, .. } => {
                let Some(lane) = crate::wire::webrtc::v020::peer::lane::verify_channel_metadata(
                    &descriptor.label,
                    descriptor.negotiated,
                ) else {
                    tracing::warn!(
                        peer = %self.state.peer_id.to_string_repr(),
                        generation,
                        label = %descriptor.label,
                        "discarding remote channel token with unknown/unmatched metadata"
                    );
                    self.discard_remote_resource_bounded(generation, resource_id)
                        .await;
                    return None;
                };
                if self.lanes.get(generation, lane).is_some() {
                    self.discard_remote_resource_bounded(generation, resource_id)
                        .await;
                    return None;
                }
                match self
                    .effect_port
                    .register_remote_channel(generation, resource_id, lane)
                {
                    ResourceRegistration::Registered(handle) => {
                        self.lanes.insert(handle);
                        None
                    }
                    ResourceRegistration::SessionPending => Some(resource),
                    ResourceRegistration::MissingOrMismatched => {
                        self.discard_remote_resource_bounded(generation, resource_id)
                            .await;
                        None
                    }
                }
            }
            PendingDiscoveredResource::Track { descriptor, .. } => {
                match crate::timer::timeout(
                    crate::timer::ids::RTC_CONTROLLER_INLINE_OPERATION,
                    CONTROLLER_INLINE_OPERATION_TIMEOUT,
                    self.effect_port.register_remote_track(
                        generation,
                        resource_id,
                        descriptor.meta.clone(),
                    ),
                )
                .await
                {
                    Ok(ResourceRegistration::Registered(_)) => None,
                    Ok(ResourceRegistration::SessionPending) => Some(resource),
                    Ok(ResourceRegistration::MissingOrMismatched) | Err(_) => {
                        tracing::warn!(
                            peer = %self.state.peer_id.to_string_repr(),
                            generation,
                            ?resource_id,
                            "remote track registration timed out or missed; discarding the resource"
                        );
                        self.discard_remote_resource_bounded(generation, resource_id)
                            .await;
                        None
                    }
                }
            }
        }
    }

    async fn discard_remote_resource_bounded(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
    ) {
        if crate::timer::timeout(
            crate::timer::ids::RTC_CONTROLLER_INLINE_OPERATION,
            CONTROLLER_INLINE_OPERATION_TIMEOUT,
            self.effect_port
                .discard_remote_resource(generation, resource_id),
        )
        .await
        .is_err()
        {
            tracing::warn!(
                peer = %self.state.peer_id.to_string_repr(),
                generation,
                "discarding remote RTC resource timed out"
            );
        }
    }

    async fn retry_pending_resources(&mut self) {
        let pending = std::mem::take(&mut self.pending_resources);
        for resource in pending {
            if let Some(resource) = self.process_discovered_resource(resource).await {
                self.pending_resources.push(resource);
            }
        }
    }

    /// Try to deliver a retained critical signaling message. Called once per
    /// event-loop turn; success clears the slot, a still-full queue leaves it
    /// retained for the next turn.
    fn flush_retry_signaling(&mut self) {
        let Some(retained) = self.retry_signaling.take() else {
            return;
        };
        if !self.retained_signaling_is_live(&retained) {
            tracing::debug!(
                peer = %self.state.peer_id.to_string_repr(),
                transport_epoch = retained.transport_epoch,
                generation = retained.generation,
                "discarding retained signaling from a superseded transport attempt"
            );
            return;
        }
        let dispatch = SignalingDispatch::tracked(
            self.state.peer_id.clone(),
            self.deps.controller_lifecycle_id,
            retained.transport_epoch,
            retained.generation,
            retained.negotiation_epoch,
            retained.message,
        );
        match self.signaling_tx.try_send(dispatch) {
            Ok(()) => {
                tracing::info!(
                    peer = %self.state.peer_id.to_string_repr(),
                    "retried retained SDP through the signaling queue"
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(dispatch)) => {
                self.retry_signaling = Some(RetainedSignaling {
                    transport_epoch: retained.transport_epoch,
                    generation: retained.generation,
                    negotiation_epoch: retained.negotiation_epoch,
                    message: dispatch.message,
                });
                self.arm_signaling_retry_wake();
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    /// Arm a one-shot wake for when the signaling queue regains capacity:
    /// releasing a queue slot is itself the retry trigger, so a retained
    /// critical message is not hostage to an unrelated event-loop turn.
    fn arm_signaling_retry_wake(&mut self) {
        if self.signaling_retry_wake_armed {
            return;
        }
        self.signaling_retry_wake_armed = true;
        let signaling_tx = self.signaling_tx.clone();
        let wake_tx = self.signaling_retry_wake_tx.clone();
        tokio::spawn(async move {
            if let Ok(permit) = signaling_tx.reserve_owned().await {
                drop(permit);
            }
            let _ = wake_tx.send(());
        });
    }

    /// Whether a retained signaling message still belongs to the transport
    /// attempt that produced it. SDP is fenced by its PC generation. A role
    /// probe carries generation 0, so it is fenced by the transport epoch plus
    /// the attempt still being in flight: replaying a superseded attempt's
    /// probe re-runs server arbitration, and the repeated Offerer assignment
    /// would rebuild the fresh generation that the newer attempt just
    /// allocated.
    fn retained_signaling_is_live(&self, retained: &RetainedSignaling) -> bool {
        if self.state.transport_epoch != retained.transport_epoch {
            return false;
        }
        if retained.generation == 0 {
            self.state.initial_connection_inflight
        } else {
            self.state.is_live_generation(retained.generation)
                || (self.state.role == super::state::Role::Answerer
                    && self.state.awaiting_offer
                    && self.state.parked_signaling_generation == Some(retained.generation))
        }
    }

    /// Submit one effect to the effect port. `CreateLane` is awaited inline (so
    /// the channel exists before the subsequent CreateOffer is issued, keeping
    /// it in the offer SDP). `SendSignaling` is fire-and-forget to the router.
    /// All other blocking effects go through the port's submit/completion flow.
    /// Returns whether later effects from the same reducer turn may execute.
    async fn execute(&mut self, effect: RtcPeerEffect) -> bool {
        match effect {
            RtcPeerEffect::CreateLane { generation, lane } => {
                // Ask the effect port to create and own the lane's data channel.
                // The session stores the channel and spawns the pump; the
                // controller only installs the returned LaneHandle token into
                // the send-path registry. Dedup: an already-registered lane is
                // not re-created.
                if self.lanes.get(generation, lane).is_none() {
                    match crate::timer::timeout(
                        crate::timer::ids::RTC_CONTROLLER_INLINE_OPERATION,
                        CONTROLLER_INLINE_OPERATION_TIMEOUT,
                        self.effect_port.create_lane(generation, lane),
                    )
                    .await
                    {
                        Ok(Ok(Some(handle))) => {
                            tracing::debug!(
                                peer = %self.state.peer_id.to_string_repr(),
                                generation,
                                ?lane,
                                "v0.20 data-channel lane created"
                            );
                            self.lanes.insert(handle)
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => {
                            tracing::error!(
                                peer = %self.state.peer_id.to_string_repr(),
                                generation,
                                ?lane,
                                %error,
                                "failed to create RTC data-channel lane"
                            );
                            if lane == LaneKind::READINESS_ANCHOR {
                                self.internal_inputs
                                    .push_back(RtcPeerInput::LaneCreationFailed {
                                        generation,
                                        lane,
                                    });
                                return false;
                            }
                        }
                        Err(_) => {
                            tracing::error!(
                                peer = %self.state.peer_id.to_string_repr(),
                                generation,
                                ?lane,
                                timeout_ms = CONTROLLER_INLINE_OPERATION_TIMEOUT.as_millis(),
                                "creating RTC data-channel lane timed out"
                            );
                            if lane == LaneKind::READINESS_ANCHOR {
                                self.internal_inputs
                                    .push_back(RtcPeerInput::LaneCreationFailed {
                                        generation,
                                        lane,
                                    });
                                return false;
                            }
                        }
                    }
                }
            }
            RtcPeerEffect::CloseSession {
                generation,
                operation_id,
            } => {
                // Remove the generation's lanes from the send-path registry.
                // A Hard Recovery may already have installed the replacement
                // generation for the same peer. The effect port removes and
                // closes the session (its close() explicitly closes each owned
                // DataChannel, RFC work item 11).
                self.lanes.remove_generation(generation);
                self.submit_blocking(RtcPeerEffect::CloseSession {
                    generation,
                    operation_id,
                });
            }
            RtcPeerEffect::SendSignaling {
                generation,
                message,
            } => {
                // The lifecycle supervisor is the sole signaling connection
                // owner. Do not let a queued RTC completion publish SDP/ICE
                // after that supervisor has withdrawn the committed
                // generation; the watch edge will leave the recovery intent
                // pending until a new generation is committed.
                if self.signaling_generation_rx.borrow().is_none() {
                    tracing::debug!(
                        peer = %self.state.peer_id.to_string_repr(),
                        "no committed signaling generation; dropping RTC signaling effect \
                         (the role-negotiation probe is re-driven when the gate opens)"
                    );
                    return true;
                }
                // N2 backpressure: never block the event loop on a full signaling
                // queue, or one peer's signaling storm would stall the node's
                // other controllers. Non-blocking try_send; a full queue drops the
                // message and counts it (observable `signaling_dropped` metric).
                let is_candidate = matches!(message, SignalingMessage::Candidate(_));
                let dispatch = SignalingDispatch::tracked(
                    self.state.peer_id.clone(),
                    self.deps.controller_lifecycle_id,
                    self.state.transport_epoch,
                    generation,
                    self.state.negotiation_epoch,
                    message,
                );
                match self.signaling_tx.try_send(dispatch) {
                    Ok(()) => {
                        // Candidates are high-volume trickle traffic; only the
                        // critical SDP/role messages get a dispatch log.
                        if !is_candidate {
                            tracing::debug!(
                                peer = %self.state.peer_id.to_string_repr(),
                                generation,
                                "v0.20 SDP/role signaling dispatched to the router queue"
                            );
                        }
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(dispatch)) => {
                        let message = dispatch.message;
                        self.state.signaling_dropped += 1;
                        self.state.bump_revision();
                        // Publish the metric immediately: a dropped offer may be
                        // the last event this peer sees for a while, so the
                        // drop must be observable now, not on the next apply.
                        let snapshot = self.state.to_snapshot();
                        let _ = self.snapshot_tx.send(snapshot.clone());
                        let _ = self.public_snapshot_tx.send(snapshot.to_public());
                        if is_candidate {
                            // Trickle ICE tolerates candidate loss (the remote
                            // connects on the candidates that get through, and
                            // ICE re-gathers on restart) — drop quietly.
                            tracing::debug!(
                                peer = %self.state.peer_id.to_string_repr(),
                                signaling_dropped = self.state.signaling_dropped,
                                "signaling queue full; dropped ICE candidate"
                            );
                        } else {
                            // Losing critical signaling strands the attempt:
                            // the reducer already marked it announced, the
                            // outer timeout does not reset it, and a later
                            // Connect is ignored outside Idle. Retain the
                            // newest message, fenced by the originating
                            // transport attempt, and retry it on every
                            // event-loop turn instead of dropping it (a newer
                            // offer/answer supersedes a retained one, which is
                            // the correct attempt semantics).
                            self.retry_signaling = Some(RetainedSignaling {
                                transport_epoch: self.state.transport_epoch,
                                generation,
                                negotiation_epoch: self.state.negotiation_epoch,
                                message,
                            });
                            self.arm_signaling_retry_wake();
                            tracing::warn!(
                                peer = %self.state.peer_id.to_string_repr(),
                                signaling_dropped = self.state.signaling_dropped,
                                "signaling queue full; retained SDP offer/answer for retry on the next event-loop turn"
                            );
                        }
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                }
            }
            // All other blocking effects (BuildSession, CreateOffer, CreateAnswer,
            // SetLocal/SetRemoteDescription, AddIceCandidate, RestartIce) go
            // through the effect port's submit/completion flow.
            blocking => {
                self.submit_blocking(blocking);
            }
        }
        true
    }

    fn submit_blocking(&mut self, effect: RtcPeerEffect) {
        let operation_id = effect.pending_identity().map(|(_, id, _, _)| id);
        let tagged = TaggedRtcPeerEffect::new(self.state.peer_id.clone(), effect)
            .expect("controller submits only completion-producing effects");
        if let Err(error) = self.effect_port.submit(tagged, self.completion_tx.clone()) {
            if let Some(operation_id) = operation_id {
                self.state.pending_operations.remove(&operation_id);
                if let Some((_, respond)) =
                    self.pending_configuration_refreshes.remove(&operation_id)
                {
                    let _ = respond.send(Err(RtcConfigurationRefreshError::EffectPort(
                        error.to_string(),
                    )));
                }
            }
            self.state.stale_completions += 1;
            tracing::error!(
                peer = %self.state.peer_id.to_string_repr(),
                %error,
                "RTC effect port rejected a controller-issued operation"
            );
        }
    }
}
