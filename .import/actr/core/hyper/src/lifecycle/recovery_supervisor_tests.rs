//! Synchronous engine tests for the responsive recovery supervisor.
//!
//! These exercise the RFC-0400 policy engine directly — no runtime, no wall
//! clock — by feeding [`tp::Input`]s with an explicit monotonic `now` and an
//! injected deterministic entropy source. They cover the Phase 2 acceptance
//! invariants that are decidable in the pure engine: snapshot acceptance,
//! offline hysteresis, mode gating, the completion procedure, retry gating,
//! escalation, cleanup supersession, and the lifecycle profile.

use std::time::Duration;

use super::{RecoverySupervisor, TimerOp, event_to_input, network_action_of};
use crate::lifecycle::network_event::{
    AppLifecycleState, CleanupReason, NetworkAvailability, NetworkEvent, NetworkRecoveryAction,
    NetworkSnapshot, NetworkTransportFlags, ReconnectReason,
};
use crate::lifecycle::recovery_policy::classification::{
    FixedEntropy, ParkCause, ReleaseMask, TriggerClass,
};
use crate::lifecycle::recovery_policy::diagnosis::{EffectDiagnosis, EffectKind, EffectOutcome};
use crate::lifecycle::recovery_policy::translate as tp;

fn ungated() -> RecoverySupervisor {
    RecoverySupervisor::new_with_entropy(
        tp::LifecycleProfile::Ungated,
        Box::new(FixedEntropy::constant(0.5)),
    )
}

fn gated() -> RecoverySupervisor {
    RecoverySupervisor::new_with_entropy(
        tp::LifecycleProfile::Gated,
        Box::new(FixedEntropy::constant(0.5)),
    )
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn default_and_debug_preserve_the_ungated_terminal_view() {
    let mut supervisor = RecoverySupervisor::default();
    let rendered = format!("{supervisor:?}");
    assert!(rendered.starts_with("RecoverySupervisor"));
    assert!(rendered.contains("view"));
    assert_eq!(supervisor.view().profile, tp::LifecycleProfile::Ungated);

    supervisor.view.recovery_mode = tp::RecoveryModeState::Terminating;
    supervisor.apply_recovery_mode(tp::RecoveryModeInput::SessionActivated);
    assert_eq!(
        supervisor.view().recovery_mode,
        tp::RecoveryModeState::Terminating,
        "a terminal supervisor must not be reactivated"
    );
}

#[test]
fn invalid_or_recordless_machine_inputs_are_fail_closed() {
    let mut supervisor = ungated();

    supervisor.apply_recovery_intent(
        tp::RecoveryIntentInput::CompleteProbe,
        true,
        1,
        tp::RecoveryImpacts::ALL,
    );
    supervisor.apply_cleanup_work(tp::CleanupWorkInput::CompleteCleanup, 1);
    supervisor.apply_offline_work(tp::OfflineWorkInput::CompleteDisconnect);
    supervisor.apply_execution(tp::ExecutionInput::Succeeded);
    supervisor.apply_retry_gate(
        tp::RetryDomain::Cleanup,
        tp::RetryGateInput::RetryableFailure,
        1,
    );

    assert_eq!(
        supervisor.view().recovery_intent,
        tp::RecoveryIntentState::Idle
    );
    assert_eq!(supervisor.view().cleanup_work, tp::CleanupWorkState::Idle);
    assert_eq!(supervisor.view().offline_work, tp::OfflineWorkState::Idle);
    assert_eq!(supervisor.view().execution, tp::ExecutionState::Idle);
    assert!(supervisor.pending_ops.is_empty());
}

#[test]
fn retry_gate_wake_pause_and_sticky_mask_update_the_current_record() {
    let mut supervisor = ungated();
    supervisor.view.recovery_record =
        Some(tp::PendingRecord::recovery(1, tp::RecoveryStrength::Probe));

    supervisor.apply_retry_gate(
        tp::RetryDomain::Recovery,
        tp::RetryGateInput::RetryableFailure,
        2,
    );
    let record = supervisor
        .view()
        .recovery_record
        .as_ref()
        .expect("recovery record should remain present");
    assert_eq!(record.gate, tp::RetryGateState::BackingOff);
    assert_eq!(record.attempt, 1);
    assert_ne!(record.retry_id, 0);

    supervisor.pending_ops.clear();
    supervisor.apply_gate_trigger(
        tp::GateTrigger::Wake {
            domain: tp::RetryDomain::Recovery,
        },
        3,
    );
    let record = supervisor
        .view()
        .recovery_record
        .as_ref()
        .expect("wake should retain the recovery record");
    assert_eq!(record.gate, tp::RetryGateState::Ready);
    assert_eq!(record.attempt, 0);
    assert_eq!(record.work_revision, 3);
    assert!(matches!(
        supervisor.pending_ops.as_slice(),
        [TimerOp::Cancel {
            key: tp::TimerId::FailureBackoff(tp::RetryDomain::Recovery)
        }]
    ));

    supervisor.pending_ops.clear();
    supervisor.apply_retry_gate(
        tp::RetryDomain::Recovery,
        tp::RetryGateInput::NewMaterialTrigger,
        4,
    );
    supervisor.apply_retry_gate(
        tp::RetryDomain::Recovery,
        tp::RetryGateInput::ExplicitPause,
        4,
    );
    assert_eq!(
        supervisor
            .view()
            .recovery_record
            .as_ref()
            .expect("pause should retain the record")
            .gate,
        tp::RetryGateState::Parked
    );
    assert_eq!(supervisor.pending_ops.len(), 2);

    {
        let record = supervisor
            .view
            .recovery_record
            .as_mut()
            .expect("parked record should exist");
        record.release_mask = ReleaseMask::single(ParkCause::ExplicitPause);
        record.release_mask.insert(ParkCause::AuthRejected);
    }
    supervisor.apply_gate_trigger(
        tp::GateTrigger::ClearMask {
            domain: tp::RetryDomain::Recovery,
            trigger: TriggerClass::ExplicitResume,
        },
        5,
    );
    assert_eq!(
        supervisor
            .view()
            .recovery_record
            .as_ref()
            .expect("auth cause should keep the record parked")
            .gate,
        tp::RetryGateState::Parked
    );
    supervisor.apply_gate_trigger(
        tp::GateTrigger::ClearMask {
            domain: tp::RetryDomain::Recovery,
            trigger: TriggerClass::SessionActivated,
        },
        6,
    );
    let record = supervisor
        .view()
        .recovery_record
        .as_ref()
        .expect("clearing a park should retain ready work");
    assert_eq!(record.gate, tp::RetryGateState::Ready);
    assert_eq!(record.work_revision, 6);

    supervisor.apply_retry_gate(
        tp::RetryDomain::Recovery,
        tp::RetryGateInput::TerminalFailure,
        6,
    );
    supervisor.apply_retry_gate(
        tp::RetryDomain::Recovery,
        tp::RetryGateInput::RetryableFailure,
        6,
    );

    supervisor.view.cleanup_record = Some(tp::PendingRecord::teardown(7));
    supervisor.apply_retry_gate(
        tp::RetryDomain::Cleanup,
        tp::RetryGateInput::RetryableFailure,
        7,
    );
    supervisor.view.offline_record = Some(tp::PendingRecord::teardown(8));
    supervisor.apply_retry_gate(
        tp::RetryDomain::Offline,
        tp::RetryGateInput::RetryableFailure,
        8,
    );
    assert_eq!(
        supervisor
            .view()
            .cleanup_record
            .as_ref()
            .expect("cleanup record should exist")
            .gate,
        tp::RetryGateState::BackingOff
    );
    assert_eq!(
        supervisor
            .view()
            .offline_record
            .as_ref()
            .expect("offline record should exist")
            .gate,
        tp::RetryGateState::BackingOff
    );
}

#[test]
fn timer_directives_install_and_clear_every_deadline_record() {
    let mut supervisor = ungated();
    let deadline = ms(100);

    for id in [
        tp::TimerId::OfflineCandidate,
        tp::TimerId::ShutdownOverall,
        tp::TimerId::TeardownOverall(tp::TeardownDomain::Cleanup),
        tp::TimerId::TeardownOverall(tp::TeardownDomain::OfflineDisconnect),
        tp::TimerId::BootstrapPhase,
    ] {
        supervisor.push_timer_op(&tp::TimerDirective::Arm {
            id,
            category: tp::TimerCategory::FailureDeadline,
            deadline,
        });
    }
    assert!(supervisor.view().offline_candidate.is_some());
    assert!(supervisor.view().shutdown_deadline.is_some());
    assert!(supervisor.view().cleanup_teardown.is_some());
    assert!(supervisor.view().offline_teardown.is_some());

    for id in [
        tp::TimerId::OfflineCandidate,
        tp::TimerId::ShutdownOverall,
        tp::TimerId::TeardownOverall(tp::TeardownDomain::Cleanup),
        tp::TimerId::TeardownOverall(tp::TeardownDomain::OfflineDisconnect),
        tp::TimerId::FailureBackoff(tp::RetryDomain::Recovery),
        tp::TimerId::BootstrapPhase,
    ] {
        supervisor.push_timer_op(&tp::TimerDirective::Cancel { id });
    }
    assert!(supervisor.view().offline_candidate.is_none());
    assert!(supervisor.view().shutdown_deadline.is_none());
    assert!(supervisor.view().cleanup_teardown.is_none());
    assert!(supervisor.view().offline_teardown.is_none());
}

fn foreground() -> tp::Input {
    tp::Input::AppEnteredForeground {
        observed_background_duration: None,
    }
}

fn reported_foreground(background_duration_ms: u64) -> tp::Input {
    tp::Input::AppEnteredForeground {
        observed_background_duration: Some(ms(background_duration_ms)),
    }
}

fn snapshot(epoch: u64, sequence: u64, path: tp::SemanticPath) -> tp::Input {
    tp::Input::NetworkSnapshot {
        source_epoch: epoch,
        sequence,
        observed_at: Duration::ZERO,
        semantic_path: path,
        route_fingerprint: 0,
    }
}

fn online(epoch: u64, sequence: u64) -> tp::Input {
    snapshot(epoch, sequence, tp::SemanticPath::Online)
}

fn offline(epoch: u64, sequence: u64) -> tp::Input {
    snapshot(epoch, sequence, tp::SemanticPath::Offline)
}

fn has_arm(outcome: &super::AcceptOutcome, key: tp::TimerId) -> bool {
    outcome
        .timer_ops
        .iter()
        .any(|op| matches!(op, super::TimerOp::Arm { key: k, .. } if *k == key))
}

// -- invariant 1: snapshot acceptance ordering ------------------------------

#[test]
fn inv1_duplicate_and_stale_snapshots_cannot_move_path_state() {
    let mut s = ungated();
    assert!(s.accept(online(1, 2), ms(0)).advanced);
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);

    // A lower sequence in the same epoch is stale and discarded.
    let stale = s.accept(offline(1, 1), ms(1));
    assert!(!stale.advanced);
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);

    // A newer sequence with identical semantics is a structural duplicate: it
    // records the sequence but advances no revision and rearms no candidate.
    let dup = s.accept(online(1, 3), ms(2));
    assert!(!dup.advanced);
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
    assert_eq!(s.view().last_snapshot.unwrap().sequence, 3);

    // A newer epoch begins a fresh monotonic sequence and is accepted.
    let reincarnate = s.accept(offline(2, 1), ms(3));
    assert!(reincarnate.advanced);
    assert_eq!(
        s.view().network_path,
        tp::NetworkPathState::OfflineCandidate
    );
}

// -- offline hysteresis (invariants 2, 3) -----------------------------------

#[test]
fn inv3_offline_candidate_arms_grace_and_permits_existing_traffic() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    let out = s.accept(offline(1, 2), ms(10));

    assert_eq!(
        s.view().network_path,
        tp::NetworkPathState::OfflineCandidate
    );
    assert!(has_arm(&out, tp::TimerId::OfflineCandidate));
    // A candidate allows existing-session traffic but no new negotiation and no
    // teardown barrier.
    assert_eq!(s.send_policy(), tp::SendProjection::ExistingOnly);
    assert_eq!(s.composite_action(), None);
}

#[test]
fn inv29_offline_grace_expiry_commits_and_derives_confirmed_disconnect() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    s.accept(offline(1, 2), ms(10));
    let candidate = s.view().offline_candidate.unwrap().candidate_id;

    let out = s.accept(
        tp::Input::OfflineGraceExpired {
            candidate_id: candidate,
        },
        ms(410),
    );
    assert!(out.advanced);
    // `DisconnectPending` implies `NetworkPath == Offline` at reconciliation
    // entry (invariant 29): both land in the same accepted step.
    assert_eq!(s.view().network_path, tp::NetworkPathState::Offline);
    assert_eq!(
        s.view().offline_work,
        tp::OfflineWorkState::DisconnectPending
    );
    assert_eq!(
        s.composite_action(),
        Some(tp::Action::ConfirmedOfflineDisconnect)
    );
    assert_eq!(s.send_policy(), tp::SendProjection::Blocked);
    // The offline-disconnect obligation arms one overall teardown deadline.
    assert!(has_arm(
        &out,
        tp::TimerId::TeardownOverall(tp::TeardownDomain::OfflineDisconnect)
    ));
}

#[test]
fn inv2_online_cancels_a_running_confirmed_disconnect_and_derives_restoration() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    s.accept(offline(1, 2), ms(10));
    let candidate = s.view().offline_candidate.unwrap().candidate_id;
    s.accept(
        tp::Input::OfflineGraceExpired {
            candidate_id: candidate,
        },
        ms(410),
    );
    let started = s
        .maybe_start_effect(ms(411))
        .expect("confirmed disconnect should start");
    assert_eq!(s.view().execution, tp::ExecutionState::Disconnecting);

    // A rollback while the disconnect effect is already running must cancel
    // it at its next commit boundary rather than let it complete, and derive
    // immediate restoration since no recovery is otherwise pending.
    let out = s.accept(online(1, 3), ms(420));
    assert!(out.cancel_effect);
    assert_eq!(
        s.view().effect.as_ref().unwrap().cancel_reason,
        Some(tp::CancelReason::PathRecovered)
    );
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );

    // The disconnect effect's own (now-stale) completion must not undo the
    // rollback or re-arm anything: its captured revision is not newer than
    // the offline record, so the offline-work extinguishment still applies,
    // but the network path and recovery intent are unaffected.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::ConfirmedOfflineDisconnect,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Cancelled,
        },
        ms(421),
    );
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
}

#[test]
fn cancelled_offline_disconnect_releases_scope_but_preserves_live_generation() {
    let mut s = ungated();
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 4,
            origin: tp::SignalingOrigin::External,
        },
        ms(0),
    );
    s.accept(online(1, 1), ms(1));
    let initial_probe = s
        .maybe_start_effect(ms(2))
        .expect("initial online snapshot should probe the live generation");
    assert_eq!(initial_probe.kind, EffectKind::Probe);
    s.accept(
        tp::Input::EffectCompleted {
            action_id: initial_probe.action_id,
            kind: initial_probe.kind,
            policy_revision: initial_probe.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(3),
    );

    s.accept(offline(1, 2), ms(10));
    let candidate = s.view().offline_candidate.unwrap().candidate_id;
    s.accept(
        tp::Input::OfflineGraceExpired {
            candidate_id: candidate,
        },
        ms(410),
    );
    let disconnect = s
        .maybe_start_effect(ms(411))
        .expect("confirmed disconnect should start");
    assert_eq!(disconnect.kind, EffectKind::ConfirmedOfflineDisconnect);
    assert!(
        s.view()
            .teardown_scopes
            .contains_in(tp::TeardownDomain::OfflineDisconnect, &4)
    );

    let online = s.accept(online(1, 3), ms(420));
    assert!(online.cancel_effect);
    s.accept(
        tp::Input::EffectCompleted {
            action_id: disconnect.action_id,
            kind: disconnect.kind,
            policy_revision: disconnect.captured_revision,
            outcome: EffectOutcome::Cancelled,
        },
        ms(421),
    );

    assert!(
        !s.view()
            .teardown_scopes
            .is_active(tp::TeardownDomain::OfflineDisconnect),
        "a superseded offline effect must release its teardown ownership"
    );
    assert_eq!(
        s.view().live_signaling_generation,
        Some(4),
        "cancellation does not prove that the healthy generation was removed"
    );

    // The path reversal conservatively retained Restore while cancellation was
    // unresolved. A later session fact supersedes it and must not recreate
    // Restore from a stale teardown scope.
    s.accept(
        tp::Input::SessionActivated {
            session_generation: 1,
        },
        ms(422),
    );
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert_eq!(s.composite_action(), None);
}

#[test]
fn inv2_online_before_grace_rolls_back_without_disconnect() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    s.accept(offline(1, 2), ms(10));
    let candidate = s.view().offline_candidate.unwrap().candidate_id;

    let out = s.accept(online(1, 3), ms(100));
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
    assert!(s.view().offline_candidate.is_none());
    assert_eq!(s.view().offline_work, tp::OfflineWorkState::Idle);
    // The candidate timer is cancelled by the rollback.
    assert!(out.timer_ops.iter().any(|op| matches!(
        op,
        super::TimerOp::Cancel {
            key: tp::TimerId::OfflineCandidate
        }
    )));

    // A stale expiry for the abandoned candidate is inert.
    let stale = s.accept(
        tp::Input::OfflineGraceExpired {
            candidate_id: candidate,
        },
        ms(410),
    );
    assert!(!stale.advanced);
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
}

// -- mode gating (invariant 5) ----------------------------------------------

#[test]
fn inv5_logout_gates_recovery_and_is_not_reactivated_by_network_facts() {
    let mut s = ungated();
    s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::UserLogout,
        },
        ms(0),
    );
    assert_eq!(s.view().recovery_mode, tp::RecoveryModeState::LoggedOut);
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);

    // A later online fact cannot reactivate recovery.
    s.accept(online(1, 5), ms(10));
    assert_eq!(s.view().recovery_mode, tp::RecoveryModeState::LoggedOut);
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    // Cleanup remains the only admissible action.
    assert_eq!(s.composite_action(), Some(tp::Action::Cleanup));
    assert_eq!(s.send_policy(), tp::SendProjection::Blocked);
}

// -- duplicate foreground (invariant 6) -------------------------------------

#[test]
fn inv6_cold_and_duplicate_foreground_create_no_recovery_work() {
    let mut s = ungated();
    // First authoritative phase from Unknown does not fabricate recovery work.
    s.accept(foreground(), ms(0));
    assert_eq!(s.view().app_phase, tp::AppPhaseState::Foreground);
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);

    // A duplicate foreground is a self-loop.
    let dup = s.accept(foreground(), ms(1));
    assert!(!dup.advanced);
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
}

#[test]
fn long_background_reconnect_does_not_requeue_itself_on_explicit_disconnect() {
    let mut g = gated();
    g.accept(foreground(), ms(0));
    g.accept(online(1, 1), ms(1));
    g.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        ms(2),
    );
    g.accept(tp::Input::AppEnteredBackground, ms(3));

    g.accept(reported_foreground(60_000), ms(60_003));
    let started = g
        .maybe_start_effect(ms(60_003))
        .expect("long background should start reconnect");
    assert_eq!(started.kind, EffectKind::Reconnect);

    // ForceReconnect intentionally disconnects the live signaling generation
    // before opening a replacement. That disconnect is output covered by this
    // effect, not a new external recovery obligation.
    g.accept(
        tp::Input::SignalingGenerationLost {
            generation: 7,
            cause: tp::SignalingLostCause::Disconnected,
        },
        ms(60_004),
    );
    assert_eq!(g.view().live_signaling_generation, None);
    assert_eq!(
        g.view()
            .recovery_record
            .as_ref()
            .expect("reconnect record should remain pending")
            .work_revision,
        started.captured_revision,
        "covered disconnect must not refresh the reconnect obligation"
    );
    g.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 9,
            origin: tp::SignalingOrigin::External,
        },
        ms(60_005),
    );
    g.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: started.kind,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(60_006),
    );

    assert_eq!(g.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert_eq!(g.composite_action(), None);
    assert!(g.maybe_start_effect(ms(60_006)).is_none());
}

#[test]
fn long_foreground_must_preempt_running_restore_before_starting_reconnect() {
    let mut s = ungated();
    s.accept(foreground(), ms(0));
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        ms(1),
    );
    s.accept(tp::Input::AppEnteredBackground, ms(2));

    // A signaling loss while the current production supervisor is Ungated
    // starts Restore even though the app phase is Background.
    s.accept(
        tp::Input::SignalingGenerationLost {
            generation: 7,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        ms(3),
    );
    let restore = s
        .maybe_start_effect(ms(3))
        .expect("signaling loss should start restore under the Ungated profile");
    assert_eq!(restore.kind, EffectKind::Restore);

    // Returning after the long-background threshold upgrades the pending intent
    // to Reconnect. The running Restore must be preempted here; otherwise it can
    // publish a short-lived signaling connection before Reconnect tears it down.
    let foreground = s.accept(reported_foreground(65_000), ms(65_002));
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::ReconnectPending
    );
    assert!(
        foreground.cancel_effect,
        "long foreground must preempt Restore before starting Reconnect"
    );

    // Even if the superseded Restore races with cancellation and reports a stale
    // success, it cannot acknowledge the newer Reconnect obligation. Releasing
    // the execution slot must preserve and start the stronger action.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: restore.action_id,
            kind: restore.kind,
            policy_revision: restore.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(65_003),
    );
    let reconnect = s
        .maybe_start_effect(ms(65_003))
        .expect("Reconnect intent should survive the stale Restore completion");
    assert_eq!(reconnect.kind, EffectKind::Reconnect);
}

#[test]
fn short_foreground_does_not_requeue_running_restore() {
    let mut s = ungated();
    s.accept(foreground(), ms(0));
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        ms(1),
    );
    s.accept(
        tp::Input::SignalingGenerationLost {
            generation: 7,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        ms(2),
    );
    let restore = s
        .maybe_start_effect(ms(2))
        .expect("signaling loss should start restore");
    assert_eq!(restore.kind, EffectKind::Restore);
    let work_revision = s
        .view()
        .recovery_record
        .as_ref()
        .expect("restore record should exist")
        .work_revision;

    s.accept(tp::Input::AppEnteredBackground, ms(3));
    let foreground = s.accept(reported_foreground(1), ms(4));

    assert!(!foreground.cancel_effect);
    assert_eq!(s.view().execution, tp::ExecutionState::Restoring);
    assert_eq!(
        s.view()
            .recovery_record
            .as_ref()
            .expect("restore record should remain pending")
            .work_revision,
        work_revision,
        "short foreground must not replace the in-flight Restore obligation"
    );

    s.accept(
        tp::Input::EffectCompleted {
            action_id: restore.action_id,
            kind: restore.kind,
            policy_revision: restore.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(5),
    );

    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert_eq!(s.composite_action(), None);
    assert!(s.maybe_start_effect(ms(5)).is_none());
}

// -- cleanup supersession (invariants 4, 31) --------------------------------

#[test]
fn inv4_cleanup_supersedes_pending_recovery_by_revision() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Reconnect,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(0),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::ReconnectPending
    );

    s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(1),
    );
    // Recovery intent is superseded; cleanup owns the slot.
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);
    assert_eq!(s.composite_action(), Some(tp::Action::Cleanup));
}

#[test]
fn inv4_cleanup_completion_cannot_acknowledge_a_later_recovery_fact() {
    let mut s = ungated();
    s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(0),
    );
    let started = s.maybe_start_effect(ms(1)).expect("cleanup should start");
    assert_eq!(s.view().execution, tp::ExecutionState::Cleaning);

    // A recovery fact arrives *after* cleanup has already started running.
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(2),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );

    // Cleanup's own completion must not touch that later recovery fact.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Cleanup,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(3),
    );
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::Idle);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending,
        "cleanup completion must not acknowledge a later recovery fact"
    );
    assert_eq!(s.composite_action(), Some(tp::Action::Restore));
}

// -- session activated during cleanup (invariant 31) ------------------------

#[test]
fn inv31_session_activated_during_cleanup_derives_a_post_cleanup_obligation() {
    let mut s = ungated();
    s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(0),
    );
    let started = s.maybe_start_effect(ms(1)).expect("cleanup should start");
    let cleanup_revision = started.captured_revision;

    // A new session commits authoritatively while the older cleanup is still
    // running.
    s.accept(
        tp::Input::SessionActivated {
            session_generation: 1,
        },
        ms(2),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    let recovery_record = s
        .view()
        .recovery_record
        .clone()
        .expect("a new recovery obligation should be derived");
    assert!(
        recovery_record.work_revision > cleanup_revision,
        "the new session's obligation must be recorded at a revision after \
         the running cleanup's own captured revision"
    );

    // The older cleanup's own completion must not extinguish that
    // post-cleanup obligation.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Cleanup,
            policy_revision: cleanup_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(3),
    );
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::Idle);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    assert_eq!(s.composite_action(), Some(tp::Action::Restore));
}

#[test]
fn inv31_session_activated_while_cleanup_waits_for_cancel_derives_restore() {
    let mut s = ungated();
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        ms(0),
    );
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let probe = s.maybe_start_effect(ms(2)).expect("probe should start");

    let cleanup = s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(3),
    );
    assert!(cleanup.cancel_effect);
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);
    assert!(
        s.view()
            .teardown_scopes
            .contains_in(tp::TeardownDomain::Cleanup, &7),
        "cleanup must own the live generation before the cancelled probe releases the slot"
    );

    // A connect attempt can cross its commit boundary before cooperative
    // cancellation is observed. The queued fact must join the already-pending
    // cleanup scope rather than appearing healthy to SessionActivated.
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 8,
            origin: tp::SignalingOrigin::External,
        },
        ms(4),
    );
    assert!(
        s.view()
            .teardown_scopes
            .contains_in(tp::TeardownDomain::Cleanup, &8),
        "cleanup must defensively own a newer generation committed at the fence"
    );

    s.accept(
        tp::Input::SessionActivated {
            session_generation: 1,
        },
        ms(5),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending,
        "a new session accepted while cleanup is waiting must survive as post-cleanup restore work"
    );

    s.accept(
        tp::Input::EffectCompleted {
            action_id: probe.action_id,
            kind: EffectKind::Probe,
            policy_revision: probe.captured_revision,
            outcome: EffectOutcome::Cancelled,
        },
        ms(6),
    );
    let cleanup = s
        .maybe_start_effect(ms(7))
        .expect("cleanup should start after probe cancellation");
    assert_eq!(cleanup.kind, EffectKind::Cleanup);

    s.accept(
        tp::Input::EffectCompleted {
            action_id: cleanup.action_id,
            kind: EffectKind::Cleanup,
            policy_revision: cleanup.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(8),
    );
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::Idle);
    assert_eq!(s.view().live_signaling_generation, None);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    assert_eq!(s.composite_action(), Some(tp::Action::Restore));
}

// -- completion procedure: success acknowledgement --------------------------

#[test]
fn probe_success_acknowledges_recovery_intent() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");
    assert_eq!(started.kind, EffectKind::Probe);
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);

    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(2),
    );
    assert_eq!(s.view().execution, tp::ExecutionState::Idle);
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert!(s.view().recovery_record.is_none());
}

// -- completion procedure: stale completion (invariant 9) -------------------

#[test]
fn inv9_stale_completion_cannot_acknowledge_work() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");

    // Wrong action id: discarded, execution unchanged.
    let out = s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id + 99,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(2),
    );
    assert!(!out.advanced);
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::ProbePending
    );
}

#[test]
fn inv8_execution_is_single_flight_while_supervisor_stays_responsive() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);

    // The supervisor keeps accepting and advancing its view while the probe
    // runs (it stays responsive)...
    let out = s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(3),
    );
    assert!(out.advanced);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    // ...but a second effect never starts while one is already executing: the
    // slot is single-flight lifecycle-wide.
    assert!(s.maybe_start_effect(ms(3)).is_none());
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);
    assert_eq!(
        s.view().effect.as_ref().unwrap().action_id,
        started.action_id
    );
}

#[test]
fn inv9_weaker_effect_completion_cannot_acknowledge_stronger_intent() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");

    // The intent is escalated to a stronger ask while the (weaker) probe is
    // still executing; single-flight keeps the same probe running.
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(3),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);

    // The original (weaker) probe now completes successfully with its own
    // captured action id, kind, and revision — it must not acknowledge the
    // newer, stronger `RestorePending` intent it was never running for.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(4),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending,
        "a weaker completed effect must not acknowledge a newer, stronger intent"
    );
    assert_eq!(s.composite_action(), Some(tp::Action::Restore));
}

// -- retry gating (invariant 10) --------------------------------------------

#[test]
fn peer_only_recovery_absorbs_signaling_coverage_without_replacing_backoff() {
    let mut supervisor = ungated();
    let mut record = tp::PendingRecord::recovery_with_impacts(
        17,
        tp::RecoveryStrength::Restore,
        tp::RecoveryImpacts::PEERS,
    );
    record.gate = tp::RetryGateState::BackingOff;
    record.attempt = 3;
    record.retry_id = 29;
    record.retry_deadline = Some(ms(900));
    supervisor.view.recovery_intent = tp::RecoveryIntentState::RestorePending;
    supervisor.view.recovery_record = Some(record);

    let outcome = supervisor.accept(
        tp::Input::SignalingConnectionRequired {
            cause: tp::SignalingConnectionRequiredCause::HeartbeatObservedDisconnected,
        },
        ms(100),
    );
    let record = supervisor
        .view()
        .recovery_record
        .as_ref()
        .expect("the shared recovery obligation must remain");

    assert!(!outcome.advanced);
    assert!(record.required_impacts.contains(tp::RecoveryImpact::Peers));
    assert!(
        record
            .required_impacts
            .contains(tp::RecoveryImpact::Signaling)
    );
    assert_eq!(record.work_revision, 17);
    assert_eq!(record.gate, tp::RetryGateState::BackingOff);
    assert_eq!(record.attempt, 3);
    assert_eq!(record.retry_id, 29);
    assert_eq!(record.retry_deadline, Some(ms(900)));
    assert!(outcome.timer_ops.is_empty());
}

#[test]
fn signaling_retry_window_cools_down_without_losing_work_and_material_news_wakes_it() {
    let mut s = ungated();
    let configured = s.accept(
        tp::Input::SignalingRecoveryConfigChanged {
            max_attempts: 2,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(3),
            backoff_multiplier_bits: 1.0_f64.to_bits(),
        },
        Duration::ZERO,
    );
    assert!(!configured.advanced, "config is not lifecycle work");
    assert_eq!(
        s.config.signaling_retry_window,
        Some(tp::SignalingRetryWindow {
            max_attempts: 2,
            cooldown: Duration::from_secs(6),
        })
    );
    assert_eq!(s.config.backoff.restore.base_delay, Duration::from_secs(1));
    assert_eq!(s.config.backoff.restore.max_delay, Duration::from_secs(3));
    assert_eq!(s.config.backoff.restore.multiplier, 1.0);

    s.accept(
        tp::Input::SignalingConnectionRequired {
            cause: tp::SignalingConnectionRequiredCause::HeartbeatObservedDisconnected,
        },
        Duration::ZERO,
    );
    let first = s
        .maybe_start_effect(Duration::ZERO)
        .expect("signaling demand should start a restore");
    assert_eq!(first.kind, EffectKind::Restore);
    s.accept(
        tp::Input::EffectCompleted {
            action_id: first.action_id,
            kind: first.kind,
            policy_revision: first.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "signaling".into(),
                },
            },
        },
        Duration::ZERO,
    );
    let first_backoff = s.view().recovery_record.clone().expect("work retained");
    assert_eq!(first_backoff.attempt, 1);
    assert_eq!(first_backoff.retry_delay_kind, tp::RetryDelayKind::Backoff);
    let first_deadline = first_backoff.retry_deadline.expect("backoff is armed");

    s.accept(
        tp::Input::RetryDeadlineExpired {
            domain: tp::RetryDomain::Recovery,
            work_revision: first_backoff.work_revision,
            retry_id: first_backoff.retry_id,
        },
        first_deadline,
    );
    let second = s
        .maybe_start_effect(first_deadline)
        .expect("retry deadline should re-arm the same obligation");
    s.accept(
        tp::Input::EffectCompleted {
            action_id: second.action_id,
            kind: second.kind,
            policy_revision: second.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "signaling".into(),
                },
            },
        },
        first_deadline,
    );
    let cooldown = s.view().recovery_record.clone().expect("work retained");
    assert_eq!(cooldown.gate, tp::RetryGateState::BackingOff);
    assert_eq!(cooldown.attempt, 2);
    assert_eq!(cooldown.retry_delay_kind, tp::RetryDelayKind::Cooldown);
    assert_eq!(
        cooldown.retry_deadline,
        Some(first_deadline + Duration::from_secs(6)),
        "the exhausted window must remain a pending obligation behind one supervisor cooldown"
    );

    let duplicate = s.accept(
        tp::Input::SignalingConnectionRequired {
            cause: tp::SignalingConnectionRequiredCause::HeartbeatObservedDisconnected,
        },
        first_deadline + Duration::from_secs(1),
    );
    assert!(
        !duplicate.advanced,
        "duplicate demand cannot bypass cooldown"
    );
    assert_eq!(
        s.view().recovery_record.as_ref().unwrap().retry_deadline,
        cooldown.retry_deadline
    );

    s.accept(
        tp::Input::SignalingCredentialPublished { revision: 1 },
        first_deadline + Duration::from_secs(1),
    );
    let woken = s.view().recovery_record.as_ref().expect("work remains");
    assert_eq!(woken.gate, tp::RetryGateState::Ready);
    assert_eq!(woken.attempt, 0);
    assert_eq!(woken.retry_deadline, None);
    assert_eq!(woken.retry_delay_kind, tp::RetryDelayKind::Backoff);
}

#[test]
fn signaling_cooldown_deadline_starts_the_next_attempt_window() {
    let mut s = ungated();
    let mut record = tp::PendingRecord::recovery_with_impacts(
        9,
        tp::RecoveryStrength::Reconnect,
        tp::RecoveryImpacts::SIGNALING,
    );
    record.gate = tp::RetryGateState::BackingOff;
    record.attempt = 4;
    record.retry_id = 12;
    record.retry_deadline = Some(ms(500));
    record.retry_delay_kind = tp::RetryDelayKind::Cooldown;
    s.view.recovery_intent = tp::RecoveryIntentState::ReconnectPending;
    s.view.recovery_record = Some(record);

    let outcome = s.accept(
        tp::Input::RetryDeadlineExpired {
            domain: tp::RetryDomain::Recovery,
            work_revision: 9,
            retry_id: 12,
        },
        ms(500),
    );
    let record = s
        .view()
        .recovery_record
        .as_ref()
        .expect("obligation remains");
    assert!(outcome.advanced);
    assert_eq!(record.gate, tp::RetryGateState::Ready);
    assert_eq!(record.attempt, 0);
    assert_eq!(record.retry_deadline, None);
    assert_eq!(record.retry_delay_kind, tp::RetryDelayKind::Backoff);
}

#[test]
fn inv10_availability_failure_backs_off_and_only_matching_deadline_rearms() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");

    let out = s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "connect".into(),
                },
            },
        },
        ms(2),
    );
    let record = s.view().recovery_record.clone().unwrap();
    assert_eq!(record.gate, tp::RetryGateState::BackingOff);
    assert_eq!(record.attempt, 1);
    assert!(has_arm(
        &out,
        tp::TimerId::FailureBackoff(tp::RetryDomain::Recovery)
    ));
    // Not Ready, so no action is selectable: it cannot hot-loop.
    assert_eq!(s.composite_action(), None);

    // A stale retry deadline (wrong id) is inert.
    let stale = s.accept(
        tp::Input::RetryDeadlineExpired {
            domain: tp::RetryDomain::Recovery,
            work_revision: record.work_revision,
            retry_id: record.retry_id + 7,
        },
        ms(3),
    );
    assert!(!stale.advanced);
    assert_eq!(
        s.view().recovery_record.as_ref().unwrap().gate,
        tp::RetryGateState::BackingOff
    );

    // The matching deadline makes it Ready again.
    s.accept(
        tp::Input::RetryDeadlineExpired {
            domain: tp::RetryDomain::Recovery,
            work_revision: record.work_revision,
            retry_id: record.retry_id,
        },
        ms(4),
    );
    assert_eq!(
        s.view().recovery_record.as_ref().unwrap().gate,
        tp::RetryGateState::Ready
    );
    assert_eq!(s.composite_action(), Some(tp::Action::Probe));
}

// -- escalation retains the causal work revision ----------------------------

#[test]
fn transport_impairment_escalates_probe_to_restore_retaining_revision() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let work_revision = s.view().recovery_record.as_ref().unwrap().work_revision;
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");

    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::TransportImpaired {
                    scopes: vec!["data".into()],
                },
            },
        },
        ms(2),
    );
    assert_eq!(s.view().execution, tp::ExecutionState::Idle);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    let record = s.view().recovery_record.as_ref().unwrap();
    // A fresh Ready gate at the *retained* causal work revision.
    assert_eq!(record.gate, tp::RetryGateState::Ready);
    assert_eq!(record.attempt, 0);
    assert_eq!(record.work_revision, work_revision);
}

// -- route change with live signaling probes, then escalates ----------------

#[test]
fn route_change_with_live_signaling_probes_and_escalates_on_typed_failure() {
    let mut s = ungated();
    // Establish an Online path and a live (externally committed) generation.
    assert!(s.accept(online(1, 1), ms(0)).advanced);
    s.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        ms(1),
    );
    // The initial Online observation derived a Restore (no live generation yet
    // at that revision); let it succeed so the intent returns to Idle.
    let warmup = s.maybe_start_effect(ms(1)).expect("initial restore starts");
    s.accept(
        tp::Input::EffectCompleted {
            action_id: warmup.action_id,
            kind: warmup.kind,
            policy_revision: warmup.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        ms(2),
    );
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);

    // A material Online -> Online route change with the generation still
    // live must derive an active Probe, not wait for an I/O or Pong failure.
    let out = s.accept(
        tp::Input::NetworkSnapshot {
            source_epoch: 1,
            sequence: 2,
            observed_at: ms(3),
            semantic_path: tp::SemanticPath::Online,
            route_fingerprint: 42,
        },
        ms(3),
    );
    assert!(out.advanced);
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::ProbePending
    );
    let probe = s.maybe_start_effect(ms(3)).expect("route probe starts");
    assert_eq!(probe.kind, EffectKind::Probe);

    // The probe effect reports the conclusive liveness observation; the
    // policy — not the effect — derives the Restore successor.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: probe.action_id,
            kind: EffectKind::Probe,
            policy_revision: probe.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::TransportImpaired {
                    scopes: vec!["signaling: pong timeout".into()],
                },
            },
        },
        ms(4),
    );
    assert_eq!(
        s.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );
    let restore = s.maybe_start_effect(ms(4)).expect("restore starts");
    assert_eq!(restore.kind, EffectKind::Restore);
}

// -- precondition failure parks with a mask ---------------------------------

#[test]
fn inv10_auth_rejection_parks_recovery_until_a_clearing_trigger() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("restore should start");

    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Restore,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::AuthRejected {
                    kind: "expired".into(),
                },
            },
        },
        ms(2),
    );
    let record = s.view().recovery_record.as_ref().unwrap();
    assert_eq!(record.gate, tp::RetryGateState::Parked);
    assert!(!record.release_mask.is_empty());
    assert_eq!(s.composite_action(), None);

    // A committed session generation clears the auth park and re-arms recovery.
    s.accept(
        tp::Input::SessionActivated {
            session_generation: 1,
        },
        ms(3),
    );
    assert_eq!(
        s.view().recovery_record.as_ref().unwrap().gate,
        tp::RetryGateState::Ready
    );
}

// -- classification table alone determines the verdict (invariant 25) ------

#[test]
fn inv25_classification_table_alone_determines_the_verdict() {
    // Two structurally distinct diagnoses for the *same* effect kind and
    // attempt produce different verdicts purely from the per-kind
    // classification table in the translation layer. `EffectOutcome::Failed`
    // carries only a typed `diagnosis`, never a verdict, so the effect that
    // reports it never has a say in retry, escalation, park, or abandon.
    let mut availability = ungated();
    availability.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = availability
        .maybe_start_effect(ms(2))
        .expect("probe should start");
    availability.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "connect".into(),
                },
            },
        },
        ms(2),
    );
    assert_eq!(
        availability.view().recovery_record.as_ref().unwrap().gate,
        tp::RetryGateState::BackingOff,
        "an availability-family diagnosis backs off"
    );

    let mut precondition = ungated();
    precondition.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = precondition
        .maybe_start_effect(ms(2))
        .expect("probe should start");
    precondition.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::AuthRejected {
                    kind: "expired".into(),
                },
            },
        },
        ms(2),
    );
    assert_eq!(
        precondition.view().recovery_record.as_ref().unwrap().gate,
        tp::RetryGateState::Parked,
        "a precondition-family diagnosis parks instead"
    );
}

// -- lifecycle profile / bootstrap (invariant 7) ----------------------------

#[test]
fn inv7_gated_profile_denies_eligibility_until_foreground() {
    let mut g = gated();
    assert!(g.bootstrap_arm(ms(0)).is_some());

    g.accept(online(1, 1), ms(0));
    g.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    // Unknown phase under Gated grants no eligibility.
    assert_eq!(g.composite_action(), None);

    // The first authoritative foreground makes recovery eligible and cancels the
    // bootstrap deadline.
    let out = g.accept(foreground(), ms(2));
    assert!(out.timer_ops.iter().any(|op| matches!(
        op,
        super::TimerOp::Cancel {
            key: tp::TimerId::BootstrapPhase
        }
    )));
    assert_eq!(g.composite_action(), Some(tp::Action::Restore));
}

#[test]
fn inv7_gated_bootstrap_expiry_keeps_profile_gated() {
    let mut g = gated();
    let out = g.accept(tp::Input::BootstrapPhaseDeadlineExpired, ms(5_000));
    assert!(
        out.status
            .iter()
            .any(|r| matches!(r, tp::StatusRecord::BootstrapDeadlineElapsed))
    );
    // Still Unknown, still ineligible: eligibility is never granted silently.
    assert_eq!(g.view().app_phase, tp::AppPhaseState::Unknown);
    assert_eq!(g.composite_action(), None);
}

#[test]
fn inv7_gated_profile_background_gates_recovery_but_preserves_intent() {
    let mut g = gated();
    // Foreground once, establishing eligibility over a healthy online path.
    g.accept(foreground(), ms(0));
    g.accept(online(1, 1), ms(1));

    // An explicit reconnect request is retained while still eligible...
    g.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Restore,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(2),
    );
    assert_eq!(
        g.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending
    );

    // ...and entering background re-gates new active recovery exactly like the
    // pre-foreground `Unknown` phase does, without discarding the intent or
    // touching the healthy path.
    g.accept(tp::Input::AppEnteredBackground, ms(3));
    assert_eq!(g.composite_action(), None);
    assert_eq!(
        g.view().recovery_intent,
        tp::RecoveryIntentState::RestorePending,
        "background must preserve a pending recovery intent, not discard it"
    );
    assert_eq!(g.view().network_path, tp::NetworkPathState::Online);

    // Foreground derives its own (weaker, observed-duration-based) probe
    // request on top of whatever is already pending; the stronger explicit
    // `RestorePending` must not be downgraded by it.
    g.accept(reported_foreground(1), ms(4));
    assert_eq!(g.composite_action(), Some(tp::Action::Restore));
}

// -- terminating cleanup arms shutdown; send policy blocked -----------------

#[test]
fn inv30_app_terminating_enters_terminating_and_blocks_sends() {
    let mut s = ungated();
    let out = s.accept(tp::Input::ShutdownRequested, ms(0));
    assert_eq!(s.view().recovery_mode, tp::RecoveryModeState::Terminating);
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);
    // Shutdown terminates the supervisor at one overall deadline.
    assert!(has_arm(&out, tp::TimerId::ShutdownOverall));
    assert!(has_arm(
        &out,
        tp::TimerId::TeardownOverall(tp::TeardownDomain::Cleanup)
    ));
    assert_eq!(s.send_policy(), tp::SendProjection::Blocked);

    // `Terminating` also cannot be reactivated by a later network fact
    // (invariant 5's other mode, alongside `LoggedOut`).
    s.accept(online(1, 1), ms(1));
    assert_eq!(s.view().recovery_mode, tp::RecoveryModeState::Terminating);
    assert_eq!(s.view().recovery_intent, tp::RecoveryIntentState::Idle);
    assert_eq!(s.send_policy(), tp::SendProjection::Blocked);
}

#[test]
fn teardown_effect_budget_uses_remaining_deadline_and_documented_fallback() {
    let mut with_deadline = ungated();
    with_deadline.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(100),
    );
    let started = with_deadline
        .maybe_start_effect(ms(2_100))
        .expect("cleanup should start");
    assert_eq!(started.teardown_budget, Some(Duration::from_secs(8)));

    // Defensive fallback: a teardown obligation restored without its recorded
    // deadline still receives the configured ten-second failure bound.
    let mut without_deadline = ungated();
    without_deadline.view.cleanup_work = tp::CleanupWorkState::CleanupPending;
    without_deadline.view.cleanup_record = Some(tp::PendingRecord::teardown(1));
    without_deadline.view.cleanup_teardown = None;
    let fallback = without_deadline
        .maybe_start_effect(ms(500))
        .expect("fallback cleanup should start");
    assert_eq!(fallback.teardown_budget, Some(Duration::from_secs(10)));
}

#[test]
fn network_action_mapping_table_is_exhaustive() {
    let cases = [
        (tp::Action::Cleanup, NetworkRecoveryAction::CleanupOnly),
        (
            tp::Action::ConfirmedOfflineDisconnect,
            NetworkRecoveryAction::Offline,
        ),
        (tp::Action::Reconnect, NetworkRecoveryAction::ForceReconnect),
        (tp::Action::Restore, NetworkRecoveryAction::Restore),
        (tp::Action::Probe, NetworkRecoveryAction::Probe),
    ];
    for (action, expected) in cases {
        assert_eq!(network_action_of(action), expected);
    }
}

#[test]
fn public_event_mapping_table_covers_every_event_arm() {
    let epoch = 7;
    let observed_at = ms(123);
    let flags = NetworkTransportFlags {
        wifi: true,
        cellular: true,
        ethernet: true,
        vpn: true,
        other: true,
    };
    for (availability, semantic_path) in [
        (NetworkAvailability::Unknown, tp::SemanticPath::Unknown),
        (NetworkAvailability::Available, tp::SemanticPath::Online),
        (NetworkAvailability::Unavailable, tp::SemanticPath::Offline),
    ] {
        let event = NetworkEvent::NetworkPathChanged {
            snapshot: NetworkSnapshot {
                sequence: 9,
                availability,
                transport: flags,
                is_expensive: true,
                is_constrained: true,
            },
        };
        assert_eq!(
            event_to_input(&event, epoch, observed_at),
            tp::Input::NetworkSnapshot {
                source_epoch: epoch,
                sequence: 9,
                observed_at,
                semantic_path,
                route_fingerprint: 0b111_1111,
            }
        );
    }

    assert_eq!(
        event_to_input(
            &NetworkEvent::AppLifecycleChanged {
                state: AppLifecycleState::Background,
            },
            epoch,
            observed_at,
        ),
        tp::Input::AppEnteredBackground
    );
    assert_eq!(
        event_to_input(
            &NetworkEvent::AppLifecycleChanged {
                state: AppLifecycleState::Foreground {
                    background_duration_ms: 456,
                },
            },
            epoch,
            observed_at,
        ),
        tp::Input::AppEnteredForeground {
            observed_background_duration: Some(ms(456)),
        }
    );

    for (reason, expected) in [
        (
            CleanupReason::AppTerminating,
            tp::CleanupReason::AppTerminating,
        ),
        (CleanupReason::UserLogout, tp::CleanupReason::UserLogout),
        (
            CleanupReason::StaleConnectionSuspected,
            tp::CleanupReason::StaleConnectionSuspected,
        ),
        (CleanupReason::ManualReset, tp::CleanupReason::ManualReset),
    ] {
        assert_eq!(
            event_to_input(
                &NetworkEvent::CleanupConnections { reason },
                epoch,
                observed_at,
            ),
            tp::Input::CleanupRequested { reason: expected }
        );
    }

    for (reason, expected) in [
        (
            ReconnectReason::NetworkPathChanged,
            tp::RecoveryRequestReason::NetworkPathChanged,
        ),
        (
            ReconnectReason::LongBackground,
            tp::RecoveryRequestReason::LongBackground,
        ),
        (
            ReconnectReason::ProbeFailed,
            tp::RecoveryRequestReason::ProbeFailed,
        ),
        (
            ReconnectReason::ManualReconnect,
            tp::RecoveryRequestReason::ManualReconnect,
        ),
        (
            ReconnectReason::StaleConnectionSuspected,
            tp::RecoveryRequestReason::StaleConnectionSuspected,
        ),
    ] {
        assert_eq!(
            event_to_input(&NetworkEvent::ForceReconnect { reason }, epoch, observed_at,),
            tp::Input::RecoveryRequested {
                minimum: tp::RecoveryStrength::Reconnect,
                reason: expected,
            }
        );
    }
}

#[test]
fn inv28_fact_reversal_extinguishes_only_its_own_domain() {
    let mut s = ungated();
    s.accept(online(1, 1), ms(0));
    s.accept(offline(1, 2), ms(10));
    // An unrelated cleanup obligation is independently pending alongside the
    // offline candidate.
    s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(11),
    );
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);

    // The online reversal extinguishes only the offline-candidate domain it
    // defines...
    s.accept(online(1, 3), ms(20));
    assert!(s.view().offline_candidate.is_none());
    assert_eq!(s.view().network_path, tp::NetworkPathState::Online);
    // ...and must not touch the independently-pending cleanup obligation.
    assert_eq!(s.view().cleanup_work, tp::CleanupWorkState::CleanupPending);
    assert_eq!(s.composite_action(), Some(tp::Action::Cleanup));
}

// -- preemption records a cancel reason (invariant 27 slot release) ---------

#[test]
fn inv27_cleanup_preempts_a_running_probe() {
    let mut s = ungated();
    s.accept(
        tp::Input::RecoveryRequested {
            minimum: tp::RecoveryStrength::Probe,
            reason: tp::RecoveryRequestReason::ManualReconnect,
        },
        ms(1),
    );
    let started = s.maybe_start_effect(ms(2)).expect("probe should start");
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);

    let out = s.accept(
        tp::Input::CleanupRequested {
            reason: tp::CleanupReason::ManualReset,
        },
        ms(2),
    );
    // Cancellation is requested but the execution slot is not directly mutated.
    assert!(out.cancel_effect);
    assert_eq!(s.view().execution, tp::ExecutionState::Probing);
    assert_eq!(
        s.view().effect.as_ref().unwrap().cancel_reason,
        Some(tp::CancelReason::PreemptedByCleanup)
    );

    // The recorded cancellation completes the slot; cleanup then runs.
    s.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: EffectKind::Probe,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Cancelled,
        },
        ms(3),
    );
    assert_eq!(s.view().execution, tp::ExecutionState::Idle);
    assert_eq!(s.composite_action(), Some(tp::Action::Cleanup));
}

fn complete_started_effect(
    supervisor: &mut RecoverySupervisor,
    started: super::StartedEffect,
    now: Duration,
    signaling_generation: &mut u64,
) {
    if matches!(started.kind, EffectKind::Restore | EffectKind::Reconnect) {
        *signaling_generation = signaling_generation.saturating_add(1);
        supervisor.accept(
            tp::Input::SignalingGenerationCommitted {
                generation: *signaling_generation,
                origin: tp::SignalingOrigin::CurrentEffect {
                    action_id: started.action_id,
                },
            },
            now,
        );
    }
    supervisor.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: started.kind,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Succeeded,
        },
        now + ms(1),
    );
}

fn cancel_started_effect(
    supervisor: &mut RecoverySupervisor,
    started: super::StartedEffect,
    now: Duration,
) {
    supervisor.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: started.kind,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Cancelled,
        },
        now,
    );
}

fn next_deterministic(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *seed
}

#[test]
fn deterministic_mobile_event_permutations_eventually_converge_online() {
    const SEEDS: u64 = 128;
    const EVENTS_PER_SEED: u64 = 48;

    for seed_number in 0..SEEDS {
        let mut supervisor = gated();
        let mut seed = seed_number + 1;
        let mut now_ms = 0;
        let mut sequence = 1;
        let mut route_fingerprint = 1;
        let mut current_semantic_path = tp::SemanticPath::Online;
        let mut signaling_generation = 1;
        let mut running = None;
        let mut previous_revision = 0;

        supervisor.accept(reported_foreground(0), ms(now_ms));
        now_ms += 1;
        supervisor.accept(
            tp::Input::NetworkSnapshot {
                source_epoch: 1,
                sequence,
                observed_at: ms(now_ms),
                semantic_path: tp::SemanticPath::Online,
                route_fingerprint,
            },
            ms(now_ms),
        );
        now_ms += 1;
        supervisor.accept(
            tp::Input::SignalingGenerationCommitted {
                generation: signaling_generation,
                origin: tp::SignalingOrigin::External,
            },
            ms(now_ms),
        );
        supervisor.accept(
            tp::Input::SessionActivated {
                session_generation: 1,
            },
            ms(now_ms),
        );

        for _ in 0..EVENTS_PER_SEED {
            now_ms += 1;
            let choice = next_deterministic(&mut seed) % 10;
            let input = match choice {
                0 => tp::Input::AppEnteredBackground,
                1 => reported_foreground(5_000),
                2 => reported_foreground(65_000),
                3 => {
                    sequence += 1;
                    route_fingerprint = route_fingerprint.wrapping_add(1);
                    current_semantic_path = tp::SemanticPath::Online;
                    tp::Input::NetworkSnapshot {
                        source_epoch: 1,
                        sequence,
                        observed_at: ms(now_ms),
                        semantic_path: tp::SemanticPath::Online,
                        route_fingerprint,
                    }
                }
                4 => {
                    sequence += 1;
                    current_semantic_path = tp::SemanticPath::Offline;
                    tp::Input::NetworkSnapshot {
                        source_epoch: 1,
                        sequence,
                        observed_at: ms(now_ms),
                        semantic_path: tp::SemanticPath::Offline,
                        route_fingerprint,
                    }
                }
                5 => tp::Input::SignalingGenerationLost {
                    generation: signaling_generation,
                    cause: tp::SignalingLostCause::RemoteReset,
                },
                6 => tp::Input::RecoveryRequested {
                    minimum: tp::RecoveryStrength::Restore,
                    reason: tp::RecoveryRequestReason::NetworkPathChanged,
                },
                7 => tp::Input::CleanupRequested {
                    reason: tp::CleanupReason::ManualReset,
                },
                8 => tp::Input::NetworkSnapshot {
                    source_epoch: 1,
                    sequence: sequence.saturating_sub(1),
                    observed_at: ms(now_ms),
                    semantic_path: tp::SemanticPath::Offline,
                    route_fingerprint: route_fingerprint.wrapping_add(9),
                },
                _ => tp::Input::NetworkSnapshot {
                    source_epoch: 1,
                    sequence,
                    observed_at: ms(now_ms),
                    semantic_path: current_semantic_path,
                    route_fingerprint,
                },
            };

            let outcome = supervisor.accept(input, ms(now_ms));
            assert!(
                supervisor.view().policy_revision >= previous_revision,
                "seed {seed_number} moved policy revision backwards"
            );
            previous_revision = supervisor.view().policy_revision;

            if outcome.cancel_effect
                && let Some(started) = running.take()
            {
                now_ms += 1;
                cancel_started_effect(&mut supervisor, started, ms(now_ms));
            }
            if running.is_none() {
                running = supervisor.maybe_start_effect(ms(now_ms));
            }
            if next_deterministic(&mut seed).is_multiple_of(3)
                && let Some(started) = running.take()
            {
                now_ms += 1;
                complete_started_effect(
                    &mut supervisor,
                    started,
                    ms(now_ms),
                    &mut signaling_generation,
                );
            }
        }

        if let Some(started) = running.take() {
            now_ms += 1;
            complete_started_effect(
                &mut supervisor,
                started,
                ms(now_ms),
                &mut signaling_generation,
            );
        }

        now_ms += 1;
        supervisor.accept(reported_foreground(5_000), ms(now_ms));
        sequence += 1;
        route_fingerprint = route_fingerprint.wrapping_add(1);
        now_ms += 1;
        supervisor.accept(
            tp::Input::NetworkSnapshot {
                source_epoch: 1,
                sequence,
                observed_at: ms(now_ms),
                semantic_path: tp::SemanticPath::Online,
                route_fingerprint,
            },
            ms(now_ms),
        );
        now_ms += 1;
        supervisor.accept(
            tp::Input::SessionActivated {
                session_generation: seed_number + 2,
            },
            ms(now_ms),
        );

        for _ in 0..32 {
            let Some(started) = supervisor.maybe_start_effect(ms(now_ms)) else {
                break;
            };
            now_ms += 1;
            complete_started_effect(
                &mut supervisor,
                started,
                ms(now_ms),
                &mut signaling_generation,
            );
        }

        assert_eq!(
            supervisor.view().app_phase,
            tp::AppPhaseState::Foreground,
            "seed {seed_number} did not finish foreground"
        );
        assert_eq!(
            supervisor.view().network_path,
            tp::NetworkPathState::Online,
            "seed {seed_number} did not finish online"
        );
        assert_eq!(
            supervisor.view().execution,
            tp::ExecutionState::Idle,
            "seed {seed_number} leaked an executing effect"
        );
        assert_eq!(
            supervisor.composite_action(),
            None,
            "seed {seed_number} retained recovery work: {:?}",
            supervisor.view()
        );
    }
}

#[test]
fn entropy_seeds_differ_within_a_process() {
    // The Weyl step guarantees distinct seeds for successive supervisors in
    // one process, regardless of what base the OS entropy produced.
    let first = super::next_entropy_seed();
    let second = super::next_entropy_seed();
    assert_ne!(first, second, "consecutive supervisor seeds must differ");
}

#[test]
fn public_cleanup_reasons_map_exactly_to_policy_reasons() {
    use crate::lifecycle::{CleanupReason, NetworkEvent};

    let cases = [
        (
            CleanupReason::AppTerminating,
            tp::CleanupReason::AppTerminating,
        ),
        (CleanupReason::UserLogout, tp::CleanupReason::UserLogout),
        (
            CleanupReason::StaleConnectionSuspected,
            tp::CleanupReason::StaleConnectionSuspected,
        ),
        (CleanupReason::ManualReset, tp::CleanupReason::ManualReset),
    ];

    for (wire_reason, policy_reason) in cases {
        assert_eq!(
            super::event_to_input(
                &NetworkEvent::CleanupConnections {
                    reason: wire_reason,
                },
                7,
                ms(9),
            ),
            tp::Input::CleanupRequested {
                reason: policy_reason,
            }
        );
    }
}

#[test]
fn public_reconnect_reasons_map_exactly_to_policy_reasons() {
    use crate::lifecycle::{NetworkEvent, ReconnectReason};

    let cases = [
        (
            ReconnectReason::NetworkPathChanged,
            tp::RecoveryRequestReason::NetworkPathChanged,
        ),
        (
            ReconnectReason::LongBackground,
            tp::RecoveryRequestReason::LongBackground,
        ),
        (
            ReconnectReason::ProbeFailed,
            tp::RecoveryRequestReason::ProbeFailed,
        ),
        (
            ReconnectReason::ManualReconnect,
            tp::RecoveryRequestReason::ManualReconnect,
        ),
        (
            ReconnectReason::StaleConnectionSuspected,
            tp::RecoveryRequestReason::StaleConnectionSuspected,
        ),
    ];

    for (wire_reason, policy_reason) in cases {
        assert_eq!(
            super::event_to_input(
                &NetworkEvent::ForceReconnect {
                    reason: wire_reason,
                },
                7,
                ms(9),
            ),
            tp::Input::RecoveryRequested {
                minimum: tp::RecoveryStrength::Reconnect,
                reason: policy_reason,
            }
        );
    }
}
