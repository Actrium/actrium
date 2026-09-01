use super::*;

#[tokio::test(start_paused = true)]
async fn accept_error_retries_only_after_the_backoff_deadline() {
    let shutdown = CancellationToken::new();
    let backoff = std::time::Duration::from_millis(100);
    let waiter = {
        let shutdown = shutdown.clone();
        tokio::spawn(async move { wait_after_accept_error(&shutdown, backoff).await })
    };

    tokio::task::yield_now().await;
    assert!(!waiter.is_finished(), "retry must remain parked initially");

    tokio::time::advance(backoff - std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert!(
        !waiter.is_finished(),
        "retry must not run before the backoff deadline"
    );

    tokio::time::advance(std::time::Duration::from_millis(1)).await;
    assert!(waiter.await.expect("backoff task must not panic"));
}

#[tokio::test(start_paused = true)]
async fn shutdown_preempts_accept_error_backoff_without_advancing_time() {
    let shutdown = CancellationToken::new();
    let started_at = tokio::time::Instant::now();
    let waiter = {
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            wait_after_accept_error(&shutdown, std::time::Duration::from_secs(30)).await
        })
    };

    tokio::task::yield_now().await;
    shutdown.cancel();

    assert!(!waiter.await.expect("backoff task must not panic"));
    assert_eq!(
        tokio::time::Instant::now(),
        started_at,
        "shutdown must not wait for the backoff deadline"
    );
}

#[tokio::test]
async fn bind_uses_the_audited_production_backoff_duration() {
    let (server, _incoming) = WebSocketServer::bind(0)
        .await
        .expect("bind an ephemeral listener");

    assert_eq!(
        server.accept_backoff,
        crate::timer::ids::WEBSOCKET_ACCEPT_BACKOFF_DURATION
    );
}
