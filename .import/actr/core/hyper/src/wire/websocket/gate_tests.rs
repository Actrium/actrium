use super::*;
use actr_protocol::{ActrType, IdentityClaims, Realm};
use actr_runtime_mailbox::{MailboxStats, MessageRecord, StorageResult};
use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

// ─── helpers ─────────────────────────────────────────────────────────────

fn test_actor_id(serial: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number: serial,
        r#type: ActrType {
            manufacturer: "test".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

/// Generate reproducible Ed25519 key pair from a fixed seed
fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// Construct a complete AIdCredential that passes verification
fn make_valid_credential(
    sk: &SigningKey,
    actor_id: &ActrId,
    expires_at: u64,
    key_id: u32,
) -> AIdCredential {
    let claims = IdentityClaims {
        actor_id: actor_id.to_string_repr(),
        expires_at,
        realm_id: actor_id.realm.realm_id,
    };
    let claims_bytes = actr_protocol::prost::Message::encode_to_vec(&claims);
    let signature = sk.sign(&claims_bytes);
    AIdCredential {
        key_id,
        claims: claims_bytes.into(),
        signature: signature.to_bytes().to_vec().into(),
    }
}

fn future_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

fn past_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 1
}

// --- mock SignalingClient (same as ais_key_cache tests, inlined to avoid module dependency) ---

struct NullSignaling;

#[async_trait]
impl crate::wire::SignalingClient for NullSignaling {
    async fn connect(&self) -> crate::transport::NetworkResult<()> {
        Ok(())
    }
    async fn disconnect(&self) -> crate::transport::NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        false
    }
    fn get_stats(&self) -> crate::wire::webrtc::SignalingStats {
        Default::default()
    }
    fn subscribe_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::wire::webrtc::SignalingEvent> {
        tokio::sync::broadcast::channel(1).1
    }
    async fn set_actor_id(&self, _: ActrId) {}
    async fn set_credential_state(&self, _: crate::lifecycle::CredentialState) {}
    async fn clear_identity(&self) {}
    async fn send_register_request(
        &self,
        _: actr_protocol::RegisterRequest,
    ) -> crate::transport::NetworkResult<actr_protocol::RegisterResponse> {
        unimplemented!()
    }
    async fn send_unregister_request(
        &self,
        _: ActrId,
        _: AIdCredential,
        _: Option<String>,
    ) -> crate::transport::NetworkResult<actr_protocol::UnregisterResponse> {
        unimplemented!()
    }
    async fn send_heartbeat(
        &self,
        _: ActrId,
        _: AIdCredential,
        _: actr_protocol::ServiceAvailabilityState,
        _: f32,
        _: f32,
    ) -> crate::transport::NetworkResult<actr_protocol::Pong> {
        unimplemented!()
    }
    async fn send_route_candidates_request(
        &self,
        _: ActrId,
        _: AIdCredential,
        _: actr_protocol::RouteCandidatesRequest,
    ) -> crate::transport::NetworkResult<actr_protocol::RouteCandidatesResponse> {
        unimplemented!()
    }
    async fn send_envelope(
        &self,
        _: actr_protocol::SignalingEnvelope,
    ) -> crate::transport::NetworkResult<()> {
        unimplemented!()
    }
    async fn receive_envelope(
        &self,
    ) -> crate::transport::NetworkResult<Option<actr_protocol::SignalingEnvelope>> {
        unimplemented!()
    }
    async fn get_signing_key(
        &self,
        _: ActrId,
        _: AIdCredential,
        _: u32,
    ) -> crate::transport::NetworkResult<(u32, Vec<u8>)> {
        Err(crate::transport::NetworkError::ConnectionError(
            "should not be called".into(),
        ))
    }
}

/// Construct WsAuthContext with pre-seeded public key (key_id already in cache, no signaling needed)
async fn make_auth_ctx(sk: &SigningKey, key_id: u32, local_actor: ActrId) -> WsAuthContext {
    let pubkey_bytes = sk.verifying_key().as_bytes().to_vec();
    let cache = AisKeyCache::new();
    cache.seed(key_id, &pubkey_bytes).await.unwrap();

    let local_credential = AIdCredential {
        key_id,
        claims: bytes::Bytes::new(),
        signature: bytes::Bytes::from(vec![0u8; 64]),
    };
    let cred_state = crate::lifecycle::CredentialState::new(local_credential, None, None);

    WsAuthContext {
        ais_key_cache: cache,
        actor_id: local_actor,
        credential_state: cred_state,
        signaling_client: Arc::new(NullSignaling),
    }
}

// ─── verify_credential ────────────────────────────────────────────────────

/// Happy path: valid credential + matching actor_id -> Some(())
#[tokio::test]
async fn verify_credential_valid_returns_some() {
    let sk = signing_key(1);
    let actor = test_actor_id(100);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let credential = make_valid_credential(&sk, &actor, future_ts(), 1);
    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);

    let result = WebSocketGate::verify_credential(&credential, &source_bytes, &ctx).await;
    assert!(result.is_some(), "valid credential should pass");
}

/// Signature flipped 1 bit -> verification fails -> None
#[tokio::test]
async fn verify_credential_tampered_signature_returns_none() {
    let sk = signing_key(2);
    let actor = test_actor_id(101);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let mut credential = make_valid_credential(&sk, &actor, future_ts(), 1);

    let mut sig = credential.signature.to_vec();
    sig[0] ^= 0xFF;
    credential.signature = sig.into();

    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);
    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none()
    );
}

/// Signature less than 64 bytes -> try_into fails -> None
#[tokio::test]
async fn verify_credential_short_signature_returns_none() {
    let sk = signing_key(3);
    let actor = test_actor_id(102);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let mut credential = make_valid_credential(&sk, &actor, future_ts(), 1);

    credential.signature = bytes::Bytes::from(vec![0u8; 32]); // too short

    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);
    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none()
    );
}

/// expires_at in the past -> expired -> None
#[tokio::test]
async fn verify_credential_expired_returns_none() {
    let sk = signing_key(4);
    let actor = test_actor_id(103);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let credential = make_valid_credential(&sk, &actor, past_ts(), 1);
    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);

    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none(),
        "expired credential should be rejected"
    );
}

/// claims.actor_id does not match ActrId from X-Actr-Source-ID -> None (prevent identity spoofing)
#[tokio::test]
async fn verify_credential_actor_id_mismatch_returns_none() {
    let sk = signing_key(5);
    let claimed_actor = test_actor_id(200); // Credential claims actor 200.
    let actual_source = test_actor_id(201); // Actual connecting peer is 201.
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;

    // The credential is signed for `claimed_actor(200)`, but the source ID is `actual_source(201)`.
    let credential = make_valid_credential(&sk, &claimed_actor, future_ts(), 1);
    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actual_source);

    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none(),
        "actor_id mismatch should be rejected"
    );
}

/// Invalid protobuf bytes in `IdentityClaims` should fail decoding and return `None`.
#[tokio::test]
async fn verify_credential_invalid_claims_proto_returns_none() {
    let sk = signing_key(6);
    let actor = test_actor_id(104);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;

    // Build garbage claim bytes and sign them. The signature is valid, but the claims cannot be decoded.
    let garbage = b"\xFF\xFF\xFF\xFF\xFF";
    let signature = sk.sign(garbage);
    let credential = AIdCredential {
        key_id: 1,
        claims: bytes::Bytes::from(garbage.to_vec()),
        signature: signature.to_bytes().to_vec().into(),
    };
    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);

    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none()
    );
}

/// Invalid protobuf bytes in `source_id_bytes` should fail `ActrId` decoding and return `None`.
#[tokio::test]
async fn verify_credential_invalid_source_id_returns_none() {
    let sk = signing_key(7);
    let actor = test_actor_id(105);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let credential = make_valid_credential(&sk, &actor, future_ts(), 1);

    let bad_source_id = b"\xFF\xFF\xFF\xFF"; // Invalid protobuf.

    assert!(
        WebSocketGate::verify_credential(&credential, bad_source_id, &ctx)
            .await
            .is_none()
    );
}

/// When `key_id` is missing from cache and signaling returns an error, return `None`.
#[tokio::test]
async fn verify_credential_unknown_key_id_returns_none() {
    let sk = signing_key(8);
    let actor = test_actor_id(106);
    // `key_id=99` is not in cache, so `NullSignaling` returns an error.
    let cache = AisKeyCache::new();
    let local_credential = AIdCredential {
        key_id: 1,
        claims: bytes::Bytes::new(),
        signature: bytes::Bytes::from(vec![0u8; 64]),
    };
    let ctx = WsAuthContext {
        ais_key_cache: cache,
        actor_id: test_actor_id(999),
        credential_state: crate::lifecycle::CredentialState::new(local_credential, None, None),
        signaling_client: Arc::new(NullSignaling),
    };

    let credential = make_valid_credential(&sk, &actor, future_ts(), 99); // `key_id=99` does not exist.
    let source_bytes = actr_protocol::prost::Message::encode_to_vec(&actor);

    assert!(
        WebSocketGate::verify_credential(&credential, &source_bytes, &ctx)
            .await
            .is_none()
    );
}

// ─── handle_envelope routing logic ───────────────────────────────────────

struct CapturingMailbox {
    enqueue_count: AtomicUsize,
    last_priority: std::sync::Mutex<Option<MessagePriority>>,
}

impl CapturingMailbox {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            enqueue_count: AtomicUsize::new(0),
            last_priority: std::sync::Mutex::new(None),
        })
    }
}

#[async_trait]
impl Mailbox for CapturingMailbox {
    async fn enqueue(
        &self,
        _from: Vec<u8>,
        _payload: Vec<u8>,
        priority: MessagePriority,
    ) -> StorageResult<Uuid> {
        self.enqueue_count.fetch_add(1, Ordering::SeqCst);
        *self.last_priority.lock().unwrap() = Some(priority);
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

fn make_rpc_envelope(request_id: &str) -> RpcEnvelope {
    RpcEnvelope {
        request_id: request_id.to_string(),
        route_key: "test".to_string(),
        payload: Some(bytes::Bytes::from("hello")),
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 5000,
        ..Default::default()
    }
}

type PendingReplies =
    Arc<RwLock<HashMap<String, (ActrId, oneshot::Sender<actr_protocol::ActorResult<Bytes>>)>>>;

fn empty_pending() -> PendingReplies {
    Arc::new(RwLock::new(HashMap::new()))
}

#[tokio::test]
async fn finish_lane_task_last_lane_removes_sink_and_fires_disconnect() {
    let peer = test_actor_id(298);
    let active_lanes = Arc::new(AtomicUsize::new(1));
    let inbound_sinks: InboundSinkMap = Arc::new(RwLock::new(HashMap::new()));
    inbound_sinks
        .write()
        .await
        .insert(peer.clone(), Arc::new(tokio::sync::Mutex::new(None)));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let hook: HookCallback = Arc::new(move |event| {
        let tx = tx.clone();
        Box::pin(async move {
            let _ = tx.send(event);
        })
    });

    WebSocketGate::finish_lane_task(
        active_lanes.clone(),
        Some(peer.clone()),
        Some(hook),
        inbound_sinks.clone(),
    )
    .await;

    assert_eq!(active_lanes.load(Ordering::SeqCst), 0);
    assert!(!inbound_sinks.read().await.contains_key(&peer));

    let event = rx.recv().await.expect("disconnect hook should fire");
    assert!(matches!(
        event,
        HookEvent::WebSocketDisconnected { peer_id } if peer_id == peer
    ));
}

#[tokio::test]
async fn finish_lane_task_non_last_lane_keeps_sink_and_skips_disconnect() {
    let peer = test_actor_id(299);
    let active_lanes = Arc::new(AtomicUsize::new(2));
    let inbound_sinks: InboundSinkMap = Arc::new(RwLock::new(HashMap::new()));
    inbound_sinks
        .write()
        .await
        .insert(peer.clone(), Arc::new(tokio::sync::Mutex::new(None)));

    let hook_calls = Arc::new(AtomicUsize::new(0));
    let hook_calls_for_hook = hook_calls.clone();
    let hook: HookCallback = Arc::new(move |event| {
        let hook_calls = hook_calls_for_hook.clone();
        Box::pin(async move {
            if matches!(event, HookEvent::WebSocketDisconnected { .. }) {
                hook_calls.fetch_add(1, Ordering::SeqCst);
            }
        })
    });

    WebSocketGate::finish_lane_task(
        active_lanes.clone(),
        Some(peer.clone()),
        Some(hook),
        inbound_sinks.clone(),
    )
    .await;

    assert_eq!(active_lanes.load(Ordering::SeqCst), 1);
    assert!(inbound_sinks.read().await.contains_key(&peer));
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn spawn_connection_tasks_get_lane_failure_removes_sink_and_fires_disconnect() {
    let peer = test_actor_id(300);
    let source_id = actr_protocol::prost::Message::encode_to_vec(&peer);
    let inbound_sinks: InboundSinkMap = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let hook: HookCallback = Arc::new(move |event| {
        let tx = tx.clone();
        Box::pin(async move {
            let _ = tx.send(event);
        })
    });

    WebSocketGate::spawn_connection_tasks(
        crate::wire::websocket::connection::WebSocketConnection::new("ws://127.0.0.1:0".into()),
        source_id,
        empty_pending(),
        Arc::new(DataChunkRegistry::new()),
        CapturingMailbox::new(),
        Some(hook),
        inbound_sinks.clone(),
    )
    .await;

    let disconnected_peer = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let event = rx
                .recv()
                .await
                .expect("disconnect hook should fire before channel closes");
            if let HookEvent::WebSocketDisconnected { peer_id } = event {
                break peer_id;
            }
        }
    })
    .await
    .expect("get_lane failure should finish lane tasks and fire disconnect");

    assert_eq!(disconnected_peer, peer);
    assert!(!inbound_sinks.read().await.contains_key(&peer));
}

/// RPC request with no pending entry should go to the mailbox with normal priority.
#[tokio::test]
async fn handle_envelope_request_goes_to_mailbox_with_normal_priority() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let envelope = make_rpc_envelope("req-1");
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![1u8, 2, 3],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *mailbox.last_priority.lock().unwrap(),
        Some(MessagePriority::Normal)
    );
}

/// `RpcSignal` should go to the mailbox with high priority.
#[tokio::test]
async fn handle_envelope_rpc_signal_uses_high_priority() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let envelope = make_rpc_envelope("sig-1");
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcSignal,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *mailbox.last_priority.lock().unwrap(),
        Some(MessagePriority::High)
    );
}

/// Missing `direction` is invalid in the current wire protocol.
#[tokio::test]
async fn handle_envelope_missing_direction_is_dropped_not_enqueued() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let mut envelope = make_rpc_envelope("missing-direction");
    envelope.direction = None;
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        0,
        "missing WS direction must be dropped"
    );
}

/// `DIRECTION_UNSPECIFIED` is only the protobuf default sentinel.
#[tokio::test]
async fn handle_envelope_unspecified_direction_is_dropped_not_enqueued() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let mut envelope = make_rpc_envelope("unspecified-direction");
    envelope.direction = Some(actr_protocol::Direction::Unspecified as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        0,
        "Unspecified WS direction must be dropped"
    );
}

/// Unknown `direction` values are invalid rather than legacy fallback.
#[tokio::test]
async fn handle_envelope_unknown_direction_is_dropped_not_enqueued() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let mut envelope = make_rpc_envelope("unknown-direction");
    envelope.direction = Some(99);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        0,
        "unknown WS direction must be dropped"
    );
}

/// An explicit Tell must be enqueued for dispatch like a Request, and must
/// not consume any pending entry.
#[tokio::test]
async fn handle_envelope_tell_direction_is_enqueued() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let (tx, _rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("tell-ws-1".to_string(), (test_actor_id(11), tx));

    let mut envelope = make_rpc_envelope("tell-ws-1");
    envelope.direction = Some(actr_protocol::Direction::Tell as i32);
    envelope.timeout_ms = 0;
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending.clone(),
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        1,
        "explicit WS Tell must be enqueued for dispatch"
    );
    assert!(
        pending.read().await.contains_key("tell-ws-1"),
        "explicit WS Tell must not consume the pending entry"
    );
}

/// RPC response with a matching pending request should wake the waiter and not enter the mailbox.
#[tokio::test]
async fn handle_envelope_response_resolves_pending_not_mailbox() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let actor = test_actor_id(1);

    let (tx, rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("req-2".to_string(), (actor, tx));

    let mut envelope = make_rpc_envelope("req-2");
    envelope.payload = Some(bytes::Bytes::from("response-payload"));
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending.clone(),
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        0,
        "response must not go to mailbox"
    );
    let result = rx.await.expect("oneshot must be resolved");
    assert!(result.is_ok(), "response payload should resolve Ok");
}

/// A response carrying both payload and error should send `Err(DecodeFailure)` to the waiter.
#[tokio::test]
async fn handle_envelope_response_both_payload_and_error_gives_decode_failure() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let actor = test_actor_id(2);
    let (tx, rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("req-3".to_string(), (actor, tx));

    let mut envelope = make_rpc_envelope("req-3");
    envelope.payload = Some(bytes::Bytes::from("x"));
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    envelope.error = Some(actr_protocol::ErrorResponse {
        code: 500,
        message: "err".to_string(),
    });
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    let result = rx.await.unwrap();
    assert!(
        matches!(result, Err(actr_protocol::ActrError::DecodeFailure(_))),
        "both payload+error should produce DecodeFailure: {result:?}"
    );
}

/// A response carrying only `error` and no payload should send `Err(Unavailable)` to the waiter.
#[tokio::test]
async fn handle_envelope_response_error_only_gives_unavailable() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let actor = test_actor_id(3);
    let (tx, rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("req-4".to_string(), (actor, tx));

    let mut envelope = make_rpc_envelope("req-4");
    envelope.payload = None;
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    envelope.error = Some(actr_protocol::ErrorResponse {
        code: 503,
        message: "unavailable".to_string(),
    });
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    let result = rx.await.unwrap();
    assert!(
        matches!(result, Err(actr_protocol::ActrError::Unavailable(_))),
        "error-only response should produce Unavailable: {result:?}"
    );
    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 0);
}

/// After response handling, the matching pending entry should be removed.
#[tokio::test]
async fn handle_envelope_response_removes_pending_entry() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let actor = test_actor_id(4);
    let (tx, _rx) = oneshot::channel::<actr_protocol::ActorResult<Bytes>>();
    pending
        .write()
        .await
        .insert("req-5".to_string(), (actor, tx));

    let mut envelope = make_rpc_envelope("req-5");
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending.clone(),
        mailbox,
    )
    .await;

    assert!(
        !pending.read().await.contains_key("req-5"),
        "pending entry must be removed after response"
    );
}

/// An explicit Response whose pending entry was already removed (caller
/// timed out) must NOT be enqueued as a new request — the #255 fix.
#[tokio::test]
async fn handle_envelope_explicit_response_with_no_pending_is_dropped_not_enqueued() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();

    let mut envelope = make_rpc_envelope("late-1");
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        0,
        "orphan late response must not be enqueued as a new request"
    );
}

/// An explicit Request is enqueued even when a pending entry happens to
/// exist for the same request_id — direction wins over pending inference.
#[tokio::test]
async fn handle_envelope_explicit_request_always_enqueues() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    // A stale pending entry with the same id must NOT divert an explicit
    // request into the response path, and must not be consumed.
    let (tx, _rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("req-stale".to_string(), (test_actor_id(9), tx));

    let mut envelope = make_rpc_envelope("req-stale");
    envelope.direction = Some(actr_protocol::Direction::Request as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending.clone(),
        mailbox.clone(),
    )
    .await;

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        1,
        "explicit Request must be enqueued"
    );
    assert!(
        pending.read().await.contains_key("req-stale"),
        "explicit Request must not consume the pending entry"
    );
}

/// An explicit Response with a matching pending entry still wakes the
/// caller — direction routing preserves the response fast path.
#[tokio::test]
async fn handle_envelope_explicit_response_with_pending_wakes_caller() {
    let mailbox = CapturingMailbox::new();
    let pending = empty_pending();
    let actor = test_actor_id(7);
    let (tx, rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert("req-resp".to_string(), (actor, tx));

    let mut envelope = make_rpc_envelope("req-resp");
    envelope.payload = Some(bytes::Bytes::from("resp"));
    envelope.direction = Some(actr_protocol::Direction::Response as i32);
    let data = actr_protocol::prost::Message::encode_to_vec(&envelope);

    WebSocketGate::handle_envelope(
        envelope,
        vec![],
        bytes::Bytes::from(data),
        PayloadType::RpcReliable,
        pending,
        mailbox.clone(),
    )
    .await;

    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 0);
    let result = rx
        .await
        .expect("oneshot must be resolved for explicit Response");
    assert!(result.is_ok());
}

#[tokio::test]
async fn send_response_returns_false_for_unknown_peer() {
    // A gate with no inbound connections: send_response must return
    // Ok(false) (not an error) so callers can fall back to another transport.
    let (_tx, rx) = tokio::sync::mpsc::channel::<InboundWsConn>(1);
    let gate = WebSocketGate::new(
        rx,
        Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        Arc::new(DataChunkRegistry::new()),
        None,
    );
    let sent = gate
        .send_response(&test_actor_id(1), RpcEnvelope::default())
        .await
        .unwrap();
    assert!(!sent, "unknown peer should yield Ok(false)");
}

#[tokio::test]
async fn send_response_returns_false_when_sink_is_none() {
    // A peer entry exists but its sink slot is None (connection dropped its
    // write-half): send_response must return Ok(false), not error.
    let (_tx, rx) = tokio::sync::mpsc::channel::<InboundWsConn>(1);
    let gate = WebSocketGate::new(
        rx,
        Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        Arc::new(DataChunkRegistry::new()),
        None,
    );
    // Insert a None sink for the peer.
    let peer = test_actor_id(2);
    let sink: WsSink = Arc::new(tokio::sync::Mutex::new(None));
    gate.inbound_sinks.write().await.insert(peer.clone(), sink);
    let sent = gate
        .send_response(&peer, RpcEnvelope::default())
        .await
        .unwrap();
    assert!(!sent, "None sink should yield Ok(false)");
}

// ─── start_receive_loop: admission and termination ───────────────────────

/// Hook probe for receive-loop tests.
///
/// `WebSocketConnectStart` fires for every inbound connection *before*
/// verification, so awaiting N of them proves the loop consumed N
/// connections — that is how these tests pin `continue` (keep serving)
/// versus `break` (stop serving) without any timing assumption.
/// `WebSocketConnected` fires only from `spawn_connection_tasks`, i.e. only
/// on the admitted path, so it is the discriminator between accept and
/// reject.
struct HookProbe {
    connect_start: mpsc::UnboundedReceiver<ActrId>,
    connected: mpsc::UnboundedReceiver<ActrId>,
    connected_count: Arc<AtomicUsize>,
}

impl HookProbe {
    /// Await `n` `ConnectStart` peers. Hooks run in independent spawned
    /// tasks, so arrival order across connections is not guaranteed — the
    /// caller compares sets, not sequences.
    ///
    /// A loop that stopped serving simply never sends the remaining events,
    /// so this is bounded: the failure mode we are pinning must surface as a
    /// legible assertion, not as a hung test.
    async fn take_connect_starts(&mut self, n: usize) -> Vec<ActrId> {
        let mut seen = Vec::with_capacity(n);
        for i in 0..n {
            let peer = tokio::time::timeout(HOOK_TIMEOUT, self.connect_start.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "only {i} of {n} ConnectStart events arrived within {HOOK_TIMEOUT:?} — \
                         the receive loop stopped serving after a rejection"
                    )
                })
                .expect("hook channel must stay open while the gate is alive");
            seen.push(peer);
        }
        seen
    }

    /// Await one `WebSocketConnected` peer, bounded for the same reason.
    async fn take_connected(&mut self) -> ActrId {
        tokio::time::timeout(HOOK_TIMEOUT, self.connected.recv())
            .await
            .expect("a verified connection must fire WebSocketConnected")
            .expect("hook channel must stay open while the gate is alive")
    }
}

/// Bound on how long a hook event may take to arrive. Only ever elapses when
/// the event is never going to fire, so it gates failure legibility, not
/// timing sensitivity.
const HOOK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn hook_probe() -> (HookCallback, HookProbe) {
    let (start_tx, connect_start) = mpsc::unbounded_channel();
    let (connected_tx, connected) = mpsc::unbounded_channel();
    let connected_count = Arc::new(AtomicUsize::new(0));
    let count_for_hook = connected_count.clone();

    let hook: HookCallback = Arc::new(move |event| {
        let start_tx = start_tx.clone();
        let connected_tx = connected_tx.clone();
        let count = count_for_hook.clone();
        Box::pin(async move {
            match event {
                HookEvent::WebSocketConnectStart { peer_id } => {
                    let _ = start_tx.send(peer_id);
                }
                HookEvent::WebSocketConnected { peer_id } => {
                    count.fetch_add(1, Ordering::SeqCst);
                    let _ = connected_tx.send(peer_id);
                }
                _ => {}
            }
        })
    });

    (
        hook,
        HookProbe {
            connect_start,
            connected,
            connected_count,
        },
    )
}

/// A started receive loop plus everything a test needs to drive and observe it.
struct RunningLoop {
    /// Kept alive so the gate (and its hook registration) outlives the loop.
    _gate: WebSocketGate,
    conn_tx: mpsc::Sender<InboundWsConn>,
    probe: HookProbe,
    /// Drops exactly when the loop task returns — see [`await_loop_exit`].
    mailbox: std::sync::Weak<CapturingMailbox>,
}

async fn start_loop(auth_ctx: Option<WsAuthContext>) -> RunningLoop {
    let (conn_tx, conn_rx) = mpsc::channel::<InboundWsConn>(8);
    let gate = WebSocketGate::new(
        conn_rx,
        empty_pending(),
        Arc::new(DataChunkRegistry::new()),
        auth_ctx,
    );
    let (hook, probe) = hook_probe();
    gate.set_hook_callback(hook);

    // The loop task is the sole owner of this mailbox once we drop ours, so
    // its `Weak` going dead is a precise, sleep-free signal that the task
    // returned.
    let mailbox = CapturingMailbox::new();
    let weak = Arc::downgrade(&mailbox);
    let as_dyn: Arc<dyn Mailbox> = mailbox.clone();
    drop(mailbox);

    gate.start_receive_loop(as_dyn)
        .await
        .expect("first start_receive_loop call must succeed");

    RunningLoop {
        _gate: gate,
        conn_tx,
        probe,
        mailbox: weak,
    }
}

/// Wait for the receive-loop task to terminate, observed through the drop of
/// the `Arc<dyn Mailbox>` it captured. The loop only releases it by returning,
/// which it only does once `conn_rx.recv()` yields `None`.
async fn await_loop_exit(mailbox: std::sync::Weak<CapturingMailbox>) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while mailbox.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("receive loop must exit once the connection channel closes");
}

/// A dialled-but-never-connected connection: enough to drive admission,
/// and its lane readers fail fast on `get_lane` if it is ever admitted.
fn unconnected_conn() -> WebSocketConnection {
    WebSocketConnection::new("ws://127.0.0.1:0".into())
}

#[tokio::test]
async fn start_receive_loop_is_single_shot() {
    let (_tx, rx) = mpsc::channel::<InboundWsConn>(1);
    let gate = WebSocketGate::new(
        rx,
        empty_pending(),
        Arc::new(DataChunkRegistry::new()),
        None,
    );

    gate.start_receive_loop(CapturingMailbox::new())
        .await
        .expect("first call takes conn_rx");

    // conn_rx has been taken, so a second call cannot start a competing loop
    // that would silently steal half the inbound connections.
    let err = gate
        .start_receive_loop(CapturingMailbox::new())
        .await
        .expect_err("second call must fail");
    assert!(
        matches!(err, ActrError::Internal(ref msg) if msg.contains("already called")),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn receive_loop_exits_when_connection_channel_closes() {
    let running = start_loop(None).await;
    drop(running.conn_tx);
    await_loop_exit(running.mailbox).await;
}

#[tokio::test]
async fn receive_loop_rejects_every_connection_when_auth_ctx_missing() {
    // No auth context is a configuration error: the loop must refuse to admit
    // anyone rather than fall through to an unauthenticated fast path.
    let mut running = start_loop(None).await;
    let peers = [test_actor_id(401), test_actor_id(402)];

    for peer in &peers {
        let source_id = actr_protocol::prost::Message::encode_to_vec(peer);
        running
            .conn_tx
            .send((unconnected_conn(), source_id, None))
            .await
            .expect("loop must be consuming connections");
    }

    // Two ConnectStart events prove the loop kept serving after the first
    // rejection instead of breaking out.
    let started = running.probe.take_connect_starts(peers.len()).await;
    for peer in &peers {
        assert!(
            started.contains(peer),
            "every inbound connection must fire ConnectStart, missing {peer}"
        );
    }

    drop(running.conn_tx);
    await_loop_exit(running.mailbox).await;

    assert_eq!(
        running.probe.connected_count.load(Ordering::SeqCst),
        0,
        "no connection may be admitted without an auth context"
    );
}

#[tokio::test]
async fn receive_loop_rejects_connection_without_credential_and_keeps_serving() {
    let sk = signing_key(11);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let mut running = start_loop(Some(ctx)).await;
    let peers = [test_actor_id(411), test_actor_id(412)];

    for peer in &peers {
        let source_id = actr_protocol::prost::Message::encode_to_vec(peer);
        // No X-Actr-Credential header at all.
        running
            .conn_tx
            .send((unconnected_conn(), source_id, None))
            .await
            .expect("loop must be consuming connections");
    }

    let started = running.probe.take_connect_starts(peers.len()).await;
    for peer in &peers {
        assert!(
            started.contains(peer),
            "a rejected connection must not stop the loop, missing {peer}"
        );
    }

    drop(running.conn_tx);
    await_loop_exit(running.mailbox).await;

    assert_eq!(
        running.probe.connected_count.load(Ordering::SeqCst),
        0,
        "a credential-less connection must never be admitted"
    );
}

#[tokio::test]
async fn receive_loop_rejects_connection_with_tampered_credential() {
    let sk = signing_key(12);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let mut running = start_loop(Some(ctx)).await;

    let peer = test_actor_id(421);
    let mut credential = make_valid_credential(&sk, &peer, future_ts(), 1);
    let mut sig = credential.signature.to_vec();
    sig[0] ^= 0xFF;
    credential.signature = sig.into();

    let source_id = actr_protocol::prost::Message::encode_to_vec(&peer);
    running
        .conn_tx
        .send((unconnected_conn(), source_id, Some(credential)))
        .await
        .expect("loop must be consuming connections");

    assert_eq!(running.probe.take_connect_starts(1).await, vec![peer]);

    drop(running.conn_tx);
    await_loop_exit(running.mailbox).await;

    assert_eq!(
        running.probe.connected_count.load(Ordering::SeqCst),
        0,
        "a connection whose signature fails verification must not be admitted"
    );
}

/// Positive control for the three rejection tests above: with the same
/// harness, a correctly signed credential *is* admitted and does fire
/// `WebSocketConnected`. Without this, a bug that silently stopped firing
/// `Connected` would make every rejection test pass vacuously.
#[tokio::test]
async fn receive_loop_admits_verified_connection() {
    let sk = signing_key(13);
    let ctx = make_auth_ctx(&sk, 1, test_actor_id(999)).await;
    let mut running = start_loop(Some(ctx)).await;

    let peer = test_actor_id(431);
    let credential = make_valid_credential(&sk, &peer, future_ts(), 1);
    let source_id = actr_protocol::prost::Message::encode_to_vec(&peer);

    running
        .conn_tx
        .send((unconnected_conn(), source_id, Some(credential)))
        .await
        .expect("loop must be consuming connections");

    assert_eq!(running.probe.take_connected().await, peer);
}
