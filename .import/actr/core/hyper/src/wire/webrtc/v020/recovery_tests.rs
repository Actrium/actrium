//! Tests for `RtcRecoverySupervisor` and the `LocalAddressProvider` bridge
//! (S5-1 / S5-5).

use std::net::IpAddr;
use std::sync::Mutex as StdMutex;

use tokio::sync::{broadcast, watch};

use super::address::LocalAddressSnapshot;
use super::recovery::{
    LocalAddressProvider, NETWORK_EVENT_CAPACITY, RtcNetworkEvent, RtcRecoverySupervisor,
    same_address_set, spawn_address_monitor,
};

#[tokio::test]
async fn publish_delivers_to_all_subscribers() {
    let supervisor = RtcRecoverySupervisor::new();
    let mut rx1 = supervisor.subscribe();
    let mut rx2 = supervisor.subscribe();

    assert_eq!(supervisor.publish(RtcNetworkEvent::NetworkAvailable), 2);

    assert_eq!(rx1.recv().await.unwrap(), RtcNetworkEvent::NetworkAvailable);
    assert_eq!(rx2.recv().await.unwrap(), RtcNetworkEvent::NetworkAvailable);
}

#[tokio::test]
async fn publish_with_no_subscribers_returns_zero() {
    let supervisor = RtcRecoverySupervisor::new();
    assert_eq!(supervisor.publish(RtcNetworkEvent::ForceReconnect), 0);
}

#[tokio::test]
async fn late_subscriber_misses_prior_events() {
    let supervisor = RtcRecoverySupervisor::new();
    let _ = supervisor.publish(RtcNetworkEvent::NetworkUnavailable);

    let mut rx = supervisor.subscribe();
    let _ = supervisor.publish(RtcNetworkEvent::NetworkAvailable);

    // The late subscriber only sees the event published after it subscribed.
    assert_eq!(rx.recv().await.unwrap(), RtcNetworkEvent::NetworkAvailable);
}

#[tokio::test]
async fn lagging_receiver_skips_missed_events_and_continues() {
    let supervisor = RtcRecoverySupervisor::new();
    let mut rx = supervisor.subscribe();

    // Publish more events than the internal buffer can hold to force lag.
    // The broadcast channel buffers capacity + 1; send beyond that evicts
    // the oldest, so the receiver falls behind.
    for _ in 0..NETWORK_EVENT_CAPACITY + 10 {
        let _ = supervisor.publish(RtcNetworkEvent::ForceReconnect);
    }

    // The next recv should report a lag (missed events), then subsequent
    // recv gets the latest.
    let outcome = rx.recv().await;
    assert!(
        matches!(outcome, Err(broadcast::error::RecvError::Lagged(_))),
        "expected lag, got {outcome:?}"
    );

    // After the lag, the receiver continues from the current point.
    let latest = rx.recv().await;
    assert_eq!(latest.unwrap(), RtcNetworkEvent::ForceReconnect);
}

#[tokio::test]
async fn app_lifecycle_publish_methods_broadcast_correctly() {
    let supervisor = RtcRecoverySupervisor::new();
    let mut rx = supervisor.subscribe();

    assert_eq!(supervisor.publish_app_background(), 1);
    assert_eq!(rx.recv().await.unwrap(), RtcNetworkEvent::AppBackground);

    assert_eq!(supervisor.publish_app_foreground(5_000), 1);
    assert_eq!(
        rx.recv().await.unwrap(),
        RtcNetworkEvent::AppForeground {
            background_duration_ms: 5_000
        }
    );
}

#[test]
fn same_address_set_ignores_order_and_detects_changes() {
    let a: Vec<IpAddr> = vec!["192.168.1.1".parse().unwrap(), "10.0.0.1".parse().unwrap()];
    let b: Vec<IpAddr> = vec!["10.0.0.1".parse().unwrap(), "192.168.1.1".parse().unwrap()];
    assert!(same_address_set(&a, &b), "same set, different order");
    let duplicates: Vec<IpAddr> = vec![
        "10.0.0.1".parse().unwrap(),
        "192.168.1.1".parse().unwrap(),
        "10.0.0.1".parse().unwrap(),
    ];
    assert!(
        same_address_set(&a, &duplicates),
        "duplicates do not constitute an address-set change"
    );

    let c: Vec<IpAddr> = vec![
        "192.168.1.1".parse().unwrap(),
        "172.16.0.1".parse().unwrap(),
    ];
    assert!(!same_address_set(&a, &c), "different set");

    assert!(same_address_set(&[], &[]), "both empty");
    assert!(!same_address_set(&a, &[]), "one empty");
}

// --- LocalAddressProvider stub + address-monitor tests ---

struct StubAddressProvider {
    tx: watch::Sender<LocalAddressSnapshot>,
    current: StdMutex<Vec<IpAddr>>,
}

impl StubAddressProvider {
    fn new(initial: Vec<IpAddr>) -> std::sync::Arc<Self> {
        let (tx, _) = watch::channel(LocalAddressSnapshot::new(initial.clone()));
        std::sync::Arc::new(Self {
            tx,
            current: StdMutex::new(initial),
        })
    }

    fn publish(&self, addresses: Vec<IpAddr>) {
        *self.current.lock().unwrap() = addresses.clone();
        let _ = self.tx.send(LocalAddressSnapshot::new(addresses));
    }

    fn set_current_without_notification(&self, addresses: Vec<IpAddr>) {
        *self.current.lock().unwrap() = addresses;
    }
}

#[async_trait::async_trait]
impl LocalAddressProvider for StubAddressProvider {
    async fn current_addresses(
        &self,
    ) -> Result<Vec<IpAddr>, crate::wire::webrtc::v020::address::LocalAddressError> {
        Ok(self.current.lock().unwrap().clone())
    }

    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
        self.tx.subscribe()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn address_monitor_broadcasts_on_change() {
    let provider = StubAddressProvider::new(vec!["192.168.1.1".parse().unwrap()]);
    let supervisor = std::sync::Arc::new(RtcRecoverySupervisor::new());
    let mut rx = supervisor.subscribe();

    let shutdown = tokio_util::sync::CancellationToken::new();
    let monitor = spawn_address_monitor(provider.clone(), supervisor, shutdown.clone());

    // Publish a different address set.
    provider.publish(vec!["10.0.0.1".parse().unwrap()]);

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for address change")
        .expect("channel closed");
    assert_eq!(event, RtcNetworkEvent::LocalAddressChanged);
    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), monitor)
        .await
        .expect("address monitor did not converge after cancellation")
        .expect("address monitor panicked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn address_monitor_silent_when_addresses_unchanged() {
    let addr: IpAddr = "192.168.1.1".parse().unwrap();
    let provider = StubAddressProvider::new(vec![addr]);
    let supervisor = std::sync::Arc::new(RtcRecoverySupervisor::new());
    let mut rx = supervisor.subscribe();

    let shutdown = tokio_util::sync::CancellationToken::new();
    let monitor = spawn_address_monitor(provider.clone(), supervisor, shutdown.clone());

    // Publish the same address set (reordered to ensure order-independence).
    provider.publish(vec![addr]);

    // Wait briefly to confirm no event arrives.
    let outcome = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
    assert!(
        outcome.is_err(),
        "should not broadcast when the address set is unchanged"
    );
    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), monitor)
        .await
        .expect("address monitor did not converge after cancellation")
        .expect("address monitor panicked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn address_monitor_polling_fallback_detects_native_interface_change() {
    let provider = StubAddressProvider::new(vec!["192.168.1.1".parse().unwrap()]);
    let supervisor = std::sync::Arc::new(RtcRecoverySupervisor::new());
    let mut rx = supervisor.subscribe();
    let shutdown = tokio_util::sync::CancellationToken::new();
    let monitor = spawn_address_monitor(provider.clone(), supervisor, shutdown.clone());

    // Simulate a platform provider whose native API exposes the new address
    // synchronously but cannot publish a watch edge.
    provider.set_current_without_notification(vec!["10.0.0.1".parse().unwrap()]);
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("polling fallback missed address change")
        .expect("supervisor channel closed");
    assert_eq!(event, RtcNetworkEvent::LocalAddressChanged);

    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), monitor)
        .await
        .expect("address monitor did not converge after cancellation")
        .expect("address monitor panicked");
}

// ---------------------------------------------------------------------------
// RFC-0400 invariant 33: per-peer recovery coverage obligations
// ---------------------------------------------------------------------------

mod inv33_obligation_log {
    use super::super::recovery::{
        PeerRecoveryExtinguishment, PeerRecoveryObligationLog, PeerRecoveryObligationStatus,
        PeerRecoveryTarget,
    };
    use super::super::status::RtcPeerSnapshot;
    use actr_framework::WebRtcPeerStatus;
    use actr_protocol::{ActrId, ActrType, Realm};

    fn obligation_peer(serial: u64) -> ActrId {
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

    fn fact_snapshot(peer: &ActrId, pc: u64, ice: u64, sendable: bool) -> RtcPeerSnapshot {
        let mut snapshot = RtcPeerSnapshot::idle(peer.clone());
        snapshot.pc_generation = pc;
        snapshot.ice_generation = ice;
        snapshot.sendable = sendable;
        snapshot.status = if sendable {
            WebRtcPeerStatus::Connected
        } else {
            WebRtcPeerStatus::Recovering
        };
        snapshot
    }

    fn extinguished_path(
        log: &PeerRecoveryObligationLog,
        target: &PeerRecoveryTarget,
    ) -> Option<PeerRecoveryExtinguishment> {
        log.records().into_iter().find_map(|record| {
            if &record.target == target {
                match record.status {
                    PeerRecoveryObligationStatus::Open => None,
                    PeerRecoveryObligationStatus::Extinguished(path) => Some(path),
                }
            } else {
                None
            }
        })
    }

    /// A success fact extinguishes an open obligation only when it is
    /// identity-validated: the fact's session is the recorded one (or newer)
    /// and, within the same session, its ICE generation is not older than the
    /// recorded one. A stale fact covers nothing.
    #[test]
    fn success_fact_extinguishes_only_identity_validated_tuples() {
        let log = PeerRecoveryObligationLog::new();
        let peer = obligation_peer(7);
        let target = PeerRecoveryTarget::new(peer.clone(), 2, 3);

        log.record(target.clone());
        assert!(log.is_open(&target), "recorded obligation starts open");

        // A success fact bearing an older ICE generation within the same
        // session is a stale fact: discarded, covers nothing.
        log.observe_snapshot(&fact_snapshot(&peer, 2, 2, true));
        assert!(
            log.is_open(&target),
            "a superseded ICE generation must not extinguish the obligation"
        );

        // A not-yet-terminal snapshot is not a success fact.
        log.observe_snapshot(&fact_snapshot(&peer, 2, 4, false));
        assert!(log.is_open(&target));

        // The identity-validated success fact extinguishes via acknowledgement.
        log.observe_snapshot(&fact_snapshot(&peer, 2, 3, true));
        assert_eq!(
            extinguished_path(&log, &target),
            Some(PeerRecoveryExtinguishment::SuccessAcknowledged)
        );

        // A success fact on another peer's session covers nothing here.
        let other_target = PeerRecoveryTarget::new(obligation_peer(8), 2, 3);
        log.record(other_target.clone());
        log.observe_snapshot(&fact_snapshot(&peer, 2, 9, true));
        assert!(log.is_open(&other_target));
    }

    /// Supersession of the peer's session generation ends the obligation
    /// through the named extinguishment path, even before the replacement
    /// session is sendable.
    #[test]
    fn superseded_session_generation_extinguishes_obligation() {
        let log = PeerRecoveryObligationLog::new();
        let peer = obligation_peer(7);
        let target = PeerRecoveryTarget::new(peer.clone(), 2, 1);
        log.record(target.clone());

        log.observe_snapshot(&fact_snapshot(&peer, 3, 0, false));
        assert_eq!(
            extinguished_path(&log, &target),
            Some(PeerRecoveryExtinguishment::Superseded)
        );
    }

    /// Peer departure and owner-lifetime end are the remaining enumerated
    /// extinguishment paths, each recorded as taken.
    #[test]
    fn peer_departure_and_owner_lifetime_end_extinguish_open_obligations() {
        let log = PeerRecoveryObligationLog::new();
        let a = PeerRecoveryTarget::new(obligation_peer(7), 1, 1);
        let b = PeerRecoveryTarget::new(obligation_peer(8), 1, 1);
        log.record(a.clone());
        log.record(b.clone());

        log.observe_departure(&a.peer);
        assert_eq!(
            extinguished_path(&log, &a),
            Some(PeerRecoveryExtinguishment::PeerDeparted)
        );
        assert!(log.is_open(&b));

        log.observe_owner_end();
        assert_eq!(
            extinguished_path(&log, &b),
            Some(PeerRecoveryExtinguishment::OwnerLifetimeEnd)
        );
    }

    /// Retention is bounded: the oldest records are dropped beyond the cap
    /// while newer obligations keep full lifecycle tracking.
    #[test]
    fn retention_is_bounded_oldest_records_dropped_first() {
        let log = PeerRecoveryObligationLog::new();
        for serial in 0..super::super::recovery::MAX_RETAINED_OBLIGATION_RECORDS as u64 {
            log.record(PeerRecoveryTarget::new(obligation_peer(serial), 1, 1));
        }
        assert_eq!(
            log.records().len(),
            super::super::recovery::MAX_RETAINED_OBLIGATION_RECORDS
        );

        let newest = PeerRecoveryTarget::new(obligation_peer(u64::MAX), 1, 1);
        log.record(newest.clone());
        assert_eq!(
            log.records().len(),
            super::super::recovery::MAX_RETAINED_OBLIGATION_RECORDS
        );
        assert!(
            log.is_open(&newest),
            "newest record survives retention bound"
        );
        let oldest = PeerRecoveryTarget::new(obligation_peer(0), 1, 1);
        assert!(
            !log.is_open(&oldest),
            "dropped record is no longer reported open"
        );
    }
}
