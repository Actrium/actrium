//! Per-hop request scopes: cooperative cancellation and absolute-deadline
//! propagation for RPC calls (#390).
//!
//! A Kotlin caller's coroutine cancellation surfaces as the drop of the
//! exported UniFFI future. Every root `call_raw` therefore owns a
//! [`ScopeGuard`]: completing normally disarms it, while timeout, error, or
//! an arbitrary future drop cancels the [`RequestScope`] exactly once. The
//! scope's hierarchical `CancellationToken` and absolute deadline are
//! inherited by nested guest calls (each hop gets a fresh request id and
//! `min(parent deadline, now + timeout)`), so descendant host-awaits return
//! promptly; Calls preserve uncertainty after dispatch and admission points
//! reject work that never started.
//!
//! Cancellation is cooperative and scoped per root: it never poisons an
//! executor runner (that policy stays reserved for real reply-channel
//! loss) and never touches unrelated roots.
//!
//! The [`ScopeHandoffRegistry`] transfers a root scope from the sending
//! side of the in-process transport to workload intake through a private
//! versioned envelope marker. The marker is stripped before ACL,
//! deduplication, or dispatch and never leaves the process.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actr_protocol::RpcEnvelope;
use tokio_util::sync::CancellationToken;

/// Envelope metadata key carrying the in-process scope handoff marker.
pub(crate) const SCOPE_MARKER_KEY: &str = "actr.internal.scope.v1";

/// Marker version understood by this build.
const SCOPE_MARKER_VERSION: &str = "1";

const STATE_PENDING: u8 = 0;
const STATE_COMPLETED: u8 = 1;
const STATE_CANCELLED: u8 = 2;

/// Why a scope left the pending state. Diagnostics only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    /// The absolute deadline expired; the caller classifies delivery separately.
    DeadlineExpired,
    /// The owning caller's future was dropped or bailed out with an error.
    CallerDropped,
}

/// One hop of an RPC call tree: cancellation, deadline, and identity.
///
/// The atomic terminal state guarantees a single outcome under the
/// response/deadline/drop races; the token makes cancellation hierarchical.
pub(crate) struct RequestScope {
    request_id: String,
    root_request_id: String,
    deadline: tokio::time::Instant,
    token: CancellationToken,
    state: AtomicU8,
    reason: Mutex<Option<CancelReason>>,
}

impl RequestScope {
    fn new(
        request_id: String,
        root_request_id: String,
        deadline: tokio::time::Instant,
        token: CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            request_id,
            root_request_id,
            deadline,
            token,
            state: AtomicU8::new(STATE_PENDING),
            reason: Mutex::new(None),
        })
    }

    /// Start a root scope whose absolute deadline is `now + timeout_ms`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn root(timeout_ms: i64) -> Arc<Self> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(timeout_ms.max(1) as u64);
        Self::root_at(deadline)
    }

    /// Start a root scope with a deadline that was created by the outbound API
    /// entry point.  Keeping the instant here avoids a second clock read for
    /// the same logical operation.
    pub(crate) fn root_at(deadline: tokio::time::Instant) -> Arc<Self> {
        let request_id = uuid::Uuid::new_v4().to_string();
        Self::new(
            request_id.clone(),
            request_id,
            deadline,
            CancellationToken::new(),
        )
    }

    /// Derive the next hop: fresh request id, child token, and the earlier
    /// of the parent deadline and this hop's own timeout budget.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn child(&self, timeout_ms: i64) -> Arc<Self> {
        let own = tokio::time::Instant::now() + Duration::from_millis(timeout_ms.max(1) as u64);
        self.child_at(own)
    }

    /// Derive a child scope from an already-created operation deadline.
    pub(crate) fn child_at(&self, deadline: tokio::time::Instant) -> Arc<Self> {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            self.root_request_id.clone(),
            deadline.min(self.deadline),
            self.token.child_token(),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Stable identity of the root call; diagnostics and logging only.
    pub(crate) fn root_request_id(&self) -> &str {
        &self.root_request_id
    }

    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn deadline(&self) -> tokio::time::Instant {
        self.deadline
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub(crate) fn deadline_passed(&self) -> bool {
        self.deadline
            .checked_duration_since(tokio::time::Instant::now())
            .is_none()
    }

    /// False once cancelled or past the deadline; admission points gate
    /// Guest invocation on this.
    pub(crate) fn is_active(&self) -> bool {
        !self.is_cancelled() && !self.deadline_passed()
    }

    /// Remaining absolute budget, rounded up to whole milliseconds with a
    /// floor of one millisecond, or `None` once the deadline has passed.
    /// No stage of the call tree starts a fresh full timeout; every
    /// downstream wait receives this value.
    pub(crate) fn remaining_ms(&self) -> Option<i64> {
        let remaining = self
            .deadline
            .checked_duration_since(tokio::time::Instant::now())?;
        let millis = remaining.as_micros().div_ceil(1_000);
        Some(millis.max(1) as i64)
    }

    /// Record the single successful terminal outcome. Returns false when
    /// the scope already reached a terminal state (a late completion after
    /// cancellation loses the race).
    pub(crate) fn record_completed(&self) -> bool {
        self.state
            .compare_exchange(
                STATE_PENDING,
                STATE_COMPLETED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Record cancellation and fire the token (idempotent; a completed
    /// scope stays completed). Descendant host-awaits observe the token and
    /// unwind promptly without claiming that submitted work did not execute.
    pub(crate) fn cancel(&self, reason: CancelReason) {
        if self
            .state
            .compare_exchange(
                STATE_PENDING,
                STATE_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            *self
                .reason
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(reason);
            tracing::debug!(
                request_id = %self.request_id,
                root_request_id = %self.root_request_id,
                reason = ?reason,
                "rpc.scope_cancelled"
            );
            self.token.cancel();
        }
    }

    /// Terminal reason, when the scope already ended.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn cancel_reason(&self) -> Option<CancelReason> {
        *self
            .reason
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Wrap this scope in the cancel-on-drop guard owned by the caller.
    pub(crate) fn guard(self: &Arc<Self>) -> ScopeGuard {
        ScopeGuard {
            scope: self.clone(),
            disarmed: false,
        }
    }
}

impl std::fmt::Debug for RequestScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestScope")
            .field("request_id", &self.request_id)
            .field("root_request_id", &self.root_request_id)
            .field("cancelled", &self.is_cancelled())
            .field("deadline_passed", &self.deadline_passed())
            .finish()
    }
}

/// Cancellation-on-drop ownership of a [`RequestScope`].
///
/// The root caller holds this across session readiness, send, and the
/// response wait. Normal completion disarms the guard; timeout records the
/// deadline reason; any other exit (send failure, caller error, arbitrary
/// future drop) cancels the scope as `CallerDropped`.
pub(crate) struct ScopeGuard {
    scope: Arc<RequestScope>,
    disarmed: bool,
}

impl ScopeGuard {
    /// Successful terminal path: record completion and stop the drop path
    /// from cancelling descendants.
    pub(crate) fn disarm(mut self) {
        self.disarmed = true;
        self.scope.record_completed();
    }

    /// Explicit cancellation with a precise reason (deadline expiry).
    pub(crate) fn cancel_with(mut self, reason: CancelReason) {
        self.disarmed = true;
        self.scope.cancel(reason);
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if !self.disarmed {
            self.scope.cancel(CancelReason::CallerDropped);
        }
    }
}

type HandoffMap = Arc<Mutex<HashMap<String, Arc<RequestScope>>>>;

/// Transport-local transfer of root scopes from the sending side to
/// workload intake, keyed by an opaque handoff id carried in the envelope
/// marker (not the request id, which dedup sees).
///
/// The map is only touched under a synchronous `std::sync::Mutex`; it is
/// never held across an `.await`.
#[derive(Clone)]
pub(crate) struct ScopeHandoffRegistry {
    inner: HandoffMap,
}

impl Default for ScopeHandoffRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeHandoffRegistry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register `scope` under a fresh handoff key. The returned guard keeps
    /// the entry alive; dropping it (send failure, timeout, caller future
    /// drop) removes the entry, so a later intake claim observes
    /// [`ScopeClaim::Abandoned`] instead of resurrecting state.
    pub(crate) fn register(&self, scope: Arc<RequestScope>) -> HandoffGuard {
        let key = uuid::Uuid::new_v4().to_string();
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.clone(), scope);
        HandoffGuard {
            registry: self.inner.clone(),
            key,
        }
    }

    /// Atomically remove and return the scope for `key`, if still present.
    pub(crate) fn claim(&self, key: &str) -> Option<Arc<RequestScope>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

/// RAII registry entry; dropping removes the handoff.
pub(crate) struct HandoffGuard {
    registry: HandoffMap,
    key: String,
}

impl HandoffGuard {
    /// The marker value to place on the envelope (`<version>:<key>`).
    pub(crate) fn marker_value(&self) -> String {
        format!("{SCOPE_MARKER_VERSION}:{}", self.key)
    }
}

impl Drop for HandoffGuard {
    fn drop(&mut self) {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
    }
}

/// Outcome of stripping the handoff marker from an inbound envelope.
#[derive(Debug)]
pub(crate) enum ScopeClaim {
    /// No marker: an ordinary inbound request (remote or scope-less local).
    Unmarked,
    /// Valid marker and the scope was still registered: the scope now
    /// belongs to the linked guest context.
    Claimed(Arc<RequestScope>),
    /// Valid marker but the registry entry is gone: the caller cancelled
    /// between the in-process send and intake. Drop before Guest admission.
    Abandoned,
    /// Malformed value or duplicated marker entries. Drop before ACL.
    Invalid,
}

/// Attach the handoff marker for `scope` to `envelope`. The returned guard
/// must outlive the send-and-wait future it was created in.
pub(crate) fn attach_scope_marker(
    envelope: &mut RpcEnvelope,
    registry: &ScopeHandoffRegistry,
    scope: &Arc<RequestScope>,
) -> HandoffGuard {
    let guard = registry.register(scope.clone());
    envelope.metadata.push(actr_protocol::MetadataEntry {
        key: SCOPE_MARKER_KEY.to_string(),
        value: guard.marker_value(),
    });
    guard
}

/// Remove the marker (if any) and claim its scope. All marker entries are
/// removed from the envelope regardless of outcome, so ACL, deduplication,
/// and dispatch never observe them.
pub(crate) fn strip_and_claim(
    envelope: &mut RpcEnvelope,
    registry: &ScopeHandoffRegistry,
) -> ScopeClaim {
    let mut markers = Vec::new();
    envelope.metadata.retain(|entry| {
        if entry.key == SCOPE_MARKER_KEY {
            markers.push(entry.value.clone());
            false
        } else {
            true
        }
    });
    match markers.len() {
        0 => ScopeClaim::Unmarked,
        1 => match parse_marker(&markers[0]) {
            Some(key) => match registry.claim(key) {
                Some(scope) => ScopeClaim::Claimed(scope),
                None => ScopeClaim::Abandoned,
            },
            None => ScopeClaim::Invalid,
        },
        _ => ScopeClaim::Invalid,
    }
}

fn parse_marker(value: &str) -> Option<&str> {
    let (version, key) = value.split_once(':')?;
    if version == SCOPE_MARKER_VERSION && !key.is_empty() {
        Some(key)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope_with_marker(value: &str) -> RpcEnvelope {
        RpcEnvelope {
            metadata: vec![actr_protocol::MetadataEntry {
                key: SCOPE_MARKER_KEY.to_string(),
                value: value.to_string(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn guard_drop_cancels_with_caller_dropped() {
        let scope = RequestScope::root(1_000);
        {
            let _guard = scope.guard();
        }
        assert!(scope.is_cancelled());
        assert_eq!(scope.cancel_reason(), Some(CancelReason::CallerDropped));
    }

    #[test]
    fn disarm_records_completion_and_disarms_drop() {
        let scope = RequestScope::root(1_000);
        scope.guard().disarm();
        assert!(!scope.is_cancelled());
        assert_eq!(scope.cancel_reason(), None);
    }

    #[test]
    fn cancel_after_completion_is_ignored() {
        let scope = RequestScope::root(1_000);
        scope.guard().disarm();
        scope.cancel(CancelReason::DeadlineExpired);
        assert!(!scope.is_cancelled());
        assert_eq!(scope.cancel_reason(), None);
    }

    #[test]
    fn guard_cancel_with_records_reason() {
        let scope = RequestScope::root(1_000);
        scope.guard().cancel_with(CancelReason::DeadlineExpired);
        assert!(scope.is_cancelled());
        assert_eq!(scope.cancel_reason(), Some(CancelReason::DeadlineExpired));
    }

    #[test]
    fn double_cancel_records_first_reason_once() {
        let scope = RequestScope::root(1_000);
        scope.cancel(CancelReason::DeadlineExpired);
        scope.cancel(CancelReason::CallerDropped);
        assert_eq!(scope.cancel_reason(), Some(CancelReason::DeadlineExpired));
    }

    #[test]
    fn child_inherits_earlier_deadline() {
        let parent = RequestScope::root(10_000);
        let slow_child = parent.child(600_000);
        assert!(slow_child.deadline() == parent.deadline());

        let parent = RequestScope::root(600_000);
        let fast_child = parent.child(10);
        let gap = fast_child
            .deadline()
            .checked_duration_since(tokio::time::Instant::now());
        assert!(gap.is_some_and(|g| g <= Duration::from_millis(10)));
    }

    #[test]
    fn parent_cancel_cancels_child_not_vice_versa() {
        let parent = RequestScope::root(1_000);
        let child = parent.child(1_000);
        let grandchild = child.child(1_000);
        child.cancel(CancelReason::CallerDropped);
        assert!(child.is_cancelled());
        assert!(grandchild.is_cancelled());
        assert!(parent.is_active());
    }

    #[test]
    fn child_keeps_fresh_request_id_and_root_identity() {
        let parent = RequestScope::root(1_000);
        let child = parent.child(1_000);
        assert_ne!(parent.request_id(), child.request_id());
        assert_eq!(parent.root_request_id(), child.root_request_id());
        assert_eq!(parent.root_request_id(), parent.request_id());
    }

    #[test]
    fn remaining_ms_rounds_up_and_floors_at_one() {
        let scope = RequestScope::root(1);
        // A sub-millisecond remainder must round up to a 1 ms budget.
        std::thread::sleep(Duration::from_micros(200));
        if !scope.deadline_passed() {
            assert_eq!(scope.remaining_ms(), Some(1));
        }
        // Past the deadline there is no budget left.
        std::thread::sleep(Duration::from_millis(2));
        assert!(scope.deadline_passed());
        assert_eq!(scope.remaining_ms(), None);
        assert!(!scope.is_active());
    }

    #[test]
    fn handoff_guard_drop_removes_entry() {
        let registry = ScopeHandoffRegistry::new();
        let scope = RequestScope::root(1_000);
        let guard = registry.register(scope);
        let marker = guard.marker_value();
        assert_eq!(registry.len(), 1);
        drop(guard);
        assert_eq!(registry.len(), 0);

        let mut envelope = envelope_with_marker(&marker);
        assert!(matches!(
            strip_and_claim(&mut envelope, &registry),
            ScopeClaim::Abandoned
        ));
        assert!(envelope.metadata.is_empty());
    }

    #[test]
    fn strip_and_claim_transfers_scope_once() {
        let registry = ScopeHandoffRegistry::new();
        let scope = RequestScope::root(1_000);
        let guard = registry.register(scope.clone());
        let marker = guard.marker_value();

        let mut envelope = envelope_with_marker(&marker);
        match strip_and_claim(&mut envelope, &registry) {
            ScopeClaim::Claimed(claimed) => assert_eq!(claimed.request_id(), scope.request_id()),
            other => panic!("expected Claimed, got {other:?}"),
        }
        assert!(envelope.metadata.is_empty());
        assert_eq!(registry.len(), 0);

        // A redelivery of the same marker is abandoned, never re-claimed.
        let mut envelope = envelope_with_marker(&marker);
        assert!(matches!(
            strip_and_claim(&mut envelope, &registry),
            ScopeClaim::Abandoned
        ));
    }

    #[test]
    fn unmarked_envelope_is_untouched() {
        let registry = ScopeHandoffRegistry::new();
        let mut envelope = RpcEnvelope::default();
        envelope.metadata.push(actr_protocol::MetadataEntry {
            key: "user.header".to_string(),
            value: "keep".to_string(),
        });
        assert!(matches!(
            strip_and_claim(&mut envelope, &registry),
            ScopeClaim::Unmarked
        ));
        assert_eq!(envelope.metadata.len(), 1);
    }

    #[test]
    fn invalid_and_duplicate_markers_are_rejected() {
        let registry = ScopeHandoffRegistry::new();
        for bad in ["1", "2:key", "garbage", ":key", "1:"] {
            let mut envelope = envelope_with_marker(bad);
            assert!(
                matches!(
                    strip_and_claim(&mut envelope, &registry),
                    ScopeClaim::Invalid
                ),
                "value {bad:?} must be Invalid"
            );
            assert!(envelope.metadata.is_empty());
        }

        // Two marker entries on one envelope are ambiguous.
        let scope = RequestScope::root(1_000);
        let guard = registry.register(scope);
        let marker = guard.marker_value();
        let mut envelope = envelope_with_marker(&marker);
        envelope.metadata.push(actr_protocol::MetadataEntry {
            key: SCOPE_MARKER_KEY.to_string(),
            value: marker,
        });
        assert!(matches!(
            strip_and_claim(&mut envelope, &registry),
            ScopeClaim::Invalid
        ));
    }

    #[test]
    fn attach_scope_marker_places_single_entry() {
        let registry = ScopeHandoffRegistry::new();
        let scope = RequestScope::root(1_000);
        let mut envelope = RpcEnvelope::default();
        let guard = attach_scope_marker(&mut envelope, &registry, &scope);
        assert_eq!(envelope.metadata.len(), 1);
        assert_eq!(envelope.metadata[0].key, SCOPE_MARKER_KEY);
        assert_eq!(registry.len(), 1);
        assert!(matches!(
            strip_and_claim(&mut envelope, &registry),
            ScopeClaim::Claimed(_)
        ));
        drop(guard);
    }
}
