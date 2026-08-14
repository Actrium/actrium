use super::*;

#[test]
fn test_transport_message_decode() {
    // Manually construct encoded message:
    // [payload_type: 1 byte][data_len: 4 bytes][data: N bytes]
    let mut encoded = Vec::new();
    encoded.push(PayloadType::RpcReliable as u8); // payload_type = 0
    encoded.extend_from_slice(&11u32.to_be_bytes()); // length = 11
    encoded.extend_from_slice(b"hello world"); // data

    let decoded =
        TransportMessage::decode(&encoded).expect("Should decode valid TransportMessage in test");

    assert_eq!(decoded.payload_type as u8, PayloadType::RpcReliable as u8);
    assert_eq!(decoded.data, b"hello world");
}

#[test]
fn test_transport_message_decode_invalid() {
    // message too short
    let data = vec![1, 0, 0];
    assert!(TransportMessage::decode(&data).is_err());

    // no effect 's payload_type
    let data = vec![99, 0, 0, 0, 5, 1, 2, 3, 4, 5];
    assert!(TransportMessage::decode(&data).is_err());
}

// ── Secure terminal driving on the physical close path (RFC-0563 re-review) ──

/// The INHERENT `close()` (what every concrete-typed call site resolves to —
/// inherent methods shadow the trait impl) must drive the secure session to
/// `Closed`, so no close path can bypass terminal state.
#[tokio::test]
async fn inherent_close_drives_secure_state_closed() {
    let conn = WebSocketConnection::new("ws://127.0.0.1:0".into());
    assert_eq!(
        conn.secure_channel_state(),
        SecureChannelState::LegacyBypass
    );

    conn.close().await.expect("inherent close should succeed");

    assert_eq!(
        conn.secure_channel_state(),
        SecureChannelState::Closed,
        "inherent close must drive the secure session terminal"
    );
}

/// The dyn-dispatch (`Arc<dyn WireHandle>`) close path must behave the same.
#[tokio::test]
async fn dyn_close_drives_secure_state_closed() {
    let conn: Arc<dyn crate::transport::WireHandle> =
        Arc::new(WebSocketConnection::new("ws://127.0.0.1:0".into()));

    conn.close().await.expect("trait close should succeed");

    assert_eq!(
        conn.secure_channel_state(),
        SecureChannelState::Closed,
        "trait close must drive the secure session terminal"
    );
}
