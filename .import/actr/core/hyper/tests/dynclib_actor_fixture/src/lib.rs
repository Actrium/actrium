//! Dynclib actor fixture for e2e tests
//!
//! A simple cdylib actor implementing:
//! - "test/double": reads i32 from payload, calls ctx.call_raw() to get x*2
//! - "test/typed-double": calls ctx.call() with an explicit 1234 ms timeout
//! - "test/echo": returns payload as-is (no outbound calls)
//! - "test/sleep": awaits a Tokio timer before returning
//! - "test/spawn": spawns a Tokio task that calls the host after dispatch returns
//! - "test/spawn-pending": starts a managed task for shutdown verification
//! - "test/send-data-chunk-error": classifies a structured host send error
//! - unknown route: returns error

use actr_framework::{Context, Dest, MessageDispatcher, Workload, entry};
use actr_protocol::{ActorResult, ActrError, DataChunk, PayloadType, RpcEnvelope, RpcRequest};
use async_trait::async_trait;
use bytes::Bytes;
use std::time::UNIX_EPOCH;

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

#[derive(Default)]
pub struct DoubleActor;

pub struct DoubleDispatcher;

#[async_trait]
impl MessageDispatcher for DoubleDispatcher {
    type Workload = DoubleActor;

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "test/double" => {
                let payload = envelope.payload.unwrap_or_default();
                if payload.len() < 4 {
                    return Err(ActrError::InvalidArgument("payload too short".into()));
                }
                let x = i32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);

                // Call ctx.call_raw() -> triggers vtable.call trampoline
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
            "test/echo" => {
                let payload = envelope.payload.unwrap_or_default();
                Ok(Bytes::from(payload))
            }
            "test/sleep" => {
                let payload = envelope.payload.unwrap_or_default();
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                Ok(Bytes::from(payload))
            }
            "test/spawn" => {
                let payload = envelope.payload.unwrap_or_default();
                let background_ctx = ctx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let target = background_ctx.self_id().clone();
                    let _ = background_ctx
                        .call_raw(&target, "test/spawned", Bytes::from(payload), 30_000)
                        .await;
                });
                Ok(Bytes::from_static(b"spawned"))
            }
            "test/spawn-pending" => {
                let background_ctx = ctx.clone();
                actr_framework::guest::dynclib::spawn(async move {
                    let target = background_ctx.self_id().clone();
                    let _ = background_ctx
                        .call_raw(&target, "test/background-started", Bytes::new(), 30_000)
                        .await;
                    std::future::pending::<()>().await;
                })?;
                Ok(Bytes::from_static(b"spawned"))
            }
            "test/send-data-chunk-error" => {
                let result = ctx
                    .send_data_chunk(
                        &Dest::Peer(ctx.self_id().clone()),
                        DataChunk {
                            stream_id: "test/error-stream".into(),
                            sequence: 1,
                            payload: Vec::new().into(),
                            metadata: Vec::new(),
                            timestamp_ms: None,
                        },
                        PayloadType::StreamReliable,
                    )
                    .await;
                match result {
                    Err(ActrError::ConnectionNotReady(info)) => Ok(Bytes::from(
                        info.retry_after_ms
                            .unwrap_or_default()
                            .to_le_bytes()
                            .to_vec(),
                    )),
                    Err(error) => Err(error),
                    Ok(()) => Err(ActrError::Internal(
                        "expected send_data_chunk to fail".into(),
                    )),
                }
            }
            "test/record_hook" => {
                let payload = envelope.payload.unwrap_or_default();
                Ok(Bytes::from(payload))
            }
            _ => Err(ActrError::UnknownRoute(envelope.route_key)),
        }
    }
}

#[async_trait]
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

entry!(DoubleActor);
