//! Disconnect/reconnect integration coverage for the RTC architecture.
//!
//! All scenarios retain their historical entry names while observing the new
//! authority: `RtcPeerSnapshot`, monotone PC generations, production signaling
//! and real DataChannel traffic. No synthetic 0.14 callback is injected.

#![allow(deprecated)]

use actr_hyper::lifecycle::{
    NetworkAvailability, NetworkEvent, NetworkEventProcessor as _, NetworkSnapshot,
    NetworkTransportFlags, process_network_event_batch,
};
use actr_hyper::test_support::{TestHarness, make_actor_id};
use actr_hyper::transport::Dest;
use actr_protocol::{
    ActrError, ActrId, Direction, RpcEnvelope, SignalingEnvelope, actr_relay, session_description,
    signaling_envelope,
};
use std::time::{Duration, Instant};

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_test_writer()
        .try_init()
        .ok();
}

fn ice_restart_offer_count_from(messages: &[SignalingEnvelope], source: &ActrId) -> usize {
    messages
        .iter()
        .filter(|envelope| {
            let Some(signaling_envelope::Flow::ActrRelay(relay)) = envelope.flow.as_ref() else {
                return false;
            };
            relay.source == *source
                && matches!(
                    relay.payload.as_ref(),
                    Some(actr_relay::Payload::SessionDescription(description))
                        if description.r#type
                            == session_description::Type::IceRestartOffer as i32
                )
        })
        .count()
}

async fn pair(vnet: bool, connect: bool) -> TestHarness {
    let mut harness = if vnet {
        TestHarness::with_vnet().await
    } else {
        TestHarness::new().await
    };
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    if connect {
        harness.connect(100, 200).await;
    }
    harness
}

async fn request_once(
    harness: &TestHarness,
    from: u64,
    to: u64,
    request_id: &str,
    timeout_ms: u32,
) -> actr_protocol::ActorResult<actr_framework::Bytes> {
    harness
        .peer(from)
        .spawn_request(to, request_id, timeout_ms)
        .await
        .expect("request task")
}

async fn request_ok(harness: &TestHarness, from: u64, to: u64, prefix: &str) {
    let stop = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut attempt = 0;
    loop {
        attempt += 1;
        if let Ok(Ok(Ok(response))) = tokio::time::timeout(
            Duration::from_secs(4),
            harness
                .peer(from)
                .spawn_request(to, &format!("{prefix}-{attempt}"), 3_000),
        )
        .await
        {
            assert_eq!(response.as_ref(), b"pong");
            return;
        }
        assert!(tokio::time::Instant::now() < stop, "{prefix}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn bidirectional(harness: &TestHarness, prefix: &str) {
    request_ok(harness, 100, 200, &format!("{prefix}-a-b")).await;
    request_ok(harness, 200, 100, &format!("{prefix}-b-a")).await;
}

async fn wait_not_sendable(harness: &TestHarness, from: u64, to: u64) {
    let target = harness.peer(to).id.clone();
    harness
        .peer(from)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("sendability must be revoked");
}

async fn restore(harness: &TestHarness) {
    harness.server.resume_forwarding();
    if let Some(vnet) = &harness.vnet {
        vnet.unblock_network();
    }
    harness.peer(100).rtc.network_available();
    harness.peer(200).rtc.network_available();
    bidirectional(harness, "restored").await;
}

fn wifi_event(sequence: u64) -> NetworkEvent {
    NetworkEvent::NetworkPathChanged {
        snapshot: NetworkSnapshot {
            sequence,
            availability: NetworkAvailability::Available,
            transport: NetworkTransportFlags {
                wifi: true,
                cellular: false,
                ethernet: false,
                vpn: false,
                other: false,
            },
            is_expensive: false,
            is_constrained: false,
        },
    }
}

async fn expect_connection_recovering(
    request: tokio::task::JoinHandle<actr_protocol::ActorResult<actr_framework::Bytes>>,
    label: &str,
) {
    let result = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .unwrap_or_else(|_| panic!("{label} did not fail fast"))
        .unwrap_or_else(|error| panic!("{label} task panicked: {error}"));
    let error = match result {
        Ok(response) => panic!(
            "{label} unexpectedly succeeded with {} response bytes",
            response.len()
        ),
        Err(error) => error,
    };
    assert!(
        matches!(
            error,
            ActrError::ConnectionNotReady(_) | ActrError::Unavailable(_) | ActrError::TimedOut
        ),
        "{label} returned an unexpected error: {error}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_wait_timeout_does_not_force_local_offer() {
    init_tracing();
    let harness = pair(false, true).await;
    let answerer = harness.peer(200).id.clone();
    harness.reset_counters();
    // Peer 200 is the answerer. A bare availability fact must request the
    // offerer to restart; it must not emit a local restart offer itself.
    harness.peer(200).rtc.network_unavailable();
    harness.peer(200).rtc.network_available();
    harness
        .wait_for_ice_restart_request_count(1, Duration::from_secs(5))
        .await;
    let messages = harness.server.received_messages().await;
    assert_eq!(ice_restart_offer_count_from(&messages, &answerer), 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_restore_batch_does_not_restart_never_ready_initial_session() {
    init_tracing();
    let harness = pair(false, false).await;
    harness.server.pause_forwarding();
    harness
        .peer(100)
        .rtc
        .connect(harness.peer(200).id.clone())
        .await;
    harness.reset_counters();
    harness.peer(100).rtc.network_available();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(harness.ice_restart_count(), 0);
    harness.server.resume_forwarding();
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_lost_then_restore_does_not_restart_never_ready_initial_session() {
    init_tracing();
    let harness = pair(false, false).await;
    harness.server.pause_forwarding();
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.connect(target).await;
    harness.reset_counters();
    harness.peer(100).rtc.network_unavailable();
    harness.peer(100).rtc.network_available();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(harness.ice_restart_count(), 0);
    harness.server.resume_forwarding();
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cleanup_then_restore_does_not_restart_fresh_initial_session() {
    init_tracing();
    let harness = pair(false, false).await;
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.connect(target.clone()).await;
    harness.peer(100).rtc.close_peer(&target).await;
    harness.reset_counters();
    harness.peer(100).rtc.network_available();
    assert_eq!(harness.peer(100).rtc.peer_count().await, 0);
    assert_eq!(harness.ice_restart_count(), 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_unreachable_peer_request_fails_bounded_and_clears_pending() {
    init_tracing();
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    let target = make_actor_id(999);
    let started = Instant::now();
    let error = harness
        .peer(100)
        .gate
        .send_request(
            &target,
            RpcEnvelope {
                request_id: "unreachable-peer".into(),
                route_key: "test.ping".into(),
                direction: Some(Direction::Request as i32),
                timeout_ms: 2_000,
                ..Default::default()
            },
        )
        .await
        .expect_err("unreachable peer");
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(matches!(
        error,
        ActrError::TimedOut | ActrError::ConnectionNotReady(_)
    ));
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_l2_signaling_unreachable_e2e_fails_bounded_without_pending_leak() {
    init_tracing();
    let harness = pair(false, false).await;
    harness.server.pause_forwarding();
    let result = tokio::time::timeout(
        Duration::from_secs(4),
        request_once(&harness, 100, 200, "signaling-unreachable", 2_000),
    )
    .await
    .expect("bounded signaling failure");
    assert!(result.is_err());
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.server.resume_forwarding();
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_l2_data_channel_not_ready_during_initial_connect_fails_bounded_then_recovers() {
    init_tracing();
    let harness = pair(false, false).await;
    harness.server.pause_forwarding();
    let first = request_once(&harness, 100, 200, "initial-not-ready", 2_000).await;
    assert!(first.is_err());
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.server.resume_forwarding();
    request_ok(&harness, 100, 200, "initial-ready-recovered").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_network_change_requests_offerer_ice_restart() {
    init_tracing();
    let harness = pair(false, true).await;
    harness.reset_counters();
    harness.peer(200).rtc.network_unavailable();
    harness.peer(200).rtc.network_available();
    harness
        .wait_for_ice_restart_request_count(1, Duration::from_secs(5))
        .await;
    request_ok(&harness, 200, 100, "answerer-restart-request").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_ice_restart_answer_does_not_unblock_before_connected() {
    init_tracing();
    let harness = pair(true, true).await;
    let source = harness.peer(100).id.clone();
    harness.vnet.as_ref().unwrap().block_network();
    harness.peer(200).rtc.network_unavailable();
    wait_not_sendable(&harness, 200, 100).await;
    harness.peer(200).rtc.network_available();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!harness.peer(200).rtc.is_sendable(&source).await);
    harness.vnet.as_ref().unwrap().unblock_network();
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(15))
        .await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stale_answerer_recovery_closes_old_session_on_deadline() {
    init_tracing();
    let harness = pair(false, true).await;
    let source = harness.peer(100).id.clone();
    let old = harness.peer(200).rtc.generation(&source).await.unwrap();
    harness.peer(200).rtc.force_reconnect();
    harness.peer(100).rtc.force_reconnect();
    let ready = harness
        .peer(200)
        .rtc
        .wait_state(&source, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old
        })
        .await
        .expect("new answerer generation");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness.peer(200).rtc.generation(&source).await,
        Some(ready.pc_generation)
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_duplicate_network_recovery_same_session_is_coalesced() {
    init_tracing();
    let harness = pair(false, true).await;
    let target = harness.peer(200).id.clone();
    let generation = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.reset_counters();
    harness.peer(100).rtc.network_unavailable();
    wait_not_sendable(&harness, 100, 200).await;
    for _ in 0..64 {
        harness.peer(100).rtc.network_available();
    }
    harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(15))
        .await;
    assert_eq!(
        harness.peer(100).rtc.generation(&target).await,
        Some(generation)
    );
    assert_eq!(harness.ice_restart_count(), 1);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_late_old_session_events_do_not_affect_new_session() {
    init_tracing();
    let harness = pair(false, true).await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.peer(100).rtc.force_reconnect();
    harness.peer(200).rtc.force_reconnect();
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old
        })
        .await
        .expect("replacement");
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    bidirectional(&harness, "late-old-events").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_both_peers_first_rpc_concurrently_is_bounded_then_mobile_restore_recovers() {
    init_tracing();
    let harness = pair(false, false).await;
    let first_a = request_once(&harness, 100, 200, "glare-a", 10_000);
    let first_b = request_once(&harness, 200, 100, "glare-b", 10_000);
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(first_a, first_b)
    })
    .await
    .expect("simultaneous first RPCs bounded");
    harness.peer(100).rtc.network_available();
    harness.peer(200).rtc.network_available();
    bidirectional(&harness, "glare-restored").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_signaling_restore_wakes_existing_restart_without_duplicate_offer() {
    init_tracing();
    let harness = pair(true, true).await;
    harness.reset_counters();
    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    restore(&harness).await;
    assert!(harness.ice_restart_count() <= 2);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_default_network_processor_mobile_restore_wakes_existing_restart_retry() {
    init_tracing();
    let harness = pair(true, true).await;
    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    harness.server.resume_forwarding();
    harness.vnet.as_ref().unwrap().unblock_network();
    harness
        .peer(100)
        .network_processor()
        .process_network_available()
        .await
        .expect("processor restore");
    bidirectional(&harness, "processor-restore").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_network_recovery_deadline_closes_transport_without_business_traffic() {
    init_tracing();
    let harness = pair(true, true).await;
    let target = harness.peer(200).id.clone();
    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    assert!(harness.peer(100).rtc.close_peer(&target).await);
    assert!(harness.peer(100).rtc.state(&target).await.is_none());
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_connection_closed_clears_recovery_guard_when_transport_already_removed() {
    init_tracing();
    let harness = pair(false, true).await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    harness.peer(100).rtc.network_unavailable();
    harness.peer(100).rtc.close_peer(&target).await;
    harness.peer(200).rtc.close_peer(&source).await;
    harness.peer(100).rtc.connect(target.clone()).await;
    harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    request_ok(&harness, 100, 200, "guard-cleared").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_data_channel_on_close_cleans_webrtc_transport() {
    init_tracing();
    let harness = pair(false, true).await;
    let target = harness.peer(200).id.clone();
    let dest = Dest::peer(target.clone());
    // Materialize the transport-layer projection around the already-sendable
    // controller; `TestHarness::connect` intentionally establishes only the
    // production RTC control plane.
    request_ok(&harness, 100, 200, "anchor-close-seed").await;
    assert!(harness.peer(100).transport_manager.has_dest(&dest).await);
    let old_generation = harness
        .peer(100)
        .rtc
        .close_lane(&target, actr_protocol::PayloadType::RpcReliable)
        .await
        .expect("close active RpcReliable lane");
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| {
            state.status == actr_framework::WebRtcPeerStatus::Recovering && !state.sendable
        })
        .await
        .expect("anchor close must revoke sendability");
    let recovered = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old_generation
        })
        .await
        .expect("anchor close must rebuild the physical session");
    assert!(recovered.pc_generation > old_generation);

    // RTC keeps the logical DestTransport/controller projection and swaps
    // its generation-bound lane underneath it; the next RPC proves the cache
    // is not stale after the old DataChannel's OnClose event.
    assert!(harness.peer(100).transport_manager.has_dest(&dest).await);
    request_ok(&harness, 100, 200, "anchor-close-recovered").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_two_peer_disconnect_reconnect() {
    init_tracing();
    let harness = pair(true, true).await;
    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    wait_not_sendable(&harness, 200, 100).await;
    restore(&harness).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_lost_available_type_changed_batch_restores_webrtc_end_to_end() {
    init_tracing();
    let harness = pair(true, true).await;
    let processor = harness.peer(100).network_processor();
    harness.simulate_disconnect();
    processor.process_network_lost().await.expect("lost");
    harness.server.resume_forwarding();
    harness.vnet.as_ref().unwrap().unblock_network();
    processor
        .process_network_available()
        .await
        .expect("available");
    processor
        .process_network_type_changed(true, false)
        .await
        .expect("type changed");
    bidirectional(&harness, "lost-available-type").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cleanup_available_type_changed_batch_rebuilds_webrtc_end_to_end() {
    init_tracing();
    let harness = pair(false, true).await;
    let processor = harness.peer(100).network_processor();
    processor.cleanup_connections().await.expect("cleanup");
    assert_eq!(harness.peer(100).rtc.peer_count().await, 0);
    processor
        .process_network_available()
        .await
        .expect("available");
    processor
        .process_network_type_changed(true, false)
        .await
        .expect("type changed");
    request_ok(&harness, 100, 200, "cleanup-rebuild").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cleanup_during_ice_restart_prioritizes_cleanup_then_mobile_restore() {
    init_tracing();
    let harness = pair(true, true).await;
    let target = harness.peer(200).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.simulate_disconnect();
    harness.peer(100).rtc.network_available();
    harness.peer(100).rtc.close_peer(&target).await;
    harness.server.resume_forwarding();
    harness.vnet.as_ref().unwrap().unblock_network();
    harness.peer(100).rtc.connect(target.clone()).await;
    let current = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    assert!(current.pc_generation > old);
    request_ok(&harness, 100, 200, "cleanup-during-restart").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_signaling_reconnect_overlapping_cleanup_does_not_revive_old_transport() {
    init_tracing();
    let harness = pair(true, true).await;
    let target = harness.peer(200).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.simulate_disconnect();
    harness.peer(100).rtc.close_peer(&target).await;
    restore(&harness).await;
    let current = harness.peer(100).rtc.state(&target).await.unwrap();
    assert!(current.pc_generation > old);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_both_peers_simultaneous_mobile_restore_has_bounded_offer_count() {
    init_tracing();
    let harness = pair(true, true).await;
    harness.reset_counters();
    harness.simulate_disconnect();
    harness.server.resume_forwarding();
    harness.vnet.as_ref().unwrap().unblock_network();
    harness.peer(100).rtc.network_available();
    harness.peer(200).rtc.network_available();
    bidirectional(&harness, "simultaneous-restore").await;
    assert!(harness.ice_restart_count() <= 2);
    assert!(harness.server.get_ice_restart_request_count() <= 2);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_offerer_recovery_latency() {
    init_tracing();
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    // Preserve the legacy topology: peer 100 is the offerer and peer 200 is
    // the request sender whose end-to-end recovery is measured.
    harness.connect(200, 100).await;
    harness.reset_counters();

    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    wait_not_sendable(&harness, 200, 100).await;
    tokio::time::sleep(Duration::from_secs(8)).await;
    let outage_restart_count = harness.ice_restart_count();

    let recovery_start = Instant::now();
    harness.simulate_reconnect();
    harness.peer(100).retry_failed().await;
    request_ok(&harness, 200, 100, "offerer-recovery").await;
    assert!(recovery_start.elapsed() < Duration::from_secs(30));
    assert!(harness.ice_restart_count() >= outage_restart_count);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_recovery_latency() {
    init_tracing();
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.connect(200, 100).await;
    harness.reset_counters();

    harness.simulate_disconnect();
    wait_not_sendable(&harness, 100, 200).await;
    wait_not_sendable(&harness, 200, 100).await;
    tokio::time::sleep(Duration::from_secs(8)).await;
    let outage_restart_count = harness.ice_restart_count();

    let recovery_start = Instant::now();
    harness.simulate_reconnect();
    harness.peer(200).retry_failed().await;
    request_ok(&harness, 200, 100, "answerer-recovery").await;
    assert!(recovery_start.elapsed() < Duration::from_secs(30));
    assert!(harness.ice_restart_count() >= outage_restart_count);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "slow VNet recovery regression test"]
async fn repro_network_event_returns_before_webrtc_ready_causing_early_rpc_timeouts() {
    init_tracing();
    const CLIENT: u64 = 100;
    const SERVER: u64 = 200;

    let harness = pair(true, true).await;
    harness.reset_counters();
    harness.vnet.as_ref().unwrap().block_network();
    harness.server.pause_forwarding();
    harness.peer(CLIENT).rtc.network_unavailable();
    wait_not_sendable(&harness, CLIENT, SERVER).await;

    assert!(harness.peer(CLIENT).signaling_client.is_connected());
    let event_started = Instant::now();
    let results = process_network_event_batch(
        vec![wifi_event(1)],
        harness.peer(CLIENT).network_processor(),
    )
    .await;
    assert!(
        results.iter().all(|result| result.success),
        "NetworkEvent path change should report success: {results:?}"
    );
    assert!(event_started.elapsed() < Duration::from_secs(3));
    assert!(harness.peer(CLIENT).signaling_client.is_connected());

    let early_1 = harness
        .peer(CLIENT)
        .spawn_request(SERVER, "network-event-early-timeout-1", 500);
    let early_2 = harness
        .peer(CLIENT)
        .spawn_request(SERVER, "network-event-early-timeout-2", 800);
    expect_connection_recovering(early_1, "first immediate RPC").await;
    expect_connection_recovering(early_2, "second immediate RPC").await;
    assert_eq!(harness.peer(CLIENT).pending_count().await, 0);

    harness.simulate_reconnect();
    request_ok(&harness, CLIENT, SERVER, "network-event-late-success").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "log-timeline foreground cleanup/request-overlap regression"]
async fn regression_log_timeline_foreground_cleanup_fails_fast_before_new_send() {
    init_tracing();
    let harness = pair(false, true).await;
    harness.peer(100).rtc.network_unavailable();
    wait_not_sendable(&harness, 100, 200).await;
    let started = Instant::now();
    let result = request_once(&harness, 100, 200, "cleanup-fast-fail", 30_000).await;
    assert!(matches!(result, Err(ActrError::ConnectionNotReady(_))));
    assert!(started.elapsed() < Duration::from_secs(1));
    harness.shutdown().await;
}
