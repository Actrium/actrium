//! T3 wiring test: proves the v0.20 production data path end-to-end through
//! the transport layer (W7).
//!
//! The test exercises the full production wiring:
//! `PeerGate` → `PeerTransport` → `DefaultWireBuilder` → `V020WireHandle` →
//! `V020DataLane` → v0.20 `DataChannel.send()` → lane pump → `InboundRouter`.
//!
//! Signaling goes through the in-memory hub (same as the interop tests);
//! the W1 `V020SignalingTransport` translation is tested separately in
//! `ws_transport.rs` tests. This test proves the transport-layer wiring.

use std::sync::Arc;
use std::time::Duration;

use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, ActrType, PayloadType, Realm};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::transport::{DefaultWireBuilder, DefaultWireBuilderConfig, PeerTransport};
use crate::wire::webrtc::v020::address::DesktopLocalAddressProvider;
use crate::wire::webrtc::v020::factory::PeerConnectionFactory;
use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::runtime::RtcRuntimeConfig;
use crate::wire::webrtc::v020::signaling::in_memory::{InMemorySignalingHub, InMemoryTransport};
use crate::wire::webrtc::v020::signaling::router::SignalingRouter;
use crate::wire::webrtc::v020::status::RtcPeerSnapshot;

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

/// A node with the v0.20 stack + production transport wiring.
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
            crate::wire::webrtc::v020::status::LaneKind,
            bytes::Bytes,
        )>,
    >,
>;

fn spawn_wired_node(hub: &Arc<InMemorySignalingHub>, local_id: ActrId) -> WiredNode {
    let (signaling_tx, signaling_rx) = mpsc::channel(64);
    let factory = Arc::new(PeerConnectionFactory::new(
        RtcRuntimeConfig::desktop_default(),
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(),
    ));

    // Recording inbound router.
    let log: InboundLog = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log2 = log.clone();
    let inbound = Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::new(
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
        crate::wire::webrtc::v020::signaling::SharedLocalId::new(local_id.clone()),
    ));
    let transport: InMemoryTransport = hub.connect(local_id.clone());
    let router_task = router.clone();
    tokio::spawn(async move { router_task.run(transport, signaling_rx).await });

    // Wire the production transport: DefaultWireBuilder with the v020 manager.
    let wire_builder = DefaultWireBuilder::new(DefaultWireBuilderConfig::default())
        .with_v020_manager(manager.clone());
    let peer_transport = Arc::new(PeerTransport::new(local_id.clone(), Arc::new(wire_builder)));

    WiredNode {
        local_id,
        manager,
        peer_transport,
        router,
        inbound_log: log,
    }
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
async fn production_wiring_rpc_round_trip_through_v020_transport() {
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

    // The transport layer sends through DefaultWireBuilder → V020WireHandle →
    // V020DataLane → v020 DataChannel.
    a.peer_transport
        .send_with_identity(&dest, PayloadType::RpcReliable, &data, None)
        .await
        .expect("send through v020 transport should succeed");

    // Verify B received the payload through the v020 inbound path.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::v020::status::LaneKind::RpcReliable
                        && payload.as_ref() == data.as_ref()
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("B should receive the RPC payload through the v020 inbound path");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_wiring_data_chunk_through_v020_transport() {
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
        .expect("stream send through v020 transport should succeed");

    // Verify B received it on the StreamReliable lane.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::v020::status::LaneKind::StreamReliable
                        && payload.as_ref() == b"stream-chunk-data"
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("B should receive the stream payload through the v020 inbound path");
}

/// T2 multi-fragment: prove the v0.20 fragment codec handles a payload larger
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
        .expect("large send through v0.20 transport should succeed");

    // Verify B received the full 200 KiB payload, byte-identical.
    timeout(WAIT, async {
        loop {
            {
                let got = b.inbound_log.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == a.local_id
                        && *lane == crate::wire::webrtc::v020::status::LaneKind::StreamReliable
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
