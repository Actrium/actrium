//! Service-side Actrium session wiring: system-route dispatch and the
//! current-policy business guard, sharing one session service and its secrets.

use std::collections::HashMap;
use std::sync::Arc;

use actr_protocol::{
    ActorResult, ActrError, DataChunk, Direction, MetadataEntry, RpcEnvelope, prost::Message as _,
};
use actrium_runtime::{
    AttachRequest, AttachResponse, BusinessRequest, DetachRequest, GetStatusRequest,
    GuardedRequest, HeartbeatRequest, InMemorySessionService, MessageKind, ResumeRequest,
    SessionSecret, TakeoverRequest, request_proof, request_signing_bytes,
};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex;

use super::{
    guard_error, invalid_metadata, metadata, missing_verified_channel, session_actr_error,
    verified_channel,
};
use crate::lifecycle::{
    IncomingDispatchRequest, IncomingGuard, IncomingGuardError, IncomingGuardErrorCode,
    IncomingMetadata, IncomingStreamChunk, SessionMethod, SessionRegistry,
};

/// Takeover carries both the takeover approval and the complete new Attach
/// request; the Service verifies them as one atomic exchange.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TakeoverPayload {
    pub takeover: TakeoverRequest,
    pub attach: AttachRequest,
}

#[derive(Default)]
struct Shared {
    secrets: Mutex<HashMap<String, SessionSecret>>,
    admitted: Mutex<HashMap<String, Admitted>>,
}

struct Admitted {
    /// Held between mailbox arrival and dequeue; `None` once the request has
    /// been rechecked at dequeue or when no durable mailbox is involved.
    guarded: Option<GuardedRequest>,
    request: BusinessRequest,
}

/// Dispatches the exact `actrium.session.v1` system routes into the session
/// service and records each granted session secret for response signing.
pub struct ActriumServiceRegistry {
    service: InMemorySessionService,
    shared: Arc<Shared>,
}

impl ActriumServiceRegistry {
    /// Builds the registry/guard pair over one session service. Install both
    /// on the same node; they share session secrets and admission state.
    pub fn new(service: InMemorySessionService) -> (Self, ActriumServiceGuard) {
        let shared = Arc::new(Shared::default());
        (
            Self {
                service: service.clone(),
                shared: shared.clone(),
            },
            ActriumServiceGuard { service, shared },
        )
    }

    async fn record_secret(&self, response: &AttachResponse, secret: SessionSecret) {
        self.shared
            .secrets
            .lock()
            .await
            .insert(response.session_id.clone(), secret);
    }
}

fn decode_payload<T: serde::de::DeserializeOwned>(envelope: &RpcEnvelope) -> ActorResult<T> {
    let payload = envelope.payload.as_deref().unwrap_or_default();
    serde_json::from_slice(payload)
        .map_err(|error| ActrError::InvalidArgument(format!("invalid session payload: {error}")))
}

fn encode_payload<T: serde::Serialize>(value: &T) -> ActorResult<Bytes> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|error| ActrError::Internal(format!("session response encoding failed: {error}")))
}

#[async_trait]
impl SessionRegistry for ActriumServiceRegistry {
    async fn dispatch(
        &self,
        method: SessionMethod,
        request: IncomingDispatchRequest<'_>,
    ) -> ActorResult<Bytes> {
        let context = request
            .peer_context
            .as_deref()
            .ok_or_else(|| missing_verified_channel().into_actr_error())?;
        let channel = verified_channel(context).map_err(|error| error.into_actr_error())?;
        match method {
            SessionMethod::Attach => {
                let attach: AttachRequest = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .attach(attach, &channel)
                    .await
                    .map_err(session_actr_error)?;
                self.record_secret(&outcome.response, outcome.session_secret)
                    .await;
                encode_payload(&outcome.response)
            }
            SessionMethod::Resume => {
                let resume: ResumeRequest = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .resume(resume, &channel)
                    .await
                    .map_err(session_actr_error)?;
                self.record_secret(&outcome.response, outcome.session_secret)
                    .await;
                encode_payload(&outcome.response)
            }
            SessionMethod::Takeover => {
                let payload: TakeoverPayload = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .takeover(payload.takeover, payload.attach, &channel)
                    .await
                    .map_err(session_actr_error)?;
                self.record_secret(&outcome.response, outcome.session_secret)
                    .await;
                encode_payload(&outcome.response)
            }
            SessionMethod::Heartbeat => {
                let heartbeat: HeartbeatRequest = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .heartbeat(heartbeat, &channel, None)
                    .await
                    .map_err(session_actr_error)?;
                encode_payload(&outcome)
            }
            SessionMethod::Detach => {
                let detach: DetachRequest = decode_payload(request.envelope)?;
                self.service
                    .detach(detach.clone(), &channel)
                    .await
                    .map_err(session_actr_error)?;
                self.shared.secrets.lock().await.remove(&detach.session_id);
                self.shared
                    .admitted
                    .lock()
                    .await
                    .retain(|_, entry| entry.request.session_id != detach.session_id);
                Ok(Bytes::new())
            }
            SessionMethod::GetStatus => {
                let status: GetStatusRequest = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .status(&status.session_id)
                    .await
                    .map_err(session_actr_error)?;
                encode_payload(&outcome)
            }
        }
    }
}

/// Authorizes business delivery against the session's current policy.
///
/// Requests and tells are admitted at arrival and rechecked at durable
/// mailbox dequeue with the identical proof input; any mutation between the
/// two observations is rejected. Responses to Service-originated calls are
/// outside the v1 scope and fail closed instead of passing unauthenticated.
pub struct ActriumServiceGuard {
    service: InMemorySessionService,
    shared: Arc<Shared>,
}

struct ProofFields {
    session_id: String,
    lease_epoch: u64,
    service_actor_id: String,
    channel_id: String,
    capability: String,
    issued_at: u64,
    deadline: u64,
    idempotency_key: Option<String>,
    proof: Vec<u8>,
}

fn required(
    metadata: IncomingMetadata<'_>,
    key: &'static str,
) -> Result<String, IncomingGuardError> {
    metadata
        .get(key)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid_metadata(format!("missing session metadata `{key}`")))
}

fn required_u64(
    metadata: IncomingMetadata<'_>,
    key: &'static str,
) -> Result<u64, IncomingGuardError> {
    metadata
        .get_u64(key)
        .map_err(IncomingGuardError::invalid_metadata)?
        .ok_or_else(|| invalid_metadata(format!("missing session metadata `{key}`")))
}

fn proof_fields(metadata: IncomingMetadata<'_>) -> Result<ProofFields, IncomingGuardError> {
    let proof = hex::decode(required(metadata, metadata::PROOF)?)
        .map_err(|error| invalid_metadata(format!("invalid request proof encoding: {error}")))?;
    Ok(ProofFields {
        session_id: required(metadata, metadata::SESSION_ID)?,
        lease_epoch: required_u64(metadata, metadata::LEASE_EPOCH)?,
        service_actor_id: required(metadata, metadata::SERVICE_ACTOR_ID)?,
        channel_id: required(metadata, metadata::CHANNEL_ID)?,
        capability: required(metadata, metadata::CAPABILITY)?,
        issued_at: required_u64(metadata, metadata::ISSUED_AT)?,
        deadline: required_u64(metadata, metadata::DEADLINE)?,
        idempotency_key: metadata.get(metadata::IDEMPOTENCY_KEY).map(str::to_owned),
        proof,
    })
}

impl ProofFields {
    fn into_request(
        self,
        route: String,
        request_id: String,
        kind: MessageKind,
        payload: Vec<u8>,
    ) -> BusinessRequest {
        BusinessRequest {
            session_id: self.session_id,
            lease_epoch: self.lease_epoch,
            service_actor_id: self.service_actor_id,
            channel_id: self.channel_id,
            route,
            capability: self.capability,
            request_id,
            issued_at: self.issued_at,
            deadline: self.deadline,
            kind,
            idempotency_key: self.idempotency_key,
            payload,
            proof: self.proof,
        }
    }
}

fn envelope_kind(envelope: &RpcEnvelope) -> Result<MessageKind, IncomingGuardError> {
    match envelope.direction {
        Some(value) if value == Direction::Request as i32 => Ok(MessageKind::Request),
        Some(value) if value == Direction::Tell as i32 => Ok(MessageKind::Tell),
        Some(value) if value == Direction::Response as i32 => Err(IncomingGuardError::new(
            IncomingGuardErrorCode::PermissionDenied,
            "responses to Service-originated calls are outside the v1 Actrium session scope"
                .to_string(),
        )),
        _ => Err(invalid_metadata("missing or unknown RPC direction")),
    }
}

fn envelope_request(
    envelope: &RpcEnvelope,
    metadata: IncomingMetadata<'_>,
) -> Result<BusinessRequest, IncomingGuardError> {
    let kind = envelope_kind(envelope)?;
    let fields = proof_fields(metadata)?;
    Ok(fields.into_request(
        envelope.route_key.clone(),
        envelope.request_id.clone(),
        kind,
        envelope.payload.as_deref().unwrap_or_default().to_vec(),
    ))
}

fn chunk_request(
    chunk: &DataChunk,
    metadata: IncomingMetadata<'_>,
) -> Result<BusinessRequest, IncomingGuardError> {
    let fields = proof_fields(metadata)?;
    Ok(fields.into_request(
        required(metadata, metadata::ROUTE)?,
        format!("{}:{}", chunk.stream_id, chunk.sequence),
        MessageKind::StreamChunk,
        chunk.payload.to_vec(),
    ))
}

#[async_trait]
impl IncomingGuard for ActriumServiceGuard {
    async fn check(&self, request: IncomingDispatchRequest<'_>) -> Result<(), IncomingGuardError> {
        let context = request
            .peer_context
            .as_deref()
            .ok_or_else(missing_verified_channel)?;
        let channel = verified_channel(context)?;
        let business = envelope_request(request.envelope, request.metadata)?;
        if business.request_id.is_empty() {
            return Err(invalid_metadata("business requests require a request id"));
        }
        let guard = self.service.guard();
        let mut admitted = self.shared.admitted.lock().await;
        if let Some(entry) = admitted.get_mut(&business.request_id) {
            // A second observation of the same id is the durable-mailbox
            // dequeue recheck. The dequeued envelope must be byte-identical
            // to the admitted proof input; the registry additionally compares
            // the recorded fingerprint and the verified channel generation.
            if request_signing_bytes(&entry.request) != request_signing_bytes(&business) {
                return Err(IncomingGuardError::new(
                    IncomingGuardErrorCode::PermissionDenied,
                    "dequeued request differs from the admitted proof input".to_string(),
                ));
            }
            let Some(guarded) = entry.guarded.take() else {
                return Err(IncomingGuardError::new(
                    IncomingGuardErrorCode::PermissionDenied,
                    "request id has already been dispatched".to_string(),
                ));
            };
            entry.request = guard
                .dequeue(guarded, &channel)
                .await
                .map_err(guard_error)?;
            return Ok(());
        }
        let guarded = guard
            .authorize(business.clone(), &channel)
            .await
            .map_err(guard_error)?;
        admitted.insert(
            business.request_id.clone(),
            Admitted {
                guarded: Some(guarded),
                request: business,
            },
        );
        Ok(())
    }

    async fn authenticate_response(
        &self,
        response: &mut RpcEnvelope,
    ) -> Result<(), IncomingGuardError> {
        // Admission-stage rejections have no admitted request and travel
        // without a proof; the Front passes those errors through unchanged.
        let Some(request) = self
            .shared
            .admitted
            .lock()
            .await
            .get(&response.request_id)
            .map(|entry| entry.request.clone())
        else {
            return Ok(());
        };
        let secret = self
            .shared
            .secrets
            .lock()
            .await
            .get(&request.session_id)
            .cloned()
            .ok_or_else(|| {
                IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the session secret is no longer available".to_string(),
                )
            })?;
        let payload = match (&response.error, &response.payload) {
            (Some(error), None) => error.encode_to_vec(),
            (None, Some(payload)) => payload.to_vec(),
            (None, None) => Vec::new(),
            (Some(_), Some(_)) => {
                return Err(invalid_metadata(
                    "a response must carry exactly one payload or error",
                ));
            }
        };
        let mut proof_input = request;
        proof_input.kind = MessageKind::Response;
        proof_input.payload = payload;
        proof_input.proof = Vec::new();
        let proof = request_proof(secret.as_bytes(), &proof_input);
        response
            .metadata
            .retain(|entry| entry.key != metadata::PROOF);
        response.metadata.push(MetadataEntry {
            key: metadata::PROOF.to_string(),
            value: hex::encode(proof),
        });
        Ok(())
    }

    async fn check_stream(
        &self,
        request: IncomingStreamChunk<'_>,
    ) -> Result<(), IncomingGuardError> {
        let context = request
            .peer_context
            .as_deref()
            .ok_or_else(missing_verified_channel)?;
        let channel = verified_channel(context)?;
        let business = chunk_request(request.chunk, request.metadata)?;
        self.service
            .guard()
            .authorize(business, &channel)
            .await
            .map_err(guard_error)?;
        Ok(())
    }
}
