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
    let (server, _incoming) = WebSocketServer::bind(0, DirectSecurity::insecure_loopback())
        .await
        .expect("bind an ephemeral listener");

    assert_eq!(
        server.accept_backoff,
        crate::timer::ids::WEBSOCKET_ACCEPT_BACKOFF_DURATION
    );
}

#[tokio::test]
async fn default_policy_cannot_bind_plaintext() {
    assert!(
        WebSocketServer::bind(0, DirectSecurity::default())
            .await
            .is_err()
    );
    let (server, _) = WebSocketServer::bind(0, DirectSecurity::insecure_loopback())
        .await
        .unwrap();
    assert!(server.local_addr.ip().is_loopback());
}

#[tokio::test]
async fn tls_listener_rejects_plaintext_and_clients_reject_untrusted_certificates() {
    use actr_protocol::PayloadType;
    let options = super::super::security::test_options();
    let security = DirectSecurity::from_options(&options, None).unwrap();
    let (server, mut incoming) = WebSocketServer::bind(0, security.clone()).await.unwrap();
    let addr = server.local_addr;
    let shutdown = CancellationToken::new();
    server.start(shutdown.clone());
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        assert!(
            tokio_tungstenite::connect_async(format!("ws://{addr}"))
                .await
                .is_err()
        );
        // A client with the default public-roots policy must reject the
        // private test CA.
        let public_roots = DirectSecurity::from_options(&Default::default(), None).unwrap();
        let untrusted =
            WebSocketConnection::new(format!("wss://{addr}")).with_security(public_roots);
        assert!(untrusted.connect().await.is_err());
        use base64::Engine as _;
        let credential = AIdCredential::default();
        let encoded = base64::engine::general_purpose::STANDARD.encode(credential.encode_to_vec());
        let client = WebSocketConnection::new(format!("wss://{addr}"))
            .with_security(security)
            .with_local_id("0102".into())
            .with_credential_b64(encoded);
        client.connect().await.unwrap();
        let (peer, source, received_credential) = incoming.recv().await.unwrap();
        assert_eq!(source, vec![1, 2]);
        assert_eq!(received_credential, Some(credential));
        for payload in [PayloadType::RpcReliable, PayloadType::StreamReliable] {
            let receive = peer.get_lane(payload).await.unwrap();
            let send = client.get_lane(payload).await.unwrap();
            send.send(bytes::Bytes::from_static(b"tls payload"))
                .await
                .unwrap();
            assert_eq!(receive.recv().await.unwrap().as_ref(), b"tls payload");
        }
        client.close().await.unwrap();
        peer.close().await.unwrap();
    })
    .await;
    shutdown.cancel();
    outcome.unwrap();
}

#[tokio::test]
async fn trusted_ca_does_not_bypass_endpoint_name_validation() {
    // The endpoint-mismatch fixture is signed by its own test CA and covers
    // only the `localhost` DNS name, so dialing the loopback IP must fail
    // endpoint-name validation even though the chain is trusted. Binding the
    // default loopback keeps this portable (127.0.0.2 is not assigned on
    // every platform's loopback).
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/direct-websocket");
    let options = actr_config::DirectWebSocketSecurity {
        tls_cert: Some(dir.join("endpoint-mismatch-cert.pem")),
        tls_key: Some(dir.join("endpoint-mismatch-key.pem")),
        ca_cert: Some(dir.join("endpoint-mismatch-ca.pem")),
        ..Default::default()
    };
    let security = DirectSecurity::from_options(&options, None).unwrap();
    let (server, _) = WebSocketServer::bind(0, security.clone()).await.unwrap();
    let client =
        WebSocketConnection::new(format!("wss://{}", server.local_addr)).with_security(security);
    let shutdown = CancellationToken::new();
    server.start(shutdown.clone());
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect()).await;
    shutdown.cancel();
    assert!(result.unwrap().is_err());
}
