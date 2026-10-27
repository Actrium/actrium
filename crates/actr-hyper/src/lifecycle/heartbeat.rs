//! Heartbeat management for ActrNode
//!
//! This module contains functions for sending periodic heartbeat messages
//! to the signaling server and handling responses.

use crate::issuer_client::{IssuerClient, RealmClientKey};
use crate::lifecycle::credential_manager::{CredentialManager, credential_expiry_to_system_time};
use crate::lifecycle::membership::MembershipHandle;
use crate::lifecycle::session_state::SessionPhase;
use crate::lifecycle::{CredentialState, SignalingRecoveryRequestCause};
use crate::transport::{AuthVerdict, NetworkError};
use crate::wire::webrtc::{HookCallback, HookEvent, SignalingClient};
use actr_protocol::{ActrId, RegisterRequest, ServiceAvailabilityState};
use actr_runtime_mailbox::Mailbox;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Invoke a [`HookCallback`], if present, awaiting its completion.
async fn fire_hook(cb: Option<&HookCallback>, event: HookEvent) {
    if let Some(cb) = cb {
        cb(event).await;
    }
}

/// Route a credential-refresh trigger to the right owner.
///
/// When the membership controller is wired, the heartbeat is a pure REPORTER:
/// it sends a `Rejected` verdict for the current credential generation and lets
/// the controller (the sole owner) drive the single-flight, generation-fenced
/// re-acquire. When the controller is disabled, it falls back to the legacy
/// fire-and-forget `CredentialManager::trigger_renewal`.
async fn request_credential_refresh(
    membership: Option<&MembershipHandle>,
    credential_manager: Option<&CredentialManager>,
) {
    if let Some(handle) = membership {
        let stale_revision = handle.current_revision();
        handle.report(AuthVerdict::Rejected, stale_revision).await;
        return;
    }
    if let Some(manager) = credential_manager {
        manager.trigger_renewal();
    } else {
        tracing::warn!(
            "Credential refresh requested but neither membership controller nor CredentialManager is installed; keeping existing identity"
        );
    }
}

/// Typical mailbox capacity for backlog ratio calculation
/// A typical_capacity of 1000 means 100 messages = 10% backlog
const TYPICAL_CAPACITY: f32 = 1000.0;

/// Log the first heartbeat failure immediately, then sample long failure runs.
/// With the typical 5s-30s heartbeat interval this keeps offline periods visible
/// without flooding client logs.
const HEARTBEAT_FAILURE_LOG_EVERY: u64 = 12;
const POWER_RESERVE_FETCH_TIMEOUT: Duration = Duration::from_millis(250);

fn should_log_heartbeat_failure(consecutive_failures: u64) -> bool {
    consecutive_failures == 1 || consecutive_failures.is_multiple_of(HEARTBEAT_FAILURE_LOG_EVERY)
}

/// Get power reserve, mailbox backlog and calculate service availability
///
/// This function fetches the power reserve from pwrzv and mailbox backlog,
/// then calculates the service availability state based on both metrics.
///
/// # Arguments
/// * `mailbox` - Mailbox instance to get backlog statistics
///
/// # Returns
/// A tuple of (power_reserve, mailbox_backlog, availability) where:
/// - `power_reserve`: Power reserve level from pwrzv (1.0 to 5.0, where higher = more available)
/// - `mailbox_backlog`: Mailbox backlog ratio (0.0 to 1.0, where higher = more backlog)
/// - `availability`: Calculated ServiceAvailabilityState
async fn get_power_reserve_and_availability(
    mailbox: &Arc<dyn Mailbox>,
) -> (f32, f32, ServiceAvailabilityState) {
    // TODO: Ensure the default value is correct
    // Get real power reserve from pwrzv (returns 1.0 to 5.0, where higher = more available)
    let power_reserve = crate::timer::timeout(
        crate::timer::ids::HEARTBEAT_POWER_RESERVE,
        POWER_RESERVE_FETCH_TIMEOUT,
        pwrzv::get_power_reserve_level_direct(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(1.0); // Default to minimum capacity on error

    // Get mailbox backlog from mailbox stats
    // Calculate backlog ratio: (queued + inflight) / typical_capacity
    let mailbox_backlog = match mailbox.status().await {
        Ok(stats) => {
            let total_messages = (stats.queued_messages + stats.inflight_messages) as f32;
            (total_messages / TYPICAL_CAPACITY).min(1.0)
        }
        Err(e) => {
            tracing::warn!("⚠️ Failed to get mailbox stats: {}", e);
            0.0
        }
    };

    // TODO: Improve availability calculation
    // Determine availability based on power reserve and mailbox backlog
    // Power reserve range: 1.0 (worst) to 5.0 (best)
    // Thresholds adjusted for 1.0-5.0 range: 4.2 (80%), 3.0 (50%), 1.8 (20%)
    let availability = if power_reserve > 4.2 && mailbox_backlog < 0.5 {
        ServiceAvailabilityState::Full
    } else if power_reserve > 3.0 && mailbox_backlog < 0.8 {
        ServiceAvailabilityState::Degraded
    } else if power_reserve > 1.8 && mailbox_backlog < 0.95 {
        ServiceAvailabilityState::Overloaded
    } else {
        ServiceAvailabilityState::Unavailable
    };

    (power_reserve, mailbox_backlog, availability)
}

/// Send a single heartbeat and handle the Pong response
///
/// This function sends a heartbeat message to the signaling server,
/// waits for the Pong response, and handles credential warnings if present.
/// If credential has expired (401 error), it triggers the Credential Manager.
///
/// # Arguments
/// * `client` - Signaling client for sending heartbeats
/// * `actor_id` - Actor ID for heartbeat messages
/// * `credential_state` - Shared credential state
/// * `mailbox` - Mailbox instance for backlog statistics
/// * `heartbeat_interval` - Interval between heartbeats (used for timeout calculation)
/// * `credential_manager` - Single-flight renewal manager
///
/// Returns `Some(new_actor_id)` only for legacy hard re-registration paths.
#[allow(clippy::too_many_arguments)]
async fn send_heartbeat_and_handle_response(
    client: &Arc<dyn SignalingClient>,
    actor_id: &ActrId,
    credential_state: &CredentialState,
    mailbox: &Arc<dyn Mailbox>,
    heartbeat_interval: Duration,
    _register_request: &RegisterRequest,
    consecutive_failures: &mut u64,
    _issuer_endpoint: &str,
    _client_key: Option<&RealmClientKey>,
    credential_manager: Option<&CredentialManager>,
    hook_callback: Option<&HookCallback>,
    membership: Option<&MembershipHandle>,
) -> Option<ActrId> {
    // Terminal realm denial: stop pinging entirely. A connected socket that was
    // realm-denied has nothing to gain from more heartbeats, and re-sending would
    // only collect the same 403 — the membership no-reprobe contract. The
    // disconnected branch in `heartbeat_task` already suppresses reconnect wakes
    // in this phase; this guards the still-connected case symmetrically.
    if membership.is_some()
        && let Some(manager) = credential_manager
        && manager.phase().await == SessionPhase::RealmUnavailable
    {
        tracing::debug!(
            "💔 Heartbeat: realm denied membership (terminal); skipping heartbeat send"
        );
        return None;
    }

    // Get current credential from shared state
    let current_credential = credential_state.credential().await;

    // Get power reserve, mailbox backlog and calculate availability
    let (power_reserve, mailbox_backlog, availability) =
        get_power_reserve_and_availability(mailbox).await;

    let ping_timeout = heartbeat_interval.mul_f64(0.4).max(Duration::from_secs(1));
    let pong_response = crate::timer::timeout(
        crate::timer::ids::HEARTBEAT_PONG,
        ping_timeout,
        client.send_heartbeat(
            actor_id.clone(),
            current_credential.clone(),
            availability,
            power_reserve,
            mailbox_backlog,
        ),
    )
    .await;

    let pong = match pong_response {
        Ok(Ok(pong)) => pong,
        Ok(Err(NetworkError::CredentialExpired(msg))) => {
            // Credential has expired, trigger soft renewal. Do not re-register
            // or close peers from the heartbeat path.
            tracing::warn!(
                "⚠️ Credential expired during heartbeat: {}. Triggering credential renewal.",
                msg
            );

            // Fire `on_credential_expiring` with the last-known expiry
            // timestamp (best-effort — the credential might already be
            // past its advertised `expires_at`, but firing the event
            // gives the workload one final chance to observe the
            // transition before we trigger renewal).
            if let Some(expires_at) = credential_state.expires_at().await {
                if let Some(new_expiry) = credential_expiry_to_system_time(&expires_at) {
                    fire_hook(hook_callback, HookEvent::CredentialExpiring { new_expiry }).await;
                } else {
                    tracing::warn!(
                        expiry_seconds = expires_at.seconds,
                        "Skipping credential-expiring hook because expiry is outside the platform range"
                    );
                }
            }

            request_credential_refresh(membership, credential_manager).await;
            return None;
        }
        Ok(Err(e)) => {
            // A connected-heartbeat response can still carry a typed auth verdict
            // (notably a realm 403). Route it to the membership owner exactly like
            // a handshake verdict, so recovery terminates or re-acquires instead
            // of the generic-failure path silently retrying a dead credential
            // forever.
            if let Some(verdict) = e.auth_verdict() {
                match verdict {
                    AuthVerdict::RealmDenied => {
                        // Already terminal — honor the no-reprobe contract: do
                        // not re-report or wake anything on later ticks.
                        if let Some(manager) = credential_manager
                            && manager.phase().await == SessionPhase::RealmUnavailable
                        {
                            tracing::debug!(
                                "💔 Heartbeat: realm already denied (terminal); suppressing repeat report"
                            );
                            return None;
                        }
                        if let Some(handle) = membership {
                            let stale_revision = handle.current_revision();
                            tracing::error!(
                                error_category = "membership_denied",
                                severity = 10,
                                "💔 Heartbeat: realm denied membership (403); reporting terminal verdict, automatic recovery will stop"
                            );
                            handle
                                .report(AuthVerdict::RealmDenied, stale_revision)
                                .await;
                            return None;
                        }
                        // No membership controller (legacy path): fall through to
                        // the generic failure handling below.
                    }
                    AuthVerdict::Rejected => {
                        tracing::warn!(
                            "⚠️ Heartbeat: credential-rejected verdict; requesting on-demand re-acquire"
                        );
                        request_credential_refresh(membership, credential_manager).await;
                        return None;
                    }
                }
            }
            *consecutive_failures += 1;
            if should_log_heartbeat_failure(*consecutive_failures) {
                tracing::warn!(
                    consecutive_failures = *consecutive_failures,
                    "⚠️ Failed to send heartbeat or receive Pong: {}",
                    e
                );
            } else {
                tracing::debug!(
                    consecutive_failures = *consecutive_failures,
                    "Suppressed repeated heartbeat failure: {}",
                    e
                );
            }
            return None;
        }
        Err(_) => {
            *consecutive_failures += 1;
            if should_log_heartbeat_failure(*consecutive_failures) {
                tracing::warn!(
                    consecutive_failures = *consecutive_failures,
                    "⚠️ Heartbeat timeout after {:?}",
                    ping_timeout
                );
            } else {
                tracing::debug!(
                    consecutive_failures = *consecutive_failures,
                    "Suppressed repeated heartbeat timeout after {:?}",
                    ping_timeout
                );
            }
            return None;
        }
    };

    if *consecutive_failures > 0 {
        tracing::info!(
            consecutive_failures = *consecutive_failures,
            "✅ Heartbeat recovered after consecutive failures"
        );
        *consecutive_failures = 0;
    }

    tracing::trace!(
        "💓 Heartbeat sent and Pong received for Actor {} (power_reserve={:.2}, mailbox_backlog={:.2}, availability={:?})",
        actor_id,
        power_reserve,
        mailbox_backlog,
        availability
    );
    // TODO: Handle suggest_interval_secs
    // Handle credential_warning
    if let Some(warning) = pong.credential_warning {
        tracing::warn!(
            "⚠️ Credential warning received: type={:?}, message={}",
            warning.r#type(),
            warning.message
        );

        // Fire `on_credential_expiring` hook once per warning so the
        // workload can preload its credential-renewed handlers (e.g. to
        // rotate derived secrets) before the refresh round-trip lands.
        // The signaling warning is an idempotent trigger source for the
        // Credential Manager; actual renewal goes through POST /issuer/renew.
        if let Some(expires_at) = credential_state.expires_at().await {
            if let Some(new_expiry) = credential_expiry_to_system_time(&expires_at) {
                fire_hook(hook_callback, HookEvent::CredentialExpiring { new_expiry }).await;
            } else {
                tracing::warn!(
                    expiry_seconds = expires_at.seconds,
                    "Skipping credential-expiring hook because expiry is outside the platform range"
                );
            }
        }
        request_credential_refresh(membership, credential_manager).await;
    }
    None
}

/// Heartbeat task that periodically sends Ping messages to signaling server
///
/// This task runs in a loop, sending heartbeat messages at the specified interval
/// and handling Pong responses, including credential warnings.
/// If credential has expired (401 error), it triggers re-registration.
///
/// # Arguments
/// * `shutdown` - Cancellation token for graceful shutdown
/// * `client` - Signaling client for sending heartbeats
/// * `actor_id` - Actor ID for heartbeat messages
/// * `credential_state` - Shared credential state
/// * `mailbox` - Mailbox instance for backlog statistics
/// * `heartbeat_interval` - Interval between heartbeats
/// * `register_request` - RegisterRequest for re-registration on credential expiry
/// * `issuer_endpoint` - Issuer HTTP endpoint for re-registration
#[allow(clippy::too_many_arguments)]
pub async fn heartbeat_task(
    shutdown: CancellationToken,
    client: Arc<dyn SignalingClient>,
    actor_id: ActrId,
    credential_state: CredentialState,
    mailbox: Arc<dyn Mailbox>,
    heartbeat_interval: Duration,
    register_request: RegisterRequest,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    credential_manager: Option<CredentialManager>,
    hook_callback: Option<HookCallback>,
    membership: Option<MembershipHandle>,
) {
    let mut interval =
        crate::timer::interval(crate::timer::ids::HEARTBEAT_SCHEDULE, heartbeat_interval);
    let mut actor_id = actor_id;
    let mut consecutive_failures = 0;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("💓 Heartbeat task received shutdown signal");
                break;
            }
            _ = interval.tick() => {
                if let Some(manager) = credential_manager.as_ref() {
                    actor_id = manager.session_state().actor_id().await;
                }

                if !client.is_connected() {
                    // Terminal realm denial has no automatic release trigger.
                    // Do not even emit duplicate demand while the credential
                    // authority is terminally unavailable.
                    if membership.is_some()
                        && let Some(manager) = credential_manager.as_ref()
                        && manager.phase().await == SessionPhase::RealmUnavailable
                    {
                        tracing::debug!(
                            "💓 Heartbeat: realm denied membership (terminal); suppressing recovery demand"
                        );
                        continue;
                    }

                    // Heartbeat is a fact producer, never a socket driver. The
                    // supervisor coalesces repeated ticks without resetting an
                    // existing attempt or backoff deadline.
                    client.request_signaling_recovery(
                        SignalingRecoveryRequestCause::HeartbeatObservedDisconnected,
                    );
                    continue;
                }

                if let Some(new_actor_id) = send_heartbeat_and_handle_response(
                    &client,
                    &actor_id,
                    &credential_state,
                    &mailbox,
                    heartbeat_interval,
                    &register_request,
                    &mut consecutive_failures,
                    &issuer_endpoint,
                    client_key.as_ref(),
                    credential_manager.as_ref(),
                    hook_callback.as_ref(),
                    membership.as_ref(),
                )
                .await {
                    tracing::info!(
                        "🔄 Heartbeat actor_id updated: {} -> {}",
                        actor_id,
                        new_actor_id,
                    );
                    actor_id = new_actor_id;
                }
            }
        }
    }
    tracing::info!("✅ Heartbeat task terminated gracefully");
}

/// Re-register actor after credential expiry
///
/// When the credential has completely expired, re-register via Issuer HTTP,
/// then disconnect/reconnect the signaling WebSocket with the new credential.
///
/// # Arguments
/// * `client` - Signaling client for reconnection
/// * `actor_id` - Current actor ID (used for logging)
/// * `register_request` - RegisterRequest containing actor type, realm, and service spec
/// * `credential_state` - Shared credential state to update
/// * `issuer_endpoint` - Issuer HTTP endpoint for registration
#[allow(dead_code)]
async fn re_register_task(
    client: Arc<dyn SignalingClient>,
    actor_id: ActrId,
    register_request: RegisterRequest,
    credential_state: CredentialState,
    issuer_endpoint: String,
    client_key: Option<RealmClientKey>,
    hook_callback: Option<HookCallback>,
) -> ActrId {
    tracing::info!(
        "🔄 Re-registering actor {} after credential expiry via Issuer HTTP (type: {}/{})",
        actor_id,
        register_request.actr_type.publisher,
        register_request.actr_type.name
    );

    // Step 1: Register via Issuer HTTP to get new credential
    let mut issuer = IssuerClient::new(&issuer_endpoint);
    if let Some(key) = client_key {
        issuer = issuer.with_client_key(key);
    }
    let resp = match issuer
        .register_with_manifest(register_request.clone())
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("❌ Issuer re-registration HTTP request failed: {}", e);
            return actor_id;
        }
    };

    match resp.result {
        Some(actr_protocol::register_response::Result::Success(register_ok)) => {
            let new_actor_id = register_ok.actr_id.clone();
            let new_credential = register_ok.credential;
            let new_expires_at = register_ok.credential_expires_at;
            let new_turn_credential = Some(register_ok.turn_credential);

            // Update shared credential state
            credential_state
                .update(new_credential.clone(), new_expires_at, new_turn_credential)
                .await;

            // Step 2: Clear old identity from signaling client
            client.clear_identity().await;

            // Step 3: Disconnect old WebSocket session
            tracing::info!("🔌 Disconnecting signaling client to refresh session");
            if let Err(e) = client.disconnect().await {
                tracing::warn!("⚠️ Disconnect failed (non-fatal, continuing): {}", e);
            }

            // Step 4: Update signaling client identity with new credential
            client.set_actor_id(new_actor_id.clone()).await;
            client.set_credential_state(credential_state.clone()).await;

            // Step 5: publish recovery demand. The supervisor owns the one-shot
            // connect action and every retry deadline.
            tracing::info!("🔗 Requesting signaling recovery with new credential");
            client.request_signaling_recovery(SignalingRecoveryRequestCause::CredentialRebound);

            tracing::debug!("TurnCredential updated, TURN authentication ready");

            if let Some(expires_at) = &new_expires_at {
                tracing::debug!("⏰ New credential expires at: {}s", expires_at.seconds);
                // Fire `on_credential_renewed` for the re-registration
                // path as well — same semantics as the refresh path.
                if let Some(new_expiry) = credential_expiry_to_system_time(expires_at) {
                    fire_hook(
                        hook_callback.as_ref(),
                        HookEvent::CredentialRenewed { new_expiry },
                    )
                    .await;
                } else {
                    tracing::warn!(
                        expiry_seconds = expires_at.seconds,
                        "Skipping credential-renewed hook because expiry is outside the platform range"
                    );
                }
            }

            new_actor_id
        }
        Some(actr_protocol::register_response::Result::Error(err)) => {
            tracing::error!(
                "❌ Issuer re-registration failed: code={}, message={}",
                err.code,
                err.message
            );
            actor_id
        }
        None => {
            tracing::error!("❌ Issuer re-registration response missing result");
            actor_id
        }
    }
}

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod tests;
