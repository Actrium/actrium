//! `SignalingRouter` — routes signaling messages between per-peer controllers
//! and the signaling transport by remote `ActrId` (RFC §"SignalingRouter").
//!
//! The router is the *only* consumer of the signaling transport's inbound
//! stream (RFC: "the SignalingClient receive stream MUST have one consumer").
//! Inbound Offer/Answer/Candidate/RoleAssignment are dispatched to the target
//! peer's controller; an offer from an unknown peer requests the manager to create
//! an answerer controller. Outbound messages from controllers are forwarded onto
//! the transport.
//!
//! The router is transport-agnostic: [`SignalingTransport`] abstracts the wire
//! (the production WebSocket signaling client, or an in-memory bus in tests).
//! It does NOT do SDP, role, or ICE decisions — those belong to the per-peer
//! controller; the router only routes.

use std::sync::Arc;

use actr_protocol::ActrId;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

use super::{SharedLocalId, SignalingDispatch, SignalingInbound};

const DEFAULT_INBOUND_DISPATCH_TASKS: usize = 256;

/// Concurrent budget-shed escalations allowed across all peers. Mirrors the
/// dispatch budget: under a sustained SDP flood from fresh identities the
/// newest escalation is dropped (logged) instead of growing router tasks and
/// memory without bound.
const MAX_CONCURRENT_ESCALATIONS: usize = 256;
const INBOUND_DISPATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// The signaling transport: how the router sends and receives wire messages.
///
/// Production binds the 0.14 `WebSocketSignalingClient` (preserving the wire
/// format); tests bind an in-memory bus. `recv` is consumed exactly once by the
/// router's run loop.
#[async_trait]
pub(crate) trait SignalingTransport: Send + Sync {
    /// Send a signaling message to a remote peer (via the signaling server).
    async fn send(&self, to: ActrId, message: SignalingMessage) -> Result<(), String>;

    /// Send a message with an optional opaque delivery context. Existing test
    /// transports keep the legacy two-argument surface; production transports
    /// MUST override this hook to preserve the context end to end.
    ///
    /// The default discards `RelayAttemptIdentity`: a transport that only
    /// overrides `send` quietly regresses to fire-and-forget relays, and the
    /// delivery-result fence never arms (target-unavailable observations and
    /// exact-attempt diagnostics silently stop working).
    async fn send_dispatch(&self, dispatch: SignalingDispatch) -> Result<(), String> {
        self.send(dispatch.peer, dispatch.message).await
    }

    /// Receive the next inbound message `(from, message)`. Returns `None` when
    /// the transport is closed.
    async fn recv(&self) -> Option<(ActrId, SignalingMessage)>;

    /// Receive either a peer message or a server-side target disposition.
    /// Legacy transports get the peer-message-only default implementation.
    async fn recv_event(&self) -> Option<SignalingInbound> {
        self.recv()
            .await
            .map(|(from, message)| SignalingInbound::PeerMessage { from, message })
    }
}

/// Whether losing this inbound message strands negotiation (no router-level
/// retransmit exists for it). Only these justify forcing a peer's recovery;
/// ordinary candidate or probe loss under congestion must not trigger a
/// reconnect storm.
fn message_strands_negotiation(message: &SignalingMessage) -> bool {
    matches!(
        message,
        SignalingMessage::Offer { .. }
            | SignalingMessage::Answer { .. }
            | SignalingMessage::IceRestartOffer { .. }
    )
}

/// Per-peer ordered dispatch chaining shared by the router (dispatch) and the
/// manager (peer teardown).
///
/// Each new dispatch from a peer awaits the previous dispatch's completion
/// before routing. Chaining on explicit completion watchers — not contending
/// a shared mutex — makes the order follow the receive loop strictly: a
/// freshly spawned task can otherwise be polled first (tokio's LIFO
/// scheduling slot), inverting same-peer signaling order. Unrelated peers
/// remain fully concurrent.
///
/// Entries are inserted on first dispatch and removed by teardown via
/// [`PeerDispatchChains::release`], so an Internet-facing endpoint cannot
/// grow this router-lifetime map without bound by signaling from fresh peer
/// identities. Releasing forgets only the chain tail watcher; an in-flight
/// dispatch still completes on its own.
#[derive(Clone, Default)]
pub(crate) struct PeerDispatchChains {
    inner: Arc<std::sync::Mutex<ChainMap>>,
}

#[derive(Default)]
struct ChainMap {
    peers: std::collections::HashMap<ActrId, PeerDispatchChain>,
    /// Monotone id of the most recent dispatch installed per entry. A
    /// finishing dispatch may only remove an entry it still owns, so a
    /// successor that installed a newer tail between the finisher's liveness
    /// check and its cleanup keeps its chain.
    next_epoch: u64,
}

#[derive(Default)]
struct PeerDispatchChain {
    /// Flips to `true` when the peer's most recent dispatch finishes; the
    /// next dispatch waits on it before routing.
    previous_done: Option<tokio::sync::watch::Receiver<bool>>,
    /// Identity of the dispatch whose completion watcher is installed as the
    /// tail (see [`ChainMap::next_epoch`]).
    epoch: u64,
    /// Test seam: when present, the peer's dispatches park on this gate
    /// before routing. Production never installs one.
    #[cfg(test)]
    gate: Option<Arc<tokio::sync::Notify>>,
}

impl PeerDispatchChains {
    /// Spawn an ordered dispatch: `build` receives the peer's previous
    /// completion watcher (to await) and the peer's test gate (if any); the
    /// returned completion sender must be flipped to `true` when the dispatch
    /// finishes, and becomes the chain's next tail.
    fn ordered<F, Fut>(&self, peer: &ActrId, tasks: &mut tokio::task::JoinSet<()>, build: F)
    where
        F: FnOnce(
            u64,
            Option<tokio::sync::watch::Receiver<bool>>,
            Option<Arc<tokio::sync::Notify>>,
            tokio::sync::watch::Sender<bool>,
        ) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut inner = self
            .inner
            .lock()
            .expect("signaling dispatch chain map poisoned");
        let previous_done = inner
            .peers
            .get_mut(peer)
            .and_then(|state| state.previous_done.take());
        #[cfg(test)]
        let gate = inner.peers.get(peer).and_then(|state| state.gate.clone());
        #[cfg(not(test))]
        let gate: Option<Arc<tokio::sync::Notify>> = None;
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);
        inner.next_epoch = inner.next_epoch.wrapping_add(1).max(1);
        let mut epoch = inner.next_epoch;
        // After a full 2^64 wrap a fresh epoch could collide with the peer's
        // installed tail; skip past it so a stale `release_tail` can never
        // remove a successor's live entry.
        if inner
            .peers
            .get(peer)
            .is_some_and(|state| state.epoch == epoch)
        {
            epoch = epoch.wrapping_add(1).max(1);
            inner.next_epoch = epoch;
        }
        let state = inner.peers.entry(peer.clone()).or_default();
        state.previous_done = Some(done_rx);
        state.epoch = epoch;
        tasks.spawn(build(epoch, previous_done, gate, done_tx));
    }

    /// Forget the closed peer's chain tail. Called from manager teardown
    /// (`close_peer`/`close_all`); an in-flight dispatch completes on its own.
    pub(crate) fn release(&self, peer: &ActrId) {
        self.inner
            .lock()
            .expect("signaling dispatch chain map poisoned")
            .peers
            .remove(peer);
    }

    /// Forget the peer's chain tail only if it still belongs to the dispatch
    /// identified by `epoch`. A successor that installed a newer tail between
    /// the finisher's controller-liveness check and this call keeps its entry,
    /// so a stale cleanup can never wipe a freshly installed chain.
    fn release_tail(&self, peer: &ActrId, epoch: u64) {
        let mut inner = self
            .inner
            .lock()
            .expect("signaling dispatch chain map poisoned");
        if inner
            .peers
            .get(peer)
            .is_some_and(|state| state.epoch == epoch)
        {
            inner.peers.remove(peer);
        }
    }

    /// Drop every chain entry. Called when the router's run loop exits: an
    /// aborted dispatch never reaches its own post-routing cleanup, and a
    /// stopped router's ordering state is meaningless anyway.
    pub(crate) fn clear(&self) {
        self.inner
            .lock()
            .expect("signaling dispatch chain map poisoned")
            .peers
            .clear();
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, peer: &ActrId) -> bool {
        self.inner
            .lock()
            .expect("signaling dispatch chain map poisoned")
            .peers
            .contains_key(peer)
    }

    /// Test seam: close (or open) the peer's dispatch gate. A closed gate
    /// parks that peer's dispatches inside their timeout budget without
    /// affecting unrelated peers.
    #[cfg(test)]
    pub(crate) fn set_dispatch_gate(&self, peer: &ActrId, open: bool) {
        let mut inner = self
            .inner
            .lock()
            .expect("signaling dispatch chain map poisoned");
        if open {
            if let Some(state) = inner.peers.get_mut(peer) {
                if let Some(gate) = state.gate.take() {
                    gate.notify_waiters();
                }
            }
        } else {
            inner.peers.entry(peer.clone()).or_default().gate =
                Some(Arc::new(tokio::sync::Notify::new()));
        }
    }
}

/// Routes signaling between controllers and the transport by remote `ActrId`.
#[derive(Clone)]
pub(crate) struct SignalingRouter {
    manager: Arc<RtcConnectionManager>,
    /// Shared local identity (rebind-updatable, CF-1).
    local_id: SharedLocalId,
    /// Serialize messages for one peer while allowing unrelated peers to
    /// dispatch concurrently. Chaining preserves WebSocket signaling order
    /// after moving dispatch out of the router's global receive loop.
    dispatch_chains: PeerDispatchChains,
    /// Concurrent in-flight dispatch tasks allowed before inbound messages
    /// are shed. Configurable so tests exercise the boundary without
    /// spawning hundreds of stalled tasks.
    dispatch_budget: usize,
    /// Peers with a budget-shed escalation still running. At most one
    /// escalation exists per peer at a time, so a flooding peer cannot spawn
    /// an unbounded stream of tasks that all contend on its own controller
    /// mailbox.
    escalating: Arc<std::sync::Mutex<std::collections::HashSet<ActrId>>>,
    /// Test seam: total inbound messages shed by budget exhaustion — a
    /// durable per-message observable for shed-path regression tests.
    #[cfg(test)]
    sheds_observed: Arc<std::sync::atomic::AtomicU64>,
}

impl SignalingRouter {
    pub(crate) fn new(manager: Arc<RtcConnectionManager>, local_id: SharedLocalId) -> Self {
        Self {
            manager,
            local_id,
            dispatch_chains: PeerDispatchChains::default(),
            dispatch_budget: DEFAULT_INBOUND_DISPATCH_TASKS,
            escalating: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            #[cfg(test)]
            sheds_observed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Lower the in-flight dispatch budget (tests exercise the shed boundary
    /// without hundreds of stalled tasks).
    #[cfg(test)]
    pub(crate) fn with_dispatch_budget(mut self, budget: usize) -> Self {
        self.dispatch_budget = budget;
        self
    }

    /// Test seam: number of peers with a budget-shed escalation in flight.
    #[cfg(test)]
    pub(crate) fn escalating_peers_for_test(&self) -> usize {
        self.escalating
            .lock()
            .expect("signaling escalation set poisoned")
            .len()
    }

    /// Test seam: total inbound messages shed by budget exhaustion so tests
    /// can observe that every fed message was processed (not just the ones
    /// whose escalation was still running when sampled).
    #[cfg(test)]
    pub(crate) fn sheds_observed_for_test(&self) -> u64 {
        self.sheds_observed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn local_id(&self) -> ActrId {
        self.local_id.get()
    }

    /// The per-peer dispatch chains, for the manager's teardown cleanup.
    pub(crate) fn dispatch_chains(&self) -> PeerDispatchChains {
        self.dispatch_chains.clone()
    }

    /// Run the routing loop: forward controller-originated messages onto the
    /// transport, and dispatch transport-originated messages to controllers.
    ///
    /// `outbound_rx` is the shared channel every controller's `SendSignaling`
    /// effect feeds (created by the manager).
    pub(crate) async fn run<T: SignalingTransport>(
        &self,
        transport: T,
        mut outbound_rx: mpsc::Receiver<SignalingDispatch>,
    ) {
        let mut inbound_tasks = tokio::task::JoinSet::new();
        // Budget-shed escalations, owned by this loop: at most one per peer
        // (see `escalating`), aborted on shutdown rather than detached.
        let mut escalation_tasks = tokio::task::JoinSet::new();
        tracing::info!("v0.20 signaling router started");
        loop {
            tokio::select! {
                outbound = outbound_rx.recv() => {
                    match outbound {
                        Some(dispatch) => {
                            // A failed send must be observable: feed the
                            // transport failure back into the exact controller
                            // generation (issue #609) instead of letting
                            // negotiation stay marked announced without
                            // delivery. The feedback is best-effort: a
                            // saturated controller mailbox must not park the
                            // router-wide outbound loop, so the hop is bounded
                            // by the same dispatch deadline as inbound work.
                            let peer = dispatch.peer.clone();
                            let attempt = dispatch.attempt;
                            // SDP payloads are the large ones; the manager only
                            // consumes critical SDP, so skip cloning
                            // candidates/role traffic on the happy path.
                            let critical_message = matches!(
                                dispatch.message,
                                SignalingMessage::Offer { .. }
                                    | SignalingMessage::Answer { .. }
                                    | SignalingMessage::IceRestartOffer { .. }
                            )
                            .then(|| dispatch.message.clone());
                            if let Err(error) = transport.send_dispatch(dispatch).await {
                                tracing::warn!(
                                    peer = %peer.to_string_repr(),
                                    generation = attempt.map(|a| a.pc_generation).unwrap_or(0),
                                    %error,
                                    "v0.20 signaling delivery failed"
                                );
                                if let (Some(attempt), Some(message)) =
                                    (attempt, critical_message)
                                {
                                    let manager = self.manager.clone();
                                    let peer = peer.clone();
                                    let _ = crate::timer::timeout(
                                        crate::timer::ids::RTC_SIGNALING_DISPATCH,
                                        INBOUND_DISPATCH_TIMEOUT,
                                        manager.handle_signaling_delivery_failed(
                                            peer,
                                            attempt,
                                            message,
                                        ),
                                    )
                                    .await;
                                }
                            }
                        }
                        // All controllers gone: stop routing.
                        None => break,
                    }
                }
                inbound = transport.recv_event() => {
                    match inbound {
                        Some(SignalingInbound::PeerMessage { from, message }) => {
                            while inbound_tasks.try_join_next().is_some() {}
                            // Reap finished escalations every iteration so a
                            // sustained multi-identity flood cannot grow the
                            // JoinSet (and its retained handles) without bound.
                            while escalation_tasks.try_join_next().is_some() {}
                            if inbound_tasks.len() >= self.dispatch_budget {
                                tracing::warn!(
                                    peer = %from.to_string_repr(),
                                    in_flight = inbound_tasks.len(),
                                    sdp_bearing = message_strands_negotiation(&message),
                                    "signaling dispatch task budget exhausted; dropping inbound message"
                                );
                                // Losing an SDP-bearing message strands
                                // negotiation; losing candidates under
                                // congestion does not. Escalate only the
                                // former, on a loop-owned escalation task so a
                                // full controller mailbox cannot stall the
                                // receive loop for every peer.
                                if message_strands_negotiation(&message) {
                                    // The peer's installed chain tail would keep
                                    // later recovery SDP queued behind a stalled
                                    // predecessor while the budget is full;
                                    // forget it so the next dispatch starts a
                                    // fresh chain (a live in-flight dispatch
                                    // still completes on its own).
                                    self.dispatch_chains.release(&from);
                                    // Escalate at most once per peer, inside
                                    // this loop's JoinSet: the escalation is
                                    // bounded, joined, reaped every iteration,
                                    // and aborted on shutdown, and a full
                                    // controller mailbox cannot multiply tasks
                                    // that contend on it. The concurrency cap
                                    // mirrors the dispatch budget: a flood from
                                    // fresh identities loses its newest
                                    // escalations (logged) instead of growing
                                    // router memory without bound.
                                    #[cfg(test)]
                                    self.sheds_observed
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    let mut escalating = self
                                        .escalating
                                        .lock()
                                        .expect("signaling escalation set poisoned");
                                    if escalating.len() >= MAX_CONCURRENT_ESCALATIONS {
                                        tracing::warn!(
                                            peer = %from.to_string_repr(),
                                            in_flight = escalating.len(),
                                            "signaling shed escalation budget exhausted; dropping escalation"
                                        );
                                    } else if escalating.insert(from.clone()) {
                                        let manager = self.manager.clone();
                                        let peer = from.clone();
                                        let escalating = self.escalating.clone();
                                        escalation_tasks.spawn(async move {
                                            let _ = crate::timer::timeout(
                                                crate::timer::ids::RTC_SIGNALING_DISPATCH,
                                                INBOUND_DISPATCH_TIMEOUT,
                                                async {
                                                    if let Some(handle) =
                                                        manager.get_controller(&peer).await
                                                    {
                                                        let _ = handle.force_reconnect().await;
                                                    }
                                                },
                                            )
                                            .await;
                                            escalating
                                                .lock()
                                                .expect("signaling escalation set poisoned")
                                                .remove(&peer);
                                        });
                                    }
                                }
                                continue;
                            }
                            let router = self.clone();
                            let peer = from.clone();
                            self.dispatch_chains.ordered(
                                &peer,
                                &mut inbound_tasks,
                                |epoch, previous_done, gate, done_tx| async move {
                                    // The budget bounds the WHOLE dispatch —
                                    // ordered handoff included — so head-of-line
                                    // latency cannot compound unboundedly across
                                    // a same-peer chain.
                                    let routing_started = std::sync::atomic::AtomicBool::new(false);
                                    // Captured before `message` moves into the
                                    // routing future.
                                    let strands_negotiation =
                                        message_strands_negotiation(&message);
                                    let routed = crate::timer::timeout(
                                        crate::timer::ids::RTC_SIGNALING_DISPATCH,
                                        INBOUND_DISPATCH_TIMEOUT,
                                        async {
                                            // Ordered handoff: a later message
                                            // from this peer never routes
                                            // before an earlier one, even if
                                            // this task was polled first
                                            // (tokio's LIFO slot).
                                            if let Some(mut previous_done) = previous_done {
                                                loop {
                                                    if *previous_done.borrow() {
                                                        break;
                                                    }
                                                    // A dropped predecessor
                                                    // (aborted dispatch) must
                                                    // not wedge the chain.
                                                    if previous_done.changed().await.is_err() {
                                                        break;
                                                    }
                                                }
                                            }
                                            if let Some(gate) = gate {
                                                gate.notified().await;
                                            }
                                            routing_started.store(true, std::sync::atomic::Ordering::Release);
                                            router.route_inbound(from.clone(), message).await;
                                        },
                                    )
                                    .await
                                    .is_err();
                                    if routed {
                                        let queued = !routing_started.load(std::sync::atomic::Ordering::Acquire);
                                        tracing::warn!(
                                            peer = %from.to_string_repr(),
                                            timeout_ms = INBOUND_DISPATCH_TIMEOUT.as_millis(),
                                            queued,
                                            "peer signaling dispatch expired"
                                        );
                                        // A lost SDP-bearing message strands
                                        // negotiation whether it expired
                                        // queued behind a same-peer
                                        // predecessor or mid-routing on a
                                        // wedged controller; only candidates
                                        // and probes may be dropped silently.
                                        if strands_negotiation
                                            && let Some(handle) =
                                                router.manager.get_controller(&from).await
                                        {
                                            let _ = crate::timer::timeout(
                                                crate::timer::ids::RTC_SIGNALING_DISPATCH,
                                                INBOUND_DISPATCH_TIMEOUT,
                                                handle.force_reconnect(),
                                            )
                                            .await;
                                        }
                                    }
                                    // Messages from peers that never obtained a
                                    // controller are dropped by contract; their
                                    // chain entries are dead weight and must
                                    // not accumulate across fresh remote
                                    // identities. A peer with a live controller
                                    // keeps its entry for teardown to release.
                                    if router.manager.get_controller(&from).await.is_none() {
                                        // The entry is dead weight only if no
                                        // successor dispatch installed a newer
                                        // tail while this one was finishing.
                                        router.dispatch_chains.release_tail(&from, epoch);
                                    }
                                    let _ = done_tx.send_replace(true);
                                },
                            );
                        },
                        Some(SignalingInbound::TargetUnavailable { target, attempt }) => {
                            self.manager.handle_target_unavailable(target, attempt).await
                        }
                        // Transport closed: stop routing.
                        None => break,
                    }
                }
                _ = inbound_tasks.join_next(), if !inbound_tasks.is_empty() => {}
            }
        }
        escalation_tasks.abort_all();
        while escalation_tasks.join_next().await.is_some() {}
        self.escalating
            .lock()
            .expect("signaling escalation set poisoned")
            .clear();
        inbound_tasks.abort_all();
        while inbound_tasks.join_next().await.is_some() {}
        self.dispatch_chains.clear();
        tracing::info!("v0.20 signaling router stopped");
    }

    /// Dispatch one inbound message to the right controller by remote `ActrId`.
    pub(crate) async fn route_inbound(&self, from: ActrId, message: SignalingMessage) {
        match message {
            SignalingMessage::Offer { desc, exchange_id } => {
                // An offer from an unknown peer creates an answerer controller
                // (the manager is idempotent for a known peer — RFC §"SignalingRouter").
                self.manager
                    .handle_incoming_offer_correlated(from, *desc, exchange_id)
                    .await;
            }
            SignalingMessage::IceRestartOffer { desc, exchange_id } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle
                        .apply_remote_ice_restart_offer_correlated(*desc, Some(exchange_id.clone()))
                        .await;
                } else {
                    // Preserve the existing wire contract: a restart offer is
                    // valid only for the old active Session. Creating a fresh
                    // answerer implicitly would leave the offerer on its stale
                    // PC/DTLS/SCTP generation.
                    let sent = self.manager.try_send_signaling(
                        from.clone(),
                        SignalingMessage::IceRestartRejected {
                            exchange_id,
                            reason: actr_protocol::ice_restart_rejected::Reason::NoActiveSession
                                as i32,
                        },
                    );
                    if !sent {
                        tracing::warn!(
                            peer = %from.to_string_repr(),
                            "outbound signaling full; dropped ICE restart rejection"
                        );
                    }
                }
            }
            SignalingMessage::IceRestartRequest {
                request_id,
                attempt,
            } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.apply_ice_restart_request(request_id, attempt).await;
                } else {
                    // A same-identity remote endpoint may have restarted and
                    // therefore lost its controller before this request
                    // arrived. The request is the recovery wake in exactly
                    // that situation: bootstrap a fresh local controller and
                    // let server role arbitration recreate the Offerer side.
                    // Subsequent requests will find the controller and be
                    // handled by the normal attempt/identity reducer.
                    tracing::warn!(
                        peer = %from.to_string_repr(),
                        "bootstrapping controller for ICE restart request from unknown peer"
                    );
                    self.manager.connect_from_inbound_recovery(from).await;
                }
            }
            SignalingMessage::Answer { desc, exchange_id } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle
                        .apply_remote_description_correlated(*desc, exchange_id)
                        .await;
                } else {
                    tracing::warn!(
                        peer = %from.to_string_repr(),
                        "dropping Answer for unknown peer (no controller)"
                    );
                }
            }
            SignalingMessage::Candidate(candidate) => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.apply_remote_candidate(candidate).await;
                } else {
                    tracing::debug!(
                        peer = %from.to_string_repr(),
                        "dropping ICE candidate for unknown peer (no controller)"
                    );
                }
            }
            SignalingMessage::RoleAssignment { is_offerer } => {
                // Unsolicited assignment for a peer we have no controller for
                // still creates the passive-side controller, but creation is
                // committed under the router dispatch's manager lifecycle
                // epoch so a racing close-all can reject late publication.
                self.manager.handle_role_assignment(from, is_offerer).await;
            }
            SignalingMessage::RoleNegotiation => {
                // RoleNegotiation only travels outbound (to the server); the
                // router never receives it inbound.
                tracing::warn!(
                    peer = %from.to_string_repr(),
                    "ignoring unexpected inbound RoleNegotiation"
                );
            }
            SignalingMessage::IceRestartRejected {
                exchange_id,
                reason,
            } => {
                tracing::info!(
                    peer = %from.to_string_repr(),
                    %exchange_id,
                    reason,
                    "remote rejected ICE restart; escalating to Hard Recovery"
                );
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.force_reconnect().await;
                }
            }
        }
    }
}
