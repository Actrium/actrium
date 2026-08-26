//! Manager tests: handle registry, idempotent ensure, answerer-on-offer,
//! query/subscribe, close-peer/close-all convergence, and lock discipline.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use actr_protocol::{ActrId, ActrType, Realm};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::timeout;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidateInit, RTCPeerConnectionState, RTCSdpType,
    RTCSessionDescription,
};

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::v020::events::PeerDriverEvent;
use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::peer::effects::{SessionFactory, SessionOps};
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;
use crate::wire::webrtc::v020::recovery::RtcNetworkEvent;

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

#[derive(Default)]
struct FakeOps {
    close_calls: Option<Arc<AtomicUsize>>,
    block_first_close: bool,
    /// When set, every `restart_ice()` call signals `entered` and then waits
    /// for a `release` permit before returning (see
    /// `ice_restarts_are_not_serialized_by_a_node_wide_limit`).
    block_restart: Option<RestartGate>,
}

/// Shared gate for `FakeOps::restart_ice`: each call signals `entered`, then
/// waits on `release`, so tests can observe restart starts and hold them.
#[derive(Clone)]
struct RestartGate {
    entered: Arc<tokio::sync::Semaphore>,
    release: Arc<tokio::sync::Semaphore>,
}

impl RestartGate {
    fn new() -> Self {
        Self {
            entered: Arc::new(tokio::sync::Semaphore::new(0)),
            release: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }
}

#[async_trait]
impl SessionOps for FakeOps {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
        // The signaling router classifies the local description by its SDP
        // type; report a real Offer so the offerer path emits
        // `SignalingMessage::Offer`.
        let mut description = RTCSessionDescription::default();
        description.sdp_type = RTCSdpType::Offer;
        Ok(description)
    }
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
        let mut description = RTCSessionDescription::default();
        description.sdp_type = RTCSdpType::Answer;
        Ok(description)
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
        if let Some(gate) = &self.block_restart {
            gate.entered.add_permits(1);
            gate.release
                .acquire()
                .await
                .expect("restart release semaphore")
                .forget();
        }
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
        Ok(crate::wire::webrtc::v020::peer::effects::test_support::fake_data_channel(label))
    }
    async fn add_track(
        &self,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, RtcError> {
        Err(RtcError::Other("no media in manager tests".into()))
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
        if let Some(calls) = &self.close_calls {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            if self.block_first_close && call == 0 {
                std::future::pending::<()>().await;
            }
        }
        Ok(())
    }
}

/// A factory that immediately succeeds (manager tests do not need build-timing
/// control).
struct AutoFactory;

#[async_trait]
impl SessionFactory for AutoFactory {
    async fn build(
        &self,
        _handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        Ok(Arc::new(FakeOps::default()))
    }
}

/// Factory whose build can be held across a manager close/create race.
struct BlockingBuildFactory {
    entered: Arc<tokio::sync::Semaphore>,
    release: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl SessionFactory for BlockingBuildFactory {
    async fn build(
        &self,
        _handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        self.entered.add_permits(1);
        self.release
            .acquire()
            .await
            .expect("build release semaphore")
            .forget();
        Ok(Arc::new(FakeOps::default()))
    }
}

/// A factory whose sessions succeed immediately except that every
/// `restart_ice()` blocks on the shared gate, so soft-recovery ICE restarts
/// can be observed and held per peer.
struct RestartBlockingFactory {
    gate: RestartGate,
    /// Per-generation event handlers, so the test can publish fake driver
    /// events into the real controllers through the production mailbox.
    handlers: Arc<std::sync::Mutex<Vec<Arc<WebRtcEventHandler>>>>,
}

#[async_trait]
impl SessionFactory for RestartBlockingFactory {
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        self.handlers
            .lock()
            .expect("restart factory handler registry")
            .push(handler);
        Ok(Arc::new(FakeOps {
            block_restart: Some(self.gate.clone()),
            ..FakeOps::default()
        }))
    }
}

struct FirstCloseBlocksFactory {
    build_calls: Arc<AtomicUsize>,
    close_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl SessionFactory for FirstCloseBlocksFactory {
    async fn build(
        &self,
        _handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        self.build_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FakeOps {
            close_calls: Some(self.close_calls.clone()),
            block_first_close: true,
            block_restart: None,
        }))
    }
}

fn manager() -> (
    Arc<RtcConnectionManager>,
    mpsc::Receiver<(
        ActrId,
        crate::wire::webrtc::v020::peer::reducer::SignalingMessage,
    )>,
) {
    let (signaling_tx, signaling_rx) = mpsc::channel(16);
    let manager = Arc::new(RtcConnectionManager::new(
        Arc::new(AutoFactory),
        signaling_tx,
    ));
    manager.supervisor().set_signaling_generation(Some(0));
    (manager, signaling_rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ensure_controller_is_idempotent_per_peer() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(1);

    let first = mgr.ensure_controller(peer.clone()).await;
    let second = mgr.ensure_controller(peer.clone()).await;

    // One live controller per peer (invariant #1): the same handle generation
    // state is shared, and the registry holds exactly one entry.
    assert_eq!(mgr.peer_count().await, 1);
    assert_eq!(first.snapshot().revision, second.snapshot().revision);

    // Concurrent ensures still converge to a single controller.
    let (a, b, c) = tokio::join!(
        mgr.ensure_controller(peer.clone()),
        mgr.ensure_controller(peer.clone()),
        mgr.ensure_controller(peer.clone()),
    );
    let _ = (a, b, c);
    assert_eq!(mgr.peer_count().await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn peer_connection_builds_are_not_serialized_by_a_node_wide_limit() {
    const PEERS: usize = 5;

    let entered = Arc::new(tokio::sync::Semaphore::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (signaling_tx, _signaling_rx) = mpsc::channel(32);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(BlockingBuildFactory {
            entered: entered.clone(),
            release: release.clone(),
        }),
        signaling_tx,
    ));
    mgr.supervisor().set_signaling_generation(Some(0));

    for serial in 1..=PEERS as u64 {
        let peer = test_actor_id(1_000 + serial);
        mgr.connect(peer.clone()).await;
        mgr.get_controller(&peer)
            .await
            .expect("controller")
            .apply_role_assignment(true)
            .await
            .expect("role assignment command");
    }

    for _ in 0..PEERS {
        timeout(WAIT, entered.acquire())
            .await
            .expect("all peer builds should start independently")
            .expect("build-entered semaphore")
            .forget();
    }

    release.add_permits(PEERS);
    mgr.close_all().await;
}

/// Soft-recovery ICE restarts must also run concurrently per peer: the
/// removed node-wide build/restart limiter must not sneak back in via the
/// restart path only. Peers are driven to Sendable through the real
/// controller effect flow (auto-succeeding `FakeOps`; its data channel
/// reports Open), then a network outage window starts Soft Recovery. Every
/// `restart_ice()` blocks on the shared gate until released, mirroring the
/// `BuildSession` coverage above.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn ice_restarts_are_not_serialized_by_a_node_wide_limit() {
    const PEERS: usize = 5;

    let gate = RestartGate::new();
    let entered = gate.entered.clone();
    let release = gate.release.clone();
    let handlers: Arc<std::sync::Mutex<Vec<Arc<WebRtcEventHandler>>>> = Arc::default();
    let (signaling_tx, mut signaling_rx) = mpsc::channel(32);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(RestartBlockingFactory {
            gate,
            handlers: handlers.clone(),
        }),
        signaling_tx,
    ));
    mgr.supervisor().set_signaling_generation(Some(0));

    let peers: Vec<_> = (1..=PEERS as u64)
        .map(|serial| test_actor_id(3_000 + serial))
        .collect();
    for peer in &peers {
        mgr.connect(peer.clone()).await;
        mgr.get_controller(peer)
            .await
            .expect("controller")
            .apply_role_assignment(true)
            .await
            .expect("role assignment command");
    }

    // Wait until every offerer has sent its initial offer: the readiness
    // anchor data channel then already exists (CreateLane precedes
    // CreateOffer), so its Open event can complete the Sendable transition.
    let mut offered: std::collections::HashSet<ActrId> = std::collections::HashSet::new();
    while offered.len() < PEERS {
        let (peer, message) = timeout(WAIT, signaling_rx.recv())
            .await
            .expect("initial signaling timeout")
            .expect("signaling channel closed");
        match message {
            SignalingMessage::Offer { .. } => {
                assert!(offered.insert(peer), "duplicate initial offer");
            }
            SignalingMessage::RoleNegotiation => {}
            other => panic!("unexpected initial signaling: {other:?}"),
        }
    }

    // Publish one Connected driver tick per peer through the production
    // handler mailbox: Negotiating -> TransportReady, and together with the
    // anchor lane's Open event each peer becomes Sendable (ever_connected).
    for peer in &peers {
        let handler = handlers
            .lock()
            .expect("restart factory handler registry")
            .iter()
            .rev()
            .find(|handler| handler.peer_id() == peer)
            .expect("build handler for peer")
            .clone();
        handler
            .mailbox()
            .try_publish(PeerDriverEvent::PeerConnectionStateChanged {
                generation: handler.generation(),
                state: RTCPeerConnectionState::Connected,
            });
    }

    for peer in &peers {
        let became_sendable = timeout(WAIT, async {
            while !mgr
                .get_snapshot(peer)
                .await
                .is_some_and(|snapshot| snapshot.sendable)
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .is_ok();
        assert!(
            became_sendable,
            "peer {} did not reach sendable",
            peer.to_string_repr()
        );
    }

    // A network outage window defers Soft Recovery on every peer;
    // availability then starts it independently (supervisor broadcast,
    // never coalesced).
    mgr.supervisor()
        .publish(RtcNetworkEvent::NetworkUnavailable);
    mgr.supervisor().publish(RtcNetworkEvent::NetworkAvailable);

    // All restarts must start independently: one held restart never gates
    // another peer's restart.
    for _ in 0..PEERS {
        timeout(WAIT, entered.acquire())
            .await
            .expect("all peer ICE restarts should start independently")
            .expect("restart-entered semaphore")
            .forget();
    }

    release.add_permits(PEERS);
    mgr.close_all().await;
}

/// RFC invariant #1: removing a registry entry before its controller has
/// converged permits a second live controller for the same ActrId. The manager
/// keeps a closing tombstone, so a concurrent ensure waits without holding the
/// global lock and creates the replacement only after the old task exits.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ensure_waits_for_inflight_close_before_recreating_peer() {
    let entered = Arc::new(tokio::sync::Semaphore::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (signaling_tx, _signaling_rx) = mpsc::channel(16);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(BlockingBuildFactory {
            entered: entered.clone(),
            release: release.clone(),
        }),
        signaling_tx,
    ));
    let peer = test_actor_id(90);

    mgr.connect(peer.clone()).await;
    let handle = mgr.get_controller(&peer).await.expect("controller");
    handle
        .apply_role_assignment(true)
        .await
        .expect("role assignment command");
    timeout(WAIT, entered.acquire())
        .await
        .expect("build did not start")
        .expect("build-entered semaphore")
        .forget();

    let close_mgr = mgr.clone();
    let close_peer = peer.clone();
    let close = tokio::spawn(async move { close_mgr.close_peer(&close_peer).await });
    timeout(WAIT, async {
        while !mgr.is_closing(&peer).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("manager did not install closing tombstone");

    let ensure_mgr = mgr.clone();
    let ensure_peer = peer.clone();
    let mut ensure = tokio::spawn(async move { ensure_mgr.ensure_controller(ensure_peer).await });
    assert!(
        timeout(Duration::from_millis(100), &mut ensure)
            .await
            .is_err(),
        "ensure must not publish a replacement while the old build/controller is live"
    );
    assert_eq!(
        mgr.peer_count().await,
        1,
        "closing tombstone stays registered"
    );

    release.add_permits(1);
    assert!(timeout(WAIT, close).await.unwrap().unwrap());
    let replacement = timeout(WAIT, ensure)
        .await
        .expect("replacement ensure did not resume")
        .expect("ensure task panicked");
    assert_eq!(replacement.peer_id(), &peer);
    assert_eq!(mgr.peer_count().await, 1);
    assert!(!mgr.is_closing(&peer).await);

    assert!(mgr.close_peer(&peer).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_close_peer_retires_its_closing_tombstone() {
    let entered = Arc::new(tokio::sync::Semaphore::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (signaling_tx, _signaling_rx) = mpsc::channel(16);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(BlockingBuildFactory {
            entered: entered.clone(),
            release,
        }),
        signaling_tx,
    ));
    let peer = test_actor_id(91);

    mgr.connect(peer.clone()).await;
    let handle = mgr.get_controller(&peer).await.expect("controller");
    handle
        .apply_role_assignment(true)
        .await
        .expect("role assignment command");
    timeout(WAIT, entered.acquire())
        .await
        .expect("build did not start")
        .expect("build-entered semaphore")
        .forget();

    let close_mgr = mgr.clone();
    let close_peer = peer.clone();
    let close = tokio::spawn(async move { close_mgr.close_peer(&close_peer).await });
    timeout(WAIT, async {
        while !mgr.is_closing(&peer).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("manager did not install closing tombstone");

    close.abort();
    let _ = close.await;

    let replacement = timeout(WAIT, mgr.ensure_controller(peer.clone()))
        .await
        .expect("cancelled close left a permanent closing tombstone");
    assert_eq!(replacement.peer_id(), &peer);
    assert!(!mgr.is_closing(&peer).await);
    assert!(mgr.close_peer(&peer).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn incoming_offer_creates_answerer_controller() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(2);

    // No controller yet.
    assert!(mgr.get_controller(&peer).await.is_none());

    // An incoming offer creates the answerer controller.
    let mut offer = RTCSessionDescription::default();
    offer.sdp_type = webrtc::peer_connection::RTCSdpType::Offer;
    mgr.handle_incoming_offer(peer.clone(), offer).await;

    assert!(mgr.get_controller(&peer).await.is_some());
    assert_eq!(mgr.peer_count().await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_creates_controller_and_drives_offer() {
    let (mgr, mut signaling_rx) = manager();
    let peer = test_actor_id(3);

    mgr.connect(peer.clone()).await;
    assert_eq!(mgr.peer_count().await, 1);

    // The offerer eventually emits an offer to the signaling channel.
    let (to, _msg) = timeout(WAIT, signaling_rx.recv())
        .await
        .expect("timed out waiting for an offer")
        .expect("signaling closed");
    assert_eq!(to, peer);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_peer_removes_and_converges() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(4);

    mgr.ensure_controller(peer.clone()).await;
    assert_eq!(mgr.peer_count().await, 1);

    assert!(mgr.close_peer(&peer).await, "peer existed");
    assert_eq!(mgr.peer_count().await, 0);
    assert!(!mgr.close_peer(&peer).await, "second close finds nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_peer_allows_fresh_offer_rebuild_with_monotone_lifecycle_floors() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(45);
    let retired = mgr.ensure_controller(peer.clone()).await;
    retired.connect().await.expect("connect command");
    retired
        .apply_role_assignment(true)
        .await
        .expect("role assignment command");
    let mut retired_rx = retired.subscribe();
    let retired_generation = timeout(WAIT, async {
        loop {
            let snapshot = retired_rx.borrow().clone();
            if snapshot.pc_generation > 0 {
                return snapshot.pc_generation;
            }
            retired_rx
                .changed()
                .await
                .expect("retired controller snapshot channel closed early");
        }
    })
    .await
    .expect("retired controller did not allocate a generation");
    assert!(mgr.close_peer(&peer).await);
    let retired_snapshot = retired.snapshot();

    let mut fresh_offer = RTCSessionDescription::default();
    fresh_offer.sdp_type = webrtc::peer_connection::RTCSdpType::Offer;
    mgr.handle_incoming_offer(peer.clone(), fresh_offer).await;
    let replacement = mgr
        .get_controller(&peer)
        .await
        .expect("fresh ordinary Offer must rebuild a per-peer close");
    let mut replacement_rx = replacement.subscribe();
    let replacement_snapshot = timeout(WAIT, async {
        loop {
            let snapshot = replacement_rx.borrow().clone();
            if snapshot.pc_generation > retired_generation
                && snapshot.revision > retired_snapshot.revision
            {
                return snapshot;
            }
            replacement_rx
                .changed()
                .await
                .expect("replacement controller snapshot channel closed early");
        }
    })
    .await
    .expect("replacement did not advance generation and revision floors");
    assert!(
        replacement_snapshot.pc_generation > retired_generation,
        "controller recreation must preserve the Manager generation floor"
    );
    assert!(
        replacement_snapshot.revision > retired_snapshot.revision,
        "controller recreation must preserve the public Hook revision floor"
    );
    assert!(mgr.close_peer(&peer).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_all_rejects_late_inbound_recreation_until_local_connect() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(5);
    mgr.ensure_controller(peer.clone()).await;
    mgr.close_all().await;

    let mut late_offer = RTCSessionDescription::default();
    late_offer.sdp_type = webrtc::peer_connection::RTCSdpType::Offer;
    mgr.handle_incoming_offer(peer.clone(), late_offer).await;
    mgr.handle_role_assignment(peer.clone(), true).await;
    assert!(
        mgr.get_controller(&peer).await.is_none(),
        "late signaling from the retired Session must not revive a peer"
    );

    mgr.connect(peer.clone()).await;
    assert!(
        mgr.get_controller(&peer).await.is_some(),
        "an explicit local connect must reopen inbound admission"
    );
    assert!(mgr.close_peer(&peer).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn new_signaling_generation_reopens_passive_inbound_admission() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(55);
    mgr.ensure_controller(peer.clone()).await;
    mgr.close_all().await;

    mgr.reopen_all_inbound_admission();
    let mut offer = RTCSessionDescription::default();
    offer.sdp_type = webrtc::peer_connection::RTCSdpType::Offer;
    mgr.handle_incoming_offer(peer.clone(), offer).await;

    assert!(
        mgr.get_controller(&peer).await.is_some(),
        "a fresh signaling generation must admit a passive answerer"
    );
    assert!(mgr.close_peer(&peer).await);
}

#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_all_wins_against_inbound_offer_paused_before_publication() {
    let (mgr, _rx) = manager();
    let mgr = Arc::new(mgr);
    let peer = test_actor_id(6);
    mgr.ensure_controller(peer.clone()).await;

    let publish = Arc::new(tokio::sync::Barrier::new(2));
    mgr.set_peer_publish_barrier_for_test(peer.clone(), publish.clone());
    let inbound_mgr = mgr.clone();
    let inbound_peer = peer.clone();
    let inbound = tokio::spawn(async move {
        let mut offer = RTCSessionDescription::default();
        offer.sdp_type = webrtc::peer_connection::RTCSdpType::Offer;
        inbound_mgr.handle_incoming_offer(inbound_peer, offer).await;
    });

    // The inbound dispatch has captured the old lifecycle epoch but has not
    // entered controller publication. Node-wide close must establish the peer
    // tombstone before that dispatch is released.
    publish.wait().await;
    mgr.close_all().await;
    publish.wait().await;
    inbound.await.expect("inbound dispatch panicked");

    assert!(
        mgr.get_controller(&peer).await.is_none(),
        "an inbound dispatch admitted before close must not publish after close"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_all_converges_every_controller() {
    let (mgr, _rx) = manager();
    for serial in 10..15 {
        mgr.connect(test_actor_id(serial)).await;
    }
    assert_eq!(mgr.peer_count().await, 5);

    // close_all must converge (not hang) even with builds/offers in flight.
    timeout(WAIT, mgr.close_all())
        .await
        .expect("close_all did not converge in time");
    assert_eq!(mgr.peer_count().await, 0);
}

/// RFC shutdown fallback: if a Session close hangs until the manager's overall
/// deadline, aborting the Controller must not reduce cleanup to Drop. The
/// retained effect port aborts the stuck close effect, resumes the in-place
/// Session close, and converges physical ownership before retiring the peer.
#[tokio::test(start_paused = true)]
async fn close_all_deadline_retries_explicit_session_cleanup_after_controller_abort() {
    let build_calls = Arc::new(AtomicUsize::new(0));
    let close_calls = Arc::new(AtomicUsize::new(0));
    let (signaling_tx, _signaling_rx) = mpsc::channel(16);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(FirstCloseBlocksFactory {
            build_calls: build_calls.clone(),
            close_calls: close_calls.clone(),
        }),
        signaling_tx,
    ));
    let peer = test_actor_id(16);

    let handle = mgr.ensure_controller(peer).await;
    handle.connect().await.expect("connect command");
    handle
        .apply_role_assignment(true)
        .await
        .expect("role assignment command");
    for _ in 0..8 {
        if build_calls.load(Ordering::SeqCst) != 0 {
            break;
        }
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
    }
    assert_eq!(build_calls.load(Ordering::SeqCst), 1);
    // Let the effect task register the returned Session and let the Controller
    // consume its build completion before close-all starts.
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }

    let close_mgr = mgr.clone();
    let close = tokio::spawn(async move { close_mgr.close_all().await });
    // Wait until the primary close effect is demonstrably blocked. At this
    // point close-all's enclosing 10 s timer has also been polled and armed.
    for _ in 0..100 {
        if close_calls.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(close_calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(11)).await;
    // The deadline aborts the Controller/close-effect task; explicit fallback
    // must then re-enter Session::close_in_place and make the second call.
    for _ in 0..100 {
        if close_calls.load(Ordering::SeqCst) == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(close_calls.load(Ordering::SeqCst), 2);
    // Also cross the bounded fallback deadline in case lower-level task
    // cancellation needs another runtime turn to converge.
    tokio::time::advance(Duration::from_secs(3)).await;
    close.await.expect("close-all task panicked");

    assert_eq!(mgr.peer_count().await, 0);
    assert_eq!(
        close_calls.load(Ordering::SeqCst),
        2,
        "fallback must retry the cancelled explicit close, not rely on Drop"
    );
}

/// A single failed peer must not pin its closing tombstone forever. This is the
/// per-peer counterpart of the close-all fallback: abort the stuck controller,
/// resume explicit Session cleanup, then allow a fresh controller generation.
#[tokio::test(start_paused = true)]
async fn close_peer_deadline_retries_cleanup_and_allows_recreation() {
    let build_calls = Arc::new(AtomicUsize::new(0));
    let close_calls = Arc::new(AtomicUsize::new(0));
    let (signaling_tx, _signaling_rx) = mpsc::channel(16);
    let mgr = Arc::new(RtcConnectionManager::new(
        Arc::new(FirstCloseBlocksFactory {
            build_calls: build_calls.clone(),
            close_calls: close_calls.clone(),
        }),
        signaling_tx,
    ));
    let peer = test_actor_id(17);

    let handle = mgr.ensure_controller(peer.clone()).await;
    handle.connect().await.expect("connect command");
    handle
        .apply_role_assignment(true)
        .await
        .expect("role assignment command");
    for _ in 0..8 {
        if build_calls.load(Ordering::SeqCst) != 0 {
            break;
        }
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
    }
    assert_eq!(build_calls.load(Ordering::SeqCst), 1);
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }

    let close_mgr = mgr.clone();
    let close_peer = peer.clone();
    let close = tokio::spawn(async move { close_mgr.close_peer(&close_peer).await });
    for _ in 0..100 {
        if close_calls.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(close_calls.load(Ordering::SeqCst), 1);

    tokio::time::advance(Duration::from_secs(11)).await;
    for _ in 0..100 {
        if close_calls.load(Ordering::SeqCst) == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        close_calls.load(Ordering::SeqCst),
        2,
        "fallback must resume explicit Session close after aborting the controller"
    );
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(close.await.expect("close-peer task panicked"));
    assert_eq!(mgr.peer_count().await, 0);

    let replacement = mgr.ensure_controller(peer.clone()).await;
    assert_eq!(replacement.peer_id(), &peer);
    assert!(!mgr.is_closing(&peer).await);
    assert_eq!(mgr.peer_count().await, 1);
    assert!(mgr.close_peer(&peer).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshots_and_subscribe_reflect_controllers() {
    let (mgr, _rx) = manager();
    let peer = test_actor_id(20);
    mgr.ensure_controller(peer.clone()).await;

    let snapshot = mgr.get_snapshot(&peer).await.expect("snapshot exists");
    assert_eq!(snapshot.peer, peer);
    assert!(!snapshot.sendable);

    let list = mgr.list_snapshots().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].peer, peer);

    // Subscribing replays the latest snapshot immediately.
    let rx = mgr.subscribe(&peer).await.expect("subscribe exists");
    assert_eq!(rx.borrow().peer, peer);
    let public_rx = mgr
        .subscribe_public(&peer)
        .await
        .expect("public subscribe exists");
    assert_eq!(public_rx.borrow().peer, peer);
    assert_eq!(public_rx.borrow().revision, rx.borrow().revision);
}

/// S5-11: the unified observation facade aggregates recovery (snapshot),
/// resource (lane count), and teardown (converged flag, registry absence)
/// signals for the T2-T3 and system/platform harnesses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn observe_aggregates_recovery_resource_and_teardown_signals() {
    let (mgr, _rx) = manager();
    let a = test_actor_id(10);
    let b = test_actor_id(11);
    mgr.ensure_controller(a.clone()).await;
    mgr.ensure_controller(b.clone()).await;

    let obs = mgr.observe().await;
    assert_eq!(obs.peer_count, 2);
    assert_eq!(obs.peers.len(), 2);
    for peer_obs in &obs.peers {
        // A freshly created controller is terminal Idle (no live session):
        // converged, no lanes.
        assert!(peer_obs.converged, "fresh controller is terminal Idle");
        assert_eq!(peer_obs.live_lanes, 0);
        assert_eq!(peer_obs.snapshot.revision, 0);
    }

    // Teardown: a closed peer leaves the observation entirely.
    assert!(mgr.close_peer(&a).await);
    let obs = mgr.observe().await;
    assert_eq!(obs.peer_count, 1);
    assert_eq!(obs.peers.len(), 1);
    assert_eq!(obs.peers[0].snapshot.peer, b);
}
