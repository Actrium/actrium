use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actrium_runtime::*;
use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

const READ: &str = "documents.read";
const WRITE: &str = "documents.write";

fn test_key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&Sha256::digest(label.as_bytes()).into())
}

fn test_digest(label: &str) -> Vec<u8> {
    Sha256::digest(label.as_bytes()).to_vec()
}

#[derive(Debug)]
struct ManualClock(AtomicU64);

impl ManualClock {
    fn new(now: u64) -> Self {
        Self(AtomicU64::new(now))
    }

    fn set(&self, now: u64) {
        self.0.store(now, Ordering::SeqCst);
    }

    fn advance(&self, seconds: u64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_epoch_secs(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Simulates time passing between the initial checks and the final commit.
struct AdvancingClock(AtomicU64);

impl Clock for AdvancingClock {
    fn now_epoch_secs(&self) -> u64 {
        self.0.fetch_add(1_000, Ordering::SeqCst) + 1_000
    }
}

struct Membership {
    principals: Mutex<HashMap<String, Principal>>,
}

impl Membership {
    fn set_role(&self, front: &Front, role: MemberRole) {
        let mut principals = self.principals.lock().unwrap();
        let principal = principals.get_mut(&front.actor_id).unwrap();
        principal.role = role;
        principal.capabilities = role_capabilities(role);
    }
}

#[async_trait]
impl MembershipProvider for Membership {
    async fn principal(
        &self,
        front_actor_id: &str,
        _member: &str,
        _client_key: &str,
    ) -> Result<Option<Principal>, SessionError> {
        Ok(self.principals.lock().unwrap().get(front_actor_id).cloned())
    }
}

struct AttachAuthorizer {
    capabilities: Mutex<BTreeSet<String>>,
    denied: AtomicBool,
    calls: AtomicUsize,
}

#[async_trait]
impl AuthorizeAttach for AttachAuthorizer {
    async fn authorize_attach(
        &self,
        _context: AttachAuthorization,
    ) -> Result<AuthorizeAttachDecision, SessionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let allowed = !self.denied.load(Ordering::SeqCst);
        Ok(AuthorizeAttachDecision {
            allowed,
            denial_reason: (!allowed).then(|| "application test denial".into()),
            allowed_capabilities: self.capabilities.lock().unwrap().iter().cloned().collect(),
        })
    }
}

struct TakeoverAuthorizer(AtomicBool);

#[async_trait]
impl AuthorizeTakeover for TakeoverAuthorizer {
    async fn authorize_takeover(&self, _context: TakeoverContext) -> Result<bool, SessionError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}

struct RequestAuthorizer(AtomicBool);

#[async_trait]
impl AuthorizeRequest for RequestAuthorizer {
    async fn authorize_request(
        &self,
        _context: RequestContext,
    ) -> Result<RequestAuthorizationDecision, SessionError> {
        let allowed = self.0.load(Ordering::SeqCst);
        Ok(RequestAuthorizationDecision {
            allowed,
            denial_reason: (!allowed).then(|| "application request revoked".into()),
        })
    }
}

struct Privacy {
    allowed: AtomicBool,
    expires_at: AtomicU64,
}

#[async_trait]
impl PrivacyPolicy for Privacy {
    async fn evaluate(&self, context: PrivacyContext) -> Result<PrivacyDecision, SessionError> {
        let allowed = self.allowed.load(Ordering::SeqCst);
        Ok(PrivacyDecision {
            state: PrivacyState {
                actor_state_digest: test_digest("actor-state"),
                application_state_digest: test_digest("application-state"),
                assurance_digest: test_digest("assurance"),
                expires_at_epoch_secs: self.expires_at.load(Ordering::SeqCst),
            },
            allowed_capabilities: if allowed {
                context.requested_capabilities
            } else {
                BTreeSet::new()
            },
        })
    }
}

struct Quota {
    attach_allowed: AtomicBool,
    request_allowed: AtomicBool,
    attach_checks: AtomicUsize,
}

#[async_trait]
impl ResourceQuota for Quota {
    async fn check_attach(
        &self,
        _context: AttachQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        self.attach_checks.fetch_add(1, Ordering::SeqCst);
        let allowed = self.attach_allowed.load(Ordering::SeqCst);
        Ok(QuotaDecision {
            allowed,
            denial_reason: (!allowed).then(|| "Attach quota exhausted".into()),
        })
    }

    async fn check_request(
        &self,
        _context: RequestQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        let allowed = self.request_allowed.load(Ordering::SeqCst);
        Ok(QuotaDecision {
            allowed,
            denial_reason: (!allowed).then(|| "request quota exhausted".into()),
        })
    }
}

struct Policies {
    membership: Arc<Membership>,
    attach: Arc<AttachAuthorizer>,
    takeover: Arc<TakeoverAuthorizer>,
    request: Arc<RequestAuthorizer>,
    privacy: Arc<Privacy>,
    quota: Arc<Quota>,
}

impl Policies {
    fn new(fronts: &[Front]) -> Self {
        let principals = fronts
            .iter()
            .map(|front| {
                (
                    front.actor_id.clone(),
                    Principal {
                        member: front.member.clone(),
                        client_key: front.client_key.clone(),
                        role: MemberRole::Owner,
                        capabilities: role_capabilities(MemberRole::Owner),
                    },
                )
            })
            .collect();
        Self {
            membership: Arc::new(Membership {
                principals: Mutex::new(principals),
            }),
            attach: Arc::new(AttachAuthorizer {
                capabilities: Mutex::new(BTreeSet::from([READ.into(), WRITE.into()])),
                denied: AtomicBool::new(false),
                calls: AtomicUsize::new(0),
            }),
            takeover: Arc::new(TakeoverAuthorizer(AtomicBool::new(false))),
            request: Arc::new(RequestAuthorizer(AtomicBool::new(true))),
            privacy: Arc::new(Privacy {
                allowed: AtomicBool::new(true),
                expires_at: AtomicU64::new(10_000),
            }),
            quota: Arc::new(Quota {
                attach_allowed: AtomicBool::new(true),
                request_allowed: AtomicBool::new(true),
                attach_checks: AtomicUsize::new(0),
            }),
        }
    }

    fn erased(&self) -> ServicePolicies {
        ServicePolicies {
            membership: self.membership.clone(),
            authorize_attach: self.attach.clone(),
            authorize_takeover: self.takeover.clone(),
            authorize_request: self.request.clone(),
            privacy: self.privacy.clone(),
            quota: self.quota.clone(),
        }
    }
}

#[derive(Clone)]
struct Front {
    actor_id: String,
    member: String,
    client_key: String,
    key: SigningKey,
}

impl Front {
    fn new(name: &str) -> Self {
        let key = test_key(name);
        Self {
            actor_id: format!("front:{name}"),
            member: format!("member:{name}"),
            client_key: format!("client:{name}"),
            key,
        }
    }
}

struct Fixture {
    service: InMemorySessionService,
    policies: Policies,
    clock: Arc<ManualClock>,
    service_key: SigningKey,
}

impl Fixture {
    fn new(fronts: &[Front]) -> Self {
        let policies = Policies::new(fronts);
        let clock = Arc::new(ManualClock::new(1_000));
        let service_key = test_key("service");
        let service = InMemorySessionService::new_with_clock(
            service_config(service_key.clone()),
            policies.erased(),
            clock.clone(),
        )
        .unwrap();
        Self {
            service,
            policies,
            clock,
            service_key,
        }
    }
}

fn service_config(service_signing_key: SigningKey) -> ServiceConfig {
    ServiceConfig {
        service_actor_id: "service:documents".into(),
        runtime_instance_id: "runtime-1".into(),
        service_signing_key,
        allowed_front_actor_types: BTreeSet::from(["actrium.front".into()]),
        program: program(),
        package_digest: test_digest("package"),
        guest_digest: test_digest("guest"),
        protocol_fingerprint: test_digest("protocol"),
        supported_runtimes: vec![runtime()],
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

fn program() -> ProgramRelease {
    ProgramRelease {
        publisher: "actrium".into(),
        name: "documents".into(),
        version: "1.0.0".into(),
    }
}

fn runtime() -> Runtime {
    Runtime {
        actrium_version: "0.11".into(),
        front_host_api: "1".into(),
        keel_api: "1".into(),
    }
}

fn role_capabilities(role: MemberRole) -> BTreeSet<String> {
    match role {
        MemberRole::Owner => BTreeSet::from([READ.into(), WRITE.into()]),
        MemberRole::Editor => BTreeSet::from([READ.into(), WRITE.into()]),
        MemberRole::Viewer => BTreeSet::from([READ.into()]),
    }
}

fn make_channel(front: &Front, id: &str, credential_expiry: u64) -> VerifiedChannel {
    channel_with_runtime(front, id, credential_expiry, "runtime-1")
}

fn channel_with_runtime(
    front: &Front,
    id: &str,
    credential_expiry: u64,
    runtime_instance: &str,
) -> VerifiedChannel {
    VerifiedChannel {
        channel_id: id.into(),
        channel_transcript_digest: test_digest(id),
        peer_actor_id: front.actor_id.clone(),
        peer_actor_type: "actrium.front".into(),
        peer_member: front.member.clone(),
        peer_client_key: front.client_key.clone(),
        peer_role: MemberRole::Owner,
        peer_actor_public_key: front.key.verifying_key().to_bytes().to_vec(),
        peer_credential_expires_at_epoch_secs: credential_expiry,
        local_runtime_instance_id: runtime_instance.into(),
        peer_runtime_instance_id: format!("{id}-front-runtime"),
        negotiation_digest: test_digest("negotiation"),
    }
}

fn attach_request(front: &Front, channel: &VerifiedChannel, attach_id: &str) -> AttachRequest {
    let mut request = AttachRequest {
        attach_id: attach_id.into(),
        front_actor_id: front.actor_id.clone(),
        service_actor_id: "service:documents".into(),
        channel_id: channel.channel_id.clone(),
        channel_transcript_digest: channel.channel_transcript_digest.clone(),
        program: program(),
        package_digest: test_digest("package"),
        guest_digest: test_digest("guest"),
        runtime: runtime(),
        protocol_fingerprint: test_digest("protocol"),
        tab_session_nonce: test_digest(&format!("attach-nonce:{attach_id}")),
        requested_lease: LeaseMode::Exclusive,
        requested_capabilities: vec![READ.into(), WRITE.into()],
        challenge_response: Vec::new(),
    };
    sign_attach_request(&mut request, &front.key);
    request
}

fn business(
    outcome: &AttachOutcome,
    channel: &VerifiedChannel,
    request_id: &str,
    capability: &str,
) -> BusinessRequest {
    let route = if capability == READ {
        "documents/read"
    } else {
        "documents/write"
    };
    let mut request = BusinessRequest {
        session_id: outcome.response.session_id.clone(),
        lease_epoch: outcome.response.lease_epoch,
        service_actor_id: "service:documents".into(),
        channel_id: channel.channel_id.clone(),
        route: route.into(),
        capability: capability.into(),
        request_id: request_id.into(),
        issued_at: 1_000,
        deadline: 2_000,
        kind: MessageKind::Request,
        idempotency_key: Some(format!("idem-{request_id}")),
        payload: format!("payload-{request_id}").into_bytes(),
        proof: Vec::new(),
    };
    request.sign(&outcome.session_secret);
    request
}

fn resume_request(outcome: &AttachOutcome, channel: &VerifiedChannel) -> ResumeRequest {
    let mut request = ResumeRequest {
        session_id: outcome.response.session_id.clone(),
        lease_epoch: outcome.response.lease_epoch,
        channel_id: channel.channel_id.clone(),
        channel_transcript_digest: channel.channel_transcript_digest.clone(),
        session_proof: Vec::new(),
    };
    request.session_proof = resume_proof(outcome.session_secret.as_bytes(), &request);
    request
}

type ChannelMutation = fn(&mut VerifiedChannel);

fn assert_code<T>(result: Result<T, SessionError>, code: AttachErrorCode) {
    match result {
        Err(error) => assert_eq!(error.code, code),
        Ok(_) => panic!("expected {code:?}, but the operation succeeded"),
    }
}

#[tokio::test]
async fn independent_identity_attach_and_service_proof() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let request = attach_request(&front, &channel, "attach-a");
    let outcome = fixture
        .service
        .attach(request.clone(), &channel)
        .await
        .unwrap();

    assert_eq!(outcome.response.service.actor_id, "service:documents");
    assert_eq!(outcome.response.session_capability_ceiling, [READ, WRITE]);
    assert_eq!(outcome.response.lease_epoch, 1);
    assert_eq!(outcome.response.lease.expires_at_epoch_secs, 1_030);
    verify_attach_response(
        &outcome.response,
        &attach_request_digest(&request),
        &fixture.service_key.verifying_key().to_bytes(),
    )
    .unwrap();
    let mut tampered = outcome.response.clone();
    tampered.lease_epoch += 1;
    assert!(
        verify_attach_response(
            &tampered,
            &attach_request_digest(&request),
            &fixture.service_key.verifying_key().to_bytes(),
        )
        .is_err()
    );
    assert!(
        verify_attach_response(
            &outcome.response,
            &[0; 32],
            &fixture.service_key.verifying_key().to_bytes(),
        )
        .is_err()
    );
    assert_ne!(outcome.session_secret.as_bytes(), &[0; 32]);
}

#[tokio::test]
async fn attach_is_idempotent_but_attach_id_conflicts_are_rejected() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let request = attach_request(&front, &channel, "same-attach");
    let first = fixture
        .service
        .attach(request.clone(), &channel)
        .await
        .unwrap();
    let replay = fixture
        .service
        .attach(request.clone(), &channel)
        .await
        .unwrap();
    assert_eq!(first.response.session_id, replay.response.session_id);
    assert_eq!(first.response.lease_epoch, replay.response.lease_epoch);
    assert_eq!(
        first.response.service_challenge,
        replay.response.service_challenge
    );

    let mut conflict = attach_request(&front, &channel, "same-attach");
    conflict.requested_capabilities = vec![READ.into()];
    sign_attach_request(&mut conflict, &front.key);
    assert_code(
        fixture.service.attach(conflict, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn occupied_returns_safe_lease_metadata() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    let alice_channel = make_channel(&alice, "a", 2_000);
    fixture
        .service
        .attach(attach_request(&alice, &alice_channel, "a1"), &alice_channel)
        .await
        .unwrap();
    let bob_channel = make_channel(&bob, "b", 2_000);
    let error = match fixture
        .service
        .attach(attach_request(&bob, &bob_channel, "b1"), &bob_channel)
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("second exclusive Attach unexpectedly succeeded"),
    };
    assert_eq!(error.code, AttachErrorCode::SessionOccupied);
    let occupied = error.occupied.unwrap();
    assert_eq!(occupied.occupant.as_deref(), Some(alice.actor_id.as_str()));
    assert_eq!(occupied.lease_epoch, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_attach_has_exactly_one_exclusive_lease() {
    let fronts: Vec<Front> = (0..8)
        .map(|index| Front::new(&format!("f{index}")))
        .collect();
    let fixture = Arc::new(Fixture::new(&fronts));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, front) in fronts.into_iter().enumerate() {
        let fixture = fixture.clone();
        tasks.spawn(async move {
            let channel = make_channel(&front, &format!("channel-{index}"), 2_000);
            fixture
                .service
                .attach(
                    attach_request(&front, &channel, &format!("attach-{index}")),
                    &channel,
                )
                .await
        });
    }
    let mut successes = 0;
    let mut occupied = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            Ok(_) => successes += 1,
            Err(error) => {
                assert_eq!(error.code, AttachErrorCode::SessionOccupied);
                occupied += 1;
            }
        }
    }
    assert_eq!(successes, 1);
    assert_eq!(occupied, 7);
}

fn takeover_request(front: &Front, old: &AttachOutcome, attach: &AttachRequest) -> TakeoverRequest {
    let mut request = TakeoverRequest {
        session_id: old.response.session_id.clone(),
        lease_epoch: old.response.lease_epoch,
        requesting_front_actor_id: front.actor_id.clone(),
        takeover_proof: Vec::new(),
    };
    request.takeover_proof = front
        .key
        .sign(&takeover_signing_bytes(&request, attach))
        .to_bytes()
        .to_vec();
    request
}

#[tokio::test]
async fn unauthorized_takeover_leaves_old_session_usable() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    let alice_channel = make_channel(&alice, "a", 2_000);
    let old = fixture
        .service
        .attach(attach_request(&alice, &alice_channel, "a1"), &alice_channel)
        .await
        .unwrap();
    let bob_channel = make_channel(&bob, "b", 2_000);
    let attach = attach_request(&bob, &bob_channel, "b1");
    let takeover = takeover_request(&bob, &old, &attach);
    assert_code(
        fixture
            .service
            .takeover(takeover, attach, &bob_channel)
            .await,
        AttachErrorCode::TakeoverDenied,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&old, &alice_channel, "old-still-valid", READ),
            &alice_channel,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn authorized_takeover_invalidates_old_session_and_channel() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    fixture.policies.takeover.0.store(true, Ordering::SeqCst);
    let alice_channel = make_channel(&alice, "a", 2_000);
    let old = fixture
        .service
        .attach(attach_request(&alice, &alice_channel, "a1"), &alice_channel)
        .await
        .unwrap();
    let bob_channel = make_channel(&bob, "b", 2_000);
    let attach = attach_request(&bob, &bob_channel, "b1");
    let takeover = takeover_request(&bob, &old, &attach);
    let new = fixture
        .service
        .takeover(takeover, attach, &bob_channel)
        .await
        .unwrap();
    assert_eq!(new.response.lease_epoch, old.response.lease_epoch + 1);
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&old, &alice_channel, "old-revoked", READ),
                &alice_channel,
            )
            .await,
        AttachErrorCode::SessionExpired,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&new, &bob_channel, "new-active", WRITE),
            &bob_channel,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn heartbeat_does_not_extend_credential_or_refresh_role_without_reauthentication() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 1_100);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .policies
        .membership
        .set_role(&front, MemberRole::Viewer);
    fixture.clock.advance(10);
    let renewed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: outcome.response.lease_epoch,
            },
            &channel,
            None,
        )
        .await
        .unwrap();
    assert_eq!(renewed.credential_expires_at_epoch_secs, 1_100);
    assert_eq!(renewed.lease_epoch, 1);
    let session = fixture
        .service
        .session(&outcome.response.session_id)
        .await
        .unwrap();
    assert_eq!(session.role, MemberRole::Owner);
    assert_eq!(session.credential_expires_at_epoch_secs, 1_100);

    fixture.clock.set(1_100);
    assert_code(
        fixture
            .service
            .heartbeat(
                HeartbeatRequest {
                    session_id: outcome.response.session_id,
                    lease_epoch: 1,
                },
                &channel,
                None,
            )
            .await,
        AttachErrorCode::ChannelExpired,
    );
}

#[tokio::test]
async fn renewed_credential_rereads_role_and_advances_channel_epoch() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 1_100);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .policies
        .membership
        .set_role(&front, MemberRole::Viewer);
    let new_channel = make_channel(&front, "b", 2_000);
    let calls_before = fixture.policies.attach.calls.load(Ordering::SeqCst);
    let renewed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: outcome.response.lease_epoch,
            },
            &channel,
            Some(&new_channel),
        )
        .await
        .unwrap();
    assert_eq!(renewed.lease_epoch, 2);
    assert_eq!(renewed.credential_expires_at_epoch_secs, 2_000);
    assert_eq!(renewed.session_capability_ceiling, [READ]);
    assert!(fixture.policies.attach.calls.load(Ordering::SeqCst) > calls_before);
    let session = fixture
        .service
        .session(&outcome.response.session_id)
        .await
        .unwrap();
    assert_eq!(session.role, MemberRole::Viewer);
    assert_eq!(session.channel_id, "b");

    let mut current = business(&outcome, &new_channel, "new-epoch", READ);
    current.lease_epoch = renewed.lease_epoch;
    current.sign(&outcome.session_secret);
    fixture
        .service
        .guard()
        .authorize(current, &new_channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn resume_requires_new_channel_and_old_session_proof_and_rejects_old_epoch() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .service
        .handle_disconnect(&outcome.response.session_id, 1)
        .await
        .unwrap();
    let new_channel = make_channel(&front, "b", 2_000);
    let resume = resume_request(&outcome, &new_channel);
    let resumed = fixture
        .service
        .resume(resume.clone(), &new_channel)
        .await
        .unwrap();
    assert_eq!(resumed.response.lease_epoch, 2);
    assert_eq!(resumed.response.session_id, outcome.response.session_id);
    verify_attach_response(
        &resumed.response,
        &resume_digest(&resume),
        &fixture.service_key.verifying_key().to_bytes(),
    )
    .unwrap();

    let mut stale = business(&resumed, &new_channel, "stale-epoch", READ);
    stale.lease_epoch = 1;
    stale.sign(&resumed.session_secret);
    assert_code(
        fixture.service.guard().authorize(stale, &new_channel).await,
        AttachErrorCode::SessionEpochMismatch,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&resumed, &new_channel, "resumed", READ),
            &new_channel,
        )
        .await
        .unwrap();
    assert_code(
        fixture.service.resume(resume, &new_channel).await,
        AttachErrorCode::SessionEpochMismatch,
    );
}

#[tokio::test]
async fn old_channel_cannot_impersonate_the_resumed_channel_or_dequeue() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel_c1 = make_channel(&front, "c1", 2_000);
    let attached = fixture
        .service
        .attach(
            attach_request(&front, &channel_c1, "attach-c1"),
            &channel_c1,
        )
        .await
        .unwrap();
    let queued_on_c1 = fixture
        .service
        .guard()
        .authorize(
            business(&attached, &channel_c1, "queued-on-c1", READ),
            &channel_c1,
        )
        .await
        .unwrap();

    fixture
        .service
        .handle_disconnect(&attached.response.session_id, 1)
        .await
        .unwrap();
    let channel_c2 = make_channel(&front, "c2", 2_000);
    let resumed = fixture
        .service
        .resume(resume_request(&attached, &channel_c2), &channel_c2)
        .await
        .unwrap();

    // C1 has the resumed session secret and can sign a request that claims C2.
    // The trusted arrival channel still says C1, so the forged id must fail.
    let forged = business(&resumed, &channel_c2, "c1-forges-c2", READ);
    assert_code(
        fixture.service.guard().authorize(forged, &channel_c1).await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    // Even rewriting the old context's public id to C2 leaves its transcript
    // and generation bound to C1 and cannot make it current.
    let mut renamed_c1 = channel_c1.clone();
    renamed_c1.channel_id = channel_c2.channel_id.clone();
    let forged = business(&resumed, &channel_c2, "renamed-c1", READ);
    assert_code(
        fixture.service.guard().authorize(forged, &renamed_c1).await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    assert_code(
        fixture
            .service
            .guard()
            .dequeue(queued_on_c1, &channel_c2)
            .await,
        AttachErrorCode::ChannelIdentityMismatch,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&resumed, &channel_c2, "actual-c2", READ),
            &channel_c2,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn guard_rejects_transcript_runtime_and_expiry_forgery() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "c1", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "attach-c1"), &channel)
        .await
        .unwrap();

    let cases: Vec<(&str, AttachErrorCode, ChannelMutation)> = vec![
        (
            "transcript",
            AttachErrorCode::ChannelIdentityMismatch,
            |channel| {
                channel.channel_transcript_digest[0] ^= 1;
            },
        ),
        (
            "local-runtime",
            AttachErrorCode::ChannelTargetMismatch,
            |channel| {
                channel.local_runtime_instance_id = "new-runtime".into();
            },
        ),
        (
            "peer-runtime",
            AttachErrorCode::ChannelIdentityMismatch,
            |channel| {
                channel.peer_runtime_instance_id = "new-front-runtime".into();
            },
        ),
        (
            "expiry",
            AttachErrorCode::ChannelIdentityMismatch,
            |channel| {
                channel.peer_credential_expires_at_epoch_secs += 1;
            },
        ),
    ];
    for (name, expected, mutate) in cases {
        let mut forged_channel = channel.clone();
        mutate(&mut forged_channel);
        let request = business(&outcome, &channel, name, READ);
        assert_code(
            fixture
                .service
                .guard()
                .authorize(request, &forged_channel)
                .await,
            expected,
        );
    }

    fixture.clock.set(2_000);
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&outcome, &channel, "expired-channel", READ),
                &channel,
            )
            .await,
        AttachErrorCode::ChannelExpired,
    );
}

#[tokio::test]
async fn expiry_and_queued_dequeue_recheck_prevent_execution() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    let queued = fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "queued-before-expiry", READ),
            &channel,
        )
        .await
        .unwrap();
    fixture.clock.set(1_030);
    assert_code(
        fixture.service.guard().dequeue(queued, &channel).await,
        AttachErrorCode::SessionExpired,
    );
}

#[tokio::test]
async fn guard_revocation_and_dequeue_recheck_are_enforced() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    let queued = fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "queued-revoked", WRITE),
            &channel,
        )
        .await
        .unwrap();
    fixture.policies.request.0.store(false, Ordering::SeqCst);
    assert_code(
        fixture.service.guard().dequeue(queued, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&outcome, &channel, "dispatch-revoked", READ),
                &channel,
            )
            .await,
        AttachErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn guard_checks_route_scope_proof_and_replay_before_dispatch() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();

    let mut wrong_route = business(&outcome, &channel, "wrong-route", READ);
    wrong_route.route = "admin/write".into();
    wrong_route.sign(&outcome.session_secret);
    assert_code(
        fixture
            .service
            .guard()
            .authorize(wrong_route, &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );

    let mut wrong_scope = business(&outcome, &channel, "wrong-scope", WRITE);
    wrong_scope.capability = READ.into();
    wrong_scope.sign(&outcome.session_secret);
    assert_code(
        fixture
            .service
            .guard()
            .authorize(wrong_scope, &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );

    let mut bad_proof = business(&outcome, &channel, "bad-proof", READ);
    bad_proof.proof[0] ^= 1;
    assert_code(
        fixture.service.guard().authorize(bad_proof, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let replay = business(&outcome, &channel, "replay", READ);
    fixture
        .service
        .guard()
        .authorize(replay.clone(), &channel)
        .await
        .unwrap();
    assert_code(
        fixture.service.guard().authorize(replay, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn response_proofs_cover_the_actual_kind_and_success_or_error_payload() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();

    let mut request = business(&outcome, &channel, "response-request", READ);
    request.idempotency_key = None;
    request.sign(&outcome.session_secret);
    let mut inherited = request.clone();
    inherited.request_id = "inherited-response-proof".into();
    inherited.kind = MessageKind::Response;
    inherited.payload = b"ok".to_vec();
    assert_code(
        fixture.service.guard().authorize(inherited, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut success = request.clone();
    success.request_id = "success-response".into();
    success.kind = MessageKind::Response;
    success.payload = b"ok".to_vec();
    success.sign(&outcome.session_secret);
    assert_ne!(success.proof, request.proof);

    let mut tampered_kind = success.clone();
    tampered_kind.request_id = "tampered-response-kind".into();
    tampered_kind.kind = MessageKind::Tell;
    assert_code(
        fixture
            .service
            .guard()
            .authorize(tampered_kind, &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );
    let mut tampered_payload = success.clone();
    tampered_payload.request_id = "tampered-response-payload".into();
    tampered_payload.payload = b"OK".to_vec();
    assert_code(
        fixture
            .service
            .guard()
            .authorize(tampered_payload, &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .service
        .guard()
        .authorize(success, &channel)
        .await
        .unwrap();

    let error_payload = vec![0x08, 0x07, 0x12, 0x06, b'd', b'e', b'n', b'i', b'e', b'd'];
    let mut error = request;
    error.request_id = "error-response".into();
    error.kind = MessageKind::Response;
    error.payload = error_payload;
    error.sign(&outcome.session_secret);
    let mut tampered_error = error.clone();
    tampered_error.request_id = "tampered-error-response".into();
    tampered_error.payload = vec![0x08, 0x08, 0x12, 0x06, b'd', b'e', b'n', b'i', b'e', b'd'];
    assert_code(
        fixture
            .service
            .guard()
            .authorize(tampered_error, &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .service
        .guard()
        .authorize(error, &channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn detach_releases_lease_without_stopping_service() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    let channel = make_channel(&alice, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&alice, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .service
        .detach(
            DetachRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 1,
            },
            &channel,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .service
            .status(&outcome.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Closed
    );
    let bob_channel = make_channel(&bob, "b", 2_000);
    fixture
        .service
        .attach(attach_request(&bob, &bob_channel, "b1"), &bob_channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn delayed_heartbeat_cannot_resurrect_a_grace_session() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .service
        .handle_disconnect(&outcome.response.session_id, 1)
        .await
        .unwrap();
    let error = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 1,
            },
            &channel,
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, AttachErrorCode::SessionExpired);
    assert_eq!(
        fixture
            .service
            .status(&outcome.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Grace
    );
}

#[tokio::test]
async fn crash_grace_expires_before_a_new_attach_is_allowed() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    let channel = make_channel(&alice, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&alice, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture
        .service
        .handle_disconnect(&outcome.response.session_id, 1)
        .await
        .unwrap();
    assert!(
        !fixture
            .service
            .expire_grace(&outcome.response.session_id)
            .await
            .unwrap()
    );
    fixture.clock.advance(5);
    assert!(
        fixture
            .service
            .expire_grace(&outcome.response.session_id)
            .await
            .unwrap()
    );
    let bob_channel = make_channel(&bob, "b", 2_000);
    fixture
        .service
        .attach(attach_request(&bob, &bob_channel, "b1"), &bob_channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn service_restart_invalidates_all_sessions_and_requires_new_runtime_channel() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    assert_code(
        fixture.service.restart("runtime-1").await,
        AttachErrorCode::ChannelTargetMismatch,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "same-runtime-restart", READ),
            &channel,
        )
        .await
        .unwrap();
    fixture.service.restart("runtime-2").await.unwrap();
    assert_eq!(
        fixture
            .service
            .status(&outcome.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Closed
    );
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&outcome, &channel, "before-restart", READ),
                &channel,
            )
            .await,
        AttachErrorCode::ChannelTargetMismatch,
    );
    assert_code(
        fixture
            .service
            .attach(attach_request(&front, &channel, "a2"), &channel)
            .await,
        AttachErrorCode::ChannelTargetMismatch,
    );
    let new_channel = channel_with_runtime(&front, "b", 2_000, "runtime-2");
    let current = fixture
        .service
        .attach(attach_request(&front, &new_channel, "a3"), &new_channel)
        .await
        .unwrap();
    assert_code(
        fixture.service.restart("runtime-1").await,
        AttachErrorCode::ChannelTargetMismatch,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&current, &new_channel, "runtime-reuse-rejected", READ),
            &new_channel,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn signature_compatibility_privacy_and_quota_fail_closed_before_lease() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);

    let mut bad_signature = attach_request(&front, &channel, "bad-signature");
    bad_signature.challenge_response[0] ^= 1;
    assert_code(
        fixture.service.attach(bad_signature, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut bad_target = attach_request(&front, &channel, "bad-target");
    bad_target.service_actor_id = "service:other".into();
    sign_attach_request(&mut bad_target, &front.key);
    assert_code(
        fixture.service.attach(bad_target, &channel).await,
        AttachErrorCode::ChannelTargetMismatch,
    );

    let mut bad_guest = attach_request(&front, &channel, "bad-guest");
    bad_guest.guest_digest[0] ^= 1;
    sign_attach_request(&mut bad_guest, &front.key);
    assert_code(
        fixture.service.attach(bad_guest, &channel).await,
        AttachErrorCode::PackageDigestMismatch,
    );

    let mut bad_protocol = attach_request(&front, &channel, "bad-protocol");
    bad_protocol.protocol_fingerprint[0] ^= 1;
    sign_attach_request(&mut bad_protocol, &front.key);
    assert_code(
        fixture.service.attach(bad_protocol, &channel).await,
        AttachErrorCode::ProtocolMismatch,
    );

    fixture
        .policies
        .privacy
        .allowed
        .store(false, Ordering::SeqCst);
    assert_code(
        fixture
            .service
            .attach(attach_request(&front, &channel, "privacy"), &channel)
            .await,
        AttachErrorCode::PrivacyPolicyUnsatisfied,
    );
    fixture
        .policies
        .privacy
        .allowed
        .store(true, Ordering::SeqCst);
    fixture
        .policies
        .quota
        .attach_allowed
        .store(false, Ordering::SeqCst);
    assert_code(
        fixture
            .service
            .attach(attach_request(&front, &channel, "quota"), &channel)
            .await,
        AttachErrorCode::RateLimited,
    );
    fixture
        .policies
        .quota
        .attach_allowed
        .store(true, Ordering::SeqCst);
    fixture.policies.attach.denied.store(true, Ordering::SeqCst);
    assert_code(
        fixture
            .service
            .attach(attach_request(&front, &channel, "authorization"), &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .policies
        .attach
        .denied
        .store(false, Ordering::SeqCst);
    fixture
        .service
        .attach(
            attach_request(&front, &channel, "success-after-denials"),
            &channel,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture.policies.quota.attach_checks.load(Ordering::SeqCst),
        2
    );
}

#[tokio::test]
async fn renewal_and_resume_reject_historical_channels_without_consuming_failed_new_channels() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel_a = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel_a, "a1"), &channel_a)
        .await
        .unwrap();
    let channel_b = make_channel(&front, "b", 2_000);
    let renewed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 1,
            },
            &channel_a,
            Some(&channel_b),
        )
        .await
        .unwrap();
    assert_eq!(renewed.lease_epoch, 2);

    assert_code(
        fixture
            .service
            .heartbeat(
                HeartbeatRequest {
                    session_id: outcome.response.session_id.clone(),
                    lease_epoch: 2,
                },
                &channel_b,
                Some(&channel_a),
            )
            .await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    let channel_c = make_channel(&front, "c", 2_000);
    fixture.policies.attach.denied.store(true, Ordering::SeqCst);
    assert_code(
        fixture
            .service
            .heartbeat(
                HeartbeatRequest {
                    session_id: outcome.response.session_id.clone(),
                    lease_epoch: 2,
                },
                &channel_b,
                Some(&channel_c),
            )
            .await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .policies
        .attach
        .denied
        .store(false, Ordering::SeqCst);
    let session = fixture
        .service
        .session(&outcome.response.session_id)
        .await
        .unwrap();
    assert_eq!(session.channel_id, "b");
    assert_eq!(session.lease_epoch, 2);

    // The denied renewal did not consume C, so a later valid renewal can use it.
    let renewed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 2,
            },
            &channel_b,
            Some(&channel_c),
        )
        .await
        .unwrap();
    assert_eq!(renewed.lease_epoch, 3);
    fixture
        .service
        .handle_disconnect(&outcome.response.session_id, 3)
        .await
        .unwrap();

    let mut old_resume = resume_request(&outcome, &channel_a);
    old_resume.lease_epoch = 3;
    old_resume.session_proof = resume_proof(outcome.session_secret.as_bytes(), &old_resume);
    assert_code(
        fixture.service.resume(old_resume, &channel_a).await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    let channel_d = make_channel(&front, "d", 2_000);
    let mut resume = resume_request(&outcome, &channel_d);
    resume.lease_epoch = 3;
    resume.session_proof = resume_proof(outcome.session_secret.as_bytes(), &resume);
    let resumed = fixture.service.resume(resume, &channel_d).await.unwrap();
    assert_eq!(resumed.response.lease_epoch, 4);
    fixture
        .service
        .guard()
        .authorize(
            business(&resumed, &channel_d, "fresh-resume", READ),
            &channel_d,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn empty_capability_ceiling_stays_empty_across_renewal_and_resume() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel_a = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel_a, "a1"), &channel_a)
        .await
        .unwrap();

    fixture.policies.attach.capabilities.lock().unwrap().clear();
    let channel_b = make_channel(&front, "b", 2_000);
    let narrowed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 1,
            },
            &channel_a,
            Some(&channel_b),
        )
        .await
        .unwrap();
    assert!(narrowed.session_capability_ceiling.is_empty());

    *fixture.policies.attach.capabilities.lock().unwrap() =
        BTreeSet::from([READ.into(), WRITE.into()]);
    let channel_c = make_channel(&front, "c", 2_000);
    let renewed = fixture
        .service
        .heartbeat(
            HeartbeatRequest {
                session_id: outcome.response.session_id.clone(),
                lease_epoch: 2,
            },
            &channel_b,
            Some(&channel_c),
        )
        .await
        .unwrap();
    assert!(renewed.session_capability_ceiling.is_empty());

    fixture
        .service
        .handle_disconnect(&outcome.response.session_id, 3)
        .await
        .unwrap();
    let channel_d = make_channel(&front, "d", 2_000);
    let mut resume = resume_request(&outcome, &channel_d);
    resume.lease_epoch = 3;
    resume.session_proof = resume_proof(outcome.session_secret.as_bytes(), &resume);
    let resumed = fixture.service.resume(resume, &channel_d).await.unwrap();
    assert!(resumed.response.session_capability_ceiling.is_empty());
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&resumed, &channel_d, "still-no-capability", READ),
                &channel_d,
            )
            .await,
        AttachErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn guard_rejects_idempotency_key_reuse_but_allows_original_dequeue() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    let first = business(&outcome, &channel, "first-request", READ);
    let queued = fixture
        .service
        .guard()
        .authorize(first.clone(), &channel)
        .await
        .unwrap();

    let mut duplicate = business(&outcome, &channel, "second-request", READ);
    duplicate.idempotency_key = first.idempotency_key.clone();
    duplicate.sign(&outcome.session_secret);
    assert_code(
        fixture
            .service
            .guard()
            .authorize(duplicate.clone(), &channel)
            .await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .service
        .guard()
        .dequeue(queued, &channel)
        .await
        .unwrap();
    assert_code(
        fixture.service.guard().authorize(duplicate, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut empty_key = business(&outcome, &channel, "empty-idempotency-key", READ);
    empty_key.idempotency_key = Some(String::new());
    empty_key.sign(&outcome.session_secret);
    assert_code(
        fixture.service.guard().authorize(empty_key, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn expired_status_closes_session_and_releases_exclusive_lease() {
    let alice = Front::new("alice");
    let bob = Front::new("bob");
    let fixture = Fixture::new(&[alice.clone(), bob.clone()]);
    let channel = make_channel(&alice, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&alice, &channel, "a1"), &channel)
        .await
        .unwrap();
    fixture.clock.set(1_030);

    let status = fixture
        .service
        .status(&outcome.response.session_id)
        .await
        .unwrap();
    assert_eq!(status.state, SessionState::Closed);
    assert_eq!(status.occupant, None);
    assert_eq!(
        fixture
            .service
            .session(&outcome.response.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Closed
    );

    let bob_channel = make_channel(&bob, "b", 2_000);
    fixture
        .service
        .attach(attach_request(&bob, &bob_channel, "b1"), &bob_channel)
        .await
        .unwrap();
}

#[tokio::test]
async fn attach_rechecks_credential_expiry_at_commit() {
    let front = Front::new("alice");
    let policies = Policies::new(std::slice::from_ref(&front));
    let service_key = test_key("service");
    let service = InMemorySessionService::new_with_clock(
        service_config(service_key),
        policies.erased(),
        Arc::new(AdvancingClock(AtomicU64::new(0))),
    )
    .unwrap();
    let channel = make_channel(&front, "a", 1_500);

    // The first time sample is 1000, but policy evaluation advances the commit
    // sample to 2000. The Attach must not allocate a lease with the stale sample.
    assert_code(
        service
            .attach(attach_request(&front, &channel, "a1"), &channel)
            .await,
        AttachErrorCode::ChannelExpired,
    );
    assert_eq!(policies.quota.attach_checks.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn identity_type_nonce_and_compatibility_checks_fail_before_lease() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);

    let mut wrong_front = attach_request(&front, &channel, "wrong-front");
    wrong_front.front_actor_id = "front:other".into();
    sign_attach_request(&mut wrong_front, &front.key);
    assert_code(
        fixture.service.attach(wrong_front, &channel).await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    let mut wrong_type = make_channel(&front, "wrong-type", 2_000);
    wrong_type.peer_actor_type = "actrium.guest".into();
    assert_code(
        fixture
            .service
            .attach(
                attach_request(&front, &wrong_type, "wrong-type"),
                &wrong_type,
            )
            .await,
        AttachErrorCode::ServiceActorTypeMismatch,
    );

    let mut wrong_channel = attach_request(&front, &channel, "wrong-channel");
    wrong_channel.channel_id = "another-channel".into();
    sign_attach_request(&mut wrong_channel, &front.key);
    assert_code(
        fixture.service.attach(wrong_channel, &channel).await,
        AttachErrorCode::ChannelIdentityMismatch,
    );

    let mut wrong_transcript = attach_request(&front, &channel, "wrong-transcript");
    wrong_transcript.channel_transcript_digest = test_digest("wrong-transcript");
    sign_attach_request(&mut wrong_transcript, &front.key);
    assert_code(
        fixture.service.attach(wrong_transcript, &channel).await,
        AttachErrorCode::ChannelTranscriptMismatch,
    );

    let mut short_nonce = attach_request(&front, &channel, "short-nonce");
    short_nonce.tab_session_nonce = test_digest("short-nonce")[..8].to_vec();
    sign_attach_request(&mut short_nonce, &front.key);
    assert_code(
        fixture.service.attach(short_nonce, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut wrong_program = attach_request(&front, &channel, "wrong-program");
    wrong_program.program.version = "2.0.0".into();
    sign_attach_request(&mut wrong_program, &front.key);
    assert_code(
        fixture.service.attach(wrong_program, &channel).await,
        AttachErrorCode::ProgramReleaseMismatch,
    );

    let mut wrong_package = attach_request(&front, &channel, "wrong-package");
    wrong_package.package_digest[0] ^= 1;
    sign_attach_request(&mut wrong_package, &front.key);
    assert_code(
        fixture.service.attach(wrong_package, &channel).await,
        AttachErrorCode::PackageDigestMismatch,
    );

    let mut wrong_runtime = attach_request(&front, &channel, "wrong-runtime");
    wrong_runtime.runtime.actrium_version = "0.12".into();
    sign_attach_request(&mut wrong_runtime, &front.key);
    assert_code(
        fixture.service.attach(wrong_runtime, &channel).await,
        AttachErrorCode::ProtocolMismatch,
    );

    let expired_channel = make_channel(&front, "expired", 1_000);
    assert_code(
        fixture
            .service
            .attach(
                attach_request(&front, &expired_channel, "expired"),
                &expired_channel,
            )
            .await,
        AttachErrorCode::ChannelExpired,
    );
    assert_eq!(
        fixture.policies.quota.attach_checks.load(Ordering::SeqCst),
        0
    );

    let outcome = fixture
        .service
        .attach(
            attach_request(&front, &channel, "success-after-validation"),
            &channel,
        )
        .await
        .unwrap();
    let mut replayed_nonce = attach_request(&front, &channel, "different-attach-id");
    replayed_nonce.tab_session_nonce = test_digest("attach-nonce:success-after-validation");
    sign_attach_request(&mut replayed_nonce, &front.key);
    assert_code(
        fixture.service.attach(replayed_nonce, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "validation-failures-no-lease", READ),
            &channel,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture.policies.quota.attach_checks.load(Ordering::SeqCst),
        1
    );
}

#[tokio::test]
async fn requested_capabilities_are_bounded_before_lease_allocation() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);

    let mut outside_program = attach_request(&front, &channel, "outside-program");
    outside_program.requested_capabilities = vec!["documents.admin".into()];
    sign_attach_request(&mut outside_program, &front.key);
    assert_code(
        fixture.service.attach(outside_program, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    fixture
        .policies
        .membership
        .set_role(&front, MemberRole::Viewer);
    let mut outside_role = attach_request(&front, &channel, "outside-role");
    outside_role.requested_capabilities = vec![WRITE.into()];
    sign_attach_request(&mut outside_role, &front.key);
    assert_code(
        fixture.service.attach(outside_role, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    fixture
        .policies
        .membership
        .set_role(&front, MemberRole::Owner);
    *fixture.policies.attach.capabilities.lock().unwrap() = BTreeSet::from([READ.into()]);
    let mut outside_application = attach_request(&front, &channel, "outside-application");
    outside_application.requested_capabilities = vec![WRITE.into()];
    sign_attach_request(&mut outside_application, &front.key);
    assert_code(
        fixture.service.attach(outside_application, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut duplicate = attach_request(&front, &channel, "duplicate-capability");
    duplicate.requested_capabilities = vec![READ.into(), READ.into()];
    sign_attach_request(&mut duplicate, &front.key);
    assert_code(
        fixture.service.attach(duplicate, &channel).await,
        AttachErrorCode::PermissionDenied,
    );

    let mut empty = attach_request(&front, &channel, "empty-capability");
    empty.requested_capabilities = vec![String::new()];
    sign_attach_request(&mut empty, &front.key);
    assert_code(
        fixture.service.attach(empty, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
    assert_eq!(
        fixture.policies.quota.attach_checks.load(Ordering::SeqCst),
        0
    );

    let mut allowed = attach_request(&front, &channel, "allowed-capability");
    allowed.requested_capabilities = vec![READ.into()];
    sign_attach_request(&mut allowed, &front.key);
    let outcome = fixture.service.attach(allowed, &channel).await.unwrap();
    assert_eq!(outcome.response.session_capability_ceiling, [READ]);
    assert_eq!(
        fixture.policies.quota.attach_checks.load(Ordering::SeqCst),
        1
    );
}

#[tokio::test]
async fn dequeue_rechecks_reduced_role_capability() {
    let front = Front::new("alice");
    let fixture = Fixture::new(std::slice::from_ref(&front));
    let channel = make_channel(&front, "a", 2_000);
    let outcome = fixture
        .service
        .attach(attach_request(&front, &channel, "a1"), &channel)
        .await
        .unwrap();
    let queued = fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "queued-write", WRITE),
            &channel,
        )
        .await
        .unwrap();

    fixture
        .policies
        .membership
        .set_role(&front, MemberRole::Viewer);
    assert_code(
        fixture.service.guard().dequeue(queued, &channel).await,
        AttachErrorCode::PermissionDenied,
    );
    fixture
        .service
        .guard()
        .authorize(
            business(&outcome, &channel, "read-after-reduction", READ),
            &channel,
        )
        .await
        .unwrap();
    assert_code(
        fixture
            .service
            .guard()
            .authorize(
                business(&outcome, &channel, "write-after-reduction", WRITE),
                &channel,
            )
            .await,
        AttachErrorCode::PermissionDenied,
    );
}
