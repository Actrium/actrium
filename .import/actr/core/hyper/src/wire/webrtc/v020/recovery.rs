//! `RtcRecoverySupervisor` — the global network/App lifecycle broadcast hub
//! (RFC §"RecoverySupervisor").
//!
//! The supervisor retains global network and App lifecycle policies, but only
//! **broadcasts facts or commands** — it MUST NOT directly modify any
//! `RtcPeerController`'s internal state or `PeerConnection`
//! (RFC: "Each Controller determines Soft Recovery, Hard Recovery or
//! termination by itself").
//!
//! Each `RtcPeerController` subscribes at spawn time and selects on the receiver
//! in its event loop. When a network event arrives, the controller feeds it to
//! its reducer as `RtcPeerCommand::NetworkEvent(event)`; the reducer (not the
//! supervisor) decides the recovery action.
//!
//! Channel semantics: `tokio::sync::broadcast` — multiple controllers subscribe,
//! the supervisor publishes. Late subscribers miss historical events (they only
//! react to future transitions). A slow consumer that falls behind gets a
//! `Lagged(n)` error; the controller conservatively expires sendability before
//! consuming the retained tail, so an evicted connectivity-loss edge cannot
//! leave a stale transport advertised as sendable.

use std::net::IpAddr;

use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

// S7A-3 (P2): the address-provider abstraction lives in `address.rs`; re-export
// it here so the recovery seams (monitor, tests) keep their existing paths.
pub(crate) use super::address::LocalAddressProvider;

/// Default broadcast capacity for network events. Network transitions are
/// infrequent (interface change, app background/foreground) so a small buffer
/// is sufficient; a lagging controller expires sendability and then reacts to
/// the retained tail.
pub(crate) const NETWORK_EVENT_CAPACITY: usize = 64;

/// Authoritative end-to-end budget for one logical peer recovery cycle. It is
/// inherited from the pre-cutover recovery contract and covers network-down
/// waiting, Soft ICE attempts, answerer request retries, and Hard rebuild as a
/// single non-extendable window.
pub(crate) const NETWORK_RECOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Network/App lifecycle facts broadcast by the recovery supervisor
/// (RFC §"RecoverySupervisor"). The supervisor only broadcasts these facts;
/// each controller independently decides Soft Recovery, Hard Recovery, or
/// termination.
///
/// This type lives here because its semantic home is the supervisor that
/// produces it, not the reducer that consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RtcNetworkEvent {
    /// The platform lost all network connectivity (no reachable interface).
    NetworkUnavailable,
    /// Network connectivity is restored (at least one usable interface).
    NetworkAvailable,
    /// The set of local interface addresses changed (interface up/down or
    /// Wi-Fi ↔ cellular switch). Triggers Hard Recovery because the current
    /// PeerConnection's ICE candidates are no longer valid.
    LocalAddressChanged,
    /// The app moved to the background (mobile lifecycle). The controller may
    /// pause non-essential work; it MUST NOT claim `sendable` while backgrounded
    /// if the platform suspends the process.
    AppBackground,
    /// The app returned to the foreground. The controller may resume recovery
    /// and re-establish connectivity if it was deferred.
    AppForeground { background_duration_ms: u64 },
    /// An explicit force-reconnect command (e.g. user-initiated "reconnect"
    /// or a server-directed reconnect). Triggers Hard Recovery.
    ForceReconnect,
    /// The lifecycle supervisor's authoritative signaling generation changed.
    /// `None` closes the peer recovery gate; `Some` permits pending ICE/SDP
    /// recovery to start against that committed generation.
    SignalingGenerationChanged { generation: Option<u64> },
}

/// The global broadcast hub for network/App lifecycle events.
///
/// Held by `RtcConnectionManager` as `Arc<RtcRecoverySupervisor>`; one clone
/// of the receiver is handed to each `RtcPeerController` at spawn time. The
/// supervisor is a *fact* source only — it never inspects or mutates
/// controller state.
pub(crate) struct RtcRecoverySupervisor {
    tx: broadcast::Sender<RtcNetworkEvent>,
    /// Latest signaling generation committed by the global lifecycle
    /// supervisor. Peer controllers subscribe to this watch and must not start
    /// ICE restart / SDP recovery effects while it is `None`.
    signaling_generation_tx: watch::Sender<Option<u64>>,
}

impl RtcRecoverySupervisor {
    pub(crate) fn new() -> Self {
        let (tx, _) = broadcast::channel(NETWORK_EVENT_CAPACITY);
        let (signaling_generation_tx, _) = watch::channel(None);
        Self {
            tx,
            signaling_generation_tx,
        }
    }

    /// Broadcast a network event to every subscribed controller. Receivers
    /// that have fallen behind observe a `Lagged` error on their next `recv`,
    /// conservatively expire sendability, and continue from the retained tail.
    ///
    /// Returns the number of subscribers that received the event (0 means no
    /// controllers are currently subscribed — e.g. before the first controller
    /// spawns, or after all controllers have been torn down).
    pub(crate) fn publish(&self, event: RtcNetworkEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    /// Subscribe to network events. Each controller calls this once at spawn
    /// and selects on the returned receiver in its event loop.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<RtcNetworkEvent> {
        self.tx.subscribe()
    }

    /// Subscribe to the signaling generation projection owned by the global
    /// lifecycle supervisor. A watch is used so controllers created after a
    /// successful signaling connection immediately observe the committed
    /// generation instead of waiting for another edge.
    pub(crate) fn subscribe_signaling_generation(&self) -> watch::Receiver<Option<u64>> {
        self.signaling_generation_tx.subscribe()
    }

    /// Project the lifecycle supervisor's authoritative signaling generation.
    /// Duplicate facts are coalesced and do not wake peer controllers.
    pub(crate) fn set_signaling_generation(&self, generation: Option<u64>) {
        self.signaling_generation_tx.send_if_modified(|current| {
            if *current == generation {
                false
            } else {
                *current = generation;
                true
            }
        });
    }

    /// S5-5: Publish an app-background transition. Platform lifecycle hooks
    /// (Android `onStop`, iOS `applicationDidEnterBackground`) call this.
    /// Each controller independently decides how to react — typically entering
    /// `Recovering` so `sendable` expires (RFC §"Hard Recovery is suitable for:
    /// Application lifecycle policy requires the connection to be re-established").
    pub(crate) fn publish_app_background(&self) -> usize {
        self.publish(RtcNetworkEvent::AppBackground)
    }

    /// S5-5: Publish an app-foreground transition. Platform lifecycle hooks
    /// (Android `onStart`, iOS `applicationWillEnterForeground`) call this.
    /// Controllers that deferred recovery while backgrounded resume now.
    pub(crate) fn publish_app_foreground(&self, background_duration_ms: u64) -> usize {
        self.publish(RtcNetworkEvent::AppForeground {
            background_duration_ms,
        })
    }
}

impl Default for RtcRecoverySupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// The per-peer identity tuple a peer-scoped recovery effect captures at
/// dispatch and validates terminal facts against (RFC-0400 "Per-peer coverage
/// for peer-scoped recovery"): peer identity, PC-session generation, and ICE
/// generation within that session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerRecoveryTarget {
    pub(crate) peer: actr_protocol::ActrId,
    pub(crate) pc_generation: u64,
    pub(crate) ice_generation: u64,
}

impl PeerRecoveryTarget {
    pub(crate) fn new(
        peer: actr_protocol::ActrId,
        pc_generation: u64,
        ice_generation: u64,
    ) -> Self {
        Self {
            peer,
            pc_generation,
            ice_generation,
        }
    }
}

/// The enumerated path through which a handed-off recovery obligation ended
/// (RFC-0400: an obligation "ends only through that owner's enumerated
/// extinguishment paths … each recorded with the path taken").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerRecoveryExtinguishment {
    /// The peer reached terminal success on the recorded tuple's session (or a
    /// same-or-newer ICE generation within it), validated by identity.
    SuccessAcknowledged,
    /// The tuple's PC session was superseded by a replacement build.
    Superseded,
    /// The peer left (controller removed or closing out from under the
    /// obligation).
    PeerDeparted,
    /// The owning manager's lifetime ended (`close_all`).
    OwnerLifetimeEnd,
}

/// One recorded resource-scoped recovery obligation handed off to this owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerRecoveryObligation {
    pub(crate) target: PeerRecoveryTarget,
    pub(crate) status: PeerRecoveryObligationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerRecoveryObligationStatus {
    Open,
    Extinguished(PeerRecoveryExtinguishment),
}

/// Bound on retained extinguished records. Records exist for audit; unbounded
/// retention across a long session would grow without limit.
pub(crate) const MAX_RETAINED_OBLIGATION_RECORDS: usize = 1024;

/// The manager-owned ledger of resource-scoped per-peer recovery obligations
/// (RFC-0400 "Per-peer coverage for peer-scoped recovery").
///
/// A peer-scoped Restore effect hands off each still-unfinished target by
/// recording one obligation here; the record call itself is the owner's
/// acknowledgement to the effect ("the handoff exists only when the receiving
/// owner has recorded the obligation and acknowledged the record to the
/// effect"). The obligation is durable in the RFC's obligation-model sense —
/// retained across event batches, offline intervals, and app lifecycle
/// transitions — and ends only through the enumerated extinguishment paths.
///
/// Completion, retry, and backoff for a handed-off target stay peer-scoped:
/// the controller's own single-flight recovery machinery (already bounded by
/// the audited ICE-restart backoff inventory) drives them; this ledger never
/// creates a supervisor retry gate.
pub(crate) struct PeerRecoveryObligationLog {
    records: std::sync::Mutex<Vec<PeerRecoveryObligation>>,
}

impl PeerRecoveryObligationLog {
    pub(crate) fn new() -> Self {
        Self {
            records: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Record one obligation and acknowledge it to the handing-off effect.
    pub(crate) fn record(&self, target: PeerRecoveryTarget) {
        let mut records = self.records.lock().expect("obligation log poisoned");
        records.push(PeerRecoveryObligation {
            target,
            status: PeerRecoveryObligationStatus::Open,
        });
        Self::retain_bounded(&mut records);
    }

    /// Apply one authoritative per-peer snapshot as a terminal-fact candidate.
    ///
    /// A success fact extinguishes an open obligation only when it is
    /// identity-validated: the snapshot's session generation must be the
    /// recorded one (or newer within the same peer), and within the same
    /// session its ICE generation must not be older than the recorded one —
    /// a fact bearing a superseded ICE generation is logged and discarded and
    /// covers nothing. A newer PC generation extinguishes via `Superseded`.
    pub(crate) fn observe_snapshot(
        &self,
        snapshot: &crate::wire::webrtc::v020::status::RtcPeerSnapshot,
    ) {
        let mut records = self.records.lock().expect("obligation log poisoned");
        for record in records.iter_mut() {
            if record.status != PeerRecoveryObligationStatus::Open
                || record.target.peer != snapshot.peer
            {
                continue;
            }
            let path = if snapshot.pc_generation > record.target.pc_generation {
                PeerRecoveryExtinguishment::Superseded
            } else if snapshot.pc_generation == record.target.pc_generation
                && snapshot.ice_generation < record.target.ice_generation
            {
                // Stale fact: an ICE generation older than the one recorded
                // covers nothing (the per-peer analogue of the effect-level
                // action_id/policy_revision guard).
                continue;
            } else if snapshot.sendable
                && snapshot.status == actr_framework::WebRtcPeerStatus::Connected
            {
                PeerRecoveryExtinguishment::SuccessAcknowledged
            } else {
                continue;
            };
            record.status = PeerRecoveryObligationStatus::Extinguished(path);
        }
    }

    /// Record the departure of one peer (controller removed by `close_peer`
    /// or removed during `close_all` draining).
    pub(crate) fn observe_departure(&self, peer: &actr_protocol::ActrId) {
        let mut records = self.records.lock().expect("obligation log poisoned");
        for record in records.iter_mut() {
            if record.status == PeerRecoveryObligationStatus::Open && &record.target.peer == peer {
                record.status = PeerRecoveryObligationStatus::Extinguished(
                    PeerRecoveryExtinguishment::PeerDeparted,
                );
            }
        }
    }

    /// Record the end of the owning manager's lifetime (`close_all`).
    pub(crate) fn observe_owner_end(&self) {
        let mut records = self.records.lock().expect("obligation log poisoned");
        for record in records.iter_mut() {
            if record.status == PeerRecoveryObligationStatus::Open {
                record.status = PeerRecoveryObligationStatus::Extinguished(
                    PeerRecoveryExtinguishment::OwnerLifetimeEnd,
                );
            }
        }
    }

    /// Whether an obligation is currently open for the exact tuple.
    pub(crate) fn is_open(&self, target: &PeerRecoveryTarget) -> bool {
        self.records
            .lock()
            .expect("obligation log poisoned")
            .iter()
            .any(|record| {
                record.status == PeerRecoveryObligationStatus::Open && &record.target == target
            })
    }

    /// A copy of the full audit ledger (bounded; oldest records evicted).
    pub(crate) fn records(&self) -> Vec<PeerRecoveryObligation> {
        self.records
            .lock()
            .expect("obligation log poisoned")
            .clone()
    }

    fn retain_bounded(records: &mut Vec<PeerRecoveryObligation>) {
        let excess = records
            .len()
            .saturating_sub(MAX_RETAINED_OBLIGATION_RECORDS);
        if excess > 0 {
            records.drain(..excess);
        }
    }
}

impl Default for PeerRecoveryObligationLog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RtcRecoverySupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcRecoverySupervisor")
            .field("subscribers", &self.tx.receiver_count())
            .field(
                "signaling_generation",
                &*self.signaling_generation_tx.borrow(),
            )
            .finish()
    }
}

/// Spawn a background task that bridges a `LocalAddressProvider` to the
/// `RtcRecoverySupervisor`: watches the address snapshot and broadcasts
/// `LocalAddressChanged` whenever the set of local IPs changes (RFC §"Local
/// addresses and sockets": "Trigger Hard Recovery when the interface address
/// changes").
///
/// The task lives for as long as the supervisor and provider are alive. It is
/// the *only* writer of `LocalAddressChanged` events — platform code should
/// not publish that variant directly.
pub(crate) fn spawn_address_monitor(
    provider: std::sync::Arc<dyn LocalAddressProvider>,
    supervisor: std::sync::Arc<RtcRecoverySupervisor>,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    // Subscribe before spawning so a change published immediately after this
    // function returns cannot be missed. The node cancellation token, not
    // watch-sender drop, owns this task's lifetime.
    let mut rx = provider.subscribe_changes();
    let mut prev = rx.borrow().clone();
    tokio::spawn(async move {
        let mut watch_open = true;
        #[cfg(test)]
        const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
        #[cfg(not(test))]
        const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
        let mut refresh = crate::timer::interval(
            crate::timer::ids::RTC_LOCAL_ADDRESS_REFRESH,
            REFRESH_INTERVAL,
        );
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // The watch channel delivers the initial snapshot immediately; treat it
        // as the baseline (no change to broadcast on first read). Polling is a
        // fallback for native providers whose OS surface cannot push an edge;
        // platform providers can still publish through the watch channel.
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = refresh.tick() => {
                    match provider.current_addresses().await {
                        Ok(addresses) => {
                            if !same_address_set(&prev.addresses, &addresses) {
                                supervisor.publish(RtcNetworkEvent::LocalAddressChanged);
                                prev = super::address::LocalAddressSnapshot {
                                    addresses,
                                    revision: prev.revision.wrapping_add(1),
                                };
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "failed to refresh local RTC addresses");
                        }
                    }
                }
                changed = rx.changed(), if watch_open => {
                    if changed.is_err() {
                        // Keep polling until node shutdown. A provider may not
                        // retain a push sender even though synchronous address
                        // enumeration remains available.
                        watch_open = false;
                        continue;
                    }
                    let current = rx.borrow().clone();
                    if !same_address_set(&prev.addresses, &current.addresses) {
                        supervisor.publish(RtcNetworkEvent::LocalAddressChanged);
                    }
                    prev = current;
                }
            }
        }
    })
}

/// Compare two address sets ignoring order and duplicates.
pub(crate) fn same_address_set(a: &[IpAddr], b: &[IpAddr]) -> bool {
    let mut a_sorted = a.to_vec();
    let mut b_sorted = b.to_vec();
    a_sorted.sort();
    b_sorted.sort();
    a_sorted.dedup();
    b_sorted.dedup();
    a_sorted == b_sorted
}

// --- S5-7: global build/restart semaphore + per-peer jitter ---

/// Default maximum number of concurrent session builds/restarts across all
/// peers. Prevents a thundering herd when network connectivity is restored
/// and many peers recover simultaneously (RFC §"Recovery must use global
/// semaphore and per-peer jitter").
pub(crate) const DEFAULT_MAX_CONCURRENT_BUILDS: usize = 4;

/// Default upper bound for the per-peer jitter delay (milliseconds). Each
/// peer waits a random duration in `[0, max_jitter)` before starting a build
/// or restart, staggering the recovery attempts.
pub(crate) const DEFAULT_MAX_JITTER_MS: u64 = 500;

/// Throttles concurrent session builds/restarts and adds per-peer jitter.
///
/// Held by `RtcConnectionManager` as `Arc<BuildThrottle>`; one clone is passed
/// to each `RtcPeerController` (and through it to the `WebRtcEffectRunner`).
/// Before a `BuildSession` or `RestartIce` effect is executed, the runner
/// acquires a semaphore permit and sleeps a jittered delay. This prevents a
/// network-recovery event from causing all peers to build/restart
/// simultaneously (RFC §"Recovery must use global semaphore and per-peer
/// jitter to avoid all peers building and [sending offers at once]").
pub(crate) struct BuildThrottle {
    semaphore: std::sync::Arc<tokio::sync::Semaphore>,
    max_jitter: std::time::Duration,
    max_concurrent: usize,
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    peak_active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    acquisitions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    contended_acquisitions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl BuildThrottle {
    pub(crate) fn new(max_concurrent: usize, max_jitter: std::time::Duration) -> Self {
        assert!(
            max_concurrent > 0,
            "build throttle requires at least one permit"
        );
        Self {
            semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            max_jitter,
            max_concurrent,
            active: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            peak_active: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            acquisitions: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            contended_acquisitions: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Acquire a build permit and apply per-peer jitter. The returned guard
    /// releases the permit on drop. The caller should hold the guard for the
    /// duration of the build/restart operation.
    pub(crate) async fn acquire(&self) -> BuildPermit {
        // Apply jitter *before* acquiring the semaphore so the wait does not
        // hold a permit slot.
        let jitter_ms = rand::random::<u64>() % (self.max_jitter.as_millis() as u64).max(1);
        if jitter_ms > 0 {
            crate::timer::sleep(
                crate::timer::ids::RTC_BUILD_JITTER,
                std::time::Duration::from_millis(jitter_ms),
            )
            .await;
        }
        if self.semaphore.available_permits() == 0 {
            self.contended_acquisitions
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("build semaphore closed");
        self.acquisitions
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let active = self
            .active
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.peak_active
            .fetch_max(active, std::sync::atomic::Ordering::AcqRel);
        BuildPermit {
            _permit: permit,
            active: self.active.clone(),
        }
    }

    #[cfg(feature = "test-utils")]
    pub(crate) fn observation(&self) -> BuildThrottleObservation {
        BuildThrottleObservation {
            max_concurrent: self.max_concurrent,
            active: self.active.load(std::sync::atomic::Ordering::Acquire),
            peak_active: self.peak_active.load(std::sync::atomic::Ordering::Acquire),
            acquisitions: self.acquisitions.load(std::sync::atomic::Ordering::Acquire),
            contended_acquisitions: self
                .contended_acquisitions
                .load(std::sync::atomic::Ordering::Acquire),
        }
    }
}

pub(crate) struct BuildPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for BuildPermit {
    fn drop(&mut self) {
        self.active
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildThrottleObservation {
    pub(crate) max_concurrent: usize,
    pub(crate) active: usize,
    pub(crate) peak_active: usize,
    pub(crate) acquisitions: usize,
    pub(crate) contended_acquisitions: usize,
}

impl std::fmt::Debug for BuildThrottle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildThrottle")
            .field("available", &self.semaphore.available_permits())
            .field("max_concurrent", &self.max_concurrent)
            .field("max_jitter_ms", &self.max_jitter.as_millis())
            .finish()
    }
}
