use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use actr_relay_security::{
    ACTOR_CHANNEL_AUDIENCE, ActorChannelCertificate, ActorChannelClaims, ActorChannelIdentity,
    ActorChannelSession, ActorIdentityKey, IdentityKey, IssuerKey, VerifiedActorChannel,
};
use actrium_runtime::*;
use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

const NOW: u64 = 1_000;
const READ: &str = "documents.read";
const WRITE: &str = "documents.write";

fn test_key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&Sha256::digest(label.as_bytes()).into())
}

fn test_digest(label: &str) -> Vec<u8> {
    Sha256::digest(label.as_bytes()).to_vec()
}

struct TestClock;
impl Clock for TestClock {
    fn now_epoch_secs(&self) -> u64 {
        NOW
    }
}

struct TestMembership;
#[async_trait]
impl MembershipProvider for TestMembership {
    async fn principal(
        &self,
        front_actor_id: &str,
        member: &str,
        client_key: &str,
    ) -> Result<Option<Principal>, SessionError> {
        if front_actor_id == "front" && member == "member-1" && client_key == "client-1" {
            Ok(Some(Principal {
                member: member.into(),
                client_key: client_key.into(),
                role: MemberRole::Owner,
                capabilities: BTreeSet::from([READ.into(), WRITE.into()]),
            }))
        } else {
            Ok(None)
        }
    }
}

struct AllowAttach;
#[async_trait]
impl AuthorizeAttach for AllowAttach {
    async fn authorize_attach(
        &self,
        _context: AttachAuthorization,
    ) -> Result<AuthorizeAttachDecision, SessionError> {
        Ok(AuthorizeAttachDecision {
            allowed: true,
            denial_reason: None,
            allowed_capabilities: vec![READ.into(), WRITE.into()],
        })
    }
}

struct AllowTakeover;
#[async_trait]
impl AuthorizeTakeover for AllowTakeover {
    async fn authorize_takeover(&self, _context: TakeoverContext) -> Result<bool, SessionError> {
        Ok(true)
    }
}

struct AllowRequest;
#[async_trait]
impl AuthorizeRequest for AllowRequest {
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

struct TestPrivacy;
#[async_trait]
impl PrivacyPolicy for TestPrivacy {
    async fn evaluate(&self, context: PrivacyContext) -> Result<PrivacyDecision, SessionError> {
        Ok(PrivacyDecision {
            state: PrivacyState {
                actor_state_digest: test_digest("actor-state"),
                application_state_digest: test_digest("application-state"),
                assurance_digest: test_digest("assurance"),
                expires_at_epoch_secs: 10_000,
            },
            allowed_capabilities: context.requested_capabilities,
        })
    }
}

struct TestQuota;
#[async_trait]
impl ResourceQuota for TestQuota {
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

fn policies() -> ServicePolicies {
    ServicePolicies {
        membership: Arc::new(TestMembership),
        authorize_attach: Arc::new(AllowAttach),
        authorize_takeover: Arc::new(AllowTakeover),
        authorize_request: Arc::new(AllowRequest),
        privacy: Arc::new(TestPrivacy),
        quota: Arc::new(TestQuota),
    }
}

fn actor_identity(
    actor_id: &str,
    actor_type: &str,
    role: &str,
    seed: [u8; 32],
    issuer: &SigningKey,
    issuer_public: &IssuerKey,
) -> ActorChannelIdentity {
    let noise = IdentityKey::generate().unwrap();
    let actor = ActorIdentityKey::from_bytes(seed);
    let claims = ActorChannelClaims {
        realm: "realm-1".into(),
        actor_id: actor_id.into(),
        actor_type: actor_type.into(),
        member: "member-1".into(),
        client_key: "client-1".into(),
        role: role.into(),
        actor_public_key: actor.public,
        noise_public_key: noise.public,
        actor_credential_digest: test_digest(actor_id).try_into().unwrap(),
        policy_digest: test_digest("test-policy").try_into().unwrap(),
        actor_state_digest: test_digest(actor_id).try_into().unwrap(),
        issuer: "realm-issuer".into(),
        provider: "provider.example".into(),
        audience: ACTOR_CHANNEL_AUDIENCE.into(),
        expires_at: 10_000,
        runtime_instance_id: format!("{actor_id}-runtime"),
    };
    let certificate =
        ActorChannelCertificate::issue(&claims, issuer_public.key_id, issuer).unwrap();
    ActorChannelIdentity::certify(
        noise,
        actor,
        certificate,
        issuer_public.clone(),
        "realm-1",
        actor_id,
        NOW,
    )
    .unwrap()
}

fn one(mut records: Vec<Vec<u8>>) -> Vec<u8> {
    assert_eq!(records.len(), 1);
    records.pop().unwrap()
}

fn complete_channel(
    front: &mut ActorChannelSession,
    service: &mut ActorChannelSession,
    issuer: &IssuerKey,
) {
    let first = front.start().unwrap().unwrap();
    assert!(service.start().unwrap().is_none());
    let second = one(service.receive(&first, None, NOW).unwrap().outbound);
    let third = one(front.receive(&second, Some(issuer), NOW).unwrap().outbound);
    let service_records = service.receive(&third, Some(issuer), NOW).unwrap().outbound;
    assert_eq!(service_records.len(), 2);
    assert!(
        !front
            .receive(&service_records[0], None, NOW)
            .unwrap()
            .became_ready
    );
    let front_records = front.receive(&service_records[1], None, NOW).unwrap();
    assert!(front_records.became_ready);
    assert_eq!(front_records.outbound.len(), 2);
    assert!(
        !service
            .receive(&front_records.outbound[0], None, NOW)
            .unwrap()
            .became_ready
    );
    assert!(
        service
            .receive(&front_records.outbound[1], None, NOW)
            .unwrap()
            .became_ready
    );
    assert!(front.is_ready() && service.is_ready());
}

fn verified_channel(verified: &VerifiedActorChannel) -> VerifiedChannel {
    VerifiedChannel {
        channel_id: verified.channel_id.clone(),
        channel_transcript_digest: verified.transcript_digest.to_vec(),
        peer_actor_id: verified.peer_claims.actor_id.clone(),
        peer_actor_type: verified.peer_claims.actor_type.clone(),
        peer_member: verified.peer_claims.member.clone(),
        peer_client_key: verified.peer_claims.client_key.clone(),
        peer_role: MemberRole::Owner,
        peer_actor_public_key: verified.peer_actor_public_key.to_vec(),
        peer_credential_expires_at_epoch_secs: verified.expires_at,
        local_runtime_instance_id: verified.local_runtime_instance_id.clone(),
        peer_runtime_instance_id: verified.peer_runtime_instance_id.clone(),
        negotiation_digest: verified.peer_claims.policy_digest.to_vec(),
    }
}

fn service_config(service_key: SigningKey) -> ServiceConfig {
    ServiceConfig {
        service_actor_id: "service".into(),
        runtime_instance_id: "service-runtime".into(),
        service_signing_key: service_key,
        allowed_front_actor_types: BTreeSet::from(["actrium.front".into()]),
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: test_digest("package"),
        guest_digest: test_digest("guest"),
        protocol_fingerprint: test_digest("protocol"),
        supported_runtimes: vec![Runtime {
            actrium_version: "0.11".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        }],
        program_capabilities: BTreeSet::from([READ.into(), WRITE.into()]),
        routes: BTreeMap::from([
            ("documents/read".into(), BTreeSet::from([READ.into()])),
            ("documents/write".into(), BTreeSet::from([WRITE.into()])),
        ]),
        attachment_policy: AttachmentPolicy::Exclusive,
        lease_duration: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(10),
        grace_period: Duration::from_secs(5),
        replay_retention: ServiceConfig::DEFAULT_REPLAY_RETENTION,
        max_replay_entries: ServiceConfig::DEFAULT_MAX_REPLAY_ENTRIES,
        max_completed_sessions: ServiceConfig::DEFAULT_MAX_COMPLETED_SESSIONS,
        max_channel_generations: ServiceConfig::DEFAULT_MAX_CHANNEL_GENERATIONS,
    }
}

fn attach_request(channel: &VerifiedChannel, key: &SigningKey, attach_id: &str) -> AttachRequest {
    let mut request = AttachRequest {
        attach_id: attach_id.into(),
        front_actor_id: channel.peer_actor_id.clone(),
        service_actor_id: "service".into(),
        channel_id: channel.channel_id.clone(),
        channel_transcript_digest: channel.channel_transcript_digest.clone(),
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: test_digest("package"),
        guest_digest: test_digest("guest"),
        runtime: Runtime {
            actrium_version: "0.11".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        },
        protocol_fingerprint: test_digest("protocol"),
        tab_session_nonce: test_digest(&format!("attach-nonce:{attach_id}")),
        requested_lease: LeaseMode::Exclusive,
        requested_capabilities: vec![READ.into(), WRITE.into()],
        challenge_response: vec![],
    };
    sign_attach_request(&mut request, key);
    request
}

fn business_request(
    session_id: &str,
    lease_epoch: u64,
    channel_id: &str,
    request_id: &str,
    secret: &SessionSecret,
) -> BusinessRequest {
    let mut request = BusinessRequest {
        session_id: session_id.into(),
        lease_epoch,
        service_actor_id: "service".into(),
        channel_id: channel_id.into(),
        route: "documents/read".into(),
        capability: READ.into(),
        request_id: request_id.into(),
        issued_at: NOW,
        deadline: 2_000,
        kind: MessageKind::Request,
        idempotency_key: None,
        payload: b"list documents".to_vec(),
        proof: vec![],
    };
    request.sign(secret);
    request
}

#[tokio::test]
async fn noise_authenticated_front_attaches_and_uses_session_guarded_rpc() {
    let issuer = test_key("issuer");
    let issuer_public = IssuerKey {
        key_id: 11,
        public_key: issuer.verifying_key().to_bytes(),
        expires_at: 20_000,
    };
    let front_key = test_key("front");
    let service_key = test_key("service");
    let front_identity = actor_identity(
        "front",
        "actrium.front",
        "owner",
        front_key.to_bytes(),
        &issuer,
        &issuer_public,
    );
    let service_identity = actor_identity(
        "service",
        "actrium.service",
        "service",
        service_key.to_bytes(),
        &issuer,
        &issuer_public,
    );

    let mut front =
        ActorChannelSession::new_front(&front_identity, "service", "channel-e2e", NOW).unwrap();
    let mut service_channel =
        ActorChannelSession::new_service(&service_identity, "front", "channel-e2e", NOW).unwrap();
    complete_channel(&mut front, &mut service_channel, &issuer_public);

    let verified = service_channel.verified().unwrap().clone();
    assert_eq!(
        verified.peer_actor_public_key,
        front_key.verifying_key().to_bytes()
    );
    let channel = verified_channel(&verified);
    let service = InMemorySessionService::new_with_clock(
        service_config(service_key.clone()),
        policies(),
        Arc::new(TestClock),
    )
    .unwrap();

    let request = attach_request(&channel, &front_key, "attach-1");
    let request_digest = attach_request_digest(&request);
    let outcome = service.attach(request, &channel).await.unwrap();
    verify_attach_response(
        &outcome.response,
        &request_digest,
        service_key.verifying_key().as_bytes(),
    )
    .unwrap();
    assert_eq!(outcome.response.lease_epoch, 1);
    assert_eq!(outcome.response.service.actor_id, "service");
    assert_eq!(
        outcome.response.session_capability_ceiling,
        vec![READ, WRITE]
    );

    let guard = service.guard();
    let rpc = business_request(
        &outcome.response.session_id,
        outcome.response.lease_epoch,
        &channel.channel_id,
        "request-1",
        &outcome.session_secret,
    );
    let queued = guard.authorize(rpc, &channel).await.unwrap();
    let delivered = guard.dequeue(queued, &channel).await.unwrap();
    assert_eq!(delivered.payload, b"list documents");

    let occupied = match service
        .attach(
            attach_request(&channel, &front_key, "attach-occupied"),
            &channel,
        )
        .await
    {
        Ok(_) => panic!("a second exclusive Attach unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(occupied.code, AttachErrorCode::SessionOccupied);

    let next_attach = attach_request(&channel, &front_key, "attach-takeover");
    let mut takeover = TakeoverRequest {
        session_id: outcome.response.session_id.clone(),
        lease_epoch: outcome.response.lease_epoch,
        requesting_front_actor_id: "front".into(),
        takeover_proof: vec![],
    };
    takeover.takeover_proof = front_key
        .sign(&takeover_signing_bytes(&takeover, &next_attach))
        .to_bytes()
        .to_vec();
    let taken = service
        .takeover(takeover, next_attach, &channel)
        .await
        .unwrap();
    assert!(taken.response.lease_epoch > outcome.response.lease_epoch);

    let stale = business_request(
        &outcome.response.session_id,
        outcome.response.lease_epoch,
        &channel.channel_id,
        "request-stale",
        &outcome.session_secret,
    );
    assert!(guard.authorize(stale, &channel).await.is_err());

    let current = business_request(
        &taken.response.session_id,
        taken.response.lease_epoch,
        &channel.channel_id,
        "request-current",
        &taken.session_secret,
    );
    assert!(guard.authorize(current, &channel).await.is_ok());
}
