//! Regression test for the historical ICE-connected/SCTP-not-ready race.
//!
//! In the RTC architecture the business boundary is the authoritative
//! controller snapshot, not a raw ICE/PeerConnection callback. `Connected`
//! is published only after the active generation's `RpcReliable` channel is
//! open. This test performs three real ICE restarts and sends a burst at the
//! first sendable snapshot edge after each restart.

use actr_hyper::test_support::TestHarness;
use actr_protocol::{Direction, RpcEnvelope};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reproduce_sctp_race_after_ice_restart() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .with_line_number(true)
        .with_file(true)
        .try_init()
        .ok();

    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.connect(100, 200).await;

    let target = harness.peer(200).id.clone();
    let initial_generation = harness
        .peer(100)
        .rtc
        .generation(&target)
        .await
        .expect("initial generation");

    for attempt in 1..=3 {
        let rtc = harness.peer(100).rtc.clone();
        rtc.network_unavailable();
        rtc.wait_state(&target, Duration::from_secs(5), |state| {
            !state.sendable && state.status == actr_framework::WebRtcPeerStatus::Recovering
        })
        .await
        .expect("network loss must revoke business sendability");

        let mut snapshots = rtc
            .subscribe(&target)
            .await
            .expect("controller must remain registered during soft recovery");
        rtc.network_available();

        let ready = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let snapshot = snapshots.borrow().clone();
                assert!(
                    snapshot.status != actr_framework::WebRtcPeerStatus::Connected
                        || snapshot.sendable,
                    "Connected must never be visible before RpcReliable is open"
                );
                if snapshot.sendable {
                    break snapshot;
                }
                snapshots
                    .changed()
                    .await
                    .expect("snapshot stream must stay alive");
            }
        })
        .await
        .expect("ICE restart must return to a sendable snapshot");

        assert_eq!(
            ready.pc_generation, initial_generation,
            "successful soft recovery must not replace the PC generation"
        );

        // No stabilization sleep: issue all RPCs on the first authoritative
        // sendable edge. If SCTP were not actually ready this burst would
        // reproduce the old write failures.
        let mut sends = Vec::new();
        for index in 0..10 {
            let gate = harness.peer(100).gate.clone();
            let target = target.clone();
            sends.push(tokio::spawn(async move {
                let envelope = RpcEnvelope {
                    request_id: format!("sctp-race-{attempt}-{index}"),
                    route_key: "test.ping".to_string(),
                    payload: Some(bytes::Bytes::from(format!(
                        "attempt {attempt} message {index}"
                    ))),
                    direction: Some(Direction::Request as i32),
                    timeout_ms: 5_000,
                    ..Default::default()
                };
                gate.send_request(&target, envelope).await
            }));
        }

        for send in sends {
            let response = tokio::time::timeout(Duration::from_secs(8), send)
                .await
                .expect("RPC burst must not hang")
                .expect("RPC task must not panic")
                .expect("first-sendable-edge RPC must succeed");
            assert_eq!(response.as_ref(), b"pong");
        }
    }

    harness.shutdown().await;
}
