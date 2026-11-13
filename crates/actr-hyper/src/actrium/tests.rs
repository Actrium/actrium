//! End-to-end adapter tests: verified-channel Attach, business admission,
//! dequeue recheck, response signing, and caller-side response verification.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use actr_protocol::{ActrId, ActrType, Direction, MetadataEntry, RpcEnvelope};
use actrium_runtime::{
    AttachAuthorization, AttachQuotaContext, AttachRequest, AttachResponse, AttachmentPolicy,
    AuthorizeAttachDecision, BusinessRequest, DetachRequest, GetStatusRequest, HeartbeatRequest,
    InMemorySessionService, MemberRole, MessageKind, Principal, PrivacyContext, PrivacyDecision,
    PrivacyState, ProgramRelease, QuotaDecision, RequestAuthorizationDecision, RequestContext,
    RequestQuotaContext, ServiceConfig, ServicePolicies, SessionError, SessionSecret,
    SessionStatus, TakeoverContext, attach_request_digest, request_proof, sign_attach_request,
    verify_attach_response, verify_request_proof,
};
use async_trait::async_trait;
use bytes::Bytes;
use ed25519_dalek::SigningKey;

use super::metadata;
use super::peer::business_metadata;
use super::service::{ActriumServiceRegistry, PendingCall, Shared as PeerShared};
use crate::lifecycle::{
    IncomingDispatchRequest, IncomingGuard, IncomingStreamChunk, SessionRegistry,
};
use crate::transport::VerifiedPeerContext;

const READ: &str = "documents.read";
const ROUTE: &str = "documents.Documents.Read";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn front_id() -> ActrId {
    ActrId {
        realm: "test@example.com".into(),
        serial_number: 1,
    }
}

fn service_id() -> ActrId {
    ActrId {
        realm: "test@example.com".into(),
        serial_number: 2,
    }
}

fn front_type() -> ActrType {
    ActrType {
        publisher: "example.com".into(),
        name: "front".into(),
        version: "1.0.0".into(),
    }
}

fn front_key() -> SigningKey {
    SigningKey::from_bytes(&[11; 32])
}

fn service_key() -> SigningKey {
    SigningKey::from_bytes(&[22; 32])
}

fn peer_context(local: &ActrId, peer: &ActrId, peer_key: &SigningKey) -> Arc<VerifiedPeerContext> {
    Arc::new(VerifiedPeerContext {
        local_actor_id: local.clone(),
        peer_actor_id: peer.clone(),
        peer_actor_type: front_type(),
        peer_member: "member-1".into(),
        peer_client_key: "key-1".into(),
        peer_role: "owner".into(),
        peer_actor_public_key: peer_key.verifying_key().to_bytes().to_vec(),
        channel_id: "channel-1".into(),
        channel_generation: 1,
        transcript_digest: vec![9; 32],
        local_runtime_instance_id: "runtime-1".into(),
        peer_runtime_instance_id: "front-runtime-1".into(),
        peer_credential_expires_at_epoch_secs: now() + 3600,
        current_policy_binding: vec![7; 32],
        current_state_binding: vec![8; 32],
    })
}

fn service_context() -> Arc<VerifiedPeerContext> {
    peer_context(&service_id(), &front_id(), &front_key())
}

fn front_context() -> Arc<VerifiedPeerContext> {
    peer_context(&front_id(), &service_id(), &service_key())
}

struct TestPolicies;

#[async_trait]
impl actrium_runtime::MembershipProvider for TestPolicies {
    async fn principal(
        &self,
        _front_actor_id: &str,
        member: &str,
        client_key: &str,
    ) -> Result<Option<Principal>, SessionError> {
        Ok(Some(Principal {
            member: member.to_string(),
            client_key: client_key.to_string(),
            role: MemberRole::Owner,
            capabilities: BTreeSet::from([READ.to_string()]),
        }))
    }
}

#[async_trait]
impl actrium_runtime::AuthorizeAttach for TestPolicies {
    async fn authorize_attach(
        &self,
        context: AttachAuthorization,
    ) -> Result<AuthorizeAttachDecision, SessionError> {
        Ok(AuthorizeAttachDecision {
            allowed: true,
            denial_reason: None,
            allowed_capabilities: context.requested_capabilities,
        })
    }
}

#[async_trait]
impl actrium_runtime::AuthorizeTakeover for TestPolicies {
    async fn authorize_takeover(&self, _context: TakeoverContext) -> Result<bool, SessionError> {
        Ok(true)
    }
}

#[async_trait]
impl actrium_runtime::AuthorizeRequest for TestPolicies {
    async fn authorize_request(
        &self,
        _context: RequestContext,
    ) -> Result<RequestAuthorizationDecision, SessionError> {
        Ok(RequestAuthorizationDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

#[async_trait]
impl actrium_runtime::PrivacyPolicy for TestPolicies {
    async fn evaluate(&self, context: PrivacyContext) -> Result<PrivacyDecision, SessionError> {
        Ok(PrivacyDecision {
            state: PrivacyState {
                actor_state_digest: vec![1; 32],
                application_state_digest: vec![2; 32],
                assurance_digest: vec![3; 32],
                expires_at_epoch_secs: now() + 3600,
            },
            allowed_capabilities: context.requested_capabilities,
        })
    }
}

#[async_trait]
impl actrium_runtime::ResourceQuota for TestPolicies {
    async fn check_attach(
        &self,
        _context: AttachQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        Ok(QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }

    async fn check_request(
        &self,
        _context: RequestQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        Ok(QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

fn service() -> InMemorySessionService {
    let config = ServiceConfig {
        service_actor_id: service_id().to_string_repr(),
        runtime_instance_id: "runtime-1".into(),
        service_signing_key: service_key(),
        allowed_front_actor_types: BTreeSet::from([front_type().to_string_repr()]),
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: vec![4; 32],
        guest_digest: vec![5; 32],
        protocol_fingerprint: vec![6; 32],
        supported_runtimes: vec![actrium_runtime::Runtime {
            actrium_version: "1".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        }],
        program_capabilities: BTreeSet::from([READ.to_string()]),
        routes: BTreeMap::from([(ROUTE.to_string(), BTreeSet::from([READ.to_string()]))]),
        attachment_policy: AttachmentPolicy::Exclusive,
        lease_duration: std::time::Duration::from_secs(60),
        heartbeat_interval: std::time::Duration::from_secs(5),
        grace_period: std::time::Duration::from_secs(30),
        replay_retention: ServiceConfig::DEFAULT_REPLAY_RETENTION,
        max_replay_entries: ServiceConfig::DEFAULT_MAX_REPLAY_ENTRIES,
        max_completed_sessions: ServiceConfig::DEFAULT_MAX_COMPLETED_SESSIONS,
    };
    let policies = ServicePolicies {
        membership: Arc::new(TestPolicies),
        authorize_attach: Arc::new(TestPolicies),
        authorize_takeover: Arc::new(TestPolicies),
        authorize_request: Arc::new(TestPolicies),
        privacy: Arc::new(TestPolicies),
        quota: Arc::new(TestPolicies),
    };
    InMemorySessionService::new(config, policies).expect("valid test config")
}

fn attach_request(channel: &VerifiedPeerContext) -> AttachRequest {
    AttachRequest {
        attach_id: "attach-1".into(),
        front_actor_id: front_id().to_string_repr(),
        service_actor_id: service_id().to_string_repr(),
        channel_id: channel.channel_id.clone(),
        channel_transcript_digest: channel.transcript_digest.clone(),
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: vec![4; 32],
        guest_digest: vec![5; 32],
        runtime: actrium_runtime::Runtime {
            actrium_version: "1".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        },
        protocol_fingerprint: vec![6; 32],
        tab_session_nonce: vec![42; 32],
        requested_lease: actrium_runtime::LeaseMode::Exclusive,
        requested_capabilities: vec![READ.to_string()],
        challenge_response: Vec::new(),
    }
}

fn request_envelope(
    route: &str,
    request_id: &str,
    payload: Vec<u8>,
    metadata: Vec<MetadataEntry>,
) -> RpcEnvelope {
    RpcEnvelope {
        request_id: request_id.into(),
        route_key: route.into(),
        direction: Some(Direction::Request as i32),
        payload: Some(Bytes::from(payload)),
        timeout_ms: 30_000,
        metadata,
        ..Default::default()
    }
}

fn dispatch_request<'a>(
    envelope: &'a RpcEnvelope,
    context: Option<Arc<VerifiedPeerContext>>,
) -> IncomingDispatchRequest<'a> {
    let front = front_id();
    let caller = Box::leak(Box::new(front));
    let actor_type = Box::leak(Box::new(front_type()));
    IncomingDispatchRequest::new(envelope, Some((caller, Some(actor_type))))
        .with_peer_context(context)
}

async fn attach(registry: &ActriumServiceRegistry) -> (AttachResponse, SessionSecret) {
    let channel = front_context();
    let mut request = attach_request(&channel);
    sign_attach_request(&mut request, &front_key());
    let digest = attach_request_digest(&request);
    let envelope = request_envelope(
        "actrium.session.v1.SessionRegistry.Attach",
        "attach-rpc-1",
        serde_json::to_vec(&request).unwrap(),
        vec![],
    );
    let response = registry
        .dispatch(
            crate::lifecycle::SessionMethod::Attach,
            dispatch_request(&envelope, Some(service_context())),
        )
        .await
        .expect("Attach succeeds");
    let response: AttachResponse = serde_json::from_slice(&response).unwrap();
    verify_attach_response(
        &response,
        &digest,
        &service_key().verifying_key().to_bytes(),
    )
    .expect("Front verifies the Service proof");
    let secret: [u8; 32] = response.session_secret.as_slice().try_into().unwrap();
    (response, SessionSecret::from_bytes(secret))
}

fn business_call(response: &AttachResponse, secret: &SessionSecret) -> RpcEnvelope {
    let mut request = BusinessRequest {
        session_id: response.session_id.clone(),
        lease_epoch: response.lease_epoch,
        service_actor_id: service_id().to_string_repr(),
        channel_id: "channel-1".into(),
        route: ROUTE.into(),
        capability: READ.into(),
        request_id: "call-1".into(),
        issued_at: now(),
        deadline: now() + 30,
        kind: MessageKind::Request,
        idempotency_key: None,
        payload: b"read document".to_vec(),
        proof: Vec::new(),
    };
    request.sign(secret);
    let metadata = business_metadata(&request);
    request_envelope(ROUTE, "call-1", request.payload.clone(), metadata)
}

#[tokio::test]
async fn attach_business_call_and_response_proof_round_trip() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;
    assert!(attach.session_id.len() > 8);
    assert_eq!(attach.session_capability_ceiling, vec![READ.to_string()]);

    // Business request passes the Guard at arrival and again at dequeue.
    let call = business_call(&attach, &secret);
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect("arrival admission");
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect("identical dequeue recheck");

    // The Service signs the actual response payload; the caller verifies it.
    let mut response = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::from_static(b"document body")),
        ..Default::default()
    };
    guard.authenticate_response(&mut response).await.unwrap();
    let proof = response
        .metadata
        .iter()
        .find(|entry| entry.key == metadata::PROOF)
        .expect("response carries a proof");
    let proof_bytes = hex::decode(&proof.value).unwrap();
    let input = BusinessRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
        service_actor_id: service_id().to_string_repr(),
        channel_id: "channel-1".into(),
        route: ROUTE.into(),
        capability: READ.into(),
        request_id: "call-1".into(),
        issued_at: now(),
        deadline: now() + 30,
        kind: MessageKind::Response,
        idempotency_key: None,
        payload: b"document body".to_vec(),
        proof: Vec::new(),
    };
    assert!(
        verify_request_proof(secret.as_bytes(), &input, &proof_bytes),
        "caller verifies the response proof"
    );

    // Detach closes the lease; further business calls are rejected.
    let detach = DetachRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
    };
    let envelope = request_envelope(
        "actrium.session.v1.SessionRegistry.Detach",
        "detach-rpc-1",
        serde_json::to_vec(&detach).unwrap(),
        vec![],
    );
    registry
        .dispatch(
            crate::lifecycle::SessionMethod::Detach,
            dispatch_request(&envelope, Some(service_context())),
        )
        .await
        .expect("Detach succeeds");
    let error = guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect_err("a detached session rejects business calls");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::Revoked
    );
}

#[tokio::test]
async fn session_methods_require_a_verified_channel() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let channel = front_context();
    let mut request = attach_request(&channel);
    sign_attach_request(&mut request, &front_key());
    let envelope = request_envelope(
        "actrium.session.v1.SessionRegistry.Attach",
        "attach-rpc-1",
        serde_json::to_vec(&request).unwrap(),
        vec![],
    );
    let error = registry
        .dispatch(
            crate::lifecycle::SessionMethod::Attach,
            dispatch_request(&envelope, None),
        )
        .await
        .expect_err("legacy transports cannot Attach");
    assert!(matches!(
        error,
        actr_protocol::ActrError::PermissionDenied(_)
    ));

    let call = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Request as i32),
        payload: Some(Bytes::from_static(b"payload")),
        ..Default::default()
    };
    let error = guard
        .check(dispatch_request(&call, None))
        .await
        .expect_err("legacy business traffic is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::PermissionDenied
    );
}

#[tokio::test]
async fn tampered_requests_and_responses_are_rejected() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;

    // A valid request with a mutated payload fails the HMAC check.
    let mut call = business_call(&attach, &secret);
    call.payload = Some(Bytes::from_static(b"forged payload"));
    let error = guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect_err("payload forgery is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::PermissionDenied
    );

    // Missing proof metadata is an invalid-metadata rejection.
    let mut call = business_call(&attach, &secret);
    call.metadata.retain(|entry| entry.key != metadata::PROOF);
    let error = guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect_err("missing proof is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::InvalidMetadata
    );

    // A genuine response with a mutated payload fails caller verification.
    let call = business_call(&attach, &secret);
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .unwrap();
    let mut response = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::from_static(b"document body")),
        ..Default::default()
    };
    guard.authenticate_response(&mut response).await.unwrap();
    response.payload = Some(Bytes::from_static(b"forged response"));

    let proof = response
        .metadata
        .iter()
        .find(|entry| entry.key == metadata::PROOF)
        .expect("response carries a proof");
    let proof_bytes = hex::decode(&proof.value).unwrap();
    let input = BusinessRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
        service_actor_id: service_id().to_string_repr(),
        channel_id: "channel-1".into(),
        route: ROUTE.into(),
        capability: READ.into(),
        request_id: "call-1".into(),
        issued_at: now(),
        deadline: now() + 30,
        kind: MessageKind::Response,
        idempotency_key: None,
        payload: b"forged response".to_vec(),
        proof: Vec::new(),
    };
    assert!(
        !verify_request_proof(secret.as_bytes(), &input, &proof_bytes),
        "response forgery is rejected"
    );
}

#[tokio::test]
async fn dequeue_rejects_a_mutated_envelope() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;
    let call = business_call(&attach, &secret);
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect("arrival admission");

    // Same request id but different content at dequeue: rejected even though
    // the original admission consumed nothing from the attacker's envelope.
    let mut mutated = call.clone();
    mutated
        .metadata
        .iter_mut()
        .find(|entry| entry.key == metadata::CAPABILITY)
        .unwrap()
        .value = "documents.write".into();
    let error = guard
        .check(dispatch_request(&mutated, Some(service_context())))
        .await
        .expect_err("a mutated dequeue envelope is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::PermissionDenied
    );

    // The untouched envelope still dequeues successfully afterwards.
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect("the original envelope remains dispatchable");
}

#[tokio::test]
async fn heartbeat_status_and_stream_chunks_use_the_same_guard() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;

    let heartbeat = HeartbeatRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
    };
    let envelope = request_envelope(
        "actrium.session.v1.SessionRegistry.Heartbeat",
        "heartbeat-rpc-1",
        serde_json::to_vec(&heartbeat).unwrap(),
        vec![],
    );
    registry
        .dispatch(
            crate::lifecycle::SessionMethod::Heartbeat,
            dispatch_request(&envelope, Some(service_context())),
        )
        .await
        .expect("Heartbeat succeeds");

    let status = GetStatusRequest {
        session_id: attach.session_id.clone(),
    };
    let envelope = request_envelope(
        "actrium.session.v1.SessionRegistry.GetStatus",
        "status-rpc-1",
        serde_json::to_vec(&status).unwrap(),
        vec![],
    );
    let status = registry
        .dispatch(
            crate::lifecycle::SessionMethod::GetStatus,
            dispatch_request(&envelope, Some(service_context())),
        )
        .await
        .expect("GetStatus succeeds");
    let status: SessionStatus = serde_json::from_slice(&status).unwrap();
    assert_eq!(status.state, actrium_runtime::SessionState::Active);

    // A stream chunk carries the same session proof shape with kind=StreamChunk.
    let mut request = BusinessRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
        service_actor_id: service_id().to_string_repr(),
        channel_id: "channel-1".into(),
        route: ROUTE.into(),
        capability: READ.into(),
        request_id: "stream-1:0".into(),
        issued_at: now(),
        deadline: now() + 30,
        kind: MessageKind::StreamChunk,
        idempotency_key: None,
        payload: b"chunk".to_vec(),
        proof: Vec::new(),
    };
    request.sign(&secret);
    let mut metadata = business_metadata(&request);
    metadata.push(MetadataEntry {
        key: metadata::ROUTE.into(),
        value: ROUTE.into(),
    });
    let chunk = actr_protocol::DataChunk {
        stream_id: "stream-1".into(),
        sequence: 0,
        payload: Bytes::from_static(b"chunk"),
        metadata,
        timestamp_ms: None,
    };
    let sender = front_id();
    let request =
        IncomingStreamChunk::new(&chunk, &sender).with_peer_context(Some(service_context()));
    guard
        .check_stream(request)
        .await
        .expect("a signed stream chunk is admitted");
}

fn pending_call(attach: &AttachResponse, secret: &SessionSecret) -> PendingCall {
    let request = BusinessRequest {
        session_id: attach.session_id.clone(),
        lease_epoch: attach.lease_epoch,
        service_actor_id: service_id().to_string_repr(),
        channel_id: "channel-1".into(),
        route: ROUTE.into(),
        capability: READ.into(),
        request_id: "call-1".into(),
        issued_at: now(),
        deadline: now() + 30,
        kind: MessageKind::Request,
        idempotency_key: None,
        payload: b"read document".to_vec(),
        proof: Vec::new(),
    };
    PendingCall {
        request,
        secret: secret.clone(),
    }
}

fn signed_response_envelope(
    request_id: &str,
    payload: &[u8],
    attach: &AttachResponse,
    secret: &SessionSecret,
) -> RpcEnvelope {
    let pending = pending_call(attach, secret);
    let mut input = pending.request;
    input.kind = MessageKind::Response;
    input.payload = payload.to_vec();
    input.proof = request_proof(secret.as_bytes(), &input);
    RpcEnvelope {
        request_id: request_id.into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::copy_from_slice(payload)),
        timeout_ms: 0,
        metadata: vec![MetadataEntry {
            key: metadata::PROOF.into(),
            value: hex::encode(input.proof),
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn inbound_response_without_proof_is_rejected() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;
    shared
        .pending
        .lock()
        .await
        .insert("call-1".into(), pending_call(&attach, &secret));

    let response = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::from_static(b"document body")),
        timeout_ms: 0,
        metadata: vec![],
        ..Default::default()
    };
    let error = guard
        .check(dispatch_request(&response, Some(service_context())))
        .await
        .expect_err("response without proof is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::PermissionDenied
    );
}

#[tokio::test]
async fn inbound_response_with_tampered_payload_is_rejected() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;
    shared
        .pending
        .lock()
        .await
        .insert("call-1".into(), pending_call(&attach, &secret));

    let mut response = signed_response_envelope("call-1", b"document body", &attach, &secret);
    response.payload = Some(Bytes::from_static(b"forged response"));

    let error = guard
        .check(dispatch_request(&response, Some(service_context())))
        .await
        .expect_err("tampered response is rejected");
    assert_eq!(
        error.code,
        crate::lifecycle::IncomingGuardErrorCode::PermissionDenied
    );
}

#[tokio::test]
async fn inbound_response_proof_round_trip_consumes_pending() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;
    shared
        .pending
        .lock()
        .await
        .insert("call-1".into(), pending_call(&attach, &secret));

    let response = signed_response_envelope("call-1", b"document body", &attach, &secret);
    guard
        .check(dispatch_request(&response, Some(service_context())))
        .await
        .expect("valid inbound response passes");
    assert!(
        shared.pending.lock().await.is_empty(),
        "pending call is consumed after verification"
    );
}

#[tokio::test]
async fn finish_releases_admitted_request() {
    let shared = Arc::new(PeerShared::default());
    let (registry, guard) = ActriumServiceRegistry::new(service(), shared.clone());
    let (attach, secret) = attach(&registry).await;

    let call = business_call(&attach, &secret);
    guard
        .check(dispatch_request(&call, Some(service_context())))
        .await
        .expect("arrival admission");

    let mut response = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::from_static(b"document body")),
        timeout_ms: 0,
        metadata: vec![],
        ..Default::default()
    };
    guard
        .authenticate_response(&mut response)
        .await
        .expect("response is signed while admitted");
    assert!(
        response
            .metadata
            .iter()
            .any(|entry| entry.key == metadata::PROOF),
        "admitted request allows response signing"
    );

    guard.finish("call-1").await.expect("finish succeeds");

    let mut response_after_finish = RpcEnvelope {
        request_id: "call-1".into(),
        route_key: ROUTE.into(),
        direction: Some(Direction::Response as i32),
        payload: Some(Bytes::from_static(b"document body")),
        timeout_ms: 0,
        metadata: vec![],
        ..Default::default()
    };
    guard
        .authenticate_response(&mut response_after_finish)
        .await
        .expect("finish is idempotent");
    assert!(
        !response_after_finish
            .metadata
            .iter()
            .any(|entry| entry.key == metadata::PROOF),
        "finished request is no longer admitted"
    );
}
