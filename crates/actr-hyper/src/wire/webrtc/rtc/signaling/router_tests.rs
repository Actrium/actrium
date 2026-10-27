//! SignalingRouter tests: routing by remote ActrId, answerer creation on
//! unknown offers, restart-request acknowledgements, and role assignment.

use std::sync::Arc;

use actr_protocol::ActrId;
use async_trait::async_trait;
use tokio::sync::mpsc;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidateInit, RTCSdpType, RTCSessionDescription,
};

use crate::wire::webrtc::rtc::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
use crate::wire::webrtc::rtc::peer::adapter::ActrRtpSender;
use crate::wire::webrtc::rtc::peer::effects::{SessionFactory, SessionOps};
use crate::wire::webrtc::rtc::peer::reducer::SignalingMessage;
use crate::wire::webrtc::rtc::signaling::{SignalingDispatch, SignalingInbound};

use super::router::SignalingRouter;

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: "test@example.com".to_string(),
        serial_number,
    }
}

fn desc(sdp_type: RTCSdpType) -> RTCSessionDescription {
    let mut d = RTCSessionDescription::default();
    d.sdp_type = sdp_type;
    d.sdp = format!("fake-{sdp_type:?}");
    d
}

struct FakeOps;

#[async_trait]
impl SessionOps for FakeOps {
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
    ) -> Result<Arc<dyn crate::wire::webrtc::rtc::peer::adapter::LaneDataChannel>, RtcError> {
        Ok(crate::wire::webrtc::rtc::peer::effects::test_support::fake_data_channel(label))
    }
    async fn add_track(
        &self,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn ActrRtpSender>, RtcError> {
        Err(RtcError::Other("no media in router tests".into()))
    }
    async fn remove_track(&self, _sender: &Arc<dyn ActrRtpSender>) -> Result<(), RtcError> {
        Ok(())
    }
    async fn get_senders(&self) -> Vec<Arc<dyn ActrRtpSender>> {
        Vec::new()
    }
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
        Default::default()
    }
    async fn close(&self) -> Result<(), RtcError> {
        Ok(())
    }
}

struct AutoFactory;

#[async_trait]
impl SessionFactory for AutoFactory {
    async fn build(
        &self,
        _handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        Ok(Arc::new(FakeOps))
    }
}

fn router() -> (
    Arc<RtcConnectionManager>,
    SignalingRouter,
    mpsc::Receiver<SignalingDispatch>,
) {
    let (signaling_tx, signaling_rx) = mpsc::channel(16);
    let manager = Arc::new(RtcConnectionManager::new(
        Arc::new(AutoFactory),
        signaling_tx,
    ));
    manager.supervisor().set_signaling_generation(Some(0));
    let local_id = test_actor_id(100);
    let router = SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::rtc::signaling::SharedLocalId::new(local_id),
    );
    (manager, router, signaling_rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_offer_creates_answerer_controller() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(1);
    assert!(manager.get_controller(&peer).await.is_none());

    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Offer {
                desc: Box::new(desc(RTCSdpType::Offer)),
                exchange_id: Some("offer-1".to_string()),
            },
        )
        .await;

    assert!(
        manager.get_controller(&peer).await.is_some(),
        "an unknown offer must create an answerer controller"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_restart_request_returns_no_active_session_ack() {
    let (manager, router, mut signaling_rx) = router();
    let peer = test_actor_id(11);

    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::IceRestartRequest {
                request_id: "request-unknown-peer".to_string(),
                attempt: 2,
            },
        )
        .await;

    assert!(
        manager.get_controller(&peer).await.is_none(),
        "a request without a Controller must not implicitly create one"
    );
    let dispatch = signaling_rx
        .recv()
        .await
        .expect("NoActiveSession Ack should be emitted");
    assert_eq!(dispatch.peer, peer);
    assert!(dispatch.attempt.is_none(), "protocol Ack is untracked");
    assert!(matches!(
        dispatch.message,
        SignalingMessage::IceRestartRequestAck {
            request_id,
            attempt: 2,
            outcome,
        } if request_id == "request-unknown-peer"
            && outcome
                == actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession as i32
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_answerer_returns_wrong_role_restart_request_ack() {
    let (manager, router, mut signaling_rx) = router();
    let peer = test_actor_id(12);

    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::RoleAssignment { is_offerer: false },
        )
        .await;
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::IceRestartRequest {
                request_id: "request-answerer".to_string(),
                attempt: 1,
            },
        )
        .await;

    assert!(manager.get_controller(&peer).await.is_some());
    let dispatch = signaling_rx
        .recv()
        .await
        .expect("WrongRole Ack should be emitted");
    assert!(matches!(
        dispatch.message,
        SignalingMessage::IceRestartRequestAck {
            request_id,
            attempt: 1,
            outcome,
        } if request_id == "request-answerer"
            && outcome == actr_protocol::ice_restart_request_ack::Outcome::WrongRole as i32
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_offerer_returns_committed_accepted_restart_request_ack() {
    let (manager, router, mut signaling_rx) = router();
    let peer = test_actor_id(13);
    let handle = manager.ensure_controller(peer.clone()).await;
    handle.connect().await.expect("connect command");
    assert!(matches!(
        signaling_rx
            .recv()
            .await
            .expect("role negotiation probe")
            .message,
        SignalingMessage::RoleNegotiation
    ));
    handle
        .apply_role_assignment(true)
        .await
        .expect("offerer role assignment");

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let snapshot = handle.snapshot();
            if snapshot.role == Role::Offerer && snapshot.pc_generation > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("offerer Controller did not commit a Session generation");

    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::IceRestartRequest {
                request_id: "request-offerer".to_string(),
                attempt: 1,
            },
        )
        .await;

    let dispatch = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let dispatch = signaling_rx
                .recv()
                .await
                .expect("Accepted Ack should be emitted");
            if matches!(
                dispatch.message,
                SignalingMessage::IceRestartRequestAck { .. }
            ) {
                break dispatch;
            }
        }
    })
    .await
    .expect("Accepted Ack should be emitted");
    assert!(matches!(
        dispatch.message,
        SignalingMessage::IceRestartRequestAck {
            request_id,
            attempt: 1,
            outcome,
        } if request_id == "request-offerer"
            && outcome == actr_protocol::ice_restart_request_ack::Outcome::Accepted as i32
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answer_and_candidate_for_unknown_peer_are_dropped_safely() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(2);

    // These must not create a controller nor panic — just dropped.
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Answer {
                desc: Box::new(desc(RTCSdpType::Answer)),
                exchange_id: Some("answer-1".to_string()),
            },
        )
        .await;
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Candidate(RTCIceCandidateInit {
                candidate: "candidate:1 1 udp 1 192.0.2.1 9 typ host".to_string(),
                sdp_mid: None,
                sdp_mline_index: None,
                username_fragment: None,
                url: None,
            }),
        )
        .await;

    assert!(manager.get_controller(&peer).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_assignment_routes_to_controller() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(3);

    // An unsolicited assignment (the other peer negotiated) creates the
    // controller so it can record its role.
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::RoleAssignment { is_offerer: false },
        )
        .await;

    let handle = manager
        .get_controller(&peer)
        .await
        .expect("controller created for unsolicited role assignment");
    // The answerer role is recorded (snapshot is still Idle but the controller
    // exists and is not sendable).
    assert!(!handle.snapshot().sendable);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outbound_message_is_forwarded_by_run_loop() {
    let (manager, _router, mut signaling_rx) = router();

    // A controller that connects sends a RoleNegotiation on the shared outbound
    // channel the router consumes.
    manager.connect(test_actor_id(4)).await;
    let dispatch = tokio::time::timeout(std::time::Duration::from_secs(5), signaling_rx.recv())
        .await
        .expect("timed out waiting for outbound role negotiation")
        .expect("outbound channel closed");
    assert_eq!(dispatch.peer, test_actor_id(4));
    assert!(matches!(
        dispatch.message,
        SignalingMessage::RoleNegotiation
    ));
}

// ── async dispatch path: isolation, ordering, timeout, cap, shutdown ────────

use super::router::SignalingTransport;
use crate::wire::webrtc::rtc::peer::handle::RtcPeerControllerHandle;
use crate::wire::webrtc::rtc::peer::state::Role;
use tokio::time::timeout as wait_within;

/// A transport whose inbound stream is scripted by the test; outbound traffic
/// is swallowed.
struct ScriptedTransport {
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<(ActrId, SignalingMessage)>>,
}

#[async_trait]
impl SignalingTransport for ScriptedTransport {
    async fn send(&self, _to: ActrId, _message: SignalingMessage) -> Result<(), String> {
        Ok(())
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        self.inbound_rx.lock().await.recv().await
    }
}

fn inbound_offer(exchange: &str) -> SignalingMessage {
    SignalingMessage::Offer {
        desc: Box::new(desc(RTCSdpType::Offer)),
        exchange_id: Some(exchange.to_string()),
    }
}

/// A router whose `run` loop is live over a scripted inbound transport.
struct RunningRouter {
    manager: Arc<RtcConnectionManager>,
    router: SignalingRouter,
    inbound_tx: mpsc::Sender<(ActrId, SignalingMessage)>,
    run: tokio::task::JoinHandle<()>,
}

/// Small budget so cap tests exercise shedding without hundreds of tasks.
const TEST_DISPATCH_BUDGET: usize = 3;

fn spawn_running_router() -> RunningRouter {
    let (manager, router, outbound_rx) = router();
    // The production wiring installs the chains into the manager so peer
    // teardown releases entries.
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let (inbound_tx, inbound_rx) = mpsc::channel::<(ActrId, SignalingMessage)>(64);
    let transport = ScriptedTransport {
        inbound_rx: tokio::sync::Mutex::new(inbound_rx),
    };
    let runner = router.clone().with_dispatch_budget(TEST_DISPATCH_BUDGET);
    let task = tokio::spawn(async move {
        runner.run(transport, outbound_rx).await;
    });
    RunningRouter {
        manager,
        router,
        inbound_tx,
        run: task,
    }
}

async fn poll_controller(
    manager: &Arc<RtcConnectionManager>,
    peer: &ActrId,
    what: &str,
) -> RtcPeerControllerHandle {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(handle) = manager.get_controller(peer).await {
            return handle;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for controller ({what})"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn assert_no_controller_within(
    manager: &Arc<RtcConnectionManager>,
    peer: &ActrId,
    window: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + window;
    while std::time::Instant::now() < deadline {
        assert!(
            manager.get_controller(peer).await.is_none(),
            "controller unexpectedly created"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// A transport whose inbound stream is scripted with full `SignalingInbound`
/// events; outbound dispatches are captured for assertion.
struct InboundEventTransport {
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<SignalingInbound>>,
    outbound_tx: mpsc::Sender<SignalingDispatch>,
}

#[async_trait]
impl SignalingTransport for InboundEventTransport {
    async fn send(&self, _to: ActrId, _message: SignalingMessage) -> Result<(), String> {
        Ok(())
    }

    async fn send_dispatch(&self, dispatch: SignalingDispatch) -> Result<(), String> {
        let _ = self.outbound_tx.send(dispatch).await;
        Ok(())
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        loop {
            match self.inbound_rx.lock().await.recv().await? {
                SignalingInbound::PeerMessage { from, message, .. } => {
                    return Some((from, message));
                }
                SignalingInbound::TargetUnavailable { .. } => continue,
            }
        }
    }

    async fn recv_event(&self) -> Option<SignalingInbound> {
        self.inbound_rx.lock().await.recv().await
    }
}

/// The run loop must carry an inbound `TargetUnavailable` to the manager end
/// to end without retiring the live attempt. The server disposition is only a
/// point-in-time absence signal; the controller's bounded deadline remains the
/// terminal owner so a mobile peer can reconnect with the same ActrId.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_loop_preserves_attempt_after_target_unavailable() {
    let (manager, router, outbound_rx) = router();
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let (inbound_tx, inbound_rx) = mpsc::channel::<SignalingInbound>(64);
    let (captured_tx, mut captured_rx) = mpsc::channel::<SignalingDispatch>(64);
    let transport = InboundEventTransport {
        inbound_rx: tokio::sync::Mutex::new(inbound_rx),
        outbound_tx: captured_tx,
    };
    let runner = router.clone().with_dispatch_budget(TEST_DISPATCH_BUDGET);
    let run = tokio::spawn(async move {
        runner.run(transport, outbound_rx).await;
    });

    // A connected controller dispatches its tracked role probe through the
    // loop; the captured identity is exactly what Actrix would echo back.
    let peer = test_actor_id(42);
    manager.connect(peer.clone()).await;
    let dispatch = wait_within(std::time::Duration::from_secs(5), captured_rx.recv())
        .await
        .expect("timed out waiting for the loop to forward the probe")
        .expect("capture channel closed");
    assert!(matches!(
        dispatch.message,
        SignalingMessage::RoleNegotiation
    ));
    let attempt = dispatch.attempt.expect("connect probe is tracked");

    let handle = poll_controller(&manager, &peer, "connect creates a controller").await;
    inbound_tx
        .send(SignalingInbound::TargetUnavailable {
            target: peer.clone(),
            attempt,
        })
        .await
        .expect("feed target-unavailable result");

    // Closing the transport drains the queued event through the router. The
    // acknowledged connect command then acts as a FIFO controller barrier: if
    // TargetUnavailable had retired the attempt, Connect would allocate a new
    // transport epoch.
    drop(inbound_tx);
    let _joined = wait_within(std::time::Duration::from_secs(2), run)
        .await
        .expect("run must return when the transport closes");
    let committed_epoch = wait_within(std::time::Duration::from_secs(2), handle.connect())
        .await
        .expect("connect barrier timed out")
        .expect("controller stopped before the connect barrier");
    assert_eq!(committed_epoch, attempt.transport_epoch);
    let preserved = handle.snapshot();
    assert_eq!(
        preserved.status,
        actr_framework::WebRtcPeerStatus::Connecting
    );
    assert_eq!(preserved.unavailable_reason, None);
    drop(manager);
}

/// A transport that fails every send, for exercising the delivery-failure
/// feedback path end to end. Its inbound side parks until the test closes
/// it, so the run loop can be ended deterministically.
struct FailingTransport {
    closed: tokio::sync::watch::Receiver<bool>,
}

#[async_trait]
impl SignalingTransport for FailingTransport {
    async fn send(&self, _to: ActrId, _message: SignalingMessage) -> Result<(), String> {
        Err("simulated transport failure".to_string())
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        let mut closed = self.closed.clone();
        loop {
            if *closed.borrow() {
                return None;
            }
            if closed.changed().await.is_err() {
                return None;
            }
        }
    }
}

/// A transport send failure on critical SDP must travel the full wiring:
/// router outbound arm -> manager -> controller command -> reducer
/// `negotiation_failed`, terminating the exact generation. (Regression pin
/// for the #609 feedback path: dropping the router callback, the
/// critical-SDP filter, or the identity fence must fail this test.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_loop_feeds_transport_failure_back_into_the_exact_generation() {
    let (manager, router, outbound_rx) = router();
    manager.set_signaling_dispatch_chains(router.dispatch_chains());
    let (transport_close_tx, transport_close_rx) = tokio::sync::watch::channel(false);
    let runner = router.clone().with_dispatch_budget(TEST_DISPATCH_BUDGET);
    let run = tokio::spawn(async move {
        runner
            .run(
                FailingTransport {
                    closed: transport_close_rx,
                },
                outbound_rx,
            )
            .await;
    });

    // Drive an offerer to the offer stage. The role probe is not critical
    // SDP (its failed send must be ignored); the offer is, and its failure
    // must terminate the attempt.
    let peer = test_actor_id(43);
    manager.connect(peer.clone()).await;
    let handle = poll_controller(&manager, &peer, "connect creates a controller").await;
    handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = handle.snapshot();
        if snapshot.unavailable_reason
            == Some(actr_framework::WebRtcUnavailableReason::NegotiationFailed)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "transport failure must terminate the attempt, got {:?}",
            snapshot.unavailable_reason
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    handle.close().await.expect("close the peer");
    let _ = transport_close_tx.send(true);
    let _joined = wait_within(std::time::Duration::from_secs(2), run)
        .await
        .expect("run must return when the transport closes");
    drop(manager);
}

/// A stalled peer (its dispatch gate is closed by the test) must not block an
/// unrelated peer's dispatch, and must complete once released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_peer_dispatch_does_not_block_unrelated_peers() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stalled = test_actor_id(10);
    let other = test_actor_id(11);

    router.dispatch_chains().set_dispatch_gate(&stalled, false);
    inbound_tx
        .send((stalled.clone(), inbound_offer("stalled-1")))
        .await
        .expect("feed stalled inbound");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    inbound_tx
        .send((other.clone(), inbound_offer("other-1")))
        .await
        .expect("feed other inbound");

    poll_controller(&manager, &other, "unrelated peer dispatches while stalled").await;
    assert!(
        manager.get_controller(&stalled).await.is_none(),
        "the stalled dispatch must not have run"
    );

    router.dispatch_chains().set_dispatch_gate(&stalled, true);
    poll_controller(&manager, &stalled, "gated dispatch completes once opened").await;
}

/// Same-peer dispatches serialize in delivery order. An inbound Offer begins
/// an answerer initial attempt; a following `RoleAssignment { is_offerer:
/// true }` then supersedes that never-sendable attempt and restarts the peer
/// as the offerer on a fresh PC generation (see `restart_initial_as_offerer`).
/// Observing role `Offerer` with a bumped `pc_generation` therefore proves the
/// Offer was dispatched first: an assignment that jumped the queue would be a
/// no-op on the fresh controller (no attempt in flight), and the Offer would
/// leave an unmoved generation-1 answerer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_peer_dispatch_preserves_delivery_order() {
    let RunningRouter {
        manager,
        inbound_tx,
        ..
    } = spawn_running_router();
    let peer = test_actor_id(12);

    inbound_tx
        .send((peer.clone(), inbound_offer("ordered-1")))
        .await
        .expect("feed offer");
    inbound_tx
        .send((
            peer.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        ))
        .await
        .expect("feed follow-up assignment");
    // The chain orders both dispatches by construction; no gate needed.
    let handle = poll_controller(&manager, &peer, "serialized same-peer dispatch").await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = handle.snapshot();
        if snapshot.role == Role::Offerer && snapshot.pc_generation >= 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the offer must be dispatched first under serialized dispatch; role = {:?}, pc_generation = {}",
            snapshot.role,
            snapshot.pc_generation
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// A dispatch stalled past its 2s budget times out; the peer stays routable
/// on its next message.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_dispatch_times_out_and_peer_recovers() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stalled = test_actor_id(13);
    let other = test_actor_id(14);

    router.dispatch_chains().set_dispatch_gate(&stalled, false);
    inbound_tx
        .send((stalled.clone(), inbound_offer("stalled-1")))
        .await
        .expect("feed stalled inbound");
    inbound_tx
        .send((other.clone(), inbound_offer("other-1")))
        .await
        .expect("feed other inbound");
    poll_controller(&manager, &other, "unrelated peer stays routable").await;

    // The dispatch budget is 2s; wait past it.
    tokio::time::sleep(std::time::Duration::from_millis(2_300)).await;
    assert!(
        manager.get_controller(&stalled).await.is_none(),
        "the stalled dispatch must have timed out without routing"
    );
    router.dispatch_chains().set_dispatch_gate(&stalled, true);
    assert_no_controller_within(&manager, &stalled, std::time::Duration::from_millis(300)).await;

    // The timed-out message is dropped, not retried; the next one dispatches.
    inbound_tx
        .send((stalled.clone(), inbound_offer("stalled-2")))
        .await
        .expect("feed follow-up inbound");
    poll_controller(&manager, &stalled, "peer recovers after timeout").await;
}

/// Beyond the in-flight task budget the newest inbound messages are dropped
/// while stalled peers keep their slots (cap is 3 under test).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dispatch_task_cap_drops_newest_inbound_messages() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stalled = [test_actor_id(21), test_actor_id(22), test_actor_id(23)];
    let overflow = test_actor_id(24);

    for peer in &stalled {
        router.dispatch_chains().set_dispatch_gate(peer, false);
    }
    for (index, peer) in stalled.iter().enumerate() {
        inbound_tx
            .send((peer.clone(), inbound_offer(&format!("stalled-{index}"))))
            .await
            .expect("feed stalled inbound");
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    inbound_tx
        .send((overflow.clone(), inbound_offer("overflow")))
        .await
        .expect("feed overflow inbound");
    assert_no_controller_within(&manager, &overflow, std::time::Duration::from_millis(500)).await;

    for peer in &stalled {
        router.dispatch_chains().set_dispatch_gate(peer, true);
    }
    for peer in &stalled {
        poll_controller(&manager, peer, "capped peer dispatches after release").await;
    }
}

/// A budget-shed SDP-bearing message must not leave its peer's dispatch-chain
/// tail installed: while the budget is full, every later Offer/Answer for that
/// peer is shed before entering the chain, so a retained tail would strand
/// recovery signaling behind a stalled predecessor until slots expire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shed_forgoes_the_peers_stalled_chain_tail() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stalled = [test_actor_id(31), test_actor_id(32), test_actor_id(33)];
    let established = test_actor_id(34);

    // One dispatched offer leaves the established peer's chain entry behind
    // (its controller keeps the entry for teardown to release).
    inbound_tx
        .send((established.clone(), inbound_offer("established-1")))
        .await
        .expect("feed established offer");
    poll_controller(&manager, &established, "established peer dispatches").await;
    assert!(router.dispatch_chains().contains(&established));

    // Park three dispatches so the whole budget is busy.
    for peer in &stalled {
        router.dispatch_chains().set_dispatch_gate(peer, false);
    }
    for (index, peer) in stalled.iter().enumerate() {
        inbound_tx
            .send((peer.clone(), inbound_offer(&format!("stalled-{index}"))))
            .await
            .expect("feed stalled inbound");
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // The next offer from the established peer is shed: its chain tail must
    // be forgotten (and one escalation started, not more).
    inbound_tx
        .send((established.clone(), inbound_offer("established-2")))
        .await
        .expect("feed shed offer");
    // The shed branch also forgets the peer's chain tail — the durable
    // observable (the escalation itself may complete between polls).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while router.dispatch_chains().contains(&established) {
        assert!(
            std::time::Instant::now() < deadline,
            "the shed offer must release its peer's stalled chain tail"
        );
        assert!(
            router.escalating_peers_for_test() <= 1,
            "at most one escalation may exist for the shed peer"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    for peer in &stalled {
        router.dispatch_chains().set_dispatch_gate(peer, true);
    }
    for peer in &stalled {
        poll_controller(&manager, peer, "capped peer dispatches after release").await;
    }
}

/// Budget-shed escalations are bounded per peer: a flooding peer can never
/// accumulate concurrent escalation tasks, and the dedup slot is released
/// once the escalation finishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn budget_shed_escalations_dedup_per_peer() {
    let RunningRouter {
        manager: _manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stalled = test_actor_id(41);
    let flood = test_actor_id(42);

    // Park all three budget slots: chained same-peer dispatches each hold a
    // JoinSet slot while waiting on their gated predecessor.
    router.dispatch_chains().set_dispatch_gate(&stalled, false);
    for index in 0..3 {
        inbound_tx
            .send((stalled.clone(), inbound_offer(&format!("stalled-{index}"))))
            .await
            .expect("feed stalled inbound");
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Flood SDP-bearing messages from a controller-less peer: every one is
    // shed (the shed counter is the durable per-message observable), and at
    // most one escalation exists for the peer at any sampled instant.
    for index in 0..5 {
        inbound_tx
            .send((flood.clone(), inbound_offer(&format!("flood-{index}"))))
            .await
            .expect("feed flood inbound");
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while router.sheds_observed_for_test() < 5 {
        assert!(
            std::time::Instant::now() < deadline,
            "every flood message must be shed and counted"
        );
        assert!(
            router.escalating_peers_for_test() <= 1,
            "at most one escalation may exist for the flooding peer"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(router.sheds_observed_for_test(), 5);
    // Dedup slots clear once each escalation finishes.
    while router.escalating_peers_for_test() > 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "escalation dedup slots must clear once the escalation finishes"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(router.escalating_peers_for_test(), 0);

    router.dispatch_chains().set_dispatch_gate(&stalled, true);
}

/// Closing the transport drains/aborts parked dispatches and `run` returns
/// even while a dispatch is stalled on its lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_drains_and_returns_when_transport_closes() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        run,
    } = spawn_running_router();
    let stalled = test_actor_id(30);

    router.dispatch_chains().set_dispatch_gate(&stalled, false);
    inbound_tx
        .send((stalled.clone(), inbound_offer("stalled-1")))
        .await
        .expect("feed stalled inbound");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    drop(inbound_tx);
    let _joined = wait_within(std::time::Duration::from_secs(2), run)
        .await
        .expect("run must return even with a parked dispatch task");
    assert!(
        !router.dispatch_chains().contains(&stalled),
        "shutdown must leave no chain entry behind"
    );
    router.dispatch_chains().set_dispatch_gate(&stalled, true);
    drop(manager);
}

/// Dispatches for peers that never obtain a controller must not leave chain
/// entries behind; a peer whose Offer created a controller keeps its entry for
/// teardown. Request-only messages may still emit a protocol Ack.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_peer_dispatches_leave_no_chain_entries() {
    let RunningRouter {
        manager,
        router,
        inbound_tx,
        ..
    } = spawn_running_router();
    let stranger = test_actor_id(40);
    let offered = test_actor_id(41);

    inbound_tx
        .send((
            stranger.clone(),
            SignalingMessage::Answer {
                desc: Box::new(desc(RTCSdpType::Answer)),
                exchange_id: Some("stranger-answer".to_string()),
            },
        ))
        .await
        .expect("feed stranger answer");
    inbound_tx
        .send((offered.clone(), inbound_offer("offered-1")))
        .await
        .expect("feed offer");

    // Every router-level message kind that does not create a controller must
    // clean up its dispatch entry the same way.
    inbound_tx
        .send((
            stranger.clone(),
            SignalingMessage::Candidate(RTCIceCandidateInit {
                candidate: "candidate:1 1 udp 1 192.0.2.1 9 typ host".to_string(),
                sdp_mid: None,
                sdp_mline_index: None,
                username_fragment: None,
                url: None,
            }),
        ))
        .await
        .expect("feed stranger candidate");
    inbound_tx
        .send((
            stranger.clone(),
            SignalingMessage::IceRestartRequest {
                request_id: "req-1".to_string(),
                attempt: 1,
            },
        ))
        .await
        .expect("feed stranger restart request");

    poll_controller(&manager, &offered, "offer creates a controller").await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let stranger_gone = !router.dispatch_chains().contains(&stranger);
        let offered_kept = router.dispatch_chains().contains(&offered);
        if stranger_gone && offered_kept {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "stranger entry released={} offered entry kept={}",
            stranger_gone,
            offered_kept
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // The manager teardown handoff releases a live peer's entry.
    manager.close_peer(&offered).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if !router.dispatch_chains().contains(&offered) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "manager close_peer must release the peer's dispatch-chain entry"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
