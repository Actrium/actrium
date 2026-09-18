//! T3 wiring test: proves the RTC production data path end-to-end through
//! the transport layer (W7).
//!
//! The test exercises the full production wiring:
//! `PeerGate` → `PeerTransport` → `DefaultWireBuilder` → `RtcWireHandle` →
//! `RtcDataLane` → RTC `DataChannel.send()` → lane pump → `InboundRouter`.
//!
//! Signaling goes through the in-memory hub (same as the interop tests);
//! the W1 `RtcSignalingTransport` translation is tested separately in
//! `ws_transport.rs` tests. This test proves the transport-layer wiring.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, ActrType, PayloadType, Realm};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::timeout;
use webrtc::error::Error as RtcError;

use crate::transport::{DefaultWireBuilder, DefaultWireBuilderConfig, PeerTransport};
use crate::wire::webrtc::rtc::address::DesktopLocalAddressProvider;
use crate::wire::webrtc::rtc::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::rtc::factory::PeerConnectionFactory;
use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
use crate::wire::webrtc::rtc::peer::effects::{SessionFactory, SessionOps};
use crate::wire::webrtc::rtc::runtime::RtcRuntimeConfig;
use crate::wire::webrtc::rtc::signaling::in_memory::{InMemorySignalingHub, InMemoryTransport};
use crate::wire::webrtc::rtc::signaling::router::SignalingRouter;
use crate::wire::webrtc::rtc::status::RtcPeerSnapshot;

const WAIT: Duration = Duration::from_secs(20);

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number,
        r#type: ActrType {
            manufacturer: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

/// A node with the RTC stack + production transport wiring.
struct WiredNode {
    local_id: ActrId,
    manager: Arc<RtcConnectionManager>,
    peer_transport: Arc<PeerTransport>,
    #[allow(dead_code)]
    router: Arc<SignalingRouter>,
    inbound_log: InboundLog,
}

type InboundLog = std::sync::Arc<
    std::sync::Mutex<
        Vec<(
            ActrId,
            crate::wire::webrtc::rtc::status::LaneKind,
            bytes::Bytes,
        )>,
    >,
>;

fn real_session_factory() -> Arc<dyn SessionFactory> {
    Arc::new(PeerConnectionFactory::new(
        RtcRuntimeConfig::desktop_default(),
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(),
    ))
}

struct FailFirstBuildsFactory {
    delegate: Arc<dyn SessionFactory>,
    fail_first: usize,
    attempts: AtomicUsize,
    generations: std::sync::Mutex<Vec<u64>>,
}

impl FailFirstBuildsFactory {
    fn new(fail_first: usize) -> Arc<Self> {
        Arc::new(Self {
            delegate: real_session_factory(),
            fail_first,
            attempts: AtomicUsize::new(0),
            generations: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn generations(&self) -> Vec<u64> {
        self.generations.lock().unwrap().clone()
    }
}

#[async_trait]
impl SessionFactory for FailFirstBuildsFactory {
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        self.generations.lock().unwrap().push(handler.generation());
        let attempt = self.attempts.fetch_add(1, Ordering::AcqRel);
        if attempt < self.fail_first {
            return Err(RtcError::Other(format!(
                "injected initial PeerConnection build failure {}",
                attempt + 1
            )));
        }
        self.delegate.build(handler).await
    }
}

fn spawn_wired_node_with_factory(
    hub: &Arc<InMemorySignalingHub>,
    local_id: ActrId,
    factory: Arc<dyn SessionFactory>,
) -> WiredNode {
    let (signaling_tx, signaling_rx) = mpsc::channel(64);

    // Recording inbound router.
    let log: InboundLog = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log2 = log.clone();
    let inbound = Arc::new(crate::wire::webrtc::rtc::inbound::InboundRouter::new(
        std::sync::Arc::new(move |msg, _lease| {
            log2.lock()
                .unwrap()
                .push((msg.peer.clone(), msg.lane, msg.payload.clone()));
        }),
    ));

    let manager = Arc::new(RtcConnectionManager::with_inbound(
        factory,
        signaling_tx,
        inbound,
    ));
    manager.supervisor().set_signaling_generation(Some(0));
    let router = Arc::new(SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::rtc::signaling::SharedLocalId::new(local_id.clone()),
    ));
    // Production wiring: teardown must release the router's per-peer chains.
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let transport: InMemoryTransport = hub.connect(local_id.clone());
    let router_task = router.clone();
    tokio::spawn(async move { router_task.run(transport, signaling_rx).await });

    // Wire the production transport: DefaultWireBuilder with the rtc manager.
    let wire_builder = DefaultWireBuilder::new(DefaultWireBuilderConfig::default())
        .with_rtc_manager(manager.clone());
    let peer_transport = Arc::new(PeerTransport::new(local_id.clone(), Arc::new(wire_builder)));

    WiredNode {
        local_id,
        manager,
        peer_transport,
        router,
        inbound_log: log,
    }
}

fn spawn_wired_node(hub: &Arc<InMemorySignalingHub>, local_id: ActrId) -> WiredNode {
    spawn_wired_node_with_factory(hub, local_id, real_session_factory())
}

async fn wait_for_exact_delivery(node: &WiredNode, from: &ActrId, payload: &[u8]) {
    timeout(WAIT, async {
        loop {
            let deliveries = node
                .inbound_log
                .lock()
                .unwrap()
                .iter()
                .filter(|(peer, lane, received)| {
                    peer == from
                        && *lane == crate::wire::webrtc::rtc::status::LaneKind::RpcReliable
                        && received.as_ref() == payload
                })
                .count();
            if deliveries == 1 {
                break;
            }
            assert_eq!(
                deliveries, 0,
                "retried message was delivered more than once"
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("retried message was not delivered");

    tokio::time::sleep(Duration::from_millis(100)).await;
    let deliveries = node
        .inbound_log
        .lock()
        .unwrap()
        .iter()
        .filter(|(peer, lane, received)| {
            peer == from
                && *lane == crate::wire::webrtc::rtc::status::LaneKind::RpcReliable
                && received.as_ref() == payload
        })
        .count();
    assert_eq!(
        deliveries, 1,
        "retried message must be delivered exactly once"
    );
}

/// Wait until a peer's snapshot reports `sendable`.
async fn await_sendable(manager: &RtcConnectionManager, peer: &ActrId) -> RtcPeerSnapshot {
    timeout(WAIT, async {
        loop {
            if let Some(mut rx) = manager.subscribe(peer).await {
                loop {
                    let snap = rx.borrow().clone();
                    if snap.sendable {
                        return snap;
                    }
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("peer {} did not become sendable", peer.to_string_repr()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_wiring_rpc_round_trip_through_rtc_transport() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_wired_node(&hub, test_actor_id(1));
    let b = spawn_wired_node(&hub, test_actor_id(2));

    // Connect A→B through the production transport layer.
    a.manager.connect(b.local_id.clone()).await;
    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    let snap_b = await_sendable(&b.manager, &a.local_id).await;
    assert!(snap_a.sendable && snap_b.sendable, "both ends sendable");

    // Send an RPC envelope through the PeerTransport (production wiring path).
    let envelope = actr_protocol::RpcEnvelope {
        request_id: "w7-rpc".to_string(),
        route_key: "test.Service.Method".to_string(),
        payload: Some(bytes::Bytes::from_static(b"rpc-payload")),
        error: None,
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 5000,
        ..Default::default()
    };
    let data = bytes::Bytes::from(ProstMessage::encode_to_vec(&envelope));
    let dest = crate::transport::Dest::peer(b.local_id.clone());

    // The transport layer sends through DefaultWireBuilder → RtcWireHandle →
    // RtcDataLane → rtc DataChannel.
    a.peer_transport
        .send_with_identity(&dest, PayloadType::RpcReliable, &data, None)
        .await
        .expect("send through rtc transport should succeed");

    // Verify B received the payload through the rtc inbound path.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::rtc::status::LaneKind::RpcReliable
                        && payload.as_ref() == data.as_ref()
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("B should receive the RPC payload through the rtc inbound path");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_wiring_lazy_connect_is_driven_by_wire_pool() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_wired_node(&hub, test_actor_id(11));
    let b = spawn_wired_node(&hub, test_actor_id(12));
    let payload = bytes::Bytes::from_static(b"wire-pool-lazy-connect");
    let dest = crate::transport::Dest::peer(b.local_id.clone());

    // Do not pre-connect either Manager. Creating the DestTransport must hand
    // an unconnected RtcWireHandle to WirePool, whose first attempt drives
    // role negotiation, a fresh PC, and readiness before this send proceeds.
    timeout(
        WAIT,
        a.peer_transport
            .send_with_identity(&dest, PayloadType::RpcReliable, &payload, None),
    )
    .await
    .expect("lazy RTC connect timed out")
    .expect("lazy RTC send failed");

    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    let snap_b = await_sendable(&b.manager, &a.local_id).await;
    assert_eq!(snap_a.pc_generation, 1);
    assert_eq!(snap_b.pc_generation, 1);

    timeout(WAIT, async {
        loop {
            if b.inbound_log
                .lock()
                .unwrap()
                .iter()
                .any(|(peer, lane, received)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::rtc::status::LaneKind::RpcReliable
                        && received.as_ref() == payload.as_ref()
                })
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("lazy-connect payload was not delivered");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn sender_as_offerer_retries_failed_pc_builds_then_delivers_message() {
    let hub = InMemorySignalingHub::new();
    let sender_factory = FailFirstBuildsFactory::new(2);
    let receiver_factory = FailFirstBuildsFactory::new(0);
    // The in-memory signaling rule assigns the lower serial as Offerer.
    let sender = spawn_wired_node_with_factory(&hub, test_actor_id(21), sender_factory.clone());
    let receiver = spawn_wired_node_with_factory(&hub, test_actor_id(22), receiver_factory.clone());
    let payload = bytes::Bytes::from_static(b"offerer-build-retry");
    let dest = crate::transport::Dest::peer(receiver.local_id.clone());

    timeout(
        WAIT,
        sender
            .peer_transport
            .send_with_identity(&dest, PayloadType::RpcReliable, &payload, None),
    )
    .await
    .expect("Offerer message-driven retries timed out")
    .expect("Offerer message-driven retries failed");

    let sender_snapshot = await_sendable(&sender.manager, &receiver.local_id).await;
    let receiver_snapshot = await_sendable(&receiver.manager, &sender.local_id).await;
    assert_eq!(
        sender_snapshot.role,
        crate::wire::webrtc::rtc::peer::state::Role::Offerer
    );
    assert_eq!(
        receiver_snapshot.role,
        crate::wire::webrtc::rtc::peer::state::Role::Answerer
    );
    assert_eq!(sender_factory.generations(), vec![1, 2, 3]);
    assert_eq!(sender_snapshot.pc_generation, 3);
    assert_eq!(receiver_factory.generations(), vec![1]);
    assert_eq!(receiver_snapshot.pc_generation, 1);
    wait_for_exact_delivery(&receiver, &sender.local_id, &payload).await;

    sender.manager.close_all().await;
    receiver.manager.close_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn sender_as_answerer_retries_failed_pc_builds_then_delivers_message() {
    let hub = InMemorySignalingHub::new();
    let sender_factory = FailFirstBuildsFactory::new(2);
    let passive_offerer_factory = FailFirstBuildsFactory::new(0);
    // The higher-serial sender loses role arbitration and becomes Answerer;
    // every retry must force the lower-serial passive Offerer to replace its
    // never-sendable PC and emit a fresh Offer.
    let sender = spawn_wired_node_with_factory(&hub, test_actor_id(32), sender_factory.clone());
    let passive_offerer =
        spawn_wired_node_with_factory(&hub, test_actor_id(31), passive_offerer_factory.clone());
    let payload = bytes::Bytes::from_static(b"answerer-build-retry");
    let dest = crate::transport::Dest::peer(passive_offerer.local_id.clone());

    timeout(
        WAIT,
        sender
            .peer_transport
            .send_with_identity(&dest, PayloadType::RpcReliable, &payload, None),
    )
    .await
    .expect("Answerer message-driven retries timed out")
    .expect("Answerer message-driven retries failed");

    let sender_snapshot = await_sendable(&sender.manager, &passive_offerer.local_id).await;
    let offerer_snapshot = await_sendable(&passive_offerer.manager, &sender.local_id).await;
    assert_eq!(
        sender_snapshot.role,
        crate::wire::webrtc::rtc::peer::state::Role::Answerer
    );
    assert_eq!(
        offerer_snapshot.role,
        crate::wire::webrtc::rtc::peer::state::Role::Offerer
    );
    assert_eq!(sender_factory.generations(), vec![1, 2, 3]);
    assert_eq!(sender_snapshot.pc_generation, 3);
    assert_eq!(passive_offerer_factory.generations(), vec![1, 2, 3]);
    assert_eq!(offerer_snapshot.pc_generation, 3);
    wait_for_exact_delivery(&passive_offerer, &sender.local_id, &payload).await;

    sender.manager.close_all().await;
    passive_offerer.manager.close_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_wiring_data_chunk_through_rtc_transport() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_wired_node(&hub, test_actor_id(1));
    let b = spawn_wired_node(&hub, test_actor_id(2));

    a.manager.connect(b.local_id.clone()).await;
    await_sendable(&a.manager, &b.local_id).await;
    await_sendable(&b.manager, &a.local_id).await;

    // Send a DataChunk payload through the transport.
    let chunk_payload = bytes::Bytes::from_static(b"stream-chunk-data");
    let dest = crate::transport::Dest::peer(b.local_id.clone());

    a.peer_transport
        .send_with_identity(&dest, PayloadType::StreamReliable, &chunk_payload, None)
        .await
        .expect("stream send through rtc transport should succeed");

    // Verify B received it on the StreamReliable lane.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::rtc::status::LaneKind::StreamReliable
                        && payload.as_ref() == b"stream-chunk-data"
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("B should receive the stream payload through the rtc inbound path");
}

/// T2 multi-fragment: prove the RTC fragment codec handles a payload larger
/// than `DC_MAX_PAYLOAD_SIZE` (requires 4 fragments). Verifies the pump
/// reassembles the fragments and the inbound payload is byte-identical.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_wiring_multi_fragment_round_trip() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_wired_node(&hub, test_actor_id(1));
    let b = spawn_wired_node(&hub, test_actor_id(2));

    a.manager.connect(b.local_id.clone()).await;
    await_sendable(&a.manager, &b.local_id).await;
    await_sendable(&b.manager, &a.local_id).await;

    // 200 KiB — requires 4 fragments at DC_MAX_PAYLOAD_SIZE (65527 bytes).
    let large: Vec<u8> = (0u8..=255).cycle().take(200 * 1024).collect();
    let large_payload = bytes::Bytes::from(large);
    let dest = crate::transport::Dest::peer(b.local_id.clone());

    a.peer_transport
        .send_with_identity(&dest, PayloadType::StreamReliable, &large_payload, None)
        .await
        .expect("large send through RTC transport should succeed");

    // Verify B received the full 200 KiB payload, byte-identical.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::rtc::status::LaneKind::StreamReliable
                        && payload.as_ref() == large_payload.as_ref()
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("B should receive the full 200 KiB multi-fragment payload");
}
