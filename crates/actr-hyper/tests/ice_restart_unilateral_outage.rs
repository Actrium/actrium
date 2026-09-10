//! Integration regression test for issue #227.
//!
//! The answerer loses its recovery gate first. The offerer then detects the
//! dead peer and starts a Soft Recovery, but its first restart offer is
//! intentionally dropped to represent the answerer still being unreachable.
//! When the answerer comes back it sends `IceRestartRequest(attempt=1)` while
//! the offerer's recovery round is already in flight.
//!
//! Before the fix the offerer de-duplicated that request and did not emit
//! another offer until the per-attempt deadline. The regression test requires
//! the request itself to produce the next `IceRestartOffer` within a short
//! wake window and then verifies that the real pair becomes usable again.

use std::time::Duration;

use actr_hyper::lifecycle::{NetworkAvailability, NetworkTransportFlags};
use actr_hyper::test_support::{
    ConnectionInitiator, ICE_RESTART_ATTEMPT_TIMEOUT, MobileRtcPairHarness, PairEndpoint,
    REMOTE_REQUEST_WAKE_GRACE, SignalingFaultRule, SignalingMessageClass, TestRtcRole,
    install_test_crypto_provider,
};

const SETUP_TIMEOUT: Duration = Duration::from_secs(10);
/// Keep the assertion derived from the audited ten-second attempt timeout; it
/// must remain shorter than a normal retry while leaving enough time for the
/// real local Actrix/RTC event path.
const WAKE_WINDOW: Duration =
    Duration::from_millis((ICE_RESTART_ATTEMPT_TIMEOUT.as_millis() / 5) as u64);

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

    // Keep signaling alive but drop only the offerer's first restart offer.
    // This makes the recovery round genuinely in-flight while the answerer is
    // still unavailable, then lets the request-triggered offer complete the
    // real Offer/Answer and data-channel recovery below.
    pair.harness().signaling_faults.set_nth_rule(
        offerer.clone(),
        answerer.clone(),
        SignalingMessageClass::IceRestartOffer,
        offer_baseline + 1,
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
    // Capture this baseline before restoring the gate so the request-triggered
    // Offer cannot race ahead of the counter used by the wake assertion.
    let offers_before_wake = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );
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
    // produce the next offer within the short grace/retry window. This offer
    // is delivered, so the assertion below can cover the complete recovery.
    let wake_offer = tokio::time::timeout(
        REMOTE_REQUEST_WAKE_GRACE + WAKE_WINDOW,
        pair.harness().signaling_faults.wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            offers_before_wake,
            REMOTE_REQUEST_WAKE_GRACE + WAKE_WINDOW,
        ),
    )
    .await;

    let wake_offer = wake_offer
        .expect("wake offer wait must not time out")
        .expect("attempt-1 request must produce a fresh offer");

    let recovered = pair.wait_pair_ready(SETUP_TIMEOUT).await;
    assert!(recovered.a_to_b.sendable);
    assert!(recovered.b_to_a.sendable);
    pair.assert_rpc_both_directions("unilateral_outage_after_attempt1_wake")
        .await;

    pair.heal_signaling_faults();
    pair.shutdown().await;

    assert!(wake_offer > offers_before_wake);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_network_recovery_retries_lost_offer_on_attempt1_wake() {
    install_test_crypto_provider();

    let mut pair = MobileRtcPairHarness::new(52_211, 52_212).await;
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

    // This time both endpoints report the network transition. B therefore
    // owns a LocalNetwork recovery origin, which must not suppress the wake
    // after its first Offer has already been announced and lost.
    pair.harness().signaling_faults.set_nth_rule(
        offerer.clone(),
        answerer.clone(),
        SignalingMessageClass::IceRestartOffer,
        offer_baseline + 1,
        SignalingFaultRule::Drop,
    );
    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.network_path(
        PairEndpoint::B,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, SETUP_TIMEOUT).await;
    pair.wait_not_sendable(PairEndpoint::B, SETUP_TIMEOUT).await;

    // Restore the Offerer first. Its local-network recovery publishes an
    // Offer, but the first one is intentionally lost.
    pair.network_path(PairEndpoint::B, 2, NetworkAvailability::Available, wifi())
        .await;
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
        .expect("the local-network recovery Offer must be observed and dropped");

    // Restoring the Answerer sends attempt=1. Once the local Offer operation
    // has finished, this request must use the same short retry/grace path as a
    // peer-failure recovery instead of waiting for the normal 10s deadline.
    let offers_before_wake = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );
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
        .expect("the returning Answerer must send attempt-1 restart request");

    let wake_offer = tokio::time::timeout(
        REMOTE_REQUEST_WAKE_GRACE + WAKE_WINDOW,
        pair.harness().signaling_faults.wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            offers_before_wake,
            REMOTE_REQUEST_WAKE_GRACE + WAKE_WINDOW,
        ),
    )
    .await
    .expect("local-network attempt-1 wake must not wait for the 10s deadline")
    .expect("attempt-1 request must produce a fresh Offer");

    pair.heal_signaling_faults();
    pair.wait_pair_ready(SETUP_TIMEOUT).await;
    pair.assert_rpc_both_directions("local_network_attempt1_wake")
        .await;
    pair.shutdown().await;

    assert!(wake_offer > offers_before_wake);
}
