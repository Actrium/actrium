//! Credential Manager — single-flight soft renew / hard rebind orchestration.
//!
//! # Trigger sources
//!
//! - Access credential expiry scheduler (5 min before expiry + 0–30s jitter).
//! - Heartbeat / signaling returns 401.
//! - (Legacy) signaling credential warning.
//!
//! # Behaviour
//!
//! 1. All triggers enter the same single-flight future.
//! 2. Call `POST /issuer/renew`.
//! 3. On success: atomically replace credentials (soft renew).
//! 4. On 401 or locally-expired renewal token: hard rebind. The flag-ON
//!    membership path re-acquires the SAME identity via `/reissue`; the flag-OFF
//!    legacy path performs a fresh `/register` (new serial), which also keeps
//!    recovery working against Issuer servers without a `/reissue` endpoint.
//! 5. On realm-denial 403: transition to `RealmUnavailable`, stop retrying. A
//!    possession-proof failure (a distinct wire code) is deferred, not terminal.
//! 6. Temporary errors: exponential backoff with jitter.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_protocol::prost::Message as _;
use actr_protocol::{
    IdentityClaims, ReissueCredentialRequest, RenewCredentialRequest, register_response,
    renew_credential_response,
};
use tokio::sync::Mutex;
use tracing::Instrument;

use crate::issuer_client::{
    IssuerClient, RealmClientKey, ReissueError, RenewError, new_control_attempt_id,
};
use crate::transport::PeerTransport;
use crate::wire::webrtc::{HookCallback, HookEvent, SignalingClient};

use super::membership::PublishedCredential;
use super::network_event::SignalingRecoveryRequestCause;
use super::node::CredentialState;
use super::session_state::{SessionPhase, SessionSnapshot, SessionState};

/// Wire code Issuer returns in a `/reissue` `RegisterResponse::Error` when the
/// caller failed to prove possession of the requested `ActrId` (a missing,
/// unknown, or grace-expired binding proof).
///
/// It is deliberately distinct from the realm-denial `403` so this recoverable
/// per-actor identity-loss condition is not mistaken for a terminal realm
/// denial. Must stay in lockstep with
/// `loran_issuer::issuer::REISSUE_POSSESSION_NOT_PROVEN_CODE`.
const REISSUE_POSSESSION_NOT_PROVEN_CODE: u32 = 422;

// ---- Re-acquire outcome (membership controller path) -----------------------

/// Terminal outcome of a single-flight re-acquire driven by the membership
/// controller. Unlike the legacy fire-and-forget `trigger_renewal`, the
/// controller path is awaitable and returns exactly what happened so the
/// coordinator can publish, back off, or enter the terminal denied phase.
// Transient outcome: constructed once per re-acquire and matched immediately,
// never stored in bulk, so the large `Renewed` variant is not worth boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum ReacquireOutcome {
    /// A fresh credential was minted (soft renew or credential-only hard
    /// rebind). Carries the generation-stamped credential to publish.
    Renewed(PublishedCredential),
    /// The realm denied membership (403 / `RealmUnavailable`). Terminal for
    /// automatic recovery.
    Denied,
    /// A transient failure — retry later on the backoff ladder.
    Deferred(String),
}

// ---- Registration Context --------------------------------------------------

/// Saved registration parameters so hard rebind can authenticate `/reissue`
/// with the same package or linked-workload context.
#[derive(Clone)]
pub(crate) enum RegistrationContext {
    /// Package-backed registration — carries the full original request
    /// including manifest bytes, manifest signature and signing key.
    Package {
        #[allow(dead_code)]
        request: actr_protocol::RegisterRequest,
        signer: std::sync::Arc<dyn crate::issuer_client::ActorKeySigner>,
    },
    /// Source-linked registration — carries the request.
    Linked {
        #[allow(dead_code)]
        request: actr_protocol::RegisterRequest,
        signer: std::sync::Arc<dyn crate::issuer_client::ActorKeySigner>,
    },
}

#[cfg(test)]
impl RegistrationContext {
    pub(crate) fn package_for_test(request: actr_protocol::RegisterRequest) -> Self {
        Self::Package {
            request,
            signer: std::sync::Arc::new(ed25519_dalek::SigningKey::from_bytes(&[41; 32])),
        }
    }

    pub(crate) fn linked_for_test(request: actr_protocol::RegisterRequest) -> Self {
        Self::Linked {
            request,
            signer: std::sync::Arc::new(ed25519_dalek::SigningKey::from_bytes(&[41; 32])),
        }
    }
}

// ---- Credential Manager ----------------------------------------------------

/// Shared credential manager — clonable, all clones share the same state.
#[derive(Clone)]
pub(crate) struct CredentialManager {
    session: SessionState,
    registration_ctx: RegistrationContext,
    issuer_endpoint: String,
    /// Signs the Issuer requests' client proof.
    client_key: Option<RealmClientKey>,

    /// Single-flight guard: only one renewal attempt at a time.
    renewing: Arc<AtomicBool>,
    /// Consecutive non-transient renewal failures. After
    /// [`NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD`] the failure is escalated
    /// to `ERROR` so a supervisor/alert can react instead of looping silently
    /// (issue #397 hardening). Reset on any success or transient failure.
    non_transient_failures: Arc<AtomicU32>,
    /// Pending renewal join handle for cancellation during shutdown.
    inflight: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Runtime handles that must be updated after hard rebind commits.
    hard_rebind_handles: Arc<Mutex<Option<HardRebindHandles>>>,
    /// Lifecycle callback notified after soft renewal commits.
    hook_callback: Arc<Mutex<Option<HookCallback>>>,
    /// Exact idempotent request bodies retained across outer recovery rounds.
    pending_control_attempts: Arc<Mutex<PendingControlAttempts>>,
}

#[derive(Default)]
struct PendingControlAttempts {
    renew: Option<RenewCredentialRequest>,
    reissue: Option<ReissueCredentialRequest>,
}

#[derive(Clone)]
pub(crate) struct HardRebindHandles {
    pub signaling_client: Arc<dyn SignalingClient>,
    pub credential_state: CredentialState,
    pub peer_transport: Option<Arc<PeerTransport>>,
    /// RTC manager. Hard rebind must drain RTC peers; otherwise
    /// old-identity sessions survive the identity change.
    pub rtc_manager: Option<Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>>,
    /// RTC signaling identity cell. Rebind adopts a fresh `ActrId`; the
    /// RTC transport must stamp it on outbound envelopes.
    pub rtc_local_id: Option<crate::wire::webrtc::rtc::signaling::SharedLocalId>,
}

impl CredentialManager {
    pub(crate) fn new(
        session: SessionState,
        registration_ctx: RegistrationContext,
        issuer_endpoint: impl Into<String>,
        client_key: Option<RealmClientKey>,
    ) -> Self {
        Self {
            session,
            registration_ctx,
            issuer_endpoint: issuer_endpoint.into(),
            client_key,
            renewing: Arc::new(AtomicBool::new(false)),
            non_transient_failures: Arc::new(AtomicU32::new(0)),
            inflight: Arc::new(Mutex::new(None)),
            hard_rebind_handles: Arc::new(Mutex::new(None)),
            hook_callback: Arc::new(Mutex::new(None)),
            pending_control_attempts: Arc::new(Mutex::new(PendingControlAttempts::default())),
        }
    }

    /// Return a clone of the managed SessionState.
    pub(crate) fn session_state(&self) -> SessionState {
        self.session.clone()
    }

    pub(crate) async fn install_hard_rebind_handles(&self, handles: HardRebindHandles) {
        *self.hard_rebind_handles.lock().await = Some(handles);
    }

    pub(crate) async fn install_hook_callback(&self, callback: Option<HookCallback>) {
        *self.hook_callback.lock().await = callback;
    }

    // ---- Membership-controller path (credential-only) ----------------------

    /// Current session phase.
    pub(crate) async fn phase(&self) -> SessionPhase {
        self.session.phase().await
    }

    /// Transition the session to `RealmUnavailable` (terminal denied).
    pub(crate) async fn mark_realm_unavailable(&self) {
        self.session.set_realm_unavailable().await;
    }

    /// Snapshot the current credential as a generation-stamped
    /// [`PublishedCredential`] for the membership `watch` channel.
    pub(crate) async fn published_credential(&self) -> PublishedCredential {
        let snapshot = self.session.snapshot().await;
        PublishedCredential {
            credential: snapshot.credential,
            credential_expires_at: Some(snapshot.credential_expires_at),
            turn_credential: Some(snapshot.turn_credential),
            actor_id: snapshot.actor_id,
            // Seed the publication clock. Subsequent results are stamped by
            // MembershipController rather than copied from session generation.
            revision: snapshot.generation,
        }
    }

    /// Single-flight, credential-only re-acquire driven by the membership
    /// controller.
    ///
    /// Mints a fresh credential (soft renew, then hard `/reissue` fallback) and
    /// updates identity/handles, but NEVER drives the socket. The membership
    /// controller publishes the returned credential revision as a material
    /// supervisor fact; the supervisor owns the recovery action and backoff.
    ///
    /// Runtime hard rebind is credential-only against the stable on-disk node
    /// AID. Issuer authenticates the saved registration context but reissues for
    /// the explicitly supplied existing identity.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(name = "credential.reacquire", skip_all)
    )]
    pub(crate) async fn reacquire(&self) -> ReacquireOutcome {
        let handles = self.hard_rebind_handles.lock().await.clone();
        let hook_callback = self.hook_callback.lock().await.clone();
        run_reacquire_once(
            self.session.clone(),
            self.issuer_endpoint.clone(),
            self.client_key.clone(),
            self.registration_ctx.clone(),
            handles,
            hook_callback,
            self.pending_control_attempts.clone(),
        )
        .await
    }

    /// Entry point for all renewal triggers. Returns immediately if a
    /// renewal is already in flight (single-flight).
    pub(crate) fn trigger_renewal(&self) {
        // Fast-path: if already renewing, skip.
        if self.renewing.swap(true, Ordering::AcqRel) {
            tracing::debug!("CredentialManager: renewal already in flight, skipping trigger");
            return;
        }

        let session = self.session.clone();
        let issuer_endpoint = self.issuer_endpoint.clone();
        let client_key = self.client_key.clone();
        let registration_ctx = self.registration_ctx.clone();
        let renewing = self.renewing.clone();
        let hard_rebind_handles = self.hard_rebind_handles.clone();
        let hook_callback = self.hook_callback.clone();
        let non_transient_failures = self.non_transient_failures.clone();
        let pending_control_attempts = self.pending_control_attempts.clone();

        // Spawn the actual work so the caller isn't blocked.
        let renewal_span = tracing::info_span!("credential.renew");
        let handle = tokio::spawn(async move {
            let handles = hard_rebind_handles.lock().await.clone();
            let hook_callback = hook_callback.lock().await.clone();
            let result = run_renewal_once(
                session,
                issuer_endpoint,
                client_key,
                registration_ctx,
                handles,
                hook_callback,
                pending_control_attempts,
            )
            .await;
            match result {
                Ok(()) => non_transient_failures.store(0, Ordering::Release),
                Err(err) => match record_failure(&non_transient_failures, &err) {
                    EscalationLevel::Warn => {
                        tracing::warn!(error = %err, "CredentialManager: renewal attempt ended");
                    }
                    EscalationLevel::Escalate => {
                        tracing::error!(
                            error = %err,
                            error_category = "credential_renewal_non_transient",
                            severity = 9,
                            "CredentialManager: renewal has failed non-transiently \
                             repeatedly; node may be unable to soft-renew — \
                             supervisor/alert intervention recommended"
                        );
                    }
                },
            }
            renewing.store(false, Ordering::Release);
        }
        .instrument(renewal_span));

        // Store the handle for potential cancellation during shutdown.
        let inflight = self.inflight.clone();
        tokio::spawn(async move {
            let mut guard = inflight.lock().await;
            *guard = Some(handle);
        });
    }

    /// Cancel any in-flight renewal (called during shutdown).
    #[allow(dead_code)]
    pub(crate) async fn cancel(&self) {
        let mut guard = self.inflight.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
        self.renewing.store(false, Ordering::Release);
    }
}

async fn pending_renew_request(
    pending: &Arc<Mutex<PendingControlAttempts>>,
    snapshot: &SessionSnapshot,
) -> RenewCredentialRequest {
    let mut guard = pending.lock().await;
    if let Some(request) = guard.renew.as_ref()
        && request.actr_id == snapshot.actor_id
        && request.renewal_token == snapshot.renewal_token
    {
        return request.clone();
    }

    let request = RenewCredentialRequest {
        actr_id: snapshot.actor_id.clone(),
        renewal_token: snapshot.renewal_token.clone(),
        attempt_id: new_control_attempt_id(),
    };
    guard.renew = Some(request.clone());
    request
}

async fn clear_pending_renew(
    pending: &Arc<Mutex<PendingControlAttempts>>,
    completed: &RenewCredentialRequest,
) {
    let mut guard = pending.lock().await;
    if guard
        .renew
        .as_ref()
        .is_some_and(|request| request.attempt_id == completed.attempt_id)
    {
        guard.renew = None;
    }
}

async fn pending_reissue_request(
    pending: &Arc<Mutex<PendingControlAttempts>>,
    snapshot: &SessionSnapshot,
) -> Option<ReissueCredentialRequest> {
    let mut guard = pending.lock().await;
    let proof = reissue_possession_proof(snapshot);
    if let Some(request) = guard.reissue.as_ref()
        && request.actr_id == snapshot.actor_id
        && request.renewal_proof == proof
    {
        return Some(request.clone());
    }
    guard.reissue = None;
    None
}

async fn store_pending_reissue(
    pending: &Arc<Mutex<PendingControlAttempts>>,
    request: &ReissueCredentialRequest,
) {
    pending.lock().await.reissue = Some(request.clone());
}

async fn clear_pending_reissue(
    pending: &Arc<Mutex<PendingControlAttempts>>,
    completed: &ReissueCredentialRequest,
) {
    let mut guard = pending.lock().await;
    if guard
        .reissue
        .as_ref()
        .is_some_and(|request| request.attempt_id == completed.attempt_id)
    {
        guard.reissue = None;
    }
}

/// An Issuer client signing with `client_key`, if any.
fn issuer_client(endpoint: &str, client_key: Option<&RealmClientKey>) -> IssuerClient {
    let issuer = IssuerClient::new(endpoint);
    match client_key {
        Some(key) => issuer.with_client_key(key.clone()),
        None => issuer,
    }
}

async fn run_renewal_once(
    session: SessionState,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    registration_ctx: RegistrationContext,
    hard_rebind_handles: Option<HardRebindHandles>,
    hook_callback: Option<HookCallback>,
    pending_control_attempts: Arc<Mutex<PendingControlAttempts>>,
) -> Result<(), String> {
    let snapshot = session.snapshot().await;

    if snapshot.renewal_token.is_empty() {
        return run_hard_rebind(
            session,
            issuer_endpoint,
            client_key,
            registration_ctx,
            hard_rebind_handles,
            hook_callback,
        )
        .await;
    }

    if is_expired(snapshot.renewal_token_expires_at.seconds) {
        return run_hard_rebind(
            session,
            issuer_endpoint,
            client_key,
            registration_ctx,
            hard_rebind_handles,
            hook_callback,
        )
        .await;
    }

    let issuer = issuer_client(&issuer_endpoint, client_key.as_ref());

    let request = pending_renew_request(&pending_control_attempts, &snapshot).await;

    let response = match issuer.renew_credential(request.clone()).await {
        Ok(response) => {
            clear_pending_renew(&pending_control_attempts, &request).await;
            response
        }
        Err(RenewError::RealmUnavailable) => {
            clear_pending_renew(&pending_control_attempts, &request).await;
            session.set_realm_unavailable().await;
            return Err("realm unavailable during renewal".to_string());
        }
        Err(RenewError::TokenRejected) => {
            clear_pending_renew(&pending_control_attempts, &request).await;
            return run_hard_rebind(
                session,
                issuer_endpoint,
                client_key,
                registration_ctx,
                hard_rebind_handles,
                hook_callback,
            )
            .await;
        }
        Err(RenewError::RateLimited { retry_after }) => {
            if let Some(delay) = retry_after {
                crate::timer::sleep(crate::timer::ids::CREDENTIAL_RATE_LIMIT, delay).await;
            }
            return Err("renewal rate limited".to_string());
        }
        Err(RenewError::Retryable(err)) => {
            let mut backoff = Backoff::new();
            crate::timer::sleep(crate::timer::ids::CREDENTIAL_RETRY_BACKOFF, backoff.next()).await;
            return Err(format!("retryable renew error: {err}"));
        }
        Err(RenewError::AttemptInProgress) => {
            return Err("renewal attempt remains retryable".to_string());
        }
        Err(
            err @ (RenewError::InvalidRequest(_)
            | RenewError::AttemptExpired
            | RenewError::AttemptConflict
            | RenewError::Protocol(_)),
        ) => {
            clear_pending_renew(&pending_control_attempts, &request).await;
            return Err(err.to_string());
        }
    };

    let ok = match response.result {
        Some(renew_credential_response::Result::Success(ok)) => ok,
        Some(renew_credential_response::Result::Error(err)) => {
            return Err(format!(
                "renew response contained error {}: {}",
                err.code, err.message
            ));
        }
        None => return Err("renew response missing result".to_string()),
    };

    if ok.actr_id != snapshot.actor_id {
        return Err("renew response changed ActrId".to_string());
    }

    let claims = IdentityClaims::decode(ok.credential.claims.as_ref())
        .map_err(|e| format!("renew credential claims decode failed: {e}"))?;
    if claims.actor_id != snapshot.actor_id.to_string_repr() {
        return Err("renew credential claims actor_id mismatch".to_string());
    }

    let credential_expires_at = ok
        .credential_expires_at
        .ok_or_else(|| "renew response missing credential expiry".to_string())?;
    let renewal_token = ok
        .renewal_token
        .ok_or_else(|| "renew response missing renewal token".to_string())?;
    let renewal_token_expires_at = ok
        .renewal_token_expires_at
        .ok_or_else(|| "renew response missing renewal token expiry".to_string())?;

    session
        .update_credentials(
            ok.credential.clone(),
            credential_expires_at,
            ok.turn_credential.clone(),
            renewal_token.clone(),
            renewal_token_expires_at,
        )
        .await;

    if let Some(handles) = hard_rebind_handles {
        handles
            .credential_state
            .update(
                ok.credential,
                Some(credential_expires_at),
                Some(ok.turn_credential),
            )
            .await;
    }

    fire_credential_renewed(hook_callback.as_ref(), &credential_expires_at).await;

    tracing::info!(
        actor_id = %snapshot.actor_id.to_string_repr(),
        credential_expires_at = credential_expires_at.seconds,
        renewal_token_expires_at = renewal_token_expires_at.seconds,
        "CredentialManager: soft renewal completed"
    );

    Ok(())
}

async fn run_hard_rebind(
    session: SessionState,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    registration_ctx: RegistrationContext,
    hard_rebind_handles: Option<HardRebindHandles>,
    hook_callback: Option<HookCallback>,
) -> Result<(), String> {
    let old_snapshot = session.snapshot().await;
    tracing::warn!(
        actor_id = %old_snapshot.actor_id.to_string_repr(),
        generation = old_snapshot.generation,
        "CredentialManager: starting hard rebind"
    );

    // Feature flag OFF (the default): the membership controller is NOT the
    // credential owner, so this legacy path performs a FRESH registration that
    // allocates a new serial — exactly as before the self-healing controller
    // existed. Stable-identity `/reissue` is reserved for the flag-ON path
    // (`run_credential_only_hard_rebind`): it needs an Issuer that implements the
    // endpoint and an actor binding still inside the reissue grace window.
    // Falling back to `/register` here keeps recovery working against older Issuer
    // servers (which answer `/reissue` with 404) and after the binding grace has
    // lapsed (403 possession failure) — the two regressions a `/reissue`-only
    // legacy path would introduce.
    let issuer = issuer_client(&issuer_endpoint, client_key.as_ref());
    let registration = registration_with_actor_proof(
        registration_ctx,
        actr_protocol::actor_key_proof::ActorKeyProofOperation::Register,
    )
    .map_err(|err| format!("hard rebind register signing failed before commit: {err}"))?;

    let response = issuer
        .register_with_manifest(registration)
        .await
        .map_err(|err| format!("hard rebind register failed before commit: {err}"))?;

    let ok = match response.result {
        Some(register_response::Result::Success(ok)) => ok,
        Some(register_response::Result::Error(err)) => {
            return Err(format!(
                "hard rebind register rejected before commit {}: {}",
                err.code, err.message
            ));
        }
        None => return Err("hard rebind register response missing result".to_string()),
    };

    let credential_expires_at = ok
        .credential_expires_at
        .ok_or_else(|| "hard rebind response missing credential expiry".to_string())?;
    // `renewal_token` / `renewal_token_expires_at` are `optional` in the proto
    // (actr.proto:217-219): an Issuer that predates the renewal-token feature
    // legitimately omits them. The access credential + expiry above are what
    // keep the node reachable, so commit them regardless. A missing token only
    // means "soft renewal unavailable" — the node stays in the empty-token state
    // it was already in and will hard-rebind again next cycle. Failing the
    // entire rebind over an optional field would discard a successful
    // re-registration and let the old credential expire into a silent outage
    // (issue #397). `credential_expires_at` stays mandatory: without an expiry
    // the credential cannot be tracked, so that case is genuinely unsalvageable.
    let renewal_token_missing = ok.renewal_token.is_none();
    let renewal_token = ok.renewal_token.unwrap_or_default();
    let renewal_token_expires_at = ok
        .renewal_token_expires_at
        .unwrap_or(prost_types::Timestamp {
            seconds: 0,
            nanos: 0,
        });
    if renewal_token_missing {
        tracing::warn!(
            actor_id = %ok.actr_id.to_string_repr(),
            "hard rebind: Issuer omitted renewal_token; committing access credential \
             but soft renewal unavailable — node will hard-rebind each cycle"
        );
    }

    let new_snapshot = SessionSnapshot {
        // Fresh `/register` allocates a new serial — adopt the identity Issuer just
        // issued rather than pinning the old one.
        actor_id: ok.actr_id.clone(),
        credential: ok.credential.clone(),
        credential_expires_at,
        turn_credential: ok.turn_credential.clone(),
        renewal_token,
        renewal_token_expires_at,
        generation: old_snapshot.generation.saturating_add(1),
    };

    session.enter_rebinding().await;
    let _old = session
        .commit_hard_rebind_unpublished(new_snapshot.clone())
        .await;

    if let Some(handles) = hard_rebind_handles {
        // Stop ingress on the old authenticated socket before draining peer
        // state. Otherwise a delayed old-identity Offer or RoleAssignment can
        // recreate a peer between close-all and disconnect.
        if let Err(err) = handles.signaling_client.disconnect().await {
            tracing::warn!(error = %err, "hard rebind signaling disconnect failed");
        }

        // RTC backend: drain all peer controllers (bounded by the manager's
        // overall close_all deadline), mirroring the 0.14 coordinator drain.
        if let Some(manager) = handles.rtc_manager.as_ref() {
            manager.close_all().await;
        }
        if let Some(peer_transport) = handles.peer_transport.as_ref()
            && let Err(err) = peer_transport.close_all().await
        {
            tracing::warn!(error = %err, "hard rebind failed to close old peer transports");
        }
        // RTC finalize: an in-flight outbound connect may have created a
        // controller after the first drain; close_all on an empty registry
        // returns immediately.
        if let Some(manager) = handles.rtc_manager.as_ref() {
            manager.close_all().await;
        }

        handles
            .credential_state
            .update(
                new_snapshot.credential.clone(),
                Some(new_snapshot.credential_expires_at),
                Some(new_snapshot.turn_credential.clone()),
            )
            .await;

        handles
            .signaling_client
            .set_actor_id(new_snapshot.actor_id.clone())
            .await;
        handles
            .signaling_client
            .set_credential_state(handles.credential_state.clone())
            .await;

        // RTC backend: the shared signaling identity cell follows the
        // committed ActrId (fresh after `/register`, unchanged after
        // `/reissue` — uniform with the coordinator/gate re-id above).
        if let Some(local_id) = handles.rtc_local_id.as_ref() {
            local_id.set(new_snapshot.actor_id.clone());
        }

        session
            .publish_session_activated(new_snapshot.generation)
            .await;

        // The credential is committed and the runtime handles carry the new
        // material — deliver the documented CredentialRenewed notification
        // now. Supervisor-owned socket recovery must not gate it.
        fire_credential_renewed(hook_callback.as_ref(), &new_snapshot.credential_expires_at).await;

        // The credential/session transaction is committed. Recovery is a
        // separate supervisor-owned obligation: this path must not perform an
        // inline connect or create a second retry policy. The recovery effect
        // transitions the session back to Active after signaling succeeds.
        handles
            .signaling_client
            .request_signaling_recovery(SignalingRecoveryRequestCause::CredentialRebound);
    } else {
        session
            .publish_session_activated(new_snapshot.generation)
            .await;
        session.set_active().await;
        fire_credential_renewed(hook_callback.as_ref(), &new_snapshot.credential_expires_at).await;
    }

    tracing::info!(
        actor_id = %new_snapshot.actor_id.to_string_repr(),
        generation = new_snapshot.generation,
        credential_expires_at = new_snapshot.credential_expires_at.seconds,
        renewal_token_expires_at = new_snapshot.renewal_token_expires_at.seconds,
        "CredentialManager: hard rebind committed"
    );

    Ok(())
}

// ---- Membership-controller re-acquire (credential-only) --------------------

/// Rebind the saved registration payload with a fresh operation-specific Actor
/// proof. Fresh `/register` and stable-identity `/reissue` proofs are not
/// interchangeable; the Realm client proof is minted separately per request.
fn registration_with_actor_proof(
    registration_ctx: RegistrationContext,
    operation: actr_protocol::actor_key_proof::ActorKeyProofOperation,
) -> crate::error::HyperResult<actr_protocol::RegisterRequest> {
    match registration_ctx {
        RegistrationContext::Package { request, signer }
        | RegistrationContext::Linked { request, signer } => {
            crate::issuer_client::sign_actor_request(request, signer.as_ref(), operation)
        }
    }
}

/// Credential-only single-flight re-acquire for the membership controller.
///
/// Tries a soft renew first (if the renewal token is usable), then falls back to
/// a stable-identity credential reissue. NEVER drives the socket. Returns the
/// generation-stamped credential to publish, or a terminal / deferred outcome.
async fn run_reacquire_once(
    session: SessionState,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    registration_ctx: RegistrationContext,
    hard_rebind_handles: Option<HardRebindHandles>,
    hook_callback: Option<HookCallback>,
    pending_control_attempts: Arc<Mutex<PendingControlAttempts>>,
) -> ReacquireOutcome {
    let snapshot = session.snapshot().await;

    let token_usable = !snapshot.renewal_token.is_empty()
        && !is_expired(snapshot.renewal_token_expires_at.seconds);

    if token_usable {
        match run_soft_renew(
            &session,
            &issuer_endpoint,
            client_key.as_ref(),
            hard_rebind_handles.as_ref(),
            hook_callback.as_ref(),
            &snapshot,
            &pending_control_attempts,
        )
        .await
        {
            SoftRenewOutcome::Renewed(published) => return ReacquireOutcome::Renewed(published),
            SoftRenewOutcome::Denied => return ReacquireOutcome::Denied,
            SoftRenewOutcome::Deferred(reason) => return ReacquireOutcome::Deferred(reason),
            // Token was rejected by Issuer — fall through to hard rebind.
            SoftRenewOutcome::FallThroughToHardRebind => {}
        }
    }

    run_credential_only_hard_rebind(
        session,
        issuer_endpoint,
        client_key,
        registration_ctx,
        hard_rebind_handles,
        hook_callback,
        pending_control_attempts,
    )
    .await
}

// Transient outcome (see `ReacquireOutcome`): local, immediately matched.
#[allow(clippy::large_enum_variant)]
enum SoftRenewOutcome {
    Renewed(PublishedCredential),
    Denied,
    Deferred(String),
    FallThroughToHardRebind,
}

/// One soft-renew round-trip (`POST /renew`). Credential-only: updates session +
/// credential_state, fires the renewed hook, and returns the published
/// credential. Never touches the socket.
async fn run_soft_renew(
    session: &SessionState,
    issuer_endpoint: &str,
    client_key: Option<&RealmClientKey>,
    hard_rebind_handles: Option<&HardRebindHandles>,
    hook_callback: Option<&HookCallback>,
    snapshot: &SessionSnapshot,
    pending_control_attempts: &Arc<Mutex<PendingControlAttempts>>,
) -> SoftRenewOutcome {
    let issuer = issuer_client(issuer_endpoint, client_key);

    let request = pending_renew_request(pending_control_attempts, snapshot).await;

    let response = match issuer.renew_credential(request.clone()).await {
        Ok(response) => {
            clear_pending_renew(pending_control_attempts, &request).await;
            response
        }
        Err(RenewError::RealmUnavailable) => {
            clear_pending_renew(pending_control_attempts, &request).await;
            session.set_realm_unavailable().await;
            return SoftRenewOutcome::Denied;
        }
        Err(RenewError::TokenRejected) => {
            clear_pending_renew(pending_control_attempts, &request).await;
            return SoftRenewOutcome::FallThroughToHardRebind;
        }
        Err(RenewError::RateLimited { .. }) => {
            return SoftRenewOutcome::Deferred("renewal rate limited".to_string());
        }
        Err(RenewError::Retryable(err)) => {
            return SoftRenewOutcome::Deferred(format!("retryable renew error: {err}"));
        }
        Err(RenewError::AttemptInProgress) => {
            return SoftRenewOutcome::Deferred("renewal attempt remains in progress".to_string());
        }
        Err(
            err @ (RenewError::InvalidRequest(_)
            | RenewError::AttemptExpired
            | RenewError::AttemptConflict
            | RenewError::Protocol(_)),
        ) => {
            clear_pending_renew(pending_control_attempts, &request).await;
            return SoftRenewOutcome::Deferred(err.to_string());
        }
    };

    let ok = match response.result {
        Some(renew_credential_response::Result::Success(ok)) => ok,
        Some(renew_credential_response::Result::Error(err)) => {
            return SoftRenewOutcome::Deferred(format!(
                "renew response contained error {}: {}",
                err.code, err.message
            ));
        }
        None => return SoftRenewOutcome::Deferred("renew response missing result".to_string()),
    };

    if ok.actr_id != snapshot.actor_id {
        return SoftRenewOutcome::Deferred("renew response changed ActrId".to_string());
    }

    let claims = match IdentityClaims::decode(ok.credential.claims.as_ref()) {
        Ok(claims) => claims,
        Err(e) => {
            return SoftRenewOutcome::Deferred(format!(
                "renew credential claims decode failed: {e}"
            ));
        }
    };
    if claims.actor_id != snapshot.actor_id.to_string_repr() {
        return SoftRenewOutcome::Deferred("renew credential claims actor_id mismatch".to_string());
    }

    let credential_expires_at = match ok.credential_expires_at {
        Some(v) => v,
        None => {
            return SoftRenewOutcome::Deferred("renew response missing credential expiry".into());
        }
    };
    let renewal_token = match ok.renewal_token.clone() {
        Some(v) => v,
        None => return SoftRenewOutcome::Deferred("renew response missing renewal token".into()),
    };
    let renewal_token_expires_at = match ok.renewal_token_expires_at {
        Some(v) => v,
        None => {
            return SoftRenewOutcome::Deferred(
                "renew response missing renewal token expiry".into(),
            );
        }
    };

    session
        .update_credentials(
            ok.credential.clone(),
            credential_expires_at,
            ok.turn_credential.clone(),
            renewal_token,
            renewal_token_expires_at,
        )
        .await;

    if let Some(handles) = hard_rebind_handles {
        handles
            .credential_state
            .update(
                ok.credential.clone(),
                Some(credential_expires_at),
                Some(ok.turn_credential.clone()),
            )
            .await;
    }

    fire_credential_renewed(hook_callback, &credential_expires_at).await;

    let generation = session.generation().await;
    tracing::info!(
        actor_id = %snapshot.actor_id.to_string_repr(),
        generation,
        credential_expires_at = credential_expires_at.seconds,
        "membership: soft renewal completed (credential-only)"
    );

    SoftRenewOutcome::Renewed(PublishedCredential {
        credential: ok.credential,
        credential_expires_at: Some(credential_expires_at),
        turn_credential: Some(ok.turn_credential),
        actor_id: snapshot.actor_id.clone(),
        // MembershipController assigns current_revision + 1 at publish time.
        revision: 0,
    })
}

/// Credential-only hard rebind: `POST /reissue` against the STABLE node AID.
///
/// Re-authenticates the registration context, mints a fresh credential for the
/// explicitly supplied existing AID, commits the new
/// snapshot, re-ids the runtime handles (gate / coordinator / transport) and
/// updates `credential_state`. It deliberately DOES NOT drive the socket — no
/// `disconnect()`, no direct signaling connect, and no client-local retry
/// scheduling. The recovery supervisor is the sole reconnect driver; the
/// coordinator publishes the returned credential revision as material input.
async fn run_credential_only_hard_rebind(
    session: SessionState,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    registration_ctx: RegistrationContext,
    hard_rebind_handles: Option<HardRebindHandles>,
    hook_callback: Option<HookCallback>,
    pending_control_attempts: Arc<Mutex<PendingControlAttempts>>,
) -> ReacquireOutcome {
    let old_snapshot = session.snapshot().await;
    tracing::warn!(
        actor_id = %old_snapshot.actor_id.to_string_repr(),
        generation = old_snapshot.generation,
        "membership: starting credential-only hard rebind"
    );

    let issuer = issuer_client(&issuer_endpoint, client_key.as_ref());
    let request = if let Some(request) =
        pending_reissue_request(&pending_control_attempts, &old_snapshot).await
    {
        request
    } else {
        let registration = match registration_with_actor_proof(
            registration_ctx.clone(),
            actr_protocol::actor_key_proof::ActorKeyProofOperation::Reissue,
        ) {
            Ok(registration) => registration,
            Err(error) => {
                return ReacquireOutcome::Deferred(format!(
                    "cannot sign Actor reissue proof: {error}"
                ));
            }
        };
        let request = ReissueCredentialRequest {
            actr_id: old_snapshot.actor_id.clone(),
            registration,
            renewal_proof: reissue_possession_proof(&old_snapshot),
            attempt_id: new_control_attempt_id(),
        };
        store_pending_reissue(&pending_control_attempts, &request).await;
        request
    };

    let response = match issuer.reissue_credential_classified(request.clone()).await {
        Ok(response) => {
            clear_pending_reissue(&pending_control_attempts, &request).await;
            response
        }
        Err(
            ReissueError::AttemptExpired
            | ReissueError::AttemptConflict
            | ReissueError::Rejected(_)
            | ReissueError::Protocol(_),
        ) => {
            clear_pending_reissue(&pending_control_attempts, &request).await;
            return ReacquireOutcome::Deferred(
                "hard rebind reissue attempt was definitively rejected".to_string(),
            );
        }
        Err(err @ (ReissueError::AttemptInProgress | ReissueError::Retryable(_))) => {
            return ReacquireOutcome::Deferred(format!("hard rebind reissue failed: {err}"));
        }
    };

    let ok = match response.result {
        Some(register_response::Result::Success(ok)) => ok,
        Some(register_response::Result::Error(err)) => {
            // A genuine realm denial (403) is terminal: no amount of credential
            // re-acquire makes the realm accept this node, so stop automatic
            // recovery and enter the loud, slow-cadence denied phase.
            if err.code == 403 {
                tracing::error!(
                    error_category = "membership_denied",
                    severity = 10,
                    code = err.code,
                    actor_id = %old_snapshot.actor_id.to_string_repr(),
                    "membership: /reissue realm-denied; stopping automatic recovery"
                );
                session.set_realm_unavailable().await;
                return ReacquireOutcome::Denied;
            }
            // A possession-proof failure is NOT a realm denial: the actor
            // binding is missing, unknown, or aged past the reissue grace. This
            // is a per-actor identity-loss condition — an operator (or a fresh
            // registration) must resolve it — so recovery must NOT be pinned to
            // terminal RealmUnavailable here. Defer so a later external trigger
            // can retry instead of permanently stopping recovery.
            if err.code == REISSUE_POSSESSION_NOT_PROVEN_CODE {
                tracing::warn!(
                    error_category = "membership_identity_loss",
                    severity = 8,
                    code = err.code,
                    actor_id = %old_snapshot.actor_id.to_string_repr(),
                    "membership: /reissue possession proof not accepted (missing/unknown/grace-expired); \
                     deferring — recovery stays open, but a fresh identity or operator action may be required"
                );
                return ReacquireOutcome::Deferred(format!(
                    "reissue possession not proven ({}): {}",
                    err.code, err.message
                ));
            }
            return ReacquireOutcome::Deferred(format!(
                "hard rebind reissue rejected {}: {}",
                err.code, err.message
            ));
        }
        None => {
            return ReacquireOutcome::Deferred(
                "hard rebind reissue response missing result".to_string(),
            );
        }
    };

    // `/reissue` is contractually stable-identity. Keep the response check as a
    // protocol guard before committing any credential material.
    if ok.actr_id != old_snapshot.actor_id {
        return ReacquireOutcome::Deferred(format!(
            "credential reissue returned a different ActrId ({} != {})",
            ok.actr_id.to_string_repr(),
            old_snapshot.actor_id.to_string_repr()
        ));
    }

    let credential_expires_at = match ok.credential_expires_at {
        Some(v) => v,
        None => {
            return ReacquireOutcome::Deferred(
                "hard rebind response missing credential expiry".into(),
            );
        }
    };
    // `renewal_token` / `renewal_token_expires_at` are `optional` in the proto
    // (actr.proto:217-219). A missing token must not discard a credential that
    // Issuer just reissued for this identity — commit it and return `Renewed` so
    // the controller publishes the credential and keeps the node reachable.
    // The node stays in the empty-token state and will hard-rebind again next
    // cycle (issue #397 symmetric to the legacy `/register` path).
    let renewal_token = match ok.renewal_token.clone() {
        Some(v) => v,
        None => {
            tracing::warn!(
                actor_id = %old_snapshot.actor_id.to_string_repr(),
                "credential-only hard rebind: Issuer omitted renewal_token; \
                 committing access credential but soft renewal unavailable — \
                 node will hard-rebind each cycle"
            );
            bytes::Bytes::new()
        }
    };
    let renewal_token_expires_at = match ok.renewal_token_expires_at {
        Some(v) => v,
        None => prost_types::Timestamp {
            seconds: 0,
            nanos: 0,
        },
    };

    let new_snapshot = SessionSnapshot {
        actor_id: old_snapshot.actor_id.clone(),
        credential: ok.credential.clone(),
        credential_expires_at,
        turn_credential: ok.turn_credential.clone(),
        renewal_token,
        renewal_token_expires_at,
        generation: old_snapshot.generation.saturating_add(1),
    };

    session.enter_rebinding().await;
    let _old = session
        .commit_hard_rebind_unpublished(new_snapshot.clone())
        .await;

    let has_runtime_handles = hard_rebind_handles.is_some();
    if let Some(handles) = hard_rebind_handles {
        handles
            .credential_state
            .update(
                new_snapshot.credential.clone(),
                Some(new_snapshot.credential_expires_at),
                Some(new_snapshot.turn_credential.clone()),
            )
            .await;

        // Re-id the runtime handles to the (unchanged) AID. This is a no-op for
        // identity but keeps the code path uniform and future-proof; crucially
        // it does NOT touch the socket.
        handles
            .signaling_client
            .set_actor_id(new_snapshot.actor_id.clone())
            .await;
        // RTC backend: the shared signaling identity cell follows the
        // committed ActrId (fresh after `/register`, unchanged after
        // `/reissue` — uniform with the coordinator/gate re-id above).
        if let Some(local_id) = handles.rtc_local_id.as_ref() {
            local_id.set(new_snapshot.actor_id.clone());
        }
    }

    session
        .publish_session_activated(new_snapshot.generation)
        .await;
    if !has_runtime_handles {
        session.set_active().await;
    }

    // Hard reissue commits and publishes new credential material exactly like
    // a soft renew does — workloads get the same documented CredentialRenewed
    // notification here.
    fire_credential_renewed(hook_callback.as_ref(), &new_snapshot.credential_expires_at).await;

    tracing::info!(
        actor_id = %new_snapshot.actor_id.to_string_repr(),
        generation = new_snapshot.generation,
        credential_expires_at = new_snapshot.credential_expires_at.seconds,
        "membership: credential-only hard rebind committed"
    );

    ReacquireOutcome::Renewed(PublishedCredential {
        credential: new_snapshot.credential,
        credential_expires_at: Some(new_snapshot.credential_expires_at),
        turn_credential: Some(new_snapshot.turn_credential),
        actor_id: new_snapshot.actor_id,
        // MembershipController assigns current_revision + 1 at publish time.
        revision: 0,
    })
}

/// Possession proof for `/issuer/reissue`: the last renewal token issued to this
/// node, even when it is already expired or was rejected by `/issuer/renew`.
/// Issuer matches its hash against the issuer-recorded binding for the actor so
/// only the holder of that token can reissue credentials for this serial.
fn reissue_possession_proof(snapshot: &SessionSnapshot) -> Option<bytes::Bytes> {
    if snapshot.renewal_token.is_empty() {
        tracing::warn!(
            actor_id = %snapshot.actor_id.to_string_repr(),
            "hard rebind has no renewal token to prove actor possession; Issuer will reject the reissue"
        );
        return None;
    }
    Some(snapshot.renewal_token.clone())
}

fn is_expired(expires_at: i64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Issuer TTLs are configurable and the token carries no authoritative
    // issuance-time bound, so a large remaining lifetime is not evidence of
    // a local clock rollback.
    expires_at <= now
}

/// Convert a remotely supplied credential expiry without panicking when the
/// timestamp exceeds the protobuf or platform `SystemTime` range.
pub(crate) fn credential_expiry_to_system_time(
    expires_at: &prost_types::Timestamp,
) -> Option<SystemTime> {
    // `prost_types::Timestamp` follows google.protobuf.Timestamp, whose upper
    // bound is 9999-12-31T23:59:59Z. Reject wire values outside that contract
    // even on platforms whose SystemTime happens to represent a wider range;
    // otherwise behavior would differ by target and the hook could still panic
    // when a downstream platform converts the value.
    const PROTOBUF_TIMESTAMP_MAX_SECONDS: i64 = 253_402_300_799;
    if expires_at.seconds > PROTOBUF_TIMESTAMP_MAX_SECONDS {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(expires_at.seconds.max(0) as u64))
}

async fn fire_credential_renewed(
    hook_callback: Option<&HookCallback>,
    expires_at: &prost_types::Timestamp,
) {
    if let Some(callback) = hook_callback {
        let Some(new_expiry) = credential_expiry_to_system_time(expires_at) else {
            tracing::warn!(
                expiry_seconds = expires_at.seconds,
                "Skipping credential-renewed hook because expiry is outside the protobuf or platform range"
            );
            return;
        };
        callback(HookEvent::CredentialRenewed { new_expiry }).await;
    }
}

// ---- Non-transient failure classification & escalation --------------------

/// Consecutive non-transient failures after which a renewal error is escalated
/// to `ERROR` instead of `WARN`. Picked to ride out one or two transient
/// blips on neighbouring heartbeat ticks while still surfacing a stuck
/// condition well inside a credential TTL.
const NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureClass {
    /// Likely to resolve on retry (5xx, rate-limit, retryable transport).
    Transient,
    /// Cannot resolve by retrying (missing optional field, version skew,
    /// permanent 4xx contract violation). Retrying forever is pointless and,
    /// before Fix 1, silently burned down into an outage (issue #397).
    NonTransient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscalationLevel {
    Warn,
    Escalate,
}

/// Classify a `run_renewal_once` error string into transient / non-transient.
///
/// The string is the same one `trigger_renewal` already logged, so matching on
/// substrings keeps the classifier in lockstep with the error sites without a
/// wider refactor that threads typed errors out of `run_renewal_once`.
fn classify_renewal_error(err: &str) -> FailureClass {
    if err.contains("missing renewal token")
        || err.contains("missing renewal token expiry")
        || err.contains("missing credential expiry")
        || err.contains("missing result")
    {
        FailureClass::NonTransient
    } else {
        FailureClass::Transient
    }
}

/// Update the consecutive-failure counter and decide whether to escalate.
///
/// `NonTransient` increments the counter; once it reaches the threshold the
/// counter resets to 0 (so the loud `ERROR` repeats once per cycle, not every
/// tick) and `Escalate` is returned. `Transient` resets the counter to 0 and
/// returns `Warn` — a single transient blip must not be able to masquerade as
/// a stuck condition. Pure over the counter so it is unit-testable without a
/// runtime.
fn record_failure(counter: &AtomicU32, err: &str) -> EscalationLevel {
    match classify_renewal_error(err) {
        FailureClass::NonTransient => {
            let prev = counter.fetch_add(1, Ordering::AcqRel);
            if prev + 1 >= NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD {
                counter.store(0, Ordering::Release);
                EscalationLevel::Escalate
            } else {
                EscalationLevel::Warn
            }
        }
        FailureClass::Transient => {
            counter.store(0, Ordering::Release);
            EscalationLevel::Warn
        }
    }
}

// ---- Exponential backoff with jitter ---------------------------------------

struct Backoff {
    attempt: u32,
}

impl Backoff {
    fn new() -> Self {
        Self { attempt: 0 }
    }

    /// Returns the next delay: 5, 10, 20, 40, 60, 60, ... seconds with
    /// ±25% jitter, capped at 60s.
    #[allow(dead_code)]
    fn next(&mut self) -> Duration {
        let base = match self.attempt {
            0 => 5,
            1 => 10,
            2 => 20,
            3 => 40,
            _ => 60,
        };
        self.attempt += 1;

        // Deterministic jitter: use attempt number as seed.
        let jitter =
            (base as f64 * 0.25 * ((self.attempt.wrapping_mul(7)) as f64 % 2.0 - 1.0)) as i64;
        let ms = ((base * 1000) as i64 + jitter * 1000i64).max(1000);
        Duration::from_millis(ms as u64)
    }
}

#[cfg(test)]
#[path = "credential_manager_tests.rs"]
mod tests;
