//! RFC-0400 resource-level invariant confirmations.
//!
//! These tests live in an integration-test crate and drive only the public
//! surface exposed by `actr-hyper` plus its `test-utils` feature. Race ordering
//! uses instance-scoped barriers or event generations; sleeps are never used
//! as synchronization.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use actr_hyper::test_support::{
    RealDataChannelPair, TestHarness, drive_mailbox_tail_while_storage_idle, timer_observation,
};
use actr_hyper::transport::{
    ConnType, ConnectionEvent, DataChannelStateSignal, DataLane, Dest, NetworkResult,
    PeerTransport, WireBuilder, WireHandle,
};
use actr_hyper::wire::webrtc::{
    HookCallback, HookEvent, ReconnectConfig, SignalingClient, SignalingConfig, SignalingEvent,
    WebSocketSignalingClient,
};
use actr_protocol::{ActrId, PayloadType};
use async_trait::async_trait;
use bytes::Bytes;
use rtc::data_channel::RTCDataChannelState;
use rtc::peer_connection::state::RTCPeerConnectionState;
use tokio::sync::{Barrier, Semaphore};

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
                .expect("host release semaphore should remain open")
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
                .expect("first release semaphore should remain open")
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
        .expect("entry semaphore should remain open")
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
        .expect("independent workload flight must finish")
        .expect("workload task should not panic")
        .expect("workload flight should succeed");
    assert!(
        !host.is_finished(),
        "the blocked host flight must remain independent"
    );
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);

    factory.host_release.add_permits(1);
    host.await
        .expect("host task should not panic")
        .expect("host flight should succeed");
    manager.close_all().await.expect("transport cleanup");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inv12_blackbox_stale_signaling_generation_cannot_publish_connected_or_observer() {
    let _guard = resource_test_lock().lock().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let server_url = format!(
        "ws://{}/signaling/ws",
        listener.local_addr().expect("listener address")
    );
    let (finish_server_tx, finish_server_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept signaling socket");
        let websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("complete WebSocket handshake");
        let _ = finish_server_rx.await;
        drop(websocket);
    });

    let client = Arc::new(WebSocketSignalingClient::new(SignalingConfig {
        server_url: url::Url::parse(&server_url).expect("valid signaling URL"),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig {
            enabled: false,
            ..ReconnectConfig::default()
        },
        auth_config: None,
        webrtc_role: None,
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
        tokio::spawn(async move { client.connect_once().await })
    };
    rendezvous(&publish, "signaling pre-publication boundary").await;
    client.invalidate_generation();
    rendezvous(&publish, "signaling publication release").await;

    connecting
        .await
        .expect("connect task should not panic")
        .expect_err("invalidated generation must not commit");
    assert!(!client.is_connected());
    assert_eq!(connected_hooks.load(Ordering::Acquire), 0);
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, SignalingEvent::Connected),
            "stale generation published Connected"
        );
    }

    finish_server_tx
        .send(())
        .expect("server should still be awaiting cleanup");
    server.await.expect("server task should finish");
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
    assert!(
        creator
            .await
            .expect_err("creator should be cancelled")
            .is_cancelled()
    );

    tokio::time::timeout(
        Duration::from_secs(2),
        manager.send(&dest, PayloadType::RpcReliable, b"replacement"),
    )
    .await
    .expect("replacement must not wait for the abandoned flight timeout")
    .expect("replacement should succeed");
    manager
        .send(&dest, PayloadType::RpcReliable, b"reuse")
        .await
        .expect("replacement must remain committed");
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
    manager
        .close_transport(&dest)
        .await
        .expect("close should supersede the first flight");
    manager
        .send(&dest, PayloadType::RpcReliable, b"replacement")
        .await
        .expect("replacement flight should commit");

    factory.first_release.add_permits(1);
    stale
        .await
        .expect("stale creator should not panic")
        .expect_err("stale creator must fail its compare-and-commit");
    manager
        .send(&dest, PayloadType::RpcReliable, b"reuse")
        .await
        .expect("replacement must remain current");
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        first_wire.close_count(),
        1,
        "wire produced by the stale flight must be closed"
    );
    manager.close_all().await.expect("transport cleanup");
}

async fn harness_with_two_peers() -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    harness
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv15_blackbox_close_linearizes_late_successful_peer_publication() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    let publish = Arc::new(Barrier::new(2));
    coordinator.set_peer_publish_barrier_for_test(target.clone(), publish.clone());

    let connecting = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move { coordinator.initiate_connection(&target).await })
    };
    rendezvous(&publish, "peer pre-publication boundary").await;
    coordinator
        .close_all_peers()
        .await
        .expect("close-all should establish the newer lifecycle epoch");
    rendezvous(&publish, "late peer publication release").await;

    connecting
        .await
        .expect("connection task should not panic")
        .expect_err("late peer publication must be rejected");
    assert!(coordinator.get_peer_session_id(&target).await.is_none());
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv16_blackbox_transport_creation_racing_close_all_cannot_deadlock() {
    let _guard = resource_test_lock().lock().await;
    let harness = harness_with_two_peers().await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    let publish = Arc::new(Barrier::new(2));
    coordinator.set_peer_publish_barrier_for_test(target.clone(), publish.clone());

    let connecting = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move { coordinator.initiate_connection(&target).await })
    };
    rendezvous(&publish, "racing peer creation boundary").await;
    tokio::time::timeout(Duration::from_secs(2), coordinator.close_all_peers())
        .await
        .expect("close-all must not deadlock behind a pre-commit creator")
        .expect("close-all should succeed");
    rendezvous(&publish, "racing peer creation release").await;
    connecting
        .await
        .expect("connection task should not panic")
        .expect_err("fenced creator must be rejected");
    assert!(coordinator.get_peer_session_id(&target).await.is_none());
    harness.shutdown().await;
}

async fn arm_state_waiters(signal: &DataChannelStateSignal) -> Vec<tokio::task::JoinHandle<()>> {
    const WAITERS: usize = 8;
    let observed = signal.snapshot();
    let armed = Arc::new(Barrier::new(WAITERS + 1));
    let waiters = (0..WAITERS)
        .map(|_| {
            let signal = signal.clone();
            let armed = armed.clone();
            tokio::spawn(async move {
                armed.wait().await;
                signal.wait_after(observed).await;
            })
        })
        .collect::<Vec<_>>();

    armed.wait().await;
    waiters
}

async fn assert_all_state_waiters_woke(waiters: Vec<tokio::task::JoinHandle<()>>) {
    tokio::time::timeout(
        Duration::from_secs(2),
        futures_util::future::join_all(waiters),
    )
    .await
    .expect("every state waiter must wake")
    .into_iter()
    .for_each(|result| result.expect("state waiter should not panic"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_data_channel_open_and_closed_wake_every_waiter() {
    let _guard = resource_test_lock().lock().await;
    let mut pair = RealDataChannelPair::new().await;
    let signal = pair.state_signal();

    let open_waiters = arm_state_waiters(&signal).await;
    pair.open().await;
    assert_all_state_waiters_woke(open_waiters).await;
    pair.lane()
        .send(Bytes::from_static(b"production-lane-open"))
        .await
        .expect("production lane should send after the real on_open callback");

    let close_waiters = arm_state_waiters(&signal).await;
    pair.close_data_channel().await;
    assert_all_state_waiters_woke(close_waiters).await;
    pair.lane()
        .send(Bytes::from_static(b"production-lane-closed"))
        .await
        .expect_err("production lane should observe the real on_close transition");
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_connection_teardown_releases_waiters_on_a_never_opened_channel() {
    let _guard = resource_test_lock().lock().await;
    let pair = RealDataChannelPair::new().await;
    let signal = pair.state_signal();

    // The lane's channel never opened: teardown must release its waiters
    // through the retained signal, not through the on_close callback that
    // teardown suppresses.
    let waiters = arm_state_waiters(&signal).await;
    let lane = pair.lane();
    let send =
        tokio::spawn(async move { lane.send(Bytes::from_static(b"parked-on-connecting")).await });

    pair.close_offer_connection().await;

    assert_all_state_waiters_woke(waiters).await;
    let result = tokio::time::timeout(Duration::from_secs(2), send)
        .await
        .expect("lane send must be released by teardown, not its failure timeout")
        .expect("lane send task should not panic");
    let error = result.expect_err("send on a torn-down connection must fail");
    assert!(
        error.is_closed_like(),
        "teardown must surface a closed-like error, got {error:?}"
    );
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_registration_after_teardown_is_refused_and_closes_the_channel() {
    let _guard = resource_test_lock().lock().await;
    let pair = RealDataChannelPair::new().await;
    pair.close_offer_connection().await;

    // A late outgoing lane creation must fail closed-like, not repopulate a
    // cleared cache with a lane whose sender would park until its timeout.
    let lane_error = pair
        .offer_get_lane(PayloadType::RpcReliable)
        .await
        .expect_err("lane creation after teardown must be refused");
    assert!(
        lane_error.is_closed_like(),
        "refused lane creation must be closed-like, got {lane_error:?}"
    );

    // A late incoming channel registration must fail closed-like and the
    // channel itself must be closed by the gate, not left dangling.
    let (channel, result) = pair
        .offer_register_fresh_channel(PayloadType::RpcReliable)
        .await;
    let register_error = result.expect_err("channel registration after teardown must be refused");
    assert!(
        register_error.is_closed_like(),
        "refused channel registration must be closed-like, got {register_error:?}"
    );
    assert!(
        matches!(
            channel.ready_state(),
            RTCDataChannelState::Closing | RTCDataChannelState::Closed
        ),
        "the refused channel must be closed by the gate, got {:?}",
        channel.ready_state()
    );
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_registration_overlapping_teardown_cannot_repopulate_cleared_caches() {
    let _guard = resource_test_lock().lock().await;
    let pair = RealDataChannelPair::new().await;

    // Park the registration after its handlers are installed and before the
    // first cache commit — the exact window a full teardown must interleave
    // into without the commit ever landing in a cleared cache.
    let channel = pair.mint_incoming_channel(PayloadType::RpcSignal).await;
    let commit = Arc::new(Barrier::new(2));
    pair.set_offer_registration_commit_barrier(PayloadType::RpcSignal, commit.clone());
    let register = pair.spawn_offer_register(channel.clone(), PayloadType::RpcSignal);

    rendezvous(&commit, "registration parked at commit boundary").await;
    pair.close_offer_connection().await;
    rendezvous(&commit, "registration commit release").await;

    let error = register
        .await
        .expect("registration task should not panic")
        .expect_err("registration committing after teardown must be refused");
    assert!(
        error.is_closed_like(),
        "refused overlapping registration must be closed-like, got {error:?}"
    );
    assert!(
        pair.offer_data_channel(PayloadType::RpcSignal)
            .await
            .is_none(),
        "a refused registration must not repopulate the cleared channel cache"
    );
    assert!(
        pair.offer_state_signal(PayloadType::RpcSignal)
            .await
            .is_none(),
        "a refused registration must not repopulate the cleared signal cache"
    );
    assert!(
        matches!(
            channel.ready_state(),
            RTCDataChannelState::Closing | RTCDataChannelState::Closed
        ),
        "the refused channel must be closed by the gate, got {:?}",
        channel.ready_state()
    );
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv17_blackbox_teardown_releases_waiters_on_every_payload_type_under_one_deadline() {
    let _guard = resource_test_lock().lock().await;
    let pair = RealDataChannelPair::new().await;

    // Lane waiters on every DataChannel payload type, all still Connecting:
    // teardown must release each of them — not just one — under a single
    // shared close deadline.
    let payload_types = [
        PayloadType::RpcReliable,
        PayloadType::RpcSignal,
        PayloadType::StreamReliable,
        PayloadType::StreamLatencyFirst,
    ];
    let mut waiters = Vec::new();
    let mut sends = Vec::new();
    for payload_type in payload_types {
        let lane = pair
            .offer_get_lane(payload_type)
            .await
            .expect("lane should be created on the open connection");
        let signal = pair
            .offer_state_signal(payload_type)
            .await
            .expect("created lane should expose its state signal");
        waiters.push(arm_state_waiters(&signal).await);
        let label = format!("parked-{payload_type:?}").into_bytes();
        sends.push(tokio::spawn(
            async move { lane.send(Bytes::from(label)).await },
        ));
    }

    pair.close_offer_connection().await;

    for waiters_on_type in waiters {
        assert_all_state_waiters_woke(waiters_on_type).await;
    }
    for send in sends {
        let result = tokio::time::timeout(Duration::from_secs(2), send)
            .await
            .expect("every lane send must be released by the shared teardown deadline")
            .expect("lane send task should not panic");
        let error = result.expect_err("send on a torn-down connection must fail");
        assert!(
            error.is_closed_like(),
            "teardown must surface a closed-like error, got {error:?}"
        );
    }
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv18_blackbox_initial_readiness_cannot_lose_a_transition_after_subscription() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    let subscribed = Arc::new(Barrier::new(2));
    coordinator
        .set_initial_readiness_subscribed_barrier_for_test(target.clone(), subscribed.clone());
    let mut events = coordinator.subscribe_events();
    let vnet = harness
        .vnet
        .as_ref()
        .expect("readiness test should use VNet")
        .clone();
    vnet.block_network();

    let connecting = tokio::spawn(async move {
        harness.connect(100, 200).await;
        harness
    });
    rendezvous(&subscribed, "readiness subscription boundary").await;
    vnet.unblock_network();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if matches!(
                events.recv().await,
                Ok(ConnectionEvent::DataChannelOpened { peer_id, .. }) if peer_id == target
            ) {
                return;
            }
        }
    })
    .await
    .expect("real DataChannel transition should occur while readiness is paused");
    rendezvous(&subscribed, "readiness state recheck release").await;

    let harness = connecting
        .await
        .expect("full-stack connection task should not panic");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_some(),
        "the transition that landed inside the production readiness window must be retained"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_reaper_never_pairs_a_new_state_with_the_previous_timestamp() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = harness_with_two_peers().await;
    harness.connect(100, 200).await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    coordinator.shutdown_background_tasks().await;

    // The age only needs to clear the 60s Closed/Failed reap threshold so the
    // regression is observable; staying close to it keeps the required
    // monotonic-clock history small, so freshly booted CI machines (uptime
    // under an hour) can still satisfy `Instant::now().checked_sub(age)`.
    let old_timestamp = coordinator
        .backdate_peer_state_for_test(&target, Duration::from_secs(65))
        .await
        .expect("connected peer should be backdatable");
    assert_eq!(
        coordinator
            .replay_current_peer_state_for_test(&target)
            .await
            .expect("duplicate state replay should succeed"),
        old_timestamp,
        "a duplicate state event must not extend stale-peer lifetime"
    );

    let state_publish = Arc::new(Barrier::new(2));
    coordinator.set_peer_state_transition_barrier_for_test(
        target.clone(),
        RTCPeerConnectionState::Closed,
        state_publish.clone(),
    );
    let closing = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move { coordinator.close_physical_peer_for_test(&target).await })
    };
    rendezvous(&state_publish, "real Closed callback publication boundary").await;

    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.check_stale_peers_for_test(),
    )
    .await
    .expect("cached state/timestamp snapshot must not trigger premature teardown");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_some(),
        "a newly visible physical state must not be paired with the old cached timestamp"
    );

    rendezvous(&state_publish, "Closed callback publication release").await;
    closing
        .await
        .expect("physical close task should not panic")
        .expect("physical close should finish");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_reaper_resynchronizes_a_terminal_state_whose_callback_was_lost() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = harness_with_two_peers().await;
    harness.connect(100, 200).await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    coordinator.shutdown_background_tasks().await;

    // Pause the real Closed callback BEFORE it publishes the cached state.
    // The cached value then stays Connected even though the physical
    // PeerConnection has already reached Closed — the same end state as a
    // callback that teardown replaced with a no-op or that never fired.
    let state_publish = Arc::new(Barrier::new(2));
    coordinator.set_peer_state_transition_barrier_for_test(
        target.clone(),
        RTCPeerConnectionState::Closed,
        state_publish.clone(),
    );
    let closing = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move { coordinator.close_physical_peer_for_test(&target).await })
    };
    rendezvous(&state_publish, "lost-callback Closed boundary").await;

    // First pass: the live/cached divergence is reconciled with a FRESH
    // timestamp, so the peer is resynchronized, not reaped.
    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.check_stale_peers_for_test(),
    )
    .await
    .expect("reaper pass should resynchronize, not reap");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_some(),
        "a freshly resynchronized terminal state must receive its full reap threshold"
    );

    // Once the reconciled Closed state ages past the threshold, the peer must
    // be reaped even though its callback is STILL paused and never published
    // the transition.
    coordinator
        .backdate_peer_state_for_test(&target, Duration::from_secs(65))
        .await
        .expect("resynchronized peer should be backdatable");
    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.check_stale_peers_for_test(),
    )
    .await
    .expect("reaper pass should reap the aged terminal peer");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_none(),
        "a peer whose terminal callback was lost must still be reaped"
    );

    rendezvous(&state_publish, "lost-callback release").await;
    closing
        .await
        .expect("physical close task should not panic")
        .expect("physical close should finish");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_idle_tick_discovers_a_lost_terminal_transition() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = harness_with_two_peers().await;
    harness.connect(100, 200).await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    coordinator.shutdown_background_tasks().await;

    // Production scheduling only: a fast idle tick drives every scan from
    // here on. No manual scans, and the Closed callback is parked so neither
    // a cached-state publication nor a ConnectionClosed event can drive
    // cleanup — without the idle tick the health task would sleep forever.
    let tick = "webrtc.coordinator.stale_peer_idle_tick";
    let before = timer_observation(tick);
    let health =
        coordinator.spawn_health_check_task_with_idle_interval_for_test(Duration::from_millis(50));

    // Wait until the idle timer is actually armed before starting the
    // physical close: if the health task's initial scan landed after the
    // close, it would reconcile immediately and arm a terminal deadline
    // instead, making the idle-tick assertions below race spuriously.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if timer_observation(tick).armed > before.armed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle tick must be armed before the terminal transition begins");

    let state_publish = Arc::new(Barrier::new(2));
    coordinator.set_peer_state_transition_barrier_for_test(
        target.clone(),
        RTCPeerConnectionState::Closed,
        state_publish.clone(),
    );
    let closing = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move { coordinator.close_physical_peer_for_test(&target).await })
    };
    rendezvous(&state_publish, "idle-tick Closed boundary").await;

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if timer_observation(tick).expired > before.expired {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle tick must keep scheduling reconciliation scans");

    // The lost Closed transition is resynchronized by the tick alone...
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if coordinator.cached_peer_state_for_test(&target).await
                == Some(RTCPeerConnectionState::Closed)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle tick must resynchronize the lost Closed transition");

    // ...and once the resynchronized state is aged past its threshold, the
    // deadline-driven wake reaps the peer with the callback STILL parked.
    coordinator
        .backdate_peer_state_for_test(&target, Duration::from_secs(65))
        .await
        .expect("resynchronized peer should be backdatable");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if coordinator.get_peer_session_id(&target).await.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a lost terminal transition must be reaped by the production health check");

    health.abort();
    rendezvous(&state_publish, "idle-tick release").await;
    closing
        .await
        .expect("physical close task should not panic")
        .expect("physical close should finish");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv19_blackbox_lost_open_event_heals_instead_of_reaping_a_healthy_peer() {
    let _guard = resource_test_lock().lock().await;
    // Freshly booted hosts may not have 121s of monotonic-clock history for
    // the never-ready backdate; skip rather than panic there (the rule is
    // pinned by the decision-table unit test).
    if std::time::Instant::now()
        .checked_sub(Duration::from_secs(121))
        .is_none()
    {
        eprintln!(
            "skipping inv19_blackbox_lost_open_event_heals: \
             host has <121s monotonic-clock history"
        );
        return;
    }
    let mut harness = harness_with_two_peers().await;
    harness.connect(100, 200).await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    coordinator.shutdown_background_tasks().await;

    // Model the DataChannelOpened event being lost to broadcast lag: the
    // connection is healthy and its DataChannels are genuinely open, but the
    // sticky readiness fact was never recorded.
    coordinator
        .clear_data_channel_opened_for_test(&target)
        .await
        .expect("peer should exist");
    assert_eq!(
        coordinator.data_channel_opened_flag_for_test(&target).await,
        Some(false)
    );
    coordinator
        .backdate_peer_state_for_test(&target, Duration::from_secs(121))
        .await
        .expect("peer should be backdatable");

    // The never-ready verdict must be revalidated against the live channels:
    // a genuinely open DataChannel heals the flag instead of reaping a
    // healthy, sendable connection.
    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.check_stale_peers_for_test(),
    )
    .await
    .expect("reaper pass should complete");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_some(),
        "a healthy peer whose open event was lost must not be reaped"
    );
    assert_eq!(
        coordinator.data_channel_opened_flag_for_test(&target).await,
        Some(true),
        "the reaper must heal the lost readiness fact from the live channels"
    );

    // The healed flag sticks: aging again must not reap either.
    coordinator
        .backdate_peer_state_for_test(&target, Duration::from_secs(121))
        .await
        .expect("peer should be backdatable");
    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.check_stale_peers_for_test(),
    )
    .await
    .expect("reaper pass should complete");
    assert!(
        coordinator.get_peer_session_id(&target).await.is_some(),
        "the healed readiness flag must stick"
    );
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
        .await,
        "the production idle select should report tail progress"
    );
    assert!(
        completed.load(Ordering::Acquire),
        "empty durable storage must not park an admitted reply/ack tail"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv22_blackbox_success_event_cancels_failure_deadline_without_polling() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = harness_with_two_peers().await;
    let coordinator = harness.peer(100).coordinator.clone();
    let target = harness.peer(200).id.clone();
    let subscribed = Arc::new(Barrier::new(2));
    coordinator.set_peer_sendable_subscribed_barrier_for_test(target.clone(), subscribed.clone());
    let timer_id = "webrtc.coordinator.peer_sendable";
    let before = timer_observation(timer_id);

    let waiter = {
        let coordinator = coordinator.clone();
        let target = target.clone();
        tokio::spawn(async move {
            coordinator
                .wait_for_peer_sendable(&target, Duration::from_secs(20))
                .await
        })
    };
    rendezvous(&subscribed, "successful wait subscription boundary").await;
    rendezvous(&subscribed, "successful wait deadline-arm release").await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if timer_observation(timer_id).armed > before.armed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("failure deadline should be armed before the success event");

    harness.connect(100, 200).await;
    assert_eq!(
        waiter.await.expect("sendability waiter should not panic"),
        coordinator.get_peer_session_id(&target).await,
        "success must complete directly from the event stream"
    );
    let after = timer_observation(timer_id);
    assert_eq!(after.armed, before.armed + 1);
    assert_eq!(
        after.expired, before.expired,
        "a success event must cancel rather than consume the failure deadline"
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inv23_blackbox_parallel_shutdown_uses_one_overall_child_deadline() {
    let _guard = resource_test_lock().lock().await;
    let mut harness = TestHarness::new().await;
    for serial in [100, 200, 300] {
        harness.add_peer(serial).await;
    }

    let coordinator = harness.peer(100).coordinator.clone();
    let hook_calls = Arc::new(AtomicUsize::new(0));
    let hook: HookCallback = Arc::new({
        let hook_calls = hook_calls.clone();
        move |event| {
            let hook_calls = hook_calls.clone();
            Box::pin(async move {
                if matches!(event, HookEvent::WebRtcDisconnected { .. }) {
                    hook_calls.fetch_add(1, Ordering::AcqRel);
                    std::future::pending::<()>().await;
                }
            })
        }
    });
    coordinator.set_hook_callback(hook);
    harness.connect(200, 100).await;
    harness.connect(300, 100).await;

    let started = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(4), coordinator.close_all_peers())
        .await
        .expect("close-all must stay within one bounded shutdown window")
        .expect("hook deadlines should not fail physical teardown");
    let elapsed = started.elapsed();

    assert_eq!(hook_calls.load(Ordering::Acquire), 2);
    assert!(
        elapsed < Duration::from_secs(2),
        "two stalled children must share one one-second deadline, elapsed={elapsed:?}"
    );
    harness.shutdown().await;
}
