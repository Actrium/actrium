//! WASM actor end-to-end test fixture
//!
//! Verification target: a real wasm32 actor that implements the `Context`
//! trait via `actr-framework` guest module, plus a panic route used by
//! Phase 1 Commit 6's regression suite to validate guest-trap behaviour
//! after a host suspension point.
//!
//! # Test protocol
//!
//! - route `test/echo`: returns the inbound payload as-is without
//!   outbound IO.
//! - route `test/spin`: runs non-yielding guest compute forever. The host must
//!   stop it through fuel or epoch interruption and rebuild the poisoned Store.
//! - route `test/double`: decodes the payload as a 4-byte little-endian
//!   i32 value `x`, calls
//!   `ctx.call_raw(self_id, "test/double_impl", payload, 30_000)`,
//!   and returns whatever the host responds with. Used by the async
//!   round-trip / tick-probe / dispatch tests.
//! - route `test/typed-double`: calls `ctx.call()` with an explicit 1234 ms
//!   timeout and returns the decoded typed response.
//! - route `test/boom-after-await`: same control flow as `test/double` up
//!   to the `ctx.call_raw` await, then panics. Used to verify that a
//!   guest panic after a real suspension surfaces as a wasmtime trap
//!   rather than corrupting the dispatch result.
//!
//! All other routes surface `ActrError::UnknownRoute(route_key)`,
//! exercising guest→host structured error propagation.

use actr_framework::{Context, Dest, MessageDispatcher, Workload, entry};
use actr_protocol::{ActorResult, ActrError, ReplyMode, RpcEnvelope, RpcRequest};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::UNIX_EPOCH;

thread_local! {
    /// Parked `test/captured-context-stream` subscription. Consumed by the
    /// later `test/drain-parked-stream` invocation (the resident
    /// run_concurrent region retires spawned subtasks, so consumption is
    /// driven by explicit dispatches rather than a detached task).
    static PARKED_STREAM: std::cell::RefCell<Option<actr_framework::DataChunkStream>> =
        const { std::cell::RefCell::new(None) };

    /// Probe counters for `test/eof-consumer`: (consumer starts, chunks
    /// seen, stream terminations observed by the consumer loop).
    static EOF_PROBE: std::cell::Cell<(u64, u64, u64)> =
        const { std::cell::Cell::new((0, 0, 0)) };
}

#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
struct TypedDoubleRequest {
    #[prost(int32, tag = "1")]
    value: i32,
}

#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
struct TypedDoubleResponse {
    #[prost(int32, tag = "1")]
    value: i32,
}

impl RpcRequest for TypedDoubleRequest {
    type Response = TypedDoubleResponse;

    fn route_key() -> &'static str {
        "test/typed_double_impl"
    }
}

async fn record_hook<C: Context>(ctx: &C, name: &'static str) {
    record_hook_value(ctx, name.to_string()).await;
}

async fn record_hook_value<C: Context>(ctx: &C, value: String) {
    let _ = ctx
        .call_raw(
            ctx.self_id(),
            "test/record_hook",
            Bytes::from(value),
            30_000,
        )
        .await;
}

fn relayed_label(event: &actr_framework::PeerEvent) -> &'static str {
    match event.relayed {
        Some(true) => "true",
        Some(false) => "false",
        None => "none",
    }
}

fn status_label(event: &actr_framework::PeerEvent) -> &'static str {
    match event.status {
        Some(actr_framework::WebRtcPeerStatus::Idle) => "idle",
        Some(actr_framework::WebRtcPeerStatus::Connecting) => "connecting",
        Some(actr_framework::WebRtcPeerStatus::Connected) => "connected",
        Some(actr_framework::WebRtcPeerStatus::Recovering) => "recovering",
        None => "none",
    }
}

fn state_status_label(status: actr_framework::WebRtcPeerStatus) -> &'static str {
    match status {
        actr_framework::WebRtcPeerStatus::Idle => "idle",
        actr_framework::WebRtcPeerStatus::Connecting => "connecting",
        actr_framework::WebRtcPeerStatus::Connected => "connected",
        actr_framework::WebRtcPeerStatus::Recovering => "recovering",
    }
}

fn network_path_label(path: actr_framework::WebRtcNetworkPath) -> &'static str {
    match path {
        actr_framework::WebRtcNetworkPath::Unknown => "unknown",
        actr_framework::WebRtcNetworkPath::Direct => "direct",
        actr_framework::WebRtcNetworkPath::Relayed => "relayed",
    }
}

fn reason_label(reason: Option<actr_framework::WebRtcUnavailableReason>) -> &'static str {
    use actr_framework::WebRtcUnavailableReason as R;
    match reason {
        Some(R::ConnectionNotReady) => "connection-not-ready",
        Some(R::PeerConnectionBuildFailed) => "peer-connection-build-failed",
        Some(R::NegotiationFailed) => "negotiation-failed",
        Some(R::IceRestartFailed) => "ice-restart-failed",
        Some(R::PeerConnectionRebuildFailed) => "peer-connection-rebuild-failed",
        Some(R::UnsupportedRtcCapability) => "unsupported-rtc-capability",
        Some(R::RtcResourceLimitExceeded) => "rtc-resource-limit-exceeded",
        Some(R::RecoveryExhausted) => "recovery-exhausted",
        Some(R::Closed) => "closed",
        // Non_exhaustive enum: an unknown future reason still needs a label.
        Some(_) => "unsupported-rtc-capability",
        None => "none",
    }
}

fn opt_bool_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "none",
    }
}

async fn record_peer_hook<C: Context>(
    ctx: &C,
    name: &'static str,
    event: &actr_framework::PeerEvent,
) {
    record_hook_value(
        ctx,
        format!(
            "{name}:peer={}:relayed={}:status={}",
            event.peer.serial_number,
            relayed_label(event),
            status_label(event),
        ),
    )
    .await;
}

// ── Workload ──────────────────────────────────────────────────────────────────

/// Per-instance in-flight counters used by the `test/inflight-probe` route to
/// prove same-instance interleaving (M5) and the WASM/native isomorphism (M6).
///
/// These live on the workload instance, NOT in module statics, so the peak
/// concurrency they report is scoped to exactly one guest instance. That makes
/// the probe **basis-agnostic**:
///
/// * WASM V2 — the `entry!` macro keeps one `DoubleActor` per linear memory, so
///   a fresh instance (rebuild after a trap) starts both counters at zero and a
///   post-trap probe reporting MAX_SEEN==1 is direct evidence of a rebuilt
///   linear memory.
/// * native Linked — one `DoubleActor` is shared (behind `&self`) across the
///   concurrent dispatches of a single dispatcher, and a *different* dispatcher
///   (a different test) owns a *different* instance. Were these module statics
///   instead, the process-global `fetch_max` peak would leak across tests and
///   break the same-key/serial (MAX_SEEN==1) properties. Instance fields give
///   every dispatcher its own isolated, parallel-safe counters.
///
/// wasm is single-threaded so these atomics are contention-free there; on native
/// they are genuinely concurrent but the SeqCst `fetch_max` is exactly the peak
/// tracker the property tests read.
pub struct DoubleActor {
    in_flight: AtomicU32,
    max_seen: AtomicU32,
}

impl Default for DoubleActor {
    fn default() -> Self {
        Self {
            in_flight: AtomicU32::new(0),
            max_seen: AtomicU32::new(0),
        }
    }
}

pub struct DoubleDispatcher;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl MessageDispatcher for DoubleDispatcher {
    type Workload = DoubleActor;

    fn reply_mode_for_route(route_key: &str) -> ReplyMode {
        if matches!(route_key, "test/explicit" | "test/forget-explicit") {
            ReplyMode::Explicit
        } else {
            ReplyMode::Return
        }
    }

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "test/echo" => Ok(Bytes::from(envelope.payload.unwrap_or_default().to_vec())),
            "test/explicit" => {
                let payload = envelope.payload.unwrap_or_default();
                if payload.len() < 4 {
                    return Err(ActrError::InvalidArgument("payload too short".into()));
                }
                let value = i32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let deliver = ctx.reply_deliver().ok_or_else(|| {
                    ActrError::Internal("explicit fixture route requires a reply deliver".into())
                })?;
                let reply = actr_framework::Reply::new(
                    ctx.request_id().to_string(),
                    ctx.reply_deadline(),
                    deliver,
                    ctx.expects_response(),
                );
                reply.send(Ok(TypedDoubleResponse { value: value * 2 }));
                Ok(Bytes::new())
            }
            "test/forget-explicit" => Ok(Bytes::new()),
            // A RETURN route may legitimately encode to zero bytes. This must
            // not be mistaken for the EXPLICIT deferred sentinel.
            "test/zero-byte-return" => Ok(Bytes::new()),
            "test/spin" => loop {
                std::hint::spin_loop();
            },
            "test/record_hook" => Ok(Bytes::from(envelope.payload.unwrap_or_default().to_vec())),
            "test/double" => {
                // payload: 4-byte little-endian i32 (RpcEnvelope.payload is optional)
                let payload = envelope.payload.unwrap_or_default();
                if payload.len() < 4 {
                    return Err(ActrError::InvalidArgument("payload too short".into()));
                }
                let x = i32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);

                // Call ctx.call_raw() -> triggers the host-import await path.
                let target = ctx.self_id().clone();
                let resp = ctx
                    .call_raw(
                        &target,
                        "test/double_impl",
                        Bytes::from(x.to_le_bytes().to_vec()),
                        30_000,
                    )
                    .await?;

                Ok(resp)
            }
            "test/typed-double" => {
                let payload = envelope.payload.unwrap_or_default();
                if payload.len() < 4 {
                    return Err(ActrError::InvalidArgument("payload too short".into()));
                }
                let value = i32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let response = ctx
                    .call(
                        &Dest::Peer(ctx.self_id().clone()),
                        TypedDoubleRequest { value },
                        1_234,
                    )
                    .await?;
                Ok(Bytes::from(response.value.to_le_bytes().to_vec()))
            }
            "test/inflight-probe" => {
                // M5 same-instance interleave probe. Bump the shared in-flight
                // counter on entry (recording the peak), await a host import the
                // test gates open, then decrement on the way out and return the
                // peak concurrency this instance ever observed as a 4-byte LE u32.
                // Two distinct-conflict-key probes suspended here at once make
                // MAX_SEEN >= 2 — proof they interleaved inside one instance.
                let cur = workload.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                workload.max_seen.fetch_max(cur, Ordering::SeqCst);
                let target = ctx.self_id().clone();
                let payload = envelope.payload.unwrap_or_default();
                let gate = ctx.call_raw(&target, "test/gate", payload, 30_000).await;
                workload.in_flight.fetch_sub(1, Ordering::SeqCst);
                gate?;
                let peak = workload.max_seen.load(Ordering::SeqCst);
                Ok(Bytes::from(peak.to_le_bytes().to_vec()))
            }
            "test/register-stream-context" => {
                // The stream consumer executes after this dispatch has
                // returned. Capturing Context is the only way the consumer
                // can issue host operations later, so this locks in the V2
                // token rebinding contract.
                let callback_ctx = ctx.clone();
                let mut incoming = ctx
                    .data_chunks(
                        "test/captured-context-stream".to_string(),
                        Default::default(),
                    )
                    .await?;
                let consumer = async move {
                    use futures_util::StreamExt;
                    while let Some(event) = incoming.next().await {
                        if event.is_err() {
                            break;
                        }
                        let _ = callback_ctx
                            .call_raw(
                                callback_ctx.self_id(),
                                "test/stream-callback",
                                Bytes::from_static(b"stream-callback"),
                                30_000,
                            )
                            .await;
                    }
                };
                #[cfg(target_arch = "wasm32")]
                actr_framework::guest::wasm::spawn_local(consumer);
                #[cfg(not(target_arch = "wasm32"))]
                tokio::spawn(consumer);
                Ok(Bytes::new())
            }
            "test/eof-consumer" => {
                // Stream-end probe: consume until the stream terminates,
                // counting chunks and the termination. The host feeds this
                // subscription via the production stream forwarder; a
                // graceful end-of-stream marker must terminate the loop
                // (FIFO, after every data chunk) — without marker forwarding
                // the consumer hangs forever and the count never reaches 1.
                let mut incoming = ctx
                    .data_chunks("test/eof-stream".to_string(), Default::default())
                    .await?;
                let consumer = async move {
                    use futures_util::StreamExt;
                    EOF_PROBE.with(|probe| {
                        let (starts, chunks, ends) = probe.get();
                        probe.set((starts + 1, chunks, ends));
                    });
                    while let Some(event) = incoming.next().await {
                        if event.is_err() {
                            break;
                        }
                        EOF_PROBE.with(|probe| {
                            let (starts, chunks, ends) = probe.get();
                            probe.set((starts, chunks + 1, ends));
                        });
                    }
                    EOF_PROBE.with(|probe| {
                        let (starts, chunks, ends) = probe.get();
                        probe.set((starts, chunks, ends + 1));
                    });
                };
                #[cfg(target_arch = "wasm32")]
                actr_framework::guest::wasm::spawn_local(consumer);
                #[cfg(not(target_arch = "wasm32"))]
                tokio::spawn(consumer);
                Ok(Bytes::new())
            }
            "test/eof-stats" => {
                let (starts, chunks, ends) = EOF_PROBE.with(|probe| probe.get());
                let mut out = starts.to_le_bytes().to_vec();
                out.extend_from_slice(&chunks.to_le_bytes());
                out.extend_from_slice(&ends.to_le_bytes());
                Ok(Bytes::from(out))
            }
            "test/park-stream-context" => {
                // Resident-region variant: on wasm32-wasip2 guest code only
                // runs while some invocation is active, and the resident
                // run_concurrent region retires subtasks when their command
                // completes — so a spawned consumer cannot outlive its
                // dispatch there. Park the subscription instead and consume
                // it in a later invocation ("test/drain-parked-stream").
                let incoming = ctx
                    .data_chunks(
                        "test/captured-context-stream".to_string(),
                        Default::default(),
                    )
                    .await?;
                PARKED_STREAM.with(|slot| {
                    *slot.borrow_mut() = Some(incoming);
                });
                Ok(Bytes::new())
            }
            "test/drain-parked-stream" => {
                let taken = PARKED_STREAM.with(|slot| slot.borrow_mut().take());
                let Some(mut incoming) = taken else {
                    return Err(ActrError::Internal(
                        "no parked stream; call test/park-stream-context first".into(),
                    ));
                };
                use futures_util::StreamExt;
                let mut drained = 0u64;
                // The test dispatches exactly one chunk; take it, then close
                // (awaiting the next item would pend on the open channel).
                if let Some(event) = incoming.next().await {
                    if event.is_ok() {
                        drained += 1;
                        // Use THIS invocation's live context: the captured
                        // registration token is only aliased during the
                        // on-data-chunk invocation (begin_stream_callback),
                        // not here.
                        let _ = ctx
                            .call_raw(
                                ctx.self_id(),
                                "test/stream-callback",
                                Bytes::from_static(b"stream-callback"),
                                30_000,
                            )
                            .await;
                    }
                }
                incoming.close().await;
                Ok(Bytes::from(drained.to_le_bytes().to_vec()))
            }
            "test/boom-after-await" => {
                // Exercise Phase 0.5 spike Test 8: perform a host-side
                // await, then panic once control returns to the guest.
                // The test host routes "test/double_impl" back to a stub
                // reply; the actual content is discarded.
                let target = ctx.self_id().clone();
                let payload = envelope.payload.unwrap_or_default();
                let _ = ctx
                    .call_raw(&target, "test/double_impl", payload, 30_000)
                    .await?;
                panic!("fixture: intentional post-await panic for trap test");
            }
            _ => Err(ActrError::UnknownRoute(envelope.route_key)),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl Workload for DoubleActor {
    type Dispatcher = DoubleDispatcher;

    async fn on_start<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        if ctx.request_id() == "lifecycle:on_start" {
            return Err(ActrError::Internal(
                "fixture lifecycle on_start failed".to_string(),
            ));
        }
        Ok(())
    }

    async fn on_ready<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        record_hook(ctx, "on_ready").await;
        Ok(())
    }

    async fn on_stop<C: Context>(&self, ctx: &C) -> ActorResult<()> {
        record_hook(ctx, "on_stop").await;
        Ok(())
    }

    async fn on_signaling_connecting<C: Context>(&self, ctx: Option<&C>) {
        if let Some(ctx) = ctx {
            record_hook(ctx, "on_signaling_connecting").await;
        }
    }

    async fn on_signaling_connected<C: Context>(&self, ctx: Option<&C>) {
        if let Some(ctx) = ctx {
            record_hook(ctx, "on_signaling_connected").await;
        }
    }

    async fn on_signaling_disconnected<C: Context>(&self, ctx: &C) {
        record_hook(ctx, "on_signaling_disconnected").await;
    }

    async fn on_websocket_connecting<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::PeerEvent,
    ) {
        record_peer_hook(ctx, "on_websocket_connecting", event).await;
    }

    async fn on_websocket_connected<C: Context>(&self, ctx: &C, event: &actr_framework::PeerEvent) {
        record_peer_hook(ctx, "on_websocket_connected", event).await;
    }

    async fn on_websocket_disconnected<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::PeerEvent,
    ) {
        record_peer_hook(ctx, "on_websocket_disconnected", event).await;
    }

    async fn on_webrtc_connecting<C: Context>(&self, ctx: &C, event: &actr_framework::PeerEvent) {
        record_peer_hook(ctx, "on_webrtc_connecting", event).await;
    }

    async fn on_webrtc_connected<C: Context>(&self, ctx: &C, event: &actr_framework::PeerEvent) {
        record_peer_hook(ctx, "on_webrtc_connected", event).await;
    }

    async fn on_webrtc_disconnected<C: Context>(&self, ctx: &C, event: &actr_framework::PeerEvent) {
        record_peer_hook(ctx, "on_webrtc_disconnected", event).await;
    }

    async fn on_webrtc_state_changed<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::WebRtcPeerState,
    ) {
        record_hook_value(
            ctx,
            format!(
                "on_webrtc_state_changed:peer={}:gen={}:rev={}:status={}:sendable={}:path={}:relayed={}:reason={}",
                event.peer.serial_number,
                event.pc_generation,
                event.revision,
                state_status_label(event.status),
                event.sendable,
                network_path_label(event.network_path),
                opt_bool_label(event.relayed),
                reason_label(event.unavailable_reason),
            ),
        )
        .await;
    }

    async fn on_credential_renewed<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::CredentialEvent,
    ) {
        let secs = event
            .new_expiry
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        record_hook_value(ctx, format!("on_credential_renewed:expiry={secs}")).await;
    }

    async fn on_credential_expiring<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::CredentialEvent,
    ) {
        let secs = event
            .new_expiry
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        record_hook_value(ctx, format!("on_credential_expiring:expiry={secs}")).await;
    }

    async fn on_mailbox_backpressure<C: Context>(
        &self,
        ctx: &C,
        event: &actr_framework::BackpressureEvent,
    ) {
        record_hook_value(
            ctx,
            format!(
                "on_mailbox_backpressure:queue_len={}:threshold={}",
                event.queue_len, event.threshold
            ),
        )
        .await;
    }
}

// ── ABI exports (generated by entry! macro) ──────────────────────────────────

entry!(DoubleActor);
