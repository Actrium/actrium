//! The injectable seam between the controller and the physical PeerConnection.
//!
//! The controller's effect shell runs blocking effects (`BuildSession`,
//! `CreateOffer`, …) against a per-generation session. To keep T1's fake
//! small and deterministic, the operations the controller actually needs are
//! abstracted behind the narrow [`SessionOps`] trait rather than the full
//! webrtc-rs `PeerConnection` trait. Production backs it
//! with [`RealSessionOps`], a thin object-safe wrapper over the real
//! `Arc<dyn PeerConnection>`; T1 tests substitute a recording effect sink or a
//! narrow fake. This is the RFC effect boundary, not a callback adapter.

use std::sync::Arc;

use async_trait::async_trait;

use webrtc::data_channel::RTCDataChannelInit;
use webrtc::error::Error as RtcError;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::peer_connection::{
    PeerConnection, RTCConfiguration, RTCIceCandidateInit, RTCSessionDescription,
};

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;

use super::adapter::{ActrRtpSender, LaneDataChannel, RealLaneDataChannel, RealRtpSender};

/// The subset of `PeerConnection` operations the controller drives. Object-safe
/// so a session can hold `Arc<dyn SessionOps>` (real or fake).
#[async_trait]
pub(crate) trait SessionOps: Send + Sync {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError>;
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError>;
    async fn set_local_description(&self, desc: RTCSessionDescription) -> Result<(), RtcError>;
    async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<(), RtcError>;
    async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<(), RtcError>;
    async fn restart_ice(&self) -> Result<(), RtcError>;
    /// S5-6: refresh the ICE server / TURN credential configuration in-place
    /// (RFC §"Soft Recovery": "TURN credential/config updates can take effect
    /// via the current PC"). A rejected change escalates to Hard Recovery.
    async fn set_configuration(&self, config: RTCConfiguration) -> Result<(), RtcError>;
    /// Create a data channel with an explicit init (per-lane QoS via
    /// `lane::lane_config`).
    async fn create_data_channel(
        &self,
        label: &str,
        init: RTCDataChannelInit,
    ) -> Result<Arc<dyn LaneDataChannel>, RtcError>;
    /// S6-2: add a local media track (media topology). The returned sender
    /// owns the RTP send side. Track add/remove serializes renegotiation
    /// through the controller (RFC §"DataChannel APIs": "Track add/remove
    /// serializes renegotiation through the Controller effect").
    async fn add_track(
        &self,
        track: Arc<dyn TrackLocal>,
    ) -> Result<Arc<dyn ActrRtpSender>, RtcError>;
    /// S6-2: remove a previously added local track by its sender.
    async fn remove_track(&self, sender: &Arc<dyn ActrRtpSender>) -> Result<(), RtcError>;
    /// S6-2: the connection's current RTP senders (media observation).
    async fn get_senders(&self) -> Vec<Arc<dyn ActrRtpSender>>;
    /// S6-3: one full stats sample (`get_stats(Instant::now(), None)`; the
    /// real API is infallible). Used to resolve the selected candidate pair
    /// into the snapshot's `network_path` on entering Sendable.
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport;
    async fn close(&self) -> Result<(), RtcError>;
}

/// Production `SessionOps`: forwards to the real v0.20 `PeerConnection`.
pub(crate) struct RealSessionOps {
    pc: Arc<dyn PeerConnection>,
}

impl RealSessionOps {
    pub(crate) fn new(pc: Arc<dyn PeerConnection>) -> Self {
        Self { pc }
    }

    /// The underlying real connection (used for data-channel creation and stats
    /// from Stage 3/4 onward).
    #[allow(dead_code)]
    pub(crate) fn peer_connection(&self) -> &Arc<dyn PeerConnection> {
        &self.pc
    }
}

#[async_trait]
impl SessionOps for RealSessionOps {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
        self.pc.create_offer(None).await
    }
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
        self.pc.create_answer(None).await
    }
    async fn set_local_description(&self, desc: RTCSessionDescription) -> Result<(), RtcError> {
        let expected_type = desc.sdp_type;
        self.pc.set_local_description(desc).await?;
        // Reconcile through the v0.20 current/pending description accessors.
        // Which slot is populated depends on the signaling state, so accept
        // either, but never report a successful operation when the driver has
        // no matching visible local description.
        let local = self.pc.local_description().await;
        let current = self.pc.current_local_description().await;
        let pending = self.pc.pending_local_description().await;
        if [local.as_ref(), current.as_ref(), pending.as_ref()]
            .into_iter()
            .flatten()
            .any(|visible| visible.sdp_type == expected_type)
        {
            Ok(())
        } else {
            Err(RtcError::Other(
                "v0.20 accepted local description but exposed no matching current/pending description"
                    .into(),
            ))
        }
    }
    async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<(), RtcError> {
        let expected_type = desc.sdp_type;
        self.pc.set_remote_description(desc).await?;
        let remote = self.pc.remote_description().await;
        let current = self.pc.current_remote_description().await;
        let pending = self.pc.pending_remote_description().await;
        if ![remote.as_ref(), current.as_ref(), pending.as_ref()]
            .into_iter()
            .flatten()
            .any(|visible| visible.sdp_type == expected_type)
        {
            return Err(RtcError::Other(
                "v0.20 accepted remote description but exposed no matching current/pending description"
                    .into(),
            ));
        }
        // The result is observation-only here: candidate generation/ufrag
        // ownership remains in the Controller, while the v0.20 API tells us
        // whether subsequent trickle delivery is known to be supported.
        let can_trickle = self.pc.can_trickle_ice_candidates().await;
        tracing::trace!(?can_trickle, "reconciled v0.20 remote description");
        Ok(())
    }
    async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<(), RtcError> {
        self.pc.add_ice_candidate(candidate).await
    }
    async fn restart_ice(&self) -> Result<(), RtcError> {
        self.pc.restart_ice().await
    }
    async fn set_configuration(&self, config: RTCConfiguration) -> Result<(), RtcError> {
        let requested_servers = config.ice_servers().to_vec();
        let _previous = self.pc.get_configuration().await;
        self.pc.set_configuration(config).await?;
        let applied = self.pc.get_configuration().await;
        if applied.ice_servers() != requested_servers {
            return Err(RtcError::Other(
                "v0.20 configuration refresh did not adopt the requested ICE server set".into(),
            ));
        }
        Ok(())
    }
    async fn create_data_channel(
        &self,
        label: &str,
        init: RTCDataChannelInit,
    ) -> Result<Arc<dyn LaneDataChannel>, RtcError> {
        let channel = self.pc.create_data_channel(label, Some(init)).await?;
        Ok(Arc::new(RealLaneDataChannel::new(channel)))
    }
    async fn add_track(
        &self,
        track: Arc<dyn TrackLocal>,
    ) -> Result<Arc<dyn ActrRtpSender>, RtcError> {
        let transceiver = self.pc.add_transceiver_from_track(track, None).await?;
        let sender = transceiver.sender().await?.ok_or_else(|| {
            RtcError::Other("v0.20 transceiver did not expose its RTP sender".into())
        })?;
        Ok(Arc::new(RealRtpSender::new(sender)))
    }
    async fn remove_track(&self, sender: &Arc<dyn ActrRtpSender>) -> Result<(), RtcError> {
        let sender = sender
            .upstream()
            .ok_or_else(|| RtcError::Other("RTP sender is not backed by WebRTC".into()))?;
        self.pc.remove_track(&sender).await
    }
    async fn get_senders(&self) -> Vec<Arc<dyn ActrRtpSender>> {
        self.pc
            .get_senders()
            .await
            .into_iter()
            .map(|sender| Arc::new(RealRtpSender::new(sender)) as Arc<dyn ActrRtpSender>)
            .collect()
    }
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
        let now = std::time::Instant::now();
        let report = self
            .pc
            .get_stats(now, webrtc::peer_connection::StatsSelector::None)
            .await;
        // Exercise the v0.20 selector-specific media observation path at the
        // same staggered/on-demand sampling point as the peer-level report.
        // These reports are intentionally observation-only and cannot gate
        // sendability.
        for sender in self.pc.get_senders().await {
            let _ = self
                .pc
                .get_stats(
                    now,
                    webrtc::peer_connection::StatsSelector::Sender(sender.id()),
                )
                .await;
        }
        for receiver in self.pc.get_receivers().await {
            let _ = self
                .pc
                .get_stats(
                    now,
                    webrtc::peer_connection::StatsSelector::Receiver(receiver.id()),
                )
                .await;
        }
        let _transceivers = self.pc.get_transceivers().await;
        report
    }
    async fn close(&self) -> Result<(), RtcError> {
        self.pc.close().await
    }
}

/// Builds the session (`Arc<dyn SessionOps>`) for one PC generation, with the
/// per-generation event handler pre-injected (RFC: "Handler MUST be created and
/// injected before `PeerConnectionBuilder::build()`").
#[async_trait]
pub(crate) trait SessionFactory: Send + Sync {
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError>;
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared fakes for reducer/controller tests (deterministic, no network).
    use super::*;
    use bytes::BytesMut;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Notify;
    use webrtc::data_channel::{DataChannelEvent, RTCDataChannelState};

    /// A `SessionOps` whose every method returns an unremarkable success. Used
    /// where the reducer only needs a session *payload* it will ignore.
    pub(crate) struct NoopSessionOps;

    #[async_trait]
    impl SessionOps for NoopSessionOps {
        async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
            Ok(RTCSessionDescription::default())
        }
        async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
            Ok(RTCSessionDescription::default())
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
            _label: &str,
            _init: RTCDataChannelInit,
        ) -> Result<Arc<dyn LaneDataChannel>, RtcError> {
            // Never exercised: pure-reducer tests ignore the session payload.
            Err(RtcError::Other("NoopSessionOps has no data channel".into()))
        }
        async fn add_track(
            &self,
            _track: Arc<dyn TrackLocal>,
        ) -> Result<Arc<dyn ActrRtpSender>, RtcError> {
            Err(RtcError::Other("NoopSessionOps has no media".into()))
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

    pub(crate) fn noop_session_ops() -> Arc<dyn SessionOps> {
        Arc::new(NoopSessionOps)
    }

    /// A minimal fake `DataChannel` whose `poll()` reports `OnOpen` once (then
    /// blocks), so a lane pump reports the readiness anchor without a real
    /// network. Everything else is an inert stub.
    pub(crate) struct FakeDataChannel {
        label: String,
        opened: AtomicBool,
    }

    impl FakeDataChannel {
        pub(crate) fn new(label: &str) -> Self {
            Self {
                label: label.to_string(),
                opened: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl LaneDataChannel for FakeDataChannel {
        async fn label(&self) -> Result<String, RtcError> {
            Ok(self.label.clone())
        }
        async fn negotiated(&self) -> Result<bool, RtcError> {
            Ok(false)
        }
        async fn ready_state(&self) -> Result<RTCDataChannelState, RtcError> {
            Ok(RTCDataChannelState::Open)
        }
        async fn outstanding_bytes(&self) -> Result<usize, RtcError> {
            Ok(0)
        }
        async fn send(&self, _data: BytesMut) -> Result<(), RtcError> {
            Ok(())
        }
        async fn send_text(&self, _text: &str) -> Result<(), RtcError> {
            Ok(())
        }
        async fn try_send(&self, _data: BytesMut) -> Result<(), RtcError> {
            Ok(())
        }
        async fn writable(&self) -> Result<(), RtcError> {
            Ok(())
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            if !self.opened.swap(true, Ordering::SeqCst) {
                Some(DataChannelEvent::OnOpen)
            } else {
                // Stay open: no further events (the pump waits without closing).
                std::future::pending().await
            }
        }
        async fn close(&self) -> Result<(), RtcError> {
            Ok(())
        }
    }

    pub(crate) fn fake_data_channel(label: &str) -> Arc<dyn LaneDataChannel> {
        Arc::new(FakeDataChannel::new(label))
    }

    /// A fake `DataChannel` that replays a scripted sequence of `DataChannelEvent`s
    /// from its `poll()`, then ends (pump exits on `OnClose` or an empty queue).
    pub(crate) struct ScriptedDataChannel {
        label: String,
        events: std::sync::Mutex<std::collections::VecDeque<DataChannelEvent>>,
    }

    impl ScriptedDataChannel {
        pub(crate) fn new(label: &str, events: Vec<DataChannelEvent>) -> Self {
            Self {
                label: label.to_string(),
                events: std::sync::Mutex::new(events.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl LaneDataChannel for ScriptedDataChannel {
        async fn label(&self) -> Result<String, RtcError> {
            Ok(self.label.clone())
        }
        async fn negotiated(&self) -> Result<bool, RtcError> {
            Ok(false)
        }
        async fn ready_state(&self) -> Result<RTCDataChannelState, RtcError> {
            Ok(RTCDataChannelState::Open)
        }
        async fn outstanding_bytes(&self) -> Result<usize, RtcError> {
            Ok(0)
        }
        async fn send(&self, _data: BytesMut) -> Result<(), RtcError> {
            Ok(())
        }
        async fn send_text(&self, _text: &str) -> Result<(), RtcError> {
            Ok(())
        }
        async fn try_send(&self, _data: BytesMut) -> Result<(), RtcError> {
            Ok(())
        }
        async fn writable(&self) -> Result<(), RtcError> {
            Ok(())
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            self.events
                .lock()
                .expect("scripted channel lock")
                .pop_front()
        }
        async fn close(&self) -> Result<(), RtcError> {
            Ok(())
        }
    }

    pub(crate) fn scripted_data_channel(
        label: &str,
        events: Vec<DataChannelEvent>,
    ) -> Arc<dyn LaneDataChannel> {
        Arc::new(ScriptedDataChannel::new(label, events))
    }

    /// A fake `DataChannel` that records `send`/`try_send` payloads and reports
    /// `OnOpen` once via `poll()` (so the lane reaches readiness). Used by the
    /// send-path tests to assert what the send path actually wrote.
    pub(crate) struct RecordingDataChannel {
        label: String,
        opened: AtomicBool,
        closed: AtomicBool,
        open_gate: Option<Arc<Notify>>,
        pub(crate) sent: std::sync::Mutex<Vec<BytesMut>>,
        pub(crate) try_sent: std::sync::Mutex<Vec<BytesMut>>,
        pub(crate) fail_try_send: AtomicBool,
        pub(crate) fail_send: AtomicBool,
        pub(crate) outstanding: std::sync::atomic::AtomicUsize,
        pub(crate) close_calls: std::sync::atomic::AtomicUsize,
    }

    impl RecordingDataChannel {
        pub(crate) fn new(label: &str) -> Self {
            Self::new_with_open_gate(label, None)
        }

        pub(crate) fn new_with_open_gate(label: &str, open_gate: Option<Arc<Notify>>) -> Self {
            Self {
                label: label.to_string(),
                opened: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                open_gate,
                sent: std::sync::Mutex::new(Vec::new()),
                try_sent: std::sync::Mutex::new(Vec::new()),
                fail_try_send: AtomicBool::new(false),
                fail_send: AtomicBool::new(false),
                outstanding: std::sync::atomic::AtomicUsize::new(0),
                close_calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl LaneDataChannel for RecordingDataChannel {
        async fn label(&self) -> Result<String, RtcError> {
            Ok(self.label.clone())
        }
        async fn negotiated(&self) -> Result<bool, RtcError> {
            Ok(false)
        }
        async fn ready_state(&self) -> Result<RTCDataChannelState, RtcError> {
            Ok(RTCDataChannelState::Open)
        }
        async fn outstanding_bytes(&self) -> Result<usize, RtcError> {
            Ok(self.outstanding.load(Ordering::SeqCst))
        }
        async fn send(&self, data: BytesMut) -> Result<(), RtcError> {
            if self.fail_send.load(Ordering::SeqCst) || self.closed.load(Ordering::SeqCst) {
                return Err(RtcError::ErrDataChannelClosed);
            }
            self.sent.lock().expect("record lock").push(data);
            Ok(())
        }
        async fn send_text(&self, text: &str) -> Result<(), RtcError> {
            self.send(BytesMut::from(text)).await
        }
        async fn try_send(&self, data: BytesMut) -> Result<(), RtcError> {
            if self.fail_try_send.load(Ordering::SeqCst) {
                return Err(RtcError::ErrSendBufferFull);
            }
            if self.closed.load(Ordering::SeqCst) {
                return Err(RtcError::ErrDataChannelClosed);
            }
            self.try_sent.lock().expect("record lock").push(data);
            Ok(())
        }
        async fn writable(&self) -> Result<(), RtcError> {
            if self.closed.load(Ordering::SeqCst) {
                Err(RtcError::ErrDataChannelClosed)
            } else {
                Ok(())
            }
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            if !self.opened.swap(true, Ordering::SeqCst) {
                if let Some(open_gate) = &self.open_gate {
                    open_gate.notified().await;
                }
                Some(DataChannelEvent::OnOpen)
            } else {
                std::future::pending().await
            }
        }
        async fn close(&self) -> Result<(), RtcError> {
            self.close_calls.fetch_add(1, Ordering::SeqCst);
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    pub(crate) fn recording_data_channel(label: &str) -> Arc<RecordingDataChannel> {
        Arc::new(RecordingDataChannel::new(label))
    }

    pub(crate) fn gated_recording_data_channel(
        label: &str,
        open_gate: Arc<Notify>,
    ) -> Arc<RecordingDataChannel> {
        Arc::new(RecordingDataChannel::new_with_open_gate(
            label,
            Some(open_gate),
        ))
    }
}
