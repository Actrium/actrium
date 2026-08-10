//! WinUI EchoApp forwarding workload.
//!
//! Mirrors `e2e/swift-echo-app/EchoApp/ActrService.swift`'s
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
//! `actr_cabi.dll` by `bindings/cabi/csharp/build-actr-cabi-dll.sh`.

use actr_framework::{Context, MessageDispatcher, Workload};
use actr_protocol::{ActorResult, ActrError, ActrType, RpcEnvelope};
use async_trait::async_trait;
use bytes::Bytes;

/// The remote EchoService actr type the app forwards to.
const ECHO_SERVICE_TYPE: &str = "actrium:EchoService:1.0.0";
/// Remote route key for `EchoService.Echo` (package.Service.Method).
const ECHO_ROUTE_KEY: &str = "echo.EchoService.Echo";
/// Call timeout for the forwarded RPC.
const ECHO_CALL_TIMEOUT_MS: i64 = 30_000;

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
