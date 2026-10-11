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
    assert!(
        !conn.is_connected(),
        "sync state reads must be safe inside Tokio"
    );
    assert_eq!(
        conn.secure_channel_state(),
        SecureChannelState::LegacyBypass
    );

    conn.close().await.expect("inherent close should succeed");
    assert!(!conn.is_connected());

    assert_eq!(
        conn.secure_channel_state(),
        SecureChannelState::Closed,
        "inherent close must drive the secure session terminal"
    );
}

#[tokio::test]
async fn virtual_connection_eof_closes_state_and_lanes_without_blocking() {
    let (tx, rx) = tokio::sync::mpsc::channel::<WsMessage>(1);
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
    let sink = futures_util::sink::unfold((), |(), _: WsMessage| async {
        Ok::<_, tokio_tungstenite::tungstenite::Error>(())
    });
    let conn = WebSocketConnection::from_parts(Box::pin(sink), stream);
    assert!(conn.is_connected());
    drop(tx);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while conn.is_connected() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(conn.secure_channel_state(), SecureChannelState::Closed);
    assert!(conn.get_lane(PayloadType::RpcReliable).await.is_err());
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

#[test]
fn debug_does_not_expose_url_secrets() {
    let conn = WebSocketConnection::new("wss://user:URL_SECRET@example.com".into());
    assert!(!format!("{conn:?}").contains("SECRET"));
}

/// Dropping the last connection handle must terminate the dispatcher task and
/// close the stream, so a gate-rejected peer cannot hold the socket open.
#[tokio::test]
async fn dropping_inbound_connection_terminates_the_stream() {
    let (server_io, client_io) = tokio::io::duplex(1024);
    let stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
        server_io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    let (sink, stream) = stream.split();
    let conn = WebSocketConnection::from_parts(Box::pin(sink), stream);
    let mut client = tokio_tungstenite::WebSocketStream::from_raw_socket(
        client_io,
        tokio_tungstenite::tungstenite::protocol::Role::Client,
        None,
    )
    .await;

    drop(conn);

    let observed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        // Anything other than a heartbeat proves teardown: Close frame, EOF,
        // or a reset-style error.
        while let Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_))) = client.next().await {}
    })
    .await;
    assert!(
        observed.is_ok(),
        "dropping the connection must terminate the stream"
    );
}

/// `close()` must not wait for the peer's Close echo: the dispatcher is
/// cancelled directly so a non-cooperative peer cannot keep the task alive.
#[tokio::test]
async fn close_terminates_dispatcher_without_peer_close_echo() {
    let (server_io, client_io) = tokio::io::duplex(1024);
    let stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
        server_io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    let (sink, stream) = stream.split();
    let conn = WebSocketConnection::from_parts(Box::pin(sink), stream);
    // Hold the client half without reading, so no Close echo is ever sent.
    let _client = tokio_tungstenite::WebSocketStream::from_raw_socket(
        client_io,
        tokio_tungstenite::tungstenite::protocol::Role::Client,
        None,
    )
    .await;

    tokio::time::timeout(std::time::Duration::from_secs(5), conn.close())
        .await
        .expect("close must not wait for a peer Close echo")
        .unwrap();
}
