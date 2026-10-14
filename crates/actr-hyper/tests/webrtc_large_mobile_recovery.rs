// The legacy batch helpers remain part of the compatibility contract exercised
// by this suite; production mobile entry points use the reconciler.
#![allow(deprecated)]
//! Integration tests for in-flight large WebRTC messages across mobile-style
//! network and app-lifecycle interruptions.
//!
//! The scenario matrix, relative timing, role cases, directions, and outcome
//! assertions are retained from the pre-RTC suite. Only the construction and
//! fault-injection seams use the RTC manager/controller/event-handler stack.

use actr_framework::Bytes;
use actr_hyper::lifecycle::{
    NetworkAvailability, NetworkEvent, NetworkEventHandle, NetworkEventProcessor,
    NetworkEventResult, NetworkRecoveryAction, NetworkSnapshot, NetworkTransportFlags,
    ReconnectReason, process_network_event_batch, select_network_recovery_action,
};
use actr_hyper::test_support::{
    TestHarness, TestRtcRole, WebRtcFragmentSendEvent, WebRtcFragmentSendHook,
    install_webrtc_fragment_send_hook_for_test, spawn_network_event_supervisor,
};
use actr_protocol::prost::Message as _;
use actr_protocol::{ActrError, DataChunk, Direction, PayloadType, RpcEnvelope};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, oneshot};

const LARGE_PAYLOAD_SIZE: usize = 200 * 1024;

fn fragment_hook_test_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

#[derive(Clone, Copy)]
struct RoleCase {
    name: &'static str,
    mobile_serial: u64,
    server_serial: u64,
}

#[derive(Clone, Copy)]
struct DirectionCase {
    name: &'static str,
    from_serial: u64,
    to_serial: u64,
}

impl RoleCase {
    fn directions(self) -> [DirectionCase; 2] {
        [
            DirectionCase {
                name: "mobile_to_server",
                from_serial: self.mobile_serial,
                to_serial: self.server_serial,
            },
            DirectionCase {
                name: "server_to_mobile",
                from_serial: self.server_serial,
                to_serial: self.mobile_serial,
            },
        ]
    }
}

const ROLE_CASES: [RoleCase; 2] = [
    RoleCase {
        name: "mobile_offerer",
        mobile_serial: 100,
        server_serial: 200,
    },
    RoleCase {
        name: "mobile_answerer",
        mobile_serial: 200,
        server_serial: 100,
    },
];

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .with_test_writer()
        .try_init()
        .ok();
}

fn generate_test_data(size: usize) -> (Vec<u8>, [u8; 32]) {
    let data: Vec<u8> = (0..size).map(|i| ((i * 31 + 7) % 251) as u8).collect();
    let hash: [u8; 32] = Sha256::digest(&data).into();
    (data, hash)
}

fn network_event(sequence: u64, available: bool, wifi: bool, cellular: bool) -> NetworkEvent {
    NetworkEvent::NetworkPathChanged {
        snapshot: NetworkSnapshot {
            sequence,
            availability: if available {
                NetworkAvailability::Available
            } else {
                NetworkAvailability::Unavailable
            },
            transport: NetworkTransportFlags {
                wifi,
                cellular,
                ethernet: false,
                vpn: false,
                other: false,
            },
            is_expensive: false,
            is_constrained: false,
        },
    }
}

fn wifi_event(sequence: u64) -> NetworkEvent {
    network_event(sequence, true, true, false)
}

fn cellular_event(sequence: u64) -> NetworkEvent {
    network_event(sequence, true, false, true)
}

fn offline_event(sequence: u64) -> NetworkEvent {
    network_event(sequence, false, false, false)
}

async fn setup_mobile_to_server_with_serials(
    _mobile_serial: u64,
    _server_serial: u64,
) -> TestHarness {
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    // One fixed negotiation gives the two legacy role cases: serial 100 is the
    // offerer and serial 200 is the answerer. Each case then exercises traffic
    // in both directions.
    harness.connect(100, 200).await;
    harness
}

/// Start a fresh physical connection from the stable Offerer. The Answerer is
/// intentionally passive and only consumes the resulting ordinary Offer.
async fn reconnect_from_offerer(harness: &TestHarness, offerer_serial: u64, answerer_serial: u64) {
    tokio::time::sleep(Duration::from_secs(1)).await;
    let answerer = harness.peer(answerer_serial).id.clone();
    let offerer = harness.peer(offerer_serial).id.clone();
    let old_offer_generation = harness
        .peer(offerer_serial)
        .rtc
        .generation(&answerer)
        .await
        .unwrap_or_default();
    let old_answer_generation = harness
        .peer(answerer_serial)
        .rtc
        .generation(&offerer)
        .await
        .unwrap_or_default();
    harness.peer(offerer_serial).rtc.close_peer(&answerer).await;
    harness
        .peer(offerer_serial)
        .rtc
        .connect(answerer.clone())
        .await;
    // connect() only enqueues negotiation. The Answerer can still expose its
    // old sendable session until the replacement Offer arrives, so fence both
    // endpoints on a newer physical generation before the fresh business call.
    let (offer_state, answer_state) = tokio::join!(
        harness
            .peer(offerer_serial)
            .rtc
            .wait_state(&answerer, Duration::from_secs(30), |state| state.sendable
                && state.pc_generation > old_offer_generation,),
        harness
            .peer(answerer_serial)
            .rtc
            .wait_state(&offerer, Duration::from_secs(30), |state| state.sendable
                && state.pc_generation > old_answer_generation,),
    );
    offer_state.expect("Offerer replacement must be sendable");
    answer_state.expect("Answerer replacement must be sendable");
}

fn spawn_large_request(
    harness: &TestHarness,
    from_serial: u64,
    to_serial: u64,
    request_id: &str,
    data: Vec<u8>,
    timeout_ms: i64,
) -> tokio::task::JoinHandle<actr_protocol::ActorResult<Bytes>> {
    let gate = harness.peer(from_serial).gate.clone();
    let target_id = harness.peer(to_serial).id.clone();
    let request_id = request_id.to_string();
    tokio::spawn(async move {
        gate.send_request(
            &target_id,
            RpcEnvelope {
                request_id,
                route_key: "test.large_echo".to_string(),
                payload: Some(Bytes::from(data)),
                direction: Some(Direction::Request as i32),
                timeout_ms,
                ..Default::default()
            },
        )
        .await
    })
}

fn assert_payload_integrity(label: &str, response: &Bytes, data: &[u8], expected_hash: &[u8; 32]) {
    assert_eq!(
        response.len(),
        data.len(),
        "{label} response length mismatch"
    );
    let response_hash: [u8; 32] = Sha256::digest(response.as_ref()).into();
    assert_eq!(
        &response_hash, expected_hash,
        "{label} response SHA-256 mismatch"
    );
    assert_eq!(
        response.as_ref(),
        data,
        "{label} response bytes were corrupted"
    );
}

async fn expect_large_request_ok_between(
    harness: &TestHarness,
    from_serial: u64,
    to_serial: u64,
    request_id: &str,
    data: &[u8],
    expected_hash: &[u8; 32],
    timeout: Duration,
) {
    let handle = spawn_large_request(
        harness,
        from_serial,
        to_serial,
        request_id,
        data.to_vec(),
        timeout.as_millis() as i64,
    );
    let response = tokio::time::timeout(timeout + Duration::from_secs(2), handle)
        .await
        .unwrap_or_else(|_| panic!("{request_id} did not complete within {timeout:?}"))
        .expect("large request task panicked")
        .unwrap_or_else(|error| panic!("{request_id} failed: {error}"));
    assert_payload_integrity(request_id, &response, data, expected_hash);
    assert_pending_empty(harness, from_serial, request_id).await;
}

async fn expect_large_request_eventually_ok_between(
    harness: &TestHarness,
    from_serial: u64,
    to_serial: u64,
    request_id: &str,
    data: &[u8],
    expected_hash: &[u8; 32],
    deadline: Duration,
) {
    let stop_at = tokio::time::Instant::now() + deadline;
    let mut attempt = 0;
    loop {
        attempt += 1;
        let attempt_id = format!("{request_id}_{attempt}");
        let handle = spawn_large_request(
            harness,
            from_serial,
            to_serial,
            &attempt_id,
            data.to_vec(),
            5_000,
        );
        match tokio::time::timeout(Duration::from_secs(7), handle).await {
            Ok(Ok(Ok(response))) => {
                assert_payload_integrity(&attempt_id, &response, data, expected_hash);
                assert_pending_empty(harness, from_serial, request_id).await;
                return;
            }
            Ok(Ok(Err(error))) => {
                assert!(
                    is_expected_bounded_transport_failure(&error),
                    "{request_id} got unexpected retry error: {error}"
                );
            }
            Ok(Err(error)) => panic!("{request_id} retry task panicked: {error}"),
            Err(_) => {}
        }
        assert!(
            tokio::time::Instant::now() < stop_at,
            "{request_id} did not recover within {deadline:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn assert_pending_empty(harness: &TestHarness, serial: u64, label: &str) {
    assert_eq!(
        harness.peer(serial).pending_count().await,
        0,
        "{label} should leave no pending request state"
    );
}

fn is_expected_bounded_transport_failure(error: &ActrError) -> bool {
    // DeliveryUncertain is a terminal result for the original operation, but
    // it is an expected bounded outcome for these in-flight recovery tests.
    // The retry helpers below always create a fresh request ID; they never
    // replay the uncertain operation automatically.
    if matches!(error, ActrError::DeliveryUncertain { .. }) {
        return true;
    }

    // Keep the legacy text checks for transport errors whose public mapping is
    // intentionally coarse (for example, a closed DataChannel becomes an
    // Unavailable message). The new uncertain outcome is matched structurally
    // above so it cannot be confused with a generic substring.
    let message = error.to_string().to_ascii_lowercase();
    [
        "connection",
        "request timeout",
        "timed out",
        "closed",
        "recovering",
        "data channel",
        "datachannel",
        "channel error",
        "not opened",
        "timeout",
        "unavailable",
        "not found",
        "no route",
        "all transport candidates exhausted",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn pause_next_multifragment_send_after(
    frag_index: u16,
) -> (
    actr_hyper::test_support::WebRtcFragmentSendHookGuard,
    oneshot::Receiver<WebRtcFragmentSendEvent>,
    Arc<Notify>,
) {
    let (event_tx, event_rx) = oneshot::channel();
    let event_tx = Arc::new(StdMutex::new(Some(event_tx)));
    let release = Arc::new(Notify::new());
    let paused = Arc::new(AtomicBool::new(false));
    let hook: WebRtcFragmentSendHook = {
        let event_tx = Arc::clone(&event_tx);
        let release = Arc::clone(&release);
        let paused = Arc::clone(&paused);
        Arc::new(move |event| {
            let event_tx = Arc::clone(&event_tx);
            let release = Arc::clone(&release);
            let paused = Arc::clone(&paused);
            Box::pin(async move {
                if event.total_frags <= 1 || event.frag_index != frag_index {
                    return;
                }
                if paused.swap(true, Ordering::SeqCst) {
                    return;
                }
                // Publish the pause only after registering its resume waiter.
                // The test may release immediately from another runtime thread.
                let resumed = release.notified();
                tokio::pin!(resumed);
                resumed.as_mut().enable();
                if let Some(tx) = event_tx
                    .lock()
                    .expect("pause hook sender mutex poisoned")
                    .take()
                {
                    let _ = tx.send(event);
                }
                resumed.await;
            })
        })
    };
    (
        install_webrtc_fragment_send_hook_for_test(hook),
        event_rx,
        release,
    )
}

async fn wait_for_send_to_pause(
    event_rx: oneshot::Receiver<WebRtcFragmentSendEvent>,
    label: &str,
) -> WebRtcFragmentSendEvent {
    let event = tokio::time::timeout(Duration::from_secs(10), event_rx)
        .await
        .unwrap_or_else(|_| panic!("{label} did not pause during fragmented send"))
        .expect("fragment pause hook dropped before firing");
    assert!(event.total_frags > 1, "{label} must be multi-fragment");
    event
}

async fn expect_bounded_failure(
    handle: tokio::task::JoinHandle<actr_protocol::ActorResult<Bytes>>,
    label: &str,
    timeout: Duration,
) -> ActrError {
    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(Err(error))) => error,
        Ok(Ok(Ok(response))) => {
            panic!(
                "{label} unexpectedly succeeded with {} bytes",
                response.len()
            )
        }
        Ok(Err(error)) => panic!("{label} task panicked: {error}"),
        Err(_) => panic!("{label} did not fail within {timeout:?}"),
    }
}

async fn expect_bounded_completion(
    handle: tokio::task::JoinHandle<actr_protocol::ActorResult<Bytes>>,
    label: &str,
    data: &[u8],
    expected_hash: &[u8; 32],
    timeout: Duration,
) {
    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(Ok(response))) => assert_payload_integrity(label, &response, data, expected_hash),
        Ok(Ok(Err(error))) => assert!(
            is_expected_bounded_transport_failure(&error),
            "{label} got unexpected bounded failure: {error}"
        ),
        Ok(Err(error)) => panic!("{label} task panicked: {error}"),
        Err(_) => panic!("{label} did not complete within {timeout:?}"),
    }
}

async fn process_mobile_events(harness: &TestHarness, case: RoleCase, events: Vec<NetworkEvent>) {
    let results =
        process_network_event_batch(events, harness.peer(case.mobile_serial).network_processor())
            .await;
    assert!(results.iter().all(|result| result.success));
}

async fn submit_mobile_event(
    handle: NetworkEventHandle,
    event: NetworkEvent,
) -> Result<NetworkEventResult, String> {
    match event {
        NetworkEvent::NetworkPathChanged { snapshot } => {
            handle.handle_network_path_changed(snapshot).await
        }
        NetworkEvent::AppLifecycleChanged { state } => {
            handle.handle_app_lifecycle_changed(state).await
        }
        NetworkEvent::CleanupConnections { reason } => handle.cleanup_connections(reason).await,
        NetworkEvent::ForceReconnect { reason } => handle.force_reconnect(reason).await,
    }
}

fn spawn_mobile_event_storm(handle: NetworkEventHandle) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let batches = [
            vec![
                offline_event(101),
                network_event(102, true, false, false),
                cellular_event(103),
            ],
            vec![
                NetworkEvent::ForceReconnect {
                    reason: ReconnectReason::StaleConnectionSuspected,
                },
                network_event(104, true, false, false),
                wifi_event(105),
            ],
            vec![network_event(106, true, false, true), wifi_event(107)],
        ];
        let mut tasks = Vec::new();
        for batch in batches {
            for event in batch {
                let handle = handle.clone();
                tasks.push(tokio::spawn(async move {
                    submit_mobile_event(handle, event).await
                }));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        for task in tasks {
            let result = task
                .await
                .expect("mobile network event task should not panic")
                .expect("mobile network event should complete");
            assert!(result.success, "mobile network event failed: {result:?}");
        }
    })
}

async fn inflight_short_offline_recovers_original_request(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let request_id = format!("{}_{}_inflight_short_offline", case.name, direction.name);
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let request = spawn_large_request(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &request_id,
            data.clone(),
            30_000,
        );
        wait_for_send_to_pause(event_rx, &request_id).await;
        harness.simulate_disconnect();
        tokio::time::sleep(Duration::from_secs(1)).await;
        harness.simulate_reconnect();
        release_send.notify_waiters();
        drop(hook_guard);
        let response = tokio::time::timeout(Duration::from_secs(30), request)
            .await
            .expect("short offline request hung")
            .expect("short offline request task panicked")
            // This case deliberately verifies that a short outage preserves
            // the original request. DeliveryUncertain would mean the route
            // generation was invalidated, so it must remain a contract failure
            // here rather than being converted into an implicit retry.
            .expect("short offline request should recover");
        assert_payload_integrity(&request_id, &response, &data, &hash);
        assert_pending_empty(&harness, direction.from_serial, &request_id).await;
        harness.shutdown().await;
    }
}

async fn inflight_network_type_switch_recovers_or_retries_once(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let request_id = format!(
            "{}_{}_inflight_network_type_switch",
            case.name, direction.name
        );
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let request = spawn_large_request(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &request_id,
            data.clone(),
            30_000,
        );
        wait_for_send_to_pause(event_rx, &request_id).await;
        let events = vec![cellular_event(1)];
        assert_eq!(
            select_network_recovery_action(&events),
            NetworkRecoveryAction::Restore
        );
        process_mobile_events(&harness, case, events).await;
        release_send.notify_waiters();
        drop(hook_guard);
        let result = tokio::time::timeout(Duration::from_secs(32), request)
            .await
            .expect("network type switch request hung")
            .expect("network type switch request task panicked");
        match result {
            Ok(response) => assert_payload_integrity(&request_id, &response, &data, &hash),
            Err(ActrError::TimedOut | ActrError::DeliveryUncertain { .. }) => {
                let retry_id = format!("{request_id}_retry");
                expect_large_request_ok_between(
                    &harness,
                    direction.from_serial,
                    direction.to_serial,
                    &retry_id,
                    &data,
                    &hash,
                    Duration::from_secs(30),
                )
                .await;
            }
            Err(error) => {
                panic!("network type switch should succeed or time out before one retry: {error}")
            }
        }
        assert_pending_empty(&harness, direction.from_serial, &request_id).await;
        harness.shutdown().await;
    }
}

async fn inflight_long_offline_fails_bounded_then_retries(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let request_id = format!("{}_{}_inflight_long_offline", case.name, direction.name);
        let retry_id = format!("{}_{}_long_offline_retry", case.name, direction.name);
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let request = spawn_large_request(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &request_id,
            data.clone(),
            2_000,
        );
        wait_for_send_to_pause(event_rx, &request_id).await;
        harness.simulate_disconnect();
        process_mobile_events(&harness, case, vec![offline_event(1)]).await;
        let error = expect_bounded_failure(request, &request_id, Duration::from_secs(5)).await;
        assert!(
            matches!(
                &error,
                ActrError::TimedOut | ActrError::DeliveryUncertain { .. }
            ),
            "long offline should time out the in-flight request, got: {error}"
        );
        assert_pending_empty(&harness, direction.from_serial, &request_id).await;
        release_send.notify_waiters();
        drop(hook_guard);
        harness.simulate_reconnect();
        let events = vec![
            NetworkEvent::ForceReconnect {
                reason: ReconnectReason::StaleConnectionSuspected,
            },
            network_event(2, true, false, false),
            cellular_event(3),
        ];
        assert_eq!(
            select_network_recovery_action(&events),
            NetworkRecoveryAction::ForceReconnect
        );
        process_mobile_events(&harness, case, events).await;
        expect_large_request_eventually_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &retry_id,
            &data,
            &hash,
            Duration::from_secs(25),
        )
        .await;
        harness.shutdown().await;
    }
}

async fn inflight_short_background_survives_foreground_restore(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let request_id = format!("{}_{}_inflight_short_background", case.name, direction.name);
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let request = spawn_large_request(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &request_id,
            data.clone(),
            5_000,
        );
        wait_for_send_to_pause(event_rx, &request_id).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let events = vec![network_event(1, true, false, false), wifi_event(2)];
        assert_eq!(
            select_network_recovery_action(&events),
            NetworkRecoveryAction::Restore
        );
        process_mobile_events(&harness, case, events).await;
        release_send.notify_waiters();
        drop(hook_guard);
        expect_bounded_completion(request, &request_id, &data, &hash, Duration::from_secs(8)).await;
        assert_pending_empty(&harness, direction.from_serial, &request_id).await;
        expect_large_request_eventually_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &format!("{request_id}_retry"),
            &data,
            &hash,
            Duration::from_secs(30),
        )
        .await;
        harness.shutdown().await;
    }
}

async fn inflight_long_background_is_bounded_and_retries(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let request_id = format!("{}_{}_inflight_long_background", case.name, direction.name);
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let request = spawn_large_request(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &request_id,
            data.clone(),
            8_000,
        );
        wait_for_send_to_pause(event_rx, &request_id).await;
        let events = vec![
            NetworkEvent::ForceReconnect {
                reason: ReconnectReason::LongBackground,
            },
            network_event(1, true, false, false),
            wifi_event(2),
        ];
        assert_eq!(
            select_network_recovery_action(&events),
            NetworkRecoveryAction::ForceReconnect
        );
        process_mobile_events(&harness, case, events).await;
        release_send.notify_waiters();
        drop(hook_guard);
        expect_bounded_completion(request, &request_id, &data, &hash, Duration::from_secs(12))
            .await;
        assert_pending_empty(&harness, direction.from_serial, &request_id).await;
        expect_large_request_eventually_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &format!("{request_id}_retry"),
            &data,
            &hash,
            Duration::from_secs(25),
        )
        .await;
        harness.shutdown().await;
    }
}

async fn mobile_large_message_baseline_after_recovery(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        expect_large_request_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &format!("{}_{}_mobile_large_baseline", case.name, direction.name),
            &data,
            &hash,
            Duration::from_secs(30),
        )
        .await;
        harness.shutdown().await;
    }
}

async fn mobile_data_chunk_channel_close_reports_delivery_uncertain(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let target_id = harness.peer(direction.to_serial).id.clone();
        let stream = DataChunk {
            stream_id: format!("{}-{}-large-data-chunk", case.name, direction.name),
            sequence: 7,
            payload: Bytes::from(vec![0x5a; LARGE_PAYLOAD_SIZE]),
            metadata: Vec::new(),
            timestamp_ms: Some(0),
        };
        let payload = Bytes::from(stream.encode_to_vec());
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let gate = harness.peer(direction.from_serial).gate.clone();
        let target_for_send = target_id.clone();
        let stream_id = stream.stream_id.clone();
        let operation_id = format!("{stream_id}#{}", stream.sequence);
        let send = tokio::spawn(async move {
            gate.send_data_chunk(
                &target_for_send,
                PayloadType::StreamReliable,
                &stream_id,
                payload,
                &operation_id,
            )
            .await
        });
        wait_for_send_to_pause(event_rx, "data chunk channel close").await;
        harness
            .peer(direction.from_serial)
            .rtc
            .close_lane(&target_id, PayloadType::StreamReliable)
            .await
            .expect("close active StreamReliable lane");
        release_send.notify_waiters();
        drop(hook_guard);
        let result = tokio::time::timeout(Duration::from_secs(10), send)
            .await
            .expect("closed DataChunk send must be bounded")
            .expect("DataChunk send task must not panic");
        let error = result.expect_err("mid-flight channel close must report uncertainty");
        assert!(
            error
                .to_string()
                .to_ascii_lowercase()
                .contains("delivery uncertain"),
            "channel close must report delivery uncertainty: {error}"
        );
        let (retry_data, retry_hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        expect_large_request_eventually_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &format!("{}_{}_after_channel_close", case.name, direction.name),
            &retry_data,
            &retry_hash,
            Duration::from_secs(25),
        )
        .await;
        harness.shutdown().await;
    }
}

async fn inflight_data_chunk_long_offline_is_bounded_or_delivery_uncertain(case: RoleCase) {
    for direction in case.directions() {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let target_id = harness.peer(direction.to_serial).id.clone();
        let stream = DataChunk {
            stream_id: format!(
                "{}-{}-inflight-long-offline-stream",
                case.name, direction.name
            ),
            sequence: 11,
            payload: Bytes::from(vec![0x33; LARGE_PAYLOAD_SIZE]),
            metadata: Vec::new(),
            timestamp_ms: Some(0),
        };
        let payload = Bytes::from(stream.encode_to_vec());
        let (hook_guard, event_rx, release_send) = pause_next_multifragment_send_after(1);
        let gate = harness.peer(direction.from_serial).gate.clone();
        let target_for_send = target_id.clone();
        let stream_id = stream.stream_id.clone();
        let operation_id = format!("{stream_id}#{}", stream.sequence);
        let send = tokio::spawn(async move {
            gate.send_data_chunk(
                &target_for_send,
                PayloadType::StreamReliable,
                &stream_id,
                payload,
                &operation_id,
            )
            .await
        });
        wait_for_send_to_pause(event_rx, "long offline data stream").await;
        harness.simulate_disconnect();
        process_mobile_events(&harness, case, vec![offline_event(1)]).await;
        release_send.notify_waiters();
        drop(hook_guard);
        let send_result = tokio::time::timeout(Duration::from_secs(20), send)
            .await
            .expect("in-flight DataChunk send should not hang after long offline")
            .expect("in-flight DataChunk task should not panic");
        if let Err(error) = send_result {
            assert!(
                is_expected_bounded_transport_failure(&error),
                "unexpected in-flight DataChunk failure: {error}"
            );
        } else {
            // SCTP acceptance is not delivery confirmation. Preserve the old
            // terminal give-up branch by explicitly closing the recovering
            // RTC controller generation.
            harness
                .peer(direction.from_serial)
                .rtc
                .close_peer(&target_id)
                .await;
        }
        harness.simulate_reconnect();
        let events = vec![
            NetworkEvent::ForceReconnect {
                reason: ReconnectReason::StaleConnectionSuspected,
            },
            network_event(2, true, false, false),
            cellular_event(3),
        ];
        assert_eq!(
            select_network_recovery_action(&events),
            NetworkRecoveryAction::ForceReconnect
        );
        process_mobile_events(&harness, case, events).await;
        let (retry_data, retry_hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        expect_large_request_eventually_ok_between(
            &harness,
            direction.from_serial,
            direction.to_serial,
            &format!(
                "{}_{}_after_data_chunk_long_offline",
                case.name, direction.name
            ),
            &retry_data,
            &retry_hash,
            Duration::from_secs(25),
        )
        .await;
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_event_storm_during_call_and_data_chunk_does_not_hang() {
    let _fragment_hook_isolation = fragment_hook_test_lock().lock().await;
    init_tracing();
    for case in ROLE_CASES {
        let harness =
            setup_mobile_to_server_with_serials(case.mobile_serial, case.server_serial).await;
        let target = harness.peer(case.server_serial).id.clone();
        let mobile_role = harness
            .peer(case.mobile_serial)
            .rtc
            .role(&target)
            .await
            .expect("mobile controller role must be available after connect");
        let (offerer_serial, answerer_serial) = if mobile_role == TestRtcRole::Offerer {
            (case.mobile_serial, case.server_serial)
        } else {
            (case.server_serial, case.mobile_serial)
        };
        let (data, hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        let processor: Arc<dyn NetworkEventProcessor> =
            harness.peer(case.mobile_serial).network_processor();
        let (network_handle, _facts, network_shutdown, network_task) =
            spawn_network_event_supervisor(processor);
        harness.simulate_disconnect();
        tokio::time::sleep(Duration::from_secs(1)).await;
        harness.simulate_reconnect();
        let storm_task = spawn_mobile_event_storm(network_handle);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let call_id = format!("{}_mobile_event_storm_concurrent_call", case.name);
        let call = spawn_large_request(
            &harness,
            case.mobile_serial,
            case.server_serial,
            &call_id,
            data.clone(),
            5_000,
        );
        let stream_id = format!("{}-mobile-event-storm-concurrent-stream", case.name);
        let stream = harness.peer(case.mobile_serial).gate.send_data_chunk(
            &target,
            PayloadType::StreamReliable,
            &stream_id,
            Bytes::from(vec![0x42; LARGE_PAYLOAD_SIZE]),
            &stream_id,
        );
        let (storm_result, call_result, stream_result) =
            tokio::time::timeout(Duration::from_secs(25), async {
                tokio::join!(storm_task, call, stream)
            })
            .await
            .expect("event storm and in-flight sends must remain bounded");
        storm_result.expect("mobile event storm task should not panic");
        match call_result.expect("call task must not panic") {
            Ok(response) => assert_payload_integrity(&call_id, &response, &data, &hash),
            Err(error) => assert!(is_expected_bounded_transport_failure(&error)),
        }
        if let Err(error) = stream_result {
            assert!(is_expected_bounded_transport_failure(&error));
        }
        assert_pending_empty(&harness, case.mobile_serial, &call_id).await;

        // Await both replacement sessions before checking the single fresh
        // post-storm request. No uncertain in-flight operation is replayed.
        reconnect_from_offerer(&harness, offerer_serial, answerer_serial).await;

        let (retry_data, retry_hash) = generate_test_data(LARGE_PAYLOAD_SIZE);
        // Preserve the legacy post-storm contract: one fresh request must
        // establish/rebuild the RTC peer controller and succeed.
        expect_large_request_ok_between(
            &harness,
            case.mobile_serial,
            case.server_serial,
            &format!("{}_after_mobile_event_storm_retry_once", case.name),
            &retry_data,
            &retry_hash,
            Duration::from_secs(30),
        )
        .await;
        network_shutdown.cancel();
        network_task
            .await
            .expect("network event reconciler task should not panic");
        harness.shutdown().await;
    }
}

// The interruption matrix was one mega-test iterating every scenario inside
// a single `#[tokio::test]`, which made it a permanent SLOW outlier (>60s,
// 32 real-PC harness setups) and forced a full-matrix rerun on any single
// scenario's flake (docs/webrtc-v020-test-plan.zh.md §10). Each scenario is
// now its own test: nextest runs them in parallel processes, one flake only
// retries its own scenario, and every test stays under the 60s SLOW budget.
// The `fragment_hook_test_lock` isolation is kept per test: nextest runs
// each test in its own process, so the global hook never crosses tests.
macro_rules! interruption_test {
    ($name:ident, $scenario:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn $name() {
            let _fragment_hook_isolation = fragment_hook_test_lock().lock().await;
            init_tracing();
            for case in ROLE_CASES {
                $scenario(case).await;
            }
        }
    };
}

interruption_test!(
    test_mobile_large_message_baseline_after_recovery,
    mobile_large_message_baseline_after_recovery
);
interruption_test!(
    test_mobile_inflight_network_type_switch_recovers_or_retries_once,
    inflight_network_type_switch_recovers_or_retries_once
);
interruption_test!(
    test_mobile_inflight_short_offline_recovers_original_request,
    inflight_short_offline_recovers_original_request
);
interruption_test!(
    test_mobile_inflight_long_offline_fails_bounded_then_retries,
    inflight_long_offline_fails_bounded_then_retries
);
interruption_test!(
    test_mobile_inflight_short_background_survives_foreground_restore,
    inflight_short_background_survives_foreground_restore
);
interruption_test!(
    test_mobile_inflight_long_background_is_bounded_and_retries,
    inflight_long_background_is_bounded_and_retries
);
interruption_test!(
    test_mobile_data_chunk_channel_close_reports_delivery_uncertain,
    mobile_data_chunk_channel_close_reports_delivery_uncertain
);
interruption_test!(
    test_mobile_inflight_data_chunk_long_offline_is_bounded_or_delivery_uncertain,
    inflight_data_chunk_long_offline_is_bounded_or_delivery_uncertain
);
