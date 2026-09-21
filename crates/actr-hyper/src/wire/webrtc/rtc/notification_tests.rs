//! NotificationHub/projection tests: revision de-dup, panic isolation, and
//! watch-based fan-out with initial replay.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actr_framework::WebRtcPeerStatus;
use actr_protocol::{ActrId, ActrType, Realm};
use tokio::sync::watch;
use tokio::time::timeout;

use crate::wire::webrtc::rtc::notification::NotificationHub;
use crate::wire::webrtc::rtc::status::{NetworkPath, RtcPeerSnapshot};

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

fn snapshot(peer: ActrId, revision: u64, sendable: bool) -> RtcPeerSnapshot {
    RtcPeerSnapshot {
        peer,
        role: crate::wire::webrtc::rtc::peer::state::Role::Offerer,
        transport_epoch: 1,
        pc_generation: 1,
        ice_generation: 0,
        revision,
        status: if sendable {
            WebRtcPeerStatus::Connected
        } else {
            WebRtcPeerStatus::Connecting
        },
        sendable,
        network_path: NetworkPath::Unknown,
        unavailable_reason: None,
        signaling_dropped: 0,
        mailbox_overflows: 0,
    }
}

#[tokio::test]
async fn hub_dedups_by_peer_and_revision() {
    let hub = NotificationHub::new();
    let peer = test_actor_id(1);
    let hits = Arc::new(AtomicUsize::new(0));
    let hits2 = hits.clone();
    hub.register_observer(Arc::new(move |_snap| {
        let hits = hits2.clone();
        Box::pin(async move {
            hits.fetch_add(1, Ordering::SeqCst);
        })
    }))
    .await;

    // Newer revision dispatches.
    assert!(hub.publish(snapshot(peer.clone(), 1, false)).await);
    // Same revision is a duplicate.
    assert!(!hub.publish(snapshot(peer.clone(), 1, false)).await);
    // Older revision is dropped.
    assert!(!hub.publish(snapshot(peer.clone(), 0, false)).await);
    // Higher revision dispatches again.
    assert!(hub.publish(snapshot(peer.clone(), 2, true)).await);

    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "only newer revisions dispatch"
    );
}

#[tokio::test]
async fn hub_dedups_per_peer_independently() {
    let hub = NotificationHub::new();
    let a = test_actor_id(1);
    let b = test_actor_id(2);

    assert!(hub.publish(snapshot(a.clone(), 1, false)).await);
    // A different peer at the same revision is not a duplicate.
    assert!(hub.publish(snapshot(b.clone(), 1, false)).await);
    // But a repeat of peer A's revision is.
    assert!(!hub.publish(snapshot(a.clone(), 1, false)).await);
}

#[tokio::test]
async fn hub_accepts_initial_revision_zero() {
    let hub = NotificationHub::new();
    let peer = test_actor_id(1);
    assert!(hub.publish(snapshot(peer.clone(), 0, false)).await);
    assert!(!hub.publish(snapshot(peer, 0, false)).await);
}

#[tokio::test]
async fn hub_accepts_revision_zero_after_peer_controller_replacement() {
    let hub = NotificationHub::new();
    let peer = test_actor_id(1);
    assert!(hub.publish(snapshot(peer.clone(), 8, true)).await);

    hub.remove_peer(&peer);

    assert!(hub.publish(snapshot(peer.clone(), 0, false)).await);
    assert!(!hub.publish(snapshot(peer, 0, false)).await);
}

#[tokio::test]
async fn hub_isolates_a_panicking_observer() {
    let hub = NotificationHub::new();
    let peer = test_actor_id(1);
    let good_hits = Arc::new(AtomicUsize::new(0));
    let good = good_hits.clone();

    // A panicking observer first, then a healthy one.
    hub.register_observer(Arc::new(move |_snap| panic!("observer blew up")))
        .await;
    hub.register_observer(Arc::new(move |_snap| {
        let good = good.clone();
        Box::pin(async move {
            good.fetch_add(1, Ordering::SeqCst);
        })
    }))
    .await;

    // The panicking observer must not stop dispatch to the healthy one.
    assert!(hub.publish(snapshot(peer.clone(), 1, false)).await);
    assert!(hub.publish(snapshot(peer.clone(), 2, false)).await);
    assert_eq!(
        good_hits.load(Ordering::SeqCst),
        2,
        "healthy observer still receives snapshots after a peer observer panics"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_peer_replays_initial_then_forwards_changes() {
    let hub = Arc::new(NotificationHub::new());
    let peer = test_actor_id(1);
    let received = Arc::new(Mutex::new(Vec::new()));
    let received2 = received.clone();
    hub.register_observer(Arc::new(move |snap| {
        let received = received2.clone();
        Box::pin(async move {
            received.lock().unwrap().push(snap.revision);
        })
    }))
    .await;

    // Seed a controller snapshot channel at revision 5 before watching.
    let (tx, rx) = watch::channel(snapshot(peer.clone(), 5, false));
    let _forwarder = hub.watch_peer(rx);

    // Initial replay delivers revision 5.
    timeout(WAIT, async {
        loop {
            if !received.lock().unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("initial replay did not arrive");

    // A newer snapshot is forwarded; a duplicate revision is not.
    tx.send(snapshot(peer.clone(), 6, true)).unwrap();
    tx.send(snapshot(peer.clone(), 6, true)).unwrap();

    timeout(WAIT, async {
        loop {
            if received.lock().unwrap().len() >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("forwarded change did not arrive");

    let seq = received.lock().unwrap().clone();
    assert_eq!(seq, vec![5, 6], "initial replay then one deduped forward");
}

// ── S6-6: legacy three-hook projection (0.14 parity) ───────────────────────

use crate::wire::webrtc::HookEvent;
use crate::wire::webrtc::rtc::notification::project_legacy_hooks;

fn snap_with(peer: ActrId, status: WebRtcPeerStatus, sendable: bool) -> RtcPeerSnapshot {
    let mut s = snapshot(peer, 1, sendable);
    s.status = status;
    s
}

fn assert_hooks(events: &[HookEvent], expected: &[&'static str]) {
    let got: Vec<&'static str> = events
        .iter()
        .map(|e| match e {
            HookEvent::WebRtcConnectStart { .. } => "connect_start",
            HookEvent::WebRtcConnected { .. } => "connected",
            HookEvent::WebRtcDisconnected { .. } => "disconnected",
            _ => "other",
        })
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn projection_first_sight_connecting_fires_connect_start() {
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        None,
        false,
        &snap_with(peer, WebRtcPeerStatus::Connecting, false),
    );
    assert_hooks(&events, &["connect_start"]);
}

#[test]
fn projection_first_sight_idle_fires_nothing() {
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(None, false, &snap_with(peer, WebRtcPeerStatus::Idle, false));
    assert_hooks(&events, &[]);
}

#[test]
fn projection_first_sight_recovering_fires_disconnected() {
    // A late-installed observer learns the current truth.
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        None,
        false,
        &snap_with(peer, WebRtcPeerStatus::Recovering, false),
    );
    assert_hooks(&events, &["disconnected"]);
}

#[test]
fn projection_sendable_edge_fires_connected() {
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        Some(WebRtcPeerStatus::Connecting),
        false,
        &snap_with(peer, WebRtcPeerStatus::Connected, true),
    );
    assert_hooks(&events, &["connected"]);
}

#[test]
fn projection_recovery_reentry_into_connecting_fires_connect_start() {
    // 0.14 parity: a Hard Recovery rebuild re-enters Connecting and the hook
    // must fire again (the pre-S6-6 projection only fired for new peers).
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        Some(WebRtcPeerStatus::Recovering),
        false,
        &snap_with(peer, WebRtcPeerStatus::Connecting, false),
    );
    assert_hooks(&events, &["connect_start"]);
}

#[test]
fn projection_recovering_then_idle_fires_both_in_order() {
    // 0.14 fires Disconnected(Recovering) and Disconnected(Idle) in commit
    // order across two transitions.
    let peer = test_actor_id(1);
    let first = project_legacy_hooks(
        Some(WebRtcPeerStatus::Connected),
        true,
        &snap_with(peer.clone(), WebRtcPeerStatus::Recovering, false),
    );
    assert_hooks(&first, &["disconnected"]);
    let second = project_legacy_hooks(
        Some(WebRtcPeerStatus::Recovering),
        false,
        &snap_with(peer, WebRtcPeerStatus::Idle, false),
    );
    assert_hooks(&second, &["disconnected"]);
}

#[test]
fn projection_repeated_same_status_fires_nothing() {
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        Some(WebRtcPeerStatus::Connecting),
        false,
        &snap_with(peer, WebRtcPeerStatus::Connecting, false),
    );
    assert_hooks(&events, &[]);
}

#[test]
fn projection_staying_sendable_fires_nothing() {
    let peer = test_actor_id(1);
    let events = project_legacy_hooks(
        Some(WebRtcPeerStatus::Connected),
        true,
        &snap_with(peer, WebRtcPeerStatus::Connected, true),
    );
    assert_hooks(&events, &[]);
}

// ── S6-6c: Connected-epoch latch ─────────────────────────────────────────

use crate::wire::webrtc::rtc::notification::apply_connected_epoch_latch;

fn connected_event(peer: &ActrId) -> HookEvent {
    HookEvent::WebRtcConnected {
        peer_id: peer.clone(),
        relayed: false,
    }
}

fn disconnected_event(peer: &ActrId) -> HookEvent {
    HookEvent::WebRtcDisconnected {
        peer_id: peer.clone(),
        status: WebRtcPeerStatus::Recovering,
    }
}

#[test]
fn latch_suppresses_repeated_connected_within_epoch() {
    // A sendable flap without an intervening disconnect (renegotiation
    // under load) must not re-fire the connected hook (P14 regression).
    let peer = test_actor_id(1);
    let (first, latched) = apply_connected_epoch_latch(vec![connected_event(&peer)], false);
    assert_hooks(&first, &["connected"]);
    assert!(latched);
    let (second, latched) = apply_connected_epoch_latch(vec![connected_event(&peer)], latched);
    assert_hooks(&second, &[]);
    assert!(latched);
}

#[test]
fn latch_rearms_after_disconnect() {
    // A genuine disconnect/reconnect cycle must still surface both edges.
    let peer = test_actor_id(1);
    let (_, latched) = apply_connected_epoch_latch(vec![connected_event(&peer)], false);
    let (disconnected, latched) =
        apply_connected_epoch_latch(vec![disconnected_event(&peer)], latched);
    assert_hooks(&disconnected, &["disconnected"]);
    assert!(!latched);
    let (reconnected, latched) = apply_connected_epoch_latch(vec![connected_event(&peer)], latched);
    assert_hooks(&reconnected, &["connected"]);
    assert!(latched);
}

#[test]
fn latch_leaves_other_events_untouched() {
    let peer = test_actor_id(1);
    let (events, latched) = apply_connected_epoch_latch(
        vec![
            HookEvent::WebRtcConnectStart {
                peer_id: peer.clone(),
            },
            disconnected_event(&peer),
        ],
        true,
    );
    assert_hooks(&events, &["connect_start", "disconnected"]);
    assert!(!latched);
}

// ── S6-6b: transition-driven hook projection (end-to-end) ──────────────────

mod projection {
    use std::sync::Arc;
    use std::time::Duration;

    use actr_protocol::{AIdCredential, ActrId, ActrType, Realm};
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use crate::context::RuntimeContext;
    use crate::inbound::{DataChunkRegistry, MediaFrameRegistry};
    use crate::lifecycle::hooks::{HookContextBuilder, WorkloadHookObserverRef};
    use crate::outbound::{Gate, HostGate};
    use crate::transport::HostTransport;
    use crate::wire::webrtc::rtc::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
    use crate::wire::webrtc::rtc::notification::run_hook_projection;
    use crate::wire::webrtc::rtc::peer::effects::{SessionFactory, SessionOps};
    use crate::wire::webrtc::{
        ReconnectConfig, SignalingClient, SignalingConfig, WebSocketSignalingClient,
    };

    const WAIT: Duration = Duration::from_secs(5);

    fn test_id(serial: u64) -> ActrId {
        ActrId {
            realm: Realm { realm_id: 1 },
            serial_number: serial,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "node".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    struct AutoFactory;

    #[async_trait]
    impl SessionFactory for AutoFactory {
        async fn build(
            &self,
            _handler: Arc<WebRtcEventHandler>,
        ) -> Result<Arc<dyn SessionOps>, webrtc::error::Error> {
            Ok(crate::wire::webrtc::rtc::peer::effects::test_support::noop_session_ops())
        }
    }

    struct HookRecorder {
        tx: mpsc::UnboundedSender<String>,
    }

    #[async_trait]
    impl crate::lifecycle::hooks::WorkloadHookObserver for HookRecorder {
        async fn on_webrtc_connecting(
            &self,
            _ctx: &RuntimeContext,
            _event: &actr_framework::PeerEvent,
        ) {
            let _ = self.tx.send("connect_start".to_string());
        }
        async fn on_webrtc_state_changed(
            &self,
            _ctx: &RuntimeContext,
            state: &actr_framework::WebRtcPeerState,
        ) {
            let _ = self.tx.send(format!(
                "state:{:?}:sendable={}",
                state.status, state.sendable
            ));
        }
        async fn on_webrtc_disconnected(
            &self,
            _ctx: &RuntimeContext,
            event: &actr_framework::PeerEvent,
        ) {
            let _ = self.tx.send(format!("disconnected:{:?}", event.status));
        }
    }

    fn test_runtime_context() -> RuntimeContext {
        let host_transport = Arc::new(HostTransport::new());
        let inproc_gate = Gate::Host(Arc::new(HostGate::new(host_transport)));
        let signaling_client: Arc<dyn SignalingClient> =
            Arc::new(WebSocketSignalingClient::new(SignalingConfig {
                server_url: url::Url::parse("ws://127.0.0.1:9").expect("valid test URL"),
                connection_timeout: 1,
                heartbeat_interval: 30,
                reconnect_config: ReconnectConfig::default(),
                auth_config: None,
                webrtc_role: None,
                inbound_queue: crate::wire::webrtc::InboundQueueConfig::default(),
            }));
        RuntimeContext::new(
            test_id(1),
            None,
            "projection-test".to_string(),
            inproc_gate,
            None,
            Arc::new(DataChunkRegistry::new()),
            Arc::new(MediaFrameRegistry::new()),
            signaling_client,
            AIdCredential {
                key_id: 1,
                claims: bytes::Bytes::from_static(b"claims"),
                signature: bytes::Bytes::from(vec![0; 64]),
            },
            None,
            Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            None,
            0,
        )
    }

    async fn expect(rx: &mut mpsc::UnboundedReceiver<String>, wanted: &str) {
        let got = tokio::time::timeout(WAIT, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for hook {wanted:?}"))
            .expect("hook channel closed");
        assert_eq!(got, wanted);
    }

    async fn expect_until(rx: &mut mpsc::UnboundedReceiver<String>, wanted: &str) {
        tokio::time::timeout(WAIT, async {
            loop {
                let got = rx.recv().await.expect("hook channel closed");
                if got == wanted {
                    break;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for hook {wanted:?}"));
    }

    /// S6-6b: hooks fire from snapshot transitions (no polling), and a
    /// projection started after the peer exists replays the current state.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn projection_is_transition_driven_with_initial_replay() {
        let (signaling_tx, _signaling_rx) = mpsc::channel(16);
        let manager = Arc::new(RtcConnectionManager::new(
            Arc::new(AutoFactory),
            signaling_tx,
        ));
        let peer = test_id(42);

        // The peer starts connecting BEFORE the projection spawns.
        let handle = manager.ensure_controller(peer.clone()).await;
        handle.connect().await.expect("connect command");

        // Late-starting projection: the initial sweep must replay the current
        // Connecting state immediately.
        let (tx, mut rx) = mpsc::unbounded_channel();
        let observer: WorkloadHookObserverRef = Arc::new(HookRecorder { tx });
        let ctx = test_runtime_context();
        let ctx_builder: HookContextBuilder = Arc::new(move || {
            let ctx = ctx.clone();
            Box::pin(async move { Some(ctx) })
        });
        let shutdown = tokio_util::sync::CancellationToken::new();
        let projection = tokio::spawn(run_hook_projection(
            manager.clone(),
            Some(observer),
            ctx_builder,
            shutdown.clone(),
        ));

        // Initial replay of the Connecting state (state hook + legacy hook).
        expect(&mut rx, "state:Connecting:sendable=false").await;
        expect(&mut rx, "connect_start").await;

        // Terminal close: transition-driven Disconnected(Idle) + state hook.
        handle.close().await.expect("close command");
        expect(&mut rx, "state:Idle:sendable=false").await;
        expect(&mut rx, "disconnected:Some(Idle)").await;

        // Remove the terminal controller and immediately create another for
        // the same ActrId. The projection must recognize the new watch source,
        // reset revision/transition state, and deliver its transitions even if
        // registry watch edges were coalesced.
        assert!(manager.close_peer(&peer).await);
        let replacement = manager.ensure_controller(peer.clone()).await;
        replacement
            .connect()
            .await
            .expect("replacement connect command");
        expect_until(&mut rx, "state:Connecting:sendable=false").await;
        expect(&mut rx, "connect_start").await;

        shutdown.cancel();
        tokio::time::timeout(WAIT, projection)
            .await
            .expect("hook projection did not converge after shutdown")
            .expect("hook projection task panicked");
    }
}
