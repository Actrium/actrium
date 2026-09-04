//! Unified session state — the single source of truth for actor identity,
//! credentials, renewal tokens, and session phase.
//!
//! # Design
//!
//! Identity and all credentials live in one atomically-replaceable snapshot.
//! Consumers read from the snapshot via [`SessionState`] on every outbound send,
//! so a soft renew automatically propagates new credentials without restarting
//! signaling or closing peers.
//!
//! On hard rebind the phase transitions to `Rebinding`, the generation is
//! bumped, and old-generation contexts are invalidated. New outbound waits
//! until the phase returns to `Active`.

use std::sync::Arc;

use actr_protocol::{AIdCredential, ActrId, TurnCredential};
use prost::bytes::Bytes;
use prost_types::Timestamp;
use tokio::sync::{RwLock, watch};

use super::SupervisorFactSink;

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

/// What the session is currently allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    /// Normal operation — outbound allowed.
    Active,
    /// Hard rebind in progress — new outbound is gated, old-generation
    /// contexts are invalidated.
    Rebinding,
    /// Realm returned 403 — no hard register allowed; peers stay alive
    /// but the credential manager stops attempting renewal.
    RealmUnavailable,
}

/// Readiness projection used by outbound callers that need to wait for a
/// hard-rebind to finish.  The generation is part of the projection so an
/// old context can be invalidated even when the replacement session is still
/// rebinding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionReadiness {
    pub(crate) phase: SessionPhase,
    pub(crate) generation: u64,
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// One complete, consistent view of actor identity and credentials.
#[derive(Clone)]
pub struct SessionSnapshot {
    pub actor_id: ActrId,
    pub credential: AIdCredential,
    pub credential_expires_at: Timestamp,
    pub turn_credential: TurnCredential,
    pub renewal_token: Bytes,
    pub renewal_token_expires_at: Timestamp,
    /// Monotonically-increasing generation. Bumped on hard rebind so that
    /// old-generation [`crate::context::RuntimeContext`] instances can
    /// detect staleness and return `ConnectionNotReady`.
    pub generation: u64,
}

// ---------------------------------------------------------------------------
// SessionState
// ---------------------------------------------------------------------------

/// Thread-safe, clonable handle to the shared session.
#[derive(Clone)]
pub struct SessionState {
    inner: Arc<RwLock<SessionStateInner>>,
    readiness_tx: watch::Sender<SessionReadiness>,
}

struct SessionStateInner {
    phase: SessionPhase,
    snapshot: SessionSnapshot,
    /// Stale detector snapshot — written immediately after a hard rebind
    /// commit so old-generation contexts can compare.
    current_generation: u64,
    /// Supervisor fact sink for `SessionActivated` facts. Installed after
    /// construction (the reconciler's input channel outlives the session).
    fact_sink: Option<SupervisorFactSink>,
}

impl SessionState {
    /// Create a new `SessionState` in the `Active` phase with generation 1.
    pub fn new(snapshot: SessionSnapshot) -> Self {
        let generation = snapshot.generation;
        let (readiness_tx, _readiness_rx) = watch::channel(SessionReadiness {
            phase: SessionPhase::Active,
            generation,
        });
        Self {
            inner: Arc::new(RwLock::new(SessionStateInner {
                phase: SessionPhase::Active,
                current_generation: snapshot.generation,
                snapshot,
                fact_sink: None,
            })),
            readiness_tx,
        }
    }

    /// Install the supervisor fact sink so an authoritative generation commit
    /// (hard rebind) reports `SessionActivated` to the connection supervisor.
    pub(crate) async fn set_fact_sink(&self, sink: SupervisorFactSink) {
        self.inner.write().await.fact_sink = Some(sink);
    }

    // ---- readers -----------------------------------------------------------

    /// Return a clone of the current snapshot (cheap — protobuf types are
    /// `Clone`). Callers that only need a field should use the specific
    /// accessor methods instead.
    pub async fn snapshot(&self) -> SessionSnapshot {
        self.inner.read().await.snapshot.clone()
    }

    /// Current actor identity.
    pub async fn actor_id(&self) -> ActrId {
        self.inner.read().await.snapshot.actor_id.clone()
    }

    /// Synchronous best-effort actor ID read for APIs that cannot be async.
    pub fn actor_id_sync(&self) -> Option<ActrId> {
        self.inner
            .try_read()
            .ok()
            .map(|guard| guard.snapshot.actor_id.clone())
    }

    /// Current access credential.
    pub async fn credential(&self) -> AIdCredential {
        self.inner.read().await.snapshot.credential.clone()
    }

    /// Current access credential expiry.
    pub async fn credential_expires_at(&self) -> Timestamp {
        self.inner.read().await.snapshot.credential_expires_at
    }

    /// Current TURN credential.
    pub async fn turn_credential(&self) -> TurnCredential {
        self.inner.read().await.snapshot.turn_credential.clone()
    }

    /// Current renewal token.
    pub async fn renewal_token(&self) -> Bytes {
        self.inner.read().await.snapshot.renewal_token.clone()
    }

    /// Current renewal token expiry.
    pub async fn renewal_token_expires_at(&self) -> Timestamp {
        self.inner.read().await.snapshot.renewal_token_expires_at
    }

    /// Current session phase.
    pub async fn phase(&self) -> SessionPhase {
        self.inner.read().await.phase
    }

    /// Current generation number.
    pub async fn generation(&self) -> u64 {
        self.inner.read().await.current_generation
    }

    /// Synchronous best-effort generation read for context builders.
    pub fn generation_sync(&self) -> Option<u64> {
        self.inner
            .try_read()
            .ok()
            .map(|guard| guard.current_generation)
    }

    /// Check whether the given generation is still current.
    pub async fn is_current_generation(&self, generation: u64) -> bool {
        self.inner.read().await.current_generation == generation
    }

    /// Subscribe to phase/generation changes for bounded outbound waits.
    pub(crate) fn subscribe_readiness(&self) -> watch::Receiver<SessionReadiness> {
        self.readiness_tx.subscribe()
    }

    /// Transition the phase and publish readiness as one indivisible step.
    ///
    /// All phase mutations MUST go through this method. The watch update
    /// happens while the write guard is still held: releasing the guard
    /// before publishing would open a window where `inner.phase` has moved
    /// but a subscriber that acquired its receiver in that window still
    /// observes the old projection (e.g. admitting outbound work under
    /// `Active` while the session is already `Rebinding`). `send_replace`
    /// also stores the value for later subscribers, so a receiver obtained
    /// after this call returns always sees the new phase/generation.
    pub(crate) async fn set_phase(&self, phase: SessionPhase) {
        let mut guard = self.inner.write().await;
        guard.phase = phase;
        self.readiness_tx.send_replace(SessionReadiness {
            phase: guard.phase,
            generation: guard.current_generation,
        });
    }

    // ---- writers -----------------------------------------------------------

    /// Atomically replace credentials (soft renew path).
    ///
    /// Preserves `actor_id`, `generation`, and `phase`. Only credential,
    /// TURN credential, and renewal token are replaced.
    pub(crate) async fn update_credentials(
        &self,
        credential: AIdCredential,
        credential_expires_at: Timestamp,
        turn_credential: TurnCredential,
        renewal_token: Bytes,
        renewal_token_expires_at: Timestamp,
    ) {
        let mut guard = self.inner.write().await;
        guard.snapshot.credential = credential;
        guard.snapshot.credential_expires_at = credential_expires_at;
        guard.snapshot.turn_credential = turn_credential;
        guard.snapshot.renewal_token = renewal_token;
        guard.snapshot.renewal_token_expires_at = renewal_token_expires_at;
    }

    /// Transition to `Rebinding` — new outbound is gated.
    pub(crate) async fn enter_rebinding(&self) {
        self.set_phase(SessionPhase::Rebinding).await;
    }

    /// Atomically commit a hard rebind.
    ///
    /// Replaces the entire snapshot with a new one, bumps the generation,
    /// and returns the *old* snapshot so callers can detach old handles.
    #[cfg(test)]
    pub(crate) async fn commit_hard_rebind(
        &self,
        new_snapshot: SessionSnapshot,
    ) -> SessionSnapshot {
        let new_generation = new_snapshot.generation;
        let old = self.commit_hard_rebind_unpublished(new_snapshot).await;
        self.publish_session_activated(new_generation).await;
        old
    }

    /// Commit the session snapshot without publishing its activation fact.
    ///
    /// Credential recovery uses this form while it updates every dependent
    /// runtime handle. Publishing before those projections are installed can
    /// let the supervisor open a socket with mixed old/new identity material.
    /// The readiness projection is updated inside the write critical
    /// section: a subscriber that acquired its receiver while the guard was
    /// held must never observe the pre-rebind generation once the commit
    /// returns.
    pub(crate) async fn commit_hard_rebind_unpublished(
        &self,
        new_snapshot: SessionSnapshot,
    ) -> SessionSnapshot {
        {
            let mut guard = self.inner.write().await;
            let old = guard.snapshot.clone();
            guard.snapshot = new_snapshot;
            guard.current_generation = guard.snapshot.generation;
            self.readiness_tx.send_replace(SessionReadiness {
                phase: guard.phase,
                generation: guard.current_generation,
            });
            old
        }
    }

    /// Publish an already-committed generation after all runtime projections
    /// carry the same identity and credential snapshot.
    pub(crate) async fn publish_session_activated(&self, generation: u64) {
        let fact_sink = {
            let guard = self.inner.read().await;
            if guard.current_generation != generation {
                return;
            }
            guard.fact_sink.clone()
        };
        if let Some(sink) = fact_sink {
            sink.session_activated(generation);
        }
    }

    /// Transition back to `Active` after signaling reconnect succeeds.
    pub(crate) async fn set_active(&self) {
        self.set_phase(SessionPhase::Active).await;
    }

    /// Transition to `RealmUnavailable` — no further renewal attempts.
    pub(crate) async fn set_realm_unavailable(&self) {
        self.set_phase(SessionPhase::RealmUnavailable).await;
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Build a default empty snapshot (used in tests / placeholder contexts).
impl SessionSnapshot {
    pub fn empty_with_id(actor_id: ActrId, generation: u64) -> Self {
        Self {
            actor_id,
            credential: AIdCredential::default(),
            credential_expires_at: Timestamp::default(),
            turn_credential: TurnCredential::default(),
            renewal_token: Bytes::new(),
            renewal_token_expires_at: Timestamp::default(),
            generation,
        }
    }
}

#[cfg(test)]
#[path = "session_state_tests.rs"]
mod tests;
