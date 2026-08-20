//! Media track ownership and dispatch for the v0.20 RTC architecture
//! (RFC Stage 6 item 1: "Use `on_track()`, `TrackRemote::poll()` and
//! `TrackLocal::poll()` to implement unique poll ownership").
//!
//! A remote track arrives at the controller as a `TrackAdded` driver event;
//! the owning [`super::peer::session::PeerConnectionSession`] stores it in its
//! resource registry and spawns one [`track_pump`] per track — the track's
//! single `poll()` consumer (mirroring the lane pump's single-consumer rule
//! for DataChannels). The pump converts received RTP packets into
//! `MediaSample`s and dispatches them to the node-level `MediaFrameRegistry`
//! — the same business-facing surface the 0.14 `on_track` reader loop fed —
//! so media consumers see no difference between the stacks.
//!
//! Tracks are generation-bound session resources: a Hard Recovery rebuilds
//! the PeerConnection and its tracks; the old generation's pumps are cancelled
//! and its tracks stopped by `Session::close()` (RFC: "Unrebound objects
//! cannot be reused across Hard Recovery").

use std::sync::Arc;

use actr_protocol::ActrId;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};

use crate::inbound::MediaFrameRegistry;

// ---- Outbound media (S6-10) ------------------------------------------------

use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU16, Ordering};

use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_local::{TrackLocal, TrackLocalEvent};

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9,
};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};

/// Immutable per-track metadata resolved once at registration: the registry
/// key (`track_id`) and the labels stamped on every dispatched sample.
#[derive(Debug, Clone)]
pub(crate) struct TrackMeta {
    pub(crate) track_id: String,
    /// Uppercase codec name derived from the negotiated MIME type
    /// (`audio/opus` → `OPUS`; 0.14 parity), `UNKNOWN` when unresolvable.
    pub(crate) codec: String,
    pub(crate) media_type: actr_framework::MediaType,
}

impl TrackMeta {
    /// Read the track's id/kind/codec once. The codec's MIME type carries both
    /// labels: the `audio/`/`video/` prefix maps to `MediaType`, the suffix
    /// (uppercased) names the codec — so no `rtc` enum types need naming here.
    pub(crate) async fn resolve(track: &Arc<dyn TrackRemote>) -> Self {
        let track_id = track.track_id().await;
        let (codec, media_type) = match track.ssrcs().await.into_iter().next() {
            Some(ssrc) => match track.codec(ssrc).await {
                Some(codec) => labels_from_mime_type(&codec.mime_type),
                None => ("UNKNOWN".to_string(), actr_framework::MediaType::Video),
            },
            None => ("UNKNOWN".to_string(), actr_framework::MediaType::Video),
        };
        Self {
            track_id,
            codec,
            media_type,
        }
    }
}

/// Split a codec MIME type into (uppercase codec name, media type), mirroring
/// the 0.14 derivation (`video/VP8` → (`VP8`, Video)). Anything without an
/// `audio/` prefix is treated as video — the 0.14 fallback behaved the same.
fn labels_from_mime_type(mime_type: &str) -> (String, actr_framework::MediaType) {
    let media_type = if mime_type.starts_with("audio/") {
        actr_framework::MediaType::Audio
    } else {
        actr_framework::MediaType::Video
    };
    let codec = mime_type
        .rsplit('/')
        .next()
        .unwrap_or("unknown")
        .to_uppercase();
    (codec, media_type)
}

/// Poll a remote track as its single `poll()` consumer, dispatching received
/// RTP payloads as `MediaSample`s to the node media registry (0.14 parity).
/// Exits on `OnEnding`/`OnEnded` or when the track's event stream closes.
///
/// The pump never writes control state: track lifecycle is observed here, and
/// readiness/recovery decisions stay with the controller's reducer.
pub(crate) async fn track_pump(
    peer_id: ActrId,
    generation: u64,
    track: Arc<dyn TrackRemote>,
    meta: TrackMeta,
    media_registry: Option<Arc<MediaFrameRegistry>>,
) {
    while let Some(event) = track.poll().await {
        match event {
            TrackRemoteEvent::OnRtpPacket(packet) => {
                let Some(registry) = &media_registry else {
                    continue;
                };
                let sample = actr_framework::MediaSample {
                    data: packet.payload,
                    timestamp: packet.header.timestamp,
                    codec: meta.codec.clone(),
                    media_type: meta.media_type,
                };
                registry
                    .dispatch(&meta.track_id, sample, peer_id.clone())
                    .await;
            }
            TrackRemoteEvent::OnEnding | TrackRemoteEvent::OnEnded => {
                tracing::debug!(
                    peer = %peer_id.to_string_repr(),
                    generation,
                    track_id = %meta.track_id,
                    "remote track ended; pump exiting"
                );
                return;
            }
            // OnOpen/OnError/OnMute/OnUnmute/OnRtcpPacket: observed lifecycle —
            // no business dispatch (0.14 dispatched RTP payloads only).
            _ => {}
        }
    }
}

/// An outbound media writer for one local track (S6-10), mirroring the 0.14
/// `WebRtcConnection::add_media_track` + `send_media_sample` bookkeeping: one
/// `TrackLocalStaticRTP` per `(track_id)`, a per-track incrementing sequence
/// number, and a random SSRC fixed at creation.
///
/// The writer is the data plane: after the controller's `add_track` registers
/// the track with the active session (and the renegotiation rides the
/// existing `NegotiationNeeded` path), business samples flow straight through
/// `write_sample` — they never re-enter the controller.
pub(crate) struct OutboundTrackWriter {
    track: Arc<TrackLocalStaticRTP>,
    /// The RTP sender returned by `add_track` (set once; needed by
    /// `remove_track`).
    sender: StdMutex<Option<Arc<dyn webrtc::rtp_transceiver::RtpSender>>>,
    sequence_number: AtomicU16,
    ssrc: u32,
    payload_type: u8,
}

impl OutboundTrackWriter {
    /// Create a writer and its local track. Codecs mirror the 0.14 set
    /// (VP8/H264/VP9 video, OPUS audio). The payload type mirrors the 0.20
    /// default codec registration (`register_default_codecs`, applied by the
    /// factory to every PC — S6-7b): VP8=96, VP9=98, H264=102, OPUS=111.
    pub(crate) fn new(track_id: &str, codec: &str, media_type: &str) -> Result<Self, String> {
        let (mime_type, kind, clock_rate, channels, payload_type) =
            match (media_type, codec.to_uppercase().as_str()) {
                ("video", "H264") => (MIME_TYPE_H264, RtpCodecKind::Video, 90000, 0, 102),
                ("video", "VP8") => (MIME_TYPE_VP8, RtpCodecKind::Video, 90000, 0, 96),
                ("video", "VP9") => (MIME_TYPE_VP9, RtpCodecKind::Video, 90000, 0, 98),
                ("audio", "OPUS") => (MIME_TYPE_OPUS, RtpCodecKind::Audio, 48000, 2, 111),
                _ => {
                    return Err(format!("Unsupported codec: {codec} for {media_type}"));
                }
            };

        let ssrc = rand::random::<u32>();
        let track = MediaStreamTrack::new(
            format!("actr-{media_type}"),
            track_id.to_string(),
            track_id.to_string(),
            kind,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: RTCRtpCodec {
                    mime_type: mime_type.to_string(),
                    clock_rate,
                    channels,
                    sdp_fmtp_line: String::new(),
                    rtcp_feedback: vec![],
                },
                ..Default::default()
            }],
        );

        Ok(Self {
            track: Arc::new(TrackLocalStaticRTP::new(track)),
            sender: StdMutex::new(None),
            sequence_number: AtomicU16::new(0),
            ssrc,
            payload_type,
        })
    }

    /// The local track as the controller's `add_track` consumes it (the same
    /// Arc — the writer and the session share one track object).
    pub(crate) fn track(&self) -> Arc<dyn TrackLocal> {
        self.track.clone()
    }

    /// Record the RTP sender returned by `add_track` (for `remove_track`).
    pub(crate) fn set_sender(&self, sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>) {
        *self.sender.lock().expect("writer sender lock") = Some(sender);
    }

    /// The recorded sender, if `add_track` completed.
    pub(crate) fn sender(&self) -> Option<Arc<dyn webrtc::rtp_transceiver::RtpSender>> {
        self.sender.lock().expect("writer sender lock").clone()
    }

    /// Write one business media sample as an RTP packet (0.14 parity:
    /// marker set, per-track sequence, fixed SSRC, timestamp from the sample).
    pub(crate) async fn write_sample(
        &self,
        sample: &actr_framework::MediaSample,
    ) -> Result<(), webrtc::error::Error> {
        let sequence_number = self.sequence_number.fetch_add(1, Ordering::Relaxed);
        let packet = rtc::rtp::Packet {
            header: rtc::rtp::Header {
                version: 2,
                padding: false,
                extension: false,
                marker: true,
                payload_type: self.payload_type,
                sequence_number,
                timestamp: sample.timestamp,
                ssrc: self.ssrc,
                ..Default::default()
            },
            payload: sample.data.clone(),
        };
        self.track.write_rtp(packet).await
    }
}

/// Poll a local track as its single `poll()` consumer (S6-10: the outbound
/// mirror of the S6-1 track pump). Local-track events are RTCP feedback from
/// the receiver — observed and logged; no business dispatch (0.14 did none
/// either). Session-owned via the JoinSet; cancelled on close.
pub(crate) async fn local_track_pump(
    peer_id: ActrId,
    generation: u64,
    track_id: String,
    track: Arc<dyn TrackLocal>,
) {
    while let Some(event) = track.poll().await {
        match event {
            TrackLocalEvent::OnRtcpPacket(packets) => {
                tracing::debug!(
                    peer = %peer_id.to_string_repr(),
                    generation,
                    %track_id,
                    rtcp_packets = packets.len(),
                    "local track RTCP feedback"
                );
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! A scripted `TrackRemote` fake: replays a fixed event sequence through
    //! `poll()`, then ends. Metadata methods return fixed values.

    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    use rtc::media_stream::{
        MediaStreamId, MediaStreamTrackId, MediaStreamTrackState, MediaTrackCapabilities,
        MediaTrackConstraints, MediaTrackSettings,
    };
    use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
    use rtc::rtp_transceiver::{RtpStreamId, SSRC};
    use webrtc::media_stream::Track;

    pub(crate) struct ScriptedTrackRemote {
        track_id: String,
        codec: RTCRtpCodec,
        kind: RtpCodecKind,
        ssrcs: Vec<SSRC>,
        events: StdMutex<std::collections::VecDeque<TrackRemoteEvent>>,
        pub(crate) stop_calls: AtomicUsize,
    }

    impl ScriptedTrackRemote {
        pub(crate) fn new(track_id: &str, mime_type: &str, events: Vec<TrackRemoteEvent>) -> Self {
            let kind = if mime_type.starts_with("audio/") {
                RtpCodecKind::Audio
            } else {
                RtpCodecKind::Video
            };
            Self {
                track_id: track_id.to_string(),
                codec: RTCRtpCodec {
                    mime_type: mime_type.to_string(),
                    ..Default::default()
                },
                kind,
                ssrcs: vec![42],
                events: StdMutex::new(events.into()),
                stop_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Track for ScriptedTrackRemote {
        async fn stream_id(&self) -> MediaStreamId {
            "stream".to_string()
        }
        async fn track_id(&self) -> MediaStreamTrackId {
            self.track_id.clone()
        }
        async fn label(&self) -> String {
            self.track_id.clone()
        }
        async fn kind(&self) -> RtpCodecKind {
            self.kind
        }
        async fn rid(&self, _ssrc: SSRC) -> Option<RtpStreamId> {
            None
        }
        async fn codec(&self, _ssrc: SSRC) -> Option<RTCRtpCodec> {
            Some(self.codec.clone())
        }
        async fn ssrcs(&self) -> Vec<SSRC> {
            self.ssrcs.clone()
        }
        async fn enabled(&self) -> bool {
            true
        }
        async fn set_enabled(&self, _enabled: bool) {}
        async fn muted(&self) -> bool {
            false
        }
        async fn set_muted(&self, _muted: bool) {}
        async fn ready_state(&self) -> MediaStreamTrackState {
            MediaStreamTrackState::Live
        }
        async fn stop(&self) {
            self.stop_calls.fetch_add(1, Ordering::SeqCst);
        }
        async fn get_capabilities(&self) -> MediaTrackCapabilities {
            MediaTrackCapabilities::default()
        }
        async fn get_constraints(&self) -> MediaTrackConstraints {
            MediaTrackConstraints::default()
        }
        async fn get_settings(&self) -> MediaTrackSettings {
            MediaTrackSettings::default()
        }
        async fn apply_constraints(&self, _constraints: Option<MediaTrackConstraints>) {}
        async fn codings(&self) -> Vec<RTCRtpEncodingParameters> {
            Vec::new()
        }
        async fn add_coding(&self, _coding: RTCRtpEncodingParameters) {}
    }

    #[async_trait::async_trait]
    impl TrackRemote for ScriptedTrackRemote {
        async fn write_rtcp(
            &self,
            _packets: Vec<Box<dyn rtc::rtcp::Packet>>,
        ) -> webrtc::error::Result<()> {
            Ok(())
        }

        async fn poll(&self) -> Option<TrackRemoteEvent> {
            self.events.lock().expect("scripted track lock").pop_front()
        }
    }

    pub(crate) fn scripted_track(
        track_id: &str,
        mime_type: &str,
        events: Vec<TrackRemoteEvent>,
    ) -> Arc<dyn TrackRemote> {
        Arc::new(ScriptedTrackRemote::new(track_id, mime_type, events))
    }

    /// A minimal `TrackLocal` fake (S6-2 tests): inert bind/write/poll.
    pub(crate) struct FakeTrackLocal {
        track_id: String,
    }

    impl FakeTrackLocal {
        pub(crate) fn new(track_id: &str) -> Self {
            Self {
                track_id: track_id.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Track for FakeTrackLocal {
        async fn stream_id(&self) -> MediaStreamId {
            "stream".to_string()
        }
        async fn track_id(&self) -> MediaStreamTrackId {
            self.track_id.clone()
        }
        async fn label(&self) -> String {
            self.track_id.clone()
        }
        async fn kind(&self) -> RtpCodecKind {
            RtpCodecKind::Video
        }
        async fn rid(&self, _ssrc: SSRC) -> Option<RtpStreamId> {
            None
        }
        async fn codec(&self, _ssrc: SSRC) -> Option<RTCRtpCodec> {
            Some(RTCRtpCodec {
                mime_type: "video/VP8".to_string(),
                ..Default::default()
            })
        }
        async fn ssrcs(&self) -> Vec<SSRC> {
            vec![7]
        }
        async fn enabled(&self) -> bool {
            true
        }
        async fn set_enabled(&self, _enabled: bool) {}
        async fn muted(&self) -> bool {
            false
        }
        async fn set_muted(&self, _muted: bool) {}
        async fn ready_state(&self) -> MediaStreamTrackState {
            MediaStreamTrackState::Live
        }
        async fn stop(&self) {}
        async fn get_capabilities(&self) -> MediaTrackCapabilities {
            MediaTrackCapabilities::default()
        }
        async fn get_constraints(&self) -> MediaTrackConstraints {
            MediaTrackConstraints::default()
        }
        async fn get_settings(&self) -> MediaTrackSettings {
            MediaTrackSettings::default()
        }
        async fn apply_constraints(&self, _constraints: Option<MediaTrackConstraints>) {}
        async fn codings(&self) -> Vec<RTCRtpEncodingParameters> {
            Vec::new()
        }
        async fn add_coding(&self, _coding: RTCRtpEncodingParameters) {}
    }

    #[async_trait::async_trait]
    impl webrtc::media_stream::track_local::TrackLocal for FakeTrackLocal {
        async fn track(&self) -> rtc::media_stream::MediaStreamTrack {
            rtc::media_stream::MediaStreamTrack::new(
                "stream".to_string(),
                self.track_id.clone(),
                self.track_id.clone(),
                RtpCodecKind::Video,
                Vec::new(),
            )
        }
        async fn bind(
            &self,
            _ctx: webrtc::media_stream::track_local::TrackLocalContext,
            _evt_rx: webrtc::runtime::Receiver<webrtc::media_stream::track_local::TrackLocalEvent>,
        ) {
        }
        async fn unbind(&self) {}
        async fn write_rtp(&self, _packet: rtc::rtp::Packet) -> webrtc::error::Result<()> {
            Ok(())
        }
        async fn write_rtcp(
            &self,
            _packets: Vec<Box<dyn rtc::rtcp::Packet>>,
        ) -> webrtc::error::Result<()> {
            Ok(())
        }
        async fn poll(&self) -> Option<webrtc::media_stream::track_local::TrackLocalEvent> {
            None
        }
    }

    /// A minimal `RtpSender` fake (S6-2 tests): fixed id, holds a local track.
    pub(crate) struct FakeRtpSender {
        id: rtc::rtp_transceiver::RTCRtpSenderId,
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    }

    impl FakeRtpSender {
        pub(crate) fn new(id: usize) -> Self {
            Self {
                id: rtc::rtp_transceiver::RTCRtpSenderId::from(id),
                track: Arc::new(FakeTrackLocal::new("local-track")),
            }
        }
    }

    #[async_trait::async_trait]
    impl webrtc::rtp_transceiver::RtpSender for FakeRtpSender {
        fn id(&self) -> rtc::rtp_transceiver::RTCRtpSenderId {
            self.id
        }
        fn track(&self) -> &Arc<dyn webrtc::media_stream::track_local::TrackLocal> {
            &self.track
        }
        async fn get_capabilities(
            &self,
            _kind: RtpCodecKind,
        ) -> webrtc::error::Result<Option<rtc::rtp_transceiver::rtp_sender::RTCRtpCapabilities>>
        {
            Ok(None)
        }
        async fn set_parameters(
            &self,
            _parameters: rtc::rtp_transceiver::rtp_sender::RTCRtpSendParameters,
            _set_parameter_options: Option<
                rtc::rtp_transceiver::rtp_sender::RTCSetParameterOptions,
            >,
        ) -> webrtc::error::Result<()> {
            Ok(())
        }
        async fn get_parameters(
            &self,
        ) -> webrtc::error::Result<rtc::rtp_transceiver::rtp_sender::RTCRtpSendParameters> {
            Ok(Default::default())
        }
        async fn replace_track(
            &self,
            _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
        ) -> webrtc::error::Result<()> {
            Ok(())
        }
        async fn set_streams(&self, _streams: Vec<MediaStreamId>) -> webrtc::error::Result<()> {
            Ok(())
        }
        async fn get_stats(
            &self,
            _now: std::time::Instant,
        ) -> webrtc::error::Result<webrtc::peer_connection::RTCStatsReport> {
            Ok(Default::default())
        }
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
