//! Mobile full-disconnect recovery through the webrtc-rs architecture.
//!
//! The mobile endpoint is exercised as both offerer and answerer. Temporary
//! loss and interface changes use soft recovery and preserve `pc_generation`;
//! explicit force-reconnect remains the hard-recovery path. Readiness is
//! asserted exclusively through the controller's authoritative snapshot.

use std::time::Duration;

use actr_hyper::test_support::{TestHarness, install_test_crypto_provider};
use actr_protocol::RpcEnvelope;

const WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
struct RoleCase {
    name: &'static str,
    mobile_serial: u64,
    server_serial: u64,
}

const ROLE_CASES: [RoleCase; 2] = [
    RoleCase {
        name: "mobile_offerer",
        mobile_serial: 100,
        server_serial: 200,
    },
    RoleCase {
        name: "mobile_answerer",
        mobile_serial: 200,
        server_serial: 100,
    },
];

async fn setup(case: RoleCase) -> TestHarness {
    let mut harness = TestHarness::with_vnet().await;
    harness
        .add_peer(case.mobile_serial.min(case.server_serial))
        .await;
    harness
        .add_peer(case.mobile_serial.max(case.server_serial))
        .await;
    harness
        .connect(
            case.mobile_serial.min(case.server_serial),
            case.mobile_serial.max(case.server_serial),
        )
        .await;
    harness
}

async fn assert_rpc_both_directions(harness: &TestHarness, case: RoleCase, label: &str) {
    for (from, to, suffix) in [
        (case.mobile_serial, case.server_serial, "mobile_to_server"),
        (case.server_serial, case.mobile_serial, "server_to_mobile"),
    ] {
        let request = RpcEnvelope {
            request_id: format!("{label}_{suffix}"),
            route_key: "test.ping".to_string(),
            payload: Some(bytes::Bytes::from_static(b"ping")),
            timeout_ms: 15_000,
            ..Default::default()
        };
        let response = harness
            .peer(from)
            .gate
            .send_request(&harness.peer(to).id, request)
            .await
            .unwrap_or_else(|error| panic!("{label} {suffix} failed: {error}"));
        assert_eq!(response.as_ref(), b"pong");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mobile_half_open_15s_semantics_recovers_with_ice_restart() {
    install_test_crypto_provider();

    for case in ROLE_CASES {
        let harness = setup(case).await;
        let server_id = harness.peer(case.server_serial).id.clone();
        let mobile_id = harness.peer(case.mobile_serial).id.clone();
        let old_generation = harness
            .peer(case.mobile_serial)
            .rtc
            .generation(&server_id)
            .await
            .expect("initial controller generation");

        harness.simulate_disconnect();
        let recovering = harness
            .peer(case.mobile_serial)
            .rtc
            .wait_state(&server_id, WAIT, |state| !state.sendable)
            .await
            .expect("mobile snapshot should become non-sendable");
        assert!(!recovering.sendable, "{} must gate sends", case.name);

        harness.simulate_reconnect();
        let mobile_state = harness
            .peer(case.mobile_serial)
            .rtc
            .wait_sendable(&server_id, WAIT)
            .await;
        harness
            .peer(case.server_serial)
            .rtc
            .wait_sendable(&mobile_id, WAIT)
            .await;

        assert_eq!(
            mobile_state.pc_generation, old_generation,
            "{} temporary loss must use soft ICE recovery",
            case.name
        );
        assert_rpc_both_directions(&harness, case, "soft_recovery").await;
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mobile_interface_change_recovers_with_ice_restart() {
    install_test_crypto_provider();

    for case in ROLE_CASES {
        let harness = setup(case).await;
        let server_id = harness.peer(case.server_serial).id.clone();
        let mobile_id = harness.peer(case.mobile_serial).id.clone();
        let old_mobile_generation = harness
            .peer(case.mobile_serial)
            .rtc
            .generation(&server_id)
            .await
            .expect("initial mobile generation");
        let old_mobile_ice_generation = harness
            .peer(case.mobile_serial)
            .rtc
            .ice_generation(&server_id)
            .await
            .expect("initial mobile ICE generation");
        let old_server_generation = harness
            .peer(case.server_serial)
            .rtc
            .generation(&mobile_id)
            .await
            .expect("initial server generation");
        let old_server_ice_generation = harness
            .peer(case.server_serial)
            .rtc
            .ice_generation(&mobile_id)
            .await
            .expect("initial server ICE generation");

        let vnet = harness.vnet.as_ref().expect("VNet harness");
        vnet.block_network();
        harness.server.pause_forwarding();
        harness.peer(case.mobile_serial).rtc.network_unavailable();
        harness.peer(case.server_serial).rtc.network_unavailable();

        // Both endpoints receive the same platform fact, as they would after
        // a mobile Wi-Fi/cellular transition. The controller keeps the
        // current PeerConnection and invokes ICE rebind/re-gather in place.
        vnet.unblock_network();
        harness.server.resume_forwarding();
        harness.peer(case.mobile_serial).rtc.local_address_changed();
        harness.peer(case.server_serial).rtc.local_address_changed();
        harness.peer(case.mobile_serial).rtc.network_available();
        harness.peer(case.server_serial).rtc.network_available();

        // Event delivery is asynchronous; wait for the authoritative sendable
        // edge and assert that the physical PC generation is reused.
        harness
            .peer(case.mobile_serial)
            .rtc
            .wait_state(&server_id, WAIT, |state| !state.sendable)
            .await
            .expect("mobile should enter soft recovery");
        harness
            .peer(case.server_serial)
            .rtc
            .wait_state(&mobile_id, WAIT, |state| !state.sendable)
            .await
            .expect("server should enter soft recovery");
        let mobile_state = harness
            .peer(case.mobile_serial)
            .rtc
            .wait_sendable(&server_id, WAIT)
            .await;
        harness
            .peer(case.server_serial)
            .rtc
            .wait_sendable(&mobile_id, WAIT)
            .await;

        assert!(
            mobile_state.pc_generation == old_mobile_generation,
            "{} interface change must use same-generation ICE recovery",
            case.name
        );
        let mobile_ice_generation = harness
            .peer(case.mobile_serial)
            .rtc
            .wait_ice_generation(&server_id, old_mobile_ice_generation, WAIT)
            .await
            .expect("mobile ICE generation after soft recovery");
        assert!(
            mobile_ice_generation > old_mobile_ice_generation,
            "{} interface change must advance the mobile ICE generation",
            case.name
        );
        let server_state = harness
            .peer(case.server_serial)
            .rtc
            .wait_sendable(&mobile_id, WAIT)
            .await;
        assert_eq!(
            server_state.pc_generation, old_server_generation,
            "{} remote generation must also remain stable",
            case.name
        );
        let server_ice_generation = harness
            .peer(case.server_serial)
            .rtc
            .wait_ice_generation(&mobile_id, old_server_ice_generation, WAIT)
            .await
            .expect("server ICE generation after soft recovery");
        assert!(
            server_ice_generation > old_server_ice_generation,
            "{} interface change must advance the server ICE generation",
            case.name
        );
        assert_rpc_both_directions(&harness, case, "address_change_soft_recovery").await;
        harness.shutdown().await;
    }
}
