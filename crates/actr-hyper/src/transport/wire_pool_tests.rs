use super::*;
use crate::transport::error::{NetworkError, NetworkResult};
use crate::transport::lane::DataLane;
use crate::transport::secure_channel::{SecureCell, SecureChannelState, SecureDataLane};
use actr_protocol::PayloadType;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Minimal WireHandle mock. `succeed` controls `connect()` outcome.
#[derive(Debug)]
struct MockWire {
    conn_type: ConnType,
    succeed: bool,
    identity: Option<WireIdentity>,
}

#[derive(Debug)]
struct BlockingWire {
    started: tokio::sync::Notify,
    closed: tokio::sync::Notify,
}

#[derive(Debug)]
struct CountingFailWire {
    attempts: std::sync::atomic::AtomicUsize,
    changed: tokio::sync::Notify,
}

#[derive(Debug)]
struct AdvancingLifecycleWire {
    lifecycle: std::sync::Mutex<crate::transport::WebRtcTransportLifecycle>,
}

#[derive(Debug)]
struct BlockingLifecycleWire {
    lifecycle: crate::transport::WebRtcTransportLifecycle,
    started: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl WireHandle for BlockingLifecycleWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebRTC
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.started.notify_one();
        std::future::pending().await
    }

    fn is_connected(&self) -> bool {
        false
    }

    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }

    fn webrtc_transport_lifecycle(&self) -> Option<crate::transport::WebRtcTransportLifecycle> {
        Some(self.lifecycle.clone())
    }
}

#[async_trait::async_trait]
impl WireHandle for AdvancingLifecycleWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebRTC
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }

    fn webrtc_transport_lifecycle(&self) -> Option<crate::transport::WebRtcTransportLifecycle> {
        Some(self.lifecycle.lock().expect("lifecycle lock").clone())
    }
}

#[async_trait::async_trait]
impl WireHandle for CountingFailWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.changed.notify_one();
        Err(NetworkError::ConnectionError("forced failure".into()))
    }

    fn is_connected(&self) -> bool {
        false
    }

    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }

    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
}

#[derive(Debug)]
struct CommitRaceWire {
    started: tokio::sync::Notify,
    release_connect: tokio::sync::Barrier,
    close_calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl WireHandle for BlockingWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.started.notify_one();
        std::future::pending().await
    }

    fn is_connected(&self) -> bool {
        false
    }

    async fn close(&self) -> NetworkResult<()> {
        self.closed.notify_one();
        Ok(())
    }

    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
}

#[async_trait::async_trait]
impl WireHandle for CommitRaceWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        self.started.notify_one();
        self.release_connect.wait().await;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn close(&self) -> NetworkResult<()> {
        self.close_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
}

#[async_trait::async_trait]
impl WireHandle for MockWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        if self.succeed {
            Ok(())
        } else {
            Err(NetworkError::ConnectionError("mock connect failure".into()))
        }
    }
    fn is_connected(&self) -> bool {
        self.succeed
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
    fn identity(&self) -> Option<WireIdentity> {
        self.identity.clone()
    }
}

fn fast_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        initial_delay_ms: 0,
        max_delay_ms: 10,
        multiplier: 1.0,
    }
}

// ── RetryConfig ─────────────────────────────────────────────────────────

#[test]
fn retry_config_default_values() {
    let r = RetryConfig::default();
    assert_eq!(r.max_attempts, 3);
    assert_eq!(r.initial_delay_ms, 1000);
    assert_eq!(r.max_delay_ms, 10000);
    assert_eq!(r.multiplier, 2.0);
}

#[test]
fn retry_config_create_backoff_yields_first_attempt_at_no_delay() {
    // The first backoff step must be immediate (delay 0) so the initial
    // connection attempt isn't needlessly delayed.
    let mut b = RetryConfig::default().create_backoff();
    let first = b.next();
    assert!(first.is_some(), "backoff should yield at least one step");
}

// ── WirePool initial state ──────────────────────────────────────────────

#[tokio::test]
async fn new_pool_is_open_with_empty_ready_set() {
    let pool = WirePool::new(RetryConfig::default());
    assert!(!pool.is_closed());

    // Empty pool: no ready candidates, no connection.
    assert!(
        !pool
            .has_live_candidate(&[ConnType::WebSocket, ConnType::WebRTC])
            .await
    );
    assert!(pool.get_connection(ConnType::WebSocket).await.is_none());

    // watch_ready receiver starts with an empty set.
    let rx = pool.watch_ready();
    assert!(rx.borrow().is_empty());
}

#[tokio::test]
async fn close_all_marks_pool_closed_and_clears_ready() {
    let pool = WirePool::new(RetryConfig::default());
    assert!(!pool.is_closed());
    pool.close_all().await;
    assert!(pool.is_closed());
    // Ready set broadcast as empty.
    assert!(pool.watch_ready().borrow().is_empty());
}

#[tokio::test]
async fn live_webrtc_lifecycle_overrides_stale_commit_snapshot() {
    let epoch_1 = crate::transport::WebRtcTransportLifecycle {
        peer_id: actr_protocol::ActrId::default(),
        controller_lifecycle_id: 7,
        transport_epoch: 1,
    };
    let epoch_2 = crate::transport::WebRtcTransportLifecycle {
        transport_epoch: 2,
        ..epoch_1.clone()
    };
    let wire = Arc::new(AdvancingLifecycleWire {
        lifecycle: std::sync::Mutex::new(epoch_1.clone()),
    });
    let pool = WirePool::new(fast_retry());
    pool.add_connection(wire.clone()).await;

    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.get_connection(ConnType::WebRTC).await.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("WebRTC wire should become ready");
    assert!(pool.connection_matches_webrtc_lifecycle(&epoch_1).await);

    *wire.lifecycle.lock().expect("lifecycle lock") = epoch_2.clone();
    assert!(
        !pool.connection_matches_webrtc_lifecycle(&epoch_1).await,
        "old terminal event must not match the advanced live attempt"
    );
    assert!(pool.connection_matches_webrtc_lifecycle(&epoch_2).await);
}

#[tokio::test]
async fn terminal_attempt_does_not_match_while_wire_pool_still_owns_retries() {
    let lifecycle = crate::transport::WebRtcTransportLifecycle {
        peer_id: actr_protocol::ActrId::default(),
        controller_lifecycle_id: 9,
        transport_epoch: 1,
    };
    let wire = Arc::new(BlockingLifecycleWire {
        lifecycle: lifecycle.clone(),
        started: tokio::sync::Notify::new(),
    });
    let pool = WirePool::new(RetryConfig::default());
    pool.add_connection(wire.clone()).await;
    wire.started.notified().await;

    assert!(
        !pool.connection_matches_webrtc_lifecycle(&lifecycle).await,
        "terminal cleanup must not detach a Connecting slot before retries finish"
    );
    pool.close_all().await;
}

#[tokio::test]
async fn close_all_interrupts_an_inflight_connection_attempt() {
    let pool = WirePool::new(RetryConfig::default());
    let wire = Arc::new(BlockingWire {
        started: tokio::sync::Notify::new(),
        closed: tokio::sync::Notify::new(),
    });

    pool.add_connection(wire.clone()).await;
    tokio::time::timeout(Duration::from_secs(1), wire.started.notified())
        .await
        .expect("connection attempt should start");

    pool.close_all().await;
    tokio::time::timeout(Duration::from_secs(1), wire.closed.notified())
        .await
        .expect("pool close must cancel and close the in-flight wire immediately");
}

#[tokio::test]
async fn close_all_wins_compare_and_commit_after_connect_succeeds() {
    let pool = Arc::new(WirePool::new(RetryConfig::default()));
    let wire = Arc::new(CommitRaceWire {
        started: tokio::sync::Notify::new(),
        release_connect: tokio::sync::Barrier::new(2),
        close_calls: std::sync::atomic::AtomicUsize::new(0),
    });

    pool.add_connection(wire.clone()).await;
    tokio::time::timeout(Duration::from_secs(1), wire.started.notified())
        .await
        .expect("connection attempt should start");

    // Hold the commit lock after the attempt starts. Once `pending` returns to
    // zero, connect() has succeeded and the task is waiting at compare-and-
    // commit rather than in the cancellation select.
    let connections = Arc::clone(&pool.connections);
    let commit_guard = connections.write().await;
    wire.release_connect.wait().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while pool.pending.load(std::sync::atomic::Ordering::Relaxed) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("successful connect should reach compare-and-commit");

    let closing_pool = Arc::clone(&pool);
    let close_all = tokio::spawn(async move {
        closing_pool.close_all().await;
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while !pool.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("close_all should publish the closed flag before taking the commit lock");

    drop(commit_guard);
    close_all.await.expect("close_all task should not panic");
    tokio::time::timeout(Duration::from_secs(1), async {
        while wire.close_calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("late successful wire should be closed");

    assert_eq!(
        wire.close_calls.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert!(pool.get_connection(ConnType::WebSocket).await.is_none());
    assert!(pool.watch_ready().borrow().is_empty());
}

#[tokio::test]
async fn mark_connection_closed_sets_failed_and_no_live_candidate() {
    let pool = WirePool::new(RetryConfig::default());
    pool.mark_connection_closed(ConnType::WebRTC).await;
    // Failed is not a live candidate.
    assert!(!pool.has_live_candidate(&[ConnType::WebRTC]).await);
    assert!(pool.get_connection(ConnType::WebRTC).await.is_none());
}

#[tokio::test]
async fn identity_checks_return_false_when_not_ready() {
    let pool = WirePool::new(RetryConfig::default());
    let id = WireIdentity::WebRtc {
        peer_id: actr_protocol::ActrId::default(),
        session_id: 1,
    };
    // No Ready slot → both identity checks are false.
    assert!(
        !pool
            .connection_matches_identity(ConnType::WebRTC, &id)
            .await
    );
    assert!(
        !pool
            .mark_connection_closed_if_same(ConnType::WebRTC, &id)
            .await,
        "non-ready slot should not be marked closed"
    );
}

// ── add_connection: success path ────────────────────────────────────────

#[tokio::test]
async fn add_connection_success_becomes_ready_and_broadcasts() {
    let pool = WirePool::new(fast_retry());
    let mut rx = pool.watch_ready();

    pool.add_connection(Arc::new(MockWire {
        conn_type: ConnType::WebSocket,
        succeed: true,
        identity: None,
    }))
    .await;

    // Await broadcast: ready set should contain WebSocket.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if rx.borrow().contains(&ConnType::WebSocket) {
                return;
            }
            let _ = tokio::time::timeout(Duration::from_millis(50), rx.changed()).await;
        }
    })
    .await
    .expect("ready broadcast should fire within 2s");

    // get_connection now returns the Ready handle.
    assert!(pool.get_connection(ConnType::WebSocket).await.is_some());
    assert!(pool.has_live_candidate(&[ConnType::WebSocket]).await);
}

// ── SecureActorChannel Verified gate ─────────────────────────────────────
// A wire whose secure channel is not `Verified` must not surface as ready,
// even after `connect()` succeeds. Proves the gate is real, not inert.

#[derive(Debug)]
struct UnverifiedWire {
    conn_type: ConnType,
    secure_state: SecureChannelState,
}

#[async_trait::async_trait]
impl WireHandle for UnverifiedWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure_state
    }
}

#[tokio::test]
async fn unverified_wire_does_not_surface_as_ready() {
    let pool = WirePool::new(fast_retry());
    let rx = pool.watch_ready();

    pool.add_connection(Arc::new(UnverifiedWire {
        conn_type: ConnType::WebSocket,
        secure_state: SecureChannelState::Handshaking,
    }))
    .await;

    // Give the background connect task time to commit the slot as Ready.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Slot is transport-Ready (live candidate), but secure state gates it out
    // of the broadcast ready set and out of get_connection.
    assert!(
        !rx.borrow().contains(&ConnType::WebSocket),
        "unverified wire must not appear in the ready set"
    );
    assert!(
        pool.get_connection(ConnType::WebSocket).await.is_none(),
        "get_connection must refuse an unverified wire"
    );
    assert!(
        pool.has_live_candidate(&[ConnType::WebSocket]).await,
        "transport-Ready wire is still a live candidate"
    );
}

// ── Secure-channel seam: state machine, transitions, data-plane gate ──────
// RFC-0563 review (alinejun): the gate must be honest (LegacyBypass, not
// Verified), transitions observable by the pool, and the data plane gated by
// a generation-bound secure_epoch.

/// Minimal `DataLane` echo mock for secure-gate tests.
#[derive(Debug, Default)]
struct EchoLane {
    sent: Arc<Mutex<Vec<bytes::Bytes>>>,
}

#[async_trait::async_trait]
impl DataLane for EchoLane {
    async fn send(&self, data: bytes::Bytes) -> NetworkResult<()> {
        self.sent.lock().await.push(data);
        Ok(())
    }
    async fn recv(&self) -> NetworkResult<bytes::Bytes> {
        Err(NetworkError::NotImplemented(
            "echo mock has no inbound".into(),
        ))
    }
    fn lane_type(&self) -> &'static str {
        "echo"
    }
}

/// `WireHandle` mock that owns a `SecureCell` and delegates the secure-state
/// trio (`secure_channel_state` / `secure_epoch` / `await_secure_state_change`)
/// to it, so the pool's listener task can observe real transitions. `get_lane`
/// returns a `SecureDataLane` bound to the same cell, exercising the data-plane
/// gate end-to-end.
#[derive(Debug)]
struct SecuredMockWire {
    conn_type: ConnType,
    secure: SecureCell,
}

impl SecuredMockWire {
    fn new(conn_type: ConnType, initial: SecureChannelState) -> Self {
        Self {
            conn_type,
            secure: SecureCell::new(initial),
        }
    }
}

#[async_trait::async_trait]
impl WireHandle for SecuredMockWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn close(&self) -> NetworkResult<()> {
        self.secure.drive_terminal(SecureChannelState::Closed);
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(Arc::new(SecureDataLane::new(
            Arc::new(EchoLane::default()),
            self.secure.clone(),
        )))
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure.state()
    }
    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }
    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }
}

/// Review test 1: `Handshaking → Verified` wakes `watch_ready` exactly once.
#[tokio::test]
async fn handshaking_to_verified_wakes_watch_ready_once() {
    let pool = WirePool::new(fast_retry());
    let mut rx = pool.watch_ready();

    let wire = Arc::new(SecuredMockWire::new(
        ConnType::WebSocket,
        SecureChannelState::Handshaking,
    ));
    let cell = wire.secure.clone();
    pool.add_connection(wire).await;

    // Slot is committed Ready, but secure state is Handshaking (not sendable),
    // so the ready set stays empty — no broadcast yet.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!rx.borrow().contains(&ConnType::WebSocket));

    // Async transition: the pool's listener task must observe this and
    // re-broadcast, surfacing the wire as ready.
    assert!(cell.set_state_validated(
        SecureChannelState::Handshaking,
        SecureChannelState::Verified,
    ));
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if rx.borrow().contains(&ConnType::WebSocket) {
                return;
            }
            let _ = tokio::time::timeout(Duration::from_millis(50), rx.changed()).await;
        }
    })
    .await
    .expect("Verified transition must wake watch_ready");

    // Exactly once: no further transition → a second `changed()` must not fire.
    let again = tokio::time::timeout(Duration::from_millis(100), rx.changed()).await;
    assert!(
        again.is_err(),
        "no further transition should wake watch_ready again"
    );
}

/// Review test 2: `Verified → Rekeying` removes readiness and blocks an
/// already-acquired lane (epoch rotation invalidates the captured lane).
#[tokio::test]
async fn verified_to_rekeying_removes_readiness_and_blocks_acquired_lane() {
    let pool = WirePool::new(fast_retry());
    let mut rx = pool.watch_ready();

    let wire = Arc::new(SecuredMockWire::new(
        ConnType::WebRTC,
        SecureChannelState::Verified,
    ));
    let cell = wire.secure.clone();
    pool.add_connection(wire).await;

    // Wait until ready, then acquire a lane bound to the Verified epoch.
    tokio::time::timeout(Duration::from_secs(2), async {
        while !rx.borrow().contains(&ConnType::WebRTC) {
            let _ = tokio::time::timeout(Duration::from_millis(50), rx.changed()).await;
        }
    })
    .await
    .unwrap();
    let conn = pool
        .get_connection(ConnType::WebRTC)
        .await
        .expect("Verified wire must be acquirable");
    let lane = conn.get_lane(PayloadType::RpcReliable).await.unwrap();
    assert!(lane.send(bytes::Bytes::new()).await.is_ok());

    // Rotate: epoch bumps, the acquired lane is now stale.
    assert!(cell.set_state_validated(SecureChannelState::Verified, SecureChannelState::Rekeying,));

    // The already-acquired lane refuses further sends.
    let send_result = lane.send(bytes::Bytes::new()).await;
    assert!(
        matches!(send_result, Err(NetworkError::SecureChannelNotReady(_))),
        "rotated-epoch lane must refuse send with SecureChannelNotReady, got {send_result:?}"
    );
    assert!(
        !send_result.unwrap_err().is_closed_like(),
        "rekey must not look like a closed wire (no eviction)"
    );

    // And the wire is no longer sendable → get_connection returns None.
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.get_connection(ConnType::WebRTC).await.is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Rekeying wire must drop out of the ready set");
}

/// Review test 3: an unwrapped wire reports `LegacyBypass`, not `Verified`,
/// and is sendable via `LegacyBypass` only.
#[tokio::test]
async fn unwrapped_wire_is_legacy_bypass_not_verified() {
    let wire = MockWire {
        conn_type: ConnType::WebSocket,
        succeed: true,
        identity: None,
    };
    assert_eq!(
        wire.secure_channel_state(),
        SecureChannelState::LegacyBypass,
        "unwrapped wire must not claim Verified"
    );
    assert!(
        wire.secure_channel_state().is_sendable(),
        "LegacyBypass must be sendable in phase 2"
    );
    assert_ne!(
        wire.secure_channel_state(),
        SecureChannelState::Verified,
        "LegacyBypass must be distinct from Verified"
    );
}

/// Review test 4: repeated lookup of one physical connection preserves its
/// secure state/epoch (state is owned by the physical session, not a
/// re-created wrapper). Modeled at the `SecureCell` level: two handles cloned
/// from the same physical cell observe identical state and a transition made
/// through one is visible through the other.
#[tokio::test]
async fn repeated_lookup_preserves_secure_state_and_epoch() {
    let cell = SecureCell::new(SecureChannelState::Handshaking);
    let view_a = cell.clone();
    let view_b = cell.clone();

    assert_eq!(view_a.state(), SecureChannelState::Handshaking);
    assert_eq!(view_a.epoch(), view_b.epoch());

    // Advance via one view (simulating a handshake driven through one lookup).
    assert!(view_a.set_state_validated(
        SecureChannelState::Handshaking,
        SecureChannelState::Verified,
    ));

    // The other view — representing a second `existing_connections` lookup of
    // the same physical session — observes the new state and epoch.
    assert_eq!(view_b.state(), SecureChannelState::Verified);
    assert_eq!(view_b.epoch(), view_a.epoch());
    assert_ne!(view_b.epoch(), 0, "epoch must have advanced");

    // A terminal transition through one view is visible through the other.
    assert!(view_b.set_state_validated(SecureChannelState::Verified, SecureChannelState::Closed,));
    assert_eq!(view_a.state(), SecureChannelState::Closed);
}

/// Review test 5: inbound application frames are rejected before
/// authentication — a `SecureDataLane` bound to a non-sendable cell refuses
/// `recv()`, so a reader task driving it would break its loop and deliver
/// nothing to the application.
#[tokio::test]
async fn inbound_frames_rejected_before_auth() {
    let cell = SecureCell::new(SecureChannelState::Handshaking);
    let lane = SecureDataLane::new(Arc::new(EchoLane::default()), cell);

    let recv_result = lane.recv().await;
    assert!(
        matches!(recv_result, Err(NetworkError::SecureChannelNotReady(_))),
        "pre-auth recv must be rejected with SecureChannelNotReady, got {recv_result:?}"
    );

    // And outbound send is equally gated.
    let send_result = lane.send(bytes::Bytes::from_static(b"x")).await;
    assert!(
        matches!(send_result, Err(NetworkError::SecureChannelNotReady(_))),
        "pre-auth send must be rejected with SecureChannelNotReady, got {send_result:?}"
    );
}

/// Regression (RFC-0563 review point 2 lifecycle coupling): the per-
/// connection state-listener task spawned in `add_connection` must exit once
/// the wire's secure state goes terminal (`Closed`/`Failed`), rather than
/// parking forever and leaking the wire's `Arc`.
///
/// Observed via `Arc::strong_count`: v0.20 deliberately retains the failed
/// handle in `WireStatus::Failed(handle)` for transport-lifecycle CAS. After
/// the terminal transition the listener must release its clone, leaving only
/// the test and the pool's failed slot.
#[tokio::test]
async fn listener_task_exits_on_terminal_secure_state() {
    let pool = WirePool::new(fast_retry());

    // Start `Verified` (sendable) so the slot surfaces as Ready and we can
    // deterministically wait for the retry task to commit, spawn the
    // listener, and return (dropping its own clone).
    let wire = Arc::new(SecuredMockWire::new(
        ConnType::WebSocket,
        SecureChannelState::Verified,
    ));
    let cell = wire.secure.clone();
    pool.add_connection(wire.clone()).await;

    // Wait for the slot to reach Ready, then let the retry task finish
    // (spawn listener + return). After this, strong refs = test + pool +
    // listener.
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.get_connection(ConnType::WebSocket).await.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Verified wire must surface as ready");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Retire the slot. v0.20 keeps the failed handle for lifecycle identity,
    // so test + pool + listener remain until the terminal edge is observed.
    pool.mark_connection_closed(ConnType::WebSocket).await;
    assert!(
        Arc::strong_count(&wire) >= 3,
        "listener must still hold the wire after slot retirement"
    );

    // Drive the secure state terminal: the listener wakes, re-broadcasts, and
    // (regression) exits instead of parking forever on the terminal epoch.
    assert!(cell.set_state_validated(SecureChannelState::Verified, SecureChannelState::Closed,));

    // Wait for the listener to observe the terminal transition and drop its
    // clone. The pool intentionally retains one failed-slot clone (the
    // `Failed(handle)` lifecycle identity that `mark_connection_closed_if_same`
    // compares against, #582). If the exit-on-terminal guard is removed, the
    // wait above times out stuck at three.
    tokio::time::timeout(Duration::from_secs(2), async {
        while Arc::strong_count(&wire) > 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("listener task must exit on terminal secure state, not leak the wire Arc");
    // Steady state: test + pool's retained failed-slot clone.
    assert_eq!(Arc::strong_count(&wire), 2);
}

// ── add_connection: failure path ────────────────────────────────────────

#[tokio::test]
async fn add_connection_failure_marks_failed_and_exhausts() {
    let pool = WirePool::new(fast_retry());
    pool.add_connection(Arc::new(MockWire {
        conn_type: ConnType::WebRTC,
        succeed: false,
        identity: None,
    }))
    .await;

    // Poll until the WebRTC slot stops being a live candidate (Connecting → Failed).
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.has_live_candidate(&[ConnType::WebRTC]).await {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("failed connection should leave the live-candidate set within 2s");

    // Failed → no connection retrievable.
    assert!(pool.get_connection(ConnType::WebRTC).await.is_none());
    assert!(!pool.has_live_candidate(&[ConnType::WebRTC]).await);
}

#[tokio::test(start_paused = true)]
async fn first_wire_retry_uses_the_configured_initial_delay() {
    let pool = WirePool::new(RetryConfig {
        max_attempts: 2,
        initial_delay_ms: 1_000,
        max_delay_ms: 10_000,
        multiplier: 2.0,
    });
    let wire = Arc::new(CountingFailWire {
        attempts: std::sync::atomic::AtomicUsize::new(0),
        changed: tokio::sync::Notify::new(),
    });

    pool.add_connection(wire.clone()).await;
    while wire.attempts.load(std::sync::atomic::Ordering::SeqCst) < 1 {
        wire.changed.notified().await;
    }

    tokio::time::advance(Duration::from_millis(999)).await;
    tokio::task::yield_now().await;
    assert_eq!(wire.attempts.load(std::sync::atomic::Ordering::SeqCst), 1);

    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        wire.attempts.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the first retry should use initial_delay_ms, not the second backoff step"
    );
}

#[tokio::test(start_paused = true)]
async fn stalled_wire_connect_attempt_has_one_failure_deadline() {
    let pool = WirePool::new(RetryConfig {
        max_attempts: 1,
        initial_delay_ms: 1_000,
        max_delay_ms: 10_000,
        multiplier: 2.0,
    });
    let wire = Arc::new(BlockingWire {
        started: tokio::sync::Notify::new(),
        closed: tokio::sync::Notify::new(),
    });

    pool.add_connection(wire.clone()).await;
    wire.started.notified().await;

    tokio::time::advance(Duration::from_secs(30) + Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert!(
        !pool.has_live_candidate(&[ConnType::WebSocket]).await,
        "a stalled connect future must not leave the pool in Connecting forever"
    );
}

// ── identity-aware close on a Ready slot ─────────────────────────────────

#[tokio::test]
async fn mark_closed_if_same_matches_and_mismatches() {
    let id = WireIdentity::WebRtc {
        peer_id: actr_protocol::ActrId::default(),
        session_id: 7,
    };
    let other = WireIdentity::WebRtc {
        peer_id: actr_protocol::ActrId::default(),
        session_id: 99,
    };
    let pool = WirePool::new(fast_retry());

    pool.add_connection(Arc::new(MockWire {
        conn_type: ConnType::WebSocket,
        succeed: true,
        identity: Some(id.clone()),
    }))
    .await;

    // Wait until Ready.
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.get_connection(ConnType::WebSocket).await.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    // Matching identity → closes (true).
    assert!(
        pool.mark_connection_closed_if_same(ConnType::WebSocket, &id)
            .await
    );
    // Slot now Failed → no longer matches anything.
    assert!(
        !pool
            .connection_matches_identity(ConnType::WebSocket, &id)
            .await
    );

    // Re-ready a fresh wire, then assert a mismatched identity does NOT close.
    let pool2 = WirePool::new(fast_retry());
    pool2
        .add_connection(Arc::new(MockWire {
            conn_type: ConnType::WebSocket,
            succeed: true,
            identity: Some(id.clone()),
        }))
        .await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool2.get_connection(ConnType::WebSocket).await.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !pool2
            .mark_connection_closed_if_same(ConnType::WebSocket, &other)
            .await,
        "mismatched identity must not close the slot"
    );
    // Still ready afterwards.
    assert!(pool2.get_connection(ConnType::WebSocket).await.is_some());
}

// ── Secure-listener arm races (RFC-0563 re-review) ──────────────────────

/// Wire whose `secure_channel_state()` drives `Handshaking → Verified` on its
/// SECOND call. Under the fixed sequencing the calls are: #1 = the pool's
/// initial ready broadcast (pre-snapshot ordering), #2 = the listener's
/// loop-top terminal probe — i.e. the transition lands exactly in the window
/// between the initial broadcast and the listener arming. With the old
/// post-spawn epoch snapshot this interleaving parked the listener forever
/// (lost wakeup).
#[derive(Debug)]
struct TransitionBetweenBroadcastAndArmWire {
    conn_type: ConnType,
    secure: SecureCell,
    state_checks: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl WireHandle for TransitionBetweenBroadcastAndArmWire {
    fn connection_type(&self) -> ConnType {
        self.conn_type
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        let n = self
            .state_checks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if n == 2 {
            let _ = self.secure.set_state_validated(
                SecureChannelState::Handshaking,
                SecureChannelState::Verified,
            );
        }
        self.secure.state()
    }
    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }
    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }
}

/// Re-review scenario: a transition landing between the initial ready
/// broadcast and the listener arming must still surface in `watch_ready`
/// (the pre-broadcast epoch snapshot makes the listener's wait return
/// immediately for an already-advanced epoch).
#[tokio::test]
async fn transition_between_broadcast_and_listener_arm_is_not_lost() {
    let pool = WirePool::new(fast_retry());
    let mut rx = pool.watch_ready();

    let wire = Arc::new(TransitionBetweenBroadcastAndArmWire {
        conn_type: ConnType::WebSocket,
        secure: SecureCell::new(SecureChannelState::Handshaking),
        state_checks: std::sync::atomic::AtomicUsize::new(0),
    });
    pool.add_connection(wire).await;

    tokio::time::timeout(Duration::from_secs(2), async {
        while !rx.borrow().contains(&ConnType::WebSocket) {
            let _ = tokio::time::timeout(Duration::from_millis(50), rx.changed()).await;
        }
    })
    .await
    .expect("transition between broadcast and listener arm must wake watch_ready");
}

/// Wire that reaches terminal `Closed` during `connect()` — i.e. BEFORE the
/// listener task arms.
#[derive(Debug)]
struct TerminalBeforeArmWire {
    secure: SecureCell,
}

#[async_trait::async_trait]
impl WireHandle for TerminalBeforeArmWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        self.secure.drive_terminal(SecureChannelState::Closed);
        Ok(())
    }
    fn is_connected(&self) -> bool {
        false
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure.state()
    }
    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }
    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }
}

/// Re-review scenario: a wire already terminal before the listener arms must
/// not park the listener forever (terminal admits no further transitions, so
/// the pre-wait terminal probe is the only exit). Pinned via `Arc::strong_count`.
#[tokio::test]
async fn terminal_before_listener_arm_exits_without_parking() {
    let pool = WirePool::new(fast_retry());
    let rx = pool.watch_ready();

    // `Closed` is a true terminal state (`is_terminal`); `LegacyBypass` is a
    // live phase-2 sendable state and would legitimately park the listener.
    let wire = Arc::new(TerminalBeforeArmWire {
        secure: SecureCell::new(SecureChannelState::Closed),
    });
    pool.add_connection(wire.clone()).await;

    // The wire committed Ready (connect returned Ok) but is terminal → it must
    // never appear in the ready set.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !rx.borrow().contains(&ConnType::WebSocket),
        "terminal wire must not surface as ready"
    );

    // Retire the slot; the pool retains the failed handle for lifecycle
    // identity, so the remaining co-owners are the test, the pool, and the
    // listener. The pre-arm terminal probe must make the listener exit
    // promptly, releasing its clone.
    pool.mark_connection_closed(ConnType::WebSocket).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while Arc::strong_count(&wire) > 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("listener must exit on pre-arm terminal state, not park forever");
    // Steady state: test + pool's retained failed-slot clone.
    assert_eq!(Arc::strong_count(&wire), 2);
}

// ── Connect-retry vs terminal Failed (RFC-0563 re-review) ───────────────

/// Wire that fails its first connect attempt and succeeds afterwards. Its
/// secure cell stays at the pre-connect state (`LegacyBypass`) — mirroring the
/// fixed contract where a single failed attempt must NOT drive terminal
/// `Failed` (the pool retries this same object).
#[derive(Debug)]
struct FailOnceThenSucceedWire {
    attempts: std::sync::atomic::AtomicUsize,
    secure: SecureCell,
    fail_forever: bool,
}

#[async_trait::async_trait]
impl WireHandle for FailOnceThenSucceedWire {
    fn connection_type(&self) -> ConnType {
        ConnType::WebSocket
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        let n = self
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if self.fail_forever || n == 1 {
            return Err(NetworkError::ConnectionError("first attempt fails".into()));
        }
        Ok(())
    }
    fn is_connected(&self) -> bool {
        false
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Err(NetworkError::NotImplemented("mock has no lane".into()))
    }
    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure.state()
    }
    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }
    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }
    fn mark_secure_failed(&self) {
        self.secure.drive_terminal(SecureChannelState::Failed);
    }
}

/// Re-review scenario: first connect attempt fails, a later retry on the SAME
/// object succeeds — the wire must end up Ready AND sendable. (With per-error
/// `Failed` driving, attempt 2 would connect into a permanently-terminal
/// session and `get_connection` would never return it.)
#[tokio::test]
async fn first_connect_failure_then_retry_success_stays_sendable() {
    let pool = WirePool::new(RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 0,
        max_delay_ms: 10,
        multiplier: 1.0,
    });

    let wire = Arc::new(FailOnceThenSucceedWire {
        attempts: std::sync::atomic::AtomicUsize::new(0),
        secure: SecureCell::new(SecureChannelState::LegacyBypass),
        fail_forever: false,
    });
    let secure = wire.secure.clone();
    pool.add_connection(wire).await;

    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.get_connection(ConnType::WebSocket).await.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("retry after a failed attempt must surface the wire as ready");

    assert_eq!(secure.state(), SecureChannelState::LegacyBypass);
    assert!(
        secure.state().is_sendable(),
        "a single failed attempt must not leave the session terminal"
    );
}

/// Re-review counterpart: terminal `Failed` is declared only at retry
/// exhaustion, via the pool's `mark_secure_failed` call.
#[tokio::test]
async fn retry_exhaustion_marks_secure_failed() {
    let pool = WirePool::new(RetryConfig {
        max_attempts: 2,
        initial_delay_ms: 0,
        max_delay_ms: 10,
        multiplier: 1.0,
    });

    let wire = Arc::new(FailOnceThenSucceedWire {
        attempts: std::sync::atomic::AtomicUsize::new(0),
        secure: SecureCell::new(SecureChannelState::LegacyBypass),
        fail_forever: true,
    });
    let secure = wire.secure.clone();
    pool.add_connection(wire).await;

    tokio::time::timeout(Duration::from_secs(2), async {
        while secure.state() != SecureChannelState::Failed {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("retry exhaustion must drive the secure session to Failed");
    assert!(secure.state().is_terminal());
    assert!(pool.get_connection(ConnType::WebSocket).await.is_none());
}

// ── Pending receive crossing a rotation (RFC-0563 re-review) ────────────

/// Lane whose `recv()` parks on an mpsc channel until a frame (or rotation)
/// ends it, notifying when the receive has actually started.
#[derive(Debug)]
struct ParkingLane {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<bytes::Bytes>>,
    recv_started: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl DataLane for ParkingLane {
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        Ok(())
    }
    async fn recv(&self) -> NetworkResult<bytes::Bytes> {
        self.recv_started.notify_one();
        let mut rx = self.rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| NetworkError::ConnectionClosed("parking lane closed".into()))
    }
    fn lane_type(&self) -> &'static str {
        "parking"
    }
}

/// Re-review scenario: a receive that starts while `Verified` must not deliver
/// its frame after the session entered `Rekeying` — the lane races the pending
/// receive against the epoch rotation and errors with `SecureChannelNotReady`.
#[tokio::test]
async fn pending_receive_crossing_rekey_errors_without_delivery() {
    let (_tx, rx) = tokio::sync::mpsc::channel(1);
    let lane_inner = Arc::new(ParkingLane {
        rx: tokio::sync::Mutex::new(rx),
        recv_started: tokio::sync::Notify::new(),
    });
    let cell = SecureCell::new(SecureChannelState::Verified);
    let lane = Arc::new(SecureDataLane::new(lane_inner.clone(), cell.clone()));

    let recv_task = tokio::spawn(async move { lane.recv().await });

    // Ensure the receive is actually pending before rotating.
    tokio::time::timeout(Duration::from_secs(1), lane_inner.recv_started.notified())
        .await
        .expect("recv should start");

    assert!(cell.set_state_validated(SecureChannelState::Verified, SecureChannelState::Rekeying,));

    // The rotation must end the pending receive promptly with NotReady — not
    // deliver a frame once one arrives.
    let result = tokio::time::timeout(Duration::from_secs(1), recv_task)
        .await
        .expect("rotation must end the pending receive")
        .expect("recv task must not panic");
    assert!(
        matches!(result, Err(NetworkError::SecureChannelNotReady(_))),
        "pending receive across rekey must error with SecureChannelNotReady, got {result:?}"
    );
    assert!(
        !result.unwrap_err().is_closed_like(),
        "rekey during a pending receive must not look like a closed wire"
    );
}

/// Re-review follow-up: a terminal transition bumps the epoch too, so `gate()`
/// must check the terminal state BEFORE the epoch comparison — otherwise a
/// held lane over a dead wire would surface `SecureChannelNotReady` and park
/// the caller on a readiness that can never arrive instead of evicting.
#[tokio::test]
async fn held_lane_over_terminal_wire_surfaces_connection_closed() {
    let cell = SecureCell::new(SecureChannelState::Verified);
    let lane = SecureDataLane::new(Arc::new(EchoLane::default()), cell.clone());

    // Terminal transition bumps the epoch (drive_terminal → set_state_validated),
    // so the epoch-rotation branch is also live here — ordering matters.
    cell.drive_terminal(SecureChannelState::Closed);
    assert_ne!(cell.epoch(), 0, "terminal transition must bump the epoch");

    let result = lane.send(bytes::Bytes::new()).await;
    assert!(
        matches!(result, Err(NetworkError::ConnectionClosed(_))),
        "terminal wire must surface ConnectionClosed (evict-worthy), got {result:?}"
    );
    assert!(result.unwrap_err().is_closed_like());
}
