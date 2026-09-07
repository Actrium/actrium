//! WinUI EchoApp forwarding workload.
//!
//! Mirrors `tests/e2e/actr/swift-echo-app/EchoApp/ActrService.swift`'s
//! `LocalEchoServiceHandlerImpl`: on each inbound call it discovers the remote
//! `actrium:EchoService:1.0.0` and forwards the raw payload to
//! `echo.EchoService.Echo`, returning the raw `EchoResponse` bytes.
//!
//! The C# WinUI host cannot reach a remote actor directly — the actr-cabi C
//! ABI only exposes `Dest::Workload` (local) calls — so this Rust workload is
//! the bridge: the C# app calls the local workload (route
//! `cabi.EchoApp.Echo`), and this dispatcher forwards to the remote service.
//!
//! Registered as `"cabi.EchoApp"` via `export_workload!` and linked into
//! `actr_cabi.dll` by `packages/actr-cabi/csharp/build-actr-cabi-dll.sh`.
//!
//! The test-only `actr_fixture_host_bridge_*` exports at the end of this file
//! are consumed by the ActrSharp fixture package's out-of-repo Windows CI
//! consumer. They deliberately exercise only the opt-in stream sink and
//! explicit `ask_host` bridge; they do not add automatic forwarding for
//! ordinary workload RPCs or tells.

use std::sync::Mutex;

use actr_framework::{Context, MessageDispatcher, Workload};
use actr_protocol::{
    ActorResult, ActrError, ActrId, ActrType, DataChunk, MetadataEntry, RpcEnvelope,
};
use async_trait::async_trait;
use bytes::Bytes;

/// The remote EchoService actr type the app forwards to.
const ECHO_SERVICE_TYPE: &str = "actrium:EchoService:1.0.0";
/// Remote route key for `EchoService.Echo` (package.Service.Method).
const ECHO_ROUTE_KEY: &str = "echo.EchoService.Echo";
/// Call timeout for the forwarded RPC.
const ECHO_CALL_TIMEOUT_MS: i64 = 30_000;

/// Exact values asserted by the out-of-repo ActrSharp host-bridge consumer.
/// These are intentionally stable test-fixture protocol values, not a public
/// workload route or a general-purpose host callback contract.
const HOST_BRIDGE_STREAM_ID: &str = "actrsharp.fixture.host-bridge.v1";
const HOST_BRIDGE_STREAM_PAYLOAD: &[u8] = b"native-stream-payload-v1";
const HOST_BRIDGE_REQUEST_ROUTE: &str = "fixture.host-bridge.ask.v1";
const HOST_BRIDGE_REQUEST_PAYLOAD: &[u8] = b"native-reverse-request-v1";
const HOST_BRIDGE_RESPONSE_PAYLOAD: &[u8] = b"csharp-reverse-response-v1";
const HOST_BRIDGE_SEQUENCE: u64 = 580;
const HOST_BRIDGE_TIMESTAMP_MS: i64 = 1_735_689_600_123;
const HOST_BRIDGE_SENDER: &str = "1a2b3c@7/acme/AskService@1.0.0";
/// Generic failure returned by this test-only fixture's C exports.
///
/// This matches the C ABI's documented generic framework/runtime failure
/// status without exporting actr-cabi's internal Rust implementation detail.
const FIXTURE_ERROR: i32 = -50;

/// Test-fixture equivalent of an explicit stream subscription. It ensures the
/// C# consumer has selected the exact stream before the fixture emits it,
/// while avoiding an Actrix deployment solely for package-consumption CI.
static HOST_BRIDGE_LISTENER: Mutex<Option<String>> = Mutex::new(None);

/// Stateless workload. All routing lives in the dispatcher.
#[derive(Default)]
pub struct EchoAppWorkload;

impl Workload for EchoAppWorkload {
    type Dispatcher = EchoAppDispatcher;
}

pub struct EchoAppDispatcher;

#[async_trait]
impl MessageDispatcher for EchoAppDispatcher {
    type Workload = EchoAppWorkload;

    /// Forward the inbound payload to the remote EchoService verbatim.
    ///
    /// The C# host encodes the `EchoRequest` protobuf (and applies the
    /// `winui-local:` prefix mirroring Swift's local transform); this
    /// workload is a pure byte forwarder so it needs no protobuf types.
    async fn dispatch<C: Context>(
        _: &Self::Workload,
        env: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        let target_type = ActrType::from_string_repr(ECHO_SERVICE_TYPE)
            .map_err(|e| ActrError::Unavailable(format!("invalid EchoService target type: {e}")))?;
        let target_id = ctx.discover_route_candidate(&target_type).await?;
        let payload = env.payload.unwrap_or_default();
        ctx.call_raw(&target_id, ECHO_ROUTE_KEY, payload, ECHO_CALL_TIMEOUT_MS)
            .await
    }
}

actr_cabi::export_workload!(EchoAppWorkload, "cabi.EchoApp");

// Declare the bundle ID so hosts can confirm which native provider was loaded
// via ActrCabiNode.ValidateNative / actr_cabi_bundle_id before driving the
// typestate chain. The CI fixture packages this cdylib as actr_cabi.dll.
actr_cabi::export_bundle!("actr.e2e.echo-app");

/// Install the exact test-fixture stream listener used by the Windows NuGet
/// consumer. The native probe rejects a trigger until this listener matches
/// [`HOST_BRIDGE_STREAM_ID`], so CI proves the consumer selected the intended
/// stream rather than observing an unfiltered managed record.
///
/// # Safety
///
/// `stream_id_ptr` must be non-null and valid for `stream_id_len` UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn actr_fixture_host_bridge_listen(
    stream_id_ptr: *const u8,
    stream_id_len: usize,
) -> i32 {
    let stream_id = match decode_fixture_stream_id(stream_id_ptr, stream_id_len) {
        Ok(stream_id) => stream_id,
        Err(code) => return code,
    };
    if stream_id != HOST_BRIDGE_STREAM_ID {
        return FIXTURE_ERROR;
    }

    *HOST_BRIDGE_LISTENER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(stream_id.to_owned());
    actr_cabi::SUCCESS
}

/// Remove the exact test-fixture stream listener. This is idempotent so the
/// C# consumer can always clean up after a partial failure.
///
/// # Safety
///
/// `stream_id_ptr` must be non-null and valid for `stream_id_len` UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn actr_fixture_host_bridge_unlisten(
    stream_id_ptr: *const u8,
    stream_id_len: usize,
) -> i32 {
    let stream_id = match decode_fixture_stream_id(stream_id_ptr, stream_id_len) {
        Ok(stream_id) => stream_id,
        Err(code) => return code,
    };
    if stream_id != HOST_BRIDGE_STREAM_ID {
        return FIXTURE_ERROR;
    }

    let mut listener = HOST_BRIDGE_LISTENER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if listener.as_deref() == Some(stream_id) {
        *listener = None;
    }
    actr_cabi::SUCCESS
}

/// Emit one fully populated native [`DataChunk`] through actr-cabi's host
/// stream bridge, then issue an explicit reverse RPC with [`actr_cabi::ask_host`].
/// The probe succeeds only if C# answers with the exact expected bytes.
///
/// This synchronous test-only entry point owns a small current-thread runtime
/// so an out-of-repo package consumer needs neither an Actrix service nor a
/// synthetic managed-only callback path. The reverse-RPC callback returns to
/// C# through the real `actr_register_reverse_rpc_sink` ABI export.
#[no_mangle]
pub extern "C" fn actr_fixture_host_bridge_trigger() -> i32 {
    let listener_is_ready = HOST_BRIDGE_LISTENER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_deref()
        == Some(HOST_BRIDGE_STREAM_ID);
    if !listener_is_ready {
        return FIXTURE_ERROR;
    }

    let sender = match ActrId::from_string_repr(HOST_BRIDGE_SENDER) {
        Ok(sender) => sender,
        Err(_) => return FIXTURE_ERROR,
    };
    let chunk = DataChunk {
        stream_id: HOST_BRIDGE_STREAM_ID.to_owned(),
        sequence: HOST_BRIDGE_SEQUENCE,
        payload: Bytes::from_static(HOST_BRIDGE_STREAM_PAYLOAD),
        metadata: vec![
            MetadataEntry {
                key: "content-type".to_owned(),
                value: "application/octet-stream".to_owned(),
            },
            MetadataEntry {
                key: "fixture".to_owned(),
                value: "pr-580".to_owned(),
            },
        ],
        timestamp_ms: Some(HOST_BRIDGE_TIMESTAMP_MS),
    };
    if !actr_cabi::forward_chunk_to_host(&chunk, &sender) {
        return FIXTURE_ERROR;
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return FIXTURE_ERROR,
    };
    match runtime.block_on(actr_cabi::ask_host(
        HOST_BRIDGE_REQUEST_ROUTE,
        Bytes::from_static(HOST_BRIDGE_REQUEST_PAYLOAD),
        5_000,
    )) {
        Ok(response) if response.as_ref() == HOST_BRIDGE_RESPONSE_PAYLOAD => actr_cabi::SUCCESS,
        Ok(_) | Err(_) => FIXTURE_ERROR,
    }
}

/// Decode the borrowed stream ID used solely by the fixture's C ABI exports.
/// Keeping this here prevents the test-only exports from widening actr-cabi's
/// production C surface.
unsafe fn decode_fixture_stream_id<'a>(
    stream_id_ptr: *const u8,
    stream_id_len: usize,
) -> Result<&'a str, i32> {
    if stream_id_ptr.is_null() {
        return Err(actr_cabi::NULL_POINTER);
    }
    // SAFETY: the C entry point documents that the pointer is readable for
    // stream_id_len bytes for this call.
    let bytes = unsafe { std::slice::from_raw_parts(stream_id_ptr, stream_id_len) };
    std::str::from_utf8(bytes).map_err(|_| actr_cabi::INVALID_UTF8)
}
