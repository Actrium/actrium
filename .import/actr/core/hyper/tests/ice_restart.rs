//! Soft ICE recovery integration tests for the v0.20 controller architecture.

use std::time::Duration;

use actr_hyper::test_support::{TestHarness, install_test_crypto_provider};
use actr_protocol::{
    ActrId, SignalingEnvelope, actr_relay, ice_restart_rejected, session_description,
    signaling_envelope,
};

const WAIT: Duration = Duration::from_secs(30);

fn restart_rejection_exchange(
    messages: &[SignalingEnvelope],
    offerer: &ActrId,
    answerer: &ActrId,
) -> Option<(String, i32)> {
    let mut restart_exchange_ids = Vec::new();
    for envelope in messages {
        let Some(signaling_envelope::Flow::ActrRelay(relay)) = envelope.flow.as_ref() else {
            continue;
        };
        if relay.source == *offerer
            && relay.target == *answerer
            && let Some(actr_relay::Payload::SessionDescription(description)) =
                relay.payload.as_ref()
            && description.r#type == session_description::Type::IceRestartOffer as i32
            && let Some(exchange_id) = description.sdp_exchange_id.as_ref()
        {
            restart_exchange_ids.push(exchange_id.clone());
        } else if relay.source == *answerer
            && relay.target == *offerer
            && let Some(actr_relay::Payload::IceRestartRejected(rejection)) = relay.payload.as_ref()
            && restart_exchange_ids.contains(&rejection.sdp_exchange_id)
        {
            return Some((
                rejection.sdp_exchange_id.clone(),
                rejection.reason.unwrap_or_default(),
            ));
        }
    }
    None
}

async fn connected(a: u64, b: u64) -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(a).await;
    harness.add_peer(b).await;
    harness.connect(a, b).await;
    harness
}

async fn wait_recovered(
    harness: &TestHarness,
    a: u64,
    b: u64,
    a_revision: u64,
    b_revision: u64,
) -> (
    actr_framework::WebRtcPeerState,
    actr_framework::WebRtcPeerState,
) {
    tokio::time::timeout(WAIT, async {
        loop {
            let a_state = harness.peer(a).rtc.state(&harness.peer(b).id).await;
            let b_state = harness.peer(b).rtc.state(&harness.peer(a).id).await;
            if let (Some(a_state), Some(b_state)) = (a_state, b_state)
                && a_state.sendable
                && b_state.sendable
                && a_state.revision > a_revision
                && b_state.revision > b_revision
            {
                break (a_state, b_state);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both endpoints should complete a new recovery epoch")
}

async fn soft_recover(
    harness: &TestHarness,
    a: u64,
    b: u64,
) -> (
    actr_framework::WebRtcPeerState,
    actr_framework::WebRtcPeerState,
) {
    let a_before = harness
        .peer(a)
        .rtc
        .state(&harness.peer(b).id)
        .await
        .expect("endpoint A state");
    let b_before = harness
        .peer(b)
        .rtc
        .state(&harness.peer(a).id)
        .await
        .expect("endpoint B state");
    harness.peer(a).rtc.network_unavailable();
    harness.peer(b).rtc.network_unavailable();
    harness.peer(a).rtc.network_available();
    harness.peer(b).rtc.network_available();
    wait_recovered(harness, a, b, a_before.revision, b_before.revision).await
}

/// An offer arriving after the answerer removed its old controller must create
/// a fresh answerer controller; it must not depend on stale global peer state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_unknown_answerer_session_rejects_restart_and_rebuilds_autonomously() {
    install_test_crypto_provider();
    let harness = connected(901, 902).await;
    let offerer_id = harness.peer(901).id.clone();
    let answerer_id = harness.peer(902).id.clone();
    let old_offerer = harness.peer(901).rtc.state(&answerer_id).await.unwrap();
    let old_answerer = harness.peer(902).rtc.state(&offerer_id).await.unwrap();

    assert!(harness.peer(902).rtc.close_peer(&offerer_id).await);
    assert_eq!(harness.peer(902).rtc.peer_count().await, 0);

    harness.peer(901).rtc.network_unavailable();
    harness.peer(901).rtc.network_available();

    let (_, rejection_reason) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(rejection) = restart_rejection_exchange(
                &harness.server.received_messages().await,
                &offerer_id,
                &answerer_id,
            ) {
                break rejection;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("answerer must reject the correlated restart offer");
    assert_eq!(
        rejection_reason,
        ice_restart_rejected::Reason::NoActiveSession as i32
    );

    let (offerer, answerer) = tokio::time::timeout(WAIT, async {
        loop {
            let offerer = harness.peer(901).rtc.state(&answerer_id).await;
            let answerer = harness.peer(902).rtc.state(&offerer_id).await;
            if let (Some(offerer), Some(answerer)) = (offerer, answerer)
                && offerer.sendable
                && answerer.sendable
                && offerer.pc_generation > old_offerer.pc_generation
                && answerer.pc_generation > old_answerer.pc_generation
            {
                break (offerer, answerer);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("restart rejection must rebuild both peer generations");

    let response = harness
        .peer(901)
        .spawn_request(902, "issue_508_autonomous_rebuild", 15_000)
        .await
        .expect("fresh RPC task should join")
        .expect("fresh RPC should succeed after full renegotiation");
    assert_eq!(response.as_ref(), b"pong");
    assert!(offerer.pc_generation > old_offerer.pc_generation);
    assert!(answerer.pc_generation > old_answerer.pc_generation);
    assert_eq!(harness.peer(902).rtc.peer_count().await, 1);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_basic_ice_restart() {
    install_test_crypto_provider();
    let harness = connected(100, 200).await;
    let target = harness.peer(200).id.clone();
    let old = harness.peer(100).rtc.state(&target).await.unwrap();

    let (new, _) = soft_recover(&harness, 100, 200).await;
    assert_eq!(new.pc_generation, old.pc_generation);
    assert!(new.revision > old.revision);
    assert!(new.sendable);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rapid_ice_restart_deduplication() {
    install_test_crypto_provider();
    let harness = connected(300, 400).await;
    let target = harness.peer(400).id.clone();
    let old = harness.peer(300).rtc.state(&target).await.unwrap();

    for _ in 0..32 {
        harness.peer(300).rtc.network_unavailable();
        harness.peer(300).rtc.network_available();
    }
    let state = harness
        .peer(300)
        .rtc
        .wait_state(&target, WAIT, |state| {
            state.sendable && state.revision > old.revision
        })
        .await
        .expect("rapid restart recovery state");
    assert_eq!(state.pc_generation, old.pc_generation);
    assert!(state.revision > old.revision);
    assert_eq!(harness.peer(300).rtc.peer_count().await, 1);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_sequential_ice_restart() {
    install_test_crypto_provider();
    let harness = connected(500, 600).await;
    let target = harness.peer(600).id.clone();
    let initial = harness.peer(500).rtc.state(&target).await.unwrap();

    let (first, _) = soft_recover(&harness, 500, 600).await;
    let (second, _) = soft_recover(&harness, 500, 600).await;

    assert_eq!(first.pc_generation, initial.pc_generation);
    assert_eq!(second.pc_generation, initial.pc_generation);
    assert!(first.revision > initial.revision);
    assert!(second.revision > first.revision);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_simultaneous_ice_restart_glare() {
    install_test_crypto_provider();
    let harness = connected(700, 800).await;
    let a_id = harness.peer(700).id.clone();
    let b_id = harness.peer(800).id.clone();
    let a_generation = harness.peer(700).rtc.generation(&b_id).await.unwrap();
    let b_generation = harness.peer(800).rtc.generation(&a_id).await.unwrap();
    let a_before = harness.peer(700).rtc.state(&b_id).await.unwrap();
    let b_before = harness.peer(800).rtc.state(&a_id).await.unwrap();

    harness.peer(700).rtc.network_unavailable();
    harness.peer(800).rtc.network_unavailable();
    let (_, _) = tokio::join!(async { harness.peer(700).rtc.network_available() }, async {
        harness.peer(800).rtc.network_available()
    },);

    let (a, b) = wait_recovered(&harness, 700, 800, a_before.revision, b_before.revision).await;
    assert_eq!(a.pc_generation, a_generation);
    assert_eq!(b.pc_generation, b_generation);
    assert!(a.sendable && b.sendable);
    harness.shutdown().await;
}
