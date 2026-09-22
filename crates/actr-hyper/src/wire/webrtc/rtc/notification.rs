//! Snapshot notification and business-hook projection (RFC §"NotificationHub").
//!
//! Controllers publish snapshots only; they never call workload/FFI hooks
//! directly. The hub is the single fan-out point: it de-duplicates by
//! `(peer, revision)`, dispatches to registered observers, and isolates the
//! controller and the webrtc driver from hook behavior — dispatch happens on the
//! node-owned projection task (not the controller), and a panicking observer is
//! caught rather than propagated.
//!
//! `run_hook_projection` is the production node-owned projection into the
//! unified state hook and the three compatibility hooks. Public/mobile state
//! subscriptions read the Controller-maintained public watch channel directly.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use actr_framework::WebRtcPeerStatus;
use actr_protocol::ActrId;
use futures_util::FutureExt as _;
use futures_util::future::BoxFuture;
use tokio::sync::{RwLock, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::wire::webrtc::rtc::status::RtcPeerSnapshot;

/// Per-peer projection memory: the previously observed status, the previous
/// sendability, and the Connected-epoch latch (see
/// [`apply_connected_epoch_latch`]).
type HookProjectionState = Arc<Mutex<HashMap<ActrId, (Option<WebRtcPeerStatus>, bool, bool)>>>;

/// A snapshot observer. Called on the node-owned projection task (never on the
/// controller), and panics are caught and logged rather than propagated.
pub(crate) type SnapshotObserver =
    Arc<dyn Fn(RtcPeerSnapshot) -> BoxFuture<'static, ()> + Send + Sync>;

/// Fan-out hub for per-peer snapshots.
pub(crate) struct NotificationHub {
    observers: RwLock<Vec<SnapshotObserver>>,
    /// Highest revision dispatched per peer; older/equal revisions are dropped
    /// as duplicates (RFC: "distribute in order by peer and revision;
    /// deduplicate repeated Snapshots").
    last_revision: Mutex<HashMap<ActrId, u64>>,
}

impl NotificationHub {
    pub(crate) fn new() -> Self {
        Self {
            observers: RwLock::new(Vec::new()),
            last_revision: Mutex::new(HashMap::new()),
        }
    }

    /// Register an observer that receives every non-duplicate snapshot.
    pub(crate) async fn register_observer(&self, observer: SnapshotObserver) {
        self.observers.write().await.push(observer);
    }

    /// Forget the dispatch history after a peer disappears. The Manager now
    /// preserves a monotone revision floor across Controller replacement; the
    /// reset remains useful for a genuinely removed peer and bounds storage.
    pub(crate) fn remove_peer(&self, peer: &ActrId) {
        self.last_revision
            .lock()
            .expect("revision lock poisoned")
            .remove(peer);
    }

    /// Number of registered observers (test/diagnostic).
    #[cfg(test)]
    pub(crate) async fn observer_count(&self) -> usize {
        self.observers.read().await.len()
    }

    /// Publish a snapshot to all observers, de-duplicated by `(peer, revision)`.
    ///
    /// Returns `true` if the snapshot was newer and dispatched, `false` if it
    /// was a duplicate/stale revision. Each observer call is wrapped in
    /// `catch_unwind` so a panicking hook cannot take down the hub or stall the
    /// forwarding task. The revision lock is a std mutex released before the
    /// (async) observer-list read, so no std mutex guard crosses an `.await`.
    pub(crate) async fn publish(&self, snapshot: RtcPeerSnapshot) -> bool {
        {
            let mut last = self.last_revision.lock().expect("revision lock poisoned");
            if last
                .get(&snapshot.peer)
                .is_some_and(|revision| snapshot.revision <= *revision)
            {
                return false; // duplicate or out-of-order older revision
            }
            last.insert(snapshot.peer.clone(), snapshot.revision);
        }

        let observers = self.observers.read().await.clone();
        for observer in observers {
            let snapshot = snapshot.clone();
            match catch_unwind(AssertUnwindSafe(|| observer(snapshot))) {
                Ok(future) => {
                    if AssertUnwindSafe(future).catch_unwind().await.is_err() {
                        tracing::warn!("notification observer panicked; ignoring and continuing");
                    }
                }
                Err(_panic) => {
                    tracing::warn!("notification observer panicked; ignoring and continuing");
                }
            }
        }
        true
    }

    /// Spawn a forwarder task that watches one controller's snapshot channel and
    /// publishes each change to the hub. This is the decoupling point: a slow or
    /// panicking observer only ever blocks this forwarder, never the controller
    /// or the webrtc driver (RFC: "prevent slow Hooks from blocking the
    /// Controller and the webrtc driver").
    pub(crate) fn watch_peer(
        self: &Arc<Self>,
        mut rx: watch::Receiver<RtcPeerSnapshot>,
    ) -> JoinHandle<()> {
        let hub = Arc::clone(self);
        tokio::spawn(async move {
            // Publish the current snapshot immediately (initial replay), then
            // every subsequent change. The cloned snapshot is bound to a local
            // first so the non-Send `watch::Ref` guard drops before the await.
            let initial = rx.borrow().clone();
            hub.publish(initial).await;
            loop {
                if rx.changed().await.is_err() {
                    break; // controller dropped its snapshot sender
                }
                let snapshot = rx.borrow_and_update().clone();
                hub.publish(snapshot).await;
            }
        })
    }
}

/// S6-6: the legacy three-hook projection for one observed snapshot, as a
/// pure function (0.14 parity):
///
/// * `WebRtcConnectStart` fires on every *transition into* `Connecting` —
///   including first sight and re-entry after a Hard Recovery rebuild (0.14
///   fires it on the public-state change, not only for new peers).
/// * `WebRtcConnected` fires on the business-sendable edge (0.14
///   `notify_webrtc_connected_if_sendable` parity), with `relayed` mapped
///   from the stats-resolved network path (S6-3).
/// * `WebRtcDisconnected` fires on entry into `Idle` or `Recovering`,
///   including the Recovering→Idle sequence (0.14 fires both, in commit
///   order). A first sight of `Recovering` fires it too (late installers
///   learn the current truth); a first sight of `Idle` is a freshly created
///   controller and fires nothing.
///
/// `prev_status` is `None` when the projection first sees the peer.
pub(crate) fn project_legacy_hooks(
    prev_status: Option<WebRtcPeerStatus>,
    was_sendable: bool,
    snap: &RtcPeerSnapshot,
) -> Vec<crate::wire::webrtc::HookEvent> {
    use crate::wire::webrtc::HookEvent;
    use actr_framework::WebRtcPeerStatus as S;

    let mut out = Vec::new();

    // ConnectStart: every transition into Connecting (first sight counts as a
    // transition from nothing).
    if matches!(snap.status, S::Connecting) && prev_status != Some(S::Connecting) {
        out.push(HookEvent::WebRtcConnectStart {
            peer_id: snap.peer.clone(),
        });
    }

    // Connected: the business-sendable edge.
    if !was_sendable && snap.sendable {
        out.push(HookEvent::WebRtcConnected {
            peer_id: snap.peer.clone(),
            relayed: matches!(snap.network_path, super::status::NetworkPath::Relayed),
        });
    }

    // Disconnected: entry into Idle or Recovering, except a first sight of
    // Idle (a freshly created controller was never connected).
    let now_down = matches!(snap.status, S::Idle | S::Recovering);
    let first_sight_idle = prev_status.is_none() && matches!(snap.status, S::Idle);
    if now_down && prev_status != Some(snap.status) && !first_sight_idle {
        out.push(HookEvent::WebRtcDisconnected {
            peer_id: snap.peer.clone(),
            status: snap.status,
        });
    }

    out
}

/// S6-6c: legacy `WebRtcConnected`/`WebRtcDisconnected` hooks must alternate.
///
/// The sendable edge is not always epoch-clean: a renegotiation or a brief
/// transport wobble can drop and regain sendability while the public status
/// stays away from `Idle`/`Recovering`, so no disconnect is projected in
/// between. Re-firing `WebRtcConnected` for such an intra-epoch flap makes
/// mobile clients observe a duplicate `connected` edge (P14 regression).
/// The latch suppresses a repeated `WebRtcConnected` until a
/// `WebRtcDisconnected` has been projected for the peer.
pub(crate) fn apply_connected_epoch_latch(
    events: Vec<crate::wire::webrtc::HookEvent>,
    connected_latched: bool,
) -> (Vec<crate::wire::webrtc::HookEvent>, bool) {
    use crate::wire::webrtc::HookEvent;

    let mut latched = connected_latched;
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        match &event {
            HookEvent::WebRtcConnected { .. } => {
                if latched {
                    continue;
                }
                latched = true;
            }
            HookEvent::WebRtcDisconnected { .. } => latched = false,
            _ => {}
        }
        out.push(event);
    }
    (out, latched)
}

/// W5/B3: snapshot→hook projection for the RTC backend.
///
/// S6-6b: fully transition-driven. The projection consumes the Manager's
/// ordered snapshot-commit stream, so rapid Controller revisions do not get
/// coalesced by a latest-value `watch` channel before Hooks see a sendability
/// edge. The peer-registry watch is retained for late-subscriber replay and
/// Controller create/remove reconciliation; there is no polling interval.
///
/// Without this projection the RTC cutover would silently drop
/// business-visible hooks. The mapping lives in [`project_legacy_hooks`]
/// (0.14 parity rules) plus the S6-5 unified `WebRtcStateChanged` hook.
pub(crate) async fn run_hook_projection(
    manager: Arc<super::manager::RtcConnectionManager>,
    hook_observer: Option<crate::lifecycle::hooks::WorkloadHookObserverRef>,
    ctx_builder: crate::lifecycle::hooks::HookContextBuilder,
    shutdown: CancellationToken,
) {
    use crate::lifecycle::hooks::build_hook_callback;
    let cb = build_hook_callback(hook_observer, ctx_builder);
    run_hook_projection_with_callback(manager, cb, shutdown).await;
}

/// Test-support entry that runs the same production projection and forwards
/// its ordered HookEvents to an evidence recorder instead of a workload.
#[cfg(feature = "test-utils")]
pub(crate) async fn run_recording_hook_projection(
    manager: Arc<super::manager::RtcConnectionManager>,
    callback: crate::wire::webrtc::HookCallback,
    shutdown: CancellationToken,
) {
    run_hook_projection_with_callback(manager, callback, shutdown).await;
}

async fn run_hook_projection_with_callback(
    manager: Arc<super::manager::RtcConnectionManager>,
    cb: crate::wire::webrtc::HookCallback,
    shutdown: CancellationToken,
) {
    use crate::wire::webrtc::HookEvent;

    let hub = Arc::new(NotificationHub::new());
    let projection_state: HookProjectionState = Arc::new(Mutex::new(HashMap::new()));
    let observer_state = projection_state.clone();
    hub.register_observer(Arc::new(move |snap| {
        let events = {
            let mut states = observer_state
                .lock()
                .expect("RTC hook projection state lock");
            let state = states
                .entry(snap.peer.clone())
                .or_insert((None, false, false));
            let mut events = vec![HookEvent::WebRtcStateChanged {
                state: snap.to_public(),
            }];
            let (legacy, latched) =
                apply_connected_epoch_latch(project_legacy_hooks(state.0, state.1, &snap), state.2);
            events.extend(legacy);
            *state = (Some(snap.status), snap.sendable, latched);
            events
        };
        let cb = cb.clone();
        Box::pin(async move {
            for event in events {
                cb(event).await;
            }
        })
    }))
    .await;
    let mut peers_rx = manager.subscribe_peers();
    let mut snapshots = manager.subscribe_peer_snapshot_events();
    let mut sources = HashMap::<ActrId, watch::Receiver<RtcPeerSnapshot>>::new();
    let mut reconcile = true;

    loop {
        if reconcile {
            // Subscribe to the ordered stream before the initial sweep. If a
            // Controller commits concurrently, either its current replay or
            // the queued event wins; revision de-dup makes both orderings safe.
            let peer_ids = manager.peer_ids().await;
            let current: HashSet<_> = peer_ids.iter().cloned().collect();
            let retired = sources
                .keys()
                .filter(|peer| !current.contains(*peer))
                .cloned()
                .collect::<Vec<_>>();
            for peer in retired {
                sources.remove(&peer);
                hub.remove_peer(&peer);
                projection_state
                    .lock()
                    .expect("RTC hook projection state lock")
                    .remove(&peer);
            }

            // Reconcile by channel identity, not only ActrId. A replacement
            // Controller inherits a monotone revision floor, so projection
            // state remains continuous even when remove+create edges coalesce.
            for peer in peer_ids {
                if let Some(rx) = manager.subscribe(&peer).await {
                    let changed_source = sources
                        .get(&peer)
                        .is_none_or(|source| !source.same_channel(&rx));
                    if changed_source {
                        let current = rx.borrow().clone();
                        sources.insert(peer, rx);
                        hub.publish(current).await;
                    }
                }
            }
            reconcile = false;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,
            changed = peers_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                reconcile = true;
            }
            event = snapshots.recv() => {
                match event {
                    Ok(snapshot) => {
                        hub.publish(snapshot).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "RTC hook snapshot stream lagged; replaying current peer states"
                        );
                        reconcile = true;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

impl Default for NotificationHub {
    fn default() -> Self {
        Self::new()
    }
}
