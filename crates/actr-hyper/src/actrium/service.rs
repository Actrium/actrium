//! Service-side Actrium session wiring: system-route dispatch and the
//! current-policy business guard, sharing one session service and admission state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard, Weak};

use actr_protocol::{
    ActorResult, ActrError, ActrId, DataChunk, Direction, MetadataEntry, RpcEnvelope,
    prost::Message as _,
};
use actrium_runtime::{
    AttachRequest, BusinessRequest, DetachRequest, GetStatusRequest, GuardedRequest,
    HeartbeatRequest, InMemorySessionService, MessageKind, ResumeRequest, SessionSecret,
    SessionState, TakeoverRequest, request_proof, request_signing_bytes, verify_request_proof,
};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex;

use super::{
    ATTACHMENT_CANCEL_ROUTE, ATTACHMENT_PREPARE_ROUTE, ATTACHMENT_UPLOAD_TRANSPORT, guard_error,
    invalid_metadata, metadata, missing_verified_channel,
    peer::{ActriumPeerSession, business_metadata},
    session_actr_error, verified_channel,
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

const ATTACHMENT_RESULT_RETENTION_SECS: u64 = 300;
const MAX_PENDING_ATTACHMENT_RESULTS: usize = 1024;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Typed Prepare fields used to validate the result stream. The Host replaces
/// field 8 directly in the original wire payload so fields introduced by a
/// newer contract survive the attempt-specific transport rewrite.
#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
pub(crate) struct PrepareAttachmentRequest {
    #[prost(string, tag = "1")]
    pub(crate) id: String,
    #[prost(string, tag = "2")]
    pub(crate) filename: String,
    #[prost(int32, tag = "3")]
    pub(crate) r#type: i32,
    #[prost(string, tag = "4")]
    pub(crate) stream_id: String,
    #[prost(string, tag = "5")]
    pub(crate) checksum: String,
    #[prost(string, tag = "6")]
    pub(crate) checksum_algorithm: String,
    #[prost(uint64, tag = "7")]
    pub(crate) size_bytes: u64,
    #[prost(string, tag = "8")]
    pub(crate) result_stream_id: String,
}

/// A successful RPC transport does not imply that the application accepted the
/// upload. Only a well-formed zero-status Prepare response retains its stream.
#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
struct PrepareAttachmentResponse {
    #[prost(int32, tag = "2")]
    status_code: i32,
}

pub(crate) fn attachment_prepare_accepted(payload: &[u8]) -> bool {
    PrepareAttachmentResponse::decode(payload).is_ok_and(|response| response.status_code == 0)
}

/// Host-to-Host cancellation for one accepted Prepare. The request proof and
/// verified channel authorize the session; both IDs select the exact reservation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AttachmentResultCancellation {
    pub(crate) stream_id: String,
    pub(crate) prepare_request_id: String,
}

#[derive(Default)]
pub(crate) struct Shared {
    admitted: Mutex<HashMap<(String, String), Admitted>>,
    /// Prepare RPCs bind one result stream to the same Host-owned session.
    /// Entries expire and are consumed by the single terminal result chunk.
    attachment_results: Mutex<HashMap<String, PendingAttachmentResult>>,
    /// The Front side of the same exchange. Weak session handles avoid a second
    /// secret cache: verification always reads the live outbound session state.
    peer_attachment_results: StdMutex<HashMap<String, PeerAttachmentResult>>,
    /// Pending outbound calls waiting for their response proof. A synchronous
    /// mutex lets the RPC-owned guard reclaim its entry even when the caller
    /// drops the future while suspended.
    pending: StdMutex<HashMap<String, PendingCall>>,
}

struct PendingAttachmentResult {
    signing: ResponseContext,
    expires_at: u64,
}

pub(crate) struct PeerAttachmentResult {
    pub(crate) session: Weak<ActriumPeerSession>,
    pub(crate) request_id: String,
    pub(crate) capability: String,
    pub(crate) transport_stream_id: String,
    expires_at: u64,
}

impl Shared {
    pub(crate) fn pending(&self) -> StdMutexGuard<'_, HashMap<String, PendingCall>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn peer_attachment_results(&self) -> StdMutexGuard<'_, HashMap<String, PeerAttachmentResult>> {
        self.peer_attachment_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn register_peer_attachment_result(
        &self,
        stream_id: String,
        transport_stream_id: String,
        request_id: String,
        capability: String,
        session: &Arc<ActriumPeerSession>,
    ) -> Result<(), IncomingGuardError> {
        let now = now_secs();
        let mut results = self.peer_attachment_results();
        results.retain(|_, entry| entry.expires_at > now && entry.session.strong_count() > 0);
        if results.contains_key(&stream_id)
            || results
                .values()
                .any(|entry| entry.transport_stream_id == transport_stream_id)
            || results.len() >= MAX_PENDING_ATTACHMENT_RESULTS
        {
            return Err(IncomingGuardError::new(
                IncomingGuardErrorCode::Unavailable,
                "the attachment result stream is already pending or at capacity".to_string(),
            ));
        }
        results.insert(
            stream_id,
            PeerAttachmentResult {
                session: Arc::downgrade(session),
                request_id,
                capability,
                transport_stream_id,
                expires_at: now.saturating_add(ATTACHMENT_RESULT_RETENTION_SECS),
            },
        );
        Ok(())
    }

    fn peer_attachment_result_for_transport(
        &self,
        transport_stream_id: &str,
    ) -> Option<(Weak<ActriumPeerSession>, String, String)> {
        let now = now_secs();
        let mut results = self.peer_attachment_results();
        results.retain(|_, entry| entry.expires_at > now && entry.session.strong_count() > 0);
        results
            .iter()
            .find(|(_, entry)| entry.transport_stream_id == transport_stream_id)
            .map(|(stream_id, entry)| {
                (
                    entry.session.clone(),
                    entry.request_id.clone(),
                    stream_id.clone(),
                )
            })
    }

    pub(crate) fn finish_peer_attachment_result_for_request(
        &self,
        stream_id: &str,
        request_id: &str,
    ) -> bool {
        let mut results = self.peer_attachment_results();
        if results
            .get(stream_id)
            .is_some_and(|entry| entry.request_id == request_id)
        {
            results.remove(stream_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn take_peer_attachment_result(
        &self,
        stream_id: &str,
    ) -> Option<PeerAttachmentResult> {
        self.peer_attachment_results().remove(stream_id)
    }

    async fn finish_attachment_result_for_request(
        &self,
        stream_id: &str,
        request_id: &str,
        session_id: &str,
    ) {
        let mut results = self.attachment_results.lock().await;
        if results.get(stream_id).is_some_and(|entry| {
            entry.signing.request_id == request_id && entry.signing.session_id == session_id
        }) {
            results.remove(stream_id);
        }
    }

    #[cfg(test)]
    pub(crate) async fn attachment_result_request_id(&self, stream_id: &str) -> Option<String> {
        self.attachment_results
            .lock()
            .await
            .get(stream_id)
            .map(|entry| entry.signing.request_id.clone())
    }

    pub(crate) fn drop_peer_attachment_results(&self, session: &ActriumPeerSession) {
        self.peer_attachment_results().retain(|_, entry| {
            entry
                .session
                .upgrade()
                .is_some_and(|candidate| !std::ptr::eq(Arc::as_ptr(&candidate), session))
        });
    }
}

/// An outbound call awaiting its response proof. Kept in the shared inbound
/// guard so responses from any peer can be verified in one place.
pub(crate) struct PendingCall {
    pub(crate) request: BusinessRequest,
    pub(crate) secret: SessionSecret,
}

/// Owns one pending response-proof entry for exactly as long as its RPC.
/// Dropping or aborting the call future therefore cannot leak its payload or
/// session secret into the long-lived node state.
pub(crate) struct PendingCallGuard {
    shared: Arc<Shared>,
    request_id: String,
    attachment_result_stream: Option<String>,
    preserve_attachment_result: bool,
}

impl PendingCallGuard {
    pub(crate) fn new(shared: Arc<Shared>, request_id: String, pending: PendingCall) -> Self {
        shared.pending().insert(request_id.clone(), pending);
        Self {
            shared,
            request_id,
            attachment_result_stream: None,
            preserve_attachment_result: false,
        }
    }

    pub(crate) fn with_attachment_result(mut self, stream_id: Option<String>) -> Self {
        self.attachment_result_stream = stream_id;
        self
    }

    pub(crate) fn preserve_attachment_result(mut self) -> Self {
        self.preserve_attachment_result = true;
        self
    }
}

impl Drop for PendingCallGuard {
    fn drop(&mut self) {
        self.shared.pending().remove(&self.request_id);
        if !self.preserve_attachment_result
            && let Some(stream_id) = self.attachment_result_stream.as_deref()
        {
            self.shared
                .finish_peer_attachment_result_for_request(stream_id, &self.request_id);
        }
    }
}

/// Lightweight metadata kept for signing a response to an admitted request.
/// The original request payload is intentionally excluded; the response proof
/// covers the response payload and these authority fields.
#[derive(Clone)]
struct ResponseContext {
    session_id: String,
    lease_epoch: u64,
    service_actor_id: String,
    channel_id: String,
    route: String,
    capability: String,
    request_id: String,
    issued_at: u64,
    deadline: u64,
    idempotency_key: Option<String>,
}

impl ResponseContext {
    fn from_request(request: &BusinessRequest) -> Self {
        Self {
            session_id: request.session_id.clone(),
            lease_epoch: request.lease_epoch,
            service_actor_id: request.service_actor_id.clone(),
            channel_id: request.channel_id.clone(),
            route: request.route.clone(),
            capability: request.capability.clone(),
            request_id: request.request_id.clone(),
            issued_at: request.issued_at,
            deadline: request.deadline,
            idempotency_key: request.idempotency_key.clone(),
        }
    }

    fn into_request(self, kind: MessageKind, payload: Vec<u8>) -> BusinessRequest {
        BusinessRequest {
            session_id: self.session_id,
            lease_epoch: self.lease_epoch,
            service_actor_id: self.service_actor_id,
            channel_id: self.channel_id,
            route: self.route,
            capability: self.capability,
            request_id: self.request_id,
            issued_at: self.issued_at,
            deadline: self.deadline,
            kind,
            idempotency_key: self.idempotency_key,
            payload,
            proof: Vec::new(),
        }
    }
}

pub(crate) fn attachment_result_stream(
    route: &str,
    payload: &[u8],
) -> Result<Option<String>, IncomingGuardError> {
    if route != ATTACHMENT_PREPARE_ROUTE {
        return Ok(None);
    }
    let prepare = PrepareAttachmentRequest::decode(payload).map_err(|error| {
        invalid_metadata(format!("invalid attachment prepare payload: {error}"))
    })?;
    if prepare.result_stream_id.is_empty() {
        return Err(invalid_metadata(
            "attachment prepare requires a result stream id",
        ));
    }
    Ok(Some(prepare.result_stream_id))
}

/// Replace every occurrence of a singular length-delimited field without
/// decoding and re-encoding the surrounding message. In particular, unknown
/// fields and their original wire representation remain byte-for-byte intact.
fn replace_length_delimited_field(
    payload: &[u8],
    field: u32,
    replacement: &str,
) -> Result<Vec<u8>, IncomingGuardError> {
    use actr_protocol::prost::encoding::{self, DecodeContext, WireType};

    let invalid = |error: actr_protocol::prost::DecodeError| {
        invalid_metadata(format!("invalid attachment prepare payload: {error}"))
    };
    let mut remaining = payload;
    let mut copied = 0;
    let mut replaced = false;
    let mut output = Vec::with_capacity(payload.len() + replacement.len() + 16);
    while !remaining.is_empty() {
        let field_start = payload.len() - remaining.len();
        let (tag, wire_type) = encoding::decode_key(&mut remaining).map_err(invalid)?;
        encoding::skip_field(wire_type, tag, &mut remaining, DecodeContext::default())
            .map_err(invalid)?;
        if tag != field {
            continue;
        }
        if wire_type != WireType::LengthDelimited {
            return Err(invalid_metadata(
                "attachment result stream id must be length-delimited",
            ));
        }
        output.extend_from_slice(&payload[copied..field_start]);
        encoding::encode_key(field, WireType::LengthDelimited, &mut output);
        encoding::encode_varint(replacement.len() as u64, &mut output);
        output.extend_from_slice(replacement.as_bytes());
        copied = payload.len() - remaining.len();
        replaced = true;
    }
    if !replaced {
        return Err(invalid_metadata(
            "attachment prepare requires a result stream id",
        ));
    }
    output.extend_from_slice(&payload[copied..]);
    Ok(output)
}

/// Give every Prepare a fresh wire-level result stream. A delayed result from
/// an abandoned upload therefore cannot be authenticated as a later Prepare
/// that happens to reuse the same caller-visible stream ID.
pub(crate) fn fresh_attachment_result_stream(
    route: &str,
    payload: &[u8],
) -> Result<Option<(String, String, Vec<u8>)>, IncomingGuardError> {
    let Some(result_stream) = attachment_result_stream(route, payload)? else {
        return Ok(None);
    };
    let transport_stream = format!("actrium-result-{}", uuid::Uuid::new_v4());
    let payload = replace_length_delimited_field(payload, 8, &transport_stream)?;
    Ok(Some((result_stream, transport_stream, payload)))
}

struct Admitted {
    /// Held between mailbox arrival and dequeue; `None` once the request has
    /// been rechecked at dequeue or when no durable mailbox is involved.
    guarded: Option<GuardedRequest>,
    /// Metadata used to sign the response without retaining the request payload.
    signing: ResponseContext,
    /// Cached signing bytes of the admitted request, used for the dequeue
    /// byte-identical recheck.
    request_signing_bytes: Vec<u8>,
    /// Prepare reservations are released when the application rejects the RPC.
    attachment_result_stream: Option<String>,
}

/// Dispatches the exact `actrium.session.v1` system routes into the session
/// service. Response signing reads the secret back from that registry so its
/// lifetime follows the registry's expiry, Detach, Takeover, and retention rules.
pub struct ActriumServiceRegistry {
    service: InMemorySessionService,
    shared: Arc<Shared>,
}

impl ActriumServiceRegistry {
    /// Builds the registry/guard pair over one session service. Install both
    /// on the same node; they share admission state and outbound pending calls
    /// so the guard can verify peer responses.
    pub(crate) fn new(
        service: InMemorySessionService,
        shared: Arc<Shared>,
    ) -> (Self, ActriumServiceGuard) {
        (
            Self {
                service: service.clone(),
                shared: shared.clone(),
            },
            ActriumServiceGuard {
                service,
                shared,
                front_events: None,
            },
        )
    }

    #[cfg(test)]
    pub(super) async fn detach_without_cleanup_for_test(
        &self,
        detach: DetachRequest,
        context: &crate::transport::VerifiedPeerContext,
    ) {
        let channel = verified_channel(context).expect("valid test channel");
        self.service
            .detach(detach, &channel)
            .await
            .expect("test session detaches");
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
                encode_payload(&outcome.response)
            }
            SessionMethod::Resume => {
                let resume: ResumeRequest = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .resume(resume, &channel)
                    .await
                    .map_err(session_actr_error)?;
                encode_payload(&outcome.response)
            }
            SessionMethod::Takeover => {
                let payload: TakeoverPayload = decode_payload(request.envelope)?;
                let outcome = self
                    .service
                    .takeover(payload.takeover, payload.attach, &channel)
                    .await
                    .map_err(session_actr_error)?;
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
                self.shared
                    .admitted
                    .lock()
                    .await
                    .retain(|_, entry| entry.signing.session_id != detach.session_id);
                self.shared
                    .attachment_results
                    .lock()
                    .await
                    .retain(|_, entry| entry.signing.session_id != detach.session_id);
                Ok(Bytes::new())
            }
            SessionMethod::CancelAttachmentResult => {
                let business = envelope_request(request.envelope, request.metadata)
                    .map_err(|error| error.into_actr_error())?;
                if business.kind != MessageKind::Request
                    || business.route != ATTACHMENT_CANCEL_ROUTE
                {
                    return Err(ActrError::InvalidArgument(
                        "attachment cancellation requires a signed Request on its exact route"
                            .to_string(),
                    ));
                }
                let guarded = self
                    .service
                    .guard()
                    .authorize(business.clone(), &channel)
                    .await
                    .map_err(session_actr_error)?;
                let cancellation: AttachmentResultCancellation =
                    serde_json::from_slice(&business.payload).map_err(|error| {
                        ActrError::InvalidArgument(format!(
                            "invalid attachment cancellation payload: {error}"
                        ))
                    })?;
                if cancellation.stream_id.is_empty() || cancellation.prepare_request_id.is_empty() {
                    return Err(ActrError::InvalidArgument(
                        "attachment cancellation requires a stream and Prepare request id"
                            .to_string(),
                    ));
                }
                self.shared
                    .finish_attachment_result_for_request(
                        &cancellation.stream_id,
                        &cancellation.prepare_request_id,
                        &business.session_id,
                    )
                    .await;
                self.shared.admitted.lock().await.insert(
                    (business.session_id.clone(), business.request_id.clone()),
                    Admitted {
                        guarded: Some(guarded),
                        signing: ResponseContext::from_request(&business),
                        request_signing_bytes: request_signing_bytes(&business),
                        attachment_result_stream: None,
                    },
                );
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
    front_events: Option<crate::FrontEventRouter>,
}

impl ActriumServiceGuard {
    pub(crate) fn with_front_events(mut self, events: crate::FrontEventRouter) -> Self {
        self.front_events = Some(events);
        self
    }
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
        .map_err(IncomingGuardError::invalid_metadata)?
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
        idempotency_key: metadata
            .get(metadata::IDEMPOTENCY_KEY)
            .map_err(IncomingGuardError::invalid_metadata)?
            .map(str::to_owned),
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

fn response_payload_bytes(response: &RpcEnvelope) -> Result<Vec<u8>, IncomingGuardError> {
    match (&response.error, &response.payload) {
        (Some(error), None) => Ok(error.encode_to_vec()),
        (None, Some(payload)) => Ok(payload.to_vec()),
        (None, None) => Ok(Vec::new()),
        (Some(_), Some(_)) => Err(invalid_metadata(
            "a response must carry exactly one payload or error",
        )),
    }
}

fn verify_inbound_response(
    response: &RpcEnvelope,
    pending: PendingCall,
) -> Result<(), IncomingGuardError> {
    let proof = response
        .metadata
        .iter()
        .find(|entry| entry.key == metadata::PROOF)
        .ok_or_else(|| {
            IncomingGuardError::new(
                IncomingGuardErrorCode::PermissionDenied,
                "inbound response is missing its proof".to_string(),
            )
        })?;
    let proof = hex::decode(&proof.value).map_err(|error| {
        invalid_metadata(format!("invalid inbound response proof encoding: {error}"))
    })?;
    let payload = response_payload_bytes(response)?;
    let mut input = pending.request;
    input.kind = MessageKind::Response;
    input.payload = payload;
    input.proof = Vec::new();
    if verify_request_proof(pending.secret.as_bytes(), &input, &proof) {
        Ok(())
    } else {
        Err(IncomingGuardError::new(
            IncomingGuardErrorCode::PermissionDenied,
            "inbound response proof verification failed".to_string(),
        ))
    }
}

#[async_trait]
impl IncomingGuard for ActriumServiceGuard {
    async fn check(&self, request: IncomingDispatchRequest<'_>) -> Result<(), IncomingGuardError> {
        let envelope = request.envelope;

        // Inbound responses to our outbound peer calls are verified here, before
        // the pending caller is woken. Non-session responses have no pending entry
        // and pass through unchanged.
        if envelope.direction == Some(Direction::Response as i32) {
            let pending = self.shared.pending().remove(&envelope.request_id);
            if let Some(pending) = pending {
                verify_inbound_response(envelope, pending)?;
            }
            return Ok(());
        }

        let context = request
            .peer_context
            .as_deref()
            .ok_or_else(missing_verified_channel)?;
        let channel = verified_channel(context)?;
        let caller = request.caller.map(|(actor_id, _)| actor_id);
        if let Some(events) = &self.front_events
            && caller == Some(&context.peer_actor_id)
            && events.admits(envelope, caller)
        {
            // Front events are reliable Tells, not session-proved business RPCs.
            // The verified channel and the discovered Service identity authorize
            // only this declared route; FrontWorkload rechecks it before delivery.
            return Ok(());
        }
        let business = envelope_request(request.envelope, request.metadata)?;
        if business.request_id.is_empty() {
            return Err(invalid_metadata("business requests require a request id"));
        }
        let request_signing_bytes = request_signing_bytes(&business);
        let result_stream = attachment_result_stream(&business.route, &business.payload)?;
        let guard = self.service.guard();
        let admission_key = (business.session_id.clone(), business.request_id.clone());
        let mut admitted = self.shared.admitted.lock().await;
        if let Some(entry) = admitted.get_mut(&admission_key) {
            // A second observation of the same id is the durable-mailbox
            // dequeue recheck. The dequeued envelope must be byte-identical
            // to the admitted proof input; the registry additionally compares
            // the recorded fingerprint and the verified channel generation.
            if entry.request_signing_bytes != request_signing_bytes {
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
            let _dequeued = guard
                .dequeue(guarded, &channel)
                .await
                .map_err(guard_error)?;
            return Ok(());
        }
        let guarded = guard
            .authorize(business.clone(), &channel)
            .await
            .map_err(guard_error)?;
        if let Some(stream_id) = result_stream.as_ref() {
            let now = now_secs();
            let mut results = self.shared.attachment_results.lock().await;
            results.retain(|_, entry| entry.expires_at > now);
            if results.len() >= MAX_PENDING_ATTACHMENT_RESULTS && !results.contains_key(stream_id) {
                return Err(IncomingGuardError::new(
                    IncomingGuardErrorCode::Unavailable,
                    "too many pending attachment result streams".to_string(),
                ));
            }
            results.insert(
                stream_id.clone(),
                PendingAttachmentResult {
                    signing: ResponseContext::from_request(&business),
                    expires_at: now.saturating_add(ATTACHMENT_RESULT_RETENTION_SECS),
                },
            );
        }
        admitted.insert(
            admission_key,
            Admitted {
                guarded: Some(guarded),
                signing: ResponseContext::from_request(&business),
                request_signing_bytes,
                attachment_result_stream: result_stream,
            },
        );
        Ok(())
    }

    async fn authenticate_response(
        &self,
        response: &mut RpcEnvelope,
    ) -> Result<(), IncomingGuardError> {
        // Sign an outbound response to an admitted inbound request. Inbound
        // response verification now happens in `check()` before the pending
        // caller is woken; this hook only produces the Service-side proof.
        let response_metadata = IncomingMetadata::new(&response.metadata);
        let Some(session_id) = response_metadata
            .get(metadata::SESSION_ID)
            .map_err(IncomingGuardError::invalid_metadata)?
        else {
            return Ok(());
        };
        let admitted = self.shared.admitted.lock().await;
        let Some(entry) = admitted.get(&(session_id.to_owned(), response.request_id.clone()))
        else {
            return Ok(());
        };
        let signing = entry.signing.clone();
        let attachment_result_stream = entry.attachment_result_stream.clone();
        drop(admitted);
        if let Some(stream_id) = attachment_result_stream {
            let accepted = response.error.is_none()
                && attachment_prepare_accepted(response.payload.as_deref().unwrap_or_default());
            if !accepted {
                self.shared
                    .finish_attachment_result_for_request(
                        &stream_id,
                        &signing.request_id,
                        &signing.session_id,
                    )
                    .await;
            }
        }
        let secret = self
            .service
            .session_secret(&signing.session_id)
            .await
            .ok_or_else(|| {
                IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the session is closed or expired".to_string(),
                )
            })?;

        let payload = response_payload_bytes(response)?;
        let mut proof_input = signing.into_request(MessageKind::Response, payload);
        let proof = request_proof(secret.as_bytes(), &proof_input);
        proof_input.proof = proof.clone();
        response
            .metadata
            .retain(|entry| entry.key != metadata::PROOF);
        response.metadata.push(MetadataEntry {
            key: metadata::PROOF.to_string(),
            value: hex::encode(proof),
        });
        Ok(())
    }

    async fn finish(
        &self,
        request_id: &str,
        metadata: IncomingMetadata<'_>,
    ) -> Result<(), IncomingGuardError> {
        let Some(session_id) = metadata
            .get(metadata::SESSION_ID)
            .map_err(IncomingGuardError::invalid_metadata)?
        else {
            return Ok(());
        };
        self.shared
            .admitted
            .lock()
            .await
            .remove(&(session_id.to_owned(), request_id.to_owned()));
        Ok(())
    }

    fn stream_alias(&self, stream_id: &str) -> Option<String> {
        self.shared
            .peer_attachment_result_for_transport(stream_id)
            .map(|(_, _, stream_id)| stream_id)
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
        let transport_stream_id = request.chunk.stream_id.clone();
        if request.sender == &context.peer_actor_id
            && self.front_events.as_ref().is_some_and(|router| {
                router.admits_stream(&transport_stream_id, &context.peer_actor_id)
            })
        {
            return Ok(());
        }
        let business = chunk_request(request.chunk, request.metadata())?;
        if let Some((session, prepare_request_id, stream_id)) = self
            .shared
            .peer_attachment_result_for_transport(&transport_stream_id)
        {
            let session = session.upgrade().ok_or_else(|| {
                IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the Front's outbound attachment session is closed".to_string(),
                )
            })?;
            session
                .verify_incoming_stream(&business, &channel)
                .await
                .map_err(|error| {
                    IncomingGuardError::new(
                        IncomingGuardErrorCode::PermissionDenied,
                        format!("outbound attachment result proof verification failed: {error}"),
                    )
                })?;
            // Cancellation may have released this attempt while verification
            // was suspended. Consume its exact reservation before restoring the
            // caller-visible ID so a late result cannot reach a replacement.
            if !self
                .shared
                .finish_peer_attachment_result_for_request(&stream_id, &prepare_request_id)
            {
                return Err(IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the attachment result attempt is no longer active".to_string(),
                ));
            }
            request.into_chunk().stream_id = stream_id;
            return Ok(());
        }
        self.service
            .guard()
            .authorize(business, &channel)
            .await
            .map_err(guard_error)?;
        Ok(())
    }

    async fn authenticate_stream(
        &self,
        target: &ActrId,
        chunk: &mut DataChunk,
    ) -> Result<(), IncomingGuardError> {
        let pending = {
            let now = now_secs();
            let mut results = self.shared.attachment_results.lock().await;
            results.retain(|_, entry| entry.expires_at > now);
            results.remove(&chunk.stream_id)
        };
        let Some(pending) = pending else {
            return Ok(());
        };
        let session = self
            .service
            .session(&pending.signing.session_id)
            .await
            .filter(|session| {
                matches!(session.state, SessionState::Active | SessionState::Grace)
                    && session.front_actor_id == target.to_string_repr()
            })
            .ok_or_else(|| {
                IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the attachment result session is closed, expired, or targets another Front"
                        .to_string(),
                )
            })?;
        let secret = self
            .service
            .session_secret(&pending.signing.session_id)
            .await
            .ok_or_else(|| {
                IncomingGuardError::new(
                    IncomingGuardErrorCode::Revoked,
                    "the attachment result session is closed or expired".to_string(),
                )
            })?;

        let now = now_secs();
        let mut proof_input = pending
            .signing
            .into_request(MessageKind::StreamChunk, chunk.payload.to_vec());
        proof_input.route = ATTACHMENT_UPLOAD_TRANSPORT.to_string();
        proof_input.request_id = format!("{}:{}", chunk.stream_id, chunk.sequence);
        proof_input.lease_epoch = session.lease_epoch;
        proof_input.service_actor_id = session.service_actor_id;
        proof_input.channel_id = session.channel_id;
        proof_input.issued_at = now;
        proof_input.deadline = now.saturating_add(30);
        proof_input.idempotency_key = None;
        proof_input.proof = request_proof(secret.as_bytes(), &proof_input);

        chunk.metadata.retain(|entry| {
            !matches!(
                entry.key.as_str(),
                metadata::SESSION_ID
                    | metadata::LEASE_EPOCH
                    | metadata::SERVICE_ACTOR_ID
                    | metadata::CHANNEL_ID
                    | metadata::CAPABILITY
                    | metadata::ROUTE
                    | metadata::ISSUED_AT
                    | metadata::DEADLINE
                    | metadata::IDEMPOTENCY_KEY
                    | metadata::PROOF
            )
        });
        chunk.metadata.extend(business_metadata(&proof_input));
        chunk.metadata.push(MetadataEntry {
            key: metadata::ROUTE.to_string(),
            value: ATTACHMENT_UPLOAD_TRANSPORT.to_string(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod attachment_reservation_tests {
    use super::*;

    fn pending(request_id: &str, session_id: &str) -> PendingAttachmentResult {
        PendingAttachmentResult {
            signing: ResponseContext {
                session_id: session_id.to_string(),
                lease_epoch: 1,
                service_actor_id: "service".to_string(),
                channel_id: "channel".to_string(),
                route: ATTACHMENT_PREPARE_ROUTE.to_string(),
                capability: "documents.read".to_string(),
                request_id: request_id.to_string(),
                issued_at: 1,
                deadline: 30,
                idempotency_key: None,
            },
            expires_at: u64::MAX,
        }
    }

    #[test]
    fn prepare_transport_streams_are_attempt_specific() {
        let prepare = PrepareAttachmentRequest {
            id: "attachment".to_string(),
            filename: "notes.txt".to_string(),
            r#type: 3,
            stream_id: "chunks".to_string(),
            checksum: "checksum".to_string(),
            checksum_algorithm: "md5".to_string(),
            size_bytes: 42,
            result_stream_id: "result".to_string(),
        };
        let mut payload = prepare.encode_to_vec();
        let unknown_start = payload.len();
        actr_protocol::prost::encoding::encode_key(
            100,
            actr_protocol::prost::encoding::WireType::LengthDelimited,
            &mut payload,
        );
        actr_protocol::prost::encoding::encode_varint(11, &mut payload);
        payload.extend_from_slice(b"newer-field");
        let unknown = payload[unknown_start..].to_vec();
        let (public, first, first_payload) =
            fresh_attachment_result_stream(ATTACHMENT_PREPARE_ROUTE, &payload)
                .unwrap()
                .unwrap();
        let (_, second, second_payload) =
            fresh_attachment_result_stream(ATTACHMENT_PREPARE_ROUTE, &payload)
                .unwrap()
                .unwrap();

        assert_eq!(public, "result");
        assert_ne!(first, second);
        for (transport, payload) in [(first, first_payload), (second, second_payload)] {
            assert!(payload.ends_with(&unknown));
            let decoded = PrepareAttachmentRequest::decode(payload.as_slice()).unwrap();
            let mut expected = prepare.clone();
            expected.result_stream_id = transport;
            assert_eq!(decoded, expected);
        }
    }

    #[tokio::test]
    async fn cleanup_only_removes_the_prepare_that_owns_the_reservation() {
        let shared = Shared::default();
        shared
            .attachment_results
            .lock()
            .await
            .insert("result".to_string(), pending("new-prepare", "new-session"));

        shared
            .finish_attachment_result_for_request("result", "old-prepare", "old-session")
            .await;
        assert_eq!(
            shared
                .attachment_results
                .lock()
                .await
                .get("result")
                .map(|entry| entry.signing.request_id.as_str()),
            Some("new-prepare")
        );
        shared
            .finish_attachment_result_for_request("result", "new-prepare", "old-session")
            .await;
        assert!(
            shared
                .attachment_results
                .lock()
                .await
                .contains_key("result")
        );
        shared
            .finish_attachment_result_for_request("result", "new-prepare", "new-session")
            .await;
        assert!(
            !shared
                .attachment_results
                .lock()
                .await
                .contains_key("result")
        );

        shared.peer_attachment_results().insert(
            "result".to_string(),
            PeerAttachmentResult {
                session: Weak::new(),
                request_id: "new-prepare".to_string(),
                capability: "documents.read".to_string(),
                transport_stream_id: "new-transport".to_string(),
                expires_at: u64::MAX,
            },
        );
        shared.finish_peer_attachment_result_for_request("result", "old-prepare");
        assert_eq!(
            shared
                .peer_attachment_results()
                .get("result")
                .map(|entry| entry.request_id.as_str()),
            Some("new-prepare")
        );
        shared.finish_peer_attachment_result_for_request("result", "new-prepare");
        assert!(!shared.peer_attachment_results().contains_key("result"));
    }
}
