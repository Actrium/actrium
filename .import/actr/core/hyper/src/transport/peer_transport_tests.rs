use super::*;
use crate::transport::ConnType;
use crate::transport::lane::DataLane;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, timeout};

struct TestFactory;

#[async_trait]
impl WireBuilder for TestFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        // Test factory: returns empty list (real usage requires actual connections)
        Ok(vec![])
    }
}

fn create_test_factory() -> Arc<dyn WireBuilder> {
    Arc::new(TestFactory)
}

#[tokio::test]
async fn test_transport_manager_creation() {
    let local_id = ActrId::default();
    let factory = create_test_factory();
    let mgr = PeerTransport::new(local_id.clone(), factory);

    assert_eq!(mgr.dest_count().await, 0);
    assert_eq!(mgr.local_id(), &local_id);
}

#[tokio::test]
async fn test_list_dests() {
    let local_id = ActrId::default();
    let factory = create_test_factory();
    let mgr = PeerTransport::new(local_id, factory);

    let dests = mgr.list_dests().await;
    assert_eq!(dests.len(), 0);
}

#[tokio::test]
async fn test_has_dest() {
    let local_id = ActrId::default();
    let factory = create_test_factory();
    let mgr = PeerTransport::new(local_id, factory);

    let dest = Dest::host();
    assert!(!mgr.has_dest(&dest).await);
}

#[tokio::test]
async fn close_transport_if_current_replaced_instance_does_not_mark_closing() {
    let local_id = ActrId::default();
    let factory = create_test_factory();
    let mgr = PeerTransport::new(local_id, factory);
    let dest = Dest::host();

    let old_transport = Arc::new(
        DestTransport::new(dest.clone(), vec![])
            .await
            .expect("old transport should be created"),
    );
    let current_transport = Arc::new(
        DestTransport::new(dest.clone(), vec![])
            .await
            .expect("current transport should be created"),
    );
    let old_ref = DestTransportRef::new(&old_transport, None);

    mgr.transports
        .write()
        .await
        .insert(dest.clone(), DestState::Connected(current_transport));

    let closed = mgr
        .close_transport_if_current(&dest, &old_ref)
        .await
        .expect("stale instance close should not fail");

    assert!(!closed);
    assert_eq!(mgr.dest_count().await, 1);
    assert!(
        !mgr.is_closing(&dest).await,
        "stale no-op close must not mark the replacement transport closing"
    );
}

// ── close_webrtc_transport_if_session: WebRTC-only teardown, WS kept alive ──

/// Minimal wire that succeeds on `connect`, counts `close` calls, and optionally
/// carries a `WireIdentity` (so `matches_webrtc_session` can match).
#[derive(Debug)]
struct CountingWire {
    conn_type: ConnType,
    identity: Option<WireIdentity>,
    close_count: Arc<AtomicUsize>,
    close_notify: Notify,
}

#[derive(Debug)]
struct FailingCloseWire {
    close_count: AtomicUsize,
}

#[async_trait]
impl WireHandle for FailingCloseWire {
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
        self.close_count.fetch_add(1, Ordering::SeqCst);
        Err(NetworkError::ConnectionError(
            "forced close failure".to_string(),
        ))
    }

    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(Arc::new(NoopLane))
    }
}

struct SingleWireFactory {
    wire: Arc<CountingWire>,
    calls: AtomicUsize,
}

#[async_trait]
impl WireBuilder for SingleWireFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![self.wire.clone()])
    }
}

impl CountingWire {
    fn new(conn_type: ConnType, identity: Option<WireIdentity>) -> Self {
        Self {
            conn_type,
            identity,
            close_count: Arc::new(AtomicUsize::new(0)),
            close_notify: Notify::new(),
        }
    }

    fn close_count(&self) -> usize {
        self.close_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl WireHandle for CountingWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }

    fn priority(&self) -> u8 {
        match self.conn_type {
            ConnType::WebSocket => 0,
            ConnType::WebRTC => 1,
        }
    }

    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn close(&self) -> NetworkResult<()> {
        self.close_count.fetch_add(1, Ordering::Relaxed);
        self.close_notify.notify_waiters();
        Ok(())
    }

    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(Arc::new(NoopLane))
    }

    fn identity(&self) -> Option<WireIdentity> {
        self.identity.clone()
    }
}

#[derive(Debug)]
struct NoopLane;

#[async_trait]
impl DataLane for NoopLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        Ok(())
    }

    fn lane_type(&self) -> &'static str {
        "noop"
    }
}

async fn wait_for_ready(transport: &DestTransport, conn_type: ConnType) {
    let mut ready = transport.watch_ready();
    timeout(Duration::from_secs(1), async {
        loop {
            if ready.borrow().contains(&conn_type) {
                return;
            }
            ready
                .changed()
                .await
                .expect("ready watch should remain open");
        }
    })
    .await
    .expect("wire should become ready");
}

#[tokio::test]
async fn close_error_clears_closing_and_close_all_drains_every_state() {
    let replacement_wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SingleWireFactory {
        wire: replacement_wire.clone(),
        calls: AtomicUsize::new(0),
    });
    let manager = PeerTransport::new(ActrId::default(), factory.clone());
    let dest = Dest::host();

    let failing_wire = Arc::new(FailingCloseWire {
        close_count: AtomicUsize::new(0),
    });
    let failing_transport = Arc::new(
        DestTransport::new(dest.clone(), vec![failing_wire.clone()])
            .await
            .expect("failing-close transport should construct"),
    );
    wait_for_ready(&failing_transport, ConnType::WebSocket).await;
    manager
        .transports
        .write()
        .await
        .insert(dest.clone(), DestState::Connected(failing_transport));

    let error = manager
        .close_transport(&dest)
        .await
        .expect_err("physical close failure should be reported");
    assert!(matches!(error, NetworkError::ConnectionError(_)));
    assert_eq!(failing_wire.close_count.load(Ordering::SeqCst), 1);
    assert!(
        !manager.is_closing(&dest).await,
        "an error must not leave the destination permanently marked closing"
    );
    assert_eq!(manager.dest_count().await, 0);

    let replacement = manager
        .get_or_create_transport(&dest)
        .await
        .expect("destination should be recreatable after failed close");
    wait_for_ready(&replacement, ConnType::WebSocket).await;
    assert_eq!(factory.calls.load(Ordering::SeqCst), 1);

    let connecting_dest = Dest::workload();
    let connecting_flight = Arc::new(ConnectionFlight::new());
    manager.transports.write().await.insert(
        connecting_dest.clone(),
        DestState::Connecting(connecting_flight.clone()),
    );

    manager.close_all().await.expect("close_all is best effort");
    assert_eq!(replacement_wire.close_count(), 1);
    assert!(connecting_flight.cancel.is_cancelled());
    assert_eq!(manager.dest_count().await, 0);
    assert!(!manager.is_closing(&dest).await);
    assert!(!manager.is_closing(&connecting_dest).await);
}

struct SupersedableFactory {
    calls: AtomicUsize,
    first_started: Notify,
    first_release: Notify,
    first_wire: Arc<CountingWire>,
    replacement_wire: Arc<CountingWire>,
}

struct IndependentDestinationFactory {
    calls: AtomicUsize,
    host_started: Notify,
    host_release: Notify,
}

struct ExistingConnectionFactory {
    normal_calls: AtomicUsize,
    existing_calls: AtomicUsize,
    wire: Arc<CountingWire>,
}

#[async_trait]
impl WireBuilder for ExistingConnectionFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.normal_calls.fetch_add(1, Ordering::AcqRel);
        Ok(vec![self.wire.clone()])
    }

    async fn existing_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.existing_calls.fetch_add(1, Ordering::AcqRel);
        Ok(vec![self.wire.clone()])
    }
}

struct FailingExistingConnectionFactory;

#[async_trait]
impl WireBuilder for FailingExistingConnectionFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        panic!("normal connection creation must not run for existing-only admission")
    }

    async fn existing_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        Err(NetworkError::ConnectionError(
            "reusable connection lookup failed".to_string(),
        ))
    }
}

#[async_trait]
impl WireBuilder for IndependentDestinationFactory {
    async fn create_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if dest == &Dest::host() {
            self.host_started.notify_one();
            self.host_release.notified().await;
        }
        Ok(vec![Arc::new(CountingWire::new(ConnType::WebSocket, None))])
    }
}

#[tokio::test]
async fn existing_only_send_does_not_start_a_physical_connection() {
    let factory = Arc::new(IndependentDestinationFactory {
        calls: AtomicUsize::new(0),
        host_started: Notify::new(),
        host_release: Notify::new(),
    });
    let manager = PeerTransport::new(ActrId::default(), factory.clone());

    let sent = manager
        .send_existing_with_identity(&Dest::workload(), PayloadType::RpcReliable, b"payload")
        .await
        .expect("absence is an admission result, not a transport error");

    assert!(sent.is_none());
    assert_eq!(factory.calls.load(Ordering::Acquire), 0);
    assert_eq!(manager.dest_count().await, 0);
}

#[tokio::test]
async fn existing_only_send_uses_a_committed_destination_without_rebuilding() {
    let factory = Arc::new(IndependentDestinationFactory {
        calls: AtomicUsize::new(0),
        host_started: Notify::new(),
        host_release: Notify::new(),
    });
    let manager = PeerTransport::new(ActrId::default(), factory.clone());
    let dest = Dest::workload();
    manager
        .get_or_create_transport(&dest)
        .await
        .expect("normal admission should establish the destination");
    assert_eq!(factory.calls.load(Ordering::Acquire), 1);

    let sent = manager
        .send_existing_with_identity(&dest, PayloadType::RpcReliable, b"payload")
        .await
        .expect("committed destination send should succeed");

    assert!(sent.is_some());
    assert_eq!(
        factory.calls.load(Ordering::Acquire),
        1,
        "existing-only send must not invoke the builder again"
    );
}

#[tokio::test]
async fn existing_only_send_materializes_an_inbound_physical_connection() {
    let wire = Arc::new(CountingWire::new(
        ConnType::WebRTC,
        Some(WireIdentity::WebRtc {
            peer_id: ActrId::default(),
            session_id: 42,
        }),
    ));
    let factory = Arc::new(ExistingConnectionFactory {
        normal_calls: AtomicUsize::new(0),
        existing_calls: AtomicUsize::new(0),
        wire,
    });
    let manager = PeerTransport::new(ActrId::default(), factory.clone());
    let dest = Dest::workload();

    let sent = manager
        .send_existing_with_identity(&dest, PayloadType::RpcReliable, b"response")
        .await
        .expect("an existing inbound connection should be reusable");

    assert!(sent.is_some());
    assert_eq!(factory.normal_calls.load(Ordering::Acquire), 0);
    assert_eq!(factory.existing_calls.load(Ordering::Acquire), 1);
    assert_eq!(manager.dest_count().await, 1);
    manager.close_all().await.expect("cleanup should succeed");
}

#[tokio::test]
async fn existing_only_lookup_error_clears_its_materialization_flight() {
    let manager = PeerTransport::new(
        ActrId::default(),
        Arc::new(FailingExistingConnectionFactory),
    );
    let dest = Dest::workload();

    let error = match manager
        .send_existing_with_identity(&dest, PayloadType::RpcReliable, b"response")
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("lookup errors must be propagated"),
    };

    assert!(matches!(error, NetworkError::ConnectionError(_)));
    assert_eq!(manager.dest_count().await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_supersedes_existing_only_materialization_before_commit() {
    let wire = Arc::new(CountingWire::new(ConnType::WebRTC, None));
    let factory = Arc::new(ExistingConnectionFactory {
        normal_calls: AtomicUsize::new(0),
        existing_calls: AtomicUsize::new(0),
        wire: wire.clone(),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    manager.set_final_commit_barrier(barrier.clone());
    let dest = Dest::workload();

    let sender_manager = manager.clone();
    let sender_dest = dest.clone();
    let sender = tokio::spawn(async move {
        sender_manager
            .send_existing_with_identity(&sender_dest, PayloadType::RpcReliable, b"response")
            .await
    });

    barrier.wait().await;
    manager
        .close_transport(&dest)
        .await
        .expect("close should cancel the exact materialization flight");
    let close_observed = wire.close_notify.notified();
    tokio::pin!(close_observed);
    close_observed.as_mut().enable();
    barrier.wait().await;

    let sent = sender
        .await
        .expect("sender task should not panic")
        .expect("superseded materialization is an admission miss");
    assert!(sent.is_none());
    timeout(Duration::from_secs(1), &mut close_observed)
        .await
        .expect("the uncommitted reusable wire must be closed");
    assert_eq!(manager.dest_count().await, 0);
}

#[tokio::test]
async fn existing_only_send_does_not_wait_for_an_inflight_destination() {
    let factory = Arc::new(IndependentDestinationFactory {
        calls: AtomicUsize::new(0),
        host_started: Notify::new(),
        host_release: Notify::new(),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();

    let creator_manager = manager.clone();
    let creator_dest = dest.clone();
    let creator =
        tokio::spawn(async move { creator_manager.get_or_create_transport(&creator_dest).await });
    timeout(Duration::from_secs(1), factory.host_started.notified())
        .await
        .expect("destination flight should enter the builder");

    let sent = timeout(
        Duration::from_secs(1),
        manager.send_existing_with_identity(&dest, PayloadType::RpcReliable, b"payload"),
    )
    .await
    .expect("existing-only admission must not wait on the flight")
    .expect("connecting state is not a transport error");

    assert!(sent.is_none());
    assert_eq!(factory.calls.load(Ordering::Acquire), 1);

    factory.host_release.notify_one();
    creator
        .await
        .expect("creator task should not panic")
        .expect("creator should finish after release");
}

#[tokio::test]
async fn inv8_independent_destination_flights_are_not_serialized() {
    let factory = Arc::new(IndependentDestinationFactory {
        calls: AtomicUsize::new(0),
        host_started: Notify::new(),
        host_release: Notify::new(),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));

    let host_manager = manager.clone();
    let host =
        tokio::spawn(async move { host_manager.get_or_create_transport(&Dest::host()).await });

    timeout(Duration::from_secs(1), factory.host_started.notified())
        .await
        .expect("host destination flight should enter the factory");

    let workload_manager = manager.clone();
    let workload = tokio::spawn(async move {
        workload_manager
            .get_or_create_transport(&Dest::workload())
            .await
    });

    timeout(Duration::from_secs(1), workload)
        .await
        .expect("workload flight should finish while the host flight remains blocked")
        .expect("workload task should not panic")
        .expect("workload flight should succeed");

    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    assert!(
        !host.is_finished(),
        "the slow host destination should remain independently blocked"
    );

    factory.host_release.notify_one();
    timeout(Duration::from_secs(1), host)
        .await
        .expect("host flight should finish")
        .expect("host task should not panic")
        .expect("host flight should succeed");
}

#[async_trait]
impl WireBuilder for SupersedableFactory {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 {
            self.first_started.notify_one();
            self.first_release.notified().await;
            Ok(vec![self.first_wire.clone()])
        } else {
            Ok(vec![self.replacement_wire.clone()])
        }
    }
}

#[tokio::test]
async fn inv14_only_current_destination_flight_can_commit_transport() {
    let first_wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let replacement_wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_started: Notify::new(),
        first_release: Notify::new(),
        first_wire: first_wire.clone(),
        replacement_wire,
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();

    let first_manager = manager.clone();
    let first_dest = dest.clone();
    let first =
        tokio::spawn(async move { first_manager.get_or_create_transport(&first_dest).await });
    timeout(Duration::from_secs(1), factory.first_started.notified())
        .await
        .expect("first flight should start");

    manager
        .close_transport(&dest)
        .await
        .expect("closing first flight should succeed");

    let replacement = timeout(
        Duration::from_secs(1),
        manager.get_or_create_transport(&dest),
    )
    .await
    .expect("replacement flight should not wait for stale creator")
    .expect("replacement flight should succeed");

    factory.first_release.notify_one();
    let stale_result = timeout(Duration::from_secs(1), first)
        .await
        .expect("stale creator should finish")
        .expect("stale creator task should not panic");
    assert!(
        stale_result.is_err(),
        "stale creator must not commit over replacement"
    );

    let current = manager
        .get_or_create_transport(&dest)
        .await
        .expect("current transport should remain available");
    assert!(Arc::ptr_eq(&current, &replacement));
    assert_eq!(factory.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        first_wire.close_count(),
        1,
        "transport produced by stale creator must be closed"
    );
}

#[tokio::test]
async fn inv13_cancelled_creator_releases_ownership_without_erasing_replacement() {
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_started: Notify::new(),
        first_release: Notify::new(),
        first_wire: Arc::new(CountingWire::new(ConnType::WebSocket, None)),
        replacement_wire: Arc::new(CountingWire::new(ConnType::WebSocket, None)),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();

    let first_manager = manager.clone();
    let first_dest = dest.clone();
    let first =
        tokio::spawn(async move { first_manager.get_or_create_transport(&first_dest).await });
    timeout(Duration::from_secs(1), factory.first_started.notified())
        .await
        .expect("first flight should start");

    first.abort();
    let _ = first.await;

    let replacement = timeout(
        Duration::from_secs(1),
        manager.get_or_create_transport(&dest),
    )
    .await
    .expect("replacement must wake from cancellation rather than the ten-second waiter timeout")
    .expect("replacement flight should succeed");
    assert_eq!(
        factory.calls.load(Ordering::Acquire),
        2,
        "replacement should own a fresh generation"
    );

    let current = manager
        .get_or_create_transport(&dest)
        .await
        .expect("replacement transport should remain committed");
    assert!(
        Arc::ptr_eq(&current, &replacement),
        "cancellation cleanup must not erase the replacement generation"
    );
}

#[tokio::test]
async fn cancelled_creator_inside_dest_transport_new_closes_created_wire() {
    let wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SingleWireFactory {
        wire: wire.clone(),
        calls: AtomicUsize::new(0),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory));
    let creation_barrier = Arc::new(tokio::sync::Barrier::new(2));
    manager.set_dest_transport_creation_barrier(creation_barrier.clone());
    let dest = Dest::host();

    let creator_manager = manager.clone();
    let creator_dest = dest.clone();
    let creator =
        tokio::spawn(async move { creator_manager.get_or_create_transport(&creator_dest).await });
    creation_barrier.wait().await;

    let flight = {
        let transports = manager.transports.read().await;
        match transports.get(&dest) {
            Some(DestState::Connecting(flight)) => flight.clone(),
            _ => panic!("creator should still own a connecting flight"),
        }
    };
    let mut flight_completed = Box::pin(flight.completion.notified());
    flight_completed.as_mut().enable();
    let mut wire_closed = Box::pin(wire.close_notify.notified());
    wire_closed.as_mut().enable();

    creator.abort();
    let _ = creator.await;
    timeout(Duration::from_secs(1), &mut flight_completed)
        .await
        .expect("creator cancellation should retire its flight");
    timeout(Duration::from_secs(1), &mut wire_closed)
        .await
        .expect("partial DestTransport cancellation should close its created wire");

    assert!(!manager.is_connecting(&dest).await);
    assert_eq!(manager.dest_count().await, 0);
    assert!(
        wire.close_count() >= 1,
        "a wire handed into a cancelled DestTransport constructor must not leak"
    );
}

#[tokio::test]
async fn cancelled_creator_at_final_commit_lock_closes_completed_transport() {
    let wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SingleWireFactory {
        wire: wire.clone(),
        calls: AtomicUsize::new(0),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory));
    let final_barrier = Arc::new(tokio::sync::Barrier::new(2));
    manager.set_final_commit_barrier(final_barrier.clone());
    let dest = Dest::host();

    let creator_manager = manager.clone();
    let creator_dest = dest.clone();
    let creator =
        tokio::spawn(async move { creator_manager.get_or_create_transport(&creator_dest).await });
    final_barrier.wait().await;

    let commit_lock = manager.transports.write().await;
    let flight = match commit_lock.get(&dest) {
        Some(DestState::Connecting(flight)) => flight.clone(),
        _ => panic!("completed creator should still own its pre-commit flight"),
    };
    let mut flight_completed = Box::pin(flight.completion.notified());
    flight_completed.as_mut().enable();
    let mut wire_closed = Box::pin(wire.close_notify.notified());
    wire_closed.as_mut().enable();

    // Let the creator leave the test hook and block on the exact write lock held
    // above, then cancel it with a fully-built DestTransport still uncommitted.
    final_barrier.wait().await;
    creator.abort();
    let _ = creator.await;
    drop(commit_lock);

    timeout(Duration::from_secs(1), &mut flight_completed)
        .await
        .expect("final-lock cancellation should retire its flight");
    timeout(Duration::from_secs(1), &mut wire_closed)
        .await
        .expect("an uncommitted completed transport must be physically closed");
    assert_eq!(manager.dest_count().await, 0);
    assert!(wire.close_count() >= 1);
}

#[tokio::test]
async fn flight_waiter_is_event_driven_and_cannot_miss_completion() {
    let first_wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_started: Notify::new(),
        first_release: Notify::new(),
        first_wire,
        replacement_wire: Arc::new(CountingWire::new(ConnType::WebSocket, None)),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();

    let creator_manager = manager.clone();
    let creator_dest = dest.clone();
    let creator =
        tokio::spawn(async move { creator_manager.get_or_create_transport(&creator_dest).await });
    timeout(Duration::from_secs(1), factory.first_started.notified())
        .await
        .expect("creator should start");

    let waiter_manager = manager.clone();
    let waiter_dest = dest.clone();
    let waiter =
        tokio::spawn(async move { waiter_manager.get_or_create_transport(&waiter_dest).await });
    factory.first_release.notify_one();

    let created = timeout(Duration::from_secs(1), creator)
        .await
        .expect("creator should finish")
        .expect("creator should not panic")
        .expect("creator should succeed");
    let observed = timeout(Duration::from_secs(1), waiter)
        .await
        .expect("waiter should wake from flight completion")
        .expect("waiter should not panic")
        .expect("waiter should succeed");

    assert!(Arc::ptr_eq(&created, &observed));
    assert_eq!(
        factory.calls.load(Ordering::Acquire),
        1,
        "waiter must share the existing flight"
    );
}

#[tokio::test(start_paused = true)]
async fn flight_waiter_does_not_expire_before_its_creator() {
    let first_wire = Arc::new(CountingWire::new(ConnType::WebSocket, None));
    let factory = Arc::new(SupersedableFactory {
        calls: AtomicUsize::new(0),
        first_started: Notify::new(),
        first_release: Notify::new(),
        first_wire,
        replacement_wire: Arc::new(CountingWire::new(ConnType::WebSocket, None)),
    });
    let manager = Arc::new(PeerTransport::new(ActrId::default(), factory.clone()));
    let dest = Dest::host();

    let creator_manager = manager.clone();
    let creator_dest = dest.clone();
    let creator =
        tokio::spawn(async move { creator_manager.get_or_create_transport(&creator_dest).await });
    factory.first_started.notified().await;

    let waiter_manager = manager.clone();
    let waiter_dest = dest.clone();
    let waiter =
        tokio::spawn(async move { waiter_manager.get_or_create_transport(&waiter_dest).await });
    tokio::task::yield_now().await;

    // The WebRTC factory legitimately owns a 30-second creation budget. A
    // follower of that exact flight must not invent a shorter ten-second
    // deadline and report failure while the owner is still making progress.
    tokio::time::advance(Duration::from_secs(10) + Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert!(
        !waiter.is_finished(),
        "a follower must share the creator flight deadline"
    );

    factory.first_release.notify_one();
    let created = creator
        .await
        .expect("creator task should not panic")
        .expect("creator should succeed within its flight budget");
    let observed = waiter
        .await
        .expect("waiter task should not panic")
        .expect("waiter should observe the creator result");
    assert!(Arc::ptr_eq(&created, &observed));
}

/// Build a `PeerTransport` whose `dest` maps to a ready `DestTransport` carrying
/// both a WebRTC wire (with the given session identity) and a WebSocket wire.
///
/// Returns the manager and the `dest` key; the caller already holds `Arc`s to
/// the two `CountingWire`s so it can assert on their `close_count` directly.
async fn peer_transport_with_webrtc_and_ws(
    peer_id: ActrId,
    session_id: u64,
    webrtc: Arc<CountingWire>,
    websocket: Arc<CountingWire>,
) -> (PeerTransport, Dest) {
    let mgr = PeerTransport::new(peer_id.clone(), create_test_factory());
    let dest = Dest::host();

    let transport = DestTransport::new(dest.clone(), vec![webrtc, websocket])
        .await
        .expect("transport should be created");

    // `DestTransport::new` spawns background `connect` tasks; wait for the
    // WebRTC wire to become Ready so `matches_webrtc_session` can observe it.
    let ready = timeout(Duration::from_secs(2), async {
        loop {
            if transport.matches_webrtc_session(&peer_id, session_id).await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "WebRTC wire should become Ready in time");

    mgr.transports
        .write()
        .await
        .insert(dest.clone(), DestState::Connected(Arc::new(transport)));

    (mgr, dest)
}

#[tokio::test]
async fn close_webrtc_transport_if_session_closes_only_webrtc_keeps_websocket() {
    let peer_id = ActrId::default();
    let session_id = 42u64;

    let webrtc = Arc::new(CountingWire::new(
        ConnType::WebRTC,
        Some(WireIdentity::WebRtc {
            peer_id: peer_id.clone(),
            session_id,
        }),
    ));
    let websocket = Arc::new(CountingWire::new(ConnType::WebSocket, None));

    let (mgr, dest) = peer_transport_with_webrtc_and_ws(
        peer_id.clone(),
        session_id,
        webrtc.clone(),
        websocket.clone(),
    )
    .await;

    let closed = mgr
        .close_webrtc_transport_if_session(&dest, &peer_id, session_id)
        .await
        .expect("session-matched close should not error");

    assert!(closed, "matching session should close the WebRTC wire");
    assert_eq!(
        webrtc.close_count(),
        1,
        "WebRTC wire should be closed exactly once"
    );
    assert_eq!(
        websocket.close_count(),
        0,
        "WebSocket fallback wire must NOT be closed by a WebRTC-only teardown"
    );
}

#[tokio::test]
async fn close_webrtc_transport_if_session_skips_on_stale_session() {
    let peer_id = ActrId::default();
    let active_session = 42u64;
    let stale_session = 7u64;

    let webrtc = Arc::new(CountingWire::new(
        ConnType::WebRTC,
        Some(WireIdentity::WebRtc {
            peer_id: peer_id.clone(),
            session_id: active_session,
        }),
    ));
    let websocket = Arc::new(CountingWire::new(ConnType::WebSocket, None));

    let (mgr, dest) = peer_transport_with_webrtc_and_ws(
        peer_id.clone(),
        active_session,
        webrtc.clone(),
        websocket.clone(),
    )
    .await;

    // A stale close event referencing a different session must be a no-op.
    let closed = mgr
        .close_webrtc_transport_if_session(&dest, &peer_id, stale_session)
        .await
        .expect("stale close should not error");

    assert!(!closed, "stale session close should be skipped");
    assert_eq!(
        webrtc.close_count(),
        0,
        "WebRTC wire must NOT be closed for a stale session"
    );
    assert_eq!(
        websocket.close_count(),
        0,
        "WebSocket wire must NOT be closed for a stale session"
    );
}
