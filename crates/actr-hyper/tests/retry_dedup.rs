//! Retry dedup and request-id semantics on the v0.20 production path.

use actr_hyper::test_support::TestHarness;
use actr_protocol::{ActrError, Direction, RpcEnvelope};
use std::time::Duration;

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_test_writer()
        .try_init()
        .ok();
}

async fn harness() -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_pending_request_id_registration() {
    init_tracing();
    let harness = harness().await;
    let gate = harness.peer(100).gate.clone();
    let _rx = gate
        .register_pending_for_test("dedup-test-1", harness.peer(200).id.clone())
        .await;
    assert_eq!(gate.pending_count().await, 1);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_two_pending_requests_have_distinct_request_ids() {
    init_tracing();
    let harness = harness().await;
    let gate = harness.peer(100).gate.clone();
    let target = harness.peer(200).id.clone();
    let _first = gate
        .register_pending_for_test("distinct-1", target.clone())
        .await;
    let _second = gate.register_pending_for_test("distinct-2", target).await;
    assert_eq!(gate.pending_count().await, 2);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_response_deduplicates_by_request_id() {
    init_tracing();
    let harness = harness().await;
    let gate = harness.peer(100).gate.clone();
    let response = gate
        .register_pending_for_test("dedup-response", harness.peer(200).id.clone())
        .await;
    assert!(
        gate.handle_response("dedup-response", Ok(bytes::Bytes::from_static(b"first")))
            .await
            .unwrap()
    );
    assert!(
        !gate
            .handle_response(
                "dedup-response",
                Ok(bytes::Bytes::from_static(b"duplicate"))
            )
            .await
            .unwrap()
    );
    assert_eq!(response.await.unwrap().unwrap().as_ref(), b"first");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_late_response_after_connection_closed_is_dropped() {
    init_tracing();
    let harness = harness().await;
    let gate = harness.peer(100).gate.clone();
    let request_id = "late-response";
    let response = gate
        .register_pending_for_test(request_id, harness.peer(200).id.clone())
        .await;

    // The v0.20 lifecycle projects terminal/recovery failure into the same
    // response correlator. Once the first terminal result consumes the id, a
    // physically late response from a retired generation is a no-op.
    assert!(
        gate.handle_response(
            request_id,
            Err(ActrError::Unavailable("connection closed".into())),
        )
        .await
        .unwrap()
    );
    assert!(
        !gate
            .handle_response(request_id, Ok(bytes::Bytes::from_static(b"late")))
            .await
            .unwrap()
    );
    assert!(response.await.unwrap().is_err());
    assert_eq!(gate.pending_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_successful_send_does_not_retry_and_uses_transport_msg_id() {
    init_tracing();
    let mut harness = harness().await;
    harness.connect(100, 200).await;
    let target = harness.peer(200).id.clone();
    let before_restarts = harness.ice_restart_count();

    let response = harness
        .peer(100)
        .gate
        .send_request(
            &target,
            RpcEnvelope {
                request_id: "one-successful-send".into(),
                route_key: "test.ping".into(),
                payload: Some(bytes::Bytes::from_static(b"request")),
                direction: Some(Direction::Request as i32),
                timeout_ms: 5_000,
                ..Default::default()
            },
        )
        .await
        .expect("healthy request");
    assert_eq!(response.as_ref(), b"pong");
    assert_eq!(harness.peer(100).pending_count().await, 0);
    assert_eq!(harness.ice_restart_count(), before_restarts);
    // Header msg-id monotonicity and single-attempt framing are asserted at
    // the T1/T2 seam in `peer::send_tests`; this T3 assertion proves the
    // healthy path does not enter recovery/retry or duplicate business reply.
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tell_has_no_pending_response_channel() {
    init_tracing();
    let mut harness = harness().await;
    harness.connect(100, 200).await;
    let gate = harness.peer(100).gate.clone();
    let request_id = "tell-no-response";
    gate.send_message(
        &harness.peer(200).id,
        RpcEnvelope {
            request_id: request_id.into(),
            route_key: "test.tell".into(),
            payload: Some(bytes::Bytes::from_static(b"tell")),
            direction: Some(Direction::Tell as i32),
            ..Default::default()
        },
    )
    .await
    .expect("tell");
    assert_eq!(gate.pending_count().await, 0);
    assert!(
        !gate
            .handle_response(request_id, Ok(bytes::Bytes::from_static(b"unexpected")))
            .await
            .unwrap()
    );
    tokio::time::timeout(Duration::from_secs(2), harness.shutdown())
        .await
        .expect("bounded shutdown");
}
