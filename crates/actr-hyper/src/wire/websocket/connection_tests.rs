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

#[test]
fn debug_does_not_expose_credentials_or_url_secrets() {
    let conn = WebSocketConnection::new("wss://user:URL_SECRET@example.com".into())
        .with_credential_b64("BEARER_SECRET".into());
    let debug = format!("{conn:?}");
    assert!(!debug.contains("SECRET"));
}

/// A wss:// URL without an attached security policy must fail closed instead
/// of silently substituting default public roots for the operator's policy.
#[tokio::test]
async fn wss_without_client_roots_fails_closed() {
    let conn = WebSocketConnection::new("wss://127.0.0.1:9".into());
    let err = conn.connect().await.unwrap_err();
    assert!(
        matches!(err, NetworkError::ConfigurationError(_)),
        "expected fail-closed configuration error, got {err:?}"
    );
}

/// A plaintext ws:// loopback connection needs no TLS client roots.
#[tokio::test]
async fn plaintext_loopback_needs_no_client_roots() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let accept = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        tokio_tungstenite::accept_async(stream).await.unwrap();
    });
    let conn = WebSocketConnection::new(format!("ws://{addr}"))
        .with_security(crate::wire::websocket::security::DirectSecurity::insecure_loopback());
    conn.connect().await.unwrap();
    conn.close().await.unwrap();
    accept.await.unwrap();
}

/// Dropping the last connection handle must terminate the dispatcher task and
/// close the stream, so a gate-rejected peer cannot hold the socket open.
#[tokio::test]
async fn dropping_inbound_connection_terminates_the_stream() {
    let (server_io, client_io) = tokio::io::duplex(1024);
    let stream = WebSocketStream::from_raw_socket(
        server_io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    let conn = WebSocketConnection::from_server_stream(stream);
    let mut client = WebSocketStream::from_raw_socket(
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
    let stream = WebSocketStream::from_raw_socket(
        server_io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    let conn = WebSocketConnection::from_server_stream(stream);
    // Hold the client half without reading, so no Close echo is ever sent.
    let _client = WebSocketStream::from_raw_socket(
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

/// A frame arriving before the gate requests a lane must remain available.
#[tokio::test]
async fn inbound_stream_buffers_the_first_frame_before_lane_readers_start() {
    let (server_io, client_io) = tokio::io::duplex(1024);
    let stream = WebSocketStream::from_raw_socket(
        server_io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    let conn = WebSocketConnection::from_server_stream(stream);
    let mut client = WebSocketStream::from_raw_socket(
        client_io,
        tokio_tungstenite::tungstenite::protocol::Role::Client,
        None,
    )
    .await;
    client
        .send(WsMessage::Binary(bytes::Bytes::from_static(
            b"\x00\x00\x00\x00\x05first",
        )))
        .await
        .unwrap();

    // Wait until the dispatcher has consumed the frame, without requesting a
    // lane: the old constructor dropped it because no reader had registered.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let queued = conn.router.read().await[PayloadType::RpcReliable as usize]
                .as_ref()
                .is_some_and(|sender| sender.capacity() < sender.max_capacity());
            if queued {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the first frame should be buffered before a lane reader starts");
    let lane = conn.get_lane(PayloadType::RpcReliable).await.unwrap();
    assert_eq!(lane.recv().await.unwrap().as_ref(), b"first");
    conn.close().await.unwrap();
}
