use super::*;
use bytes::Bytes;

#[tokio::test]
async fn websocket_without_a_sink_is_proven_not_attempted() {
    let sink: WsSink = Arc::new(Mutex::new(None));
    let lane = WebSocketDataLane::new(sink, PayloadType::RpcReliable, mpsc::channel(1).1);

    let attempt = lane
        .send_with_progress(Bytes::from_static(b"payload"))
        .await;
    assert_eq!(attempt.progress, DeliveryProgress::NotAttempted);
    assert!(matches!(
        attempt.result,
        Err(NetworkError::WebSocketClosed(_))
    ));
}

#[test]
fn websocket_interrupted_write_is_unknown_and_not_retryable() {
    use tokio_tungstenite::tungstenite::Error as WsError;

    // A sink error after `send()` has started is intentionally modeled as an
    // interrupted/partially accepted write. The adapter must not claim that
    // the payload was NotAttempted, because DestTransport would replay it.
    let attempt = map_websocket_send_result(Err(WsError::ConnectionClosed));
    assert_eq!(attempt.progress, DeliveryProgress::Unknown);
    assert!(matches!(
        attempt.result,
        Err(NetworkError::WebSocketClosed(_))
    ));
}

// ── Trait-default `send_with_progress` contract ─────────────────────────────

/// Lane that only implements `send`, inheriting the trait-default
/// `send_with_progress` classification.
#[derive(Debug)]
struct DefaultProgressLane {
    mode: DefaultProgressMode,
}

#[derive(Debug, Clone, Copy)]
enum DefaultProgressMode {
    ClosedLike,
    SecureNotReady,
    Generic,
    Ok,
}

#[async_trait::async_trait]
impl DataLane for DefaultProgressLane {
    async fn send(&self, _data: Bytes) -> NetworkResult<()> {
        match self.mode {
            DefaultProgressMode::ClosedLike => Err(NetworkError::DataChannelClosed(
                "closed before write".into(),
            )),
            DefaultProgressMode::SecureNotReady => Err(NetworkError::SecureChannelNotReady(
                "secure gate not ready before write".into(),
            )),
            DefaultProgressMode::Generic => Err(NetworkError::SendError("boom".into())),
            DefaultProgressMode::Ok => Ok(()),
        }
    }

    fn lane_type(&self) -> &'static str {
        "default-progress"
    }
}

#[tokio::test]
async fn default_send_with_progress_classifies_by_error_shape() {
    // The trait default serves lanes that only implement `send` and is
    // fail-safe: such a lane cannot prove where a closed-like error
    // happened relative to the wire, so closed-like errors are
    // conservatively Unknown — auto-replaying a send that may have
    // partially entered a transport would duplicate the payload.
    // SecureChannelNotReady stays NotAttempted: it is a pre-send gate by
    // construction. Adapters with a proven pre-send state check override
    // `send_with_progress` to report NotAttempted (see the WebSocket tests
    // above, the v0.20 lane, and ClosedLikePreSendLane in
    // dest_transport_tests).
    for (mode, expected) in [
        (DefaultProgressMode::ClosedLike, DeliveryProgress::Unknown),
        (
            DefaultProgressMode::SecureNotReady,
            DeliveryProgress::NotAttempted,
        ),
        (DefaultProgressMode::Generic, DeliveryProgress::Unknown),
    ] {
        let lane = DefaultProgressLane { mode };
        let attempt = lane
            .send_with_progress(Bytes::from_static(b"payload"))
            .await;
        assert!(
            attempt.result.is_err(),
            "{mode:?} should surface the send error"
        );
        assert_eq!(
            attempt.progress, expected,
            "{mode:?} must classify as {expected:?} under the default contract"
        );
    }

    let lane = DefaultProgressLane {
        mode: DefaultProgressMode::Ok,
    };
    let attempt = lane
        .send_with_progress(Bytes::from_static(b"payload"))
        .await;
    assert!(attempt.result.is_ok());
    assert_eq!(attempt.progress, DeliveryProgress::SubmittedLocally);
}

// ── Fragment header encode / decode ───────────────────────────────────────

#[test]
fn test_encode_decode_fragment_header_single() {
    let mut buf = Vec::new();
    encode_fragment_header(&mut buf, 42, 0, 1);
    assert_eq!(buf.len(), FRAGMENT_HEADER_SIZE);

    let payload = Bytes::from(b"hello".as_slice().to_vec());
    let mut raw = buf;
    raw.extend_from_slice(&payload);

    let (msg_id, frag_index, total_frags, decoded_payload) =
        decode_fragment_header(Bytes::from(raw)).unwrap();
    assert_eq!(msg_id, 42);
    assert_eq!(frag_index, 0);
    assert_eq!(total_frags, 1);
    assert_eq!(decoded_payload, payload);
}

#[test]
fn test_encode_decode_fragment_header_multi() {
    let mut buf = Vec::new();
    encode_fragment_header(&mut buf, 0xDEAD_BEEF, 3, 7);
    let (msg_id, frag_index, total_frags, _) = decode_fragment_header(Bytes::from(buf)).unwrap();
    assert_eq!(msg_id, 0xDEAD_BEEF);
    assert_eq!(frag_index, 3);
    assert_eq!(total_frags, 7);
}

#[test]
fn test_decode_too_short_returns_error() {
    let short = Bytes::from_static(b"short");
    assert!(decode_fragment_header(short).is_err());
}

// ── ReassemblyBuffer ─────────────────────────────────────────────────────

#[test]
fn test_reassembly_single_fragment() {
    let mut buf = ReassemblyBuffer::new();
    let payload = Bytes::from_static(b"single");
    let result = buf.insert(1, 0, 1, payload.clone());
    assert_eq!(result, Some(payload));
}

#[test]
fn test_reassembly_two_fragments_in_order() {
    let mut buf = ReassemblyBuffer::new();
    let part0 = Bytes::from_static(b"hello ");
    let part1 = Bytes::from_static(b"world");

    assert!(buf.insert(5, 0, 2, part0).is_none());
    let result = buf.insert(5, 1, 2, part1).unwrap();
    assert_eq!(result, Bytes::from_static(b"hello world"));
}

#[test]
fn test_reassembly_two_fragments_out_of_order() {
    let mut buf = ReassemblyBuffer::new();
    let part0 = Bytes::from_static(b"hello ");
    let part1 = Bytes::from_static(b"world");

    assert!(buf.insert(7, 1, 2, part1).is_none());
    let result = buf.insert(7, 0, 2, part0).unwrap();
    assert_eq!(result, Bytes::from_static(b"hello world"));
}

#[test]
fn test_reassembly_multiple_messages_interleaved() {
    let mut buf = ReassemblyBuffer::new();

    assert!(buf.insert(1, 0, 2, Bytes::from_static(b"A1")).is_none());
    assert!(buf.insert(2, 0, 2, Bytes::from_static(b"B1")).is_none());
    assert!(buf.insert(1, 1, 2, Bytes::from_static(b"A2")).is_some());
    let msg2 = buf.insert(2, 1, 2, Bytes::from_static(b"B2")).unwrap();
    assert_eq!(msg2, Bytes::from_static(b"B1B2"));
}

// ── Fragment count calculation ────────────────────────────────────────────

#[test]
fn test_fragment_count_small_message() {
    let size = DC_MAX_PAYLOAD_SIZE;
    let count = size.div_ceil(DC_MAX_PAYLOAD_SIZE);
    assert_eq!(
        count, 1,
        "message equal to payload size should be 1 fragment"
    );
}

#[test]
fn test_fragment_count_one_byte_over() {
    let size = DC_MAX_PAYLOAD_SIZE + 1;
    let count = size.div_ceil(DC_MAX_PAYLOAD_SIZE);
    assert_eq!(count, 2, "one byte over should require 2 fragments");
}

#[test]
fn test_fragment_count_200kb() {
    let size: usize = 200 * 1024; // 200 KB
    let count = size.div_ceil(DC_MAX_PAYLOAD_SIZE);
    // 200*1024 / (65535 - 8) = 204800 / 65527 = 3.126 -> 4 fragments
    assert_eq!(count, 4);
}

// ── Round-trip via mpsc (simulated channel, no real DataChannel) ──────────

/// Helper: build a framed bytes buffer as the DataChannel would produce it.
fn make_frame(msg_id: u32, frag_index: u16, total_frags: u16, payload: &[u8]) -> Bytes {
    let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + payload.len());
    encode_fragment_header(&mut buf, msg_id, frag_index, total_frags);
    buf.extend_from_slice(payload);
    Bytes::from(buf)
}

/// Simulate what the recv() loop does: decode header from a raw frame.
fn recv_one(raw: Bytes, reassembly: &mut ReassemblyBuffer) -> Option<Bytes> {
    let (msg_id, frag_index, total_frags, payload) = decode_fragment_header(raw).unwrap();
    if total_frags == 1 {
        return Some(payload);
    }
    reassembly.insert(msg_id, frag_index, total_frags, payload)
}

#[test]
fn test_roundtrip_small_message() {
    let data = b"small message";
    let frame = make_frame(0, 0, 1, data);
    let mut buf = ReassemblyBuffer::new();
    let result = recv_one(frame, &mut buf).unwrap();
    assert_eq!(result.as_ref(), data);
}

#[test]
fn test_roundtrip_exactly_max_payload() {
    let data = vec![0xABu8; DC_MAX_PAYLOAD_SIZE];
    let frame = make_frame(1, 0, 1, &data);
    let mut buf = ReassemblyBuffer::new();
    let result = recv_one(frame, &mut buf).unwrap();
    assert_eq!(result.as_ref(), data.as_slice());
}

#[test]
fn test_roundtrip_one_byte_over_max_payload() {
    let data = vec![0xCDu8; DC_MAX_PAYLOAD_SIZE + 1];
    let (part0, part1) = data.split_at(DC_MAX_PAYLOAD_SIZE);

    let frame0 = make_frame(2, 0, 2, part0);
    let frame1 = make_frame(2, 1, 2, part1);

    let mut buf = ReassemblyBuffer::new();
    assert!(recv_one(frame0, &mut buf).is_none());
    let result = recv_one(frame1, &mut buf).unwrap();
    assert_eq!(result.as_ref(), data.as_slice());
}

#[test]
fn test_roundtrip_200kb_message() {
    let data: Vec<u8> = (0u8..=255).cycle().take(200 * 1024).collect();
    let total_frags = data.len().div_ceil(DC_MAX_PAYLOAD_SIZE) as u16;

    let mut buf = ReassemblyBuffer::new();
    let mut result = None;
    for (i, chunk) in data.chunks(DC_MAX_PAYLOAD_SIZE).enumerate() {
        let frame = make_frame(99, i as u16, total_frags, chunk);
        result = recv_one(frame, &mut buf);
    }
    let result = result.unwrap();
    assert_eq!(result.as_ref(), data.as_slice());
}

#[tokio::test]
async fn test_mpsc_lane() {
    use actr_protocol::RpcEnvelope;

    let (tx, rx) = mpsc::channel(10);
    let lane = MpscLane::new(PayloadType::RpcReliable, tx.clone(), rx);

    let envelope = RpcEnvelope {
        request_id: "test-1".to_string(),
        route_key: "test.route".to_string(),
        payload: Some(Bytes::from_static(b"hello")),
        traceparent: None,
        tracestate: None,
        metadata: vec![],
        timeout_ms: 30000,
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
    };
    lane.send_envelope(envelope.clone()).await.unwrap();

    let received = lane.recv_envelope().await.unwrap();
    assert_eq!(received.request_id, "test-1");
    assert_eq!(received.payload, Some(Bytes::from_static(b"hello")));
}

#[tokio::test]
async fn test_mpsc_lane_clone() {
    use actr_protocol::RpcEnvelope;

    let (tx, rx) = mpsc::channel(10);
    let lane = MpscLane::new(PayloadType::RpcReliable, tx.clone(), rx);

    let lane2 = lane.clone();

    let envelope = RpcEnvelope {
        request_id: "test-2".to_string(),
        route_key: "test.route".to_string(),
        payload: Some(Bytes::from_static(b"test")),
        traceparent: None,
        tracestate: None,
        metadata: vec![],
        timeout_ms: 30000,
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
    };
    lane.send_envelope(envelope.clone()).await.unwrap();

    let received = lane2.recv_envelope().await.unwrap();
    assert_eq!(received.request_id, "test-2");
    assert_eq!(received.payload, Some(Bytes::from_static(b"test")));
}

#[tokio::test]
async fn test_mpsc_lane_with_shared_rx() {
    use actr_protocol::RpcEnvelope;

    let (tx, rx) = mpsc::channel(10);
    let rx_shared = Arc::new(Mutex::new(rx));

    let lane = MpscLane::new_shared(PayloadType::RpcReliable, tx.clone(), rx_shared.clone());

    let envelope = RpcEnvelope {
        request_id: "test-3".to_string(),
        route_key: "test.route".to_string(),
        payload: Some(Bytes::from_static(b"shared")),
        traceparent: None,
        tracestate: None,
        metadata: vec![],
        timeout_ms: 30000,
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
    };
    lane.send_envelope(envelope.clone()).await.unwrap();

    let received = lane.recv_envelope().await.unwrap();
    assert_eq!(received.request_id, "test-3");
    assert_eq!(received.payload, Some(Bytes::from_static(b"shared")));
}

#[test]
fn test_lane_type_name() {
    let (tx, rx) = mpsc::channel(10);
    let lane = MpscLane::new(PayloadType::RpcReliable, tx, rx);
    assert_eq!(lane.lane_type(), "Mpsc");
}

// ── ReassemblyBuffer rejection paths (memory-exhaustion defenses) ──────────
//
// Each guard below defends a distinct adversarial/malformed shape; a
// regression in the arithmetic would silently weaken the bounds, so every
// rejection path is pinned by a dedicated test (the `#[cfg(test)] insert`
// wrapper discards errors, so these drive `try_insert` directly).

/// Limits small enough to trip every budget deterministically.
fn tiny_limits() -> ReassemblyLimits {
    ReassemblyLimits {
        max_messages: 2,
        max_bytes: 64,
        max_message_bytes: 48,
        max_fragments_per_message: 2,
    }
}

fn frag(len: usize) -> Bytes {
    Bytes::from(vec![0u8; len])
}

#[test]
fn try_insert_rejects_invalid_index() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    // total_frags == 0 is invalid.
    let err = buf.try_insert(1, 0, 0, frag(8)).unwrap_err();
    assert!(matches!(
        err,
        ReassemblyError::InvalidIndex { index: 0, total: 0 }
    ));
    // frag_index >= total_frags is invalid.
    let err = buf.try_insert(1, 2, 2, frag(8)).unwrap_err();
    assert!(matches!(
        err,
        ReassemblyError::InvalidIndex { index: 2, total: 2 }
    ));
    // The rejected message left no partial state behind.
    assert!(buf.try_insert(1, 0, 1, frag(8)).unwrap().is_some());
}

#[test]
fn try_insert_rejects_too_many_fragments() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    let err = buf.try_insert(1, 0, 3, frag(8)).unwrap_err();
    assert!(matches!(
        err,
        ReassemblyError::TooManyFragments { total: 3, max: 2 }
    ));
    // Discarded: the same msg_id restarts cleanly.
    assert!(buf.try_insert(1, 0, 2, frag(8)).unwrap().is_none());
}

#[test]
fn try_insert_rejects_inconsistent_total() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    assert!(buf.try_insert(1, 0, 2, frag(8)).unwrap().is_none());
    // total=1 is within every earlier guard (index < total <= max) but
    // disagrees with the recorded total of 2.
    let err = buf.try_insert(1, 0, 1, frag(8)).unwrap_err();
    assert!(matches!(
        err,
        ReassemblyError::InconsistentTotal {
            msg_id: 1,
            expected: 2,
            actual: 1
        }
    ));
    // The inconsistent entry was discarded, not merged.
    assert!(buf.try_insert(1, 0, 2, frag(8)).unwrap().is_none());
}

#[test]
fn try_insert_evicts_oldest_when_message_budget_is_full() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    // Two in-flight (incomplete) messages fill the message budget.
    assert!(buf.try_insert(1, 0, 2, frag(8)).unwrap().is_none());
    assert!(buf.try_insert(2, 0, 2, frag(8)).unwrap().is_none());
    assert!(buf.try_insert(3, 0, 2, frag(8)).unwrap().is_none());
    assert!(buf.pending.contains_key(&3));
    assert_eq!(buf.pending.len(), 2);
    assert!(
        !buf.pending.contains_key(&1) || !buf.pending.contains_key(&2),
        "one pre-existing partial must be evicted"
    );
}

#[test]
fn try_insert_rejects_message_too_large() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    // First fragment is fine; the second pushes the message past
    // max_message_bytes (48): 24 + 28 > 48.
    assert!(buf.try_insert(1, 0, 2, frag(24)).unwrap().is_none());
    let err = buf.try_insert(1, 1, 2, frag(28)).unwrap_err();
    assert!(matches!(err, ReassemblyError::MessageTooLarge));
    // The oversized message was discarded wholesale: its bytes no longer
    // count against the budget.
    assert!(buf.try_insert(1, 0, 2, frag(24)).unwrap().is_none());
}

#[test]
fn try_insert_evicts_other_partial_when_byte_budget_is_full() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    // 32 + 32 == max_bytes (64): both fit per-message but the aggregate is
    // now full. The next insert must extend an EXISTING message (a new msg_id
    // would hit the message-budget guard first) and stay under the
    // per-message cap so only the aggregate guard can fire.
    assert!(buf.try_insert(1, 0, 2, frag(32)).unwrap().is_none());
    assert!(buf.try_insert(2, 0, 2, frag(32)).unwrap().is_none());
    assert!(buf.try_insert(1, 1, 2, frag(8)).unwrap().is_some());
    assert!(
        !buf.pending.contains_key(&2),
        "competing oldest partial was evicted"
    );
    assert_eq!(buf.pending_bytes, 0);
}

#[test]
fn try_insert_duplicate_fragment_consumes_no_budget() {
    let mut buf = ReassemblyBuffer::with_limits(tiny_limits());
    assert!(buf.try_insert(1, 0, 2, frag(16)).unwrap().is_none());
    // Retransmitted duplicate is accepted (Ok(None)) without accounting.
    assert!(buf.try_insert(1, 0, 2, frag(16)).unwrap().is_none());
    // Budget still holds only one copy: 16 (dup) + 32 == 48 == per-message cap.
    assert!(buf.try_insert(1, 1, 2, frag(32)).unwrap().is_some());
}
