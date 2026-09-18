//! Tests for the RTC inbound bridge.

use super::inbound::{InboundMessage, InboundRouter};
use super::inbound_bridge::{MailboxSlot, dispatcher_bridge};
use crate::inbound::DataChunkRegistry;
use crate::inbound::dispatcher::InboundDispatcher;
use crate::wire::webrtc::rtc::status::LaneKind;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, ActrType, Realm};
use actr_runtime_mailbox::{Mailbox, MailboxStats, MessagePriority, MessageRecord, StorageResult};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

fn test_id(serial: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number: serial,
        r#type: ActrType {
            manufacturer: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

struct CapturingMailbox {
    enqueue_count: AtomicUsize,
    enqueued: Notify,
}

struct BlockingMailbox {
    entered: Notify,
}

#[async_trait]
impl Mailbox for BlockingMailbox {
    async fn enqueue(
        &self,
        _from: Vec<u8>,
        _payload: Vec<u8>,
        _priority: MessagePriority,
    ) -> StorageResult<Uuid> {
        self.entered.notify_one();
        std::future::pending().await
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

impl CapturingMailbox {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            enqueue_count: AtomicUsize::new(0),
            enqueued: Notify::new(),
        })
    }
}

#[async_trait]
impl Mailbox for CapturingMailbox {
    async fn enqueue(
        &self,
        _from: Vec<u8>,
        _payload: Vec<u8>,
        _priority: MessagePriority,
    ) -> StorageResult<Uuid> {
        self.enqueue_count.fetch_add(1, Ordering::SeqCst);
        self.enqueued.notify_one();
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

#[tokio::test]
async fn bridge_routes_rpc_to_mailbox() {
    let pending: crate::transport::correlation::PendingRequestsMap =
        std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let dispatcher = Arc::new(InboundDispatcher::new(pending, registry));
    let mailbox = CapturingMailbox::new();
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    // Send an RPC request through the bridge.
    let envelope = actr_protocol::RpcEnvelope {
        request_id: "bridge-rpc".to_string(),
        route_key: "test".to_string(),
        payload: Some(bytes::Bytes::from("hello")),
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 5000,
        ..Default::default()
    };
    let data = bytes::Bytes::from(ProstMessage::encode_to_vec(&envelope));
    assert!(router.route(InboundMessage {
        peer: test_id(2),
        generation: 1,
        lane: LaneKind::RpcReliable,
        payload: data,
    }));

    // Wait for the async dispatch to complete.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while mailbox.enqueue_count.load(Ordering::SeqCst) == 0 {
            mailbox.enqueued.notified().await;
        }
    })
    .await
    .expect("mailbox should receive the RPC request");
    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 1);
    shutdown.cancel();
    tasks.await.expect("bridge tasks converge");
}

#[tokio::test]
async fn bridge_drops_message_when_mailbox_not_set() {
    let pending: crate::transport::correlation::PendingRequestsMap =
        std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let dispatcher = Arc::new(InboundDispatcher::new(pending, registry));
    let slot: MailboxSlot = Arc::new(RwLock::new(None)); // mailbox not set
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    // This should not panic; the message is dropped with a warning.
    assert!(router.route(InboundMessage {
        peer: test_id(2),
        generation: 1,
        lane: LaneKind::RpcReliable,
        payload: bytes::Bytes::from("test"),
    }));
    // Give the spawned task a chance to run.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    shutdown.cancel();
    tasks.await.expect("bridge tasks converge");
}

#[tokio::test]
async fn bridge_shutdown_cancels_a_blocked_dispatch_and_converges() {
    let pending: crate::transport::correlation::PendingRequestsMap =
        std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let dispatcher = Arc::new(InboundDispatcher::new(pending, registry));
    let mailbox = Arc::new(BlockingMailbox {
        entered: Notify::new(),
    });
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    let envelope = actr_protocol::RpcEnvelope {
        request_id: "blocked-bridge-rpc".to_string(),
        route_key: "test".to_string(),
        payload: Some(bytes::Bytes::from("hello")),
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 5000,
        ..Default::default()
    };
    assert!(router.route(InboundMessage {
        peer: test_id(2),
        generation: 1,
        lane: LaneKind::RpcReliable,
        payload: bytes::Bytes::from(ProstMessage::encode_to_vec(&envelope)),
    }));
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        mailbox.entered.notified(),
    )
    .await
    .expect("dispatch should reach the blocking mailbox");

    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), tasks)
        .await
        .expect("bridge task group must converge after shutdown")
        .expect("bridge monitor should exit cleanly");
}
