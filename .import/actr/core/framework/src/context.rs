//! Context trait - Execution context interface for actors

use actr_protocol::{ActorResult, ActrError, ActrId, ActrType, DataChunk, PayloadType};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::future::BoxFuture;

/// Default RPC timeout (milliseconds) applied by the four public Context APIs
/// when the caller does not pass an explicit deadline.
///
/// Rust runtime helpers reference this constant directly. Other language
/// facades expose the same value in their own runtime packages; generated
/// dispatch code MUST NOT inline it.
pub const DEFAULT_RPC_TIMEOUT_MS: i64 = 30_000;

/// Reject invalid explicit RPC deadlines before a guest enters transport.
#[allow(dead_code)]
pub(crate) fn validate_rpc_timeout_ms(timeout_ms: i64) -> ActorResult<()> {
    if timeout_ms <= 0 {
        return Err(ActrError::InvalidArgument(format!(
            "RPC call timeout_ms must be > 0, got {timeout_ms}"
        )));
    }
    Ok(())
}

/// Boxed callback future that is `Send` on native targets and local on WASM.
///
/// This mirrors [`MaybeSendSync`]: native runtimes may move callbacks between
/// executor threads, while Component Model and web guests run cooperatively on
/// one WASM thread and may legitimately capture a non-`Send` [`Context`].
#[cfg(not(target_arch = "wasm32"))]
pub type MaybeSendBoxFuture<'a, T> = futures_util::future::BoxFuture<'a, T>;

/// Boxed callback future that is `Send` on native targets and local on WASM.
#[cfg(target_arch = "wasm32")]
pub type MaybeSendBoxFuture<'a, T> = futures_util::future::LocalBoxFuture<'a, T>;

// ── MaybeSendSync marker ────────────────────────────────────────────────
//
// Auto-trait-style marker that is `Send + Sync` on native targets and empty
// on `wasm32`. Per Option U γ-unified §3.1 the user-facing `Context` bound
// is `Clone + 'static`, but native runtime layers (tokio multi-thread,
// `WorkloadHookObserver`) must produce `Send` futures; pinning `Send + Sync`
// behind this marker lets the framework trait carry a single `?Send`
// definition while silently re-asserting the native auto traits through a
// cfg-gated blanket impl.

/// Auto-trait-style marker — `Send + Sync` on native, empty on `wasm32`.
///
/// Used as a supertrait on `Context`, `Workload`, and `MessageDispatcher`
/// so `async_trait` default bodies compile in both modes without adding
/// explicit `Send` / `Sync` bounds to the user-visible trait definition.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSendSync for T {}

/// Auto-trait-style marker — `Send + Sync` on native, empty on `wasm32`.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSendSync for T {}

/// Actor execution context interface.
///
/// Defines the complete interface for an actor to interact with the runtime:
/// - Context data access (`self_id`, `request_id`, …)
/// - Communication primitives (`call`, `tell`)
///
/// # Design principles
///
/// - **Interface only**: the framework provides no implementation; the runtime
///   does.
/// - **Generic parameter**: user code accepts `<C: Context>` rather than
///   `&dyn Context`, so dispatch monomorphises and avoids vtables.
///
/// # cfg dispatch
///
/// The trait is `?Send` on `wasm32` (browser single-threaded, futures can
/// legitimately not be `Send`) and `#[async_trait]` (Send mode) on native
/// targets so tokio-multi-thread spawners downstream keep working. The
/// `MaybeSendSync` supertrait adds `Send + Sync` on native only — per
/// Option U γ-unified §3.1 the user-visible bound stays `Clone + 'static`
/// while `Workload` default method bodies (which need `Send` futures under
/// the native `async_trait`) compile without any extra constraint on the
/// generic `C`.
///
/// # Example
///
/// ```rust,ignore
/// async fn my_handler<C: Context>(ctx: &C) {
///     let id = ctx.self_id();
///     let response = ctx.call(&target, request).await?;
/// }
/// ```
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Context: Clone + MaybeSendSync + 'static {
    // ========== Data Access Methods ==========

    /// Get the current Actor's ID
    fn self_id(&self) -> &ActrId;

    /// Get the caller's Actor ID
    ///
    /// - `Some(caller_id)`: Called by another Actor
    /// - `None`: System internal call (e.g., lifecycle hooks)
    fn caller_id(&self) -> Option<&ActrId>;

    /// Get the unique request ID
    ///
    /// A new request_id is generated for each RPC call, used to match requests and responses.
    fn request_id(&self) -> &str;

    /// Whether the caller is waiting for a response (`call`) or not (`tell`).
    ///
    /// Set per-request by the runtime from the inbound envelope. Used by
    /// generated `ReplyMode::Explicit` dispatch arms to build the `Reply<T>`
    /// handle with the right `emit_response` flag: a `tell` into an EXPLICIT
    /// method completes dedup with empty bytes and emits no RESPONSE envelope.
    /// The default is `true` (a `call`); contexts that never host EXPLICIT
    /// handlers never have this queried.
    fn expects_response(&self) -> bool {
        true
    }

    /// Reply-delivery handle for `ReplyMode::Explicit` methods (RFC-0323).
    ///
    /// `None` on contexts that do not host EXPLICIT dispatch (the default):
    /// tests, dummy contexts, and runtimes without the pump wired. A
    /// generated EXPLICIT dispatch arm calls this and, if `None`, fails the
    /// dispatch with an `Internal` error rather than silently dropping the
    /// reply (reject-don't-degrade). The hyper `RuntimeContext` overrides this
    /// with the per-request `Deliver` that wires `reply.send` → RESPONSE
    /// envelope + dedup completion + mailbox ACK.
    fn reply_deliver(&self) -> Option<std::sync::Arc<dyn crate::Deliver>> {
        None
    }

    /// Caller deadline for an EXPLICIT reply (RFC-0323): arrival time plus
    /// `envelope.timeout_ms`. `None` for a TELL or a route with no timeout.
    /// The generated EXPLICIT dispatch arm reads this so `Reply::deadline()`
    /// reflects the real caller timeout rather than unconditionally `None`.
    /// Sending after the deadline is still valid — the caller discards the
    /// orphan RESPONSE. The default `None` suits contexts that never host
    /// EXPLICIT dispatch.
    fn reply_deadline(&self) -> Option<crate::ReplyDeadline> {
        None
    }

    // ========== Communication Methods ==========

    /// Send a type-safe RPC request and wait for response
    ///
    /// This is the primary way to call other Actors, providing full type safety guarantees.
    ///
    /// # Type Inference
    ///
    /// Response type is automatically inferred from `R::Response`, no manual annotation needed:
    ///
    /// ```rust,ignore
    /// let request = EchoRequest { message: "hello".to_string() };
    /// let response: EchoResponse = ctx.call(&target, request).await?;
    /// //              ^^^^^^^^^^^^ Inferred from EchoRequest::Response
    /// ```
    ///
    /// # Error Handling
    ///
    /// - `ProtocolError::TransportError`: Network transport failure
    /// - `ProtocolError::Actr(DecodeFailure)`: Response decode failure
    /// - `ProtocolError::Actr(UnknownRoute)`: Route does not exist
    /// - Errors returned by remote Actor's business logic
    ///
    /// # Parameters
    ///
    /// - `target`: Target destination (`Dest::Host` for local, `Dest::Peer(id)` for remote)
    /// - `request`: Request message implementing `RpcRequest` trait
    ///
    /// Applies the [`DEFAULT_RPC_TIMEOUT_MS`] deadline; pass an explicit value
    /// through [`Context::call_with_timeout`] instead.
    ///
    /// # Returns
    ///
    /// Returns response message of type `R::Response`
    async fn call<R: actr_protocol::RpcRequest>(
        &self,
        target: &crate::Dest,
        request: R,
    ) -> ActorResult<R::Response> {
        self.call_with_timeout(target, request, DEFAULT_RPC_TIMEOUT_MS)
            .await
    }

    /// Send a type-safe RPC request with an explicit timeout (milliseconds).
    ///
    /// Same semantics as [`Context::call`], but the caller controls the
    /// deadline. Implementations MUST validate `timeout_ms > 0` before entering
    /// transport.
    async fn call_with_timeout<R: actr_protocol::RpcRequest>(
        &self,
        target: &crate::Dest,
        request: R,
        timeout_ms: i64,
    ) -> ActorResult<R::Response>;

    /// Send a type-safe one-way message (no response expected)
    ///
    /// Used for sending notifications, events, etc. that do not require a response.
    ///
    /// # Semantics
    ///
    /// - **Fire-and-forget**: Does not wait for response after sending
    /// - **No delivery guarantee**: Message may be lost if target is unreachable
    /// - **Low latency**: Does not block waiting for response
    ///
    /// # Parameters
    ///
    /// - `target`: Target destination (`Dest::Host` for local, `Dest::Peer(id)` for remote)
    /// - `message`: Message implementing `RpcRequest` trait
    async fn tell<R: actr_protocol::RpcRequest>(
        &self,
        target: &crate::Dest,
        message: R,
    ) -> ActorResult<()>;

    // ========== Fast Path: DataChunk Methods ==========

    /// Register a DataChunk callback for a specific stream
    ///
    /// When a DataChunk with matching stream_id arrives, the registered callback will be invoked.
    /// Delivery is FIFO and run-to-completion for one stream. Different streams
    /// use independent inbound workers, although a serial-only workload backend
    /// may still serialize their final guest callbacks.
    ///
    /// # Parameters
    ///
    /// - `stream_id`: Stream identifier (must be globally unique)
    /// - `callback`: Handler function that receives (DataChunk, sender ActrId)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// ctx.register_stream("log-stream", |chunk, sender| {
    ///     Box::pin(async move {
    ///         println!("Received chunk {} from {:?}", chunk.sequence, sender);
    ///         Ok(())
    ///     })
    /// }).await?;
    /// ```
    async fn register_stream<F>(&self, stream_id: String, callback: F) -> ActorResult<()>
    where
        F: Fn(DataChunk, ActrId) -> MaybeSendBoxFuture<'static, ActorResult<()>>
            + MaybeSendSync
            + 'static;

    /// Unregister a DataChunk callback
    ///
    /// # Parameters
    ///
    /// - `stream_id`: Stream identifier to unregister
    async fn unregister_stream(&self, stream_id: &str) -> ActorResult<()>;

    /// Send a DataChunk to a destination with explicit lane selection.
    ///
    /// Use [`PayloadType::StreamReliable`] for ordered reliable delivery (default) or
    /// [`PayloadType::StreamLatencyFirst`] for low-latency partial-reliable delivery.
    ///
    /// # Parameters
    ///
    /// - `target`: Target destination
    /// - `chunk`: DataChunk to send
    /// - `payload_type`: Lane selection (`StreamReliable` or `StreamLatencyFirst`)
    async fn send_data_chunk(
        &self,
        target: &crate::Dest,
        chunk: DataChunk,
        payload_type: PayloadType,
    ) -> ActorResult<()>;

    /// Discover a remote Actor of the specified type via the signaling server.
    ///
    /// Returns a route candidate or an error if none are available. Concrete
    /// selection strategy is decided by the Context implementation.
    async fn discover_route_candidate(&self, target_type: &ActrType) -> ActorResult<ActrId>;

    /// Resolve a dependency alias to its declared [`ActrType`].
    ///
    /// The runtime builds an `alias -> ActrType` map when the workload is
    /// attached. Packaged workloads use the authenticated
    /// `manifest.lock.toml`; linked iOS/Android workloads use their dynamic
    /// `manifest.toml`. Generated dispatchers and business code use this
    /// uniform API before calling [`Context::discover_route_candidate`].
    ///
    /// Returns [`ActrError::DependencyNotFound`] when the alias is absent.
    async fn resolve_dependency(&self, alias: &str) -> ActorResult<ActrType> {
        Err(ActrError::DependencyNotFound {
            service_name: alias.to_string(),
            message: format!("dependency alias `{alias}` is not resolvable on this context"),
        })
    }

    /// Send a raw RPC request (untyped bytes) and wait for response
    ///
    /// This is a lower-level method for dynamic dispatch scenarios where the
    /// request/response types are not known at compile time (e.g., FFI bindings).
    ///
    /// # Parameters
    ///
    /// - `target`: Target Actor ID
    /// - `route_key`: Route key (e.g., "echo.EchoService/Echo")
    /// - `payload`: Raw request payload bytes
    ///
    /// Applies the [`DEFAULT_RPC_TIMEOUT_MS`] deadline and always sends on the
    /// [`PayloadType::RpcReliable`] lane; use [`Context::call_raw_with_timeout`]
    /// or [`Context::call_raw_with_payload_type`] for explicit control.
    ///
    /// # Returns
    ///
    /// Returns raw response payload bytes
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // For FFI or dynamic dispatch scenarios
    /// let response = ctx.call_raw(
    ///     &target_id,
    ///     "echo.EchoService/Echo",
    ///     request_bytes.into(),
    /// ).await?;
    /// ```
    async fn call_raw(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: Bytes,
    ) -> ActorResult<Bytes> {
        self.call_raw_with_timeout(target, route_key, payload, DEFAULT_RPC_TIMEOUT_MS)
            .await
    }

    /// Send a raw RPC request with an explicit timeout (milliseconds).
    ///
    /// Same semantics as [`Context::call_raw`], but the caller controls the
    /// deadline. Implementations MUST validate `timeout_ms > 0` before entering
    /// transport and MUST send the request on the [`PayloadType::RpcReliable`]
    /// lane regardless of caller input.
    async fn call_raw_with_timeout(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: Bytes,
        timeout_ms: i64,
    ) -> ActorResult<Bytes>;

    /// Send a raw RPC request on an explicitly selected RPC payload lane.
    ///
    /// This additive entry point lets generated dynamic dispatch preserve a
    /// route's declared `RpcReliable` or `RpcSignal` lane without changing the
    /// established [`Context::call_raw`] signature. Implementations that have
    /// not opted into explicit lane selection remain compatible: reliable
    /// calls delegate to `call_raw`, while other lanes fail instead of being
    /// silently downgraded.
    async fn call_raw_with_payload_type(
        &self,
        target: &ActrId,
        route_key: &str,
        payload_type: PayloadType,
        payload: bytes::Bytes,
        timeout_ms: i64,
    ) -> ActorResult<bytes::Bytes> {
        if payload_type != PayloadType::RpcReliable {
            return Err(ActrError::InvalidArgument(format!(
                "this Context implementation does not support raw RPC lane {payload_type:?}"
            )));
        }
        self.call_raw_with_timeout(target, route_key, payload, timeout_ms)
            .await
    }

    // ========== Fast Path: MediaTrack Methods (WebRTC Native) ==========

    /// Register a WebRTC native media track callback
    ///
    /// When media samples arrive on the specified track, the registered callback will be invoked.
    /// Uses WebRTC native RTCTrackRemote, no protobuf serialization overhead.
    ///
    /// # Parameters
    ///
    /// - `track_id`: Media track identifier (must match WebRTC track ID in SDP)
    /// - `callback`: Handler function that receives native media samples
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use actr_framework::MediaSample;
    ///
    /// ctx.register_media_track("video-track-1", |sample, sender| {
    ///     Box::pin(async move {
    ///         // Decode and render video frame (native RTP payload)
    ///         println!("Received {} bytes at timestamp {}",
    ///                  sample.data.len(), sample.timestamp);
    ///         decoder.decode(&sample.data).await?;
    ///         Ok(())
    ///     })
    /// }).await?;
    /// ```
    ///
    /// # Architecture Note
    ///
    /// MediaTrack uses WebRTC native RTP channels (RTCTrackRemote), NOT DataChannel.
    /// This provides:
    /// - Zero protobuf serialization overhead
    /// - Native RTP header information (timestamp, SSRC, etc.)
    /// - Optimal latency (~1-2ms lower than DataChannel)
    async fn register_media_track<F>(&self, track_id: String, callback: F) -> ActorResult<()>
    where
        F: Fn(MediaSample, ActrId) -> BoxFuture<'static, ActorResult<()>> + Send + Sync + 'static;

    /// Unregister a media track callback
    ///
    /// # Parameters
    ///
    /// - `track_id`: Media track identifier to unregister
    async fn unregister_media_track(&self, track_id: &str) -> ActorResult<()>;

    /// Send media samples via WebRTC native track
    ///
    /// Sends raw media samples through WebRTC RTCRtpSender (native RTP).
    /// This is much more efficient than sending through DataChannel.
    ///
    /// # Parameters
    ///
    /// - `target`: Target destination
    /// - `track_id`: Track identifier
    /// - `sample`: Media sample to send
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use actr_framework::{MediaSample, MediaType};
    ///
    /// let sample = MediaSample {
    ///     data: encoded_frame.into(),
    ///     timestamp: rtp_timestamp,
    ///     codec: "H264".to_string(),
    ///     media_type: MediaType::Video,
    /// };
    ///
    /// ctx.send_media_sample(&target, "video-track-1", sample).await?;
    /// ```
    async fn send_media_sample(
        &self,
        target: &crate::Dest,
        track_id: &str,
        sample: MediaSample,
    ) -> ActorResult<()>;

    /// Add a media track to the WebRTC connection with the target
    ///
    /// Creates a new RTP track on the PeerConnection and triggers SDP renegotiation.
    /// Must be called before `send_media_sample()` for the given track.
    ///
    /// # Parameters
    ///
    /// - `target`: Target destination
    /// - `track_id`: Media track identifier
    /// - `codec`: Codec name (e.g., "VP8", "H264", "OPUS")
    /// - `media_type`: Media type ("video" or "audio")
    async fn add_media_track(
        &self,
        target: &crate::Dest,
        track_id: &str,
        codec: &str,
        media_type: &str,
    ) -> ActorResult<()>;

    /// Remove a media track from the WebRTC connection with the target.
    ///
    /// If the track exists, this removes the RTP sender from the PeerConnection
    /// and triggers SDP renegotiation so repeated start/stop cycles do not keep
    /// stale tracks alive on the connection.
    async fn remove_media_track(&self, target: &crate::Dest, track_id: &str) -> ActorResult<()>;

    // ========== Observation ==========

    /// Emit a log record from the workload side, routed through whichever
    /// observability pipeline the runtime installs.
    ///
    /// The default implementation forwards to `tracing` using the configured
    /// `target = "actr_framework::workload"`. Runtimes that embed workloads
    /// in environments without `tracing` (e.g. `wasm32-unknown-unknown`
    /// running in a browser Service Worker) override this to surface records
    /// through whatever host hook is available (`console.log`, host import, ...).
    fn log(&self, level: LogLevel, msg: &str) {
        match level {
            LogLevel::Trace => tracing::trace!(target: "actr_framework::workload", "{msg}"),
            LogLevel::Debug => tracing::debug!(target: "actr_framework::workload", "{msg}"),
            LogLevel::Info => tracing::info!(target: "actr_framework::workload", "{msg}"),
            LogLevel::Warn => tracing::warn!(target: "actr_framework::workload", "{msg}"),
            LogLevel::Error => tracing::error!(target: "actr_framework::workload", "{msg}"),
        }
    }
}

/// Severity level for [`Context::log`].
///
/// Mirrors the five standard levels exposed by `tracing` / `log` so runtimes
/// can map to whatever sink is available on the target (native `tracing`
/// subscriber, browser `console.*`, host-import log channel, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Very fine-grained diagnostic information, typically disabled in
    /// production.
    Trace,
    /// Fine-grained information useful while debugging.
    Debug,
    /// Informational messages that mark normal operation.
    Info,
    /// Conditions that are unexpected but do not prevent continued
    /// operation.
    Warn,
    /// Failures that likely require operator attention.
    Error,
}

/// Media sample data from WebRTC native track
///
/// Lightweight wrapper around WebRTC native RTP sample.
#[derive(Clone)]
pub struct MediaSample {
    /// Raw sample data (encoded audio/video frame)
    pub data: bytes::Bytes,

    /// Sample timestamp (from RTP timestamp)
    pub timestamp: u32,

    /// Codec-specific information
    pub codec: String,

    /// Media type (audio or video)
    pub media_type: MediaType,
}

/// Media type enum
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Audio,
    Video,
}

#[cfg(test)]
mod tests {
    use super::{Context, DEFAULT_RPC_TIMEOUT_MS};
    use crate::{Dest, MediaSample};
    use actr_protocol::{
        ActorResult, ActrError, ActrId, ActrType, DataChunk, PayloadType, RpcRequest,
    };
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::future::BoxFuture;
    use std::sync::{Arc, Mutex};

    /// Captures the timeout values that reach the explicit-timeout lanes so
    /// the trait's default-lane forwarding can be asserted.
    #[derive(Clone)]
    struct TimeoutRecordingContext {
        self_id: ActrId,
        recorded: Arc<Mutex<Vec<i64>>>,
    }

    impl TimeoutRecordingContext {
        fn new() -> (Self, Arc<Mutex<Vec<i64>>>) {
            let recorded = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    self_id: ActrId::default(),
                    recorded: recorded.clone(),
                },
                recorded,
            )
        }
    }

    #[async_trait]
    impl Context for TimeoutRecordingContext {
        fn self_id(&self) -> &ActrId {
            &self.self_id
        }

        fn caller_id(&self) -> Option<&ActrId> {
            None
        }

        fn request_id(&self) -> &str {
            "recording-ctx"
        }

        async fn call_with_timeout<R: RpcRequest>(
            &self,
            _target: &Dest,
            _request: R,
            timeout_ms: i64,
        ) -> ActorResult<R::Response> {
            self.recorded.lock().unwrap().push(timeout_ms);
            Err(ActrError::NotImplemented("typed test stub".into()))
        }

        async fn tell<R: RpcRequest>(&self, _target: &Dest, _message: R) -> ActorResult<()> {
            Ok(())
        }

        async fn register_stream<F>(&self, _stream_id: String, _callback: F) -> ActorResult<()>
        where
            F: Fn(DataChunk, ActrId) -> crate::MaybeSendBoxFuture<'static, ActorResult<()>>
                + crate::MaybeSendSync
                + 'static,
        {
            Ok(())
        }

        async fn unregister_stream(&self, _stream_id: &str) -> ActorResult<()> {
            Ok(())
        }

        async fn send_data_chunk(
            &self,
            _target: &Dest,
            _chunk: DataChunk,
            _payload_type: PayloadType,
        ) -> ActorResult<()> {
            Ok(())
        }

        async fn discover_route_candidate(&self, _target_type: &ActrType) -> ActorResult<ActrId> {
            Err(ActrError::NotImplemented("discover test stub".into()))
        }

        async fn call_raw_with_timeout(
            &self,
            _target: &ActrId,
            _route_key: &str,
            _payload: Bytes,
            timeout_ms: i64,
        ) -> ActorResult<Bytes> {
            self.recorded.lock().unwrap().push(timeout_ms);
            Ok(Bytes::new())
        }

        async fn register_media_track<F>(&self, _track_id: String, _callback: F) -> ActorResult<()>
        where
            F: Fn(MediaSample, ActrId) -> BoxFuture<'static, ActorResult<()>>
                + Send
                + Sync
                + 'static,
        {
            Ok(())
        }

        async fn unregister_media_track(&self, _track_id: &str) -> ActorResult<()> {
            Ok(())
        }

        async fn send_media_sample(
            &self,
            _target: &Dest,
            _track_id: &str,
            _sample: MediaSample,
        ) -> ActorResult<()> {
            Ok(())
        }

        async fn add_media_track(
            &self,
            _target: &Dest,
            _track_id: &str,
            _codec: &str,
            _media_type: &str,
        ) -> ActorResult<()> {
            Ok(())
        }

        async fn remove_media_track(&self, _target: &Dest, _track_id: &str) -> ActorResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn call_raw_default_lane_applies_default_timeout() {
        let (ctx, recorded) = TimeoutRecordingContext::new();

        let _ = ctx
            .call_raw(&ActrId::default(), "echo.EchoService/Echo", Bytes::new())
            .await;

        assert_eq!(
            *recorded.lock().unwrap(),
            vec![DEFAULT_RPC_TIMEOUT_MS],
            "default raw lane must forward the shared default timeout",
        );
    }

    #[tokio::test]
    async fn call_raw_with_payload_type_forwards_reliable_lane() {
        let (ctx, recorded) = TimeoutRecordingContext::new();

        let _ = ctx
            .call_raw_with_payload_type(
                &ActrId::default(),
                "echo.EchoService/Echo",
                PayloadType::RpcReliable,
                Bytes::new(),
                5_000,
            )
            .await;

        assert_eq!(
            *recorded.lock().unwrap(),
            vec![5_000],
            "reliable lane must delegate to call_raw_with_timeout unchanged",
        );
    }

    #[tokio::test]
    async fn call_raw_with_payload_type_rejects_non_reliable_lane() {
        let (ctx, recorded) = TimeoutRecordingContext::new();

        let result = ctx
            .call_raw_with_payload_type(
                &ActrId::default(),
                "echo.EchoService/Echo",
                PayloadType::RpcSignal,
                Bytes::new(),
                5_000,
            )
            .await;

        match result {
            Err(ActrError::InvalidArgument(_)) => {}
            other => panic!("expected InvalidArgument for RpcSignal lane, got {other:?}"),
        }
        assert!(
            recorded.lock().unwrap().is_empty(),
            "rejected lane must not reach the transport",
        );
    }
}
