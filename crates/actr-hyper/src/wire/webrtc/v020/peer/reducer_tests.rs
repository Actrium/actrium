//! Pure-reducer tests for the T1 peer state machine.
//!
//! These drive `reduce` directly (no tasks, no real PeerConnection) and assert
//! both the phase transitions and the architecture invariants: one-active-generation,
//! stale-event/stale-completion rejection, close-during-build (late success and
//! late failure), the close barrier, and O1 mailbox-overflow hard recovery.

use super::reducer::{
    EffectCompletion, LaneEvent, RtcPeerCommand, RtcPeerEffect, RtcPeerInput,
    SOFT_HARD_ESCALATION_ATTEMPT, SignalingMessage, reduce,
};
use super::state::{PendingRtcOperationKind, RecoveryOrigin, Role, RtcPeerPhase, RtcPeerState};
use crate::wire::webrtc::v020::events::{PeerDriverEvent, PeerStateTick};
use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;
use crate::wire::webrtc::v020::status::{LaneKind, NetworkPath, RtcUnavailableReason};

use actr_framework::WebRtcPeerStatus;
use actr_protocol::ActrId;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidate, RTCIceCandidateType, RTCIceConnectionState, RTCIceProtocol,
    RTCPeerConnectionState, RTCSdpType, RTCSessionDescription,
};

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: "test@example.com".to_string(),
        serial_number,
    }
}

fn sdp(sdp_type: RTCSdpType) -> RTCSessionDescription {
    let mut desc = RTCSessionDescription::default();
    desc.sdp_type = sdp_type;
    desc.sdp = format!("fake-{sdp_type:?}-sdp");
    desc
}

fn offer() -> RTCSessionDescription {
    sdp(RTCSdpType::Offer)
}

fn answer() -> RTCSessionDescription {
    sdp(RTCSdpType::Answer)
}

/// An SDP carrying a specific ICE ufrag (for ufrag-tagging tests).
fn sdp_with_ufrag(sdp_type: RTCSdpType, ufrag: &str) -> RTCSessionDescription {
    let mut d = RTCSessionDescription::default();
    d.sdp_type = sdp_type;
    d.sdp = format!("v=0\r\na=ice-ufrag:{ufrag}\r\na=ice-pwd:somepwd\r\n");
    d
}

/// A remote candidate tagged with a specific ufrag.
fn candidate(ufrag: &str) -> webrtc::peer_connection::RTCIceCandidateInit {
    webrtc::peer_connection::RTCIceCandidateInit {
        candidate: format!("candidate:1 1 udp 2130706431 192.0.2.1 5000 typ host ufrag {ufrag}"),
        sdp_mid: Some("0".to_string()),
        sdp_mline_index: Some(0),
        username_fragment: Some(ufrag.to_string()),
        url: None,
    }
}

fn local_candidate() -> RTCIceCandidate {
    RTCIceCandidate {
        id: "local-candidate".to_string(),
        foundation: "1".to_string(),
        priority: 2_130_706_431,
        address: "192.0.2.2".to_string(),
        protocol: RTCIceProtocol::Udp,
        port: 5001,
        typ: RTCIceCandidateType::Host,
        component: 1,
        ..RTCIceCandidate::default()
    }
}

fn has_add_ice_candidate(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::AddIceCandidate { generation: g, .. } if *g == generation
        )
    })
}

/// A successful `BuildSession` completion. The physical Session is owned by
/// the effect runner and never crosses the reducer boundary.
fn build_ok(generation: u64, operation_id: u64) -> RtcPeerInput {
    RtcPeerInput::Completion(EffectCompletion::BuildSession {
        generation,
        operation_id,
        result: Ok(()),
    })
}

fn connected_tick(generation: u64) -> RtcPeerInput {
    RtcPeerInput::StateTick(Box::new(PeerStateTick {
        generation,
        seq: 1,
        pc_state: Some(RTCPeerConnectionState::Connected),
        ice_state: Some(RTCIceConnectionState::Connected),
        gathering_state: None,
        signaling_state: None,
        overflow_count: 0,
    }))
}

fn failed_tick(generation: u64) -> RtcPeerInput {
    RtcPeerInput::StateTick(Box::new(PeerStateTick {
        generation,
        seq: 1,
        pc_state: Some(RTCPeerConnectionState::Failed),
        ice_state: None,
        gathering_state: None,
        signaling_state: None,
        overflow_count: 0,
    }))
}

fn closed_tick(generation: u64) -> RtcPeerInput {
    RtcPeerInput::StateTick(Box::new(PeerStateTick {
        generation,
        seq: 1,
        pc_state: Some(RTCPeerConnectionState::Closed),
        ice_state: None,
        gathering_state: None,
        signaling_state: None,
        overflow_count: 0,
    }))
}

fn rpc_reliable_opened(generation: u64) -> RtcPeerInput {
    RtcPeerInput::Lane(LaneEvent::Opened {
        generation,
        lane: LaneKind::RpcReliable,
    })
}

// ── effect assertion helpers ────────────────────────────────────────────────

fn has_build_session(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::BuildSession { generation: g, .. } if *g == generation
        )
    })
}

fn has_close_session(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::CloseSession { generation: g, .. } if *g == generation
        )
    })
}

fn close_operation(effects: &[RtcPeerEffect], generation: u64) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::CloseSession {
                generation: actual,
                operation_id,
            } if *actual == generation => Some(*operation_id),
            _ => None,
        })
        .expect("close effect identity")
}

fn restart_identity(effects: &[RtcPeerEffect], generation: u64) -> (u64, u64) {
    effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::RestartIce {
                generation: actual,
                operation_id,
                ice_generation,
            } if *actual == generation => Some((*operation_id, *ice_generation)),
            _ => None,
        })
        .expect("restart effect identity")
}

fn pending_operation(state: &RtcPeerState, kind: PendingRtcOperationKind, generation: u64) -> u64 {
    state
        .pending_operations
        .iter()
        .find_map(|(operation_id, pending)| {
            (pending.kind == kind && pending.pc_generation == generation).then_some(*operation_id)
        })
        .expect("pending operation identity")
}

fn has_create_offer(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::CreateOffer { generation: g, .. } if *g == generation
        )
    })
}

fn has_create_answer(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::CreateAnswer { generation: g, .. } if *g == generation
        )
    })
}

fn has_set_remote(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::SetRemoteDescription { generation: g, .. } if *g == generation
        )
    })
}

fn has_set_local(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::SetLocalDescription { generation: g, .. } if *g == generation
        )
    })
}

fn has_send_offer(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| matches!(
        e,
        RtcPeerEffect::SendSignaling { generation: g, message: SignalingMessage::Offer { .. } } if *g == generation
    ))
}

fn has_send_answer(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| matches!(
        e,
        RtcPeerEffect::SendSignaling { generation: g, message: SignalingMessage::Answer { .. } } if *g == generation
    ))
}

fn has_restart_request(effects: &[RtcPeerEffect], attempt: u32) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { attempt: actual, .. },
                ..
            } if *actual == attempt
        )
    })
}

fn has_role_negotiation(effects: &[RtcPeerEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::RoleNegotiation,
                ..
            }
        )
    })
}

fn role_assignment(is_offerer: bool) -> RtcPeerInput {
    RtcPeerInput::Command(RtcPeerCommand::ApplyRoleAssignment { is_offerer })
}

// ── flow helpers ────────────────────────────────────────────────────────────

/// Drive the offerer from Idle to just before transport/anchor, returning the
/// state and the assigned generation. The connect flow is role-negotiated:
/// `Connect` requests the server to arbitrate, then the assignment starts the build.
fn drive_offerer_to_negotiating(state: &mut RtcPeerState) -> u64 {
    let effects = reduce(state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(state.role_negotiation_inflight);
    assert!(has_role_negotiation(&effects));
    assert!(
        !has_build_session(&effects, 1),
        "no build before role assignment"
    );

    let effects = reduce(state, role_assignment(true));
    assert!(has_build_session(&effects, 1));

    let effects = reduce(state, build_ok(1, 1));
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert_eq!(state.active_generation, Some(1));
    assert!(has_create_offer(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: 1,
            operation_id: 2,
            result: Ok(Box::new(offer())),
        }),
    );
    assert!(has_set_local(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 3,
            result: Ok(()),
        }),
    );
    assert!(has_send_offer(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(answer()))),
    );
    assert!(has_set_remote(&effects, 1));

    reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 4,
            result: Ok(()),
        }),
    );
    1
}

/// Drive the answerer from Idle to just before transport/anchor.
fn drive_answerer_to_negotiating(state: &mut RtcPeerState) -> u64 {
    let effects = reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(offer()))),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.role, Role::Answerer);
    assert!(has_build_session(&effects, 1));

    let effects = reduce(state, build_ok(1, 1));
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert!(has_set_remote(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation: 1,
            operation_id: 3,
            result: Ok(Box::new(answer())),
        }),
    );
    assert!(has_set_local(&effects, 1));

    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 4,
            result: Ok(()),
        }),
    );
    assert!(has_send_answer(&effects, 1));
    1
}

/// Finish the last two steps to Sendable for an already-negotiating peer.
fn drive_to_sendable(state: &mut RtcPeerState, generation: u64) {
    reduce(state, connected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::TransportReady);
    assert!(!state.sendable(), "not sendable until the anchor opens");

    reduce(state, rpc_reliable_opened(generation));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
}

// ── transition tests ────────────────────────────────────────────────────────

#[test]
fn offerer_flow_reaches_sendable_and_connected() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    assert_eq!(state.role, Role::Offerer);

    drive_to_sendable(&mut state, generation);

    assert!(state.sendable());
    assert_eq!(state.public_status(), WebRtcPeerStatus::Connected);
    let snapshot = state.to_snapshot();
    assert_eq!(snapshot.status, WebRtcPeerStatus::Connected);
    assert!(snapshot.sendable);
    assert_eq!(snapshot.pc_generation, generation);
}

#[test]
fn initial_readiness_lane_failure_terminates_before_offer_can_continue() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRoleAssignment { is_offerer: true }),
    );
    let generation = state.building_generation.expect("building generation");
    let operation_id = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::BuildSession { operation_id, .. } => Some(*operation_id),
            _ => None,
        })
        .expect("build operation");
    let build_effects = reduce(&mut state, build_ok(generation, operation_id));
    assert!(build_effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CreateLane {
            generation: candidate,
            lane: LaneKind::RpcReliable,
        } if *candidate == generation
    )));

    let failure_effects = reduce(
        &mut state,
        RtcPeerInput::LaneCreationFailed {
            generation,
            lane: LaneKind::RpcReliable,
        },
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
    assert!(failure_effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession {
            generation: candidate,
            ..
        } if *candidate == generation
    )));
}

#[test]
fn target_unavailable_preserves_the_exact_recovery_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(583));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;

    // A silent remote death first enters the normal recovery path.
    let effects = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::RestartIce { generation: actual, .. } if *actual == generation
    )));

    let negotiation_epoch = state.negotiation_epoch;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation: generation,
            negotiation_epoch,
        }),
    );

    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.unavailable_reason, None);
    assert!(state.recovery_inflight);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession { generation: actual, .. } if *actual == generation
    )));
    assert!(!has_build_session(&effects, generation + 1));

    // Once the exact ICE restart exchange converges, the retained Session can
    // become sendable again. This is the mobile background/foreground path
    // that immediate PeerUnavailable retirement used to destroy.
    state.recovery_negotiated = true;
    reduce(&mut state, connected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(!state.recovery_inflight);
}

#[test]
fn target_unavailable_still_converges_at_existing_recovery_budget() {
    let mut state = RtcPeerState::new(test_actor_id(584));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    reduce(&mut state, failed_tick(generation));
    let negotiation_epoch = state.negotiation_epoch;

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation: generation,
            negotiation_epoch,
        }),
    );

    // Permanent absence is still bounded by the existing recovery budget; the
    // point-in-time server result does not create an unbounded retry loop.
    let recovery_cycle = state.recovery_cycle;
    let exhausted = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed { recovery_cycle },
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::RecoveryExhausted)
    );
    assert!(exhausted.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession { generation: actual, .. } if *actual == generation
    )));
}

#[test]
fn stale_target_unavailable_result_cannot_terminate_a_new_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(584));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;

    let negotiation_epoch = state.negotiation_epoch;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::TargetUnavailable {
            transport_epoch: transport_epoch.saturating_sub(1),
            pc_generation: generation,
            negotiation_epoch,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert_eq!(state.unavailable_reason, None);
}

#[test]
fn target_unavailable_preserves_live_gen0_probe_and_fences_stale_result() {
    // The signaling gate flaps None -> Some while a delivered gen-0 role
    // probe is still in flight: the gate-opening edge re-drives the probe
    // under a fresh negotiation epoch. Both probes dispatch with the
    // identical (transport_epoch, pc_generation = 0) identity, so only the
    // exact probe's epoch may terminate the live arbitration.
    let mut state = RtcPeerState::new(test_actor_id(585));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert!(state.role_negotiation_inflight);
    let retired_epoch = state.negotiation_epoch;
    let transport_epoch = state.transport_epoch;

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged { generation: None },
        )),
    );
    let after_open = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(3),
            },
        )),
    );
    assert!(after_open.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::RoleNegotiation,
            ..
        }
    )));
    assert!(state.role_negotiation_inflight);
    let live_epoch = state.negotiation_epoch;
    assert_ne!(live_epoch, retired_epoch, "each probe bumps the epoch");

    // The delayed negative result for the retired probe matches on transport
    // epoch and generation 0 but names the retired epoch: it must be stale.
    let stale = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation: 0,
            negotiation_epoch: retired_epoch,
        }),
    );
    assert!(stale.is_empty());
    assert!(state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.unavailable_reason, None);

    // The live probe's own point-in-time result is non-terminal too. The
    // existing attempt deadline remains responsible for bounded failure, while
    // a temporarily suspended mobile peer can reconnect with the same ActrId.
    let live = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation: 0,
            negotiation_epoch: live_epoch,
        }),
    );
    assert!(live.is_empty());
    assert!(state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.unavailable_reason, None);

    // The initial role-negotiation deadline still owns the terminal decision:
    // a permanently absent target converges to `NegotiationFailed` on the
    // exact probe epoch instead of an unbounded retry loop.
    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: live_epoch,
        },
    );
    assert!(!state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
}

#[test]
fn failed_initial_sdp_delivery_terminates_the_exact_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    let transport_epoch = state.transport_epoch;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::SignalingDeliveryFailed {
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession {
            generation: candidate,
            ..
        } if *candidate == generation
    )));
}

#[test]
fn stale_sdp_delivery_failure_cannot_terminate_a_new_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_offerer_to_negotiating(&mut state);
    let transport_epoch = state.transport_epoch;

    // A delayed failure from a retired transport attempt must be discarded:
    // the epoch fence alone rejects it before the generation check runs.
    let stale_epoch = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::SignalingDeliveryFailed {
            transport_epoch: transport_epoch.saturating_sub(1),
            pc_generation: generation,
        }),
    );
    assert!(stale_epoch.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert_eq!(state.unavailable_reason, None);

    // Same for a superseded PC generation within the live transport attempt.
    let stale_generation = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::SignalingDeliveryFailed {
            transport_epoch,
            pc_generation: generation.saturating_add(1),
        }),
    );
    assert!(stale_generation.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert_eq!(state.unavailable_reason, None);
}

#[test]
fn answerer_flow_reaches_sendable_and_connected() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    assert_eq!(state.role, Role::Answerer);

    drive_to_sendable(&mut state, generation);

    assert!(state.sendable());
    assert_eq!(state.public_status(), WebRtcPeerStatus::Connected);
}

#[test]
fn connected_requires_both_transport_and_anchor() {
    // Anchor opens before transport is ready: must not become sendable early.
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);

    reduce(&mut state, rpc_reliable_opened(generation));
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert!(
        !state.sendable(),
        "anchor open without transport is not sendable"
    );

    reduce(&mut state, connected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(state.sendable());
}

#[test]
fn initial_build_failure_returns_to_idle() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: 1,
            result: Err(webrtc::error::Error::Other("boom".into())),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::PeerConnectionBuildFailed)
    );
    assert_eq!(state.public_status(), WebRtcPeerStatus::Idle);
}

#[test]
fn fresh_connect_after_terminal_idle_advances_transport_epoch() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: 1,
            result: Err(webrtc::error::Error::Other("boom".into())),
        }),
    );
    assert_eq!(state.transport_epoch, 1);
    assert!(state.terminally_unavailable());

    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert_eq!(state.transport_epoch, 2);
    assert!(!state.terminally_unavailable());
}

#[test]
fn terminal_attempt_clears_attempt_scoped_negotiation_cache() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    state.pending_local_desc = Some(Box::new(offer()));
    state.local_ufrag = Some("stale-local".into());
    state.remote_ufrag = Some("stale-remote".into());
    state.known_remote_ufrags.push_back("stale-remote".into());
    state.pending_candidates.push(candidate("future"));
    state.renegotiation_inflight = true;
    state.address_changed = true;

    reduce(&mut state, failed_tick(generation));

    assert!(state.terminally_unavailable());
    assert_eq!(state.role, Role::Undetermined);
    assert_eq!(state.network_path, NetworkPath::Unknown);
    assert!(state.pending_local_desc.is_none());
    assert!(state.local_ufrag.is_none());
    assert!(state.remote_ufrag.is_none());
    assert!(state.known_remote_ufrags.is_empty());
    assert!(state.pending_candidates.is_empty());
    assert!(!state.renegotiation_inflight);
    assert!(!state.address_changed);
}

// ── role negotiation & glare (Stage 3) ───────────────────────────────────────

#[test]
fn connect_negotiates_role_before_building() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let effects = reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    // Connect requests the server to arbitrate; it must not build/offer yet.
    assert!(state.role_negotiation_inflight);
    assert!(has_role_negotiation(&effects));
    assert!(!has_build_session(&effects, 1));
    assert_eq!(state.role, super::state::Role::Undetermined);
}

#[test]
fn exact_role_negotiation_deadline_fails_only_matching_epoch() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    let epoch = state.negotiation_epoch;

    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: epoch.wrapping_add(1),
        },
    );
    assert!(
        state.role_negotiation_inflight,
        "stale deadline must be ignored"
    );
    assert_eq!(state.unavailable_reason, None);

    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: epoch,
        },
    );
    assert!(!state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
}

#[test]
fn initial_connection_deadline_covers_pc_connected_until_readiness_opens() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    let generation = state.building_generation.expect("initial generation");
    let build_operation =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, generation);
    reduce(&mut state, build_ok(generation, build_operation));
    reduce(&mut state, connected_tick(generation));

    assert!(state.ever_connected, "the physical PC reported Connected");
    assert!(state.initial_connection_inflight);
    assert_eq!(state.phase, RtcPeerPhase::TransportReady);
    assert!(!state.sendable(), "readiness anchor has not opened");

    let transport_epoch = state.transport_epoch;
    let stale = reduce(
        &mut state,
        RtcPeerInput::InitialConnectionDeadlineElapsed {
            transport_epoch: transport_epoch.wrapping_add(1),
        },
    );
    assert!(stale.is_empty());
    assert_eq!(state.active_generation, Some(generation));

    let effects = reduce(
        &mut state,
        RtcPeerInput::InitialConnectionDeadlineElapsed { transport_epoch },
    );
    assert!(has_close_session(&effects, generation));
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.initial_connection_inflight);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
}

#[test]
fn role_assignment_offerer_starts_offerer_build() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    let effects = reduce(&mut state, role_assignment(true));
    assert_eq!(state.role, super::state::Role::Offerer);
    assert!(!state.role_negotiation_inflight);
    assert!(has_build_session(&effects, 1));
}

/// A passive peer assigned OFFERER (the dialer negotiated and lost the
/// serial tie-break) must drive the offer itself — 0.14
/// `Coordinator::handle_role_assignment` spawns `start_offer_connection`.
/// Recording the role without building deadlocks the pair: the dialer is
/// the answerer and waits forever (node-level dial-on-demand relies on
/// this arm; the smoke e2e hangs without it).
#[test]
fn unsolicited_offerer_assignment_drives_build() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    // No Connect first — the assignment arrives unsolicited (the other peer
    // initiated the role negotiation).
    let effects = reduce(&mut state, role_assignment(true));
    assert_eq!(state.role, super::state::Role::Offerer);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(has_build_session(&effects, 1));

    // ...and the build completion drives the offer, as in the awaited flow.
    let effects = reduce(&mut state, build_ok(1, 1));
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert!(has_create_offer(&effects, 1));
}

#[test]
fn repeated_offerer_assignment_restarts_never_sendable_initial_pc() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    let generation = state.building_generation.expect("initial generation");
    let build_operation =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, generation);
    let build_effects = reduce(&mut state, build_ok(generation, build_operation));
    assert!(has_create_offer(&build_effects, generation));
    let stale_offer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateOffer, generation);
    let transport_epoch = state.transport_epoch;

    let effects = reduce(&mut state, role_assignment(true));

    assert_eq!(state.role, Role::Offerer);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(state.initial_connection_inflight);
    assert_eq!(state.transport_epoch, transport_epoch + 1);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.building_generation, Some(generation + 1));
    assert!(has_close_session(&effects, generation));
    assert!(has_build_session(&effects, generation + 1));

    let stale_before = state.stale_completions;
    let stale_effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: stale_offer_operation,
            result: Ok(Box::new(offer())),
        }),
    );
    assert!(stale_effects.is_empty());
    assert_eq!(state.stale_completions, stale_before + 1);
    assert_eq!(state.building_generation, Some(generation + 1));
}

#[test]
fn repeated_offerer_assignment_rebuilds_for_recreated_remote_answerer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let effects = reduce(&mut state, role_assignment(true));

    assert_eq!(state.role, super::state::Role::Offerer);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.building_generation, Some(generation + 1));
    assert!(has_build_session(&effects, generation + 1));
}

/// A passive peer assigned ANSWERER records the role and waits for the
/// offer (no build, no effects).
#[test]
fn unsolicited_answerer_assignment_awaits_offer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let effects = reduce(&mut state, role_assignment(false));
    assert_eq!(state.role, super::state::Role::Answerer);
    assert!(state.awaiting_offer);
    assert!(effects.is_empty());
}

#[test]
fn role_assignment_answerer_waits_then_answers_offer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    let effects = reduce(&mut state, role_assignment(false));
    // Assigned answerer: no build, waits for the offer.
    assert_eq!(state.role, super::state::Role::Answerer);
    assert!(state.awaiting_offer);
    assert!(effects.is_empty(), "answerer assignment does not build");
    assert_eq!(state.building_generation, None);

    // The offer then drives the answerer build.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(offer()))),
    );
    assert!(has_build_session(&effects, 1));
    assert!(!state.awaiting_offer);
    assert_eq!(state.building_generation, Some(1));
}

#[test]
fn assigned_offerer_ignores_competing_offer_glare() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    let stale_before = state.stale_events;

    // I am the assigned offerer; a competing offer is glare and must be ignored
    // (the server made me the offerer), not answered.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(offer()))),
    );
    assert_eq!(state.stale_events, stale_before + 1);
    assert!(!has_build_session(&effects, generation + 1));
    assert_eq!(state.role, super::state::Role::Offerer);
    assert_eq!(state.active_generation, Some(generation));
}

#[test]
fn incoming_offer_wins_over_inflight_role_negotiation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert!(state.role_negotiation_inflight);

    // An offer arrives before the assignment: we become the answerer (offer
    // wins) and the in-flight role negotiation is abandoned.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(offer()))),
    );
    assert_eq!(state.role, super::state::Role::Answerer);
    assert!(!state.role_negotiation_inflight);
    assert!(has_build_session(&effects, 1));
}

#[test]
fn ordinary_sdp_exchange_rejects_stale_answer_and_accepts_matching_answer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    let effects = reduce(&mut state, build_ok(1, 1));
    assert!(has_create_offer(&effects, 1));
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: 1,
            operation_id: 2,
            result: Ok(Box::new(offer())),
        }),
    );
    assert!(has_set_local(&effects, 1));
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 3,
            result: Ok(()),
        }),
    );
    let exchange_id = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::SendSignaling {
                message:
                    SignalingMessage::Offer {
                        exchange_id: Some(exchange_id),
                        ..
                    },
                ..
            } => Some(exchange_id.clone()),
            _ => None,
        })
        .expect("ordinary Offer must carry an exchange id");

    let stale_before = state.stale_events;
    let stale_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(answer()),
            exchange_id: Some("stale-exchange".to_string()),
        }),
    );
    assert!(stale_effects.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);
    assert_eq!(
        state.pending_local_sdp_exchange_id.as_deref(),
        Some(exchange_id.as_str())
    );

    let matching_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(answer()),
            exchange_id: Some(exchange_id),
        }),
    );
    assert!(has_set_remote(&matching_effects, 1));
    assert!(state.pending_local_sdp_exchange_id.is_none());

    let stale_before = state.stale_events;
    let duplicate_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(answer()))),
    );
    assert!(duplicate_effects.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);
}

#[test]
fn answer_echoes_the_remote_offer_exchange_id() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let exchange_id = "remote-offer-exchange".to_string();
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(offer()),
            exchange_id: Some(exchange_id.clone()),
        }),
    );
    reduce(&mut state, build_ok(1, 1));
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation: 1,
            operation_id: 3,
            result: Ok(Box::new(answer())),
        }),
    );
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 4,
            result: Ok(()),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::Answer {
                exchange_id: Some(actual),
                ..
            },
            ..
        } if actual == &exchange_id
    )));
    assert!(state.pending_remote_sdp_exchange_id.is_none());
}

#[test]
fn local_candidate_waits_until_offer_is_queued_and_keeps_fifo_order() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    reduce(&mut state, build_ok(1, 1));
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: 1,
            operation_id: 2,
            result: Ok(Box::new(sdp_with_ufrag(RTCSdpType::Offer, "local-1"))),
        }),
    );
    assert!(has_set_local(&effects, 1));

    let early = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::LocalIceCandidate {
            generation: 1,
            candidate: local_candidate(),
        }),
    );
    assert!(early.is_empty(), "candidate must wait behind the Offer");
    assert_eq!(state.pending_local_candidates.len(), 1);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 3,
            result: Ok(()),
        }),
    );
    let signaling = effects
        .iter()
        .filter_map(|effect| match effect {
            RtcPeerEffect::SendSignaling { message, .. } => Some(message),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        signaling.first(),
        Some(SignalingMessage::Offer { .. })
    ));
    assert!(matches!(
        signaling.get(1),
        Some(SignalingMessage::Candidate(_))
    ));
    assert!(state.pending_local_candidates.is_empty());
}

#[test]
fn old_active_candidate_cannot_enter_replacement_signaling_exchange() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.building_generation, Some(old_generation + 1));

    let stale_before = state.stale_events;
    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::LocalIceCandidate {
            generation: old_generation,
            candidate: local_candidate(),
        }),
    );
    assert!(effects.is_empty());
    assert!(state.pending_local_candidates.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);
}

#[test]
fn hard_recovery_rejects_inflight_sdp_completion_from_old_active_generation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded {
            generation: old_generation,
        }),
    );
    let create_offer_operation = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::CreateOffer {
                generation,
                operation_id,
            } if *generation == old_generation => Some(*operation_id),
            _ => None,
        })
        .expect("renegotiation CreateOffer");
    state.pending_remote_offer = Some(Box::new(sdp_with_ufrag(RTCSdpType::Offer, "stale")));
    state
        .pending_local_candidates
        .push((old_generation, candidate("stale")));
    state.last_remote_restart_request = Some(("stale-request".to_string(), 3));
    state.renegotiation_pending = true;

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.building_generation, Some(old_generation + 1));
    let stale_before = state.stale_completions;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: old_generation,
            operation_id: create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(
                RTCSdpType::Offer,
                "old-active-offer",
            ))),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(state.pending_local_desc.is_none());
    assert!(state.local_ufrag.is_none());
    assert!(state.pending_remote_offer.is_none());
    assert!(state.pending_local_candidates.is_empty());
    assert!(state.last_remote_restart_request.is_none());
    assert!(!state.renegotiation_inflight);
    assert!(!state.renegotiation_pending);
}

#[test]
fn network_loss_rejects_inflight_sdp_completion_from_the_previous_flight() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded { generation }),
    );
    let create_offer_operation = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::CreateOffer {
                generation: actual_generation,
                operation_id,
            } if *actual_generation == generation => Some(*operation_id),
            _ => None,
        })
        .expect("renegotiation CreateOffer");

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    let stale_before = state.stale_completions;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(
                RTCSdpType::Offer,
                "pre-loss-offer",
            ))),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(state.pending_local_desc.is_none());
    assert!(state.local_ufrag.is_none());
}

#[test]
fn established_renegotiation_failure_starts_soft_recovery_immediately() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded { generation }),
    );
    let operation_id = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::CreateOffer {
                operation_id,
                generation: actual_generation,
            } if *actual_generation == generation => Some(*operation_id),
            _ => None,
        })
        .expect("renegotiation CreateOffer");

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id,
            result: Err(webrtc::error::Error::Other(
                "renegotiation failed".to_string(),
            )),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert!(has_restart_ice(&effects, generation));
}

#[test]
fn pending_replacement_negotiation_failure_retries_hard_on_attempt_deadline() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let replacement = old_generation + 1;
    let build_operation =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, replacement);
    let effects = reduce(&mut state, build_ok(replacement, build_operation));
    let offer_operation = effects
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::CreateOffer {
                generation,
                operation_id,
            } if *generation == replacement => Some(*operation_id),
            _ => None,
        })
        .expect("replacement CreateOffer");

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: replacement,
            operation_id: offer_operation,
            result: Err(webrtc::error::Error::Other(
                "replacement negotiation failed".to_string(),
            )),
        }),
    );
    assert!(has_close_session(&effects, replacement));
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.active_generation, Some(old_generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.address_changed, "deadline must choose Hard retry");

    let recovery_epoch = state.recovery_epoch;
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
    );
    assert!(has_build_session(&effects, replacement + 1));
    assert_eq!(state.building_generation, Some(replacement + 1));
    assert!(!state.address_changed);
}

// ── ICE generation / ufrag candidate tagging (Stage 3) ───────────────────────

#[test]
fn session_ice_ufrag_parsing() {
    use super::state::session_ice_ufrag;
    // Single-bundle agreement.
    let sdp = "v=0\r\na=ice-ufrag:AbCd\r\na=ice-pwd:x\r\na=ice-ufrag:AbCd\r\n";
    assert_eq!(session_ice_ufrag(sdp).as_deref(), Some("AbCd"));
    // Media sections disagree → ambiguous.
    let ambiguous = "a=ice-ufrag:AAA\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=ice-ufrag:BBB\r\n";
    assert_eq!(session_ice_ufrag(ambiguous), None);
    // No ufrag at all.
    assert_eq!(session_ice_ufrag("v=0\r\na=ice-pwd:x\r\n"), None);
}

#[test]
fn classify_candidate_ufrag_apply_drop_buffer() {
    use super::state::CandidateUfragDisposition as Disp;
    let mut state = RtcPeerState::new(test_actor_id(1));
    // No SDP yet: unknown ufrag buffers.
    assert_eq!(state.classify_candidate_ufrag(Some("AAA")), Disp::Buffer);

    state.set_remote_ufrag("AAA".to_string());
    // Current ufrag applies.
    assert_eq!(state.classify_candidate_ufrag(Some("AAA")), Disp::Apply);
    // Unknown (but current is set) → buffer as a possible future generation.
    assert_eq!(state.classify_candidate_ufrag(Some("ZZZ")), Disp::Buffer);

    // A new ICE generation makes the old one stale.
    state.set_remote_ufrag("CCC".to_string());
    assert_eq!(state.classify_candidate_ufrag(Some("CCC")), Disp::Apply);
    assert_eq!(state.classify_candidate_ufrag(Some("AAA")), Disp::DropStale);
    // Missing ufrag buffers.
    assert_eq!(state.classify_candidate_ufrag(None), Disp::Buffer);
}

#[test]
fn candidate_with_current_ufrag_waits_for_remote_description_completion() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    // Answerer learns the offer's ufrag on the way to active.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "AAA"),
        ))),
    );
    assert_eq!(state.remote_ufrag.as_deref(), Some("AAA"));
    let build_effects = reduce(&mut state, build_ok(1, 1));
    assert!(has_set_remote(&build_effects, 1));

    let stale_before = state.stale_events;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteCandidate(candidate("AAA"))),
    );
    assert!(
        !has_add_ice_candidate(&effects, 1),
        "candidate must not race set_remote_description"
    );
    assert_eq!(state.pending_candidates.len(), 1);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_add_ice_candidate(&effects, 1));
    assert!(state.pending_candidates.is_empty());
    assert_eq!(
        state.stale_events, stale_before,
        "current-ufrag candidate is applied"
    );
}

#[test]
fn candidate_before_sdp_is_buffered_until_remote_description_completes() {
    let mut state = RtcPeerState::new(test_actor_id(1));

    // A candidate with an unknown ufrag before any SDP is buffered, not applied.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteCandidate(candidate("AAA"))),
    );
    assert!(!has_add_ice_candidate(&effects, 1));
    assert_eq!(state.pending_candidates.len(), 1);

    // The matching offer arrives: ufrag learned, but the session is still
    // building, so the candidate stays buffered.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "AAA"),
        ))),
    );
    assert_eq!(state.pending_candidates.len(), 1);

    // Build completion only schedules SetRemoteDescription.  It must not race
    // the buffered AddIceCandidate effect.
    let effects = reduce(&mut state, build_ok(1, 1));
    assert!(has_set_remote(&effects, 1));
    assert!(!has_add_ice_candidate(&effects, 1));
    assert_eq!(state.pending_candidates.len(), 1);

    // Only the exact remote-description completion unlocks the candidate.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_add_ice_candidate(&effects, 1));
    assert!(state.pending_candidates.is_empty());
}

#[test]
fn stale_remote_description_operation_cannot_unlock_candidates() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "AAA"),
        ))),
    );
    reduce(&mut state, build_ok(1, 1));
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteCandidate(candidate("AAA"))),
    );

    let stale_before = state.stale_completions;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 99,
            result: Ok(()),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.stale_completions, stale_before + 1);
    assert_eq!(state.pending_candidates.len(), 1);
    assert_eq!(state.pending_remote_description_operation, Some((1, 2)));

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_add_ice_candidate(&effects, 1));
}

#[test]
fn remote_descriptions_are_serialized_and_only_latest_creates_answer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "AAA"),
        ))),
    );
    reduce(&mut state, build_ok(1, 1));

    // A same-generation renegotiation offer arrives while the initial remote
    // description is still in flight.  It is queued rather than submitted in
    // parallel.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "AAA"),
        ))),
    );
    assert!(!has_set_remote(&effects, 1));
    assert!(state.queued_remote_description.is_some());

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_set_remote(&effects, 1));
    assert!(!has_create_answer(&effects, 1));
    assert_eq!(state.pending_remote_description_operation, Some((1, 3)));

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 3,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&effects, 1));
}

// ── invariant: one active generation ────────────────────────────────────────

#[test]
fn connect_is_noop_when_not_idle_preserving_single_active_generation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    // A second Connect while Sendable must not allocate a new generation or
    // disturb the active one.
    let effects = reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert!(effects.is_empty(), "Connect while Sendable is a no-op");
    assert_eq!(state.active_generation, Some(generation));
    assert!(state.sendable());
}

// ── invariant: stale events / completions ───────────────────────────────────

#[test]
fn stale_driver_event_from_old_generation_does_not_change_state() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    let stale_before = state.stale_events;

    // A candidate / event tagged with a never-live generation is rejected.
    reduce(&mut state, connected_tick(99));
    assert_eq!(state.stale_events, stale_before + 1);
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(state.sendable());
}

#[test]
fn stale_completion_from_old_generation_is_dropped_and_counted() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    let stale_before = state.stale_completions;

    // A CreateOffer completion for a superseded generation must not produce a
    // SetLocalDescription effect or change negotiation state.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation: 99,
            operation_id: 1,
            result: Ok(Box::new(offer())),
        }),
    );
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(!has_set_local(&effects, 99));
    assert_eq!(state.active_generation, Some(generation));
}

// ── N5 regression: spurious BuildSession completions (orphan hardening) ──────
//
// The property test found that a stale/spurious `BuildSession` completion could
// (a) close a never-allocated generation, (b) close a generation that had since
// become active, or (c) clear the genuinely in-flight build during close. These
// named tests lock the orphan rule directly so a future change cannot silently
// regress it behind the randomized property test.

#[test]
fn spurious_build_completion_for_never_allocated_generation_closes_nothing() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let stale_before = state.stale_completions;

    // A successful build completion for a generation that was never allocated
    // (nothing is in flight) is merely stale: it must not emit a CloseSession
    // for a non-existent session.
    let effects = reduce(&mut state, build_ok(99, 1));
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(
        !has_close_session(&effects, 99),
        "must not close a never-allocated generation"
    );
    assert_eq!(state.active_generation, None);
    assert_eq!(state.phase, RtcPeerPhase::Idle);
}

#[test]
fn spurious_build_completion_for_active_generation_does_not_close_it() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    let stale_before = state.stale_completions;

    // A duplicate successful build completion for the *active* generation is
    // stale: it must not close the live, sendable session.
    let effects = reduce(&mut state, build_ok(generation, 1));
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(
        !has_close_session(&effects, generation),
        "must not close the active session on a stale build completion"
    );
    assert_eq!(state.active_generation, Some(generation));
    assert!(state.sendable());
}

#[test]
fn close_during_build_ignores_spurious_completion_for_other_generation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    assert_eq!(state.building_generation, Some(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    let stale_before = state.stale_completions;

    // A build completion for a generation that is NOT the in-flight build must
    // not clear the real build or close anything / finish the close early.
    let effects = reduce(&mut state, build_ok(99, 1));
    assert_eq!(state.stale_completions, stale_before + 1);
    assert!(
        !has_close_session(&effects, 99),
        "must not close a spurious generation during close"
    );
    assert_eq!(
        state.building_generation,
        Some(1),
        "the genuine in-flight build must not be cleared"
    );
    assert_eq!(
        state.phase,
        RtcPeerPhase::Closing,
        "close barrier must not finish on a spurious completion"
    );
}

// ── close barrier: close-during-build ───────────────────────────────────────

#[test]
fn close_during_role_arbitration_finishes_without_waiting_for_a_completion() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(state.role_negotiation_inflight);
    assert_eq!(state.building_generation, None);

    let effects = reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));

    assert!(
        effects.is_empty(),
        "there is no session generation to close"
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.close_requested);
    assert!(!state.role_negotiation_inflight);
    assert!(state.terminal_close_done);
}

#[test]
fn close_during_build_late_success_closes_orphan_and_never_becomes_sendable() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    assert_eq!(state.phase, RtcPeerPhase::Building);

    // Close while the build is in flight.
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    assert!(state.close_requested);

    // The build completes *after* close was requested (late success).
    let effects = reduce(&mut state, build_ok(1, 1));
    // The orphan session must be closed, never activated.
    assert!(has_close_session(&effects, 1));
    assert_eq!(state.active_generation, None);
    assert!(!state.sendable());
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    let close_operation_id = close_operation(&effects, 1);

    // Once the orphan close completes, the barrier releases to Idle.
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CloseSession {
            generation: 1,
            operation_id: close_operation_id,
            result: Ok(()),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.close_requested);
    assert!(!state.sendable());
}

#[test]
fn close_during_build_late_failure_finishes_close_without_session() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, role_assignment(true));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);

    // Build fails after close was requested (late failure): nothing to close,
    // so the barrier releases immediately.
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: 1,
            result: Err(webrtc::error::Error::Other("boom".into())),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.close_requested);
}

#[test]
fn explicit_close_from_sendable_waits_for_session_close() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    assert!(has_close_session(&effects, generation));
    assert!(!state.sendable(), "sendable drops immediately on close");
    let close_operation_id = close_operation(&effects, generation);

    // The controller stays in Closing until the session close completes.
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CloseSession {
            generation,
            operation_id: close_operation_id,
            result: Ok(()),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(state.public_status(), WebRtcPeerStatus::Idle);
}

// ── O1: mailbox overflow → hard recovery ────────────────────────────────────

#[test]
fn mailbox_overflow_on_active_generation_hard_recovers() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    assert!(state.sendable());

    let effects = reduce(&mut state, RtcPeerInput::MailboxOverflow { generation });
    // S5-4: make-before-break — old session stays alive (active), new is
    // being built (building). The old is not closed yet (only after atomic
    // switch when the pending session is ready).
    assert!(has_build_session(&effects, generation + 1));
    assert!(!state.sendable(), "sendable drops on overflow");
    assert_eq!(
        state.active_generation,
        Some(generation),
        "old session kept alive (make-before-break)"
    );
    assert_eq!(state.building_generation, Some(generation + 1));
    assert_eq!(state.pending_generation, None, "pending set after build");
    // S5-9: the overflow is observable via the counter.
    assert_eq!(state.mailbox_overflows, 1);
    assert_eq!(state.to_snapshot().mailbox_overflows, 1);
}

/// A second overflow on the new active generation increments the counter again.
#[test]
fn mailbox_overflow_counter_increments_on_repeat() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let _ = reduce(&mut state, RtcPeerInput::MailboxOverflow { generation });
    assert_eq!(state.mailbox_overflows, 1);

    // Simulate the new generation becoming active, then overflowing.
    let new_gen = generation + 1;
    state.active_generation = Some(new_gen);
    state.building_generation = None;
    state.phase = RtcPeerPhase::Sendable;
    let _ = reduce(
        &mut state,
        RtcPeerInput::MailboxOverflow {
            generation: new_gen,
        },
    );
    assert_eq!(state.mailbox_overflows, 2);
    assert_eq!(state.to_snapshot().mailbox_overflows, 2);
}

#[test]
fn mailbox_overflow_on_non_active_generation_is_ignored() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(&mut state, RtcPeerInput::MailboxOverflow { generation: 99 });
    assert!(effects.is_empty());
    assert!(state.sendable());
}

#[test]
fn mailbox_overflow_on_pending_generation_replaces_only_pending() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let pending_generation = old_generation + 1;
    let build_operation = pending_operation(
        &state,
        PendingRtcOperationKind::BuildSession,
        pending_generation,
    );
    reduce(&mut state, build_ok(pending_generation, build_operation));

    let effects = reduce(
        &mut state,
        RtcPeerInput::MailboxOverflow {
            generation: pending_generation,
        },
    );

    assert!(has_close_session(&effects, pending_generation));
    assert!(has_build_session(&effects, pending_generation + 1));
    assert_eq!(state.active_generation, Some(old_generation));
    assert_eq!(state.building_generation, Some(pending_generation + 1));
    assert_eq!(state.mailbox_overflows, 1);
}

#[test]
fn mailbox_overflow_on_building_generation_abandons_and_rebuilds_it() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let building_generation = old_generation + 1;

    let effects = reduce(
        &mut state,
        RtcPeerInput::MailboxOverflow {
            generation: building_generation,
        },
    );

    assert!(!has_close_session(&effects, building_generation));
    assert!(has_build_session(&effects, building_generation + 1));
    assert_eq!(state.active_generation, Some(old_generation));
    assert_eq!(state.building_generation, Some(building_generation + 1));
    assert_eq!(state.mailbox_overflows, 1);
}

// ── recovery transitions ─────────────────────────────────────────────────────

#[test]
fn pending_anchor_close_retires_pending_generation_and_clears_readiness() {
    // Make-before-break: an active sendable session plus a pending
    // replacement whose readiness anchor already reported open. The
    // controller removed the pending anchor's lane handle on Closed; the
    // reducer must retire the pending generation and clear its readiness so
    // a later connected tick cannot atomic-switch the dead generation into
    // the active slot and tear down the working connection.
    let mut state = RtcPeerState::new(test_actor_id(1));
    let active = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, active);
    let pending = active + 1;
    state.pending_generation = Some(pending);
    state.pending_pc_connected = true;
    state.pending_rpc_reliable_open = true;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Lane(LaneEvent::Closed {
            generation: pending,
            lane: LaneKind::READINESS_ANCHOR,
        }),
    );

    assert_eq!(state.pending_generation, None);
    assert!(!state.pending_rpc_reliable_open);
    assert!(!state.pending_pc_connected);
    assert!(has_close_session(&effects, pending));
    // The working active generation is untouched.
    assert_eq!(state.active_generation, Some(active));
    assert!(state.sendable());
}

#[test]
fn anchor_close_drops_sendable_and_starts_hard_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Lane(LaneEvent::Closed {
            generation,
            lane: LaneKind::RpcReliable,
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(!state.sendable());
    assert_eq!(state.public_status(), WebRtcPeerStatus::Recovering);
    assert_eq!(state.building_generation, Some(generation + 1));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::BuildSession {
            generation: next,
            ..
        } if *next == generation + 1
    )));
}

#[test]
fn failed_transport_after_connect_recovers_not_idle() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(!state.sendable());
}

#[test]
fn coalesced_failed_pc_state_overrides_previous_connected_ice_state() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(
        &mut state,
        RtcPeerInput::StateTick(Box::new(PeerStateTick {
            generation,
            seq: 2,
            pc_state: Some(RTCPeerConnectionState::Failed),
            ice_state: Some(RTCIceConnectionState::Connected),
            ..Default::default()
        })),
    );

    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(!state.sendable());
    assert!(has_restart_ice(&effects, generation));
}

#[test]
fn failed_transport_on_initial_connect_returns_idle() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    // Fail before ever connecting.
    let effects = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
    assert_eq!(state.active_generation, None);
    assert!(has_close_session(&effects, generation));
}

#[test]
fn closed_established_session_retires_generation_and_hard_recovers() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(&mut state, closed_tick(generation));

    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(state.recovery_inflight);
    assert_eq!(state.active_generation, None);
    assert!(has_close_session(&effects, generation));
    assert!(has_build_session(&effects, generation + 1));
    assert_eq!(state.transport_epoch, 1, "Hard Recovery keeps the flight");
}

#[test]
fn driver_exit_established_session_retires_generation_and_hard_recovers() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::DriverExited {
            generation,
            error: Some(webrtc::error::Error::Other("driver stopped".into())),
        }),
    );

    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(has_close_session(&effects, generation));
    assert!(has_build_session(&effects, generation + 1));
}

// ── snapshot revision monotonicity ──────────────────────────────────────────

#[test]
fn snapshot_revision_is_monotonic_across_transitions() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    assert_eq!(state.to_snapshot().revision, 0);

    // Connect + negotiate advances the revision (Building, then Negotiating).
    let generation = drive_offerer_to_negotiating(&mut state);
    let after_negotiating = state.to_snapshot().revision;
    assert!(
        after_negotiating > 0,
        "revision advanced during connect/negotiate"
    );

    // Reaching sendable must keep the revision climbing (never regress).
    drive_to_sendable(&mut state, generation);
    let after_sendable = state.to_snapshot().revision;
    assert!(
        after_sendable > after_negotiating,
        "revision keeps climbing to sendable"
    );
}

// ── property test (dependency-free, deterministic) ──────────────────────────

/// A tiny deterministic PRNG (xorshift64*) so the property test is reproducible
/// without adding a `proptest`/`quickcheck` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// Build a pseudo-random reducer input. Generations are drawn from a small range
/// so both live and stale generations are exercised.
fn random_input(rng: &mut Rng) -> RtcPeerInput {
    let generation = rng.below(4);
    let op = rng.below(6);
    match rng.below(15) {
        0 => RtcPeerInput::Command(RtcPeerCommand::Connect),
        1 => RtcPeerInput::Command(RtcPeerCommand::Close),
        2 => RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(offer()))),
        3 => RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(answer()))),
        14 => RtcPeerInput::Command(RtcPeerCommand::ApplyRoleAssignment {
            is_offerer: rng.below(2) == 0,
        }),
        4 => RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation,
            operation_id: op,
            result: if rng.below(2) == 0 {
                Ok(())
            } else {
                Err(webrtc::error::Error::Other("boom".into()))
            },
        }),
        5 => RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: op,
            result: if rng.below(2) == 0 {
                Ok(Box::new(offer()))
            } else {
                Err(webrtc::error::Error::Other("boom".into()))
            },
        }),
        6 => RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: op,
            result: if rng.below(2) == 0 {
                Ok(())
            } else {
                Err(webrtc::error::Error::Other("boom".into()))
            },
        }),
        7 => RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: op,
            result: if rng.below(2) == 0 {
                Ok(())
            } else {
                Err(webrtc::error::Error::Other("boom".into()))
            },
        }),
        8 => RtcPeerInput::Completion(EffectCompletion::CloseSession {
            generation,
            operation_id: op,
            result: Ok(()),
        }),
        9 => RtcPeerInput::StateTick(Box::new(PeerStateTick {
            generation,
            seq: 1,
            pc_state: Some(RTCPeerConnectionState::Connected),
            ice_state: Some(RTCIceConnectionState::Connected),
            gathering_state: None,
            signaling_state: None,
            overflow_count: 0,
        })),
        10 => RtcPeerInput::StateTick(Box::new(PeerStateTick {
            generation,
            seq: 1,
            pc_state: Some(RTCPeerConnectionState::Failed),
            ice_state: None,
            gathering_state: None,
            signaling_state: None,
            overflow_count: 0,
        })),
        11 => RtcPeerInput::StateTick(Box::new(PeerStateTick {
            generation,
            seq: 1,
            pc_state: Some(RTCPeerConnectionState::Disconnected),
            ice_state: None,
            gathering_state: None,
            signaling_state: None,
            overflow_count: 0,
        })),
        12 => RtcPeerInput::Lane(LaneEvent::Opened {
            generation,
            lane: LaneKind::RpcReliable,
        }),
        13 => RtcPeerInput::Lane(LaneEvent::Closed {
            generation,
            lane: LaneKind::RpcReliable,
        }),
        _ => RtcPeerInput::MailboxOverflow { generation },
    }
}

/// Property: across arbitrary input sequences the reducer never panics and the
/// core invariants always hold — at most one live sendable generation, building
/// and active generations are disjoint, generations and revisions are monotone.
#[test]
fn property_random_inputs_preserve_invariants() {
    for seed in 1..=12u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut state = RtcPeerState::new(test_actor_id(seed));
        let mut last_generation = state.next_pc_generation;
        let mut last_revision = state.revision;

        for step in 0..400 {
            let input = random_input(&mut rng);
            let effects = reduce(&mut state, input);

            // The reducer is total: effects are well-formed (generations it
            // references were allocated at some point).
            for effect in &effects {
                let g = match effect {
                    RtcPeerEffect::BuildSession { generation, .. }
                    | RtcPeerEffect::CreateOffer { generation, .. }
                    | RtcPeerEffect::CreateAnswer { generation, .. }
                    | RtcPeerEffect::CreateLane { generation, .. }
                    | RtcPeerEffect::SetLocalDescription { generation, .. }
                    | RtcPeerEffect::SetRemoteDescription { generation, .. }
                    | RtcPeerEffect::AddIceCandidate { generation, .. }
                    | RtcPeerEffect::RestartIce { generation, .. }
                    | RtcPeerEffect::CloseSession { generation, .. }
                    | RtcPeerEffect::RefreshConfiguration { generation, .. }
                    | RtcPeerEffect::GetStats { generation, .. }
                    | RtcPeerEffect::SendSignaling { generation, .. } => *generation,
                };
                assert!(
                    g < state.next_pc_generation,
                    "seed {seed} step {step}: effect references unallocated generation {g}"
                );
            }

            // Invariant: sendable implies an active generation that is connected
            // with the anchor open, not recovering and not closing.
            if state.sendable() {
                assert!(state.active_generation.is_some());
                assert!(state.pc_connected && state.rpc_reliable_open);
                assert!(!state.recovery_inflight && !state.close_requested);
                assert_eq!(state.public_status(), WebRtcPeerStatus::Connected);
            }

            // Invariant: building and active generations are disjoint.
            if let (Some(b), Some(a)) = (state.building_generation, state.active_generation) {
                assert_ne!(b, a, "seed {seed} step {step}: building == active");
            }

            // Invariant: generations and revisions are monotone non-decreasing.
            assert!(state.next_pc_generation >= last_generation);
            assert!(state.revision >= last_revision);
            last_generation = state.next_pc_generation;
            last_revision = state.revision;
        }
    }
}

// ── S5-1: RecoverySupervisor broadcast → reducer ────────────────────────────

#[test]
fn network_event_increments_observable_counter_and_bumps_revision() {
    use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;

    let mut state = RtcPeerState::new(test_actor_id(1));
    let before_revision = state.to_snapshot().revision;
    assert_eq!(state.network_events_received, 0);

    // The reducer stub (S5-1) accepts the event, increments the
    // observability counter, and bumps the revision so subscribers can
    // detect the receipt. The actual recovery policy lands in S5-2+.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    assert!(effects.is_empty(), "S5-1 stub emits no effects");
    assert_eq!(state.network_events_received, 1);
    assert!(state.to_snapshot().revision > before_revision);

    // Multiple events keep counting.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert_eq!(state.network_events_received, 3);
}

#[test]
fn network_recovery_waits_for_committed_signaling_generation() {
    use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;

    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged { generation: None },
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let before_signaling = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(
        !has_restart_ice(&before_signaling, generation),
        "NetworkAvailable must not start ICE while signaling is gated"
    );
    assert!(state.recovery_inflight);
    assert!(!state.recovery_attempt_started);

    let after_signaling = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(7),
            },
        )),
    );
    assert!(
        has_restart_ice(&after_signaling, generation),
        "a committed signaling generation must release pending ICE recovery"
    );
}

#[test]
fn recovery_origin_survives_gate_deferral_and_soft_retry() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // A peer failure while signaling is gated must remain a PeerFailure after
    // the gate opens; NetworkAvailable/SignalingGenerationChanged are merely
    // the facts that release the deferred cycle.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged { generation: None },
        )),
    );
    let deferred = reduce(&mut state, failed_tick(generation));
    assert!(!has_restart_ice(&deferred, generation));
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::PeerFailure));
    assert!(state.recovery_inflight);
    assert!(!state.recovery_attempt_started);

    let resumed = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(7),
            },
        )),
    );
    assert!(has_restart_ice(&resumed, generation));
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::PeerFailure));

    let retry_epoch = state.recovery_epoch;
    let retry = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: retry_epoch,
        },
    );
    assert!(has_restart_ice(&retry, generation));
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::PeerFailure));
}

#[test]
fn gate_opening_re_drives_deferred_role_negotiation_probe() {
    // Connect while no signaling generation is committed: the probe this
    // reduction emits is dropped by the controller's generation gate, and
    // before the re-drive the 5s role deadline failed the attempt as
    // NegotiationFailed (issue: pair-contract suite never became sendable).
    let mut state = RtcPeerState::new(test_actor_id(1));
    // Production controller construction overwrites the synthetic default
    // (`Some(0)`) with the supervisor's watch, which is None until the
    // lifecycle supervisor commits a generation.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged { generation: None },
        )),
    );
    assert!(state.signaling_generation.is_none());
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    assert!(state.role_negotiation_inflight);
    let epoch_of_dropped_probe = state.negotiation_epoch;

    // The deadline is the bounded wait the connection budget exposes, so it
    // fails the attempt even though the probe was never deliverable — but
    // `role_probe_undelivered` keeps the door open for the gate-opening
    // resurrection (under load the supervisor's first commit can land after
    // the initial deadline).
    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: epoch_of_dropped_probe,
        },
    );
    assert!(!state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
    assert!(
        state.role_probe_undelivered,
        "the failed attempt must remember its probe was never delivered"
    );

    // The gate-opening edge resurrects the deadline-failed attempt as a
    // fresh one and re-sends the probe under a fresh deadline epoch.
    let after_open = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(3),
            },
        )),
    );
    assert!(
        after_open.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::RoleNegotiation,
                ..
            }
        )),
        "gate opening must re-send the dropped role-negotiation probe"
    );
    assert_eq!(state.signaling_generation, Some(3));
    assert!(state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.unavailable_reason, None);
    assert!(!state.role_probe_undelivered);
    assert_ne!(
        state.negotiation_epoch, epoch_of_dropped_probe,
        "the re-driven probe must arm a fresh deadline epoch"
    );

    // The deadline armed for the dropped attempt is retired; only the fresh
    // epoch's deadline fails the negotiation.
    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: epoch_of_dropped_probe,
        },
    );
    assert!(
        state.role_negotiation_inflight,
        "a deadline from the retired epoch must not fail the re-driven probe"
    );
    let fresh_epoch = state.negotiation_epoch;
    reduce(
        &mut state,
        RtcPeerInput::RoleNegotiationDeadlineElapsed {
            negotiation_epoch: fresh_epoch,
        },
    );
    assert!(!state.role_negotiation_inflight);
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::NegotiationFailed)
    );
}

// ── S5-2: Soft ICE restart ───────────────────────────────────────────────────

/// Helper: drive an offerer from Idle through connect → build → offer →
/// connected → sendable, returning the active generation. Reuses the existing
/// `drive_offerer_to_negotiating` + `drive_to_sendable` helpers.
fn drive_offerer_to_sendable(state: &mut RtcPeerState) -> u64 {
    let generation = drive_offerer_to_negotiating(state);
    drive_to_sendable(state, generation);
    generation
}

#[test]
fn transport_failure_starts_soft_recovery_with_restart_ice() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // Transport fails: the reducer should start Soft Recovery (ICE restart)
    // because the address has not changed.
    let effects = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(
        has_restart_ice(&effects, generation),
        "expected RestartIce effect for generation {generation}"
    );
}

#[test]
fn disconnected_starts_soft_recovery_when_sendable() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    let effects = reduce(&mut state, disconnected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(has_restart_ice(&effects, generation));
}

#[test]
fn soft_recovery_does_not_double_restart_on_repeated_failure() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // First failure starts Soft Recovery.
    let effects1 = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(has_restart_ice(&effects1, generation));

    // Second failure while already recovering: must NOT re-issue RestartIce
    // (the first restart is still in flight; idempotent).
    let effects2 = reduce(&mut state, failed_tick(generation));
    assert!(
        !has_restart_ice(&effects2, generation),
        "must not double-issue RestartIce"
    );
    assert_eq!(state.ice_generation, ice_before + 1); // unchanged
}

#[test]
fn business_recovery_hint_wakes_answerer_once_per_recovery_cycle() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    // The normal answerer recovery is request-only. A demand arriving after
    // the first request must still wake the peer, but concurrent demands in
    // the same logical cycle must share one request/epoch.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let start = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_request(&start, 1));
    let cycle = state.recovery_cycle;
    let transport_epoch = state.transport_epoch;
    assert_eq!(state.business_hint_cycle, None);

    let wake = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::BusinessRecoveryHint {
            transport_epoch,
            pc_generation: generation,
        }),
    );
    assert!(has_restart_request(&wake, 2));
    assert_eq!(state.business_hint_cycle, Some(cycle));
    assert_eq!(state.local_restart_request_attempt, 2);

    let duplicate = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::BusinessRecoveryHint {
            transport_epoch,
            pc_generation: generation,
        }),
    );
    assert!(duplicate.is_empty());
    assert_eq!(state.business_hint_cycle, Some(cycle));
    assert_eq!(state.local_restart_request_attempt, 2);
}

#[test]
fn business_recovery_hint_does_not_race_answerer_sdp() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let start = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_request(&start, 1));

    let offer = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some("business-hint-sdp-race".to_string()),
        }),
    );
    assert!(has_set_remote(&offer, generation));
    let set_remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    let remote_completed = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&remote_completed, generation));

    let request_attempt = state.local_restart_request_attempt;
    let transport_epoch = state.transport_epoch;
    let hint = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::BusinessRecoveryHint {
            transport_epoch,
            pc_generation: generation,
        }),
    );
    assert!(hint.is_empty());
    assert_eq!(
        state.local_restart_request_attempt, request_attempt,
        "a demand hint must not race an in-flight CreateAnswer"
    );
    assert_eq!(state.business_hint_cycle, None);
}

#[test]
fn business_attempt2_preserves_offerer_inflight_operations() {
    for stage in 0..3 {
        let mut answerer = RtcPeerState::new(test_actor_id(2));
        drive_answerer_to_sendable_with_ufrag(&mut answerer, "remote-1");
        let answerer_generation = answerer.active_generation.unwrap();
        reduce(&mut answerer, disconnected_tick(answerer_generation));
        let request_id = answerer.local_restart_request_id.clone().unwrap();
        assert_eq!(answerer.local_restart_request_attempt, 1);

        let mut offerer = RtcPeerState::new(test_actor_id(1));
        let generation = drive_offerer_to_sendable(&mut offerer);
        let restart = reduce(
            &mut offerer,
            RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
                request_id: request_id.clone(),
                attempt: 1,
            }),
        );
        let (operation_id, ice_generation) = restart_identity(&restart, generation);
        if stage > 0 {
            reduce(
                &mut offerer,
                RtcPeerInput::Completion(EffectCompletion::RestartIce {
                    generation,
                    operation_id,
                    ice_generation,
                    result: Ok(()),
                }),
            );
        }
        if stage > 1 {
            let operation_id =
                pending_operation(&offerer, PendingRtcOperationKind::CreateOffer, generation);
            reduce(
                &mut offerer,
                RtcPeerInput::Completion(EffectCompletion::CreateOffer {
                    generation,
                    operation_id,
                    result: Ok(Box::new(offer())),
                }),
            );
        }
        let wake_epoch = answerer.transport_epoch;
        let wake = reduce(
            &mut answerer,
            RtcPeerInput::Command(RtcPeerCommand::BusinessRecoveryHint {
                transport_epoch: wake_epoch,
                pc_generation: answerer_generation,
            }),
        );
        assert!(has_restart_request(&wake, 2));
        let kind = match stage {
            0 => PendingRtcOperationKind::RestartIce,
            1 => PendingRtcOperationKind::CreateOffer,
            _ => PendingRtcOperationKind::SetLocalDescription,
        };
        let operation_id = pending_operation(&offerer, kind, generation);
        let epoch = offerer.recovery_epoch;
        let effects = reduce(
            &mut offerer,
            RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
                request_id,
                attempt: answerer.local_restart_request_attempt,
            }),
        );
        assert!(
            effects.is_empty(),
            "stage {stage}: business wake must coalesce"
        );
        assert_eq!(offerer.recovery_epoch, epoch);
        assert_eq!(offerer.ice_generation, ice_generation);
        assert_eq!(pending_operation(&offerer, kind, generation), operation_id);
        let completion = match stage {
            0 => EffectCompletion::RestartIce {
                generation,
                operation_id,
                ice_generation,
                result: Ok(()),
            },
            1 => EffectCompletion::CreateOffer {
                generation,
                operation_id,
                result: Ok(Box::new(offer())),
            },
            _ => EffectCompletion::SetLocalDescription {
                generation,
                operation_id,
                result: Ok(()),
            },
        };
        let stale = offerer.stale_completions;
        let continued = reduce(&mut offerer, RtcPeerInput::Completion(completion));
        assert_eq!(offerer.stale_completions, stale);
        assert!(
            !continued.is_empty(),
            "the original negotiation must continue"
        );
    }
}

#[test]
fn local_address_changed_triggers_soft_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // A local address change starts an in-place ICE restart. The current
    // PeerConnection remains active and no replacement generation is built.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(
        !state.address_changed,
        "local address changes do not request Hard Recovery"
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(!has_build_session(&effects, generation + 1));
    assert!(has_restart_ice(&effects, generation));
    assert_eq!(
        state.active_generation,
        Some(generation),
        "Soft Recovery keeps the current session"
    );
    assert_eq!(state.building_generation, None);
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.ice_generation, ice_before + 1);
}

#[test]
fn local_address_changed_supersedes_build_holding_pre_change_snapshot() {
    let mut state = RtcPeerState::new(test_actor_id(1));

    // Initial connect: the role assignment starts a build, which resolves the
    // provider's address set at build time.
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    let effects = reduce(&mut state, role_assignment(true));
    assert!(has_build_session(&effects, 1));
    assert_eq!(state.building_generation, Some(1));
    assert_eq!(state.active_generation, None);

    // The local address set changes while the build is in flight: the build
    // is consuming the pre-change snapshot, and a same-generation ICE restart
    // cannot repair a Session that has not been born yet — supersede it with
    // a build that resolves the provider's current set.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(
        has_build_session(&effects, 2),
        "the superseding build resolves the provider's current address set"
    );
    assert!(
        !has_restart_ice(&effects, 1),
        "no soft restart: there is no live generation to restart"
    );
    assert_eq!(state.building_generation, Some(2));
    assert_eq!(state.active_generation, None);
}

#[test]
fn address_edge_coalesced_mid_soft_round_republishes_on_convergence() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // A temporary loss starts a soft round on the pre-edge address set.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(state.recovery_attempt_started);
    let ice_after_first = state.ice_generation;

    // An address edge mid-flight is remembered, not dropped: the in-flight
    // round keeps its exchange identity.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(
        !has_restart_ice(&effects, generation),
        "the in-flight round is not restarted"
    );
    assert!(state.address_edge_pending);

    // The round converges on the pre-edge set; convergence re-publishes one
    // restart so the link moves onto the current address set. (The anchor
    // lane stays open across a soft restart, so convergence arrives as the
    // reconnect tick.)
    state.recovery_negotiated = true;
    let effects = reduce(&mut state, connected_tick(generation));
    assert!(
        has_restart_ice(&effects, generation),
        "convergence must re-publish one soft restart"
    );
    assert_eq!(state.ice_generation, ice_after_first + 1);
    assert!(!state.address_edge_pending);
}

#[test]
fn address_edge_after_negotiation_restarts_immediately_before_attempt_deadline() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    let first_epoch = state.recovery_epoch;
    let first_ice_generation = state.ice_generation;

    // Model the mobile handover edge observed after the first Offer/Answer
    // completed but before the physical ICE path became ready.
    state.recovery_negotiated = true;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );

    assert!(
        has_restart_ice(&effects, generation),
        "a post-negotiation address edge must start the fresh rebind immediately"
    );
    assert!(state.recovery_epoch > first_epoch);
    assert!(state.ice_generation > first_ice_generation);
    assert!(state.recovery_attempt_started);
    assert!(!state.address_edge_pending);
}

#[test]
fn answerer_defers_address_edge_without_suppressing_request_retry() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let start = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_request(&start, 1));
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("answerer recovery request identity");
    let epoch = state.recovery_epoch;
    let ice_generation = state.ice_generation;

    // The current restart Offer/Answer has completed, but the physical path
    // is still unavailable. An address edge must not create another request
    // while the Offerer may still be executing this same ICE restart.
    state.phase = RtcPeerPhase::TransportReady;
    state.recovery_negotiated = true;
    state.recovery_attempt_started = true;
    state.rpc_reliable_open = false;
    state.pc_connected = false;
    state.awaiting_offer = false;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );

    assert!(effects.is_empty());
    assert_eq!(state.recovery_epoch, epoch);
    assert_eq!(state.ice_generation, ice_generation);
    assert!(state.recovery_attempt_started);
    assert!(state.address_edge_pending);

    // Deferring the edge does not remove the Answerer's wake-up path. The
    // existing attempt deadline still emits the next attempt with the same
    // request identity after the current exchange has had its full window.
    let retry = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch,
        },
    );
    assert!(retry.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartRequest {
                request_id: actual,
                attempt: 2,
            },
            generation: actual_generation,
        } if actual == &request_id && *actual_generation == generation
    )));
}

#[test]
fn restart_ice_completion_offerer_creates_restart_offer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Start Soft Recovery.
    let restart = reduce(&mut state, failed_tick(generation));
    let ice_gen = state.ice_generation;
    let (operation_id, ice_generation) = restart_identity(&restart, generation);

    // RestartIce completes successfully: offerer should create a restart offer.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id,
            ice_generation,
            result: Ok(()),
        }),
    );
    assert!(
        effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::CreateOffer { generation: g, .. } if *g == generation
        )),
        "offerer should create restart offer after RestartIce"
    );
    assert_eq!(state.ice_generation, ice_gen); // ICE gen unchanged
}

#[test]
fn answerer_asymmetric_failure_request_drives_offerer_replacement_offer() {
    // Asymmetric physical failure: the answerer's session dies while the
    // offerer stays sendable. The answerer cannot build a replacement (no
    // local Offer), so its restart request must make the STABLE offerer
    // hard-recover: a replacement generation whose completed build creates
    // the fresh Offer the passive answerer accepts. Soft recovery alone
    // would leave the answerer parked on a dead generation until Idle.
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let request = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "asymmetric-1".to_string(),
            attempt: 4,
        }),
    );
    assert!(
        has_build_session(&request, generation + 1),
        "the stable offerer must build a replacement session for the dead answerer"
    );
    assert!(
        !has_restart_ice(&request, generation),
        "no soft restart: the offerer's own generation is healthy"
    );
    // Make-before-break: the healthy offerer session stays alive while the
    // replacement negotiates.
    assert_eq!(state.active_generation, Some(generation));

    // The replacement build completes: the offerer creates the fresh Offer
    // the passive answerer needs.
    let operation_id = pending_operation(
        &state,
        PendingRtcOperationKind::BuildSession,
        generation + 1,
    );
    let offer_effects = reduce(&mut state, build_ok(generation + 1, operation_id));
    assert!(
        has_create_offer(&offer_effects, generation + 1),
        "the replacement build must produce a fresh Offer for the answerer"
    );
}

#[test]
fn offerer_deduplicates_restart_request_but_retries_newer_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // Ordinary attempts drive the soft path: a shared network blip is
    // repaired by an ICE restart on the live generation, which the
    // answerer's live session can accept.
    let first = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "request-1".to_string(),
            attempt: 1,
        }),
    );
    assert!(has_restart_ice(&first, generation));
    assert_eq!(state.ice_generation, ice_before + 1);

    let duplicate = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "request-1".to_string(),
            attempt: 1,
        }),
    );
    assert!(duplicate.is_empty());

    finish_restart_offer(&mut state, generation);
    let retry = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "request-1".to_string(),
            attempt: 2,
        }),
    );
    assert!(has_restart_ice(&retry, generation));
    assert_eq!(state.ice_generation, ice_before + 2);

    let duplicate_retry = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "request-1".to_string(),
            attempt: 2,
        }),
    );
    assert!(
        duplicate_retry.is_empty(),
        "the same retransmission attempt must remain deduplicated after recovery state is reset"
    );

    // Persistent retransmission proves the answerer's physical session is
    // dead: escalate to Hard Recovery so the passive answerer gets a fresh
    // Offer it can accept.
    finish_restart_offer(&mut state, generation);
    let escalation = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "request-1".to_string(),
            attempt: 4,
        }),
    );
    assert!(
        has_build_session(&escalation, generation + 1),
        "persistent retransmission must build a replacement session"
    );
}

#[test]
fn attempt1_restart_request_wakes_offerer_round_in_flight() {
    // Issue #227 (one-sided outage): only the ANSWERER side lost the network;
    // the offerer's own Soft Recovery is already in flight. The answerer's
    // first correlated wake (attempt=1) must restart the round immediately —
    // same reset-then-restart path later retransmissions use — instead of
    // being de-duped and stalling the pair for a full attempt-deadline
    // cadence (iOS on-device echo was 5326ms before the fix).
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // The offerer's own round is running (its PC also died, e.g. it detected
    // the dead ICE).
    state.recovery_inflight = true;
    state.recovery_attempt_started = true;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "wake-1".to_string(),
            attempt: 1,
        }),
    );

    assert!(
        has_restart_ice(&effects, generation),
        "attempt=1 must wake the offerer into an immediate RestartIce (fresh offer), not wait for the deadline"
    );
    assert_eq!(
        state.ice_generation,
        ice_before + 1,
        "the woken round rolls the ICE generation"
    );
    assert_eq!(
        state.last_remote_restart_request,
        Some(("wake-1".to_string(), 1)),
        "the wake is recorded as the latest remote request"
    );
}

#[test]
fn attempt1_restart_request_is_deduped_while_local_restart_is_inflight() {
    // A peer request can race the Offerer's own driver-side recovery. If the
    // local RestartIce/CreateOffer chain is still pending, the request is a
    // duplicate of that active negotiation and must not create a second offer
    // (the p08 remote-first recovery contract).
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    let local_recovery = reduce(&mut state, failed_tick(generation));
    assert!(has_restart_ice(&local_recovery, generation));
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::PeerFailure));

    let raced_wake = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "raced-wake".to_string(),
            attempt: 1,
        }),
    );

    assert!(
        raced_wake.is_empty(),
        "attempt=1 must not start a second offer while local restart negotiation is pending"
    );
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(state.recovery_started_by_remote_request);
}

#[test]
fn response_recovery_hint_starts_missing_soft_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;

    // Model a live session whose response arrived through the lane, but whose
    // controller has no recovery cycle despite being non-sendable.
    state.phase = RtcPeerPhase::TransportReady;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    state.recovery_inflight = false;
    state.recovery_attempt_started = false;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-1".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert!(has_restart_ice(&effects, generation));
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::PeerFailure));
}

#[test]
fn response_recovery_hint_is_noop_without_active_generation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let transport_epoch = state.transport_epoch;
    let stale_before = state.stale_events;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-no-generation".to_string(),
            transport_epoch,
            pc_generation: 0,
        }),
    );

    assert!(effects.is_empty());
    assert!(!state.recovery_inflight);
    assert_eq!(state.stale_events, stale_before);
}

#[test]
fn response_recovery_hint_starts_hard_recovery_after_address_change() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    state.phase = RtcPeerPhase::TransportReady;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    state.address_changed = true;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-hard".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert!(has_build_session(&effects, generation + 1));
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert_eq!(state.building_generation, Some(generation + 1));
}

#[test]
fn response_recovery_hint_defers_until_recovery_gate_reopens() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    state.phase = RtcPeerPhase::TransportReady;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    state.signaling_generation = None;

    let deferred = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-deferred".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );
    assert!(deferred.is_empty());
    assert!(state.recovery_inflight);
    assert!(!state.recovery_attempt_started);

    let resumed = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(0),
            },
        )),
    );
    assert!(has_restart_ice(&resumed, generation));
    assert!(state.recovery_attempt_started);
}

#[test]
fn response_recovery_hint_is_rejected_after_close_requested() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    state.close_requested = true;
    let stale_before = state.stale_events;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-closed".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);
    assert!(!state.recovery_inflight);
}

#[test]
fn response_recovery_hint_is_observational_when_sendable() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-sendable".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert!(effects.is_empty());
    assert!(state.sendable());
    assert_eq!(state.ice_generation, 0);
    assert_eq!(state.stale_events, 0);
}

#[test]
fn response_recovery_hint_does_not_restart_an_owned_recovery_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let first = reduce(&mut state, failed_tick(generation));
    assert!(has_restart_ice(&first, generation));
    let ice_generation = state.ice_generation;
    let transport_epoch = state.transport_epoch;

    let hint = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-2".to_string(),
            transport_epoch,
            pc_generation: generation,
        }),
    );

    assert!(hint.is_empty());
    assert_eq!(state.ice_generation, ice_generation);
    assert_eq!(state.stale_events, 0);
}

#[test]
fn stale_response_recovery_hint_is_ignored() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let transport_epoch = state.transport_epoch;
    // Keep the generation live but make the controller non-sendable. Without
    // this setup the sendable fast path would hide a removed/inverted identity
    // fence and the test could pass without proving the hint was rejected as
    // stale.
    state.phase = RtcPeerPhase::TransportReady;
    state.pc_connected = false;
    state.rpc_reliable_open = false;
    let stale_before = state.stale_events;
    let stale = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-stale".to_string(),
            transport_epoch: transport_epoch.saturating_sub(1),
            pc_generation: generation,
        }),
    );

    assert!(stale.is_empty());
    assert!(!state.recovery_inflight);
    assert_eq!(state.ice_generation, 0);
    assert_eq!(state.stale_events, stale_before + 1);

    let stale_generation = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RecoveryHint {
            request_id: "response-hint-stale-generation".to_string(),
            transport_epoch,
            pc_generation: generation.saturating_sub(1),
        }),
    );
    assert!(stale_generation.is_empty());
    assert_eq!(state.ice_generation, 0);
    assert_eq!(state.stale_events, stale_before + 2);
}

#[test]
fn attempt1_restart_request_graces_local_network_announced_offer_then_retries() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Build the announced Offer through the production reducer path so this
    // test exercises the same RestartIce -> CreateOffer -> SetLocalDescription
    // state that the controller uses before the request wake arrives.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let restart = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::LocalNetwork));
    let (restart_operation_id, restart_ice_generation) = restart_identity(&restart, generation);
    let create_offer = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id: restart_operation_id,
            ice_generation: restart_ice_generation,
            result: Ok(()),
        }),
    );
    assert!(has_create_offer(&create_offer, generation));
    let create_offer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateOffer, generation);
    let set_local = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(RTCSdpType::Offer, "restart-offer"))),
        }),
    );
    assert!(has_set_local(&set_local, generation));
    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    let announced = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Ok(()),
        }),
    );
    assert!(announced.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartOffer { .. },
            ..
        }
    )));
    assert!(state.pending_local_sdp_exchange_id.is_some());
    let epoch_before = state.recovery_epoch;

    let wake = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "grace-1".to_string(),
            attempt: 1,
        }),
    );

    assert!(wake.is_empty());
    assert!(state.remote_request_wake_pending);
    assert!(state.recovery_started_by_remote_request);
    assert!(state.recovery_epoch > epoch_before);

    let retry_epoch = state.recovery_epoch;
    let retry = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: retry_epoch,
        },
    );

    assert!(has_restart_ice(&retry, generation));
    assert!(!state.remote_request_wake_pending);
}

#[test]
fn attempt1_restart_request_accepts_answer_during_grace_window() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let restart = reduce(&mut state, failed_tick(generation));
    let (restart_operation_id, restart_ice_generation) = restart_identity(&restart, generation);
    let create_offer = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id: restart_operation_id,
            ice_generation: restart_ice_generation,
            result: Ok(()),
        }),
    );
    assert!(has_create_offer(&create_offer, generation));
    let create_offer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateOffer, generation);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(RTCSdpType::Offer, "grace-offer"))),
        }),
    );
    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    let announced = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Ok(()),
        }),
    );
    assert!(announced.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartOffer { .. },
            ..
        }
    )));
    let exchange_id = state
        .pending_local_sdp_exchange_id
        .clone()
        .expect("announced restart offer exchange identity");

    let wake = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "grace-answer".to_string(),
            attempt: 1,
        }),
    );
    assert!(wake.is_empty());
    assert!(state.remote_request_wake_pending);
    let grace_epoch = state.recovery_epoch;

    // The matching Answer arrives before the short grace expires. It must be
    // applied to the current restart exchange and cancel the grace timer rather
    // than launching a second RestartIce.
    let answer_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(answer()),
            exchange_id: Some(exchange_id),
        }),
    );
    assert!(has_set_remote(&answer_effects, generation));
    let set_remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    assert!(!state.remote_request_wake_pending);
    assert!(!state.recovery_negotiated);
    assert!(state.recovery_epoch > grace_epoch);
    // The original grace fires while SDP application is still running.
    let stale_grace = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: grace_epoch,
        },
    );
    assert!(stale_grace.is_empty());
    assert_eq!(
        pending_operation(
            &state,
            PendingRtcOperationKind::SetRemoteDescription,
            generation
        ),
        set_remote_operation
    );
    let completed = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(completed.is_empty());
    assert!(!state.remote_request_wake_pending);
    assert!(state.recovery_negotiated);
    assert!(state.recovery_epoch > grace_epoch);

    let stale_grace = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: grace_epoch,
        },
    );
    assert!(stale_grace.is_empty());
    reduce(&mut state, connected_tick(generation));
    assert!(state.sendable());
}

#[test]
fn legacy_answer_cannot_cross_a_restart_retry_boundary() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    // Model the already-established Answerer's previous ICE credentials. The
    // next legacy Answer must prove a changed remote ufrag after the retry.
    state.set_remote_ufrag("remote-before-retry".to_string());

    // Announce the first restart Offer through the real reducer/effect chain.
    let restart = reduce(&mut state, failed_tick(generation));
    let (restart_operation_id, restart_ice_generation) = restart_identity(&restart, generation);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id: restart_operation_id,
            ice_generation: restart_ice_generation,
            result: Ok(()),
        }),
    );
    let create_offer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateOffer, generation);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(
                RTCSdpType::Offer,
                "restart-offer-1",
            ))),
        }),
    );
    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    let first_offer = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Ok(()),
        }),
    );
    assert!(first_offer.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartOffer { .. },
            ..
        }
    )));

    // The Answerer wake arrives before the first Answer. Let the grace window
    // expire so a second local restart Offer supersedes the first exchange.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "legacy-fence".to_string(),
            attempt: 1,
        }),
    );
    let grace_epoch = state.recovery_epoch;
    let second_restart = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: grace_epoch,
        },
    );
    assert!(has_restart_ice(&second_restart, generation));
    assert!(state.legacy_answer_fence.is_some());

    // Finish the second local restart far enough to publish its new Offer.
    // The old Answer is deliberately delivered after this boundary.
    let (second_restart_operation, second_ice_generation) =
        restart_identity(&second_restart, generation);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id: second_restart_operation,
            ice_generation: second_ice_generation,
            result: Ok(()),
        }),
    );
    let second_create_offer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateOffer, generation);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id: second_create_offer_operation,
            result: Ok(Box::new(sdp_with_ufrag(
                RTCSdpType::Offer,
                "restart-offer-2",
            ))),
        }),
    );
    let second_set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    let second_offer = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: second_set_local_operation,
            result: Ok(()),
        }),
    );
    assert!(second_offer.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartOffer { .. },
            ..
        }
    )));
    assert!(state.pending_local_sdp_exchange_id.is_some());

    // The old legacy Answer has the previous remote ufrag and no exchange id;
    // it must be rejected even though the new Offer is already pending.
    let stale_before = state.stale_events;
    let stale_answer = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Answer, "remote-before-retry")),
            exchange_id: None,
        }),
    );
    assert!(stale_answer.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);

    // A changed remote ICE ufrag still cannot prove which restart produced an
    // uncorrelated Answer: a delayed Answer from an older restart can carry a
    // different ufrag after a later restart has superseded it.
    let stale_after_ufrag_change = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Answer, "remote-after-retry")),
            exchange_id: None,
        }),
    );
    assert!(stale_after_ufrag_change.is_empty());
    assert_eq!(state.stale_events, stale_before + 2);

    // The current restart remains usable through the protocol-level exchange
    // id, which is the only unambiguous correlation signal after the fence.
    let current_exchange_id = state
        .pending_local_sdp_exchange_id
        .clone()
        .expect("the current restart must retain its exchange id");
    let current_answer = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescriptionCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Answer, "remote-after-retry")),
            exchange_id: Some(current_exchange_id),
        }),
    );
    assert!(has_set_remote(&current_answer, generation));
    let remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: remote_operation,
            result: Ok(()),
        }),
    );
    assert!(state.legacy_answer_fence.is_none());
}

#[test]
fn remote_request_recovery_coalesces_late_local_address_edge() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // A request-driven recovery is already negotiated, but its physical
    // readiness has not converged. A late local address fact must not publish
    // a second Offer for the same request-driven round.
    state.recovery_inflight = true;
    state.recovery_attempt_started = true;
    state.recovery_negotiated = true;
    state.recovery_started_by_remote_request = true;
    state.phase = RtcPeerPhase::Recovering;
    state.pc_connected = false;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );

    assert!(effects.is_empty());
    assert!(state.address_edge_pending);
    assert_eq!(state.active_generation, Some(generation));
}

#[test]
fn attempt2_restart_request_wakes_offerer_into_fresh_round() {
    // A later attempt can retry a round with no RTC operation in flight.
    // It resets the started
    // flag and drives a fresh Soft Recovery offer immediately, instead of
    // waiting for the per-attempt deadline.
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    state.recovery_inflight = true;
    state.recovery_attempt_started = true;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "wake-1".to_string(),
            attempt: 2,
        }),
    );

    assert!(
        has_restart_ice(&effects, generation),
        "attempt=2 must wake the offerer into an immediate RestartIce (fresh offer), not wait for the deadline"
    );
    assert_eq!(
        state.ice_generation,
        ice_before + 1,
        "the woken round rolls the ICE generation"
    );
    assert_eq!(
        state.last_remote_restart_request,
        Some(("wake-1".to_string(), 2)),
        "the newer attempt is recorded as the latest remote request"
    );
}

#[test]
fn ineligible_restart_request_is_not_consumed_before_session_is_ready() {
    let mut state = RtcPeerState::new(test_actor_id(1));

    assert!(
        reduce(
            &mut state,
            RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
                request_id: "early-request".to_string(),
                attempt: 1,
            }),
        )
        .is_empty()
    );
    assert!(state.last_remote_restart_request.is_none());

    let generation = drive_offerer_to_sendable(&mut state);
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "early-request".to_string(),
            attempt: 1,
        }),
    );
    assert!(has_restart_ice(&effects, generation));
    assert_eq!(
        state.last_remote_restart_request.as_ref(),
        Some(&("early-request".to_string(), 1))
    );
}

#[test]
fn offerer_does_not_double_restart_when_first_answerer_wake_races_network_available() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let first = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_ice(&first, generation));
    assert_eq!(state.recovery_origin, Some(RecoveryOrigin::LocalNetwork));
    let ice_generation = state.ice_generation;

    let raced_wake = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "raced-request".to_string(),
            attempt: 1,
        }),
    );
    assert!(raced_wake.is_empty());
    assert_eq!(state.ice_generation, ice_generation);

    finish_restart_offer(&mut state, generation);
    let retry = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
            request_id: "raced-request".to_string(),
            attempt: 2,
        }),
    );
    assert!(has_restart_ice(&retry, generation));
    assert_eq!(state.ice_generation, ice_generation + 1);
}

#[test]
fn restart_ice_failure_escalates_to_hard_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let restart = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    let (operation_id, ice_generation) = restart_identity(&restart, generation);

    // S5-3: Soft Recovery (RestartIce) failure → escalate to Hard Recovery.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id,
            ice_generation,
            result: Err(webrtc::error::Error::Other("restart failed".into())),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(
        !has_close_session(&effects, generation),
        "old NOT closed yet (make-before-break)"
    );
    assert!(
        has_build_session(&effects, generation + 1),
        "new gen building"
    );
    assert_eq!(
        state.active_generation,
        Some(generation),
        "old session kept alive"
    );
    assert_eq!(state.building_generation, Some(generation + 1));
}

#[test]
fn network_available_starts_soft_recovery_when_recovering() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let ice_before = state.ice_generation;

    // Network goes unavailable → enter Recovering (no restart yet).
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.ice_generation, ice_before); // no restart yet

    // Network available → start Soft Recovery.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(has_restart_ice(&effects, generation));
}

#[test]
fn exact_soft_recovery_deadline_retries_same_pc_without_duplicate_fact_extension() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let epoch = state.recovery_epoch;
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(
        state.recovery_epoch, epoch,
        "duplicate fact cannot extend deadline"
    );

    let stale = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch.wrapping_sub(1),
        },
    );
    assert!(stale.is_empty());
    assert_eq!(state.building_generation, None);

    let ice_generation = state.ice_generation;
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch,
        },
    );
    assert!(has_restart_ice(&effects, generation));
    assert!(!has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.building_generation, None);
    assert_eq!(state.ice_generation, ice_generation + 1);
    assert!(
        state.recovery_epoch > epoch,
        "the next soft attempt owns a new epoch"
    );
}

#[test]
fn offerer_soft_retry_ladder_escalates_to_hard_after_threshold() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // A temporary loss starts Soft Recovery on the live generation.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.soft_recovery_attempts, 0);

    // Below the threshold every deadline-driven retry stays on the soft
    // ladder: same PC, no replacement build.
    for attempt in 1..SOFT_HARD_ESCALATION_ATTEMPT {
        let recovery_epoch = state.recovery_epoch;
        let effects = reduce(
            &mut state,
            RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
        );
        assert!(
            has_restart_ice(&effects, generation),
            "attempt {attempt} stays on the soft ladder"
        );
        assert!(!has_build_session(&effects, generation + 1));
        assert_eq!(state.soft_recovery_attempts, attempt);
        assert_eq!(state.active_generation, Some(generation));
    }

    // The threshold-th failure to converge proves the restart is not
    // repairing the transport; escalate to a checked Hard rebuild.
    let recovery_epoch = state.recovery_epoch;
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
    );
    assert!(
        !has_restart_ice(&effects, generation),
        "the threshold-th failure leaves the soft ladder"
    );
    assert!(
        has_build_session(&effects, generation + 1),
        "persistent soft failure escalates to a checked Hard rebuild"
    );
    assert_eq!(state.building_generation, Some(generation + 1));
    assert_eq!(
        state.active_generation,
        Some(generation),
        "make-before-break keeps the old session alive"
    );
    assert_eq!(state.soft_recovery_attempts, 0);
}

#[test]
fn transport_ready_soft_recovery_deadline_retries_same_pc() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    let epoch = state.recovery_epoch;
    let ice_generation = state.ice_generation;

    // A restart answer may be applied just before an old positive transport
    // callback advances the soft round to TransportReady. This is still the
    // active PC, not a pending Hard replacement.
    state.phase = RtcPeerPhase::TransportReady;
    state.recovery_negotiated = true;
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch,
        },
    );

    assert!(has_restart_ice(&effects, generation));
    assert!(!has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.ice_generation, ice_generation + 1);
}

#[test]
fn answerer_restart_request_retries_until_the_recovery_budget_without_rebuilding() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let request = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(!has_restart_ice(&request, generation));
    assert!(has_restart_request(&request, 1));

    // The request attempt count is not a separate failure budget. Each
    // deadline retransmits the same correlated request while the one logical
    // recovery cycle remains in flight; only RecoveryBudgetElapsed terminates
    // the cycle.
    let request_id = state.local_restart_request_id.clone();
    let recovery_cycle = state.recovery_cycle;
    for expected_attempt in 2..=8 {
        let epoch = state.recovery_epoch;
        let effects = reduce(
            &mut state,
            RtcPeerInput::RecoveryDeadlineElapsed {
                recovery_epoch: epoch,
            },
        );
        assert!(has_restart_request(&effects, expected_attempt));
        assert!(!has_build_session(&effects, generation + 1));
        assert_eq!(state.active_generation, Some(generation));
        assert_eq!(state.building_generation, None);
        assert_eq!(state.pending_generation, None);
        assert_eq!(state.recovery_cycle, recovery_cycle);
        assert_eq!(state.local_restart_request_id, request_id);
        assert_eq!(
            state.local_restart_request_attempt, expected_attempt,
            "every six-second deadline advances the request attempt"
        );
    }

    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed { recovery_cycle },
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession { generation: actual, .. } if *actual == generation
    )));
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::RecoveryExhausted)
    );
    assert_eq!(state.active_generation, None);
}

#[test]
fn transport_reconnect_after_soft_recovery_clears_address_changed() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Soft Recovery from a transport failure.
    let _ = reduce(&mut state, failed_tick(generation));
    assert!(state.recovery_inflight);

    // A stale Connected callback from the old ICE epoch must not restore
    // sendability before the restart offer/answer exchange completes.
    reduce(&mut state, connected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(!state.pc_connected);

    // The offer/answer completion is covered by the signaling/effect tests;
    // once that exact recovery epoch is negotiated, a new Connected callback
    // plus the still-open anchor restores Sendable.
    state.recovery_negotiated = true;
    reduce(&mut state, connected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(!state.recovery_inflight);
}

/// Check whether the effects include a `RestartIce` for `generation`.
fn has_restart_ice(effects: &[RtcPeerEffect], generation: u64) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            RtcPeerEffect::RestartIce { generation: g, .. } if *g == generation
        )
    })
}

/// A state tick reporting `Disconnected` on both PC and ICE for `generation`.
fn disconnected_tick(generation: u64) -> RtcPeerInput {
    RtcPeerInput::StateTick(Box::new(PeerStateTick {
        generation,
        seq: 1,
        pc_state: Some(RTCPeerConnectionState::Disconnected),
        ice_state: Some(RTCIceConnectionState::Disconnected),
        ..Default::default()
    }))
}

// ── S5-3: Hard PeerConnection rebuild ───────────────────────────────────────

/// Drive a Hard Recovery to completion (build the replacement session) and
/// return the new generation. The new session enters Negotiating with a fresh
/// CreateOffer (offerer role). With make-before-break, the new session is
/// pending (not active) while the old is still alive.
fn drive_hard_recovery_to_negotiating(state: &mut RtcPeerState, old_gen: u64) -> u64 {
    let new_gen = old_gen + 1;
    // The BuildSession completes for the new generation.
    let operation_id = pending_operation(state, PendingRtcOperationKind::BuildSession, new_gen);
    let effects = reduce(state, build_ok(new_gen, operation_id));
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
    assert_eq!(state.pending_generation, Some(new_gen));
    assert!(has_create_offer(&effects, new_gen));
    new_gen
}

#[test]
fn force_reconnect_triggers_hard_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(
        !has_close_session(&effects, generation),
        "old NOT closed yet (make-before-break)"
    );
    assert!(has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.building_generation, Some(generation + 1));
}

#[test]
fn old_session_closing_during_replacement_build_keeps_replacement() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    let replacement = old_generation + 1;
    assert_eq!(state.building_generation, Some(replacement));

    let effects = reduce(&mut state, closed_tick(old_generation));

    assert_eq!(state.active_generation, None);
    assert_eq!(state.building_generation, Some(replacement));
    assert!(has_close_session(&effects, old_generation));
    assert!(
        !has_build_session(&effects, replacement + 1),
        "the already-building replacement must not be abandoned"
    );
}

#[test]
fn app_background_enters_recovering_from_sendable() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let _ = drive_offerer_to_sendable(&mut state);
    assert!(state.sendable());

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppBackground,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(!state.sendable(), "sendable must expire on background");
}

#[test]
fn app_background_is_noop_when_not_sendable() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    // Idle peer: AppBackground should not change phase.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppBackground,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.recovery_inflight);
}

#[test]
fn app_foreground_recovers_from_background_state() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // App goes background → Recovering.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppBackground,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);

    // App returns to foreground → Soft Recovery (address unchanged).
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppForeground {
                background_duration_ms: 5_000,
            },
        )),
    );
    assert!(
        has_restart_ice(&effects, generation),
        "foreground triggers Soft Recovery"
    );
    assert_eq!(state.ice_generation, 1);
}

#[test]
fn app_foreground_uses_the_same_long_background_boundary_as_lifecycle_policy() {
    let threshold = crate::lifecycle::LONG_BACKGROUND_RECONNECT_THRESHOLD_MS;
    for (duration, expect_hard) in [(threshold - 1, false), (threshold, true)] {
        let mut state = RtcPeerState::new(test_actor_id(1));
        let generation = drive_offerer_to_sendable(&mut state);
        reduce(
            &mut state,
            RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
                crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppBackground,
            )),
        );

        let effects = reduce(
            &mut state,
            RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
                crate::wire::webrtc::v020::recovery::RtcNetworkEvent::AppForeground {
                    background_duration_ms: duration,
                },
            )),
        );
        if expect_hard {
            assert!(has_build_session(&effects, generation + 1));
            assert!(!has_restart_ice(&effects, generation));
        } else {
            assert!(has_restart_ice(&effects, generation));
            assert!(!has_build_session(&effects, generation + 1));
        }
    }
}

// --- S5-6: TURN config refresh ---

/// A `RefreshConfiguration` command on a sendable peer emits the effect for
/// the active generation.
#[test]
fn refresh_configuration_emits_effect_for_active_generation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let config = Box::new(RTCConfiguration::default());
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config,
            respond: None,
        }),
    );
    assert!(
        effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::RefreshConfiguration { generation: g, .. } if *g == generation
        )),
        "expected RefreshConfiguration effect for generation {generation}"
    );
    // The command does not change phase or recovery state.
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(!state.recovery_inflight);
}

/// `RefreshConfiguration` on an idle peer (no active session) is a no-op.
#[test]
fn refresh_configuration_noop_without_active_session() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let config = Box::new(RTCConfiguration::default());
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config,
            respond: None,
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Idle);
}

/// Successful `RefreshConfiguration` completion does not change state.
#[test]
fn refresh_configuration_success_is_noop() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let issued = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config: Box::new(RTCConfiguration::default()),
            respond: None,
        }),
    );
    let operation_id = issued
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::RefreshConfiguration { operation_id, .. } => Some(*operation_id),
            _ => None,
        })
        .expect("refresh effect");
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RefreshConfiguration {
            generation,
            operation_id,
            result: Ok(()),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(!state.recovery_inflight);
}

/// Rejected `RefreshConfiguration` completion escalates to Hard Recovery.
#[test]
fn refresh_configuration_failure_escalates_to_hard_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let issued = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config: Box::new(RTCConfiguration::default()),
            respond: None,
        }),
    );
    let operation_id = issued
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::RefreshConfiguration { operation_id, .. } => Some(*operation_id),
            _ => None,
        })
        .expect("refresh effect");
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RefreshConfiguration {
            generation,
            operation_id,
            result: Err(webrtc::error::Error::Other("bad config".into())),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(state.recovery_inflight);
    assert!(
        has_build_session(&effects, generation + 1),
        "expected BuildSession for next generation"
    );
}

/// A stale `RefreshConfiguration` completion (wrong generation) is dropped.
#[test]
fn refresh_configuration_stale_completion_dropped() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let issued = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config: Box::new(RTCConfiguration::default()),
            respond: None,
        }),
    );
    let operation_id = issued
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::RefreshConfiguration { operation_id, .. } => Some(*operation_id),
            _ => None,
        })
        .expect("refresh effect");
    let stale_before = state.stale_completions;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RefreshConfiguration {
            generation: generation + 100, // not a live generation
            operation_id,
            result: Ok(()),
        }),
    );
    assert!(effects.is_empty());
    assert!(state.stale_completions > stale_before);
}

/// `RefreshConfiguration` is not emitted when close is requested.
#[test]
fn refresh_configuration_suppressed_during_close() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let _generation = drive_offerer_to_sendable(&mut state);
    state.close_requested = true;

    let config = Box::new(RTCConfiguration::default());
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::RefreshConfiguration {
            config,
            respond: None,
        }),
    );
    assert!(effects.is_empty());
}

#[test]
fn total_recovery_budget_survives_same_pc_soft_retry() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let active = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let cycle = state.recovery_cycle;
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    let attempt_epoch = state.recovery_epoch;

    // The per-attempt deadline starts another Soft attempt on the same PC and
    // changes the attempt epoch, but must never extend/change the total cycle
    // identity.
    let retry = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: attempt_epoch,
        },
    );
    assert!(retry.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::RestartIce { generation, .. } if *generation == active
    )));
    assert!(!retry.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::BuildSession { generation, .. } if *generation > active
    )));
    assert_eq!(state.active_generation, Some(active));
    assert_eq!(state.recovery_cycle, cycle);

    let exhausted = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: cycle,
        },
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(!state.recovery_inflight);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::RecoveryExhausted)
    );
    assert!(exhausted.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::CloseSession { generation, .. } if *generation == active
    )));
}

#[test]
fn offerer_role_assignment_cancels_soft_recovery_and_starts_fresh_attempt() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Start a same-PC Soft Recovery and retain the exact RestartIce identity
    // so the test can prove that the role assignment retires its operation.
    let restart = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    let (restart_operation_id, ice_generation) = restart_identity(&restart, generation);
    let old_transport_epoch = state.transport_epoch;
    let old_recovery_cycle = state.recovery_cycle;

    // The Offerer assignment is authoritative: abandon the recovery attempt,
    // close the old generation, and start a fresh initial Offerer attempt.
    let effects = reduce(&mut state, role_assignment(true));
    let replacement = generation + 1;
    assert_eq!(state.role, Role::Offerer);
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(!state.recovery_inflight);
    assert!(state.initial_connection_inflight);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.building_generation, Some(replacement));
    assert_eq!(state.transport_epoch, old_transport_epoch + 1);
    assert_eq!(state.recovery_cycle, old_recovery_cycle);
    assert!(has_close_session(&effects, generation));
    assert!(has_build_session(&effects, replacement));

    // The old recovery budget and its RestartIce completion are stale after
    // the fresh transport attempt takes ownership.
    let stale_budget = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: old_recovery_cycle,
        },
    );
    assert!(stale_budget.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Building);
    let stale_before = state.stale_completions;
    let stale_restart = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id: restart_operation_id,
            ice_generation,
            result: Ok(()),
        }),
    );
    assert!(stale_restart.is_empty());
    assert_eq!(state.stale_completions, stale_before + 1);
    assert_eq!(state.building_generation, Some(replacement));
}

#[test]
fn offerer_role_assignment_retires_pending_hard_recovery_replacement() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);

    // Hard Recovery leaves the old generation active while the replacement is
    // built. Complete the build so both active and pending generations must be
    // retired by the RoleAssignment path.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let pending_generation = old_generation + 1;
    assert!(state.recovery_inflight);
    assert!(has_build_session(&effects, pending_generation));
    let build_operation_id = pending_operation(
        &state,
        PendingRtcOperationKind::BuildSession,
        pending_generation,
    );
    reduce(&mut state, build_ok(pending_generation, build_operation_id));
    assert_eq!(state.active_generation, Some(old_generation));
    assert_eq!(state.pending_generation, Some(pending_generation));

    let effects = reduce(&mut state, role_assignment(true));
    let replacement = pending_generation + 1;
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(!state.recovery_inflight);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.building_generation, Some(replacement));
    assert!(has_close_session(&effects, old_generation));
    assert!(has_close_session(&effects, pending_generation));
    assert!(has_build_session(&effects, replacement));
}

#[test]
fn answerer_role_assignment_does_not_cancel_inflight_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    reduce(&mut state, failed_tick(generation));
    let recovery_epoch = state.recovery_epoch;
    assert_eq!(state.role, Role::Answerer);
    assert!(state.recovery_inflight);

    // The old Coordinator only replaced an existing session for an Offerer
    // assignment. An Answerer continues request-only recovery and waits for a
    // fresh remote Offer.
    let effects = reduce(&mut state, role_assignment(false));
    assert!(effects.is_empty());
    assert_eq!(state.role, Role::Answerer);
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert_eq!(state.recovery_epoch, recovery_epoch);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));
}

#[test]
fn recovery_exhaustion_closes_late_successful_build_as_orphan() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let active = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let building = active + 1;
    let operation_id = pending_operation(&state, PendingRtcOperationKind::BuildSession, building);
    let cycle = state.recovery_cycle;

    reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: cycle,
        },
    );
    let orphan_close = reduce(&mut state, build_ok(building, operation_id));

    assert!(state.terminally_unavailable());
    assert!(has_close_session(&orphan_close, building));
    assert_ne!(state.active_generation, Some(building));
}

#[test]
fn explicit_close_after_terminal_waits_for_abandoned_build() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let active = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let building = active + 1;
    let operation_id = pending_operation(&state, PendingRtcOperationKind::BuildSession, building);
    let cycle = state.recovery_cycle;
    reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: cycle,
        },
    );

    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    assert!(state.close_requested);
    assert!(!state.terminal_close_done);

    let orphan_close = reduce(&mut state, build_ok(building, operation_id));
    assert!(has_close_session(&orphan_close, building));
    assert!(!state.terminal_close_done);
}

#[test]
fn stale_total_recovery_budget_cannot_exhaust_newer_cycle() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let cycle = state.recovery_cycle;
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: cycle.wrapping_sub(1),
        },
    );
    assert!(effects.is_empty());
    assert!(state.recovery_inflight);
    assert_ne!(state.phase, RtcPeerPhase::Idle);
}

#[test]
fn address_change_while_offline_waits_for_network_before_soft_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let deferred = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(!has_build_session(&deferred, generation + 1));

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.active_generation, Some(generation));
    assert!(has_restart_ice(&effects, generation));
    assert!(!has_build_session(&effects, generation + 1));
}

#[test]
fn address_change_during_soft_recovery_is_coalesced() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Soft Recovery starts first. A later address snapshot fact is coalesced
    // into the same in-place recovery attempt; it must not allocate a second
    // PeerConnection generation.
    let _ = reduce(&mut state, disconnected_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    // Soft Recovery started (no address change yet).
    assert!(state.ice_generation > 0);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(effects.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.building_generation, None);
    assert_eq!(state.pending_generation, None);
}

#[test]
fn network_available_with_address_change_waits_for_signaling() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // NetworkUnavailable → Recovering (active still Some, no restart yet).
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::SignalingGenerationChanged {
                generation: None,
            },
        )),
    );

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    let network_only = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(!has_build_session(&network_only, generation + 1));

    let signaling_ready = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::SignalingGenerationChanged {
                generation: Some(7),
            },
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(state.active_generation, Some(generation));
    assert!(has_restart_ice(&signaling_ready, generation));
    assert!(!has_build_session(&signaling_ready, generation + 1));
}

#[test]
fn hard_recovery_new_generation_negotiates_as_offerer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);
    assert_eq!(state.role, Role::Offerer);

    // Trigger explicit Hard Recovery via ForceReconnect.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);

    // The new generation builds and enters Negotiating with a fresh offer.
    let new_gen = drive_hard_recovery_to_negotiating(&mut state, old_gen);
    assert_eq!(new_gen, old_gen + 1);
    assert_eq!(
        state.role,
        Role::Offerer,
        "role preserved across Hard Recovery"
    );

    // The old generation is still active (make-before-break) and will be
    // closed only after the atomic switch.
    assert!(!state.sessions_to_close.contains(&old_gen));
    assert_eq!(state.active_generation, Some(old_gen));
    assert_eq!(state.pending_generation, Some(new_gen));
}

#[test]
fn answerer_waits_for_offerer_offer_instead_of_hard_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let old_gen = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, old_gen);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert!(!has_build_session(&effects, old_gen + 1));
    assert!(!has_role_negotiation(&effects));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(!state.recovery_attempt_started);
    assert!(state.awaiting_offer);
    assert!(!state.address_changed);
    assert_eq!(state.active_generation, Some(old_gen));
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.building_generation, None);
    let recovery_epoch = state.recovery_epoch;
    let recovery_cycle = state.recovery_cycle;
    let old_transport_epoch = state.transport_epoch;

    // An attempt deadline must not convert the passive Answerer wait into a
    // locally-created replacement generation — but it is no longer silent:
    // the parked wait re-arms its epoch and RETRANSMITS the correlated
    // restart request so a healthy offerer's attempt counter reaches its
    // hard-recovery escalation (a silent park would strand both sides).
    // Parked via a hard-recovery path, no restart request has been sent
    // yet: the first parked deadline sends attempt 1 (initial wake).
    let attempt_before = state.local_restart_request_attempt;
    assert_eq!(attempt_before, 0);
    let deadline_effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
    );
    assert!(
        deadline_effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { .. },
                ..
            }
        )),
        "the parked deadline must retransmit the restart request"
    );
    assert!(
        !has_build_session(&deadline_effects, old_gen + 1),
        "no locally-created replacement generation"
    );
    assert_eq!(state.recovery_cycle, recovery_cycle);
    assert_eq!(state.active_generation, Some(old_gen));
    assert_eq!(state.building_generation, None);
    assert_eq!(
        state.local_restart_request_attempt,
        attempt_before + 1,
        "each parked deadline drives the retransmission counter"
    );

    // The Offerer now initiates a fresh ordinary Offer. The Answerer accepts
    // it by superseding the old physical session and building exactly one new
    // session for the incoming Offer.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "replacement"),
        ))),
    );

    assert!(has_close_session(&effects, old_gen));
    assert!(has_build_session(&effects, old_gen + 1));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.building_generation, Some(old_gen + 1));
    assert!(!state.recovery_inflight);
    assert_eq!(state.transport_epoch, old_transport_epoch + 1);
}

#[test]
fn answerer_local_address_change_starts_soft_recovery() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    let ice_before = state.ice_generation;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );

    assert!(!has_build_session(&effects, generation + 1));
    assert!(!has_restart_ice(&effects, generation));
    assert!(has_restart_request(&effects, 1));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.building_generation, None);
    assert_eq!(state.pending_generation, None);
    assert_eq!(state.ice_generation, ice_before + 1);
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert!(!state.awaiting_offer);
}

#[test]
fn answerer_physical_failure_retires_before_parking() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(&mut state, failed_tick(generation));

    assert!(has_close_session(&effects, generation));
    assert!(!has_restart_ice(&effects, generation));
    assert!(has_restart_request(&effects, 1));
    assert!(!has_build_session(&effects, generation + 1));
    assert!(!has_role_negotiation(&effects));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert!(state.awaiting_offer);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));
    assert_eq!(state.local_restart_request_attempt, 1);
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("failed Answerer must create a restart request identity");

    // The retired generation cannot process a duplicate terminal callback;
    // the first wake remains the only immediate request for this parked round.
    let duplicate_effects = reduce(&mut state, failed_tick(generation));
    assert!(
        !duplicate_effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { .. },
                ..
            }
        )),
        "duplicate Failed callbacks must not emit a second immediate request"
    );
    assert_eq!(
        state.local_restart_request_id.as_deref(),
        Some(request_id.as_str())
    );
    assert_eq!(state.local_restart_request_attempt, 1);
    // A per-attempt deadline does not create a local replacement (no build),
    // but retransmits the correlated restart request so a healthy Offerer
    // wakes toward hard recovery. The physical close was already scheduled by
    // the Failed fact; the single recovery budget still performs terminal
    // logical cleanup if no Offer arrives.
    let recovery_epoch = state.recovery_epoch;
    let deadline_effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
    );
    assert!(
        deadline_effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest {
                    request_id: actual,
                    attempt: 2,
                },
                ..
            } if actual == &request_id
        )),
        "the parked deadline must drive the offerer wake"
    );
    assert!(
        !deadline_effects
            .iter()
            .any(|e| matches!(e, RtcPeerEffect::BuildSession { .. }))
    );

    let recovery_cycle = state.recovery_cycle;
    let budget_effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed { recovery_cycle },
    );
    assert!(
        !budget_effects
            .iter()
            .any(|effect| matches!(effect, RtcPeerEffect::BuildSession { .. }))
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::RecoveryExhausted)
    );
    assert_eq!(state.active_generation, None);
    assert!(!state.awaiting_offer);
}

#[test]
fn answerer_failed_after_request_preserves_request_identity() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let initial_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(has_restart_request(&initial_effects, 1));
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("the initial Answerer recovery must create a request identity");
    let request_attempt = state.local_restart_request_attempt;
    let request_epoch = state.recovery_epoch;

    // A terminal state callback can arrive after the request-only recovery has
    // already started. Parking the physical failure must keep the same
    // correlated request instead of resetting the exchange to attempt 1.
    let failed_effects = reduce(&mut state, failed_tick(generation));
    assert!(!has_restart_ice(&failed_effects, generation));
    assert!(has_restart_request(&failed_effects, 2));
    assert!(!has_build_session(&failed_effects, generation + 1));
    assert!(state.awaiting_offer);
    assert_eq!(
        state.local_restart_request_id.as_deref(),
        Some(request_id.as_str())
    );
    assert_eq!(state.local_restart_request_attempt, request_attempt + 1);
    assert!(state.recovery_epoch > request_epoch);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));

    let duplicate_effects = reduce(&mut state, failed_tick(generation));
    assert!(
        !duplicate_effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { .. },
                ..
            }
        )),
        "repeated Failed callbacks after parking must not create a request storm"
    );
    assert_eq!(state.local_restart_request_attempt, request_attempt + 1);

    let retry_epoch = state.recovery_epoch;
    let retry = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: retry_epoch,
        },
    );
    assert!(retry.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            generation: actual_generation,
            message: SignalingMessage::IceRestartRequest {
                request_id: actual_id,
                attempt: 3,
            },
        } if *actual_generation == generation && actual_id == &request_id
    )));
}

#[test]
fn closed_answerer_retains_retired_generation_for_offer_wake() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let close_effects = reduce(&mut state, closed_tick(generation));

    assert!(has_close_session(&close_effects, generation));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));
    assert!(!state.is_live_generation(generation));
    assert!(state.awaiting_offer);

    let recovery_epoch = state.recovery_epoch;
    let wake_effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
    );
    assert!(
        wake_effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                generation: actual,
                message: SignalingMessage::IceRestartRequest { attempt: 1, .. },
            } if *actual == generation
        )),
        "a closed Answerer must wake the healthy Offerer using the retired generation identity"
    );
}

#[test]
fn driver_exit_answerer_retains_retired_generation_for_offer_wake() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::DriverExited {
            generation,
            error: Some(webrtc::error::Error::Other("driver stopped".into())),
        }),
    );

    assert!(has_close_session(&effects, generation));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));
    assert!(state.awaiting_offer);
}

#[test]
fn closed_answerer_wakes_stable_offerer_to_hard_recovery() {
    let mut answerer = RtcPeerState::new(test_actor_id(2));
    let answerer_generation = drive_answerer_to_negotiating(&mut answerer);
    drive_to_sendable(&mut answerer, answerer_generation);
    reduce(&mut answerer, closed_tick(answerer_generation));

    let mut offerer = RtcPeerState::new(test_actor_id(1));
    let offerer_generation = drive_offerer_to_sendable(&mut offerer);
    let mut request_id = None;

    for expected_attempt in 1..=4 {
        let recovery_epoch = answerer.recovery_epoch;
        let wake_effects = reduce(
            &mut answerer,
            RtcPeerInput::RecoveryDeadlineElapsed { recovery_epoch },
        );
        let (actual_request_id, actual_attempt) = wake_effects
            .iter()
            .find_map(|effect| match effect {
                RtcPeerEffect::SendSignaling {
                    generation,
                    message:
                        SignalingMessage::IceRestartRequest {
                            request_id,
                            attempt,
                        },
                } => {
                    assert_eq!(*generation, answerer_generation);
                    Some((request_id.clone(), *attempt))
                }
                _ => None,
            })
            .expect("parked Answerer must keep waking the Offerer");
        assert_eq!(actual_attempt, expected_attempt);
        if let Some(previous) = &request_id {
            assert_eq!(previous, &actual_request_id);
        } else {
            request_id = Some(actual_request_id.clone());
        }

        let offerer_effects = reduce(
            &mut offerer,
            RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequest {
                request_id: actual_request_id,
                attempt: actual_attempt,
            }),
        );
        if expected_attempt < 4 {
            assert!(has_restart_ice(&offerer_effects, offerer_generation));
            // The Offer is sent but cannot repair the dead remote session.
            // Complete local operations before the next timed retransmission.
            finish_restart_offer(&mut offerer, offerer_generation);
        } else {
            assert!(has_build_session(&offerer_effects, offerer_generation + 1));
        }
    }
}

fn finish_restart_offer(state: &mut RtcPeerState, generation: u64) {
    let operation_id = pending_operation(state, PendingRtcOperationKind::RestartIce, generation);
    let ice_generation = state.ice_generation;
    reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id,
            ice_generation,
            result: Ok(()),
        }),
    );
    let operation_id = pending_operation(state, PendingRtcOperationKind::CreateOffer, generation);
    reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::CreateOffer {
            generation,
            operation_id,
            result: Ok(Box::new(offer())),
        }),
    );
    let operation_id = pending_operation(
        state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id,
            result: Ok(()),
        }),
    );
    assert!(state.pending_local_sdp_exchange_id.is_some());
}

#[test]
fn late_answerer_retirement_starts_a_fresh_recovery_cycle() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::NetworkAvailable,
        )),
    );
    let cycle_before_retirement = state.recovery_cycle;
    assert!(state.recovery_inflight);

    reduce(&mut state, closed_tick(generation));

    assert_eq!(
        state.recovery_cycle,
        cycle_before_retirement + 1,
        "retiring the physical Answerer session starts a fresh end-to-end budget"
    );
    assert!(state.awaiting_offer);
    assert_eq!(state.parked_signaling_generation, Some(generation));
}

#[test]
fn hard_recovery_old_generation_events_ignored_after_pending() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);

    // Trigger Hard Recovery.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );

    // Complete the replacement build → pending_generation is set.
    let build_operation_id =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, old_gen + 1);
    reduce(&mut state, build_ok(old_gen + 1, build_operation_id));
    assert_eq!(state.pending_generation, Some(old_gen + 1));

    // A connected tick for the old generation must be ignored (not stale,
    // but not processed — the old session is retiring).
    let before_stale = state.stale_events;
    let before_connected = state.pc_connected;
    reduce(&mut state, connected_tick(old_gen));
    assert_eq!(
        state.stale_events, before_stale,
        "old gen not stale (still live)"
    );
    assert_eq!(
        state.pc_connected, before_connected,
        "old gen event must not change active state"
    );
    assert_eq!(state.phase, RtcPeerPhase::Negotiating);
}

#[test]
fn hard_recovery_closes_terminal_old_generation_without_replacing_pending() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_generation = drive_offerer_to_sendable(&mut state);
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    let replacement = old_generation + 1;
    let build_operation_id =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, replacement);
    reduce(&mut state, build_ok(replacement, build_operation_id));

    let effects = reduce(&mut state, closed_tick(old_generation));

    assert_eq!(state.active_generation, None);
    assert_eq!(state.pending_generation, Some(replacement));
    assert!(has_close_session(&effects, old_generation));
    assert!(!has_build_session(&effects, replacement + 1));
}

#[test]
fn hard_recovery_clears_old_ufrags_and_candidates() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);

    // Seed some ufrag/candidate state from the old session.
    state.local_ufrag = Some("old-ufrag".to_string());
    state.remote_ufrag = Some("remote-ufrag".to_string());
    state
        .known_remote_ufrags
        .push_back("old-remote".to_string());
    state
        .pending_candidates
        .push(webrtc::peer_connection::RTCIceCandidateInit {
            candidate: "candidate:1 1 udp 1 1.2.3.4 5 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            username_fragment: Some("old-remote".to_string()),
            url: None,
        });

    // Trigger Hard Recovery.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );

    assert!(state.local_ufrag.is_none(), "local ufrag cleared");
    assert!(state.remote_ufrag.is_none(), "remote ufrag cleared");
    assert!(
        state.known_remote_ufrags.is_empty(),
        "known remote ufrags cleared"
    );
    assert!(
        state.pending_candidates.is_empty(),
        "pending candidates cleared"
    );
    let _ = old_gen;
}

#[test]
fn hard_recovery_consumes_address_changed_flag() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let _gen = drive_offerer_to_sendable(&mut state);

    // Trigger Hard Recovery via ForceReconnect.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert!(
        !state.address_changed,
        "address_changed flag consumed by Hard Recovery"
    );

    // If the new build fails, the failure path should stay in Recovering
    // (ever_connected is true from the old session), not Idle. This confirms
    // the address_changed flag was consumed: a subsequent failure uses the
    // normal recovery path, not Hard.
    let new_gen = state.building_generation.unwrap();
    let operation_id = pending_operation(&state, PendingRtcOperationKind::BuildSession, new_gen);
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation: new_gen,
            operation_id,
            result: Err(webrtc::error::Error::Other("rebuild failed".into())),
        }),
    );
    assert_eq!(
        state.phase,
        RtcPeerPhase::Recovering,
        "build failure after Hard Recovery stays Recovering (ever_connected)"
    );
}

#[test]
fn hard_recovery_during_soft_recovery_escalates() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // Start Soft Recovery (transport failure).
    let restart = reduce(&mut state, failed_tick(generation));
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(state.recovery_inflight);
    let ice_after_soft = state.ice_generation;
    let (operation_id, ice_generation) = restart_identity(&restart, generation);

    // An explicit ForceReconnect while Soft Recovery is in flight escalates
    // to Hard Recovery.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert!(
        !has_close_session(&effects, generation),
        "old NOT closed yet (make-before-break)"
    );
    assert!(has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.ice_generation, 0, "ICE gen reset for new PC");
    let _ = ice_after_soft;

    // The stale RestartIce completion (for the old generation) is dropped
    // because building_generation was replaced.
    let before = state.stale_completions;
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::RestartIce {
            generation,
            operation_id,
            ice_generation,
            result: Ok(()),
        }),
    );
    assert_eq!(state.stale_completions, before + 1);
}

#[test]
fn make_before_break_atomic_switch_on_pending_ready() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);
    assert!(state.sendable());

    // Trigger explicit Hard Recovery (make-before-break: old stays active).
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert_eq!(state.active_generation, Some(old_gen));
    assert_eq!(state.building_generation, Some(old_gen + 1));

    // Build completes → pending_generation set (old still active).
    let build_operation_id =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, old_gen + 1);
    reduce(&mut state, build_ok(old_gen + 1, build_operation_id));
    assert_eq!(state.pending_generation, Some(old_gen + 1));
    assert_eq!(state.active_generation, Some(old_gen));
    assert!(!state.sendable(), "not sendable during make-before-break");

    // Pending session connects → pending_pc_connected.
    reduce(&mut state, connected_tick(old_gen + 1));
    assert!(state.pending_pc_connected);
    assert_eq!(state.phase, RtcPeerPhase::TransportReady);
    assert!(!state.sendable(), "not sendable until anchor opens");

    // Pending anchor opens → atomic switch fires.
    let effects = reduce(&mut state, rpc_reliable_opened(old_gen + 1));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(state.sendable(), "sendable after atomic switch");
    assert_eq!(state.active_generation, Some(old_gen + 1));
    assert_eq!(state.pending_generation, None);
    assert!(!state.recovery_inflight);
    // Old session is now retiring (closed).
    assert!(has_close_session(&effects, old_gen));
    assert!(state.sessions_to_close.contains(&old_gen));
}

#[test]
fn make_before_break_old_session_ignored_during_switch() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);

    // Trigger Hard Recovery.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );
    // Build completes → pending.
    let build_operation_id =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, old_gen + 1);
    reduce(&mut state, build_ok(old_gen + 1, build_operation_id));

    // Old session's lane events are ignored during make-before-break.
    let before = state.rpc_reliable_open;
    reduce(
        &mut state,
        RtcPeerInput::Lane(LaneEvent::Closed {
            generation: old_gen,
            lane: LaneKind::RpcReliable,
        }),
    );
    assert_eq!(
        state.rpc_reliable_open, before,
        "old session lane close must not change active state"
    );

    // Old session's state ticks are also ignored.
    reduce(&mut state, failed_tick(old_gen));
    assert_eq!(
        state.phase,
        RtcPeerPhase::Negotiating,
        "old session failure must not trigger recovery"
    );
}

#[test]
fn make_before_break_build_failure_retires_old_and_recovers() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let old_gen = drive_offerer_to_sendable(&mut state);

    // Trigger Hard Recovery.
    let _ = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
        )),
    );

    // The replacement build fails.
    let build_operation_id =
        pending_operation(&state, PendingRtcOperationKind::BuildSession, old_gen + 1);
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::BuildSession {
            generation: old_gen + 1,
            operation_id: build_operation_id,
            result: Err(webrtc::error::Error::Other("rebuild failed".into())),
        }),
    );
    // S5-4: the old session is retired (it was already failing) and the
    // peer stays in Recovering for a retry.
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert!(has_close_session(&effects, old_gen), "old session retired");
    assert_eq!(state.active_generation, None);
    assert!(state.recovery_inflight);
}

// ── S6-3: get_stats → network_path ─────────────────────────────────────────

#[test]
fn entering_sendable_emits_get_stats() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_negotiating(&mut state);
    reduce(&mut state, connected_tick(generation));

    let effects = reduce(&mut state, rpc_reliable_opened(generation));
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(
        effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::GetStats { generation: g, .. } if *g == generation
        )),
        "entering Sendable must emit a GetStats effect, got {effects:?}"
    );
}

#[test]
fn staying_sendable_does_not_re_emit_get_stats() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    // A no-op input while already Sendable must not re-sample.
    let effects = reduce(&mut state, RtcPeerInput::MailboxOverflow { generation });
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, RtcPeerEffect::GetStats { .. })),
        "GetStats re-emitted while already Sendable"
    );
}

#[test]
fn get_stats_completion_with_unresolvable_report_keeps_path_unknown() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let revision_before = state.revision;

    let mut operation_id = state.operation_id;
    for attempt in 1..=3 {
        let effects = reduce(
            &mut state,
            RtcPeerInput::Completion(EffectCompletion::GetStats {
                generation,
                operation_id,
                report: Default::default(),
            }),
        );
        if attempt < 3 {
            operation_id = effects
                .iter()
                .find_map(|effect| match effect {
                    RtcPeerEffect::GetStats { operation_id, .. } => Some(*operation_id),
                    _ => None,
                })
                .expect("missing selected pair must retry within the bounded budget");
        } else {
            assert!(
                effects.is_empty(),
                "the third unresolved sample must exhaust the retry budget"
            );
        }
    }
    assert_eq!(
        state.network_path,
        crate::wire::webrtc::v020::status::NetworkPath::Unknown
    );
    assert_eq!(
        state.revision, revision_before,
        "an empty report changes nothing"
    );
}

#[test]
fn get_stats_completion_from_a_stale_generation_is_dropped() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let stale_before = state.stale_completions;

    let operation_id = state.operation_id;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::GetStats {
            generation: generation + 99,
            operation_id,
            report: Default::default(),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(
        state.stale_completions,
        stale_before + 1,
        "stale GetStats completion must be counted"
    );
    assert_eq!(
        state.network_path,
        crate::wire::webrtc::v020::status::NetworkPath::Unknown
    );
}

// ── S6-10: answerer-side renegotiation offer ───────────────────────────────

/// Drive the answerer to Sendable with a ufrag-carrying initial offer, so the
/// state records a remote ufrag for renegotiation matching.
fn drive_answerer_to_sendable_with_ufrag(state: &mut RtcPeerState, ufrag: &str) -> u64 {
    let effects = reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, ufrag),
        ))),
    );
    assert!(has_build_session(&effects, 1));
    reduce(state, build_ok(1, 1));
    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: 2,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&effects, 1));
    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation: 1,
            operation_id: 3,
            result: Ok(Box::new(answer())),
        }),
    );
    assert!(has_set_local(&effects, 1));
    let effects = reduce(
        state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: 4,
            result: Ok(()),
        }),
    );
    assert!(has_send_answer(&effects, 1));
    drive_to_sendable(state, 1);
    1
}

#[test]
fn answerer_restart_offer_deadline_rearms_only_for_a_new_ice_generation() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    let first = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some("deadline-first".to_string()),
        }),
    );
    assert!(has_set_remote(&first, generation));
    let first_deadline = state.answerer_offer_deadline;
    let first_epoch = state.recovery_epoch;
    let first_ice_generation = state.ice_generation;

    // A retransmitted Offer from the same ICE generation is queued behind the
    // current SetRemoteDescription, but it must not refresh the attempt timer.
    let duplicate = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some("deadline-duplicate".to_string()),
        }),
    );
    assert!(duplicate.is_empty());
    assert_eq!(state.answerer_offer_deadline, first_deadline);
    assert_eq!(state.recovery_epoch, first_epoch);
    assert_eq!(state.ice_generation, first_ice_generation);
    assert!(state.queued_remote_description.is_some());

    // A changed remote ICE credential is a new exchange and is allowed to
    // receive its own deadline window.
    let changed = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-3")),
            exchange_id: Some("deadline-new-generation".to_string()),
        }),
    );
    assert!(changed.is_empty());
    assert_eq!(state.ice_generation, first_ice_generation + 1);
    assert_ne!(state.answerer_offer_deadline, first_deadline);
    assert!(state.recovery_epoch > first_epoch);
}

/// Start an answerer recovery request and apply the Offer that completes the
/// request/Offer hand-off. The returned operation is the production
/// SetRemoteDescription step used by the stage-specific failure tests below.
fn prepare_answerer_restart_offer_for_sdp_failure(state: &mut RtcPeerState) -> (u64, u64) {
    let generation = drive_answerer_to_sendable_with_ufrag(state, "remote-1");
    reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let request = reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_request(&request, 1));

    let offer = reduce(
        state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some("sdp-failure-recovery-offer".to_string()),
        }),
    );
    assert!(has_set_remote(&offer, generation));
    let operation_id = pending_operation(
        state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    (generation, operation_id)
}

#[test]
fn answerer_set_remote_description_failure_wakes_offerer() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let (generation, operation_id) = prepare_answerer_restart_offer_for_sdp_failure(&mut state);
    let recovery_epoch = state.recovery_epoch;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id,
            result: Err(webrtc::error::Error::Other("set remote failed".into())),
        }),
    );

    assert!(has_restart_request(&effects, 2));
    assert_eq!(state.local_restart_request_attempt, 2);
    assert!(!state.recovery_negotiated);
    assert!(state.recovery_epoch > recovery_epoch);
}

#[test]
fn answerer_create_answer_failure_wakes_offerer() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let (generation, set_remote_operation) =
        prepare_answerer_restart_offer_for_sdp_failure(&mut state);

    let remote_completed = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&remote_completed, generation));
    let create_answer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateAnswer, generation);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation,
            operation_id: create_answer_operation,
            result: Err(webrtc::error::Error::Other("create answer failed".into())),
        }),
    );

    assert!(has_restart_request(&effects, 2));
    assert_eq!(state.local_restart_request_attempt, 2);
    assert!(!state.recovery_negotiated);
}

#[test]
fn answerer_set_local_description_failure_wakes_offerer() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let (generation, set_remote_operation) =
        prepare_answerer_restart_offer_for_sdp_failure(&mut state);

    let remote_completed = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&remote_completed, generation));
    let create_answer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateAnswer, generation);
    let local = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation,
            operation_id: create_answer_operation,
            result: Ok(Box::new(answer())),
        }),
    );
    assert!(has_set_local(&local, generation));
    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );

    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Err(webrtc::error::Error::Other("set local failed".into())),
        }),
    );

    assert!(has_restart_request(&effects, 2));
    assert_eq!(state.local_restart_request_attempt, 2);
    assert!(!state.recovery_negotiated);
}

/// A same-ufrag offer on an established session is a renegotiation (media
/// track add/remove): the answerer applies it (SetRemoteDescription) instead
/// of dropping it as stale.
#[test]
fn answerer_applies_same_ufrag_offer_as_renegotiation() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    let stale_before = state.stale_events;

    // The remote re-offers with the SAME ufrag (no ICE restart) carrying a
    // new media section.
    let remote_ufrag = state.remote_ufrag.clone().expect("remote ufrag known");
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, &remote_ufrag),
        ))),
    );

    assert_eq!(state.stale_events, stale_before, "must not count as stale");
    assert!(
        has_set_remote(&effects, generation),
        "expected SetRemoteDescription for the active generation"
    );
    assert_eq!(
        state.phase,
        RtcPeerPhase::Sendable,
        "session stays sendable"
    );
}

/// A wire-tagged ICE restart offer with changed ufrag stays on the same
/// PeerConnection. It must not be misclassified as a fresh initial offer.
#[test]
fn answerer_applies_new_ufrag_offer_as_soft_ice_restart() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    let ice_generation = state.ice_generation;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-2"),
        ))),
    );

    assert!(has_set_remote(&effects, generation));
    assert!(!has_build_session(&effects, generation + 1));
    assert!(!has_close_session(&effects, generation));
    assert_eq!(state.active_generation, Some(generation));
    assert_eq!(state.ice_generation, ice_generation + 1);
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
    assert_eq!(
        state.applied_remote_description_generation, None,
        "new ICE generation stays gated until remote SDP completes"
    );
}

#[test]
fn answerer_parked_restart_reuses_open_lane_without_second_on_open() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    assert!(state.sendable());

    // A hard recovery request can park an Answerer while its active
    // PeerConnection and RpcReliable DataChannel are still physically alive.
    // The DataChannel will not emit another OnOpen for the same-generation ICE
    // restart, so its readiness fact must survive the park.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::ForceReconnect,
        )),
    );
    assert!(!has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, Some(generation));
    assert!(state.awaiting_offer);
    assert!(state.rpc_reliable_open);
    assert!(!state.sendable());

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-2"),
        ))),
    );
    assert!(has_set_remote(&effects, generation));
    assert!(!state.awaiting_offer);

    let set_remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&effects, generation));

    let create_answer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateAnswer, generation);
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation,
            operation_id: create_answer_operation,
            result: Ok(Box::new(sdp_with_ufrag(RTCSdpType::Answer, "local-2"))),
        }),
    );
    assert!(has_set_local(&effects, generation));

    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Ok(()),
        }),
    );
    assert!(state.recovery_negotiated);

    // No LaneEvent::Opened is injected here: the existing RpcReliable lane is
    // the readiness anchor for this ICE restart.
    reduce(&mut state, connected_tick(generation));
    assert!(state.sendable());
    assert_eq!(state.phase, RtcPeerPhase::Sendable);
    assert!(!state.recovery_inflight);
}

#[test]
fn answerer_parked_restart_does_not_restore_closed_lane() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    // Closing the readiness anchor revokes the physical fact before the
    // Answerer parks. A later ICE restart must not treat the old lane as open.
    reduce(
        &mut state,
        RtcPeerInput::Lane(LaneEvent::Closed {
            generation,
            lane: LaneKind::READINESS_ANCHOR,
        }),
    );
    assert!(!state.rpc_reliable_open);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-2"),
        ))),
    );
    assert!(has_set_remote(&effects, generation));

    let set_remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&effects, generation));

    let create_answer_operation =
        pending_operation(&state, PendingRtcOperationKind::CreateAnswer, generation);
    let effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CreateAnswer {
            generation,
            operation_id: create_answer_operation,
            result: Ok(Box::new(sdp_with_ufrag(RTCSdpType::Answer, "local-2"))),
        }),
    );
    assert!(has_set_local(&effects, generation));

    let set_local_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetLocalDescription,
        generation,
    );
    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetLocalDescription {
            generation,
            operation_id: set_local_operation,
            result: Ok(()),
        }),
    );
    reduce(&mut state, connected_tick(generation));
    assert!(!state.sendable());
    assert!(!state.rpc_reliable_open);
}

#[test]
fn answerer_failed_session_retires_generation_before_parking() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    let effects = reduce(&mut state, failed_tick(generation));

    assert!(has_close_session(&effects, generation));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));
    assert!(!state.pc_connected);
    assert!(!state.rpc_reliable_open);
    assert!(!state.sendable());
    assert!(state.awaiting_offer);

    let restart = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some("v020-restart-1".to_string()),
        }),
    );
    assert!(restart.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartRejected { exchange_id, reason },
            ..
        } if exchange_id == "v020-restart-1"
            && *reason == actr_protocol::ice_restart_rejected::Reason::NoActiveSession as i32
    )));
    assert!(!has_set_remote(&restart, generation));
}

/// The answerer's own soft recovery is request-only: no local RestartIce
/// effect is dispatched, and its local ICE restart happens implicitly when the
/// offerer's restart offer is applied. A local restart_ice() racing that offer
/// would re-roll credentials after the answer was sent, wedge the round (the
/// offerer de-dupes the attempt-1 request), and escalate both ends to a Hard
/// rebuild at the attempt deadline.
#[test]
fn answerer_soft_recovery_sends_request_without_local_restart_ice() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );

    assert!(
        !has_restart_ice(&effects, generation),
        "answerer soft recovery must not dispatch a local RestartIce effect"
    );
    assert!(
        has_restart_request(&effects, 1),
        "answerer soft recovery dispatches the restart request directly"
    );
    assert!(state.recovery_inflight);
    assert!(state.recovery_attempt_started);
    assert!(!state.recovery_negotiated);
    assert_eq!(state.phase, RtcPeerPhase::Recovering);
}

#[test]
fn offerer_restart_request_ack_outcome_is_committed_with_the_request() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);
    let (respond, mut outcome) = tokio::sync::oneshot::channel();

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestCommitted {
            request_id: "committed-request".to_string(),
            attempt: 1,
            respond,
        }),
    );

    assert_eq!(
        outcome.try_recv().expect("committed request outcome"),
        actr_protocol::ice_restart_request_ack::Outcome::Accepted as i32
    );
    assert!(has_restart_ice(&effects, generation));
}

#[test]
fn restart_request_committed_outcome_distinguishes_role_and_session() {
    let mut answerer = RtcPeerState::new(test_actor_id(2));
    let (respond, mut outcome) = tokio::sync::oneshot::channel();
    reduce(
        &mut answerer,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestCommitted {
            request_id: "wrong-role".to_string(),
            attempt: 1,
            respond,
        }),
    );
    assert_eq!(
        outcome.try_recv().expect("wrong-role outcome"),
        actr_protocol::ice_restart_request_ack::Outcome::WrongRole as i32
    );

    let mut offerer_without_session = RtcPeerState::new(test_actor_id(3));
    offerer_without_session.role = Role::Offerer;
    let (respond, mut outcome) = tokio::sync::oneshot::channel();
    reduce(
        &mut offerer_without_session,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestCommitted {
            request_id: "no-session".to_string(),
            attempt: 1,
            respond,
        }),
    );
    assert_eq!(
        outcome.try_recv().expect("no-session outcome"),
        actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession as i32
    );
}

#[test]
fn answerer_no_active_session_ack_starts_fresh_role_negotiation() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    let request = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(has_restart_request(&request, 1));
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("answerer request identity");

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id,
            attempt: 1,
            outcome: actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession as i32,
        }),
    );

    assert!(has_close_session(&effects, generation));
    assert!(has_role_negotiation(&effects));
    assert_eq!(state.role, Role::Undetermined);
    assert!(state.role_negotiation_inflight);
    assert!(state.initial_connection_inflight);
    assert_eq!(state.active_generation, None);
    assert_eq!(state.phase, RtcPeerPhase::Building);
}

#[test]
fn answerer_accepted_ack_keeps_the_current_request_flight() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    let request = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(has_restart_request(&request, 1));
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("answerer request identity");
    let stale_events = state.stale_events;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id: request_id.clone(),
            attempt: 1,
            outcome: actr_protocol::ice_restart_request_ack::Outcome::Accepted as i32,
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.stale_events, stale_events);
    assert_eq!(
        state.local_restart_request_id.as_deref(),
        Some(request_id.as_str())
    );
    assert_eq!(state.phase, RtcPeerPhase::Recovering);

    // A duplicate Accepted Ack is idempotent and must not restart the
    // Answerer or retire its generation.
    let duplicate = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id,
            attempt: 1,
            outcome: actr_protocol::ice_restart_request_ack::Outcome::Accepted as i32,
        }),
    );
    assert!(duplicate.is_empty());
    assert_eq!(state.stale_events, stale_events);
}

#[test]
fn answerer_restart_request_ack_rejects_stale_and_unknown_outcomes() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);
    let request = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::LocalAddressChanged,
        )),
    );
    assert!(has_restart_request(&request, 1));
    let request_id = state
        .local_restart_request_id
        .clone()
        .expect("answerer request identity");

    let stale_id = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id: "stale-request".to_string(),
            attempt: 1,
            outcome: actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession as i32,
        }),
    );
    assert!(stale_id.is_empty());
    assert_eq!(state.stale_events, 1);
    assert_eq!(
        state.local_restart_request_id.as_deref(),
        Some(request_id.as_str())
    );

    let stale_attempt = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id: request_id.clone(),
            attempt: 2,
            outcome: actr_protocol::ice_restart_request_ack::Outcome::WrongRole as i32,
        }),
    );
    assert!(stale_attempt.is_empty());
    assert_eq!(state.stale_events, 2);

    let unknown_outcome = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyIceRestartRequestAck {
            request_id,
            attempt: 1,
            outcome: 99,
        }),
    );
    assert!(unknown_outcome.is_empty());
    assert_eq!(state.stale_events, 3);
}

/// Once the answerer joins the offerer's recovery round by applying the
/// restart offer, its already-sent request is complete: the answer path must
/// not re-emit another restart request (a late duplicate would race the
/// answer and re-roll credentials out from under it).
#[test]
fn answerer_does_not_reemit_request_after_applying_restart_offer() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    // The answerer's own round starts (request-only).
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let start = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    assert!(has_restart_request(&start, 1));
    let request_attempt_epoch = state.recovery_epoch;

    // The offerer's restart offer lands: the answerer applies it (implicit
    // local restart inside set_remote_description) and must produce only the
    // remote-description effect — never another restart request.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-2"),
        ))),
    );
    assert!(has_set_remote(&effects, generation));
    assert!(
        !has_restart_ice(&effects, generation),
        "the local restart is implicit in set_remote_description, not an effect"
    );
    assert!(
        !effects.iter().any(|e| matches!(
            e,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { .. },
                ..
            }
        )),
        "applying the restart offer must not re-emit the restart request"
    );
    assert_ne!(
        state.recovery_epoch, request_attempt_epoch,
        "the accepted restart offer must supersede the request-only deadline"
    );
    let stale_deadline = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: request_attempt_epoch,
        },
    );
    assert!(
        stale_deadline.is_empty(),
        "the request deadline must be stale once the restart offer is accepted"
    );

    // The answer path follows the remote description completion.
    let set_remote_operation = pending_operation(
        &state,
        PendingRtcOperationKind::SetRemoteDescription,
        generation,
    );
    let answer_effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation,
            operation_id: set_remote_operation,
            result: Ok(()),
        }),
    );
    assert!(has_create_answer(&answer_effects, generation));
}

/// A restart Offer can complete its SDP exchange while a failed Answerer still
/// has no usable RpcReliable readiness anchor. The next deadline must continue
/// the existing request identity instead of parking through
/// `clear_negotiation_for_recovery`, which would reset the attempt to one.
#[test]
fn answerer_failed_restart_offer_retries_same_request_identity() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");

    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let start = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkAvailable,
        )),
    );
    let request_id = start
        .iter()
        .find_map(|effect| match effect {
            RtcPeerEffect::SendSignaling {
                message:
                    SignalingMessage::IceRestartRequest {
                        request_id,
                        attempt: 1,
                    },
                ..
            } => Some(request_id.clone()),
            _ => None,
        })
        .expect("initial Answerer recovery request");
    let epoch = state.recovery_epoch;

    // Model the post-Offer/Answer state observed by CG-001: SDP/ICE
    // negotiation completed, but the physical anchor was revoked by Failed.
    state.phase = RtcPeerPhase::TransportReady;
    state.recovery_negotiated = true;
    state.recovery_attempt_started = true;
    state.rpc_reliable_open = false;
    state.awaiting_offer = false;
    assert_eq!(state.active_generation, Some(generation));

    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch,
        },
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RtcPeerEffect::SendSignaling {
            message: SignalingMessage::IceRestartRequest {
                request_id: actual,
                attempt: 2,
            },
            ..
        } if actual == &request_id
    )));
    assert!(!has_build_session(&effects, generation + 1));
    assert!(!state.awaiting_offer);
    assert!(!state.recovery_negotiated);
    assert_eq!(
        state.local_restart_request_id.as_deref(),
        Some(request_id.as_str())
    );
    assert_eq!(state.local_restart_request_attempt, 2);
}

/// A normal (non-restart-tagged) offer with fresh ICE credentials comes from
/// a replacement physical PeerConnection. The established answerer must retire
/// its old generation and build a fresh Session instead of applying that SDP to
/// the stale PC.
#[test]
fn answerer_rebuilds_for_fresh_offer_with_new_ufrag() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    let old_transport_epoch = state.transport_epoch;

    // Reproduce the production edge: the old connection is inside its total
    // recovery budget and owns attempt-scoped SDP/ICE facts when the remote's
    // freshly-created PC sends an ordinary Offer.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(
            RtcNetworkEvent::NetworkUnavailable,
        )),
    );
    let old_recovery_cycle = state.recovery_cycle;
    state.pending_candidates.push(candidate("old-candidate"));
    state.pending_candidates.push(candidate("remote-fresh-pc"));
    state.last_remote_restart_request = Some(("old-request".to_string(), 3));
    assert!(state.recovery_inflight);

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-fresh-pc"),
        ))),
    );

    assert!(has_close_session(&effects, generation));
    assert!(has_build_session(&effects, generation + 1));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.building_generation, Some(generation + 1));
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.transport_epoch, old_transport_epoch + 1);
    assert!(!state.recovery_inflight, "fresh Offer cancels old budget");
    assert!(!state.ever_connected, "replacement is a fresh attempt");
    assert_eq!(
        state.pending_candidates.len(),
        1,
        "a matching trickle candidate that overtook the fresh Offer survives"
    );
    assert_eq!(
        state.pending_candidates[0].username_fragment.as_deref(),
        Some("remote-fresh-pc")
    );
    assert!(state.last_remote_restart_request.is_none());

    // A ready timer fact from the old recovery cycle cannot terminate the new
    // building generation after the Offer has committed the fresh attempt.
    let timer_effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryBudgetElapsed {
            recovery_cycle: old_recovery_cycle,
        },
    );
    assert!(timer_effects.is_empty());
    assert_eq!(state.phase, RtcPeerPhase::Building);
    assert_eq!(state.building_generation, Some(generation + 1));

    let build_operation = pending_operation(
        &state,
        PendingRtcOperationKind::BuildSession,
        generation + 1,
    );
    let build_effects = reduce(&mut state, build_ok(generation + 1, build_operation));
    assert!(has_set_remote(&build_effects, generation + 1));
    let (remote_generation, remote_operation) = state
        .pending_remote_description_operation
        .expect("fresh remote description operation");
    let remote_effects = reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::SetRemoteDescription {
            generation: remote_generation,
            operation_id: remote_operation,
            result: Ok(()),
        }),
    );
    assert!(
        has_add_ice_candidate(&remote_effects, generation + 1),
        "the preserved fresh candidate flushes after remote SDP commits"
    );
}

/// A renegotiation offer when there is no active session is still dropped.
#[test]
fn answerer_drops_same_ufrag_offer_without_session() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    let remote_ufrag = state.remote_ufrag.clone().expect("remote ufrag known");
    // Remove the active generation (simulating a retired session).
    state.active_generation = None;
    let stale_before = state.stale_events;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteDescription(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, &remote_ufrag),
        ))),
    );
    let _ = generation;
    assert_eq!(state.stale_events, stale_before + 1);
    assert!(effects.is_empty());
}

#[test]
fn answerer_rejects_restart_offer_when_controller_has_no_active_session() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    reduce(&mut state, closed_tick(generation));
    assert_eq!(state.active_generation, None);
    assert_eq!(state.parked_signaling_generation, Some(generation));

    let exchange_id = "restart-without-active-session".to_string();
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: Some(exchange_id.clone()),
        }),
    );
    assert_eq!(
        effects.len(),
        1,
        "a rejected restart must emit exactly one effect"
    );
    assert!(matches!(
        &effects[0],
        RtcPeerEffect::SendSignaling {
            generation: actual_generation,
            message: SignalingMessage::IceRestartRejected {
                exchange_id: actual_exchange,
                reason,
            },
        } if *actual_generation == generation
            && actual_exchange == &exchange_id
            && *reason == actr_protocol::ice_restart_rejected::Reason::NoActiveSession as i32
    ));
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, RtcPeerEffect::BuildSession { .. })),
        "a rejected restart must not build an ordinary replacement session"
    );
    assert_eq!(state.stale_events, 1);
    assert_eq!(state.active_generation, None);
}

#[test]
fn offerer_without_session_ignores_correlated_restart_offer_without_rejection() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    state.role = Role::Offerer;

    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-1")),
            exchange_id: Some("glare-restart".to_string()),
        }),
    );
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRejected { .. },
                ..
            }
        )),
        "an Offerer stays on the stale-offer path; no rejection"
    );
    assert!(
        !has_build_session(&effects, 1),
        "the stale competing offer builds nothing"
    );
    assert_eq!(state.stale_events, 1);
}

#[test]
fn uncorrelated_restart_offer_without_session_falls_through_to_ordinary_offer() {
    let mut state = RtcPeerState::new(test_actor_id(2));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    reduce(&mut state, closed_tick(generation));
    assert_eq!(state.active_generation, None);

    // A legacy uncorrelated restart offer (a pre-correlation peer in a
    // rolling upgrade) has no exchange id to fence a rejection with — and a
    // peer that old may not understand IceRestartRejected at all — so it
    // keeps the ordinary-offer fallthrough that builds the replacement.
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-2")),
            exchange_id: None,
        }),
    );
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRejected { .. },
                ..
            }
        )),
        "an uncorrelated offer must not be rejected"
    );
    assert!(
        has_build_session(&effects, generation + 1),
        "the ordinary-offer fallthrough builds the replacement session"
    );
}

#[test]
fn undetermined_controller_rejects_restart_offer_without_session() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let exchange_id = "restart-after-budget-exhaustion".to_string();
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
            desc: Box::new(sdp_with_ufrag(RTCSdpType::Offer, "remote-1")),
            exchange_id: Some(exchange_id.clone()),
        }),
    );

    assert_eq!(
        effects.len(),
        1,
        "a rejected restart must emit exactly one effect"
    );
    assert!(matches!(
        &effects[0],
        RtcPeerEffect::SendSignaling {
            generation: 0,
            message: SignalingMessage::IceRestartRejected {
                exchange_id: actual_exchange,
                reason,
            },
        } if actual_exchange == &exchange_id
            && *reason == actr_protocol::ice_restart_rejected::Reason::NoActiveSession as i32
    ));
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, RtcPeerEffect::BuildSession { .. })),
        "an undetermined controller must not build an ordinary replacement session"
    );
    assert_eq!(state.stale_events, 1);
    assert_eq!(state.role, Role::Undetermined);
    assert_eq!(state.active_generation, None);
}

#[test]
fn undetermined_controller_drops_legacy_restart_offer_without_session() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let stale_before = state.stale_events;
    let effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(
            sdp_with_ufrag(RTCSdpType::Offer, "remote-legacy"),
        ))),
    );

    assert!(effects.is_empty());
    assert_eq!(state.stale_events, stale_before + 1);
    assert_eq!(state.role, Role::Undetermined);
    assert_eq!(state.active_generation, None);
}

/// Offerer-side `NegotiationNeeded` (local track add/remove mid-connection)
/// drives a serialized re-offer.
#[test]
fn offerer_negotiation_needed_creates_reoffer() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded { generation }),
    );
    assert!(
        has_create_offer(&effects, generation),
        "offerer renegotiation must produce CreateOffer"
    );
    assert!(state.renegotiation_inflight);

    // A duplicate while the re-offer is in flight merges (no second offer).
    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded { generation }),
    );
    assert!(effects.is_empty(), "in-flight duplicate must merge");
    assert!(state.renegotiation_pending);
}

/// BREAKING (v0.20): answerer-side `NegotiationNeeded` is deliberately
/// dropped — answerer-initiated media renegotiation is not supported (the
/// 0.14 `Coordinator::renegotiate_connection` re-offered from either role).
/// The answerer still applies INCOMING offerer re-offers (covered by
/// `answerer_applies_same_ufrag_offer_as_renegotiation`).
#[test]
fn answerer_negotiation_needed_is_dropped() {
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_sendable_with_ufrag(&mut state, "remote-1");
    assert_eq!(state.phase, RtcPeerPhase::Sendable);

    let effects = reduce(
        &mut state,
        RtcPeerInput::DriverEvent(PeerDriverEvent::NegotiationNeeded { generation }),
    );
    assert!(
        effects.is_empty(),
        "answerer NegotiationNeeded must not produce effects, got {effects:?}"
    );
    assert!(
        !state.renegotiation_inflight && !state.renegotiation_pending,
        "dropped event must not arm renegotiation bookkeeping"
    );
    assert_eq!(
        state.phase,
        RtcPeerPhase::Sendable,
        "session stays sendable"
    );
}

// ── graceful close is a transport terminal state (mp06) ────────────────────

#[test]
fn graceful_close_convergence_stamps_closed_reason_for_retirement() {
    // A graceful close must reach the same terminal shape as a failed
    // attempt (Idle + reason + no generations): that shape is what the
    // controller's retirement edge checks to publish
    // `peer_transport_retired`, reclaiming the outbound DestTransport.
    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_offerer_to_sendable(&mut state);

    let effects = reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert_eq!(state.phase, RtcPeerPhase::Closing);
    let operation_id = close_operation(&effects, generation);

    reduce(
        &mut state,
        RtcPeerInput::Completion(EffectCompletion::CloseSession {
            generation,
            operation_id,
            result: Ok(()),
        }),
    );
    assert_eq!(state.phase, RtcPeerPhase::Idle);
    assert!(state.terminal_close_done);
    assert_eq!(
        state.unavailable_reason,
        Some(RtcUnavailableReason::Closed),
        "graceful close must stamp a reason so the retirement edge fires"
    );
    assert!(
        state.terminally_unavailable(),
        "converged graceful close must satisfy the retirement predicate"
    );
}

#[test]
fn graceful_close_of_idle_controller_stamps_closed_reason() {
    // The early-return convergence point (close of a freshly-created or
    // otherwise idle controller) is equally terminal for the transport.
    let mut state = RtcPeerState::new(test_actor_id(1));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Connect));
    reduce(&mut state, RtcPeerInput::Command(RtcPeerCommand::Close));
    assert!(state.terminal_close_done);
    assert_eq!(state.unavailable_reason, Some(RtcUnavailableReason::Closed));
    assert!(state.terminally_unavailable());
}

#[test]
fn answerer_deadline_after_background_defer_retransmits_same_request() {
    // p08 + answerer_retransmits combined contract: a background/gate edge
    // arriving over an in-flight answerer restart request must neither wipe
    // the exchange identity nor block the deadline-driven retransmit. The
    // retransmission keeps the correlated request id and bumps only the
    // attempt counter — no second exchange, no hard rebuild.
    use crate::wire::webrtc::v020::recovery::RtcNetworkEvent as Ev;

    let mut state = RtcPeerState::new(test_actor_id(1));
    let generation = drive_answerer_to_negotiating(&mut state);
    drive_to_sendable(&mut state, generation);

    // Soft recovery starts: the answerer emits its restart request
    // immediately (request-only recovery — no local RestartIce effect).
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(Ev::NetworkUnavailable)),
    );
    let start_effects = reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(Ev::NetworkAvailable)),
    );
    assert!(
        !start_effects
            .iter()
            .any(|effect| matches!(effect, RtcPeerEffect::RestartIce { .. })),
        "answerer soft recovery must not dispatch a local RestartIce effect"
    );
    assert!(
        start_effects.iter().any(|effect| matches!(
            effect,
            RtcPeerEffect::SendSignaling {
                message: SignalingMessage::IceRestartRequest { attempt: 1, .. },
                ..
            }
        )),
        "answerer soft recovery must dispatch the restart request directly"
    );
    assert!(state.recovery_attempt_started);
    let request_id = state.local_restart_request_id.clone();
    let attempt_before = state.local_restart_request_attempt;
    let epoch = state.recovery_epoch;
    assert!(
        request_id.is_some(),
        "answerer must have sent a restart request"
    );

    // The lifecycle edge (background / gate withdrawal) arrives over the
    // in-flight round: it must pause sendability without wiping the round.
    reduce(
        &mut state,
        RtcPeerInput::Command(RtcPeerCommand::NetworkEvent(Ev::AppBackground)),
    );
    assert!(
        state.recovery_attempt_started,
        "an in-flight recovery round must survive the background defer"
    );
    assert_eq!(
        state.local_restart_request_id, request_id,
        "the restart request identity must survive the background defer"
    );
    assert_eq!(state.recovery_epoch, epoch, "deadline is not extended");
    assert!(!state.pc_connected, "sendable must expire while gated");

    // The per-attempt deadline elapses while paused: the answerer
    // retransmits the SAME correlated request instead of rebuilding the PC.
    let effects = reduce(
        &mut state,
        RtcPeerInput::RecoveryDeadlineElapsed {
            recovery_epoch: epoch,
        },
    );
    let retransmit = effects.iter().any(|effect| match effect {
        RtcPeerEffect::SendSignaling {
            message:
                SignalingMessage::IceRestartRequest {
                    request_id: id,
                    attempt,
                },
            ..
        } => id == request_id.as_ref().expect("request id") && *attempt == attempt_before + 1,
        _ => false,
    });
    assert!(
        retransmit,
        "deadline over a paused in-flight round must retransmit the same request"
    );
    assert!(
        !has_build_session(&effects, generation + 1),
        "no hard rebuild while the in-flight round is alive"
    );
    assert_eq!(state.active_generation, Some(generation));
}
