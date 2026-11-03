//! Real RFC 6455 clients exercise authentication and routing boundaries.
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use loran_relay::{AuthError, Authenticator, Principal, Relay, RelayLimits};
use loran_relay_protocol::{
    ClientControl, CloseReason, ErrorCode, SUBPROTOCOL, ServerControl, decode_data, encode_data,
};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct TestAuth;
#[async_trait]
impl Authenticator for TestAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        let (namespace, name) = token.split_once(':').ok_or(AuthError)?;
        if name != endpoint || namespace == "invalid" {
            return Err(AuthError);
        }
        Ok(Principal {
            namespace: namespace.to_owned(),
            endpoint: endpoint.to_owned(),
            metadata: "public proof".into(),
            authorization_context: String::new(),
            expires_at: match namespace {
                "expired" => Some(SystemTime::now() - Duration::from_secs(1)),
                "short" => Some(SystemTime::now() + Duration::from_millis(300)),
                _ => None,
            },
        })
    }
}

struct Server {
    url: String,
    relay: Arc<Relay>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}
impl Server {
    async fn new(limits: RelayLimits) -> Self {
        Self::with_auth(limits, Arc::new(TestAuth)).await
    }

    async fn with_auth(limits: RelayLimits, auth: Arc<dyn Authenticator>) -> Self {
        let cancel = CancellationToken::new();
        let relay = Relay::new(limits, auth, cancel.clone()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        let router = relay.router();
        let shutdown = cancel.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            url,
            relay,
            cancel,
            task,
        }
    }
    async fn socket(&self) -> Socket {
        let mut request = self.url.as_str().into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
        let (socket, response) = connect_async(request).await.unwrap();
        assert_eq!(response.headers()["sec-websocket-protocol"], SUBPROTOCOL);
        socket
    }
    async fn login(&self, endpoint: &str, namespace: &str) -> Socket {
        let mut socket = self.socket().await;
        control(
            &mut socket,
            ClientControl::Hello {
                endpoint: endpoint.into(),
                token: format!("{namespace}:{endpoint}"),
            },
        )
        .await;
        assert!(matches!(
            read(&mut socket).await,
            ServerControl::Welcome { .. }
        ));
        socket
    }
}

struct RevocableAuth {
    revoked: std::sync::RwLock<HashSet<(String, String)>>,
}

struct DelayedAuth {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[async_trait]
impl Authenticator for DelayedAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        TestAuth.authenticate(endpoint, token).await
    }

    async fn authorize_pair(&self, _source: &Principal, _target: &Principal) -> bool {
        self.started.notify_one();
        self.release.notified().await;
        true
    }
}

#[async_trait]
impl Authenticator for RevocableAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        TestAuth.authenticate(endpoint, token).await
    }

    fn active(&self, principal: &Principal) -> bool {
        !self
            .revoked
            .read()
            .unwrap()
            .contains(&(principal.namespace.clone(), principal.endpoint.clone()))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

async fn control(socket: &mut Socket, message: ClientControl) {
    socket
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}
async fn message(socket: &mut Socket) -> Message {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
                other => return other,
            }
        }
    })
    .await
    .expect("relay response deadline")
}
async fn read(socket: &mut Socket) -> ServerControl {
    let Message::Text(text) = message(socket).await else {
        panic!("expected control");
    };
    serde_json::from_str(&text).unwrap()
}
async fn open(a: &mut Socket, b: &mut Socket, peer: &str) -> u64 {
    control(
        a,
        ClientControl::Connect {
            request: 10,
            peer: peer.into(),
        },
    )
    .await;
    let ServerControl::Incoming { channel, .. } = read(b).await else {
        panic!("expected incoming");
    };
    control(b, ClientControl::Accept { channel }).await;
    assert!(
        matches!(read(a).await, ServerControl::Connected { channel: id, request: Some(10), .. } if id == channel)
    );
    assert!(matches!(read(b).await, ServerControl::Connected { channel: id, .. } if id == channel));
    channel
}

#[tokio::test]
async fn pairing_never_applies_a_stale_authorization_to_a_replacement_session() {
    let auth = Arc::new(DelayedAuth {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let server = Server::with_auth(RelayLimits::default(), auth.clone()).await;
    let mut source = server.login("source", "realm").await;
    let _original = server.login("target", "realm").await;
    control(
        &mut source,
        ClientControl::Connect {
            request: 1,
            peer: "target".into(),
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(3), auth.started.notified())
        .await
        .expect("authorization did not start");
    let mut replacement = server.login("target", "realm").await;
    auth.release.notify_one();
    assert!(matches!(
        read(&mut source).await,
        ServerControl::Error {
            request: Some(1),
            code: ErrorCode::PeerUnavailable,
        }
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), replacement.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn revoking_one_principal_leaves_other_realms_connected() {
    let auth = Arc::new(RevocableAuth {
        revoked: std::sync::RwLock::new(HashSet::new()),
    });
    let server = Server::with_auth(RelayLimits::default(), auth.clone()).await;
    let mut revoked = server.login("revoked", "realm-one").await;
    let mut left = server.login("left", "realm-two").await;
    let mut right = server.login("right", "realm-two").await;
    let channel = open(&mut left, &mut right, "right").await;

    auth.revoked
        .write()
        .unwrap()
        .insert(("realm-one".into(), "revoked".into()));
    assert_eq!(server.relay.disconnect_inactive(), 1);
    assert!(matches!(message(&mut revoked).await, Message::Close(_)));

    left.send(Message::Binary(
        encode_data(channel, b"still connected").unwrap(),
    ))
    .await
    .unwrap();
    let Message::Binary(frame) = message(&mut right).await else {
        panic!("unrelated relay channel was closed");
    };
    let (actual_channel, data) =
        decode_data(frame, RelayLimits::default().max_frame_bytes).unwrap();
    assert_eq!(actual_channel, channel);
    assert_eq!(&data[..], b"still connected");
}

#[tokio::test]
async fn readiness_is_peer_acknowledged_and_data_is_bidirectional() {
    let server = Server::new(RelayLimits::default()).await;
    let mut a = server.login("a", "realm-one").await;
    let mut b = server.login("b", "realm-one").await;
    control(
        &mut a,
        ClientControl::Connect {
            request: 1,
            peer: "b".into(),
        },
    )
    .await;
    let ServerControl::Incoming {
        channel,
        peer,
        metadata,
    } = read(&mut b).await
    else {
        panic!();
    };
    assert_eq!(peer, "a");
    assert_eq!(metadata, "public proof");
    a.send(Message::Binary(encode_data(channel, b"too early").unwrap()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut a).await,
        ServerControl::Error {
            request: None,
            code: ErrorCode::NotReady
        }
    );
    control(&mut b, ClientControl::Accept { channel }).await;
    assert!(matches!(
        read(&mut a).await,
        ServerControl::Connected {
            request: Some(1),
            ..
        }
    ));
    assert!(matches!(
        read(&mut b).await,
        ServerControl::Connected { request: None, .. }
    ));
    a.send(Message::Binary(encode_data(channel, b"request").unwrap()))
        .await
        .unwrap();
    let Message::Binary(data) = message(&mut b).await else {
        panic!();
    };
    assert_eq!(
        decode_data(data, 1024).unwrap(),
        (channel, bytes::Bytes::from_static(b"request"))
    );
    b.send(Message::Binary(encode_data(channel, b"response").unwrap()))
        .await
        .unwrap();
    let Message::Binary(data) = message(&mut a).await else {
        panic!();
    };
    assert_eq!(
        decode_data(data, 1024).unwrap(),
        (channel, bytes::Bytes::from_static(b"response"))
    );
}

#[tokio::test]
async fn namespace_and_channel_membership_cannot_be_spoofed() {
    let server = Server::new(RelayLimits::default()).await;
    let mut a = server.login("a", "one").await;
    let mut b = server.login("b", "one").await;
    let mut c = server.login("c", "two").await;
    control(
        &mut c,
        ClientControl::Connect {
            request: 4,
            peer: "b".into(),
        },
    )
    .await;
    assert_eq!(
        read(&mut c).await,
        ServerControl::Error {
            request: Some(4),
            code: ErrorCode::PeerUnavailable
        }
    );
    let channel = open(&mut a, &mut b, "b").await;
    c.send(Message::Binary(encode_data(channel, b"spoof").unwrap()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut c).await,
        ServerControl::Error {
            request: None,
            code: ErrorCode::InvalidChannel
        }
    );
    control(&mut c, ClientControl::Close { channel }).await;
    assert_eq!(
        read(&mut c).await,
        ServerControl::Error {
            request: None,
            code: ErrorCode::InvalidChannel
        }
    );
    a.send(Message::Binary(
        encode_data(channel, b"still alive").unwrap(),
    ))
    .await
    .unwrap();
    assert!(matches!(message(&mut b).await, Message::Binary(_)));
}

#[tokio::test]
async fn simultaneous_connects_share_one_channel_and_both_wait_for_accept() {
    let server = Server::new(RelayLimits::default()).await;
    let mut a = server.login("a", "one").await;
    let mut b = server.login("b", "one").await;
    control(
        &mut a,
        ClientControl::Connect {
            request: 11,
            peer: "b".into(),
        },
    )
    .await;
    let ServerControl::Incoming { channel, .. } = read(&mut b).await else {
        panic!();
    };
    control(
        &mut b,
        ClientControl::Connect {
            request: 22,
            peer: "a".into(),
        },
    )
    .await;
    control(&mut b, ClientControl::Accept { channel }).await;
    assert!(
        matches!(read(&mut a).await, ServerControl::Connected { channel: id, request: Some(11), .. } if id == channel)
    );
    assert!(
        matches!(read(&mut b).await, ServerControl::Connected { channel: id, request: Some(22), .. } if id == channel)
    );
}

#[tokio::test]
async fn replacing_endpoint_closes_old_channels_without_removing_new_connection() {
    let server = Server::new(RelayLimits::default()).await;
    let mut a = server.login("a", "one").await;
    let mut b = server.login("b", "one").await;
    let old = open(&mut a, &mut b, "b").await;
    let mut replacement = server.login("a", "one").await;
    assert_eq!(
        read(&mut b).await,
        ServerControl::Closed {
            channel: old,
            reason: CloseReason::PeerLeft,
            request: None,
        }
    );
    assert!(matches!(message(&mut a).await, Message::Close(_)));
    let new = open(&mut replacement, &mut b, "b").await;
    assert_ne!(old, new);
    assert_eq!(server.relay.connection_count(), 2);
}

#[tokio::test]
async fn credentials_and_subprotocol_are_required() {
    let server = Server::new(RelayLimits::default()).await;
    assert!(connect_async(&server.url).await.is_err());
    for namespace in ["invalid", "expired"] {
        let mut socket = server.socket().await;
        control(
            &mut socket,
            ClientControl::Hello {
                endpoint: "a".into(),
                token: format!("{namespace}:a"),
            },
        )
        .await;
        let Message::Close(Some(frame)) = message(&mut socket).await else {
            panic!();
        };
        assert_eq!(u16::from(frame.code), 1008);
    }
}

#[tokio::test]
async fn pending_channels_expire_and_shutdown_closes_live_sockets() {
    let server = Server::new(RelayLimits {
        connect_timeout_secs: 1,
        heartbeat_interval_secs: 1,
        heartbeat_timeout_secs: 10,
        ..Default::default()
    })
    .await;
    let mut a = server.login("a", "one").await;
    let mut b = server.login("b", "one").await;
    control(
        &mut a,
        ClientControl::Connect {
            request: 1,
            peer: "b".into(),
        },
    )
    .await;
    let ServerControl::Incoming { channel, .. } = read(&mut b).await else {
        panic!();
    };
    assert_eq!(
        read(&mut a).await,
        ServerControl::Closed {
            channel,
            reason: CloseReason::TimedOut,
            request: Some(1),
        }
    );
    server.cancel.cancel();
    assert!(matches!(message(&mut a).await, Message::Close(_)));
}

#[tokio::test]
async fn live_expiry_and_missing_hello_release_admission_slots() {
    let server = Server::new(RelayLimits {
        max_connections: 1,
        authentication_timeout_secs: 1,
        ..Default::default()
    })
    .await;
    let mut a = server.login("a", "short").await;
    let Message::Close(Some(frame)) = message(&mut a).await else {
        panic!()
    };
    assert_eq!(u16::from(frame.code), 1008);
    let mut silent = server.socket().await;
    let Message::Close(Some(frame)) = message(&mut silent).await else {
        panic!()
    };
    assert_eq!(u16::from(frame.code), 1008);
    let mut replacement = server.login("b", "normal").await;
    server.cancel.cancel();
    assert!(matches!(message(&mut replacement).await, Message::Close(_)));
}
