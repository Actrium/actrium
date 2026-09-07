//! RTC recovery while the production WebSocket signaling path is disrupted.

use std::time::Duration;

use actr_hyper::test_support::{TestHarness, TestRtcRole, install_test_crypto_provider};

const WAIT: Duration = Duration::from_secs(35);

async fn connected(with_vnet: bool, a: u64, b: u64) -> TestHarness {
    let mut harness = if with_vnet {
        TestHarness::with_vnet().await
    } else {
        TestHarness::new().await
    };
    harness.add_peer(a).await;
    harness.add_peer(b).await;
    harness.connect(a, b).await;
    harness
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_full_ice_restart_with_real_signaling() {
    install_test_crypto_provider();
    let mut harness = connected(false, 100, 200).await;
    let target = harness.peer(200).id.clone();

    harness.server.shutdown().await;
    harness.peer(100).rtc.network_unavailable();
    harness.peer(100).rtc.network_available();

    let state = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(10), |state| !state.sendable)
        .await
        .expect("signaling loss should keep the peer non-sendable");
    assert!(!state.sendable);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ice_restart_with_paused_forwarding() {
    install_test_crypto_provider();
    let harness = connected(false, 101, 202).await;
    let a_id = harness.peer(101).id.clone();
    let b_id = harness.peer(202).id.clone();

    harness.server.pause_forwarding();
    harness.peer(101).rtc.network_unavailable();
    harness.peer(202).rtc.network_unavailable();
    harness.peer(101).rtc.network_available();
    harness.peer(202).rtc.network_available();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!harness.peer(101).rtc.is_sendable(&b_id).await);

    harness.server.resume_forwarding();
    // Re-publishing an authoritative available fact wakes recovery after the
    // signaling transport becomes usable again.
    harness.peer(101).rtc.network_available();
    harness.peer(202).rtc.network_available();
    tokio::join!(
        harness.peer(101).rtc.wait_sendable(&b_id, WAIT),
        harness.peer(202).rtc.wait_sendable(&a_id, WAIT),
    );
    tokio::time::timeout(Duration::from_secs(30), harness.shutdown())
        .await
        .expect("harness shutdown must converge");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_request_wakes_inflight_ice_restart_retry() {
    install_test_crypto_provider();
    let harness = connected(false, 103, 204).await;
    let offerer = harness.peer(103).id.clone();
    let answerer = harness.peer(204).id.clone();

    harness.server.pause_forwarding();
    harness.peer(103).rtc.network_unavailable();
    harness.peer(103).rtc.network_available();
    tokio::time::sleep(Duration::from_millis(250)).await;

    harness.server.resume_forwarding();
    let started = tokio::time::Instant::now();
    harness.peer(204).rtc.network_unavailable();
    harness.peer(204).rtc.network_available();
    harness.peer(103).rtc.network_available();
    harness.peer(103).rtc.wait_sendable(&answerer, WAIT).await;
    harness.peer(204).rtc.wait_sendable(&offerer, WAIT).await;
    assert!(started.elapsed() < WAIT);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_answerer_retransmits_lost_ice_restart_request() {
    install_test_crypto_provider();
    let harness = connected(false, 105, 206).await;
    let offerer = harness.peer(105).id.clone();
    let answerer = harness.peer(206).id.clone();
    let old = harness.peer(206).rtc.state(&offerer).await.unwrap();

    harness.server.pause_forwarding();
    harness.peer(206).rtc.network_unavailable();
    harness.peer(206).rtc.network_available();
    tokio::time::sleep(Duration::from_secs(1)).await;
    harness.server.resume_forwarding();
    harness.peer(206).rtc.network_available();
    harness.peer(105).rtc.network_available();

    let answerer_state = harness.peer(206).rtc.wait_sendable(&offerer, WAIT).await;
    harness.peer(105).rtc.wait_sendable(&answerer, WAIT).await;
    assert_eq!(answerer_state.pc_generation, old.pc_generation);
    assert!(answerer_state.revision > old.revision);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_offerer_initiated_rebuild_recovers_after_restart_disruption() {
    install_test_crypto_provider();
    let harness = connected(true, 107, 208).await;
    let first_id = harness.peer(107).id.clone();
    let second_id = harness.peer(208).id.clone();
    let first_role = harness.peer(107).rtc.role(&second_id).await.unwrap();
    let (offerer_serial, offerer, answerer_serial, answerer) = if first_role == TestRtcRole::Offerer
    {
        (107, first_id, 208, second_id)
    } else {
        (208, second_id, 107, first_id)
    };
    let old_generation = harness
        .peer(offerer_serial)
        .rtc
        .generation(&answerer)
        .await
        .unwrap();

    let vnet = harness.vnet.as_ref().expect("VNet harness");
    vnet.block_network();
    harness.peer(107).rtc.network_unavailable();
    harness.peer(208).rtc.network_unavailable();
    harness.peer(107).rtc.network_available();
    harness.peer(208).rtc.network_available();

    // An Answerer does not create a replacement PC after its physical recovery
    // fails. Explicitly restart from the stable Offerer so the Answerer can
    // consume the resulting fresh ordinary Offer.
    tokio::time::sleep(Duration::from_secs(1)).await;
    harness.peer(offerer_serial).rtc.close_peer(&answerer).await;
    harness
        .peer(offerer_serial)
        .rtc
        .connect(answerer.clone())
        .await;

    let rebuilding = harness
        .peer(answerer_serial)
        .rtc
        .wait_state(&offerer, WAIT, |state| state.pc_generation > old_generation)
        .await
        .expect("Offerer restart should deliver a fresh Offer to the Answerer");
    assert!(!rebuilding.sendable);

    vnet.unblock_network();
    harness.peer(offerer_serial).rtc.network_available();
    harness.peer(answerer_serial).rtc.network_available();
    let (recovered, _) = tokio::join!(
        harness
            .peer(offerer_serial)
            .rtc
            .wait_sendable(&answerer, WAIT),
        harness
            .peer(answerer_serial)
            .rtc
            .wait_sendable(&offerer, WAIT),
    );
    assert!(recovered.pc_generation > old_generation);
    tokio::time::timeout(Duration::from_secs(30), harness.shutdown())
        .await
        .expect("harness shutdown must converge");
}
