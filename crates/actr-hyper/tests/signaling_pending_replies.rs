//! Regression tests for signaling reply waiters during connection rebuilds.

use actr_hyper::test_support::{dummy_credential, make_actor_id};
use actr_hyper::wire::webrtc::{
    DisconnectReason, InboundQueueConfig, ReconnectConfig, SignalingClient, SignalingConfig,
    SignalingEvent, WebSocketSignalingClient,
};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    ActrType, GetSigningKeyResponse, RouteCandidatesRequest, RouteCandidatesResponse,
    SIGNALING_ENVELOPE_VERSION, SignalingEnvelope, SignalingToActr, actr_to_signaling,
    route_candidates_request, route_candidates_response, signaling_envelope, signaling_to_actr,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

type SilentServer = (String, oneshot::Receiver<()>, tokio::task::JoinHandle<()>);

async fn start_silent_signaling_server() -> SilentServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind silent signaling server");
    let url = format!("ws://{}", listener.local_addr().expect("local addr"));
    let (first_message_tx, first_message_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept websocket");
        let mut ws = accept_async(stream)
            .await
            .expect("accept websocket handshake");
        let mut first_message_tx = Some(first_message_tx);

        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Binary(_)) => {
                    if let Some(tx) = first_message_tx.take() {
                        let _ = tx.send(());
                    }
                    // Intentionally do not reply. The test simulates a request
                    // that is still pending when the client rebuilds signaling.
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    });

    (url, first_message_rx, handle)
}

fn test_signaling_config(url: &str) -> SignalingConfig {
    SignalingConfig {
        server_url: Url::parse(url).expect("parse signaling url"),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig {
            enabled: false,
            ..ReconnectConfig::default()
        },
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }
}

fn route_candidates_request() -> RouteCandidatesRequest {
    RouteCandidatesRequest {
        target_type: ActrType {
            manufacturer: "acme".to_string(),
            name: "StreamEchoServer".to_string(),
            version: "1.0.0".to_string(),
        },
        criteria: Some(route_candidates_request::NodeSelectionCriteria {
            candidate_count: 1,
            ranking_factors: Vec::new(),
            minimal_dependency_requirement: None,
            minimal_health_requirement: None,
        }),
        client_location: None,
        client_fingerprint: "service_semantic:test".to_string(),
    }
}

async fn receive_binary_envelope(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> SignalingEnvelope {
    while let Some(message) = websocket.next().await {
        match message.expect("test websocket should remain valid") {
            Message::Binary(data) => {
                return SignalingEnvelope::decode(data.as_ref())
                    .expect("test server should receive a valid signaling envelope");
            }
            Message::Ping(payload) => {
                websocket
                    .send(Message::Pong(payload))
                    .await
                    .expect("test websocket should answer ping");
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    panic!("test websocket closed before receiving a signaling envelope");
}

fn route_candidates_success_response(reply_for: String) -> SignalingEnvelope {
    SignalingEnvelope {
        envelope_version: SIGNALING_ENVELOPE_VERSION,
        envelope_id: "route-candidates-retry-response".to_string(),
        reply_for: Some(reply_for),
        traceparent: None,
        tracestate: None,
        flow: Some(signaling_envelope::Flow::ServerToActr(SignalingToActr {
            target: make_actor_id(43),
            payload: Some(signaling_to_actr::Payload::RouteCandidatesResponse(
                RouteCandidatesResponse {
                    result: Some(route_candidates_response::Result::Success(
                        route_candidates_response::RouteCandidatesOk {
                            candidates: vec![make_actor_id(99)],
                            ws_address_map: Vec::new(),
                        },
                    )),
                },
            )),
        })),
    }
}

fn signing_key_success_response(reply_for: String) -> SignalingEnvelope {
    SignalingEnvelope {
        envelope_version: SIGNALING_ENVELOPE_VERSION,
        envelope_id: "signing-key-retry-response".to_string(),
        reply_for: Some(reply_for),
        traceparent: None,
        tracestate: None,
        flow: Some(signaling_envelope::Flow::ServerToActr(SignalingToActr {
            target: make_actor_id(43),
            payload: Some(signaling_to_actr::Payload::GetSigningKeyResponse(
                GetSigningKeyResponse {
                    key_id: 7,
                    pubkey: vec![0xAB; 32].into(),
                },
            )),
        })),
    }
}

/// A spontaneous stream end (remote network loss, no local lifecycle
/// command) must fail an in-flight request promptly with the retryable
/// typed error — not park it until the five-second response timeout — and
/// the automatic retry must ride the replacement connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_request_fails_promptly_on_stream_end_and_retries_after_reconnect() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stream-end signaling server");
    let server_url = format!(
        "ws://{}",
        listener.local_addr().expect("stream-end server address")
    );
    let (first_message_tx, first_message_rx) = oneshot::channel();
    let (second_message_tx, second_message_rx) = oneshot::channel();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("server should accept first connection");
        let mut first_websocket = accept_async(first_stream)
            .await
            .expect("server should complete first websocket handshake");
        let first_envelope = receive_binary_envelope(&mut first_websocket).await;
        first_message_tx
            .send(first_envelope)
            .expect("test should receive first route request");

        // Abruptly drop the socket with no WebSocket close handshake: the
        // client's receiver task observes a spontaneous stream end.
        drop(first_websocket);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("server should accept replacement connection");
        let mut second_websocket = accept_async(second_stream)
            .await
            .expect("server should complete replacement websocket handshake");
        let second_envelope = receive_binary_envelope(&mut second_websocket).await;
        let response = route_candidates_success_response(second_envelope.envelope_id.clone());
        second_message_tx
            .send(second_envelope)
            .expect("test should receive retried route request");
        second_websocket
            .send(Message::Binary(response.encode_to_vec().into()))
            .await
            .expect("server should send the retried route response");
    });

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");
    // Subscribe before the server can close the first socket. The signaling
    // event stream is a non-replaying broadcast channel; subscribing after
    // the first request lets a fast receiver task publish `Disconnected`
    // before this test starts waiting for it.
    let mut events = client.subscribe_events();

    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .send_route_candidates_request(
                    make_actor_id(43),
                    dummy_credential(),
                    route_candidates_request(),
                )
                .await
        })
    };

    let first = tokio::time::timeout(Duration::from_secs(2), first_message_rx)
        .await
        .expect("server should receive the first route request")
        .expect("first route request channel should remain open");

    // The server has already dropped its socket. Wait until the client
    // observes the stream end before reconnecting, so connect() cannot be
    // mistaken for a no-op on the stale connected flag.
    let dropped_at = Instant::now();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("client should publish its disconnect transition")
        {
            Ok(SignalingEvent::Disconnected { .. }) => break,
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    client
        .connect()
        .await
        .expect("replacement signaling connection should succeed");

    let second = tokio::time::timeout(Duration::from_secs(2), second_message_rx)
        .await
        .expect("server should receive the retried route request")
        .expect("retried route request channel should remain open");
    assert_ne!(first.envelope_id, second.envelope_id);

    let response = tokio::time::timeout(Duration::from_secs(2), request_task)
        .await
        .expect("retried route request should complete")
        .expect("route request task should not panic")
        .expect("route request should succeed after spontaneous stream end");
    match response.result {
        Some(route_candidates_response::Result::Success(success)) => {
            assert_eq!(success.candidates, vec![make_actor_id(99)]);
        }
        other => panic!("expected successful retried route response, got {other:?}"),
    }

    // With a working Disconnected race the whole loss-to-retry round trip is
    // milliseconds; a value approaching the five-second response timeout
    // means the waiter parked on the dead socket instead.
    assert!(
        dropped_at.elapsed() < Duration::from_secs(4),
        "stream-end failure should retire the first attempt immediately, took {:?}",
        dropped_at.elapsed()
    );

    client.disconnect().await.expect("disconnect test client");
    server_task
        .await
        .expect("stream-end server should not panic");
}

/// When the replacement connection never arrives, the single retry must give
/// up at the bounded reconnect wait instead of hanging the caller.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_request_retry_expires_without_reconnect() {
    let (server_url, first_message_rx, server_task) = start_silent_signaling_server().await;

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");

    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .send_route_candidates_request(
                    make_actor_id(43),
                    dummy_credential(),
                    route_candidates_request(),
                )
                .await
        })
    };

    first_message_rx
        .await
        .expect("server should receive the route request");

    client
        .disconnect_for_recovery()
        .await
        .expect("recovery disconnect should drop the waiter and arm the retry");

    let started = Instant::now();
    let error = tokio::time::timeout(Duration::from_secs(10), request_task)
        .await
        .expect("retry wait should be bounded")
        .expect("route request task should not panic")
        .expect_err("retry without a replacement connection must expire");
    assert!(
        error
            .to_string()
            .contains("Timed out waiting for signaling reconnect"),
        "unexpected error: {error}"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(5),
        "bounded reconnect wait should span the full budget, took {:?}",
        started.elapsed()
    );

    server_task.abort();
}

/// GetSigningKey is a read-only idempotent query and must share the single
/// automatic replay of dropped waiters.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_signing_key_retries_after_recovery_reconnect() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind signing-key signaling server");
    let server_url = format!(
        "ws://{}",
        listener.local_addr().expect("signing-key server address")
    );
    let (first_message_tx, first_message_rx) = oneshot::channel();
    let (release_first_tx, release_first_rx) = oneshot::channel();
    let (second_message_tx, second_message_rx) = oneshot::channel();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("server should accept first connection");
        let mut first_websocket = accept_async(first_stream)
            .await
            .expect("server should complete first websocket handshake");
        let first_envelope = receive_binary_envelope(&mut first_websocket).await;
        first_message_tx
            .send(first_envelope)
            .expect("test should receive first signing-key request");

        release_first_rx
            .await
            .expect("test should release the first connection");
        drop(first_websocket);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("server should accept replacement connection");
        let mut second_websocket = accept_async(second_stream)
            .await
            .expect("server should complete replacement websocket handshake");
        let second_envelope = receive_binary_envelope(&mut second_websocket).await;
        let response = signing_key_success_response(second_envelope.envelope_id.clone());
        second_message_tx
            .send(second_envelope)
            .expect("test should receive retried signing-key request");
        second_websocket
            .send(Message::Binary(response.encode_to_vec().into()))
            .await
            .expect("server should send the retried signing-key response");
    });

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");

    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .get_signing_key(make_actor_id(43), dummy_credential(), 7)
                .await
        })
    };

    let first = tokio::time::timeout(Duration::from_secs(2), first_message_rx)
        .await
        .expect("server should receive the first signing-key request")
        .expect("first signing-key channel should remain open");
    match first.flow {
        Some(signaling_envelope::Flow::ActrToServer(request_flow)) => {
            assert!(matches!(
                request_flow.payload,
                Some(actr_to_signaling::Payload::GetSigningKeyRequest(_))
            ));
        }
        other => panic!("expected ActrToServer flow, got {other:?}"),
    }

    client
        .disconnect_for_recovery()
        .await
        .expect("recovery disconnect should drop the signing-key waiter");
    release_first_tx
        .send(())
        .expect("server should release the first websocket");
    client
        .connect()
        .await
        .expect("replacement signaling connection should succeed");

    let second = tokio::time::timeout(Duration::from_secs(2), second_message_rx)
        .await
        .expect("server should receive the retried signing-key request")
        .expect("retried signing-key channel should remain open");
    assert_ne!(first.envelope_id, second.envelope_id);

    let (key_id, pubkey) = tokio::time::timeout(Duration::from_secs(2), request_task)
        .await
        .expect("retried signing-key request should complete")
        .expect("signing-key task should not panic")
        .expect("signing key should be fetched after signaling reconnect");
    assert_eq!(key_id, 7);
    assert_eq!(pubkey, vec![0xAB; 32]);

    client.disconnect().await.expect("disconnect test client");
    server_task
        .await
        .expect("signing-key server should not panic");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_route_candidates_request_retries_after_recovery_reconnect() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind reconnecting signaling server");
    let server_url = format!(
        "ws://{}",
        listener.local_addr().expect("reconnecting server address")
    );
    let (first_message_tx, first_message_rx) = oneshot::channel();
    let (release_first_tx, release_first_rx) = oneshot::channel();
    let (second_message_tx, second_message_rx) = oneshot::channel();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("server should accept first connection");
        let mut first_websocket = accept_async(first_stream)
            .await
            .expect("server should complete first websocket handshake");
        let first_envelope = receive_binary_envelope(&mut first_websocket).await;
        first_message_tx
            .send(first_envelope)
            .expect("test should receive first route request");

        release_first_rx
            .await
            .expect("test should release the first connection");
        drop(first_websocket);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("server should accept replacement connection");
        let mut second_websocket = accept_async(second_stream)
            .await
            .expect("server should complete replacement websocket handshake");
        let second_envelope = receive_binary_envelope(&mut second_websocket).await;
        let response = route_candidates_success_response(second_envelope.envelope_id.clone());
        second_message_tx
            .send(second_envelope)
            .expect("test should receive retried route request");
        second_websocket
            .send(Message::Binary(response.encode_to_vec().into()))
            .await
            .expect("server should send the retried route response");
    });

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");

    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .send_route_candidates_request(
                    make_actor_id(43),
                    dummy_credential(),
                    route_candidates_request(),
                )
                .await
        })
    };

    let first = tokio::time::timeout(Duration::from_secs(2), first_message_rx)
        .await
        .expect("server should receive the first route request")
        .expect("first route request channel should remain open");

    client
        .disconnect_for_recovery()
        .await
        .expect("recovery disconnect should drop the old signaling generation");
    release_first_tx
        .send(())
        .expect("server should release the first websocket");
    client
        .connect()
        .await
        .expect("replacement signaling connection should succeed");

    let second = tokio::time::timeout(Duration::from_secs(2), second_message_rx)
        .await
        .expect("server should receive the retried route request")
        .expect("retried route request channel should remain open");
    assert_eq!(first.flow, second.flow);
    assert_ne!(first.envelope_id, second.envelope_id);

    let response = tokio::time::timeout(Duration::from_secs(2), request_task)
        .await
        .expect("retried route request should complete")
        .expect("route request task should not panic")
        .expect("route request should succeed after signaling reconnect");
    match response.result {
        Some(route_candidates_response::Result::Success(success)) => {
            assert_eq!(success.candidates, vec![make_actor_id(99)]);
        }
        other => panic!("expected successful retried route response, got {other:?}"),
    }

    client.disconnect().await.expect("disconnect test client");
    server_task
        .await
        .expect("reconnecting server should not panic");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_route_candidates_request_retries_at_most_once() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind retry-limit signaling server");
    let server_url = format!(
        "ws://{}",
        listener.local_addr().expect("retry-limit server address")
    );
    let (first_message_tx, first_message_rx) = oneshot::channel();
    let (release_first_tx, release_first_rx) = oneshot::channel();
    let (second_message_tx, second_message_rx) = oneshot::channel();
    let (release_second_tx, release_second_rx) = oneshot::channel();

    let server_task = tokio::spawn(async move {
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("server should accept first connection");
        let mut first_websocket = accept_async(first_stream)
            .await
            .expect("server should complete first websocket handshake");
        first_message_tx
            .send(receive_binary_envelope(&mut first_websocket).await)
            .expect("test should receive first route request");
        release_first_rx
            .await
            .expect("test should release the first websocket");
        drop(first_websocket);

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("server should accept replacement connection");
        let mut second_websocket = accept_async(second_stream)
            .await
            .expect("server should complete replacement websocket handshake");
        second_message_tx
            .send(receive_binary_envelope(&mut second_websocket).await)
            .expect("test should receive the one retry");
        release_second_rx
            .await
            .expect("test should release the replacement websocket");
        drop(second_websocket);
    });

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");
    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .send_route_candidates_request(
                    make_actor_id(43),
                    dummy_credential(),
                    route_candidates_request(),
                )
                .await
        })
    };

    let _first = tokio::time::timeout(Duration::from_secs(2), first_message_rx)
        .await
        .expect("server should receive the first route request")
        .expect("first route request channel should remain open");
    client
        .disconnect_for_recovery()
        .await
        .expect("recovery disconnect should drop the first waiter");
    release_first_tx
        .send(())
        .expect("server should release the first websocket");
    client
        .connect()
        .await
        .expect("replacement signaling connection should succeed");

    let _second = tokio::time::timeout(Duration::from_secs(2), second_message_rx)
        .await
        .expect("server should receive exactly one retry")
        .expect("retry request channel should remain open");
    client
        .disconnect_for_recovery()
        .await
        .expect("second recovery disconnect should drop the retry waiter");
    release_second_tx
        .send(())
        .expect("server should release the replacement websocket");

    let error = tokio::time::timeout(Duration::from_secs(2), request_task)
        .await
        .expect("route request should not wait for a second retry")
        .expect("route request task should not panic")
        .expect_err("second dropped waiter should terminate the one-retry policy");
    assert_eq!(
        error.to_string(),
        "Receiver dropped while waiting for signaling response"
    );

    client.disconnect().await.expect("disconnect test client");
    server_task
        .await
        .expect("retry-limit server should not panic");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_route_candidates_waiter_uses_short_response_timeout() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    let (server_url, first_message_rx, server_task) = start_silent_signaling_server().await;
    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");

    let started = Instant::now();
    let request_task = {
        let client = client.clone();
        let actor_id = make_actor_id(43);
        let credential = dummy_credential();
        let request = route_candidates_request();

        tokio::spawn(async move {
            client
                .send_route_candidates_request(actor_id, credential, request)
                .await
        })
    };

    first_message_rx
        .await
        .expect("server should receive route candidates request");

    let result = tokio::time::timeout(Duration::from_secs(7), request_task)
        .await
        .expect("pending route candidates request should use the short response timeout")
        .expect("request task should not panic");
    let elapsed = started.elapsed();

    let err = result.expect_err("request should fail when signaling server stays silent");
    assert!(
        err.to_string()
            .contains("Timed out waiting for signaling response"),
        "unexpected error: {err}"
    );
    assert!(
        elapsed >= Duration::from_secs(4),
        "request should wait for its response budget; elapsed={elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(7),
        "request/response timeout must not use the old 15s window; elapsed={elapsed:?}"
    );

    let _ = client.disconnect().await;
    server_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_route_candidates_waiter_fails_promptly_on_disconnect() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    let (server_url, first_message_rx, server_task) = start_silent_signaling_server().await;
    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");

    let request_task = {
        let client = client.clone();
        let actor_id = make_actor_id(42);
        let credential = dummy_credential();
        let request = route_candidates_request();

        tokio::spawn(async move {
            client
                .send_route_candidates_request(actor_id, credential, request)
                .await
        })
    };

    first_message_rx
        .await
        .expect("server should receive route candidates request");

    client
        .disconnect()
        .await
        .expect("disconnect should be best effort");

    let result = tokio::time::timeout(Duration::from_millis(500), request_task)
        .await
        .expect("pending route candidates request should fail promptly after disconnect")
        .expect("request task should not panic");

    let err = result.expect_err("request should fail after disconnect drops pending waiter");
    let message = err.to_string();
    // Disconnecting with a request in flight has two legal outcomes, decided
    // by whether the socket is invalidated before or after the send completes:
    // either the pending waiter is dropped, or the send itself observes the
    // dead socket. Both satisfy what this test is actually asserting — that
    // the caller fails promptly instead of hanging until the response timeout
    // — so accept either rather than pinning one interleaving, which the test
    // did before and which made it fail under CI scheduling pressure.
    assert!(
        message.contains("Receiver dropped while waiting for signaling response")
            || message.contains("Signaling send completed after the WebSocket was invalidated"),
        "unexpected error: {err}"
    );

    server_task.abort();
}

/// A server that completes the handshake but never reads the socket again
/// must trip the client's pong watchdog — an automatic loss with no local
/// lifecycle command and no stream end. The in-flight RouteCandidates waiter
/// is dropped promptly with the retryable typed error and replays once on
/// the replacement connection.
///
/// The watchdog tolerates one full ping interval of scheduling slack, so this
/// test waits out the real silence window (ping ticks at 5s, pong timeout at
/// 10s: the loss publishes on the third tick, ~15s in).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_request_replays_after_pong_watchdog_loss() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind watchdog signaling server");
    let server_url = format!(
        "ws://{}",
        listener.local_addr().expect("watchdog server address")
    );
    let (second_message_tx, second_message_rx) = oneshot::channel();

    let server_task = tokio::spawn(async move {
        // First connection: complete the handshake, then never read again.
        // The route request frame is simply left in the kernel buffer, and
        // tokio-tungstenite only answers pings while reading, so the client
        // observes total silence: the pong watchdog is the path that fires,
        // not the stream-end receiver.
        let (first_stream, _) = listener
            .accept()
            .await
            .expect("server should accept first connection");
        let first_websocket = accept_async(first_stream)
            .await
            .expect("server should complete first websocket handshake");
        tokio::spawn(async move {
            let _held = first_websocket;
            std::future::pending::<()>().await;
        });

        let (second_stream, _) = listener
            .accept()
            .await
            .expect("server should accept replacement connection");
        let mut second_websocket = accept_async(second_stream)
            .await
            .expect("server should complete replacement websocket handshake");
        let second_envelope = receive_binary_envelope(&mut second_websocket).await;
        let response = route_candidates_success_response(second_envelope.envelope_id.clone());
        second_message_tx
            .send(second_envelope)
            .expect("test should receive retried route request");
        second_websocket
            .send(Message::Binary(response.encode_to_vec().into()))
            .await
            .expect("server should send retried route response");
    });

    let client = Arc::new(WebSocketSignalingClient::new(test_signaling_config(
        &server_url,
    )));
    client.connect().await.expect("connect signaling client");
    // Arm the non-replaying event receiver before the long silence window so
    // a delayed watchdog event cannot be published before this test subscribes.
    let mut events = client.subscribe_events();
    let silence_started = Instant::now();

    // Send the request late in the silence window: the waiter's own
    // five-second response budget must still be running when the watchdog
    // publishes the loss (~15s in), otherwise the attempt ends in a plain
    // response timeout and the replay path never engages. The server never
    // reads this frame; its arrival is implied by the retried envelope.
    tokio::time::sleep(Duration::from_secs(13)).await;
    let request_task = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .send_route_candidates_request(
                    make_actor_id(43),
                    dummy_credential(),
                    route_candidates_request(),
                )
                .await
        })
    };

    let reason = loop {
        match tokio::time::timeout(Duration::from_secs(25), events.recv())
            .await
            .expect("watchdog should publish its disconnect transition")
        {
            Ok(SignalingEvent::Disconnected { reason }) => break reason,
            Ok(_) => continue,
            Err(_) => continue,
        }
    };
    assert!(
        matches!(reason, DisconnectReason::PongTimeout),
        "the silent server must trip the pong watchdog, got {reason:?}"
    );
    let silent_for = silence_started.elapsed();
    assert!(
        silent_for >= Duration::from_secs(10),
        "the watchdog must wait out the pong-timeout silence window, fired after {silent_for:?}"
    );
    assert!(
        silent_for < Duration::from_secs(24),
        "the watchdog fired too late: {silent_for:?}"
    );

    client
        .connect()
        .await
        .expect("replacement signaling connection should succeed");

    tokio::time::timeout(Duration::from_secs(2), second_message_rx)
        .await
        .expect("server should receive the retried route request")
        .expect("retried route request channel should remain open");

    let response = tokio::time::timeout(Duration::from_secs(2), request_task)
        .await
        .expect("retried route request should complete")
        .expect("route request task should not panic")
        .expect("route request should succeed after the watchdog loss");
    match response.result {
        Some(route_candidates_response::Result::Success(success)) => {
            assert_eq!(success.candidates, vec![make_actor_id(99)]);
        }
        other => panic!("expected successful retried route response, got {other:?}"),
    }

    client.disconnect().await.expect("disconnect test client");
    server_task.await.expect("watchdog server should not panic");
}
