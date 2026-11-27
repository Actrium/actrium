//! Context operations supported across the OCI IPC boundary.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use actr_protocol::oci::{Body, HostOperation, HostRequest};
use actr_protocol::{ActorResult, ActrError, ActrId, ActrType, DataChunk, PayloadType, RpcRequest};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use prost::Message;
use tokio::sync::oneshot;

use crate::{Context, Dest, MaybeSendBoxFuture, MaybeSendSync, MediaSample};

use super::{Port, disconnected};

#[derive(Clone)]
pub(super) struct GuestContext {
    pub(super) port: Arc<Port>,
    pub(super) active: Arc<AtomicBool>,
    pub(super) token: Vec<u8>,
    pub(super) self_id: ActrId,
    pub(super) caller: Option<ActrId>,
    pub(super) request_id: String,
    pub(super) expects_response: bool,
}

struct PendingReply<'a> {
    port: &'a Port,
    sequence: u64,
}

impl Drop for PendingReply<'_> {
    fn drop(&mut self) {
        self.port.pending.lock().unwrap().remove(&self.sequence);
    }
}

impl GuestContext {
    async fn request(
        &self,
        operation: HostOperation,
        mut request: HostRequest,
    ) -> ActorResult<Bytes> {
        if !self.active.load(Ordering::Acquire) {
            return Err(ActrError::PermissionDenied(
                "OCI invocation has ended".into(),
            ));
        }
        let _slot =
            self.port.slots.try_acquire().map_err(|_| {
                ActrError::Unavailable("OCI Guest host-request limit reached".into())
            })?;
        request.token.clone_from(&self.token);
        request.operation = operation as i32;
        let (sequence, receipt, receive) = {
            // Assign IDs in the same order in which frames enter the wire queue.
            let mut next = self.port.sequence.lock().unwrap();
            let sequence = *next;
            *next = next.checked_add(1).ok_or_else(disconnected)?;
            let (reply, receive) = oneshot::channel();
            self.port.pending.lock().unwrap().insert(sequence, reply);
            let receipt = self.port.enqueue(sequence, Body::HostRequest(request));
            (sequence, receipt, receive)
        };
        let _pending = PendingReply {
            port: &self.port,
            sequence,
        };
        receipt?
            .await
            .map_err(|_| disconnected())?
            .map_err(super::io_error)?;
        receive.await.map_err(|_| disconnected())?.map(Bytes::from)
    }
}

#[async_trait]
impl Context for GuestContext {
    fn self_id(&self) -> &ActrId {
        &self.self_id
    }

    fn caller_id(&self) -> Option<&ActrId> {
        self.caller.as_ref()
    }

    fn request_id(&self) -> &str {
        &self.request_id
    }

    fn expects_response(&self) -> bool {
        self.expects_response
    }

    async fn call_dependency(
        &self,
        alias: &str,
        route: &str,
        payload: Bytes,
    ) -> ActorResult<Bytes> {
        self.request(
            HostOperation::Call,
            HostRequest {
                alias: alias.into(),
                route: route.into(),
                payload: payload.to_vec(),
                ..Default::default()
            },
        )
        .await
    }

    async fn https_request(&self, alias: &str, body: Bytes) -> ActorResult<Bytes> {
        self.request(
            HostOperation::Https,
            HostRequest {
                alias: alias.into(),
                payload: body.to_vec(),
                ..Default::default()
            },
        )
        .await
    }

    async fn call<R: RpcRequest>(
        &self,
        target: &Dest,
        request: R,
        timeout_ms: i64,
    ) -> ActorResult<R::Response> {
        let bytes = self
            .call_raw_with_payload_type(
                peer(target)?,
                R::route_key(),
                R::payload_type(),
                request.encode_to_vec().into(),
                timeout_ms,
            )
            .await?;
        R::Response::decode(bytes).map_err(decode_error)
    }

    async fn tell<R: RpcRequest>(&self, target: &Dest, message: R) -> ActorResult<()> {
        if R::payload_type() != PayloadType::RpcReliable {
            return Err(unsupported());
        }
        self.request(
            HostOperation::Tell,
            HostRequest {
                target: Some(peer(target)?.clone()),
                route: R::route_key().into(),
                payload: message.encode_to_vec(),
                ..Default::default()
            },
        )
        .await
        .map(drop)
    }

    async fn call_raw(
        &self,
        target: &ActrId,
        route_key: &str,
        payload: Bytes,
        timeout_ms: i64,
    ) -> ActorResult<Bytes> {
        self.request(
            HostOperation::Call,
            HostRequest {
                target: Some(target.clone()),
                route: route_key.into(),
                payload: payload.to_vec(),
                timeout_ms,
                ..Default::default()
            },
        )
        .await
    }

    async fn discover_route_candidate(&self, actor_type: &ActrType) -> ActorResult<ActrId> {
        let bytes = self
            .request(
                HostOperation::Discover,
                HostRequest {
                    actor_type: Some(actor_type.clone()),
                    ..Default::default()
                },
            )
            .await?;
        ActrId::decode(bytes).map_err(decode_error)
    }

    async fn resolve_dependency(&self, alias: &str) -> ActorResult<ActrType> {
        let bytes = self
            .request(
                HostOperation::Resolve,
                HostRequest {
                    alias: alias.into(),
                    ..Default::default()
                },
            )
            .await?;
        ActrType::decode(bytes).map_err(decode_error)
    }

    async fn register_stream<F>(&self, _: String, _: F) -> ActorResult<()>
    where
        F: Fn(DataChunk, ActrId) -> MaybeSendBoxFuture<'static, ActorResult<()>>
            + MaybeSendSync
            + 'static,
    {
        Err(unsupported())
    }

    async fn unregister_stream(&self, _: &str) -> ActorResult<()> {
        Err(unsupported())
    }

    async fn send_data_chunk(&self, _: &Dest, _: DataChunk, _: PayloadType) -> ActorResult<()> {
        Err(unsupported())
    }

    async fn register_media_track<F>(&self, _: String, _: F) -> ActorResult<()>
    where
        F: Fn(MediaSample, ActrId) -> BoxFuture<'static, ActorResult<()>> + Send + Sync + 'static,
    {
        Err(unsupported())
    }

    async fn unregister_media_track(&self, _: &str) -> ActorResult<()> {
        Err(unsupported())
    }

    async fn send_media_sample(&self, _: &Dest, _: &str, _: MediaSample) -> ActorResult<()> {
        Err(unsupported())
    }

    async fn add_media_track(&self, _: &Dest, _: &str, _: &str, _: &str) -> ActorResult<()> {
        Err(unsupported())
    }

    async fn remove_media_track(&self, _: &Dest, _: &str) -> ActorResult<()> {
        Err(unsupported())
    }
}

fn peer(target: &Dest) -> ActorResult<&ActrId> {
    match target {
        Dest::Peer(peer) => Ok(peer),
        _ => Err(ActrError::PermissionDenied(
            "OCI requests require an authorized remote actor".into(),
        )),
    }
}

fn decode_error(error: prost::DecodeError) -> ActrError {
    ActrError::DecodeFailure(error.to_string())
}

fn unsupported() -> ActrError {
    ActrError::NotImplemented("operation is outside the OCI Guest API".into())
}
