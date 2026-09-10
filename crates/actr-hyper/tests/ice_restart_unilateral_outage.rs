//! Integration regression test for issue #227.
//!
//! The answerer loses its recovery gate first. The offerer then detects the
//! dead peer and starts a Soft Recovery, but its first restart offer is
//! intentionally dropped to represent the answerer still being unreachable.
//! When the answerer comes back it sends `IceRestartRequest(attempt=1)` while
//! the offerer's recovery round is already in flight.
//!
//! Before the fix the offerer de-duplicated that request and did not emit
//! another offer until the six-second attempt deadline. The regression test
//! requires the request itself to produce the next `IceRestartOffer` within a
//! short wake window.

use std::time::Duration;

use actr_hyper::lifecycle::{NetworkAvailability, NetworkTransportFlags};
use actr_hyper::test_support::{
    ConnectionInitiator, LinkDirection, MobileRtcPairHarness, PairEndpoint, SignalingFaultRule,
    SignalingMessageClass, TestRtcRole, install_test_crypto_provider,
};

const SETUP_TIMEOUT: Duration = Duration::from_secs(10);
/// Must be shorter than the six-second attempt-deadline cadence while still
/// leaving enough time for the real local Actrix/RTC event path.
const WAKE_WINDOW: Duration = Duration::from_secs(2);

fn wifi() -> NetworkTransportFlags {
    NetworkTransportFlags {
        wifi: true,
        cellular: false,
        ethernet: false,
        vpn: false,
        other: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unilateral_outage_offerer_resends_offer_on_attempt1_wake() {
    install_test_crypto_provider();

    // Production Actrix uses reverse ActorId ordering for the role tie-break.
    // A is the mobile/Answerer side and B is the service/Offerer side.
    let mut pair = MobileRtcPairHarness::new(52_201, 52_202).await;
    pair.establish(ConnectionInitiator::B).await;
    let initial = pair.wait_pair_ready(SETUP_TIMEOUT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    assert_eq!(initial.role_b, TestRtcRole::Offerer);

    let answerer = pair.id(PairEndpoint::A);
    let offerer = pair.id(PairEndpoint::B);
    let offer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );

    // Keep signaling alive but drop the offerer's first restart offer. This
    // makes the offerer's recovery round genuinely in-flight while the
    // answerer is still unavailable, without allowing a successful answer to
    // race the request wake under test.
    pair.set_signaling_fault(
        LinkDirection::BToA,
        SignalingMessageClass::IceRestartOffer,
        SignalingFaultRule::Drop,
    );

    // The answerer's platform gate closes first, so it cannot emit its own
    // restart request before the offerer has entered recovery.
    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, SETUP_TIMEOUT).await;

    // The offerer observes the peer failure and starts its own Soft Recovery.
    // Injecting the fact through TestRtcControl still uses the production
    // controller/event-mailbox path; the first generated offer is observed by
    // the directed signaling fault registry and dropped.
    pair.harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .fail_peer(&answerer)
        .await
        .expect("offerer controller must accept the failure fact");
    pair.wait_not_sendable(PairEndpoint::B, SETUP_TIMEOUT).await;
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            offer_baseline,
            SETUP_TIMEOUT,
        )
        .await
        .expect("the offerer's first Soft Recovery offer must be observed and dropped");

    // Restore the answerer's platform gate. Its request is the material fact
    // that tells the offerer the peer is back and waiting for a fresh offer.
    let request_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartRequest,
    );
    pair.network_path(PairEndpoint::A, 2, NetworkAvailability::Available, wifi())
        .await;
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &answerer,
            &offerer,
            SignalingMessageClass::IceRestartRequest,
            request_baseline,
            SETUP_TIMEOUT,
        )
        .await
        .expect("the returning answerer must send attempt-1 restart request");

    // The issue-227 fix makes this request reset the in-flight round and
    // produce the next offer immediately. The offer is still dropped so the
    // assertion observes the signaling action itself rather than a later
    // readiness transition.
    let offers_before_wake = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );
    let wake_offer = tokio::time::timeout(
        WAKE_WINDOW,
        pair.harness().signaling_faults.wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            offers_before_wake,
            WAKE_WINDOW,
        ),
    )
    .await;

    pair.heal_signaling_faults();
    pair.shutdown().await;

    assert!(
        wake_offer.is_ok() && wake_offer.expect("wake wait result").is_some(),
        "attempt-1 restart request must trigger a fresh offer within {WAKE_WINDOW:?}"
    );
}
