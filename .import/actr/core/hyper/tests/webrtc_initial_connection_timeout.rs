use actr_hyper::test_support::TestHarness;
use actr_protocol::{
    ActrId, SignalingEnvelope, actr_relay, session_description::Type as SdpType, signaling_envelope,
};
use std::time::{Duration, Instant};

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_test_writer()
        .try_init()
        .ok();
}

fn count_initial_offers(
    messages: &[SignalingEnvelope],
    source_id: &ActrId,
    target_id: &ActrId,
) -> usize {
    messages
        .iter()
        .filter(|envelope| {
            let Some(signaling_envelope::Flow::ActrRelay(relay)) = envelope.flow.as_ref() else {
                return false;
            };
            if &relay.source != source_id || &relay.target != target_id {
                return false;
            }
            let Some(actr_relay::Payload::SessionDescription(sd)) = relay.payload.as_ref() else {
                return false;
            };
            sd.r#type() == SdpType::Offer
        })
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_connection_waits_for_delayed_ice_candidate_without_factory_retry() {
    init_tracing();

    let mut harness = TestHarness::new().await;
    harness
        .server
        .delay_ice_candidates_for(Duration::from_secs(6));
    harness.add_peer(5100).await;
    harness.add_peer(5200).await;

    let offerer_id = harness.peer(5100).id.clone();
    let answerer_id = harness.peer(5200).id.clone();

    let started = Instant::now();
    harness
        .connect_with_timeout(5100, 5200, Duration::from_secs(15))
        .await;
    let elapsed = started.elapsed();

    assert!(
        harness.peer(5100).rtc.is_sendable(&answerer_id).await,
        "create_connection must return only after DataChannel is open"
    );
    assert!(
        harness.server.delayed_ice_candidate_count() > 0,
        "test must delay at least one ICE candidate relay"
    );
    assert!(
        elapsed >= Duration::from_secs(5),
        "connection should exercise the old 5s DataChannel-open window; elapsed={elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "connection should complete inside the first full-stack request timeout; elapsed={elapsed:?}"
    );

    let messages = harness.server.received_messages().await;
    let offer_count = count_initial_offers(&messages, &offerer_id, &answerer_id);
    assert_eq!(
        offer_count, 1,
        "delayed ICE candidates should not force a second initial offer/factory retry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_negotiation_timeout_is_shorter_than_initial_connection_budget() {
    init_tracing();

    let mut harness = TestHarness::new().await;
    harness.add_peer(5300).await;
    harness.add_peer(5400).await;
    let answerer_id = harness.peer(5400).id.clone();
    harness.server.pause_forwarding();

    let started = Instant::now();
    harness.peer(5300).rtc.connect(answerer_id.clone()).await;
    let result = harness
        .peer(5300)
        .rtc
        .wait_state(&answerer_id, Duration::from_secs(7), |state| {
            state.unavailable_reason.is_some()
        })
        .await;
    let elapsed = started.elapsed();

    assert!(
        result.is_some(),
        "expected the v0.20 controller snapshot to expose role-negotiation failure"
    );
    assert!(
        elapsed >= Duration::from_secs(4),
        "role negotiation should wait for its response budget; elapsed={elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(7),
        "role negotiation must not consume the old 15s window; elapsed={elapsed:?}"
    );

    harness.shutdown().await;
}
