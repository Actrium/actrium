//! `V020WireHandle` — a [`WireHandle`] implementation backed by the v0.20
//! `RtcConnectionManager`.
//!
//! This is the production-wiring bridge: the transport layer's `PeerTransport`
//! manages connections as `WireHandle`s; this handle wraps a v0.20 peer
//! connection so the transport can route sends through the v0.20 data path
//! (Manager → Controller → LaneHandle → SendPermit → DataChannel).
//!
//! `connect()` requests the v0.20 Manager to ensure the peer is connected (the
//! controller drives offer/answer/ICE via the signaling router) and then waits
//! (bounded) for the peer snapshot to report `sendable` — the 0.14
//! `WebRtcCoordinator::create_connection` blocking contract, which the
//! transport layer's connection flight relies on. `get_lane()` returns a
//! [`V020DataLane`] that sends through the v0.20 send path. `close()` requests the
//! Manager to close the peer.

use std::sync::Arc;

use async_trait::async_trait;

use actr_protocol::{ActrId, PayloadType};

use crate::transport::{
    ConnType, DataLane, NetworkError, NetworkResult, SecureChannelState, SecureDataLane,
    WebRtcTransportLifecycle, WireHandle, WireIdentity,
};
use crate::wire::webrtc::v020::data_lane::V020DataLane;
use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::peer::handle::RtcPeerControllerHandle;
use crate::wire::webrtc::v020::status::LaneKind;

/// A [`WireHandle`] backed by the v0.20 `RtcConnectionManager`.
///
/// One handle per remote peer. `connect()` drives the v0.20 controller to
/// `Sendable`; `get_lane()` returns a lane that sends through the v0.20
/// `RtcPeerControllerHandle` (Snapshot preflight + SendPermit).
pub(crate) struct V020WireHandle {
    manager: Arc<RtcConnectionManager>,
    peer_id: ActrId,
    /// The controller handle, populated after `connect()`. Used for the sync
    /// `is_connected()` check via `RtcPeerControllerHandle::snapshot()`.
    controller: std::sync::Mutex<Option<RtcPeerControllerHandle>>,
}

impl std::fmt::Debug for V020WireHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V020WireHandle")
            .field("peer_id", &self.peer_id.to_string_repr())
            .finish()
    }
}

impl V020WireHandle {
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
impl WireHandle for V020WireHandle {
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
            return Ok(());
        }
        let Some((handle, mut attempt_epoch)) = self
            .manager
            .connect_transport_attempt(self.peer_id.clone())
            .await
            .map_err(|error| {
                NetworkError::ConnectionClosed(format!(
                    "v0.20 controller closed while starting peer {}: {error}",
                    self.peer_id.to_string_repr()
                ))
            })?
        else {
            return Err(NetworkError::ConnectionError(format!(
                "v0.20 controller admission changed while connecting peer {}",
                self.peer_id.to_string_repr()
            )));
        };
        // Keep the Controller identity while the connection is still being
        // established. Retry exhaustion/cancellation must be able to close a
        // Controller that allocated a PC but never reached Sendable.
        *self.controller.lock().unwrap() = Some(handle.clone());

        // The v0.20 manager's connect is enqueue-only: the controller drives
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
                        return Err(NetworkError::PeerConnectionClosed(format!(
                            "v0.20 peer {} connection attempt became terminal ({:?})",
                            self.peer_id.to_string_repr(),
                            snapshot.unavailable_reason
                        )));
                    }
                    tokio::select! {
                        changed = rx.changed() => {
                            if changed.is_err() {
                                return Err(NetworkError::ConnectionClosed(format!(
                                    "v0.20 controller closed while connecting peer {}",
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
                return Err(NetworkError::ConnectionError(format!(
                    "v0.20 peer {} did not become sendable within the connect window",
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
            NetworkError::InvalidOperation(format!(
                "no v0.20 lane for payload type {payload_type:?}"
            ))
        })?;

        let handle = self.controller.lock().unwrap().clone().ok_or_else(|| {
            NetworkError::ConnectionError(format!(
                "no connected v0.20 controller for peer {}",
                self.peer_id.to_string_repr()
            ))
        })?;

        let snapshot = handle.snapshot();
        if snapshot.status == actr_framework::WebRtcPeerStatus::Idle
            && snapshot.unavailable_reason.is_some()
        {
            return Err(NetworkError::PeerConnectionClosed(format!(
                "v0.20 peer {} transport attempt is terminal ({:?})",
                self.peer_id.to_string_repr(),
                snapshot.unavailable_reason
            )));
        }

        let secure = handle.secure_cell();
        let lane: Arc<dyn DataLane> = Arc::new(V020DataLane::new(handle, lane));
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

    use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::v020::peer::effects::{SessionFactory, SessionOps};

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
        // Compile-time check: V020WireHandle implements WireHandle.
        fn assert_wire_handle<T: WireHandle>() {}
        assert_wire_handle::<V020WireHandle>();
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
        let wire = Arc::new(V020WireHandle::new(manager.clone(), peer.clone()));
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
}
