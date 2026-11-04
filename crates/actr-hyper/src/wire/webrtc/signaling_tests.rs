use super::*;
use crate::lifecycle::test_recovery_policy as tp;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering as UsizeOrdering};

/// Simple fake SignalingClient implementation for testing the reconnect helper.
struct FakeSignalingClient {
    event_tx: broadcast::Sender<SignalingEvent>,
    connected: AtomicBool,
    connect_calls: Arc<AtomicUsize>,
    actor_id: tokio::sync::Mutex<Option<ActrId>>,
    credential_state: tokio::sync::Mutex<Option<CredentialState>>,
}

#[async_trait]
impl SignalingClient for FakeSignalingClient {
    async fn connect(&self) -> NetworkResult<()> {
        self.connect_calls.fetch_add(1, UsizeOrdering::SeqCst);
        Ok(())
    }

    async fn disconnect(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn send_register_request(
        &self,
        _request: RegisterRequest,
    ) -> NetworkResult<RegisterResponse> {
        unimplemented!("not needed in tests");
    }

    async fn send_unregister_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _reason: Option<String>,
    ) -> NetworkResult<UnregisterResponse> {
        unimplemented!("not needed in tests");
    }

    async fn send_heartbeat(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _availability: ServiceAvailabilityState,
        _power_reserve: f32,
        _mailbox_backlog: f32,
    ) -> NetworkResult<Pong> {
        unimplemented!("not needed in tests");
    }

    async fn send_route_candidates_request(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _request: RouteCandidatesRequest,
    ) -> NetworkResult<RouteCandidatesResponse> {
        unimplemented!("not needed in tests");
    }

    async fn get_signing_key(
        &self,
        _actor_id: ActrId,
        _credential: AIdCredential,
        _key_id: u32,
    ) -> NetworkResult<(u32, Vec<u8>)> {
        unimplemented!("not needed in tests");
    }

    async fn send_envelope(&self, _envelope: SignalingEnvelope) -> NetworkResult<()> {
        unimplemented!("not needed in tests");
    }

    async fn receive_envelope(&self) -> NetworkResult<Option<SignalingEnvelope>> {
        unimplemented!("not needed in tests");
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn get_stats(&self) -> SignalingStats {
        SignalingStats::default()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<SignalingEvent> {
        self.event_tx.subscribe()
    }

    async fn set_actor_id(&self, actor_id: ActrId) {
        *self.actor_id.lock().await = Some(actor_id);
    }

    async fn set_credential_state(&self, credential_state: CredentialState) {
        *self.credential_state.lock().await = Some(credential_state);
    }

    async fn clear_identity(&self) {
        *self.actor_id.lock().await = None;
        *self.credential_state.lock().await = None;
    }
}

fn make_fake_client() -> Arc<FakeSignalingClient> {
    let (event_tx, _erx) = broadcast::channel(64);
    Arc::new(FakeSignalingClient {
        event_tx,
        connected: AtomicBool::new(false),
        connect_calls: Arc::new(AtomicUsize::new(0)),
        actor_id: tokio::sync::Mutex::new(None),
        credential_state: tokio::sync::Mutex::new(None),
    })
}

/// Helper: create a minimal SignalingConfig with an unreachable URL.
fn make_config() -> SignalingConfig {
    SignalingConfig {
        server_url: Url::parse("ws://127.0.0.1:1/signaling/ws").unwrap(),
        connection_timeout: 2,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig::default(),
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }
}

/// Helper: create a WebSocketSignalingClient wrapped in Arc
fn make_ws_client(config: SignalingConfig) -> Arc<WebSocketSignalingClient> {
    Arc::new(WebSocketSignalingClient::new(config))
}

/// Publish the socket state a committed connection would have. The
/// send-failure teardown is fenced on this published `Connected` state, so
/// tests exercising send failures must run against a live published
/// generation rather than a bare `connected` flag.
fn publish_connected_socket_state(client: &WebSocketSignalingClient) {
    let generation = client.connection_generation.load(Ordering::Acquire);
    *client
        .socket_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = SignalingSocketState::Connected {
        generation,
        action_id: None,
    };
    client.connected.store(true, Ordering::Release);
}

fn attach_supervisor_fact_sink(
    client: &WebSocketSignalingClient,
) -> mpsc::UnboundedReceiver<tp::Input> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(tx));
    assert_reconnect_config_fact(&mut rx, &client.config.reconnect_config);
    rx
}

fn assert_reconnect_config_fact(
    rx: &mut mpsc::UnboundedReceiver<tp::Input>,
    config: &ReconnectConfig,
) {
    assert_eq!(
        rx.try_recv(),
        Ok(tp::Input::SignalingRecoveryConfigChanged {
            max_attempts: config.max_attempts,
            initial_delay: Duration::from_secs(config.initial_delay),
            max_delay: Duration::from_secs(config.max_delay),
            backoff_multiplier_bits: config.backoff_multiplier.to_bits(),
        }),
        "installing the concrete client must transfer reconnect policy ownership to the supervisor"
    );
}

async fn next_supervisor_fact(rx: &mut mpsc::UnboundedReceiver<tp::Input>) -> tp::Input {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("signaling should emit a supervisor fact promptly")
        .expect("supervisor fact channel should remain open")
}

#[test]
fn late_supervisor_fact_sink_seeds_the_committed_generation() {
    let client = make_ws_client(make_config());
    let generation = 23;
    {
        let _commit_guard = client.lock_connection_commit();
        client
            .connection_generation
            .store(generation, Ordering::Release);
        client.connected.store(true, Ordering::Release);
        *client
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = SignalingSocketState::Connected {
            generation,
            action_id: None,
        };
    }

    let (tx, mut facts) = mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(tx));
    assert_reconnect_config_fact(&mut facts, &client.config.reconnect_config);
    assert_eq!(
        facts.try_recv(),
        Ok(tp::Input::SignalingGenerationCommitted {
            generation,
            origin: tp::SignalingOrigin::External,
        }),
        "a sink installed after the handshake must learn the live generation before a later loss"
    );
}

#[test]
fn supervisor_sink_receives_the_public_reconnect_window_configuration() {
    let mut config = make_config();
    config.reconnect_config = ReconnectConfig {
        enabled: true,
        max_attempts: 3,
        initial_delay: 2,
        max_delay: 7,
        backoff_multiplier: 1.5,
    };
    let client = make_ws_client(config);
    let (tx, mut facts) = mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(tx));

    assert_reconnect_config_fact(&mut facts, &client.config.reconnect_config);
    assert!(
        facts.try_recv().is_err(),
        "a disconnected client has no seed generation"
    );
}

#[tokio::test]
async fn probe_alive_times_out_when_sink_lock_is_stalled() {
    let client = make_ws_client(make_config());
    client.connected.store(true, Ordering::Release);

    let _sink_guard = client.ws_sink.lock().await;

    let result = tokio::time::timeout(
        Duration::from_millis(250),
        client.probe_alive(Duration::from_millis(20)),
    )
    .await
    .expect("probe should be bounded by its own timeout");

    let err = result.expect_err("stalled sink lock should fail the probe");
    assert!(
        err.to_string()
            .contains("Timed out sending signaling probe ping"),
        "unexpected error: {err}"
    );
    assert!(
        !client.is_connected(),
        "stalled probe send should mark signaling disconnected"
    );
    assert_eq!(client.get_stats().disconnections, 1);
    assert!(
        client.pending_pongs.lock().await.is_empty(),
        "failed probe send should remove its pending pong waiter"
    );
}

#[tokio::test]
async fn probe_send_and_pong_wait_share_one_absolute_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("server should accept");
        let _ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("server handshake should complete");
        std::future::pending::<()>().await;
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test URL should parse");
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    client
        .connect()
        .await
        .expect("test signaling socket should connect");

    let sink_guard = client.ws_sink.lock().await;
    let probe_client = client.clone();
    let probe =
        tokio::spawn(async move { probe_client.probe_alive(Duration::from_millis(100)).await });
    tokio::time::sleep(Duration::from_millis(70)).await;
    drop(sink_guard);

    let result = tokio::time::timeout(Duration::from_millis(60), probe)
        .await
        .expect("probe must spend one 100ms budget across send and pong wait")
        .expect("probe task should not panic");
    assert!(result.is_err(), "server deliberately withholds pong");

    client.disconnect().await.ok();
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn concurrent_connect_waiter_uses_the_active_attempt_deadline() {
    let mut config = make_config();
    config.connection_timeout = 30;
    let client = make_ws_client(config);
    client.connecting.store(true, Ordering::Release);

    let waiter_client = client.clone();
    let waiter = tokio::spawn(async move { waiter_client.wait_for_connection_result(0).await });
    tokio::task::yield_now().await;

    tokio::time::advance(
        Duration::from_secs(CONCURRENT_CONNECT_WAIT_TIMEOUT_SECS) + Duration::from_millis(1),
    )
    .await;
    tokio::task::yield_now().await;
    assert!(
        !waiter.is_finished(),
        "a waiter must not time out before the active 30-second handshake attempt"
    );

    client.connected.store(true, Ordering::Release);
    client.connection_attempt_notify.notify_waiters();
    waiter
        .await
        .expect("waiter task should not panic")
        .expect("waiter should observe the shared attempt success");
}

#[tokio::test(start_paused = true)]
async fn disconnect_lock_waits_share_one_absolute_deadline() {
    let client = make_ws_client(make_config());
    let ping_guard = client.ping_task.lock().await;
    let receiver_guard = client.receiver_task.lock().await;

    let disconnect_client = client.clone();
    let disconnect = tokio::spawn(async move { disconnect_client.disconnect_internal(true).await });
    tokio::task::yield_now().await;

    tokio::time::advance(
        Duration::from_secs(DISCONNECT_LOCK_TIMEOUT_SECS) + Duration::from_millis(1),
    )
    .await;
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
    assert!(
        disconnect.is_finished(),
        "disconnect must not restart a full timeout for every blocked lock"
    );

    drop(receiver_guard);
    drop(ping_guard);
    disconnect
        .await
        .expect("disconnect task should not panic")
        .expect("disconnect should continue after its shared teardown deadline");
}

#[tokio::test]
async fn recovery_disconnect_marks_unavailable_before_reply_waiters_wake() {
    let client = make_ws_client(make_config());
    publish_connected_socket_state(&client);
    let (reply_tx, reply_rx) = oneshot::channel();
    client
        .pending_replies
        .lock()
        .await
        .insert("pending-read".to_string(), reply_tx);

    // Hold the next cleanup lock so a dropped reply waiter runs while
    // disconnect is still tearing down the old socket. This deterministically
    // exposes the retry-before-disconnect race seen under concurrent CI load.
    let pong_guard = client.pending_pongs.lock().await;
    let disconnect_client = Arc::clone(&client);
    let disconnect = tokio::spawn(async move { disconnect_client.disconnect_for_recovery().await });

    tokio::time::timeout(Duration::from_secs(2), reply_rx)
        .await
        .expect("disconnect should retire the pending reply promptly")
        .expect_err("the old socket can no longer deliver this reply");
    assert!(client.idempotent_retry_allowed());
    assert!(
        !client.is_connected(),
        "a dropped reply waiter must not retry on the socket being disconnected"
    );

    drop(pong_guard);
    tokio::time::timeout(Duration::from_secs(2), disconnect)
        .await
        .expect("disconnect should finish after cleanup is released")
        .expect("disconnect task should not panic")
        .expect("recovery disconnect should succeed");
    assert_eq!(client.get_stats().disconnections, 1);
}

#[tokio::test(start_paused = true)]
async fn send_envelope_times_out_while_waiting_for_sink_lock() {
    let client = make_ws_client(make_config());
    publish_connected_socket_state(&client);
    let mut events = client.subscribe_events();
    let _sink_guard = client.ws_sink.lock().await;

    let send_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move { client.send_envelope(SignalingEnvelope::default()).await })
    };
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(SIGNALING_SEND_TIMEOUT_SECS + 1)).await;

    let err = send_task
        .await
        .expect("send task should join")
        .expect_err("stalled sink lock should time out");
    assert!(
        err.to_string().contains("sink lock/send timed out"),
        "unexpected error: {err}"
    );
    assert!(
        !client.is_connected(),
        "a timed-out signaling commit should mark the socket disconnected"
    );
    assert_eq!(client.get_stats().disconnections, 1);
    assert_eq!(client.get_stats().errors, 1);
    assert!(matches!(
        events.recv().await,
        Ok(SignalingEvent::Disconnected {
            reason: DisconnectReason::SendTimeout
        })
    ));
}

/// RFC-0419 §6 R4 double-sided deadline boundary for the signaling send
/// timeout: one millisecond before `SIGNALING_SEND_TIMEOUT_SECS` the send is
/// still pending; once the virtual clock reaches the deadline it times out
/// (fires no earlier than its instant).
#[tokio::test(start_paused = true)]
async fn send_envelope_timeout_fires_no_earlier_than_its_deadline() {
    let client = make_ws_client(make_config());
    publish_connected_socket_state(&client);
    let _sink_guard = client.ws_sink.lock().await;

    let send_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move { client.send_envelope(SignalingEnvelope::default()).await })
    };
    // Let the send reach the stalled sink lock so its timeout timer is armed
    // at the current virtual instant.
    tokio::task::yield_now().await;

    tokio::time::advance(
        Duration::from_secs(SIGNALING_SEND_TIMEOUT_SECS) - Duration::from_millis(1),
    )
    .await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(
        !send_task.is_finished(),
        "signaling send must stay pending one tick before its deadline"
    );

    tokio::time::advance(Duration::from_millis(1)).await;
    let err = send_task
        .await
        .expect("send task should join")
        .expect_err("reaching the deadline must time out the send");
    assert!(
        err.to_string().contains("sink lock/send timed out"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn send_envelope_failure_publishes_disconnect_fact_without_local_retry() {
    let client = make_ws_client(make_config());
    let mut facts = attach_supervisor_fact_sink(&client);
    client.connection_generation.store(23, Ordering::Release);
    publish_connected_socket_state(&client);
    let mut events = client.subscribe_events();

    let err = client
        .send_envelope(SignalingEnvelope::default())
        .await
        .expect_err("a connected flag without a WebSocket sink should fail");
    assert!(
        err.to_string().contains("Not connected"),
        "unexpected error: {err}"
    );
    assert!(!client.is_connected());
    assert_eq!(client.get_stats().disconnections, 1);
    assert_eq!(client.get_stats().errors, 1);
    assert!(matches!(
        events.recv().await,
        Ok(SignalingEvent::Disconnected {
            reason: DisconnectReason::SendFailed
        })
    ));
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationLost {
            generation: 23,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        "an envelope failure must report the exact live generation as a remote reset"
    );
}

#[tokio::test]
async fn receiver_stream_end_reports_the_committed_generation_as_remote_reset() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
    let (close_server_tx, close_server_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept tcp connection");
        let websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete websocket handshake");
        server_ready_tx
            .send(())
            .expect("client should still wait for server readiness");
        let _ = close_server_rx.await;
        drop(websocket);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let (fact_tx, mut fact_rx) = tokio::sync::mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(fact_tx));
    assert_reconnect_config_fact(&mut fact_rx, &client.config.reconnect_config);

    client
        .connect()
        .await
        .expect("client should establish the test websocket");
    server_ready_rx
        .await
        .expect("test server should report handshake completion");
    let generation = client.connection_generation.load(Ordering::Acquire);
    assert_eq!(
        fact_rx.recv().await,
        Some(
            crate::lifecycle::RecoveryInput::SignalingGenerationCommitted {
                generation,
                origin: crate::lifecycle::SignalingOrigin::External,
            }
        )
    );

    close_server_tx
        .send(())
        .expect("test server should still be waiting");
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), fact_rx.recv())
            .await
            .expect("stream termination should emit a supervisor fact"),
        Some(crate::lifecycle::RecoveryInput::SignalingGenerationLost {
            generation,
            cause: crate::lifecycle::SignalingLostCause::RemoteReset,
        })
    );
    assert!(!client.is_connected());

    server_task
        .await
        .expect("test websocket server should not panic");
}

#[tokio::test]
async fn connect_and_explicit_disconnect_emit_exact_supervisor_facts() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept tcp connection");
        let mut ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete websocket handshake");
        while let Some(message) = ws_stream.next().await {
            if matches!(
                message.expect("test websocket should remain valid"),
                tokio_tungstenite::tungstenite::Message::Close(_)
            ) {
                break;
            }
        }
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let mut facts = attach_supervisor_fact_sink(&client);

    client
        .connect()
        .await
        .expect("explicit signaling connect should succeed");
    let generation = client.connection_generation.load(Ordering::Acquire);
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationCommitted {
            generation,
            origin: tp::SignalingOrigin::External,
        },
        "socket publication must report its exact generation and external origin"
    );

    client
        .disconnect()
        .await
        .expect("explicit signaling disconnect should succeed");
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationLost {
            generation,
            cause: tp::SignalingLostCause::Disconnected,
        },
        "explicit disconnect must report the generation that was live before invalidation"
    );

    tokio::time::timeout(Duration::from_secs(2), server_task)
        .await
        .expect("test server should stop after close")
        .expect("test server should not panic");
}

#[tokio::test]
async fn supervisor_recovery_connect_tags_commit_with_action_identity() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("server should accept");
        let _ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("server should complete handshake");
        std::future::pending::<()>().await;
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let mut facts = attach_supervisor_fact_sink(&client);

    client
        .connect_once_for_recovery(41)
        .await
        .expect("recovery signaling connect should succeed");
    let generation = client.connection_generation.load(Ordering::Acquire);
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationCommitted {
            generation,
            origin: tp::SignalingOrigin::CurrentEffect { action_id: 41 },
        }
    );
    assert_eq!(
        *client
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        SignalingSocketState::Connected {
            generation,
            action_id: Some(41),
        },
        "the socket resource must retain the supervisor action that committed it"
    );

    client
        .disconnect()
        .await
        .expect("disconnect should succeed");
    assert_eq!(
        *client
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        SignalingSocketState::Disconnected {
            last_generation: Some(generation),
        }
    );
    server_task.abort();
}

#[tokio::test]
async fn receiver_stream_end_emits_remote_reset_for_committed_generation() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept tcp connection");
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete websocket handshake");
        let _ = release_rx.await;
        drop(ws_stream);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let mut facts = attach_supervisor_fact_sink(&client);

    client
        .connect()
        .await
        .expect("explicit signaling connect should succeed");
    let generation = client.connection_generation.load(Ordering::Acquire);
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationCommitted {
            generation,
            origin: tp::SignalingOrigin::External,
        }
    );

    release_tx
        .send(())
        .expect("test server should still wait for release");
    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationLost {
            generation,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        "a spontaneous receiver-loop end must report the committed generation as reset"
    );

    server_task.await.expect("test server should not panic");
}

#[tokio::test(start_paused = true)]
async fn ping_send_failure_emits_remote_reset_for_snapshotted_generation() {
    let client = make_ws_client(make_config());
    let mut facts = attach_supervisor_fact_sink(&client);
    client.connection_generation.store(41, Ordering::Release);
    client.connected.store(true, Ordering::Release);

    client.start_ping_task().await;
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(PING_INTERVAL_SECS)).await;

    assert_eq!(
        next_supervisor_fact(&mut facts).await,
        tp::Input::SignalingGenerationLost {
            generation: 41,
            cause: tp::SignalingLostCause::RemoteReset,
        },
        "ping failure must report the generation captured when the task started"
    );
    assert!(!client.is_connected());
}

#[tokio::test]
async fn send_envelope_failure_does_not_wait_for_reentrant_hook() {
    let client = make_ws_client(make_config());
    publish_connected_socket_state(&client);

    let hook_entered = Arc::new(tokio::sync::Notify::new());
    let hook_release = Arc::new(tokio::sync::Notify::new());
    let hook_entered_for_cb = hook_entered.clone();
    let hook_release_for_cb = hook_release.clone();
    client.set_hook_callback(Arc::new(move |event| {
        let hook_entered = hook_entered_for_cb.clone();
        let hook_release = hook_release_for_cb.clone();
        Box::pin(async move {
            if matches!(event, HookEvent::SignalingDisconnected) {
                hook_entered.notify_one();
                hook_release.notified().await;
            }
        })
    }));

    tokio::time::timeout(
        Duration::from_millis(250),
        client.send_envelope(SignalingEnvelope::default()),
    )
    .await
    .expect("send failure must return without waiting for the hook")
    .expect_err("missing WebSocket sink should fail");

    tokio::time::timeout(Duration::from_millis(250), hook_entered.notified())
        .await
        .expect("disconnected hook should still be invoked");
    hook_release.notify_one();
    tokio::task::yield_now().await;
}

#[tokio::test]
async fn explicit_connect_retries_after_concurrent_attempt_fails() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept tcp connection");
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete websocket handshake");
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(ws_stream);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.connection_timeout = 2;
    config.reconnect_config = ReconnectConfig {
        enabled: false,
        ..ReconnectConfig::default()
    };
    let client = make_ws_client(config);
    let (fact_tx, mut fact_rx) = tokio::sync::mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(fact_tx));
    assert_reconnect_config_fact(&mut fact_rx, &client.config.reconnect_config);

    client.connecting.store(true, Ordering::Release);
    let connect_task = {
        let client = client.clone();
        tokio::spawn(async move { client.connect().await })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;
    client.connecting.store(false, Ordering::Release);
    let _ = client.event_tx.send(SignalingEvent::Disconnected {
        reason: DisconnectReason::ConnectionFailed("simulated auto attempt failed".into()),
    });

    tokio::time::timeout(Duration::from_secs(2), connect_task)
        .await
        .expect("explicit connect should not wait for auto backoff")
        .expect("connect task should not panic")
        .expect("explicit connect should retry after concurrent failure");

    assert!(
        client.is_connected(),
        "explicit recovery connect should establish signaling"
    );
    assert_eq!(
        fact_rx.recv().await,
        Some(
            crate::lifecycle::RecoveryInput::SignalingGenerationCommitted {
                generation: client.connection_generation.load(Ordering::Acquire),
                origin: crate::lifecycle::SignalingOrigin::External,
            }
        )
    );

    client.disconnect().await.ok();
    let _ = tokio::time::timeout(Duration::from_secs(1), server_task).await;
}

#[tokio::test]
async fn network_restore_connect_preempts_connect_backoff() {
    let reserved_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should reserve a local port");
    let addr = reserved_listener
        .local_addr()
        .expect("reserved listener should have local addr");
    drop(reserved_listener);

    let mut config = make_config();
    config.server_url =
        Url::parse(&format!("ws://{addr}/signaling/ws")).expect("test URL should parse");
    config.connection_timeout = 1;
    config.reconnect_config = ReconnectConfig {
        enabled: true,
        max_attempts: 10,
        initial_delay: 30,
        max_delay: 30,
        backoff_multiplier: 1.0,
    };
    let client = make_ws_client(config);
    let mut rx = client.subscribe_events();

    let long_connect_task = {
        let client = client.clone();
        tokio::spawn(async move { client.connect().await })
    };

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Ok(SignalingEvent::Disconnected {
                    reason: DisconnectReason::ConnectionFailed(_),
                }) => break,
                Ok(_) => continue,
                Err(e) => panic!("unexpected signaling event receive error: {e}"),
            }
        }
    })
    .await
    .expect("long connect should fail first attempt and enter backoff");
    assert!(
        !client.connecting.load(Ordering::Acquire),
        "connect() must release connecting while sleeping in backoff"
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("network restore should make the signaling endpoint reachable");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("restored test server should accept tcp connection");
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("restored test server should complete websocket handshake");
        tokio::time::sleep(Duration::from_millis(250)).await;
        drop(ws_stream);
    });

    let restore_result = tokio::time::timeout(
        Duration::from_secs(CONCURRENT_CONNECT_WAIT_TIMEOUT_SECS + 2),
        {
            let client = client.clone();
            async move { client.connect().await }
        },
    )
    .await
    .expect("restore connect should complete within the concurrent wait window");

    long_connect_task.abort();
    server_task.abort();
    client.disconnect().await.ok();

    assert!(
        restore_result.is_ok(),
        "network restore should not be blocked by an older connect() backoff; got {restore_result:?}"
    );
}

#[tokio::test]
async fn explicit_connect_is_one_attempt_even_when_reconnect_config_is_enabled() {
    let reserved_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should reserve a local port");
    let addr = reserved_listener
        .local_addr()
        .expect("reserved listener should have local addr");
    drop(reserved_listener);

    let mut config = make_config();
    config.server_url =
        Url::parse(&format!("ws://{addr}/signaling/ws")).expect("test URL should parse");
    config.connection_timeout = 1;
    config.reconnect_config = ReconnectConfig {
        enabled: true,
        max_attempts: 10,
        initial_delay: 30,
        max_delay: 30,
        backoff_multiplier: 1.0,
    };
    let client = make_ws_client(config);

    let result = client.connect().await;
    assert!(result.is_err(), "the unavailable endpoint should fail");
    assert_eq!(
        client.get_stats().connections,
        0,
        "a failed one-shot attempt must not commit a socket"
    );
    assert_eq!(
        *client
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        SignalingSocketState::Disconnected {
            last_generation: Some(0),
        },
        "the attempt guard must return a failed connection flight to Disconnected"
    );
}

#[tokio::test]
async fn test_publish_disconnected_transition_fires_hook_once() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (event_tx, mut event_rx) = broadcast::channel(4);
    let hook_count = Arc::new(AtomicUsize::new(0));
    let hook_count_for_cb = hook_count.clone();
    let hook_callback: HookCallback = Arc::new(move |event| {
        let hook_count = hook_count_for_cb.clone();
        Box::pin(async move {
            if matches!(event, HookEvent::SignalingDisconnected) {
                hook_count.fetch_add(1, UsizeOrdering::SeqCst);
            }
        }) as Pin<Box<dyn Future<Output = ()> + Send>>
    });

    let first = WebSocketSignalingClient::publish_disconnected_transition(
        true,
        &stats,
        &event_tx,
        Some(hook_callback.clone()),
        DisconnectReason::StreamEnded,
    )
    .await;
    assert!(
        first,
        "first connected->disconnected transition should publish"
    );
    assert_eq!(hook_count.load(UsizeOrdering::SeqCst), 1);
    assert_eq!(stats.snapshot().disconnections, 1);
    assert!(matches!(
        event_rx.recv().await,
        Ok(SignalingEvent::Disconnected {
            reason: DisconnectReason::StreamEnded
        })
    ));

    let second = WebSocketSignalingClient::publish_disconnected_transition(
        false,
        &stats,
        &event_tx,
        Some(hook_callback),
        DisconnectReason::PongTimeout,
    )
    .await;
    assert!(
        !second,
        "stale duplicate disconnected transition should be ignored"
    );
    assert_eq!(hook_count.load(UsizeOrdering::SeqCst), 1);
    assert_eq!(stats.snapshot().disconnections, 1);
    assert!(event_rx.try_recv().is_err());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 1. Configuration defaults
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_reconnect_config_defaults() {
    let cfg = ReconnectConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.max_attempts, 10);
    assert_eq!(cfg.initial_delay, 1);
    assert_eq!(cfg.max_delay, 60);
    assert!((cfg.backoff_multiplier - 2.0).abs() < f64::EPSILON);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 2. Initial state
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_websocket_signaling_client_initial_state_disconnected() {
    let client = WebSocketSignalingClient::new(make_config());
    assert!(
        !client.is_connected(),
        "newly created client should be Disconnected"
    );
    assert!(
        !client.connecting.load(Ordering::Acquire),
        "newly created client should not be in connecting state"
    );
}

#[test]
fn test_initial_stats_are_zero() {
    let client = WebSocketSignalingClient::new(make_config());
    let stats = client.get_stats();
    assert_eq!(stats.connections, 0);
    assert_eq!(stats.disconnections, 0);
    assert_eq!(stats.messages_sent, 0);
    assert_eq!(stats.messages_received, 0);
    assert_eq!(stats.errors, 0);
}

#[test]
fn test_signaling_url_log_redacts_credential_query_params() {
    let url = Url::parse(
            "wss://example.com/signaling?actor_id=abc&key_id=7&claims=claims-value&signature=signature-value&token=token-value",
        )
        .unwrap();

    let redacted = WebSocketSignalingClient::redact_signaling_url_for_log(&url);

    assert!(redacted.contains("actor_id=abc"));
    assert!(redacted.contains("key_id=7"));
    assert!(redacted.contains("claims=REDACTED"));
    assert!(redacted.contains("signature=REDACTED"));
    assert!(redacted.contains("token=REDACTED"));
    assert!(!redacted.contains("claims-value"));
    assert!(!redacted.contains("signature-value"));
    assert!(!redacted.contains("token-value"));
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 4. connect() concurrency exclusion
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_connect_fast_path_when_already_connected() {
    let client = make_ws_client(make_config());
    // Manually set as connected
    client.connected.store(true, Ordering::Release);

    // connect() should return Ok immediately without establishing a new connection
    let result = client.connect().await;
    assert!(
        result.is_ok(),
        "connect() should return Ok when already connected"
    );
    // Should not change connecting flag
    assert!(!client.connecting.load(Ordering::Acquire));
}

#[tokio::test]
async fn test_connect_sets_connecting_flag() {
    let mut config = make_config();
    config.reconnect_config.enabled = false; // disable retry, fail fast
    config.connection_timeout = 1;
    let client = make_ws_client(config);

    // Connection will fail (unreachable address), but should properly clean up connecting flag
    let result = client.connect().await;
    assert!(
        result.is_err(),
        "connecting to unreachable address should fail"
    );
    assert!(
        !client.connecting.load(Ordering::Acquire),
        "connecting flag should be cleared after connection failure"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 5. Event broadcast
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_event_subscribe_receives_events() {
    let client = make_ws_client(make_config());
    let mut rx = client.subscribe_events();

    // Manually send event
    let _ = client.event_tx.send(SignalingEvent::Connected);

    match tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await {
        Ok(Ok(SignalingEvent::Connected)) => {} // expect Connected event
        other => panic!("expected Connected event, but got {:?}", other),
    }
}

#[tokio::test]
async fn test_disconnect_event_on_connect_failure() {
    let mut config = make_config();
    config.reconnect_config.enabled = false;
    config.connection_timeout = 1;
    let client = make_ws_client(config);
    let mut rx = client.subscribe_events();

    // Connection fails
    let _ = client.connect().await;

    // Should receive Disconnected(ConnectionFailed) event
    match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await {
        Ok(Ok(SignalingEvent::Disconnected {
            reason: DisconnectReason::ConnectionFailed(_),
        })) => {} // expected
        other => panic!(
            "expected Disconnected(ConnectionFailed) event, but got {:?}",
            other
        ),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 6. disconnect() state cleanup
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_disconnect_clears_connected_flag() {
    let client = make_ws_client(make_config());
    let generation = client.connection_generation.load(Ordering::Acquire);
    let (fact_tx, mut fact_rx) = tokio::sync::mpsc::unbounded_channel();
    client.set_supervisor_fact_sink(SupervisorFactSink::new(fact_tx));
    assert_reconnect_config_fact(&mut fact_rx, &client.config.reconnect_config);
    // Simulate connected state
    client.connected.store(true, Ordering::Release);
    assert!(client.is_connected());

    let result = client.disconnect().await;
    assert!(result.is_ok());
    assert!(
        !client.is_connected(),
        "should be Disconnected after disconnect()"
    );
    assert_eq!(
        client.connection_generation.load(Ordering::Acquire),
        generation + 1,
        "disconnect must invalidate every in-flight connection attempt"
    );
    assert_eq!(
        fact_rx.recv().await,
        Some(crate::lifecycle::RecoveryInput::SignalingGenerationLost {
            generation,
            cause: crate::lifecycle::SignalingLostCause::Disconnected,
        })
    );
}

#[tokio::test]
async fn test_disconnect_increments_disconnection_stat() {
    let client = make_ws_client(make_config());
    client.connected.store(true, Ordering::Release);

    let stats_before = client.get_stats().disconnections;
    let _ = client.disconnect().await;
    let stats_after = client.get_stats().disconnections;
    assert_eq!(
        stats_after,
        stats_before + 1,
        "disconnect() should increment disconnection count"
    );
}

#[tokio::test]
async fn test_disconnect_idempotent() {
    let client = make_ws_client(make_config());

    // Calling disconnect() while not connected should not panic
    let r1 = client.disconnect().await;
    let r2 = client.disconnect().await;
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_explicit_disconnect_cancels_in_flight_explicit_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept tcp connection");
        accepted_tx
            .send(())
            .expect("test should still wait for accepted connection");
        let _ = release_rx.await;
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete websocket handshake");
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(ws_stream);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.connection_timeout = 5;
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let mut rx = client.subscribe_events();

    let connecting_client = client.clone();
    let connect_task = tokio::spawn(async move { connecting_client.connect().await });

    tokio::time::timeout(Duration::from_secs(1), accepted_rx)
        .await
        .expect("explicit connection should reach the server")
        .expect("server accept notifier should remain open");

    client
        .disconnect()
        .await
        .expect("disconnect should invalidate the in-flight explicit connection");
    release_tx
        .send(())
        .expect("test server handshake should still be waiting");

    tokio::time::timeout(Duration::from_secs(2), connect_task)
        .await
        .expect("cancelled explicit connect should exit promptly")
        .expect("explicit connect task should not panic")
        .expect_err("the stale explicit connect must not report success");

    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
        assert!(
            !matches!(event, SignalingEvent::Connected),
            "cancelled explicit connect must not publish Connected"
        );
    }

    assert!(
        !client.is_connected(),
        "cancelled explicit connect must not resurrect signaling"
    );
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("test server task should finish")
        .expect("test server task should not panic");
}

#[tokio::test]
async fn explicit_disconnect_interrupts_a_stalled_handshake_without_server_progress() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("server should accept");
        accepted_tx.send(()).ok();
        std::future::pending::<()>().await;
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test URL should parse");
    config.connection_timeout = 30;
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let connecting_client = client.clone();
    let connect = tokio::spawn(async move { connecting_client.connect().await });
    accepted_rx
        .await
        .expect("connect attempt should reach the stalled server");

    client
        .disconnect()
        .await
        .expect("disconnect should invalidate the active generation");
    let result = tokio::time::timeout(Duration::from_millis(250), connect)
        .await
        .expect("generation invalidation must interrupt the handshake immediately")
        .expect("connect task should not panic");
    assert!(result.is_err(), "the invalidated handshake cannot commit");

    server.abort();
}

#[tokio::test]
async fn inv12_stale_signaling_generation_cannot_publish_connected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (first_accepted_tx, first_accepted_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_first_tx, release_first_rx) = tokio::sync::oneshot::channel::<()>();
    let (finish_server_tx, finish_server_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("test server should accept first tcp connection");
        first_accepted_tx
            .send(())
            .expect("test should still wait for first connection");
        let _ = release_first_rx.await;
        let first_ws = tokio_tungstenite::accept_async(first_stream)
            .await
            .expect("test server should complete first websocket handshake");
        drop(first_ws);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("test server should accept replacement tcp connection");
        let second_ws = tokio_tungstenite::accept_async(second_stream)
            .await
            .expect("test server should complete replacement websocket handshake");
        let _ = finish_server_rx.await;
        drop(second_ws);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.connection_timeout = 5;
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let mut rx = client.subscribe_events();

    let stale_client = client.clone();
    let stale_connect = tokio::spawn(async move { stale_client.connect().await });

    tokio::time::timeout(Duration::from_secs(1), first_accepted_rx)
        .await
        .expect("first connection should reach the server")
        .expect("first accept notifier should remain open");

    // A stronger supervisor action fences the superseded Restore synchronously
    // before requesting task cancellation. The explicit connect may still run
    // to its next commit boundary, but it must no longer publish Connected.
    client.invalidate_generation();

    let replacement_client = client.clone();
    let replacement_connect = tokio::spawn(async move { replacement_client.connect().await });
    release_first_tx
        .send(())
        .expect("first handshake should still be waiting");

    tokio::time::timeout(Duration::from_secs(2), stale_connect)
        .await
        .expect("stale generation should finish promptly")
        .expect("stale connection task should not panic")
        .expect_err("stale generation must not report success");
    tokio::time::timeout(Duration::from_secs(2), replacement_connect)
        .await
        .expect("replacement must not wait for the concurrent-connect timeout")
        .expect("replacement connection task should not panic")
        .expect("replacement generation should connect");

    assert!(
        client.is_connected(),
        "the current generation should own the signaling connection"
    );
    let mut connected_events = 0;
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
        if matches!(event, SignalingEvent::Connected) {
            connected_events += 1;
        }
    }
    assert_eq!(
        connected_events, 1,
        "only the replacement generation may publish Connected"
    );

    client
        .disconnect()
        .await
        .expect("replacement connection should disconnect cleanly");
    finish_server_tx
        .send(())
        .expect("test server should still be waiting");
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("test server task should finish")
        .expect("test server task should not panic");
}

#[tokio::test]
async fn inv12_generation_change_after_validation_cannot_publish_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (finish_server_tx, finish_server_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener
            .accept()
            .await
            .expect("test server should accept the tcp connection");
        let websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("test server should complete the websocket handshake");
        let _ = finish_server_rx.await;
        drop(websocket);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.connection_timeout = 5;
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);
    let publish_barrier = Arc::new(tokio::sync::Barrier::new(2));
    assert!(
        client
            .connection_publish_barrier
            .set(publish_barrier.clone())
            .is_ok(),
        "test publication barrier should only be installed once"
    );
    let mut events = client.subscribe_events();

    let stale_client = client.clone();
    let stale_connect = tokio::spawn(async move { stale_client.connect().await });

    tokio::time::timeout(Duration::from_secs(2), publish_barrier.wait())
        .await
        .expect("connection should reach the post-validation publication barrier");
    client.invalidate_generation();
    publish_barrier.wait().await;

    tokio::time::timeout(Duration::from_secs(2), stale_connect)
        .await
        .expect("stale connection should finish promptly")
        .expect("stale connection task should not panic")
        .expect_err("generation invalidated before publication must not commit");
    assert!(!client.is_connected());
    assert!(client.ws_sink.lock().await.is_none());
    assert!(client.ws_stream.lock().await.is_none());
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, SignalingEvent::Connected),
            "an invalidated generation must not publish Connected"
        );
    }

    finish_server_tx
        .send(())
        .expect("test server should still be waiting");
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("test server task should finish")
        .expect("test server task should not panic");
}

#[tokio::test]
async fn test_cancelled_connect_future_releases_single_flight_immediately() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener
            .local_addr()
            .expect("test listener should have local addr")
    );
    let (first_accepted_tx, first_accepted_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_first_tx, release_first_rx) = tokio::sync::oneshot::channel::<()>();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("test server should accept first tcp connection");
        first_accepted_tx
            .send(())
            .expect("test should still wait for first connection");
        let _ = release_first_rx.await;
        drop(first_stream);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("test server should accept replacement tcp connection");
        let second_ws = tokio_tungstenite::accept_async(second_stream)
            .await
            .expect("replacement websocket handshake should complete");
        let _ = finish_rx.await;
        drop(second_ws);
    });

    let mut config = make_config();
    config.server_url = Url::parse(&server_url).expect("test websocket URL should parse");
    config.connection_timeout = 5;
    config.reconnect_config.enabled = false;
    let client = make_ws_client(config);

    let first_client = client.clone();
    let first_connect = tokio::spawn(async move { first_client.connect().await });
    tokio::time::timeout(Duration::from_secs(1), first_accepted_rx)
        .await
        .expect("first connection should reach server")
        .expect("first accept notifier should remain open");

    first_connect.abort();
    let _ = first_connect.await;
    assert!(
        !client.connecting.load(Ordering::Acquire),
        "dropping the owner future must release single-flight without a timeout"
    );
    release_first_tx
        .send(())
        .expect("test server should still hold first stream");

    tokio::time::timeout(Duration::from_secs(2), client.connect())
        .await
        .expect("replacement connect must not wait for the concurrent-connect timeout")
        .expect("replacement connection should succeed");
    assert!(client.is_connected());

    client
        .disconnect()
        .await
        .expect("replacement connection should disconnect");
    finish_tx.send(()).expect("test server should still wait");
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("test server should finish")
        .expect("test server should not panic");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 8. URL construction tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_build_url_without_identity() {
    let config = make_config();
    let expected_base = config.server_url.to_string();
    let client = WebSocketSignalingClient::new(config);

    let url = client.build_url_with_identity().await;
    assert_eq!(
        url.to_string(),
        expected_base,
        "URL should not contain identity parameters when actor_id is not set"
    );
}

#[tokio::test]
async fn test_build_url_with_webrtc_role() {
    let mut config = make_config();
    config.webrtc_role = Some("answer".to_string());
    let client = WebSocketSignalingClient::new(config);

    let url = client.build_url_with_identity().await;
    assert!(
        url.query().unwrap_or("").contains("webrtc_role=answer"),
        "URL should contain webrtc_role parameter, actual URL: {}",
        url
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 9. Inbound channel reset
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_reset_inbound_channel_creates_fresh_channel() {
    let client = WebSocketSignalingClient::new(make_config());

    // Enqueue a message into the current session's bounded queue
    {
        let egress = client.inbound_egress.lock().await;
        egress
            .send(SignalingEnvelope::default(), 16)
            .await
            .expect("enqueue should succeed while capacity is available");
    }

    // Reset channel
    client.reset_inbound_channel().await;

    // Old messages should not be visible in the new channel
    let mut rx = client.inbound_rx.lock().await;
    let result = rx.try_recv();
    assert!(
        result.is_err(),
        "old messages should not be visible in the new channel after reset"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 10. Envelope ID incrementing
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_envelope_id_monotonically_increasing() {
    let client = WebSocketSignalingClient::new(make_config());

    let id1 = client.next_envelope_id().await;
    let id2 = client.next_envelope_id().await;
    let id3 = client.next_envelope_id().await;

    assert_eq!(id1, "env-1");
    assert_eq!(id2, "env-2");
    assert_eq!(id3, "env-3");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 11. send_envelope should return error when not connected
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_send_envelope_fails_when_not_connected() {
    let client = WebSocketSignalingClient::new(make_config());
    let envelope = SignalingEnvelope::default();

    let result = client.send_envelope(envelope).await;
    assert!(
        result.is_err(),
        "send_envelope should return error when not connected"
    );
    match result {
        Err(NetworkError::ConnectionError(msg)) => {
            assert!(
                msg.contains("not connected") || msg.contains("Not connected"),
                "error message should contain 'not connected', actual: {}",
                msg
            );
        }
        other => panic!("expected ConnectionError, got {:?}", other),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 12. FakeSignalingClient trait implementation verification
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_fake_client_tracks_connect_calls() {
    let client = make_fake_client();
    assert_eq!(client.connect_calls.load(UsizeOrdering::SeqCst), 0);

    client.connect().await.unwrap();
    client.connect().await.unwrap();
    client.connect().await.unwrap();

    assert_eq!(
        client.connect_calls.load(UsizeOrdering::SeqCst),
        3,
        "FakeSignalingClient should accurately track connect call count"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 14. Pong liveness watchdog active-time accounting
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn pong_watchdog_counts_only_completed_active_ticks() {
    let mut watchdog = PongWatchdog::new(7);

    assert_eq!(watchdog.tick(7, PING_INTERVAL_SECS), 5);
    assert_eq!(watchdog.tick(7, PING_INTERVAL_SECS), 10);
    assert_eq!(watchdog.tick(7, PING_INTERVAL_SECS), 15);
}

#[test]
fn pong_watchdog_resets_when_liveness_advances() {
    let mut watchdog = PongWatchdog::new(7);
    assert_eq!(watchdog.tick(7, PING_INTERVAL_SECS), 5);

    assert_eq!(watchdog.tick(8, PING_INTERVAL_SECS), 0);
    assert_eq!(watchdog.tick(8, PING_INTERVAL_SECS), 5);
}

#[test]
fn delayed_resume_wake_counts_as_one_ping_interval() {
    let mut watchdog = PongWatchdog::new(7);

    // The scheduling delay is deliberately absent from the input: whether
    // this tick arrived after 5 seconds or after an hour of device suspend,
    // it represents only one completed active-time interval.
    let silent_active_secs = watchdog.tick(7, PING_INTERVAL_SECS);

    assert_eq!(silent_active_secs, PING_INTERVAL_SECS);
    assert!(silent_active_secs <= PONG_TIMEOUT_SECS);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 12. Bounded inbound queue (issue #351)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Helper: build an `InboundQueueConfig` with explicit bounds for testing.
fn inbound_config(
    max_messages: usize,
    max_bytes: usize,
    backpressure_timeout_ms: u64,
) -> InboundQueueConfig {
    InboundQueueConfig {
        max_messages,
        max_bytes,
        max_frame_bytes: DEFAULT_INBOUND_MAX_FRAME_BYTES,
        backpressure_timeout: Duration::from_millis(backpressure_timeout_ms),
    }
}

/// Sustained flood: a slow/stopped consumer cannot grow the queue past the
/// configured message bound; the producer observes `Saturated` (fail-closed
/// trigger) once the bound is held past the backpressure deadline.
#[tokio::test]
async fn test_inbound_egress_bounded_by_message_count() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (egress, _rx) = InboundEgress::new(inbound_config(2, 1 << 20, 50), stats.clone());

    // Fill the message-count bound (consumer never drains).
    egress.send(SignalingEnvelope::default(), 8).await.unwrap();
    egress.send(SignalingEnvelope::default(), 8).await.unwrap();

    // The next send cannot be admitted and must saturate (not block forever).
    let result = egress.send(SignalingEnvelope::default(), 8).await;
    assert_eq!(result, Err(InboundEnqueueError::Saturated));
    assert_eq!(stats.inbound_saturated.load(Ordering::Relaxed), 1);
    assert_eq!(stats.inbound_enqueued.load(Ordering::Relaxed), 2);
}

/// The byte budget caps the queue before the message-count bound does.
#[tokio::test]
async fn test_inbound_egress_bounded_by_byte_budget() {
    let stats = Arc::new(AtomicSignalingStats::default());
    // Generous message count, tiny byte budget (100 bytes).
    let (egress, _rx) = InboundEgress::new(inbound_config(16, 100, 50), stats.clone());

    // First frame reserves 60 of 100 bytes.
    egress.send(SignalingEnvelope::default(), 60).await.unwrap();

    // Second frame would need another 60 bytes (120 > 100): blocked by the byte
    // budget even though only 1 of 16 message slots is used.
    let result = egress.send(SignalingEnvelope::default(), 60).await;
    assert_eq!(result, Err(InboundEnqueueError::Saturated));
    assert_eq!(stats.inbound_saturated.load(Ordering::Relaxed), 1);
}

/// Recovery after saturation: once the consumer drains, producers can enqueue
/// again (no permanent latch).
#[tokio::test]
async fn test_inbound_egress_recovers_after_drain() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (egress, mut rx) = InboundEgress::new(inbound_config(1, 1 << 20, 50), stats.clone());

    egress.send(SignalingEnvelope::default(), 8).await.unwrap();
    // Queue is now full (1/1); the next send saturates.
    assert_eq!(
        egress.send(SignalingEnvelope::default(), 8).await,
        Err(InboundEnqueueError::Saturated)
    );

    // Consumer drains one message, freeing a slot and its byte budget.
    let queued = rx.recv().await.expect("one message should be queued");
    drop(queued);

    // Producer recovers: the next enqueue succeeds.
    egress.send(SignalingEnvelope::default(), 8).await.unwrap();
    assert_eq!(stats.inbound_enqueued.load(Ordering::Relaxed), 2);
}

/// Dropping the receiver (session reset/disconnect) releases every queued byte
/// reservation — no orphaned budget.
#[tokio::test]
async fn test_inbound_egress_releases_bytes_on_queue_drop() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (egress, rx) = InboundEgress::new(inbound_config(8, 1000, 50), stats.clone());

    egress
        .send(SignalingEnvelope::default(), 400)
        .await
        .unwrap();
    egress
        .send(SignalingEnvelope::default(), 400)
        .await
        .unwrap();
    assert_eq!(egress.queued_bytes(), 800);

    // Simulate a session reset: dropping the receiver drains and drops all
    // queued envelopes, returning their byte permits.
    drop(rx);
    // Allow the channel teardown to settle (permits release synchronously on drop).
    assert_eq!(egress.queued_bytes(), 0);
    assert_eq!(egress.depth(), 0);
}

/// Observability: depth/bytes gauges track enqueues and drains.
#[tokio::test]
async fn test_inbound_egress_depth_and_byte_gauges() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (egress, mut rx) = InboundEgress::new(inbound_config(8, 1000, 50), stats.clone());

    assert_eq!(egress.depth(), 0);
    assert_eq!(egress.queued_bytes(), 0);

    egress
        .send(SignalingEnvelope::default(), 128)
        .await
        .unwrap();
    assert_eq!(egress.depth(), 1);
    assert_eq!(egress.queued_bytes(), 128);

    let queued = rx.recv().await.unwrap();
    drop(queued);
    assert_eq!(egress.depth(), 0);
    assert_eq!(egress.queued_bytes(), 0);
}

/// A frame whose byte cost exceeds the whole byte budget can never be admitted
/// and saturates promptly (defense against a single oversized reservation).
#[tokio::test]
async fn test_inbound_egress_rejects_frame_larger_than_budget() {
    let stats = Arc::new(AtomicSignalingStats::default());
    let (egress, _rx) = InboundEgress::new(inbound_config(8, 64, 50), stats.clone());

    let result = egress.send(SignalingEnvelope::default(), 4096).await;
    assert_eq!(result, Err(InboundEnqueueError::Saturated));
}

// ---------------------------------------------------------------------------
// Generation-fenced send-failure teardown (PR #229 review, Finding 3)
// ---------------------------------------------------------------------------

/// Bind a local WebSocket server that completes every handshake and then
/// holds each connection open without ever reading it again. Holding (rather
/// than closing) keeps the client's receiver task parked, so the only
/// teardown observed comes from the code under test.
async fn spawn_silent_ws_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind silent signaling server");
    let url = format!("ws://{}", listener.local_addr().expect("server address"));
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                if let Ok(websocket) = tokio_tungstenite::accept_async(stream).await {
                    let _held = websocket;
                    std::future::pending::<()>().await;
                }
            });
        }
    });
    (url, task)
}

fn local_server_config(url: &str) -> SignalingConfig {
    SignalingConfig {
        server_url: Url::parse(url).unwrap(),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig::default(),
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }
}

/// A send failure on a socket whose generation a recovery already superseded
/// must not touch the replacement connection: no liveness swap, no task
/// aborts, no sink teardown, and no spurious Disconnected transition.
#[tokio::test]
async fn send_failure_teardown_skips_superseded_generation() {
    let (url, server) = spawn_silent_ws_server().await;
    let client = make_ws_client(local_server_config(&url));
    client.connect().await.expect("initial connect");

    // The send under test began on this generation; capture it before the
    // recovery takeover, exactly as send_envelope snapshots its socket.
    let failed_generation = client.connection_generation.load(Ordering::Acquire);
    client
        .disconnect_for_recovery()
        .await
        .expect("recovery disconnect");
    client
        .connect()
        .await
        .expect("replacement connect on the bumped generation");
    assert!(client.is_connected(), "replacement socket must be live");

    let mut events = client.subscribe_events();
    client
        .handle_envelope_send_failure(failed_generation, DisconnectReason::SendFailed)
        .await;

    assert!(
        client.is_connected(),
        "a superseded send failure must not drop the replacement connection"
    );
    let ping_alive = {
        let guard = client.ping_task.lock().await;
        guard.as_ref().is_some_and(|handle| !handle.is_finished())
    };
    assert!(ping_alive, "replacement ping task must keep running");
    let receiver_alive = {
        let guard = client.receiver_task.lock().await;
        guard.as_ref().is_some_and(|handle| !handle.is_finished())
    };
    assert!(
        receiver_alive,
        "replacement receiver task must keep running"
    );
    assert!(
        client.ws_sink.lock().await.is_some(),
        "replacement sink must stay installed"
    );
    assert!(
        matches!(
            events.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ),
        "no disconnect transition may be published for the live replacement"
    );

    client.disconnect().await.expect("disconnect test client");
    server.abort();
}

/// A send failure on the still-live generation owns the teardown: old tasks
/// are aborted, the sink is retired, waiters are dropped, and exactly one
/// Disconnected transition is published for that generation.
#[tokio::test]
async fn send_failure_teardown_completes_for_live_generation() {
    let (url, server) = spawn_silent_ws_server().await;
    let client = make_ws_client(local_server_config(&url));
    client.connect().await.expect("initial connect");
    let generation = client.connection_generation.load(Ordering::Acquire);

    let mut events = client.subscribe_events();
    client
        .handle_envelope_send_failure(generation, DisconnectReason::SendFailed)
        .await;

    assert!(!client.is_connected(), "failed socket must be invalidated");
    assert!(
        client.ping_task.lock().await.is_none(),
        "old ping task must be aborted"
    );
    assert!(
        client.receiver_task.lock().await.is_none(),
        "old receiver task must be aborted"
    );
    assert!(
        client.ws_sink.lock().await.is_none(),
        "old sink must be retired"
    );
    let published = matches!(
        &*client
            .socket_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        SignalingSocketState::Disconnected {
            last_generation: Some(published)
        } if *published == generation
    );
    assert!(
        published,
        "the failure handler must publish its own Disconnected state"
    );
    match tokio::time::timeout(std::time::Duration::from_secs(1), events.recv()).await {
        Ok(Ok(SignalingEvent::Disconnected { reason })) => {
            assert!(matches!(reason, DisconnectReason::SendFailed));
        }
        other => panic!("expected one Disconnected transition, got {other:?}"),
    }

    client.disconnect().await.expect("disconnect test client");
    server.abort();
}
