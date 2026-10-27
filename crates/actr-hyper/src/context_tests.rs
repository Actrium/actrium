use std::time::Duration;

use super::*;
use crate::test_support::runtime_context_with_host_transport;
use crate::transport::HostTransport;

fn ctx() -> RuntimeContext {
    runtime_context_with_host_transport(ActrId::default(), Arc::new(HostTransport::new()))
}

fn actr_type(publisher: &str, name: &str, ver: &str) -> ActrType {
    ActrType {
        publisher: publisher.into(),
        name: name.into(),
        version: ver.into(),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
struct TimeoutRequest {
    #[prost(bytes = "vec", tag = "1")]
    payload: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct TimeoutResponse {
    #[prost(bytes = "vec", tag = "1")]
    payload: Vec<u8>,
}

impl RpcRequest for TimeoutRequest {
    type Response = TimeoutResponse;

    fn route_key() -> &'static str {
        "test.Timeout.Call"
    }
}

// ── select_gate ─────────────────────────────────────────────────────────

#[tokio::test]
async fn select_gate_shell_and_local_use_inproc() {
    let c = ctx();
    assert!(c.select_gate(&Dest::Host).is_ok());
    assert!(c.select_gate(&Dest::Workload).is_ok());
}

#[tokio::test]
async fn select_gate_peer_errors_when_outproc_unset() {
    // Build a context with outproc_gate = None (unlike the test_support
    // helper, which sets Some). Peer dest must fail.
    use crate::inbound::{DataChunkRegistry, MediaFrameRegistry};
    use crate::outbound::{Gate, HostGate};
    use crate::wire::webrtc::{
        InboundQueueConfig, ReconnectConfig, SignalingConfig, WebSocketSignalingClient,
    };
    let host = Arc::new(HostTransport::new());
    let inproc = Gate::Host(Arc::new(HostGate::new(host)));
    let c = RuntimeContext::new(
        ActrId::default(),
        None,
        "req".into(),
        inproc,
        None, // outproc_gate unset
        Arc::new(DataChunkRegistry::new()),
        Arc::new(MediaFrameRegistry::new()),
        Arc::new(WebSocketSignalingClient::new(SignalingConfig {
            server_url: url::Url::parse("ws://127.0.0.1:9").unwrap(),
            connection_timeout: 1,
            heartbeat_interval: 30,
            reconnect_config: ReconnectConfig::default(),
            auth_config: None,
            webrtc_role: None,
            inbound_queue: InboundQueueConfig::default(),
        })) as Arc<dyn SignalingClient>,
        AIdCredential::default(),
        None,
        Arc::new(RwLock::new(HashMap::new())),
        None,
        0,
    );
    match c.select_gate(&Dest::Peer(ActrId::default())) {
        Err(ActrError::Internal(_)) => {}
        Err(_) => panic!("expected Internal error, got a different ActrError variant"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

#[tokio::test]
async fn select_gate_actor_ok_when_outproc_set() {
    // The test_support helper sets outproc_gate = Some(inproc) → Actor ok.
    let c = ctx();
    assert!(c.select_gate(&Dest::Peer(ActrId::default())).is_ok());
}

// ── extract_target_id ───────────────────────────────────────────────────

#[tokio::test]
async fn extract_target_id_resolves_self_for_local_dests() {
    let c = ctx();
    let self_id = c.self_id().clone();
    assert_eq!(c.extract_target_id(&Dest::Host), &self_id);
    assert_eq!(c.extract_target_id(&Dest::Workload), &self_id);

    let remote = ActrId {
        serial_number: 99,
        ..ActrId::default()
    };
    assert_eq!(c.extract_target_id(&Dest::Peer(remote.clone())), &remote);
}

// ── ensure_session_ready ────────────────────────────────────────────────

#[tokio::test]
async fn ensure_session_ready_ok_without_session_state() {
    let c = ctx();
    assert!(c.ensure_session_ready().await.is_ok());
}

#[tokio::test]
async fn ensure_session_ready_rejects_stale_generation() {
    let mut c = ctx();
    let snap =
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 5);
    let ss = SessionState::new(snap);
    c.session_state = Some(ss.clone());
    c.context_generation = 0;
    let err = c.ensure_session_ready().await.unwrap_err();
    assert!(matches!(err, ActrError::ConnectionNotReady(_)));

    c.context_generation = 5;
    ss.enter_rebinding().await;
    let err = c.ensure_session_ready().await.unwrap_err();
    assert!(matches!(err, ActrError::ConnectionNotReady(_)));

    ss.set_active().await;
    assert!(c.ensure_session_ready().await.is_ok());
}

#[tokio::test]
async fn ensure_session_ready_until_waits_for_rebinding_to_finish() {
    let mut c = ctx();
    let ss = SessionState::new(
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 7),
    );
    c.session_state = Some(ss.clone());
    c.context_generation = 7;
    ss.enter_rebinding().await;

    let deadline = crate::transport::RpcDeadline::from_duration(std::time::Duration::from_secs(1));
    let mut wait =
        Box::pin(c.ensure_session_ready_until(crate::timer::ids::RUNTIME_CALL_PREFLIGHT, deadline));

    // The preflight must remain pending while the current generation is
    // rebinding instead of surfacing an immediate ConnectionNotReady.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut wait)
            .await
            .is_err()
    );

    ss.set_active().await;
    assert!(wait.await.is_ok());
}

#[tokio::test]
async fn ensure_session_ready_until_rejects_replaced_generation() {
    let mut c = ctx();
    let ss = SessionState::new(
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 7),
    );
    c.session_state = Some(ss.clone());
    c.context_generation = 7;
    ss.enter_rebinding().await;

    let deadline = crate::transport::RpcDeadline::from_duration(std::time::Duration::from_secs(1));
    let waiter = tokio::spawn(async move {
        c.ensure_session_ready_until(crate::timer::ids::RUNTIME_CALL_PREFLIGHT, deadline)
            .await
    });

    ss.commit_hard_rebind_unpublished(
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 8),
    )
    .await;

    let error = waiter
        .await
        .expect("session preflight must not panic")
        .expect_err("an old context must not revive after a generation change");
    assert!(matches!(error, ActrError::ConnectionNotReady(_)));
}

#[tokio::test]
async fn ensure_session_ready_until_times_out_while_rebinding() {
    let mut c = ctx();
    let ss = SessionState::new(
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 7),
    );
    c.session_state = Some(ss.clone());
    c.context_generation = 7;
    ss.enter_rebinding().await;

    let error = c
        .ensure_session_ready_until(
            crate::timer::ids::RUNTIME_CALL_PREFLIGHT,
            crate::transport::RpcDeadline::from_duration(std::time::Duration::from_millis(20)),
        )
        .await
        .expect_err("a rebinding session must not wait beyond its deadline");
    assert!(matches!(error, ActrError::TimedOut));
}

async fn ctx_with_active_session() -> (RuntimeContext, SessionState) {
    let mut c = ctx();
    let snap =
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 5);
    let ss = SessionState::new(snap);
    c.session_state = Some(ss.clone());
    c.context_generation = 5;
    c.ensure_session_ready()
        .await
        .expect("session should be ready");
    (c, ss)
}

#[tokio::test(start_paused = true)]
async fn call_readiness_consumes_caller_deadline_before_gate_dispatch() {
    let (c, ss) = ctx_with_active_session().await;

    // Put the current session generation into the actual readiness phase used
    // by outbound preflight. The caller deadline must bound this wait; a later
    // stage must not get a fresh budget after a full readiness stall.
    ss.enter_rebinding().await;
    let request = tokio::spawn(async move {
        c.call(&Dest::Host, TimeoutRequest { payload: vec![1] }, 100)
            .await
    });

    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert!(
        request.is_finished(),
        "readiness wait must be bounded by the caller deadline"
    );

    let error = request
        .await
        .expect("request task should not panic")
        .expect_err("blocked readiness should time out");
    assert!(matches!(error, ActrError::TimedOut), "got {error:?}");
}

#[tokio::test(start_paused = true)]
async fn tell_readiness_consumes_send_budget_before_gate_dispatch() {
    let (c, ss) = ctx_with_active_session().await;

    ss.enter_rebinding().await;
    let request = tokio::spawn(async move {
        c.tell(&Dest::Host, TimeoutRequest { payload: vec![1] })
            .await
    });

    // Exactly the fixed Tell budget: a regression that lets readiness run
    // unbudgeted and re-arm a fresh gate budget would keep the future
    // pending (or succeed) after this advance.
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(13)).await;
    tokio::task::yield_now().await;
    assert!(
        request.is_finished(),
        "readiness wait must be bounded by the Tell send budget"
    );

    let error = request
        .await
        .expect("request task should not panic")
        .expect_err("blocked readiness should time out");
    assert!(matches!(error, ActrError::TimedOut), "got {error:?}");
}

#[tokio::test(start_paused = true)]
async fn data_chunk_readiness_consumes_send_budget_before_gate_dispatch() {
    let (c, ss) = ctx_with_active_session().await;

    ss.enter_rebinding().await;
    let request = tokio::spawn(async move {
        c.send_data_chunk(
            &Dest::Host,
            actr_protocol::DataChunk {
                stream_id: "stage-budget".to_string(),
                sequence: 1,
                ..Default::default()
            },
            actr_protocol::PayloadType::StreamReliable,
        )
        .await
    });

    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(15)).await;
    tokio::task::yield_now().await;
    assert!(
        request.is_finished(),
        "readiness wait must be bounded by the DataChunk send budget"
    );

    let error = request
        .await
        .expect("request task should not panic")
        .expect_err("blocked readiness should time out");
    assert!(matches!(error, ActrError::TimedOut), "got {error:?}");
}

// ── BootstrapContextBuilder ─────────────────────────────────────────────

fn builder() -> BootstrapContextBuilder {
    use crate::outbound::{Gate, HostGate};
    let host = Arc::new(HostGate::new(Arc::new(HostTransport::new())));
    let inproc = Gate::Host(host);
    BootstrapContextBuilder::new(
        inproc.clone(),
        Some(inproc),
        Arc::new(DataChunkRegistry::new()),
        Arc::new(MediaFrameRegistry::new()),
        {
            use crate::wire::webrtc::{
                InboundQueueConfig, ReconnectConfig, SignalingConfig, WebSocketSignalingClient,
            };
            Arc::new(WebSocketSignalingClient::new(SignalingConfig {
                server_url: url::Url::parse("ws://127.0.0.1:9").unwrap(),
                connection_timeout: 1,
                heartbeat_interval: 30,
                reconnect_config: ReconnectConfig::default(),
                auth_config: None,
                webrtc_role: None,
                inbound_queue: InboundQueueConfig::default(),
            })) as Arc<dyn SignalingClient>
        },
        None,
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(RwLock::new(HashMap::new())),
        None,
        0,
    )
}

#[test]
fn bootstrap_builder_reuses_prebuilt_alias_map() {
    let b = builder();
    let first = b.build_bootstrap(&ActrId::default(), &AIdCredential::default());
    let second = b.build_bootstrap(&ActrId::default(), &AIdCredential::default());

    assert!(Arc::ptr_eq(
        &first.alias_map_for_test(),
        &second.alias_map_for_test()
    ));
}

#[test]
fn bootstrap_builder_setters_update_state() {
    let mut b = builder();
    b.set_session_state(None);
    b.set_generation(42);
    let _c = b.build_bootstrap(&ActrId::default(), &AIdCredential::default());
}

#[tokio::test]
async fn build_bootstrap_prefers_session_generation_when_set() {
    let mut b = builder();
    let snap =
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 9);
    let ss = SessionState::new(snap);
    b.set_session_state(Some(ss.clone()));

    let c = b.build_bootstrap(&ActrId::default(), &AIdCredential::default());
    assert_eq!(c.context_generation, 9);
    assert!(c.ensure_session_ready().await.is_ok());
}

#[tokio::test]
async fn build_bootstrap_falls_back_to_builder_generation() {
    let b = builder();
    let c = b.build_bootstrap(&ActrId::default(), &AIdCredential::default());
    assert_eq!(c.context_generation, 0);
    assert!(c.ensure_session_ready().await.is_ok());
    assert!(c.caller_id.is_none());
}

// ── get_dependency_fingerprint ──────────────────────────────────────────

use actr_config::lock::{LOCK_FILE_VERSION, LockFile, LockMetadata, LockedDependency};

fn ctx_with_lock(lock: Option<LockFile>) -> RuntimeContext {
    use crate::inbound::{DataChunkRegistry, MediaFrameRegistry};
    use crate::outbound::{Gate, HostGate};
    use crate::wire::webrtc::{
        InboundQueueConfig, ReconnectConfig, SignalingConfig, WebSocketSignalingClient,
    };
    let host = Arc::new(HostTransport::new());
    let inproc = Gate::Host(Arc::new(HostGate::new(host)));
    RuntimeContext::new(
        ActrId::default(),
        None,
        "req".into(),
        inproc,
        None,
        Arc::new(DataChunkRegistry::new()),
        Arc::new(MediaFrameRegistry::new()),
        Arc::new(WebSocketSignalingClient::new(SignalingConfig {
            server_url: url::Url::parse("ws://127.0.0.1:9").unwrap(),
            connection_timeout: 1,
            heartbeat_interval: 30,
            reconnect_config: ReconnectConfig::default(),
            auth_config: None,
            webrtc_role: None,
            inbound_queue: InboundQueueConfig::default(),
        })) as Arc<dyn SignalingClient>,
        AIdCredential::default(),
        lock.map(Arc::new),
        Arc::new(RwLock::new(HashMap::new())),
        None,
        0,
    )
}

fn dep(actr_type: &str, fingerprint: &str) -> LockedDependency {
    LockedDependency {
        name: actr_type.to_string(),
        alias: actr_type.to_string(),
        actr_type: actr_type.to_string(),
        description: None,
        fingerprint: fingerprint.to_string(),
        published_at: None,
        tags: vec![],
        cached_at: String::new(),
        files: vec![],
    }
}

#[test]
fn lock_alias_map_requires_v2_and_never_uses_name_as_alias() {
    let mut dependency = dep("acme/Sensor@1.0.0", "sha256:sensor");
    dependency.name = "SensorService".to_string();
    dependency.alias = "sensor".to_string();

    let v1 = LockFile {
        metadata: None,
        dependencies: vec![dependency.clone()],
    };
    assert!(build_alias_map(&v1).is_empty());

    let v2 = LockFile {
        metadata: Some(LockMetadata {
            version: LOCK_FILE_VERSION,
            generated_at: String::new(),
        }),
        dependencies: vec![dependency],
    };
    let aliases = build_alias_map(&v2);
    assert!(!aliases.contains_key("SensorService"));
    assert_eq!(
        aliases
            .get("sensor")
            .map(ActrType::to_string_repr)
            .as_deref(),
        Some("acme/Sensor@1.0.0")
    );
}

#[tokio::test]
async fn get_dependency_fingerprint_returns_none_without_lock() {
    let c = ctx_with_lock(None);
    assert_eq!(
        c.get_dependency_fingerprint(&actr_type("acme", "Sensor", "1.0.0")),
        None
    );
}

#[tokio::test]
async fn get_dependency_fingerprint_exact_key_match() {
    // to_string_repr uses ':' separators.
    let lock = LockFile {
        metadata: Some(LockMetadata {
            version: LOCK_FILE_VERSION,
            generated_at: String::new(),
        }),
        dependencies: vec![dep("acme/Sensor@1.0.0", "sha256:exact")],
    };
    let c = ctx_with_lock(Some(lock));
    assert_eq!(
        c.get_dependency_fingerprint(&actr_type("acme", "Sensor", "1.0.0")),
        Some("sha256:exact".to_string())
    );
}

#[tokio::test]
async fn get_dependency_fingerprint_missing_dependency() {
    let lock = LockFile {
        metadata: None,
        dependencies: vec![dep("acme/Other@2.0.0", "sha256:other")],
    };
    let c = ctx_with_lock(Some(lock));
    assert_eq!(
        c.get_dependency_fingerprint(&actr_type("acme", "Sensor", "1.0.0")),
        None
    );
}

// ── sender-side call/tell argument validation (#254 / #256) ──────────────

#[tokio::test]
async fn typed_call_rejects_non_positive_timeout() {
    let c = ctx();

    for timeout_ms in [0, -1] {
        let err = c
            .call(&Dest::Workload, TimeoutRequest::default(), timeout_ms)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ActrError::InvalidArgument(_)),
            "typed call must reject timeout_ms={timeout_ms}, got {err:?}"
        );
    }
}

#[tokio::test]
async fn typed_call_honors_caller_timeout() {
    let c = ctx();
    let err = c
        .call(&Dest::Workload, TimeoutRequest::default(), 5)
        .await
        .unwrap_err();

    assert!(
        matches!(err, ActrError::DeliveryUncertain { .. }),
        "typed call must propagate its timeout to the transport, got {err:?}"
    );
}

#[tokio::test]
async fn call_raw_rejects_zero_timeout() {
    let c = ctx();
    let err = c
        .call_raw(
            &Dest::Workload,
            "pkg.Service.Method".to_string(),
            PayloadType::RpcReliable,
            Bytes::from_static(b"payload"),
            0,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::InvalidArgument(_)),
        "timeout_ms == 0 must be rejected at the call entry point, got {err:?}"
    );
}

#[tokio::test]
async fn call_raw_rejects_negative_timeout() {
    // A negative i64 would otherwise wrap through `as u64` into an
    // effectively infinite Duration.
    let c = ctx();
    let err = c
        .call_raw(
            &Dest::Workload,
            "pkg.Service.Method".to_string(),
            PayloadType::RpcReliable,
            Bytes::from_static(b"payload"),
            -1,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::InvalidArgument(_)),
        "negative timeout_ms must be rejected, got {err:?}"
    );
}

#[tokio::test]
async fn call_raw_rejects_stream_payload_type() {
    let c = ctx();
    let err = c
        .call_raw(
            &Dest::Workload,
            "pkg.Service.Method".to_string(),
            PayloadType::StreamReliable,
            Bytes::from_static(b"payload"),
            5000,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::InvalidArgument(_)),
        "StreamReliable must be rejected on the call path, got {err:?}"
    );
}

#[tokio::test]
async fn call_raw_rejects_media_payload_type() {
    let c = ctx();
    let err = c
        .call_raw(
            &Dest::Workload,
            "pkg.Service.Method".to_string(),
            PayloadType::MediaRtp,
            Bytes::from_static(b"payload"),
            5000,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::InvalidArgument(_)),
        "MediaRtp must be rejected on the call path, got {err:?}"
    );
}

#[tokio::test]
async fn tell_raw_rejects_stream_payload_type() {
    let c = ctx();
    let err = c
        .tell_raw(
            &Dest::Workload,
            "pkg.Service.Method".to_string(),
            PayloadType::StreamLatencyFirst,
            Bytes::from_static(b"payload"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::InvalidArgument(_)),
        "StreamLatencyFirst must be rejected on the tell path, got {err:?}"
    );
}

#[tokio::test]
async fn tell_raw_accepts_rpc_signal_and_stamps_tell() {
    // The tell path accepts both RPC payload types and puts a
    // Direction::Tell envelope on the wire with timeout_ms == 0 as filler.
    let transport = Arc::new(HostTransport::new());
    let c = runtime_context_with_host_transport(ActrId::default(), transport.clone());
    let lane = transport
        .get_lane(PayloadType::RpcSignal, None)
        .await
        .unwrap();

    c.tell_raw(
        &Dest::Workload,
        "pkg.Service.Method".to_string(),
        PayloadType::RpcSignal,
        Bytes::from_static(b"payload"),
    )
    .await
    .unwrap();

    let sent = lane.recv_envelope().await.unwrap();
    assert_eq!(sent.direction, Some(Direction::Tell as i32));
    assert_eq!(sent.timeout_ms, 0);
}

#[tokio::test]
async fn realm_unavailable_maps_to_permission_denied_on_both_readiness_paths() {
    // A realm-denied session is terminal: both readiness entry points must
    // surface the same PermissionDenied contract, never a retryable
    // ConnectionNotReady.
    let mut c = ctx();
    let ss = SessionState::new(
        crate::lifecycle::session_state::SessionSnapshot::empty_with_id(ActrId::default(), 9),
    );
    c.session_state = Some(ss.clone());
    c.context_generation = 9;
    ss.set_realm_unavailable().await;

    let err = c.ensure_session_ready().await.unwrap_err();
    assert!(
        matches!(err, ActrError::PermissionDenied(_)),
        "legacy path: got {err:?}"
    );

    let deadline = crate::transport::RpcDeadline::from_duration(std::time::Duration::from_secs(1));
    let err = c
        .ensure_session_ready_until(crate::timer::ids::RUNTIME_CALL_PREFLIGHT, deadline)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActrError::PermissionDenied(_)),
        "deadline path: got {err:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn cancelled_call_after_local_consumption_is_not_retryable() {
    use actr_protocol::Classify;
    let transport = Arc::new(HostTransport::new());
    let mut context = runtime_context_with_host_transport(ActrId::default(), transport.clone());
    let scope = crate::request_scope::RequestScope::root_at(
        tokio::time::Instant::now() + Duration::from_secs(10),
    );
    context.scope = Some(scope.clone());
    let call = tokio::spawn(async move {
        context
            .call_raw(
                &Dest::Workload,
                "test.cancel".into(),
                PayloadType::RpcReliable,
                Bytes::from_static(b"request"),
                1_000,
            )
            .await
    });
    let consumed = transport.recv_reliable_raw().await.unwrap();
    scope.cancel(crate::request_scope::CancelReason::CallerDropped);
    let error = call.await.unwrap().unwrap_err();
    assert!(
        matches!(&error, ActrError::DeliveryUncertain { operation_id, .. }
        if operation_id == &consumed.request_id)
    );
    assert!(!error.is_retryable());
    let late = transport
        .complete_response(&consumed.request_id, Bytes::new())
        .await;
    assert!(matches!(
        late,
        Err(crate::transport::NetworkError::InvalidArgument(_))
    ));
}
