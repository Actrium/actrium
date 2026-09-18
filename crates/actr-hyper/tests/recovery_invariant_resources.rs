//! Resource-level invariant confirmations after the webrtc-rs cutover.
//!
//! Transport-flight tests remain black-box tests of `PeerTransport`; RTC tests
//! use the production T3 path and observe only authoritative snapshots,
//! generations, public lanes, and bounded teardown. No old Coordinator cache
//! or callback-registration seam is reintroduced.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use actr_hyper::test_support::{
    TestHarness, drive_mailbox_tail_while_storage_idle, timer_observation,
};
use actr_hyper::transport::{
    ConnType, DataLane, Dest, NetworkResult, PeerTransport, WireBuilder, WireHandle,
};
use actr_hyper::wire::webrtc::{
    HookCallback, HookEvent, InboundQueueConfig, ReconnectConfig, SignalingClient, SignalingConfig,
    SignalingEvent, WebSocketSignalingClient,
};
use actr_protocol::{ActrId, Direction, PayloadType, RpcEnvelope};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{Barrier, Semaphore, watch};

fn resource_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn rendezvous(barrier: &Barrier, description: &str) {
    tokio::time::timeout(Duration::from_secs(10), barrier.wait())
        .await
        .unwrap_or_else(|_| panic!("timed out at {description}"));
}

#[derive(Debug)]
struct NoopLane;

#[async_trait]
impl DataLane for NoopLane {
    async fn send(&self, _data: Bytes) -> NetworkResult<()> {
        Ok(())
    }

    fn lane_type(&self) -> &'static str {
        "invariant-noop"
    }
}

#[derive(Debug)]
struct TestWire {
    close_count: AtomicUsize,
}

impl TestWire {
    fn new() -> Self {
        Self {
            close_count: AtomicUsize::new(0),
        }
    }

    fn close_count(&self) -> usize {
        self.close_count.load(Ordering::Acquire)
    }
}

#[async_trait]
impl WireHandle for TestWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        0
    }

    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn close(&self) -> NetworkResult<()> {
        self.close_count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(Arc::new(NoopLane))
    }
}

struct IndependentDestinationFactory {
    calls: AtomicUsize,
    host_entered: Semaphore,
    host_release: Semaphore,
}

#[async_trait]
impl WireBuilder for IndependentDestinationFactory {
    async fn create_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if dest == &Dest::host() {
            self.host_entered.add_permits(1);
            self.host_release
                .acquire()
                .await
                .expect("host release semaphore")
                .forget();
        }
        Ok(vec![Arc::new(TestWire::new())])
    }
}

struct SupersedableFactory {
    calls: AtomicUsize,
    first_entered: Semaphore,
    first_release: Semaphore,
    first_wire: Arc<TestWire>,
    replacement_wire: Arc<TestWire>,
}

#[async_trait]
impl WireBuilder for SupersedableFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 {
            self.first_entered.add_permits(1);
            self.first_release
                .acquire()
                .await
                .expect("first release semaphore")
                .forget();
            Ok(vec![self.first_wire.clone()])
        } else {
            Ok(vec![self.replacement_wire.clone()])
        }
    }
}

async fn wait_for_entry(semaphore: &Semaphore, description: &str) {
    tokio::time::timeout(Duration::from_secs(2), semaphore.acquire())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {description}"))
        .expect("entry semaphore")
        .forget();
}

#[tokio::test]
async fn inv8_blackbox_independent_destination_flights_are_not_serialized() {
    let _guard = resource_test_lock().lock().await;
    let factory = Arc::new(IndependentDestinationFactory {
        calls: AtomicUsize::new(0),
        host_entered: Semaphore::new(0),
        host_release: Semaphore::new(0),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let host = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .send(&Dest::host(), PayloadType::RpcReliable, b"host")
                .await
        })
    };
    wait_for_entry(&factory.host_entered, "blocked host destination").await;
    let workload = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .send(&Dest::workload(), PayloadType::RpcReliable, b"workload")
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(2), workload)
        .await
        .expect("independent flight")
        .expect("workload task")
        .expect("workload send");
    assert!(!host.is_finished());
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    factory.host_release.add_permits(1);
    host.await.expect("host task").expect("host send");
    manager.close_all().await.expect("transport cleanup");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inv12_blackbox_stale_signaling_generation_cannot_publish_connected_or_observer() {
    let _guard = resource_test_lock().lock().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let server_url = format!("ws://{}/signaling/ws", listener.local_addr().unwrap());
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("WebSocket handshake");
        let _ = finish_rx.await;
        drop(websocket);
    });
    let client = Arc::new(WebSocketSignalingClient::new(SignalingConfig {
        server_url: url::Url::parse(&server_url).unwrap(),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig {
            enabled: false,
            ..ReconnectConfig::default()
        },
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }));
    let publish = Arc::new(Barrier::new(2));
    client.set_connection_publish_barrier_for_test(publish.clone());
    let connected_hooks = Arc::new(AtomicUsize::new(0));
    let hook: HookCallback = Arc::new({
        let connected_hooks = connected_hooks.clone();
        move |event| {
            let connected_hooks = connected_hooks.clone();
            Box::pin(async move {
                if matches!(event, HookEvent::SignalingConnected) {
                    connected_hooks.fetch_add(1, Ordering::AcqRel);
                }
            })
        }
    });
    client.set_hook_callback(hook);
    let mut events = client.subscribe_events();
    let connecting = {
        let client = client.clone();
        tokio::spawn(async move { client.connect().await })
    };
    rendezvous(&publish, "signaling pre-publication boundary").await;
    client.invalidate_generation();
    rendezvous(&publish, "signaling publication release").await;
    connecting
        .await
        .expect("connect task")
        .expect_err("invalidated generation must not commit");
    assert!(!client.is_connected());
    assert_eq!(connected_hooks.load(Ordering::Acquire), 0);
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, SignalingEvent::Connected));
    }
    finish_tx.send(()).expect("server cleanup");
    server.await.expect("server task");
}

#[tokio::test]
async fn inv13_blackbox_cancelled_creator_releases_ownership_without_erasing_replacement() {
    let _guard = resource_test_lock().lock().await;
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_entered: Semaphore::new(0),
        first_release: Semaphore::new(0),
        first_wire: Arc::new(TestWire::new()),
        replacement_wire: Arc::new(TestWire::new()),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();
    let creator = {
        let manager = manager.clone();
        let dest = dest.clone();
        tokio::spawn(async move {
            manager
                .send(&dest, PayloadType::RpcReliable, b"first")
                .await
        })
    };
    wait_for_entry(&factory.first_entered, "first destination creator").await;
    creator.abort();
    assert!(creator.await.unwrap_err().is_cancelled());
    tokio::time::timeout(
        Duration::from_secs(2),
        manager.send(&dest, PayloadType::RpcReliable, b"replacement"),
    )
    .await
    .expect("replacement deadline")
    .expect("replacement send");
    manager
        .send(&dest, PayloadType::RpcReliable, b"reuse")
        .await
        .expect("reuse");
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    manager.close_all().await.expect("transport cleanup");
}

#[tokio::test]
async fn inv14_blackbox_only_current_destination_flight_can_commit_transport_state() {
    let _guard = resource_test_lock().lock().await;
    let first_wire = Arc::new(TestWire::new());
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_entered: Semaphore::new(0),
        first_release: Semaphore::new(0),
        first_wire: first_wire.clone(),
        replacement_wire: Arc::new(TestWire::new()),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();
    let stale = {
        let manager = manager.clone();
        let dest = dest.clone();
        tokio::spawn(async move {
            manager
                .send(&dest, PayloadType::RpcReliable, b"stale")
                .await
        })
    };
    wait_for_entry(&factory.first_entered, "stale destination flight").await;
    manager.close_transport(&dest).await.expect("supersede");
    manager
        .send(&dest, PayloadType::RpcReliable, b"replacement")
        .await
        .expect("replacement");
    factory.first_release.add_permits(1);
    stale.await.expect("stale task").expect_err("stale commit");
    manager
        .send(&dest, PayloadType::RpcReliable, b"reuse")
        .await
        .expect("reuse");
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    assert_eq!(first_wire.close_count(), 1);
    manager.close_all().await.expect("transport cleanup");
}

async fn harness_with_two_peers() -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness
}

async fn connected_harness() -> TestHarness {
    let mut harness = harness_with_two_peers().await;
    harness.connect(100, 200).await;
    harness
}

fn arm_snapshot_waiters(
    rx: &watch::Receiver<actr_framework::WebRtcPeerState>,
    count: usize,
) -> Vec<tokio::task::JoinHandle<actr_framework::WebRtcPeerState>> {
    let revision = rx.borrow().revision;
    (0..count)
        .map(|_| {
            let mut rx = rx.clone();
            tokio::spawn(async move {
                loop {
                    let state = rx.borrow().clone();
                    if state.revision > revision {
                        return state;
                    }
                    rx.changed().await.expect("snapshot sender");
                }
            })
        })
        .collect()
}

fn arm_sendability_waiters(
    rx: &watch::Receiver<actr_framework::WebRtcPeerState>,
    count: usize,
    sendable: bool,
) -> Vec<tokio::task::JoinHandle<actr_framework::WebRtcPeerState>> {
    let revision = rx.borrow().revision;
    (0..count)
        .map(|_| {
            let mut rx = rx.clone();
            tokio::spawn(async move {
                loop {
                    let state = rx.borrow().clone();
                    if state.revision > revision && state.sendable == sendable {
                        return state;
                    }
                    rx.changed().await.expect("snapshot sender");
                }
            })
        })
        .collect()
}

async fn collect_snapshot_waiters(
    waiters: Vec<tokio::task::JoinHandle<actr_framework::WebRtcPeerState>>,
) -> Vec<actr_framework::WebRtcPeerState> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut states = Vec::new();
        for waiter in waiters {
            states.push(waiter.await.expect("snapshot waiter"));
        }
        states
    })
    .await
    .expect("all snapshot waiters must wake")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv15_blackbox_close_linearizes_late_successful_peer_publication() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let target = harness.peer(200).id.clone();
    let rtc = harness.peer(100).rtc.clone();

    let publish = Arc::new(Barrier::new(2));
    rtc.set_peer_publish_barrier(target.clone(), publish.clone());
    let connecting = {
        let rtc = rtc.clone();
        let target = target.clone();
        tokio::spawn(async move { rtc.connect(target).await })
    };

    rendezvous(&publish, "RTC controller pre-publication boundary").await;
    rtc.close_all().await;
    rendezvous(&publish, "late RTC controller publication release").await;
    connecting.await.expect("connect race task");

    let remaining = rtc.state(&target).await;
    assert!(
        remaining.is_none(),
        "close-all epoch must reject late controller publication, got {remaining:?}"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv16_blackbox_transport_creation_racing_close_all_cannot_deadlock() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let target = harness.peer(200).id.clone();
    let rtc = harness.peer(100).rtc.clone();
    tokio::time::timeout(Duration::from_secs(12), async {
        for _ in 0..16 {
            let connect = rtc.connect(target.clone());
            let close = rtc.close_all();
            tokio::join!(connect, close);
            rtc.close_all().await;
            assert_eq!(rtc.peer_count().await, 0);
        }
    })
    .await
    .expect("create/close-all race must remain bounded");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_data_channel_open_and_closed_wake_every_waiter() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let rx = harness.peer(100).rtc.subscribe(&target).await.unwrap();

    let recovery_waiters = arm_sendability_waiters(&rx, 8, false);
    harness.peer(100).rtc.network_unavailable();
    let states = collect_snapshot_waiters(recovery_waiters).await;
    assert!(states.iter().all(|state| !state.sendable));

    let open_waiters = arm_sendability_waiters(&rx, 8, true);
    harness.peer(100).rtc.network_available();
    harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(15))
        .await;
    assert!(!collect_snapshot_waiters(open_waiters).await.is_empty());

    let close_waiters = arm_sendability_waiters(&rx, 8, false);
    harness.peer(100).rtc.close_peer(&target).await;
    harness.peer(200).rtc.close_peer(&source).await;
    let states = collect_snapshot_waiters(close_waiters).await;
    assert!(states.iter().all(|state| !state.sendable));
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_connection_teardown_releases_waiters_on_a_never_opened_channel() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let target = harness.peer(200).id.clone();
    harness.server.pause_forwarding();
    harness.peer(100).rtc.connect(target.clone()).await;
    let rx = loop {
        if let Some(rx) = harness.peer(100).rtc.subscribe(&target).await {
            break rx;
        }
        tokio::task::yield_now().await;
    };
    assert!(!rx.borrow().sendable);
    let waiters = arm_snapshot_waiters(&rx, 8);
    assert!(harness.peer(100).rtc.close_peer(&target).await);
    let states = collect_snapshot_waiters(waiters).await;
    assert!(states.iter().all(|state| !state.sendable));
    harness.server.resume_forwarding();
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_registration_after_teardown_is_refused_and_closes_the_channel() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let mut rx = harness.peer(100).rtc.subscribe(&target).await.unwrap();
    assert!(harness.peer(100).rtc.close_peer(&target).await);
    harness.peer(200).rtc.close_peer(&source).await;
    while rx.borrow().status != actr_framework::WebRtcPeerStatus::Idle {
        if rx.changed().await.is_err() {
            break;
        }
    }
    assert!(!rx.borrow().sendable);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(harness.peer(100).rtc.state(&target).await.is_none());
    assert_eq!(harness.peer(100).rtc.peer_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_registration_overlapping_teardown_cannot_repopulate_cleared_caches() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let rtc = harness.peer(100).rtc.clone();
    let reconnect = tokio::spawn({
        let rtc = rtc.clone();
        async move {
            rtc.force_reconnect();
        }
    });
    let close = tokio::spawn({
        let rtc = rtc.clone();
        let target = target.clone();
        async move { rtc.close_peer(&target).await }
    });
    reconnect.await.expect("recovery task");
    close.await.expect("close task");
    rtc.close_peer(&target).await;
    harness.peer(200).rtc.close_peer(&source).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(rtc.state(&target).await.is_none());
    assert_eq!(rtc.peer_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_teardown_releases_waiters_on_every_payload_type_under_one_deadline() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let gate = harness.peer(100).gate.clone();

    gate.send_message_with_type(
        &target,
        PayloadType::RpcSignal,
        RpcEnvelope {
            request_id: "rpc-signal-lane".into(),
            direction: Some(Direction::Tell as i32),
            ..Default::default()
        },
    )
    .await
    .expect("RpcSignal lane");
    for lane in [PayloadType::StreamReliable, PayloadType::StreamLatencyFirst] {
        gate.send_data_chunk(
            &target,
            lane,
            lane.as_str_name(),
            Bytes::from_static(b"lane-probe"),
            lane.as_str_name(),
        )
        .await
        .expect("stream lane");
    }

    let rx = harness.peer(100).rtc.subscribe(&target).await.unwrap();
    let waiters = arm_snapshot_waiters(&rx, 4 * 8);
    let started = tokio::time::Instant::now();
    harness.peer(100).rtc.close_peer(&target).await;
    harness.peer(200).rtc.close_peer(&source).await;
    let states = collect_snapshot_waiters(waiters).await;
    assert_eq!(states.len(), 32);
    assert!(states.iter().all(|state| !state.sendable));
    assert!(started.elapsed() < Duration::from_secs(5));
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv18_blackbox_initial_readiness_cannot_lose_a_transition_after_subscription() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.connect(target.clone()).await;
    let mut rx = loop {
        if let Some(rx) = harness.peer(100).rtc.subscribe(&target).await {
            break rx;
        }
        tokio::task::yield_now().await;
    };
    tokio::time::timeout(Duration::from_secs(15), async {
        while !rx.borrow().sendable {
            rx.changed().await.expect("snapshot stream");
        }
    })
    .await
    .expect("initial readiness edge");
    assert_eq!(
        rx.borrow().status,
        actr_framework::WebRtcPeerStatus::Connected
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_reaper_never_pairs_a_new_state_with_the_previous_timestamp() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let before = harness.peer(100).rtc.state(&target).await.unwrap();
    harness.peer(100).rtc.network_unavailable();
    let recovering = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("Recovering state");
    assert!(recovering.revision > before.revision);
    harness.peer(100).rtc.network_available();
    let ready = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(15))
        .await;
    assert!(ready.revision > recovering.revision);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_reaper_resynchronizes_a_terminal_state_whose_callback_was_lost() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old = harness.peer(100).rtc.generation(&target).await.unwrap();
    harness.peer(100).rtc.force_reconnect();
    harness.peer(200).rtc.force_reconnect();
    let ready = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old
        })
        .await
        .expect("hard recovery replacement");
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness
            .peer(100)
            .rtc
            .state(&target)
            .await
            .unwrap()
            .pc_generation,
        ready.pc_generation
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_idle_tick_discovers_a_lost_terminal_transition() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let mut rx = harness.peer(100).rtc.subscribe(&target).await.unwrap();
    assert!(harness.peer(100).rtc.close_peer(&target).await);
    harness.peer(200).rtc.close_peer(&source).await;
    while rx.borrow().status != actr_framework::WebRtcPeerStatus::Idle {
        if rx.changed().await.is_err() {
            break;
        }
    }
    assert!(!rx.borrow().sendable);
    assert_eq!(harness.peer(100).rtc.peer_count().await, 0);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_lost_open_event_heals_instead_of_reaping_a_healthy_peer() {
    let _guard = resource_test_lock().lock().await;
    let harness = connected_harness().await;
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.network_unavailable();
    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("Recovering state");
    harness.peer(100).rtc.network_available();
    let ready = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(15))
        .await;
    assert!(ready.sendable);
    let response = harness
        .peer(100)
        .spawn_request(200, "healed-readiness", 5_000)
        .await
        .expect("request task")
        .expect("request after heal");
    assert_eq!(response.as_ref(), b"pong");
    harness.shutdown().await;
}

#[tokio::test]
async fn inv20_blackbox_empty_mailbox_storage_keeps_driving_inflight_reply_ack_tail() {
    let _guard = resource_test_lock().lock().await;
    let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let tail_completed = completed.clone();
    assert!(
        drive_mailbox_tail_while_storage_idle(async move {
            tail_completed.store(true, Ordering::Release);
        })
        .await
    );
    assert!(completed.load(Ordering::Acquire));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv22_blackbox_success_event_cancels_failure_deadline_without_polling() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let target = harness.peer(200).id.clone();
    harness.peer(100).rtc.connect(target.clone()).await;
    let timer_id = "webrtc.coordinator.peer_sendable";
    let before = timer_observation(timer_id);
    let waiter = {
        let rtc = harness.peer(100).rtc.clone();
        let target = target.clone();
        tokio::spawn(async move { rtc.wait_sendable(&target, Duration::from_secs(20)).await })
    };
    let ready = waiter.await.expect("sendability waiter");
    assert!(ready.sendable);
    let after = timer_observation(timer_id);
    assert!(after.armed > before.armed);
    assert_eq!(after.expired, before.expired);
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv23_blackbox_parallel_shutdown_uses_one_overall_child_deadline() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = TestHarness::new().await;
    for serial in [100, 200, 300] {
        harness.add_peer(serial).await;
    }
    harness.connect(100, 200).await;
    harness.connect(100, 300).await;
    assert_eq!(harness.peer(100).rtc.peer_count().await, 2);
    let started = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(10), harness.peer(100).rtc.close_all())
        .await
        .expect("close-all overall deadline");
    assert_eq!(harness.peer(100).rtc.peer_count().await, 0);
    assert!(started.elapsed() < Duration::from_secs(10));
    harness.shutdown().await;
}
