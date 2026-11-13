//! Peer-side Actrium session client: connect over the verified Actor
//! channel, Attach, sign business calls, and maintain the lease.
//!
//! In the P2P model every node that participates in Actrium sessions also
//! hosts an `InMemorySessionService`; any such node can initiate an Attach to
//! another peer and become a caller. [`ActrRef::attach_actrium`] performs the
//! Attach exchange and returns an [`ActriumFrontSession`] whose calls carry
//! session metadata and HMAC proofs. Responses are verified by the shared
//! inbound guard before the pending caller is woken.
//!
//! v1 scope: a peer is both a Service (it receives Attach and runs a Guard)
//! and a caller (it can Attach to other peers). Service-originated business
//! calls and streams toward a caller are handled by the Service side, not
//! this module.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_framework::Dest;
use actr_protocol::{
    ActorResult, ActrError, ActrId, Direction, MetadataEntry, PayloadType, RpcEnvelope,
};
use actrium_runtime::{
    AttachRequest, AttachResponse, BusinessRequest, DetachRequest, HeartbeatOutcome,
    HeartbeatRequest, LeaseMode, MessageKind, ProgramRelease, ResumeRequest, SessionSecret,
    TakeoverRequest, attach_request_digest, resume_digest, resume_proof, sign_attach_request,
    takeover_signing_bytes, verify_attach_response,
};
use bytes::Bytes;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::metadata;
use super::service::{PendingCall, Shared as PeerShared, TakeoverPayload};
use crate::actr_ref::ActrRef;
use crate::outbound::PeerGate;
use crate::transport::{PeerTransport, RpcDeadline, VerifiedPeerContext};

const SESSION_RPC_TIMEOUT_MS: i64 = 30_000;

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
    /// The attaching peer's long-term Ed25519 key, matching the key bound at
    /// registration and proven during the relay handshake.
    pub actor_signing_key: ed25519_dalek::SigningKey,
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
}

/// One attached Front–Service lease. Clones share the same session state.
/// Dropping the last handle stops the heartbeat; the Service reclaims the
/// lease when it expires. Call [`ActriumFrontSession::detach`] to release it
/// immediately.
pub struct ActriumFrontSession {
    runtime: Arc<PeerRuntime>,
    service: ActrId,
    front_actor_id: String,
    service_actor_id: String,
    params: ActriumAttachParams,
    state: Mutex<SessionState>,
    heartbeat: Mutex<Option<JoinHandle<()>>>,
}

impl ActriumFrontSession {
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
    ) -> ActorResult<Resp> {
        let payload = serde_json::to_vec(request)
            .map_err(|error| ActrError::Internal(format!("session encoding failed: {error}")))?;
        let response = self.session_rpc_bytes(method, payload).await?;
        serde_json::from_slice(&response)
            .map_err(|error| ActrError::Internal(format!("invalid session response: {error}")))
    }

    async fn session_rpc_bytes(&self, method: &str, payload: Vec<u8>) -> ActorResult<Bytes> {
        let envelope = Self::envelope(method, payload)?;
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        self.runtime
            .gate
            .send_request_with_type_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                deadline,
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
                let request = {
                    let state = session.state.lock().await;
                    HeartbeatRequest {
                        session_id: state.session_id.clone(),
                        lease_epoch: state.lease_epoch,
                    }
                };
                match session
                    .session_rpc::<_, HeartbeatOutcome>("Heartbeat", &request)
                    .await
                {
                    Ok(_) => {}
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "Actrium session heartbeat failed");
                        if message.contains("SessionExpired")
                            || message.contains("SessionEpochMismatch")
                        {
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
    ) -> ActorResult<(RpcEnvelope, PendingCall)> {
        let state = self.state.lock().await.clone();
        let now = now_secs();
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
        ))
    }

    /// Call a Service route with a session-signed request. The response proof
    /// is verified by the Front guard before this future resolves.
    pub async fn call(
        &self,
        route: &str,
        capability: &str,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> ActorResult<Vec<u8>> {
        let (envelope, pending) = self
            .business_envelope(route, capability, MessageKind::Request, payload, timeout)
            .await?;
        let request_id = envelope.request_id.clone();
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        self.runtime
            .shared
            .pending
            .lock()
            .await
            .insert(request_id.clone(), pending);
        let result = self
            .runtime
            .gate
            .send_request_with_type_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                deadline,
            )
            .await;
        if result.is_err() {
            self.runtime.shared.pending.lock().await.remove(&request_id);
        }
        result.map(|bytes| bytes.to_vec())
    }

    /// Fire-and-forget tell with a session-signed request. No response proof
    /// exists for tells; the Service enforces the same Guard admission.
    pub async fn tell(&self, route: &str, capability: &str, payload: Vec<u8>) -> ActorResult<()> {
        let (envelope, _pending) = self
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
            .send_tell_with_type_until(
                &self.service,
                PayloadType::RpcReliable,
                envelope,
                RpcDeadline::from_duration(Duration::from_secs(30)),
            )
            .await
    }

    /// Resume the lease over a freshly authenticated channel after a
    /// reconnect. No-op when the current channel generation is unchanged.
    pub async fn resume(&self) -> ActorResult<()> {
        let channel = self.runtime.verified_context(&self.service).await?;
        let previous = self.state.lock().await.clone();
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
        let response: AttachResponse = self.session_rpc("Resume", &request).await?;
        let secret = self.verify_response(&response, &resume_digest(&request), &channel)?;
        *self.state.lock().await = SessionState {
            session_id: response.session_id,
            lease_epoch: response.lease_epoch,
            secret,
            channel,
        };
        Ok(())
    }

    /// Take over another Front's exclusive lease. The caller must be
    /// authorized by the Service's takeover policy; the proof binds the old
    /// lease to the complete new Attach request.
    pub async fn takeover(self: &Arc<Self>, session_id: &str, lease_epoch: u64) -> ActorResult<()> {
        let channel = self.runtime.verified_context(&self.service).await?;
        let mut attach = self.attach_request(&channel);
        sign_attach_request(&mut attach, &self.params.actor_signing_key);
        let request_digest = attach_request_digest(&attach);
        let mut takeover = TakeoverRequest {
            session_id: session_id.to_string(),
            lease_epoch,
            requesting_front_actor_id: self.front_actor_id.clone(),
            takeover_proof: Vec::new(),
        };
        use ed25519_dalek::Signer as _;
        takeover.takeover_proof = self
            .params
            .actor_signing_key
            .sign(&takeover_signing_bytes(&takeover, &attach))
            .to_bytes()
            .to_vec();
        let response: AttachResponse = self
            .session_rpc("Takeover", &TakeoverPayload { takeover, attach })
            .await?;
        let secret = self.verify_response(&response, &request_digest, &channel)?;
        *self.state.lock().await = SessionState {
            session_id: response.session_id.clone(),
            lease_epoch: response.lease_epoch,
            secret,
            channel,
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
        let request = {
            let state = self.state.lock().await;
            DetachRequest {
                session_id: state.session_id.clone(),
                lease_epoch: state.lease_epoch,
            }
        };
        let payload = serde_json::to_vec(&request)
            .map_err(|error| ActrError::Internal(format!("session encoding failed: {error}")))?;
        self.session_rpc_bytes("Detach", payload).await?;
        if let Some(handle) = self.heartbeat.lock().await.take() {
            handle.abort();
        }
        Ok(())
    }
}

impl ActrRef {
    /// Connect to a peer Service over the verified Actor channel and Attach.
    /// Requires `Node<Attached>::with_actrium_service` before registration.
    pub async fn attach_actrium(
        &self,
        service: &ActrId,
        params: ActriumAttachParams,
    ) -> ActorResult<Arc<ActriumFrontSession>> {
        let runtime = self.shared.peer_runtime.clone().ok_or_else(|| {
            ActrError::InvalidArgument(
                "the node is not configured for Actrium sessions; call with_actrium_service before registration"
                    .to_string(),
            )
        })?;
        let channel = runtime.verified_context(service).await?;
        let session = Arc::new(ActriumFrontSession {
            runtime,
            service: service.clone(),
            front_actor_id: self.actor_id().to_string_repr(),
            service_actor_id: service.to_string_repr(),
            params,
            state: Mutex::new(SessionState {
                session_id: String::new(),
                lease_epoch: 0,
                secret: SessionSecret::from_bytes([0; 32]),
                channel: channel.clone(),
            }),
            heartbeat: Mutex::new(None),
        });
        let mut attach = session.attach_request(&channel);
        sign_attach_request(&mut attach, &session.params.actor_signing_key);
        let response: AttachResponse = session.session_rpc("Attach", &attach).await?;
        let secret =
            session.verify_response(&response, &attach_request_digest(&attach), &channel)?;
        *session.state.lock().await = SessionState {
            session_id: response.session_id,
            lease_epoch: response.lease_epoch,
            secret,
            channel,
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
