//! Secure-channel state, cell, and data-lane gate.
//!
//! See RFC-0563 (`rfcs/0563-secure-actor-channels.md`). Phase 2 introduces the
//! honest state machine, the physical-session-owned [`SecureCell`], and the
//! [`SecureDataLane`] decorator that gates actual `send`/`recv`. No real
//! handshake is performed yet (phase 3+, blocked on RFC #573's `AIdCredential`
//! prerequisite); the only sendable-but-unverified state is the honestly-labeled
//! [`SecureChannelState::LegacyBypass`], documented for phase-3 removal.
//!
//! The secure state is owned by the *physical* `WireHandle` impl
//! (`WebRtcConnection` / `WebSocketConnection` / `ClientWebSocketHandle`), not a
//! transient decorator above `WireHandle`. This means re-wrapping the same
//! underlying connection cannot lose or duplicate authenticated state, and a
//! single `SecureDataLane` wrap at lane-creation gates both the inbound
//! reader's `recv()` and the outbound `send()` through one epoch.
//!
//! TODO(phase3): WebRTC media RTP (tracks managed via `add_media_track` /
//! `remove_media_track`) currently sends outside this gate — there is no direct
//! RTP send API path today, but once one exists it must be gated on the same
//! session `SecureCell`.

use super::error::{NetworkError, NetworkResult};
#[cfg(test)]
use super::lane::DeliveryProgress;
use super::lane::{DataLane, SendAttemptResult};
use actr_protocol::{ActrId, ActrType};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Trusted output of a completed mutual-authentication exchange.
///
/// This is runtime evidence, not a credential or a bearer token. Hyper stores
/// and forwards these values without interpreting Actrium member roles. The
/// handshake implementation must construct the complete value before asking
/// [`SecureCell::publish_verified`] to expose application I/O. In particular,
/// a new channel generation must publish a new value rather than inheriting
/// the preceding generation's identity or policy/state binding.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerifiedPeerContext {
    pub local_actor_id: ActrId,
    pub peer_actor_id: ActrId,
    pub peer_actor_type: ActrType,
    pub peer_member: String,
    pub peer_client_key: String,
    /// Opaque role claim from the verified credential. Hyper has no role enum.
    pub peer_role: String,
    pub peer_actor_public_key: Vec<u8>,
    pub channel_id: String,
    pub channel_generation: u64,
    pub transcript_digest: Vec<u8>,
    pub local_runtime_instance_id: String,
    pub peer_runtime_instance_id: String,
    pub peer_credential_expires_at_epoch_secs: u64,
    /// Current policy and state bindings captured by the same verification.
    pub current_policy_binding: Vec<u8>,
    pub current_state_binding: Vec<u8>,
}

/// Secure-channel state for a physical `WireHandle` session. See RFC-0563 §1.
///
/// Phase 2 only ever constructs [`SecureChannelState::LegacyBypass`]; the
/// remaining variants exist so phase 3+ can drive the real state machine
/// (`New → Handshaking → Verified`, rekey, terminal) without touching the seam
/// again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureChannelState {
    /// Pre-handshake. Not sendable.
    New,
    /// Handshake in flight. Not sendable.
    Handshaking,
    /// Mutual auth + key confirmation complete. Sendable.
    Verified,
    /// Key rotation in progress. Not sendable.
    Rekeying,
    /// Transport closed. Terminal, not sendable.
    Closed,
    /// Auth/transport failed. Terminal, not sendable.
    Failed,
    /// Transport-only compat bypass: transport ready, NO crypto verification.
    /// Sendable in phase 2 only; phase 3 removes it from the sendable set once
    /// `AIdCredential` (RFC #573) lands.
    LegacyBypass,
}

impl SecureChannelState {
    const fn as_u8(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Handshaking => 1,
            Self::Verified => 2,
            Self::Rekeying => 3,
            Self::Closed => 4,
            Self::Failed => 5,
            Self::LegacyBypass => 6,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::New,
            1 => Self::Handshaking,
            2 => Self::Verified,
            3 => Self::Rekeying,
            4 => Self::Closed,
            5 => Self::Failed,
            _ => Self::LegacyBypass,
        }
    }

    /// The authoritative sendability predicate (RFC-0563, aligned with #504):
    /// `Sendable = transport ready + mutual authentication + key confirmation`.
    ///
    /// Phase 2 admits `LegacyBypass` (transport ready, no auth) so runtime
    /// behavior is preserved honestly; phase 3 narrows this to `Verified` only.
    pub const fn is_sendable(self) -> bool {
        matches!(self, Self::Verified | Self::LegacyBypass)
    }

    /// Whether this state is terminal (no further transitions possible).
    /// The `WirePool` state-listener task uses this to exit once the wire it
    /// tracks is retired, so the task does not park forever holding an `Arc`
    /// to a closed/failed wire (RFC-0563 review point 2 lifecycle coupling).
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Failed)
    }

    /// Whether a transition `from → to` is legal. See RFC-0563 §1 state machine.
    ///
    /// Terminal states (`Closed`, `Failed`) admit no outgoing transitions.
    /// `LegacyBypass` may upgrade to `New` (phase 3 handshake start) or go
    /// terminal; it may not jump to `Verified` (that would assert verification
    /// that never happened).
    const fn is_legal_transition(from: Self, to: Self) -> bool {
        use SecureChannelState as S;
        matches!(
            (from, to),
            (S::New, S::Handshaking)
                | (S::New, S::Closed)
                | (S::New, S::Failed)
                | (S::Handshaking, S::Verified)
                | (S::Handshaking, S::Closed)
                | (S::Handshaking, S::Failed)
                | (S::Verified, S::Rekeying)
                | (S::Verified, S::Closed)
                | (S::Verified, S::Failed)
                | (S::Rekeying, S::Verified)
                | (S::Rekeying, S::Closed)
                | (S::Rekeying, S::Failed)
                | (S::LegacyBypass, S::New)
                | (S::LegacyBypass, S::Closed)
                | (S::LegacyBypass, S::Failed)
        )
    }
}

/// Lock-free secure-channel state cell owned by a physical `WireHandle`.
///
/// Mirrors the generation+`Notify` pattern of `DataChannelStateSignal`
/// (`lane.rs`): the `secure_epoch` counter closes the `Notify` registration
/// window so a transition landing immediately before a listener arms its
/// notification is still observable through [`Self::wait_after`]. Every
/// validated transition bumps the epoch (Release) and wakes waiters, so:
/// - `WirePool` can observe `Handshaking → Verified` (and rekey/terminal)
///   transitions exactly once and re-broadcast the ready set (point 2);
/// - already-acquired `SecureDataLane`s see the rotated epoch and refuse
///   further `send`/`recv` (point 3).
///
/// All loads use `Acquire`, all epoch bumps use `AcqRel` (point 6): future code
/// installs transcript/session keys before publishing `Verified`, and the
/// Release/Acquire pair guarantees that publication is visible only after the
/// keys are.
#[derive(Clone, Debug)]
pub(crate) struct SecureCell {
    state: Arc<AtomicU8>,
    secure_epoch: Arc<AtomicU64>,
    changed: Arc<tokio::sync::Notify>,
    /// Actrium strict cells require this context and `Verified` for every I/O.
    /// Legacy cells retain their pre-Actrium compatibility behavior while their
    /// handshake lacks the full binding above.
    verified_only: bool,
    verified_peer: Arc<RwLock<Option<Arc<VerifiedPeerContext>>>>,
}

impl SecureCell {
    pub(crate) fn new(initial: SecureChannelState) -> Self {
        Self::with_policy(initial, false)
    }

    /// Start an Actrium strict cell. No application I/O is possible until
    /// [`Self::publish_verified`] installs a complete peer context.
    pub(crate) fn new_actrium(initial: SecureChannelState) -> Self {
        assert_ne!(
            initial,
            SecureChannelState::Verified,
            "use new_actrium_verified to publish an authenticated context"
        );
        Self::with_policy(initial, true)
    }

    /// Construct an already-verified strict session. The context is installed
    /// before the externally visible cell (and its `Verified` state) exists.
    #[cfg(test)]
    pub(crate) fn new_actrium_verified(context: VerifiedPeerContext) -> Self {
        let cell = Self::with_policy(SecureChannelState::Handshaking, true);
        assert!(cell.publish_verified(SecureChannelState::Handshaking, context));
        cell
    }

    fn with_policy(initial: SecureChannelState, verified_only: bool) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(initial.as_u8())),
            secure_epoch: Arc::new(AtomicU64::new(0)),
            changed: Arc::new(tokio::sync::Notify::new()),
            verified_only,
            verified_peer: Arc::new(RwLock::new(None)),
        }
    }

    /// Current secure-channel state (Acquire — pairs with transition Release).
    pub(crate) fn state(&self) -> SecureChannelState {
        SecureChannelState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Current secure epoch. Bumps on every validated transition.
    pub(crate) fn epoch(&self) -> u64 {
        self.secure_epoch.load(Ordering::Acquire)
    }

    /// Immutable verified identity and binding for this channel generation.
    pub(crate) fn verified_peer(&self) -> Option<Arc<VerifiedPeerContext>> {
        self.verified_peer
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Publish a complete context before the `Verified` state becomes visible.
    ///
    /// The context write lock serializes rekey publication. If the state CAS
    /// loses to a concurrent terminal transition, the previous context/state is
    /// retained and no readiness notification is emitted. Strict cells cannot
    /// reach `Verified` through `set_state_validated`.
    pub(crate) fn publish_verified(
        &self,
        from: SecureChannelState,
        context: VerifiedPeerContext,
    ) -> bool {
        if !self.verified_only
            || !matches!(
                from,
                SecureChannelState::Handshaking | SecureChannelState::Rekeying
            )
            || !SecureChannelState::is_legal_transition(from, SecureChannelState::Verified)
        {
            return false;
        }

        let mut slot = self
            .verified_peer
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = slot.replace(Arc::new(context));
        if self
            .state
            .compare_exchange(
                from.as_u8(),
                SecureChannelState::Verified.as_u8(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            *slot = previous;
            return false;
        }
        drop(slot);
        self.secure_epoch.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_waiters();
        true
    }

    pub(crate) fn is_io_sendable(&self) -> bool {
        if !self.verified_only {
            return self.state().is_sendable();
        }
        self.state() == SecureChannelState::Verified && self.verified_peer().is_some()
    }

    /// Validated compare-and-set: applies `to` only if the current state is
    /// `from` AND `(from, to)` is a legal transition. On success bumps the
    /// epoch (AcqRel) and wakes waiters; returns `true`. Returns `false`
    /// otherwise (no state change, no wake).
    pub(crate) fn set_state_validated(
        &self,
        from: SecureChannelState,
        to: SecureChannelState,
    ) -> bool {
        if !SecureChannelState::is_legal_transition(from, to)
            || (self.verified_only && to == SecureChannelState::Verified)
        {
            return false;
        }
        let prev = self.state.compare_exchange(
            from.as_u8(),
            to.as_u8(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if prev.is_err() {
            return false;
        }
        self.secure_epoch.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_waiters();
        true
    }

    /// Drive the secure state to a terminal state regardless of the current
    /// value (used by `connect()` failure and `close()`). Returns the state
    /// that was observed before the transition (so callers can log it). Still
    /// validates legality: a terminal state is a no-op.
    pub(crate) fn drive_terminal(&self, to: SecureChannelState) -> SecureChannelState {
        let current = self.state();
        if !SecureCell::set_state_validated(self, current, to) {
            // `current` is already terminal, or `current → to` is illegal
            // (e.g. `Closed → Failed`); leave state untouched.
            return current;
        }
        current
    }

    /// Wait for physical-session retirement without treating a rekey as death.
    pub(crate) async fn wait_terminal(&self) {
        loop {
            let epoch = self.epoch();
            if self.state().is_terminal() {
                return;
            }
            self.wait_after(epoch).await;
        }
    }

    /// Wait until the epoch advances past `observed_epoch`. Mirrors
    /// `DataChannelStateSignal::wait_after` to close the `Notify` registration
    /// window: a transition landing between the generation check and arming the
    /// notification is still observed.
    pub(crate) async fn wait_after(&self, observed_epoch: u64) {
        loop {
            if self.epoch() != observed_epoch {
                return;
            }
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.epoch() != observed_epoch {
                return;
            }
            changed.await;
        }
    }
}

/// `DataLane` decorator that gates `send`/`recv` on the owning session's
/// `SecureCell`. Created at lane-creation time by the physical `WireHandle`
/// impl, so the *cached* lane is the gated lane — the inbound reader's
/// `recv()` and the outbound `send()` both pass through one gate (point 3).
///
/// The `epoch_at_wrap` snapshot invalidates already-acquired lanes on a
/// `Rekeying` rotation even if the state later returns to sendable: a lane
/// acquired under epoch N refuses I/O once the epoch advances to N+1, forcing
/// the caller to re-acquire a fresh lane bound to the new epoch.
#[derive(Debug)]
pub(crate) struct SecureDataLane {
    inner: Arc<dyn DataLane>,
    cell: SecureCell,
    epoch_at_wrap: u64,
}

impl SecureDataLane {
    pub(crate) fn new(inner: Arc<dyn DataLane>, cell: SecureCell) -> Self {
        Self {
            epoch_at_wrap: cell.epoch(),
            inner,
            cell,
        }
    }

    /// Check the secure gate. Error mapping is load-bearing for `DestTransport`:
    /// - terminal state → `ConnectionClosed` (closed-like; evicting the wire IS
    ///   the right response);
    /// - epoch rotated or alive-but-not-sendable (`New`/`Handshaking`/
    ///   `Rekeying`) → `SecureChannelNotReady` (NOT closed-like; the caller
    ///   waits for secure readiness and re-acquires a lane — a rekey must not
    ///   tear down the transport).
    ///
    /// Terminal MUST be checked before the epoch comparison: a terminal
    /// transition also bumps the epoch, so an epoch-first check would surface a
    /// dead wire as `SecureChannelNotReady` and park the caller on a readiness
    /// that can never arrive.
    fn gate(&self) -> NetworkResult<()> {
        let state = self.cell.state();
        if state.is_terminal() {
            return Err(NetworkError::ConnectionClosed(format!(
                "secure channel terminal ({state:?})"
            )));
        }
        if self.cell.epoch() != self.epoch_at_wrap {
            return Err(NetworkError::SecureChannelNotReady(
                "secure epoch rotated; lane no longer valid".to_string(),
            ));
        }
        if !self.cell.is_io_sendable() {
            return Err(NetworkError::SecureChannelNotReady(format!(
                "secure channel not sendable ({state:?}, verified context: {})",
                self.cell.verified_peer().is_some()
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl DataLane for SecureDataLane {
    async fn send(&self, data: Bytes) -> NetworkResult<()> {
        self.send_with_progress(data).await.into_result()
    }

    async fn send_with_progress(&self, data: Bytes) -> SendAttemptResult {
        // The gate check is provably pre-send (it inspects session state
        // before touching `inner`), so gate failures stay `NotAttempted`:
        // the bounded readiness-wait → lane re-acquisition self-heal trip is
        // safe because nothing entered a transport. A permanently rotated
        // epoch wastes at most that one bounded trip before the lane is
        // re-wrapped on the current epoch or its wire is evicted.
        if let Err(error) = self.gate() {
            return SendAttemptResult::failed_pre_send(error);
        }
        self.inner.send_with_progress(data).await
    }

    async fn recv(&self) -> NetworkResult<Bytes> {
        self.gate()?;
        // Race the pending receive against a secure rotation and re-check
        // before delivery: a receive that starts while `Verified` must not
        // deliver its frame after the session entered `Rekeying` (or any other
        // non-sendable state). Canceling `inner.recv()` may drop one in-flight
        // frame — that is the correct fail-closed behavior on rotation.
        let frame = tokio::select! {
            frame = self.inner.recv() => frame?,
            _ = self.cell.wait_after(self.epoch_at_wrap) => {
                // Epoch rotated while the receive was pending; gate() now
                // reports the precise cause (terminal vs not-ready).
                self.gate()?;
                unreachable!("gate must fail after epoch rotation");
            }
        };
        self.gate()?;
        Ok(frame)
    }

    async fn try_recv(&self) -> NetworkResult<Option<Bytes>> {
        self.gate()?;
        self.inner.try_recv().await
    }

    async fn send_envelope(&self, envelope: actr_protocol::RpcEnvelope) -> NetworkResult<()> {
        self.gate()?;
        self.inner.send_envelope(envelope).await
    }

    async fn recv_envelope(&self) -> NetworkResult<actr_protocol::RpcEnvelope> {
        self.gate()?;
        // Same rotation race + pre-delivery re-check as `recv` above.
        let envelope = tokio::select! {
            envelope = self.inner.recv_envelope() => envelope?,
            _ = self.cell.wait_after(self.epoch_at_wrap) => {
                self.gate()?;
                unreachable!("gate must fail after epoch rotation");
            }
        };
        self.gate()?;
        Ok(envelope)
    }

    fn lane_type(&self) -> &'static str {
        self.inner.lane_type()
    }

    fn is_healthy(&self) -> bool {
        self.cell.is_io_sendable()
            && self.cell.epoch() == self.epoch_at_wrap
            && self.inner.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified_context(channel_generation: u64) -> VerifiedPeerContext {
        VerifiedPeerContext {
            local_actor_id: ActrId {
                realm: "local@example.com".into(),
                serial_number: 1,
            },
            peer_actor_id: ActrId {
                realm: "peer@example.com".into(),
                serial_number: 2,
            },
            peer_actor_type: ActrType {
                publisher: "acme".into(),
                name: "front".into(),
                version: "1.0.0".into(),
            },
            peer_member: "member-1".into(),
            peer_client_key: "client-1".into(),
            peer_role: "opaque-verified-role".into(),
            peer_actor_public_key: vec![7; 32],
            channel_id: "channel-1".into(),
            channel_generation,
            transcript_digest: vec![8; 32],
            local_runtime_instance_id: "runtime-local".into(),
            peer_runtime_instance_id: "runtime-peer".into(),
            peer_credential_expires_at_epoch_secs: 4_000_000_000,
            current_policy_binding: vec![9; 32],
            current_state_binding: vec![10; 32],
        }
    }

    /// Inner lane whose send fails after an ambiguous partial write.
    #[derive(Debug)]
    struct AmbiguousInnerLane;

    #[async_trait::async_trait]
    impl DataLane for AmbiguousInnerLane {
        async fn send(&self, _data: Bytes) -> NetworkResult<()> {
            unreachable!("AmbiguousInnerLane is driven through send_with_progress")
        }

        async fn send_with_progress(&self, _data: Bytes) -> SendAttemptResult {
            SendAttemptResult::failed(
                NetworkError::SendError("partial write".into()),
                DeliveryProgress::Unknown,
            )
        }

        fn lane_type(&self) -> &'static str {
            "ambiguous-inner"
        }
    }

    #[tokio::test]
    async fn secure_lane_preserves_inner_uncertain_progress() {
        let cell = SecureCell::new_actrium_verified(verified_context(1));
        let lane = SecureDataLane::new(Arc::new(AmbiguousInnerLane), cell);

        let attempt = lane
            .send_with_progress(Bytes::from_static(b"payload"))
            .await;

        assert!(
            matches!(attempt.result, Err(NetworkError::SendError(_))),
            "the inner error must pass through the decorator: {attempt:?}"
        );
        assert_eq!(
            attempt.progress,
            DeliveryProgress::Unknown,
            "the decorator must not reclassify an inner ambiguous failure"
        );
    }

    #[tokio::test]
    async fn secure_gate_failure_is_proven_not_attempted() {
        // A not-sendable secure state rejects before the inner lane is ever
        // invoked: no bytes can have entered the transport.
        let cell = SecureCell::new(SecureChannelState::Rekeying);
        let lane = SecureDataLane::new(Arc::new(AmbiguousInnerLane), cell);

        let attempt = lane
            .send_with_progress(Bytes::from_static(b"payload"))
            .await;

        assert!(matches!(
            attempt.result,
            Err(NetworkError::SecureChannelNotReady(_))
        ));
        assert_eq!(attempt.progress, DeliveryProgress::NotAttempted);
    }

    #[tokio::test]
    async fn actrium_cell_allows_io_only_after_context_and_verified_publication() {
        let cell = SecureCell::new_actrium(SecureChannelState::Handshaking);
        let lane = SecureDataLane::new(Arc::new(AmbiguousInnerLane), cell.clone());

        let before = lane
            .send_with_progress(Bytes::from_static(b"payload"))
            .await;
        assert!(matches!(
            before.result,
            Err(NetworkError::SecureChannelNotReady(_))
        ));
        assert_eq!(before.progress, DeliveryProgress::NotAttempted);
        assert!(matches!(
            lane.recv().await,
            Err(NetworkError::SecureChannelNotReady(_))
        ));
        assert!(cell.verified_peer().is_none());

        assert!(
            !cell.set_state_validated(
                SecureChannelState::Handshaking,
                SecureChannelState::Verified,
            ),
            "strict cells cannot publish Verified without a context"
        );
        let context = verified_context(7);
        assert!(cell.publish_verified(SecureChannelState::Handshaking, context.clone()));
        assert_eq!(cell.verified_peer().as_deref(), Some(&context));

        // Lanes wrap an epoch, so acquire a fresh lane after publication just
        // as the production readiness/reacquisition path does.
        let lane = SecureDataLane::new(Arc::new(AmbiguousInnerLane), cell);
        let after = lane
            .send_with_progress(Bytes::from_static(b"payload"))
            .await;
        assert!(
            matches!(after.result, Err(NetworkError::SendError(_))),
            "the verified lane must reach the inner transport: {after:?}"
        );
        assert_eq!(after.progress, DeliveryProgress::Unknown);
        assert!(
            matches!(lane.recv().await, Err(NetworkError::InvalidOperation(_))),
            "a verified receive must reach the inner lane rather than stop at the secure gate"
        );
    }

    #[test]
    fn actrium_cell_never_treats_legacy_bypass_as_io_ready() {
        let cell = SecureCell::new_actrium(SecureChannelState::LegacyBypass);
        assert!(!cell.is_io_sendable());
        assert_eq!(cell.state(), SecureChannelState::LegacyBypass);
        assert!(cell.verified_peer().is_none());
    }

    #[test]
    fn actrium_rekey_requires_a_new_generation_context() {
        let cell = SecureCell::new_actrium_verified(verified_context(1));
        assert!(
            cell.set_state_validated(SecureChannelState::Verified, SecureChannelState::Rekeying,)
        );
        assert!(!cell.is_io_sendable());
        assert!(
            !cell.set_state_validated(SecureChannelState::Rekeying, SecureChannelState::Verified,),
            "rekey cannot silently reuse generation 1"
        );

        let generation_two = verified_context(2);
        assert!(cell.publish_verified(SecureChannelState::Rekeying, generation_two.clone()));
        assert_eq!(cell.verified_peer().as_deref(), Some(&generation_two));
        assert!(cell.is_io_sendable());
    }

    #[test]
    fn failed_verified_publication_preserves_previous_context_and_state() {
        let cell = SecureCell::new_actrium_verified(verified_context(3));
        let before = cell.verified_peer();
        assert!(!cell.publish_verified(SecureChannelState::Verified, verified_context(4)));
        assert_eq!(cell.state(), SecureChannelState::Verified);
        assert_eq!(cell.verified_peer(), before);
        assert_eq!(cell.epoch(), 1);
    }
}
