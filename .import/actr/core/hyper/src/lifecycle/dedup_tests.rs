use super::*;

fn ok_bytes(s: &str) -> ActorResult<Bytes> {
    Ok(Bytes::from(s.as_bytes().to_vec()))
}

#[test]
fn fresh_request_is_marked_and_concurrent_duplicate_returns_waiter() {
    let mut d = DedupState::new();
    assert!(matches!(d.check_or_mark("req-1"), DedupOutcome::Fresh));
    assert_eq!(d.len(), 1);
    assert!(matches!(
        d.check_or_mark("req-1"),
        DedupOutcome::InFlight(_)
    ));
}

#[tokio::test]
async fn in_flight_duplicate_waiter_receives_original_result() {
    let mut d = DedupState::new();
    assert!(matches!(d.check_or_mark("req-1"), DedupOutcome::Fresh));

    let mut waiter = match d.check_or_mark("req-1") {
        DedupOutcome::InFlight(waiter) => waiter,
        other => panic!("expected InFlight waiter, got {other:?}"),
    };

    d.complete("req-1", ok_bytes("hello"));
    let _ = waiter.changed().await;

    let result = waiter
        .borrow()
        .clone()
        .expect("waiter should observe completed result");
    assert!(
        matches!(result, Ok(ref b) if b == "hello"),
        "expected waiter to receive original Ok(\"hello\")"
    );
}

#[test]
fn in_flight_entry_is_not_evicted_by_completed_cache_ttl() {
    let mut d = DedupState {
        ttl: Duration::from_nanos(1),
        ..DedupState::new()
    };
    assert!(matches!(d.check_or_mark("req-slow"), DedupOutcome::Fresh));

    // A different request triggers lazy eviction. The in-flight entry must
    // stay protected even when completed-cache TTL is tiny.
    d.check_or_mark("req-other");
    assert!(matches!(
        d.check_or_mark("req-slow"),
        DedupOutcome::InFlight(_)
    ));
}

#[test]
fn dedup_ttl_covers_reliable_rpc_retry_window() {
    assert!(
        DEDUP_TTL >= Duration::from_secs(20),
        "dedup TTL should cover late RpcReliable retries"
    );
}

#[test]
fn completed_duplicate_returns_cached_success_or_error() {
    let mut d = DedupState::new();
    d.check_or_mark("req-1");
    d.complete("req-1", ok_bytes("hello"));

    let outcome = d.check_or_mark("req-1");
    assert!(
        matches!(outcome, DedupOutcome::Duplicate(Ok(ref b)) if b == "hello"),
        "expected cached Ok(\"hello\")"
    );

    use actr_protocol::ActrError;
    let mut d = DedupState::new();
    d.check_or_mark("req-err");
    d.complete(
        "req-err",
        Err(ActrError::InvalidArgument("bad input".to_string())),
    );

    let outcome = d.check_or_mark("req-err");
    assert!(
        matches!(
            outcome,
            DedupOutcome::Duplicate(Err(ActrError::InvalidArgument(_)))
        ),
        "expected cached Err"
    );
}

#[test]
fn expired_entry_is_evicted_and_treated_as_fresh() {
    let mut d = DedupState {
        ttl: Duration::from_nanos(1), // expire immediately
        ..DedupState::new()
    };
    d.check_or_mark("req-old");
    d.complete("req-old", ok_bytes("v1"));

    // Force TTL expiry by advancing: we can't time-travel Instant in stable Rust,
    // so set TTL to 0 and trigger eviction on the next check.
    // Insert a fresh entry to trigger evict_expired, then re-check the old one.
    d.check_or_mark("req-new"); // triggers evict with ttl=1ns, old entry expires
    assert!(matches!(d.check_or_mark("req-old"), DedupOutcome::Fresh));
}

#[test]
fn different_request_ids_are_independent() {
    let mut d = DedupState::new();
    d.check_or_mark("req-a");
    d.complete("req-a", ok_bytes("a"));

    assert!(matches!(d.check_or_mark("req-b"), DedupOutcome::Fresh));
    assert!(matches!(
        d.check_or_mark("req-a"),
        DedupOutcome::Duplicate(_)
    ));
}

#[tokio::test]
async fn explicit_inflight_sweep_evicts_aged_entry_and_notifies_waiter() {
    // RFC-0323 §5/§7: an EXPLICIT InFlight marker that never completes
    // (e.g. a Reply parked forever) must be swept — scoped to the
    // `outstanding_replies` registry ids — and its parked duplicate waiter
    // notified with TimedOut, not left hanging.
    let mut d = DedupState::new();
    assert!(matches!(d.check_or_mark("req-parked"), DedupOutcome::Fresh));

    let mut waiter = match d.check_or_mark("req-parked") {
        DedupOutcome::InFlight(w) => w,
        other => panic!("expected InFlight waiter, got {other:?}"),
    };

    // Aged beyond INFLIGHT_TTL: the scoped sweep must evict + send TimedOut.
    let future = std::time::Instant::now() + INFLIGHT_TTL + Duration::from_secs(1);
    d.sweep_inflight_explicit(future, &["req-parked".to_string()]);

    let _ = waiter.changed().await;
    let result = waiter.borrow().clone().expect("waiter notified");
    assert!(
        matches!(result, Err(actr_protocol::ActrError::TimedOut)),
        "expected Err(TimedOut) after sweep, got {result:?}"
    );

    // The InFlight marker is gone: a fresh check_or_mark now marks again.
    assert!(matches!(d.check_or_mark("req-parked"), DedupOutcome::Fresh));
}

#[tokio::test]
async fn explicit_inflight_sweep_does_not_evict_unaged_entry() {
    // Below INFLIGHT_TTL the scoped sweep must leave the InFlight marker
    // intact so a fast duplicate still waits for the real result.
    let mut d = DedupState::new();
    assert!(matches!(d.check_or_mark("req-live"), DedupOutcome::Fresh));

    let now = std::time::Instant::now();
    d.sweep_inflight_explicit(now, &["req-live".to_string()]);

    assert!(matches!(
        d.check_or_mark("req-live"),
        DedupOutcome::InFlight(_)
    ));
}

#[tokio::test]
async fn expired_reply_boundary_evicts_dedup_before_generic_ttl() {
    let mut d = DedupState::new();
    assert!(matches!(
        d.check_or_mark("req-deadline"),
        DedupOutcome::Fresh
    ));
    let mut waiter = match d.check_or_mark("req-deadline") {
        DedupOutcome::InFlight(waiter) => waiter,
        other => panic!("expected InFlight waiter, got {other:?}"),
    };

    // The reply registry has already decided this request crossed its caller
    // deadline. It must free dedup now, rather than waiting for INFLIGHT_TTL.
    d.expire_inflight_explicit(&["req-deadline".to_string()]);

    let _ = waiter.changed().await;
    assert!(matches!(
        waiter.borrow().clone(),
        Some(Err(actr_protocol::ActrError::TimedOut))
    ));
    assert!(matches!(
        d.check_or_mark("req-deadline"),
        DedupOutcome::Fresh
    ));
}

#[tokio::test]
async fn return_inflight_is_not_evicted_by_explicit_sweep() {
    // RFC-0323 §5: a RETURN handler may legitimately run longer than
    // INFLIGHT_TTL. Its InFlight marker is NOT in the outstanding_replies
    // registry, so the scoped sweep must leave it intact — otherwise a late
    // retry would re-run the handler (cache flip / double side-effects).
    let mut d = DedupState::new();
    assert!(matches!(
        d.check_or_mark("req-slow-return"),
        DedupOutcome::Fresh
    ));

    // Pretend a duplicate parked on it.
    let waiter = match d.check_or_mark("req-slow-return") {
        DedupOutcome::InFlight(w) => w,
        other => panic!("expected InFlight waiter, got {other:?}"),
    };

    // Aged beyond INFLIGHT_TTL, but the id is NOT EXPLICIT (absent from the
    // explicit_ids set the sweep is scoped to). The entry must stay, and no
    // TimedOut notification is sent to its parked duplicate waiter.
    let future = std::time::Instant::now() + INFLIGHT_TTL + Duration::from_secs(1);
    d.sweep_inflight_explicit(future, &["req-explicit-other".to_string()]);

    assert!(
        waiter.borrow().is_none(),
        "RETURN in-flight waiter must not be notified by the EXPLICIT sweep"
    );
    assert!(matches!(
        d.check_or_mark("req-slow-return"),
        DedupOutcome::InFlight(_)
    ));
}
