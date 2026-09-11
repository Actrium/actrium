//! `RtcPeerControllerHandle` — the read/send face of a peer controller for the
//! manager, peer gate, and business layer (RFC §"RtcPeerControllerHandle").
//!
//! Reading state (snapshot/subscribe) never waits for the controller to process
//! a message round — it reads the latest published snapshot directly. Commands
//! go through the bounded command mailbox. The handle does NOT carry the data
//! plane (business payloads bypass the controller entirely; Stage 4 adds
//! `LaneHandle`/`SendPermit`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use actr_protocol::ActrId;
use tokio::sync::{mpsc, oneshot, watch};
use webrtc::peer_connection::RTCSessionDescription;

use crate::wire::webrtc::v020::media::OutboundTrackWriter;
use crate::wire::webrtc::v020::peer::lane_handle::{ActiveLaneRegistry, LaneHandle};
use crate::wire::webrtc::v020::peer::reducer::{RtcPeerCommand, SignalingMessage};
use crate::wire::webrtc::v020::peer::send::{RtcSendAttempt, RtcSendError};
use crate::wire::webrtc::v020::status::{LaneKind, RtcPeerSnapshot};

/// Error returned when the controller task has gone away (e.g. after close).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ControllerGone {
    #[error("peer controller is no longer running")]
    Gone,
}

struct SecureGenerationCell {
    pc_generation: u64,
    cell: crate::transport::SecureCell,
}

/// A cheap, cloneable handle to one peer's controller.
#[derive(Clone)]
pub(crate) struct RtcPeerControllerHandle {
    peer_id: ActrId,
    controller_lifecycle_id: u64,
    command_tx: mpsc::Sender<RtcPeerCommand>,
    snapshot_rx: watch::Receiver<RtcPeerSnapshot>,
    public_snapshot_rx: watch::Receiver<actr_framework::WebRtcPeerState>,
    /// The same mailbox owned by the generation-scoped v0.20 event handler.
    /// Retained on the handle only for the `test-utils` fault-injection seam;
    /// production callers have no API that can publish driver facts.
    #[cfg(feature = "test-utils")]
    event_mailbox: crate::wire::webrtc::v020::events::PeerEventMailbox,
    lanes: ActiveLaneRegistry,
    /// Exceptional teardown capability retained by the Manager. Normal
    /// lifecycle always closes through the Controller command/barrier; this
    /// runner reference exists solely so a close-all deadline that aborts the
    /// Controller task can still explicitly close physical Sessions.
    effect_port: Arc<dyn super::effect_port::RtcPeerEffectPort>,
    /// Monotonically increasing message-id counter for fragment correlation.
    /// Shared across clones so every send gets a unique id. The pump's
    /// `ReassemblyBuffer` is per-lane, so cross-lane id reuse is safe.
    msg_id_counter: Arc<AtomicU32>,
    /// S6-10: outbound media writers by track id (the media data plane —
    /// writers bypass the controller after registration, like lane sends).
    media_writers:
        Arc<std::sync::Mutex<std::collections::HashMap<String, Arc<OutboundTrackWriter>>>>,
    /// Session-generation secure state shared by every transport handle
    /// materialized for this controller. When Hard Recovery publishes a new
    /// PC generation, the prior cell is closed and a fresh explicit
    /// LegacyBypass cell is installed for the replacement Session.
    secure: Arc<std::sync::Mutex<SecureGenerationCell>>,
    /// Outbound message-size cap shared with the receiver's reassembly
    /// limits (both derive from `RtcBufferConfig::max_inbound_message_bytes`).
    /// A payload above it would be fragmented, sent, and then rejected by the
    /// remote's reassembly — reporting success while silently dropping the
    /// message — so the send path rejects it up front with `MessageTooLarge`.
    max_message_bytes: usize,
}

impl RtcPeerControllerHandle {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        peer_id: ActrId,
        controller_lifecycle_id: u64,
        command_tx: mpsc::Sender<RtcPeerCommand>,
        snapshot_rx: watch::Receiver<RtcPeerSnapshot>,
        public_snapshot_rx: watch::Receiver<actr_framework::WebRtcPeerState>,
        #[cfg(feature = "test-utils")]
        event_mailbox: crate::wire::webrtc::v020::events::PeerEventMailbox,
        lanes: ActiveLaneRegistry,
        effect_port: Arc<dyn super::effect_port::RtcPeerEffectPort>,
        max_message_bytes: usize,
    ) -> Self {
        let initial_pc_generation = snapshot_rx.borrow().pc_generation;
        Self {
            peer_id,
            controller_lifecycle_id,
            command_tx,
            snapshot_rx,
            public_snapshot_rx,
            #[cfg(feature = "test-utils")]
            event_mailbox,
            lanes,
            effect_port,
            msg_id_counter: Arc::new(AtomicU32::new(0)),
            media_writers: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            secure: Arc::new(std::sync::Mutex::new(SecureGenerationCell {
                pc_generation: initial_pc_generation,
                cell: crate::transport::SecureCell::new(
                    crate::transport::SecureChannelState::LegacyBypass,
                ),
            })),
            max_message_bytes,
        }
    }

    pub(crate) fn secure_cell(&self) -> crate::transport::SecureCell {
        let pc_generation = self.snapshot().pc_generation;
        let mut secure = self
            .secure
            .lock()
            .expect("RTC secure-generation lock poisoned");
        if secure.pc_generation != pc_generation {
            secure
                .cell
                .drive_terminal(crate::transport::SecureChannelState::Closed);
            *secure = SecureGenerationCell {
                pc_generation,
                cell: crate::transport::SecureCell::new(
                    crate::transport::SecureChannelState::LegacyBypass,
                ),
            };
        }
        secure.cell.clone()
    }

    pub(crate) fn secure_state(&self) -> crate::transport::SecureChannelState {
        self.secure_cell().state()
    }

    pub(crate) fn secure_epoch(&self) -> u64 {
        self.snapshot().revision
    }

    pub(crate) async fn await_secure_state_change(&self, observed_epoch: u64) {
        let mut snapshot_rx = self.subscribe();
        // `subscribe()` clones the handle's long-lived receiver, whose seen
        // watermark never advances (`snapshot()` uses plain `borrow()`), so
        // `changed()` on a fresh clone is immediately ready after any publish
        // and the caller would spin instead of parking. `borrow_and_update()`
        // marks the current revision seen; `changed()` then waits for the
        // next publish past it.
        if snapshot_rx.borrow_and_update().revision > observed_epoch {
            return;
        }
        let cell = self.secure_cell();
        let cell_epoch = cell.epoch();
        tokio::select! {
            _ = snapshot_rx.changed() => {}
            _ = cell.wait_after(cell_epoch) => {}
        }
    }

    pub(crate) fn mark_secure_failed(&self) {
        self.secure_cell()
            .drive_terminal(crate::transport::SecureChannelState::Failed);
    }

    pub(crate) fn mark_secure_closed(&self) {
        self.secure_cell()
            .drive_terminal(crate::transport::SecureChannelState::Closed);
    }

    pub(crate) fn peer_id(&self) -> &ActrId {
        &self.peer_id
    }

    /// Stable identity of this Controller lifetime within its Manager.
    pub(crate) fn controller_lifecycle_id(&self) -> u64 {
        self.controller_lifecycle_id
    }

    /// The latest published snapshot (non-blocking; replays immediately even for
    /// a late subscriber — RFC: "SUBSCRIBE MUST replay the latest Snapshot
    /// immediately").
    pub(crate) fn snapshot(&self) -> RtcPeerSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub(crate) fn try_business_recovery_hint(&self) -> bool {
        let snapshot = self.snapshot();
        self.command_tx
            .try_send(RtcPeerCommand::BusinessRecoveryHint {
                transport_epoch: snapshot.transport_epoch,
                pc_generation: snapshot.pc_generation,
            })
            .is_ok()
    }

    /// Subscribe to snapshot updates; the receiver sees the latest immediately.
    pub(crate) fn subscribe(&self) -> watch::Receiver<RtcPeerSnapshot> {
        self.snapshot_rx.clone()
    }

    /// Subscribe directly to the Controller-maintained public snapshot. One
    /// watch channel per peer avoids allocating a forwarding task for every
    /// mobile/business subscriber while preserving immediate replay.
    pub(crate) fn subscribe_public(&self) -> watch::Receiver<actr_framework::WebRtcPeerState> {
        self.public_snapshot_rx.clone()
    }

    /// Inject the same state fact that `PeerConnectionEventHandler` publishes.
    /// This deliberately enters through the v0.20 mailbox so integration tests
    /// validate the production reducer/recovery path without resurrecting the
    /// removed 0.14 `ConnectionEvent` bus.
    #[cfg(feature = "test-utils")]
    pub(crate) fn inject_peer_connection_state_for_test(
        &self,
        state: webrtc::peer_connection::RTCPeerConnectionState,
    ) {
        let generation = self.snapshot().pc_generation;
        self.event_mailbox.try_publish(
            crate::wire::webrtc::v020::events::PeerDriverEvent::PeerConnectionStateChanged {
                generation,
                state,
            },
        );
    }

    /// The active lane handle for a lane on a generation, if registered and
    /// live (data plane; does not wait for a controller round).
    pub(crate) fn lane_handle(&self, generation: u64, lane: LaneKind) -> Option<LaneHandle> {
        self.lanes.get(generation, lane)
    }

    /// The shared active-lane registry (for the send path's `SendPermit`).
    pub(crate) fn lanes(&self) -> &ActiveLaneRegistry {
        &self.lanes
    }

    /// Exceptional Manager fallback after the Controller task has been
    /// aborted by the close-all overall deadline. This preserves explicit
    /// Session/DataChannel/PeerConnection cleanup instead of pretending Drop
    /// performs asynchronous WebRTC close.
    pub(crate) async fn force_close_sessions(&self) {
        self.effect_port.close_all_sessions().await;
    }

    /// Reliable ordered send on a lane (RFC: "Reliable RPC SHOULD use
    /// `send().await`"). Waits for send-buffer capacity (bounded by the
    /// channel's configured send-buffer limit). Preflights `Snapshot.sendable`,
    /// builds a `SendPermit`, and re-validates generation + cancellation before
    /// touching the channel.
    pub(crate) async fn send(
        &self,
        lane: LaneKind,
        payload: bytes::Bytes,
    ) -> Result<(), RtcSendError> {
        let permit = self.permit_for(lane).await?;
        self.send_with_permit(&permit, payload, false).await
    }

    /// Latency-first non-blocking send (RFC: "the latency-first Lane MAY fail
    /// fast with `try_send()`"). Fails fast with `DataChannelBufferFull` when the
    /// send buffer is full rather than blocking.
    pub(crate) async fn try_send(
        &self,
        lane: LaneKind,
        payload: bytes::Bytes,
    ) -> Result<(), RtcSendError> {
        let permit = self.permit_for(lane).await?;
        self.send_with_permit(&permit, payload, true).await
    }

    /// Internal send seam carrying delivery progress to the transport layer.
    /// `SendPermit` remains private to this module; callers receive only the
    /// stable RTC error plus the conservative progress classification.
    pub(crate) async fn send_with_progress(
        &self,
        lane: LaneKind,
        payload: bytes::Bytes,
        non_blocking: bool,
    ) -> RtcSendAttempt {
        let result = if non_blocking {
            self.try_send(lane, payload).await
        } else {
            self.send(lane, payload).await
        };
        RtcSendAttempt::from_result(result)
    }

    /// Run a send against a validated permit: `send`/`try_send`, mapping a
    /// session-close with outstanding bytes to `DataChunkDeliveryUncertain`.
    /// Send-buffer flow control is the webrtc crate's per-channel send-buffer
    /// limit (reliable lanes block in `send`, the latency-first lane fails
    /// fast in `try_send`).
    ///
    /// Every DataChannel message is prepended with an 8-byte fragment header
    /// (wire-compatible with 0.14 `WebRtcDataLane`). Payloads exceeding
    /// `DC_MAX_PAYLOAD_SIZE` are split across multiple frames; the receiver
    /// (in `lane_pump`) reassembles them before dispatch.
    async fn send_with_permit(
        &self,
        permit: &super::send::SendPermit,
        payload: bytes::Bytes,
        non_blocking: bool,
    ) -> Result<(), RtcSendError> {
        use crate::transport::{DC_MAX_PAYLOAD_SIZE, FRAGMENT_HEADER_SIZE, encode_fragment_header};

        // Shared outbound cap: the receiver's reassembly rejects anything
        // above `max_inbound_message_bytes` after the sender already reported
        // success, so reject the send up front instead.
        if payload.len() > self.max_message_bytes {
            return Err(RtcSendError::MessageTooLarge);
        }

        let msg_id = self.msg_id_counter.fetch_add(1, Ordering::Relaxed);
        let data_len = payload.len();

        if data_len <= DC_MAX_PAYLOAD_SIZE {
            // Single fragment (total_frags = 1).
            let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + data_len);
            encode_fragment_header(&mut buf, msg_id, 0, 1);
            buf.extend_from_slice(&payload);
            let frame = bytes::BytesMut::from(&buf[..]);
            let result = if non_blocking {
                permit.channel().try_send(frame).await
            } else {
                permit.channel().send(frame).await
            };
            match result {
                Ok(()) => Ok(()),
                Err(e) => Err(self.map_send_outcome(permit, e, false).await),
            }
        } else {
            // Multi-fragment send: split into DC_MAX_PAYLOAD_SIZE chunks.
            let total_frags = data_len.div_ceil(DC_MAX_PAYLOAD_SIZE);
            if total_frags > u16::MAX as usize {
                return Err(RtcSendError::MessageTooLarge);
            }
            let total_frags = total_frags as u16;
            let mut sent_any = false;
            tracing::debug!(
                peer = %self.peer_id().to_string_repr(),
                generation = permit.generation(),
                lane = ?permit.lane(),
                msg_id,
                total_bytes = data_len,
                fragments = total_frags,
                "v0.20 fragmenting message"
            );
            for (frag_index, chunk) in payload.chunks(DC_MAX_PAYLOAD_SIZE).enumerate() {
                let generation_current = permit.is_valid(self.snapshot().pc_generation);
                let ready_state = permit.channel().ready_state().await;
                let channel_open = matches!(
                    ready_state,
                    Ok(webrtc::data_channel::RTCDataChannelState::Open)
                );
                if sent_any && (!generation_current || !channel_open) {
                    return Err(RtcSendError::DataChunkDeliveryUncertain);
                }
                if !generation_current || !channel_open {
                    // Nothing of this message was sent yet. A definitively
                    // closed channel keeps `DataChannelClosed` so the
                    // transport layer invalidates the stale lane instead of
                    // retrying against generic not-ready; an unreadable or
                    // still-connecting channel stays `ConnectionNotReady`.
                    tracing::debug!(
                        peer = %self.peer_id().to_string_repr(),
                        generation = permit.generation(),
                        lane = ?permit.lane(),
                        generation_current,
                        channel_open,
                        ready_state = ?ready_state,
                        "v0.20 send preflight failed before any fragment was sent"
                    );
                    return Err(match ready_state {
                        Ok(webrtc::data_channel::RTCDataChannelState::Closed)
                        | Ok(webrtc::data_channel::RTCDataChannelState::Closing) => {
                            RtcSendError::DataChannelClosed
                        }
                        _ => RtcSendError::ConnectionNotReady,
                    });
                }
                let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + chunk.len());
                encode_fragment_header(&mut buf, msg_id, frag_index as u16, total_frags);
                buf.extend_from_slice(chunk);
                let frame = bytes::BytesMut::from(&buf[..]);
                let result = if non_blocking {
                    permit.channel().try_send(frame).await
                } else {
                    permit.channel().send(frame).await
                };
                if let Err(e) = result {
                    return Err(self.map_send_outcome(permit, e, sent_any).await);
                }
                sent_any = true;
                #[cfg(feature = "test-utils")]
                crate::transport::notify_webrtc_fragment_sent_for_test(
                    crate::transport::WebRtcFragmentSendEvent {
                        msg_id,
                        frag_index: frag_index as u16,
                        total_frags,
                        fragment_payload_len: chunk.len(),
                        message_len: data_len,
                    },
                )
                .await;
            }
            Ok(())
        }
    }

    /// Map a failed send to a stable error: if this fragmented send already
    /// enqueued a frame, or the channel reports other outstanding bytes, a
    /// close makes delivery uncertain; otherwise the send failed normally.
    async fn map_send_outcome(
        &self,
        permit: &super::send::SendPermit,
        error: webrtc::error::Error,
        sent_any: bool,
    ) -> RtcSendError {
        let base = super::send::map_send_error(&error);
        if !matches!(
            base,
            RtcSendError::ConnectionNotReady | RtcSendError::DataChannelClosed
        ) {
            return base;
        }
        // The send failed as not-ready. If there were outstanding (unconfirmed)
        // bytes in the buffer when the session closed, the delivery is uncertain
        // (RFC: "Actr SHOULD report DataChunkDeliveryUncertain").
        let outstanding = permit.channel().outstanding_bytes().await.unwrap_or(0);
        if sent_any || outstanding > 0 {
            tracing::debug!(
                peer = %self.peer_id().to_string_repr(),
                generation = permit.generation(),
                lane = ?permit.lane(),
                outstanding_bytes = outstanding,
                "v0.20 send failed with delivery uncertain (buffered or sent frames)"
            );
            RtcSendError::DataChunkDeliveryUncertain
        } else if matches!(base, RtcSendError::DataChannelClosed) {
            tracing::warn!(
                peer = %self.peer_id().to_string_repr(),
                generation = permit.generation(),
                lane = ?permit.lane(),
                "v0.20 send failed on a closed data channel"
            );
            base
        } else {
            base
        }
    }

    /// Build + validate a `SendPermit` for a lane: `Snapshot.sendable` preflight,
    /// get-or-lazily-create the lane handle, and re-validate generation +
    /// cancellation (RFC §"Send path and SendPermit" steps 2/3/4/5).
    async fn permit_for(&self, lane: LaneKind) -> Result<super::send::SendPermit, RtcSendError> {
        // Preflight: the peer must be sendable per the authoritative snapshot.
        let snapshot = self.snapshot();
        if !snapshot.sendable {
            tracing::trace!(
                peer = %self.peer_id().to_string_repr(),
                ?lane,
                "v0.20 send permit denied: peer not sendable"
            );
            return Err(RtcSendError::ConnectionNotReady);
        }
        let handle = self.lane_or_ensure(lane, snapshot.pc_generation).await?;
        let permit =
            super::send::SendPermit::from_handle(self.peer_id(), snapshot.pc_generation, &handle);
        // Final race guard: re-read the snapshot; the generation must still be
        // current and the session uncancelled before we touch the channel.
        if !permit.is_valid(self.snapshot().pc_generation) {
            tracing::debug!(
                peer = %self.peer_id().to_string_repr(),
                generation = permit.generation(),
                ?lane,
                "v0.20 send permit stale: generation moved or session cancelled"
            );
            return Err(RtcSendError::ConnectionNotReady);
        }
        Ok(permit)
    }

    /// Get the lane handle, lazily creating the lane via the controller if it
    /// does not exist yet, waiting (bounded) for registration.
    async fn lane_or_ensure(
        &self,
        lane: LaneKind,
        generation: u64,
    ) -> Result<LaneHandle, RtcSendError> {
        if let Some(handle) = self.lane_handle(generation, lane) {
            return Ok(handle);
        }
        // Lazily create the lane via the controller.
        self.send_command(RtcPeerCommand::EnsureLane(lane))
            .await
            .map_err(|_| RtcSendError::ConnectionNotReady)?;
        // Bounded wait for the controller to create + register the lane. N1:
        // sleep on the registry's change watch instead of busy-polling; the
        // outer timer-facade timeout bounds the wait. The handle holds a
        // registry clone, so the watch sender outlives this receiver and
        // `changed()` cannot fail.
        let mut changes = self.lanes.subscribe_changes();
        let wait = async {
            loop {
                if let Some(handle) = self.lane_handle(generation, lane) {
                    return handle;
                }
                let _ = changes.changed().await;
            }
        };
        match crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(5),
            wait,
        )
        .await
        {
            Ok(handle) => Ok(handle),
            // Tolerate an insert landing exactly at the deadline.
            Err(_) => self
                .lane_handle(generation, lane)
                .ok_or(RtcSendError::ConnectionNotReady),
        }
    }

    /// Ask the controller to ensure the peer is connected (offerer flow).
    /// Returns the transport epoch after the command has been reduced and its
    /// snapshot published, rather than merely acknowledging mailbox enqueue.
    pub(crate) async fn connect(&self) -> Result<u64, ControllerGone> {
        let (respond, committed) = tokio::sync::oneshot::channel();
        self.send_command_with_span(
            RtcPeerCommand::ConnectCommitted { respond },
            tracing::Span::current(),
            false,
        )
        .await?;
        committed.await.map_err(|_| ControllerGone::Gone)
    }

    /// Deliver one signaling message while preserving the receive-side span
    /// across the controller mailbox. The span is library metadata only; the
    /// reducer still receives the same business command as before.
    pub(crate) async fn apply_signaling_message_with_span(
        &self,
        message: SignalingMessage,
        span: tracing::Span,
    ) -> Result<(), ControllerGone> {
        let command = match message {
            SignalingMessage::Offer { desc, exchange_id } => {
                RtcPeerCommand::ApplyRemoteDescriptionCorrelated { desc, exchange_id }
            }
            SignalingMessage::Answer { desc, exchange_id } => {
                RtcPeerCommand::ApplyRemoteDescriptionCorrelated { desc, exchange_id }
            }
            SignalingMessage::IceRestartOffer { desc, exchange_id } => {
                RtcPeerCommand::ApplyRemoteIceRestartOfferCorrelated {
                    desc,
                    exchange_id: Some(exchange_id),
                }
            }
            SignalingMessage::IceRestartRequest {
                request_id,
                attempt,
            } => RtcPeerCommand::ApplyIceRestartRequest {
                request_id,
                attempt,
            },
            SignalingMessage::Candidate(candidate) => {
                RtcPeerCommand::ApplyRemoteCandidate(candidate)
            }
            SignalingMessage::RoleAssignment { is_offerer } => {
                RtcPeerCommand::ApplyRoleAssignment { is_offerer }
            }
            SignalingMessage::IceRestartRejected { .. } => RtcPeerCommand::NetworkEvent(
                crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
            ),
            SignalingMessage::IceRestartRequestAck {
                request_id,
                attempt,
                outcome,
            } => RtcPeerCommand::ApplyIceRestartRequestAck {
                request_id,
                attempt,
                outcome,
            },
            SignalingMessage::RoleNegotiation => return Ok(()),
        };
        self.send_command_with_span(command, span, true).await
    }

    /// Terminate only the initial attempt observed by a timed-out caller.
    /// WirePool may already have started a newer retry on the same Controller;
    /// the transport epoch makes this cancellation safe in that race.
    pub(crate) async fn abort_transport_attempt(
        &self,
        transport_epoch: u64,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::AbortTransportAttempt { transport_epoch })
            .await
    }

    /// Ask the controller to close the peer (terminal close barrier).
    pub(crate) async fn close(&self) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::Close).await
    }

    /// Refresh ICE/TURN configuration on the current generation. The reducer
    /// escalates a rejected in-place update to hard recovery.
    pub(crate) async fn refresh_configuration(
        &self,
        configuration: webrtc::peer_connection::RTCConfiguration,
    ) -> Result<(), super::reducer::RtcConfigurationRefreshError> {
        let (respond, result) = tokio::sync::oneshot::channel();
        self.send_command(RtcPeerCommand::RefreshConfiguration {
            config: Box::new(configuration),
            respond: Some(respond),
        })
        .await
        .map_err(|_| super::reducer::RtcConfigurationRefreshError::ControllerGone)?;
        result
            .await
            .map_err(|_| super::reducer::RtcConfigurationRefreshError::ControllerGone)?
    }

    /// S6-2: add a local media track on the active session (media topology).
    /// Serialized by the controller; the response carries the new RTP sender.
    pub(crate) async fn add_track(
        &self,
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, String> {
        let (respond, rx) = tokio::sync::oneshot::channel();
        self.send_command(RtcPeerCommand::AddTrack { track, respond })
            .await
            .map_err(|_| "peer controller is no longer running".to_string())?;
        rx.await
            .map_err(|_| "peer controller dropped the track add".to_string())?
    }

    /// S6-2: remove a previously added local track by its sender.
    pub(crate) async fn remove_track(
        &self,
        sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Result<(), String> {
        let (respond, rx) = tokio::sync::oneshot::channel();
        self.send_command(RtcPeerCommand::RemoveTrack { sender, respond })
            .await
            .map_err(|_| "peer controller is no longer running".to_string())?;
        rx.await
            .map_err(|_| "peer controller dropped the track remove".to_string())?
    }

    // ── S6-10: outbound media (0.14 add/send/remove_media_track parity) ─────

    /// Add a media track on the active session and register its outbound
    /// writer. Re-adding the same `track_id` is a no-op (0.14 reuse
    /// semantics). The resulting renegotiation rides the controller's
    /// `NegotiationNeeded` effect path.
    ///
    /// BREAKING (v0.20): renegotiation is offerer-only. On an answerer-role
    /// peer the track registers locally but the re-offer never happens, so
    /// the remote never sees it — answerer-initiated media renegotiation is
    /// deliberately unsupported (RFC-0504 §"Deliberate regressions"). The
    /// offerer side must own media topology changes (or the roles must be
    /// swapped at connect time).
    pub(crate) async fn add_media_track(
        &self,
        track_id: &str,
        codec: &str,
        media_type: &str,
    ) -> Result<(), String> {
        if self
            .media_writers
            .lock()
            .expect("media writers lock")
            .contains_key(track_id)
        {
            return Ok(());
        }
        let writer = Arc::new(OutboundTrackWriter::new(track_id, codec, media_type)?);
        let sender = self.add_track(writer.track()).await?;
        writer.set_sender(sender);
        self.media_writers
            .lock()
            .expect("media writers lock")
            .insert(track_id.to_string(), writer);
        Ok(())
    }

    /// Remove a media track: unregister the writer and remove the track from
    /// the active session (triggers renegotiation).
    pub(crate) async fn remove_media_track(&self, track_id: &str) -> Result<(), String> {
        let writer = self
            .media_writers
            .lock()
            .expect("media writers lock")
            .remove(track_id)
            .ok_or_else(|| format!("track not found: {track_id}"))?;
        let sender = writer
            .sender()
            .ok_or_else(|| format!("track {track_id} was never registered"))?;
        self.remove_track(sender).await
    }

    /// Send one media sample on a registered track (data plane: straight to
    /// the track's RTP writer, no controller round-trip).
    pub(crate) async fn send_media_sample(
        &self,
        track_id: &str,
        sample: &actr_framework::MediaSample,
    ) -> Result<(), String> {
        let writer = self
            .media_writers
            .lock()
            .expect("media writers lock")
            .get(track_id)
            .cloned()
            .ok_or_else(|| format!("track not found: {track_id}"))?;
        writer.write_sample(sample).await.map_err(|e| e.to_string())
    }

    /// Deliver a remote SDP offer/answer (from the signaling router).
    pub(crate) async fn apply_remote_description(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::ApplyRemoteDescription(Box::new(desc)))
            .await
    }

    /// Deliver a wire-tagged ICE restart offer. This distinction lets the
    /// reducer keep a Soft Recovery on the current PC while treating an
    /// ordinary changed-ufrag Offer as a fresh physical connection.
    pub(crate) async fn apply_remote_ice_restart_offer(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::ApplyRemoteIceRestartOffer(Box::new(desc)))
            .await
    }

    /// Apply a restart request and return the outcome committed by the
    /// controller reducer. The signaling router uses this to avoid deriving
    /// Ack state from a snapshot taken before the command was serialized.
    pub(crate) async fn apply_ice_restart_request_with_outcome(
        &self,
        request_id: String,
        attempt: u32,
    ) -> Result<i32, ControllerGone> {
        let (respond, outcome) = oneshot::channel();
        self.send_command_with_span(
            RtcPeerCommand::ApplyIceRestartRequestCommitted {
                request_id,
                attempt,
                respond,
            },
            tracing::Span::current(),
            true,
        )
        .await?;
        outcome.await.map_err(|_| ControllerGone::Gone)
    }

    pub(crate) async fn signaling_delivery_failed(
        &self,
        transport_epoch: u64,
        pc_generation: u64,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::SignalingDeliveryFailed {
            transport_epoch,
            pc_generation,
        })
        .await
    }

    /// Best-effort response-path prompt. The snapshot identity is captured at
    /// submission so the reducer can discard a hint that races a replacement
    /// Controller/PC attempt without blocking the caller on a full mailbox.
    pub(crate) fn try_recovery_hint(&self, request_id: String) -> bool {
        let snapshot = self.snapshot();
        self.command_tx
            .try_send(RtcPeerCommand::RecoveryHint {
                request_id,
                transport_epoch: snapshot.transport_epoch,
                pc_generation: snapshot.pc_generation,
            })
            .is_ok()
    }

    /// Deliver the server's role assignment (from the signaling router).
    pub(crate) async fn apply_role_assignment(
        &self,
        is_offerer: bool,
    ) -> Result<(), ControllerGone> {
        self.apply_signaling_message_with_span(
            SignalingMessage::RoleAssignment { is_offerer },
            tracing::Span::current(),
        )
        .await
    }

    pub(crate) async fn target_unavailable(
        &self,
        transport_epoch: u64,
        pc_generation: u64,
        negotiation_epoch: u64,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::TargetUnavailable {
            transport_epoch,
            pc_generation,
            negotiation_epoch,
        })
        .await
    }

    /// Preserve the triggering inbound message as a causal link when the router
    /// escalates a shed or timed-out dispatch through the controller mailbox.
    pub(crate) async fn force_reconnect_with_span(
        &self,
        span: tracing::Span,
    ) -> Result<(), ControllerGone> {
        self.send_command_with_span(
            RtcPeerCommand::NetworkEvent(
                crate::wire::webrtc::v020::recovery::RtcNetworkEvent::ForceReconnect,
            ),
            span,
            true,
        )
        .await
    }

    async fn send_command(&self, command: RtcPeerCommand) -> Result<(), ControllerGone> {
        self.command_tx
            .send(command)
            .await
            .map_err(|_| ControllerGone::Gone)
    }

    async fn send_command_with_span(
        &self,
        command: RtcPeerCommand,
        span: tracing::Span,
        remote: bool,
    ) -> Result<(), ControllerGone> {
        self.send_command(RtcPeerCommand::Traced {
            span,
            remote,
            command: Box::new(command),
        })
        .await
    }
}
