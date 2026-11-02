use super::*;
use crate::inbound::DataChunkRegistry;
use crate::lifecycle::{node::PeerTypes, session_state::SessionSnapshot};
use crate::transport::correlation::RpcCorrelation;
use actr_relay_security::{
    CERTIFICATE_DOMAIN, Certificate, IdentityClaims, IssuedIdentity, signing_bytes,
};
use actr_runtime_mailbox::{Mailbox, MailboxStats, MessagePriority, MessageRecord, StorageResult};
use ed25519_dalek::{Signer, SigningKey};
use uuid::Uuid;

struct EmptyMailbox;

#[async_trait]
impl Mailbox for EmptyMailbox {
    async fn enqueue(
        &self,
        _: Vec<u8>,
        _: Option<Vec<u8>>,
        _: Vec<u8>,
        _: MessagePriority,
    ) -> StorageResult<Uuid> {
        Ok(Uuid::new_v4())
    }
    async fn dequeue(&self) -> StorageResult<Vec<MessageRecord>> {
        Ok(vec![])
    }
    async fn ack(&self, _: Uuid) -> StorageResult<()> {
        Ok(())
    }
    async fn status(&self) -> StorageResult<MailboxStats> {
        Ok(MailboxStats {
            queued_messages: 0,
            inflight_messages: 0,
            queued_by_priority: Default::default(),
        })
    }
}

fn identity(endpoint: &str, now: u64) -> LocalIdentity {
    let key = IdentityKey::generate().unwrap();
    let signer = SigningKey::from_bytes(&[7; 32]);
    let claims = serde_json::to_vec(&IdentityClaims {
        domain: CERTIFICATE_DOMAIN.into(),
        endpoint: endpoint.into(),
        actor_type: "example/Client@1.0.0".into(),
        client_key: "fixture".into(),
        public_key: key.public,
        admission_commitment: key.admission_commitment(),
        expires_at: now + 120,
    })
    .unwrap();
    let signature = signer.sign(&signing_bytes(&claims)).to_bytes().to_vec();
    let issued = IssuedIdentity {
        certificate: Certificate {
            key_id: 1,
            claims,
            signature,
        },
        issuer: IssuerKey {
            key_id: 1,
            public_key: *signer.verifying_key().as_bytes(),
            expires_at: now + 600,
        },
    };
    key.certify(issued, endpoint, now).unwrap()
}

struct Fixture {
    client: Arc<RelayClient>,
    transport: Transport,
    captured: mpsc::UnboundedReceiver<Message>,
    local: ActrId,
    remote: ActrId,
    remote_identity: LocalIdentity,
    issuer: IssuerKey,
    now: u64,
    shutdown: CancellationToken,
}

impl Fixture {
    async fn new() -> Self {
        let now = now_secs();
        let local = ActrId::from_string_repr("2.test@example.com").unwrap();
        let remote = ActrId::from_string_repr("1.test@example.com").unwrap();
        let local_identity = identity(&local.to_string_repr(), now);
        let remote_identity = identity(&remote.to_string_repr(), now);
        let issuer = local_identity.issued.issuer.clone();
        let gate = Arc::new(WebSocketGate::new_relay(
            Arc::new(RpcCorrelation::new()),
            Arc::new(DataChunkRegistry::new()),
            PeerTypes::default(),
        ));
        gate.start_receive_loop(Arc::new(EmptyMailbox))
            .await
            .unwrap();
        let session = SessionState::new(SessionSnapshot::empty_with_id(local.clone(), 1));
        let shutdown = CancellationToken::new();
        let client = RelayClient::new(
            "ws://unused".into(),
            "http://unused".into(),
            None,
            session,
            gate,
            shutdown.clone(),
        );
        client.issuers.write().await.insert(1, issuer.clone());
        let (tx, mut outgoing) = mpsc::channel::<Outgoing>(16);
        let (captured_tx, captured) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(out) = outgoing.recv().await {
                let Outgoing { message, ack, .. } = out;
                if captured_tx.send(message).is_err() {
                    break;
                }
                if let Some(ack) = ack {
                    let _ = ack.send(Ok(()));
                }
            }
        });
        let transport = Transport {
            tx,
            budget: Arc::new(Semaphore::new(QUEUE_BYTES)),
            encryption_budget: Arc::new(Semaphore::new(QUEUE_BYTES)),
            cancel: shutdown.child_token(),
            identity: Arc::new(local_identity),
        };
        client.status.send_replace(Status::Ready(transport.clone()));
        Self {
            client,
            transport,
            captured,
            local,
            remote,
            remote_identity,
            issuer,
            now,
            shutdown,
        }
    }

    async fn next_message(&mut self) -> Message {
        tokio::time::timeout(Duration::from_secs(3), self.captured.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn next_binary(&mut self) -> Bytes {
        let message = self.next_message().await;
        let Message::Binary(frame) = message else {
            panic!("expected binary frame: {message:?}");
        };
        let (channel, payload) = decode_data(frame, MAX_FRAME).unwrap();
        assert_eq!(channel, 42);
        payload
    }

    async fn start_dial(
        &mut self,
        target: ActrId,
    ) -> (
        u64,
        tokio::task::JoinHandle<NetworkResult<(u64, WebSocketConnection)>>,
    ) {
        let client = self.client.clone();
        let expected = target.to_string_repr();
        let dial = tokio::spawn(async move { client.connect_peer(target).await });
        let Message::Text(text) = self.next_message().await else {
            panic!("expected connect control");
        };
        let ClientControl::Connect { request, peer } = serde_json::from_str(&text).unwrap() else {
            panic!("expected connect control");
        };
        assert_eq!(peer, expected);
        (request, dial)
    }

    async fn connected(&self, request: Option<u64>) -> NetworkResult<()> {
        self.client
            .handle_control(
                ServerControl::Connected {
                    channel: 42,
                    request,
                    peer: self.remote.to_string_repr(),
                    metadata: String::new(),
                },
                &self.transport,
            )
            .await
    }

    async fn handshake(&mut self) -> SecureSession {
        let budget = Arc::new(Semaphore::new(QUEUE_BYTES));
        let mut remote = SecureSession::new(
            &self.remote_identity,
            &self.local.to_string_repr(),
            42,
            self.now,
        )
        .unwrap();
        let first = remote.start().unwrap().unwrap();
        self.client
            .receive_secure(42, Bytes::from(first), &self.transport, &budget)
            .await
            .unwrap();
        let second = self.next_binary().await;
        let third = remote
            .receive(&second, Some(&self.issuer), self.now)
            .unwrap()
            .outbound
            .pop()
            .unwrap();
        self.client
            .receive_secure(42, Bytes::from(third), &self.transport, &budget)
            .await
            .unwrap();
        let fourth = self.next_binary().await;
        let fifth = remote
            .receive(&fourth, None, self.now)
            .unwrap()
            .outbound
            .pop()
            .unwrap();
        self.client
            .receive_secure(42, Bytes::from(fifth), &self.transport, &budget)
            .await
            .unwrap();
        remote
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

#[tokio::test]
async fn connected_response_cannot_replace_the_requested_peer() {
    let mut fixture = Fixture::new().await;
    let target = ActrId::from_string_repr("3.test@example.com").unwrap();
    let (request, dial) = fixture.start_dial(target).await;
    assert!(fixture.connected(Some(request)).await.is_err());
    assert!(!fixture.client.channels.read().await.contains_key(&42));
    fixture.client.clear().await;
    assert!(dial.await.unwrap().is_err());
}

#[tokio::test]
async fn matching_outbound_peer_completes_handshake_and_carries_data() {
    let mut fixture = Fixture::new().await;
    let (request, dial) = fixture.start_dial(fixture.remote.clone()).await;
    fixture.connected(Some(request)).await.unwrap();
    let mut remote = fixture.handshake().await;
    let (_, connection) = tokio::time::timeout(Duration::from_secs(3), dial)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let lane = connection
        .get_lane(PayloadType::StreamReliable)
        .await
        .unwrap();
    lane.send(Bytes::from_static(b"valid peer")).await.unwrap();
    let ciphertext = fixture.next_binary().await;
    let plaintext = remote
        .receive(&ciphertext, None, fixture.now)
        .unwrap()
        .application
        .unwrap();
    assert!(plaintext.ends_with(b"valid peer"));
}

#[tokio::test]
async fn reused_ready_channel_completes_the_pending_dial() {
    let mut fixture = Fixture::new().await;
    let (request, dial) = fixture.start_dial(fixture.remote.clone()).await;
    fixture
        .client
        .handle_control(
            ServerControl::Incoming {
                channel: 42,
                peer: fixture.remote.to_string_repr(),
                metadata: String::new(),
            },
            &fixture.transport,
        )
        .await
        .unwrap();
    let Message::Text(text) = fixture.next_message().await else {
        panic!("expected accept control");
    };
    assert!(matches!(
        serde_json::from_str::<ClientControl>(&text).unwrap(),
        ClientControl::Accept { channel: 42 }
    ));
    fixture.connected(None).await.unwrap();
    let _remote = fixture.handshake().await;
    assert!(*fixture.client.channels.read().await[&42].ready.borrow());
    fixture.connected(Some(request)).await.unwrap();
    let (id, _) = tokio::time::timeout(Duration::from_secs(3), dial)
        .await
        .expect("reused ready channel must resolve the pending dial")
        .unwrap()
        .unwrap();
    assert_eq!(id, 42);
}

#[tokio::test]
async fn cancelled_request_does_not_break_an_existing_incoming_channel() {
    let mut fixture = Fixture::new().await;
    fixture
        .client
        .handle_control(
            ServerControl::Incoming {
                channel: 42,
                peer: fixture.remote.to_string_repr(),
                metadata: String::new(),
            },
            &fixture.transport,
        )
        .await
        .unwrap();
    let _accept = fixture.next_message().await;
    fixture.connected(Some(999)).await.unwrap();
    let _remote = fixture.handshake().await;
    assert!(*fixture.client.channels.read().await[&42].ready.borrow());
}

#[tokio::test]
async fn historical_failed_peers_do_not_exhaust_capacity() {
    let fixture = Fixture::new().await;
    fixture.shutdown.cancel();
    for serial in 1..=MAX_DESIRED_PEERS + 1 {
        let peer = ActrId {
            realm: "test@example.com".into(),
            serial_number: serial as u64 + 100,
        };
        let error = fixture.client.connect_peer(peer).await.unwrap_err();
        assert!(!error.to_string().contains("capacity exhausted"));
    }
    assert_eq!(fixture.client.desired.lock().await.len(), MAX_DESIRED_PEERS);
    assert!(fixture.client.peer_locks.lock().await.len() <= 1);
}

#[tokio::test]
async fn capacity_eviction_preserves_active_and_dialing_peers() {
    let fixture = Fixture::new().await;
    let peers: Vec<_> = (0..=MAX_DESIRED_PEERS)
        .map(|serial| ActrId {
            realm: "test@example.com".into(),
            serial_number: serial as u64 + 100,
        })
        .collect();
    for peer in &peers[..MAX_DESIRED_PEERS] {
        fixture.client.remember_peer(peer).await.unwrap();
    }
    fixture
        .client
        .peers
        .write()
        .await
        .insert(peers[0].clone(), 42);
    let dialing = Arc::new(Mutex::new(()));
    fixture
        .client
        .peer_locks
        .lock()
        .await
        .insert(peers[1].clone(), Arc::downgrade(&dialing));

    fixture
        .client
        .remember_peer(&peers[MAX_DESIRED_PEERS])
        .await
        .unwrap();
    let desired = fixture.client.desired.lock().await;
    assert!(desired.contains(&peers[0]));
    assert!(desired.contains(&peers[1]));
    assert!(!desired.contains(&peers[2]));
}
