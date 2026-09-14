use super::*;
use crate::lifecycle::CredentialState;
use crate::transport::{NetworkError, NetworkResult};
use crate::wire::webrtc::{SignalingEvent, SignalingStats};
use actr_protocol::{
    AIdCredential, ActrId, Pong, RegisterRequest, RegisterResponse, RouteCandidatesRequest,
    RouteCandidatesResponse, ServiceAvailabilityState, SignalingEnvelope, UnregisterResponse,
};
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use tokio::sync::{Notify, Semaphore, broadcast, mpsc, watch};

#[test]
fn signaling_probe_budget_is_two_seconds() {
    assert_eq!(SIGNALING_PROBE_TIMEOUT, std::time::Duration::from_secs(2));
}

struct ForceReconnectFakeSignalingClient {
    connected: AtomicBool,
    connect_should_fail: bool,
    disconnect_calls: AtomicUsize,
    recovery_disconnect_calls: AtomicUsize,
    connect_calls: AtomicUsize,
    auto_reconnect_suppressed: AtomicBool,
    invalidate_generation_calls: AtomicUsize,
    event_tx: broadcast::Sender<SignalingEvent>,
    /// When set, `disconnect` blocks after signaling `disconnect_entered` and
    /// until `disconnect_release` is notified, so a test can cancel an effect
    /// deterministically while it is mid-teardown.
    block_disconnect: AtomicBool,
    disconnect_entered: Arc<tokio::sync::Notify>,
    disconnect_release: Arc<tokio::sync::Notify>,
}

impl ForceReconnectFakeSignalingClient {
    fn new(connect_should_fail: bool) -> Self {
        let (event_tx, _rx) = broadcast::channel(8);
        Self {
            connected: AtomicBool::new(false),
            connect_should_fail,
            disconnect_calls: AtomicUsize::new(0),
            recovery_disconnect_calls: AtomicUsize::new(0),
            connect_calls: AtomicUsize::new(0),
            auto_reconnect_suppressed: AtomicBool::new(false),
            invalidate_generation_calls: AtomicUsize::new(0),
            event_tx,
            block_disconnect: AtomicBool::new(false),
            disconnect_entered: Arc::new(tokio::sync::Notify::new()),
            disconnect_release: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

#[async_trait::async_trait]
impl SignalingClient for ForceReconnectFakeSignalingClient {
    async fn connect(&self) -> NetworkResult<()> {
        self.connect_calls.fetch_add(1, AtomicOrdering::SeqCst);
        if self.connect_should_fail {
            return Err(NetworkError::ConnectionError(
                "forced connect failure".to_string(),
            ));
        }

        self.connected.store(true, AtomicOrdering::SeqCst);
        Ok(())
    }

    fn invalidate_generation(&self) {
        self.auto_reconnect_suppressed
            .store(true, AtomicOrdering::SeqCst);
        self.invalidate_generation_calls
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    async fn disconnect(&self) -> NetworkResult<()> {
        if self.block_disconnect.load(AtomicOrdering::SeqCst) {
            self.disconnect_entered.notify_one();
            self.disconnect_release.notified().await;
        }
        self.disconnect_calls.fetch_add(1, AtomicOrdering::SeqCst);
        self.auto_reconnect_suppressed
            .store(true, AtomicOrdering::SeqCst);
        self.connected.store(false, AtomicOrdering::SeqCst);
        Ok(())
    }

    async fn disconnect_for_recovery(&self) -> NetworkResult<()> {
        if self.block_disconnect.load(AtomicOrdering::SeqCst) {
            self.disconnect_entered.notify_one();
            self.disconnect_release.notified().await;
        }
        self.recovery_disconnect_calls
            .fetch_add(1, AtomicOrdering::SeqCst);
        self.auto_reconnect_suppressed
            .store(true, AtomicOrdering::SeqCst);
        self.connected.store(false, AtomicOrdering::SeqCst);
        Ok(())
    }

    async fn send_register_request(
        &self,
        _request: RegisterRequest,
    ) -> NetworkResult<RegisterResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_unregister_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _reason: Option<String>,
    ) -> NetworkResult<UnregisterResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_heartbeat(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _availability: ServiceAvailabilityState,
        _power_reserve: f32,
        _mailbox_backlog: f32,
    ) -> NetworkResult<Pong> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_route_candidates_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _request: RouteCandidatesRequest,
    ) -> NetworkResult<RouteCandidatesResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn get_signing_key(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _key_id: u32,
    ) -> NetworkResult<(u32, Vec<u8>)> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_envelope(&self, _envelope: SignalingEnvelope) -> NetworkResult<()> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    fn is_connected(&self) -> bool {
        self.connected.load(AtomicOrdering::SeqCst)
    }

    fn get_stats(&self) -> SignalingStats {
        SignalingStats::default()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent> {
        self.event_tx.subscribe()
    }

    async fn set_actor_id(&self, _actor_id: ActrId) {}

    async fn set_credential_state(&self, _credential_state: CredentialState) {}

    async fn clear_identity(&self) {}
}

fn snapshot(sequence: u64, availability: NetworkAvailability) -> NetworkSnapshot {
    NetworkSnapshot {
        sequence,
        availability,
        transport: NetworkTransportFlags::default(),
        is_expensive: false,
        is_constrained: false,
    }
}

#[tokio::test]
async fn reconciler_publishes_send_projection_after_each_accepted_input() {
    let (event_tx, event_rx) = mpsc::channel(8);
    let (fact_sink, channel) = supervisor_internal_channel();
    let mut send_projection_rx = channel.subscribe_send_projection();
    let handle = NetworkEventHandle::new_with_fact_sink(event_tx, fact_sink);
    let shutdown = CancellationToken::new();
    let processor: Arc<dyn NetworkEventProcessor> = Arc::new(PreemptionFenceProcessor {
        token: CancellationToken::new(),
        fence_calls: AtomicUsize::new(0),
        fence_observed_before_cancel: AtomicBool::new(false),
    });
    let reconciler = tokio::spawn(run_network_event_reconciler_with_channel(
        event_rx,
        channel,
        processor,
        shutdown.clone(),
    ));

    assert_eq!(
        *send_projection_rx.borrow_and_update(),
        tp::SendProjection::Normal
    );

    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Unavailable))
        .await
        .expect("offline observation should be accepted");
    assert_eq!(
        *send_projection_rx.borrow_and_update(),
        tp::SendProjection::ExistingOnly
    );

    handle
        .cleanup_connections(CleanupReason::ManualReset)
        .await
        .expect("cleanup should be accepted");
    assert_eq!(
        *send_projection_rx.borrow_and_update(),
        tp::SendProjection::Blocked
    );

    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(1), reconciler)
        .await
        .expect("reconciler should stop after bounded shutdown")
        .expect("reconciler task should not panic");
}

#[test]
fn restarted_monitor_gets_a_fresh_epoch_while_clones_keep_the_incarnation() {
    let (event_tx, _event_rx) = mpsc::channel(1);
    let first = NetworkEventHandle::new(event_tx);
    let shared = first.clone();
    let restarted = first.new_monitor_incarnation();

    assert_eq!(shared.source_epoch, first.source_epoch);
    assert!(restarted.source_epoch > first.source_epoch);
}

#[tokio::test]
async fn newer_monitor_incarnation_fences_old_lifecycle_and_snapshot_facts() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    let processor: Arc<dyn NetworkEventProcessor> =
        Arc::new(DefaultNetworkEventProcessor::new(signaling.clone()));
    let (event_tx, event_rx) = mpsc::channel(8);
    let (fact_sink, channel) =
        supervisor_internal_channel_with_profile(tp::LifecycleProfile::Gated);
    let mut send_projection_rx = channel.subscribe_send_projection();
    let old_monitor = NetworkEventHandle::new_with_fact_sink(event_tx, fact_sink);
    let current_monitor = old_monitor.new_monitor_incarnation();
    let shutdown = CancellationToken::new();
    let reconciler = tokio::spawn(run_network_event_reconciler_with_channel(
        event_rx,
        channel,
        processor,
        shutdown.clone(),
    ));

    current_monitor
        .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
            background_duration_ms: 0,
        })
        .await
        .expect("the current monitor should establish foreground");

    old_monitor
        .handle_app_lifecycle_changed(AppLifecycleState::Background)
        .await
        .expect("a stale lifecycle callback should be acknowledged as discarded");
    old_monitor
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Unavailable))
        .await
        .expect("a stale path callback should be acknowledged as discarded");
    assert_eq!(
        *send_projection_rx.borrow_and_update(),
        tp::SendProjection::Normal,
        "a lifecycle fact from the new incarnation must also fence old path snapshots"
    );

    current_monitor
        .handle_app_lifecycle_changed(AppLifecycleState::Background)
        .await
        .expect("the current monitor should still be accepted");
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), reconciler)
        .await
        .expect("reconciler should stop")
        .expect("reconciler should not panic");
}

#[test]
fn recovery_timer_ids_keep_their_normative_categories() {
    use crate::timer::TimerCategory;

    let cases = [
        (
            tp::TimerId::OfflineCandidate,
            TimerCategory::BusinessHysteresis,
        ),
        (tp::TimerId::ShutdownOverall, TimerCategory::FailureDeadline),
        (
            tp::TimerId::TeardownOverall(tp::TeardownDomain::Cleanup),
            TimerCategory::FailureDeadline,
        ),
        (
            tp::TimerId::TeardownOverall(tp::TeardownDomain::OfflineDisconnect),
            TimerCategory::FailureDeadline,
        ),
        (tp::TimerId::BootstrapPhase, TimerCategory::FailureDeadline),
        (
            tp::TimerId::FailureBackoff(tp::RetryDomain::Recovery),
            TimerCategory::FailureBackoff,
        ),
        (
            tp::TimerId::FailureBackoff(tp::RetryDomain::Cleanup),
            TimerCategory::FailureBackoff,
        ),
        (
            tp::TimerId::FailureBackoff(tp::RetryDomain::Offline),
            TimerCategory::FailureBackoff,
        ),
    ];

    for (id, expected_category) in cases {
        assert_eq!(recovery_timer_definition(id).category, expected_category);
    }
}

#[test]
fn observed_effect_outcomes_preserve_every_completion_class() {
    let cases = [
        (EffectOutcome::Succeeded, ObservedOutcome::Succeeded),
        (
            EffectOutcome::CompletedWithResiduals {
                residuals: vec![EffectDiagnosis::Timeout {
                    stage: "residual".to_string(),
                }],
            },
            ObservedOutcome::CompletedWithResiduals,
        ),
        (
            EffectOutcome::Abandoned {
                residuals: vec![EffectDiagnosis::Timeout {
                    stage: "abandoned".to_string(),
                }],
            },
            ObservedOutcome::Abandoned,
        ),
        (
            EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::Timeout {
                    stage: "test".to_string(),
                },
            },
            ObservedOutcome::Failed,
        ),
        (EffectOutcome::Cancelled, ObservedOutcome::Cancelled),
        (
            EffectOutcome::Aborted {
                cause: AbortCause::SupervisorCancellation,
            },
            ObservedOutcome::Aborted,
        ),
    ];

    for (outcome, expected) in cases {
        assert_eq!(observed_outcome(&outcome), expected);
    }
}

#[test]
fn retry_failure_diagnostic_requires_the_matching_running_signaling_action() {
    let running = tp::SignalingReconnectState::Running {
        work_revision: 7,
        action_id: 42,
        strength: tp::RecoveryStrength::Restore,
        attempt: 3,
    };
    let matching = tp::Input::EffectCompleted {
        action_id: 42,
        kind: EffectKind::Restore,
        policy_revision: 7,
        outcome: EffectOutcome::Failed {
            diagnosis: EffectDiagnosis::PathUnreachable {
                stage: "signaling".to_string(),
            },
        },
    };
    let unrelated = tp::Input::EffectCompleted {
        action_id: 43,
        kind: EffectKind::Cleanup,
        policy_revision: 8,
        outcome: EffectOutcome::Failed {
            diagnosis: EffectDiagnosis::Timeout {
                stage: "cleanup".to_string(),
            },
        },
    };

    assert!(matching_running_signaling_failure(&running, &matching).is_some());
    assert!(matching_running_signaling_failure(&running, &unrelated).is_none());
    assert!(
        matching_running_signaling_failure(&tp::SignalingReconnectState::Idle, &matching,)
            .is_none()
    );
}

#[test]
fn session_notification_without_fact_sink_is_a_safe_noop() {
    let (event_tx, _event_rx) = mpsc::channel(1);
    let handle = NetworkEventHandle::new(event_tx);

    handle.notify_session_activated(17);
}

#[test]
fn recovery_effect_errors_preserve_typed_network_diagnoses() {
    let cases = [
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::ConnectionError("offline".to_string()),
            ),
            EffectDiagnosis::PathUnreachable {
                stage: "connect: Connection error: offline".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "probe",
                NetworkError::TimeoutError("deadline".to_string()),
            ),
            EffectDiagnosis::Timeout {
                stage: "probe: Timeout error: deadline".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::ResourceExhaustedError("sockets".to_string()),
            ),
            EffectDiagnosis::ResourceExhausted {
                resource: "connect: Resource exhausted: sockets".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::AuthenticationError("revoked".to_string()),
            ),
            EffectDiagnosis::AuthRejected {
                kind: "connect: Authentication error: revoked".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::ConfigurationError("endpoint".to_string()),
            ),
            EffectDiagnosis::ConfigRejected {
                detail: "connect: Configuration error: endpoint".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "decode",
                NetworkError::DeserializationError("invalid frame".to_string()),
            ),
            EffectDiagnosis::InvariantViolation {
                detail: "decode: Deserialization error: invalid frame".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::signaling_server_not_ready("signaling returned 503", None),
            ),
            EffectDiagnosis::PathUnreachable {
                stage: "connect: Other error: signaling returned 503".to_string(),
            },
        ),
        (
            NetworkRecoveryError::from_network_error(
                "connect",
                NetworkError::signaling_server_not_ready(
                    "signaling returned 503",
                    Some(Duration::from_secs(7)),
                ),
            ),
            EffectDiagnosis::Overloaded {
                retry_after: Duration::from_secs(7),
            },
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.into_diagnosis(), expected);
    }
}

#[test]
fn signaling_directive_only_fences_superseded_connection_attempts() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    processor.invalidate_signaling_connection_attempts();
    assert_eq!(
        signaling
            .invalidate_generation_calls
            .load(AtomicOrdering::SeqCst),
        1,
        "InvalidateConnectionAttempts must fence the superseded attempt"
    );
    assert!(
        signaling
            .auto_reconnect_suppressed
            .load(AtomicOrdering::SeqCst),
        "the fake records that the generation fence was applied"
    );
}

struct PreemptionFenceProcessor {
    token: CancellationToken,
    fence_calls: AtomicUsize,
    fence_observed_before_cancel: AtomicBool,
}

#[async_trait::async_trait]
impl NetworkEventProcessor for PreemptionFenceProcessor {
    fn invalidate_signaling_connection_attempts(&self) {
        self.fence_observed_before_cancel
            .store(!self.token.is_cancelled(), AtomicOrdering::SeqCst);
        self.fence_calls.fetch_add(1, AtomicOrdering::SeqCst);
    }

    async fn process_network_available(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_type_changed(
        &self,
        _is_wifi: bool,
        _is_cellular: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn stronger_reconnect_fences_restore_before_requesting_cancellation() {
    let mut supervisor = RecoverySupervisor::new(tp::LifecycleProfile::Ungated);
    supervisor.accept(
        tp::Input::AppEnteredForeground {
            observed_background_duration: None,
        },
        Duration::ZERO,
    );
    supervisor.accept(
        tp::Input::SignalingGenerationCommitted {
            generation: 7,
            origin: tp::SignalingOrigin::External,
        },
        Duration::from_millis(1),
    );
    supervisor.accept(tp::Input::AppEnteredBackground, Duration::from_millis(2));
    supervisor.accept(
        tp::Input::SignalingGenerationLost {
            generation: 7,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        Duration::from_millis(3),
    );
    let started = supervisor
        .maybe_start_effect(Duration::from_millis(3))
        .expect("signaling loss should start Restore");
    assert_eq!(
        started.kind,
        crate::lifecycle::recovery_policy::diagnosis::EffectKind::Restore
    );

    let token = CancellationToken::new();
    let recorder = Arc::new(PreemptionFenceProcessor {
        token: token.clone(),
        fence_calls: AtomicUsize::new(0),
        fence_observed_before_cancel: AtomicBool::new(false),
    });
    let processor: Arc<dyn NetworkEventProcessor> = recorder.clone();
    let mut effect = Some(RunningEffect {
        action_id: started.action_id,
        token: token.clone(),
    });
    let (internal_tx, _internal_rx) = mpsc::unbounded_channel();
    let (status_tx, _status_rx) = watch::channel(SupervisorStatus::default());
    let (send_projection_tx, _send_projection_rx) = watch::channel(tp::SendProjection::Normal);
    let mut timers = HashMap::new();
    let mut last_action_id = Some(started.action_id);
    let mut last_outcome = None;

    let terminate = handle_supervisor_input(
        &mut supervisor,
        tp::Input::AppEnteredForeground {
            observed_background_duration: Some(Duration::from_secs(65)),
        },
        Duration::from_secs(65),
        tokio::time::Instant::now(),
        &internal_tx,
        &mut timers,
        &mut effect,
        &processor,
        &status_tx,
        &mut last_action_id,
        &mut last_outcome,
        &send_projection_tx,
    );

    assert!(!terminate);
    assert_eq!(recorder.fence_calls.load(AtomicOrdering::SeqCst), 1);
    assert!(
        recorder
            .fence_observed_before_cancel
            .load(AtomicOrdering::SeqCst),
        "the old Restore must lose commit rights before cancellation is requested"
    );
    assert!(token.is_cancelled(), "the old Restore must be cancelled");
}

#[tokio::test]
async fn cancelled_recovery_action_does_not_poison_later_actions() {
    // Regression for the processor-side execution tracker that poisoned itself
    // when the supervisor cancelled an effect after `begin` but before
    // `complete` (a dropped future never runs `complete`). The tracker was
    // removed; the supervisor's `view.execution` is the sole single-flight
    // owner, so a cancelled action can no longer wedge later ones.
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    signaling
        .block_disconnect
        .store(true, AtomicOrdering::SeqCst);
    let processor: Arc<dyn NetworkEventProcessor> =
        Arc::new(DefaultNetworkEventProcessor::new(signaling.clone()));

    // Run a ForceReconnect effect and cancel it once it is mid-teardown (blocked
    // in disconnect) but before it can complete — exactly what the reconciler
    // does when a stronger policy preempts a running effect.
    let entered = signaling.disconnect_entered.clone();
    let token = tokio_util::sync::CancellationToken::new();
    let task_processor = processor.clone();
    let task_token = token.clone();
    let task = tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = task_token.cancelled() => {}
            _ = task_processor
                .process_network_recovery_action(NetworkRecoveryAction::ForceReconnect) => {}
        }
    });
    entered.notified().await;
    token.cancel();
    task.await.expect("cancelled effect task joins");

    // A later action still begins and completes normally.
    signaling
        .block_disconnect
        .store(false, AtomicOrdering::SeqCst);
    let result = processor
        .process_network_recovery_action(NetworkRecoveryAction::Restore)
        .await;
    assert!(
        result.is_ok(),
        "a cancelled action must not poison later actions: {result:?}"
    );
}

#[tokio::test]
async fn force_reconnect_does_not_delegate_retry_ownership_to_socket() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    processor
        .force_reconnect()
        .await
        .expect("ForceReconnect should restore signaling");
}

#[tokio::test]
async fn force_reconnect_failure_reports_without_scheduling_local_retry() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(true));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    let result = processor.force_reconnect().await;

    assert!(result.is_err());
    assert_eq!(
        signaling.disconnect_calls.load(AtomicOrdering::SeqCst),
        0,
        "ForceReconnect must route every signaling teardown through the recovery contract"
    );
    assert!(
        signaling
            .recovery_disconnect_calls
            .load(AtomicOrdering::SeqCst)
            >= 1,
        "ForceReconnect cleanup must drop idempotent waiters through the recovery teardown"
    );
    assert_eq!(
        signaling.connect_calls.load(AtomicOrdering::SeqCst),
        1,
        "ForceReconnect restore should make one quick connect attempt"
    );
}

/// The ForceReconnect teardown must be recovery-aware: an in-flight
/// RouteCandidates/GetSigningKey waiter dropped by the automatic rebuild
/// replays once on the replacement connection instead of failing fast the
/// way a manual teardown does.
#[tokio::test]
async fn force_reconnect_cleanup_uses_recovery_aware_signaling_teardown() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    processor
        .force_reconnect()
        .await
        .expect("ForceReconnect should restore signaling");

    assert_eq!(
        signaling.disconnect_calls.load(AtomicOrdering::SeqCst),
        0,
        "no manual fail-fast signaling teardown may run during a ForceReconnect"
    );
    assert!(
        signaling
            .recovery_disconnect_calls
            .load(AtomicOrdering::SeqCst)
            >= 1,
        "the rebuild cleanup itself must use disconnect_for_recovery"
    );
}

#[tokio::test]
async fn restore_failure_reports_without_resetting_socket_backoff() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(true));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    let result = processor
        .process_network_recovery_action(NetworkRecoveryAction::Restore)
        .await;

    assert!(result.is_err());
    assert_eq!(
        signaling.connect_calls.load(AtomicOrdering::SeqCst),
        1,
        "Restore should make one quick connect attempt"
    );
}

#[tokio::test]
async fn restore_executes_one_quick_connect_without_socket_backoff() {
    let signaling = Arc::new(ForceReconnectFakeSignalingClient::new(false));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    processor
        .process_network_recovery_action(NetworkRecoveryAction::Restore)
        .await
        .expect("Restore should connect successfully");

    assert_eq!(
        signaling.connect_calls.load(AtomicOrdering::SeqCst),
        1,
        "Restore should make one quick connect attempt"
    );
}

#[test]
fn foreground_background_duration_is_clamped_at_ingest() {
    // Absurd values (wall-clock jump in the binding layer) are clamped.
    let absurd = AppLifecycleState::Foreground {
        background_duration_ms: u64::MAX,
    };
    assert_eq!(
        absurd.sanitized(),
        AppLifecycleState::Foreground {
            background_duration_ms: MAX_BACKGROUND_DURATION_MS,
        }
    );

    // Legal values pass through unchanged, including the bound itself.
    let legal = AppLifecycleState::Foreground {
        background_duration_ms: LONG_BACKGROUND_RECONNECT_THRESHOLD_MS,
    };
    assert_eq!(legal.sanitized(), legal);
    let at_bound = AppLifecycleState::Foreground {
        background_duration_ms: MAX_BACKGROUND_DURATION_MS,
    };
    assert_eq!(at_bound.sanitized(), at_bound);
    assert_eq!(
        AppLifecycleState::Background.sanitized(),
        AppLifecycleState::Background
    );
}

#[test]
fn snapshot_is_offline_and_should_restore() {
    let offline = snapshot(1, NetworkAvailability::Unavailable);
    assert!(offline.is_offline());
    assert!(!offline.should_restore());

    let online = snapshot(2, NetworkAvailability::Available);
    assert!(!online.is_offline());
    assert!(online.should_restore());

    // Unknown is neither offline (not Unavailable) nor restorable (not Available).
    let unknown = snapshot(3, NetworkAvailability::Unknown);
    assert!(!unknown.is_offline());
    assert!(!unknown.should_restore());
}

// ---------------------------------------------------------------------------
// S3: fact sink plumbing and bounded teardown
// ---------------------------------------------------------------------------

use crate::lifecycle::recovery_policy::diagnosis::EffectOutcome as TpEffectOutcome;
use crate::lifecycle::recovery_policy::translate as tp;

#[test]
fn fact_sink_delivers_facts_to_internal_channel_in_order() {
    let (sink, mut channel) = super::supervisor_internal_channel();
    sink.signaling_generation_committed(7, SignalingFactOrigin::External);
    sink.session_activated(3);
    sink.signaling_generation_lost(7, SignalingFactLostCause::Disconnected);

    let a = channel.rx.try_recv().expect("committed fact");
    assert!(matches!(
        a,
        tp::Input::SignalingGenerationCommitted { generation: 7, .. }
    ));
    let b = channel.rx.try_recv().expect("session fact");
    assert!(matches!(
        b,
        tp::Input::SessionActivated {
            session_generation: 3
        }
    ));
    let c = channel.rx.try_recv().expect("lost fact");
    assert!(matches!(
        c,
        tp::Input::SignalingGenerationLost { generation: 7, .. }
    ));
}

#[test]
fn fact_sink_preserves_effect_origin_and_loss_cause() {
    let (sink, mut channel) = super::supervisor_internal_channel();
    sink.signaling_generation_committed(11, SignalingFactOrigin::CurrentEffect { action_id: 41 });
    sink.signaling_generation_lost(11, SignalingFactLostCause::Superseded);

    assert_eq!(
        channel.rx.try_recv(),
        Ok(tp::Input::SignalingGenerationCommitted {
            generation: 11,
            origin: tp::SignalingOrigin::CurrentEffect { action_id: 41 },
        })
    );
    assert_eq!(
        channel.rx.try_recv(),
        Ok(tp::Input::SignalingGenerationLost {
            generation: 11,
            cause: tp::SignalingLostCause::Superseded,
        })
    );
}

#[test]
fn fact_sink_is_non_blocking_during_a_burst_or_after_close() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sink = SupervisorFactSink::new(tx);

    sink.session_activated(1);
    sink.session_activated(2);
    assert!(matches!(
        rx.try_recv().expect("first fact should occupy the channel"),
        tp::Input::SessionActivated {
            session_generation: 1
        }
    ));
    assert!(matches!(
        rx.try_recv().expect("second fact should remain queued"),
        tp::Input::SessionActivated {
            session_generation: 2
        }
    ));

    drop(rx);
    sink.session_activated(3);
}

#[test]
fn queued_signaling_config_uses_the_retargeted_mobile_profile() {
    let (sink, mut channel) =
        supervisor_internal_channel_with_profile(tp::LifecycleProfile::Ungated);
    sink.signaling_recovery_configured(10, Duration::from_secs(30), Duration::from_secs(60), 2.0);
    channel.set_profile(tp::LifecycleProfile::Gated);

    let config_input = channel
        .rx
        .try_recv()
        .expect("signaling config should remain queued until the reconciler starts");
    let mut supervisor = RecoverySupervisor::new_with_entropy(
        channel.profile,
        Box::new(crate::lifecycle::recovery_policy::classification::FixedEntropy::constant(0.5)),
    );
    supervisor.accept(config_input, Duration::ZERO);
    supervisor.accept(
        tp::Input::AppEnteredForeground {
            observed_background_duration: Some(Duration::ZERO),
        },
        Duration::ZERO,
    );
    supervisor.accept(
        tp::Input::SignalingConnectionRequired {
            cause: tp::SignalingConnectionRequiredCause::InitialConnectDeferred,
        },
        Duration::ZERO,
    );
    let started = supervisor
        .maybe_start_effect(Duration::ZERO)
        .expect("the retargeted mobile supervisor should start signaling recovery");
    supervisor.accept(
        tp::Input::EffectCompleted {
            action_id: started.action_id,
            kind: started.kind,
            policy_revision: started.captured_revision,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "signaling".to_string(),
                },
            },
        },
        Duration::ZERO,
    );

    match supervisor.signaling_reconnect_state() {
        tp::SignalingReconnectState::BackingOff { deadline, .. } => assert!(
            deadline <= crate::lifecycle::recovery_supervisor::MOBILE_SIGNALING_MAX_BACKOFF,
            "jittered mobile backoff exceeded the cap: {deadline:?}"
        ),
        other => panic!("expected capped mobile backoff, got {other:?}"),
    }
}

#[test]
fn fact_sink_retains_terminal_generation_fact_after_capacity_burst() {
    let (sink, mut channel) = super::supervisor_internal_channel();

    for generation in 1..=128 {
        sink.session_activated(generation);
    }
    sink.signaling_generation_lost(77, SignalingFactLostCause::RemoteReset);

    let mut received = 0;
    let mut saw_terminal_loss = false;
    while let Ok(input) = channel.rx.try_recv() {
        received += 1;
        saw_terminal_loss |= matches!(
            input,
            tp::Input::SignalingGenerationLost { generation: 77, .. }
        );
    }

    assert_eq!(received, 129, "a fact burst must remain lossless");
    assert!(
        saw_terminal_loss,
        "the terminal generation loss must survive a full fact channel"
    );
}

#[derive(Clone, Copy)]
enum EffectExit {
    Succeed,
    Fail,
    Panic,
    Pending,
}

struct EffectExitProcessor {
    exit: EffectExit,
    entered: Notify,
}

impl EffectExitProcessor {
    fn new(exit: EffectExit) -> Self {
        Self {
            exit,
            entered: Notify::new(),
        }
    }
}

#[async_trait::async_trait]
impl NetworkEventProcessor for EffectExitProcessor {
    async fn process_network_available(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_type_changed(
        &self,
        _is_wifi: bool,
        _is_cellular: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_recovery_action(
        &self,
        _action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        self.entered.notify_one();
        match self.exit {
            EffectExit::Succeed => Ok(()),
            EffectExit::Fail => Err("forced effect failure".to_string()),
            EffectExit::Panic => panic!("forced effect panic"),
            EffectExit::Pending => std::future::pending().await,
        }
    }
}

fn restore_effect(action_id: u64) -> StartedEffect {
    StartedEffect {
        action_id,
        kind: crate::lifecycle::recovery_policy::diagnosis::EffectKind::Restore,
        action: tp::Action::Restore,
        captured_revision: 7,
        teardown_budget: None,
    }
}

async fn receive_effect_completion(
    rx: &mut mpsc::UnboundedReceiver<tp::Input>,
) -> crate::lifecycle::recovery_policy::diagnosis::EffectOutcome {
    let input = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("effect should complete within the test deadline")
        .expect("effect completion channel should stay open");
    let tp::Input::EffectCompleted {
        action_id,
        kind,
        policy_revision,
        outcome,
    } = input
    else {
        panic!("expected EffectCompleted, got {input:?}");
    };
    assert_eq!(action_id, 1);
    assert_eq!(
        kind,
        crate::lifecycle::recovery_policy::diagnosis::EffectKind::Restore
    );
    assert_eq!(policy_revision, 7);
    outcome
}

#[test]
fn dropping_running_effect_cancels_detached_work() {
    let token = CancellationToken::new();
    let running = RunningEffect {
        action_id: 1,
        token: token.clone(),
    };

    drop(running);

    assert!(
        token.is_cancelled(),
        "aborting the reconciler must revoke its detached effect"
    );
}

#[tokio::test]
async fn effect_reports_exactly_one_terminal_completion_for_every_exit_path() {
    let cases = [
        (EffectExit::Succeed, "succeed"),
        (EffectExit::Fail, "fail"),
        (EffectExit::Panic, "panic"),
        (EffectExit::Pending, "cancel"),
    ];

    for (exit, name) in cases {
        let processor = Arc::new(EffectExitProcessor::new(exit));
        let token = CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_effect(restore_effect(1), token.clone(), processor.clone(), tx);

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            processor.entered.notified(),
        )
        .await
        .unwrap_or_else(|_| panic!("{name} effect should start"));
        if matches!(exit, EffectExit::Pending) {
            token.cancel();
        }

        let outcome = receive_effect_completion(&mut rx).await;
        match exit {
            EffectExit::Succeed => assert!(matches!(outcome, TpEffectOutcome::Succeeded)),
            EffectExit::Fail => assert!(matches!(outcome, TpEffectOutcome::Failed { .. })),
            EffectExit::Panic => assert!(matches!(
                outcome,
                TpEffectOutcome::Aborted {
                    cause: AbortCause::PanicOrContractViolation
                }
            )),
            EffectExit::Pending => assert!(matches!(outcome, TpEffectOutcome::Cancelled)),
        }

        tokio::task::yield_now().await;
        assert!(
            matches!(
                rx.try_recv(),
                Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected)
            ),
            "{name} effect must report exactly one terminal completion"
        );
    }
}

#[tokio::test]
async fn effect_completion_queues_behind_existing_internal_input() {
    let processor = Arc::new(EffectExitProcessor::new(EffectExit::Succeed));
    let (tx, mut rx) = mpsc::unbounded_channel();
    tx.send(tp::Input::ShutdownRequested)
        .expect("test should queue the earlier internal input");

    spawn_effect(
        restore_effect(1),
        CancellationToken::new(),
        processor.clone(),
        tx,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        processor.entered.notified(),
    )
    .await
    .expect("effect should run while an earlier internal input is queued");
    tokio::task::yield_now().await;

    assert!(matches!(rx.try_recv(), Ok(tp::Input::ShutdownRequested)));
    assert!(matches!(
        receive_effect_completion(&mut rx).await,
        TpEffectOutcome::Succeeded
    ));
}

struct CleanupSessionProcessor {
    actions: StdMutex<Vec<NetworkRecoveryAction>>,
    cleanup_entered: Notify,
    cleanup_release: Notify,
    recovery_entered: Notify,
}

impl CleanupSessionProcessor {
    fn actions(&self) -> Vec<NetworkRecoveryAction> {
        self.actions.lock().expect("actions mutex poisoned").clone()
    }
}

#[async_trait::async_trait]
impl NetworkEventProcessor for CleanupSessionProcessor {
    async fn process_network_available(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_type_changed(
        &self,
        _is_wifi: bool,
        _is_cellular: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_recovery_action(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        self.actions
            .lock()
            .expect("actions mutex poisoned")
            .push(action);
        self.recovery_entered.notify_one();
        Ok(())
    }

    async fn run_bounded_teardown(
        &self,
        action: NetworkRecoveryAction,
        _budget: std::time::Duration,
    ) -> TeardownReport {
        self.actions
            .lock()
            .expect("actions mutex poisoned")
            .push(action);
        self.cleanup_entered.notify_one();
        self.cleanup_release.notified().await;
        TeardownReport::succeeded()
    }
}

#[tokio::test]
async fn session_activated_during_cleanup_runs_post_cleanup_recovery() {
    let processor = Arc::new(CleanupSessionProcessor {
        actions: StdMutex::new(Vec::new()),
        cleanup_entered: Notify::new(),
        cleanup_release: Notify::new(),
        recovery_entered: Notify::new(),
    });
    let (event_tx, event_rx) = mpsc::channel(8);
    let (fact_sink, channel) = supervisor_internal_channel();
    let handle = NetworkEventHandle::new_with_fact_sink(event_tx, fact_sink);
    let shutdown = CancellationToken::new();
    let reconciler = tokio::spawn(run_network_event_reconciler_with_channel(
        event_rx,
        channel,
        processor.clone(),
        shutdown.clone(),
    ));

    let result = handle
        .cleanup_connections(CleanupReason::ManualReset)
        .await
        .expect("cleanup should be accepted");
    assert!(result.success);
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        processor.cleanup_entered.notified(),
    )
    .await
    .expect("cleanup effect should start");

    handle.notify_session_activated(2);
    processor.cleanup_release.notify_one();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        processor.recovery_entered.notified(),
    )
    .await
    .expect("new session should derive recovery after cleanup");

    assert_eq!(
        processor.actions(),
        vec![
            NetworkRecoveryAction::CleanupOnly,
            NetworkRecoveryAction::Restore
        ]
    );

    // External shutdown now runs the bounded-teardown cleanup effect before
    // the loop exits; leave a stored permit so it completes promptly.
    processor.cleanup_release.notify_one();
    shutdown.cancel();
    reconciler.await.expect("reconciler should stop cleanly");
}

struct ControlledRecoveryProcessor {
    started_tx: mpsc::UnboundedSender<NetworkRecoveryAction>,
    release: Semaphore,
}

#[async_trait::async_trait]
impl NetworkEventProcessor for ControlledRecoveryProcessor {
    async fn process_network_available(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_type_changed(
        &self,
        _is_wifi: bool,
        _is_cellular: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_recovery_action(
        &self,
        action: NetworkRecoveryAction,
    ) -> Result<(), String> {
        self.started_tx
            .send(action)
            .map_err(|_| "test action receiver closed".to_string())?;
        self.release
            .acquire()
            .await
            .map_err(|_| "test release semaphore closed".to_string())?
            .forget();
        Ok(())
    }
}

fn online_route_snapshot(sequence: u64, wifi: bool) -> NetworkSnapshot {
    online_route_snapshot_with_cost(sequence, wifi, !wifi)
}

fn online_route_snapshot_with_cost(
    sequence: u64,
    wifi: bool,
    is_expensive: bool,
) -> NetworkSnapshot {
    NetworkSnapshot {
        sequence,
        availability: NetworkAvailability::Available,
        transport: NetworkTransportFlags {
            wifi,
            cellular: !wifi,
            ..NetworkTransportFlags::default()
        },
        is_expensive,
        is_constrained: false,
    }
}

#[test]
fn rtc_material_path_change_emits_local_address_edge() {
    use crate::wire::webrtc::rtc::recovery::{RtcNetworkEvent, RtcRecoverySupervisor};

    let supervisor = RtcRecoverySupervisor::new();
    let mut events = supervisor.subscribe();
    let mut previous_path = None;
    let mut path_change_pending = false;

    let wifi = online_route_snapshot_with_cost(1, true, false);
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: wifi.clone(),
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("initial availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert!(
        events.try_recv().is_err(),
        "initial path must not restart ICE"
    );

    // A billing/data-saver-only change is not a local transport change and
    // must not trigger an ICE rebind.
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: online_route_snapshot_with_cost(2, true, true),
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("cost-only availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert!(
        events.try_recv().is_err(),
        "cost-only changes must not restart ICE"
    );

    let cellular = online_route_snapshot_with_cost(3, false, true);
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged { snapshot: cellular },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert_eq!(
        events.try_recv().expect("material path change fact"),
        RtcNetworkEvent::LocalAddressChanged
    );
}

#[test]
fn rtc_offline_online_restore_does_not_emit_duplicate_local_address_edge() {
    use crate::wire::webrtc::rtc::recovery::{RtcNetworkEvent, RtcRecoverySupervisor};

    let supervisor = RtcRecoverySupervisor::new();
    let mut events = supervisor.subscribe();
    let mut previous_path = None;
    let mut path_change_pending = false;

    let online = online_route_snapshot_with_cost(1, true, false);
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: online.clone(),
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("initial availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert!(events.try_recv().is_err());

    let mut offline = online.clone();
    offline.sequence = 2;
    offline.availability = NetworkAvailability::Unavailable;
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged { snapshot: offline },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("offline availability fact"),
        RtcNetworkEvent::NetworkUnavailable
    );
    assert!(
        events.try_recv().is_err(),
        "Offline must not emit a local-address edge"
    );
    assert!(!path_change_pending, "Offline clears pending path edges");

    let mut restored = online;
    restored.sequence = 3;
    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged { snapshot: restored },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("restore availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert!(
        events.try_recv().is_err(),
        "Offline -> Online restore must not duplicate LocalAddressChanged"
    );
}

#[test]
fn rtc_path_change_survives_android_unknown_intermediate_state() {
    use crate::wire::webrtc::rtc::recovery::{RtcNetworkEvent, RtcRecoverySupervisor};

    let supervisor = RtcRecoverySupervisor::new();
    let mut events = supervisor.subscribe();
    let mut previous_path = None;
    let mut path_change_pending = false;

    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: online_route_snapshot(1, true),
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("initial availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );

    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: NetworkSnapshot {
                sequence: 2,
                availability: NetworkAvailability::Unknown,
                transport: NetworkTransportFlags::default(),
                is_expensive: false,
                is_constrained: false,
            },
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert!(
        events.try_recv().is_err(),
        "Unknown is held as a pending edge"
    );
    assert!(path_change_pending);

    super::publish_rtc_network_fact(
        &supervisor,
        &NetworkEvent::NetworkPathChanged {
            snapshot: online_route_snapshot(3, false),
        },
        previous_path,
        &mut path_change_pending,
        &mut previous_path,
    );
    assert_eq!(
        events.try_recv().expect("availability fact"),
        RtcNetworkEvent::NetworkAvailable
    );
    assert_eq!(
        events.try_recv().expect("pending path change fact"),
        RtcNetworkEvent::LocalAddressChanged
    );
    assert!(!path_change_pending);
}

#[tokio::test]
async fn gated_reconciler_waits_for_authoritative_foreground() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let processor = Arc::new(ControlledRecoveryProcessor {
        started_tx,
        release: Semaphore::new(0),
    });
    let (event_tx, event_rx) = mpsc::channel(16);
    let (fact_sink, channel) =
        supervisor_internal_channel_with_profile(tp::LifecycleProfile::Gated);
    let handle = NetworkEventHandle::new_with_fact_sink(event_tx, fact_sink);
    let SupervisorInternalChannel {
        tx: internal_tx,
        rx: internal_rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    let (status_tx, status_rx) = watch::channel(SupervisorStatus::default());
    let shutdown = CancellationToken::new();
    let reconciler = tokio::spawn(reconcile_loop(
        event_rx,
        internal_tx,
        internal_rx,
        processor.clone(),
        shutdown.clone(),
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: None,
        },
    ));

    handle
        .handle_network_path_changed(online_route_snapshot(1, true))
        .await
        .expect("initial online route should be accepted");
    assert_eq!(status_rx.borrow().policy_revision, 1);
    assert_eq!(
        status_rx.borrow().last_action_id,
        None,
        "Gated recovery must stay ineligible while app phase is Unknown"
    );
    assert!(started_rx.try_recv().is_err());

    handle
        .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
            background_duration_ms: 0,
        })
        .await
        .expect("authoritative foreground should be accepted");
    assert_eq!(status_rx.borrow().last_action_id, Some(1));
    assert_eq!(
        started_rx.recv().await,
        Some(NetworkRecoveryAction::Restore)
    );

    // One permit releases the in-flight Restore; the second lets the
    // shutdown-injected bounded-teardown cleanup effect complete.
    processor.release.add_permits(2);
    shutdown.cancel();
    reconciler.await.expect("reconciler should stop cleanly");
}

#[tokio::test]
async fn current_effect_fact_and_event_storm_preserve_monotonic_supervisor_status() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let processor = Arc::new(ControlledRecoveryProcessor {
        started_tx,
        release: Semaphore::new(0),
    });
    let (event_tx, event_rx) = mpsc::channel(16);
    let (fact_sink, channel) = supervisor_internal_channel();
    let handle = NetworkEventHandle::new_with_fact_sink(event_tx, fact_sink.clone());
    let SupervisorInternalChannel {
        tx: internal_tx,
        rx: internal_rx,
        send_projection_tx,
        profile,
        clock_origin,
    } = channel;
    let (status_tx, mut status_rx) = watch::channel(SupervisorStatus::default());
    let shutdown = CancellationToken::new();
    let reconciler = tokio::spawn(reconcile_loop(
        event_rx,
        internal_tx,
        internal_rx,
        processor.clone(),
        shutdown.clone(),
        status_tx,
        send_projection_tx,
        ReconcilerConfig {
            profile,
            clock_origin,
            rtc_recovery: None,
        },
    ));

    handle
        .handle_network_path_changed(online_route_snapshot(1, true))
        .await
        .expect("initial online route should be accepted");
    assert_eq!(
        started_rx.recv().await,
        Some(NetworkRecoveryAction::Restore)
    );
    let mut statuses = vec![status_rx.borrow_and_update().clone()];
    assert_eq!(statuses[0].policy_revision, 1);
    assert_eq!(statuses[0].last_action_id, Some(1));
    assert_eq!(statuses[0].last_outcome, None);

    fact_sink
        .signaling_generation_committed(7, SignalingFactOrigin::CurrentEffect { action_id: 1 });
    status_rx
        .wait_for(|status| status.policy_revision == 2)
        .await
        .expect("current-effect fact should be reconciled");
    statuses.push(status_rx.borrow_and_update().clone());

    for sequence in 2..=11 {
        handle
            .handle_network_path_changed(online_route_snapshot(sequence, sequence % 2 == 1))
            .await
            .expect("material route change should be accepted");
        let status = status_rx.borrow_and_update().clone();
        assert_eq!(status.policy_revision, sequence + 1);
        assert_eq!(status.last_action_id, Some(1));
        assert_eq!(status.last_outcome, None);
        statuses.push(status);
    }

    for sequence in 12..=111 {
        handle
            .handle_network_path_changed(online_route_snapshot(sequence, true))
            .await
            .expect("duplicate route snapshot should be accepted");
        let status = status_rx.borrow_and_update().clone();
        assert_eq!(
            status.policy_revision, 12,
            "structural duplicates must not advance policy"
        );
        assert_eq!(status.last_action_id, Some(1));
        assert_eq!(status.last_outcome, None);
        statuses.push(status);
    }

    processor.release.add_permits(1);
    status_rx
        .wait_for(|status| status.last_outcome == Some(ObservedOutcome::Succeeded))
        .await
        .expect("the first effect completion should remain current");
    let first_completion = status_rx.borrow_and_update().clone();
    assert_eq!(first_completion.policy_revision, 12);
    assert_eq!(first_completion.last_action_id, Some(1));
    statuses.push(first_completion);

    handle
        .force_reconnect(ReconnectReason::ManualReconnect)
        .await
        .expect("a later recovery request should be accepted");
    assert_eq!(
        started_rx.recv().await,
        Some(NetworkRecoveryAction::ForceReconnect)
    );
    let second_started = status_rx.borrow_and_update().clone();
    assert_eq!(second_started.policy_revision, 13);
    assert_eq!(second_started.last_action_id, Some(2));
    assert_eq!(
        second_started.last_outcome,
        Some(ObservedOutcome::Succeeded),
        "the previous terminal outcome should remain observable while new work runs"
    );
    statuses.push(second_started);

    processor.release.add_permits(1);
    status_rx
        .changed()
        .await
        .expect("the second effect completion should publish status");
    let second_completion = status_rx.borrow_and_update().clone();
    assert_eq!(second_completion.policy_revision, 13);
    assert_eq!(second_completion.last_action_id, Some(2));
    assert_eq!(
        second_completion.last_outcome,
        Some(ObservedOutcome::Succeeded)
    );
    statuses.push(second_completion);

    assert!(statuses.windows(2).all(|pair| {
        pair[0].policy_revision <= pair[1].policy_revision
            && pair[0].last_action_id <= pair[1].last_action_id
    }));

    // A permit for the shutdown-injected bounded-teardown cleanup effect.
    processor.release.add_permits(1);
    shutdown.cancel();
    reconciler.await.expect("reconciler should stop cleanly");
}

#[test]
fn teardown_outcome_maps_every_report_class() {
    assert!(matches!(
        super::teardown_outcome(TeardownReport::succeeded()),
        TpEffectOutcome::Succeeded
    ));
    assert!(matches!(
        super::teardown_outcome(TeardownReport::completed_with_residuals(vec!["x".into()])),
        TpEffectOutcome::CompletedWithResiduals { .. }
    ));
    assert!(matches!(
        super::teardown_outcome(TeardownReport::abandoned(vec!["x".into()])),
        TpEffectOutcome::Abandoned { .. }
    ));
    let failed = TeardownReport {
        reached_goal: false,
        deadline_reached: false,
        residuals: vec![],
    };
    assert!(matches!(
        super::teardown_outcome(failed),
        TpEffectOutcome::Failed { .. }
    ));
}

/// Records bounded-teardown invocations; `hang` makes the teardown never
/// complete so the shutdown overall deadline is the only way out.
struct ShutdownTeardownProcessor {
    teardowns: StdMutex<Vec<(NetworkRecoveryAction, std::time::Duration)>>,
    hang: bool,
}

impl ShutdownTeardownProcessor {
    fn new(hang: bool) -> Self {
        Self {
            teardowns: StdMutex::new(Vec::new()),
            hang,
        }
    }

    fn teardowns(&self) -> Vec<(NetworkRecoveryAction, std::time::Duration)> {
        self.teardowns
            .lock()
            .expect("teardowns mutex poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl NetworkEventProcessor for ShutdownTeardownProcessor {
    async fn process_network_available(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_lost(&self) -> Result<(), String> {
        Ok(())
    }

    async fn process_network_type_changed(
        &self,
        _is_wifi: bool,
        _is_cellular: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn cleanup_connections(&self) -> Result<(), String> {
        Ok(())
    }

    async fn run_bounded_teardown(
        &self,
        action: NetworkRecoveryAction,
        budget: std::time::Duration,
    ) -> TeardownReport {
        self.teardowns
            .lock()
            .expect("teardowns mutex poisoned")
            .push((action, budget));
        if self.hang {
            std::future::pending::<()>().await;
        }
        TeardownReport::succeeded()
    }
}

#[tokio::test]
async fn pre_cancelled_offline_effect_does_not_poll_bounded_teardown() {
    let processor = Arc::new(ShutdownTeardownProcessor::new(false));
    let token = CancellationToken::new();
    token.cancel();
    let (tx, mut rx) = mpsc::unbounded_channel();

    spawn_effect(
        StartedEffect {
            action_id: 1,
            kind:
                crate::lifecycle::recovery_policy::diagnosis::EffectKind::ConfirmedOfflineDisconnect,
            action: tp::Action::ConfirmedOfflineDisconnect,
            captured_revision: 7,
            teardown_budget: Some(Duration::from_secs(1)),
        },
        token,
        processor.clone(),
        tx,
    );

    let completion = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("pre-cancelled effect should complete")
        .expect("effect completion channel should remain open");
    assert!(matches!(
        completion,
        tp::Input::EffectCompleted {
            action_id: 1,
            kind:
                crate::lifecycle::recovery_policy::diagnosis::EffectKind::ConfirmedOfflineDisconnect,
            policy_revision: 7,
            outcome: TpEffectOutcome::Cancelled,
        }
    ));
    assert!(
        processor.teardowns().is_empty(),
        "cancellation observed before the first poll must leave the transport untouched"
    );
}

#[tokio::test]
async fn external_shutdown_runs_bounded_teardown_before_exit() {
    let processor = Arc::new(ShutdownTeardownProcessor::new(false));
    let (_event_tx, event_rx) = mpsc::channel::<NetworkEventRequest>(8);
    let (_fact_sink, channel) = supervisor_internal_channel();
    let shutdown = CancellationToken::new();
    let reconciler = tokio::spawn(run_network_event_reconciler_with_channel(
        event_rx,
        channel,
        processor.clone(),
        shutdown.clone(),
    ));

    // External cancellation must not exit the loop directly: it is lowered to
    // `ShutdownRequested`, whose decision derives the cleanup obligation and
    // runs it as a bounded teardown before the reconciler ends.
    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), reconciler)
        .await
        .expect("reconciler must exit promptly once the teardown is discharged")
        .expect("reconciler task should not panic");

    let teardowns = processor.teardowns();
    assert_eq!(teardowns.len(), 1, "exactly one cleanup obligation runs");
    assert_eq!(teardowns[0].0, NetworkRecoveryAction::CleanupOnly);
    assert!(
        teardowns[0].1 > std::time::Duration::ZERO,
        "the cleanup effect must inherit a bounded teardown budget"
    );
}

#[tokio::test(start_paused = true)]
async fn external_shutdown_with_hanging_cleanup_ends_at_shutdown_deadline() {
    let processor = Arc::new(ShutdownTeardownProcessor::new(true));
    let (_event_tx, event_rx) = mpsc::channel::<NetworkEventRequest>(8);
    let (_fact_sink, channel) = supervisor_internal_channel();
    let shutdown = CancellationToken::new();
    let started_at = tokio::time::Instant::now();
    let reconciler = tokio::spawn(run_network_event_reconciler_with_channel(
        event_rx,
        channel,
        processor.clone(),
        shutdown.clone(),
    ));

    shutdown.cancel();
    // The cleanup effect never completes; the injected `ShutdownRequested`
    // armed the shutdown overall deadline, which terminates the supervisor
    // unconditionally (virtual time auto-advances to the deadline).
    reconciler
        .await
        .expect("reconciler must terminate at the shutdown overall deadline");
    assert!(
        started_at.elapsed() >= std::time::Duration::from_secs(10),
        "termination must come from the 10s shutdown deadline, not an early exit"
    );
    assert_eq!(processor.teardowns().len(), 1);
}

#[derive(Clone, Copy)]
enum DisconnectMode {
    Err,
    Hang,
}

struct TeardownFakeSignaling {
    connected: AtomicBool,
    disconnect_mode: DisconnectMode,
    invalidated: AtomicBool,
    hang: tokio::sync::Notify,
    event_tx: broadcast::Sender<SignalingEvent>,
}

impl TeardownFakeSignaling {
    fn new(mode: DisconnectMode) -> Self {
        let (event_tx, _rx) = broadcast::channel(8);
        Self {
            connected: AtomicBool::new(true),
            disconnect_mode: mode,
            invalidated: AtomicBool::new(false),
            hang: tokio::sync::Notify::new(),
            event_tx,
        }
    }
}

#[async_trait::async_trait]
impl SignalingClient for TeardownFakeSignaling {
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn disconnect(&self) -> NetworkResult<()> {
        match self.disconnect_mode {
            DisconnectMode::Err => Err(NetworkError::ConnectionError(
                "forced disconnect failure".to_string(),
            )),
            DisconnectMode::Hang => {
                // Never notified: the caller's overall budget must abandon this.
                self.hang.notified().await;
                Ok(())
            }
        }
    }

    fn invalidate_generation(&self) {
        self.invalidated.store(true, AtomicOrdering::SeqCst);
    }

    async fn send_register_request(
        &self,
        _request: RegisterRequest,
    ) -> NetworkResult<RegisterResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_unregister_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _reason: Option<String>,
    ) -> NetworkResult<UnregisterResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_heartbeat(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _availability: ServiceAvailabilityState,
        _power_reserve: f32,
        _mailbox_backlog: f32,
    ) -> NetworkResult<Pong> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_route_candidates_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _request: RouteCandidatesRequest,
    ) -> NetworkResult<RouteCandidatesResponse> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn get_signing_key(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _key_id: u32,
    ) -> NetworkResult<(u32, Vec<u8>)> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn send_envelope(&self, _envelope: SignalingEnvelope) -> NetworkResult<()> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>> {
        Err(NetworkError::ConnectionError("unused".to_string()))
    }

    fn is_connected(&self) -> bool {
        self.connected.load(AtomicOrdering::SeqCst)
    }

    fn get_stats(&self) -> SignalingStats {
        SignalingStats::default()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent> {
        self.event_tx.subscribe()
    }

    async fn set_actor_id(&self, _actor_id: ActrId) {}

    async fn set_credential_state(&self, _credential_state: CredentialState) {}

    async fn clear_identity(&self) {}
}

#[tokio::test]
async fn bounded_cleanup_invalidates_up_front_and_records_residuals() {
    let signaling = Arc::new(TeardownFakeSignaling::new(DisconnectMode::Err));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());

    let report = processor
        .bounded_cleanup(std::time::Duration::from_secs(10))
        .await;

    assert!(
        signaling.invalidated.load(AtomicOrdering::SeqCst),
        "commit rights are invalidated synchronously before any physical step"
    );
    assert!(report.reached_goal, "local teardown completed");
    assert!(!report.deadline_reached);
    assert_eq!(
        report.residuals.len(),
        1,
        "the failed disconnect step is recorded as a residual, not an early return"
    );
}

#[tokio::test(start_paused = true)]
async fn bounded_cleanup_abandons_remaining_steps_at_budget() {
    let signaling = Arc::new(TeardownFakeSignaling::new(DisconnectMode::Hang));
    let processor = DefaultNetworkEventProcessor::new(signaling.clone());
    let budget = std::time::Duration::from_millis(100);

    // Drive time forward past the budget while the hanging disconnect is in
    // flight; the overall deadline must abandon the remaining steps.
    let (report, ()) = tokio::join!(processor.bounded_cleanup(budget), async {
        tokio::time::advance(std::time::Duration::from_millis(150)).await;
    });

    assert!(signaling.invalidated.load(AtomicOrdering::SeqCst));
    assert!(report.deadline_reached, "budget expired");
    assert!(!report.reached_goal, "goal not confirmed");
    assert_eq!(report.residuals.len(), 1);
}

/// RFC-0400 invariant 33 (per-peer coverage for peer-scoped recovery) at the
/// effect boundary: a supervisor-driven Restore captures its target set at
/// dispatch, and its completion is synthesized from per-peer terminal facts —
/// dispatch alone never yields a success-class outcome for an uncovered
/// target. These tests drive the real manager, real controllers, and the
/// exact trait method `spawn_effect` calls.
mod inv33_peer_coverage {
    use super::super::{
        DefaultNetworkEventProcessor, EffectDiagnosis, EffectReport, NetworkEventProcessor,
        NetworkRecoveryAction,
    };
    use super::ForceReconnectFakeSignalingClient;
    use crate::lifecycle::CredentialState;
    use crate::transport::NetworkResult;
    use crate::wire::webrtc::rtc::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
    use crate::wire::webrtc::rtc::peer::effects::test_support::fake_data_channel;
    use crate::wire::webrtc::rtc::peer::effects::{SessionFactory, SessionOps};
    use crate::wire::webrtc::rtc::recovery::PeerRecoveryTarget;
    use crate::wire::webrtc::{SignalingEvent, SignalingStats};
    use actr_protocol::{
        AIdCredential, ActrId, ActrType, Pong, Realm, RegisterRequest, RegisterResponse,
        RouteCandidatesRequest, RouteCandidatesResponse, ServiceAvailabilityState,
        SignalingEnvelope, UnregisterResponse,
    };
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tokio::sync::{broadcast, mpsc};
    use tokio::time::timeout;
    use webrtc::error::Error as RtcError;
    use webrtc::peer_connection::{RTCConfiguration, RTCIceCandidateInit, RTCSessionDescription};

    const WAIT: Duration = Duration::from_secs(5);

    fn peer_id(serial: u64) -> ActrId {
        ActrId {
            realm: Realm { realm_id: 1 },
            serial_number: serial,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "node".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    /// Instantly-built fake PC sessions: negotiation calls succeed, the data
    /// channel is a fake that opens, and nothing ever reports a PC connection
    /// state — so the controller holds a session identity (generation > 0)
    /// without ever being covered by a success fact.
    struct InstantOps;

    #[async_trait::async_trait]
    impl SessionOps for InstantOps {
        async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
            Ok(RTCSessionDescription::default())
        }
        async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
            Ok(RTCSessionDescription::default())
        }
        async fn set_local_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
            Ok(())
        }
        async fn set_remote_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
            Ok(())
        }
        async fn add_ice_candidate(&self, _c: RTCIceCandidateInit) -> Result<(), RtcError> {
            Ok(())
        }
        async fn restart_ice(&self) -> Result<(), RtcError> {
            Ok(())
        }
        async fn set_configuration(&self, _c: RTCConfiguration) -> Result<(), RtcError> {
            Ok(())
        }
        async fn create_data_channel(
            &self,
            label: &str,
            _init: webrtc::data_channel::RTCDataChannelInit,
        ) -> Result<Arc<dyn webrtc::data_channel::DataChannel>, RtcError> {
            Ok(fake_data_channel(label))
        }
        async fn add_track(
            &self,
            _t: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
        ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, RtcError> {
            Err(RtcError::Other("no media in inv33 tests".into()))
        }
        async fn remove_track(
            &self,
            _s: &Arc<dyn webrtc::rtp_transceiver::RtpSender>,
        ) -> Result<(), RtcError> {
            Ok(())
        }
        async fn get_senders(&self) -> Vec<Arc<dyn webrtc::rtp_transceiver::RtpSender>> {
            Vec::new()
        }
        async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
            Default::default()
        }
        async fn close(&self) -> Result<(), RtcError> {
            Ok(())
        }
    }

    struct InstantFactory;

    #[async_trait::async_trait]
    impl SessionFactory for InstantFactory {
        async fn build(
            &self,
            _handler: Arc<WebRtcEventHandler>,
        ) -> Result<Arc<dyn SessionOps>, RtcError> {
            Ok(Arc::new(InstantOps))
        }
    }

    fn manager() -> Arc<RtcConnectionManager> {
        let (signaling_tx, _signaling_rx) = mpsc::channel(16);
        let manager = Arc::new(RtcConnectionManager::new(
            Arc::new(InstantFactory),
            signaling_tx,
        ));
        manager.supervisor().set_signaling_generation(Some(0));
        manager
    }

    /// A signaling client whose recovery connect closes a manager peer before
    /// returning success — the deterministic injection point for "the target
    /// peer conclusively left the session between dispatch and synthesis".
    struct DepartOnConnectSignaling {
        inner: Arc<ForceReconnectFakeSignalingClient>,
        depart: StdMutex<Option<(Arc<RtcConnectionManager>, ActrId)>>,
    }

    impl DepartOnConnectSignaling {
        fn new() -> Self {
            Self {
                inner: Arc::new(ForceReconnectFakeSignalingClient::new(false)),
                depart: StdMutex::new(None),
            }
        }

        fn close_this_peer_on_connect(&self, manager: Arc<RtcConnectionManager>, peer: ActrId) {
            *self.depart.lock().unwrap() = Some((manager, peer));
        }
    }

    #[async_trait::async_trait]
    impl crate::wire::webrtc::SignalingClient for DepartOnConnectSignaling {
        async fn connect_once_for_recovery(&self, action_id: u64) -> NetworkResult<()> {
            let departure = self.depart.lock().unwrap().take();
            if let Some((manager, peer)) = departure {
                manager.close_peer(&peer).await;
            }
            self.inner.connect_once_for_recovery(action_id).await
        }
        async fn connect(&self) -> NetworkResult<()> {
            self.inner.connect().await
        }
        async fn disconnect(&self) -> NetworkResult<()> {
            self.inner.disconnect().await
        }
        async fn send_register_request(
            &self,
            request: RegisterRequest,
        ) -> NetworkResult<RegisterResponse> {
            self.inner.send_register_request(request).await
        }
        async fn send_unregister_request(
            &self,
            actor_id: ActrId,
            credential: AIdCredential,
            reason: Option<String>,
        ) -> NetworkResult<UnregisterResponse> {
            self.inner
                .send_unregister_request(actor_id, credential, reason)
                .await
        }
        async fn send_heartbeat(
            &self,
            actor_id: ActrId,
            credential: AIdCredential,
            availability: ServiceAvailabilityState,
            power_reserve: f32,
            mailbox_backlog: f32,
        ) -> NetworkResult<Pong> {
            self.inner
                .send_heartbeat(
                    actor_id,
                    credential,
                    availability,
                    power_reserve,
                    mailbox_backlog,
                )
                .await
        }
        async fn send_route_candidates_request(
            &self,
            actor_id: ActrId,
            credential: AIdCredential,
            request: RouteCandidatesRequest,
        ) -> NetworkResult<RouteCandidatesResponse> {
            self.inner
                .send_route_candidates_request(actor_id, credential, request)
                .await
        }
        async fn get_signing_key(
            &self,
            actor_id: ActrId,
            credential: AIdCredential,
            key_id: u32,
        ) -> NetworkResult<(u32, Vec<u8>)> {
            self.inner
                .get_signing_key(actor_id, credential, key_id)
                .await
        }
        async fn send_envelope(&self, envelope: SignalingEnvelope) -> NetworkResult<()> {
            self.inner.send_envelope(envelope).await
        }
        async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>> {
            self.inner.receive_envelope().await
        }
        fn is_connected(&self) -> bool {
            self.inner.is_connected()
        }
        fn get_stats(&self) -> SignalingStats {
            self.inner.get_stats()
        }
        fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent> {
            self.inner.subscribe_events()
        }
        async fn set_actor_id(&self, actor_id: ActrId) {
            self.inner.set_actor_id(actor_id).await
        }
        async fn set_credential_state(&self, credential_state: CredentialState) {
            self.inner.set_credential_state(credential_state).await
        }
        async fn clear_identity(&self) {
            self.inner.clear_identity().await
        }
    }

    /// Build a manager peer that holds a session identity (generation > 0)
    /// but is not covered by a success fact: an offerer negotiating against
    /// instant fake sessions that never report a PC connection state.
    async fn peer_with_uncovered_session(manager: &Arc<RtcConnectionManager>, peer: &ActrId) {
        let handle = manager.ensure_controller(peer.clone()).await;
        handle.connect().await.expect("connect command");
        handle
            .apply_role_assignment(true)
            .await
            .expect("role assignment command");
        let mut rx = handle.subscribe();
        timeout(WAIT, async {
            loop {
                if rx.borrow().pc_generation > 0 {
                    return;
                }
                rx.changed().await.expect("snapshot watch stays live");
            }
        })
        .await
        .expect("controller never allocated a session generation");
    }

    /// Parse `session=` and `ice=` out of a handoff residual scope string.
    fn parse_handoff_tuple(scope: &str) -> (u64, u64) {
        let session = scope
            .split("handoff:session=")
            .nth(1)
            .and_then(|rest| rest.split(':').next())
            .and_then(|v| v.parse::<u64>().ok())
            .expect("handoff scope carries session=");
        let ice = scope
            .split(":ice=")
            .nth(1)
            .and_then(|rest| rest.split(':').next())
            .and_then(|v| v.parse::<u64>().ok())
            .expect("handoff scope carries ice=");
        (session, ice)
    }

    /// Dispatch alone never yields a success-class outcome for an uncovered
    /// target: the Restore completes its shared prerequisites, yet the peer —
    /// still uncovered by a success fact — is named in exactly one handoff
    /// residual whose tuple is recorded as an open obligation with the
    /// receiving owner (`RtcConnectionManager`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn restore_dispatch_hands_off_uncovered_target_as_residual() {
        let manager = manager();
        let peer = peer_id(701);
        peer_with_uncovered_session(&manager, &peer).await;

        let processor = DefaultNetworkEventProcessor::new(Arc::new(
            ForceReconnectFakeSignalingClient::new(false),
        ))
        .with_rtc_manager(Some(manager.clone()));
        let report = processor
            .process_network_recovery_effect_for_action(NetworkRecoveryAction::Restore, 4201)
            .await
            .expect("restore completes its shared prerequisites");

        assert_eq!(
            report.residuals.len(),
            1,
            "the uncovered target is named in exactly one residual"
        );
        let EffectDiagnosis::TransportImpaired { scopes } = &report.residuals[0] else {
            panic!(
                "handoff residual is carried as TransportImpaired: {:?}",
                report.residuals[0]
            );
        };
        assert_eq!(scopes.len(), 1);
        let scope = &scopes[0];
        assert!(
            scope.starts_with(&format!("peer={}:", peer.to_string_repr())),
            "residual names the target peer: {scope}"
        );
        assert!(
            scope.contains(":handoff:session="),
            "residual is a handoff record: {scope}"
        );
        assert!(
            scope.ends_with(":owner=RtcConnectionManager"),
            "residual names the receiving resource owner: {scope}"
        );

        // The residual tuple is exactly what the owner's ledger holds open.
        let (session, ice) = parse_handoff_tuple(scope);
        let target = PeerRecoveryTarget::new(peer.clone(), session, ice);
        assert!(
            manager.recovery_obligation_log().is_open(&target),
            "the record call is the acknowledgement: the tuple must be open in the owner"
        );

        // A later peer departure runs through the manager's real close path
        // and extinguishes the handed-off obligation.
        assert!(manager.close_peer(&peer).await);
        assert!(
            !manager.recovery_obligation_log().is_open(&target),
            "peer departure through the production close path extinguishes the obligation"
        );
    }

    /// A target peer that conclusively leaves the session between dispatch
    /// and synthesis is named in one out-of-scope residual; no obligation is
    /// handed off for it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn restore_names_departed_target_as_out_of_scope_residual() {
        let manager = manager();
        let peer = peer_id(702);
        peer_with_uncovered_session(&manager, &peer).await;

        let signaling = Arc::new(DepartOnConnectSignaling::new());
        signaling.close_this_peer_on_connect(manager.clone(), peer.clone());
        let processor =
            DefaultNetworkEventProcessor::new(signaling).with_rtc_manager(Some(manager.clone()));
        let report = processor
            .process_network_recovery_effect_for_action(NetworkRecoveryAction::Restore, 4202)
            .await
            .expect("restore completes its shared prerequisites");

        assert_eq!(report.residuals.len(), 1);
        let EffectDiagnosis::TransportImpaired { scopes } = &report.residuals[0] else {
            panic!(
                "out-of-scope residual is carried as TransportImpaired: {:?}",
                report.residuals[0]
            );
        };
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            scopes[0],
            format!(
                "peer={}:out_of_scope:peer_left_session",
                peer.to_string_repr()
            ),
            "the departed target is named as conclusively out of scope"
        );
        assert!(
            manager.recovery_obligation_log().records().is_empty(),
            "an out-of-scope target is named, not handed off"
        );
    }

    /// With no target set (no peer holds an uncovered session) the Restore
    /// keeps the pure success contract: no residuals.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn restore_without_targets_completes_pure() {
        let manager = manager();
        // A controller without a session identity is not a target: its
        // generation is still zero.
        manager.ensure_controller(peer_id(703)).await;

        let processor = DefaultNetworkEventProcessor::new(Arc::new(
            ForceReconnectFakeSignalingClient::new(false),
        ))
        .with_rtc_manager(Some(manager));
        let report = processor
            .process_network_recovery_effect_for_action(NetworkRecoveryAction::Restore, 4203)
            .await
            .expect("restore completes");
        assert_eq!(report, EffectReport::PURE);
    }
}
