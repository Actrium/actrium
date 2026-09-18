//! `WebRtcEventHandler` (RFC §"WebRtcEventHandler").
//!
//! One handler instance is created per PC generation and injected into
//! `PeerConnectionBuilder::build()` *before* any event can fire, so even events
//! produced during `build()` have a defined receiver (RFC: "Handler MUST be
//! created and injected before `PeerConnectionBuilder::build()`").
//!
//! The handler is a pure translator: it converts each RTC
//! `PeerConnectionEventHandler` callback into a [`PeerDriverEvent`] and publishes
//! it to the peer's mailbox with the non-blocking [`PeerEventMailbox::try_publish`].
//!
//! Per the RFC's hard rules, the handler MUST NOT:
//!
//! * call business hooks;
//! * take a Coordinator/Manager global write lock;
//! * await a (potentially back-pressured) channel send;
//! * perform Offer/Answer;
//! * call `restart_ice()` or `close()`;
//! * start long-running DataChannel/Track pump loops;
//! * process business messages.
//!
//! State callbacks perform a single non-blocking publish. Raw DataChannel/Track
//! callbacks perform a bounded registry insertion and schedule only a finite,
//! generation-owned metadata normalization task; they never start the sole
//! `poll()` consumer. The handler holds no PC handle or hook references.

use std::sync::Arc;

use actr_protocol::ActrId;
use async_trait::async_trait;

use webrtc::data_channel::DataChannel as UpstreamDataChannel;
use webrtc::media_stream::track_remote::TrackRemote;
use webrtc::peer_connection::{
    PeerConnectionEventHandler, RTCIceConnectionState, RTCIceGatheringState,
    RTCPeerConnectionIceErrorEvent, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
    RTCSignalingState,
};

use super::events::{DataChannelDescriptor, PeerDriverEvent, PeerEventMailbox, TrackDescriptor};
use super::peer::adapter::{LaneDataChannel, RealLaneDataChannel};
use super::peer::session::SessionResourceRegistry;

/// Translates RTC driver callbacks for one PC generation into
/// [`PeerDriverEvent`]s on the peer mailbox.
pub(crate) struct WebRtcEventHandler {
    peer_id: ActrId,
    generation: u64,
    event_tx: PeerEventMailbox,
    resources: SessionResourceRegistry,
}

impl WebRtcEventHandler {
    pub(crate) fn new(peer_id: ActrId, generation: u64, event_tx: PeerEventMailbox) -> Self {
        Self::new_with_resource_limit(
            peer_id,
            generation,
            event_tx,
            crate::wire::webrtc::rtc::config::RtcBufferConfig::default().max_session_resources,
        )
    }

    pub(crate) fn new_with_resource_limit(
        peer_id: ActrId,
        generation: u64,
        event_tx: PeerEventMailbox,
        max_session_resources: usize,
    ) -> Self {
        Self {
            peer_id,
            generation,
            event_tx,
            resources: SessionResourceRegistry::with_limit(max_session_resources),
        }
    }

    /// Registry created before `build()` and later transferred into the
    /// generation's `PeerConnectionSession` by the effect runner.
    pub(crate) fn resources(&self) -> SessionResourceRegistry {
        self.resources.clone()
    }

    pub(crate) fn peer_id(&self) -> &ActrId {
        &self.peer_id
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    fn publish(&self, event: PeerDriverEvent) {
        self.event_tx.try_publish(event);
    }

    /// A clone of the handler's mailbox sender. Tests use this to drive fake
    /// driver events into the controller's mailbox (the production driver uses
    /// the handler directly). The handler remains a pure translator.
    #[cfg(test)]
    pub(crate) fn mailbox(&self) -> PeerEventMailbox {
        self.event_tx.clone()
    }

    /// Drive the data-channel callback with Actr's narrow adapter in tests.
    #[cfg(test)]
    pub(crate) async fn on_test_data_channel(&self, data_channel: Arc<dyn LaneDataChannel>) {
        self.handle_data_channel(data_channel).await;
    }

    async fn handle_data_channel(&self, data_channel: Arc<dyn LaneDataChannel>) {
        let Ok(resource_id) = self.resources.try_insert_data_channel(data_channel.clone()) else {
            self.event_tx.mark_overflow(self.generation);
            return;
        };
        let generation = self.generation;
        let event_tx = self.event_tx.clone();
        self.resources.spawn_normalization(async move {
            let descriptor = DataChannelDescriptor {
                label: data_channel.label().await.unwrap_or_default(),
                negotiated: data_channel.negotiated().await.unwrap_or(false),
            };
            event_tx.try_publish(PeerDriverEvent::DataChannelDiscovered {
                generation,
                resource_id,
                descriptor,
            });
        });
    }
}

#[async_trait]
impl PeerConnectionEventHandler for WebRtcEventHandler {
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation = self.generation,
            ?state,
            "RTC peer connection state changed"
        );
        self.publish(PeerDriverEvent::PeerConnectionStateChanged {
            generation: self.generation,
            state,
        });
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation = self.generation,
            ?state,
            "RTC ICE connection state changed"
        );
        self.publish(PeerDriverEvent::IceConnectionStateChanged {
            generation: self.generation,
            state,
        });
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation = self.generation,
            ?state,
            "RTC ICE gathering state changed"
        );
        self.publish(PeerDriverEvent::IceGatheringStateChanged {
            generation: self.generation,
            state,
        });
    }

    async fn on_signaling_state_change(&self, state: RTCSignalingState) {
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation = self.generation,
            ?state,
            "RTC signaling state changed"
        );
        self.publish(PeerDriverEvent::SignalingStateChanged {
            generation: self.generation,
            state,
        });
    }

    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        self.publish(PeerDriverEvent::LocalIceCandidate {
            generation: self.generation,
            candidate: event.candidate,
        });
    }

    async fn on_ice_candidate_error(&self, event: RTCPeerConnectionIceErrorEvent) {
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation = self.generation,
            error_text = %event.error_text,
            "RTC ICE candidate error"
        );
        self.publish(PeerDriverEvent::IceCandidateError {
            generation: self.generation,
            error_text: event.error_text,
        });
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn UpstreamDataChannel>) {
        let data_channel: Arc<dyn LaneDataChannel> =
            Arc::new(RealLaneDataChannel::new(data_channel));
        self.handle_data_channel(data_channel).await;
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let Ok(resource_id) = self.resources.try_insert_remote_track(track.clone()) else {
            self.event_tx.mark_overflow(self.generation);
            return;
        };
        let generation = self.generation;
        let event_tx = self.event_tx.clone();
        self.resources.spawn_normalization(async move {
            let descriptor = TrackDescriptor {
                meta: super::media::TrackMeta::resolve(&track).await,
            };
            event_tx.try_publish(PeerDriverEvent::TrackDiscovered {
                generation,
                resource_id,
                descriptor,
            });
        });
    }

    async fn on_negotiation_needed(&self) {
        self.publish(PeerDriverEvent::NegotiationNeeded {
            generation: self.generation,
        });
    }
}
