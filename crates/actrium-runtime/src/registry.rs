use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use actrium_core::session::{
    AttachErrorCode, AttachRequest, AttachResponse, AuthorizeAttach as AttachAuthorization,
    DetachRequest, FrontServiceSession, HeartbeatRequest, Lease, LeaseMode, PrivacyState,
    ResumeRequest, ServiceInfo, SessionState, SessionStatus, TakeoverRequest, VerifiedChannel,
};
use rand::{RngCore, rngs::OsRng};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::ServiceConfig;
use crate::crypto::{
    attach_request_digest, attach_request_fingerprint, request_fingerprint, request_signing_bytes,
    resume_digest, resume_signing_bytes, sign_attach_response, verify_attach_request, verify_hmac,
    verify_takeover, verifying_key,
};
use crate::guard::BusinessRequest;
use crate::policy::{
    AttachQuotaContext, Clock, PrivacyContext, QuotaDecision, RequestContext, RequestQuotaContext,
    ServicePolicies, SystemClock, TakeoverContext,
};
use crate::{ConfigError, Principal, SessionError};

/// Host-only session key. It deliberately has no `Debug`, `Serialize`, or
/// `Display` implementation and is not part of `AttachResponse`.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionSecret([u8; 32]);

impl SessionSecret {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn random() -> Self {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}

#[derive(Clone)]
pub struct AttachOutcome {
    pub response: AttachResponse,
    pub session_secret: SessionSecret,
}

#[derive(Debug, Clone)]
pub struct HeartbeatOutcome {
    pub lease_epoch: u64,
    pub lease_expires_at_epoch_secs: u64,
    pub credential_expires_at_epoch_secs: u64,
    pub session_capability_ceiling: Vec<String>,
}

#[derive(Clone)]
struct SessionEntry {
    record: FrontServiceSession,
    channel: VerifiedChannel,
    #[allow(dead_code)]
    guest_digest: Vec<u8>,
    #[allow(dead_code)]
    runtime: actrium_core::session::Runtime,
    privacy: PrivacyState,
    secret: SessionSecret,
    grace_expires_at: Option<u64>,
    /// Channel IDs are single-use within a session. Renewal and Resume must
    /// authenticate a fresh generation rather than reactivate an old channel.
    used_channel_ids: HashSet<String>,
}

impl SessionEntry {
    fn hard_expiry(&self) -> u64 {
        self.record
            .credential_expires_at_epoch_secs
            .min(self.record.lease_expires_at_epoch_secs)
            .min(self.privacy.expires_at_epoch_secs)
            .min(self.grace_expires_at.unwrap_or(u64::MAX))
    }

    fn status(&self) -> SessionStatus {
        SessionStatus {
            state: self.record.state,
            lease_epoch: self.record.lease_epoch,
            expires_at_epoch_secs: self.hard_expiry(),
            occupant: matches!(
                self.record.state,
                SessionState::Active | SessionState::Grace
            )
            .then(|| self.record.front_actor_id.clone()),
        }
    }
}

#[derive(Clone)]
struct CompletedAttach {
    fingerprint: Vec<u8>,
    session_id: String,
    response: AttachResponse,
}

struct RequestUse {
    fingerprint: Vec<u8>,
    dequeued: bool,
}

#[derive(Default)]
struct State {
    runtime_instance_id: String,
    sessions: HashMap<String, SessionEntry>,
    active: Option<String>,
    completed: HashMap<String, CompletedAttach>,
    used_nonces: HashSet<Vec<u8>>,
    requests: HashMap<(String, String), RequestUse>,
    idempotency_keys: HashMap<(String, String), String>,
    runtime_instance_ids: HashSet<String>,
    next_epoch: u64,
}

struct Authorization {
    principal: Principal,
    attach_context: AttachAuthorization,
    capabilities: BTreeSet<String>,
}

/// A linearizable, in-memory registry for one exact Service Actor.
///
/// All lease mutations and all Guard checks use the same async mutex. Policy
/// callbacks execute while that mutex is held, so check-and-commit cannot race.
#[derive(Clone)]
pub struct InMemorySessionRegistry {
    config: Arc<ServiceConfig>,
    policies: ServicePolicies,
    clock: Arc<dyn Clock>,
    state: Arc<Mutex<State>>,
}

impl InMemorySessionRegistry {
    pub fn new(config: ServiceConfig, policies: ServicePolicies) -> Result<Self, ConfigError> {
        Self::new_with_clock(config, policies, Arc::new(SystemClock))
    }

    pub fn new_with_clock(
        config: ServiceConfig,
        policies: ServicePolicies,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ConfigError> {
        config.validate()?;
        let runtime_instance_id = config.runtime_instance_id.clone();
        Ok(Self {
            config: Arc::new(config),
            policies,
            clock,
            state: Arc::new(Mutex::new(State {
                runtime_instance_ids: HashSet::from([runtime_instance_id.clone()]),
                runtime_instance_id,
                next_epoch: 1,
                ..State::default()
            })),
        })
    }

    fn now(&self) -> u64 {
        self.clock.now_epoch_secs()
    }

    fn validate_channel(
        &self,
        channel: &VerifiedChannel,
        state: &State,
        now: u64,
    ) -> Result<(), SessionError> {
        if channel.local_runtime_instance_id != state.runtime_instance_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTargetMismatch,
                "the channel belongs to a different Service runtime instance",
            ));
        }
        if !channel.is_valid_at(now) {
            return Err(SessionError::new(
                AttachErrorCode::ChannelExpired,
                "the Front credential on this channel has expired",
            ));
        }
        if channel.channel_transcript_digest.len() != 32 {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTranscriptMismatch,
                "the channel transcript is not a SHA-256 digest",
            ));
        }
        // Parsing is mandatory even before signature verification; arbitrary
        // credential bytes must not be treated as an authenticated public key.
        verifying_key(&channel.peer_actor_public_key)?;
        Ok(())
    }

    fn validate_front_type(&self, channel: &VerifiedChannel) -> Result<(), SessionError> {
        if !self
            .config
            .allowed_front_actor_types
            .contains(&channel.peer_actor_type)
        {
            return Err(SessionError::new(
                AttachErrorCode::ServiceActorTypeMismatch,
                "the authenticated peer is not an allowed Front Actor type",
            ));
        }
        Ok(())
    }

    async fn authorize(
        &self,
        channel: &VerifiedChannel,
        program: actrium_core::session::ProgramRelease,
        requested: &[String],
        strict_requested_scope: bool,
    ) -> Result<Authorization, SessionError> {
        let principal = self
            .policies
            .membership
            .principal(
                &channel.peer_actor_id,
                &channel.peer_member,
                &channel.peer_client_key,
            )
            .await?
            .ok_or_else(|| SessionError::denied("the Front is not a current Realm member"))?;
        if principal.member != channel.peer_member
            || principal.client_key != channel.peer_client_key
        {
            return Err(SessionError::denied(
                "the membership provider returned a different principal",
            ));
        }
        let requested_set: BTreeSet<String> = requested.iter().cloned().collect();
        if requested_set.len() != requested.len() || requested_set.iter().any(String::is_empty) {
            return Err(SessionError::denied(
                "requestedCapabilities must be unique and non-empty",
            ));
        }
        let attach_context = AttachAuthorization {
            front_actor_id: channel.peer_actor_id.clone(),
            member: principal.member.clone(),
            client_key: principal.client_key.clone(),
            role: principal.role,
            program,
            requested_capabilities: requested.to_vec(),
        };
        let decision = self
            .policies
            .authorize_attach
            .authorize_attach(attach_context.clone())
            .await?;
        if !decision.allowed {
            return Err(SessionError::denied(
                decision
                    .denial_reason
                    .unwrap_or_else(|| "the application denied Attach".into()),
            ));
        }
        let application: BTreeSet<String> = decision.allowed_capabilities.into_iter().collect();
        if strict_requested_scope {
            for capability in &requested_set {
                if !self.config.program_capabilities.contains(capability) {
                    return Err(SessionError::denied(format!(
                        "capability {capability} is outside Program policy"
                    )));
                }
                if !principal.capabilities.contains(capability) {
                    return Err(SessionError::denied(format!(
                        "capability {capability} is outside the current member role"
                    )));
                }
                if !application.contains(capability) {
                    return Err(SessionError::denied(format!(
                        "capability {capability} was not granted by the application"
                    )));
                }
            }
        }
        let capabilities = if requested.is_empty() && strict_requested_scope {
            self.config
                .program_capabilities
                .intersection(&principal.capabilities)
                .filter(|capability| application.contains(*capability))
                .cloned()
                .collect()
        } else if strict_requested_scope {
            requested_set
        } else {
            requested_set
                .intersection(&self.config.program_capabilities)
                .filter(|capability| {
                    principal.capabilities.contains(*capability)
                        && application.contains(*capability)
                })
                .cloned()
                .collect()
        };
        Ok(Authorization {
            principal,
            attach_context,
            capabilities,
        })
    }

    async fn privacy(
        &self,
        authorization: &Authorization,
        channel: &VerifiedChannel,
        session_id: Option<String>,
        route: Option<String>,
        now: u64,
        strict_requested_scope: bool,
    ) -> Result<(PrivacyState, BTreeSet<String>), SessionError> {
        let decision = self
            .policies
            .privacy
            .evaluate(PrivacyContext {
                front_actor_id: channel.peer_actor_id.clone(),
                member: authorization.principal.member.clone(),
                client_key: authorization.principal.client_key.clone(),
                role: authorization.principal.role,
                requested_capabilities: authorization.capabilities.clone(),
                channel: channel.clone(),
                session_id,
                route,
            })
            .await?;
        if decision.state.expires_at_epoch_secs <= now {
            return Err(SessionError::new(
                AttachErrorCode::PrivacyPolicyUnsatisfied,
                "the current privacy or assurance state has expired",
            ));
        }
        if strict_requested_scope
            && authorization
                .capabilities
                .iter()
                .any(|capability| !decision.allowed_capabilities.contains(capability))
        {
            return Err(SessionError::new(
                AttachErrorCode::PrivacyPolicyUnsatisfied,
                "privacy policy does not permit every requested capability",
            ));
        }
        let capabilities = authorization
            .capabilities
            .intersection(&decision.allowed_capabilities)
            .cloned()
            .collect();
        Ok((decision.state, capabilities))
    }

    fn response(
        &self,
        session_id: String,
        epoch: u64,
        lease: Lease,
        capabilities: BTreeSet<String>,
        privacy: PrivacyState,
        request_digest: &[u8],
    ) -> AttachResponse {
        let mut response = AttachResponse {
            session_id,
            lease_epoch: epoch,
            lease,
            service: ServiceInfo {
                actor_id: self.config.service_actor_id.clone(),
                program: self.config.program_string(),
                package_digest: self.config.package_digest.clone(),
                protocol_fingerprint: self.config.protocol_fingerprint.clone(),
            },
            session_capability_ceiling: capabilities.into_iter().collect(),
            privacy,
            service_challenge: Vec::new(),
        };
        sign_attach_response(
            &mut response,
            request_digest,
            &self.config.service_signing_key,
        );
        response
    }

    pub async fn attach(
        &self,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        self.attach_internal(request, channel, None).await
    }

    pub async fn takeover(
        &self,
        takeover: TakeoverRequest,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        self.attach_internal(request, channel, Some(takeover)).await
    }

    async fn attach_internal(
        &self,
        request: AttachRequest,
        channel: &VerifiedChannel,
        takeover: Option<TakeoverRequest>,
    ) -> Result<AttachOutcome, SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();

        // Steps 1-2: authenticated credential and exact caller identity.
        self.validate_channel(channel, &state, now)?;
        if request.front_actor_id != channel.peer_actor_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelIdentityMismatch,
                "Attach frontActorId differs from the verified peer",
            ));
        }

        // Step 3: only explicitly allowed Front Actor types may attach.
        self.validate_front_type(channel)?;

        let fingerprint = attach_request_fingerprint(&request);
        let request_digest = attach_request_digest(&request);
        let idempotent = match state.completed.get(&request.attach_id) {
            Some(completed) if completed.fingerprint != fingerprint => {
                return Err(SessionError::denied(
                    "attachId was already used for different request content",
                ));
            }
            Some(_) => true,
            None => false,
        };

        // Step 4: channel binding, fresh nonce, and a real Actor signature over
        // the canonical request and transcript digest.
        if request.attach_id.is_empty() {
            return Err(SessionError::denied("attachId must not be empty"));
        }
        if request.channel_id != channel.channel_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelIdentityMismatch,
                "Attach channelId differs from the verified channel",
            ));
        }
        if request.channel_transcript_digest != channel.channel_transcript_digest
            || request.channel_transcript_digest.len() != 32
        {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTranscriptMismatch,
                "Attach transcript differs from the verified channel",
            ));
        }
        if request.tab_session_nonce.len() < 16 {
            return Err(SessionError::denied("tabSessionNonce is too short"));
        }
        verify_attach_request(&request, &channel.peer_actor_public_key)?;
        if state.used_nonces.contains(&request.tab_session_nonce) && !idempotent {
            return Err(SessionError::denied(
                "tabSessionNonce has already been used",
            ));
        }

        // Step 5: exact Service Actor target; type-only discovery is insufficient.
        if request.service_actor_id != self.config.service_actor_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTargetMismatch,
                "Attach targets a different Service ActorId",
            ));
        }

        // Steps 6-8: immutable Program, package/Guest, protocol, and one runtime.
        if !self.config.program_matches(&request.program) {
            return Err(SessionError::new(
                AttachErrorCode::ProgramReleaseMismatch,
                "Program Release does not match the Service",
            ));
        }
        if request.package_digest != self.config.package_digest {
            return Err(SessionError::new(
                AttachErrorCode::PackageDigestMismatch,
                "package digest does not match the Service",
            ));
        }
        if request.guest_digest != self.config.guest_digest {
            return Err(SessionError::new(
                AttachErrorCode::PackageDigestMismatch,
                "Guest digest does not match the Service package",
            ));
        }
        if request.protocol_fingerprint != self.config.protocol_fingerprint
            || !self.config.runtime_is_supported(&request.runtime)
        {
            return Err(SessionError::new(
                AttachErrorCode::ProtocolMismatch,
                "protocol fingerprint or runtime contract is incompatible",
            ));
        }

        if idempotent {
            let completed = state
                .completed
                .get(&request.attach_id)
                .expect("checked above");
            let entry = state
                .sessions
                .get(&completed.session_id)
                .ok_or_else(|| SessionError::expired("the idempotent session no longer exists"))?;
            if entry.record.state != SessionState::Active
                || entry.hard_expiry() <= now
                || entry.record.channel_id != request.channel_id
                || entry.record.lease_epoch != completed.response.lease_epoch
            {
                return Err(SessionError::expired(
                    "the original idempotent Attach session is no longer active",
                ));
            }
            return Ok(AttachOutcome {
                response: completed.response.clone(),
                session_secret: entry.secret.clone(),
            });
        }

        // Step 9: current membership, application authorization, and role,
        // Program, and application capability bounds.
        let authorization = self
            .authorize(
                channel,
                request.program.clone(),
                &request.requested_capabilities,
                true,
            )
            .await?;

        // Step 10: v1 admits only the configured exclusive attachment policy.
        if request.requested_lease != LeaseMode::Exclusive
            || self.config.attachment_policy != crate::AttachmentPolicy::Exclusive
        {
            return Err(SessionError::denied(
                "attachment policy requires an exclusive lease",
            ));
        }

        // Step 11: observe and authorize takeover while holding the lease lock.
        // The actual old-session invalidation happens only in the final commit.
        let active_unexpired = state.active.as_ref().and_then(|session_id| {
            state.sessions.get(session_id).and_then(|entry| {
                (matches!(
                    entry.record.state,
                    SessionState::Active | SessionState::Grace
                ) && entry.hard_expiry() > now)
                    .then(|| (session_id.clone(), entry.clone()))
            })
        });
        let takeover_epoch = if let Some((old_session_id, old_entry)) = &active_unexpired {
            let takeover = takeover
                .as_ref()
                .ok_or_else(|| SessionError::occupied(old_entry.status()))?;
            if takeover.session_id != *old_session_id
                || takeover.lease_epoch != old_entry.record.lease_epoch
                || takeover.requesting_front_actor_id != request.front_actor_id
            {
                return Err(SessionError::new(
                    AttachErrorCode::TakeoverDenied,
                    "Takeover does not identify the current lease and requesting Front",
                ));
            }
            let key = verifying_key(&channel.peer_actor_public_key)?;
            verify_takeover(takeover, &request, &key)?;
            let allowed = self
                .policies
                .authorize_takeover
                .authorize_takeover(TakeoverContext {
                    existing_session: old_entry.record.clone(),
                    requesting: authorization.attach_context.clone(),
                })
                .await?;
            if !allowed {
                return Err(SessionError::new(
                    AttachErrorCode::TakeoverDenied,
                    "the independent Takeover policy denied the request",
                ));
            }
            Some(old_entry.record.lease_epoch + 1)
        } else {
            if takeover.is_some() {
                return Err(SessionError::expired(
                    "the Takeover target is not an active unexpired lease",
                ));
            }
            None
        };

        // Step 12: current state, assurance, privacy, and protected capabilities.
        let (privacy, ceiling) = self
            .privacy(&authorization, channel, None, None, now, true)
            .await?;

        // Step 13: resource and rate quotas are checked under the same lock.
        let quota = self
            .policies
            .quota
            .check_attach(AttachQuotaContext {
                front_actor_id: request.front_actor_id.clone(),
                active_session_count: usize::from(active_unexpired.is_some()),
                retained_session_count: state.sessions.len(),
                requested_capabilities: ceiling.clone(),
            })
            .await?;
        ensure_quota(quota)?;

        // Policy callbacks are async and may take long enough for a credential
        // or privacy state to expire. Revalidate at the commit instant rather
        // than granting a lease using the time sampled before those callbacks.
        let now = self.now();
        self.validate_channel(channel, &state, now)?;

        // Compute the final ceiling before allocating the lease, then atomically
        // invalidate any replaced session and publish exactly one active lease.
        let lease_expiry = now
            .saturating_add(self.config.lease_duration.as_secs())
            .min(channel.peer_credential_expires_at_epoch_secs)
            .min(privacy.expires_at_epoch_secs);
        if lease_expiry <= now {
            return Err(SessionError::expired("no positive lease lifetime remains"));
        }
        let epoch = takeover_epoch.unwrap_or(state.next_epoch);
        state.next_epoch = state.next_epoch.max(epoch + 1);
        if let Some(active_id) = state.active.take()
            && let Some(previous) = state.sessions.get_mut(&active_id)
        {
            previous.record.state = SessionState::Closed;
            previous.grace_expires_at = None;
        }
        let session_id = Uuid::new_v4().to_string();
        let secret = SessionSecret::random();
        let lease = Lease {
            mode: LeaseMode::Exclusive,
            expires_at_epoch_secs: lease_expiry,
            heartbeat_interval_ms: self.config.heartbeat_interval.as_millis() as u32,
        };
        let record = FrontServiceSession {
            session_id: session_id.clone(),
            lease_epoch: epoch,
            front_actor_id: request.front_actor_id.clone(),
            service_actor_id: request.service_actor_id.clone(),
            channel_id: request.channel_id.clone(),
            channel_transcript_digest: request.channel_transcript_digest.clone(),
            member: authorization.principal.member.clone(),
            client_key: authorization.principal.client_key.clone(),
            role: authorization.principal.role,
            program: request.program.clone(),
            package_digest: request.package_digest.clone(),
            protocol_fingerprint: request.protocol_fingerprint.clone(),
            session_capability_ceiling: ceiling.iter().cloned().collect(),
            lease: lease.clone(),
            state: SessionState::Active,
            credential_expires_at_epoch_secs: channel.peer_credential_expires_at_epoch_secs,
            lease_expires_at_epoch_secs: lease_expiry,
        };
        let response = self.response(
            session_id.clone(),
            epoch,
            lease,
            ceiling,
            privacy.clone(),
            &request_digest,
        );
        state.sessions.insert(
            session_id.clone(),
            SessionEntry {
                record,
                channel: channel.clone(),
                guest_digest: request.guest_digest.clone(),
                runtime: request.runtime.clone(),
                privacy,
                secret: secret.clone(),
                grace_expires_at: None,
                used_channel_ids: HashSet::from([channel.channel_id.clone()]),
            },
        );
        state.active = Some(session_id.clone());
        state.used_nonces.insert(request.tab_session_nonce.clone());
        state.completed.insert(
            request.attach_id.clone(),
            CompletedAttach {
                fingerprint,
                session_id,
                response: response.clone(),
            },
        );
        Ok(AttachOutcome {
            response,
            session_secret: secret,
        })
    }

    fn session_channel_error() -> SessionError {
        SessionError::new(
            AttachErrorCode::ChannelIdentityMismatch,
            "the request is not bound to the session's current channel",
        )
    }

    fn ensure_live(entry: &SessionEntry, now: u64, allow_grace: bool) -> Result<(), SessionError> {
        if entry.record.state != SessionState::Active
            && !(allow_grace && entry.record.state == SessionState::Grace)
        {
            return Err(SessionError::expired("the session is not active"));
        }
        if entry.hard_expiry() <= now {
            return Err(SessionError::expired("a session hard deadline has expired"));
        }
        Ok(())
    }

    /// Heartbeat extends only the lease. Without a newly authenticated channel
    /// it never changes credential expiry, role, capabilities, or channel epoch.
    pub async fn heartbeat(
        &self,
        request: HeartbeatRequest,
        channel: &VerifiedChannel,
        renewed_channel: Option<&VerifiedChannel>,
    ) -> Result<HeartbeatOutcome, SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        self.validate_channel(channel, &state, now)?;
        let old = state
            .sessions
            .get(&request.session_id)
            .cloned()
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        Self::ensure_live(&old, now, true)?;
        if old.record.lease_epoch != request.lease_epoch {
            return Err(SessionError::epoch());
        }
        if old.record.channel_id != channel.channel_id
            || old.record.channel_transcript_digest != channel.channel_transcript_digest
            || old.record.front_actor_id != channel.peer_actor_id
            || old.record.member != channel.peer_member
            || old.record.client_key != channel.peer_client_key
        {
            return Err(Self::session_channel_error());
        }

        let (new_channel, principal, ceiling, privacy, epoch) =
            if let Some(new_channel) = renewed_channel {
                if new_channel.channel_id == old.record.channel_id {
                    return Err(SessionError::denied(
                        "credential renewal requires reauthentication on a new channel",
                    ));
                }
                if old.used_channel_ids.contains(&new_channel.channel_id) {
                    return Err(SessionError::new(
                        AttachErrorCode::ChannelIdentityMismatch,
                        "credential renewal cannot reuse an old channel generation",
                    ));
                }
                self.validate_channel(new_channel, &state, now)?;
                self.validate_front_type(new_channel)?;
                if new_channel.peer_actor_id != old.record.front_actor_id
                    || new_channel.peer_member != old.record.member
                    || new_channel.peer_client_key != old.record.client_key
                {
                    return Err(SessionError::new(
                        AttachErrorCode::ChannelIdentityMismatch,
                        "renewed credential changed the session principal",
                    ));
                }
                let authorization = self
                    .authorize(
                        new_channel,
                        old.record.program.clone(),
                        &old.record.session_capability_ceiling,
                        false,
                    )
                    .await?;
                let (privacy, ceiling) = self
                    .privacy(
                        &authorization,
                        new_channel,
                        Some(old.record.session_id.clone()),
                        None,
                        now,
                        false,
                    )
                    .await?;
                (
                    new_channel.clone(),
                    Some(authorization.principal),
                    ceiling,
                    privacy,
                    old.record.lease_epoch + 1,
                )
            } else {
                (
                    old.channel.clone(),
                    None,
                    old.record
                        .session_capability_ceiling
                        .iter()
                        .cloned()
                        .collect(),
                    old.privacy.clone(),
                    old.record.lease_epoch,
                )
            };
        let now = self.now();
        Self::ensure_live(&old, now, true)?;
        self.validate_channel(&new_channel, &state, now)?;
        let credential_expiry = new_channel.peer_credential_expires_at_epoch_secs;
        let lease_expiry = now
            .saturating_add(self.config.lease_duration.as_secs())
            .min(credential_expiry)
            .min(privacy.expires_at_epoch_secs);
        if lease_expiry <= now {
            return Err(SessionError::expired(
                "credential or policy prevents lease renewal",
            ));
        }
        let entry = state
            .sessions
            .get_mut(&request.session_id)
            .expect("entry exists");
        entry.record.state = SessionState::Active;
        entry.record.lease_epoch = epoch;
        entry.record.channel_id = new_channel.channel_id.clone();
        entry.record.channel_transcript_digest = new_channel.channel_transcript_digest.clone();
        entry.record.credential_expires_at_epoch_secs = credential_expiry;
        entry.record.lease_expires_at_epoch_secs = lease_expiry;
        entry.record.lease.expires_at_epoch_secs = lease_expiry;
        entry.record.session_capability_ceiling = ceiling.iter().cloned().collect();
        if let Some(principal) = principal {
            entry.record.role = principal.role;
            entry.record.member = principal.member;
            entry.record.client_key = principal.client_key;
        }
        entry
            .used_channel_ids
            .insert(new_channel.channel_id.clone());
        entry.channel = new_channel;
        entry.privacy = privacy;
        entry.grace_expires_at = None;
        let final_capabilities = entry.record.session_capability_ceiling.clone();
        state.next_epoch = state.next_epoch.max(epoch + 1);
        Ok(HeartbeatOutcome {
            lease_epoch: epoch,
            lease_expires_at_epoch_secs: lease_expiry,
            credential_expires_at_epoch_secs: credential_expiry,
            session_capability_ceiling: final_capabilities,
        })
    }

    pub fn make_resume_proof(&self, secret: &SessionSecret, request: &ResumeRequest) -> Vec<u8> {
        crate::crypto::resume_proof(secret.as_bytes(), request)
    }

    pub async fn resume(
        &self,
        request: ResumeRequest,
        new_channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        self.validate_channel(new_channel, &state, now)?;
        self.validate_front_type(new_channel)?;
        let old = state
            .sessions
            .get(&request.session_id)
            .cloned()
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        Self::ensure_live(&old, now, true)?;
        if old.record.lease_epoch != request.lease_epoch {
            return Err(SessionError::epoch());
        }
        if new_channel.channel_id == old.record.channel_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelIdentityMismatch,
                "Resume requires a new VerifiedChannel generation",
            ));
        }
        if old.used_channel_ids.contains(&new_channel.channel_id) {
            return Err(SessionError::new(
                AttachErrorCode::ChannelIdentityMismatch,
                "Resume cannot reuse an old channel generation",
            ));
        }
        if request.channel_id != new_channel.channel_id
            || request.channel_transcript_digest != new_channel.channel_transcript_digest
            || new_channel.peer_actor_id != old.record.front_actor_id
            || new_channel.peer_member != old.record.member
            || new_channel.peer_client_key != old.record.client_key
            || new_channel.local_runtime_instance_id != state.runtime_instance_id
        {
            return Err(Self::session_channel_error());
        }
        if !verify_hmac(
            old.secret.as_bytes(),
            &resume_signing_bytes(&request),
            &request.session_proof,
        ) {
            return Err(SessionError::denied(
                "old session proof verification failed",
            ));
        }
        let authorization = self
            .authorize(
                new_channel,
                old.record.program.clone(),
                &old.record.session_capability_ceiling,
                false,
            )
            .await?;
        let (privacy, ceiling) = self
            .privacy(
                &authorization,
                new_channel,
                Some(old.record.session_id.clone()),
                None,
                now,
                false,
            )
            .await?;
        let now = self.now();
        Self::ensure_live(&old, now, true)?;
        self.validate_channel(new_channel, &state, now)?;
        let lease_expiry = now
            .saturating_add(self.config.lease_duration.as_secs())
            .min(new_channel.peer_credential_expires_at_epoch_secs)
            .min(privacy.expires_at_epoch_secs);
        if lease_expiry <= now {
            return Err(SessionError::expired(
                "Resume has no positive lease lifetime",
            ));
        }
        let epoch = old.record.lease_epoch + 1;
        let lease = Lease {
            mode: LeaseMode::Exclusive,
            expires_at_epoch_secs: lease_expiry,
            heartbeat_interval_ms: old.record.lease.heartbeat_interval_ms,
        };
        let response = self.response(
            old.record.session_id.clone(),
            epoch,
            lease.clone(),
            ceiling.clone(),
            privacy.clone(),
            &resume_digest(&request),
        );
        let entry = state
            .sessions
            .get_mut(&request.session_id)
            .expect("entry exists");
        entry.record.lease_epoch = epoch;
        entry.record.channel_id = new_channel.channel_id.clone();
        entry.record.channel_transcript_digest = new_channel.channel_transcript_digest.clone();
        entry.record.role = authorization.principal.role;
        entry.record.credential_expires_at_epoch_secs =
            new_channel.peer_credential_expires_at_epoch_secs;
        entry.record.lease_expires_at_epoch_secs = lease_expiry;
        entry.record.lease = lease;
        entry.record.session_capability_ceiling = ceiling.into_iter().collect();
        entry.record.state = SessionState::Active;
        entry
            .used_channel_ids
            .insert(new_channel.channel_id.clone());
        entry.channel = new_channel.clone();
        entry.privacy = privacy;
        entry.grace_expires_at = None;
        state.next_epoch = state.next_epoch.max(epoch + 1);
        state.active = Some(request.session_id.clone());
        Ok(AttachOutcome {
            response,
            session_secret: old.secret,
        })
    }

    pub async fn detach(
        &self,
        request: DetachRequest,
        channel: &VerifiedChannel,
    ) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        self.validate_channel(channel, &state, now)?;
        let entry = state
            .sessions
            .get(&request.session_id)
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        Self::ensure_live(entry, now, true)?;
        if entry.record.lease_epoch != request.lease_epoch {
            return Err(SessionError::epoch());
        }
        if entry.record.channel_id != channel.channel_id
            || entry.record.channel_transcript_digest != channel.channel_transcript_digest
            || entry.record.front_actor_id != channel.peer_actor_id
            || entry.record.member != channel.peer_member
            || entry.record.client_key != channel.peer_client_key
        {
            return Err(Self::session_channel_error());
        }
        let entry = state
            .sessions
            .get_mut(&request.session_id)
            .expect("entry exists");
        entry.record.state = SessionState::Closed;
        entry.grace_expires_at = None;
        if state.active.as_deref() == Some(request.session_id.as_str()) {
            state.active = None;
        }
        Ok(())
    }

    /// A transport crash moves Active to Grace without releasing the lease.
    pub async fn handle_disconnect(
        &self,
        session_id: &str,
        lease_epoch: u64,
    ) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        let entry = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        if entry.record.lease_epoch != lease_epoch {
            return Err(SessionError::epoch());
        }
        Self::ensure_live(entry, now, false)?;
        entry.record.state = SessionState::Grace;
        entry.grace_expires_at = Some(
            now.saturating_add(self.config.grace_period.as_secs())
                .min(entry.hard_expiry()),
        );
        Ok(())
    }

    /// Closes a Grace session only after its grace or hard deadline.
    pub async fn expire_grace(&self, session_id: &str) -> Result<bool, SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        let entry = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        if entry.record.state != SessionState::Grace {
            return Ok(false);
        }
        if entry.grace_expires_at.is_none_or(|deadline| deadline > now) {
            return Ok(false);
        }
        entry.record.state = SessionState::Closed;
        entry.grace_expires_at = None;
        if state.active.as_deref() == Some(session_id) {
            state.active = None;
        }
        Ok(true)
    }

    /// Service stop/restart invalidates every old session and channel binding.
    pub async fn invalidate_all(&self) {
        let mut state = self.state.lock().await;
        for entry in state.sessions.values_mut() {
            entry.record.state = SessionState::Closed;
            entry.grace_expires_at = None;
        }
        state.active = None;
    }

    pub async fn restart(
        &self,
        new_runtime_instance_id: impl Into<String>,
    ) -> Result<(), SessionError> {
        let new_runtime_instance_id = new_runtime_instance_id.into();
        let mut state = self.state.lock().await;
        if new_runtime_instance_id.is_empty()
            || state
                .runtime_instance_ids
                .contains(&new_runtime_instance_id)
        {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTargetMismatch,
                "Service restart requires a fresh runtime instance identity",
            ));
        }
        for entry in state.sessions.values_mut() {
            entry.record.state = SessionState::Closed;
            entry.grace_expires_at = None;
        }
        state.active = None;
        state
            .runtime_instance_ids
            .insert(new_runtime_instance_id.clone());
        state.runtime_instance_id = new_runtime_instance_id;
        Ok(())
    }

    fn close_expired(state: &mut State, session_id: &str, now: u64) {
        let expired = state.sessions.get(session_id).is_some_and(|entry| {
            matches!(
                entry.record.state,
                SessionState::Active | SessionState::Grace
            ) && entry.hard_expiry() <= now
        });
        if !expired {
            return;
        }
        let entry = state.sessions.get_mut(session_id).expect("checked above");
        entry.record.state = SessionState::Closed;
        entry.grace_expires_at = None;
        if state.active.as_deref() == Some(session_id) {
            state.active = None;
        }
    }

    pub async fn status(&self, session_id: &str) -> Result<SessionStatus, SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();
        Self::close_expired(&mut state, session_id, now);
        state
            .sessions
            .get(session_id)
            .map(SessionEntry::status)
            .ok_or_else(|| SessionError::expired("unknown session"))
    }

    pub async fn session(&self, session_id: &str) -> Option<FrontServiceSession> {
        let mut state = self.state.lock().await;
        let now = self.now();
        Self::close_expired(&mut state, session_id, now);
        state
            .sessions
            .get(session_id)
            .map(|entry| entry.record.clone())
    }

    pub(crate) async fn check_business_request(
        &self,
        request: &BusinessRequest,
        dequeue: bool,
    ) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        let now = self.now();

        // Guard step 1: exact Service and explicitly allowed route.
        if request.service_actor_id != self.config.service_actor_id {
            return Err(SessionError::new(
                AttachErrorCode::ChannelTargetMismatch,
                "business request targets a different Service",
            ));
        }
        let route_capabilities = self
            .config
            .routes
            .get(&request.route)
            .ok_or_else(|| SessionError::denied("route is not allowed"))?;

        // Guard steps 2-4: current channel, Active state, epoch, and every hard
        // expiry are checked before any role or capability authorization.
        let entry = state
            .sessions
            .get(&request.session_id)
            .cloned()
            .ok_or_else(|| SessionError::expired("unknown session"))?;
        if entry.record.channel_id != request.channel_id {
            return Err(Self::session_channel_error());
        }
        Self::ensure_live(&entry, now, false)?;
        if entry.record.lease_epoch != request.lease_epoch {
            return Err(SessionError::epoch());
        }

        // Step 5: re-read current membership and current application permission.
        let principal = self
            .policies
            .membership
            .principal(
                &entry.record.front_actor_id,
                &entry.record.member,
                &entry.record.client_key,
            )
            .await?
            .ok_or_else(|| SessionError::denied("session membership was revoked"))?;
        if principal.member != entry.record.member
            || principal.client_key != entry.record.client_key
        {
            return Err(SessionError::denied(
                "current principal does not match the session",
            ));
        }
        let decision = self
            .policies
            .authorize_request
            .authorize_request(RequestContext {
                session: entry.record.clone(),
                principal: principal.clone(),
                route: request.route.clone(),
                capability: request.capability.clone(),
                request_id: request.request_id.clone(),
            })
            .await?;
        if !decision.allowed {
            return Err(SessionError::denied(
                decision
                    .denial_reason
                    .unwrap_or_else(|| "the application denied this request".into()),
            ));
        }

        // Step 6: route, current role, and immutable session ceiling all apply.
        if !route_capabilities.contains(&request.capability)
            || !principal.capabilities.contains(&request.capability)
            || !entry
                .record
                .session_capability_ceiling
                .contains(&request.capability)
        {
            return Err(SessionError::denied(
                "capability is outside route, current role, or session scope",
            ));
        }

        // Step 7: HMAC proof covers all host metadata and the payload digest.
        // Replay state is updated only after every subsequent policy also passes.
        if request.request_id.is_empty()
            || !verify_hmac(
                entry.secret.as_bytes(),
                &request_signing_bytes(request),
                request_proof_bytes(request),
            )
        {
            return Err(SessionError::denied("request id or HMAC proof is invalid"));
        }
        let fingerprint = request_fingerprint(request);
        let key = (request.session_id.clone(), request.request_id.clone());
        if let Some(previous) = state.requests.get(&key) {
            if !dequeue || previous.fingerprint != fingerprint || previous.dequeued {
                return Err(SessionError::denied("request id has already been admitted"));
            }
        } else if dequeue {
            return Err(SessionError::denied(
                "queued request was not admitted by the Guard",
            ));
        }
        let idempotency_key = if let Some(value) = &request.idempotency_key {
            if value.is_empty() {
                return Err(SessionError::denied("idempotency key must not be empty"));
            }
            let key = (request.session_id.clone(), value.clone());
            match state.idempotency_keys.get(&key) {
                Some(previous_request_id) if previous_request_id != &request.request_id => {
                    return Err(SessionError::denied(
                        "idempotency key was already used by another request",
                    ));
                }
                None if dequeue => {
                    return Err(SessionError::denied(
                        "queued idempotency key was not admitted by the Guard",
                    ));
                }
                _ => {}
            }
            Some(key)
        } else {
            None
        };

        // Step 8: privacy/assurance and resource limits are rechecked at execution.
        let authorization = Authorization {
            principal: principal.clone(),
            attach_context: AttachAuthorization {
                front_actor_id: entry.record.front_actor_id.clone(),
                member: entry.record.member.clone(),
                client_key: entry.record.client_key.clone(),
                role: principal.role,
                program: entry.record.program.clone(),
                requested_capabilities: vec![request.capability.clone()],
            },
            capabilities: BTreeSet::from([request.capability.clone()]),
        };
        let (privacy, capabilities) = self
            .privacy(
                &authorization,
                &entry.channel,
                Some(entry.record.session_id.clone()),
                Some(request.route.clone()),
                now,
                true,
            )
            .await?;
        if !capabilities.contains(&request.capability) {
            return Err(SessionError::new(
                AttachErrorCode::PrivacyPolicyUnsatisfied,
                "current privacy policy revoked the request capability",
            ));
        }
        let quota = self
            .policies
            .quota
            .check_request(RequestQuotaContext {
                session_id: request.session_id.clone(),
                route: request.route.clone(),
                request_id: request.request_id.clone(),
                payload_len: request.payload.len(),
            })
            .await?;
        ensure_quota(quota)?;
        let now = self.now();
        Self::ensure_live(&entry, now, false)?;
        if privacy.expires_at_epoch_secs <= now {
            return Err(SessionError::new(
                AttachErrorCode::PrivacyPolicyUnsatisfied,
                "privacy policy expired while the request was being authorized",
            ));
        }
        if dequeue {
            state
                .requests
                .get_mut(&key)
                .expect("checked above")
                .dequeued = true;
        } else {
            if let Some(key) = idempotency_key {
                state
                    .idempotency_keys
                    .insert(key, request.request_id.clone());
            }
            state.requests.insert(
                key,
                RequestUse {
                    fingerprint,
                    dequeued: false,
                },
            );
        }
        Ok(())
    }
}

fn request_proof_bytes(request: &BusinessRequest) -> &[u8] {
    &request.proof
}

fn ensure_quota(decision: QuotaDecision) -> Result<(), SessionError> {
    if decision.allowed {
        Ok(())
    } else {
        Err(SessionError::new(
            AttachErrorCode::RateLimited,
            decision
                .denial_reason
                .unwrap_or_else(|| "resource quota denied the operation".into()),
        ))
    }
}
