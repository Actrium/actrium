//! VNet black-box coverage for the responsive recovery supervisor and the
//! RTC per-peer controllers.

use std::sync::Arc;
use std::time::Duration;

use actr_hyper::lifecycle::{
    AppLifecycleState, NetworkAvailability, NetworkEventProcessor, NetworkSnapshot,
    NetworkTransportFlags, SignalingFactLostCause, SignalingFactOrigin,
};
use actr_hyper::test_support::{TestHarness, TestSignalingServer, spawn_network_event_supervisor};
use actr_protocol::prost::Message as _;
use actr_protocol::{DataChunk, PayloadType};

fn snapshot(sequence: u64, availability: NetworkAvailability, wifi: bool) -> NetworkSnapshot {
    NetworkSnapshot {
        sequence,
        availability,
        transport: NetworkTransportFlags {
            wifi,
            cellular: !wifi && availability == NetworkAvailability::Available,
            ethernet: false,
            vpn: false,
            other: false,
        },
        is_expensive: false,
        is_constrained: false,
    }
}

async fn setup_bidirectional_vnet() -> TestHarness {
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.connect(100, 200).await;
    rpc(&harness, 100, 200, "setup-a-b").await;
    rpc(&harness, 200, 100, "setup-b-a").await;
    harness
}

async fn rpc(harness: &TestHarness, from: u64, to: u64, request_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let request =
            harness
                .peer(from)
                .spawn_request(to, &format!("{request_id}-{attempt}"), 3_000);
        if let Ok(Ok(Ok(response))) = tokio::time::timeout(Duration::from_secs(4), request).await {
            assert_eq!(response.as_ref(), b"pong");
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{request_id}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn bidirectional_rpc(harness: &TestHarness, prefix: &str) {
    rpc(harness, 100, 200, &format!("{prefix}-a-b")).await;
    rpc(harness, 200, 100, &format!("{prefix}-b-a")).await;
}

async fn send_stream(harness: &TestHarness, from: u64, to: u64, stream_id: &str) {
    let chunk = DataChunk {
        stream_id: stream_id.into(),
        sequence: 1,
        payload: bytes::Bytes::from_static(b"stream-payload"),
        metadata: Vec::new(),
        timestamp_ms: Some(0),
    };
    let operation_id = format!("{stream_id}#{}", chunk.sequence);
    harness
        .peer(from)
        .gate
        .send_data_chunk(
            &harness.peer(to).id,
            PayloadType::StreamReliable,
            stream_id,
            bytes::Bytes::from(chunk.encode_to_vec()),
            &operation_id,
        )
        .await
        .expect("stream send");
    let (_, lane, bytes) =
        tokio::time::timeout(Duration::from_secs(5), harness.peer(to).receive_payload())
            .await
            .expect("stream receive timeout")
            .expect("stream payload");
    assert_eq!(lane, PayloadType::StreamReliable);
    let received = DataChunk::decode(bytes.as_ref()).expect("DataChunk");
    assert_eq!(received.stream_id, stream_id);
    assert_eq!(received.payload.as_ref(), b"stream-payload");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn passive_data_chunk_observation_does_not_block_after_raw_queue_fills() {
    const CHUNK_COUNT: usize = 600;
    const STREAM_ID: &str = "passive-observer-over-raw-queue-capacity";

    let harness = setup_bidirectional_vnet().await;
    let target = harness.peer(200).id.clone();

    // Intentionally do not call receive_payload(): the passive DataChunk
    // observer must keep dispatching after the separate raw queue is full.
    for index in 0..CHUNK_COUNT {
        let sequence = index as u64 + 1;
        let chunk = DataChunk {
            stream_id: STREAM_ID.into(),
            sequence,
            payload: bytes::Bytes::from_static(b"observed"),
            metadata: Vec::new(),
            timestamp_ms: Some(sequence as i64),
        };
        let operation_id = format!("{STREAM_ID}#{sequence}");
        harness
            .peer(100)
            .gate
            .send_data_chunk(
                &target,
                PayloadType::StreamReliable,
                STREAM_ID,
                bytes::Bytes::from(chunk.encode_to_vec()),
                &operation_id,
            )
            .await
            .expect("DataChunk send");

        // Pace against the observer so this exercises the raw-payload queue
        // limit without overflowing the separate RTC ingress budget.
        let expected = index + 1;
        assert!(
            harness
                .peer(200)
                .wait_data_chunk_count(STREAM_ID, expected, Duration::from_secs(5))
                .await,
            "passive observer stopped at {} while waiting for {expected}",
            harness.peer(200).data_chunk_count(STREAM_ID)
        );
    }

    assert!(
        harness
            .peer(200)
            .wait_data_chunk_count(STREAM_ID, CHUNK_COUNT, Duration::from_secs(20))
            .await,
        "passive observer should receive all chunks even when raw payloads are not drained"
    );
    rpc(&harness, 100, 200, "rpc-after-passive-observation-overflow").await;
    harness.shutdown().await;
}

fn supervisor(
    harness: &TestHarness,
    serial: u64,
) -> (
    actr_hyper::lifecycle::NetworkEventHandle,
    tokio_util::sync::CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let processor: Arc<dyn NetworkEventProcessor> = harness.peer(serial).network_processor();
    let (handle, _facts, shutdown, task) = spawn_network_event_supervisor(processor);
    (handle, shutdown, task)
}

async fn stop_supervisor(
    shutdown: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
) {
    shutdown.cancel();
    task.await.expect("supervisor task");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn short_foreground_preserves_inflight_restore_and_recovers_vnet_bidirectionally() {
    let harness = setup_bidirectional_vnet().await;
    let (handle, shutdown, task) = supervisor(&harness, 100);
    harness.simulate_disconnect();
    harness.simulate_reconnect();
    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Available, true))
        .await
        .expect("restore event");
    handle
        .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
            background_duration_ms: 1_000,
        })
        .await
        .expect("short foreground");
    bidirectional_rpc(&harness, "short-foreground").await;
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn long_foreground_preempts_inflight_restore_and_rebuilds_vnet_bidirectionally() {
    let harness = setup_bidirectional_vnet().await;
    let target = harness.peer(200).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    let (handle, shutdown, task) = supervisor(&harness, 100);
    harness.simulate_disconnect();
    harness.simulate_reconnect();
    handle
        .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
            background_duration_ms: 120_000,
        })
        .await
        .expect("long foreground");
    harness.peer(100).rtc.force_reconnect();
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old
        })
        .await
        .expect("hard recovery generation");
    bidirectional_rpc(&harness, "long-foreground").await;
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vnet_offline_rollback_before_grace_keeps_session_and_sends_no_sdp() {
    let harness = setup_bidirectional_vnet().await;
    let target = harness.peer(200).id.clone();
    let old_generation = harness.peer(100).rtc.generation(&target).await.unwrap();
    let before = harness.ice_restart_count();
    let (handle, shutdown, task) = supervisor(&harness, 100);
    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Unavailable, false))
        .await
        .expect("offline event");
    handle
        .handle_network_path_changed(snapshot(2, NetworkAvailability::Available, true))
        .await
        .expect("rollback event");
    harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(15))
        .await;
    assert_eq!(
        harness.peer(100).rtc.generation(&target).await,
        Some(old_generation)
    );
    assert!(harness.ice_restart_count() <= before + 1);
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vnet_committed_offline_restores_once_and_recovers_bidirectionally() {
    let harness = setup_bidirectional_vnet().await;
    harness.reset_counters();
    let (handle, shutdown, task) = supervisor(&harness, 100);
    harness.simulate_disconnect();
    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Unavailable, false))
        .await
        .expect("offline");
    harness.simulate_reconnect();
    handle
        .handle_network_path_changed(snapshot(2, NetworkAvailability::Available, true))
        .await
        .expect("restore");
    bidirectional_rpc(&harness, "committed-offline").await;
    assert!(harness.ice_restart_count() <= 2, "restart storm");
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepted_event_while_webrtc_not_ready_fails_fast_without_pending_leak() {
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness.server.pause_forwarding();
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.connect(target).await;
    let (handle, shutdown, task) = supervisor(&harness, 100);
    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Available, true))
        .await
        .expect("accepted event");
    let request = harness.peer(100).spawn_request(200, "not-ready", 30_000);
    let error = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("fast failure")
        .expect("request task")
        .expect_err("initial connection is not ready");
    assert!(
        error
            .to_string()
            .to_ascii_lowercase()
            .contains("connection")
    );
    assert_eq!(harness.peer(100).pending_count().await, 0);
    harness.server.resume_forwarding();
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cleanup_request_and_recovery_overlap_cannot_revive_stale_session() {
    let harness = setup_bidirectional_vnet().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let retired = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.peer(100).rtc.force_reconnect();
    harness.peer(100).rtc.close_peer(&target).await;
    harness.peer(200).rtc.close_peer(&source).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(harness.peer(100).rtc.state(&target).await.is_none());
    harness.peer(100).rtc.connect(target.clone()).await;
    let current = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    assert!(current.pc_generation > retired);
    rpc(&harness, 100, 200, "cleanup-recovery-race").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_real_signaling_restore_retries_after_same_endpoint_returns() {
    let mut harness = setup_bidirectional_vnet().await;
    let processor: Arc<dyn NetworkEventProcessor> = harness.peer(100).network_processor();
    let (handle, sink, shutdown, task) = spawn_network_event_supervisor(processor);
    harness
        .peer(100)
        .signaling_client
        .set_supervisor_fact_sink(sink.clone());
    sink.signaling_generation_committed(1, SignalingFactOrigin::External);

    let signaling_port = harness.server.port();
    harness.server.shutdown().await;
    let disconnected_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while harness.peer(100).signaling_client.is_connected() {
        assert!(
            tokio::time::Instant::now() < disconnected_deadline,
            "signaling client did not observe the real server shutdown"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    sink.signaling_generation_lost(1, SignalingFactLostCause::RemoteReset);
    handle
        .handle_network_path_changed(snapshot(1, NetworkAvailability::Available, true))
        .await
        .expect("online recovery should be accepted while signaling is down");
    tokio::time::sleep(Duration::from_millis(750)).await;
    assert!(
        !harness.peer(100).signaling_client.is_connected(),
        "the first real signaling restore should fail while the endpoint is down"
    );

    harness.server = TestSignalingServer::start_on_port(signaling_port)
        .await
        .expect("same signaling endpoint returns");
    let reconnected_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !harness.peer(100).signaling_client.is_connected() {
        assert!(
            tokio::time::Instant::now() < reconnected_deadline,
            "supervisor did not retry the signaling restore"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bidirectional_rpc(&harness, "signaling-returned").await;
    stop_supervisor(shutdown, task).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recovered_vnet_delivers_reliable_streams_bidirectionally() {
    let harness = setup_bidirectional_vnet().await;
    harness.simulate_disconnect();
    harness.simulate_reconnect();
    bidirectional_rpc(&harness, "stream-recovery-prime").await;
    send_stream(&harness, 100, 200, "stream-a-b").await;
    send_stream(&harness, 200, 100, "stream-b-a").await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exhausted_real_ice_restart_falls_back_to_new_session_bidirectionally() {
    let harness = setup_bidirectional_vnet().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.simulate_disconnect();
    harness.peer(100).rtc.network_available();
    harness.peer(100).rtc.force_reconnect();
    harness.peer(200).rtc.force_reconnect();
    harness.server.resume_forwarding();
    harness.vnet.as_ref().unwrap().unblock_network();
    let ready = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old
        })
        .await
        .expect("hard recovery after exhausted restart");
    assert!(ready.pc_generation > old);
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    bidirectional_rpc(&harness, "hard-fallback").await;
    harness.shutdown().await;
}
