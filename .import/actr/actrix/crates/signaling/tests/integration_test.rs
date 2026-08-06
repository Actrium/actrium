//! Signaling service integration tests
//!
//! Tests core signaling flows to ensure WebSocket connection, registration,
//! heartbeat, and signaling relay functionality work correctly

use actr_protocol::{
    AIdCredential, ActrToSignaling, ActrType, Ping, Realm, SIGNALING_ENVELOPE_VERSION,
    SignalingEnvelope, actr_to_signaling, signaling_envelope, signaling_to_actr,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use platform::config::ActrixConfig;
use platform::realm::Realm as RealmEntity;
use platform::storage::db::set_db_path;
use prost::Message as ProstMessage;
use signaling::axum_router::create_signaling_router;
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::OnceCell;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};
use uuid::Uuid;

/// Test helper: create minimal config
#[allow(dead_code)]
fn create_test_config() -> ActrixConfig {
    toml::from_str(
        r#"
        enable = 8
        name = "test-signaling"
        env = "test"
        sqlite = ":memory:"
        actrix_shared_key = "test-key-12345678901234567890123456789012"

        [bind.ice]
        ip = "127.0.0.1"
        port = 3478
    "#,
    )
    .expect("Failed to parse test config")
}

/// Test helper: create signaling server
async fn create_test_server() -> (String, tokio::task::JoinHandle<()>) {
    // Initialize test database (ensure global database is set up)
    // Use a temp directory for the database to avoid polluting local state
    static INIT: OnceCell<()> = OnceCell::const_new();

    INIT.get_or_init(|| async {
        // nextest runs each test in its own process, so a fixed path would be
        // concurrently deleted and rewritten; suffix with the PID to isolate each
        // process's test database.
        let db_dir =
            std::env::temp_dir().join(format!("actrix_signaling_test_db_{}", std::process::id()));
        std::fs::create_dir_all(&db_dir).expect("Failed to create test database directory");
        let db_file = db_dir.join("actrix.db");
        if db_file.exists() {
            let _ = std::fs::remove_file(&db_file);
        }

        let db_dir_str = db_dir
            .to_str()
            .expect("Failed to convert DB directory path to string");

        // Try to set the global database path, ignoring already-initialized errors
        match set_db_path(Path::new(db_dir_str)).await {
            Ok(()) => {}
            Err(e) => {
                let err_msg = e.to_string();
                if !err_msg.contains("already initialized") && !err_msg.contains("Database already")
                {
                    panic!("Failed to initialize test database: {e}");
                }
            }
        }
    })
    .await;

    // Ensure the test-required realm exists — avoids 403 errors due to missing realm
    if RealmEntity::get_by_name("test_realm")
        .await
        .unwrap_or(None)
        .is_none()
    {
        let _ = RealmEntity::create("test_realm".to_string(), String::new()).await;
    }

    // Create router using create_signaling_router
    let app = create_signaling_router()
        .await
        .expect("Failed to create router");

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{addr}/ws");

    // Start server (with ConnectInfo support)
    let handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    (ws_url, handle)
}

/// Test helper: create SignalingEnvelope
fn create_envelope(flow: signaling_envelope::Flow) -> SignalingEnvelope {
    SignalingEnvelope {
        envelope_version: SIGNALING_ENVELOPE_VERSION,
        envelope_id: Uuid::new_v4().to_string(),
        reply_for: None,
        traceparent: None,
        tracestate: None,
        flow: Some(flow),
    }
}

/// Test helper: create test credential (placeholder, not used for actual verification)
fn create_test_credential() -> AIdCredential {
    AIdCredential {
        key_id: 1,
        claims: Bytes::from_static(b"test-claims-placeholder"),
        signature: Bytes::from_static(&[0u8; 64]),
    }
}

/// Test helper: create test ActrId (for relay)
fn create_test_actr_id(serial: u64) -> actr_protocol::ActrId {
    actr_protocol::ActrId {
        serial_number: serial,
        realm: Realm { realm_id: 1001 },
        r#type: ActrType {
            manufacturer: "test".to_string(),
            name: "device".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

/// Test helper: send protobuf message
async fn send_protobuf(
    ws: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        TungsteniteMessage,
    >,
    envelope: &SignalingEnvelope,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = Vec::new();
    envelope.encode(&mut buf)?;
    ws.send(TungsteniteMessage::Binary(Bytes::from(buf)))
        .await?;
    Ok(())
}

/// Test helper: receive protobuf message
async fn receive_protobuf(
    ws: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> Result<SignalingEnvelope, Box<dyn std::error::Error>> {
    let msg = timeout(Duration::from_secs(5), ws.next())
        .await?
        .ok_or("Connection closed")??;

    match msg {
        TungsteniteMessage::Binary(data) => {
            let envelope = SignalingEnvelope::decode(&data[..])?;
            Ok(envelope)
        }
        _ => Err("Expected binary message".into()),
    }
}

#[tokio::test]
async fn test_websocket_connection() {
    // Create server
    let (ws_url, _handle) = create_test_server().await;

    // Connect WebSocket
    let result = connect_async(&ws_url).await;
    assert!(result.is_ok(), "WebSocket connection failed");

    let (ws_stream, _) = result.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send Ping to test connection (using placeholder credential)
    let ping_msg = ActrToSignaling {
        source: create_test_actr_id(999),
        credential: create_test_credential(),
        payload: Some(actr_to_signaling::Payload::Ping(Ping {
            availability: 100,
            mailbox_backlog: 0.0,
            power_reserve: 80.0,
            // Other fields use default values
            ..Default::default()
        })),
    };
    let ping = create_envelope(signaling_envelope::Flow::ActrToServer(ping_msg));

    send_protobuf(&mut write, &ping).await.unwrap();

    // Receive response (expect error due to invalid credential)
    let response = receive_protobuf(&mut read).await;
    assert!(response.is_ok(), "Failed to receive response");

    let envelope = response.unwrap();
    assert!(
        matches!(
            envelope.flow,
            Some(signaling_envelope::Flow::ServerToActr(_))
        ),
        "Expected ServerToActr flow"
    );

    // Verify error response received (realm or credential validation failed)
    if let Some(signaling_envelope::Flow::ServerToActr(signaling_msg)) = envelope.flow {
        match signaling_msg.payload {
            Some(signaling_to_actr::Payload::Error(ref err)) => {
                assert!(
                    err.code == 401 || err.code == 403,
                    "Expected 401 or 403 error code, got {}",
                    err.code
                );
                println!(
                    "✅ WebSocket connection test passed: received expected error (code={})",
                    err.code
                );
            }
            other => {
                panic!("Expected Error response, got: {other:?}");
            }
        }
    }
}

#[tokio::test]
async fn test_credential_validation() {
    let (ws_url, _handle) = create_test_server().await;
    let (ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send multiple Pings with invalid credentials
    for i in 0..3 {
        let ping_msg = ActrToSignaling {
            source: create_test_actr_id(999),
            credential: create_test_credential(),
            payload: Some(actr_to_signaling::Payload::Ping(Ping {
                availability: 100,
                mailbox_backlog: 0.0,
                power_reserve: 80.0,
                ..Default::default()
            })),
        };
        let ping = create_envelope(signaling_envelope::Flow::ActrToServer(ping_msg));

        send_protobuf(&mut write, &ping).await.unwrap();

        // Receive error response
        let response = receive_protobuf(&mut read).await.unwrap();

        if let Some(signaling_envelope::Flow::ServerToActr(signaling_msg)) = response.flow {
            // Verify error returned (realm or credential validation failed)
            assert!(
                matches!(
                    signaling_msg.payload,
                    Some(signaling_to_actr::Payload::Error(_))
                ),
                "Expected Error for ping {i} due to invalid realm or credential"
            );
        } else {
            panic!("Expected ServerToActr flow for ping {i}");
        }
    }

    println!("✅ Credential validation test passed");
}

#[tokio::test]
async fn test_invalid_message_handling() {
    let (ws_url, _handle) = create_test_server().await;
    let (ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send invalid protobuf data
    write
        .send(TungsteniteMessage::Binary(Bytes::from_static(&[
            0xFF, 0xFF, 0xFF,
        ])))
        .await
        .unwrap();

    // Server should close connection or send error
    let response = timeout(Duration::from_secs(2), read.next()).await;

    // Could be connection close or error response
    match response {
        Ok(Some(Ok(TungsteniteMessage::Close(_)))) => {
            println!("✅ Server closed connection on invalid message");
        }
        Ok(Some(Ok(TungsteniteMessage::Binary(data)))) => {
            // Try to parse error response
            if let Ok(envelope) = SignalingEnvelope::decode(&data[..]) {
                println!("✅ Server sent error response: {envelope:?}");
            }
        }
        Ok(None) => {
            println!("✅ Connection closed after invalid message");
        }
        Err(_) => {
            println!("⚠️  Timeout waiting for response (acceptable)");
        }
        _ => {
            println!("⚠️  Unexpected response type");
        }
    }
}

/// An oversized signaling frame is rejected at the WebSocket transport
/// boundary (`max_frame_size`/`max_message_size` on the server's
/// `WebSocketUpgrade`) so the server never allocates/queues it (issue #351).
/// The connection is torn down; the post-read check in `handle_websocket`
/// remains as defense in depth.
#[tokio::test]
async fn test_oversized_inbound_frame_rejected_at_transport_boundary() {
    let (ws_url, _handle) = create_test_server().await;
    let (ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send an oversized ~512 KiB Binary frame (above the 256 KiB transport cap).
    // The send itself may succeed (the client buffers the frame); the server
    // rejects it at the transport boundary and tears down the connection.
    let oversized = vec![0xABu8; 512 * 1024];
    let _ = write
        .send(TungsteniteMessage::Binary(oversized.into()))
        .await;

    // The server must reject the oversized frame and tear down the connection.
    // Acceptable: a Close frame (ideally 1009 Message Too Big), a transport
    // error (e.g. connection reset), or end-of-stream. A normal Binary
    // response would mean the oversized frame was accepted — a regression.
    let response = timeout(Duration::from_secs(5), read.next())
        .await
        .expect("server should react to the oversized frame within 5s");

    match response {
        Some(Ok(TungsteniteMessage::Close(_))) => {}
        Some(Err(_)) | None => {}
        Some(Ok(other)) => panic!(
            "oversized frame should have been rejected at the transport boundary, \
             but the connection stayed open and yielded {other:?}"
        ),
    }
}
