//! Integration harness for the webrtc-rs architecture.
//!
//! Every peer uses the same production boundaries as a node:
//! `PeerGate -> PeerTransport -> DefaultWireBuilder -> RtcConnectionManager ->
//! RtcPeerController -> PeerConnectionSession`. Signaling runs through the
//! production WebSocket adapter. The optional packet gate is injected at the
//! RTC runtime/socket layer, so network tests do not depend on the removed
//! 0.14 coordinator or callback adapter.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_protocol::prost::Message as _;
use actr_protocol::{
    AIdCredential, ActrError, ActrId, DataChunk, Direction, PayloadType, RpcEnvelope,
};
use async_trait::async_trait;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc, watch};

use super::{
    TestSignalingServer, VNetPair, create_credential_state_for_test, dummy_credential,
    install_test_crypto_provider, make_actor_id,
};
use crate::lifecycle::{
    DefaultNetworkEventProcessor, NetworkEvent, NetworkEventProcessor, NetworkRecoveryAction,
    NetworkRecoveryError, TeardownReport,
};
use crate::outbound::{PeerGate, PendingRequestsMap};
use crate::transport::{
    DefaultWireBuilder, DefaultWireBuilderConfig, PayloadTypeExt, PeerTransport,
};
use crate::wire::webrtc::rtc::address::{
    DesktopLocalAddressProvider, LocalAddressProvider, TestLocalAddressProvider,
};
use crate::wire::webrtc::rtc::factory::PeerConnectionFactory;
use crate::wire::webrtc::rtc::inbound::{InboundMessage, InboundRouter};
use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
use crate::wire::webrtc::rtc::recovery::RtcNetworkEvent;
use crate::wire::webrtc::rtc::runtime::RtcRuntimeConfig;
use crate::wire::webrtc::rtc::signaling::SharedLocalId;
use crate::wire::webrtc::rtc::signaling::router::SignalingRouter;
use crate::wire::webrtc::rtc::signaling::ws_transport::RtcSignalingTransport;
use crate::wire::webrtc::{SignalingClient, WebSocketSignalingClient};

/// Test-facing control plane over one node's RTC manager.
#[derive(Clone)]
pub struct TestRtcControl {
    manager: Arc<RtcConnectionManager>,
}

/// Actual controller role observed after signaling arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRtcRole {
    Undetermined,
    Offerer,
    Answerer,
}

#[derive(Debug, Clone)]
pub struct TestRtcPeerResourceObservation {
    pub state: actr_framework::WebRtcPeerState,
    pub role: TestRtcRole,
    pub ice_generation: u64,
    pub signaling_dropped: u64,
    pub mailbox_overflows: u64,
    pub live_lanes: usize,
    pub converged: bool,
}

#[derive(Debug, Clone)]
pub struct TestRtcResourceObservation {
    pub peer_count: usize,
    pub inbound_dropped_messages: u64,
    pub peers: Vec<TestRtcPeerResourceObservation>,
    pub recovery_obligations: usize,
}

#[derive(Debug, Clone)]
pub struct TestEvidenceEvent {
    pub observed_at: SystemTime,
    pub endpoint: Option<ActrId>,
    pub source: &'static str,
    pub peer: Option<ActrId>,
    pub pc_generation: Option<u64>,
    pub revision: Option<u64>,
    pub status: Option<actr_framework::WebRtcPeerStatus>,
    pub sendable: Option<bool>,
    pub operation: String,
    pub result: String,
}

/// Shared, append-only evidence stream for integration tests. It is an
/// observer only: recording never feeds a fact back into RTC state.
#[derive(Clone, Default)]
pub struct TestEvidenceTimeline {
    events: Arc<StdMutex<Vec<TestEvidenceEvent>>>,
}

impl TestEvidenceTimeline {
    pub fn record(
        &self,
        endpoint: Option<ActrId>,
        source: &'static str,
        peer: Option<ActrId>,
        operation: impl Into<String>,
        result: impl Into<String>,
    ) {
        self.events
            .lock()
            .expect("test evidence timeline lock poisoned")
            .push(TestEvidenceEvent {
                observed_at: SystemTime::now(),
                endpoint,
                source,
                peer,
                pc_generation: None,
                revision: None,
                status: None,
                sendable: None,
                operation: operation.into(),
                result: result.into(),
            });
    }

    pub fn record_state(
        &self,
        endpoint: ActrId,
        source: &'static str,
        state: &actr_framework::WebRtcPeerState,
        operation: impl Into<String>,
    ) {
        self.events
            .lock()
            .expect("test evidence timeline lock poisoned")
            .push(TestEvidenceEvent {
                observed_at: SystemTime::now(),
                endpoint: Some(endpoint),
                source,
                peer: Some(state.peer.clone()),
                pc_generation: Some(state.pc_generation),
                revision: Some(state.revision),
                status: Some(state.status),
                sendable: Some(state.sendable),
                operation: operation.into(),
                result: format!("reason={:?}", state.unavailable_reason),
            });
    }

    fn record_driver_event(
        &self,
        endpoint: ActrId,
        peer: ActrId,
        event: &crate::wire::webrtc::rtc::events::PeerDriverEvent,
    ) {
        self.events
            .lock()
            .expect("test evidence timeline lock poisoned")
            .push(TestEvidenceEvent {
                observed_at: SystemTime::now(),
                endpoint: Some(endpoint),
                source: "handler",
                peer: Some(peer),
                pc_generation: Some(event.generation()),
                revision: None,
                status: None,
                sendable: None,
                operation: format!("{event:?}"),
                result: "published to controller mailbox".to_string(),
            });
    }

    pub fn events(&self) -> Vec<TestEvidenceEvent> {
        self.events
            .lock()
            .expect("test evidence timeline lock poisoned")
            .clone()
    }

    pub fn render_with_signaling(
        &self,
        signaling: &crate::wire::webrtc::rtc::fault::DirectedSignalingFaultRegistry,
    ) -> String {
        let mut rows = self
            .events()
            .into_iter()
            .map(|event| {
                let timestamp = event
                    .observed_at
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros();
                (
                    timestamp,
                    format!(
                        "{timestamp} | endpoint={:?} | source={} | peer={:?} | generation={:?} | revision={:?} | status={:?} | sendable={:?} | operation={} | result={}",
                        event.endpoint,
                        event.source,
                        event.peer,
                        event.pc_generation,
                        event.revision,
                        event.status,
                        event.sendable,
                        event.operation,
                        event.result,
                    ),
                )
            })
            .collect::<Vec<_>>();
        rows.extend(signaling.trace().into_iter().map(|event| {
            let timestamp = event
                .observed_at
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros();
            (
                timestamp,
                format!(
                    "{timestamp} | endpoint={:?} | source=signaling | peer={:?} | generation=- | revision=- | status=- | sendable=- | operation={:?}/exchange={:?}/attempt={:?}/ordinal={} | fault={:?}",
                    event.from,
                    event.to,
                    event.class,
                    event.exchange_id,
                    event.attempt,
                    event.ordinal,
                    event.rule,
                ),
            )
        }));
        rows.sort_by_key(|(timestamp, _)| *timestamp);
        rows.into_iter()
            .map(|(_, row)| row)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Prints the merged evidence only during unwinding, keeping successful CI
/// logs compact while making every failed pair/multi-peer case auditable.
pub struct TestEvidenceFailureGuard {
    label: String,
    timeline: TestEvidenceTimeline,
    signaling: crate::wire::webrtc::rtc::fault::DirectedSignalingFaultRegistry,
}

impl TestEvidenceFailureGuard {
    pub fn new(
        label: impl Into<String>,
        timeline: TestEvidenceTimeline,
        signaling: crate::wire::webrtc::rtc::fault::DirectedSignalingFaultRegistry,
    ) -> Self {
        Self {
            label: label.into(),
            timeline,
            signaling,
        }
    }
}

impl Drop for TestEvidenceFailureGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "\n===== {} merged RTC evidence =====\n{}\n===== end RTC evidence =====",
                self.label,
                self.timeline.render_with_signaling(&self.signaling)
            );
        }
    }
}

/// Ordered evidence emitted by the production snapshot-to-hook projection.
#[derive(Clone, Default)]
pub struct TestRtcHookRecorder {
    events: Arc<std::sync::Mutex<Vec<crate::wire::webrtc::HookEvent>>>,
    changed: Arc<Notify>,
    evidence: Option<(ActrId, TestEvidenceTimeline)>,
}

impl TestRtcHookRecorder {
    fn with_evidence(endpoint: ActrId, evidence: TestEvidenceTimeline) -> Self {
        Self {
            events: Arc::default(),
            changed: Arc::default(),
            evidence: Some((endpoint, evidence)),
        }
    }

    fn callback(&self) -> crate::wire::webrtc::HookCallback {
        let events = self.events.clone();
        let changed = self.changed.clone();
        let evidence = self.evidence.clone();
        Arc::new(move |event| {
            let events = events.clone();
            let changed = changed.clone();
            let evidence = evidence.clone();
            Box::pin(async move {
                if let Some((endpoint, timeline)) = evidence {
                    match &event {
                        crate::wire::webrtc::HookEvent::WebRtcStateChanged { state } => {
                            timeline.record_state(
                                endpoint,
                                "hook",
                                state,
                                "on_webrtc_state_changed",
                            );
                        }
                        _ => timeline.record(
                            Some(endpoint),
                            "hook",
                            None,
                            format!("{event:?}"),
                            "emitted",
                        ),
                    }
                }
                events
                    .lock()
                    .expect("RTC hook recorder lock poisoned")
                    .push(event);
                changed.notify_one();
            })
        })
    }

    pub fn events(&self) -> Vec<crate::wire::webrtc::HookEvent> {
        self.events
            .lock()
            .expect("RTC hook recorder lock poisoned")
            .clone()
    }

    pub fn state_changes_for(&self, peer: &ActrId) -> Vec<actr_framework::WebRtcPeerState> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                crate::wire::webrtc::HookEvent::WebRtcStateChanged { state }
                    if &state.peer == peer =>
                {
                    Some(state)
                }
                _ => None,
            })
            .collect()
    }

    pub fn legacy_edges_for(&self, peer: &ActrId) -> Vec<&'static str> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                crate::wire::webrtc::HookEvent::WebRtcConnectStart { peer_id }
                    if &peer_id == peer =>
                {
                    Some("connecting")
                }
                crate::wire::webrtc::HookEvent::WebRtcConnected { peer_id, .. }
                    if &peer_id == peer =>
                {
                    Some("connected")
                }
                crate::wire::webrtc::HookEvent::WebRtcDisconnected { peer_id, .. }
                    if &peer_id == peer =>
                {
                    Some("disconnected")
                }
                _ => None,
            })
            .collect()
    }

    /// Wait for one legacy compatibility edge. Unified state and legacy hook
    /// callbacks are projected sequentially, so observing the authoritative
    /// state revision does not by itself prove that the following legacy
    /// callback has already completed.
    pub async fn wait_legacy_edge(
        &self,
        peer: &ActrId,
        expected: &'static str,
        timeout: Duration,
    ) -> bool {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                if self.legacy_edges_for(peer).contains(&expected) {
                    return;
                }
                self.changed.notified().await;
            }
        })
        .await
        .is_ok()
    }

    /// Wait until the production hook projection exposes a matching public
    /// state for one peer. This observes the same callback stream available to
    /// Swift/Kotlin workloads; it does not read controller internals.
    pub async fn wait_state<F>(
        &self,
        peer: &ActrId,
        timeout: Duration,
        predicate: F,
    ) -> Option<actr_framework::WebRtcPeerState>
    where
        F: Fn(&actr_framework::WebRtcPeerState) -> bool,
    {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                if let Some(state) = self
                    .state_changes_for(peer)
                    .into_iter()
                    .rev()
                    .find(&predicate)
                {
                    return state;
                }
                self.changed.notified().await;
            }
        })
        .await
        .ok()
    }
}

impl TestRtcControl {
    /// Observe and fault the manager of a fully started production Node.
    #[cfg(test)]
    pub(crate) fn for_node(node: &crate::ActrRef) -> Self {
        Self {
            manager: node
                .shared
                .rtc_manager
                .clone()
                .expect("RTC-enabled fixture node"),
        }
    }

    /// Hold the concrete channel so tests can prove reuse rather than merely
    /// comparing lane counts. This does not change readiness or controller state.
    #[cfg(test)]
    pub(crate) async fn lane_probe(&self, peer: &ActrId, payload: PayloadType) -> TestLaneProbe {
        let controller = self
            .manager
            .get_controller(peer)
            .await
            .expect("fixture controller");
        let generation = controller.snapshot().pc_generation;
        let lane = crate::wire::webrtc::rtc::status::LaneKind::from_payload_type(payload)
            .expect("DataChannel payload");
        let handle = controller
            .lane_handle(generation, lane)
            .expect("fixture lane");
        TestLaneProbe {
            channel: handle.channel().clone(),
        }
    }

    /// Close physical resources without removing their logical Controller,
    /// then deliver its terminal driver fact. This models S4 explicitly;
    /// it does not pretend the test merely changed a network interface.
    #[cfg(test)]
    pub(crate) async fn retire_physical_session(&self, peer: &ActrId) -> u64 {
        let handle = self
            .manager
            .get_controller(peer)
            .await
            .expect("fixture controller");
        let generation = handle.snapshot().pc_generation;
        handle.force_close_sessions().await;
        handle.inject_peer_connection_state_for_test(
            webrtc::peer_connection::RTCPeerConnectionState::Closed,
        );
        generation
    }

    fn new(manager: Arc<RtcConnectionManager>) -> Self {
        Self { manager }
    }

    /// Initiate the production offerer path.
    pub async fn connect(&self, peer: ActrId) {
        self.manager.connect(peer).await;
    }

    /// Latest authoritative public snapshot.
    pub async fn state(&self, peer: &ActrId) -> Option<actr_framework::WebRtcPeerState> {
        self.manager.get_snapshot(peer).await.map(|s| s.to_public())
    }

    /// Subscribe with immediate replay of the current authoritative state.
    pub async fn subscribe(
        &self,
        peer: &ActrId,
    ) -> Option<watch::Receiver<actr_framework::WebRtcPeerState>> {
        self.manager.subscribe_public(peer).await
    }

    /// Await `sendable=true` without polling once the controller exists.
    pub async fn wait_sendable(
        &self,
        peer: &ActrId,
        timeout: Duration,
    ) -> actr_framework::WebRtcPeerState {
        self.wait_state_diagnosed(peer, timeout, |state| state.sendable)
            .await
            .unwrap_or_else(|diagnostics| {
                panic!("peer {peer} did not become sendable within {timeout:?}: {diagnostics}")
            })
    }

    /// Await a snapshot satisfying a test predicate. The latest snapshot is
    /// evaluated immediately, so late subscribers do not miss a transition.
    pub async fn wait_state<F>(
        &self,
        peer: &ActrId,
        timeout: Duration,
        predicate: F,
    ) -> Option<actr_framework::WebRtcPeerState>
    where
        F: Fn(&actr_framework::WebRtcPeerState) -> bool,
    {
        self.wait_state_diagnosed(peer, timeout, predicate)
            .await
            .ok()
    }

    /// Like `wait_state`, but retain bounded observations for failure reports.
    /// Capture evidence inside the original deadline: diagnosing a timeout
    /// must not wait for another manager lock or a controller response.
    pub(crate) async fn wait_state_diagnosed<F>(
        &self,
        peer: &ActrId,
        timeout: Duration,
        predicate: F,
    ) -> Result<actr_framework::WebRtcPeerState, String>
    where
        F: Fn(&actr_framework::WebRtcPeerState) -> bool,
    {
        const OBSERVATION_LIMIT: usize = 8;
        let started = tokio::time::Instant::now();
        let mut observations = VecDeque::with_capacity(OBSERVATION_LIMIT);
        let mut phase = "looking up controller";
        let result =
            crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
                let mut peer_set = self.manager.subscribe_peers();
                loop {
                    phase = "looking up controller";
                    // Keep watching a closing controller's public state, as the
                    // pre-diagnostics `subscribe` path did. Only an active
                    // controller exposes the internal snapshot.
                    let (mut rx, controller) = match self.manager.get_controller(peer).await {
                        Some(controller) => (controller.subscribe_public(), Some(controller)),
                        None => match self.manager.subscribe_public(peer).await {
                            Some(rx) => (rx, None),
                            None => {
                                phase = "waiting for peer registry change";
                                peer_set
                                    .changed()
                                    .await
                                    .expect("manager peer registry watch closed");
                                continue;
                            }
                        },
                    };
                    loop {
                        let state = rx.borrow().clone();
                        if predicate(&state) {
                            return state;
                        }
                        if observations.len() == OBSERVATION_LIMIT {
                            observations.pop_front();
                        }
                        // The public predicate remains the oracle. The internal
                        // snapshot adds role, ICE generation and overflow counters;
                        // it may be newer, so report both revisions explicitly.
                        observations.push_back((
                            started.elapsed(),
                            state,
                            controller.as_ref().map(|c| c.snapshot()),
                        ));
                        phase = "waiting for public state change";
                        if rx.changed().await.is_err() {
                            break;
                        }
                    }
                    phase = "waiting for peer registry change";
                    peer_set
                        .changed()
                        .await
                        .expect("manager peer registry watch closed");
                }
            })
            .await;
        result.map_err(|_| {
            format!(
                "peer={peer}, budget={timeout:?}, phase={phase}, recent observations \
                 (elapsed, public state, controller snapshot; oldest first)={observations:#?}"
            )
        })
    }

    pub async fn generation(&self, peer: &ActrId) -> Option<u64> {
        self.state(peer).await.map(|state| state.pc_generation)
    }

    pub async fn ice_generation(&self, peer: &ActrId) -> Option<u64> {
        self.manager
            .get_snapshot(peer)
            .await
            .map(|state| state.ice_generation)
    }

    pub async fn wait_ice_generation(
        &self,
        peer: &ActrId,
        exclusive_minimum: u64,
        timeout: Duration,
    ) -> Option<u64> {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            let mut peer_set = self.manager.subscribe_peers();
            loop {
                if let Some(mut rx) = self.manager.subscribe(peer).await {
                    loop {
                        let state = rx.borrow().clone();
                        if state.ice_generation > exclusive_minimum {
                            return state.ice_generation;
                        }
                        if rx.changed().await.is_err() {
                            break;
                        }
                    }
                }
                peer_set
                    .changed()
                    .await
                    .expect("manager peer registry watch closed");
            }
        })
        .await
        .ok()
    }

    pub async fn is_sendable(&self, peer: &ActrId) -> bool {
        self.state(peer).await.is_some_and(|state| state.sendable)
    }

    pub async fn role(&self, peer: &ActrId) -> Option<TestRtcRole> {
        let role = self.manager.get_snapshot(peer).await?.role;
        Some(match role {
            crate::wire::webrtc::rtc::peer::state::Role::Undetermined => TestRtcRole::Undetermined,
            crate::wire::webrtc::rtc::peer::state::Role::Offerer => TestRtcRole::Offerer,
            crate::wire::webrtc::rtc::peer::state::Role::Answerer => TestRtcRole::Answerer,
        })
    }

    pub fn network_unavailable(&self) -> usize {
        self.publish(RtcNetworkEvent::NetworkUnavailable)
    }

    pub fn network_available(&self) -> usize {
        self.publish(RtcNetworkEvent::NetworkAvailable)
    }

    pub fn local_address_changed(&self) -> usize {
        self.publish(RtcNetworkEvent::LocalAddressChanged)
    }

    pub fn force_reconnect(&self) -> usize {
        self.publish(RtcNetworkEvent::ForceReconnect)
    }

    /// Inject a RTC `PeerConnectionStateChanged(Failed)` fact through the
    /// controller's real event-handler mailbox.
    pub async fn fail_peer(&self, peer: &ActrId) -> Result<u64, String> {
        let handle = self
            .manager
            .get_controller(peer)
            .await
            .ok_or_else(|| "peer controller not found".to_string())?;
        let generation = handle.snapshot().pc_generation;
        handle.inject_peer_connection_state_for_test(
            webrtc::peer_connection::RTCPeerConnectionState::Failed,
        );
        Ok(generation)
    }

    /// Publish an authoritative network/app fact to every peer controller.
    pub(crate) fn publish(&self, event: RtcNetworkEvent) -> usize {
        self.manager.supervisor().publish(event)
    }

    pub async fn close_peer(&self, peer: &ActrId) -> bool {
        self.manager.close_peer(peer).await
    }

    pub async fn refresh_configuration(
        &self,
        peer: &ActrId,
        configuration: webrtc::peer_connection::RTCConfiguration,
    ) -> Result<(), String> {
        let handle = self
            .manager
            .get_controller(peer)
            .await
            .ok_or_else(|| "peer controller not found".to_string())?;
        handle
            .refresh_configuration(configuration)
            .await
            .map_err(|error| error.to_string())
    }

    /// Close one concrete DataChannel through the generation-owned lane
    /// registry.  This test-only seam exercises the real RTC `close()` ->
    /// `DataChannel::poll()` -> Controller recovery path without injecting a
    /// synthetic callback.
    pub async fn close_lane(
        &self,
        peer: &ActrId,
        payload_type: PayloadType,
    ) -> Result<u64, String> {
        let lane = crate::wire::webrtc::rtc::status::LaneKind::from_payload_type(payload_type)
            .ok_or_else(|| format!("{payload_type:?} is not a DataChannel lane"))?;
        let handle = self
            .manager
            .get_controller(peer)
            .await
            .ok_or_else(|| "peer controller not found".to_string())?;
        let generation = handle.snapshot().pc_generation;
        let lane_handle = handle
            .lane_handle(generation, lane)
            .ok_or_else(|| format!("lane {lane:?} is not active on generation {generation}"))?;
        lane_handle
            .channel()
            .close()
            .await
            .map_err(|error| error.to_string())?;
        Ok(generation)
    }

    pub async fn close_all(&self) {
        self.manager.close_all().await;
    }

    /// Deterministically pause a locally initiated controller creation at its
    /// pre-publication boundary. Used to prove close-all linearization without
    /// relying on scheduler timing or delayed signaling.
    pub fn set_peer_publish_barrier(&self, peer: ActrId, barrier: Arc<tokio::sync::Barrier>) {
        self.manager
            .set_peer_publish_barrier_for_test(peer, barrier);
    }

    pub async fn peer_count(&self) -> usize {
        self.manager.peer_ids().await.len()
    }

    /// Number of open manager-owned peer recovery obligations. The ledger is
    /// the authoritative hand-off record used by the v0.20 recovery contract;
    /// exposing only the count keeps this test seam read-only and bounded.
    pub fn recovery_obligation_count(&self) -> usize {
        self.manager.recovery_obligation_log().open_count()
    }

    /// Await a manager registry size through its production watch edge.
    pub async fn wait_peer_count(&self, expected: usize, timeout: Duration) -> bool {
        let mut changed = self.manager.subscribe_peers();
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                if self.manager.peer_ids().await.len() == expected {
                    return true;
                }
                if changed.changed().await.is_err() {
                    return false;
                }
            }
        })
        .await
        .unwrap_or(false)
    }

    /// Unified public-state/resource/teardown observation from the production
    /// manager.
    pub async fn observation(&self) -> TestRtcResourceObservation {
        let observation = self.manager.observe().await;
        TestRtcResourceObservation {
            peer_count: observation.peer_count,
            inbound_dropped_messages: observation.inbound_dropped_messages,
            peers: observation
                .peers
                .into_iter()
                .map(|peer| {
                    let role = match peer.snapshot.role {
                        crate::wire::webrtc::rtc::peer::state::Role::Undetermined => {
                            TestRtcRole::Undetermined
                        }
                        crate::wire::webrtc::rtc::peer::state::Role::Offerer => {
                            TestRtcRole::Offerer
                        }
                        crate::wire::webrtc::rtc::peer::state::Role::Answerer => {
                            TestRtcRole::Answerer
                        }
                    };
                    TestRtcPeerResourceObservation {
                        state: peer.snapshot.to_public(),
                        role,
                        ice_generation: peer.snapshot.ice_generation,
                        signaling_dropped: peer.snapshot.signaling_dropped,
                        mailbox_overflows: peer.snapshot.mailbox_overflows,
                        live_lanes: peer.live_lanes,
                        converged: peer.converged,
                    }
                })
                .collect(),
            recovery_obligations: self.recovery_obligation_count(),
        }
    }

    pub(crate) fn recovery_supervisor(
        &self,
    ) -> Arc<crate::wire::webrtc::rtc::recovery::RtcRecoverySupervisor> {
        self.manager.supervisor().clone()
    }
}

/// A physical-channel observation token; holding it never grants sendability.
#[cfg(test)]
pub(crate) struct TestLaneProbe {
    channel: Arc<dyn crate::wire::webrtc::rtc::peer::adapter::LaneDataChannel>,
}

#[cfg(test)]
impl TestLaneProbe {
    pub(crate) fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.channel, &other.channel)
    }

    pub(crate) async fn is_open(&self) -> bool {
        matches!(
            self.channel.ready_state().await,
            Ok(webrtc::data_channel::RTCDataChannelState::Open)
        )
    }
}

/// Network-event processor used by the harness. Signaling recovery is
/// delegated to the production processor; RTC recovery is broadcast as facts
/// and each `RtcPeerController` independently chooses soft/hard recovery.
pub struct RtcAwareNetworkEventProcessor {
    inner: Arc<DefaultNetworkEventProcessor>,
    rtc: TestRtcControl,
}

impl RtcAwareNetworkEventProcessor {
    fn new(
        signaling: Arc<dyn SignalingClient>,
        peer_transport: Arc<PeerTransport>,
        rtc: TestRtcControl,
    ) -> Self {
        Self {
            inner: Arc::new(DefaultNetworkEventProcessor::new_with_peer_transport(
                signaling,
                Some(peer_transport),
            )),
            rtc,
        }
    }

    fn publish_action(&self, action: NetworkRecoveryAction) {
        match action {
            NetworkRecoveryAction::Noop | NetworkRecoveryAction::Probe => {}
            NetworkRecoveryAction::Offline => {
                self.rtc.publish(RtcNetworkEvent::NetworkUnavailable);
            }
            NetworkRecoveryAction::Restore => {
                self.rtc.publish(RtcNetworkEvent::NetworkAvailable);
            }
            NetworkRecoveryAction::CleanupOnly | NetworkRecoveryAction::ForceReconnect => {
                self.rtc.publish(RtcNetworkEvent::ForceReconnect);
            }
        }
    }
}

#[async_trait]
impl NetworkEventProcessor for RtcAwareNetworkEventProcessor {
    fn prepare_network_event(&self, event: &NetworkEvent) {
        self.inner.prepare_network_event(event);
    }

    fn invalidate_signaling_connection_attempts(&self) {
        self.inner.invalidate_signaling_connection_attempts();
    }

    async fn process_network_available(&self) -> Result<(), String> {
        let result = self.inner.process_network_available().await;
        if result.is_ok() {
            self.rtc.publish(RtcNetworkEvent::NetworkAvailable);
        }
        result
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        self.rtc.publish(RtcNetworkEvent::NetworkUnavailable);
        self.inner.process_network_lost().await
    }

    async fn process_network_type_changed(
        &self,
        is_wifi: bool,
        is_cellular: bool,
    ) -> Result<(), String> {
        let result = self
            .inner
            .process_network_type_changed(is_wifi, is_cellular)
            .await;
        if result.is_ok() {
            self.rtc.publish(RtcNetworkEvent::LocalAddressChanged);
        }
        result
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        self.rtc.close_all().await;
        self.inner.cleanup_connections().await
    }

    async fn probe_connectivity(&self) -> Result<(), String> {
        self.inner.probe_connectivity().await
    }

    async fn force_reconnect(&self) -> Result<(), String> {
        self.rtc.publish(RtcNetworkEvent::ForceReconnect);
        self.inner.force_reconnect().await
    }

    async fn process_network_recovery_action(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        let result = self.inner.process_network_recovery_action(action).await;
        if result.is_ok() {
            self.publish_action(action);
        }
        result
    }

    async fn process_network_recovery_effect(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), NetworkRecoveryError> {
        let result = self.inner.process_network_recovery_effect(action).await;
        if result.is_ok() {
            self.publish_action(action);
        }
        result
    }

    async fn run_bounded_teardown(
        &self,
        action: NetworkRecoveryAction,
        budget: Duration,
    ) -> TeardownReport {
        self.publish_action(action);
        self.rtc.close_all().await;
        self.inner.run_bounded_teardown(action, budget).await
    }
}

/// A single node in an integration topology.
#[derive(Default)]
struct DataChunkStreamObservation {
    count: usize,
    latest_payload: Option<bytes::Bytes>,
}

#[derive(Default)]
struct DataChunkRecorder {
    streams: StdMutex<HashMap<String, DataChunkStreamObservation>>,
    changed: Notify,
}

impl DataChunkRecorder {
    fn record(&self, stream_id: String, payload: bytes::Bytes) {
        let mut streams = self
            .streams
            .lock()
            .expect("test DataChunk recorder lock poisoned");
        let observation = streams.entry(stream_id).or_default();
        observation.count += 1;
        observation.latest_payload = Some(payload);
        drop(streams);
        self.changed.notify_waiters();
    }

    fn snapshot(&self, stream_id: &str) -> (usize, Option<bytes::Bytes>) {
        self.streams
            .lock()
            .expect("test DataChunk recorder lock poisoned")
            .get(stream_id)
            .map(|observation| (observation.count, observation.latest_payload.clone()))
            .unwrap_or_default()
    }
}

/// Options for adding a peer to [`TestHarness`]. This keeps identity,
/// credentials, RTC configuration, and media observation independent without
/// adding another combination-specific `add_peer_*` method.
#[derive(Default)]
pub struct TestPeerOptions {
    serial: u64,
    identity: Option<ActrId>,
    rtc_configuration: Option<webrtc::peer_connection::RTCConfiguration>,
    credential: Option<AIdCredential>,
    media_registry: Option<Arc<crate::inbound::MediaFrameRegistry>>,
}

impl TestPeerOptions {
    pub fn new(serial: u64) -> Self {
        Self {
            serial,
            ..Self::default()
        }
    }

    pub fn with_identity(mut self, identity: ActrId) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn with_rtc_configuration(
        mut self,
        rtc_configuration: webrtc::peer_connection::RTCConfiguration,
    ) -> Self {
        self.rtc_configuration = Some(rtc_configuration);
        self
    }

    pub fn with_credential(mut self, credential: AIdCredential) -> Self {
        self.credential = Some(credential);
        self
    }

    pub fn with_media_registry(
        mut self,
        media_registry: Arc<crate::inbound::MediaFrameRegistry>,
    ) -> Self {
        self.media_registry = Some(media_registry);
        self
    }
}

pub struct TestPeer {
    pub id: ActrId,
    pub signaling_client: Arc<dyn SignalingClient>,
    pub gate: Arc<PeerGate>,
    pub transport_manager: Arc<PeerTransport>,
    pub rtc: TestRtcControl,
    pub hooks: TestRtcHookRecorder,
    credential: AIdCredential,
    request_counts: Arc<StdMutex<HashMap<String, usize>>>,
    request_changed: Arc<Notify>,
    data_chunks: Arc<DataChunkRecorder>,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<(InboundMessage, OwnedSemaphorePermit)>>,
    router_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    hook_shutdown: tokio_util::sync::CancellationToken,
    hook_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl TestPeer {
    /// Build a test-only `RuntimeContext` whose peer route uses this endpoint's
    /// production `PeerGate`. This lets external integration harnesses exercise
    /// `RuntimeContext -> PeerGate -> RTC` while the fixture owns signaling,
    /// inbound dispatch, and teardown. The context has its own data/media
    /// registries: receive-side callbacks registered through it are not wired
    /// to the fixture's inbound dispatcher. Observe inbound DataChunks through
    /// `data_chunk_count` / `data_chunk_observation` instead. The fixture does not
    /// run an in-process host dispatcher, so this context is intended for Peer
    /// destinations; Host/Workload calls require a separately serviced
    /// `HostTransport`.
    pub fn runtime_context_for_peer_calls(
        &self,
        request_id: impl Into<String>,
    ) -> crate::context::RuntimeContext {
        use crate::outbound::Gate;

        let peer_gate = Gate::Peer(self.gate.clone());
        let inproc_gate = Gate::Host(Arc::new(crate::outbound::HostGate::new(Arc::new(
            crate::transport::HostTransport::new(),
        ))));
        crate::context::RuntimeContext::new(
            self.id.clone(),
            None,
            request_id.into(),
            inproc_gate,
            Some(peer_gate),
            Arc::new(crate::inbound::DataChunkRegistry::new()),
            Arc::new(crate::inbound::MediaFrameRegistry::new()),
            self.signaling_client.clone(),
            self.credential.clone(),
            None,
            Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            None,
            0,
        )
    }

    pub async fn pending_count(&self) -> usize {
        self.gate.pending_count().await
    }

    pub fn request_count(&self, request_id: &str) -> usize {
        self.request_counts
            .lock()
            .expect("test request recorder lock poisoned")
            .get(request_id)
            .copied()
            .unwrap_or(0)
    }

    /// Wait for a request to cross the real RTC inbound lane and reach the
    /// remote business dispatcher. This replaces scheduler polling in in-flight
    /// tests and also supplies a duplicate-execution oracle.
    pub async fn wait_request_count(
        &self,
        request_id: &str,
        expected: usize,
        timeout: Duration,
    ) -> bool {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                let notified = self.request_changed.notified();
                let mut notified = std::pin::pin!(notified);
                notified.as_mut().enable();
                if self.request_count(request_id) >= expected {
                    return true;
                }
                notified.await;
            }
        })
        .await
        .is_ok()
    }

    pub fn data_chunk_count(&self, stream_id: &str) -> usize {
        self.data_chunks.snapshot(stream_id).0
    }

    /// Return an atomic count/payload snapshot for one observed stream.
    pub fn data_chunk_observation(&self, stream_id: &str) -> (usize, Option<bytes::Bytes>) {
        self.data_chunks.snapshot(stream_id)
    }

    /// Wait for an inbound stream to reach the requested delivery count.
    pub async fn wait_data_chunk_count(
        &self,
        stream_id: &str,
        expected: usize,
        timeout: Duration,
    ) -> bool {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                let notified = self.data_chunks.changed.notified();
                let mut notified = std::pin::pin!(notified);
                notified.as_mut().enable();
                if self.data_chunk_count(stream_id) >= expected {
                    return true;
                }
                notified.await;
            }
        })
        .await
        .is_ok()
    }

    pub fn network_processor(&self) -> Arc<RtcAwareNetworkEventProcessor> {
        Arc::new(RtcAwareNetworkEventProcessor::new(
            self.signaling_client.clone(),
            self.transport_manager.clone(),
            self.rtc.clone(),
        ))
    }

    /// Production signaling/transport effects without any test-only RTC fact
    /// publication. Pair this processor with
    /// `spawn_gated_rtc_network_event_supervisor`; using the RTC-aware
    /// compatibility processor there would publish every accepted fact twice.
    pub fn production_network_processor(&self) -> Arc<dyn NetworkEventProcessor> {
        Arc::new(DefaultNetworkEventProcessor::new_with_peer_transport(
            self.signaling_client.clone(),
            Some(self.transport_manager.clone()),
        ))
    }

    pub fn spawn_request(
        &self,
        target_serial: u64,
        request_id: &str,
        timeout_ms: u32,
    ) -> tokio::task::JoinHandle<actr_protocol::ActorResult<actr_framework::Bytes>> {
        self.spawn_request_with_route(target_serial, request_id, "test.method", timeout_ms)
    }

    pub fn spawn_request_with_route(
        &self,
        target_serial: u64,
        request_id: &str,
        route_key: &str,
        timeout_ms: u32,
    ) -> tokio::task::JoinHandle<actr_protocol::ActorResult<actr_framework::Bytes>> {
        let gate = self.gate.clone();
        let target = make_actor_id(target_serial);
        let envelope = RpcEnvelope {
            request_id: request_id.to_string(),
            route_key: route_key.to_string(),
            payload: Some(bytes::Bytes::from_static(b"test_payload")),
            timeout_ms: timeout_ms as i64,
            ..Default::default()
        };
        tokio::spawn(async move { gate.send_request(&target, envelope).await })
    }

    /// The harness dispatcher is always active; this handle preserves the
    /// former call pattern while avoiding multiple consumers of one inbound
    /// stream.
    pub fn start_echo_responder(&self, _name: &str) -> tokio::task::JoinHandle<()> {
        tokio::spawn(std::future::pending())
    }

    pub fn start_response_receiver(&self, _name: &str) -> tokio::task::JoinHandle<()> {
        tokio::spawn(std::future::pending())
    }

    pub fn start_rpc_dispatcher(&self, _name: &str) -> tokio::task::JoinHandle<()> {
        tokio::spawn(std::future::pending())
    }

    /// Receive the next non-RPC inbound payload recorded by the RTC router.
    ///
    /// The raw payload queue is best-effort, bounded to 512 messages and
    /// 32 MiB independently of the production ingress budget. Unread payloads
    /// are dropped at either limit so observation cannot prevent RPC processing.
    pub async fn receive_payload(&self) -> Option<(ActrId, PayloadType, bytes::Bytes)> {
        let (msg, _lease) = self.inbound_rx.lock().await.recv().await?;
        Some((msg.peer, msg.lane.to_payload_type(), msg.payload))
    }

    pub async fn restart_ice(&self, _target_serial: u64) -> anyhow::Result<()> {
        self.rtc.publish(RtcNetworkEvent::NetworkUnavailable);
        self.rtc.publish(RtcNetworkEvent::NetworkAvailable);
        Ok(())
    }

    pub async fn retry_failed(&self) {
        self.rtc.publish(RtcNetworkEvent::NetworkAvailable);
    }

    async fn shutdown(&self) {
        // Let the production projection observe controller teardown before its
        // callback task is cancelled; mobile consumers rely on the final
        // non-sendable revision just as much as on the ready revision.
        self.rtc.close_all().await;
        self.hook_shutdown.cancel();
        if let Some(task) = self.hook_task.lock().await.take() {
            let _ = task.await;
        }
        let _ = self.transport_manager.close_all().await;
        let _ = self.signaling_client.disconnect().await;
        if let Some(task) = self.router_task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

/// Multi-peer harness using real WebSocket signaling and real RTC peer
/// connections. With `with_vnet`, UDP loss is injected below the builder-owned
/// sockets through the RTC runtime.
pub struct TestHarness {
    pub server: TestSignalingServer,
    pub vnet: Option<VNetPair>,
    pub rtc_faults: crate::wire::webrtc::test_runtime::DirectedRtcFaultRegistry,
    pub signaling_faults: crate::wire::webrtc::rtc::fault::DirectedSignalingFaultRegistry,
    pub evidence: TestEvidenceTimeline,
    // The production-Actrix pair/multi-peer matrix needs the directed RTC
    // fault and observation seams. Keep them out of the legacy VNet harness so
    // its historical recovery cases retain their original Runtime, signaling,
    // address-provider, and scheduling path.
    rtc_matrix_instrumentation: bool,
    wildcard_rebind_addresses: bool,
    address_providers: HashMap<u64, Arc<TestLocalAddressProvider>>,
    peers: HashMap<u64, TestPeer>,
    bg_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl TestHarness {
    fn init_tracing() {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init();
    }

    pub async fn new() -> Self {
        Self::init_tracing();
        install_test_crypto_provider();
        Self {
            server: TestSignalingServer::start()
                .await
                .expect("failed to start test signaling server"),
            vnet: None,
            rtc_faults: Default::default(),
            signaling_faults: Default::default(),
            evidence: Default::default(),
            rtc_matrix_instrumentation: false,
            wildcard_rebind_addresses: false,
            address_providers: HashMap::new(),
            peers: HashMap::new(),
            bg_tasks: Vec::new(),
        }
    }

    pub async fn with_vnet() -> Self {
        Self::init_tracing();
        install_test_crypto_provider();
        Self {
            server: TestSignalingServer::start()
                .await
                .expect("failed to start test signaling server"),
            vnet: Some(VNetPair::new().await.expect("failed to create VNet pair")),
            rtc_faults: Default::default(),
            signaling_faults: Default::default(),
            evidence: Default::default(),
            rtc_matrix_instrumentation: false,
            wildcard_rebind_addresses: false,
            address_providers: HashMap::new(),
            peers: HashMap::new(),
            bg_tasks: Vec::new(),
        }
    }

    /// Build a harness against an externally owned signaling service. This is
    /// the entry used by `MobileRtcPairHarness` with real Actrix Signaling;
    /// no compatibility mock server is started.
    pub async fn with_external_signaling(signaling_url: impl Into<String>) -> Self {
        Self::init_tracing();
        install_test_crypto_provider();
        Self {
            server: TestSignalingServer::external(signaling_url),
            vnet: None,
            rtc_faults: Default::default(),
            signaling_faults: Default::default(),
            evidence: Default::default(),
            rtc_matrix_instrumentation: true,
            wildcard_rebind_addresses: false,
            address_providers: HashMap::new(),
            peers: HashMap::new(),
            bg_tasks: Vec::new(),
        }
    }

    /// External-signaling variant with the compatibility packet gate. New
    /// direction-aware scenarios should prefer `MobileRtcPairHarness` faults.
    pub async fn with_external_signaling_and_vnet(signaling_url: impl Into<String>) -> Self {
        Self::init_tracing();
        install_test_crypto_provider();
        Self {
            server: TestSignalingServer::external(signaling_url),
            vnet: Some(VNetPair::new().await.expect("failed to create VNet pair")),
            rtc_faults: Default::default(),
            signaling_faults: Default::default(),
            evidence: Default::default(),
            rtc_matrix_instrumentation: true,
            wildcard_rebind_addresses: false,
            address_providers: HashMap::new(),
            peers: HashMap::new(),
            bg_tasks: Vec::new(),
        }
    }

    /// External-signaling RTC fixture that switches the test provider to
    /// wildcard bind inputs after its first synthetic address change. This
    /// keeps the initial pair deterministic while exercising webrtc-rs's live
    /// interface expansion on an ICE-restart rebind.
    pub async fn with_external_signaling_and_wildcard_rebind(
        signaling_url: impl Into<String>,
    ) -> Self {
        let mut harness = Self::with_external_signaling(signaling_url).await;
        harness.wildcard_rebind_addresses = true;
        harness
    }

    pub async fn add_peer(&mut self, serial: u64) {
        self.add_peer_with_options(TestPeerOptions::new(serial))
            .await;
    }

    pub async fn add_peer_with_rtc_configuration(
        &mut self,
        serial: u64,
        rtc_configuration: webrtc::peer_connection::RTCConfiguration,
    ) {
        self.add_peer_with_options(
            TestPeerOptions::new(serial).with_rtc_configuration(rtc_configuration),
        )
        .await;
    }

    /// Add a peer authenticated by an externally owned production signaling
    /// fixture. Mobile pair tests use Actrix-issued-equivalent credentials
    /// instead of the compatibility dummy token.
    pub async fn add_peer_with_rtc_configuration_and_credential(
        &mut self,
        serial: u64,
        rtc_configuration: webrtc::peer_connection::RTCConfiguration,
        credential: AIdCredential,
    ) {
        self.add_peer_with_options(
            TestPeerOptions::new(serial)
                .with_rtc_configuration(rtc_configuration)
                .with_credential(credential),
        )
        .await;
    }

    /// Add a peer whose production RTC sessions dispatch remote media into
    /// the supplied node-level registry. This is the T3 media fixture: it uses
    /// the same manager, controller, effect runner, track poll pump, and
    /// `MediaFrameRegistry` wiring as a real node.
    pub async fn add_peer_with_media_registry(
        &mut self,
        serial: u64,
        media_registry: Arc<crate::inbound::MediaFrameRegistry>,
    ) {
        self.add_peer_with_options(
            TestPeerOptions::new(serial).with_media_registry(media_registry),
        )
        .await;
    }

    /// Add a peer with an explicit combination of identity, transport, and
    /// observation options. Existing convenience methods remain available.
    pub async fn add_peer_with_options(&mut self, options: TestPeerOptions) {
        let serial = options.serial;
        let identity = options.identity.unwrap_or_else(|| make_actor_id(serial));
        let rtc_configuration = options
            .rtc_configuration
            .unwrap_or_else(|| webrtc::peer_connection::RTCConfigurationBuilder::default().build());
        let credential = options.credential.unwrap_or_else(dummy_credential);
        self.add_peer_with_options_and_identity(
            serial,
            identity,
            rtc_configuration,
            credential,
            options.media_registry,
        )
        .await;
    }

    async fn add_peer_with_options_and_identity(
        &mut self,
        serial: u64,
        id: ActrId,
        rtc_configuration: webrtc::peer_connection::RTCConfiguration,
        credential: AIdCredential,
        media_registry: Option<Arc<crate::inbound::MediaFrameRegistry>>,
    ) {
        assert!(!self.peers.contains_key(&serial), "duplicate peer {serial}");
        assert_eq!(
            id.serial_number, serial,
            "test peer serial must match the supplied ActrId"
        );
        let initial_addresses = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];
        let mutable_address_provider = if self.wildcard_rebind_addresses {
            Arc::new(TestLocalAddressProvider::new_with_wildcard_rebind(
                initial_addresses,
            ))
        } else {
            Arc::new(TestLocalAddressProvider::new(initial_addresses))
        };
        let address_provider: Arc<dyn LocalAddressProvider> = if self.rtc_matrix_instrumentation {
            mutable_address_provider.clone()
        } else {
            Arc::new(DesktopLocalAddressProvider::with_override(vec![
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            ]))
        };
        let credential_state = create_credential_state_for_test(credential.clone());
        let signaling = WebSocketSignalingClient::connect_to_with_identity(
            &self.server.url(),
            id.clone(),
            credential_state.clone(),
        )
        .await
        .expect("failed to connect test signaling client");
        let signaling_client: Arc<dyn SignalingClient> = signaling;
        let shared_local_id = SharedLocalId::new(id.clone());
        let faults = crate::wire::webrtc::test_runtime::TestFaultRegistries {
            rtc: self.rtc_faults.clone(),
            signaling: self.signaling_faults.clone(),
        };

        let mut runtime = RtcRuntimeConfig::desktop_default();
        if let Some(vnet) = &self.vnet {
            runtime = runtime.with_runtime_override(
                crate::wire::webrtc::test_runtime::packet_filter_runtime(
                    vnet.packet_blocked_flag(),
                ),
            );
        }
        let factory = PeerConnectionFactory::new_with_configuration(
            runtime,
            address_provider,
            rtc_configuration,
        );
        let factory = if self.rtc_matrix_instrumentation {
            faults.wrap_factory(factory, shared_local_id.clone())
        } else {
            factory
        };
        let factory = Arc::new(factory);

        let (inbound_tx, mut dispatcher_rx) = mpsc::channel(512);
        let (raw_tx, raw_rx) = mpsc::channel(512);
        let raw_bytes = Arc::new(Semaphore::new(32 * 1024 * 1024));
        let router = Arc::new(InboundRouter::new(Arc::new(move |message, lease| {
            if inbound_tx.try_send((message, lease)).is_err() {
                tracing::warn!("test inbound dispatcher is full; dropping message");
            }
        })));
        let (signaling_tx, signaling_rx) = mpsc::channel(128);
        let manager = RtcConnectionManager::with_inbound(factory, signaling_tx, router);
        let manager = if self.rtc_matrix_instrumentation {
            let local_id = id.clone();
            let handler_evidence = self.evidence.clone();
            manager.with_driver_event_observer_for_test(Arc::new(move |peer, event| {
                handler_evidence.record_driver_event(local_id.clone(), peer.clone(), event);
            }))
        } else {
            manager
        };
        let manager = Arc::new(match media_registry {
            Some(registry) => manager.with_media_registry(registry),
            None => manager,
        });
        // This standalone harness has no lifecycle supervisor, so commit a
        // synthetic signaling generation and open the RTC recovery gate
        // (dea7ff08 convention, shared with the interop spawners). Tests
        // that exercise supervisor-owned gating spawn their own reconciler
        // (see `spawn_gated_rtc_network_event_supervisor` / the
        // `recovery_vnet_supervisor` wiring); its first projection overrides
        // this synthetic value.
        manager.supervisor().set_signaling_generation(Some(0));
        let hook_shutdown = tokio_util::sync::CancellationToken::new();
        let (hooks, hook_task) = if self.rtc_matrix_instrumentation {
            let hooks = TestRtcHookRecorder::with_evidence(id.clone(), self.evidence.clone());
            let task = tokio::spawn(
                crate::wire::webrtc::rtc::notification::run_recording_hook_projection(
                    manager.clone(),
                    hooks.callback(),
                    hook_shutdown.clone(),
                ),
            );
            (hooks, Some(task))
        } else {
            (TestRtcHookRecorder::default(), None)
        };
        let signaling_router = Arc::new(SignalingRouter::new(
            manager.clone(),
            shared_local_id.clone(),
        ));
        manager.set_signaling_dispatch_chains(signaling_router.dispatch_chains());
        let ws_transport = RtcSignalingTransport::new(
            signaling_client.clone(),
            shared_local_id.clone(),
            credential_state,
        );
        let router_for_task = signaling_router.clone();
        let test_faults = self.rtc_matrix_instrumentation.then_some(faults);
        let router_task = tokio::spawn(async move {
            if let Some(faults) = test_faults {
                let transport = faults.wrap_signaling(ws_transport, shared_local_id);
                router_for_task.run(transport, signaling_rx).await;
                return;
            }
            router_for_task.run(ws_transport, signaling_rx).await;
        });

        let pending: PendingRequestsMap =
            std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
        let wire_config = DefaultWireBuilderConfig {
            enable_websocket: false,
            pending_requests: Some(pending.clone()),
            ..Default::default()
        };
        let wire_builder = DefaultWireBuilder::new(wire_config).with_rtc_manager(manager.clone());
        let transport_manager = Arc::new(PeerTransport::new(id.clone(), Arc::new(wire_builder)));
        let gate = Arc::new(PeerGate::with_rtc_manager(
            transport_manager.clone(),
            manager.clone(),
            pending,
            None,
        ));

        let dispatcher_gate = gate.clone();
        let request_counts = Arc::new(StdMutex::new(HashMap::<String, usize>::new()));
        let request_counts_for_dispatcher = request_counts.clone();
        let request_changed = Arc::new(Notify::new());
        let request_changed_for_dispatcher = request_changed.clone();
        let data_chunks = Arc::new(DataChunkRecorder::default());
        let data_chunks_for_dispatcher = data_chunks.clone();
        let dispatcher_task = tokio::spawn(async move {
            while let Some((message, lease)) = dispatcher_rx.recv().await {
                if !message.lane.to_payload_type().is_rpc() {
                    if message.lane.to_payload_type() == PayloadType::StreamReliable
                        && let Ok(chunk) = DataChunk::decode(message.payload.as_ref())
                    {
                        data_chunks_for_dispatcher.record(chunk.stream_id, chunk.payload);
                    }
                    // The passive recorder has finished dispatching. Raw
                    // observation owns a separate byte permit, never the
                    // production lease needed to admit later chunks and RPCs.
                    drop(lease);
                    let Some(raw_lease) = u32::try_from(message.payload.len())
                        .ok()
                        .and_then(|bytes| raw_bytes.clone().try_acquire_many_owned(bytes).ok())
                    else {
                        tracing::debug!(
                            "test raw payload byte budget is full; dropping unread payload"
                        );
                        continue;
                    };
                    if raw_tx.try_send((message, raw_lease)).is_err() {
                        tracing::debug!(
                            "test raw payload queue is full or closed; dropping unread payload"
                        );
                    }
                    continue;
                }
                let _lease = lease;
                let envelope = match RpcEnvelope::decode(message.payload.as_ref()) {
                    Ok(envelope) => envelope,
                    Err(error) => {
                        tracing::warn!(%error, "test dispatcher dropped invalid RPC envelope");
                        continue;
                    }
                };
                match envelope.direction.and_then(|v| Direction::try_from(v).ok()) {
                    Some(Direction::Response) => {
                        let result = match (envelope.payload, envelope.error) {
                            (Some(payload), None) => Ok(payload),
                            (_, Some(error)) => Err(ActrError::Unavailable(format!(
                                "RPC error {}: {}",
                                error.code, error.message
                            ))),
                            _ => Err(ActrError::DecodeFailure(
                                "response has neither payload nor error".to_string(),
                            )),
                        };
                        let _ = dispatcher_gate
                            .handle_response(&envelope.request_id, result)
                            .await;
                    }
                    Some(Direction::Request) => {
                        *request_counts_for_dispatcher
                            .lock()
                            .expect("test request recorder lock poisoned")
                            .entry(envelope.request_id.clone())
                            .or_default() += 1;
                        request_changed_for_dispatcher.notify_waiters();
                        // Some recovery tests intentionally leave a request
                        // pending so controller teardown must release it.  The
                        // request still traverses the real RTC data plane;
                        // only the business responder is suppressed.
                        if envelope.route_key == "test.no_response" {
                            continue;
                        }
                        let response_payload = match envelope.route_key.as_str() {
                            "test.large_echo" => envelope.payload.clone(),
                            // Preserve trace-context integration coverage
                            // without installing a second consumer on the
                            // controller-owned inbound stream.
                            "trace/reflect" => Some(bytes::Bytes::copy_from_slice(
                                envelope.traceparent.as_deref().unwrap_or("").as_bytes(),
                            )),
                            _ => Some(bytes::Bytes::from_static(b"pong")),
                        };
                        let response = RpcEnvelope {
                            request_id: envelope.request_id,
                            route_key: "response".to_string(),
                            payload: response_payload,
                            direction: Some(Direction::Response as i32),
                            ..Default::default()
                        };
                        let _ = dispatcher_gate.send_response(&message.peer, response).await;
                    }
                    Some(Direction::Tell) => {}
                    _ => tracing::warn!("test dispatcher dropped RPC with invalid direction"),
                }
            }
        });
        self.bg_tasks.push(dispatcher_task);
        if self.rtc_matrix_instrumentation {
            self.address_providers
                .insert(serial, mutable_address_provider);
        }

        self.peers.insert(
            serial,
            TestPeer {
                id,
                signaling_client,
                gate,
                transport_manager,
                rtc: TestRtcControl::new(manager),
                hooks,
                credential,
                request_counts,
                request_changed,
                data_chunks,
                inbound_rx: tokio::sync::Mutex::new(raw_rx),
                router_task: tokio::sync::Mutex::new(Some(router_task)),
                hook_shutdown,
                hook_task: tokio::sync::Mutex::new(hook_task),
            },
        );
    }

    pub fn peer(&self, serial: u64) -> &TestPeer {
        self.peers
            .get(&serial)
            .unwrap_or_else(|| panic!("peer {serial} not found"))
    }

    pub fn peer_mut(&mut self, serial: u64) -> &mut TestPeer {
        self.peers
            .get_mut(&serial)
            .unwrap_or_else(|| panic!("peer {serial} not found"))
    }

    pub fn peer_serials(&self) -> Vec<u64> {
        self.peers.keys().copied().collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Replace the exact local address set used by future PC generations for
    /// one endpoint. The caller separately injects the platform/network fact.
    pub fn set_local_addresses(&self, serial: u64, addresses: Vec<IpAddr>) -> u64 {
        self.address_providers
            .get(&serial)
            .unwrap_or_else(|| panic!("peer {serial} address provider not found"))
            .set_addresses(addresses)
    }

    /// Remove and fully shut down one independent endpoint while leaving the
    /// other endpoints and the externally owned Actrix services alive. This is
    /// the process-restart seam used by the mobile pair contract.
    pub async fn remove_peer(&mut self, serial: u64) -> bool {
        let Some(peer) = self.peers.remove(&serial) else {
            return false;
        };
        self.address_providers.remove(&serial);
        peer.shutdown().await;
        true
    }

    pub async fn connect(&mut self, from: u64, to: u64) {
        self.connect_with_timeout(from, to, Duration::from_secs(20))
            .await;
    }

    pub async fn connect_with_timeout(&mut self, from: u64, to: u64, timeout: Duration) {
        let target = self.peer(to).id.clone();
        self.peer(from).rtc.connect(target.clone()).await;
        self.peer(from).rtc.wait_sendable(&target, timeout).await;
        let source = self.peer(from).id.clone();
        self.peer(to).rtc.wait_sendable(&source, timeout).await;
    }

    /// Wait until the production signaling handler has completed the
    /// actor-registration phase for this peer. The WebSocket client reports
    /// connected after the handshake, while the server registers the
    /// `actor_id -> client_id` index inside its upgrade task. A Ping/Pong is
    /// processed only after that registration code, so it provides an
    /// event-driven readiness barrier for pair tests.
    pub async fn wait_for_signaling_registration(&self, serial: u64, timeout: Duration) {
        self.peer(serial)
            .signaling_client
            .probe_alive(timeout)
            .await
            .unwrap_or_else(|error| {
                panic!("peer {serial} signaling registration did not become ready: {error}")
            });
    }

    pub fn simulate_disconnect(&self) {
        let vnet = self.vnet.as_ref().expect("with_vnet harness required");
        vnet.block_network();
        self.server.pause_forwarding();
        for peer in self.peers.values() {
            peer.rtc.publish(RtcNetworkEvent::NetworkUnavailable);
        }
    }

    pub fn simulate_reconnect(&self) {
        let vnet = self.vnet.as_ref().expect("with_vnet harness required");
        self.server.resume_forwarding();
        vnet.unblock_network();
        for peer in self.peers.values() {
            peer.rtc.publish(RtcNetworkEvent::NetworkAvailable);
        }
    }

    pub fn is_disconnected(&self) -> bool {
        self.vnet.as_ref().is_some_and(VNetPair::is_blocked)
    }

    pub async fn wait_for_ice_restart_count(&self, min: u32, timeout: Duration) -> u32 {
        crate::timer::timeout(crate::timer::ids::WEBRTC_ICE_RESTART_WAIT, timeout, async {
            loop {
                let count = self.server.get_ice_restart_count();
                if count >= min {
                    return count;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("ICE restart count did not reach {min}"))
    }

    pub async fn wait_for_ice_restart_request_count(&self, min: u32, timeout: Duration) -> u32 {
        crate::timer::timeout(crate::timer::ids::WEBRTC_ICE_RESTART_WAIT, timeout, async {
            loop {
                let count = self.server.get_ice_restart_request_count();
                if count >= min {
                    return count;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("ICE restart request count did not reach {min}"))
    }

    pub fn reset_counters(&self) {
        self.server.reset_counters();
    }

    pub fn ice_restart_count(&self) -> u32 {
        self.server.get_ice_restart_count()
    }

    pub async fn shutdown(mut self) {
        for task in self.bg_tasks.drain(..) {
            task.abort();
            let _ = task.await;
        }
        for peer in self.peers.values() {
            peer.shutdown().await;
        }
        self.peers.clear();
        self.rtc_faults.clear();
        self.server.shutdown().await;
    }
}
