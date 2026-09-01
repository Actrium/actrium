//! Context for actor-internal execution.
//!
//! Mirrors the `actr` `Context` trait and provides communication primitives
//! used inside actors.

use std::rc::Rc;

use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    ActorResult, ActrError, ActrId, ActrType, Direction, MetadataEntry, PayloadType, RpcEnvelope,
    RpcRequest,
};
use bytes::Bytes;
use futures::future::{Either, select};

use crate::WebContext;
use crate::outbound::Gate;
use crate::web_context::RuntimeBridge;

/// Runtime context for actors in the Web runtime.
///
/// Mirrors `actr`'s `RuntimeContext`.
pub struct RuntimeContext {
    /// Current actor ID.
    self_id: ActrId,

    /// Caller actor ID.
    caller_id: Option<ActrId>,

    /// Trace identifiers.
    traceparent: String,
    tracestate: String,

    /// Request ID.
    request_id: String,

    /// Outbound gate.
    gate: Gate,

    /// Runtime bridge for operations that require lower-level runtime support,
    /// such as `call_raw` and `discover`.
    bridge: Option<Rc<dyn RuntimeBridge>>,
}

impl RuntimeContext {
    /// Create a new context.
    pub fn new(
        self_id: ActrId,
        caller_id: Option<ActrId>,
        traceparent: String,
        tracestate: String,
        request_id: String,
        gate: Gate,
    ) -> Self {
        Self {
            self_id,
            caller_id,
            traceparent,
            tracestate,
            request_id,
            gate,
            bridge: None,
        }
    }

    /// Attach a `RuntimeBridge` for handler execution.
    pub fn with_bridge(mut self, bridge: Rc<dyn RuntimeBridge>) -> Self {
        self.bridge = Some(bridge);
        self
    }

    async fn call_raw_with_type(
        &self,
        target: &ActrId,
        route_key: &str,
        payload_type: PayloadType,
        payload: &[u8],
        timeout_ms: i64,
    ) -> ActorResult<Vec<u8>> {
        crate::outbound::validate_rpc_payload_type(payload_type)?;
        let timeout_ms = crate::outbound::validate_rpc_timeout_ms(timeout_ms)?;
        let started_at = web_time::Instant::now();
        let request_id = js_sys::Math::random().to_string();

        if let Some(bridge) = &self.bridge {
            bridge.register_pending_rpc(request_id.clone());
            let connection = bridge.ensure_connection(target);
            let deadline = gloo_timers::future::TimeoutFuture::new(timeout_ms);
            futures::pin_mut!(connection, deadline);
            match select(connection, deadline).await {
                Either::Left((result, _)) => {
                    if let Err(error) = result {
                        log::warn!(
                            "[Context] ensure_connection skipped for {}: {}",
                            target,
                            error
                        );
                    }
                }
                Either::Right(((), _)) => {
                    bridge.unregister_pending_rpc(&request_id);
                    return Err(ActrError::TimedOut);
                }
            }
        }

        let Some(remaining_timeout_ms) = remaining_timeout_ms(timeout_ms, started_at.elapsed())
        else {
            if let Some(bridge) = &self.bridge {
                bridge.unregister_pending_rpc(&request_id);
            }
            return Err(ActrError::TimedOut);
        };

        let pending_request_id = request_id.clone();
        let envelope = RpcEnvelope {
            route_key: route_key.to_string(),
            payload: Some(Bytes::from(payload.to_vec())),
            error: None,
            direction: Some(Direction::Request as i32),
            traceparent: Some(self.traceparent.clone()),
            tracestate: Some(self.tracestate.clone()),
            request_id,
            metadata: vec![MetadataEntry {
                key: "sender_actr_id".to_string(),
                value: self.self_id.to_string_repr(),
            }],
            timeout_ms: remaining_timeout_ms,
        };

        let response = self
            .gate
            .send_request_with_type(target, payload_type, envelope)
            .await;
        if response.is_err()
            && let Some(bridge) = &self.bridge
        {
            bridge.unregister_pending_rpc(&pending_request_id);
        }
        Ok(response?.to_vec())
    }

    async fn tell_raw_with_type(
        &self,
        target: &ActrId,
        route_key: &str,
        payload_type: PayloadType,
        payload: Vec<u8>,
    ) -> ActorResult<()> {
        crate::outbound::validate_rpc_payload_type(payload_type)?;
        if let Some(bridge) = &self.bridge
            && let Err(error) = bridge.ensure_connection(target).await
        {
            log::warn!(
                "[Context] ensure_connection skipped for {}: {}",
                target,
                error
            );
        }

        let envelope = RpcEnvelope {
            route_key: route_key.to_string(),
            payload: Some(Bytes::from(payload)),
            error: None,
            direction: Some(Direction::Tell as i32),
            traceparent: Some(self.traceparent.clone()),
            tracestate: Some(self.tracestate.clone()),
            request_id: js_sys::Math::random().to_string(),
            metadata: vec![MetadataEntry {
                key: "sender_actr_id".to_string(),
                value: self.self_id.to_string_repr(),
            }],
            timeout_ms: 0,
        };

        self.gate
            .send_message_with_type(target, payload_type, envelope)
            .await
    }
}

#[async_trait::async_trait(?Send)]
impl WebContext for RuntimeContext {
    // ========== Basic Info ==========

    fn self_id(&self) -> &ActrId {
        &self.self_id
    }

    fn caller_id(&self) -> Option<&ActrId> {
        self.caller_id.as_ref()
    }

    fn trace_id(&self) -> &str {
        &self.traceparent
    }

    fn request_id(&self) -> &str {
        &self.request_id
    }

    // ========== RPC Communication ==========

    async fn call_raw(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: &[u8],
        timeout_ms: i64,
    ) -> ActorResult<Vec<u8>> {
        self.call_raw_with_type(
            target,
            route_key,
            PayloadType::RpcReliable,
            payload,
            timeout_ms,
        )
        .await
    }

    async fn discover(&self, target_type: &ActrType) -> ActorResult<ActrId> {
        match &self.bridge {
            Some(bridge) => bridge.discover_target(target_type).await,
            None => Err(actr_protocol::ActrError::Unavailable(
                "RuntimeBridge not available for discover".to_string(),
            )),
        }
    }

    // ========== Type-Safe Messaging ==========

    async fn call<R: RpcRequest>(
        &self,
        target: &ActrId,
        request: R,
        timeout_ms: i64,
    ) -> ActorResult<R::Response> {
        let payload = request.encode_to_vec();
        let response_bytes = self
            .call_raw_with_type(
                target,
                R::route_key(),
                R::payload_type(),
                &payload,
                timeout_ms,
            )
            .await?;

        R::Response::decode(response_bytes.as_slice()).map_err(|e| {
            actr_protocol::ActrError::DecodeFailure(format!(
                "Failed to decode {}: {}",
                std::any::type_name::<R::Response>(),
                e
            ))
        })
    }

    async fn tell<R: RpcRequest>(&self, target: &ActrId, message: R) -> ActorResult<()> {
        // 1. Encode the message and delegate to the shared raw tell path.
        //    Passing the Vec by value lets tell_raw move it into Bytes with
        //    no extra copy (a &[u8] parameter would force a clone).
        let payload = message.encode_to_vec();
        self.tell_raw_with_type(target, R::route_key(), R::payload_type(), payload)
            .await
    }

    async fn tell_raw(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: Vec<u8>,
    ) -> ActorResult<()> {
        self.tell_raw_with_type(target, route_key, PayloadType::RpcSignal, payload)
            .await
    }

    // ========== Stream Registration ==========
    //
    // Stream registration on the SW side mainly forwards registration requests
    // to the DOM side. The actual callback runs on the DOM fast path.

    async fn register_stream(
        &self,
        stream_id: String,
        callback: Box<dyn FnMut(Bytes) + 'static>,
    ) -> ActorResult<()> {
        log::info!("[Context] register_stream: {}", stream_id);

        match &self.bridge {
            Some(bridge) => bridge.register_stream_handler(stream_id, callback),
            None => Err(actr_protocol::ActrError::Unavailable(
                "RuntimeBridge not available for register_stream".to_string(),
            )),
        }
    }

    async fn unregister_stream(&self, stream_id: &str) -> ActorResult<()> {
        log::info!("[Context] unregister_stream: {}", stream_id);

        match &self.bridge {
            Some(bridge) => bridge.unregister_stream_handler(stream_id),
            None => Err(actr_protocol::ActrError::Unavailable(
                "RuntimeBridge not available for unregister_stream".to_string(),
            )),
        }
    }

    async fn register_media_track(
        &self,
        _track_id: String,
        _callback: Box<dyn FnMut(Bytes) + 'static>,
    ) -> ActorResult<()> {
        // Media-track fast path is intentionally not wired on the web target
        // (see crates/actr-framework/src/web/context.rs §"DataChunk / MediaTrack
        // fast paths"). Returning Unavailable is consistent with the
        // framework `WebContext` shape so callers fail loud rather than
        // believe a registration that did nothing.
        Err(actr_protocol::ActrError::Unavailable(
            "register_media_track is not supported in the web runtime".to_string(),
        ))
    }

    async fn unregister_media_track(&self, _track_id: &str) -> ActorResult<()> {
        Err(actr_protocol::ActrError::Unavailable(
            "unregister_media_track is not supported in the web runtime".to_string(),
        ))
    }

    // ========== Stream Sending ==========

    async fn send_media_sample(
        &self,
        target: &ActrId,
        track_id: &str,
        data: Bytes,
    ) -> ActorResult<()> {
        log::debug!(
            "[Context] send_media_sample: target={:?}, track_id={}, size={}",
            target,
            track_id,
            data.len()
        );

        // Prefix the payload with `track_id`.
        // Format: [track_id_len(4) | track_id(N) | data(M)]
        let track_id_bytes = track_id.as_bytes();
        let mut payload = Vec::with_capacity(4 + track_id_bytes.len() + data.len());
        payload.extend_from_slice(&(track_id_bytes.len() as u32).to_be_bytes());
        payload.extend_from_slice(track_id_bytes);
        payload.extend_from_slice(&data);

        // Send fast-path data through the gate.
        self.gate
            .send_data_chunk(
                target,
                actr_protocol::PayloadType::MediaRtp,
                Bytes::from(payload),
            )
            .await
    }

    async fn send_data_chunk(
        &self,
        target: &ActrId,
        stream_id: &str,
        data: Bytes,
    ) -> ActorResult<()> {
        log::debug!(
            "[Context] send_data_chunk: target={:?}, stream_id={}, size={}",
            target,
            stream_id,
            data.len()
        );

        // Prefix the payload with `stream_id`.
        // Format: [stream_id_len(4) | stream_id(N) | data(M)]
        let stream_id_bytes = stream_id.as_bytes();
        let mut payload = Vec::with_capacity(4 + stream_id_bytes.len() + data.len());
        payload.extend_from_slice(&(stream_id_bytes.len() as u32).to_be_bytes());
        payload.extend_from_slice(stream_id_bytes);
        payload.extend_from_slice(&data);

        // Send fast-path data through the gate (defaulting to STREAM_RELIABLE).
        self.gate
            .send_data_chunk(
                target,
                actr_protocol::PayloadType::StreamReliable,
                Bytes::from(payload),
            )
            .await
    }
}

fn remaining_timeout_ms(timeout_ms: u32, elapsed: std::time::Duration) -> Option<i64> {
    let remaining = std::time::Duration::from_millis(u64::from(timeout_ms))
        .checked_sub(elapsed)?
        .as_millis();
    (remaining > 0).then(|| i64::try_from(remaining).expect("timeout is bounded by i32::MAX"))
}

#[cfg(test)]
mod timeout_tests {
    use super::remaining_timeout_ms;
    use std::time::Duration;

    #[test]
    fn connection_time_is_deducted_from_call_timeout() {
        assert_eq!(
            remaining_timeout_ms(1_000, Duration::from_millis(250)),
            Some(750)
        );
        assert_eq!(
            remaining_timeout_ms(1_000, Duration::from_millis(1_000)),
            None
        );
        assert_eq!(
            remaining_timeout_ms(1_000, Duration::from_millis(1_001)),
            None
        );
    }
}
