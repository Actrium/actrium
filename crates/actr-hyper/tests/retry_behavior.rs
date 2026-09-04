//! Integration tests for outbound behavior across the v0.20 RTC lifecycle.
//!
//! These are T3 tests: every request traverses `PeerGate -> PeerTransport ->
//! RtcConnectionManager -> RtcPeerController -> PeerConnectionSession` and
//! real WebSocket signaling. Recovery admission is derived only from the
//! authoritative controller snapshot.

use actr_framework::Bytes;
use actr_hyper::test_support::TestHarness;
use actr_protocol::{ActrError, Direction, PayloadType, RpcEnvelope};
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

async fn connected(vnet: bool) -> TestHarness {
    let mut harness = if vnet {
        TestHarness::with_vnet().await
    } else {
        TestHarness::new().await
    };
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.connect(100, 200).await;
    harness
}

async fn rpc(harness: &TestHarness, request_id: &str, timeout_ms: u32) -> Bytes {
    harness
        .peer(100)
        .spawn_request(200, request_id, timeout_ms)
        .await
        .expect("request task must not panic")
        .unwrap_or_else(|error| panic!("{request_id} failed: {error}"))
}

async fn rpc_eventually(harness: &TestHarness, request_id: &str, deadline: Duration) -> Bytes {
    let stop = tokio::time::Instant::now() + deadline;
    let mut attempt = 0;
    loop {
        attempt += 1;
        let id = format!("{request_id}-{attempt}");
        let result = tokio::time::timeout(
            Duration::from_secs(4),
            harness.peer(100).spawn_request(200, &id, 3_000),
        )
        .await;
        if let Ok(Ok(Ok(response))) = result {
            return response;
        }
        assert!(
            tokio::time::Instant::now() < stop,
            "{request_id} did not recover before {deadline:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn enter_recovering(harness: &TestHarness) {
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.network_unavailable();
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| {
            !state.sendable && state.status == actr_framework::WebRtcPeerStatus::Recovering
        })
        .await
        .expect("controller must publish Recovering/sendable=false");
}

async fn assert_connection_not_ready(
    handle: tokio::task::JoinHandle<actr_protocol::ActorResult<Bytes>>,
    label: &str,
) {
    let error = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap_or_else(|_| panic!("{label} did not fail promptly"))
        .unwrap_or_else(|error| panic!("{label} task panicked: {error}"))
        .unwrap_err();
    assert!(
        matches!(error, ActrError::ConnectionNotReady(_)),
        "{label} returned {error}, expected ConnectionNotReady"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_succeeds_after_wirepool_reconnect() {
    init_tracing();
    let harness = connected(true).await;
    let target = harness.peer(200).id.clone();

    harness.vnet.as_ref().unwrap().block_network();
    enter_recovering(&harness).await;
    harness.vnet.as_ref().unwrap().unblock_network();
    harness.peer(100).rtc.network_available();
    harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    assert_eq!(
        rpc(&harness, "wirepool-reconnect", 5_000).await.as_ref(),
        b"pong"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_fails_during_outage_then_succeeds_after_recovery() {
    init_tracing();
    let harness = connected(true).await;
    harness.simulate_disconnect();
    enter_recovering(&harness).await;
    assert_connection_not_ready(
        harness.peer(100).spawn_request(200, "outage-call", 30_000),
        "outage call",
    )
    .await;

    harness.simulate_reconnect();
    assert_eq!(
        rpc_eventually(&harness, "post-recovery", Duration::from_secs(20))
            .await
            .as_ref(),
        b"pong"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_calls_share_dest_transport() {
    init_tracing();
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    let target = harness.peer(100).id.clone();
    let gate = harness.peer(200).gate.clone();

    let mut calls = Vec::new();
    for index in 0..16 {
        let gate = gate.clone();
        let target = target.clone();
        calls.push(tokio::spawn(async move {
            gate.send_request(
                &target,
                RpcEnvelope {
                    request_id: format!("concurrent-{index}"),
                    route_key: "test.method".into(),
                    payload: Some(bytes::Bytes::from_static(b"request")),
                    direction: Some(Direction::Request as i32),
                    timeout_ms: 15_000,
                    ..Default::default()
                },
            )
            .await
        }));
    }
    for call in calls {
        assert_eq!(
            call.await
                .expect("call task")
                .expect("concurrent call")
                .as_ref(),
            b"pong"
        );
    }
    assert_eq!(harness.peer(200).transport_manager.dest_count().await, 1);
    assert_eq!(harness.peer(200).rtc.peer_count().await, 1);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_returns_promptly_after_connection_closed_cleanup() {
    init_tracing();
    let harness = connected(false).await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();

    let mut calls = Vec::new();
    for index in 0..32 {
        calls.push(
            harness
                .peer(100)
                .spawn_request(200, &format!("close-race-{index}"), 30_000),
        );
    }
    let close_a = harness.peer(100).rtc.close_peer(&target);
    let close_b = harness.peer(200).rtc.close_peer(&source);
    let (closed_a, closed_b) = tokio::join!(close_a, close_b);
    assert!(closed_a && closed_b);

    tokio::time::timeout(Duration::from_secs(8), async {
        for call in calls {
            // A request may already have completed, or teardown may fail it;
            // neither outcome may retain a 30-second pending tail.
            let _ = call.await.expect("request task must not panic");
        }
    })
    .await
    .expect("close cleanup must release every request promptly");
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stale_close_event_does_not_kill_pending_requests() {
    init_tracing();
    let harness = connected(false).await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old_generation = harness.peer(100).rtc.generation(&target).await.unwrap();

    harness.peer(100).rtc.force_reconnect();
    harness.peer(200).rtc.force_reconnect();
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old_generation
        })
        .await
        .expect("replacement generation");
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;

    // Old-generation handler and pump exits can now race this burst; none may
    // clean pending work owned by the replacement generation.
    let mut calls = Vec::new();
    for index in 0..16 {
        calls.push(harness.peer(100).spawn_request(
            200,
            &format!("after-stale-close-{index}"),
            8_000,
        ));
    }
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap().as_ref(), b"pong");
    }
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_tell_does_not_register_pending_requests() {
    init_tracing();
    let harness = connected(false).await;
    let target = harness.peer(200).id.clone();
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness
        .peer(100)
        .gate
        .send_message(
            &target,
            RpcEnvelope {
                request_id: "tell-no-pending".into(),
                route_key: "test.tell".into(),
                payload: Some(bytes::Bytes::from_static(b"fire-and-forget")),
                direction: Some(Direction::Tell as i32),
                ..Default::default()
            },
        )
        .await
        .expect("tell");
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_send_data_chunk_rejected_during_recovery() {
    init_tracing();
    let harness = connected(false).await;
    let target = harness.peer(200).id.clone();
    harness
        .peer(100)
        .gate
        .send_data_chunk(
            &target,
            PayloadType::StreamReliable,
            "healthy",
            bytes::Bytes::from_static(b"chunk"),
            "healthy",
        )
        .await
        .expect("healthy stream send");

    enter_recovering(&harness).await;
    let started = Instant::now();
    let error = harness
        .peer(100)
        .gate
        .send_data_chunk(
            &target,
            PayloadType::StreamReliable,
            "recovering",
            bytes::Bytes::from_static(b"chunk"),
            "recovering",
        )
        .await
        .expect_err("recovery must reject stream send");
    assert!(matches!(error, ActrError::ConnectionNotReady(_)));
    assert!(started.elapsed() < Duration::from_secs(1));
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_envelope_timeout_truncates_retry_backoff() {
    init_tracing();
    let harness = connected(true).await;
    // Keep the controller's last snapshot sendable while dropping all packets;
    // this exercises the request deadline rather than the recovery preflight.
    harness.vnet.as_ref().unwrap().block_network();
    let started = Instant::now();
    let result = harness
        .peer(100)
        .spawn_request(200, "short-envelope-timeout", 3_000)
        .await
        .expect("request task");
    let elapsed = started.elapsed();
    assert!(result.is_err());
    assert!(
        elapsed >= Duration::from_millis(2_500),
        "elapsed={elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(5), "elapsed={elapsed:?}");
    harness.vnet.as_ref().unwrap().unblock_network();
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_succeeds_after_full_disconnect_reconnect_cycle() {
    init_tracing();
    let harness = connected(true).await;
    harness.simulate_disconnect();
    enter_recovering(&harness).await;
    harness.simulate_reconnect();
    assert_eq!(
        rpc_eventually(&harness, "full-cycle", Duration::from_secs(20))
            .await
            .as_ref(),
        b"pong"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_fast_fails_during_recovery_window() {
    init_tracing();
    let harness = connected(false).await;
    enter_recovering(&harness).await;
    let started = Instant::now();
    assert_connection_not_ready(
        harness
            .peer(100)
            .spawn_request(200, "recovery-fast-fail", 30_000),
        "request during recovery",
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(1));
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_call_fast_fails_while_cleanup_owns_mutation_authority() {
    init_tracing();
    let harness = connected(false).await;
    // NetworkUnavailable is the production cleanup/recovery ownership fence:
    // the controller revokes permits before physical mutation starts.
    enter_recovering(&harness).await;
    let started = Instant::now();
    assert_connection_not_ready(
        harness
            .peer(100)
            .spawn_request(200, "cleanup-authority", 30_000),
        "request while cleanup owns the session",
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(1));
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_recovery_ready_hook_unblocks_mobile_retry() {
    init_tracing();
    let harness = connected(false).await;
    let target = harness.peer(200).id.clone();
    let mut states = harness
        .peer(100)
        .rtc
        .subscribe(&target)
        .await
        .expect("late subscription must replay current state");
    assert!(states.borrow().sendable, "initial replay must be sendable");

    enter_recovering(&harness).await;
    while states.borrow().status != actr_framework::WebRtcPeerStatus::Recovering {
        states.changed().await.expect("state stream");
    }
    assert!(!states.borrow().sendable);
    assert_connection_not_ready(
        harness
            .peer(100)
            .spawn_request(200, "mobile-guarded", 30_000),
        "mobile request while recovering",
    )
    .await;

    harness.peer(100).rtc.network_available();
    while !states.borrow().sendable {
        states.changed().await.expect("state stream");
    }
    assert_eq!(
        states.borrow().status,
        actr_framework::WebRtcPeerStatus::Connected
    );
    assert_eq!(
        rpc(&harness, "mobile-retry-after-ready", 5_000)
            .await
            .as_ref(),
        b"pong"
    );
    harness.shutdown().await;
}
