//! Tests for the RTC inbound bridge.

use super::inbound::{InboundMessage, InboundRouter};
use super::inbound_bridge::{MailboxSlot, dispatcher_bridge};
use crate::inbound::DataChunkRegistry;
use crate::inbound::dispatcher::InboundDispatcher;
use crate::wire::webrtc::rtc::status::LaneKind;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, DataChunk};
use actr_runtime_mailbox::{Mailbox, MailboxStats, MessagePriority, MessageRecord, StorageResult};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, RwLock, mpsc};
use uuid::Uuid;

/// Upper bound for every "this must happen" wait. A failure surfaces as a
/// timeout instead of a hang, which keeps these tests deterministic.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn test_id(serial: u64) -> ActrId {
    ActrId {
        realm: "test@example.com".into(),
        serial_number: serial,
    }
}

/// Route an RPC request through the bridge.
fn route_rpc(
    router: &InboundRouter,
    request_id: &str,
    peer: u64,
    generation: u64,
    lane: LaneKind,
) -> bool {
    let envelope = actr_protocol::RpcEnvelope {
        request_id: request_id.to_string(),
        route_key: "test".to_string(),
        direction: Some(actr_protocol::Direction::Request as i32),
        ..Default::default()
    };
    router.route(InboundMessage {
        peer: test_id(peer),
        generation,
        lane,
        payload: bytes::Bytes::from(envelope.encode_to_vec()),
    })
}

/// Route a data chunk through the bridge.
fn route_chunk(router: &InboundRouter, stream_id: &str, sequence: u64, lane: LaneKind) -> bool {
    let chunk = DataChunk {
        stream_id: stream_id.to_string(),
        sequence,
        payload: bytes::Bytes::from_static(b"chunk"),
        ..Default::default()
    };
    router.route(InboundMessage {
        peer: test_id(2),
        generation: 1,
        lane,
        payload: bytes::Bytes::from(chunk.encode_to_vec()),
    })
}

/// Wait until the mailbox has seen at least `count` enqueues.
async fn wait_for_enqueues(mailbox: &CapturingMailbox, count: usize) {
    while mailbox.enqueue_count.load(Ordering::SeqCst) < count {
        mailbox.enqueued.notified().await;
    }
}

struct CapturingMailbox {
    enqueue_count: AtomicUsize,
    enqueued: Notify,
    first_enqueue_release: Option<Notify>,
    enqueued_requests: std::sync::Mutex<Vec<String>>,
}

struct BlockingMailbox {
    entered: Notify,
}

#[async_trait]
impl Mailbox for BlockingMailbox {
    async fn enqueue(
        &self,
        _from: Vec<u8>,
        _from_type: Option<Vec<u8>>,
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
            first_enqueue_release: None,
            enqueued_requests: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// A capturing mailbox whose first enqueue blocks until
    /// [`CapturingMailbox::release_first`] is called.
    fn blocking_first() -> Arc<Self> {
        Arc::new(Self {
            enqueue_count: AtomicUsize::new(0),
            enqueued: Notify::new(),
            first_enqueue_release: Some(Notify::new()),
            enqueued_requests: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn release_first(&self) {
        if let Some(release) = &self.first_enqueue_release {
            release.notify_one();
        }
    }

    /// Request ids in the order the mailbox received them.
    fn requests(&self) -> Vec<String> {
        self.enqueued_requests
            .lock()
            .expect("capturing mailbox request log")
            .clone()
    }
}

#[async_trait]
impl Mailbox for CapturingMailbox {
    async fn enqueue(
        &self,
        _from: Vec<u8>,
        _from_type: Option<Vec<u8>>,
        payload: Vec<u8>,
        _priority: MessagePriority,
    ) -> StorageResult<Uuid> {
        let previous = self.enqueue_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(envelope) = actr_protocol::RpcEnvelope::decode(&payload[..]) {
            self.enqueued_requests
                .lock()
                .expect("capturing mailbox request log")
                .push(envelope.request_id);
        }
        self.enqueued.notify_one();
        if previous == 0 {
            if let Some(release) = &self.first_enqueue_release {
                release.notified().await;
            }
        }
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
    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        registry,
        Default::default(),
    ));
    let mailbox = CapturingMailbox::new();
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    assert!(route_rpc(
        &router,
        "bridge-rpc",
        2,
        1,
        LaneKind::RpcReliable
    ));

    // Wait for the async dispatch to complete.
    tokio::time::timeout(TIMEOUT, wait_for_enqueues(&mailbox, 1))
        .await
        .expect("mailbox should receive the RPC request");
    assert_eq!(mailbox.enqueue_count.load(Ordering::SeqCst), 1);
    assert_eq!(mailbox.requests(), ["bridge-rpc"]);
    shutdown.cancel();
    tasks.await.expect("bridge tasks converge");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_preserves_data_chunk_order_per_lane() {
    let pending: crate::transport::correlation::PendingRequestsMap =
        std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let (sequence_tx, mut sequence_rx) = mpsc::unbounded_channel();
    let callback: crate::inbound::DataChunkCallback = Arc::new(move |chunk: DataChunk, _sender| {
        let sequence_tx = sequence_tx.clone();
        Box::pin(async move {
            let _ = sequence_tx.send(chunk.sequence);
            Ok(())
        })
    });
    registry
        .register("ordered-stream".to_string(), callback)
        .unwrap();

    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        registry,
        Default::default(),
    ));
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(CapturingMailbox::new())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    for sequence in [0u64, 1] {
        assert!(route_chunk(
            &router,
            "ordered-stream",
            sequence,
            LaneKind::StreamReliable
        ));
    }

    let mut received = Vec::new();
    for _ in 0..2 {
        received.push(
            tokio::time::timeout(TIMEOUT, sequence_rx.recv())
                .await
                .expect("ordered stream callback should complete")
                .expect("ordered stream callback channel should remain open"),
        );
    }
    assert_eq!(received, [0, 1]);

    shutdown.cancel();
    tasks.await.expect("bridge tasks converge");
}

/// The pre-v0.20 gate drained `RpcReliable` and `RpcSignal` from a single
/// receive loop, so both lanes are one traffic class: a dispatch in either lane
/// must not overtake an earlier one, even when it comes from another peer or
/// generation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_rpc_lanes_share_one_fifo_worker() {
    let pending = Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        Arc::new(DataChunkRegistry::new()),
        Default::default(),
    ));
    let mailbox = CapturingMailbox::blocking_first();
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    assert!(route_rpc(
        &router,
        "rpc-reliable-first",
        2,
        1,
        LaneKind::RpcReliable
    ));
    tokio::time::timeout(TIMEOUT, wait_for_enqueues(&mailbox, 1))
        .await
        .expect("the first RPC dispatch should block inside the mailbox");

    // Another lane, and another peer/generation: same traffic class, so both
    // wait behind the blocked dispatch.
    assert!(route_rpc(&router, "rpc-signal", 3, 7, LaneKind::RpcSignal));
    assert!(route_rpc(
        &router,
        "rpc-reliable-second",
        2,
        1,
        LaneKind::RpcReliable
    ));
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            wait_for_enqueues(&mailbox, 2)
        )
        .await
        .is_err(),
        "RpcSignal must not overtake a blocked RpcReliable dispatch: both lanes share one FIFO worker"
    );
    assert_eq!(router.inflight().0, 3, "queued work must retain its lease");

    mailbox.release_first();
    tokio::time::timeout(TIMEOUT, wait_for_enqueues(&mailbox, 3))
        .await
        .expect("queued RPC dispatches should resume after the first completes");
    assert_eq!(
        mailbox.requests(),
        ["rpc-reliable-first", "rpc-signal", "rpc-reliable-second"]
    );

    shutdown.cancel();
    tokio::time::timeout(TIMEOUT, tasks)
        .await
        .expect("bridge tasks converge")
        .expect("bridge monitor should exit cleanly");
    assert_eq!(router.inflight(), (0, 0));
}

/// The three traffic classes own independent workers: blocking the RPC class
/// must not stall either stream class. Cross-class *ordering* is deliberately
/// not asserted — the classes run concurrently by design.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_keeps_traffic_classes_independent() {
    let pending = Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
    let callback: crate::inbound::DataChunkCallback = Arc::new(move |chunk: DataChunk, _sender| {
        let stream_tx = stream_tx.clone();
        Box::pin(async move {
            let _ = stream_tx.send(chunk.stream_id.clone());
            Ok(())
        })
    });
    // One stream per class, so each class is observed on its own.
    registry
        .register("reliable-stream".to_string(), callback.clone())
        .unwrap();
    registry
        .register("latency-first-stream".to_string(), callback)
        .unwrap();

    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        registry,
        Default::default(),
    ));
    let mailbox = CapturingMailbox::blocking_first();
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    // Occupy the RPC class with a dispatch that never leaves the mailbox.
    assert!(route_rpc(
        &router,
        "rpc-blocked",
        2,
        1,
        LaneKind::RpcReliable
    ));
    tokio::time::timeout(TIMEOUT, wait_for_enqueues(&mailbox, 1))
        .await
        .expect("the RPC dispatch should reach the mailbox");

    // Both stream classes keep draining while the RPC class is blocked. Their
    // relative order is unspecified, so only the set of observed classes is
    // asserted.
    assert!(route_chunk(
        &router,
        "reliable-stream",
        0,
        LaneKind::StreamReliable
    ));
    assert!(route_chunk(
        &router,
        "latency-first-stream",
        0,
        LaneKind::StreamLatencyFirst
    ));

    let mut observed = Vec::new();
    for _ in 0..2 {
        observed.push(
            tokio::time::timeout(TIMEOUT, stream_rx.recv())
                .await
                .expect("both stream classes must dispatch while RPC is blocked")
                .expect("stream callback channel should remain open"),
        );
    }
    observed.sort();
    assert_eq!(observed, ["latency-first-stream", "reliable-stream"]);

    assert_eq!(
        mailbox.enqueue_count.load(Ordering::SeqCst),
        1,
        "the blocked RPC class must stay blocked while streams drain"
    );

    mailbox.release_first();
    shutdown.cancel();
    tokio::time::timeout(TIMEOUT, tasks)
        .await
        .expect("bridge tasks converge")
        .expect("bridge monitor should exit cleanly");
    assert_eq!(router.inflight(), (0, 0));
}

/// Admission, unlike dispatch, is not per class. `InboundRouter` enforces one
/// message and byte budget across every lane and keeps each accepted message's
/// capacity lease until its dispatch completes, so a blocked class can consume
/// the shared capacity and an idle class's message is then dropped at admission
/// before its worker runs. This test pins that overload behavior explicitly;
/// class-isolated admission budgets would remove the coupling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_admission_is_shared_across_classes() {
    let pending = Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        Arc::new(DataChunkRegistry::new()),
        Default::default(),
    ));
    let mailbox = CapturingMailbox::blocking_first();
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let config = super::config::RtcBufferConfig {
        max_inbound_dispatch_messages: 2,
        ..super::config::RtcBufferConfig::default()
    };
    let router = InboundRouter::new_with_config(handler, config);

    // One active plus one queued RPC dispatch hold the entire shared budget.
    assert!(route_rpc(
        &router,
        "rpc-active",
        2,
        1,
        LaneKind::RpcReliable
    ));
    tokio::time::timeout(TIMEOUT, wait_for_enqueues(&mailbox, 1))
        .await
        .expect("the active RPC dispatch should block inside the mailbox");
    assert!(route_rpc(
        &router,
        "rpc-queued",
        2,
        1,
        LaneKind::RpcReliable
    ));
    assert_eq!(router.inflight().0, 2, "queued work must retain its lease");

    // The stream worker is idle, but admission is shared: the router rejects
    // the chunk before the stream class can run.
    assert!(
        !route_chunk(&router, "admission-stream", 0, LaneKind::StreamReliable),
        "shared admission must reject stream traffic while the RPC class holds the budget"
    );

    // Draining the blocked dispatch frees the budget and admission recovers.
    mailbox.release_first();
    tokio::time::timeout(TIMEOUT, async {
        while router.inflight().0 != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the RPC class should drain once the mailbox releases");
    assert!(route_chunk(
        &router,
        "admission-stream",
        0,
        LaneKind::StreamReliable
    ));

    shutdown.cancel();
    tokio::time::timeout(TIMEOUT, tasks)
        .await
        .expect("bridge tasks converge")
        .expect("bridge monitor should exit cleanly");
}

#[tokio::test]
async fn bridge_drops_message_when_mailbox_not_set() {
    let pending: crate::transport::correlation::PendingRequestsMap =
        std::sync::Arc::new(crate::transport::correlation::RpcCorrelation::new());
    let registry = Arc::new(DataChunkRegistry::new());
    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        registry,
        Default::default(),
    ));
    let slot: MailboxSlot = Arc::new(RwLock::new(None)); // mailbox not set
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    // This should not panic; the message is dropped with a warning.
    assert!(route_rpc(
        &router,
        "dropped-rpc",
        2,
        1,
        LaneKind::RpcReliable
    ));
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
    let dispatcher = Arc::new(InboundDispatcher::new(
        pending,
        registry,
        Default::default(),
    ));
    let mailbox = Arc::new(BlockingMailbox {
        entered: Notify::new(),
    });
    let slot: MailboxSlot = Arc::new(RwLock::new(Some(mailbox.clone())));
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (handler, tasks) = dispatcher_bridge(dispatcher, slot, shutdown.clone());
    let router = InboundRouter::new(handler);

    assert!(route_rpc(
        &router,
        "blocked-bridge-rpc",
        2,
        1,
        LaneKind::RpcReliable
    ));
    tokio::time::timeout(TIMEOUT, mailbox.entered.notified())
        .await
        .expect("dispatch should reach the blocking mailbox");

    // The other RPC lane belongs to the same traffic class, so this message
    // stays queued behind the blocked dispatch. Shutdown must release both the
    // active lease and the queued lease.
    assert!(route_rpc(
        &router,
        "queued-bridge-signal",
        2,
        1,
        LaneKind::RpcSignal
    ));
    assert_eq!(router.inflight().0, 2);
    shutdown.cancel();
    tokio::time::timeout(TIMEOUT, tasks)
        .await
        .expect("bridge task group must converge after shutdown")
        .expect("bridge monitor should exit cleanly");
    assert_eq!(router.inflight(), (0, 0));
}
