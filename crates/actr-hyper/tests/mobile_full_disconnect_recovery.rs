//! Mobile full-disconnect recovery through the webrtc-rs v0.20 architecture.
//!
//! The mobile endpoint is exercised as both offerer and answerer. Temporary
//! loss must use soft recovery and preserve `pc_generation`; an interface
//! change must use hard recovery and publish a newer generation. Readiness is
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
async fn test_mobile_half_open_65s_semantics_rebuilds_webrtc() {
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
        let old_server_generation = harness
            .peer(case.server_serial)
            .rtc
            .generation(&mobile_id)
            .await
            .expect("initial server generation");

        let vnet = harness.vnet.as_ref().expect("VNet harness");
        vnet.block_network();
        harness.server.pause_forwarding();
        harness.peer(case.mobile_serial).rtc.network_unavailable();
        harness.peer(case.server_serial).rtc.network_unavailable();

        // A changed local interface is the RFC hard-recovery trigger. Both
        // endpoints receive the same platform fact, as they would after a
        // mobile Wi-Fi/cellular transition.
        vnet.unblock_network();
        harness.server.resume_forwarding();
        harness.peer(case.mobile_serial).rtc.local_address_changed();
        harness.peer(case.server_serial).rtc.local_address_changed();
        harness.peer(case.mobile_serial).rtc.network_available();
        harness.peer(case.server_serial).rtc.network_available();

        // Event delivery is asynchronous. Waiting for `sendable` alone could
        // observe the still-connected old generation before the hard-recovery
        // facts reach the controller, so require both generation replacement
        // and restored readiness.
        let mobile_state = harness
            .peer(case.mobile_serial)
            .rtc
            .wait_state(&server_id, WAIT, |state| {
                state.sendable && state.pc_generation > old_mobile_generation
            })
            .await
            .expect("mobile should rebuild and become sendable");
        harness
            .peer(case.server_serial)
            .rtc
            .wait_state(&mobile_id, WAIT, |state| {
                state.sendable && state.pc_generation > old_server_generation
            })
            .await
            .expect("server should rebuild and become sendable");

        assert!(
            mobile_state.pc_generation > old_mobile_generation,
            "{} interface change must rebuild the PeerConnection",
            case.name
        );
        assert_rpc_both_directions(&harness, case, "hard_recovery").await;
        harness.shutdown().await;
    }
}
