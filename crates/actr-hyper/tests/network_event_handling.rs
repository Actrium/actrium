#![allow(deprecated)]
//! Network-event integration tests over the RTC controller hierarchy.

use std::sync::Arc;
use std::time::Duration;

use actr_hyper::lifecycle::{
    NetworkAvailability, NetworkEvent, NetworkEventProcessor, NetworkSnapshot,
    NetworkTransportFlags, process_network_event_batch,
};
use actr_hyper::test_support::{
    TestHarness, install_test_crypto_provider, spawn_network_event_supervisor,
};
use actr_protocol::RpcEnvelope;

const WAIT: Duration = Duration::from_secs(30);

fn network_event(sequence: u64, available: bool, wifi: bool, cellular: bool) -> NetworkEvent {
    NetworkEvent::NetworkPathChanged {
        snapshot: NetworkSnapshot {
            sequence,
            availability: if available {
                NetworkAvailability::Available
            } else {
                NetworkAvailability::Unavailable
            },
            transport: NetworkTransportFlags {
                wifi,
                cellular,
                ethernet: false,
                vpn: false,
                other: false,
            },
            is_expensive: cellular,
            is_constrained: false,
        },
    }
}

async fn connected_harness(a: u64, b: u64) -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(a).await;
    harness.add_peer(b).await;
    harness.connect(a, b).await;
    harness
}

async fn request_ok(harness: &TestHarness, from: u64, to: u64, id: &str) {
    let response = harness
        .peer(from)
        .gate
        .send_request(
            &harness.peer(to).id,
            RpcEnvelope {
                request_id: id.to_string(),
                route_key: "test.ping".to_string(),
                payload: Some(bytes::Bytes::from_static(b"ping")),
                timeout_ms: 15_000,
                ..Default::default()
            },
        )
        .await
        .expect("RPC should succeed");
    assert_eq!(response.as_ref(), b"pong");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_network_available_triggers_recovery() {
    install_test_crypto_provider();
    let harness = connected_harness(100, 200).await;
    let target = harness.peer(200).id.clone();
    let old_generation = harness.peer(100).rtc.generation(&target).await.unwrap();

    harness.peer(100).rtc.network_unavailable();
    let results = process_network_event_batch(
        vec![network_event(1, true, true, false)],
        harness.peer(100).network_processor(),
    )
    .await;
    assert_eq!(results.len(), 1);
    assert!(results[0].success);

    let state = harness.peer(100).rtc.wait_sendable(&target, WAIT).await;
    assert_eq!(
        state.pc_generation, old_generation,
        "restore should use soft recovery"
    );
    request_ok(&harness, 100, 200, "available_recovered").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_network_lost_cleanup() {
    install_test_crypto_provider();
    let harness = connected_harness(300, 400).await;
    let target = harness.peer(400).id.clone();
    let processor = harness.peer(300).network_processor();

    processor
        .process_network_lost()
        .await
        .expect("network lost");
    let state = harness
        .peer(300)
        .rtc
        .wait_state(&target, WAIT, |state| !state.sendable)
        .await
        .expect("snapshot should gate sends");
    assert!(!state.sendable);
    assert!(!harness.peer(300).signaling_client.is_connected());
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_result_feedback_mechanism() {
    install_test_crypto_provider();
    let harness = connected_harness(500, 600).await;
    let processor: Arc<dyn NetworkEventProcessor> = harness.peer(500).network_processor();
    let (handle, _sink, shutdown, task) = spawn_network_event_supervisor(processor);

    let result = handle
        .handle_network_path_changed(match network_event(1, true, true, false) {
            NetworkEvent::NetworkPathChanged { snapshot } => snapshot,
            _ => unreachable!(),
        })
        .await
        .expect("feedback should arrive");
    assert!(result.success);
    assert!(result.duration_ms < 30_000);

    shutdown.cancel();
    task.await.expect("supervisor should stop");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_network_repeatedly_changing() {
    install_test_crypto_provider();
    let harness = connected_harness(700, 800).await;
    let target = harness.peer(800).id.clone();
    let old_generation = harness.peer(700).rtc.generation(&target).await.unwrap();

    // Preserve the original integration scenario: three distinct lost/restore
    // cycles. `process_network_event_batch` deliberately selects one action for
    // an entire batch, so collapsing all six facts into one call would test
    // only the final Restore action and silently remove the repeated recovery.
    for cycle in 0..3u64 {
        let lost = process_network_event_batch(
            vec![network_event(cycle * 2 + 1, false, false, false)],
            harness.peer(700).network_processor(),
        )
        .await;
        assert_eq!(lost.len(), 1);
        assert!(lost[0].success);
        harness
            .peer(700)
            .rtc
            .wait_state(&target, WAIT, |state| !state.sendable)
            .await
            .expect("lost fact must gate sends");

        let restored = process_network_event_batch(
            vec![network_event(cycle * 2 + 2, true, true, false)],
            harness.peer(700).network_processor(),
        )
        .await;
        assert_eq!(restored.len(), 1);
        assert!(restored[0].success);
        let state = harness.peer(700).rtc.wait_sendable(&target, WAIT).await;
        assert_eq!(
            state.pc_generation, old_generation,
            "temporary loss should recover on the same PeerConnection"
        );
    }
    request_ok(&harness, 700, 800, "repeated-network-recovery").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_manual_cleanup_connections() {
    install_test_crypto_provider();
    let harness = connected_harness(900, 1000).await;
    let processor = harness.peer(900).network_processor();

    processor.cleanup_connections().await.expect("cleanup");
    assert_eq!(harness.peer(900).rtc.peer_count().await, 0);
    assert_eq!(harness.peer(900).transport_manager.dest_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cleanup_then_network_events() {
    install_test_crypto_provider();
    let harness = connected_harness(1100, 1200).await;
    let processor = harness.peer(1100).network_processor();
    processor.cleanup_connections().await.expect("cleanup");

    // Restore reconnects signaling. A subsequent business send lazily creates
    // a fresh controller/session rather than reviving closed state.
    processor
        .process_network_available()
        .await
        .expect("restore signaling");
    request_ok(&harness, 1100, 1200, "cleanup_then_restore").await;
    assert_eq!(harness.peer(1100).rtc.peer_count().await, 1);
    harness.shutdown().await;
}
