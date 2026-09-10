//! Multi-peer isolation over real Actrix services, production mobile recovery
//! feed, and independent v0.20 controllers.

use std::collections::HashMap;
use std::time::Duration;

use actr_hyper::lifecycle::{NetworkAvailability, NetworkTransportFlags};
use actr_hyper::test_support::{
    ActrixTestServices, LinkDirection, MobileEventDriver, PacketRule, SignalingFaultRule,
    SignalingMessageClass, TestEvidenceFailureGuard, TestHarness, bindable_non_loopback_ipv4,
    make_actor_id,
};
use actr_protocol::RpcEnvelope;

const WAIT: Duration = Duration::from_secs(30);
// Production Actrix assigns the offerer by reverse ActorId order when neither
// endpoint declares an answer preference. Keep hub A above all remotes so the
// MP matrix's A-originated restart-offer assertions exercise the intended side.
const A: u64 = 52_010;
const B: u64 = 52_002;
const C: u64 = 52_003;
const D: u64 = 52_004;
const REMOTES: [u64; 3] = [B, C, D];

fn wifi() -> NetworkTransportFlags {
    NetworkTransportFlags {
        wifi: true,
        cellular: false,
        ethernet: false,
        vpn: false,
        other: false,
    }
}

fn cellular() -> NetworkTransportFlags {
    NetworkTransportFlags {
        wifi: false,
        cellular: true,
        ethernet: false,
        vpn: false,
        other: false,
    }
}

async fn rpc(harness: &TestHarness, from: u64, to: u64, request_id: &str) {
    harness.evidence.record(
        Some(harness.peer(from).id.clone()),
        "business",
        Some(harness.peer(to).id.clone()),
        format!("RPC request_id={request_id}"),
        "started",
    );
    let response = harness
        .peer(from)
        .gate
        .send_request(
            &harness.peer(to).id,
            RpcEnvelope {
                request_id: request_id.to_owned(),
                route_key: "test.ping".to_string(),
                payload: Some(bytes::Bytes::from_static(b"ping")),
                timeout_ms: 15_000,
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("{request_id}: {error}"));
    assert_eq!(response.as_ref(), b"pong");
    harness.evidence.record(
        Some(harness.peer(from).id.clone()),
        "business",
        Some(harness.peer(to).id.clone()),
        format!("RPC request_id={request_id}"),
        "success",
    );
}

async fn pair_rpc(harness: &TestHarness, a: u64, b: u64, label: &str) {
    rpc(harness, a, b, &format!("{label}_{a}_{b}")).await;
    rpc(harness, b, a, &format!("{label}_{b}_{a}")).await;
}

struct MultiPeerTopology {
    services: ActrixTestServices,
    harness: TestHarness,
    a_mobile: MobileEventDriver,
    _failure_evidence: TestEvidenceFailureGuard,
}

impl MultiPeerTopology {
    async fn new() -> Self {
        let services = ActrixTestServices::start()
            .await
            .expect("Actrix services start");
        let mut harness = TestHarness::with_external_signaling(services.signaling_url()).await;
        for serial in [A, B, C, D] {
            harness
                .add_peer_with_rtc_configuration_and_credential(
                    serial,
                    services.rtc_configuration(&make_actor_id(serial), false),
                    services.aid_credential(&make_actor_id(serial)),
                )
                .await;
        }
        let a_mobile = MobileEventDriver::spawn(&harness, A);
        a_mobile.foreground(Duration::ZERO).await;
        for remote in REMOTES {
            harness.connect(A, remote).await;
        }
        let failure_evidence = TestEvidenceFailureGuard::new(
            "MobileRtcMultiPeer(A-B/C/D)",
            harness.evidence.clone(),
            harness.signaling_faults.clone(),
        );
        let topology = Self {
            services,
            harness,
            a_mobile,
            _failure_evidence: failure_evidence,
        };
        topology.assert_all_links("baseline").await;
        // Seed the active path observation through the same native event seam
        // used by mobile platforms. Later address changes must be accompanied
        // by a new path event; changing the test provider alone is not a
        // production signal and must not drive recovery.
        topology
            .network_path(1, NetworkAvailability::Available, wifi())
            .await;
        topology
    }

    fn peer_id(&self, serial: u64) -> actr_protocol::ActrId {
        self.harness.peer(serial).id.clone()
    }

    async fn generations(&self) -> HashMap<(u64, u64), u64> {
        let mut generations = HashMap::new();
        for remote in REMOTES {
            generations.insert(
                (A, remote),
                self.harness
                    .peer(A)
                    .rtc
                    .generation(&self.peer_id(remote))
                    .await
                    .expect("A controller generation"),
            );
            generations.insert(
                (remote, A),
                self.harness
                    .peer(remote)
                    .rtc
                    .generation(&self.peer_id(A))
                    .await
                    .expect("remote controller generation"),
            );
        }
        generations
    }

    async fn ice_generations(&self) -> HashMap<(u64, u64), u64> {
        let mut generations = HashMap::new();
        for remote in REMOTES {
            generations.insert(
                (A, remote),
                self.harness
                    .peer(A)
                    .rtc
                    .ice_generation(&self.peer_id(remote))
                    .await
                    .expect("A controller ICE generation"),
            );
            generations.insert(
                (remote, A),
                self.harness
                    .peer(remote)
                    .rtc
                    .ice_generation(&self.peer_id(A))
                    .await
                    .expect("remote controller ICE generation"),
            );
        }
        generations
    }

    async fn wait_link(&self, remote: u64) {
        let a_state = self
            .harness
            .peer(A)
            .rtc
            .wait_sendable(&self.peer_id(remote), WAIT)
            .await;
        let remote_state = self
            .harness
            .peer(remote)
            .rtc
            .wait_sendable(&self.peer_id(A), WAIT)
            .await;
        let a_hook = self
            .harness
            .peer(A)
            .hooks
            .wait_state(&self.peer_id(remote), WAIT, |state| {
                state.revision == a_state.revision
            })
            .await
            .expect("A public hook must project the authoritative link state");
        let remote_hook = self
            .harness
            .peer(remote)
            .hooks
            .wait_state(&self.peer_id(A), WAIT, |state| {
                state.revision == remote_state.revision
            })
            .await
            .expect("remote public hook must project the authoritative link state");
        assert_eq!(a_hook, a_state);
        assert_eq!(remote_hook, remote_state);
        self.harness.evidence.record_state(
            self.peer_id(A),
            "controller",
            &a_state,
            format!("PairReady A->{remote}"),
        );
        self.harness.evidence.record_state(
            self.peer_id(remote),
            "controller",
            &remote_state,
            format!("PairReady {remote}->A"),
        );
        assert!(
            self.harness
                .peer(A)
                .hooks
                .state_changes_for(&self.peer_id(remote))
                .windows(2)
                .all(|states| states[0].revision < states[1].revision)
        );
        assert!(
            self.harness
                .peer(A)
                .hooks
                .legacy_edges_for(&self.peer_id(remote))
                .contains(&"connected")
        );
        assert!(
            self.harness
                .peer(remote)
                .hooks
                .legacy_edges_for(&self.peer_id(A))
                .contains(&"connected")
        );
        assert!(
            self.harness
                .peer(remote)
                .hooks
                .state_changes_for(&self.peer_id(A))
                .windows(2)
                .all(|states| states[0].revision < states[1].revision)
        );
        self.harness.rtc_faults.retain_generation(
            &self.peer_id(A),
            &self.peer_id(remote),
            a_state.pc_generation,
        );
        self.harness.rtc_faults.retain_generation(
            &self.peer_id(remote),
            &self.peer_id(A),
            remote_state.pc_generation,
        );
    }

    async fn wait_link_fault_hit(&self, remote: u64, generations: &HashMap<(u64, u64), u64>) {
        let a = self
            .harness
            .rtc_faults
            .handle(
                &self.peer_id(A),
                &self.peer_id(remote),
                generations[&(A, remote)],
            )
            .expect("A directed Runtime fault handle");
        let remote_handle = self
            .harness
            .rtc_faults
            .handle(
                &self.peer_id(remote),
                &self.peer_id(A),
                generations[&(remote, A)],
            )
            .expect("remote directed Runtime fault handle");
        tokio::select! {
            hit = a.wait_for_hit_after(0, Duration::from_secs(10)) => {
                hit.expect("A-side v0.20 Runtime fault did not hit");
            }
            hit = remote_handle.wait_for_hit_after(0, Duration::from_secs(10)) => {
                hit.expect("remote-side v0.20 Runtime fault did not hit");
            }
        }
        self.harness.evidence.record(
            Some(self.peer_id(A)),
            "fault",
            Some(self.peer_id(remote)),
            "RTC link fault",
            "hit real v0.20 Runtime",
        );
    }

    async fn assert_all_links(&self, label: &str) {
        for remote in REMOTES {
            self.wait_link(remote).await;
            pair_rpc(&self.harness, A, remote, label).await;
        }
    }

    async fn network_path(
        &self,
        sequence: u64,
        availability: NetworkAvailability,
        transport: NetworkTransportFlags,
    ) {
        self.a_mobile
            .network_path(sequence, availability, transport)
            .await;
    }

    async fn foreground(&self, background_duration: Duration) {
        self.a_mobile.foreground(background_duration).await;
    }

    async fn background(&self) {
        self.a_mobile.background().await;
    }

    async fn shutdown(self) {
        self.harness
            .evidence
            .record(None, "teardown", None, "multi-peer shutdown", "started");
        tokio::time::timeout(Duration::from_secs(30), async {
            self.a_mobile.shutdown().await;
            for serial in [A, B, C, D] {
                self.harness.peer(serial).rtc.close_all().await;
            }
            for serial in [A, B, C, D] {
                let peer = self.harness.peer(serial);
                assert!(peer.rtc.wait_peer_count(0, Duration::from_secs(10)).await);
                let observation = peer.rtc.observation().await;
                assert_eq!(observation.peer_count, 0);
                assert_eq!(peer.pending_count().await, 0);
            }
            self.harness.rtc_faults.clear();
            self.harness.signaling_faults.clear();
            assert_eq!(self.harness.rtc_faults.scope_count(), 0);
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
            evidence.record(None, "teardown", None, "multi-peer shutdown", "converged");
        })
        .await
        .expect("multi-peer teardown exceeded its total deadline");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp01_one_link_fault_does_not_change_or_block_other_peer_connections() {
    let topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    let a = topology.peer_id(A);
    let b = topology.peer_id(B);

    assert!(topology.harness.rtc_faults.set_link_rule(
        (&a, initial[&(A, B)]),
        (&b, initial[&(B, A)]),
        LinkDirection::Both,
        PacketRule::Blackhole,
    ));
    topology.wait_link_fault_hit(B, &initial).await;
    let blocked = topology
        .harness
        .peer(A)
        .rtc
        .wait_state(&b, WAIT, |state| !state.sendable)
        .await
        .expect("A-B must become non-sendable from a real v0.20 ICE/PC callback");
    let blocked_hook = topology
        .harness
        .peer(A)
        .hooks
        .wait_state(&b, WAIT, |state| state.revision == blocked.revision)
        .await
        .expect("A's public hook must close admission only for B");
    assert_eq!(blocked_hook, blocked);
    topology.harness.evidence.record_state(
        a.clone(),
        "controller",
        &blocked,
        "MP01 A-B admission closed",
    );

    pair_rpc(&topology.harness, A, C, "faulted_ab_ac_healthy").await;
    pair_rpc(&topology.harness, A, D, "faulted_ab_ad_healthy").await;
    assert_eq!(
        topology
            .harness
            .peer(A)
            .rtc
            .generation(&topology.peer_id(C))
            .await,
        Some(initial[&(A, C)])
    );
    assert_eq!(
        topology
            .harness
            .peer(A)
            .rtc
            .generation(&topology.peer_id(D))
            .await,
        Some(initial[&(A, D)])
    );

    assert!(
        topology
            .harness
            .rtc_faults
            .heal_link((&a, initial[&(A, B)]), (&b, initial[&(B, A)]),)
    );
    topology
        .network_path(1, NetworkAvailability::Available, wifi())
        .await;
    topology.assert_all_links("restored").await;
    assert_eq!(topology.harness.peer(A).rtc.peer_count().await, 3);
    for remote in REMOTES {
        assert_eq!(topology.harness.peer(remote).rtc.peer_count().await, 1);
    }
    topology.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp02_a_address_change_soft_recovers_each_link_without_cross_peer_state() {
    let Some(changed_address) = bindable_non_loopback_ipv4() else {
        eprintln!("mp02: skipping — no bindable non-loopback IPv4 interface on this host");
        return;
    };
    let topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    let initial_ice = topology.ice_generations().await;
    let a_id = topology.peer_id(A);
    let initial_a_bound: HashMap<u64, usize> = REMOTES
        .iter()
        .map(|&remote| {
            let bound = topology
                .harness
                .rtc_faults
                .handle(&a_id, &topology.peer_id(remote), initial[&(A, remote)])
                .expect("A active generation fault handle")
                .bound_socket_addresses();
            (remote, bound.len())
        })
        .collect();
    topology
        .harness
        .set_local_addresses(A, vec![changed_address]);
    topology
        .network_path(2, NetworkAvailability::Available, cellular())
        .await;

    for remote in REMOTES {
        topology
            .harness
            .peer(A)
            .rtc
            .wait_ice_generation(&topology.peer_id(remote), initial_ice[&(A, remote)], WAIT)
            .await
            .unwrap_or_else(|| panic!("A-{remote} must advance its ICE generation"));
        topology
            .harness
            .rtc_faults
            .handle(&a_id, &topology.peer_id(remote), initial[&(A, remote)])
            .expect("A active generation fault handle")
            .wait_for_bound_address(initial_a_bound[&remote], changed_address, WAIT)
            .await
            .unwrap_or_else(|| panic!("A-{remote} ICE restart must bind the changed address"));
        topology
            .harness
            .peer(A)
            .rtc
            .wait_state(&topology.peer_id(remote), WAIT, |state| {
                state.sendable && state.pc_generation == initial[&(A, remote)]
            })
            .await
            .unwrap_or_else(|| panic!("A-{remote} must complete same-generation ICE recovery"));
        topology
            .harness
            .peer(remote)
            .rtc
            .wait_ice_generation(&topology.peer_id(A), initial_ice[&(remote, A)], WAIT)
            .await
            .unwrap_or_else(|| panic!("{remote}-A must advance its ICE generation"));
        topology
            .harness
            .peer(remote)
            .rtc
            .wait_state(&topology.peer_id(A), WAIT, |state| {
                state.sendable && state.pc_generation == initial[&(remote, A)]
            })
            .await
            .unwrap_or_else(|| panic!("{remote}-A must complete same-generation ICE recovery"));
    }
    topology.assert_all_links("mp02_soft_recovered").await;
    let recovered_ice = topology.ice_generations().await;
    for remote in REMOTES {
        let state = topology
            .harness
            .peer(A)
            .rtc
            .state(&topology.peer_id(remote))
            .await
            .expect("A recovered state");
        assert_eq!(state.pc_generation, initial[&(A, remote)]);
        assert!(recovered_ice[&(A, remote)] > initial_ice[&(A, remote)]);
    }
    assert_eq!(topology.harness.peer(A).rtc.peer_count().await, 3);
    topology.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp03_one_node_event_soft_recovers_three_controllers_independently_within_one_deadline() {
    let Some(changed_address) = bindable_non_loopback_ipv4() else {
        eprintln!("mp03: skipping — no bindable non-loopback IPv4 interface on this host");
        return;
    };
    let topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    let initial_ice = topology.ice_generations().await;
    let a_id = topology.peer_id(A);
    let initial_a_bound: HashMap<u64, usize> = REMOTES
        .iter()
        .map(|&remote| {
            let bound = topology
                .harness
                .rtc_faults
                .handle(&a_id, &topology.peer_id(remote), initial[&(A, remote)])
                .expect("A active generation fault handle")
                .bound_socket_addresses();
            (remote, bound.len())
        })
        .collect();
    topology
        .harness
        .set_local_addresses(A, vec![changed_address]);
    topology
        .network_path(2, NetworkAvailability::Available, cellular())
        .await;
    tokio::time::timeout(WAIT, async {
        for remote in REMOTES {
            topology
                .harness
                .peer(A)
                .rtc
                .wait_ice_generation(&topology.peer_id(remote), initial_ice[&(A, remote)], WAIT)
                .await
                .expect("A controller must advance its ICE generation");
            topology
                .harness
                .rtc_faults
                .handle(&a_id, &topology.peer_id(remote), initial[&(A, remote)])
                .expect("A active generation fault handle")
                .wait_for_bound_address(initial_a_bound[&remote], changed_address, WAIT)
                .await
                .expect("A controller's ICE restart must bind the changed address");
            topology
                .harness
                .peer(A)
                .rtc
                .wait_state(&topology.peer_id(remote), WAIT, |state| {
                    state.sendable && state.pc_generation == initial[&(A, remote)]
                })
                .await
                .expect("A controller must publish its recovered ICE generation");
            topology
                .harness
                .peer(remote)
                .rtc
                .wait_ice_generation(&topology.peer_id(A), initial_ice[&(remote, A)], WAIT)
                .await
                .expect("remote controller must advance its ICE generation");
            topology
                .harness
                .peer(remote)
                .rtc
                .wait_state(&topology.peer_id(A), WAIT, |state| {
                    state.sendable && state.pc_generation == initial[&(remote, A)]
                })
                .await
                .expect("remote controller must publish its recovered ICE generation");
        }
        topology.assert_all_links("mp03_restored").await;
    })
    .await
    .expect("all three controllers must recover within one topology deadline");
    let restored = topology.generations().await;
    let restored_ice = topology.ice_generations().await;
    for remote in REMOTES {
        assert_eq!(restored[&(A, remote)], initial[&(A, remote)]);
        assert_eq!(restored[&(remote, A)], initial[&(remote, A)]);
        assert!(restored_ice[&(A, remote)] > initial_ice[&(A, remote)]);
        assert!(restored_ice[&(remote, A)] > initial_ice[&(remote, A)]);
    }
    topology.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp04_ab_signaling_partition_does_not_starve_ac_or_ad_recovery() {
    let topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    let a = topology.peer_id(A);
    let b = topology.peer_id(B);
    topology.harness.signaling_faults.set_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::IceRestartOffer,
        SignalingFaultRule::Drop,
    );

    topology.background().await;
    topology.foreground(Duration::from_secs(5)).await;
    topology
        .harness
        .signaling_faults
        .wait_for_hits_after(&a, &b, SignalingMessageClass::IceRestartOffer, 0, WAIT)
        .await
        .expect("the A-B signaling partition must hit a restart offer");

    topology.wait_link(C).await;
    topology.wait_link(D).await;
    pair_rpc(&topology.harness, A, C, "mp04_ac_not_starved").await;
    pair_rpc(&topology.harness, A, D, "mp04_ad_not_starved").await;
    assert_eq!(
        topology
            .harness
            .peer(A)
            .rtc
            .generation(&topology.peer_id(C))
            .await,
        Some(initial[&(A, C)])
    );
    assert_eq!(
        topology
            .harness
            .peer(A)
            .rtc
            .generation(&topology.peer_id(D))
            .await,
        Some(initial[&(A, D)])
    );

    topology.harness.signaling_faults.heal_pair(&a, &b);
    topology
        .network_path(1, NetworkAvailability::Available, wifi())
        .await;
    topology.wait_link(B).await;
    pair_rpc(&topology.harness, A, B, "mp04_ab_restored").await;
    topology.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp05_close_peer_during_recovery_cannot_revive_it_or_affect_other_links() {
    let topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    let a = topology.peer_id(A);
    let b = topology.peer_id(B);
    assert!(topology.harness.rtc_faults.set_link_rule(
        (&a, initial[&(A, B)]),
        (&b, initial[&(B, A)]),
        LinkDirection::Both,
        PacketRule::Blackhole,
    ));
    topology.wait_link_fault_hit(B, &initial).await;
    let blocked = topology
        .harness
        .peer(A)
        .rtc
        .wait_state(&b, WAIT, |state| !state.sendable)
        .await
        .expect("A-B recovery must be driven by the real v0.20 callback");
    let blocked_hook = topology
        .harness
        .peer(A)
        .hooks
        .wait_state(&b, WAIT, |state| state.revision == blocked.revision)
        .await
        .expect("A's public hook must expose A-B recovery before close_peer");
    assert_eq!(blocked_hook, blocked);
    assert!(topology.harness.peer(A).rtc.close_peer(&b).await);
    assert!(
        topology
            .harness
            .rtc_faults
            .heal_link((&a, initial[&(A, B)]), (&b, initial[&(B, A)]),)
    );

    assert!(
        topology
            .harness
            .peer(A)
            .rtc
            .wait_peer_count(2, Duration::from_secs(10))
            .await,
        "closed A-B controller must stay removed after its stale callback"
    );
    pair_rpc(&topology.harness, A, C, "mp05_ac_healthy").await;
    pair_rpc(&topology.harness, A, D, "mp05_ad_healthy").await;
    assert_eq!(topology.harness.peer(A).rtc.peer_count().await, 2);
    topology.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mp06_close_all_converges_then_explicit_connect_creates_only_fresh_links() {
    let mut topology = MultiPeerTopology::new().await;
    let initial = topology.generations().await;
    topology.harness.peer(A).rtc.close_all().await;
    assert!(
        topology
            .harness
            .peer(A)
            .rtc
            .wait_peer_count(0, Duration::from_secs(10))
            .await,
        "A close_all must remove every local controller"
    );
    let closed = topology.harness.peer(A).rtc.observation().await;
    assert_eq!(closed.peer_count, 0);
    for remote in REMOTES {
        assert!(
            topology
                .harness
                .peer(A)
                .rtc
                .state(&topology.peer_id(remote))
                .await
                .is_none()
        );
        let remote_closed = topology
            .harness
            .peer(remote)
            .rtc
            .wait_state(&topology.peer_id(A), WAIT, |state| !state.sendable)
            .await
            .unwrap_or_else(|| panic!("remote {remote} must observe A's closed session"));
        let remote_hook = topology
            .harness
            .peer(remote)
            .hooks
            .wait_state(&topology.peer_id(A), WAIT, |state| {
                state.revision == remote_closed.revision
            })
            .await
            .unwrap_or_else(|| panic!("remote {remote} hook missed A's closed session"));
        assert_eq!(remote_hook, remote_closed);
    }

    for remote in REMOTES {
        topology.harness.connect(A, remote).await;
    }
    topology.assert_all_links("mp06_fresh_links").await;
    let rebuilt = topology.generations().await;
    for remote in REMOTES {
        assert!(rebuilt[&(A, remote)] > initial[&(A, remote)]);
        assert!(rebuilt[&(remote, A)] > initial[&(remote, A)]);
    }
    assert_eq!(topology.harness.peer(A).rtc.peer_count().await, 3);
    topology.shutdown().await;
}
