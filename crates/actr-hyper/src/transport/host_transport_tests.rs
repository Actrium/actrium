use super::*;
use actr_protocol::Classify;
use std::time::Duration;

fn envelope(request_id: &str) -> RpcEnvelope {
    RpcEnvelope {
        request_id: request_id.to_string(),
        route_key: "echo".to_string(),
        payload: Some(Bytes::from_static(b"hi")),
        ..Default::default()
    }
}

#[tokio::test]
async fn new_transport_has_reliable_lane() {
    let t = HostTransport::new();
    let lane = t
        .get_lane(PayloadType::RpcReliable, None)
        .await
        .expect("reliable lane must exist");
    // Cache: second call returns same lane (Arc identity).
    let lane2 = t.get_lane(PayloadType::RpcReliable, None).await.unwrap();
    assert!(Arc::ptr_eq(&lane, &lane2));
}

#[tokio::test]
async fn signal_lane_created_lazily_and_cached() {
    let t = HostTransport::new();
    let lane = t
        .get_lane(PayloadType::RpcSignal, None)
        .await
        .expect("signal lane should be created on demand");
    let lane2 = t.get_lane(PayloadType::RpcSignal, None).await.unwrap();
    assert!(Arc::ptr_eq(&lane, &lane2));
}

#[tokio::test]
async fn stream_lane_requires_channel_id() {
    let t = HostTransport::new();
    let err = t
        .get_lane(PayloadType::StreamLatencyFirst, None)
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::InvalidArgument(_)));
}

#[tokio::test]
async fn stream_lane_missing_channel_errors() {
    let t = HostTransport::new();
    let err = t
        .get_lane(
            PayloadType::StreamLatencyFirst,
            Some("never-created".into()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::ChannelNotFound(_)));
}

#[tokio::test]
async fn media_lane_requires_track_id_and_errors_when_missing() {
    let t = HostTransport::new();
    let err = t.get_lane(PayloadType::MediaRtp, None).await.unwrap_err();
    assert!(matches!(err, NetworkError::InvalidArgument(_)));

    let err = t
        .get_lane(PayloadType::MediaRtp, Some("no-track".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::ChannelNotFound(_)));
}

#[tokio::test]
async fn send_message_delivers_to_reliable_lane() {
    let t = HostTransport::new();
    // send_message resolves Ok on the reliable lane (get_lane + send_envelope).
    t.send_message(PayloadType::RpcReliable, None, envelope("r1"))
        .await
        .unwrap();
}

#[tokio::test]
async fn send_message_fails_for_unknown_stream_channel() {
    let t = HostTransport::new();
    let err = t
        .send_message(
            PayloadType::StreamLatencyFirst,
            Some("nope".into()),
            envelope("r2"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::ChannelNotFound(_)));
}

#[tokio::test(start_paused = true)]
async fn send_request_times_out_without_response() {
    let t = Arc::new(HostTransport::new());
    let caller = t.clone();
    let mut env = envelope("req-timeout");
    env.timeout_ms = 10; // 10ms
    let request = tokio::spawn(async move {
        caller
            .send_request(PayloadType::RpcReliable, None, env)
            .await
    });
    let submitted = tokio::time::timeout(Duration::from_secs(1), t.recv_reliable_raw())
        .await
        .expect("request must reach the workload queue")
        .unwrap();
    assert_eq!(submitted.request_id, "req-timeout");
    tokio::time::advance(Duration::from_millis(10)).await;
    let err = request.await.unwrap().unwrap_err();
    assert!(
        matches!(err, ActrError::DeliveryUncertain { .. }),
        "got {err:?}"
    );
    assert!(!err.is_retryable());
}

#[tokio::test(start_paused = true)]
async fn send_request_timeout_removes_pending_entry() {
    let t = Arc::new(HostTransport::new());
    let caller = t.clone();
    let mut env = envelope("req-timeout-cleanup");
    env.timeout_ms = 10;
    let request = tokio::spawn(async move {
        caller
            .send_request(PayloadType::RpcReliable, None, env)
            .await
    });
    let submitted = tokio::time::timeout(Duration::from_secs(1), t.recv_reliable_raw())
        .await
        .expect("request must reach the workload queue")
        .unwrap();
    assert_eq!(submitted.request_id, "req-timeout-cleanup");
    tokio::time::advance(Duration::from_millis(10)).await;
    let err = request.await.unwrap().unwrap_err();
    assert!(
        matches!(err, ActrError::DeliveryUncertain { .. }),
        "got {err:?}"
    );
    assert!(!err.is_retryable());

    // The timed-out entry must not linger: the map is empty and a late
    // completion is rejected as unknown.
    assert_eq!(t.pending_len(), 0);
    let err = t
        .complete_response("req-timeout-cleanup", Bytes::from_static(b"late"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, NetworkError::InvalidArgument(_)),
        "got {err:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn send_request_uses_one_deadline_across_channel_admission_and_response() {
    let t = Arc::new(HostTransport::new());

    // Saturate the reliable lane so the request must first wait for channel
    // admission. The production capacity is intentionally exercised here:
    // this regression is about the real bounded queue, not a test-only lane.
    for sequence in 0..1024 {
        t.send_message(
            PayloadType::RpcReliable,
            None,
            envelope(&format!("queued-{sequence}")),
        )
        .await
        .expect("filling the reliable lane should succeed");
    }

    let request_transport = Arc::clone(&t);
    let request = tokio::spawn(async move {
        let mut env = envelope("request-after-backpressure");
        env.timeout_ms = 100;
        request_transport
            .send_request(PayloadType::RpcReliable, None, env)
            .await
    });

    tokio::task::yield_now().await;
    assert_eq!(
        t.pending_len(),
        1,
        "request should be waiting for admission"
    );

    // Spend 60 ms of the caller budget waiting for capacity, then admit the
    // request. Only 40 ms must remain for the response wait.
    tokio::time::advance(Duration::from_millis(60)).await;
    t.reliable_rx
        .lock()
        .await
        .recv()
        .await
        .expect("the saturated lane should contain an envelope");
    tokio::task::yield_now().await;
    assert!(!request.is_finished(), "no response has been completed yet");

    tokio::time::advance(Duration::from_millis(40)).await;
    tokio::task::yield_now().await;
    assert!(
        request.is_finished(),
        "channel admission must consume the original caller deadline"
    );

    let err = request
        .await
        .expect("request task should not panic")
        .expect_err("request should time out at the original deadline");
    assert!(
        matches!(err, ActrError::DeliveryUncertain { .. }),
        "got {err:?}"
    );
    assert!(!err.is_retryable());
    assert_eq!(t.pending_len(), 0, "timeout must remove correlation state");
}

#[tokio::test]
async fn send_request_lane_failure_removes_pending_entry() {
    let t = HostTransport::new();
    let mut env = envelope("req-lane-missing");
    env.timeout_ms = 5000;
    // get_lane fails before anything is sent (channel never created).
    let err = t
        .send_request(
            PayloadType::StreamLatencyFirst,
            Some("never-created".into()),
            env,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ActrError::NotFound(_)), "got {err:?}");

    assert_eq!(t.pending_len(), 0);
    let err = t
        .complete_error("req-lane-missing", ActrError::Internal("late".into()))
        .await
        .unwrap_err();
    assert!(
        matches!(err, NetworkError::InvalidArgument(_)),
        "got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_request_success_removes_pending_entry() {
    let t = Arc::new(HostTransport::new());
    let t2 = t.clone();
    let handle = tokio::spawn(async move {
        let mut env = envelope("req-clean");
        env.timeout_ms = 5000;
        t2.send_request(PayloadType::RpcReliable, None, env).await
    });

    // Wait (bounded) for the spawned task to register its pending entry.
    tokio::time::timeout(Duration::from_secs(5), async {
        while t.pending_len() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pending entry must be registered");
    assert_eq!(t.pending_len(), 1);

    t.complete_response("req-clean", Bytes::from_static(b"resp"))
        .await
        .unwrap();
    handle.await.unwrap().unwrap();
    assert_eq!(t.pending_len(), 0);
}

#[tokio::test]
async fn complete_response_unknown_id_errors() {
    let t = HostTransport::new();
    let err = t
        .complete_response("unknown", Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::InvalidArgument(_)));
}

#[tokio::test]
async fn complete_error_unknown_id_errors() {
    let t = HostTransport::new();
    let err = t
        .complete_error("unknown", ActrError::Internal("x".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, NetworkError::InvalidArgument(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_request_completes_with_response_bytes() {
    let t2 = Arc::new(HostTransport::new());
    let t3 = t2.clone();
    let handle = tokio::spawn(async move {
        let mut env = envelope("req-ok");
        env.timeout_ms = 5000;
        t3.send_request(PayloadType::RpcReliable, None, env).await
    });

    // Let the spawned task register its pending entry + send, then complete.
    tokio::time::sleep(Duration::from_millis(150)).await;

    t2.complete_response("req-ok", Bytes::from_static(b"resp"))
        .await
        .unwrap();
    let resp = handle.await.unwrap().unwrap();
    assert_eq!(resp, Bytes::from_static(b"resp"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_request_completes_with_error() {
    let t = Arc::new(HostTransport::new());
    let t2 = t.clone();
    let handle = tokio::spawn(async move {
        let mut env = envelope("req-err");
        env.timeout_ms = 5000;
        t2.send_request(PayloadType::RpcReliable, None, env).await
    });

    tokio::time::sleep(Duration::from_millis(150)).await;

    t.complete_error("req-err", ActrError::NotFound("missing".into()))
        .await
        .unwrap();
    let err = handle.await.unwrap().unwrap_err();
    assert!(matches!(err, ActrError::NotFound(_)), "got {err:?}");
}

#[test]
fn default_impl_matches_new() {
    // Default must behave like new() (reliable channel present).
    let _t = HostTransport::default();
}

#[tokio::test(start_paused = true)]
async fn submitted_request_timeout_cannot_prove_non_delivery() {
    let transport = Arc::new(HostTransport::new());
    let caller = transport.clone();
    let request = tokio::spawn(async move {
        let mut env = envelope("already-consumed");
        env.timeout_ms = 100;
        caller
            .send_request(PayloadType::RpcReliable, None, env)
            .await
    });

    // A workload can consume and act on this request before the caller expires.
    let consumed = tokio::time::timeout(Duration::from_secs(1), transport.recv_reliable_raw())
        .await
        .expect("the request must reach the workload queue")
        .unwrap();
    assert_eq!(consumed.request_id, "already-consumed");
    tokio::time::advance(Duration::from_millis(100)).await;
    let error = request.await.unwrap().unwrap_err();
    assert!(
        matches!(&error, ActrError::DeliveryUncertain { operation_id, .. } if operation_id == "already-consumed")
    );
    assert!(!error.is_retryable());
    assert_eq!(transport.pending_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn request_deadline_before_queue_submission_is_retryable() {
    let t = HostTransport::new();
    for n in 0..1024 {
        t.send_message(
            PayloadType::RpcReliable,
            None,
            envelope(&format!("fill-{n}")),
        )
        .await
        .unwrap();
    }
    let mut env = envelope("not-submitted");
    env.timeout_ms = 100;
    let error = t
        .send_request(PayloadType::RpcReliable, None, env)
        .await
        .unwrap_err();
    assert!(matches!(error, ActrError::TimedOut));
    assert!(error.is_retryable());
    assert_eq!(t.pending_len(), 0);
    assert_eq!(t.reliable_rx.lock().await.len(), 1024);
}
