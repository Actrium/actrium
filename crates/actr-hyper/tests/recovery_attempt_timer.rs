use std::time::{Duration, Instant};

use actr_hyper::lifecycle::{NetworkAvailability, NetworkTransportFlags};
use actr_hyper::test_support::{
    ConnectionInitiator, ICE_RESTART_ATTEMPT_TIMEOUT, LinkDirection, MobileRtcPairHarness,
    PacketRule, PairEndpoint, SignalingFaultRule, SignalingMessageClass, TestRtcRole,
    bindable_non_loopback_ipv4,
};

const WAIT: Duration = Duration::from_secs(30);
/// Delay injected into the first restart Offer. Every deadline computation in
/// this file derives from this plus the authoritative
/// [`ICE_RESTART_ATTEMPT_TIMEOUT`], so changing the production budget fails
/// the guard here instead of silently widening the race it pins down.
const INJECTED_OFFER_DELAY: Duration = Duration::from_secs(2);

fn wifi() -> NetworkTransportFlags {
    NetworkTransportFlags {
        wifi: true,
        cellular: false,
        ethernet: false,
        vpn: false,
        other: false,
    }
}

fn cellular() -> NetworkTransportFlags {
    NetworkTransportFlags {
        wifi: false,
        cellular: true,
        ethernet: false,
        vpn: false,
        other: false,
    }
}

/// Reproduce the recovery timing seen in the mobile/service logs:
///
/// 1. The Answerer sends an ICE-restart request to the Offerer.
/// 2. The offer arrives after a short signaling delay and the Answerer sends
///    the correlated answer.
/// 3. The physical RTC path is still unavailable, so the readiness anchor
///    never opens.
/// 4. The old recovery-attempt timer would fire and send another
///    request before the physical connection becomes ready.
///
/// The regression is that the timer must be restarted when the Offer is
/// accepted. The Answerer's request-armed deadline would otherwise expire one
/// Offer-delivery delay before the re-anchored window and emit request #2
/// against the still-negotiating exchange, racing the Offerer's own retry
/// cadence. The Answerer must therefore stay quiet for a full attempt window
/// after the correlated answer; from there the Offerer's follow-up soft
/// restart Offers own the visible retry cadence.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn recovery_attempt_timer_restarts_after_offer_before_rtc_readiness() {
    // A's lower ActorId makes A the Answerer under the production Actrix role
    // fallback. B is the endpoint that creates the restart offer.
    let mut pair = MobileRtcPairHarness::new(51_181, 51_182).await;
    pair.establish(ConnectionInitiator::B).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);

    let a = pair.id(PairEndpoint::A);
    let b = pair.id(PairEndpoint::B);
    let request_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartRequest,
    );
    let offer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::IceRestartOffer,
    );
    let answer_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::Answer,
    );

    // Keep the first restart offer inside the ten-second attempt window, but
    // far enough away from the request to make the ordering deterministic.
    // The rule is scoped to the next offer only; retries are delivered.
    pair.harness().signaling_faults.set_nth_rule(
        b.clone(),
        a.clone(),
        SignalingMessageClass::IceRestartOffer,
        offer_baseline + 1,
        SignalingFaultRule::Delay(INJECTED_OFFER_DELAY),
    );

    // Signaling remains healthy while the real RTC UDP path is blackholed.
    // This lets the Offer/Answer complete while keeping the physical
    // readiness anchor unavailable, which is the timer race under test.
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.background(PairEndpoint::A).await;
    pair.foreground(PairEndpoint::A, Duration::from_secs(5))
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &a,
            &b,
            SignalingMessageClass::IceRestartRequest,
            request_baseline,
            WAIT,
        )
        .await
        .expect("Answerer A must emit the first correlated restart request");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &b,
            &a,
            SignalingMessageClass::IceRestartOffer,
            offer_baseline,
            WAIT,
        )
        .await
        .expect("Offerer B must emit the delayed restart offer");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(&a, &b, SignalingMessageClass::Answer, answer_baseline, WAIT)
        .await
        .expect("Answerer A must answer the restart offer before readiness");

    let trace = pair.signaling_trace();
    let first_request = trace
        .iter()
        .find(|event| {
            event.from == a
                && event.to == b
                && event.class == SignalingMessageClass::IceRestartRequest
                && event.ordinal > request_baseline
        })
        .cloned()
        .expect("first restart request trace");
    let restart_offer = trace
        .iter()
        .find(|event| {
            event.from == b
                && event.to == a
                && event.class == SignalingMessageClass::IceRestartOffer
                && event.ordinal > offer_baseline
        })
        .cloned()
        .expect("restart offer trace");
    let restart_answer = trace
        .iter()
        .find(|event| {
            event.from == a
                && event.to == b
                && event.class == SignalingMessageClass::Answer
                && event.ordinal > answer_baseline
        })
        .cloned()
        .expect("restart answer trace");

    assert_eq!(first_request.attempt, Some(1));
    assert_eq!(
        restart_offer.rule,
        SignalingFaultRule::Delay(INJECTED_OFFER_DELAY)
    );
    assert!(first_request.observed_after <= restart_offer.observed_after);
    assert!(restart_offer.observed_after <= restart_answer.observed_after);
    assert!(restart_offer.exchange_id.is_some());

    // The old timer was armed when request #1 was sent and would expire one
    // full window later — that is, INJECTED_OFFER_DELAY before the re-anchored
    // deadline the fixed controller arms at Offer acceptance. If the
    // re-anchor is lost, A's stale timer expires at
    // answer + (ICE_RESTART_ATTEMPT_TIMEOUT - INJECTED_OFFER_DELAY) and emits
    // request #2 against the still-negotiating exchange.
    //
    // B's own Offerer cadence legitimately fires at that same point: it arms
    // at Offer #1's send and cannot see the injected delivery delay, so its
    // follow-up Offers are expected there and the guard must target A's
    // requests only.
    //
    // Keep the guard open a further INJECTED_OFFER_DELAY/2 past the stale
    // deadline (11s under the current 10s budget) so a late-firing stale
    // timer is still caught, while A's earliest legitimate retransmission —
    // answer + ICE_RESTART_ATTEMPT_TIMEOUT, reachable only if B's next Offer
    // fails to supersede the window first — stays outside the guard.
    let guard_started = Instant::now();
    let guard = ICE_RESTART_ATTEMPT_TIMEOUT + INJECTED_OFFER_DELAY / 2;
    tokio::time::sleep(guard).await;
    let guard_elapsed = guard_started.elapsed();

    let stale_request = pair.signaling_trace().into_iter().find(|event| {
        event.from == a
            && event.to == b
            && event.class == SignalingMessageClass::IceRestartRequest
            && event.attempt.is_some_and(|attempt| attempt >= 2)
            && event.observed_after >= restart_answer.observed_after
            && event.observed_after <= restart_answer.observed_after + guard
    });
    assert!(
        stale_request.is_none(),
        "request #2 must wait for the fresh {ICE_RESTART_ATTEMPT_TIMEOUT:?} window re-armed at Offer acceptance; a retry inside the {guard:?} post-answer guard means the stale request-armed timer survived (event: {stale_request:?})"
    );

    let during_guard = pair.snapshot().await.expect("pair snapshot during guard");
    assert!(!during_guard.a_to_b.sendable);
    assert_eq!(
        during_guard.a_to_b.pc_generation, initial.a_to_b.pc_generation,
        "the timer must not churn the live generation"
    );

    // Heal the injected path and let the production recovery finish before
    // teardown. This also proves the test did not leave a permanent fault in
    // the shared runtime registry.
    let heal_started = Instant::now();
    pair.heal_rtc_faults().await;
    pair.heal_signaling_faults();
    pair.network_path(PairEndpoint::A, 1, NetworkAvailability::Available, wifi())
        .await;
    pair.wait_pair_ready(WAIT).await;
    let heal_to_ready = heal_started.elapsed();
    let echo_started = Instant::now();
    let echo = pair
        .rpc(
            PairEndpoint::A,
            PairEndpoint::B,
            "recovery_attempt_timer_fixed_a_to_b",
        )
        .await
        .expect("post-recovery RPC");
    assert_eq!(echo.as_ref(), b"pong");
    let echo_elapsed = echo_started.elapsed();
    pair.assert_rpc_both_directions("recovery_attempt_timer_fixed")
        .await;
    eprintln!(
        "recovery_attempt_timer timings: post-answer guard={guard_elapsed:?}, RTC-heal-to-ready={heal_to_ready:?}, ready-to-echo={echo_elapsed:?}"
    );
    pair.shutdown().await;
}

/// Exercise the complete handover path with the production bind mechanism:
/// the initial session binds the old address, a new address is published while
/// the first restart's SDP/ICE exchange is already negotiated, and the next
/// same-generation ICE restart gives webrtc-rs wildcard bind inputs. The
/// upstream driver must expand them to the new live interface before the old
/// ten-second attempt deadline, then the pair must become ready and serve RPC.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn address_change_rebinds_wildcards_before_attempt_deadline_and_restores_echo() {
    let Some(new_address) = bindable_non_loopback_ipv4() else {
        eprintln!("skipping wildcard rebind test: no bindable non-loopback IPv4 address");
        return;
    };

    // A's higher ActorId makes it the Offerer under Actrix arbitration. That
    // lets the test observe A's first restart Offer/Answer before delivering
    // the address-change edge to the same active recovery attempt.
    let mut pair = MobileRtcPairHarness::new_with_wildcard_rebind(51_201, 51_200).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Offerer);

    let a = pair.id(PairEndpoint::A);
    let b = pair.id(PairEndpoint::B);
    let initial_bound = pair.rtc_bound_addresses(PairEndpoint::A).await;
    assert!(
        initial_bound
            .iter()
            .any(|address| address.ip().is_loopback()),
        "initial recovery fixture must bind the old loopback address: {initial_bound:?}"
    );
    let initial_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .ice_generation(&b)
        .await
        .expect("initial A ICE generation");
    let offer_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartOffer,
    );
    let answer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::Answer,
    );

    // Keep the first restart's physical path unavailable while allowing its
    // signaling exchange to complete. This creates the same negotiated but
    // not-ready state as the mobile handover logs.
    pair.set_rtc_fault(LinkDirection::Both, PacketRule::Blackhole)
        .await;
    pair.wait_rtc_fault_hit(Duration::from_secs(10)).await;
    pair.network_path(
        PairEndpoint::A,
        1,
        NetworkAvailability::Unavailable,
        NetworkTransportFlags::default(),
    )
    .await;
    pair.wait_not_sendable(PairEndpoint::A, WAIT).await;
    pair.network_path(PairEndpoint::A, 2, NetworkAvailability::Available, wifi())
        .await;

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &a,
            &b,
            SignalingMessageClass::IceRestartOffer,
            offer_baseline,
            WAIT,
        )
        .await
        .expect("A must send the first recovery Offer");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(&b, &a, SignalingMessageClass::Answer, answer_baseline, WAIT)
        .await
        .expect("B must answer the first recovery Offer");
    let offer_count_after_first = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartOffer,
    );

    // Allow the Answer to be applied by A before publishing the address edge.
    // The RTC path is still blackholed, so A cannot become sendable in this
    // interval and the edge must be handled by recovery policy.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let address_change_started = Instant::now();
    pair.harness()
        .set_local_addresses(pair.serial(PairEndpoint::A), vec![new_address]);
    // The production mobile path is event-driven: changing the deterministic
    // test provider alone is not a network signal. Model the native Wi-Fi ->
    // cellular handover so the reconciler emits LocalAddressChanged and the
    // wildcard ICE rebind starts immediately.
    pair.network_path(
        PairEndpoint::A,
        3,
        NetworkAvailability::Available,
        cellular(),
    )
    .await;

    // The wildcard rebind is independently required to remain prompt. Keep
    // this guard below the ten-second recovery-attempt deadline so it does not
    // depend on the retry timer to prove the address migration.
    let rebound = pair
        .wait_rtc_bound_address(
            PairEndpoint::A,
            initial_bound.len(),
            new_address,
            Duration::from_secs(5),
        )
        .await
        .expect("A must bind the new address through wildcard ICE rebind");
    let address_to_rebind = address_change_started.elapsed();
    assert_eq!(rebound.ip(), new_address);
    assert!(
        address_to_rebind < Duration::from_secs(5),
        "wildcard rebind must remain prompt during recovery: {address_to_rebind:?}"
    );

    // The address edge arrives after the first Offer/Answer. A second Offer
    // proves the edge started a fresh same-generation soft recovery rather
    // than merely changing the provider's test data.
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &a,
            &b,
            SignalingMessageClass::IceRestartOffer,
            offer_count_after_first,
            Duration::from_secs(5),
        )
        .await
        .expect("the address edge must trigger a second soft-recovery Offer");

    pair.heal_rtc_faults().await;
    pair.heal_signaling_faults();
    pair.network_path(
        PairEndpoint::A,
        4,
        NetworkAvailability::Available,
        cellular(),
    )
    .await;
    let recovered = pair.wait_pair_ready(WAIT).await;
    assert_eq!(recovered.a_to_b.pc_generation, initial.a_to_b.pc_generation);
    let recovered_ice_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .ice_generation(&b)
        .await
        .expect("recovered A ICE generation");
    assert!(recovered_ice_generation > initial_ice_generation);

    let echo_started = Instant::now();
    let echo = pair
        .rpc(
            PairEndpoint::A,
            PairEndpoint::B,
            "wildcard_rebind_recovery_echo",
        )
        .await
        .expect("Echo after wildcard rebind recovery");
    let echo_elapsed = echo_started.elapsed();
    assert_eq!(echo.as_ref(), b"pong");
    eprintln!(
        "wildcard_rebind_recovery timings: address-to-rebind={address_to_rebind:?}, ready-to-echo={echo_elapsed:?}"
    );
    pair.shutdown().await;
}
