//! PeerGate connection establishment, teardown, and rebuild integration tests.
//!
//! All scenarios use the production webrtc-rs v0.20 layering and real
//! WebSocket signaling. No synthetic 0.14 `ConnectionEvent` is injected.

use std::time::Duration;

use actr_hyper::test_support::TestHarness;
use actr_protocol::{Direction, RpcEnvelope};

async fn connected() -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.connect(100, 200).await;
    harness
}

async fn rpc(harness: &TestHarness, request_id: &str) {
    let response = tokio::time::timeout(
        Duration::from_secs(10),
        harness.peer(100).spawn_request(200, request_id, 10_000),
    )
    .await
    .unwrap_or_else(|_| panic!("{request_id} timed out"))
    .expect("request task must not panic")
    .unwrap_or_else(|error| panic!("{request_id} failed: {error}"));
    assert_eq!(response.as_ref(), b"pong");
}

#[tokio::test]
async fn test_peer_connection_establishment() {
    let harness = connected().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();

    let offerer = harness
        .peer(100)
        .rtc
        .state(&target)
        .await
        .expect("offerer snapshot");
    let answerer = harness
        .peer(200)
        .rtc
        .state(&source)
        .await
        .expect("answerer snapshot");
    assert!(offerer.sendable && answerer.sendable);
    assert!(offerer.pc_generation > 0 && answerer.pc_generation > 0);
    rpc(&harness, "establishment-probe").await;
    harness.shutdown().await;
}

#[tokio::test]
async fn test_connection_rebuild_after_failure() {
    let harness = connected().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old_generation = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.reset_counters();

    // Preserve the legacy trigger: a failed PeerConnection callback, followed
    // by the network-online retry hint. The compatibility seam publishes the
    // v0.20 `PeerConnectionStateChanged(Failed)` fact through the exact mailbox
    // used by `PeerConnectionEventHandler`.
    assert_eq!(
        harness.peer(100).rtc.fail_peer(&target).await.unwrap(),
        old_generation
    );
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("failed callback must invalidate the old sendable snapshot");
    harness.peer(100).retry_failed().await;
    harness
        .wait_for_ice_restart_count(1, Duration::from_secs(20))
        .await;
    let recovered = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| state.sendable)
        .await
        .expect("failed connection must become sendable after ICE restart");
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    assert_eq!(
        recovered.pc_generation, old_generation,
        "an Offerer soft recovery should keep the existing PeerConnection generation"
    );
    rpc(&harness, "rebuild-probe").await;
    harness.shutdown().await;
}

#[tokio::test]
async fn test_pending_requests_cleanup_on_close() {
    let harness = connected().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let gate = harness.peer(100).gate.clone();

    // The harness deliberately suppresses the business response for this
    // route, leaving a real request pending after it crossed the DataChannel.
    let request_target = target.clone();
    let request = tokio::spawn(async move {
        gate.send_request(
            &request_target,
            RpcEnvelope {
                request_id: "pending-cleanup-on-close".into(),
                route_key: "test.no_response".into(),
                payload: Some(bytes::Bytes::from_static(b"pending")),
                direction: Some(Direction::Request as i32),
                timeout_ms: 30_000,
                ..Default::default()
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), async {
        while harness.peer(100).pending_count().await != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("request must enter the pending map");

    let (closed_a, closed_b) = tokio::join!(
        harness.peer(100).rtc.close_peer(&target),
        harness.peer(200).rtc.close_peer(&source),
    );
    assert!(closed_a && closed_b);

    let result = tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .expect("controller close must release pending requests")
        .expect("request task must not panic");
    assert!(
        result.is_err(),
        "a closed request cannot produce a response"
    );
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test]
async fn test_reconnect_and_send_after_close() {
    let harness = connected().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let retired_generation = harness.peer(100).rtc.generation(&target).await.unwrap();

    let (closed_a, closed_b) = tokio::join!(
        harness.peer(100).rtc.close_peer(&target),
        harness.peer(200).rtc.close_peer(&source),
    );
    assert!(closed_a && closed_b);
    assert!(harness.peer(100).rtc.state(&target).await.is_none());

    harness.peer(100).rtc.connect(target.clone()).await;
    let replacement = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    assert!(
        replacement.pc_generation > retired_generation,
        "manager tombstones must prevent generation reuse"
    );

    rpc(&harness, "request-after-explicit-close").await;
    harness.shutdown().await;
}
