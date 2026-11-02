//! Issue #248 regressions on the webrtc 0.21.1 client and the pinned TURN server.
//! Allocate replay is covered by the server pin to upstream webrtc-rs/webrtc#896.
//! The client includes the retirement fix from webrtc-rs/webrtc#912. These tests
//! exercise Actr's production RTC recovery/close path against a real UDP TURN
//! server, including consecutive ICE restarts and bidirectional business RPCs.
//! Run with `cargo test --locked -p actr-hyper --features test-utils
//! --test turn_credential_rotation -- --nocapture --test-threads=1`.

use super::*;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::stun::error_code::{CODE_ALLOC_MISMATCH, ErrorCodeAttribute};
use rtc::stun::message::{CLASS_SUCCESS_RESPONSE, Getter, Message};
use rtc::turn::client::{Client, ClientConfig, Event};
use turn_crate::allocation::five_tuple::FiveTuple;

async fn exchange(socket: &UdpSocket, server: SocketAddr, raw: &[u8]) -> Message {
    socket.send_to(raw, server).await.unwrap();
    let mut buffer = [0; 2048];
    let (size, source) =
        tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buffer))
            .await
            .expect("TURN response deadline")
            .unwrap();
    assert_eq!(source, server);
    let mut message = Message::new();
    message.raw = buffer[..size].to_vec();
    message.decode().unwrap();
    message
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocate_retransmission_replays_success() {
    install_test_crypto_provider();
    let auth = Arc::new(
        turn::Authenticator::new(
            TURN_SHARED_TEST_KEY.to_owned(),
            "provider.example".to_owned(),
        )
        .unwrap()
        .with_inactive_realms(turn_bindings().clone()),
    );
    let (address, server) = start_turn(auth).await;
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    let credential = turn_credential(&make_actor_id(46_541), expires);
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local = socket.local_addr().unwrap();
    let mut client = Client::new(
        ClientConfig {
            stun_serv_addr: address.to_string(),
            turn_serv_addr: address.to_string(),
            local_addr: local,
            transport_protocol: TransportProtocol::UDP,
            username: credential.username,
            password: credential.password,
            realm: String::new(),
            software: String::new(),
            rto_in_ms: 0,
            allocation_refresh_interval_cap: None,
        },
        rtc::crypto::default_provider().unwrap(),
    )
    .unwrap();
    client.allocate(std::time::Instant::now()).unwrap();
    let anonymous = client.poll_write().unwrap();
    let challenge = exchange(&socket, address, &anonymous.message).await;
    client
        .handle_read(TaggedBytesMut {
            now: std::time::Instant::now(),
            transport: TransportContext {
                local_addr: local,
                peer_addr: address,
                transport_protocol: TransportProtocol::UDP,
                ecn: None,
            },
            message: bytes::BytesMut::from(challenge.raw.as_slice()),
        })
        .unwrap();
    let authenticated = client.poll_write().unwrap();
    let first = exchange(&socket, address, &authenticated.message).await;
    assert_eq!(
        first.typ.class, CLASS_SUCCESS_RESPONSE,
        "initial allocation must succeed"
    );
    // Replay the exact datagram, as if the original success response were lost.
    let replay = exchange(&socket, address, &authenticated.message).await;
    let mut replay_code = ErrorCodeAttribute::default();
    let replay_error = replay_code
        .get_from(&replay)
        .ok()
        .map(|()| replay_code.code.0);
    let allocations = server.get_allocations_info(None).await.unwrap();
    eprintln!(
        "Allocate replay: first={}, replay={}, error={replay_error:?}, allocations={}",
        first.typ,
        replay.typ,
        allocations.len()
    );
    // A distinct authenticated transaction on the same tuple must still be rejected.
    client
        .handle_read(TaggedBytesMut {
            now: std::time::Instant::now(),
            transport: TransportContext {
                local_addr: local,
                peer_addr: address,
                transport_protocol: TransportProtocol::UDP,
                ecn: None,
            },
            message: bytes::BytesMut::from(first.raw.as_slice()),
        })
        .unwrap();
    let relay_addr = match client.poll_event().unwrap() {
        Event::AllocateResponse(_, relay) => relay,
        event => panic!("expected Allocate success: {event:?}"),
    };
    client.allocate(std::time::Instant::now()).unwrap();
    let anonymous = client.poll_write().unwrap();
    let challenge = exchange(&socket, address, &anonymous.message).await;
    client
        .handle_read(TaggedBytesMut {
            now: std::time::Instant::now(),
            transport: TransportContext {
                local_addr: local,
                peer_addr: address,
                transport_protocol: TransportProtocol::UDP,
                ecn: None,
            },
            message: bytes::BytesMut::from(challenge.raw.as_slice()),
        })
        .unwrap();
    let fresh = client.poll_write().unwrap();
    let conflict = exchange(&socket, address, &fresh.message).await;
    let mut code = ErrorCodeAttribute::default();
    code.get_from(&conflict).unwrap();
    // Positive cleanup control: an explicit Refresh(0) removes the allocation.
    client
        .relay(relay_addr)
        .unwrap()
        .close(std::time::Instant::now())
        .unwrap();
    let delete = client.poll_write().unwrap();
    let deleted = exchange(&socket, address, &delete.message).await;
    let remaining = server.get_allocations_info(None).await.unwrap().len();
    turn::shutdown_turn_server(&server).await.unwrap();
    assert_eq!(code.code.0, CODE_ALLOC_MISMATCH.0);
    assert_eq!(deleted.typ.class, CLASS_SUCCESS_RESPONSE);
    assert_eq!(remaining, 0);
    assert_eq!(allocations.len(), 1);
    assert_eq!(replay.transaction_id, first.transaction_id);
    assert_eq!(
        replay.typ, first.typ,
        "same Allocate transaction must replay success, not 437"
    );
    assert_eq!(replay.raw, first.raw, "replay must preserve the response");
    let mut replay_relay = rtc::turn::proto::relayaddr::RelayedAddress::default();
    replay_relay.get_from(&replay).unwrap();
    assert_eq!(
        SocketAddr::new(replay_relay.ip, replay_relay.port),
        relay_addr
    );
}

async fn wait_for_retirement(server: &turn_crate::server::Server, old: &[FiveTuple]) -> usize {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = server
            .get_allocations_info(Some(old.to_vec()))
            .await
            .unwrap()
            .len();
        if remaining == 0 || tokio::time::Instant::now() >= deadline {
            return remaining;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn allocation_lifecycle(restarts: usize) {
    install_test_crypto_provider();
    let auth = Arc::new(
        turn::Authenticator::new(
            TURN_SHARED_TEST_KEY.to_owned(),
            "provider.example".to_owned(),
        )
        .unwrap()
        .with_inactive_realms(turn_bindings().clone()),
    );
    let (address, server) = start_turn(auth).await;
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    let mut harness = TestHarness::new().await;
    for serial in [46_551, 46_552] {
        harness
            .add_peer_with_rtc_configuration(
                serial,
                relay_configuration(address, &turn_credential(&make_actor_id(serial), expires)),
            )
            .await;
    }
    harness.connect(46_551, 46_552).await;
    rpc(&harness, 46_551, 46_552, "allocation_initial").await;
    rpc(&harness, 46_552, 46_551, "allocation_initial_reverse").await;
    let initial = server.get_allocations_info(None).await.unwrap();
    assert_eq!(initial.len(), 2, "one relay allocation per loopback peer");
    let mut active: Vec<_> = initial.keys().copied().collect();
    for restart in 1..=restarts {
        let mut generations = Vec::new();
        for (serial, other) in [(46_551, 46_552), (46_552, 46_551)] {
            let peer = harness.peer(serial);
            let remote = &harness.peer(other).id;
            generations.push((
                serial,
                other,
                peer.rtc.generation(remote).await.unwrap(),
                peer.rtc.ice_generation(remote).await.unwrap(),
            ));
        }
        for serial in [46_551, 46_552] {
            harness.peer(serial).rtc.network_unavailable();
        }
        for (serial, other) in [(46_551, 46_552), (46_552, 46_551)] {
            harness
                .peer(serial)
                .rtc
                .wait_state(&harness.peer(other).id, WAIT, |state| !state.sendable)
                .await
                .unwrap();
        }
        for serial in [46_551, 46_552] {
            harness.peer(serial).rtc.network_available();
        }
        for (serial, other, pc_generation, ice_generation) in generations {
            let peer = harness.peer(serial);
            let remote = &harness.peer(other).id;
            peer.rtc
                .wait_ice_generation(remote, ice_generation, WAIT)
                .await
                .expect("recovery must advance the ICE generation");
            peer.rtc.wait_sendable(remote, WAIT).await;
            assert_eq!(
                peer.rtc.generation(remote).await,
                Some(pc_generation),
                "ICE restart must exercise rebind on the existing PeerConnection"
            );
            rpc(
                &harness,
                serial,
                other,
                &format!("allocation_restart_{restart}_{serial}"),
            )
            .await;
        }
        let remaining = wait_for_retirement(&server, &active).await;
        let current = server.get_allocations_info(None).await.unwrap();
        eprintln!(
            "restart={restart}/{restarts}: retired_still_present={remaining}, total={}",
            current.len()
        );
        assert_eq!(
            remaining, 0,
            "restart {restart} must release the old generation"
        );
        assert_eq!(
            current.len(),
            2,
            "restart {restart} must not grow allocations"
        );
        assert!(
            active.iter().all(|tuple| !current.contains_key(tuple)),
            "restart {restart} must replace both old transport tuples"
        );
        active = current.keys().copied().collect();
    }

    // Check the application's close path before shutting down the fixture:
    // server shutdown must never supply the cleanup that this test asserts.
    for serial in [46_551, 46_552] {
        harness.peer(serial).rtc.close_all().await;
    }
    let remaining = wait_for_retirement(&server, &active).await;
    let total = server.get_allocations_info(None).await.unwrap().len();
    eprintln!("close after {restarts} restarts: retired_still_present={remaining}, total={total}");
    harness.shutdown().await;
    turn::shutdown_turn_server(&server).await.unwrap();
    assert_eq!(
        remaining, 0,
        "retired transport allocations must be released before the 600-second server expiry"
    );
    assert_eq!(total, 0, "close must release every allocation");
}

async fn bounded_allocation_lifecycle(restarts: usize) {
    // The real server uses the upstream 600-second allocation lifetime. Bound
    // the whole case below it, so natural expiry cannot satisfy our assertions.
    tokio::time::timeout(Duration::from_secs(180), allocation_lifecycle(restarts))
        .await
        .expect("allocation lifecycle must finish before server expiry");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn peer_close_releases_allocations() {
    bounded_allocation_lifecycle(0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ice_restart_releases_old_allocations() {
    bounded_allocation_lifecycle(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn consecutive_ice_restarts_release_each_generation() {
    bounded_allocation_lifecycle(3).await;
}
