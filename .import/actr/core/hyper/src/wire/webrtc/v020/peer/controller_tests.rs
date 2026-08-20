//! Controller task tests: prove the reducer/effect/mailbox wiring under real
//! async timing, using a fake `SessionFactory` + fake `SessionOps` (no network,
//! deterministic). These cover T1 race scenarios end-to-end through the
//! event loop — close-during-build (late success/failure), the full
//! offerer/answerer flows to `Sendable`, and multi-controller isolation.

use std::sync::Arc;
use std::time::Duration;

use actr_protocol::{ActrId, ActrType, Realm};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidateInit, RTCPeerConnectionState, RTCSdpType,
    RTCSessionDescription,
};

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::v020::events::PeerDriverEvent;
use crate::wire::webrtc::v020::status::RtcPeerSnapshot;

use super::controller::RtcPeerController;
use super::effects::{SessionFactory, SessionOps};
use super::handle::RtcPeerControllerHandle;
use super::reducer::{LaneEvent, SignalingMessage};
use crate::wire::webrtc::v020::status::LaneKind;

const WAIT: Duration = Duration::from_secs(5);

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

fn desc(sdp_type: RTCSdpType) -> RTCSessionDescription {
    let mut d = RTCSessionDescription::default();
    d.sdp_type = sdp_type;
    d.sdp = format!("fake-{sdp_type:?}");
    d
}

// ── fake session ops / factory ──────────────────────────────────────────────

/// A `SessionOps` whose methods succeed with canned descriptions.
struct FakeSessionOps;

#[async_trait]
impl SessionOps for FakeSessionOps {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Offer))
    }
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Answer))
    }
    async fn set_local_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
        Ok(())
    }
    async fn set_remote_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
        Ok(())
    }
    async fn add_ice_candidate(&self, _c: RTCIceCandidateInit) -> Result<(), RtcError> {
        Ok(())
    }
    async fn restart_ice(&self) -> Result<(), RtcError> {
        Ok(())
    }
    async fn set_configuration(&self, _config: RTCConfiguration) -> Result<(), RtcError> {
        Ok(())
    }
    async fn create_data_channel(
        &self,
        label: &str,
        _init: webrtc::data_channel::RTCDataChannelInit,
    ) -> Result<Arc<dyn webrtc::data_channel::DataChannel>, RtcError> {
        // Reports OnOpen via the fake so the offerer's readiness anchor reaches
        // Sendable through the real pump path.
        Ok(super::effects::test_support::fake_data_channel(label))
    }
    async fn add_track(
        &self,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, RtcError> {
        Ok(Arc::new(
            crate::wire::webrtc::v020::media::test_support::FakeRtpSender::new(1),
        ))
    }
    async fn remove_track(
        &self,
        _sender: &Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Result<(), RtcError> {
        Ok(())
    }
    async fn get_senders(&self) -> Vec<Arc<dyn webrtc::rtp_transceiver::RtpSender>> {
        Vec::new()
    }
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
        Default::default()
    }
    async fn close(&self) -> Result<(), RtcError> {
        Ok(())
    }
}

/// A pending `build()` the test completes manually (controls build timing).
struct BuildRequest {
    handler: Arc<WebRtcEventHandler>,
    respond: oneshot::Sender<Result<Arc<dyn SessionOps>, RtcError>>,
}

/// A factory that defers each `build()` to the test via a channel, so tests
/// decide exactly when (and whether) a build completes.
struct FakeSessionFactory {
    builds: mpsc::Sender<BuildRequest>,
}

#[async_trait]
impl SessionFactory for FakeSessionFactory {
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        let (tx, rx) = oneshot::channel();
        // If the test is gone, fail the build rather than hang.
        self.builds
            .send(BuildRequest {
                handler,
                respond: tx,
            })
            .await
            .map_err(|_| RtcError::Other("fake factory closed".into()))?;
        rx.await
            .map_err(|_| RtcError::Other("build dropped".into()))?
    }
}

/// Harness: a controller under test plus the knobs to drive it.
struct Harness {
    handle: RtcPeerControllerHandle,
    lane_tx: mpsc::Sender<LaneEvent>,
    signaling_rx: mpsc::Receiver<(ActrId, SignalingMessage)>,
    builds: mpsc::Receiver<BuildRequest>,
    /// Kept alive so the broadcast sender is not dropped (which would close
    /// the controller's network event receiver and exit its event loop).
    _supervisor: crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor,
    _task: tokio::task::JoinHandle<()>,
}

fn spawn_harness(serial: u64) -> Harness {
    spawn_harness_with_signaling_capacity(serial, 16)
}

/// A harness whose shared signaling queue has an explicit (small) capacity, to
/// exercise N2 full-queue backpressure deterministically.
fn spawn_harness_with_signaling_capacity(serial: u64, capacity: usize) -> Harness {
    let (signaling_tx, signaling_rx) = mpsc::channel(capacity);
    let (builds_tx, builds_rx) = mpsc::channel(16);
    let factory = Arc::new(FakeSessionFactory { builds: builds_tx });
    let inbound = std::sync::Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::discard());
    let supervisor = crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor::new();
    supervisor.set_signaling_generation(Some(0));
    let build_throttle =
        std::sync::Arc::new(crate::wire::webrtc::v020::recovery::BuildThrottle::new(
            crate::wire::webrtc::v020::recovery::DEFAULT_MAX_CONCURRENT_BUILDS,
            std::time::Duration::ZERO, // no jitter in tests
        ));
    let (handle, lane_tx, task) = RtcPeerController::spawn(
        test_actor_id(serial),
        factory,
        signaling_tx,
        supervisor.subscribe(),
        supervisor.subscribe_signaling_generation(),
        crate::wire::webrtc::v020::peer::controller::SharedNodeDeps {
            inbound,
            build_throttle,
            media_registry: None,
            buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig::default(),
            #[cfg(feature = "test-utils")]
            driver_event_observer: None,
            controller_lifecycle_id: 1,
            peer_generation_invalidated: None,
            peer_sendability_changed: None,
            peer_transport_retired: None,
            peer_snapshot_events: None,
        },
    );
    Harness {
        handle,
        lane_tx,
        signaling_rx,
        builds: builds_rx,
        _supervisor: supervisor,
        _task: task,
    }
}

/// Wait for the next build request and return it.
async fn next_build(h: &mut Harness) -> BuildRequest {
    timeout(WAIT, h.builds.recv())
        .await
        .expect("timed out waiting for a build request")
        .expect("build channel closed")
}

/// Complete a pending build with a session (or an error). Uses `is_ok` so the
/// test does not require the session payload to be `Debug`.
fn complete_build(build: BuildRequest, result: Result<Arc<dyn SessionOps>, RtcError>) {
    assert!(
        build.respond.send(result).is_ok(),
        "build response should be delivered to the awaiting factory"
    );
}

/// A successful build with the canned fake session.
fn succeed_build(build: BuildRequest) {
    complete_build(build, Ok(Arc::new(FakeSessionOps)));
}

/// Wait until the snapshot satisfies `pred`, returning it.
async fn await_snapshot(
    h: &Harness,
    what: &str,
    pred: impl Fn(&RtcPeerSnapshot) -> bool,
) -> RtcPeerSnapshot {
    let mut rx = h.handle.subscribe();
    timeout(WAIT, async move {
        loop {
            let snap = rx.borrow().clone();
            if pred(&snap) {
                return snap;
            }
            rx.changed().await.expect("snapshot channel closed");
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for snapshot: {what}"))
}

/// Act as the signaling server: read the controller's `RoleNegotiation` and
/// answer with a `RoleAssignment`. The connect flow is role-negotiated, so a
/// build only starts after this.
async fn assign_role(h: &mut Harness, is_offerer: bool) {
    let (_peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("timed out waiting for role negotiation")
        .expect("signaling closed");
    assert!(
        matches!(message, SignalingMessage::RoleNegotiation),
        "expected RoleNegotiation, got {message:?}"
    );
    h.handle
        .apply_role_assignment(is_offerer)
        .await
        .expect("send role assignment");
}

// ── full-flow tests ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_offerer_flow_reaches_sendable() {
    let mut h = spawn_harness(1);
    h.handle.connect().await.expect("send connect");

    // Server assigns the offerer role → the build starts.
    assign_role(&mut h, true).await;
    let build = next_build(&mut h).await;
    let handler = build.handler.clone();
    succeed_build(build);

    // The controller sends the local offer to the signaling router.
    let (peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("timed out waiting for offer")
        .expect("signaling closed");
    assert_eq!(peer, test_actor_id(1));
    assert!(matches!(message, SignalingMessage::Offer { .. }));

    // Remote answers; controller applies it.
    h.handle
        .apply_remote_description(desc(RTCSdpType::Answer))
        .await
        .expect("send answer");

    // Transport connects, then the RpcReliable anchor opens → Sendable.
    handler
        .mailbox()
        .try_publish(PeerDriverEvent::PeerConnectionStateChanged {
            generation: 1,
            state: RTCPeerConnectionState::Connected,
        });
    h.lane_tx
        .send(LaneEvent::Opened {
            generation: 1,
            lane: LaneKind::RpcReliable,
        })
        .await
        .expect("send lane open");

    let snapshot = await_snapshot(&h, "sendable", |s| s.sendable).await;
    assert!(snapshot.sendable);
    assert_eq!(snapshot.status, actr_framework::WebRtcPeerStatus::Connected);
    assert_eq!(snapshot.pc_generation, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_answerer_flow_reaches_sendable() {
    let mut h = spawn_harness(2);
    // Incoming offer → controller builds and answers.
    h.handle
        .apply_remote_description(desc(RTCSdpType::Offer))
        .await
        .expect("send offer");

    let build = next_build(&mut h).await;
    let handler = build.handler.clone();
    succeed_build(build);

    // The answerer sends its answer to the router.
    let (_peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("timed out waiting for answer")
        .expect("signaling closed");
    assert!(matches!(message, SignalingMessage::Answer { .. }));

    handler
        .mailbox()
        .try_publish(PeerDriverEvent::PeerConnectionStateChanged {
            generation: 1,
            state: RTCPeerConnectionState::Connected,
        });
    h.lane_tx
        .send(LaneEvent::Opened {
            generation: 1,
            lane: LaneKind::RpcReliable,
        })
        .await
        .expect("send lane open");

    let snapshot = await_snapshot(&h, "sendable", |s| s.sendable).await;
    assert!(snapshot.sendable);
}

// ── close-during-build ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_close_during_build_late_success() {
    let mut h = spawn_harness(1);
    h.handle.connect().await.expect("send connect");
    assign_role(&mut h, true).await;

    // Build is now in flight (blocked awaiting the test's response).
    let build = next_build(&mut h).await;

    // Close while the build is in flight.
    h.handle.close().await.expect("send close");

    // The build completes *late*, successfully. The orphan must be closed.
    succeed_build(build);

    // The controller must converge to Idle and never become sendable.
    let snapshot = await_snapshot(&h, "idle after close", |s| {
        s.status == actr_framework::WebRtcPeerStatus::Idle && !s.sendable && s.pc_generation == 0
    })
    .await;
    assert!(!snapshot.sendable);
    assert_eq!(snapshot.pc_generation, 0, "no active session remains");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_close_during_build_late_failure() {
    let mut h = spawn_harness(1);
    h.handle.connect().await.expect("send connect");
    assign_role(&mut h, true).await;
    let build = next_build(&mut h).await;

    h.handle.close().await.expect("send close");

    // The build fails late; nothing to close, barrier releases to Idle.
    complete_build(build, Err(RtcError::Other("build failed".into())));

    let snapshot = await_snapshot(&h, "idle after close", |s| {
        s.status == actr_framework::WebRtcPeerStatus::Idle && !s.sendable
    })
    .await;
    assert!(!snapshot.sendable);
}

/// RFC task-convergence contract: an abnormal Controller exit (all command
/// handles dropped) must cancel and join an in-flight build. The build may not
/// remain detached and later install a Session after the Controller is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_exit_cancels_and_joins_inflight_build() {
    let mut h = spawn_harness(2);
    h.handle.connect().await.expect("send connect");
    assign_role(&mut h, true).await;
    let build = next_build(&mut h).await;

    let Harness {
        handle,
        lane_tx: _lane_tx,
        signaling_rx: _signaling_rx,
        builds: _builds,
        _supervisor,
        _task,
    } = h;
    drop(handle);

    timeout(WAIT, _task)
        .await
        .expect("controller exit did not converge with an in-flight build")
        .expect("controller task panicked");
    assert!(
        build.respond.send(Ok(Arc::new(FakeSessionOps))).is_err(),
        "runner teardown must abort and join the build receiver"
    );
}

/// T2 resource-boundary contract: a remote DataChannel callback may fire
/// before `build()` returns. The raw channel remains in the generation-owned
/// registry, the Controller retains only its token, and acceptance is retried
/// after the Session is installed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_channel_discovered_during_build_is_accepted_once_session_exists() {
    use webrtc::peer_connection::PeerConnectionEventHandler;

    let mut h = spawn_harness(21);
    h.handle
        .apply_remote_description(desc(RTCSdpType::Offer))
        .await
        .expect("send incoming offer");
    let build = next_build(&mut h).await;
    let handler = build.handler.clone();
    let resources = handler.resources();

    PeerConnectionEventHandler::on_data_channel(
        handler.as_ref(),
        super::effects::test_support::fake_data_channel("RPC_RELIABLE"),
    )
    .await;
    tokio::task::yield_now().await;
    assert!(
        h.handle.lane_handle(1, LaneKind::RpcReliable).is_none(),
        "token cannot be accepted before the physical Session is installed"
    );

    succeed_build(build);
    timeout(WAIT, async {
        loop {
            if h.handle.lane_handle(1, LaneKind::RpcReliable).is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("deferred DataChannel token was not accepted after build");
    assert_eq!(
        resources.len(),
        0,
        "accepted token consumes the raw resource"
    );

    h.handle.close().await.expect("close controller");
    timeout(WAIT, h._task)
        .await
        .expect("controller teardown timed out")
        .expect("controller task panicked");
}

/// Invalid metadata cannot leave an unowned raw resource behind. Rejection
/// explicitly closes and removes the channel even while the Session is still
/// building.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_remote_channel_token_is_closed_and_removed_during_build() {
    use std::sync::atomic::Ordering;
    use webrtc::peer_connection::PeerConnectionEventHandler;

    let mut h = spawn_harness(22);
    h.handle
        .apply_remote_description(desc(RTCSdpType::Offer))
        .await
        .expect("send incoming offer");
    let build = next_build(&mut h).await;
    let handler = build.handler.clone();
    let resources = handler.resources();
    let channel = super::effects::test_support::recording_data_channel("UNKNOWN_LANE");

    PeerConnectionEventHandler::on_data_channel(handler.as_ref(), channel.clone()).await;
    timeout(WAIT, async {
        loop {
            if channel.close_calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("rejected DataChannel was not closed");
    assert_eq!(
        resources.len(),
        0,
        "rejected token is removed from registry"
    );
    assert!(h.handle.lane_handle(1, LaneKind::RpcReliable).is_none());

    succeed_build(build);
    h.handle.close().await.expect("close controller");
    timeout(WAIT, h._task)
        .await
        .expect("controller teardown timed out")
        .expect("controller task panicked");
}

// ── multi-controller isolation ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multiple_controllers_run_in_parallel_isolated() {
    let mut a = spawn_harness(10);
    let mut b = spawn_harness(11);

    a.handle.connect().await.expect("connect a");
    b.handle.connect().await.expect("connect b");
    assign_role(&mut a, true).await;
    assign_role(&mut b, true).await;

    // Both build concurrently; each controller is independent.
    let build_a = next_build(&mut a).await;
    let build_b = next_build(&mut b).await;
    let handler_a = build_a.handler.clone();
    succeed_build(build_a);
    succeed_build(build_b);

    // Drive only A to sendable; B stays connecting.
    let (_p, _offer) = timeout(WAIT, a.signaling_rx.recv()).await.unwrap().unwrap();
    a.handle
        .apply_remote_description(desc(RTCSdpType::Answer))
        .await
        .unwrap();
    handler_a
        .mailbox()
        .try_publish(PeerDriverEvent::PeerConnectionStateChanged {
            generation: 1,
            state: RTCPeerConnectionState::Connected,
        });
    a.lane_tx
        .send(LaneEvent::Opened {
            generation: 1,
            lane: LaneKind::RpcReliable,
        })
        .await
        .unwrap();

    let snap_a = await_snapshot(&a, "a sendable", |s| s.sendable).await;
    assert!(snap_a.sendable);

    // B is unaffected by A's progress.
    let snap_b = b.handle.snapshot();
    assert!(!snap_b.sendable, "controller B must be isolated from A");
    assert_eq!(snap_b.peer, test_actor_id(11));

    // Explicitly close A; B remains independently controlled.
    a.handle.close().await.unwrap();
    let _ = await_snapshot(&a, "a idle", |s| {
        s.status == actr_framework::WebRtcPeerStatus::Idle
    })
    .await;
    assert!(!b.handle.snapshot().sendable);
    assert_eq!(b.handle.snapshot().peer, test_actor_id(11));
}

/// N1 regression: the controller task must converge on a *terminal* Close even
/// when a stale handle clone still holds the command channel open. Before the
/// fix, the run loop only exited on command-channel closure (every handle
/// dropped), so a leaked clone hung `close_peer`'s join.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_exits_on_terminal_close_despite_leaked_handle() {
    let mut h = spawn_harness(1);
    h.handle.connect().await.expect("send connect");
    assign_role(&mut h, true).await;
    let build = next_build(&mut h).await;
    succeed_build(build);

    // Leak a handle clone that is never dropped: it keeps the command channel
    // open even after the original handle is closed.
    let leaked = h.handle.clone();

    h.handle.close().await.expect("send close");

    // The controller must exit on the terminal close barrier, not wait for the
    // (still-open) command channel to close.
    timeout(WAIT, h._task)
        .await
        .expect("controller did not exit on terminal close (leaked handle)")
        .expect("controller task panicked");

    drop(leaked);
}

/// N2 backpressure: a full shared signaling queue must not stall the controller
/// (one peer's storm never blocks another), and drops are counted in the
/// `signaling_dropped` metric rather than silently lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_signaling_queue_drops_and_counts_without_stalling() {
    // A capacity-1 signaling queue that is never drained, so it fills after the
    // first signaling message.
    let mut h = spawn_harness_with_signaling_capacity(1, 1);

    // Connect sends RoleNegotiation, filling the queue.
    h.handle.connect().await.expect("send connect");

    // Drive the offerer to send an offer (and beyond) on the full queue. These
    // sends must drop (counted) rather than block the event loop.
    h.handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer");
    let build = next_build(&mut h).await;
    succeed_build(build);

    // The controller must stay responsive on the full queue: it keeps producing
    // snapshots and the drop metric climbs past zero. A stalling controller
    // (blocking `.send().await`) would never get here.
    let snap = timeout(WAIT, async {
        loop {
            let snap = h.handle.snapshot();
            if snap.signaling_dropped >= 1 {
                return snap;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("signaling_dropped metric did not increment on a full queue");

    assert!(
        snap.signaling_dropped >= 1,
        "expected dropped signaling to be counted, got {}",
        snap.signaling_dropped
    );

    // The controller is still alive and obeys a terminal close (no stall).
    h.handle.close().await.expect("send close");
    timeout(WAIT, h._task)
        .await
        .expect("controller stalled on the full signaling queue")
        .expect("controller task panicked");
}

/// N2 follow-up: critical SDP retained on a full queue must be retried once
/// the queue drains — the reducer already marked the attempt announced, so a
/// plain drop would strand negotiation (no attempt reset, `Connect` ignored
/// outside `Idle`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_queue_retains_critical_sdp_and_retries_once_drained() {
    // Capacity-1 queue; the initial RoleNegotiation fills it.
    let mut h = spawn_harness_with_signaling_capacity(1, 1);
    h.handle.connect().await.expect("send connect");
    h.handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer");
    let build = next_build(&mut h).await;
    succeed_build(build);

    // The offer hits the full queue: counted as dropped-observable, but now
    // retained instead of lost.
    timeout(WAIT, async {
        loop {
            if h.handle.snapshot().signaling_dropped >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("full queue did not increment signaling_dropped");

    // Drain the queued RoleNegotiation to make capacity.
    let Ok(Some((_peer, first))) = timeout(WAIT, h.signaling_rx.recv()).await else {
        panic!("expected the queued RoleNegotiation");
    };
    assert!(matches!(first, SignalingMessage::RoleNegotiation));

    // Wake the event loop with any command; the retained offer must be
    // retried through the freed queue.
    h.handle.connect().await.expect("wake the event loop");
    let Ok(Some((_peer, retried))) = timeout(WAIT, h.signaling_rx.recv()).await else {
        panic!("retained SDP was not retried after the queue drained");
    };
    assert!(
        matches!(retried, SignalingMessage::Offer { .. }),
        "expected the retained offer, got {retried:?}"
    );

    h.handle.close().await.expect("send close");
}

/// Stage 4B/4C: the lane pump is the single poll() consumer, covering every
/// DataChannelEvent — OnOpen/OnClose to the controller as LaneEvents, OnMessage
/// payloads to the InboundRouter (data plane); buffered-amount events are
/// ignored without stalling the pump.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lane_pump_routes_all_events_to_inbound() {
    use super::effects::test_support::scripted_data_channel;
    use crate::wire::webrtc::v020::inbound::{InboundMessage, InboundRouter};
    use std::sync::Mutex as StdMutex;
    use webrtc::data_channel::{DataChannelEvent, RTCDataChannelMessage};

    let peer = test_actor_id(1);
    let received: std::sync::Arc<StdMutex<Vec<InboundMessage>>> =
        std::sync::Arc::new(StdMutex::new(Vec::new()));
    let received2 = received.clone();
    let inbound = std::sync::Arc::new(InboundRouter::new(std::sync::Arc::new(
        move |msg, _lease| {
            received2.lock().unwrap().push(msg);
        },
    )));
    let (lane_tx, mut lane_rx) = mpsc::channel(16);

    // The pump expects every DataChannel message to carry the 8-byte fragment
    // header (wire-compatible with 0.14). Frame the test payloads so the pump
    // can decode and reassemble them, then assert the stripped payloads.
    fn frame_payload(msg_id: u32, payload: &[u8]) -> bytes::BytesMut {
        use crate::transport::{FRAGMENT_HEADER_SIZE, encode_fragment_header};
        let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + payload.len());
        encode_fragment_header(&mut buf, msg_id, 0, 1);
        buf.extend_from_slice(payload);
        bytes::BytesMut::from(&buf[..])
    }

    let msg1 = RTCDataChannelMessage {
        is_string: false,
        data: frame_payload(0, b"hello"),
    };
    let msg2 = RTCDataChannelMessage {
        is_string: false,
        data: frame_payload(1, &[0x01, 0x02]),
    };
    let channel = scripted_data_channel(
        "RPC_RELIABLE",
        vec![
            DataChannelEvent::OnOpen,
            DataChannelEvent::OnMessage(msg1),
            DataChannelEvent::OnBufferedAmountHigh,
            DataChannelEvent::OnBufferedAmountLow,
            DataChannelEvent::OnMessage(msg2),
            DataChannelEvent::OnClose,
        ],
    );

    let pump = tokio::spawn(super::session::lane_pump(
        1,
        LaneKind::RpcReliable,
        channel,
        super::session::LanePlumbing {
            peer_id: peer.clone(),
            lane_tx,
            inbound,
            reassembly_limits: crate::transport::ReassemblyLimits::default(),
        },
    ));

    // The lane becomes Open, then Closed as the script plays out.
    let open = timeout(WAIT, lane_rx.recv())
        .await
        .expect("lane open event");
    assert!(matches!(
        open,
        Some(LaneEvent::Opened {
            generation: 1,
            lane: LaneKind::RpcReliable
        })
    ));
    let closed = timeout(WAIT, lane_rx.recv())
        .await
        .expect("lane close event");
    assert!(matches!(
        closed,
        Some(LaneEvent::Closed {
            generation: 1,
            lane: LaneKind::RpcReliable
        })
    ));

    // The pump exits after OnClose.
    timeout(WAIT, pump)
        .await
        .expect("pump should exit on close")
        .expect("pump panicked");

    // Inbound received both payloads in order, tagged with peer/generation/lane.
    let got = received.lock().unwrap();
    assert_eq!(got.len(), 2, "both messages routed to inbound");
    assert_eq!(got[0].peer, peer);
    assert_eq!(got[0].generation, 1);
    assert_eq!(got[0].lane, LaneKind::RpcReliable);
    assert_eq!(got[0].payload.as_ref(), b"hello");
    assert_eq!(got[1].payload.as_ref(), &[0x01, 0x02]);
}

/// R7/item 11: `session.close()` explicitly calls each owned
/// `DataChannel::close()` (RFC work item 11: "Shutdown explicitly calls each
/// `DataChannel::close()`") and cancels the pump tasks registered in its
/// JoinSet. The pump stops consuming events after close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_close_explicitly_closes_owned_channels_and_cancels_pumps() {
    use super::effects::test_support::recording_data_channel;
    use std::sync::atomic::Ordering;

    let peer = test_actor_id(1);
    let ops: Arc<dyn SessionOps> = Arc::new(super::effects::test_support::NoopSessionOps);
    let mut session = super::session::PeerConnectionSession::new(1, ops);

    // Register two channels with close-call counters.
    let ch_reliable = recording_data_channel("RPC_RELIABLE");
    let ch_signal = recording_data_channel("RPC_SIGNAL");
    let (lane_tx, mut lane_rx) = mpsc::channel(16);
    let inbound = std::sync::Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::discard());
    let (task_started_tx, task_started_rx) = tokio::sync::oneshot::channel();
    let (task_dropped_tx, task_dropped_rx) = tokio::sync::oneshot::channel();
    session.spawn_task(async move {
        struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for DropSignal {
            fn drop(&mut self) {
                if let Some(tx) = self.0.take() {
                    let _ = tx.send(());
                }
            }
        }
        let _signal = DropSignal(Some(task_dropped_tx));
        let _ = task_started_tx.send(());
        std::future::pending::<()>().await;
    });
    task_started_rx.await.expect("session task did not start");

    session.register_channel(
        LaneKind::RpcReliable,
        ch_reliable.clone(),
        super::session::LanePlumbing {
            peer_id: peer.clone(),
            lane_tx: lane_tx.clone(),
            inbound: inbound.clone(),
            reassembly_limits: crate::transport::ReassemblyLimits::default(),
        },
    );
    session.register_channel(
        LaneKind::RpcSignal,
        ch_signal.clone(),
        super::session::LanePlumbing {
            peer_id: peer.clone(),
            lane_tx,
            inbound,
            reassembly_limits: crate::transport::ReassemblyLimits::default(),
        },
    );

    // Both pumps report OnOpen.
    for _ in 0..2 {
        let event = timeout(WAIT, lane_rx.recv())
            .await
            .expect("lane open event");
        assert!(matches!(event, Some(LaneEvent::Opened { .. })));
    }

    // Close the session: must explicitly close each channel and abort pumps.
    session.close().await.expect("session close");
    task_dropped_rx
        .await
        .expect("session.close() returned before its owned tasks converged");

    assert_eq!(
        ch_reliable.close_calls.load(Ordering::SeqCst),
        1,
        "session.close() must explicitly call DataChannel::close() on RpcReliable"
    );
    assert_eq!(
        ch_signal.close_calls.load(Ordering::SeqCst),
        1,
        "session.close() must explicitly call DataChannel::close() on RpcSignal"
    );
}

// ── S5-1: RecoverySupervisor broadcast → controller ──────────────────────────

#[tokio::test]
async fn network_event_broadcast_reaches_controller_and_bumps_revision() {
    use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;

    // The harness creates its own supervisor; we publish to it and verify the
    // controller's snapshot revision bumps (the reducer increments
    // `network_events_received` and bumps the revision on every NetworkEvent).
    let harness = spawn_harness(99);
    let before = harness.handle.snapshot().revision;

    assert_eq!(
        harness
            ._supervisor
            .publish(RtcNetworkEvent::NetworkUnavailable),
        1,
        "exactly one controller is subscribed"
    );

    // The controller's event loop selects on the broadcast receiver and feeds
    // the event to the reducer; the reducer bumps the revision.
    let snap = tokio::time::timeout(WAIT, async {
        loop {
            let s = harness.handle.snapshot();
            if s.revision > before {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("snapshot revision bumped after network event broadcast");

    assert!(snap.revision > before);
    // Clean up: drop the handle so the controller exits.
    harness.handle.close().await.unwrap();
}

// ── S6-2: media topology (add/remove track) + renegotiation ────────────────

/// Drive the harness through the offerer flow to Sendable; returns the build
/// handler's mailbox for driving further driver events.
async fn drive_to_sendable(h: &mut Harness) -> crate::wire::webrtc::v020::events::PeerEventMailbox {
    h.handle.connect().await.expect("send connect");
    assign_role(h, true).await;
    let build = next_build(h).await;
    let handler = build.handler.clone();
    succeed_build(build);

    // Consume the initial offer, answer it, connect transport, open anchor.
    let (_peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("initial offer")
        .expect("signaling closed");
    assert!(matches!(message, SignalingMessage::Offer { .. }));
    h.handle
        .apply_remote_description(desc(RTCSdpType::Answer))
        .await
        .expect("send answer");
    handler
        .mailbox()
        .try_publish(PeerDriverEvent::PeerConnectionStateChanged {
            generation: 1,
            state: RTCPeerConnectionState::Connected,
        });
    h.lane_tx
        .send(LaneEvent::Opened {
            generation: 1,
            lane: LaneKind::RpcReliable,
        })
        .await
        .expect("send lane open");
    await_snapshot(h, "sendable", |s| s.sendable).await;
    handler.mailbox()
}

#[tokio::test]
async fn lagged_network_receiver_expires_sendability_and_recovers_from_retained_tail() {
    use crate::wire::webrtc::v020::recovery::{NETWORK_EVENT_CAPACITY, RtcNetworkEvent};

    let mut h = spawn_harness(100);
    let _mailbox = drive_to_sendable(&mut h).await;

    // This test uses the current-thread runtime and does not await while
    // publishing, so the controller cannot drain the broadcast queue. Evict
    // the leading NetworkUnavailable edge and retain NetworkAvailable facts.
    h._supervisor.publish(RtcNetworkEvent::NetworkUnavailable);
    for _ in 0..NETWORK_EVENT_CAPACITY + 10 {
        h._supervisor.publish(RtcNetworkEvent::NetworkAvailable);
    }

    let (_peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("ICE restart offer after lag recovery")
        .expect("signaling closed");
    assert!(
        matches!(message, SignalingMessage::IceRestartOffer { .. }),
        "expected an ICE restart offer, got {message:?}"
    );
    assert_eq!(h.handle.snapshot().pc_generation, 1);

    h.handle.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn add_track_on_active_session_returns_sender() {
    let mut h = spawn_harness(1);
    let _mailbox = drive_to_sendable(&mut h).await;

    let track = Arc::new(
        crate::wire::webrtc::v020::media::test_support::FakeTrackLocal::new("local-video"),
    );
    let sender = h
        .handle
        .add_track(track)
        .await
        .expect("add_track on the active session");
    assert_eq!(
        sender.id(),
        rtc::rtp_transceiver::RTCRtpSenderId::from(1usize)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn add_track_without_active_session_is_not_ready() {
    let h = spawn_harness(2);
    let track = Arc::new(
        crate::wire::webrtc::v020::media::test_support::FakeTrackLocal::new("local-video"),
    );
    let err = h
        .handle
        .add_track(track)
        .await
        .err()
        .expect("add_track without a session must fail");
    assert!(err.contains("not ready"), "unexpected error: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remove_track_after_add_succeeds() {
    let mut h = spawn_harness(3);
    let _mailbox = drive_to_sendable(&mut h).await;

    let track = Arc::new(
        crate::wire::webrtc::v020::media::test_support::FakeTrackLocal::new("local-video"),
    );
    let sender = h.handle.add_track(track).await.expect("add track");
    h.handle
        .remove_track(sender)
        .await
        .expect("remove_track on the active session");
}

/// S6-2: track add/remove serializes renegotiation through the controller —
/// after a topology change, the driver's NegotiationNeeded produces exactly
/// one new offer on the signaling channel (offerer, sendable).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn negotiation_needed_after_topology_change_produces_one_offer() {
    let mut h = spawn_harness(4);
    let mailbox = drive_to_sendable(&mut h).await;

    let track = Arc::new(
        crate::wire::webrtc::v020::media::test_support::FakeTrackLocal::new("local-video"),
    );
    h.handle.add_track(track).await.expect("add track");

    // The driver fires NegotiationNeeded for the new track; the controller
    // must emit exactly one renegotiation offer.
    mailbox.try_publish(PeerDriverEvent::NegotiationNeeded { generation: 1 });
    let (peer, message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("renegotiation offer")
        .expect("signaling closed");
    assert_eq!(peer, test_actor_id(4));
    assert!(
        matches!(message, SignalingMessage::Offer { .. }),
        "expected a renegotiation offer, got {message:?}"
    );
}
