//! Production-equivalent two-endpoint mobile RTC integration harness.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use actr_protocol::{ActrId, DataChunk, PayloadType, RpcEnvelope};
use prost::Message as _;
use tokio::sync::watch;

use super::{
    ActrixTestServices, DirectedRtcFaultMetrics, DirectedSignalingFaultMetrics, LinkDirection,
    PacketRule, SignalingFaultRule, SignalingMessageClass, SignalingTraceEvent,
    TestEvidenceFailureGuard, TestEvidenceTimeline, TestHarness, TestRtcRole, make_actor_id,
};
use crate::lifecycle::{
    AppLifecycleState, CleanupReason, NetworkAvailability, NetworkEventHandle, NetworkSnapshot,
    NetworkTransportFlags, SupervisorStatus,
};

/// Logical endpoint selector. Platform facts are injected into exactly one
/// endpoint unless a scenario explicitly calls both drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairEndpoint {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionInitiator {
    A,
    B,
}

/// Pair-level test oracle. This is a projection over two authoritative local
/// snapshots, never a third production state machine.
#[derive(Debug, Clone)]
pub struct PairSnapshot {
    pub a_to_b: actr_framework::WebRtcPeerState,
    pub b_to_a: actr_framework::WebRtcPeerState,
    pub role_a: TestRtcRole,
    pub role_b: TestRtcRole,
}

impl PairSnapshot {
    pub fn ready(&self) -> bool {
        self.a_to_b.sendable && self.b_to_a.sendable
    }
}

/// One endpoint's production-equivalent mobile lifecycle entry and policy
/// status recorder.
pub struct MobileEventDriver {
    endpoint: ActrId,
    evidence: TestEvidenceTimeline,
    handle: NetworkEventHandle,
    shutdown: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
    status: watch::Receiver<SupervisorStatus>,
}

impl MobileEventDriver {
    /// Attach the production gated lifecycle reconciler and RTC recovery
    /// feed to one endpoint in any integration topology. Mobile tests submit
    /// native network-path events explicitly; address polling is intentionally
    /// not part of the mobile production path.
    pub fn spawn(harness: &TestHarness, serial: u64) -> Self {
        let peer = harness.peer(serial);
        let processor = peer.production_network_processor();
        let (handle, facts, shutdown, task, status) =
            super::spawn_gated_rtc_network_event_supervisor(processor, &peer.rtc);
        // Wire the signaling client's generation facts into this supervisor
        // and seed a live generation (the `recovery_vnet_supervisor` wiring).
        // The harness client connects eagerly at `add_peer`, before this sink
        // exists, so its commit fact would be lost — the seed stands in for
        // it. Without a generation the supervisor's first projection closes
        // the recovery gate and every role-negotiation probe is dropped at
        // the gate until the 5s deadline fails it.
        peer.signaling_client
            .set_supervisor_fact_sink(facts.clone());
        facts.signaling_generation_committed(1, crate::lifecycle::SignalingFactOrigin::External);
        Self {
            endpoint: peer.id.clone(),
            evidence: harness.evidence.clone(),
            handle,
            shutdown,
            task,
            status,
        }
    }

    pub fn handle(&self) -> &NetworkEventHandle {
        &self.handle
    }

    pub fn status(&self) -> SupervisorStatus {
        self.status.borrow().clone()
    }

    fn record_platform(&self, operation: impl Into<String>) {
        self.evidence.record(
            Some(self.endpoint.clone()),
            "platform",
            None,
            operation,
            "submitted",
        );
    }

    fn record_accepted(&self, operation: impl Into<String>) {
        let status = self.status();
        self.evidence.record(
            Some(self.endpoint.clone()),
            "reconciler",
            None,
            operation,
            format!(
                "accepted policy_revision={} action_id={:?} outcome={:?}",
                status.policy_revision, status.last_action_id, status.last_outcome
            ),
        );
    }

    pub async fn background(&self) {
        self.record_platform("AppBackground");
        self.handle
            .handle_app_lifecycle_changed(AppLifecycleState::Background)
            .await
            .expect("background event accepted");
        self.record_accepted("AppBackground");
    }

    pub async fn foreground(&self, background_duration: Duration) {
        let operation = format!("AppForeground background_duration={background_duration:?}");
        self.record_platform(operation.clone());
        self.handle
            .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
                background_duration_ms: background_duration.as_millis() as u64,
            })
            .await
            .expect("foreground event accepted");
        self.record_accepted(operation);
    }

    pub async fn network_path(
        &self,
        sequence: u64,
        availability: NetworkAvailability,
        transport: NetworkTransportFlags,
    ) {
        self.network_path_from_monitor(
            &self.handle,
            "active-monitor",
            sequence,
            availability,
            transport,
        )
        .await;
    }

    /// Submit a path fact from an explicit monitor incarnation while keeping
    /// the platform→reconciler evidence continuous. P13 uses this to prove an
    /// old monitor handle is acknowledged but cannot regress the active epoch.
    pub async fn network_path_from_monitor(
        &self,
        handle: &NetworkEventHandle,
        monitor: &str,
        sequence: u64,
        availability: NetworkAvailability,
        transport: NetworkTransportFlags,
    ) {
        let operation = format!(
            "NetworkPath monitor={monitor} sequence={sequence} availability={availability:?} transport={transport:?}"
        );
        self.record_platform(operation.clone());
        handle
            .handle_network_path_changed(NetworkSnapshot {
                sequence,
                availability,
                transport,
                is_expensive: false,
                is_constrained: false,
            })
            .await
            .expect("network path event accepted");
        self.record_accepted(operation);
    }

    pub async fn cleanup(&self, reason: CleanupReason) {
        let operation = format!("Cleanup reason={reason:?}");
        self.record_platform(operation.clone());
        self.handle
            .cleanup_connections(reason)
            .await
            .expect("cleanup event accepted");
        self.record_accepted(operation);
    }

    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.task.await;
    }
}

/// Two independent Actr endpoints connected through production Actrix
/// Signaling/STUN/TURN and real webrtc-rs PeerConnections.
pub struct MobileRtcPairHarness {
    services: ActrixTestServices,
    harness: TestHarness,
    a_serial: u64,
    b_serial: u64,
    a_mobile: Option<MobileEventDriver>,
    b_mobile: Option<MobileEventDriver>,
    _failure_evidence: TestEvidenceFailureGuard,
}

impl MobileRtcPairHarness {
    pub async fn new(a_serial: u64, b_serial: u64) -> Self {
        Self::new_with_address_mode(a_serial, b_serial, false).await
    }

    /// Build a pair whose first bind is deterministic and whose next address
    /// change makes the test provider return dual-stack wildcards. The real
    /// webrtc-rs driver then expands those wildcards during ICE-restart rebind.
    pub async fn new_with_wildcard_rebind(a_serial: u64, b_serial: u64) -> Self {
        Self::new_with_address_mode(a_serial, b_serial, true).await
    }

    async fn new_with_address_mode(
        a_serial: u64,
        b_serial: u64,
        wildcard_rebind_addresses: bool,
    ) -> Self {
        assert_ne!(
            a_serial, b_serial,
            "pair endpoints require distinct identities"
        );
        let services = ActrixTestServices::start()
            .await
            .expect("Actrix Signaling/STUN/TURN services must start");
        let mut harness = if wildcard_rebind_addresses {
            TestHarness::with_external_signaling_and_wildcard_rebind(services.signaling_url()).await
        } else {
            TestHarness::with_external_signaling(services.signaling_url()).await
        };
        let a_id = make_actor_id(a_serial);
        let b_id = make_actor_id(b_serial);
        harness
            .add_peer_with_rtc_configuration_and_credential(
                a_serial,
                services.rtc_configuration(&a_id, false),
                services.aid_credential(&a_id),
            )
            .await;
        harness
            .add_peer_with_rtc_configuration_and_credential(
                b_serial,
                services.rtc_configuration(&b_id, false),
                services.aid_credential(&b_id),
            )
            .await;
        // `connect_to_with_identity()` returns after the WebSocket handshake,
        // but the Actrix upgrade task registers the actor asynchronously.
        // Wait for both Ping/Pong barriers before any RoleNegotiation can
        // address the peer through actor_id_index.
        harness
            .wait_for_signaling_registration(a_serial, Duration::from_secs(5))
            .await;
        harness
            .wait_for_signaling_registration(b_serial, Duration::from_secs(5))
            .await;

        let a_mobile = Some(MobileEventDriver::spawn(&harness, a_serial));
        let b_mobile = Some(MobileEventDriver::spawn(&harness, b_serial));
        let failure_evidence = TestEvidenceFailureGuard::new(
            format!("MobileRtcPair({a_serial},{b_serial})"),
            harness.evidence.clone(),
            harness.signaling_faults.clone(),
        );
        let pair = Self {
            services,
            harness,
            a_serial,
            b_serial,
            a_mobile,
            b_mobile,
            _failure_evidence: failure_evidence,
        };
        pair.foreground(PairEndpoint::A, Duration::ZERO).await;
        pair.foreground(PairEndpoint::B, Duration::ZERO).await;
        pair
    }

    pub fn services(&self) -> &ActrixTestServices {
        &self.services
    }

    pub fn harness(&self) -> &TestHarness {
        &self.harness
    }

    pub fn id(&self, endpoint: PairEndpoint) -> ActrId {
        self.harness.peer(self.serial(endpoint)).id.clone()
    }

    pub fn serial(&self, endpoint: PairEndpoint) -> u64 {
        match endpoint {
            PairEndpoint::A => self.a_serial,
            PairEndpoint::B => self.b_serial,
        }
    }

    pub fn events(&self, endpoint: PairEndpoint) -> &MobileEventDriver {
        match endpoint {
            PairEndpoint::A => self.a_mobile.as_ref(),
            PairEndpoint::B => self.b_mobile.as_ref(),
        }
        .expect("mobile event driver already stopped")
    }

    /// Submit exactly one Connect from the selected endpoint. Actrix remains
    /// authoritative for the final offerer/answerer assignment; the Connect
    /// caller can therefore become the answerer while the passive endpoint is
    /// assigned offerer and produces the first SDP offer.
    pub async fn establish(&mut self, initiator: ConnectionInitiator) {
        match initiator {
            ConnectionInitiator::A => self.harness.connect(self.a_serial, self.b_serial).await,
            ConnectionInitiator::B => self.harness.connect(self.b_serial, self.a_serial).await,
        }
        let ready = self.wait_pair_ready(Duration::from_secs(30)).await;
        let a = self.id(PairEndpoint::A);
        let b = self.id(PairEndpoint::B);
        let a_key = (a.realm.as_str(), a.serial_number);
        let b_key = (b.realm.as_str(), b.serial_number);
        // The fixture registers no explicit `webrtc_role=answer` preference,
        // so production Actrix falls back to its reverse ActorId ordering.
        let expected = if a_key > b_key {
            (TestRtcRole::Offerer, TestRtcRole::Answerer)
        } else {
            (TestRtcRole::Answerer, TestRtcRole::Offerer)
        };
        assert_eq!((ready.role_a, ready.role_b), expected);
        assert_eq!(self.harness.peer(self.a_serial).rtc.peer_count().await, 1);
        assert_eq!(self.harness.peer(self.b_serial).rtc.peer_count().await, 1);
        self.assert_rpc_both_directions("baseline").await;
    }

    pub async fn snapshot(&self) -> Option<PairSnapshot> {
        let a_id = self.id(PairEndpoint::A);
        let b_id = self.id(PairEndpoint::B);
        Some(PairSnapshot {
            a_to_b: self.harness.peer(self.a_serial).rtc.state(&b_id).await?,
            b_to_a: self.harness.peer(self.b_serial).rtc.state(&a_id).await?,
            role_a: self.harness.peer(self.a_serial).rtc.role(&b_id).await?,
            role_b: self.harness.peer(self.b_serial).rtc.role(&a_id).await?,
        })
    }

    pub async fn wait_pair_ready(&self, timeout: Duration) -> PairSnapshot {
        let a_id = self.id(PairEndpoint::A);
        let b_id = self.id(PairEndpoint::B);
        let (a_to_b, b_to_a) = tokio::join!(
            self.harness
                .peer(self.a_serial)
                .rtc
                .wait_state_diagnosed(&b_id, timeout, |state| state.sendable),
            self.harness
                .peer(self.b_serial)
                .rtc
                .wait_state_diagnosed(&a_id, timeout, |state| state.sendable),
        );
        let (a_to_b, b_to_a) = match (a_to_b, b_to_a) {
            (Ok(a_to_b), Ok(b_to_a)) => (a_to_b, b_to_a),
            (a_to_b, b_to_a) => {
                // A succeeded direction only needs a compact summary; the failing
                // side carries the full diagnostics.
                let compact = |state: actr_framework::WebRtcPeerState| {
                    format!(
                        "sendable={} status={:?} pc_generation={} revision={} \
                         network_path={:?} unavailable_reason={:?}",
                        state.sendable,
                        state.status,
                        state.pc_generation,
                        state.revision,
                        state.network_path,
                        state.unavailable_reason,
                    )
                };
                let a_to_b = a_to_b.map(compact).unwrap_or_else(|d| d);
                let b_to_a = b_to_a.map(compact).unwrap_or_else(|d| d);
                let trace = self.signaling_trace();
                let recent = &trace[trace.len().saturating_sub(32)..];
                panic!(
                    "pair did not become sendable within {timeout:?}: \
                     A->B={a_to_b}; B->A={b_to_a}; \
                     recent signaling (oldest first)={recent:#?}"
                );
            }
        };
        let snapshot = PairSnapshot {
            a_to_b,
            b_to_a,
            role_a: self
                .harness
                .peer(self.a_serial)
                .rtc
                .role(&b_id)
                .await
                .expect("A controller role missing"),
            role_b: self
                .harness
                .peer(self.b_serial)
                .rtc
                .role(&a_id)
                .await
                .expect("B controller role missing"),
        };
        assert!(snapshot.ready());
        self.harness.evidence.record_state(
            self.id(PairEndpoint::A),
            "controller",
            &snapshot.a_to_b,
            "PairReady A->B",
        );
        self.harness.evidence.record_state(
            self.id(PairEndpoint::B),
            "controller",
            &snapshot.b_to_a,
            "PairReady B->A",
        );
        self.assert_hook_projection(&snapshot, timeout).await;
        self.harness
            .rtc_faults
            .retain_generation(&a_id, &b_id, snapshot.a_to_b.pc_generation);
        self.harness
            .rtc_faults
            .retain_generation(&b_id, &a_id, snapshot.b_to_a.pc_generation);
        snapshot
    }

    async fn assert_hook_projection(&self, snapshot: &PairSnapshot, timeout: Duration) {
        for (local, remote, authoritative) in [
            (PairEndpoint::A, PairEndpoint::B, &snapshot.a_to_b),
            (PairEndpoint::B, PairEndpoint::A, &snapshot.b_to_a),
        ] {
            let remote_id = self.id(remote);
            let hook_state = self
                .harness
                .peer(self.serial(local))
                .hooks
                .wait_state(&remote_id, timeout, |state| {
                    state.revision == authoritative.revision
                })
                .await
                .unwrap_or_else(|| {
                    panic!(
                        "{local:?} hook did not expose authoritative revision {}",
                        authoritative.revision
                    )
                });
            assert_eq!(&hook_state, authoritative);

            let states = self
                .harness
                .peer(self.serial(local))
                .hooks
                .state_changes_for(&remote_id);
            assert!(
                states
                    .windows(2)
                    .all(|pair| pair[0].revision < pair[1].revision),
                "{local:?} hook revisions must be strictly monotonic"
            );
            assert!(states.iter().all(|state| state.peer == remote_id));
            assert!(
                self.harness
                    .peer(self.serial(local))
                    .hooks
                    .wait_legacy_edge(&remote_id, "connected", timeout)
                    .await,
                "{local:?} legacy hook never exposed the connected edge"
            );
            let legacy_edges = self
                .harness
                .peer(self.serial(local))
                .hooks
                .legacy_edges_for(&remote_id);
            assert!(
                legacy_edges
                    .windows(2)
                    .all(|edges| edges != ["connected", "connected"]),
                "{local:?} legacy hooks emitted duplicate connected edges: {legacy_edges:?}"
            );
        }
    }

    /// PairUsable test oracle: both local public states admit sends and one
    /// real RPC succeeds in each direction over the already-established link.
    pub async fn assert_pair_usable(&self, label: &str, timeout: Duration) -> PairSnapshot {
        let snapshot = self.wait_pair_ready(timeout).await;
        self.assert_rpc_both_directions(label).await;
        snapshot
    }

    /// Observe one endpoint closing mobile business admission through both the
    /// authoritative manager watch and the production public Hook projection.
    pub async fn wait_not_sendable(
        &self,
        endpoint: PairEndpoint,
        timeout: Duration,
    ) -> actr_framework::WebRtcPeerState {
        let remote = match endpoint {
            PairEndpoint::A => PairEndpoint::B,
            PairEndpoint::B => PairEndpoint::A,
        };
        let remote_id = self.id(remote);
        let state = self
            .harness
            .peer(self.serial(endpoint))
            .rtc
            .wait_state(&remote_id, timeout, |state| !state.sendable)
            .await
            .unwrap_or_else(|| panic!("{endpoint:?} did not become non-sendable"));
        let hook = self
            .harness
            .peer(self.serial(endpoint))
            .hooks
            .wait_state(&remote_id, timeout, |hook| hook.revision == state.revision)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "{endpoint:?} public hook missed non-sendable revision {}",
                    state.revision
                )
            });
        assert_eq!(hook, state);
        self.harness.evidence.record_state(
            self.id(endpoint),
            "controller",
            &state,
            "business admission closed",
        );
        state
    }

    pub async fn rpc(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        request_id: &str,
    ) -> actr_protocol::ActorResult<actr_framework::Bytes> {
        self.rpc_with_timeout(from, to, request_id, 15_000).await
    }

    pub async fn rpc_with_timeout(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        request_id: &str,
        timeout_ms: u32,
    ) -> actr_protocol::ActorResult<actr_framework::Bytes> {
        assert_ne!(from, to, "pair RPC requires distinct endpoints");
        self.harness.evidence.record(
            Some(self.id(from)),
            "business",
            Some(self.id(to)),
            format!("RPC request_id={request_id}"),
            "started",
        );
        let result = self
            .harness
            .peer(self.serial(from))
            .gate
            .send_request(
                &self.id(to),
                RpcEnvelope {
                    request_id: request_id.to_owned(),
                    route_key: "test.ping".to_string(),
                    payload: Some(bytes::Bytes::from_static(b"ping")),
                    timeout_ms: i64::from(timeout_ms),
                    ..Default::default()
                },
            )
            .await;
        self.harness.evidence.record(
            Some(self.id(from)),
            "business",
            Some(self.id(to)),
            format!("RPC request_id={request_id}"),
            if result.is_ok() { "success" } else { "failed" },
        );
        result
    }

    pub fn spawn_large_echo(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        request_id: &str,
        payload: bytes::Bytes,
        timeout: Duration,
    ) -> tokio::task::JoinHandle<actr_protocol::ActorResult<actr_framework::Bytes>> {
        let gate = self.harness.peer(self.serial(from)).gate.clone();
        let target = self.id(to);
        let request_id = request_id.to_owned();
        tokio::spawn(async move {
            gate.send_request(
                &target,
                RpcEnvelope {
                    request_id,
                    route_key: "test.large_echo".to_string(),
                    payload: Some(payload),
                    timeout_ms: timeout.as_millis() as i64,
                    ..Default::default()
                },
            )
            .await
        })
    }

    pub async fn assert_rpc_both_directions(&self, label: &str) {
        for (from, to, suffix) in [
            (PairEndpoint::A, PairEndpoint::B, "a_to_b"),
            (PairEndpoint::B, PairEndpoint::A, "b_to_a"),
        ] {
            let response = self
                .rpc(from, to, &format!("{label}_{suffix}"))
                .await
                .unwrap_or_else(|error| panic!("{label} {suffix} RPC failed: {error}"));
            assert_eq!(response.as_ref(), b"pong");
        }
    }

    pub async fn send_data_chunk(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        stream_id: &str,
        payload: bytes::Bytes,
    ) {
        let chunk = DataChunk {
            stream_id: stream_id.to_owned(),
            sequence: 1,
            payload: payload.clone(),
            metadata: Vec::new(),
            timestamp_ms: Some(0),
        };
        let operation_id = crate::outbound::data_chunk_operation_id(stream_id, chunk.sequence);
        self.harness
            .peer(self.serial(from))
            .gate
            .send_data_chunk(
                &self.id(to),
                PayloadType::StreamReliable,
                stream_id,
                bytes::Bytes::from(chunk.encode_to_vec()),
                &operation_id,
            )
            .await
            .expect("pair DataChunk send");
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(15),
            async {
                loop {
                    let (_, lane, encoded) = self
                        .harness
                        .peer(self.serial(to))
                        .receive_payload()
                        .await
                        .expect("pair DataChunk receive stream ended");
                    assert_eq!(lane, PayloadType::StreamReliable);
                    let received = DataChunk::decode(encoded.as_ref()).expect("valid DataChunk");
                    if received.stream_id != stream_id {
                        // A delivery-uncertain pre-recovery chunk may arrive just
                        // before the replacement becomes authoritative. Consume
                        // it, but never mistake it for the new stream's proof.
                        continue;
                    }
                    assert_eq!(received.payload.as_ref(), payload.as_ref());
                    assert_eq!(
                        self.harness
                            .peer(self.serial(to))
                            .data_chunk_count(stream_id),
                        1,
                        "DataChunk stream {stream_id} was delivered more than once"
                    );
                    return;
                }
            },
        )
        .await
        .expect("pair DataChunk receive timeout");
    }

    /// Start a DataChunk send without waiting for its remote observation so a
    /// test can interrupt a concrete fragmented send at the RTC lane seam.
    pub fn spawn_data_chunk(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        stream_id: &str,
        payload: bytes::Bytes,
    ) -> tokio::task::JoinHandle<actr_protocol::ActorResult<()>> {
        let chunk = DataChunk {
            stream_id: stream_id.to_owned(),
            sequence: 1,
            payload,
            metadata: Vec::new(),
            timestamp_ms: Some(0),
        };
        let operation_id =
            crate::outbound::data_chunk_operation_id(&chunk.stream_id, chunk.sequence);
        let gate = self.harness.peer(self.serial(from)).gate.clone();
        let target = self.id(to);
        let stream_id = stream_id.to_owned();
        tokio::spawn(async move {
            gate.send_data_chunk(
                &target,
                PayloadType::StreamReliable,
                &stream_id,
                bytes::Bytes::from(chunk.encode_to_vec()),
                &operation_id,
            )
            .await
        })
    }

    pub async fn assert_data_chunk_both_directions(&self, label: &str) {
        self.send_data_chunk(
            PairEndpoint::A,
            PairEndpoint::B,
            &format!("{label}_a_to_b"),
            bytes::Bytes::from_static(b"a-to-b"),
        )
        .await;
        self.send_data_chunk(
            PairEndpoint::B,
            PairEndpoint::A,
            &format!("{label}_b_to_a"),
            bytes::Bytes::from_static(b"b-to-a"),
        )
        .await;
    }

    /// Apply a physical data-plane rule to the currently active local
    /// generations at both ends of this one logical link.
    pub async fn set_rtc_fault(&self, direction: LinkDirection, rule: PacketRule) {
        let snapshot = self
            .snapshot()
            .await
            .expect("RTC fault requires an established pair");
        assert!(
            self.harness.rtc_faults.set_link_rule(
                (&self.id(PairEndpoint::A), snapshot.a_to_b.pc_generation),
                (&self.id(PairEndpoint::B), snapshot.b_to_a.pc_generation),
                direction,
                rule,
            ),
            "active pair generations were not registered at the Runtime seam"
        );
        self.harness.evidence.record(
            Some(self.id(PairEndpoint::A)),
            "fault",
            Some(self.id(PairEndpoint::B)),
            format!("RTC direction={direction:?} rule={rule:?}"),
            "installed",
        );
    }

    pub async fn heal_rtc_faults(&self) {
        let snapshot = self
            .snapshot()
            .await
            .expect("RTC heal requires an established pair");
        assert!(self.harness.rtc_faults.heal_link(
            (&self.id(PairEndpoint::A), snapshot.a_to_b.pc_generation),
            (&self.id(PairEndpoint::B), snapshot.b_to_a.pc_generation),
        ));
        self.harness.evidence.record(
            Some(self.id(PairEndpoint::A)),
            "fault",
            Some(self.id(PairEndpoint::B)),
            "RTC pair fault",
            "healed",
        );
    }

    pub async fn rtc_fault_metrics(&self, endpoint: PairEndpoint) -> DirectedRtcFaultMetrics {
        let snapshot = self
            .snapshot()
            .await
            .expect("fault metrics require an established pair");
        let (local, remote, generation) = match endpoint {
            PairEndpoint::A => (
                self.id(PairEndpoint::A),
                self.id(PairEndpoint::B),
                snapshot.a_to_b.pc_generation,
            ),
            PairEndpoint::B => (
                self.id(PairEndpoint::B),
                self.id(PairEndpoint::A),
                snapshot.b_to_a.pc_generation,
            ),
        };
        self.harness
            .rtc_faults
            .handle(&local, &remote, generation)
            .expect("active fault handle")
            .metrics()
    }

    /// Wait until the configured physical link fault is observed on either
    /// endpoint's generation-scoped RTC Runtime wrapper.
    pub async fn wait_rtc_fault_hit(
        &self,
        timeout: Duration,
    ) -> (DirectedRtcFaultMetrics, DirectedRtcFaultMetrics) {
        let snapshot = self
            .snapshot()
            .await
            .expect("fault milestone requires an established pair");
        let a = self
            .harness
            .rtc_faults
            .handle(
                &self.id(PairEndpoint::A),
                &self.id(PairEndpoint::B),
                snapshot.a_to_b.pc_generation,
            )
            .expect("A fault handle");
        let b = self
            .harness
            .rtc_faults
            .handle(
                &self.id(PairEndpoint::B),
                &self.id(PairEndpoint::A),
                snapshot.b_to_a.pc_generation,
            )
            .expect("B fault handle");
        tokio::select! {
            hit = a.wait_for_hit_after(0, timeout) => {
                hit.expect("physical RTC fault did not hit A's active generation");
            }
            hit = b.wait_for_hit_after(0, timeout) => {
                hit.expect("physical RTC fault did not hit B's active generation");
            }
        }
        let metrics = (a.metrics(), b.metrics());
        assert!(
            metrics.0.outgoing_hits
                + metrics.0.incoming_hits
                + metrics.1.outgoing_hits
                + metrics.1.incoming_hits
                > 0
        );
        self.harness.evidence.record(
            Some(self.id(PairEndpoint::A)),
            "fault",
            Some(self.id(PairEndpoint::B)),
            "RTC pair fault",
            format!("hit a={:?} b={:?}", metrics.0, metrics.1),
        );
        metrics
    }

    /// Exact addresses supplied to `PeerConnectionBuilder::with_udp_addrs`
    /// for the endpoint's current generation.
    pub async fn rtc_bind_addresses(&self, endpoint: PairEndpoint) -> Vec<SocketAddr> {
        let snapshot = self
            .snapshot()
            .await
            .expect("bind evidence requires an established pair");
        let (local, remote, generation) = match endpoint {
            PairEndpoint::A => (
                self.id(PairEndpoint::A),
                self.id(PairEndpoint::B),
                snapshot.a_to_b.pc_generation,
            ),
            PairEndpoint::B => (
                self.id(PairEndpoint::B),
                self.id(PairEndpoint::A),
                snapshot.b_to_a.pc_generation,
            ),
        };
        self.harness
            .rtc_faults
            .handle(&local, &remote, generation)
            .expect("active generation fault handle")
            .bind_addresses()
    }

    /// Actual local socket addresses observed at the Runtime boundary for the
    /// endpoint's current generation. Includes same-generation ICE-restart
    /// rebinds, unlike [`Self::rtc_bind_addresses`].
    pub async fn rtc_bound_addresses(&self, endpoint: PairEndpoint) -> Vec<SocketAddr> {
        let snapshot = self
            .snapshot()
            .await
            .expect("bound-address evidence requires an established pair");
        let (local, remote, generation) = match endpoint {
            PairEndpoint::A => (
                self.id(PairEndpoint::A),
                self.id(PairEndpoint::B),
                snapshot.a_to_b.pc_generation,
            ),
            PairEndpoint::B => (
                self.id(PairEndpoint::B),
                self.id(PairEndpoint::A),
                snapshot.b_to_a.pc_generation,
            ),
        };
        self.harness
            .rtc_faults
            .handle(&local, &remote, generation)
            .expect("active generation fault handle")
            .bound_socket_addresses()
    }

    pub async fn wait_rtc_bound_address(
        &self,
        endpoint: PairEndpoint,
        baseline: usize,
        expected_ip: IpAddr,
        timeout: Duration,
    ) -> Option<SocketAddr> {
        let snapshot = self
            .snapshot()
            .await
            .expect("bound-address evidence requires an established pair");
        let (local, remote, generation) = match endpoint {
            PairEndpoint::A => (
                self.id(PairEndpoint::A),
                self.id(PairEndpoint::B),
                snapshot.a_to_b.pc_generation,
            ),
            PairEndpoint::B => (
                self.id(PairEndpoint::B),
                self.id(PairEndpoint::A),
                snapshot.b_to_a.pc_generation,
            ),
        };
        self.harness
            .rtc_faults
            .handle(&local, &remote, generation)
            .expect("active generation fault handle")
            .wait_for_bound_address(baseline, expected_ip, timeout)
            .await
    }

    pub fn rtc_fault_scope_count(&self) -> usize {
        self.harness.rtc_faults.scope_count()
    }

    pub fn set_signaling_fault(
        &self,
        direction: LinkDirection,
        class: SignalingMessageClass,
        rule: SignalingFaultRule,
    ) {
        let a = self.id(PairEndpoint::A);
        let b = self.id(PairEndpoint::B);
        match direction {
            LinkDirection::AToB => self.harness.signaling_faults.set_rule(a, b, class, rule),
            LinkDirection::BToA => self.harness.signaling_faults.set_rule(b, a, class, rule),
            LinkDirection::Both => {
                self.harness
                    .signaling_faults
                    .set_rule(a.clone(), b.clone(), class, rule);
                self.harness.signaling_faults.set_rule(b, a, class, rule);
            }
        }
        self.harness.evidence.record(
            Some(self.id(PairEndpoint::A)),
            "fault",
            Some(self.id(PairEndpoint::B)),
            format!("signaling direction={direction:?} class={class:?} rule={rule:?}"),
            "installed",
        );
    }

    pub fn heal_signaling_faults(&self) {
        self.harness
            .signaling_faults
            .heal_pair(&self.id(PairEndpoint::A), &self.id(PairEndpoint::B));
        self.harness.evidence.record(
            Some(self.id(PairEndpoint::A)),
            "fault",
            Some(self.id(PairEndpoint::B)),
            "signaling pair fault",
            "healed",
        );
    }

    pub fn signaling_fault_metrics(&self) -> DirectedSignalingFaultMetrics {
        self.harness.signaling_faults.metrics()
    }

    pub fn signaling_hits(
        &self,
        from: PairEndpoint,
        to: PairEndpoint,
        class: SignalingMessageClass,
    ) -> usize {
        self.harness
            .signaling_faults
            .hits(&self.id(from), &self.id(to), class)
    }

    pub fn signaling_trace(&self) -> Vec<SignalingTraceEvent> {
        self.harness.signaling_faults.trace()
    }

    pub async fn background(&self, endpoint: PairEndpoint) {
        self.events(endpoint).background().await;
    }

    pub async fn foreground(&self, endpoint: PairEndpoint, background_duration: Duration) {
        self.events(endpoint).foreground(background_duration).await;
    }

    pub async fn network_path(
        &self,
        endpoint: PairEndpoint,
        sequence: u64,
        availability: NetworkAvailability,
        transport: NetworkTransportFlags,
    ) {
        self.events(endpoint)
            .network_path(sequence, availability, transport)
            .await;
    }

    /// Deliver the same explicit cleanup command used by a terminating mobile
    /// application. Unlike lifecycle facts, cleanup is a command and is never
    /// mirrored to the remote endpoint.
    pub async fn cleanup(&self, endpoint: PairEndpoint, reason: CleanupReason) {
        self.events(endpoint).cleanup(reason).await;
    }

    /// Destroy one complete endpoint and create a fresh node with the same
    /// ActrId against the still-running Actrix services. The returned handle is
    /// a new monitor incarnation owned by the new process model.
    pub async fn restart_endpoint(&mut self, endpoint: PairEndpoint) {
        let serial = self.serial(endpoint);
        let driver = match endpoint {
            PairEndpoint::A => self.a_mobile.take(),
            PairEndpoint::B => self.b_mobile.take(),
        }
        .expect("mobile event driver already stopped");
        driver.shutdown().await;
        assert!(
            self.harness.remove_peer(serial).await,
            "endpoint must exist"
        );

        let id = make_actor_id(serial);
        self.harness
            .add_peer_with_rtc_configuration_and_credential(
                serial,
                self.services.rtc_configuration(&id, false),
                self.services.aid_credential(&id),
            )
            .await;
        let replacement = Some(MobileEventDriver::spawn(&self.harness, serial));
        match endpoint {
            PairEndpoint::A => self.a_mobile = replacement,
            PairEndpoint::B => self.b_mobile = replacement,
        }
        self.foreground(endpoint, Duration::ZERO).await;
    }

    pub async fn shutdown(mut self) {
        self.harness
            .evidence
            .record(None, "teardown", None, "pair shutdown", "started");
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(30),
            async {
                if let Some(driver) = self.a_mobile.take() {
                    driver.shutdown().await;
                }
                if let Some(driver) = self.b_mobile.take() {
                    driver.shutdown().await;
                }
                self.harness.peer(self.a_serial).rtc.close_all().await;
                self.harness.peer(self.b_serial).rtc.close_all().await;
                assert!(
                    self.harness
                        .peer(self.a_serial)
                        .rtc
                        .wait_peer_count(0, Duration::from_secs(10))
                        .await
                );
                assert!(
                    self.harness
                        .peer(self.b_serial)
                        .rtc
                        .wait_peer_count(0, Duration::from_secs(10))
                        .await
                );
                for serial in [self.a_serial, self.b_serial] {
                    let peer = self.harness.peer(serial);
                    let observation = peer.rtc.observation().await;
                    assert_eq!(observation.peer_count, 0);
                    assert_eq!(peer.pending_count().await, 0);
                }
                self.harness.rtc_faults.clear();
                assert_eq!(self.harness.rtc_faults.scope_count(), 0);
                self.harness.signaling_faults.clear();
                assert_eq!(self.harness.signaling_faults.active_rule_count(), 0);
                assert!(
                    self.harness
                        .signaling_faults
                        .wait_deferred_idle(Duration::from_secs(5))
                        .await
                );
                let evidence = self.harness.evidence.clone();
                self.harness.shutdown().await;
                self.services.shutdown().await;
                evidence.record(None, "teardown", None, "pair shutdown", "converged");
            },
        )
        .await
        .expect("pair teardown exceeded its total deadline");
    }
}
