use super::*;

#[test]
fn compile_rejects_non_wasm_bytes() {
    let err = WasmHost::compile(b"definitely not a wasm component").unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(err.to_string().contains("Component"));
}

#[test]
fn compile_rejects_empty_bytes() {
    let err = WasmHost::compile(&[]).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
}

#[test]
fn compile_with_limits_rejects_oversized_component_before_parsing() {
    let limits = WasmRuntimeLimits {
        max_component_bytes: 3,
        ..WasmRuntimeLimits::default()
    };
    let error = WasmHost::compile_with_limits(b"\0asm", &limits).unwrap_err();
    assert!(matches!(
        error,
        WasmError::ResourceLimitExceeded("component byte size")
    ));
}

#[test]
fn compile_rejects_legacy_core_module_magic() {
    // `\0asm` magic + invalid body must still fail (host requires
    // Component Model binaries).
    let bogus = b"\0asm\x01\x00\x00\x00garbage";
    let err = WasmHost::compile(bogus).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
}

/// Compile a WAT component source into a real wasmtime [`Component`] on an
/// engine configured exactly like production ([`build_engine`]).
fn probe_wat(src: &str) -> WasmResult<()> {
    let bytes = wat::parse_str(src).expect("test WAT must assemble");
    let engine =
        build_engine(&crate::config::WasmRuntimeLimits::default()).expect("engine must build");
    let component = Component::from_binary(&engine, &bytes).expect("component must load");
    probe_world(&component, &engine)
}

#[test]
fn probe_world_rejects_component_without_workload_world() {
    // `(false, false)` arm: a component that exports no recognised
    // `actr:workload/workload` world (here: no exports at all) must be a
    // clean `LoadFailed`, never a panic.
    let err = probe_wat("(component)").unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("no recognised"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_retired_v1_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v1)
          (export "actr:workload/workload@0.1.0" (instance $v1))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string()
            .contains("rebuild the package with the current SDK"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_accepts_current_v08_component() {
    let src = r#"
        (component
          (instance $v08)
          (export "actr:workload/workload@0.8.0" (instance $v08))
        )
    "#;
    probe_wat(src).expect("current workload ABI should be accepted");
}

#[test]
fn probe_world_rejects_retired_v06_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v06)
          (export "actr:workload/workload@0.6.0" (instance $v06))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("@0.6.0")
            && err.to_string().contains("@0.8.0")
            && err
                .to_string()
                .contains("rebuild the package with the current SDK"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_retired_v07_actor_identity_with_rebuild_hint() {
    let src = r#"(component
      (instance $old)
      (export "actr:workload/workload@0.7.0" (instance $old))
    )"#;
    let err = probe_wat(src).unwrap_err().to_string();
    assert!(
        err.contains("retired")
            && err.contains("@0.7.0")
            && err.contains("@0.8.0")
            && err.contains("rebuild"),
        "{err}"
    );
}

#[test]
fn probe_world_rejects_retired_v5_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v5)
          (export "actr:workload/workload@0.5.0" (instance $v5))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("EXPLICIT reply")
            && err.to_string().contains("@0.8.0")
            && err
                .to_string()
                .contains("rebuild the package with the current SDK"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_retired_v4_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v4)
          (export "actr:workload/workload@0.4.0" (instance $v4))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("on-webrtc-state-changed")
            && err.to_string().contains("@0.8.0")
            && err
                .to_string()
                .contains("rebuild the package with the current SDK"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_retired_v3_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v3)
          (export "actr:workload/workload@0.3.0" (instance $v3))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("payload-type lane")
            && err.to_string().contains("@0.8.0")
            && err
                .to_string()
                .contains("rebuild the package with the current SDK"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_retired_v2_component_with_rebuild_hint() {
    let src = r#"
        (component
          (instance $v2)
          (export "actr:workload/workload@0.2.0" (instance $v2))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("caller-selected timeout") && err.to_string().contains("@0.8.0"),
        "unexpected message: {err}"
    );
}

#[test]
fn probe_world_rejects_component_exporting_multiple_worlds() {
    // A component that exports multiple ABI versions is ambiguous and must
    // be a clean `LoadFailed`, never a panic.
    let src = r#"
        (component
          (instance $v1)
          (instance $v3)
          (export "actr:workload/workload@0.1.0" (instance $v1))
          (export "actr:workload/workload@0.3.0" (instance $v3))
        )
    "#;
    let err = probe_wat(src).unwrap_err();
    assert!(matches!(err, WasmError::LoadFailed(_)));
    assert!(
        err.to_string().contains("multiple"),
        "unexpected message: {err}"
    );
}

#[test]
fn epoch_deadline_preserves_sub_millisecond_precision() {
    let limits = WasmRuntimeLimits {
        epoch_tick: std::time::Duration::from_micros(1),
        invocation_timeout: std::time::Duration::from_secs(5),
        ..WasmRuntimeLimits::default()
    };

    assert_eq!(epoch_deadline_ticks(&limits), 5_000_001);
}

#[test]
fn epoch_deadline_rounds_fractional_tick_up() {
    let limits = WasmRuntimeLimits {
        epoch_tick: std::time::Duration::from_micros(1_900),
        invocation_timeout: std::time::Duration::from_millis(5),
        ..WasmRuntimeLimits::default()
    };

    assert_eq!(epoch_deadline_ticks(&limits), 3);
}

#[derive(Clone, Default)]
struct CaptureDeliver(Arc<std::sync::Mutex<Vec<ReplyOutcome>>>);

impl Deliver for CaptureDeliver {
    fn deliver(&self, outcome: ReplyOutcome) {
        self.0.lock().expect("capture mutex poisoned").push(outcome);
    }
}

fn invocation_context(request_id: &str) -> InvocationContext {
    InvocationContext {
        self_id: actr_protocol::ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 1,
        },
        caller_id: None,
        request_id: request_id.to_string(),
    }
}

fn noop_host_abi() -> HostAbiFn {
    Arc::new(|_| Box::pin(async { crate::workload::HostOperationResult::Done }))
}

#[test]
fn deferred_reply_invocation_expires_at_inflight_ttl() {
    let limits = WasmRuntimeLimits::default();
    let mut state = HostState::new(&limits);
    let delivered = CaptureDeliver::default();
    let captured = Arc::clone(&delivered.0);
    let token = state.alloc_invocation_with_reply(
        invocation_context("deferred-reply"),
        noop_host_abi(),
        Some(Arc::new(delivered)),
        true,
        Some(Arc::new(std::sync::atomic::AtomicBool::new(true))),
        None,
    );
    let admitted_at = state.invocations[&token].admitted_at;

    state.finish_invocation(token, true);
    assert_eq!(state.invocation_count(), 1);

    let expired = state.sweep_expired_deferred_replies(
        crate::lifecycle::dedup::INFLIGHT_TTL,
        admitted_at + crate::lifecycle::dedup::INFLIGHT_TTL,
    );

    assert_eq!(expired, 1);
    assert_eq!(state.invocation_count(), 0);
    let outcomes = captured.lock().expect("capture mutex poisoned");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].request_id, "deferred-reply");
    assert!(matches!(
        &outcomes[0].result,
        Err(actr_protocol::ActrError::TimedOut)
    ));
    assert!(outcomes[0].emit_response);
}

#[test]
fn running_invocation_is_not_expired_by_deferred_reply_sweep() {
    let limits = WasmRuntimeLimits::default();
    let mut state = HostState::new(&limits);
    let delivered = CaptureDeliver::default();
    let captured = Arc::clone(&delivered.0);
    let token = state.alloc_invocation_with_reply(
        invocation_context("running-handler"),
        noop_host_abi(),
        Some(Arc::new(delivered)),
        true,
        None,
        None,
    );
    let admitted_at = state.invocations[&token].admitted_at;

    let expired = state.sweep_expired_deferred_replies(
        crate::lifecycle::dedup::INFLIGHT_TTL,
        admitted_at + crate::lifecycle::dedup::INFLIGHT_TTL,
    );

    assert_eq!(expired, 0);
    assert_eq!(state.invocation_count(), 1);
    assert!(captured.lock().expect("capture mutex poisoned").is_empty());
}

#[test]
fn deferred_tell_expiry_completes_without_response() {
    let limits = WasmRuntimeLimits::default();
    let mut state = HostState::new(&limits);
    let delivered = CaptureDeliver::default();
    let captured = Arc::clone(&delivered.0);
    let token = state.alloc_invocation_with_reply(
        invocation_context("deferred-tell"),
        noop_host_abi(),
        Some(Arc::new(delivered)),
        false,
        Some(Arc::new(std::sync::atomic::AtomicBool::new(true))),
        None,
    );
    let admitted_at = state.invocations[&token].admitted_at;
    state.finish_invocation(token, true);

    let expired = state.sweep_expired_deferred_replies(
        crate::lifecycle::dedup::INFLIGHT_TTL,
        admitted_at + crate::lifecycle::dedup::INFLIGHT_TTL,
    );

    assert_eq!(expired, 1);
    let outcomes = captured.lock().expect("capture mutex poisoned");
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].emit_response);
    assert!(matches!(&outcomes[0].result, Ok(bytes) if bytes.is_empty()));
}

#[test]
fn reply_import_completion_retires_token_when_dispatch_finishes() {
    let limits = WasmRuntimeLimits::default();
    let mut state = HostState::new(&limits);
    let delivered = CaptureDeliver::default();
    let captured = Arc::clone(&delivered.0);
    let token = state.alloc_invocation_with_reply(
        invocation_context("reply-import-wins"),
        noop_host_abi(),
        Some(Arc::new(delivered)),
        true,
        None,
        None,
    );

    assert!(state.deliver_reply(token, Ok(actr_framework::Bytes::from_static(b"explicit"))));
    assert_eq!(
        state.invocation_count(),
        1,
        "token stays live until the dispatch export returns"
    );
    assert!(state.invocation_reply_completed(token));

    state.finish_invocation(token, false);
    assert_eq!(state.invocation_count(), 0);
    assert_eq!(captured.lock().expect("capture mutex poisoned").len(), 1);
}
