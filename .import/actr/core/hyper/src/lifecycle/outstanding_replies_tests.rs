use super::*;

#[tokio::test]
async fn register_complete_balances_counter() {
    let r = OutstandingReplies::new();
    r.register("req-1").await;
    r.register("req-2").await;
    assert_eq!(r.len(), 2);

    r.complete("req-1").await;
    assert_eq!(r.len(), 1);
    r.complete("req-2").await;
    assert_eq!(r.len(), 0);
}

#[tokio::test]
async fn complete_is_idempotent() {
    // RFC-0323 §5: the Deliver + the sweep can race; complete must not
    // under-decrement the counter on a double call.
    let r = OutstandingReplies::new();
    r.register("req-1").await;
    r.complete("req-1").await;
    assert_eq!(r.len(), 0);
    r.complete("req-1").await; // no-op
    assert_eq!(r.len(), 0);
}

#[tokio::test]
async fn sweep_evicts_aged_and_releases_counter() {
    // RFC-0323 §7: a Reply never sent must not leak the counter (which would
    // pin the pump at max_pending_replies forever).
    let r = OutstandingReplies::new();
    r.register("req-leak").await;
    assert_eq!(r.len(), 1);

    let ttl = Duration::from_secs(60);
    let future = std::time::Instant::now() + ttl + Duration::from_secs(1);
    r.sweep(ttl, future).await;
    assert_eq!(r.len(), 0);
}

#[tokio::test]
async fn sweep_keeps_unaged_entries() {
    let r = OutstandingReplies::new();
    r.register("req-live").await;
    let ttl = Duration::from_secs(60);
    r.sweep(ttl, std::time::Instant::now()).await;
    assert_eq!(r.len(), 1);
}
