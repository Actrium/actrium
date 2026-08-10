//! Integration tests for actr-cabi.
//!
//! Validates: (1) `export_workload!` registers a workload visible to
//! `find_workload`, (2) `init_runtime` installs the global Tokio runtime.
//!
//! Full node round-trip (MockActrixServer → from_config → link → register →
//! start → call_raw) needs a rendered actr.toml + mock actrix lifecycle and is
//! added in a follow-up once the C ABI surface is wired to a Swift consumer.

use actr_framework::{Context, MessageDispatcher, Workload};
use actr_protocol::{ActorResult, RpcEnvelope};
use async_trait::async_trait;
use bytes::Bytes;

// ── Test workload (Rust, registered via export_workload!) ───────────────────

#[derive(Default)]
struct TestEchoWorkload;

impl Workload for TestEchoWorkload {
    type Dispatcher = TestEchoDispatcher;
}

struct TestEchoDispatcher;

#[async_trait]
impl MessageDispatcher for TestEchoDispatcher {
    type Workload = TestEchoWorkload;
    async fn dispatch<C: Context>(
        _: &Self::Workload,
        env: RpcEnvelope,
        _: &C,
    ) -> ActorResult<Bytes> {
        // Echo the request payload back unchanged.
        Ok(env.payload.unwrap_or_default())
    }
}

actr_cabi::export_workload!(TestEchoWorkload, "cabi.TestEcho");

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn workload_is_registered() {
    let entry = actr_cabi::find_workload("cabi.TestEcho");
    assert!(
        entry.is_some(),
        "export_workload! should register cabi.TestEcho"
    );
}

#[test]
fn unknown_workload_not_found() {
    assert!(actr_cabi::find_workload("cabi.does-not-exist").is_none());
}

#[test]
fn runtime_init_returns_success() {
    // Idempotent; safe to call from multiple tests.
    let code = actr_cabi::init_runtime();
    assert_eq!(code, 0, "init_runtime should return SUCCESS (0)");
}

#[test]
fn opaque_handles_reject_wrong_type_and_double_release() {
    let name = b"cabi.TestEcho";
    // SAFETY: `name` is valid UTF-8 and remains readable for the duration of
    // the synchronous registry lookup.
    let handle = unsafe {
        actr_cabi::actr_workload_create(name.as_ptr().cast::<std::ffi::c_char>(), name.len())
    };
    assert!(!handle.is_null());

    // SAFETY: opaque handles may be supplied to any state-specific entry point;
    // a wrong typestate must return HANDLE_INVALID without dereferencing it.
    assert_eq!(
        unsafe { actr_cabi::actr_ref_shutdown(handle) },
        actr_cabi::HANDLE_INVALID
    );

    // SAFETY: release accepts opaque tokens. Repeating it is deliberately a
    // safe no-op so foreign finalizer races cannot double-free native memory.
    unsafe {
        actr_cabi::actr_release_handle(handle);
        actr_cabi::actr_release_handle(handle);
    }
}
