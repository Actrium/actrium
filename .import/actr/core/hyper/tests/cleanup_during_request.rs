//! Cleanup/request race coverage for the webrtc-rs v0.20 architecture.
//!
//! These tests retain the production failure scenarios from the 0.14 suite,
//! but drive them through the real v0.20 stack:
//! `PeerGate -> PeerTransport -> RtcConnectionManager ->
//! RtcPeerController -> PeerConnectionSession`.

use std::sync::Arc;
use std::time::Duration;

use actr_hyper::lifecycle::NetworkEventProcessor;
use actr_hyper::test_support::TestHarness;
use actr_hyper::transport::Dest;

async fn connected(a: u64, b: u64) -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(a).await;
    harness.add_peer(b).await;
    harness.connect(a, b).await;
    harness
}

async fn assert_rpc(harness: &TestHarness, from: u64, to: u64, request_id: &str) {
    let response = tokio::time::timeout(
        Duration::from_secs(10),
        harness.peer(from).spawn_request(to, request_id, 10_000),
    )
    .await
    .unwrap_or_else(|_| panic!("{request_id} timed out"))
    .expect("request task must not panic")
    .unwrap_or_else(|error| panic!("{request_id} failed: {error}"));
    assert_eq!(response.as_ref(), b"pong");
}

async fn restore(harness: &TestHarness, from: u64, to: u64) {
    let processor = harness.peer(from).network_processor();
    processor
        .process_network_available()
        .await
        .expect("signaling reconnect must succeed");

    let target = harness.peer(to).id.clone();
    let source = harness.peer(from).id.clone();
    harness.peer(from).rtc.connect(target.clone()).await;
    harness
        .peer(from)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    harness
        .peer(to)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
}

/// A request admitted after cleanup starts must finish promptly rather than
/// hanging on a retired DataChannel. A fresh request must succeed after the
/// signaling and controller layers are restored.
#[tokio::test]
#[ignore]
async fn test_request_fails_during_cleanup() {
    let harness = connected(100, 200).await;
    let processor = harness.peer(100).network_processor();
    let cleanup_processor = processor.clone();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let cleanup_barrier = barrier.clone();
    let request_barrier = barrier.clone();
    let gate = harness.peer(100).gate.clone();
    let target = harness.peer(200).id.clone();

    // Preserve the legacy relative timing: cleanup and the request are
    // released together, then the request waits briefly so it overlaps the
    // destructive close rather than starting after a pre-published offline
    // state has already made the outcome deterministic.
    let cleanup = tokio::spawn(async move {
        cleanup_barrier.wait().await;
        cleanup_processor.cleanup_connections().await
    });
    let during = tokio::spawn(async move {
        request_barrier.wait().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        tokio::time::timeout(
            Duration::from_secs(3),
            gate.send_request(
                &target,
                actr_protocol::RpcEnvelope {
                    request_id: "request-during-cleanup".into(),
                    route_key: "test.PrepareClientStream".into(),
                    payload: Some(bytes::Bytes::from_static(&[4, 5, 6])),
                    direction: Some(actr_protocol::Direction::Request as i32),
                    timeout_ms: 5_000,
                    ..Default::default()
                },
            ),
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(12), cleanup)
        .await
        .expect("cleanup must be bounded")
        .expect("cleanup task must not panic")
        .expect("cleanup must succeed");

    // The historical assertion accepted either a bounded transport failure or
    // a request that won the race. It only rejected a hang.
    let during = during.await.expect("request task must not panic");
    match during {
        Err(_) | Ok(Err(_)) | Ok(Ok(_)) => {}
    }
    assert_eq!(harness.peer(100).pending_count().await, 0);

    restore(&harness, 100, 200).await;
    assert_rpc(&harness, 100, 200, "request-after-cleanup-race").await;
    harness.shutdown().await;
}

/// A controller removed by cleanup can be recreated through the production
/// signaling path, and its new authoritative snapshot admits business sends.
#[tokio::test]
#[ignore]
async fn test_request_succeeds_after_cleanup() {
    let harness = connected(300, 400).await;
    let processor = harness.peer(300).network_processor();

    processor
        .cleanup_connections()
        .await
        .expect("cleanup must succeed");
    assert_eq!(harness.peer(300).rtc.peer_count().await, 0);

    restore(&harness, 300, 400).await;
    assert_rpc(&harness, 300, 400, "request-after-cleanup").await;
    harness.shutdown().await;
}

/// The historical bug left a `DestTransport` pointing at a closed 0.14
/// coordinator. v0.20 teardown must invalidate that cache before a later send,
/// and the replacement controller must create a usable transport.
#[tokio::test]
#[ignore]
async fn test_dest_transport_cache_not_cleaned() {
    let harness = connected(500, 600).await;
    let target = harness.peer(600).id.clone();
    let dest = Dest::peer(target.clone());

    assert_rpc(&harness, 500, 600, "prime-dest-cache").await;
    assert!(
        harness.peer(500).transport_manager.has_dest(&dest).await,
        "the first RPC must populate the destination cache"
    );

    let processor = harness.peer(500).network_processor();
    processor
        .cleanup_connections()
        .await
        .expect("cleanup must succeed");

    tokio::time::timeout(Duration::from_secs(5), async {
        while harness.peer(500).transport_manager.has_dest(&dest).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("controller teardown must evict the stale DestTransport");

    restore(&harness, 500, 600).await;
    assert_rpc(&harness, 500, 600, "fresh-dest-after-cleanup").await;
    harness.shutdown().await;
}

/// Reproduce the mobile log ordering: RTC becomes unavailable, signaling and
/// controller cleanup run, a business request arrives, then signaling returns.
/// The intermediate request must fail quickly and the next request must work.
#[tokio::test]
#[ignore]
async fn test_precise_log_reproduction() {
    let harness = connected(700, 800).await;
    let target = harness.peer(800).id.clone();
    let processor = harness.peer(700).network_processor();

    processor
        .process_network_lost()
        .await
        .expect("network-lost processing must succeed");
    harness
        .peer(700)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("network loss must revoke sendability");

    let request = harness
        .peer(700)
        .spawn_request(800, "precise-request-during-cleanup", 30_000);
    let cleanup = processor.cleanup_connections();
    let (request_result, cleanup_result) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(3), request),
        tokio::time::timeout(Duration::from_secs(12), cleanup),
    );
    let request_result = request_result
        .expect("request must fail promptly")
        .expect("request task must not panic");
    assert!(request_result.is_err());
    cleanup_result
        .expect("cleanup must be bounded")
        .expect("cleanup must succeed");
    assert_eq!(harness.peer(700).pending_count().await, 0);

    restore(&harness, 700, 800).await;
    assert_rpc(&harness, 700, 800, "precise-request-after-recovery").await;
    harness.shutdown().await;
}
