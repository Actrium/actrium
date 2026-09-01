//! Regression tests for #423: a successful soft renewal must be visible to
//! the very next signaling handshake.
//!
//! Root cause being guarded: `Node::start` used to construct two independent
//! `CredentialState` instances from the initial credential — one installed on
//! the signaling client, another stored on the node and passed to
//! `HardRebindHandles`. Soft renewal only updated the `HardRebindHandles`
//! instance, so after a signaling disconnect the reconnect URL still carried
//! the initial, possibly-expired credential and looped on 401.
//!
//! The fix shares a single `CredentialState` between the signaling client and
//! `HardRebindHandles` (one `Arc<RwLock<...>>`). These tests pin that
//! invariant: after the shared authority is updated the way a soft renewal
//! updates it, the handshake URL built by the signaling client carries the
//! renewed credential — both before and after a real disconnect/reconnect
//! against a live test signaling server.

use actr_hyper::lifecycle::CredentialState;
use actr_hyper::test_support::{TestSignalingServer, install_test_crypto_provider, make_actor_id};
use actr_hyper::wire::webrtc::{
    InboundQueueConfig, ReconnectConfig, SignalingClient, SignalingConfig, WebSocketSignalingClient,
};
use actr_protocol::{AIdCredential, TurnCredential};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Initialize tracing for test output (idempotent).
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();
}

/// Helper: create a `SignalingConfig` pointing at the given test server URL
/// with a short, fast reconnect policy so the disconnect/reconnect cycle
/// completes quickly under test.
fn make_config(server_url: &str) -> SignalingConfig {
    SignalingConfig {
        server_url: Url::parse(server_url).expect("valid URL"),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig {
            enabled: true,
            max_attempts: 5,
            initial_delay: 1,
            max_delay: 4,
            backoff_multiplier: 2.0,
        },
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }
}

/// Build a credential whose `claims` field is exactly the given bytes, so the
/// test can assert which credential a handshake URL carries by decoding the
/// `claims` query parameter.
fn make_credential(claims: &[u8]) -> AIdCredential {
    AIdCredential {
        key_id: 7,
        claims: bytes::Bytes::copy_from_slice(claims),
        signature: bytes::Bytes::from(vec![0u8; 64]),
    }
}

/// Extract the base64 `claims` query parameter from a handshake URL.
fn claims_b64_from_url(url: &Url) -> String {
    url.query_pairs()
        .find(|(k, _)| k == "claims")
        .map(|(_, v)| v.to_string())
        .expect("handshake URL must carry a claims query parameter")
}

/// Assert that the handshake URL carries exactly `expected_claims` in its
/// `claims` query parameter.
fn assert_url_carries_claims(url: &Url, expected_claims: &[u8]) {
    let got = claims_b64_from_url(url);
    let want = BASE64.encode(expected_claims);
    assert_eq!(
        got, want,
        "handshake URL claims do not match the expected credential"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 1: a soft-renewal-style update to the shared credential authority is
// visible to the next signaling handshake, and survives a real
// disconnect/reconnect against a live test server.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#[tokio::test]
async fn soft_renewal_visible_to_next_signaling_handshake() {
    init_tracing();
    install_test_crypto_provider();

    let mut server = TestSignalingServer::start().await.expect("server starts");
    let client = Arc::new(WebSocketSignalingClient::new(make_config(&server.url())));

    let actor_id = make_actor_id(423);

    // Initial registration: install actor_id + the shared credential state on
    // the signaling client BEFORE connecting (mirrors `Node::start` step 3).
    let cred_a = make_credential(b"claims-initial");
    let shared_state = CredentialState::new(
        cred_a.clone(),
        Some(prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        }),
        Some(TurnCredential::default()),
    );
    client.set_actor_id(actor_id.clone()).await;
    client.set_credential_state(shared_state.clone()).await;

    // ── Step 1: initial connect — handshake carries the initial credential.
    client.connect().await.expect("initial connect succeeds");
    assert!(
        server.get_connection_count() >= 1,
        "server should have observed the initial connection"
    );
    assert_url_carries_claims(&client.test_handshake_url().await, cred_a.claims.as_ref());

    // ── Step 2: soft renewal succeeds — `HardRebindHandles.credential_state`
    // (a clone of the SAME shared Arc after the #423 fix) is updated in place.
    let cred_b = make_credential(b"claims-renewed");
    shared_state
        .update_for_test(
            cred_b.clone(),
            Some(prost_types::Timestamp {
                seconds: 2,
                nanos: 0,
            }),
            Some(TurnCredential::default()),
        )
        .await;

    // The very next handshake must carry the renewed credential, never the
    // stale initial one. This is the core invariant the fix restores.
    assert_url_carries_claims(&client.test_handshake_url().await, cred_b.claims.as_ref());

    // ── Step 3: force a signaling disconnect, then reconnect (no restart).
    client
        .disconnect()
        .await
        .expect("disconnect should succeed");
    // Give the server a moment to register the disconnect.
    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect()
        .await
        .expect("reconnect succeeds with the renewed credential");
    assert!(
        client.is_connected(),
        "client should be connected after reconnect"
    );

    // The reconnect handshake still carries the renewed credential — the
    // signaling client did not fall back to the initial credential.
    assert_url_carries_claims(&client.test_handshake_url().await, cred_b.claims.as_ref());

    client.disconnect().await.ok();
    server.shutdown().await;
    tracing::info!("✅ soft_renewal_visible_to_next_signaling_handshake passed");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 2: documents the bug mechanism — a SECOND, independently-constructed
// `CredentialState` does NOT propagate to the signaling client. This is why
// the fix shares one instance instead of calling `CredentialState::new`
// twice; do not reintroduce a second construction on the start path.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#[tokio::test]
async fn separate_credential_state_does_not_propagate_to_handshake() {
    init_tracing();
    install_test_crypto_provider();

    let mut server = TestSignalingServer::start().await.expect("server starts");
    let client = Arc::new(WebSocketSignalingClient::new(make_config(&server.url())));

    let actor_id = make_actor_id(424);

    // The signaling client holds its OWN credential state (clone of state_client).
    let cred_a = make_credential(b"claims-initial");
    let state_client = CredentialState::new(
        cred_a.clone(),
        Some(prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        }),
        Some(TurnCredential::default()),
    );
    client.set_actor_id(actor_id).await;
    client.set_credential_state(state_client.clone()).await;
    client.connect().await.expect("connect succeeds");
    assert_url_carries_claims(&client.test_handshake_url().await, cred_a.claims.as_ref());

    // A second, independently-constructed state — exactly what the bug did.
    let cred_b = make_credential(b"claims-renewed");
    let state_handles = CredentialState::new(
        cred_b.clone(),
        Some(prost_types::Timestamp {
            seconds: 2,
            nanos: 0,
        }),
        Some(TurnCredential::default()),
    );
    // Soft-renewal updates the "handles" state only.
    state_handles
        .update_for_test(
            cred_b.clone(),
            Some(prost_types::Timestamp {
                seconds: 3,
                nanos: 0,
            }),
            Some(TurnCredential::default()),
        )
        .await;

    // The signaling client still carries the INITIAL credential — the update
    // never reached it because the two states share no `Arc`. This is the
    // "alive but undiscoverable" 401-loop root cause.
    assert_url_carries_claims(&client.test_handshake_url().await, cred_a.claims.as_ref());

    client.disconnect().await.ok();
    server.shutdown().await;
    tracing::info!("✅ separate_credential_state_does_not_propagate_to_handshake passed");
}
