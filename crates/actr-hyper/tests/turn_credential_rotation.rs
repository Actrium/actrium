//! TURN credential rotation through webrtc-rs v0.20 `set_configuration`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_hyper::test_support::{TestHarness, install_test_crypto_provider, make_actor_id};
use actr_protocol::{ActrId, RpcEnvelope, TurnCredential};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::net::UdpSocket;
use turn_crate::auth::AuthHandler;
use webrtc::peer_connection::{
    RTCConfiguration, RTCConfigurationBuilder, RTCIceServer, RTCIceTransportPolicy,
};

const TURN_REALM: &str = "actr.test";
const TURN_SHARED_TEST_KEY: &str = "actrix-turn-rotation-integration-test-key";
const WAIT: Duration = Duration::from_secs(60);

type HmacSha1 = Hmac<Sha1>;

struct CountingAuthenticator {
    inner: Arc<turn::Authenticator>,
    auth_calls: Arc<AtomicUsize>,
}

impl CountingAuthenticator {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let auth_calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(
            turn::Authenticator::new(
                TURN_SHARED_TEST_KEY.to_owned(),
                "provider.example".to_owned(),
            )
            .expect("TURN authenticator"),
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

fn turn_credential(id: &ActrId, expires_at: u64) -> TurnCredential {
    // Username must match the scoped format enforced by `turn::Authenticator`:
    // `{expires_at}:realm={realm}:provider=provider.example:audience=actr.turn:actor={actr_id}`
    let username = format!(
        "{expires_at}:realm={}:provider=provider.example:audience=actr.turn:actor={id}",
        id.realm
    );
    let mut mac = HmacSha1::new_from_slice(TURN_SHARED_TEST_KEY.as_bytes())
        .expect("HMAC-SHA1 accepts the fixed test key");
    mac.update(username.as_bytes());
    TurnCredential {
        username,
        password: BASE64_STANDARD.encode(mac.finalize().into_bytes()),
        expires_at,
        protocol_version: 1,
        realm: id.realm.clone(),
        provider: "provider.example".to_string(),
        audience: "actr.turn".to_string(),
    }
}

fn relay_configuration(address: SocketAddr, credential: &TurnCredential) -> RTCConfiguration {
    RTCConfigurationBuilder::default()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![format!("turn:{address}?transport=udp")],
            username: credential.username.clone(),
            credential: credential.password.clone(),
        }])
        .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
        .build()
}

async fn rpc(harness: &TestHarness, from: u64, to: u64, id: &str) {
    let response = harness
        .peer(from)
        .gate
        .send_request(
            &harness.peer(to).id,
            RpcEnvelope {
                request_id: id.to_string(),
                route_key: "turn.rotation.test".to_string(),
                payload: Some(bytes::Bytes::from_static(b"ping")),
                timeout_ms: 20_000,
                ..Default::default()
            },
        )
        .await
        .expect("relay RPC");
    assert_eq!(response.as_ref(), b"pong");
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(WAIT, async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("condition should become true");
}

async fn start_turn(
    auth: Arc<dyn AuthHandler + Send + Sync>,
) -> (SocketAddr, turn_crate::server::Server) {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let address = socket.local_addr().unwrap();
    let server = turn::create_turn_server(
        socket,
        "127.0.0.1",
        TURN_REALM,
        (49_152, 65_535),
        auth,
        true,
    )
    .await
    .expect("TURN server");
    (address, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turn_credential_rotation_propagates_new_relay_credentials_to_actrix_turn() {
    install_test_crypto_provider();
    let authenticator = Arc::new(
        turn::Authenticator::new(
            TURN_SHARED_TEST_KEY.to_owned(),
            "provider.example".to_owned(),
        )
        .expect("TURN authenticator"),
    );
    let (turn_address, turn_server) = start_turn(authenticator.clone()).await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let offerer_id = make_actor_id(46_501);
    let answerer_id = make_actor_id(46_502);
    let old_offerer = turn_credential(&offerer_id, now + 300);
    let answerer_credential = turn_credential(&answerer_id, now + 300);

    let mut harness = TestHarness::new().await;
    harness
        .add_peer_with_rtc_configuration(46_501, relay_configuration(turn_address, &old_offerer))
        .await;
    harness
        .add_peer_with_rtc_configuration(
            46_502,
            relay_configuration(turn_address, &answerer_credential),
        )
        .await;
    harness.connect(46_501, 46_502).await;
    rpc(&harness, 46_501, 46_502, "turn_rotation_initial").await;
    wait_until(|| authenticator.cache_stats().0 >= 2).await;

    let new_credential = turn_credential(&offerer_id, now + 600);
    harness
        .peer(46_501)
        .rtc
        .refresh_configuration(
            &answerer_id,
            relay_configuration(turn_address, &new_credential),
        )
        .await
        .expect("v0.20 set_configuration command");
    harness.peer(46_501).rtc.network_unavailable();
    harness
        .peer(46_501)
        .rtc
        .wait_state(&answerer_id, WAIT, |state| !state.sendable)
        .await
        .expect("credential refresh recovery should gate sends before restart");
    harness.peer(46_501).rtc.network_available();
    harness
        .peer(46_501)
        .rtc
        .wait_sendable(&answerer_id, WAIT)
        .await;

    wait_until(|| authenticator.cache_stats().0 >= 3).await;
    rpc(&harness, 46_501, 46_502, "turn_rotation_after").await;
    harness.shutdown().await;
    turn::shutdown_turn_server(&turn_server).await.unwrap();
}

/// A re-allocation is detected by counting the server's TURN authentication
/// transactions. The v0.20 factory enables the upstream ICE-restart rebind
/// (webrtc-rs/webrtc#868), so a plain ICE restart drops and rebinds the UDP
/// transport; its old TURN allocation cannot be reused and a fresh allocation
/// must authenticate again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plain_ice_restart_rebinds_and_refreshes_relay_allocation() {
    install_test_crypto_provider();
    let (counting, auth_calls) = CountingAuthenticator::new();
    let (turn_address, turn_server) = start_turn(Arc::new(counting)).await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let offerer_id = make_actor_id(46_521);
    let answerer_id = make_actor_id(46_522);

    let mut harness = TestHarness::new().await;
    harness
        .add_peer_with_rtc_configuration(
            46_521,
            relay_configuration(turn_address, &turn_credential(&offerer_id, now + 300)),
        )
        .await;
    harness
        .add_peer_with_rtc_configuration(
            46_522,
            relay_configuration(turn_address, &turn_credential(&answerer_id, now + 300)),
        )
        .await;
    harness.connect(46_521, 46_522).await;
    rpc(&harness, 46_521, 46_522, "plain_restart_initial").await;
    let auth_before = auth_calls.load(Ordering::SeqCst);

    harness.peer(46_521).rtc.network_unavailable();
    harness.peer(46_522).rtc.network_unavailable();
    harness
        .peer(46_521)
        .rtc
        .wait_state(&answerer_id, WAIT, |state| !state.sendable)
        .await
        .expect("offerer should gate sends while ICE restart is pending");
    harness
        .peer(46_522)
        .rtc
        .wait_state(&offerer_id, WAIT, |state| !state.sendable)
        .await
        .expect("answerer should gate sends while ICE restart is pending");
    harness.peer(46_521).rtc.network_available();
    harness.peer(46_522).rtc.network_available();
    harness
        .peer(46_521)
        .rtc
        .wait_sendable(&answerer_id, WAIT)
        .await;
    harness
        .peer(46_522)
        .rtc
        .wait_sendable(&offerer_id, WAIT)
        .await;
    rpc(&harness, 46_521, 46_522, "plain_restart_after").await;

    let auth_after = auth_calls.load(Ordering::SeqCst);
    assert!(
        auth_after > auth_before,
        "plain ICE restart must refresh the TURN allocation after the rebind replaces the UDP transport"
    );
    harness.shutdown().await;
    turn::shutdown_turn_server(&turn_server).await.unwrap();
}
