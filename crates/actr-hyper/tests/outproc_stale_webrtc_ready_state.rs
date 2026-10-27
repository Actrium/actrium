//! Stale WebRTC readiness regression tests for the answerer send path.
//!
//! The RTC controller snapshot is the sole sendability authority. These
//! tests use real VNet/signaling loss and recovery instead of injecting the
//! removed 0.14 `ConnectionEvent` stream.

use std::time::Duration;

use actr_hyper::test_support::TestHarness;
use actr_protocol::{ActrError, Direction, RpcEnvelope};

const OFFERER_SERIAL: u64 = 100;
const ANSWERER_SERIAL: u64 = 200;

fn request(request_id: &str) -> RpcEnvelope {
    RpcEnvelope {
        request_id: request_id.into(),
        route_key: "test.method".into(),
        payload: Some(bytes::Bytes::from_static(b"answerer-probe")),
        direction: Some(Direction::Request as i32),
        timeout_ms: 30_000,
        ..Default::default()
    }
}

async fn connected() -> TestHarness {
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(OFFERER_SERIAL).await;
    harness.add_peer(ANSWERER_SERIAL).await;
    harness.connect(OFFERER_SERIAL, ANSWERER_SERIAL).await;
    harness
}

#[tokio::test]
async fn answerer_failed_peer_does_not_keep_sending_on_stale_webrtc() {
    let harness = connected().await;
    let offerer = harness.peer(OFFERER_SERIAL).id.clone();
    let answerer = harness.peer(ANSWERER_SERIAL).id.clone();

    // Hold both packet and signaling paths down so the answerer cannot recover
    // before the send-path assertion observes the authoritative unavailable
    // snapshot.
    harness.simulate_disconnect();
    harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .wait_state(&offerer, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("answerer must publish sendable=false during failure");

    {
        let mut outage_request = request("answerer-send-during-failure");
        outage_request.timeout_ms = 300;
        let waiting = harness
            .peer(ANSWERER_SERIAL)
            .gate
            .send_request(&offerer, outage_request);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut waiting)
                .await
                .is_err(),
            "reliable sends must wait for recovery before their deadline"
        );
        assert_eq!(harness.peer(ANSWERER_SERIAL).pending_count().await, 0);
        let result = tokio::time::timeout(Duration::from_secs(2), &mut waiting)
            .await
            .expect("the caller's deadline must bound readiness waiting");
        assert!(matches!(result, Err(ActrError::TimedOut)), "{result:?}");
        assert_eq!(harness.peer(ANSWERER_SERIAL).pending_count().await, 0);
        assert_eq!(
            harness
                .peer(OFFERER_SERIAL)
                .request_count("answerer-send-during-failure"),
            0,
            "the unavailable answerer must not submit through a stale DataChannel"
        );
    }
    harness.simulate_reconnect();
    harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .wait_sendable(&offerer, Duration::from_secs(20))
        .await;
    harness
        .peer(OFFERER_SERIAL)
        .rtc
        .wait_sendable(&answerer, Duration::from_secs(20))
        .await;
    let response = harness
        .peer(ANSWERER_SERIAL)
        .gate
        .send_request(&offerer, request("answerer-send-after-recovery"))
        .await
        .expect("answerer must use the recovered session");
    assert_eq!(response.as_ref(), b"pong");
    harness.shutdown().await;
}

#[tokio::test]
async fn stale_failed_event_does_not_reblock_peer_after_same_session_recovers() {
    let harness = connected().await;
    let offerer = harness.peer(OFFERER_SERIAL).id.clone();
    let original_generation = harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .generation(&offerer)
        .await
        .expect("initial generation");

    harness.simulate_disconnect();
    let unavailable = harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .wait_state(&offerer, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("disconnect must revoke sendability");

    harness.simulate_reconnect();
    let recovered = harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .wait_sendable(&offerer, Duration::from_secs(20))
        .await;
    assert_eq!(
        recovered.pc_generation, original_generation,
        "ordinary network restoration should use a same-PC ICE restart"
    );
    assert!(recovered.revision > unavailable.revision);

    // Give delayed driver ticks from the failed ICE epoch a scheduling window.
    // A current-generation snapshot must remain authoritative after the soft
    // recovery completion.
    tokio::time::sleep(Duration::from_millis(750)).await;
    let settled = harness
        .peer(ANSWERER_SERIAL)
        .rtc
        .state(&offerer)
        .await
        .expect("answerer snapshot after recovery");
    assert!(settled.sendable);
    assert_eq!(settled.pc_generation, original_generation);

    let response = harness
        .peer(ANSWERER_SERIAL)
        .gate
        .send_request(&offerer, request("answerer-after-stale-driver-ticks"))
        .await
        .expect("stale failure observation must not reblock the recovered peer");
    assert_eq!(response.as_ref(), b"pong");
    harness.shutdown().await;
}
