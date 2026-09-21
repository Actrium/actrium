//! Dynclib guest-side `Context` implementation backed by the compressed ABI.

use crate::guest::dynclib_abi::{
    self as abi, AbiPayload, AbiReply, GuestDataChunkV1, HostCallRawV1, HostCallV1, HostDiscoverV1,
    HostRegisterStreamV1, HostResolveDependencyV1, HostSendDataChunkV1, HostTellV1,
    HostUnregisterStreamV1, InvocationContextV1, abi_error_to_actr, dest_to_v1,
    reply_to_actr_error,
};
use crate::guest::vtable::HostVTable;
use crate::{Context, Dest, LogLevel, MediaSample};
use actr_protocol::{ActorResult, ActrError, ActrId, ActrType, DataChunk, PayloadType};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// cdylib guest-side actor execution context.
pub struct DynclibContext {
    vtable: *const HostVTable,
    bridge_token: u64,
    retained: bool,
    self_id: ActrId,
    caller_id: Option<ActrId>,
    request_id: String,
}

unsafe impl Send for DynclibContext {}
unsafe impl Sync for DynclibContext {}

fn stream_senders() -> &'static Mutex<
    HashMap<String, futures_channel::mpsc::Sender<ActorResult<crate::DataChunkEvent>>>,
> {
    static SENDERS: OnceLock<
        Mutex<HashMap<String, futures_channel::mpsc::Sender<ActorResult<crate::DataChunkEvent>>>>,
    > = OnceLock::new();
    SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn dispatch_registered_stream(payload: GuestDataChunkV1) -> ActorResult<()> {
    let bridge_token = payload.bridge_token;
    let stream_id = payload.chunk.stream_id.clone();
    let tx = {
        let senders = stream_senders()
            .lock()
            .map_err(|_| ActrError::Internal("stream sender registry poisoned".into()))?;
        senders.get(&stream_id).cloned()
    };

    let Some(mut tx) = tx else {
        return Err(ActrError::NotFound(format!(
            "no stream subscription for '{stream_id}'"
        )));
    };

    let stream_id2 = stream_id.clone();
    let fut = async move {
        use futures_util::SinkExt;
        let event = crate::DataChunkEvent {
            chunk: payload.chunk,
            sender: payload.sender,
        };
        tx.send(Ok(event)).await.map_err(|_| {
            ActrError::NotFound(format!("stream subscription for '{stream_id2}' is gone"))
        })
    };
    let result = crate::guest::dynclib::block_on(bridge_token, fut)?;
    if result.is_err() {
        // The receiver was dropped without close() (best-effort Drop path
        // abandons the async host unregister). Remove the stale entry so the
        // host tears down its side.
        if let Ok(mut senders) = stream_senders().lock() {
            senders.remove(&stream_id);
        }
    }
    result
}

#[doc(hidden)]
pub fn clear_stream_senders() {
    if let Ok(mut senders) = stream_senders().lock() {
        senders.clear();
    }
}

impl DynclibContext {
    /// Construct a context from host-injected invocation data.
    ///
    /// # Safety
    ///
    /// `vtable` must remain valid for the lifetime of the returned context.
    pub unsafe fn from_invocation(
        vtable: *const HostVTable,
        ctx: InvocationContextV1,
        bridge_token: u64,
    ) -> Result<Self, ActrError> {
        if vtable.is_null() {
            return Err(ActrError::Internal("HostVTable pointer is null".into()));
        }

        let retain_code = unsafe { ((*vtable).retain_context)(bridge_token) };
        if retain_code != abi::code::SUCCESS {
            return Err(ActrError::Internal(format!(
                "failed to retain dynclib bridge token {bridge_token}: {retain_code}"
            )));
        }

        Ok(Self {
            vtable,
            bridge_token,
            retained: true,
            self_id: ctx.self_id,
            caller_id: ctx.caller_id,
            request_id: ctx.request_id,
        })
    }

    fn vt(&self) -> &HostVTable {
        unsafe { &*self.vtable }
    }

    /// Outbound chunk send through the host frame ABI (sink `send_fn` body).
    async fn send_data_chunk_host(
        &self,
        target: &Dest,
        chunk: DataChunk,
        payload_type: PayloadType,
    ) -> ActorResult<()> {
        let payload = HostSendDataChunkV1 {
            dest: dest_to_v1(target),
            chunk,
            payload_type: payload_type as i32,
        };
        let reply = self.invoke_frame(payload.to_frame().map_err(abi_error_to_actr)?)?;
        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }
        Ok(())
    }

    /// Host-side stream unregister used by [`crate::DataChunkStream`] teardown.
    async fn unregister_stream_host(&self, stream_id: String) -> ActorResult<()> {
        let payload = HostUnregisterStreamV1 { stream_id };
        let reply = self.invoke_frame(payload.to_frame().map_err(abi_error_to_actr)?)?;
        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }
        Ok(())
    }

    fn invoke_frame(&self, frame: abi::AbiFrame) -> Result<AbiReply, ActrError> {
        let frame_bytes = abi::encode_message(&frame).map_err(abi_error_to_actr)?;
        let mut reply_ptr: *mut u8 = std::ptr::null_mut();
        let mut reply_len: usize = 0;

        let bridge_token =
            crate::guest::dynclib::runtime::active_bridge_token().unwrap_or(self.bridge_token);
        let code = unsafe {
            (self.vt().invoke)(
                bridge_token,
                frame_bytes.as_ptr(),
                frame_bytes.len(),
                &mut reply_ptr,
                &mut reply_len,
            )
        };

        if code != abi::code::SUCCESS {
            return Err(abi_error_to_actr(code));
        }

        let bytes = if reply_ptr.is_null() || reply_len == 0 {
            Vec::new()
        } else {
            let data = unsafe { std::slice::from_raw_parts(reply_ptr, reply_len).to_vec() };
            unsafe { (self.vt().free_host_buf)(reply_ptr, reply_len) };
            data
        };

        abi::decode_message::<AbiReply>(&bytes).map_err(abi_error_to_actr)
    }
}

impl Clone for DynclibContext {
    fn clone(&self) -> Self {
        let code = unsafe { (self.vt().retain_context)(self.bridge_token) };
        if code != abi::code::SUCCESS {
            tracing::error!(
                bridge_token = self.bridge_token,
                code,
                "failed to retain dynclib context during clone"
            );
        }

        Self {
            vtable: self.vtable,
            bridge_token: self.bridge_token,
            retained: code == abi::code::SUCCESS,
            self_id: self.self_id.clone(),
            caller_id: self.caller_id.clone(),
            request_id: self.request_id.clone(),
        }
    }
}

impl Drop for DynclibContext {
    fn drop(&mut self) {
        if self.retained {
            unsafe { (self.vt().release_context)(self.bridge_token) };
        }
    }
}

#[async_trait]
impl Context for DynclibContext {
    fn self_id(&self) -> &ActrId {
        &self.self_id
    }

    fn caller_id(&self) -> Option<&ActrId> {
        self.caller_id.as_ref()
    }

    fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Override the default `Context::log` to route the record across the FFI
    /// boundary into the host's tracing global via the vtable `log` trampoline.
    ///
    /// A cdylib statically bundles its own `tracing` crate with an isolated
    /// (null) global dispatcher, so the default impl — which calls
    /// `tracing::info!(target: "actr_framework::workload", ...)` — has its
    /// events dropped at the callsite and they never reach the host's
    /// subscriber. Forwarding through the host vtable lands them in the host
    /// process's tracing global, which the host-installed subscriber covers
    /// (stderr fmt layer + OTLP exporter), restoring stderr and OTLP
    /// visibility for workload logs. Mirrors the web target's FFI log bridge
    /// at `crate::web::context::Context::log`.
    fn log(&self, level: LogLevel, msg: &str) {
        let level_byte: u8 = match level {
            LogLevel::Trace => 0,
            LogLevel::Debug => 1,
            LogLevel::Info => 2,
            LogLevel::Warn => 3,
            LogLevel::Error => 4,
        };
        // Safety: `HostVTable::log` accepts a raw pointer + length to a UTF-8
        // buffer; `msg` is a valid `&str` for the call's duration and the host
        // trampoline only reads `msg_len` bytes (no retention beyond the call).
        unsafe {
            (self.vt().log)(self.bridge_token, level_byte, msg.as_ptr(), msg.len());
        }
    }

    async fn call<R: actr_protocol::RpcRequest>(
        &self,
        target: &Dest,
        request: R,
        timeout_ms: i64,
    ) -> ActorResult<R::Response> {
        let payload = HostCallV1 {
            route_key: R::route_key().to_string(),
            dest: dest_to_v1(target),
            payload: request.encode_to_vec(),
            timeout_ms,
            payload_type: R::payload_type() as i32,
        };
        let frame = payload.to_frame().map_err(abi_error_to_actr)?;
        let reply = self.invoke_frame(frame)?;

        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }

        R::Response::decode(reply.payload.as_slice())
            .map_err(|e| ActrError::DecodeFailure(format!("response decode failed: {e}")))
    }

    async fn tell<R: actr_protocol::RpcRequest>(
        &self,
        target: &Dest,
        message: R,
    ) -> ActorResult<()> {
        let payload = HostTellV1 {
            route_key: R::route_key().to_string(),
            dest: dest_to_v1(target),
            payload: message.encode_to_vec(),
            payload_type: R::payload_type() as i32,
        };
        let frame = payload.to_frame().map_err(abi_error_to_actr)?;
        let reply = self.invoke_frame(frame)?;

        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }

        Ok(())
    }

    async fn data_chunks(
        &self,
        stream_id: String,
        options: crate::DataChunkStreamOptions,
    ) -> ActorResult<crate::DataChunkStream> {
        let capacity = crate::validate_stream_options(&options)?;

        // Check, insert, host-register, and rollback in ONE lock scope:
        // `invoke_frame` is synchronous, so holding the lock across it keeps
        // the duplicate check atomic with the insertion (a concurrent
        // data_chunks on another thread cannot slip between them).
        let (tx, rx) = futures_channel::mpsc::channel(capacity);
        {
            let mut senders = stream_senders()
                .lock()
                .map_err(|_| ActrError::Internal("stream sender registry poisoned".into()))?;
            if senders.contains_key(&stream_id) {
                return Err(ActrError::InvalidArgument(format!(
                    "stream '{stream_id}' already has an active subscription"
                )));
            }
            senders.insert(stream_id.clone(), tx);

            let payload = HostRegisterStreamV1 {
                stream_id: stream_id.clone(),
            };
            let reply = self.invoke_frame(payload.to_frame().map_err(abi_error_to_actr)?)?;
            if reply.status != abi::code::SUCCESS {
                senders.remove(&stream_id);
                return Err(reply_to_actr_error(reply));
            }
        }

        let cancel_stream_id = stream_id.clone();
        let wait_stream_id = stream_id;
        let this = self.clone();
        let closer = crate::StreamCloser::with_wait(
            Box::new(move || {
                if let Ok(mut senders) = stream_senders().lock() {
                    senders.remove(&cancel_stream_id);
                }
            }),
            Box::new(move || {
                Box::pin(async move {
                    let _ = this.unregister_stream_host(wait_stream_id).await;
                })
            }),
        );
        Ok(crate::DataChunkStream::from_parts(rx, closer))
    }

    async fn data_chunk_sink(
        &self,
        target: &Dest,
        stream_id: String,
        payload_type: PayloadType,
    ) -> ActorResult<crate::DataChunkSink> {
        let this = self.clone();
        let dest = target.clone();
        let send_fn: crate::SendDataChunkFn = Box::new(move |chunk| {
            let this = this.clone();
            let dest = dest.clone();
            Box::pin(async move { this.send_data_chunk_host(&dest, chunk, payload_type).await })
        });
        crate::DataChunkSink::new(stream_id, payload_type, send_fn)
    }

    async fn discover_route_candidate(&self, target_type: &ActrType) -> ActorResult<ActrId> {
        let payload = HostDiscoverV1 {
            target_type: target_type.clone(),
        };
        let frame = payload.to_frame().map_err(abi_error_to_actr)?;
        let reply = self.invoke_frame(frame)?;

        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }

        ActrId::decode(reply.payload.as_slice())
            .map_err(|e| ActrError::DecodeFailure(format!("discover result decode failed: {e}")))
    }

    async fn resolve_dependency(&self, alias: &str) -> ActorResult<ActrType> {
        let payload = HostResolveDependencyV1 {
            alias: alias.to_string(),
        };
        let reply = self.invoke_frame(payload.to_frame().map_err(abi_error_to_actr)?)?;
        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }
        ActrType::decode(reply.payload.as_slice()).map_err(|error| {
            ActrError::DecodeFailure(format!("resolve_dependency result decode failed: {error}"))
        })
    }

    async fn call_raw(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: Bytes,
        timeout_ms: i64,
    ) -> ActorResult<Bytes> {
        self.call_raw_with_payload_type(
            target,
            route_key,
            PayloadType::RpcReliable,
            payload,
            timeout_ms,
        )
        .await
    }

    async fn call_raw_with_payload_type(
        &self,
        target: &ActrId,
        route_key: &str,
        payload_type: PayloadType,
        payload: Bytes,
        timeout_ms: i64,
    ) -> ActorResult<Bytes> {
        let request = HostCallRawV1 {
            route_key: route_key.to_string(),
            target: target.clone(),
            payload: payload.to_vec(),
            timeout_ms,
            payload_type: payload_type as i32,
        };
        let frame = request.to_frame().map_err(abi_error_to_actr)?;
        let reply = self.invoke_frame(frame)?;

        if reply.status != abi::code::SUCCESS {
            return Err(reply_to_actr_error(reply));
        }

        Ok(Bytes::from(reply.payload))
    }

    async fn register_media_track<F>(&self, _track_id: String, _callback: F) -> ActorResult<()>
    where
        F: Fn(MediaSample, ActrId) -> BoxFuture<'static, ActorResult<()>> + Send + Sync + 'static,
    {
        Err(ActrError::NotImplemented(
            "WebRTC media tracks are not supported in dynclib environment".into(),
        ))
    }

    async fn unregister_media_track(&self, _track_id: &str) -> ActorResult<()> {
        Err(ActrError::NotImplemented(
            "WebRTC media tracks are not supported in dynclib environment".into(),
        ))
    }

    async fn send_media_sample(
        &self,
        _target: &Dest,
        _track_id: &str,
        _sample: MediaSample,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented(
            "WebRTC media tracks are not supported in dynclib environment".into(),
        ))
    }

    async fn add_media_track(
        &self,
        _target: &Dest,
        _track_id: &str,
        _codec: &str,
        _media_type: &str,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented(
            "WebRTC media tracks are not supported in dynclib environment".into(),
        ))
    }

    async fn remove_media_track(&self, _target: &Dest, _track_id: &str) -> ActorResult<()> {
        Err(ActrError::NotImplemented(
            "WebRTC media tracks are not supported in dynclib environment".into(),
        ))
    }
}
