use super::*;
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

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

async fn connected_pair() -> (
    WebSocketConnection,
    WebSocketStream<MaybeTlsStream<TcpStream>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (client, server) = tokio::join!(TcpStream::connect(address), listener.accept());
    let client = WebSocketStream::from_raw_socket(
        MaybeTlsStream::Plain(client.unwrap()),
        tokio_tungstenite::tungstenite::protocol::Role::Client,
        None,
    )
    .await;
    let server = WebSocketStream::from_raw_socket(
        MaybeTlsStream::Plain(server.unwrap().0),
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;
    (WebSocketConnection::from_server_stream(server), client)
}

#[tokio::test]
async fn silent_peer_is_retired_without_a_call_deadline() {
    let (connection, _silent_peer) = connected_pair().await;
    let lane = connection.get_lane(PayloadType::RpcReliable).await.unwrap();
    tokio::time::pause();
    // Keep the peer socket open but never poll it: no FIN/RST and no Pong.
    tokio::time::timeout(
        PING_INTERVAL + PROBE_TIMEOUT + Duration::from_secs(1),
        connection.secure.wait_terminal(),
    )
    .await
    .unwrap();
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
    assert!(!lane.is_healthy());
    let attempt = lane
        .send_with_progress(bytes::Bytes::from_static(b"later call"))
        .await;
    assert_eq!(
        attempt.progress,
        crate::transport::DeliveryProgress::NotAttempted
    );
    assert!(attempt.proven_pre_send);
    tokio::task::yield_now().await;
    assert!(connection.sink.lock().await.is_none());
}

#[tokio::test]
async fn matching_pongs_keep_an_idle_session_alive() {
    let (connection, mut peer) = connected_pair().await;
    tokio::time::pause();
    for sequence in 1u64..=3 {
        let frame = tokio::time::timeout(PING_INTERVAL + PROBE_TIMEOUT, peer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            frame,
            WsMessage::Ping(sequence.to_be_bytes().to_vec().into())
        );
        // Flush tungstenite's automatic Pong; no business response is sent.
        peer.flush().await.unwrap();
        assert!(connection.secure.state().is_sendable());
    }
    connection.close().await.unwrap();
    assert!(connection.sink.lock().await.is_none());
}

#[tokio::test]
async fn sink_contention_cannot_block_session_retirement() {
    let (connection, _silent_peer) = connected_pair().await;
    let guard = connection.sink.lock().await;
    tokio::time::pause();
    tokio::time::timeout(
        PING_INTERVAL + PROBE_TIMEOUT + Duration::from_secs(1),
        connection.secure.wait_terminal(),
    )
    .await
    .unwrap();
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
    drop(guard);
    tokio::task::yield_now().await;
    assert!(connection.sink.lock().await.is_none());
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn retired_websocket_destination_is_rebuilt_for_later_work() {
    use crate::transport::{Dest, PeerTransport, WireBuilder};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Builder {
        next: Mutex<Option<WebSocketConnection>>,
        builds: AtomicUsize,
    }
    #[async_trait]
    impl WireBuilder for Builder {
        async fn create_connections(&self, _: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Arc::new(
                self.next.lock().await.take().expect("fresh wire"),
            )])
        }
    }
    let local = actr_protocol::ActrId {
        realm: actr_protocol::Realm { realm_id: 1 },
        serial_number: 1,
        r#type: actr_protocol::ActrType {
            manufacturer: "test".into(),
            name: "ws-recovery".into(),
            version: "1".into(),
        },
    };
    let mut target = local.clone();
    target.serial_number = 2;
    let dest = Dest::peer(target);
    let (old, _silent_peer) = connected_pair().await;
    let builder = Arc::new(Builder {
        next: Mutex::new(Some(old.clone())),
        builds: AtomicUsize::new(0),
    });
    let transport = PeerTransport::new(local, builder.clone());
    transport
        .send(&dest, PayloadType::RpcReliable, b"original")
        .await
        .unwrap();
    let mut revisions = transport.watch_transports_revision();
    tokio::time::pause();
    tokio::time::timeout(
        PING_INTERVAL + PROBE_TIMEOUT + Duration::from_secs(1),
        async {
            while transport.watch_ready_for(&dest).await.is_some() {
                revisions.changed().await.unwrap();
            }
        },
    )
    .await
    .unwrap();
    assert!(old.secure.state().is_terminal());
    assert!(
        old.connect().await.is_err(),
        "retired objects cannot replace their socket"
    );
    tokio::time::resume();

    let (replacement, mut peer) = connected_pair().await;
    *builder.next.lock().await = Some(replacement.clone());
    transport
        .send(&dest, PayloadType::RpcReliable, b"later work")
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(1), peer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let WsMessage::Binary(data) = frame else {
        panic!("expected only the new application frame")
    };
    assert_eq!(TransportMessage::decode(&data).unwrap().data, b"later work");
    assert_eq!(builder.builds.load(Ordering::SeqCst), 2);
    replacement.close().await.unwrap();
}

async fn backpressured_pair() -> (
    WebSocketConnection,
    WebSocketStream<MaybeTlsStream<TcpStream>>,
    mpsc::Receiver<bytes::Bytes>,
) {
    let (connection, mut peer) = connected_pair().await;
    let (tx, rx) = mpsc::channel(1);
    tx.send(bytes::Bytes::from_static(b"first")).await.unwrap();
    connection
        .register_route(PayloadType::RpcReliable, tx)
        .await
        .unwrap();
    let mut blocked = connection.receive_blocked.subscribe();
    let mut frame = vec![PayloadType::RpcReliable as u8];
    frame.extend_from_slice(&6u32.to_be_bytes());
    frame.extend_from_slice(b"second");
    peer.send(WsMessage::Binary(frame.into())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), blocked.wait_for(|value| *value))
        .await
        .unwrap()
        .unwrap();
    (connection, peer, rx)
}

async fn drain_backpressure(
    connection: &WebSocketConnection,
    rx: &mut mpsc::Receiver<bytes::Bytes>,
) {
    let mut blocked = connection.receive_blocked.subscribe();
    assert_eq!(rx.recv().await.unwrap().as_ref(), b"first");
    assert_eq!(rx.recv().await.unwrap().as_ref(), b"second");
    tokio::time::timeout(Duration::from_secs(1), blocked.wait_for(|value| !*value))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn receive_backpressure_preserves_messages_and_healthy_session() {
    let (connection, mut peer, mut rx) = backpressured_pair().await;
    tokio::time::pause();
    // Keep the application queue full across several heartbeat intervals while
    // letting a healthy peer read and answer each control Ping.
    for _ in 0..3 {
        tokio::time::advance(PING_INTERVAL).await;
        tokio::time::resume();
        let frame = tokio::time::timeout(PROBE_TIMEOUT, peer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            frame,
            WsMessage::Ping(RECEIVE_BACKPRESSURE_PING.to_vec().into())
        );
        peer.flush().await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::pause();
        assert!(connection.secure.state().is_sendable());
    }
    drain_backpressure(&connection, &mut rx).await;
    // Leave time paused and let Tokio advance to the watchdog's next timer.
    // This also gives the watchdog a chance to observe the cleared queue before
    // the next interval elapses, avoiding a race with manually advancing time.
    // Reading resumes without losing or reordering queued messages; fresh
    // matching Pongs keep the same physical session alive afterwards.
    let mut normal_probes = 0;
    // Stay alive beyond the original pressure deadline after the queue drains.
    while normal_probes < 6 {
        let frame = tokio::time::timeout(PING_INTERVAL + PROBE_TIMEOUT, peer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let WsMessage::Ping(payload) = frame else {
            panic!("expected a liveness or pressure Ping");
        };
        if payload.len() == 8 {
            normal_probes += 1;
        } else {
            assert_eq!(payload.as_ref(), RECEIVE_BACKPRESSURE_PING);
        }
        peer.flush().await.unwrap();
        tokio::task::yield_now().await;
        assert!(connection.secure.state().is_sendable());
    }
    tokio::time::resume();
    connection.close().await.unwrap();
}

#[tokio::test]
async fn silent_peer_is_retired_during_receive_backpressure() {
    let (connection, mut peer, _rx) = backpressured_pair().await;
    // Observe the first heartbeat so the stall budget is armed before pausing.
    let frame = tokio::time::timeout(Duration::from_secs(1), peer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        frame,
        WsMessage::Ping(RECEIVE_BACKPRESSURE_PING.to_vec().into())
    );
    // Keep both the full application queue and the peer socket open. The peer
    // never flushes its Pong or reads again, but small heartbeat writes succeed.
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    tokio::time::timeout(
        RECEIVE_BACKPRESSURE_TIMEOUT + Duration::from_secs(1),
        connection.secure.wait_terminal(),
    )
    .await
    .unwrap();
    assert!(started.elapsed() >= RECEIVE_BACKPRESSURE_TIMEOUT - Duration::from_secs(1));
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
    tokio::task::yield_now().await;
    assert!(connection.sink.lock().await.is_none());
}

#[tokio::test]
async fn pressure_heartbeats_do_not_extend_receive_backpressure_deadline() {
    let (connection, mut peer, _rx) = backpressured_pair().await;
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    let mut heartbeats = 0;
    tokio::time::timeout(
        RECEIVE_BACKPRESSURE_TIMEOUT + Duration::from_secs(1),
        async {
            loop {
                tokio::select! {
                    biased;
                    _ = connection.secure.wait_terminal() => break,
                    frame = peer.next() => {
                        assert_eq!(
                            frame.unwrap().unwrap(),
                            WsMessage::Ping(RECEIVE_BACKPRESSURE_PING.to_vec().into())
                        );
                        heartbeats += 1;
                        peer.flush().await.unwrap();
                    }
                }
            }
        },
    )
    .await
    .unwrap();
    assert!(
        heartbeats >= 2,
        "cover renewal attempts across heartbeat cycles"
    );
    assert!(started.elapsed() >= RECEIVE_BACKPRESSURE_TIMEOUT - Duration::from_secs(1));
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
}

#[tokio::test]
async fn silent_peer_is_retired_after_receive_backpressure_clears() {
    let (connection, _silent_peer, mut rx) = backpressured_pair().await;
    tokio::time::pause();
    tokio::time::advance(PING_INTERVAL + PROBE_TIMEOUT).await;
    tokio::task::yield_now().await;
    assert!(connection.secure.state().is_sendable());
    drain_backpressure(&connection, &mut rx).await;
    tokio::time::timeout(
        PING_INTERVAL + PROBE_TIMEOUT + Duration::from_secs(1),
        connection.secure.wait_terminal(),
    )
    .await
    .unwrap();
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
}

#[tokio::test]
async fn two_actrium_peers_survive_receive_backpressure() {
    let (connection, peer, mut rx) = backpressured_pair().await;
    let peer = WebSocketConnection::from_server_stream(peer);
    let mut heartbeats = peer.peer_backpressure.subscribe();
    tokio::time::timeout(Duration::from_secs(1), heartbeats.changed())
        .await
        .unwrap()
        .unwrap();
    tokio::time::pause();
    // Each notification proves the opposite dispatcher actually read a new
    // heartbeat. Cover several full probe budgets with both watchdogs active.
    for _ in 0..5 {
        tokio::time::advance(PING_INTERVAL).await;
        tokio::time::resume();
        tokio::time::timeout(PROBE_TIMEOUT, heartbeats.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(connection.secure.state().is_sendable());
        assert!(peer.secure.state().is_sendable());
        tokio::task::yield_now().await;
        tokio::time::pause();
    }
    tokio::time::resume();
    drain_backpressure(&connection, &mut rx).await;
    let lane = peer.get_lane(PayloadType::RpcReliable).await.unwrap();
    lane.send(bytes::Bytes::from_static(b"third"))
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        b"third"
    );
    assert!(connection.secure.state().is_sendable());
    assert!(peer.secure.state().is_sendable());
    connection.close().await.unwrap();
    peer.close().await.unwrap();
}

#[tokio::test]
async fn peer_backpressure_heartbeat_does_not_disable_liveness_forever() {
    let (connection, mut peer) = connected_pair().await;
    let mut heartbeats = connection.peer_backpressure.subscribe();
    peer.send(WsMessage::Ping(RECEIVE_BACKPRESSURE_PING.to_vec().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), heartbeats.changed())
        .await
        .unwrap()
        .unwrap();
    // The peer remains open but neither renews its pressure indication nor
    // answers normal probes. The last heartbeat must have a bounded lifetime.
    tokio::time::pause();
    tokio::time::timeout(
        PING_INTERVAL + PROBE_TIMEOUT + Duration::from_secs(1),
        connection.secure.wait_terminal(),
    )
    .await
    .unwrap();
    assert_eq!(connection.secure.state(), SecureChannelState::Failed);
}
