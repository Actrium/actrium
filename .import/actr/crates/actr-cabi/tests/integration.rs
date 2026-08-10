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
fn abi_version_is_one() {
    assert_eq!(actr_cabi::ABI_VERSION, 1, "ABI_VERSION constant");
    assert_eq!(actr_cabi::actr_cabi_abi_version(), 1, "C ABI query");
}

#[test]
fn actr_version_matches_cargo_pkg_version() {
    assert_eq!(actr_cabi::ACTR_VERSION, env!("CARGO_PKG_VERSION"));
    // SAFETY: out_len is a writable local; actr_cabi_actr_version writes the
    // static string length into it and returns a static pointer.
    let mut len = 0usize;
    let ptr = unsafe { actr_cabi::actr_cabi_actr_version(&mut len) };
    assert!(
        !ptr.is_null(),
        "actr_cabi_actr_version must not return null"
    );
    // SAFETY: the pointer is valid for `len` bytes for the process lifetime.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    let version = std::str::from_utf8(bytes).expect("version must be valid UTF-8");
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn bundle_id_unset_without_export_bundle() {
    // This test binary does not call export_bundle!, so no bundle is
    // registered. (tests/bundle.rs is a separate binary that does, so its
    // process-global inventory stays isolated from this one.)
    assert_eq!(actr_cabi::bundle_id(), None);
    assert_eq!(actr_cabi::bundle_count(), 0);
    // SAFETY: out_len is a writable local; the query returns null and zeroes
    // out_len when no bundle is registered.
    let mut len = 42usize;
    let ptr = unsafe { actr_cabi::actr_cabi_bundle_id(&mut len) };
    assert!(ptr.is_null(), "bundle_id should be null when unregistered");
    assert_eq!(len, 0, "out_len must be zeroed when returning null");
    assert_eq!(actr_cabi::actr_cabi_bundle_count(), 0);
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
