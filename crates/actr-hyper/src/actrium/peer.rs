//! Peer-side Actrium session client: connect over the verified Actor
//! channel, Attach, sign business calls, and maintain the lease.
//!
//! In the P2P model every node that participates in Actrium sessions also
//! hosts an `InMemorySessionService`; any such node can initiate an Attach to
//! another peer and become a caller. [`ActrRef::attach_actrium`] performs the
//! Attach exchange and returns an [`ActriumPeerSession`] whose calls carry
//! session metadata and HMAC proofs. Responses are verified by the shared
//! inbound guard before the pending caller is woken.
//!
//! v1 scope: a peer is both a Service (it receives Attach and runs a Guard)
//! and a caller (it can Attach to other peers). Service-originated business
//! calls and streams toward a caller are handled by the Service side, not
//! this module.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_framework::{Context as _, Dest};
use actr_protocol::{
    ActorResult, ActrError, ActrId, DataChunk, Direction, MetadataEntry, PayloadType, RpcEnvelope,
    prost::Message as _,
};
use actrium_runtime::{
    AttachRequest, AttachResponse, BusinessRequest, DetachRequest, HeartbeatOutcome,
    HeartbeatRequest, LeaseMode, MessageKind, ProgramRelease, ResumeRequest, SessionSecret,
    TakeoverRequest, VerifiedChannel, attach_request_digest, resume_digest, resume_proof,
    sign_attach_request, takeover_signing_bytes, verify_attach_response, verify_request_proof,
};
use bytes::Bytes;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::service::{
    AttachmentResultCancellation, PendingCall, PendingCallGuard, Shared as PeerShared,
    TakeoverPayload, attachment_prepare_accepted, fresh_attachment_result_stream,
};
use super::{ATTACHMENT_CANCEL_ROUTE, metadata};
use crate::actr_ref::ActrRef;
use crate::outbound::PeerGate;
use crate::transport::{PeerTransport, RpcDeadline, VerifiedPeerContext};

const SESSION_RPC_TIMEOUT_MS: i64 = 30_000;
/// Resume runs before the next business request, which may race the verified
/// channel rebuild after a reconnect. Wait for that rebuild instead of failing
/// the request the moment the old channel disappears.
const CHANNEL_RECOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const CHANNEL_RECOVERY_POLL: Duration = Duration::from_millis(200);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Everything the attaching peer declares for one Attach attempt.
/// Authority-bearing identity fields come from the verified channel and the
/// node's own Actor key, never from the Guest.
#[derive(Clone)]
pub struct ActriumAttachParams {
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
    pub runtime: actrium_runtime::Runtime,
    pub requested_capabilities: Vec<String>,
}

/// Handles to the running node's outbound path plus the shared peer state.
pub(crate) struct PeerRuntime {
    gate: Arc<PeerGate>,
    transport: Arc<PeerTransport>,
    shared: Arc<PeerShared>,
}

impl PeerRuntime {
    pub(crate) fn new(
        gate: Arc<PeerGate>,
        transport: Arc<PeerTransport>,
        shared: Arc<PeerShared>,
    ) -> Self {
        Self {
            gate,
            transport,
            shared,
        }
    }

    /// Connect to the peer and return the verified channel view. Legacy wires
    /// never produce a context, so Attach cannot silently downgrade.
    async fn verified_context(&self, service: &ActrId) -> ActorResult<Arc<VerifiedPeerContext>> {
        let transport = self
            .transport
            .get_or_create_transport(&Dest::Peer(service.clone()))
            .await
            .map_err(|error| {
                ActrError::Unavailable(format!("Actrium peer connection failed: {error}"))
            })?;
        transport.verified_peer_context().await.ok_or_else(|| {
            ActrError::PermissionDenied(
                "the peer has no verified Actor channel; Actrium Attach requires mutual authentication"
                    .to_string(),
            )
        })
    }
}

#[derive(Clone)]
struct SessionState {
    session_id: String,
    lease_epoch: u64,
    secret: SessionSecret,
    channel: Arc<VerifiedPeerContext>,
    expires_at_epoch_secs: u64,
    recovery_required: bool,
}

/// One attached peer session. Clones share the same session state.
/// Dropping the last handle stops the heartbeat; the Service reclaims the
/// lease when it expires. Call [`ActriumPeerSession::detach`] to release it
/// immediately.
pub struct ActriumPeerSession {
    runtime: Arc<PeerRuntime>,
    service: ActrId,
    front_actor_id: String,
    service_actor_id: String,
    params: ActriumAttachParams,
    actor_signing_key: ed25519_dalek::SigningKey,
    state: Mutex<SessionState>,
    heartbeat: Mutex<Option<JoinHandle<()>>>,
}

impl ActriumPeerSession {
    pub async fn session_id(&self) -> String {
        self.state.lock().await.session_id.clone()
    }

    pub async fn lease_epoch(&self) -> u64 {
        self.state.lock().await.lease_epoch
    }

    fn envelope(method: &str, payload: Vec<u8>) -> ActorResult<RpcEnvelope> {
        Ok(RpcEnvelope {
            request_id: Uuid::new_v4().to_string(),
            route_key: format!("actrium.session.v1.SessionRegistry.{method}"),
            direction: Some(Direction::Request as i32),
            payload: Some(Bytes::from(payload)),
            timeout_ms: SESSION_RPC_TIMEOUT_MS,
            ..Default::default()
        })
    }

    async fn session_rpc<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        request: &Req,
        channel: &VerifiedPeerContext,
    ) -> ActorResult<Resp> {
        let payload = serde_json::to_vec(request)
            .map_err(|error| ActrError::Internal(format!("session encoding failed: {error}")))?;
        let response = self.session_rpc_bytes(method, payload, channel).await?;
        serde_json::from_slice(&response)
            .map_err(|error| ActrError::Internal(format!("invalid session response: {error}")))
    }

    async fn session_rpc_bytes(
        &self,
        method: &str,
        payload: Vec<u8>,
        channel: &VerifiedPeerContext,
    ) -> ActorResult<Bytes> {
        let envelope = Self::envelope(method, payload)?;
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        self.runtime
            .gate
            .send_request_on_verified_channel_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                deadline,
                channel,
            )
            .await
    }

    fn attach_request(&self, channel: &VerifiedPeerContext) -> AttachRequest {
        AttachRequest {
            attach_id: Uuid::new_v4().to_string(),
            front_actor_id: self.front_actor_id.clone(),
            service_actor_id: self.service_actor_id.clone(),
            channel_id: channel.channel_id.clone(),
            channel_transcript_digest: channel.transcript_digest.clone(),
            program: self.params.program.clone(),
            package_digest: self.params.package_digest.clone(),
            guest_digest: self.params.guest_digest.clone(),
            runtime: self.params.runtime.clone(),
            protocol_fingerprint: self.params.protocol_fingerprint.clone(),
            tab_session_nonce: rand::random::<[u8; 32]>().to_vec(),
            requested_lease: LeaseMode::Exclusive,
            requested_capabilities: self.params.requested_capabilities.clone(),
            challenge_response: Vec::new(),
        }
    }

    fn verify_response(
        &self,
        response: &AttachResponse,
        request_digest: &[u8],
        channel: &VerifiedPeerContext,
    ) -> ActorResult<SessionSecret> {
        verify_attach_response(response, request_digest, &channel.peer_actor_public_key)
            .map_err(|error| ActrError::PermissionDenied(error.to_string()))?;
        if response.service.actor_id != self.service_actor_id {
            return Err(ActrError::PermissionDenied(
                "AttachResponse names a different Service".to_string(),
            ));
        }
        let secret: [u8; 32] = response.session_secret.as_slice().try_into().map_err(|_| {
            ActrError::PermissionDenied("AttachResponse carries no valid session secret".into())
        })?;
        Ok(SessionSecret::from_bytes(secret))
    }

    fn response_expiry(response: &AttachResponse, channel: &VerifiedPeerContext) -> u64 {
        response
            .lease
            .expires_at_epoch_secs
            .min(response.privacy.expires_at_epoch_secs)
            .min(channel.peer_credential_expires_at_epoch_secs)
    }

    fn expired_session_error() -> ActrError {
        ActrError::PermissionDenied(
            "actrium.session/v1:SessionExpired:the local session requires a fresh Attach"
                .to_string(),
        )
    }

    fn ensure_usable(state: &SessionState, now: u64) -> ActorResult<()> {
        if state.recovery_required || now >= state.expires_at_epoch_secs {
            Err(Self::expired_session_error())
        } else {
            Ok(())
        }
    }

    /// A rebuilt peer transport becomes usable asynchronously. Resume waits
    /// for that verified channel (bounded) so the request that triggered
    /// recovery is not rejected by the handshake still being in flight.
    async fn channel_after_reconnect(&self) -> ActorResult<Arc<VerifiedPeerContext>> {
        let deadline = tokio::time::Instant::now() + CHANNEL_RECOVERY_TIMEOUT;
        loop {
            match self.runtime.verified_context(&self.service).await {
                Ok(channel) => return Ok(channel),
                Err(error)
                    if Self::channel_is_recovering(&error)
                        && tokio::time::Instant::now() < deadline =>
                {
                    crate::timer::sleep(
                        crate::timer::ids::ACTRIUM_SESSION_CHANNEL_RECOVERY,
                        CHANNEL_RECOVERY_POLL,
                    )
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn channel_is_recovering(error: &ActrError) -> bool {
        match error {
            ActrError::Unavailable(_) => true,
            ActrError::PermissionDenied(message) => message.contains("no verified Actor channel"),
            _ => false,
        }
    }

    async fn start_heartbeat(self: &Arc<Self>, interval_ms: u32) {
        let interval = Duration::from_millis(u64::from(interval_ms.max(1)));
        let weak = Arc::downgrade(self);
        let handle = tokio::spawn(async move {
            let mut interval =
                crate::timer::interval(crate::timer::ids::ACTRIUM_SESSION_HEARTBEAT, interval);
            // The first tick fires immediately; skip it so the lease is
            // renewed on the Service-granted cadence, not at Attach time.
            interval.tick().await;
            loop {
                interval.tick().await;
                let Some(session) = weak.upgrade() else {
                    break;
                };
                let (request, channel) = {
                    let state = session.state.lock().await;
                    (
                        HeartbeatRequest {
                            session_id: state.session_id.clone(),
                            lease_epoch: state.lease_epoch,
                        },
                        state.channel.clone(),
                    )
                };
                match session
                    .session_rpc::<_, HeartbeatOutcome>("Heartbeat", &request, &channel)
                    .await
                {
                    Ok(outcome) => {
                        let mut state = session.state.lock().await;
                        if state.session_id == request.session_id
                            && state.lease_epoch == outcome.lease_epoch
                        {
                            state.expires_at_epoch_secs = outcome
                                .lease_expires_at_epoch_secs
                                .min(outcome.credential_expires_at_epoch_secs);
                        }
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "Actrium session heartbeat failed");
                        if message.contains("SessionExpired")
                            || message.contains("SessionEpochMismatch")
                            || message.contains("ChannelExpired")
                        {
                            let mut state = session.state.lock().await;
                            if state.session_id == request.session_id
                                && state.lease_epoch == request.lease_epoch
                            {
                                state.recovery_required = true;
                            }
                            break;
                        }
                    }
                }
            }
        });
        *self.heartbeat.lock().await = Some(handle);
    }

    async fn business_envelope(
        &self,
        route: &str,
        capability: &str,
        kind: MessageKind,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> ActorResult<(RpcEnvelope, PendingCall, Arc<VerifiedPeerContext>)> {
        let state = self.state.lock().await.clone();
        let now = now_secs();
        Self::ensure_usable(&state, now)?;
        let mut request = BusinessRequest {
            session_id: state.session_id,
            lease_epoch: state.lease_epoch,
            service_actor_id: self.service_actor_id.clone(),
            channel_id: state.channel.channel_id.clone(),
            route: route.to_string(),
            capability: capability.to_string(),
            request_id: Uuid::new_v4().to_string(),
            issued_at: now,
            deadline: now.saturating_add(timeout.as_secs().max(1)),
            kind,
            idempotency_key: None,
            payload: payload.clone(),
            proof: Vec::new(),
        };
        request.sign(&state.secret);
        let metadata = business_metadata(&request);
        let pending = PendingCall {
            request: request.clone(),
            secret: state.secret,
        };
        Ok((
            RpcEnvelope {
                request_id: request.request_id,
                route_key: route.to_string(),
                direction: Some(match kind {
                    MessageKind::Request => Direction::Request as i32,
                    MessageKind::Tell => Direction::Tell as i32,
                    _ => unreachable!("only Request and Tell use business_envelope"),
                }),
                payload: Some(Bytes::from(payload)),
                timeout_ms: match kind {
                    MessageKind::Request => timeout.as_millis().min(i64::MAX as u128) as i64,
                    _ => 0,
                },
                metadata,
                ..Default::default()
            },
            pending,
            state.channel,
        ))
    }

    /// Call a peer Service route with a session-signed request. The response
    /// proof is verified by the shared inbound guard before this future resolves.
    pub async fn call(
        self: &Arc<Self>,
        route: &str,
        capability: &str,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> ActorResult<Vec<u8>> {
        let (result_stream, transport_stream, payload) =
            match fresh_attachment_result_stream(route, &payload)
                .map_err(|error| error.into_actr_error())?
            {
                Some((result_stream, transport_stream, payload)) => {
                    (Some(result_stream), Some(transport_stream), payload)
                }
                None => (None, None, payload),
            };
        let (envelope, pending, channel) = self
            .business_envelope(route, capability, MessageKind::Request, payload, timeout)
            .await?;
        let request_id = envelope.request_id.clone();
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        if let (Some(stream_id), Some(transport_stream_id)) =
            (result_stream.as_ref(), transport_stream)
        {
            self.runtime
                .shared
                .register_peer_attachment_result(
                    stream_id.clone(),
                    transport_stream_id,
                    request_id.clone(),
                    capability.to_string(),
                    self,
                )
                .map_err(|error| error.into_actr_error())?;
        }
        let preserve_result = result_stream.is_some();
        let pending = PendingCallGuard::new(self.runtime.shared.clone(), request_id, pending)
            .with_attachment_result(result_stream);
        let result = self
            .runtime
            .gate
            .send_request_on_verified_channel_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                deadline,
                &channel,
            )
            .await
            .map(|bytes| bytes.to_vec());
        if preserve_result
            && result
                .as_ref()
                .is_ok_and(|bytes| attachment_prepare_accepted(bytes))
        {
            let _pending = pending.preserve_attachment_result();
        }
        result
    }

    /// Fire-and-forget tell with a session-signed request. No response proof
    /// exists for tells; the Service enforces the same Guard admission.
    pub async fn tell(&self, route: &str, capability: &str, payload: Vec<u8>) -> ActorResult<()> {
        let (envelope, _pending, channel) = self
            .business_envelope(
                route,
                capability,
                MessageKind::Tell,
                payload,
                Duration::from_secs(1),
            )
            .await?;
        self.runtime
            .gate
            .send_tell_on_verified_channel_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                RpcDeadline::from_duration(Duration::from_secs(30)),
                &channel,
            )
            .await
    }

    /// Cancel one accepted Prepare on the Service. This signed Host RPC never
    /// enters the Guest, and its acknowledgement uses the normal response proof.
    async fn cancel_attachment_result(
        &self,
        stream_id: &str,
        prepare_request_id: String,
        capability: &str,
    ) -> ActorResult<()> {
        let payload = serde_json::to_vec(&AttachmentResultCancellation {
            stream_id: stream_id.to_string(),
            prepare_request_id,
        })
        .map_err(|error| ActrError::Internal(format!("session encoding failed: {error}")))?;
        let (envelope, pending_call, channel) = self
            .business_envelope(
                ATTACHMENT_CANCEL_ROUTE,
                capability,
                MessageKind::Request,
                payload,
                Duration::from_secs(30),
            )
            .await?;
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        let pending = PendingCallGuard::new(
            self.runtime.shared.clone(),
            envelope.request_id.clone(),
            pending_call,
        );
        let result = self
            .runtime
            .gate
            .send_request_on_verified_channel_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                deadline,
                &channel,
            )
            .await;
        drop(pending);
        result.map(|_| ())
    }

    /// Send a stream chunk to this session's Service. Session metadata is
    /// generated by the Host and replaces any caller-supplied proof fields;
    /// the signed route and sequence are the same values checked by the Guard.
    pub async fn send_data_chunk(
        &self,
        route: &str,
        capability: &str,
        mut chunk: DataChunk,
        payload_type: PayloadType,
        timeout: Duration,
    ) -> ActorResult<()> {
        let state = self.state.lock().await.clone();
        let now = now_secs();
        Self::ensure_usable(&state, now)?;
        let request_id = format!("{}:{}", chunk.stream_id, chunk.sequence);
        let mut request = BusinessRequest {
            session_id: state.session_id,
            lease_epoch: state.lease_epoch,
            service_actor_id: self.service_actor_id.clone(),
            channel_id: state.channel.channel_id.clone(),
            route: route.to_string(),
            capability: capability.to_string(),
            request_id,
            issued_at: now,
            deadline: now.saturating_add(timeout.as_secs().max(1)),
            kind: MessageKind::StreamChunk,
            idempotency_key: None,
            payload: chunk.payload.to_vec(),
            proof: Vec::new(),
        };
        request.sign(&state.secret);

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
        chunk.metadata.extend(business_metadata(&request));
        chunk.metadata.push(MetadataEntry {
            key: metadata::ROUTE.to_string(),
            value: route.to_string(),
        });

        let stream_id = chunk.stream_id.clone();
        let operation_id = format!("{stream_id}#{}", chunk.sequence);
        self.runtime
            .gate
            .send_data_chunk_on_verified_channel_until(
                &self.service,
                payload_type,
                &stream_id,
                Bytes::from(chunk.encode_to_vec()),
                RpcDeadline::from_duration(timeout),
                &operation_id,
                &state.channel,
            )
            .await
    }

    /// Verify a Service-originated terminal attachment result against the live
    /// outbound session. The shared Guard holds only a Weak handle to self, so
    /// the secret and channel generation are never copied into a second cache.
    pub(crate) async fn verify_incoming_stream(
        &self,
        request: &BusinessRequest,
        channel: &VerifiedChannel,
    ) -> ActorResult<()> {
        let now = now_secs();
        let state = self.state.lock().await.clone();
        Self::ensure_usable(&state, now)?;
        let valid = request.session_id == state.session_id
            && request.lease_epoch == state.lease_epoch
            && request.service_actor_id == self.service_actor_id
            && request.channel_id == state.channel.channel_id
            && channel.channel_id == state.channel.channel_id
            && channel.peer_actor_id == self.service_actor_id
            && request.route == super::ATTACHMENT_UPLOAD_TRANSPORT
            && request.kind == MessageKind::StreamChunk
            && request.issued_at <= now.saturating_add(60)
            && request.deadline >= now
            && verify_request_proof(state.secret.as_bytes(), request, &request.proof);
        if valid {
            Ok(())
        } else {
            Err(ActrError::PermissionDenied(
                "the attachment result does not match the current outbound session".to_string(),
            ))
        }
    }

    /// Resume the lease over a freshly authenticated channel after a
    /// reconnect. No-op when the current channel generation is unchanged.
    pub async fn resume(&self) -> ActorResult<()> {
        let previous = self.state.lock().await.clone();
        Self::ensure_usable(&previous, now_secs())?;
        let channel = self.channel_after_reconnect().await?;
        if previous.channel.channel_id == channel.channel_id {
            return Ok(());
        }
        let mut request = ResumeRequest {
            session_id: previous.session_id,
            lease_epoch: previous.lease_epoch,
            channel_id: channel.channel_id.clone(),
            channel_transcript_digest: channel.transcript_digest.clone(),
            session_proof: Vec::new(),
        };
        request.session_proof = resume_proof(previous.secret.as_bytes(), &request);
        let response: AttachResponse = self.session_rpc("Resume", &request, &channel).await?;
        let secret = self.verify_response(&response, &resume_digest(&request), &channel)?;
        *self.state.lock().await = SessionState {
            session_id: response.session_id.clone(),
            lease_epoch: response.lease_epoch,
            secret,
            channel: channel.clone(),
            expires_at_epoch_secs: Self::response_expiry(&response, &channel),
            recovery_required: false,
        };
        Ok(())
    }

    /// Take over another peer's exclusive lease. The caller must be
    /// authorized by the Service's takeover policy; the proof binds the old
    /// lease to the complete new Attach request.
    pub async fn takeover(self: &Arc<Self>, session_id: &str, lease_epoch: u64) -> ActorResult<()> {
        let channel = self.runtime.verified_context(&self.service).await?;
        let mut attach = self.attach_request(&channel);
        sign_attach_request(&mut attach, &self.actor_signing_key);
        let request_digest = attach_request_digest(&attach);
        let mut takeover = TakeoverRequest {
            session_id: session_id.to_string(),
            lease_epoch,
            requesting_front_actor_id: self.front_actor_id.clone(),
            takeover_proof: Vec::new(),
        };
        use ed25519_dalek::Signer as _;
        takeover.takeover_proof = self
            .actor_signing_key
            .sign(&takeover_signing_bytes(&takeover, &attach))
            .to_bytes()
            .to_vec();
        let response: AttachResponse = self
            .session_rpc("Takeover", &TakeoverPayload { takeover, attach }, &channel)
            .await?;
        let secret = self.verify_response(&response, &request_digest, &channel)?;
        *self.state.lock().await = SessionState {
            session_id: response.session_id.clone(),
            lease_epoch: response.lease_epoch,
            secret,
            channel: channel.clone(),
            expires_at_epoch_secs: Self::response_expiry(&response, &channel),
            recovery_required: false,
        };
        self.restart_heartbeat(response.lease.heartbeat_interval_ms)
            .await;
        Ok(())
    }

    async fn restart_heartbeat(self: &Arc<Self>, interval_ms: u32) {
        if let Some(handle) = self.heartbeat.lock().await.take() {
            handle.abort();
        }
        self.start_heartbeat(interval_ms).await;
    }

    /// Release the lease immediately and stop the heartbeat.
    pub async fn detach(&self) -> ActorResult<()> {
        let (request, channel) = {
            let state = self.state.lock().await;
            (
                DetachRequest {
                    session_id: state.session_id.clone(),
                    lease_epoch: state.lease_epoch,
                },
                state.channel.clone(),
            )
        };
        let payload = serde_json::to_vec(&request)
            .map_err(|error| ActrError::Internal(format!("session encoding failed: {error}")))?;
        self.session_rpc_bytes("Detach", payload, &channel).await?;
        self.state.lock().await.recovery_required = true;
        if let Some(handle) = self.heartbeat.lock().await.take() {
            handle.abort();
        }
        self.runtime.shared.drop_peer_attachment_results(self);
        Ok(())
    }
}

impl ActrRef {
    /// Unregister a local stream and cancel the matching Actrium attachment
    /// result reservation on both peers. Explicit upload cancellation waits for
    /// the Service acknowledgement so the stream can immediately be prepared again.
    pub async fn unregister_stream(&self, stream_id: &str) -> ActorResult<()> {
        // Remove the old callback before releasing its reservation. A retry may
        // register the same caller-visible stream while the Host cancellation is
        // in flight, but this call must never unregister that replacement.
        let unregistered = self.app_context().await.unregister_stream(stream_id).await;
        let cancellation = if let Some(runtime) = self.shared.peer_runtime.as_ref()
            && let Some(reservation) = runtime.shared.take_peer_attachment_result(stream_id)
            && let Some(session) = reservation.session.upgrade()
        {
            session
                .cancel_attachment_result(
                    &reservation.transport_stream_id,
                    reservation.request_id,
                    &reservation.capability,
                )
                .await
        } else {
            Ok(())
        };
        cancellation.and(unregistered)
    }

    /// Connect to a peer Service over the verified Actor channel and Attach.
    /// Requires `Node<Attached>::with_actrium_service` before registration.
    pub async fn attach_actrium(
        &self,
        service: &ActrId,
        params: ActriumAttachParams,
    ) -> ActorResult<Arc<ActriumPeerSession>> {
        let runtime = self.shared.peer_runtime.clone().ok_or_else(|| {
            ActrError::InvalidArgument(
                "the node is not configured for Actrium sessions; call with_actrium_service before registration"
                    .to_string(),
            )
        })?;
        let channel = runtime.verified_context(service).await?;
        let actor_signing_key =
            ed25519_dalek::SigningKey::from_bytes(&self.shared.actor_signer.signing_key_bytes());
        let session = Arc::new(ActriumPeerSession {
            runtime,
            service: service.clone(),
            front_actor_id: self.actor_id().to_string_repr(),
            service_actor_id: service.to_string_repr(),
            params,
            actor_signing_key,
            state: Mutex::new(SessionState {
                session_id: String::new(),
                lease_epoch: 0,
                secret: SessionSecret::from_bytes([0; 32]),
                channel: channel.clone(),
                expires_at_epoch_secs: u64::MAX,
                recovery_required: false,
            }),
            heartbeat: Mutex::new(None),
        });
        let mut attach = session.attach_request(&channel);
        sign_attach_request(&mut attach, &session.actor_signing_key);
        let response: AttachResponse = session.session_rpc("Attach", &attach, &channel).await?;
        let secret =
            session.verify_response(&response, &attach_request_digest(&attach), &channel)?;
        *session.state.lock().await = SessionState {
            session_id: response.session_id.clone(),
            lease_epoch: response.lease_epoch,
            secret,
            channel: channel.clone(),
            expires_at_epoch_secs: ActriumPeerSession::response_expiry(&response, &channel),
            recovery_required: false,
        };
        session
            .start_heartbeat(response.lease.heartbeat_interval_ms)
            .await;
        Ok(session)
    }
}

pub(crate) fn business_metadata(request: &BusinessRequest) -> Vec<MetadataEntry> {
    let mut entries = vec![
        (metadata::SESSION_ID, request.session_id.clone()),
        (metadata::LEASE_EPOCH, request.lease_epoch.to_string()),
        (metadata::SERVICE_ACTOR_ID, request.service_actor_id.clone()),
        (metadata::CHANNEL_ID, request.channel_id.clone()),
        (metadata::CAPABILITY, request.capability.clone()),
        (metadata::ISSUED_AT, request.issued_at.to_string()),
        (metadata::DEADLINE, request.deadline.to_string()),
        (metadata::PROOF, hex::encode(&request.proof)),
    ]
    .into_iter()
    .map(|(key, value)| MetadataEntry {
        key: key.to_string(),
        value,
    })
    .collect::<Vec<_>>();
    if let Some(key) = &request.idempotency_key {
        entries.push(MetadataEntry {
            key: metadata::IDEMPOTENCY_KEY.to_string(),
            value: key.clone(),
        });
    }
    entries
}
