use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use actrium_runtime::*;
use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

fn test_key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&Sha256::digest(label.as_bytes()).into())
}

fn test_digest(label: &str) -> Vec<u8> {
    Sha256::digest(label.as_bytes()).to_vec()
}

const READ: &str = "documents.read";
const CREDENTIAL_EXPIRY: u64 = 100_000;

struct ManualClock(AtomicU64);

impl ManualClock {
    fn new(now: u64) -> Self {
        Self(AtomicU64::new(now))
    }

    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn advance(&self, seconds: u64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_epoch_secs(&self) -> u64 {
        self.now()
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
        Ok(
            (front_actor_id == "front" && member == "member" && client_key == "client").then(
                || Principal {
                    member: member.into(),
                    client_key: client_key.into(),
                    role: MemberRole::Owner,
                    capabilities: BTreeSet::from([READ.into()]),
                },
            ),
        )
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
            allowed_capabilities: vec![READ.into()],
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
                expires_at_epoch_secs: CREDENTIAL_EXPIRY,
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

struct Fixture {
    service: InMemorySessionService,
    clock: Arc<ManualClock>,
    front_key: SigningKey,
}

impl Fixture {
    fn new() -> Self {
        let clock = Arc::new(ManualClock::new(1_000));
        let service = InMemorySessionService::new_with_clock(
            service_config(),
            ServicePolicies {
                membership: Arc::new(TestMembership),
                authorize_attach: Arc::new(AllowAttach),
                authorize_takeover: Arc::new(AllowTakeover),
                authorize_request: Arc::new(AllowRequest),
                privacy: Arc::new(TestPrivacy),
                quota: Arc::new(TestQuota),
            },
            clock.clone(),
        )
        .unwrap();
        Self {
            service,
            clock,
            front_key: test_key("front"),
        }
    }

    fn channel(&self, id: &str) -> VerifiedChannel {
        VerifiedChannel {
            channel_id: id.into(),
            channel_transcript_digest: test_digest(id),
            peer_actor_id: "front".into(),
            peer_actor_type: "actrium.front".into(),
            peer_member: "member".into(),
            peer_client_key: "client".into(),
            peer_role: MemberRole::Owner,
            peer_actor_public_key: self.front_key.verifying_key().to_bytes().to_vec(),
            peer_credential_expires_at_epoch_secs: CREDENTIAL_EXPIRY,
            local_runtime_instance_id: "runtime".into(),
            peer_runtime_instance_id: format!("front-runtime-{id}"),
            negotiation_digest: test_digest("negotiation"),
        }
    }

    fn attach_request(&self, channel: &VerifiedChannel, id: &str) -> AttachRequest {
        let mut request = AttachRequest {
            attach_id: id.into(),
            front_actor_id: "front".into(),
            service_actor_id: "service".into(),
            channel_id: channel.channel_id.clone(),
            channel_transcript_digest: channel.channel_transcript_digest.clone(),
            program: program(),
            package_digest: test_digest("package"),
            guest_digest: test_digest("guest"),
            runtime: runtime(),
            protocol_fingerprint: test_digest("protocol"),
            tab_session_nonce: test_digest(&format!("attach-nonce:{id}")),
            requested_lease: LeaseMode::Exclusive,
            requested_capabilities: vec![READ.into()],
            challenge_response: vec![],
        };
        sign_attach_request(&mut request, &self.front_key);
        request
    }

    fn business(
        &self,
        outcome: &AttachOutcome,
        channel: &VerifiedChannel,
        request_id: &str,
        issued_at: u64,
    ) -> BusinessRequest {
        let mut request = BusinessRequest {
            session_id: outcome.response.session_id.clone(),
            lease_epoch: outcome.response.lease_epoch,
            service_actor_id: "service".into(),
            channel_id: channel.channel_id.clone(),
            route: "documents/read".into(),
            capability: READ.into(),
            request_id: request_id.into(),
            issued_at,
            deadline: issued_at + 100,
            kind: MessageKind::Request,
            idempotency_key: Some(format!("idem-{request_id}")),
            payload: request_id.as_bytes().to_vec(),
            proof: vec![],
        };
        request.sign(&outcome.session_secret);
        request
    }

    async fn heartbeat(&self, outcome: &AttachOutcome, channel: &VerifiedChannel) {
        self.service
            .heartbeat(
                HeartbeatRequest {
                    session_id: outcome.response.session_id.clone(),
                    lease_epoch: outcome.response.lease_epoch,
                },
                channel,
                None,
            )
            .await
            .unwrap();
    }

    fn assert_bounded(&self, stats: RetentionStats) {
        assert!(stats.completed <= 4, "completed: {}", stats.completed);
        assert!(stats.used_nonces <= 4, "nonces: {}", stats.used_nonces);
        assert!(stats.requests <= 4, "requests: {}", stats.requests);
        assert!(
            stats.idempotency_keys <= 4,
            "idempotency keys: {}",
            stats.idempotency_keys
        );
        assert!(stats.closed_sessions <= 2);
        assert!(stats.used_channel_ids <= stats.sessions * 3);
    }
}

fn service_config() -> ServiceConfig {
    ServiceConfig {
        service_actor_id: "service".into(),
        runtime_instance_id: "runtime".into(),
        service_signing_key: test_key("service"),
        allowed_front_actor_types: BTreeSet::from(["actrium.front".into()]),
        program: program(),
        package_digest: test_digest("package"),
        guest_digest: test_digest("guest"),
        protocol_fingerprint: test_digest("protocol"),
        supported_runtimes: vec![runtime()],
        program_capabilities: BTreeSet::from([READ.into()]),
        routes: BTreeMap::from([("documents/read".into(), BTreeSet::from([READ.into()]))]),
        attachment_policy: AttachmentPolicy::Exclusive,
        lease_duration: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(10),
        grace_period: Duration::from_secs(5),
        replay_retention: Duration::from_secs(20),
        max_replay_entries: 4,
        max_completed_sessions: 2,
        max_channel_generations: 3,
    }
}

fn program() -> ProgramRelease {
    ProgramRelease {
        publisher: "actrium".into(),
        name: "documents".into(),
        version: "1".into(),
    }
}

fn runtime() -> Runtime {
    Runtime {
        actrium_version: "0.11".into(),
        front_host_api: "1".into(),
        keel_api: "1".into(),
    }
}

fn resume_request(outcome: &AttachOutcome, channel: &VerifiedChannel) -> ResumeRequest {
    let mut request = ResumeRequest {
        session_id: outcome.response.session_id.clone(),
        lease_epoch: outcome.response.lease_epoch,
        channel_id: channel.channel_id.clone(),
        channel_transcript_digest: channel.channel_transcript_digest.clone(),
        session_proof: vec![],
    };
    request.session_proof = resume_proof(outcome.session_secret.as_bytes(), &request);
    request
}

#[tokio::test]
async fn continuous_unique_requests_are_bounded_and_evicted_replays_stay_rejected() {
    let fixture = Fixture::new();
    let channel = fixture.channel("channel-a");
    let outcome = fixture
        .service
        .attach(fixture.attach_request(&channel, "attach-a"), &channel)
        .await
        .unwrap();
    let first = fixture.business(&outcome, &channel, "request-0", fixture.clock.now());

    for index in 0..24 {
        let now = fixture.clock.now();
        let request = fixture.business(&outcome, &channel, &format!("request-{index}"), now);
        let queued = fixture
            .service
            .guard()
            .authorize(request, &channel)
            .await
            .unwrap();
        fixture
            .service
            .guard()
            .dequeue(queued, &channel)
            .await
            .unwrap();
        fixture.assert_bounded(fixture.service.registry().retention_stats().await);
        fixture.heartbeat(&outcome, &channel).await;

        if index == 5 {
            // The first request was displaced by capacity, but is still inside
            // its time window. Its signed issuance time is below replay_floor.
            let error = fixture
                .service
                .guard()
                .authorize(first.clone(), &channel)
                .await
                .unwrap_err();
            assert_eq!(error.code, AttachErrorCode::PermissionDenied);
        }
        fixture.clock.advance(1);
    }

    assert_eq!(
        fixture
            .service
            .status(&outcome.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Active
    );
    let error = fixture
        .service
        .guard()
        .authorize(first, &channel)
        .await
        .unwrap_err();
    assert_eq!(error.code, AttachErrorCode::SessionExpired);
}

#[tokio::test]
async fn attach_detach_and_resume_loops_do_not_retain_unbounded_history() {
    let fixture = Fixture::new();
    for index in 0..12 {
        let channel = fixture.channel(&format!("attach-channel-{index}"));
        let outcome = fixture
            .service
            .attach(
                fixture.attach_request(&channel, &format!("attach-{index}")),
                &channel,
            )
            .await
            .unwrap();
        fixture
            .service
            .detach(
                DetachRequest {
                    session_id: outcome.response.session_id,
                    lease_epoch: outcome.response.lease_epoch,
                },
                &channel,
            )
            .await
            .unwrap();
        fixture.assert_bounded(fixture.service.registry().retention_stats().await);
        fixture.clock.advance(1);
    }

    let initial_channel = fixture.channel("resume-channel-0");
    let mut outcome = fixture
        .service
        .attach(
            fixture.attach_request(&initial_channel, "attach-resume"),
            &initial_channel,
        )
        .await
        .unwrap();
    let session_id = outcome.response.session_id.clone();
    let mut current_channel = initial_channel;
    for index in 1..16 {
        fixture
            .service
            .handle_disconnect(&session_id, outcome.response.lease_epoch)
            .await
            .unwrap();
        let next_channel = fixture.channel(&format!("resume-channel-{index}"));
        outcome = fixture
            .service
            .resume(resume_request(&outcome, &next_channel), &next_channel)
            .await
            .unwrap();
        current_channel = next_channel;
        fixture.assert_bounded(fixture.service.registry().retention_stats().await);
    }

    assert_eq!(outcome.response.session_id, session_id);
    assert_eq!(
        fixture.service.status(&session_id).await.unwrap().state,
        SessionState::Active
    );
    let request = fixture.business(
        &outcome,
        &current_channel,
        "after-resume-churn",
        fixture.clock.now(),
    );
    fixture
        .service
        .guard()
        .authorize(request, &current_channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn ttl_reclaims_closed_sessions_without_touching_the_active_session() {
    let fixture = Fixture::new();
    let old_channel = fixture.channel("old");
    let old = fixture
        .service
        .attach(
            fixture.attach_request(&old_channel, "old-attach"),
            &old_channel,
        )
        .await
        .unwrap();
    fixture
        .service
        .detach(
            DetachRequest {
                session_id: old.response.session_id.clone(),
                lease_epoch: old.response.lease_epoch,
            },
            &old_channel,
        )
        .await
        .unwrap();

    let active_channel = fixture.channel("active");
    let active = fixture
        .service
        .attach(
            fixture.attach_request(&active_channel, "active-attach"),
            &active_channel,
        )
        .await
        .unwrap();
    fixture.clock.advance(21);
    fixture.heartbeat(&active, &active_channel).await;

    assert!(
        fixture
            .service
            .session(&old.response.session_id)
            .await
            .is_none()
    );
    assert_eq!(
        fixture
            .service
            .status(&active.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Active
    );
    fixture.assert_bounded(fixture.service.registry().retention_stats().await);
}

#[tokio::test]
async fn invalidate_all_releases_sessions_but_keeps_bounded_attach_tombstones() {
    let fixture = Fixture::new();
    let channel = fixture.channel("channel");
    let attach = fixture.attach_request(&channel, "attach-original");
    let outcome = fixture
        .service
        .attach(attach.clone(), &channel)
        .await
        .unwrap();
    let request = fixture.business(&outcome, &channel, "request", fixture.clock.now());
    fixture
        .service
        .guard()
        .authorize(request.clone(), &channel)
        .await
        .unwrap();

    fixture.service.invalidate_all().await;
    let stats = fixture.service.registry().retention_stats().await;
    assert_eq!(stats.sessions, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.requests, 0);
    assert_eq!(stats.idempotency_keys, 0);
    assert_eq!(stats.used_channel_ids, 0);
    assert_eq!(stats.used_nonces, 1);

    assert!(
        fixture
            .service
            .guard()
            .authorize(request, &channel)
            .await
            .is_err()
    );
    let replay = match fixture.service.attach(attach, &channel).await {
        Err(error) => error,
        Ok(_) => panic!("replayed Attach unexpectedly succeeded"),
    };
    assert_eq!(replay.code, AttachErrorCode::PermissionDenied);

    for index in 0..10 {
        let attach = fixture.attach_request(&channel, &format!("fresh-attach-{index}"));
        fixture.service.attach(attach, &channel).await.unwrap();
        fixture.service.invalidate_all().await;
        fixture.assert_bounded(fixture.service.registry().retention_stats().await);
    }
}
