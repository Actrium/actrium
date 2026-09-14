//! `RtcWireHandle` — a [`WireHandle`] implementation backed by the RTC
//! `RtcConnectionManager`.
//!
//! This is the production-wiring bridge: the transport layer's `PeerTransport`
//! manages connections as `WireHandle`s; this handle wraps a RTC peer
//! connection so the transport can route sends through the RTC data path
//! (Manager → Controller → LaneHandle → SendPermit → DataChannel).
//!
//! `connect()` requests the RTC Manager to ensure the peer is connected (the
//! controller drives offer/answer/ICE via the signaling router) and then waits
//! (bounded) for the peer snapshot to report `sendable` — the 0.14
//! `WebRtcCoordinator::create_connection` blocking contract, which the
//! transport layer's connection flight relies on. `get_lane()` returns a
//! [`RtcDataLane`] that sends through the RTC send path. `close()` requests the
//! Manager to close the peer.

use std::sync::Arc;

use async_trait::async_trait;

use actr_protocol::{ActrId, PayloadType};

use crate::transport::{
    ConnType, DataLane, NetworkError, NetworkResult, SecureChannelState, SecureDataLane,
    WebRtcTransportLifecycle, WireHandle, WireIdentity,
};
use crate::wire::webrtc::rtc::data_lane::RtcDataLane;
use crate::wire::webrtc::rtc::manager::RtcConnectionManager;
use crate::wire::webrtc::rtc::peer::handle::RtcPeerControllerHandle;
use crate::wire::webrtc::rtc::status::LaneKind;

/// A [`WireHandle`] backed by the RTC `RtcConnectionManager`.
///
/// One handle per remote peer. `connect()` drives the RTC controller to
/// `Sendable`; `get_lane()` returns a lane that sends through the RTC
/// `RtcPeerControllerHandle` (Snapshot preflight + SendPermit).
pub(crate) struct RtcWireHandle {
    manager: Arc<RtcConnectionManager>,
    peer_id: ActrId,
    /// The controller handle, populated after `connect()`. Used for the sync
    /// `is_connected()` check via `RtcPeerControllerHandle::snapshot()`.
    controller: std::sync::Mutex<Option<RtcPeerControllerHandle>>,
}

impl std::fmt::Debug for RtcWireHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcWireHandle")
            .field("peer_id", &self.peer_id.to_string_repr())
            .finish()
    }
}

impl RtcWireHandle {
    pub(crate) fn new(manager: Arc<RtcConnectionManager>, peer_id: ActrId) -> Self {
        Self {
            manager,
            peer_id,
            controller: std::sync::Mutex::new(None),
        }
    }

    /// Materialize a transport-layer handle around an already-sendable inbound
    /// or controller-owned Session. This creates only local routing state; it
    /// never starts a new PeerConnection negotiation.
    pub(crate) fn from_existing(
        manager: Arc<RtcConnectionManager>,
        peer_id: ActrId,
        controller: RtcPeerControllerHandle,
    ) -> Self {
        Self {
            manager,
            peer_id,
            controller: std::sync::Mutex::new(Some(controller)),
        }
    }
}

#[async_trait]
impl WireHandle for RtcWireHandle {
    fn connection_type(&self) -> ConnType {
        ConnType::WebRTC
    }

    fn priority(&self) -> u8 {
        // Match 0.14's WebRTC priority (1). The transport layer picks the
        // highest-priority connected handle; WebSocket is lower.
        1
    }

    async fn connect(&self) -> NetworkResult<()> {
        if self
            .controller
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|handle| handle.snapshot().sendable)
        {
            tracing::debug!(
                peer = %self.peer_id.to_string_repr(),
                "RTC wire connect skipped: controller already sendable"
            );
            return Ok(());
        }
        let Some((handle, mut attempt_epoch)) = self
            .manager
            .connect_transport_attempt(self.peer_id.clone())
            .await
            .map_err(|error| {
                NetworkError::ConnectionClosed(format!(
                    "RTC controller closed while starting peer {}: {error}",
                    self.peer_id.to_string_repr()
                ))
            })?
        else {
            return Err(NetworkError::ConnectionError(format!(
                "RTC controller admission changed while connecting peer {}",
                self.peer_id.to_string_repr()
            )));
        };
        // Keep the Controller identity while the connection is still being
        // established. Retry exhaustion/cancellation must be able to close a
        // Controller that allocated a PC but never reached Sendable.
        *self.controller.lock().unwrap() = Some(handle.clone());

        // The RTC manager's connect is enqueue-only: the controller drives
        // role negotiation + offer/answer + ICE asynchronously. The
        // WireHandle contract (0.14 `create_connection` parity) is that
        // `connect()` resolves once the peer is usable — the send path
        // preflights `snapshot.sendable` and fails fast otherwise — so wait
        // (bounded) for the snapshot to report sendable.
        let mut rx = handle.subscribe();
        let mut secure_epoch = handle.secure_epoch();
        let waited = crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(10),
            async {
                loop {
                    let snapshot = rx.borrow().clone();
                    if snapshot.transport_epoch < attempt_epoch {
                        // A watch receiver immediately replays its current
                        // value. Ignore any terminal snapshot from before the
                        // Controller's committed Connect boundary.
                    } else {
                        // If another caller legitimately advanced the same
                        // Controller while this transport was waiting, join the
                        // newer attempt rather than letting stale cleanup close
                        // it as a failed older flight.
                        attempt_epoch = snapshot.transport_epoch;
                    }
                    if snapshot.sendable
                        && snapshot.transport_epoch == attempt_epoch
                        && handle.secure_state().is_sendable()
                    {
                        return Ok(());
                    }
                    if snapshot.status == actr_framework::WebRtcPeerStatus::Idle
                        && snapshot.unavailable_reason.is_some()
                        && snapshot.transport_epoch == attempt_epoch
                    {
                        tracing::debug!(
                            peer = %self.peer_id.to_string_repr(),
                            transport_epoch = attempt_epoch,
                            reason = ?snapshot.unavailable_reason,
                            "RTC connection attempt became terminal while waiting for sendable"
                        );
                        return Err(NetworkError::PeerConnectionClosed(format!(
                            "RTC peer {} connection attempt became terminal ({:?})",
                            self.peer_id.to_string_repr(),
                            snapshot.unavailable_reason
                        )));
                    }
                    tokio::select! {
                        changed = rx.changed() => {
                            if changed.is_err() {
                                tracing::debug!(
                                    peer = %self.peer_id.to_string_repr(),
                                    transport_epoch = attempt_epoch,
                                    "RTC controller closed while waiting for sendable"
                                );
                                return Err(NetworkError::ConnectionClosed(format!(
                                    "RTC controller closed while connecting peer {}",
                                    self.peer_id.to_string_repr()
                                )));
                            }
                        }
                        _ = handle.await_secure_state_change(secure_epoch) => {
                            secure_epoch = handle.secure_epoch();
                        }
                    }
                }
            },
        )
        .await;
        match waited {
            Ok(result) => result?,
            Err(_) => {
                // The Controller normally publishes its own terminal deadline
                // 500 ms earlier. Keep the caller boundary authoritative too:
                // if scheduler delay wins that race, retire only this exact
                // epoch so WirePool's next retry can leave Idle and allocate a
                // fresh PeerConnection generation.
                let _ = handle.abort_transport_attempt(attempt_epoch).await;
                tracing::warn!(
                    peer = %self.peer_id.to_string_repr(),
                    transport_epoch = attempt_epoch,
                    "RTC peer did not become sendable within the connect window; aborted the transport attempt"
                );
                return Err(NetworkError::ConnectionError(format!(
                    "RTC peer {} did not become sendable within the connect window",
                    self.peer_id.to_string_repr()
                )));
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        // Non-blocking snapshot check: read the controller's latest snapshot
        // and check `sendable` (Connected == sendable, RFC Stage 4 exit
        // criterion). If the controller doesn't exist yet, returns false.
        let guard = self.controller.lock().unwrap();
        guard
            .as_ref()
            .is_some_and(|h| h.snapshot().sendable && h.secure_state().is_sendable())
    }

    async fn close(&self) -> NetworkResult<()> {
        let controller = self.controller.lock().unwrap().clone();
        self.manager.close_peer(&self.peer_id).await;
        if let Some(controller) = controller {
            controller.mark_secure_closed();
        }
        *self.controller.lock().unwrap() = None;
        Ok(())
    }

    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        let lane = LaneKind::from_payload_type(payload_type).ok_or_else(|| {
            NetworkError::InvalidOperation(format!("no RTC lane for payload type {payload_type:?}"))
        })?;

        let handle = self.controller.lock().unwrap().clone().ok_or_else(|| {
            NetworkError::ConnectionError(format!(
                "no connected RTC controller for peer {}",
                self.peer_id.to_string_repr()
            ))
        })?;

        let snapshot = handle.snapshot();
        if snapshot.status == actr_framework::WebRtcPeerStatus::Idle
            && snapshot.unavailable_reason.is_some()
        {
            return Err(NetworkError::PeerConnectionClosed(format!(
                "RTC peer {} transport attempt is terminal ({:?})",
                self.peer_id.to_string_repr(),
                snapshot.unavailable_reason
            )));
        }

        let secure = handle.secure_cell();
        let lane: Arc<dyn DataLane> = Arc::new(RtcDataLane::new(handle, lane));
        Ok(Arc::new(SecureDataLane::new(lane, secure)))
    }

    fn identity(&self) -> Option<WireIdentity> {
        let generation = self
            .controller
            .lock()
            .unwrap()
            .as_ref()
            .map(|handle| handle.snapshot().pc_generation)
            .unwrap_or(0);
        (generation > 0).then(|| WireIdentity::WebRtc {
            peer_id: self.peer_id.clone(),
            session_id: generation,
        })
    }

    fn webrtc_transport_lifecycle(&self) -> Option<WebRtcTransportLifecycle> {
        let guard = self.controller.lock().unwrap();
        let handle = guard.as_ref()?;
        let snapshot = handle.snapshot();
        (snapshot.transport_epoch > 0).then(|| WebRtcTransportLifecycle {
            peer_id: self.peer_id.clone(),
            controller_lifecycle_id: handle.controller_lifecycle_id(),
            transport_epoch: snapshot.transport_epoch,
        })
    }

    fn secure_channel_state(&self) -> SecureChannelState {
        self.controller
            .lock()
            .unwrap()
            .as_ref()
            .map_or(SecureChannelState::LegacyBypass, |handle| {
                handle.secure_state()
            })
    }

    fn secure_epoch(&self) -> u64 {
        self.controller
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, RtcPeerControllerHandle::secure_epoch)
    }

    async fn await_secure_state_change(&self, observed_epoch: u64) {
        let controller = self.controller.lock().unwrap().clone();
        if let Some(controller) = controller {
            controller.await_secure_state_change(observed_epoch).await;
        } else {
            std::future::pending::<()>().await;
        }
    }

    fn mark_secure_failed(&self) {
        if let Some(controller) = self.controller.lock().unwrap().as_ref() {
            controller.mark_secure_failed();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use actr_protocol::{ActrType, Realm};
    use async_trait::async_trait;
    use tokio::sync::mpsc;
    use webrtc::error::Error as RtcError;

    use crate::transport::wire_pool::{RetryConfig, WirePool, WireStatusKind};
    use crate::transport::{DefaultWireBuilder, DefaultWireBuilderConfig, Dest, WireBuilder};
    use crate::wire::webrtc::rtc::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::rtc::peer::effects::{SessionFactory, SessionOps};
    use crate::wire::webrtc::rtc::peer::reducer::SignalingMessage;

    use super::*;

    struct FailingSessionFactory;

    #[async_trait]
    impl SessionFactory for FailingSessionFactory {
        async fn build(
            &self,
            _handler: Arc<WebRtcEventHandler>,
        ) -> Result<Arc<dyn SessionOps>, RtcError> {
            Err(RtcError::Other("intentional build failure".to_string()))
        }
    }

    struct RecordingFailingSessionFactory {
        generations: Arc<std::sync::Mutex<Vec<u64>>>,
    }

    #[async_trait]
    impl SessionFactory for RecordingFailingSessionFactory {
        async fn build(
            &self,
            handler: Arc<WebRtcEventHandler>,
        ) -> Result<Arc<dyn SessionOps>, RtcError> {
            self.generations.lock().unwrap().push(handler.generation());
            Err(RtcError::Other("intentional build failure".to_string()))
        }
    }

    fn test_actor_id(serial_number: u64) -> ActrId {
        ActrId {
            realm: Realm { realm_id: 1 },
            serial_number,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "wire-handle-test".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    #[test]
    fn wire_handle_is_webrtc_type() {
        // Compile-time check: RtcWireHandle implements WireHandle.
        fn assert_wire_handle<T: WireHandle>() {}
        assert_wire_handle::<RtcWireHandle>();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wire_pool_retries_rtc_with_a_fresh_pc_generation() {
        let generations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (signaling_tx, mut signaling_rx) = mpsc::channel(16);
        let manager = Arc::new(RtcConnectionManager::new(
            Arc::new(RecordingFailingSessionFactory {
                generations: generations.clone(),
            }),
            signaling_tx,
        ));
        manager.supervisor().set_signaling_generation(Some(0));
        let peer = test_actor_id(3);
        let builder = DefaultWireBuilder::new(DefaultWireBuilderConfig {
            enable_websocket: false,
            enable_webrtc: true,
            ..Default::default()
        })
        .with_rtc_manager(manager.clone());

        // The builder must return an unconnected candidate immediately. If it
        // pre-connects, the first failure is consumed here and WirePool never
        // owns the configured retry loop.
        let mut connections = crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_millis(200),
            builder.create_connections(&Dest::peer(peer.clone())),
        )
        .await
        .expect("RTC builder pre-connected the candidate")
        .expect("build RTC candidate");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].connection_type(), ConnType::WebRTC);
        assert!(generations.lock().unwrap().is_empty());

        let role_manager = manager.clone();
        let role_peer = peer.clone();
        let role_responder = tokio::spawn(async move {
            while let Some(dispatch) = signaling_rx.recv().await {
                if matches!(dispatch.message, SignalingMessage::RoleNegotiation) {
                    role_manager
                        .handle_role_assignment(role_peer.clone(), true)
                        .await;
                }
            }
        });

        let pool = WirePool::new(RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            multiplier: 1.0,
        });
        pool.add_connection(connections.remove(0)).await;

        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(3),
            async {
                loop {
                    if generations.lock().unwrap().len() == 3
                        && manager.get_controller(&peer).await.is_none()
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            },
        )
        .await
        .expect("WirePool did not exhaust all RTC retries");

        assert_eq!(*generations.lock().unwrap(), vec![1, 2, 3]);
        // Retry exhaustion is the single terminal declaration point: the slot
        // must settle terminally Failed (not Connecting) and the Controller
        // must already be retired from the manager registry. The pool task
        // commits the Failed slot after its final close, so poll for it.
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(3),
            async {
                loop {
                    if pool.connection_status_for_test(ConnType::WebRTC).await
                        == Some(WireStatusKind::Failed)
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            },
        )
        .await
        .expect("retry exhaustion must leave the WebRTC slot terminally Failed");
        assert!(
            manager.get_controller(&peer).await.is_none(),
            "retry exhaustion must retire the RTC controller"
        );
        pool.close_all().await;
        role_responder.abort();
        let _ = role_responder.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fresh_connect_does_not_consume_the_previous_attempts_terminal_snapshot() {
        let (signaling_tx, _signaling_rx) = mpsc::channel(8);
        let manager = Arc::new(RtcConnectionManager::new(
            Arc::new(FailingSessionFactory),
            signaling_tx,
        ));
        let peer = test_actor_id(2);
        let controller = manager.ensure_controller(peer.clone()).await;

        controller.connect().await.expect("seed connect command");
        controller
            .apply_role_assignment(true)
            .await
            .expect("seed role assignment");
        let terminal = crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(2),
            async {
                let mut snapshots = controller.subscribe();
                loop {
                    let snapshot = snapshots.borrow().clone();
                    if snapshot.status == actr_framework::WebRtcPeerStatus::Idle
                        && snapshot.unavailable_reason.is_some()
                    {
                        break snapshot;
                    }
                    snapshots
                        .changed()
                        .await
                        .expect("controller closed before terminal snapshot");
                }
            },
        )
        .await
        .expect("seed attempt did not become terminal");

        // On a current-thread runtime the regressed enqueue-only implementation
        // could read the watch value before the Controller reduced Connect. The
        // first available value is deliberately the previous attempt's terminal
        // snapshot; a fresh flight must wait for the committed attempt boundary
        // instead of failing from that stale replay.
        let wire = Arc::new(RtcWireHandle::new(manager.clone(), peer.clone()));
        let connect = tokio::spawn({
            let wire = wire.clone();
            async move { wire.connect().await }
        });
        tokio::task::yield_now().await;
        assert!(
            !connect.is_finished(),
            "fresh connect consumed the previous attempt's terminal snapshot"
        );

        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(2),
            async {
                loop {
                    let snapshot = controller.snapshot();
                    if snapshot.transport_epoch > terminal.transport_epoch {
                        assert_eq!(
                            snapshot.transport_epoch,
                            terminal.transport_epoch + 1,
                            "one fresh Connect must start exactly one transport epoch"
                        );
                        assert_ne!(
                            snapshot.status,
                            actr_framework::WebRtcPeerStatus::Idle,
                            "the fresh attempt must leave the old terminal Idle state"
                        );
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            },
        )
        .await
        .expect("fresh Connect was not reduced");

        connect.abort();
        let _ = connect.await;
        assert!(manager.close_peer(&peer).await);
    }

    async fn snapshot_where(
        controller: &RtcPeerControllerHandle,
        predicate: impl Fn(&crate::wire::webrtc::rtc::status::RtcPeerSnapshot) -> bool,
    ) -> crate::wire::webrtc::rtc::status::RtcPeerSnapshot {
        let mut snapshots = controller.subscribe();
        loop {
            let snapshot = snapshots.borrow().clone();
            if predicate(&snapshot) {
                return snapshot;
            }
            snapshots
                .changed()
                .await
                .expect("controller closed while waiting for a snapshot");
        }
    }

    /// Wait for one exact transport epoch to become terminally unavailable.
    /// The generous window only bounds a failure; under the paused clock it
    /// cannot fire before the awaited watch edge.
    async fn terminal_snapshot_for_epoch(
        controller: &RtcPeerControllerHandle,
        transport_epoch: u64,
    ) -> crate::wire::webrtc::rtc::status::RtcPeerSnapshot {
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            Duration::from_secs(30),
            snapshot_where(controller, |snapshot| {
                snapshot.transport_epoch == transport_epoch
                    && snapshot.status == actr_framework::WebRtcPeerStatus::Idle
                    && snapshot.unavailable_reason.is_some()
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("epoch {transport_epoch} never became terminal"))
    }

    #[tokio::test(start_paused = true)]
    async fn caller_sendable_window_fallback_aborts_the_exact_transport_attempt() {
        let (signaling_tx, _signaling_rx) = mpsc::channel(8);
        // The controller runs on a frozen policy clock while the caller's
        // wait uses the (paused) tokio clock: the 10-second caller boundary is
        // the only deadline that can retire a wedged attempt, so the fallback
        // and its epoch fencing are exercised deterministically.
        let manager = Arc::new(
            RtcConnectionManager::new(Arc::new(FailingSessionFactory), signaling_tx)
                .with_policy_clock_for_test(Arc::new(
                    crate::wire::webrtc::rtc::peer::policy_clock::ManualPolicyClock::default(),
                )),
        );
        manager.supervisor().set_signaling_generation(Some(0));
        let peer = test_actor_id(4);
        let controller = manager.ensure_controller(peer.clone()).await;

        // Flight 1: no role assignment ever arrives, so the epoch-1 attempt
        // wedges in Building and only the caller window can retire it.
        let wire = Arc::new(RtcWireHandle::new(manager.clone(), peer.clone()));
        let error = wire
            .connect()
            .await
            .expect_err("frozen attempt must hit the caller deadline");
        assert!(
            matches!(
                &error,
                NetworkError::ConnectionError(message)
                    if message.contains("did not become sendable within the connect window")
            ),
            "unexpected fallback error: {error:?}"
        );

        // The fallback retired exactly the waited epoch terminally, and the
        // Controller stays alive so WirePool's next retry can leave Idle.
        let terminal = terminal_snapshot_for_epoch(&controller, 1).await;
        assert_eq!(
            terminal.unavailable_reason,
            Some(actr_framework::WebRtcUnavailableReason::NegotiationFailed)
        );
        assert!(
            manager.get_controller(&peer).await.is_some(),
            "the fallback must abort the attempt, not close the controller"
        );

        // Flight 2: the watch replays the epoch-1 terminal snapshot to a
        // fresh flight; that stale replay must be ignored (epoch fencing),
        // leaving the flight waiting on its own epoch-2 attempt.
        let wire2 = Arc::new(RtcWireHandle::new(manager.clone(), peer.clone()));
        let connect2 = tokio::spawn({
            let wire2 = wire2.clone();
            async move { wire2.connect().await }
        });
        snapshot_where(&controller, |snapshot| {
            snapshot.transport_epoch == 2
                && snapshot.status != actr_framework::WebRtcPeerStatus::Idle
        })
        .await;
        assert!(
            !connect2.is_finished(),
            "the stale epoch-1 terminal snapshot failed the fresh flight"
        );

        let error2 = connect2
            .await
            .expect("fresh connect task panicked")
            .expect_err("epoch-2 attempt must hit the caller deadline");
        assert!(
            matches!(
                &error2,
                NetworkError::ConnectionError(message)
                    if message.contains("did not become sendable within the connect window")
            ),
            "unexpected fallback error: {error2:?}"
        );
        let terminal2 = terminal_snapshot_for_epoch(&controller, 2).await;
        assert_eq!(
            terminal2.unavailable_reason,
            Some(actr_framework::WebRtcUnavailableReason::NegotiationFailed)
        );

        assert!(manager.close_peer(&peer).await);
    }
}
