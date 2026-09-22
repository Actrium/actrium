//! Real end-to-end interop tests for the v0.20 RTC architecture (Stage 3).
//!
//! Each node is a full stack: a real `PeerConnectionFactory` (loopback) +
//! `RtcConnectionManager` + `SignalingRouter` + an in-memory signaling hub.
//! Nodes complete offer/answer/candidate/role with **real** PeerConnections,
//! proving Offerer/Answerer interop, simultaneous-connect glare convergence,
//! and multi-peer isolation. No fake PeerConnections, and no dependence on the
//! old global coordinator map — role/ICE/flight state lives in each controller.

use std::sync::Arc;
use std::time::Duration;

use actr_protocol::{ActrId, ActrType, Realm};
use tokio::sync::mpsc;
use tokio::time::timeout;

use actr_framework::WebRtcPeerStatus;

use crate::wire::webrtc::v020::address::DesktopLocalAddressProvider;
use crate::wire::webrtc::v020::factory::PeerConnectionFactory;
use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::runtime::RtcRuntimeConfig;
use crate::wire::webrtc::v020::signaling::in_memory::{InMemorySignalingHub, InMemoryTransport};
use crate::wire::webrtc::v020::signaling::router::SignalingRouter;
use crate::wire::webrtc::v020::status::{NetworkPath, RtcPeerSnapshot};

const WAIT: Duration = Duration::from_secs(20);

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number,
        r#type: ActrType {
            publisher: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

/// One full v0.20 node: real factory + manager + router onto the shared hub.
struct RtcNode {
    local_id: ActrId,
    manager: Arc<RtcConnectionManager>,
    #[allow(dead_code)]
    router: Arc<SignalingRouter>,
}

fn spawn_node(hub: &Arc<InMemorySignalingHub>, local_id: ActrId) -> RtcNode {
    let (signaling_tx, signaling_rx) = mpsc::channel(64);
    let factory = Arc::new(PeerConnectionFactory::new(
        RtcRuntimeConfig::desktop_default(),
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(), // loopback host candidates only; no STUN/TURN
    ));
    let manager = Arc::new(RtcConnectionManager::new(factory, signaling_tx));
    manager.supervisor().set_signaling_generation(Some(0));
    let router = Arc::new(SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::v020::signaling::SharedLocalId::new(local_id.clone()),
    ));
    // Production wiring: teardown must release the router's per-peer chains.
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let transport: InMemoryTransport = hub.connect(local_id.clone());
    let router_task = router.clone();
    tokio::spawn(async move { router_task.run(transport, signaling_rx).await });
    RtcNode {
        local_id,
        manager,
        router,
    }
}

/// A node with the node-level media registry attached (S6-10 tests).
fn spawn_node_with_media(
    hub: &Arc<InMemorySignalingHub>,
    local_id: ActrId,
    media_registry: Arc<crate::inbound::MediaFrameRegistry>,
) -> RtcNode {
    let (signaling_tx, signaling_rx) = mpsc::channel(64);
    let factory = Arc::new(PeerConnectionFactory::new(
        RtcRuntimeConfig::desktop_default(),
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(),
    ));
    let manager = Arc::new(
        RtcConnectionManager::new(factory, signaling_tx).with_media_registry(media_registry),
    );
    manager.supervisor().set_signaling_generation(Some(0));
    let router = Arc::new(SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::v020::signaling::SharedLocalId::new(local_id.clone()),
    ));
    // Production wiring: teardown must release the router's per-peer chains.
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let transport: InMemoryTransport = hub.connect(local_id.clone());
    let router_task = router.clone();
    tokio::spawn(async move { router_task.run(transport, signaling_rx).await });
    RtcNode {
        local_id,
        manager,
        router,
    }
}

/// A node with a real recording inbound router (Stage 4F: assert payloads).
fn spawn_node_with_inbound(
    hub: &Arc<InMemorySignalingHub>,
    local_id: ActrId,
    inbound: Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
) -> RtcNode {
    let (signaling_tx, signaling_rx) = mpsc::channel(64);
    let factory = Arc::new(PeerConnectionFactory::new(
        RtcRuntimeConfig::desktop_default(),
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(),
    ));
    let manager = Arc::new(RtcConnectionManager::with_inbound(
        factory,
        signaling_tx,
        inbound,
    ));
    // Match `spawn_node`/`spawn_node_with_media`: this standalone harness has
    // no lifecycle supervisor, so commit a synthetic signaling generation and
    // open the RTC recovery gate (dea7ff08). Without it the controller drops
    // the role-negotiation probe at the gate and nothing re-drives it.
    manager.supervisor().set_signaling_generation(Some(0));
    let router = Arc::new(SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::v020::signaling::SharedLocalId::new(local_id.clone()),
    ));
    // Production wiring: teardown must release the router's per-peer chains.
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let transport: InMemoryTransport = hub.connect(local_id.clone());
    let router_task = router.clone();
    tokio::spawn(async move { router_task.run(transport, signaling_rx).await });
    RtcNode {
        local_id,
        manager,
        router,
    }
}

/// Wait until a peer's snapshot reports `sendable`, tolerating the controller
/// not existing yet (it is created on an incoming offer).
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
async fn offerer_answerer_interop_reaches_sendable() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node(&hub, test_actor_id(2));

    // A (serial 1) initiates to B (serial 2); the hub assigns A as offerer.
    a.manager.connect(b.local_id.clone()).await;

    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    let snap_b = await_sendable(&b.manager, &a.local_id).await;

    assert!(snap_a.sendable, "A should be sendable to B");
    assert!(snap_b.sendable, "B should be sendable to A");
    assert_eq!(snap_a.status, WebRtcPeerStatus::Connected);
    assert_eq!(snap_b.status, WebRtcPeerStatus::Connected);
    assert_eq!(snap_a.pc_generation, 1);
    assert_eq!(snap_b.pc_generation, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_connect_glare_converges_to_single_connection() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node(&hub, test_actor_id(2));

    // Glare: both ends initiate at the same time.
    let ((), ()) = tokio::join!(
        a.manager.connect(b.local_id.clone()),
        b.manager.connect(a.local_id.clone())
    );

    // The hub's role arbitration converges: A (lower serial) is the offerer, B
    // the answerer, and both reach Sendable over one logical connection.
    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    let snap_b = await_sendable(&b.manager, &a.local_id).await;

    assert!(snap_a.sendable, "A should be sendable after glare");
    assert!(snap_b.sendable, "B should be sendable after glare");
    assert_eq!(snap_a.status, WebRtcPeerStatus::Connected);
    assert_eq!(snap_b.status, WebRtcPeerStatus::Connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn multi_peer_connect_isolated_and_close_one_keeps_others() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node(&hub, test_actor_id(2));
    let c = spawn_node(&hub, test_actor_id(3));
    let d = spawn_node(&hub, test_actor_id(4));

    // A connects to B, C, D concurrently (A is offerer to all, serial 1 < 2,3,4).
    a.manager.connect(b.local_id.clone()).await;
    a.manager.connect(c.local_id.clone()).await;
    a.manager.connect(d.local_id.clone()).await;

    // All three pairs reach Sendable independently.
    for peer in [&b.local_id, &c.local_id, &d.local_id] {
        let snap = await_sendable(&a.manager, peer).await;
        assert!(snap.sendable, "A should be sendable to {peer:?}");
    }
    for node in [&b, &c, &d] {
        let snap = await_sendable(&node.manager, &a.local_id).await;
        assert!(snap.sendable, "{:?} should be sendable to A", node.local_id);
    }

    // Close C only: B and D stay sendable (isolation), C's controller is gone.
    assert!(a.manager.close_peer(&c.local_id).await);
    for peer in [&b.local_id, &d.local_id] {
        let snap = await_sendable(&a.manager, peer).await;
        assert!(
            snap.sendable,
            "closing C must not affect A's link to {peer:?}"
        );
    }
    assert!(a.manager.get_controller(&c.local_id).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_peer_tears_down_controller_and_allows_reconnect() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node(&hub, test_actor_id(2));

    a.manager.connect(b.local_id.clone()).await;
    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    assert!(snap_a.sendable);

    // An explicit close tears the controller down: it is removed from the
    // registry (the close barrier + task convergence, not a lingering Idle row).
    assert!(a.manager.close_peer(&b.local_id).await);
    assert!(
        a.manager.get_controller(&b.local_id).await.is_none(),
        "close_peer must remove the controller from the registry"
    );

    // A fresh connect to the same peer rebuilds a controller and reaches
    // Sendable again, proving the close fully converged.
    a.manager.connect(b.local_id.clone()).await;
    let snap_a2 = await_sendable(&a.manager, &b.local_id).await;
    assert!(
        snap_a2.sendable,
        "reconnect after close should reach Sendable"
    );
    assert_eq!(snap_a2.status, WebRtcPeerStatus::Connected);
}

// ── Stage 4F: business send path through all four lanes over real PCs ────────

/// Recorded inbound message (peer, lane, payload).
type InboundLog = std::sync::Arc<
    std::sync::Mutex<
        Vec<(
            actr_protocol::ActrId,
            crate::wire::webrtc::v020::status::LaneKind,
            bytes::Bytes,
        )>,
    >,
>;

fn recording_inbound() -> (
    Arc<crate::wire::webrtc::v020::inbound::InboundRouter>,
    InboundLog,
) {
    let log: InboundLog = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log2 = log.clone();
    let router = Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::new(
        std::sync::Arc::new(move |msg, _lease| {
            log2.lock()
                .unwrap()
                .push((msg.peer.clone(), msg.lane, msg.payload.clone()));
        }),
    ));
    (router, log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_four_lanes_send_business_data_over_real_pcs() {
    use crate::wire::webrtc::v020::status::LaneKind;
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();

    let hub = InMemorySignalingHub::new();
    let (inbound_a, log_a) = recording_inbound();
    let (inbound_b, log_b) = recording_inbound();
    let a = spawn_node_with_inbound(&hub, test_actor_id(1), inbound_a);
    let b = spawn_node_with_inbound(&hub, test_actor_id(2), inbound_b);

    // A initiates; the hub assigns A (lower serial) as offerer.
    a.manager.connect(b.local_id.clone()).await;
    let snap_a = await_sendable(&a.manager, &b.local_id).await;
    let snap_b = await_sendable(&b.manager, &a.local_id).await;
    assert!(snap_a.sendable && snap_b.sendable, "both ends sendable");

    // A -> B on all four lanes. The anchor (RpcReliable) is already up; the
    // other lanes are created lazily on first send.
    let handle_b = a
        .manager
        .get_controller(&b.local_id)
        .await
        .expect("A has a controller for B");

    // Reliable lanes via send() (blocking); latency-first via try_send(). Send
    // each lane's payload and confirm its delivery before moving to the next —
    // confirming per-lane removes the SCTP ordering race between lazily-created
    // lanes.
    async fn send_and_confirm(
        handle: &crate::wire::webrtc::v020::peer::handle::RtcPeerControllerHandle,
        log: &InboundLog,
        from: &actr_protocol::ActrId,
        lane: crate::wire::webrtc::v020::status::LaneKind,
        payload: &'static [u8],
        try_first: bool,
    ) {
        let bytes = bytes::Bytes::from_static(payload);
        // Retry until the send is admitted (lazy lane creation + renegotiation)
        // AND the receiver logs it.
        timeout(WAIT, async {
            loop {
                let result = if try_first {
                    handle.try_send(lane, bytes.clone()).await
                } else {
                    handle.send(lane, bytes.clone()).await
                };
                if result.is_ok() {
                    let delivered = {
                        log.lock().unwrap().iter().any(|(peer, l, p)| {
                            *peer == *from && *l == lane && p.as_ref() == payload
                        })
                    };
                    if delivered {
                        return;
                    }
                    // Sent but not yet observed: resend is safe for the assertion
                    // (the reliable lanes are idempotent here because we key on
                    // exact payload; a duplicate just matches again).
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("lane {lane:?} payload not delivered to receiver"));
    }

    send_and_confirm(
        &handle_b,
        &log_b,
        &a.local_id,
        LaneKind::RpcReliable,
        b"rpc-reliable",
        false,
    )
    .await;
    send_and_confirm(
        &handle_b,
        &log_b,
        &a.local_id,
        LaneKind::RpcSignal,
        b"rpc-signal",
        false,
    )
    .await;
    send_and_confirm(
        &handle_b,
        &log_b,
        &a.local_id,
        LaneKind::StreamReliable,
        b"stream-reliable",
        false,
    )
    .await;
    send_and_confirm(
        &handle_b,
        &log_b,
        &a.local_id,
        LaneKind::StreamLatencyFirst,
        b"stream-latency",
        true,
    )
    .await;

    // B -> A on RpcReliable proves the reverse direction over the same channel.
    let handle_a = b
        .manager
        .get_controller(&a.local_id)
        .await
        .expect("B has a controller for A");
    handle_a
        .send(
            LaneKind::RpcReliable,
            bytes::Bytes::from_static(b"back-at-you"),
        )
        .await
        .expect("B->A RpcReliable send");

    timeout(WAIT, async {
        loop {
            {
                let got = log_a.lock().unwrap();
                if got.iter().any(|(peer, lane, payload)| {
                    *peer == b.local_id
                        && *lane == LaneKind::RpcReliable
                        && payload.as_ref() == b"back-at-you"
                }) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("A did not receive the reverse RpcReliable payload from B");
}

/// S6-3: after a loopback interop connection reaches Sendable, the stats
/// sample resolves the selected candidate pair and the snapshot reports a
/// Direct network path (host candidates on loopback).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sendable_snapshot_resolves_network_path_via_stats() {
    let hub = InMemorySignalingHub::new();
    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node(&hub, test_actor_id(2));

    a.manager.connect(b.local_id.clone()).await;
    await_sendable(&a.manager, &b.local_id).await;

    // The GetStats effect fires on entering Sendable; its completion updates
    // network_path asynchronously. Await the resolution.
    timeout(WAIT, async {
        loop {
            let snap = a
                .manager
                .get_snapshot(&b.local_id)
                .await
                .expect("snapshot exists");
            if snap.network_path != NetworkPath::Unknown {
                assert_eq!(
                    snap.network_path,
                    NetworkPath::Direct,
                    "loopback host candidates must classify Direct"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("network_path did not resolve via stats");
}

/// S6-10: outbound media over a real PC pair — A adds a track (renegotiation
/// rides the controller's effect path), sends samples, and B's session track
/// pump dispatches them to the node media registry with 0.14 labels.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn outbound_media_sample_reaches_remote_registry() {
    use actr_framework::MediaType;

    let hub = InMemorySignalingHub::new();
    let registry = Arc::new(crate::inbound::MediaFrameRegistry::new());
    let samples = Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let samples = samples.clone();
        registry.register(
            "video-1".to_string(),
            Arc::new(move |sample, sender| {
                let samples = samples.clone();
                Box::pin(async move {
                    samples.lock().unwrap().push((sample, sender));
                    Ok(())
                })
            }),
        );
    }

    let a = spawn_node(&hub, test_actor_id(1));
    let b = spawn_node_with_media(&hub, test_actor_id(2), registry);

    a.manager.connect(b.local_id.clone()).await;
    await_sendable(&a.manager, &b.local_id).await;
    await_sendable(&b.manager, &a.local_id).await;

    let handle_a = a
        .manager
        .get_controller(&b.local_id)
        .await
        .expect("controller for B");
    handle_a
        .add_media_track("video-1", "VP8", "video")
        .await
        .expect("add media track");

    // Send until the track binds (post-renegotiation) and a sample lands.
    let sample = actr_framework::MediaSample {
        data: bytes::Bytes::from_static(b"frame-one"),
        timestamp: 4242,
        codec: "VP8".to_string(),
        media_type: MediaType::Video,
    };
    timeout(WAIT, async {
        loop {
            if !samples.lock().unwrap().is_empty() {
                return;
            }
            // Pre-bind writes error; retry until the track is live.
            let _ = handle_a.send_media_sample("video-1", &sample).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("media sample did not reach the remote registry");

    let got = samples.lock().unwrap();
    let (first, sender) = &got[0];
    assert_eq!(first.data.as_ref(), b"frame-one");
    assert_eq!(first.timestamp, 4242);
    assert_eq!(first.codec, "VP8");
    assert_eq!(first.media_type, MediaType::Video);
    assert_eq!(sender, &a.local_id, "sender is the sending node's id");
}
