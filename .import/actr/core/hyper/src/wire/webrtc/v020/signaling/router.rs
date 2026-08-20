//! `SignalingRouter` — routes signaling messages between per-peer controllers
//! and the signaling transport by remote `ActrId` (RFC §"SignalingRouter").
//!
//! The router is the *only* consumer of the signaling transport's inbound
//! stream (RFC: "the SignalingClient receive stream MUST have one consumer").
//! Inbound Offer/Answer/Candidate/RoleAssignment are dispatched to the target
//! peer's controller; an offer from an unknown peer requests the manager to create
//! an answerer controller. Outbound messages from controllers are forwarded onto
//! the transport.
//!
//! The router is transport-agnostic: [`SignalingTransport`] abstracts the wire
//! (the production WebSocket signaling client, or an in-memory bus in tests).
//! It does NOT do SDP, role, or ICE decisions — those belong to the per-peer
//! controller; the router only routes.

use std::sync::Arc;

use actr_protocol::ActrId;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

use super::SharedLocalId;

/// The signaling transport: how the router sends and receives wire messages.
///
/// Production binds the 0.14 `WebSocketSignalingClient` (preserving the wire
/// format); tests bind an in-memory bus. `recv` is consumed exactly once by the
/// router's run loop.
#[async_trait]
pub(crate) trait SignalingTransport: Send + Sync {
    /// Send a signaling message to a remote peer (via the signaling server).
    async fn send(&self, to: ActrId, message: SignalingMessage);

    /// Receive the next inbound message `(from, message)`. Returns `None` when
    /// the transport is closed.
    async fn recv(&self) -> Option<(ActrId, SignalingMessage)>;
}

/// Routes signaling between controllers and the transport by remote `ActrId`.
pub(crate) struct SignalingRouter {
    manager: Arc<RtcConnectionManager>,
    /// Shared local identity (rebind-updatable, CF-1).
    local_id: SharedLocalId,
}

impl SignalingRouter {
    pub(crate) fn new(manager: Arc<RtcConnectionManager>, local_id: SharedLocalId) -> Self {
        Self { manager, local_id }
    }

    pub(crate) fn local_id(&self) -> ActrId {
        self.local_id.get()
    }

    /// Run the routing loop: forward controller-originated messages onto the
    /// transport, and dispatch transport-originated messages to controllers.
    ///
    /// `outbound_rx` is the shared channel every controller's `SendSignaling`
    /// effect feeds (created by the manager).
    pub(crate) async fn run<T: SignalingTransport>(
        &self,
        transport: T,
        mut outbound_rx: mpsc::Receiver<(ActrId, SignalingMessage)>,
    ) {
        loop {
            tokio::select! {
                outbound = outbound_rx.recv() => {
                    match outbound {
                        Some((to, message)) => transport.send(to, message).await,
                        // All controllers gone: stop routing.
                        None => break,
                    }
                }
                inbound = transport.recv() => {
                    match inbound {
                        Some((from, message)) => self.route_inbound(from, message).await,
                        // Transport closed: stop routing.
                        None => break,
                    }
                }
            }
        }
    }

    /// Dispatch one inbound message to the right controller by remote `ActrId`.
    pub(crate) async fn route_inbound(&self, from: ActrId, message: SignalingMessage) {
        match message {
            SignalingMessage::Offer { desc, exchange_id } => {
                // An offer from an unknown peer creates an answerer controller
                // (the manager is idempotent for a known peer — RFC §"SignalingRouter").
                self.manager
                    .handle_incoming_offer_correlated(from, *desc, exchange_id)
                    .await;
            }
            SignalingMessage::IceRestartOffer { desc, exchange_id } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle
                        .apply_remote_ice_restart_offer_correlated(*desc, Some(exchange_id.clone()))
                        .await;
                } else {
                    // Preserve the existing wire contract: a restart offer is
                    // valid only for the old active Session. Creating a fresh
                    // answerer implicitly would leave the offerer on its stale
                    // PC/DTLS/SCTP generation.
                    let sent = self.manager.try_send_signaling(
                        from.clone(),
                        SignalingMessage::IceRestartRejected {
                            exchange_id,
                            reason: actr_protocol::ice_restart_rejected::Reason::NoActiveSession
                                as i32,
                        },
                    );
                    if !sent {
                        tracing::warn!(
                            peer = %from.to_string_repr(),
                            "outbound signaling full; dropped ICE restart rejection"
                        );
                    }
                }
            }
            SignalingMessage::IceRestartRequest {
                request_id,
                attempt,
            } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.apply_ice_restart_request(request_id, attempt).await;
                } else {
                    tracing::warn!(
                        peer = %from.to_string_repr(),
                        "dropping ICE restart request for unknown peer"
                    );
                }
            }
            SignalingMessage::Answer { desc, exchange_id } => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle
                        .apply_remote_description_correlated(*desc, exchange_id)
                        .await;
                } else {
                    tracing::warn!(
                        peer = %from.to_string_repr(),
                        "dropping Answer for unknown peer (no controller)"
                    );
                }
            }
            SignalingMessage::Candidate(candidate) => {
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.apply_remote_candidate(candidate).await;
                } else {
                    tracing::debug!(
                        peer = %from.to_string_repr(),
                        "dropping ICE candidate for unknown peer (no controller)"
                    );
                }
            }
            SignalingMessage::RoleAssignment { is_offerer } => {
                // Unsolicited assignment for a peer we have no controller for
                // still creates the passive-side controller, but creation is
                // committed under the router dispatch's manager lifecycle
                // epoch so a racing close-all can reject late publication.
                self.manager.handle_role_assignment(from, is_offerer).await;
            }
            SignalingMessage::RoleNegotiation => {
                // RoleNegotiation only travels outbound (to the server); the
                // router never receives it inbound.
                tracing::warn!(
                    peer = %from.to_string_repr(),
                    "ignoring unexpected inbound RoleNegotiation"
                );
            }
            SignalingMessage::IceRestartRejected {
                exchange_id,
                reason,
            } => {
                tracing::info!(
                    peer = %from.to_string_repr(),
                    %exchange_id,
                    reason,
                    "remote rejected ICE restart; escalating to Hard Recovery"
                );
                if let Some(handle) = self.manager.get_controller(&from).await {
                    let _ = handle.force_reconnect().await;
                }
            }
        }
    }
}
