use super::*;
use crate::transport::SendAttemptResult;
use crate::transport::lane::DataLane;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::{Duration, timeout};

#[derive(Debug)]
struct FakeLane {
    send_count: Arc<AtomicUsize>,
    send_closed: bool,
    healthy: bool,
}

#[async_trait]
impl DataLane for FakeLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        self.send_count.fetch_add(1, Ordering::Relaxed);
        if self.send_closed {
            return Err(NetworkError::DataChannelClosed(
                "test send closed".to_string(),
            ));
        }
        Ok(())
    }

    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        // This lane models a failure detected *before* any concrete write, so
        // it explicitly declares the proven pre-send classification the
        // production default cannot infer.
        let progress = if self.send_closed {
            DeliveryProgress::NotAttempted
        } else {
            DeliveryProgress::SubmittedLocally
        };
        let result = self.send(data).await;
        SendAttemptResult {
            result,
            progress,
            proven_pre_send: self.send_closed,
        }
    }

    fn lane_type(&self) -> &'static str {
        "fake"
    }

    fn is_healthy(&self) -> bool {
        self.healthy
    }
}

#[derive(Debug)]
struct FakeWire {
    conn_type: ConnType,
    connect_fails: bool,
    lane_closed: bool,
    lane_send_closed: bool,
    lane_non_established: bool,
    lane_healthy: bool,
    retry_get_lane_closed_after_invalidate: bool,
    connected: AtomicBool,
    invalidated: AtomicBool,
    send_count: Arc<AtomicUsize>,
    identity: Option<WireIdentity>,
}

impl FakeWire {
    fn new(conn_type: ConnType) -> Self {
        Self {
            conn_type,
            connect_fails: false,
            lane_closed: false,
            lane_send_closed: false,
            lane_non_established: false,
            lane_healthy: true,
            retry_get_lane_closed_after_invalidate: false,
            connected: AtomicBool::new(false),
            invalidated: AtomicBool::new(false),
            send_count: Arc::new(AtomicUsize::new(0)),
            identity: None,
        }
    }

    fn connect_fails(mut self) -> Self {
        self.connect_fails = true;
        self
    }

    fn lane_closed(mut self) -> Self {
        self.lane_closed = true;
        self
    }

    fn lane_send_closed(mut self) -> Self {
        self.lane_send_closed = true;
        self
    }

    fn lane_non_established(mut self) -> Self {
        self.lane_non_established = true;
        self
    }

    fn lane_unhealthy(mut self) -> Self {
        self.lane_healthy = false;
        self
    }

    fn retry_get_lane_closed_after_invalidate(mut self) -> Self {
        self.retry_get_lane_closed_after_invalidate = true;
        self
    }

    fn with_identity(mut self, identity: WireIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    fn with_send_count(mut self, send_count: Arc<AtomicUsize>) -> Self {
        self.send_count = send_count;
        self
    }
}

#[async_trait]
impl WireHandle for FakeWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }

    fn priority(&self) -> u8 {
        match self.conn_type {
            ConnType::WebSocket => 0,
            ConnType::WebRTC => 1,
        }
    }

    async fn connect(&self) -> NetworkResult<()> {
        if self.connect_fails {
            return Err(NetworkError::ConnectionError("connect failed".into()));
        }
        self.connected.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    async fn close(&self) -> NetworkResult<()> {
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        if self.retry_get_lane_closed_after_invalidate && self.invalidated.load(Ordering::Relaxed) {
            return Err(NetworkError::PeerConnectionClosed(
                "test peer connection closed".to_string(),
            ));
        }
        if self.lane_closed {
            return Err(NetworkError::DataChannelClosed(
                "test lane closed".to_string(),
            ));
        }
        if self.lane_non_established {
            return Err(NetworkError::DataChannelNotOpen(
                "non-Established".to_string(),
            ));
        }
        Ok(Arc::new(FakeLane {
            send_count: Arc::clone(&self.send_count),
            send_closed: self.lane_send_closed,
            healthy: self.lane_healthy,
        }))
    }

    async fn invalidate_lane(&self, _payload_type: PayloadType) {
        self.invalidated.store(true, Ordering::Relaxed);
    }

    fn identity(&self) -> Option<WireIdentity> {
        self.identity.clone()
    }
}

async fn test_transport(connections: Vec<Arc<dyn WireHandle>>) -> DestTransport {
    let conn_mgr = Arc::new(WirePool::new(RetryConfig {
        max_attempts: 1,
        initial_delay_ms: 0,
        max_delay_ms: 0,
        multiplier: 1.0,
    }));
    for conn in connections {
        conn_mgr.add_connection(conn).await;
    }
    DestTransport { conn_mgr }
}

#[tokio::test]
async fn retry_get_lane_closed_after_send_closed_falls_back_to_websocket() {
    let peer_id = actr_protocol::ActrId::default();
    let websocket_sends = Arc::new(AtomicUsize::new(0));
    let transport = test_transport(vec![
        Arc::new(
            FakeWire::new(ConnType::WebRTC)
                .lane_send_closed()
                .retry_get_lane_closed_after_invalidate()
                .with_identity(WireIdentity::WebRtc {
                    peer_id,
                    session_id: 9,
                }),
        ),
        Arc::new(FakeWire::new(ConnType::WebSocket).with_send_count(Arc::clone(&websocket_sends))),
    ])
    .await;

    let identity = timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::RpcReliable, b"payload"),
    )
    .await
    .expect("send should not hang")
    .expect("closed WebRTC retry should be evicted and WebSocket fallback should send");

    assert!(identity.is_none());
    assert_eq!(websocket_sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn send_returns_when_only_webrtc_candidate_exhausted() {
    let transport = test_transport(vec![Arc::new(
        FakeWire::new(ConnType::WebRTC).connect_fails(),
    )])
    .await;

    let err = timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::StreamReliable, b"payload"),
    )
    .await
    .expect("send should not hang")
    .expect_err("send should fail after WebRTC candidate is exhausted");

    assert!(
        matches!(err, NetworkError::NoRoute(ref msg) if msg.contains("all transport candidates exhausted")),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn send_falls_back_to_websocket_when_webrtc_fails() {
    let websocket_sends = Arc::new(AtomicUsize::new(0));
    let transport = test_transport(vec![
        Arc::new(FakeWire::new(ConnType::WebRTC).connect_fails()),
        Arc::new(FakeWire::new(ConnType::WebSocket).with_send_count(Arc::clone(&websocket_sends))),
    ])
    .await;

    timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::StreamReliable, b"payload"),
    )
    .await
    .expect("send should not hang")
    .expect("send should use WebSocket fallback");

    assert_eq!(websocket_sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn send_falls_back_when_webrtc_lane_is_not_healthy() {
    let websocket_sends = Arc::new(AtomicUsize::new(0));
    let transport = test_transport(vec![
        Arc::new(FakeWire::new(ConnType::WebRTC).lane_unhealthy()),
        Arc::new(FakeWire::new(ConnType::WebSocket).with_send_count(Arc::clone(&websocket_sends))),
    ])
    .await;

    timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::StreamReliable, b"payload"),
    )
    .await
    .expect("send should not hang")
    .expect("an unhealthy WebRTC lane should fall back to WebSocket");

    assert_eq!(websocket_sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn stale_webrtc_ready_candidate_does_not_wait_forever() {
    let peer_id = actr_protocol::ActrId::default();
    let transport = test_transport(vec![Arc::new(
        FakeWire::new(ConnType::WebRTC)
            .lane_closed()
            .with_identity(WireIdentity::WebRtc {
                peer_id,
                session_id: 7,
            }),
    )])
    .await;

    let err = timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::StreamReliable, b"payload"),
    )
    .await
    .expect("send should not hang")
    .expect_err("stale WebRTC-only candidate should fail clearly");

    assert!(
        matches!(err, NetworkError::NoRoute(ref msg) if msg.contains("all transport candidates exhausted")),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn non_established_webrtc_candidate_is_evicted() {
    let peer_id = actr_protocol::ActrId::default();
    let transport = test_transport(vec![Arc::new(
        FakeWire::new(ConnType::WebRTC)
            .lane_non_established()
            .with_identity(WireIdentity::WebRtc {
                peer_id,
                session_id: 8,
            }),
    )])
    .await;

    let err = timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::RpcReliable, b"response"),
    )
    .await
    .expect("send should not hang")
    .expect_err("non-established WebRTC candidate should be evicted");

    assert!(
        matches!(err, NetworkError::NoRoute(ref msg) if msg.contains("all transport candidates exhausted")),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn generic_error_with_closed_text_does_not_trigger_stale_self_heal() {
    #[derive(Debug)]
    struct GenericClosedTextWire {
        connected: AtomicBool,
    }

    #[async_trait]
    impl WireHandle for GenericClosedTextWire {
        fn connection_type(&self) -> ConnType {
            ConnType::WebRTC
        }

        fn priority(&self) -> u8 {
            1
        }

        async fn connect(&self) -> NetworkResult<()> {
            self.connected.store(true, Ordering::Relaxed);
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected.load(Ordering::Relaxed)
        }

        async fn close(&self) -> NetworkResult<()> {
            self.connected.store(false, Ordering::Relaxed);
            Ok(())
        }

        async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
            Err(NetworkError::WebRtcError(
                "validation failed: closed is only payload text".to_string(),
            ))
        }
    }

    let transport = test_transport(vec![Arc::new(GenericClosedTextWire {
        connected: AtomicBool::new(false),
    })])
    .await;

    let result = timeout(
        Duration::from_millis(200),
        transport.send_with_identity(PayloadType::RpcReliable, b"payload"),
    )
    .await;

    assert!(
        result.is_err(),
        "generic error text containing 'closed' should not mark the candidate failed"
    );
}

#[tokio::test]
async fn has_healthy_connection_false_when_no_connections() {
    // Empty DestTransport has no ready connections → not healthy.
    let t = DestTransport::new(Dest::Host, vec![]).await.unwrap();
    assert!(!t.has_healthy_connection().await);
    // watch_ready returns a live receiver (empty initially).
    let rx = t.watch_ready();
    assert!(rx.borrow().is_empty());
}

// ── Rekey must not evict the wire (RFC-0563 re-review) ──────────────────

/// Lane whose first send fails with `SecureChannelNotReady` (simulating a
/// Verified→Rekeying rotation detected at send time); subsequent sends succeed
/// (rekey completed, fresh lane re-acquired).
#[derive(Debug)]
struct RekeyRecoveryLane {
    send_attempts: Arc<AtomicUsize>,
    ok_sends: Arc<AtomicUsize>,
}

#[async_trait]
impl DataLane for RekeyRecoveryLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        let n = self.send_attempts.fetch_add(1, Ordering::Relaxed);
        if n == 0 {
            return Err(NetworkError::SecureChannelNotReady(
                "rekey in flight".to_string(),
            ));
        }
        self.ok_sends.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        // The scripted `SecureChannelNotReady` is raised before any write is
        // attempted (the rekey gate never touches the transport), so this
        // lane explicitly declares the proven pre-send classification.
        let first = self.send_attempts.load(Ordering::Relaxed) == 0;
        let progress = if first {
            DeliveryProgress::NotAttempted
        } else {
            DeliveryProgress::SubmittedLocally
        };
        let result = self.send(data).await;
        SendAttemptResult {
            result,
            progress,
            proven_pre_send: first,
        }
    }

    fn lane_type(&self) -> &'static str {
        "rekey-recovery"
    }
}

#[derive(Debug)]
struct RekeyRecoveryWire {
    conn_type: ConnType,
    send_attempts: Arc<AtomicUsize>,
    ok_sends: Arc<AtomicUsize>,
    close_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl WireHandle for RekeyRecoveryWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn close(&self) -> NetworkResult<()> {
        self.close_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(Arc::new(RekeyRecoveryLane {
            send_attempts: Arc::clone(&self.send_attempts),
            ok_sends: Arc::clone(&self.ok_sends),
        }))
    }
}

/// Re-review scenario: a mid-rekey send error must wait for readiness and
/// re-acquire a lane on the SAME wire — no eviction, no close, send succeeds.
#[tokio::test]
async fn rekey_not_ready_does_not_evict_wire_and_send_recovers() {
    let send_attempts = Arc::new(AtomicUsize::new(0));
    let ok_sends = Arc::new(AtomicUsize::new(0));
    let close_calls = Arc::new(AtomicUsize::new(0));
    let transport = test_transport(vec![Arc::new(RekeyRecoveryWire {
        conn_type: ConnType::WebRTC,
        send_attempts: Arc::clone(&send_attempts),
        ok_sends: Arc::clone(&ok_sends),
        close_calls: Arc::clone(&close_calls),
    })])
    .await;

    let result = timeout(
        Duration::from_secs(2),
        transport.send_with_identity(PayloadType::RpcReliable, b"payload"),
    )
    .await
    .expect("send must not hang on a mid-rekey wire");

    assert!(
        result.is_ok(),
        "send must succeed after rekey recovery, got {result:?}"
    );
    assert_eq!(ok_sends.load(Ordering::Relaxed), 1);
    assert_eq!(
        close_calls.load(Ordering::Relaxed),
        0,
        "a rekeying (not closed) wire must NOT be closed or evicted"
    );
    // The wire is still pooled and acquirable.
    assert!(
        transport
            .conn_mgr
            .get_connection(ConnType::WebRTC)
            .await
            .is_some()
    );
}

// ── Route selection vs. delivery progress (RFC-0507) ─────────────────────

/// Wire that always serves the same pre-built lane, so a route-selection test
/// controls the exact `SendAttemptResult` the route loop observes.
#[derive(Debug)]
struct StaticLaneWire {
    conn_type: ConnType,
    lane: Arc<dyn DataLane>,
    identity: Option<WireIdentity>,
}

#[async_trait]
impl WireHandle for StaticLaneWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }

    fn priority(&self) -> u8 {
        match self.conn_type {
            ConnType::WebSocket => 0,
            ConnType::WebRTC => 1,
        }
    }

    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(self.lane.clone())
    }

    async fn invalidate_lane(&self, _payload_type: PayloadType) {}

    fn identity(&self) -> Option<WireIdentity> {
        self.identity.clone()
    }
}

/// Lane that raises a secure-readiness error but incorrectly reports
/// `Unknown`. DestTransport must still recover this pre-send condition before
/// applying the terminal ambiguous-delivery rule.
#[derive(Debug)]
struct MisclassifiedSecureLane {
    send_attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl DataLane for MisclassifiedSecureLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        if self.send_attempts.fetch_add(1, Ordering::Relaxed) == 0 {
            Err(NetworkError::SecureChannelNotReady(
                "rekey in flight".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        let result = self.send(data).await;
        SendAttemptResult {
            result,
            // Intentionally model an adapter that failed to classify the
            // secure gate before the write.
            progress: DeliveryProgress::Unknown,
            proven_pre_send: false,
        }
    }

    fn lane_type(&self) -> &'static str {
        "misclassified-secure"
    }
}

#[tokio::test]
async fn secure_not_ready_precedes_unknown_delivery_classification() {
    let send_attempts = Arc::new(AtomicUsize::new(0));
    let transport = test_transport(vec![Arc::new(StaticLaneWire {
        conn_type: ConnType::WebRTC,
        lane: Arc::new(MisclassifiedSecureLane {
            send_attempts: Arc::clone(&send_attempts),
        }),
        identity: Some(WireIdentity::WebRtc {
            peer_id: actr_protocol::ActrId::default(),
            session_id: 23,
        }),
    })])
    .await;

    let result = timeout(
        Duration::from_secs(1),
        transport.send_with_identity(PayloadType::RpcReliable, b"payload"),
    )
    .await
    .expect("misclassified secure readiness must not hang");

    assert!(
        result.is_ok(),
        "secure readiness should recover: {result:?}"
    );
    assert_eq!(
        send_attempts.load(Ordering::Relaxed),
        2,
        "the send should be retried after secure readiness, not surfaced as uncertain"
    );
}

/// Lane whose send fails after an ambiguous partial write: the error gives no
/// proof about how many bytes entered the transport.
#[derive(Debug)]
struct AmbiguousLane {
    send_count: Arc<AtomicUsize>,
}

#[async_trait]
impl DataLane for AmbiguousLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        unreachable!("AmbiguousLane is driven through send_with_progress")
    }

    async fn send_with_progress(&self, _data: bytes::Bytes) -> SendAttemptResult {
        self.send_count.fetch_add(1, Ordering::Relaxed);
        SendAttemptResult::failed(
            NetworkError::SendError("ambiguous partial write".into()),
            DeliveryProgress::Unknown,
        )
    }

    fn lane_type(&self) -> &'static str {
        "ambiguous"
    }
}

/// Lane whose send provably fails before any write, declared explicitly like
/// a production adapter with a pre-send check.
#[derive(Debug)]
struct ClosedLikePreSendLane {
    send_count: Arc<AtomicUsize>,
}

#[async_trait]
impl DataLane for ClosedLikePreSendLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        unreachable!("ClosedLikePreSendLane is driven through send_with_progress")
    }

    async fn send_with_progress(&self, _data: bytes::Bytes) -> SendAttemptResult {
        self.send_count.fetch_add(1, Ordering::Relaxed);
        SendAttemptResult::failed_pre_send(NetworkError::DataChannelClosed(
            "proven pre-send closure".into(),
        ))
    }

    fn lane_type(&self) -> &'static str {
        "closed-like-pre-send"
    }
}

#[tokio::test]
async fn uncertain_first_lane_suppresses_fallback_replay() {
    let webrtc_sends = Arc::new(AtomicUsize::new(0));
    let websocket_sends = Arc::new(AtomicUsize::new(0));
    let peer_id = actr_protocol::ActrId::default();
    let transport = test_transport(vec![
        Arc::new(StaticLaneWire {
            conn_type: ConnType::WebRTC,
            lane: Arc::new(AmbiguousLane {
                send_count: Arc::clone(&webrtc_sends),
            }),
            identity: Some(WireIdentity::WebRtc {
                peer_id,
                session_id: 11,
            }),
        }),
        Arc::new(FakeWire::new(ConnType::WebSocket).with_send_count(Arc::clone(&websocket_sends))),
    ])
    .await;

    let attempt = timeout(
        Duration::from_secs(1),
        transport.send_with_progress(PayloadType::RpcReliable, b"payload", None),
    )
    .await
    .expect("send should not hang");

    assert_eq!(
        attempt.progress,
        DeliveryProgress::Unknown,
        "the ambiguous progress fact must reach the operation owner"
    );
    assert!(attempt.result.is_err());
    assert_eq!(webrtc_sends.load(Ordering::Relaxed), 1);
    assert_eq!(
        websocket_sends.load(Ordering::Relaxed),
        0,
        "an uncertain first lane must not be replayed on a fallback route"
    );
}

#[tokio::test]
async fn not_attempted_first_lane_still_falls_back() {
    let webrtc_sends = Arc::new(AtomicUsize::new(0));
    let websocket_sends = Arc::new(AtomicUsize::new(0));
    let peer_id = actr_protocol::ActrId::default();
    let transport = test_transport(vec![
        Arc::new(StaticLaneWire {
            conn_type: ConnType::WebRTC,
            lane: Arc::new(ClosedLikePreSendLane {
                send_count: Arc::clone(&webrtc_sends),
            }),
            identity: Some(WireIdentity::WebRtc {
                peer_id,
                session_id: 12,
            }),
        }),
        Arc::new(FakeWire::new(ConnType::WebSocket).with_send_count(Arc::clone(&websocket_sends))),
    ])
    .await;

    let attempt = timeout(
        Duration::from_secs(1),
        transport.send_with_progress(PayloadType::RpcReliable, b"payload", None),
    )
    .await
    .expect("send should not hang");

    assert!(
        attempt.result.is_ok(),
        "a proven pre-send failure keeps the self-heal fallback: {:?}",
        attempt.result
    );
    assert_eq!(
        webrtc_sends.load(Ordering::Relaxed),
        2,
        "the WebRTC lane is attempted at most twice (initial + one bounded \
         self-heal retry) before the WebSocket fallback"
    );
    assert_eq!(
        websocket_sends.load(Ordering::Relaxed),
        1,
        "a NotAttempted first lane may still fall back to WebSocket exactly once"
    );
}
