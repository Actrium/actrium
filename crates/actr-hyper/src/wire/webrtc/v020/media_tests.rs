//! Tests for the Stage 6 track pump: unique poll ownership, MediaSample
//! dispatch to the node registry (0.14 parity), and lifecycle exit.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use actr_protocol::ActrId;
use webrtc::media_stream::track_remote::TrackRemoteEvent;

use crate::inbound::MediaFrameRegistry;
use crate::wire::webrtc::v020::media::test_support::{ScriptedTrackRemote, scripted_track};
use crate::wire::webrtc::v020::media::{TrackMeta, track_pump};

const WAIT: Duration = Duration::from_secs(5);

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: "test@example.com".to_string(),
        serial_number,
    }
}

fn rtp_packet(payload: &'static [u8], timestamp: u32) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            timestamp,
            ..Default::default()
        },
        payload: bytes::Bytes::from_static(payload),
    }
}

#[tokio::test]
async fn rtp_packets_dispatch_as_media_samples_with_014_labels() {
    let track = scripted_track(
        "video-1",
        "video/VP8",
        vec![
            TrackRemoteEvent::OnRtpPacket(rtp_packet(b"frame-one", 1000)),
            TrackRemoteEvent::OnRtpPacket(rtp_packet(b"frame-two", 2000)),
            TrackRemoteEvent::OnEnded,
        ],
    );
    let meta = TrackMeta::resolve(&track).await;
    assert_eq!(meta.track_id, "video-1");
    assert_eq!(meta.codec, "VP8");
    assert_eq!(meta.media_type, actr_framework::MediaType::Video);

    let registry = Arc::new(MediaFrameRegistry::new());
    let hits = Arc::new(AtomicUsize::new(0));
    let samples = Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let hits = hits.clone();
        let samples = samples.clone();
        registry.register(
            "video-1".to_string(),
            Arc::new(move |sample, sender| {
                let hits = hits.clone();
                let samples = samples.clone();
                Box::pin(async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    samples.lock().unwrap().push((sample, sender));
                    Ok(())
                })
            }),
        );
    }

    track_pump(test_actor_id(7), 1, track, meta, Some(registry.clone())).await;

    // The dispatch spawns per-sample tasks; wait for both callbacks.
    tokio::time::timeout(WAIT, async {
        while hits.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both samples dispatched");

    let got = samples.lock().unwrap().clone();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0.data.as_ref(), b"frame-one");
    assert_eq!(got[0].0.timestamp, 1000);
    assert_eq!(got[0].0.codec, "VP8");
    assert_eq!(got[0].0.media_type, actr_framework::MediaType::Video);
    assert_eq!(got[0].1, test_actor_id(7));
    assert_eq!(got[1].0.data.as_ref(), b"frame-two");
}

#[tokio::test]
async fn audio_mime_maps_to_audio_media_type() {
    let track = scripted_track("audio-1", "audio/opus", vec![TrackRemoteEvent::OnEnded]);
    let meta = TrackMeta::resolve(&track).await;
    assert_eq!(meta.codec, "OPUS");
    assert_eq!(meta.media_type, actr_framework::MediaType::Audio);
}

#[tokio::test]
async fn pump_without_registry_drops_frames_quietly() {
    let track = scripted_track(
        "video-9",
        "video/H264",
        vec![
            TrackRemoteEvent::OnRtpPacket(rtp_packet(b"x", 1)),
            TrackRemoteEvent::OnEnded,
        ],
    );
    let meta = TrackMeta::resolve(&track).await;
    // No registry: the pump consumes events and exits without dispatch.
    crate::timer::timeout(
        crate::timer::ids::WEBRTC_PEER_SENDABLE,
        WAIT,
        track_pump(test_actor_id(3), 1, track, meta, None),
    )
    .await
    .expect("pump should exit after the scripted events");
}

#[tokio::test]
async fn poll_stream_end_exits_the_pump() {
    // Empty script: poll() returns None immediately.
    let track = scripted_track("video-0", "video/VP9", vec![]);
    let meta = TrackMeta::resolve(&track).await;
    crate::timer::timeout(
        crate::timer::ids::WEBRTC_PEER_SENDABLE,
        WAIT,
        track_pump(test_actor_id(4), 1, track, meta, None),
    )
    .await
    .expect("pump must exit when the event stream closes");
}

#[tokio::test]
async fn session_close_stops_owned_tracks() {
    use crate::wire::webrtc::v020::peer::effects::test_support::NoopSessionOps;
    use crate::wire::webrtc::v020::peer::session::PeerConnectionSession;

    let mut session = PeerConnectionSession::new(1, Arc::new(NoopSessionOps));
    let track = Arc::new(ScriptedTrackRemote::new("video-77", "video/VP8", vec![]));
    let track_id = session
        .register_track(track.clone(), &test_actor_id(5), None)
        .await;
    assert_eq!(track_id, "video-77");
    assert_eq!(track.stop_calls.load(Ordering::SeqCst), 0);

    session.close().await.expect("close");
    assert_eq!(
        track.stop_calls.load(Ordering::SeqCst),
        1,
        "session close must stop each owned track (RFC: Track cancellation)"
    );
}
