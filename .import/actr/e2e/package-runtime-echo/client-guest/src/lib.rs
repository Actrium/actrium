//! Package Runtime Echo client guest workload.
//!
//! The guest discovers the remote EchoService, caches the resolved actor ID,
//! and retries discovery once after a failed call.

pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

use actr_framework::{Context, MessageDispatcher, Workload, entry};
use actr_protocol::{ActorResult, ActrError, ActrId, ActrType, RpcEnvelope, RpcRequest};
use async_trait::async_trait;
use bytes::Bytes;
use prost::Message as ProstMessage;

use echo::{EchoRequest, EchoResponse};

const ECHO_SERVICE_MANUFACTURER: &str = "actrium";
const ECHO_SERVICE_NAME: &str = "EchoService";

fn echo_service_version() -> String {
    std::env::var("ECHO_ACTR_VERSION").unwrap_or_else(|_| "1.0.0".to_string())
}

pub struct EchoEchoServiceEchoRpc {
    pub request: EchoRequest,
}

impl RpcRequest for EchoEchoServiceEchoRpc {
    type Response = EchoResponse;

    fn route_key(&self) -> &'static str {
        "echo.EchoService.Echo"
    }

    fn payload_type(&self) -> actr_protocol::PayloadType {
        actr_protocol::PayloadType::RpcReliable
    }

    fn encode_request(&self) -> Bytes {
        use prost::Message as _;
        self.request.encode_to_vec().into()
    }

    fn decode_response(data: &[u8]) -> Result<EchoResponse, prost::DecodeError> {
        prost::Message::decode(data)
    }
}

pub struct ClientGuestWorkload {
    cached_server_id: std::cell::Cell<Option<ActrId>>,
}

impl Default for ClientGuestWorkload {
    fn default() -> Self {
        Self {
            cached_server_id: std::cell::Cell::new(None),
        }
    }
}

unsafe impl Send for ClientGuestWorkload {}
unsafe impl Sync for ClientGuestWorkload {}

pub struct ClientGuestDispatcher;

#[async_trait]
impl MessageDispatcher for ClientGuestDispatcher {
    type Workload = ClientGuestWorkload;

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "echo.EchoService.Echo" => {
                let payload = envelope.payload.unwrap_or_default();
                let req = EchoRequest::decode(payload.as_ref())
                    .map_err(|e| ActrError::DecodeFailure(format!("decode EchoRequest: {e}")))?;
                let resp = proxy_echo(workload, ctx, req).await?;
                Ok(Bytes::from(resp.encode_to_vec()))
            }
            _ => Err(ActrError::UnknownRoute(envelope.route_key)),
        }
    }
}

async fn proxy_echo<C: Context>(
    workload: &ClientGuestWorkload,
    ctx: &C,
    req: EchoRequest,
) -> ActorResult<EchoResponse> {
    let server_id = resolve_server(workload, ctx).await?;
    let echo_req = EchoRequest {
        message: req.message.clone(),
    };

    match ctx
        .call_with_timeout(
            &actr_framework::Dest::Peer(server_id.clone()),
            EchoEchoServiceEchoRpc { request: echo_req },
            30_000,
        )
        .await
    {
        Ok(resp) => Ok(resp),
        Err(err) => {
            workload.cached_server_id.set(None);
            let fresh_id = discover_server(ctx).await?;
            workload.cached_server_id.set(Some(fresh_id.clone()));
            let retry_req = EchoRequest {
                message: req.message.clone(),
            };
            ctx.call_with_timeout(
                &actr_framework::Dest::Peer(fresh_id),
                EchoEchoServiceEchoRpc { request: retry_req },
                30_000,
            )
                .await
                .map_err(|retry_err| {
                    ActrError::Internal(format!(
                        "retry after cache clear failed: {retry_err} (original: {err})"
                    ))
                })
        }
    }
}

async fn resolve_server<C: Context>(
    workload: &ClientGuestWorkload,
    ctx: &C,
) -> ActorResult<ActrId> {
    if let Some(id) = workload.cached_server_id.take() {
        workload.cached_server_id.set(Some(id.clone()));
        return Ok(id);
    }

    let id = discover_server(ctx).await?;
    workload.cached_server_id.set(Some(id.clone()));
    Ok(id)
}

async fn discover_server<C: Context>(ctx: &C) -> ActorResult<ActrId> {
    let target_type = ActrType {
        manufacturer: ECHO_SERVICE_MANUFACTURER.to_string(),
        name: ECHO_SERVICE_NAME.to_string(),
        version: echo_service_version(),
    };
    ctx.discover_route_candidate(&target_type).await
}

impl Workload for ClientGuestWorkload {
    type Dispatcher = ClientGuestDispatcher;
}

entry!(ClientGuestWorkload);
