//! TURN credential rotation integration test.
//!
//! The peers are restricted to relay candidates and authenticate against the
//! real TURN implementation used by Actrix.  After a credential refresh, the
//! test observes a fresh TURN authentication and the restart request issued by
//! the active-peer rotation watcher.

use actr_config::{IceServer, IceTransportPolicy};
use actr_hyper::{
    inbound::MediaFrameRegistry,
    lifecycle::CredentialState,
    outbound::PeerGate,
    test_support::{
        TestSignalingServer, dummy_credential, install_test_crypto_provider, make_actor_id,
        spawn_echo_responder, spawn_response_receiver,
    },
    transport::{DefaultWireBuilder, DefaultWireBuilderConfig, PeerTransport},
    wire::webrtc::{SignalingClient, WebRtcConfig, WebRtcCoordinator, WebSocketSignalingClient},
};
use actr_protocol::{ActrId, Direction, RpcEnvelope, TurnCredential};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::UdpSocket, task::JoinHandle};
use turn_crate::auth::AuthHandler;

const TURN_REALM: &str = "actr.test";
const TURN_SHARED_TEST_KEY: &str = "turn-rotation-integration-key";
const TEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Bound on how long the relay path may take to settle after an ICE restart.
/// webrtc-rs v0.20 refreshes an existing allocation when TURN credentials
/// change, so this does not intentionally wait for a 437/re-allocation path.
const ROTATION_RECOVERY_TIMEOUT: Duration = Duration::from_secs(60);

type HmacSha1 = Hmac<Sha1>;

/// Install a tracing subscriber that also bridges the `log` records emitted by
/// the webrtc-rs / TURN crates, so a failing run can show the TURN Allocate /
/// Refresh exchange on stderr. Level comes from `RUST_LOG`; the default is
/// quiet — use e.g.
/// `RUST_LOG=info,turn=debug,rtc_turn=debug,rtc_ice=debug` to trace the relay
/// re-allocation during rotation.  No-op if a subscriber is already set.
fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_log::LogTracer::init();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}

/// An [`AuthHandler`] that delegates to the real Actrix [`turn::Authenticator`]
/// while counting how many TURN authentication transactions the server
/// performs.  Unlike [`turn::Authenticator::cache_stats`] (which dedupes by
/// username and so cannot tell "reused an allocation" apart from
/// "re-allocated with the same credential"), an invocation count rises on every
/// Allocate/Refresh/ChannelBind — so a *fresh* allocation is always visible.
struct CountingAuthenticator {
    inner: Arc<turn::Authenticator>,
    auth_calls: Arc<AtomicUsize>,
}

impl CountingAuthenticator {
    fn new(shared_key: &str) -> (Self, Arc<AtomicUsize>) {
        let auth_calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(
            turn::Authenticator::new(shared_key.to_owned())
                .expect("Actrix TURN authenticator should initialize"),
        );
        (
            Self {
                inner,
                auth_calls: auth_calls.clone(),
            },
            auth_calls,
        )
    }
}

impl AuthHandler for CountingAuthenticator {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
    ) -> Result<Vec<u8>, turn_crate::Error> {
        self.auth_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.auth_handle(username, realm, src_addr)
    }
}

struct RelayPeer {
    id: ActrId,
    credential_state: CredentialState,
    coordinator: Arc<WebRtcCoordinator>,
    signaling_client: Arc<dyn SignalingClient>,
    gate: Arc<PeerGate>,
    transport: Arc<PeerTransport>,
}

impl RelayPeer {
    async fn shutdown(self) {
        self.coordinator.shutdown_background_tasks().await;
        let _ = self.transport.close_all().await;
        let _ = self.coordinator.close_all_peers().await;
        let _ = self.signaling_client.disconnect().await;
    }
}

fn make_turn_credential(id: &ActrId, expires_at: u64) -> TurnCredential {
    let username = format!("{expires_at}:{id}");
    let mut mac = HmacSha1::new_from_slice(TURN_SHARED_TEST_KEY.as_bytes())
        .expect("HMAC-SHA1 accepts the fixed test key");
    mac.update(username.as_bytes());

    TurnCredential {
        username,
        password: BASE64_STANDARD.encode(mac.finalize().into_bytes()),
        expires_at,
    }
}

fn relay_only_config(turn_address: std::net::SocketAddr) -> WebRtcConfig {
    WebRtcConfig {
        ice_servers: vec![IceServer {
            urls: vec![format!("turn:{turn_address}?transport=udp")],
            username: None,
            credential: None,
        }],
        ice_transport_policy: IceTransportPolicy::Relay,
        ..Default::default()
    }
}

async fn create_relay_peer(
    id: ActrId,
    server_url: &str,
    webrtc_config: WebRtcConfig,
    turn_credential: TurnCredential,
) -> RelayPeer {
    let credential_state = CredentialState::new(dummy_credential(), None, Some(turn_credential));
    let signaling_client = WebSocketSignalingClient::connect_to_with_identity(
        server_url,
        id.clone(),
        credential_state.clone(),
    )
    .await
    .expect("peer should connect to the test signaling server");
    let signaling_client = signaling_client as Arc<dyn SignalingClient>;
    let coordinator = Arc::new(WebRtcCoordinator::new(
        id.clone(),
        credential_state.clone(),
        signaling_client.clone(),
        webrtc_config,
        Arc::new(MediaFrameRegistry::new()),
    ));
    coordinator
        .clone()
        .start()
        .await
        .expect("peer coordinator should start");

    let wire_builder = Arc::new(DefaultWireBuilder::new(
        Some(coordinator.clone()),
        DefaultWireBuilderConfig::default(),
    ));
    let transport = Arc::new(PeerTransport::new(id.clone(), wire_builder));
    let gate = Arc::new(PeerGate::new(transport.clone(), Some(coordinator.clone())));

    RelayPeer {
        id,
        credential_state,
        coordinator,
        signaling_client,
        gate,
        transport,
    }
}

fn request(request_id: &str) -> RpcEnvelope {
    RpcEnvelope {
        request_id: request_id.to_owned(),
        route_key: "turn.rotation.test".to_owned(),
        payload: Some(bytes::Bytes::from_static(b"ping")),
        direction: Some(Direction::Request as i32),
        timeout_ms: TEST_TIMEOUT.as_millis() as i64,
        ..Default::default()
    }
}

async fn assert_rpc_succeeds(peer: &RelayPeer, target: &ActrId, request_id: &str) {
    let response = tokio::time::timeout(
        TEST_TIMEOUT,
        peer.gate.send_request(target, request(request_id)),
    )
    .await
    .expect("relay RPC should finish before its deadline")
    .expect("relay RPC should succeed");
    assert_eq!(response.as_ref(), b"pong");
}

/// Assert that the relay data path carries RPC traffic after a credential
/// rotation, retrying until the ICE restart and in-place TURN refresh settle.
///
/// Traffic can fail transiently while the new ICE generation is negotiated.
/// Each attempt uses a fresh request id so a timed-out pending entry never
/// collides with a retry.
async fn assert_rpc_eventually_succeeds(peer: &RelayPeer, target: &ActrId, base_request_id: &str) {
    tokio::time::timeout(ROTATION_RECOVERY_TIMEOUT, async {
        let mut attempt = 0u32;
        loop {
            let request_id = format!("{base_request_id}_{attempt}");
            attempt += 1;
            match tokio::time::timeout(
                Duration::from_secs(2),
                peer.gate.send_request(target, request(&request_id)),
            )
            .await
            {
                Ok(Ok(response)) if response.as_ref() == b"pong" => break,
                Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    })
    .await
    .expect("relay RPC should recover after the TURN credential rotation");
}

async fn wait_for_turn_auth_entries(expected_minimum: usize) {
    tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            if turn::Authenticator::cache_stats().0 >= expected_minimum {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("Actrix TURN server should authenticate the expected credentials");
}

async fn wait_for_ice_restart(server: &TestSignalingServer) {
    tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            if server.get_ice_restart_count() >= 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("credential update should initiate an ICE restart");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turn_credential_rotation_propagates_new_relay_credentials_to_actrix_turn() {
    install_test_crypto_provider();
    init_logging();
    turn::Authenticator::clear_cache();

    let turn_socket = Arc::new(
        UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("test TURN socket should bind"),
    );
    let turn_address = turn_socket
        .local_addr()
        .expect("test TURN socket should report its address");
    let turn_server = turn::create_turn_server(
        turn_socket,
        "127.0.0.1",
        TURN_REALM,
        Arc::new(
            turn::Authenticator::new(TURN_SHARED_TEST_KEY.to_owned())
                .expect("Actrix TURN authenticator should initialize"),
        ),
    )
    .await
    .expect("Actrix TURN service should start");

    let mut signaling_server = TestSignalingServer::start()
        .await
        .expect("test signaling server should start");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_secs();
    let offerer_id = make_actor_id(46_501);
    let answerer_id = make_actor_id(46_502);
    let config = relay_only_config(turn_address);
    assert_eq!(config.ice_transport_policy, IceTransportPolicy::Relay);

    let offerer = create_relay_peer(
        offerer_id.clone(),
        &signaling_server.url(),
        config.clone(),
        make_turn_credential(&offerer_id, now + 300),
    )
    .await;
    let answerer = create_relay_peer(
        answerer_id.clone(),
        &signaling_server.url(),
        config,
        make_turn_credential(&answerer_id, now + 300),
    )
    .await;

    let echo_task: JoinHandle<()> = spawn_echo_responder(
        answerer.coordinator.clone(),
        answerer.gate.clone(),
        "turn-answerer",
    );
    let response_task: JoinHandle<()> = spawn_response_receiver(
        offerer.coordinator.clone(),
        offerer.gate.clone(),
        "turn-offerer",
    );

    assert_rpc_succeeds(&offerer, &answerer.id, "turn_rotation_initial").await;
    wait_for_turn_auth_entries(2).await;

    signaling_server.reset_counters();
    offerer
        .credential_state
        .update_for_test(
            dummy_credential(),
            None,
            Some(make_turn_credential(&offerer.id, now + 600)),
        )
        .await;

    wait_for_ice_restart(&signaling_server).await;
    // The new username must authenticate against Actrix during the in-place
    // TURN allocation refresh. It becomes the third unique authenticator cache
    // entry (offerer old, answerer old, offerer refreshed).
    wait_for_turn_auth_entries(3).await;

    // The assertion under test: the existing relay path carries RPC traffic
    // again after configuration refresh plus ICE restart, without relying on
    // the old 437/rebuild behavior.
    assert_rpc_eventually_succeeds(&offerer, &answerer.id, "turn_rotation_after").await;

    echo_task.abort();
    response_task.abort();
    let _ = echo_task.await;
    let _ = response_task.await;
    offerer.shutdown().await;
    answerer.shutdown().await;
    signaling_server.shutdown().await;
    turn::shutdown_turn_server(&turn_server)
        .await
        .expect("Actrix TURN service should stop");
}

/// A plain ICE restart (no credential change) must reuse the existing TURN
/// allocation rather than tear it down and re-allocate on a fresh socket.
///
/// A re-allocation is detected by counting the server's TURN authentication
/// transactions: reusing the allocation performs no new Allocate (and, because
/// the channel bindings/permissions are still within their lifetimes, no new
/// authenticated transaction at all), so the count stays flat — whereas a
/// re-allocation would authenticate again.  This is the boundary the
/// fresh-relay-socket rebinding must not cross: the rebind is gated on a
/// configuration change, so a plain restart keeps the existing socket and
/// allocation and never risks a same-5-tuple 437.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plain_ice_restart_reuses_relay_allocation_without_reallocating() {
    install_test_crypto_provider();
    init_logging();
    turn::Authenticator::clear_cache();

    let (counting_auth, auth_calls) = CountingAuthenticator::new(TURN_SHARED_TEST_KEY);

    let turn_socket = Arc::new(
        UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("test TURN socket should bind"),
    );
    let turn_address = turn_socket
        .local_addr()
        .expect("test TURN socket should report its address");
    let turn_server = turn::create_turn_server(
        turn_socket,
        "127.0.0.1",
        TURN_REALM,
        Arc::new(counting_auth),
    )
    .await
    .expect("Actrix TURN service should start");

    let mut signaling_server = TestSignalingServer::start()
        .await
        .expect("test signaling server should start");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_secs();
    let offerer_id = make_actor_id(46_521);
    let answerer_id = make_actor_id(46_522);
    let config = relay_only_config(turn_address);
    assert_eq!(config.ice_transport_policy, IceTransportPolicy::Relay);

    let offerer = create_relay_peer(
        offerer_id.clone(),
        &signaling_server.url(),
        config.clone(),
        make_turn_credential(&offerer_id, now + 300),
    )
    .await;
    let answerer = create_relay_peer(
        answerer_id.clone(),
        &signaling_server.url(),
        config,
        make_turn_credential(&answerer_id, now + 300),
    )
    .await;

    let echo_task: JoinHandle<()> = spawn_echo_responder(
        answerer.coordinator.clone(),
        answerer.gate.clone(),
        "turn-answerer",
    );
    let response_task: JoinHandle<()> = spawn_response_receiver(
        offerer.coordinator.clone(),
        offerer.gate.clone(),
        "turn-offerer",
    );

    // Establish the relay data path before the restart.
    assert_rpc_succeeds(&offerer, &answerer.id, "plain_restart_initial").await;

    let auth_before = auth_calls.load(Ordering::SeqCst);

    // A plain ICE restart: no credential change, so the relayer keeps the
    // existing allocation (it re-emits the existing relay candidate) and the
    // driver must not tear down / rebind the relay socket.
    offerer
        .coordinator
        .restart_ice(&answerer.id)
        .await
        .expect("plain ICE restart should be accepted");

    // The relay data path must keep carrying RPC traffic after the restart.
    assert_rpc_eventually_succeeds(&offerer, &answerer.id, "plain_restart_after").await;

    // The decisive assertion: the restart performed no new TURN allocation,
    // which would have forced at least one more authenticated transaction.
    let auth_after = auth_calls.load(Ordering::SeqCst);
    assert_eq!(
        auth_after, auth_before,
        "plain ICE restart must reuse the existing TURN allocation (no new TURN authentication)"
    );

    echo_task.abort();
    response_task.abort();
    let _ = echo_task.await;
    let _ = response_task.await;
    offerer.shutdown().await;
    answerer.shutdown().await;
    signaling_server.shutdown().await;
    turn::shutdown_turn_server(&turn_server)
        .await
        .expect("Actrix TURN service should stop");
}
