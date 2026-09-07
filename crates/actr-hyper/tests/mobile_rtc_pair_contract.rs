//! PR-gate pair contract for mobile recovery through real Actrix services and
//! the production v0.20 lifecycle feed.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use actr_hyper::lifecycle::{CleanupReason, NetworkAvailability, NetworkTransportFlags};
use actr_hyper::test_support::{
    ConnectionInitiator, LinkDirection, MobileRtcPairHarness, PacketRule, PairEndpoint,
    PairSnapshot, SignalingFaultRule, SignalingMessageClass, TestRtcRole, WebRtcFragmentSendEvent,
    WebRtcFragmentSendHook, bindable_non_loopback_ipv4, install_webrtc_fragment_send_hook_for_test,
};
use actr_protocol::prost::Message as _;
use actr_protocol::{ActrError, DataChunk, PayloadType};
use tokio::sync::{Mutex as AsyncMutex, Notify, oneshot};

const WAIT: Duration = Duration::from_secs(30);

fn fragment_hook_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn pause_after_first_fragment() -> (
    actr_hyper::test_support::WebRtcFragmentSendHookGuard,
    oneshot::Receiver<WebRtcFragmentSendEvent>,
    Arc<Notify>,
) {
    let (event_tx, event_rx) = oneshot::channel();
    let event_tx = Arc::new(StdMutex::new(Some(event_tx)));
    let release = Arc::new(Notify::new());
    let hook: WebRtcFragmentSendHook = {
        let event_tx = event_tx.clone();
        let release = release.clone();
        Arc::new(move |event| {
            let event_tx = event_tx.clone();
            let release = release.clone();
            Box::pin(async move {
                if event.total_frags <= 1 || event.frag_index != 0 {
                    return;
                }
                let tx = { event_tx.lock().expect("fragment hook lock poisoned").take() };
                if let Some(tx) = tx {
                    let _ = tx.send(event);
                    release.notified().await;
                }
            })
        })
    };
    (
        install_webrtc_fragment_send_hook_for_test(hook),
        event_rx,
        release,
    )
}

/// A fresh physical connection must be initiated by the stable Offerer. The
/// Answerer only waits for this Offer after its request-only recovery attempt.
async fn reconnect_from_offerer(
    pair: &MobileRtcPairHarness,
    offerer_endpoint: PairEndpoint,
    answerer_endpoint: PairEndpoint,
) {
    tokio::time::sleep(Duration::from_secs(1)).await;
    let answerer = pair.id(answerer_endpoint);
    let offerer = pair.harness().peer(pair.serial(offerer_endpoint));
    let _ = offerer.rtc.close_peer(&answerer).await;
    offerer.rtc.connect(answerer).await;
}

fn stable_recovery_endpoints(snapshot: &PairSnapshot) -> (PairEndpoint, PairEndpoint) {
    match (snapshot.role_a, snapshot.role_b) {
        (TestRtcRole::Offerer, TestRtcRole::Answerer) => (PairEndpoint::A, PairEndpoint::B),
        (TestRtcRole::Answerer, TestRtcRole::Offerer) => (PairEndpoint::B, PairEndpoint::A),
        roles => panic!("pair must have one Offerer and one Answerer, got {roles:?}"),
    }
}

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

fn assert_bind_ip(addresses: &[std::net::SocketAddr], expected: Ipv4Addr) {
    assert!(
        !addresses.is_empty()
            && addresses
                .iter()
                .all(|address| address.ip() == IpAddr::V4(expected)),
        "v0.20 builder used unexpected bind addresses: {addresses:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p00_a_is_only_connect_initiator_and_pair_is_bidirectionally_usable() {
    // Production Actrix uses reverse ActorId ordering for the stable role
    // tie-break. Keep A above B when this scenario requires A to offer.
    let mut pair = MobileRtcPairHarness::new(51_002, 51_001).await;
    pair.establish(ConnectionInitiator::A).await;
    let snapshot = pair.wait_pair_ready(WAIT).await;

    assert_eq!(snapshot.role_a, TestRtcRole::Offerer);
    assert_eq!(snapshot.role_b, TestRtcRole::Answerer);
    assert!(
        pair.signaling_trace().iter().any(|event| {
            event.from == pair.id(PairEndpoint::A)
                && event.to == pair.id(PairEndpoint::B)
                && event.class == SignalingMessageClass::Offer
        }),
        "A must be the only initial offer producer"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    pair.assert_rpc_both_directions("p00_a_initiates").await;
    pair.assert_data_chunk_both_directions("p00_a_initiates")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p00_b_is_only_connect_initiator_and_a_is_real_answerer() {
    let mut pair = MobileRtcPairHarness::new(51_011, 51_012).await;
    pair.establish(ConnectionInitiator::B).await;
    let snapshot = pair.wait_pair_ready(WAIT).await;

    assert_eq!(snapshot.role_a, TestRtcRole::Answerer);
    assert_eq!(snapshot.role_b, TestRtcRole::Offerer);
    assert!(
        pair.signaling_trace().iter().any(|event| {
            event.from == pair.id(PairEndpoint::B)
                && event.to == pair.id(PairEndpoint::A)
                && event.class == SignalingMessageClass::Offer
        }),
        "B must be the only initial offer producer"
    );
    assert_eq!(pair.harness().peer(51_011).rtc.peer_count().await, 1);
    assert_eq!(pair.harness().peer(51_012).rtc.peer_count().await, 1);
    pair.assert_rpc_both_directions("p00_b_initiates").await;
    pair.assert_data_chunk_both_directions("p00_b_initiates")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p00_connect_initiator_can_be_answerer_under_actrix_arbitration() {
    let mut pair = MobileRtcPairHarness::new(51_013, 51_014).await;
    pair.establish(ConnectionInitiator::A).await;
    let snapshot = pair.wait_pair_ready(WAIT).await;

    assert_eq!(snapshot.role_a, TestRtcRole::Answerer);
    assert_eq!(snapshot.role_b, TestRtcRole::Offerer);
    assert!(
        pair.signaling_trace().iter().any(|event| {
            event.from == pair.id(PairEndpoint::B)
                && event.to == pair.id(PairEndpoint::A)
                && event.class == SignalingMessageClass::Offer
        }),
        "the passive B endpoint must offer after A loses Actrix arbitration"
    );
    pair.assert_rpc_both_directions("p00_initiator_is_answerer")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p01_a_only_short_background_uses_production_feed_and_soft_recovers() {
    let mut pair = MobileRtcPairHarness::new(51_022, 51_021).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let initial_policy_revision = pair.events(PairEndpoint::A).status().policy_revision;

    pair.background(PairEndpoint::A).await;
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&pair.id(PairEndpoint::B), WAIT, |state| !state.sendable)
        .await
        .expect("A must gate sends while backgrounded");
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .hooks
        .wait_state(&pair.id(PairEndpoint::B), WAIT, |state| !state.sendable)
        .await
        .expect("the production hook must expose A as not sendable");
    let blocked_send = tokio::time::timeout(
        Duration::from_secs(2),
        pair.rpc(PairEndpoint::A, PairEndpoint::B, "p01_blocked_send"),
    )
    .await
    .expect("business admission while gated must be bounded");
    assert!(
        blocked_send.is_err(),
        "mobile business must not admit a new send while the public state is not sendable"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .pending_count()
            .await,
        0
    );
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;

    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert!(
        pair.events(PairEndpoint::A).status().policy_revision > initial_policy_revision,
        "gated lifecycle policy must process the A-only events"
    );
    let ready_hook = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .hooks
        .wait_state(&pair.id(PairEndpoint::B), WAIT, |state| state.sendable)
        .await
        .expect("the production hook must expose A as sendable after recovery");
    assert_eq!(ready_hook.pc_generation, restored.a_to_b.pc_generation);
    let hook_states = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .hooks
        .state_changes_for(&pair.id(PairEndpoint::B));
    assert!(
        hook_states
            .windows(2)
            .all(|states| states[0].revision < states[1].revision),
        "public hook revisions must be strictly monotonic per peer"
    );
    pair.assert_rpc_both_directions("p01_short_background")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p02_a_only_lifecycle_with_bidirectional_link_blackhole_soft_recovers() {
    let mut pair = MobileRtcPairHarness::new(51_032, 51_031).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;

    pair.background(PairEndpoint::A).await;
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;

    pair.heal_rtc_faults().await;
    pair.network_path(PairEndpoint::A, 1, NetworkAvailability::Available, wifi())
        .await;
    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    pair.assert_rpc_both_directions("p02_soft_blackhole").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p03_a_only_long_background_preserves_duration_and_hard_rebuilds_pair() {
    let mut pair = MobileRtcPairHarness::new(51_042, 51_041).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;

    pair.background(PairEndpoint::A).await;
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(65))
        .await;
    let (offerer, answerer) = stable_recovery_endpoints(&initial);
    reconnect_from_offerer(&pair, offerer, answerer).await;
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&pair.id(PairEndpoint::B), WAIT, |state| {
            state.sendable && state.pc_generation > initial.a_to_b.pc_generation
        })
        .await
        .expect("A must complete its long-background replacement");
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_state(&pair.id(PairEndpoint::A), WAIT, |state| {
            state.sendable && state.pc_generation > initial.b_to_a.pc_generation
        })
        .await
        .expect("B must switch to A's long-background replacement");
    let rebuilt = pair.wait_pair_ready(WAIT).await;

    assert!(rebuilt.a_to_b.pc_generation > initial.a_to_b.pc_generation);
    assert!(rebuilt.b_to_a.pc_generation > initial.b_to_a.pc_generation);
    assert_eq!(pair.rtc_fault_scope_count(), 2);
    pair.assert_rpc_both_directions("p03_long_background").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p04_a_only_offline_online_with_real_link_loss_soft_recovers_both_ends() {
    let mut pair = MobileRtcPairHarness::new(51_052, 51_051).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;

    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;

    pair.heal_rtc_faults().await;
    pair.network_path(PairEndpoint::A, 2, NetworkAvailability::Available, wifi())
        .await;
    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    pair.assert_rpc_both_directions("p04_offline_online").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p05_a_address_provider_change_soft_recovers_after_transient_path_loss_without_mirroring_platform_fact_to_b()
 {
    let mut pair = MobileRtcPairHarness::new(51_062, 51_061).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let changed_address = bindable_non_loopback_ipv4();
    let initial_a_bound = pair.rtc_bound_addresses(PairEndpoint::A).await;
    let initial_b_bound = pair.rtc_bound_addresses(PairEndpoint::B).await;
    let initial_a_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .ice_generation(&pair.id(PairEndpoint::B))
        .await
        .expect("initial A ICE generation");
    let initial_b_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .ice_generation(&pair.id(PairEndpoint::A))
        .await
        .expect("initial B ICE generation");
    assert_bind_ip(
        &pair.rtc_bind_addresses(PairEndpoint::A).await,
        Ipv4Addr::LOCALHOST,
    );
    pair.network_path(PairEndpoint::A, 1, NetworkAvailability::Available, wifi())
        .await;
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;

    pair.harness()
        .set_local_addresses(pair.serial(PairEndpoint::A), vec![changed_address]);
    pair.network_path(
        PairEndpoint::A,
        2,
        NetworkAvailability::Available,
        cellular(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;
    let a_id = pair.id(PairEndpoint::A);
    let b_id = pair.id(PairEndpoint::B);
    let a_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_ice_generation(&b_id, initial_a_ice_generation, WAIT)
        .await
        .expect("A must attempt same-generation ICE recovery first");
    assert!(a_ice_generation > initial_a_ice_generation);
    let a_rebound_address = pair
        .wait_rtc_bound_address(
            PairEndpoint::A,
            initial_a_bound.len(),
            changed_address,
            WAIT,
        )
        .await;
    let a_rebound_address =
        a_rebound_address.expect("A ICE restart must bind a socket on the changed address");
    assert_eq!(a_rebound_address.ip(), changed_address);
    pair.heal_rtc_faults().await;
    let b_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_ice_generation(&a_id, initial_b_ice_generation, WAIT)
        .await
        .expect("B must apply the same-generation ICE restart");
    let b_rebound_address = pair
        .wait_rtc_bound_address(
            PairEndpoint::B,
            initial_b_bound.len(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            WAIT,
        )
        .await
        .expect("B ICE restart must bind a socket on its current address");
    assert_eq!(b_rebound_address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    let recovered = pair.wait_pair_ready(WAIT).await;

    assert_eq!(recovered.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(recovered.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert!(b_ice_generation > initial_b_ice_generation);
    assert_eq!(pair.rtc_fault_scope_count(), 2);
    pair.assert_rpc_both_directions("p05_address_change_soft")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p06_available_to_available_address_change_soft_recovers_without_offline_callback() {
    let mut pair = MobileRtcPairHarness::new(51_064, 51_063).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let changed_address = bindable_non_loopback_ipv4();
    let initial_a_bound = pair.rtc_bound_addresses(PairEndpoint::A).await;
    let initial_b_bound = pair.rtc_bound_addresses(PairEndpoint::B).await;
    let initial_a_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .ice_generation(&pair.id(PairEndpoint::B))
        .await
        .expect("initial A ICE generation");
    let initial_b_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .ice_generation(&pair.id(PairEndpoint::A))
        .await
        .expect("initial B ICE generation");

    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Available,
        cellular(),
    )
    .await;
    pair.harness()
        .set_local_addresses(pair.serial(PairEndpoint::A), vec![changed_address]);
    pair.network_path(PairEndpoint::A, 2, NetworkAvailability::Available, wifi())
        .await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;
    let a_id = pair.id(PairEndpoint::A);
    let b_id = pair.id(PairEndpoint::B);
    let (a_ice_generation, b_ice_generation) = tokio::join!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .wait_ice_generation(&b_id, initial_a_ice_generation, WAIT),
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .wait_ice_generation(&a_id, initial_b_ice_generation, WAIT),
    );
    assert!(
        a_ice_generation.is_some(),
        "A must detect its new address without an offline callback"
    );
    assert!(
        b_ice_generation.is_some(),
        "B must complete the same-generation ICE restart"
    );
    let (a_rebound_address, b_rebound_address) = tokio::join!(
        pair.wait_rtc_bound_address(
            PairEndpoint::A,
            initial_a_bound.len(),
            changed_address,
            WAIT,
        ),
        pair.wait_rtc_bound_address(
            PairEndpoint::B,
            initial_b_bound.len(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            WAIT,
        ),
    );
    assert_eq!(
        a_rebound_address
            .expect("A ICE restart must bind the changed address")
            .ip(),
        changed_address
    );
    assert_eq!(
        b_rebound_address
            .expect("B ICE restart must bind its current address")
            .ip(),
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    );
    let recovered = pair.wait_pair_ready(WAIT).await;

    assert_eq!(recovered.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(recovered.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert!(a_ice_generation.expect("A ICE generation") > initial_a_ice_generation);
    assert!(b_ice_generation.expect("B ICE generation") > initial_b_ice_generation);
    pair.assert_rpc_both_directions("p06_available_soft").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p07_complete_offline_requests_are_bounded_and_pair_recovers_bidirectionally() {
    let mut pair = MobileRtcPairHarness::new(51_066, 51_065).await;
    pair.establish(ConnectionInitiator::A).await;
    pair.wait_pair_ready(WAIT).await;
    let signaling_partition_baseline =
        pair.signaling_hits(PairEndpoint::A, PairEndpoint::B, SignalingMessageClass::Any);
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.set_signaling_fault(
        LinkDirection::Both,
        SignalingMessageClass::Any,
        SignalingFaultRule::Drop,
    );
    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;

    let a_to_b = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .spawn_request_with_route(
            pair.serial(PairEndpoint::B),
            "p07_offline_a_to_b",
            "test.ping",
            2_000,
        );
    let b_to_a = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .spawn_request_with_route(
            pair.serial(PairEndpoint::A),
            "p07_offline_b_to_a",
            "test.ping",
            2_000,
        );
    let (a_to_b, b_to_a) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(5), a_to_b),
        tokio::time::timeout(Duration::from_secs(5), b_to_a),
    );
    for (direction, outcome) in [("A->B", a_to_b), ("B->A", b_to_a)] {
        let outcome = outcome
            .unwrap_or_else(|_| panic!("offline {direction} request exceeded its deadline"))
            .unwrap_or_else(|error| panic!("offline {direction} request task panicked: {error}"));
        assert!(
            outcome.is_err(),
            "offline {direction} request must not report success"
        );
    }
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .pending_count()
            .await,
        0
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .pending_count()
            .await,
        0
    );

    pair.heal_rtc_faults().await;
    pair.network_path(PairEndpoint::A, 2, NetworkAvailability::Available, wifi())
        .await;
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::A),
            &pair.id(PairEndpoint::B),
            SignalingMessageClass::Any,
            signaling_partition_baseline,
            Duration::from_secs(10),
        )
        .await
        .expect("restored A must encounter the signaling partition");
    pair.heal_signaling_faults();
    pair.wait_pair_ready(WAIT).await;
    pair.assert_rpc_both_directions("p07_after_offline").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p08_b_detects_failure_before_a_platform_callback_without_duplicate_recovery() {
    let mut pair = MobileRtcPairHarness::new(51_072, 51_071).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.wait_not_sendable(PairEndpoint::B, WAIT).await;

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::B),
            &pair.id(PairEndpoint::A),
            SignalingMessageClass::IceRestartRequest,
            0,
            WAIT,
        )
        .await
        .expect("remote detector B must request recovery from offerer A");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::A),
            &pair.id(PairEndpoint::B),
            SignalingMessageClass::IceRestartOffer,
            0,
            WAIT,
        )
        .await
        .expect("A must start B's requested restart before A receives a platform callback");

    let trace_before_late_callback = pair.signaling_trace();
    let exchanges_before_late_callback = trace_before_late_callback
        .iter()
        .filter(|event| event.class == SignalingMessageClass::IceRestartOffer)
        .filter_map(|event| event.exchange_id.clone())
        .collect::<HashSet<_>>();
    assert_eq!(
        exchanges_before_late_callback.len(),
        1,
        "B-first detection must establish exactly one in-flight restart exchange"
    );

    pair.background(PairEndpoint::A).await;
    pair.heal_rtc_faults().await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;
    pair.network_path(PairEndpoint::A, 1, NetworkAvailability::Available, wifi())
        .await;
    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    let exchanges_after_recovery = pair
        .signaling_trace()
        .into_iter()
        .filter(|event| event.class == SignalingMessageClass::IceRestartOffer)
        .filter_map(|event| event.exchange_id)
        .collect::<HashSet<_>>();
    assert_eq!(
        exchanges_after_recovery, exchanges_before_late_callback,
        "the late A platform callback must coalesce into B's existing recovery exchange"
    );
    pair.assert_rpc_both_directions("p08_remote_first").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p09_a_answerer_requests_the_real_offerer_to_restart() {
    let mut pair = MobileRtcPairHarness::new(51_081, 51_082).await;
    pair.establish(ConnectionInitiator::B).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    let trace_baseline = pair.signaling_trace().len();
    let request_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartRequest,
    );
    let offer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );

    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;

    // Synchronize on the restart handshake itself, not on readiness.
    // `wait_pair_ready` is satisfied by whatever sendable snapshot is already
    // current, and the projection replays the latest value immediately, so it
    // returns the pre-background snapshot whenever the lifecycle fact has not
    // been projected yet — leaving the assertions below to race the recovery
    // they are meant to observe.
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::A),
            &pair.id(PairEndpoint::B),
            SignalingMessageClass::IceRestartRequest,
            request_baseline,
            WAIT,
        )
        .await
        .expect("answerer A must request its offerer B to restart");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::B),
            &pair.id(PairEndpoint::A),
            SignalingMessageClass::IceRestartOffer,
            offer_baseline,
            WAIT,
        )
        .await
        .expect("offerer B must produce the correlated restart offer");
    pair.wait_pair_ready(WAIT).await;

    let trace = pair.signaling_trace();
    let request_index = trace
        .iter()
        .enumerate()
        .skip(trace_baseline)
        .find_map(|(index, event)| {
            (event.from == pair.id(PairEndpoint::A)
                && event.to == pair.id(PairEndpoint::B)
                && event.class == SignalingMessageClass::IceRestartRequest)
                .then_some(index)
        })
        .expect("A restart request trace");
    let offer_index = trace
        .iter()
        .enumerate()
        .skip(request_index + 1)
        .find_map(|(index, event)| {
            (event.from == pair.id(PairEndpoint::B)
                && event.to == pair.id(PairEndpoint::A)
                && event.class == SignalingMessageClass::IceRestartOffer)
                .then_some(index)
        })
        .expect("B correlated restart offer trace");
    assert!(request_index < offer_index);
    assert!(trace[request_index].exchange_id.is_some());
    assert_eq!(trace[request_index].attempt, Some(1));
    assert!(trace[offer_index].exchange_id.is_some());
    assert!(trace[request_index].observed_after <= trace[offer_index].observed_after);
    pair.assert_rpc_both_directions("p09_answerer_restart")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p10_simultaneous_endpoint_recovery_converges_to_one_active_pair() {
    let mut pair = MobileRtcPairHarness::new(51_092, 51_091).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let trace_baseline = pair.signaling_trace().len();

    tokio::join!(
        pair.background(PairEndpoint::A),
        pair.background(PairEndpoint::B)
    );
    tokio::join!(
        pair.foreground(PairEndpoint::A, Duration::from_secs(5)),
        pair.foreground(PairEndpoint::B, Duration::from_secs(5))
    );
    let restored = pair.wait_pair_ready(WAIT).await;

    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    let recovery_trace = pair.signaling_trace();
    let restart_exchanges = recovery_trace[trace_baseline..]
        .iter()
        .filter(|event| event.class == SignalingMessageClass::IceRestartOffer)
        .filter_map(|event| event.exchange_id.clone())
        .collect::<HashSet<_>>();
    assert_eq!(
        restart_exchanges.len(),
        1,
        "simultaneous facts must converge to one effective restart exchange"
    );
    let request_ids = recovery_trace[trace_baseline..]
        .iter()
        .filter(|event| event.class == SignalingMessageClass::IceRestartRequest)
        .filter_map(|event| event.exchange_id.clone())
        .collect::<HashSet<_>>();
    assert!(
        request_ids.len() <= 1,
        "glare produced orphan restart requests"
    );
    pair.assert_rpc_both_directions("p10_simultaneous").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p11_signaling_partition_does_not_kill_an_established_data_plane() {
    let mut pair = MobileRtcPairHarness::new(51_094, 51_093).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let trace_baseline = pair.signaling_trace().len();
    pair.set_signaling_fault(
        LinkDirection::Both,
        SignalingMessageClass::Any,
        SignalingFaultRule::Drop,
    );

    pair.assert_rpc_both_directions("p11_signaling_partition")
        .await;
    pair.assert_data_chunk_both_directions("p11_signaling_partition")
        .await;
    let stable = pair.wait_pair_ready(WAIT).await;
    assert_eq!(stable.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(stable.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.signaling_trace().len(),
        trace_baseline,
        "an established data plane must not depend on signaling traffic"
    );

    pair.heal_signaling_faults();
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p12_dropped_soft_restart_offer_retries_on_the_same_peer_connection() {
    let mut pair = MobileRtcPairHarness::new(51_096, 51_095).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let trace_baseline = pair.signaling_trace().len();
    let restart_hit_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartOffer,
    );
    pair.set_signaling_fault(
        LinkDirection::AToB,
        SignalingMessageClass::IceRestartOffer,
        SignalingFaultRule::Drop,
    );

    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &pair.id(PairEndpoint::A),
            &pair.id(PairEndpoint::B),
            SignalingMessageClass::IceRestartOffer,
            restart_hit_baseline,
            WAIT,
        )
        .await
        .expect("the first soft restart offer must hit the injected drop");
    assert!(
        pair.signaling_trace()[trace_baseline..]
            .iter()
            .any(|event| {
                event.from == pair.id(PairEndpoint::A)
                    && event.to == pair.id(PairEndpoint::B)
                    && event.class == SignalingMessageClass::IceRestartOffer
                    && event.rule == SignalingFaultRule::Drop
            }),
        "the first soft restart offer must record the injected drop"
    );
    pair.heal_signaling_faults();
    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);

    let recovery_trace = pair.signaling_trace();
    let dropped_restart_exchanges = recovery_trace[trace_baseline..]
        .iter()
        .filter(|event| {
            event.class == SignalingMessageClass::IceRestartOffer
                && event.rule == SignalingFaultRule::Drop
        })
        .filter_map(|event| event.exchange_id.clone())
        .collect::<HashSet<_>>();
    assert_eq!(
        dropped_restart_exchanges.len(),
        1,
        "the injected fault must drop exactly the first soft exchange"
    );
    let restart_exchanges = recovery_trace[trace_baseline..]
        .iter()
        .filter(|event| event.class == SignalingMessageClass::IceRestartOffer)
        .filter_map(|event| event.exchange_id.clone())
        .collect::<HashSet<_>>();
    assert!(
        restart_exchanges.len() >= 2,
        "the attempt timeout must create a fresh soft exchange on the same PC"
    );
    assert_eq!(
        recovery_trace[trace_baseline..]
            .iter()
            .filter(|event| event.class == SignalingMessageClass::Offer)
            .count(),
        0,
        "a dropped soft offer must not trigger an ordinary hard-replacement offer"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    pair.assert_rpc_both_directions("p12_soft_retry").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p13_callback_storm_and_stale_monitor_epoch_do_not_regress_pair_state() {
    let mut pair = MobileRtcPairHarness::new(51_098, 51_097).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let trace_baseline = pair.signaling_trace().len();
    let old_monitor = pair.events(PairEndpoint::A).handle().clone();
    let new_monitor = old_monitor.new_monitor_incarnation();

    for sequence in 1..=8 {
        pair.events(PairEndpoint::A)
            .network_path_from_monitor(
                &new_monitor,
                "replacement-monitor",
                sequence,
                NetworkAvailability::Available,
                wifi(),
            )
            .await;
    }
    pair.events(PairEndpoint::A)
        .network_path_from_monitor(
            &old_monitor,
            "stale-monitor",
            u64::MAX,
            NetworkAvailability::Unavailable,
            NetworkTransportFlags::default(),
        )
        .await;
    for _ in 0..4 {
        pair.foreground(PairEndpoint::A, Duration::ZERO).await;
    }

    let stable = pair.wait_pair_ready(WAIT).await;
    assert_eq!(stable.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(stable.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert!(
        pair.signaling_trace()[trace_baseline..]
            .iter()
            .all(|event| {
                !matches!(
                    event.class,
                    SignalingMessageClass::Offer
                        | SignalingMessageClass::IceRestartOffer
                        | SignalingMessageClass::IceRestartRequest
                )
            }),
        "duplicate/stale platform facts must not start signaling recovery"
    );
    let hook_states = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .hooks
        .state_changes_for(&pair.id(PairEndpoint::B));
    assert!(
        hook_states
            .windows(2)
            .all(|states| states[0].revision < states[1].revision)
    );
    pair.assert_rpc_both_directions("p13_after_storm").await;
    pair.shutdown().await;
}

async fn p14_inflight_rpc_is_bounded_across_hard_recovery(
    from: PairEndpoint,
    to: PairEndpoint,
    a_serial: u64,
    b_serial: u64,
) {
    const REQUEST_ID: &str = "p14_inflight";
    let mut pair = MobileRtcPairHarness::new(a_serial, b_serial).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let request = pair
        .harness()
        .peer(pair.serial(from))
        .spawn_request_with_route(pair.serial(to), REQUEST_ID, "test.no_response", 30_000);
    assert!(
        pair.harness()
            .peer(pair.serial(to))
            .wait_request_count(REQUEST_ID, 1, Duration::from_secs(10))
            .await,
        "in-flight request must cross the real v0.20 lane before recovery"
    );

    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(65))
        .await;
    let (offerer, answerer) = stable_recovery_endpoints(&initial);
    reconnect_from_offerer(&pair, offerer, answerer).await;
    let outcome = tokio::time::timeout(Duration::from_secs(15), request)
        .await
        .expect("in-flight request must complete or fail within the recovery bound")
        .expect("request task must not panic");
    assert!(outcome.is_err(), "the no-response request cannot succeed");
    assert_eq!(
        pair.harness().peer(pair.serial(from)).pending_count().await,
        0,
        "recovery must release the request's pending entry"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(to))
            .request_count(REQUEST_ID),
        1,
        "recovery must not execute the in-flight RPC twice"
    );
    pair.wait_pair_ready(WAIT).await;
    pair.assert_rpc_both_directions("p14_after_recovery").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p14_a_to_b_inflight_rpc_is_bounded() {
    p14_inflight_rpc_is_bounded_across_hard_recovery(
        PairEndpoint::A,
        PairEndpoint::B,
        51_102,
        51_101,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p14_b_to_a_inflight_rpc_is_bounded() {
    p14_inflight_rpc_is_bounded_across_hard_recovery(
        PairEndpoint::B,
        PairEndpoint::A,
        51_112,
        51_111,
    )
    .await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InFlightDataOutcome {
    Delivered,
    DeliveryUncertain,
    TransportFailed,
}

async fn p15_inflight_fragmented_message_is_integral_or_bounded(
    from: PairEndpoint,
    to: PairEndpoint,
    a_serial: u64,
    b_serial: u64,
) {
    let _serial_guard = fragment_hook_lock().lock().await;
    let mut pair = MobileRtcPairHarness::new(a_serial, b_serial).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let payload = bytes::Bytes::from(
        (0..200 * 1024)
            .map(|index| ((index * 31 + 7) % 251) as u8)
            .collect::<Vec<_>>(),
    );
    let (hook_guard, paused_rx, release) = pause_after_first_fragment();
    let request = pair.spawn_large_echo(
        from,
        to,
        "p15_inflight_fragmented",
        payload.clone(),
        Duration::from_secs(20),
    );
    let paused = tokio::time::timeout(Duration::from_secs(30), paused_rx)
        .await
        .expect("large send must reach the fragmentation seam")
        .expect("fragment hook must remain installed");
    assert!(paused.total_frags > 1);

    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(65))
        .await;
    let (offerer, answerer) = stable_recovery_endpoints(&initial);
    reconnect_from_offerer(&pair, offerer, answerer).await;
    release.notify_waiters();
    drop(hook_guard);

    let outcome = tokio::time::timeout(Duration::from_secs(25), request)
        .await
        .expect("in-flight large message must succeed or fail within the recovery bound")
        .expect("large-message request task must not panic");
    let rpc_delivered = match outcome {
        Ok(response) => {
            assert_eq!(response.len(), payload.len());
            assert_eq!(response.as_ref(), payload.as_ref());
            true
        }
        Err(error) => {
            assert!(
                matches!(
                    &error,
                    ActrError::ConnectionNotReady(_)
                        | ActrError::Unavailable(_)
                        | ActrError::TimedOut
                        | ActrError::DeliveryUncertain { .. }
                ),
                "in-flight RPC returned an unexpected transport error: {error}"
            );
            false
        }
    };
    let remote_execution_count = pair
        .harness()
        .peer(pair.serial(to))
        .request_count("p15_inflight_fragmented");
    assert!(
        remote_execution_count <= 1,
        "large RPC executed more than once"
    );
    if rpc_delivered {
        assert_eq!(remote_execution_count, 1);
    }
    assert_eq!(
        pair.harness().peer(pair.serial(from)).pending_count().await,
        0,
        "in-flight fragmented send must not leave a permanent pending entry"
    );
    pair.wait_pair_ready(WAIT).await;

    let data_stream_id = "p15_inflight_data_chunk";
    let data_payload = bytes::Bytes::from(vec![0x5a; 200 * 1024]);
    let (data_hook_guard, data_paused_rx, data_release) = pause_after_first_fragment();
    let data_send = pair.spawn_data_chunk(from, to, data_stream_id, data_payload.clone());
    let data_paused = tokio::time::timeout(Duration::from_secs(30), data_paused_rx)
        .await
        .expect("large DataChunk must reach the fragmentation seam")
        .expect("DataChunk fragment hook must remain installed");
    assert!(data_paused.total_frags > 1);

    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(65))
        .await;
    reconnect_from_offerer(&pair, offerer, answerer).await;
    data_release.notify_waiters();
    drop(data_hook_guard);
    let data_outcome = tokio::time::timeout(Duration::from_secs(20), data_send)
        .await
        .expect("in-flight DataChunk send must be bounded")
        .expect("DataChunk send task must not panic");
    let delivery = match data_outcome {
        Err(error) if error.to_string().contains("delivery uncertain") => {
            InFlightDataOutcome::DeliveryUncertain
        }
        Err(_) => InFlightDataOutcome::TransportFailed,
        Ok(()) => {
            let received = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let (_, lane, encoded) = pair
                        .harness()
                        .peer(pair.serial(to))
                        .receive_payload()
                        .await
                        .expect("remote payload recorder closed");
                    let received = DataChunk::decode(encoded.as_ref())
                        .expect("in-flight DataChunk must never be corrupted");
                    if received.stream_id == data_stream_id {
                        return (lane, received);
                    }
                }
            })
            .await;
            match received {
                Ok((lane, received)) => {
                    assert_eq!(lane, PayloadType::StreamReliable);
                    assert_eq!(received.payload.as_ref(), data_payload.as_ref());
                    InFlightDataOutcome::Delivered
                }
                Err(_) => InFlightDataOutcome::DeliveryUncertain,
            }
        }
    };
    tracing::info!(?delivery, from = ?from, to = ?to, "bounded in-flight DataChunk outcome");
    pair.wait_pair_ready(WAIT).await;
    let data_delivery_count = pair
        .harness()
        .peer(pair.serial(to))
        .data_chunk_count(data_stream_id);
    assert!(
        data_delivery_count <= 1,
        "in-flight DataChunk was delivered more than once"
    );
    if delivery == InFlightDataOutcome::Delivered {
        assert_eq!(data_delivery_count, 1);
    }
    pair.assert_rpc_both_directions("p15_after_recovery").await;
    pair.assert_data_chunk_both_directions("p15_after_recovery")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p15_a_to_b_inflight_fragmented_message_is_bounded() {
    p15_inflight_fragmented_message_is_integral_or_bounded(
        PairEndpoint::A,
        PairEndpoint::B,
        51_122,
        51_121,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p15_b_to_a_inflight_fragmented_message_is_bounded() {
    p15_inflight_fragmented_message_is_integral_or_bounded(
        PairEndpoint::B,
        PairEndpoint::A,
        51_132,
        51_131,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p16_cleanup_then_endpoint_restart_rejects_old_handle_and_builds_a_new_pair() {
    let mut pair = MobileRtcPairHarness::new(51_142, 51_141).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let stale_handle = pair.events(PairEndpoint::A).handle().clone();
    let stale_hooks = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .hooks
        .clone();

    pair.cleanup(PairEndpoint::A, CleanupReason::AppTerminating)
        .await;
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_state(&pair.id(PairEndpoint::A), WAIT, |state| !state.sendable)
        .await
        .expect("B must observe the terminated A session through its real PC callback");
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .hooks
        .wait_state(&pair.id(PairEndpoint::A), WAIT, |state| !state.sendable)
        .await
        .expect("B's public hook must close business admission for terminated A");
    pair.restart_endpoint(PairEndpoint::A).await;
    assert!(
        stale_handle
            .handle_app_lifecycle_changed(actr_hyper::lifecycle::AppLifecycleState::Foreground {
                background_duration_ms: 0,
            })
            .await
            .is_err(),
        "a callback handle from the terminated endpoint must not reach the replacement"
    );
    let stale_hook_count = stale_hooks.events().len();

    pair.establish(ConnectionInitiator::A).await;
    let rebuilt = pair.wait_pair_ready(WAIT).await;
    // A was fully terminated and restarted with a new Manager, so its local
    // generation sequence legitimately starts over. Stale-handle/hook
    // isolation above is the cross-process freshness proof.
    assert!(rebuilt.a_to_b.pc_generation > 0);
    assert!(rebuilt.b_to_a.pc_generation > initial.b_to_a.pc_generation);
    assert_eq!(
        stale_hooks.events().len(),
        stale_hook_count,
        "the terminated endpoint's hook projection must remain stopped"
    );
    pair.assert_rpc_both_directions("p16_restarted_endpoint")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p17_unknown_path_probe_does_not_replace_a_healthy_pair() {
    let mut pair = MobileRtcPairHarness::new(51_152, 51_151).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let trace_baseline = pair.signaling_trace().len();

    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unknown,
        NetworkTransportFlags {
            vpn: true,
            ..wifi()
        },
    )
    .await;
    let probed = pair.wait_pair_ready(WAIT).await;
    assert_eq!(probed.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(probed.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.signaling_trace().len(),
        trace_baseline,
        "a successful Unknown-path probe must not renegotiate the RTC pair"
    );
    pair.assert_rpc_both_directions("p17_probe_success").await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p18_active_close_removes_old_pair_and_explicit_connect_rebuilds_it() {
    let mut pair = MobileRtcPairHarness::new(51_162, 51_161).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    assert!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .close_peer(&pair.id(PairEndpoint::B))
            .await
    );
    assert!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .wait_peer_count(0, WAIT)
            .await,
        "closed peer must be removed from A's manager registry"
    );
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_state(&pair.id(PairEndpoint::A), WAIT, |state| !state.sendable)
        .await
        .expect("B must close business admission after A actively closes the PC");
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .hooks
        .wait_state(&pair.id(PairEndpoint::A), WAIT, |state| !state.sendable)
        .await
        .expect("B's public hook must expose the remote close");

    pair.establish(ConnectionInitiator::A).await;
    let rebuilt = pair.wait_pair_ready(WAIT).await;
    assert!(rebuilt.a_to_b.pc_generation > initial.a_to_b.pc_generation);
    assert!(rebuilt.b_to_a.pc_generation > initial.b_to_a.pc_generation);
    pair.assert_rpc_both_directions("p18_explicit_reconnect")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p19_independent_endpoint_events_converge_without_creating_a_second_pair() {
    let mut pair = MobileRtcPairHarness::new(51_172, 51_171).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;

    pair.background(PairEndpoint::A).await;
    pair.network_path(
        PairEndpoint::B,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(4))
        .await;
    pair.network_path(PairEndpoint::B, 2, NetworkAvailability::Available, wifi())
        .await;

    let restored = pair.wait_pair_ready(WAIT).await;
    assert_eq!(restored.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .rtc
            .peer_count()
            .await,
        1
    );
    pair.assert_rpc_both_directions("p19_independent_events")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p20_screen_cycle_without_platform_fact_keeps_pair_usable() {
    let mut pair = MobileRtcPairHarness::new(51_182, 51_181).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    let a_policy_revision = pair.events(PairEndpoint::A).status().policy_revision;
    let b_policy_revision = pair.events(PairEndpoint::B).status().policy_revision;
    let trace_baseline = pair.signaling_trace().len();

    pair.assert_rpc_both_directions("p20_no_platform_fact")
        .await;
    pair.assert_data_chunk_both_directions("p20_no_platform_fact")
        .await;
    let stable = pair.wait_pair_ready(WAIT).await;
    assert_eq!(stable.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    assert_eq!(stable.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    assert_eq!(
        pair.events(PairEndpoint::A).status().policy_revision,
        a_policy_revision
    );
    assert_eq!(
        pair.events(PairEndpoint::B).status().policy_revision,
        b_policy_revision
    );
    assert_eq!(pair.signaling_trace().len(), trace_baseline);
    pair.shutdown().await;
}
