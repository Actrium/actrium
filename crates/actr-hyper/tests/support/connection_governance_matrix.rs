//! Connection-governance matrix coverage for the RTC path.
//!
//! These scenarios keep the opposite endpoint alive and do not call the
//! test-only `close_peer`/`connect` recovery helper after the fault. That is
//! deliberate: the assertions are about the production controller's own
//! recovery protocol, not about whether a caller can manually create a new
//! pair.

use std::time::Duration;

use actr_hyper::test_support::{
    ConnectionInitiator, MobileRtcPairHarness, PairEndpoint, SignalingMessageClass, TestRtcRole,
};
use actr_protocol::{ActrError, ActrId};

const READY_WAIT: Duration = Duration::from_secs(30);
const RECOVERY_WAIT: Duration = Duration::from_secs(45);

async fn same_identity_restart(role: TestRtcRole) {
    let (a, b) = if role == TestRtcRole::Offerer {
        (68_002, 68_001)
    } else {
        (68_001, 68_002)
    };
    let mut pair = MobileRtcPairHarness::new(a, b).await;
    pair.establish(ConnectionInitiator::A).await;
    let before = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(before.role_a, role);
    // Restart only the remote endpoint, with the same protocol identity.
    // Normal business is the only reconnect trigger after the fixture restart.
    pair.restart_endpoint(PairEndpoint::B).await;
    pair.harness()
        .wait_for_signaling_registration(b, READY_WAIT)
        .await;
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        pair.rpc(
            PairEndpoint::A,
            PairEndpoint::B,
            "first-after-same-id-peer-restart",
        ),
    )
    .await;
    let result = outcome.expect("first post-restart request must remain bounded");
    match &result {
        Ok(reply) => assert_eq!(reply.as_ref(), b"pong"),
        Err(ActrError::DeliveryUncertain { operation_id, .. }) => {
            assert_eq!(operation_id, "first-after-same-id-peer-restart")
        }
        Err(ActrError::TimedOut) => {}
        other => panic!("unexpected first operation outcome: {other:?}"),
    }
    // The first operation may already have entered the stale wire. Do not
    // replay it. Let recovery succeed or exhaust naturally, then issue a
    // distinct operation; no direct RTC connect/close is permitted here.
    let settled = if pair
        .harness()
        .peer(a)
        .rtc
        .state(&pair.id(PairEndpoint::B))
        .await
        .is_some()
    {
        pair.harness()
            .peer(a)
            .rtc
            .wait_state(&pair.id(PairEndpoint::B), Duration::from_secs(68), |s| {
                s.sendable || s.status == actr_framework::WebRtcPeerStatus::Idle
            })
            .await
            .is_some()
    } else {
        true
    };
    let fresh = if settled {
        Some(
            pair.rpc(
                PairEndpoint::A,
                PairEndpoint::B,
                "new-operation-after-same-id-restart",
            )
            .await,
        )
    } else {
        None
    };
    pair.shutdown().await;
    assert!(
        settled,
        "old attempt must settle inside its recovery budget"
    );
    assert!(
        matches!(fresh, Some(Ok(ref reply)) if reply.as_ref() == b"pong"),
        "new operation did not recover: {fresh:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_offerer_same_id_restart_bounds_old_call_then_new_operation_recovers() {
    same_identity_restart(TestRtcRole::Offerer).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_answerer_same_id_restart_bounds_old_call_then_new_operation_recovers() {
    same_identity_restart(TestRtcRole::Answerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn mixed_role_mobile_keeps_healthy_peer_rpc_and_stream_during_other_link_fault() {
    use actr_hyper::lifecycle::{NetworkAvailability, NetworkTransportFlags};
    use actr_hyper::test_support::{
        ActrixTestServices, LinkDirection, MobileEventDriver, PacketRule, TestHarness,
        make_actor_id,
    };
    use actr_protocol::{DataChunk, PayloadType, prost::Message};
    const A: u64 = 67_002;
    const B: u64 = 67_001;
    const C: u64 = 67_003;
    let services = ActrixTestServices::start().await.unwrap();
    let mut h = TestHarness::with_external_signaling(services.signaling_url()).await;
    for serial in [A, B, C] {
        let id = make_actor_id(serial);
        h.add_peer_with_rtc_configuration_and_credential(
            serial,
            services.rtc_configuration(&id, false),
            services.aid_credential(&id),
        )
        .await;
        h.wait_for_signaling_registration(serial, READY_WAIT).await;
    }
    let mobile = MobileEventDriver::spawn(&h, A);
    mobile.foreground(Duration::ZERO).await;
    h.connect(A, B).await;
    h.connect(A, C).await;
    assert_eq!(
        h.peer(A).rtc.role(&h.peer(B).id).await,
        Some(TestRtcRole::Offerer)
    );
    assert_eq!(
        h.peer(A).rtc.role(&h.peer(C).id).await,
        Some(TestRtcRole::Answerer)
    );
    let ab = h.peer(A).rtc.generation(&h.peer(B).id).await.unwrap();
    let ba = h.peer(B).rtc.generation(&h.peer(A).id).await.unwrap();
    let ac = h.peer(A).rtc.generation(&h.peer(C).id).await.unwrap();
    assert!(h.rtc_faults.set_link_rule(
        (&h.peer(A).id, ab),
        (&h.peer(B).id, ba),
        LinkDirection::Both,
        PacketRule::Blackhole
    ));
    h.peer(A)
        .rtc
        .wait_state(&h.peer(B).id, READY_WAIT, |s| !s.sendable)
        .await
        .expect("fault must actually revoke A-B");
    for (from, to) in [(A, C), (C, A)] {
        let response = h
            .peer(from)
            .spawn_request(to, "mixed-healthy-rpc", 5_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.as_ref(), b"pong");
        let chunk = DataChunk {
            stream_id: format!("mixed-{from}"),
            sequence: 1,
            payload: bytes::Bytes::from(vec![0x55; 32 * 1024]),
            ..Default::default()
        };
        h.peer(from)
            .gate
            .send_data_chunk(
                &h.peer(to).id,
                PayloadType::StreamReliable,
                &chunk.stream_id,
                bytes::Bytes::from(chunk.encode_to_vec()),
                &format!("{}#1", chunk.stream_id),
            )
            .await
            .unwrap();
        let (_, lane, received) = tokio::time::timeout(READY_WAIT, h.peer(to).receive_payload())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lane, PayloadType::StreamReliable);
        assert_eq!(DataChunk::decode(received).unwrap(), chunk);
    }
    assert_eq!(h.peer(A).rtc.generation(&h.peer(C).id).await, Some(ac));
    assert!(
        h.rtc_faults
            .heal_link((&h.peer(A).id, ab), (&h.peer(B).id, ba))
    );
    mobile
        .network_path(
            1,
            NetworkAvailability::Available,
            NetworkTransportFlags {
                wifi: true,
                ..Default::default()
            },
        )
        .await;
    h.peer(A).rtc.wait_sendable(&h.peer(B).id, READY_WAIT).await;
    let recovered = h
        .peer(A)
        .spawn_request(B, "mixed-recovered-rpc", 5_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.as_ref(), b"pong");
    assert_eq!(h.peer(A).pending_count().await, 0);
    assert_eq!(h.peer(A).rtc.peer_count().await, 2);
    mobile.shutdown().await;
    h.shutdown().await;
    services.shutdown().await;
}

async fn natural_budget(role: TestRtcRole) {
    use actr_hyper::test_support::{LinkDirection, SignalingFaultRule};
    let serials = if role == TestRtcRole::Offerer {
        (53_002, 53_001)
    } else {
        (53_001, 53_002)
    };
    let mut pair = MobileRtcPairHarness::new(serials.0, serials.1).await;
    pair.establish(ConnectionInitiator::A).await;
    let b = pair.id(PairEndpoint::B);
    let original = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(original.role_a, role);
    pair.set_signaling_fault(
        LinkDirection::Both,
        SignalingMessageClass::Any,
        SignalingFaultRule::Drop,
    );
    let start = std::time::Instant::now();
    pair.background(PairEndpoint::A).await;
    // Do not call close/force/connect or advance protocol clocks. Natural
    // Controller budget expiration must close the actual Session and retire
    // the existing transport while there is no business traffic.
    let terminal = pair
        .harness()
        .peer(serials.0)
        .rtc
        .wait_state(&b, Duration::from_secs(68), |s| {
            s.unavailable_reason == Some(actr_framework::WebRtcUnavailableReason::RecoveryExhausted)
        })
        .await;
    let elapsed = start.elapsed();
    let pending = pair.harness().peer(serials.0).pending_count().await;
    let mut retired = false;
    for _ in 0..100 {
        if pair
            .harness()
            .peer(serials.0)
            .transport_manager
            .dest_count()
            .await
            == 0
        {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pair.shutdown().await;
    let terminal = terminal.expect("natural budget must terminate the existing attempt");
    assert!(!terminal.sendable);
    assert_eq!(terminal.pc_generation, 0);
    assert!(
        elapsed >= Duration::from_secs(59) && elapsed < Duration::from_secs(68),
        "{elapsed:?}"
    );
    assert_eq!(pending, 0);
    assert!(
        retired,
        "natural terminal edge must retire cached DestTransport"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offerer_natural_recovery_budget_retires_transport_without_business_traffic() {
    natural_budget(TestRtcRole::Offerer).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_natural_recovery_budget_retires_transport_without_business_traffic() {
    natural_budget(TestRtcRole::Answerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_without_active_session_rejects_restart_before_new_connection() {
    use actr_hyper::test_support::{LinkDirection, SignalingFaultRule};
    let mut pair = MobileRtcPairHarness::new(52_901, 52_902).await;
    pair.establish(ConnectionInitiator::A).await;
    let a = pair.id(PairEndpoint::A);
    let b = pair.id(PairEndpoint::B);
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    // Prevent a restart offer from overtaking observation of the empty slot.
    pair.set_signaling_fault(
        LinkDirection::BToA,
        SignalingMessageClass::IceRestartOffer,
        SignalingFaultRule::Drop,
    );
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .retire_physical_session(&b)
        .await;
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&b, READY_WAIT, |s| s.pc_generation == 0 && !s.sendable)
        .await
        .expect("S4: no active physical session");
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .rtc
            .peer_count()
            .await,
        1,
        "S4 retains Controller; this is not the unknown-Controller case"
    );
    // Count only an offer emitted after removing the drop rule. An earlier
    // observed-but-dropped offer is not evidence that the router received it.
    let dropped_offers =
        pair.harness()
            .signaling_faults
            .hits(&b, &a, SignalingMessageClass::IceRestartOffer);
    pair.heal_signaling_faults();
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &b,
            &a,
            SignalingMessageClass::IceRestartOffer,
            dropped_offers,
            RECOVERY_WAIT,
        )
        .await
        .expect("restart must reach the empty-session Answerer");
    let rejection = pair
        .harness()
        .signaling_faults
        .wait_for_hits_after(
            &a,
            &b,
            SignalingMessageClass::IceRestartRejected,
            0,
            Duration::from_secs(15),
        )
        .await;
    let a_state = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .state(&b)
        .await
        .expect("A controller state after the rejection");
    assert_eq!(
        a_state.pc_generation, 0,
        "the rejection must not build a replacement session on the answerer"
    );
    let evidence = pair.signaling_trace();
    pair.shutdown().await;
    assert!(
        rejection.is_some(),
        "no-active Session must explicitly reject ICE restart; trace={evidence:?}"
    );
    let rejected = evidence
        .iter()
        .find(|event| event.class == SignalingMessageClass::IceRestartRejected)
        .expect("rejection event in the trace");
    let delivered_exchanges: Vec<&str> = evidence
        .iter()
        .filter(|event| {
            event.class == SignalingMessageClass::IceRestartOffer
                && event.rule == SignalingFaultRule::Deliver
        })
        .filter_map(|event| event.exchange_id.as_deref())
        .collect();
    assert!(
        delivered_exchanges.contains(&rejected.exchange_id.as_deref().unwrap_or("")),
        "the rejection must answer a delivered restart exchange; trace={evidence:?}"
    );
    assert_eq!(
        rejected.reject_reason,
        Some(actr_protocol::ice_restart_rejected::Reason::NoActiveSession as i32),
        "the rejection must carry the NoActiveSession reason; trace={evidence:?}"
    );
}

fn live_lanes(
    observation: &actr_hyper::test_support::TestRtcResourceObservation,
    peer: &ActrId,
) -> usize {
    observation
        .peers
        .iter()
        .find(|entry| entry.state.peer == *peer)
        .map(|entry| entry.live_lanes)
        .unwrap_or_else(|| panic!("resource observation has no peer {}", peer.to_string_repr()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_parked_open_lane_rejoins_without_manual_reconnect() {
    // The lower serial is the Answerer under the test Actrix role tie-break.
    let mut pair = MobileRtcPairHarness::new(52_001, 52_002).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    assert_eq!(initial.role_b, TestRtcRole::Offerer);

    let answerer = pair.id(PairEndpoint::A);
    let offerer = pair.id(PairEndpoint::B);
    let anchor_before = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .lane_probe(&offerer, actr_protocol::PayloadType::RpcReliable)
        .await;
    assert!(anchor_before.is_open().await);
    let before_resources = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .observation()
        .await;
    let before_lanes = live_lanes(&before_resources, &offerer);
    assert!(
        before_lanes > 0,
        "the established Answerer must own a live lane"
    );
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

    // Deliver the address-change fact while the concrete PC/lane are still
    // healthy. Unlike Background -> short Foreground, this enters the
    // Answerer's explicit awaiting_offer (parked) path.
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .local_address_changed();
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&offerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("Answerer must revoke business sendability while parked");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &answerer,
            &offerer,
            SignalingMessageClass::IceRestartRequest,
            request_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("parked Answerer must wake the Offerer");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            offer_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("Offerer must answer the parked Answerer's wake");

    let restored = pair.wait_pair_ready(RECOVERY_WAIT).await;
    assert_eq!(
        restored.a_to_b.pc_generation, initial.a_to_b.pc_generation,
        "a same-generation parked recovery must not build a replacement PC"
    );
    assert_eq!(
        restored.b_to_a.pc_generation, initial.b_to_a.pc_generation,
        "the Offerer must stay on the same PC generation for soft recovery"
    );
    let after_resources = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .observation()
        .await;
    assert_eq!(
        live_lanes(&after_resources, &offerer),
        before_lanes,
        "the parked Answerer must retain its lane set across the ICE restart"
    );
    let anchor_after = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .lane_probe(&offerer, actr_protocol::PayloadType::RpcReliable)
        .await;
    assert!(anchor_after.is_open().await);
    assert!(
        anchor_before.same_channel(&anchor_after),
        "parked recovery must reuse the exact open anchor"
    );

    pair.assert_rpc_both_directions("answerer_parked_open_lane")
        .await;
    pair.assert_data_chunk_both_directions("answerer_parked_open_lane")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_failed_session_recovers_through_offerer_without_manual_reconnect() {
    // A failed physical Answerer Session cannot be repaired by a same-generation
    // ICE restart because its RpcReliable readiness anchor was revoked. Retire
    // the failed generation, reject the stale restart Offer, and let the
    // Offerer create a replacement PC. The synthetic Failed fact enters through
    // the production controller mailbox; the test does not kill a real driver.
    let mut pair = MobileRtcPairHarness::new(52_101, 52_102).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    let answerer = pair.id(PairEndpoint::A);
    let offerer = pair.id(PairEndpoint::B);
    let request_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartRequest,
    );
    let ordinary_offer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::Offer,
    );
    let rejected_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartRejected,
    );

    let failed_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .fail_peer(&offerer)
        .await
        .expect("Answerer failed-session fact must be injectable in the matrix harness");
    assert_eq!(failed_generation, initial.a_to_b.pc_generation);
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&offerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("failed Answerer session must revoke sendability");

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &answerer,
            &offerer,
            SignalingMessageClass::IceRestartRequest,
            request_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("failed Answerer must wake the Offerer");
    let requests = pair
        .signaling_trace()
        .into_iter()
        .filter(|event| {
            event.from == answerer
                && event.to == offerer
                && event.class == SignalingMessageClass::IceRestartRequest
                && event.ordinal > request_baseline
        })
        .collect::<Vec<_>>();
    assert!(
        !requests.is_empty(),
        "the recovery cycle must expose a request"
    );
    assert_eq!(
        requests.first().and_then(|event| event.attempt),
        Some(1),
        "the first recovery wake must start at attempt 1"
    );
    let request_ids = requests
        .iter()
        .map(|event| event.exchange_id.clone())
        .collect::<Vec<_>>();
    assert!(
        request_ids.windows(2).all(|ids| ids[0] == ids[1]),
        "one recovery cycle must keep one request identity: {request_ids:?}"
    );
    let attempts = requests
        .iter()
        .filter_map(|event| event.attempt)
        .collect::<Vec<_>>();
    assert!(
        attempts
            .windows(2)
            .all(|attempts| attempts[0] < attempts[1]),
        "Answerer recovery attempts must increase monotonically: {attempts:?}"
    );
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &answerer,
            &offerer,
            SignalingMessageClass::IceRestartRejected,
            rejected_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("failed Answerer must reject a restart for its retired Session");
    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::Offer,
            ordinary_offer_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("persistent Answerer failure must make the Offerer publish a replacement Offer");

    let rebuilt = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&offerer, RECOVERY_WAIT, |state| {
            state.sendable && state.pc_generation > initial.a_to_b.pc_generation
        })
        .await
        .expect("Answerer must consume the replacement Offer and build a new PC");
    let offerer_rebuilt = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_state(&answerer, RECOVERY_WAIT, |state| {
            state.sendable && state.pc_generation > initial.b_to_a.pc_generation
        })
        .await
        .expect("Offerer must become sendable on its replacement PC");
    assert!(rebuilt.pc_generation > initial.a_to_b.pc_generation);
    assert!(offerer_rebuilt.pc_generation > initial.b_to_a.pc_generation);

    pair.assert_rpc_both_directions("answerer_failed_session")
        .await;
    pair.assert_data_chunk_both_directions("answerer_failed_session")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_failed_session_bounds_an_inflight_call_without_replay() {
    // A is the Answerer. The request is deliberately submitted before the
    // local failure and the remote test handler never replies. Recovery must
    // release the caller's pending entry without executing the request twice.
    let mut pair = MobileRtcPairHarness::new(52_121, 52_122).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    let offerer = pair.id(PairEndpoint::B);
    let request_id = "answerer_failed_inflight_call";
    let request = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .spawn_request_with_route(
            pair.serial(PairEndpoint::B),
            request_id,
            "test.no_response",
            30_000,
        );
    assert!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .wait_request_count(request_id, 1, READY_WAIT)
            .await,
        "the Call must reach the Offerer's real inbound dispatcher before failure"
    );

    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .fail_peer(&offerer)
        .await
        .expect("Answerer failed-session fact must be injectable");
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&offerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("failed Answerer must revoke sendability");

    let result = tokio::time::timeout(Duration::from_secs(15), request)
        .await
        .expect("the in-flight Call must finish within the recovery bound")
        .expect("the in-flight Call task must not panic");
    assert!(
        matches!(&result, Err(ActrError::DeliveryUncertain { operation_id, .. }) if operation_id == request_id),
        "submitted Call must keep its operation identity: {result:?}"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .pending_count()
            .await,
        0,
        "Answerer failure must release its submitted Call pending entry"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .request_count(request_id),
        1,
        "the in-flight Call must not be replayed"
    );
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answerer_closed_anchor_allows_offerer_direct_replacement_without_manual_reconnect() {
    // Closing a shared DataChannel is observed by both physical endpoints.
    // Therefore the Offerer is allowed to start replacement directly; this
    // scenario is intentionally separate from the locally injected
    // Answerer-only failure above, where the wake must come from the Answerer.
    let mut pair = MobileRtcPairHarness::new(52_151, 52_152).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Answerer);
    let answerer = pair.id(PairEndpoint::A);
    let offerer = pair.id(PairEndpoint::B);
    let ordinary_offer_baseline = pair.signaling_hits(
        PairEndpoint::B,
        PairEndpoint::A,
        SignalingMessageClass::Offer,
    );

    let closed_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .close_lane(&offerer, actr_protocol::PayloadType::RpcReliable)
        .await
        .expect("Answerer readiness anchor must be closable in the matrix harness");
    assert_eq!(closed_generation, initial.a_to_b.pc_generation);
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&offerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("closed Answerer anchor must revoke sendability");

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::Offer,
            ordinary_offer_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("Offerer must directly publish a replacement Offer after shared anchor close");
    let rebuilt = pair.wait_pair_ready(RECOVERY_WAIT).await;
    assert!(rebuilt.a_to_b.pc_generation > initial.a_to_b.pc_generation);
    assert!(rebuilt.b_to_a.pc_generation > initial.b_to_a.pc_generation);
    pair.assert_rpc_both_directions("answerer_closed_anchor_direct_offer")
        .await;
    pair.assert_data_chunk_both_directions("answerer_closed_anchor_direct_offer")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offerer_failed_session_soft_recovers_without_manual_reconnect() {
    // The higher serial is the Offerer. A local failure fact should make it
    // start an ICE restart on the existing PC before any hard replacement is
    // needed.
    let mut pair = MobileRtcPairHarness::new(52_252, 52_251).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Offerer);
    let offerer = pair.id(PairEndpoint::A);
    let answerer = pair.id(PairEndpoint::B);
    let restart_baseline = pair.signaling_hits(
        PairEndpoint::A,
        PairEndpoint::B,
        SignalingMessageClass::IceRestartOffer,
    );

    let failed_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .fail_peer(&answerer)
        .await
        .expect("Offerer failed-session fact must be injectable in the matrix harness");
    assert_eq!(failed_generation, initial.a_to_b.pc_generation);
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&answerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("failed Offerer session must revoke sendability");

    pair.harness()
        .signaling_faults
        .wait_for_hits_after(
            &offerer,
            &answerer,
            SignalingMessageClass::IceRestartOffer,
            restart_baseline,
            RECOVERY_WAIT,
        )
        .await
        .expect("Offerer must publish a same-generation ICE restart Offer");
    let restored = pair.wait_pair_ready(RECOVERY_WAIT).await;
    assert_eq!(
        restored.a_to_b.pc_generation, initial.a_to_b.pc_generation,
        "Offerer failure should initially use Soft Recovery"
    );
    assert_eq!(restored.b_to_a.pc_generation, initial.b_to_a.pc_generation);
    pair.assert_rpc_both_directions("offerer_failed_session")
        .await;
    pair.assert_data_chunk_both_directions("offerer_failed_session")
        .await;
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offerer_closed_anchor_recovers_without_manual_reconnect() {
    // The higher serial is the Offerer. This is the mirror of the Answerer
    // anchor-close case and verifies that the Offerer owns hard replacement.
    let mut pair = MobileRtcPairHarness::new(52_202, 52_201).await;
    pair.establish(ConnectionInitiator::A).await;
    let initial = pair.wait_pair_ready(READY_WAIT).await;
    assert_eq!(initial.role_a, TestRtcRole::Offerer);
    let offerer = pair.id(PairEndpoint::A);
    let answerer = pair.id(PairEndpoint::B);

    // Keep one request in the remote Handler while the Offerer's readiness
    // anchor is closed. The request must be released with a bounded terminal
    // result; it must not remain attached to the retired generation.
    let inflight_id = "offerer_closed_anchor_inflight";
    let inflight = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .spawn_request_with_route(
            pair.serial(PairEndpoint::A),
            inflight_id,
            "test.no_response",
            30_000,
        );
    assert!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::A))
            .wait_request_count(inflight_id, 1, READY_WAIT)
            .await,
        "the in-flight request must reach the Offerer's real inbound lane"
    );

    let closed_generation = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .close_lane(&answerer, actr_protocol::PayloadType::RpcReliable)
        .await
        .expect("Offerer readiness anchor must be closable in the matrix harness");
    assert_eq!(closed_generation, initial.a_to_b.pc_generation);
    pair.harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&answerer, READY_WAIT, |state| !state.sendable)
        .await
        .expect("closed Offerer anchor must revoke sendability");

    let inflight_result = tokio::time::timeout(Duration::from_secs(15), inflight)
        .await
        .expect("the in-flight request must finish within the recovery bound")
        .expect("the in-flight request task must not panic");
    assert!(
        matches!(&inflight_result, Err(ActrError::DeliveryUncertain { operation_id, .. }) if operation_id == inflight_id),
        "submitted request must report retirement uncertainty with its identity: {inflight_result:?}"
    );
    assert_eq!(
        pair.harness()
            .peer(pair.serial(PairEndpoint::B))
            .pending_count()
            .await,
        0,
        "retiring the old Offerer generation must release the caller pending entry"
    );

    let rebuilt = pair
        .harness()
        .peer(pair.serial(PairEndpoint::A))
        .rtc
        .wait_state(&answerer, RECOVERY_WAIT, |state| {
            state.sendable && state.pc_generation > initial.a_to_b.pc_generation
        })
        .await
        .expect("Offerer must autonomously build a replacement PC");
    let answerer_rebuilt = pair
        .harness()
        .peer(pair.serial(PairEndpoint::B))
        .rtc
        .wait_state(&offerer, RECOVERY_WAIT, |state| {
            state.sendable && state.pc_generation > initial.b_to_a.pc_generation
        })
        .await
        .expect("Answerer must consume the replacement Offer");
    assert!(rebuilt.pc_generation > initial.a_to_b.pc_generation);
    assert!(answerer_rebuilt.pc_generation > initial.b_to_a.pc_generation);

    pair.assert_rpc_both_directions("offerer_closed_anchor")
        .await;
    pair.assert_data_chunk_both_directions("offerer_closed_anchor")
        .await;
    pair.shutdown().await;
}
