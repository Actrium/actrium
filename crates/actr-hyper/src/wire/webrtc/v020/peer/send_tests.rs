//! Send-path tests (Stage 4D): `SendPermit` + lazy lane creation + stable send
//! errors over a controller driven to `Sendable` with recording data channels.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use actr_protocol::{ActrId, ActrType, Realm};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::timeout;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidateInit, RTCPeerConnectionState, RTCSdpType,
    RTCSessionDescription,
};

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::v020::events::PeerDriverEvent;
use crate::wire::webrtc::v020::inbound::InboundRouter;
use crate::wire::webrtc::v020::peer::controller::RtcPeerController;
use crate::wire::webrtc::v020::peer::effects::test_support::{
    RecordingDataChannel, recording_data_channel,
};
use crate::wire::webrtc::v020::peer::effects::{SessionFactory, SessionOps};
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;
use crate::wire::webrtc::v020::peer::send::RtcSendError;
use crate::wire::webrtc::v020::signaling::SignalingDispatch;
use crate::wire::webrtc::v020::status::LaneKind;

const WAIT: Duration = Duration::from_secs(5);

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number,
        r#type: ActrType {
            manufacturer: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

fn desc(sdp_type: RTCSdpType) -> RTCSessionDescription {
    let mut d = RTCSessionDescription::default();
    d.sdp_type = sdp_type;
    d.sdp = format!("fake-{sdp_type:?}");
    d
}

/// Session ops whose data channels are recording channels, shared with the test.
struct RecordingSessionOps {
    channels: Arc<StdMutex<HashMap<String, Arc<RecordingDataChannel>>>>,
}

#[async_trait]
impl SessionOps for RecordingSessionOps {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Offer))
    }
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Answer))
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
        label: &str,
        _init: webrtc::data_channel::RTCDataChannelInit,
    ) -> Result<Arc<dyn webrtc::data_channel::DataChannel>, RtcError> {
        let channel = recording_data_channel(label);
        self.channels
            .lock()
            .unwrap()
            .insert(label.to_string(), channel.clone());
        Ok(channel)
    }
    async fn add_track(
        &self,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, RtcError> {
        Ok(Arc::new(
            crate::wire::webrtc::v020::media::test_support::FakeRtpSender::new(1),
        ))
    }
    async fn remove_track(
        &self,
        _sender: &Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Result<(), RtcError> {
        Ok(())
    }
    async fn get_senders(&self) -> Vec<Arc<dyn webrtc::rtp_transceiver::RtpSender>> {
        Vec::new()
    }
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
        Default::default()
    }
    async fn close(&self) -> Result<(), RtcError> {
        Ok(())
    }
}

/// A factory that immediately builds a session of recording channels, capturing
/// the controller's event mailbox from the build handler so the test can drive
/// peer-connection state.
struct RecordingFactory {
    channels: Arc<StdMutex<HashMap<String, Arc<RecordingDataChannel>>>>,
    mailbox: Arc<StdMutex<Option<crate::wire::webrtc::v020::events::PeerEventMailbox>>>,
}

#[async_trait]
impl SessionFactory for RecordingFactory {
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        *self.mailbox.lock().unwrap() = Some(handler.mailbox());
        Ok(Arc::new(RecordingSessionOps {
            channels: self.channels.clone(),
        }))
    }
}

struct SendHarness {
    handle: crate::wire::webrtc::v020::peer::handle::RtcPeerControllerHandle,
    channels: Arc<StdMutex<HashMap<String, Arc<RecordingDataChannel>>>>,
    signaling_rx: mpsc::Receiver<SignalingDispatch>,
    mailbox: Arc<StdMutex<Option<crate::wire::webrtc::v020::events::PeerEventMailbox>>>,
    /// Kept alive so the broadcast sender is not dropped (which would close
    /// the controller's network event receiver and exit its event loop).
    _supervisor: crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor,
    _task: tokio::task::JoinHandle<()>,
}

fn spawn_send_harness(serial: u64) -> SendHarness {
    let (signaling_tx, signaling_rx) = mpsc::channel(16);
    let channels = Arc::new(StdMutex::new(HashMap::new()));
    let mailbox = Arc::new(StdMutex::new(None));
    let factory = Arc::new(RecordingFactory {
        channels: channels.clone(),
        mailbox: mailbox.clone(),
    });
    let inbound = std::sync::Arc::new(InboundRouter::discard());
    let supervisor = crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor::new();
    supervisor.set_signaling_generation(Some(0));
    let (handle, _lane_tx, task) = RtcPeerController::spawn(
        test_actor_id(serial),
        factory,
        signaling_tx,
        supervisor.subscribe(),
        supervisor.subscribe_signaling_generation(),
        crate::wire::webrtc::v020::peer::controller::SharedNodeDeps {
            inbound,
            media_registry: None,
            buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig::default(),
            #[cfg(feature = "test-utils")]
            driver_event_observer: None,
            signaling_dispatch_observer: None,
            controller_lifecycle_id: 1,
            peer_generation_invalidated: None,
            peer_sendability_changed: None,
            peer_transport_retired: None,
            peer_snapshot_events: None,
            recovery_obligations: None,
        },
    );
    SendHarness {
        handle,
        channels,
        signaling_rx,
        mailbox,
        _supervisor: supervisor,
        _task: task,
    }
}

fn channel_of(h: &SendHarness, label: &str) -> Arc<RecordingDataChannel> {
    let guard = h.channels.lock().unwrap();
    guard
        .get(label)
        .unwrap_or_else(|| panic!("channel {label} should exist"))
        .clone()
}

/// Drive the harness to Sendable: role-assign offerer (build completes at once),
/// apply the answer, mark the PC connected. The RpcReliable recording channel's
/// pump reports OnOpen on its own, reaching sendable.
async fn drive_to_sendable(h: &mut SendHarness) {
    h.handle.connect().await.expect("send connect");
    // Server assigns the offerer role (the RoleNegotiation is on signaling_rx).
    let dispatch = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("role negotiation")
        .expect("signaling closed");
    assert!(matches!(
        dispatch.message,
        SignalingMessage::RoleNegotiation
    ));
    h.handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer");

    // Apply the remote answer, then mark the transport connected via the
    // controller's own event mailbox (captured from the build handler). Wait for
    // the build to complete so the mailbox is captured.
    h.handle
        .apply_remote_description(desc(RTCSdpType::Answer))
        .await
        .expect("apply answer");
    let mailbox = timeout(WAIT, async {
        loop {
            if let Some(m) = h.mailbox.lock().unwrap().clone() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("controller event mailbox should be captured after build");
    mailbox.try_publish(PeerDriverEvent::PeerConnectionStateChanged {
        generation: 1,
        state: RTCPeerConnectionState::Connected,
    });

    // Wait for sendable (RpcReliable recording channel opens via its pump).
    timeout(WAIT, async {
        loop {
            let snap = h.handle.snapshot();
            if snap.sendable {
                return snap;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("peer did not become sendable");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_on_rpc_reliable_writes_to_the_channel() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    h.handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"hello"))
        .await
        .expect("reliable send should succeed when sendable");

    let channel = channel_of(&h, "RPC_RELIABLE");
    let sent = channel.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    // The handle prepends an 8-byte fragment header (wire-compatible with 0.14).
    let (_, _, _, payload) =
        crate::transport::decode_fragment_header(sent[0].clone().freeze()).unwrap();
    assert_eq!(payload.as_ref(), b"hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_before_sendable_is_connection_not_ready() {
    let h = spawn_send_harness(1);
    // Do not connect: the peer is Idle (not sendable).
    let err = h
        .handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"x"))
        .await
        .expect_err("send before sendable must fail");
    assert_eq!(err, RtcSendError::ConnectionNotReady);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn try_send_on_latency_first_lazily_creates_the_lane() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    // The latency-first lane does not exist yet; try_send must lazily create it
    // (EnsureLane) and then write through.
    h.handle
        .try_send(
            LaneKind::StreamLatencyFirst,
            bytes::Bytes::from_static(b"fast"),
        )
        .await
        .expect("latency-first try_send should lazily create the lane and succeed");

    let channel = channel_of(&h, "STREAM_LATENCY_FIRST");
    let try_sent = channel.try_sent.lock().unwrap();
    assert_eq!(try_sent.len(), 1);
    // The handle prepends an 8-byte fragment header (wire-compatible with 0.14).
    let (_, _, _, payload) =
        crate::transport::decode_fragment_header(try_sent[0].clone().freeze()).unwrap();
    assert_eq!(payload.as_ref(), b"fast");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn try_send_on_full_buffer_is_buffer_full_error() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    let channel = channel_of(&h, "RPC_RELIABLE");
    channel.fail_try_send.store(true, Ordering::SeqCst);

    let err = h
        .handle
        .try_send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"x"))
        .await
        .expect_err("try_send on a full buffer must fail fast");
    assert_eq!(err, RtcSendError::DataChannelBufferFull);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_after_close_is_connection_not_ready() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    h.handle.close().await.expect("send close");
    // Wait for the terminal close to publish not-sendable.
    timeout(WAIT, async {
        loop {
            if !h.handle.snapshot().sendable {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("peer did not become not-sendable after close");

    let err = h
        .handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"x"))
        .await
        .expect_err("send after close must fail");
    assert_eq!(err, RtcSendError::ConnectionNotReady);
}

// ── Stage 4E: send path, uncertain delivery, explicit close ──────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_writes_through() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;
    h.handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"ok"))
        .await
        .expect("send on an open lane should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_above_shared_message_limit_fails_message_too_large() {
    // The receiver's reassembly caps a message at
    // `max_inbound_message_bytes` (default 8 MiB) and silently drops
    // anything larger; the send path must reject such a payload up front
    // with the non-retryable MessageTooLarge instead of reporting success.
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;
    let oversized = vec![
        0u8;
        crate::wire::webrtc::v020::config::RtcBufferConfig::default()
            .max_inbound_message_bytes
            + 1
    ];
    let err = h
        .handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from(oversized))
        .await
        .expect_err("payload above the shared message limit must fail");
    assert_eq!(err, RtcSendError::MessageTooLarge);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_with_outstanding_bytes_reports_delivery_uncertain() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    // The channel holds outstanding (unconfirmed) bytes, and send now fails as
    // not-ready: the delivery of those bytes is uncertain.
    let channel = channel_of(&h, "RPC_RELIABLE");
    channel
        .outstanding
        .store(512, std::sync::atomic::Ordering::SeqCst);
    channel
        .fail_send
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let err = h
        .handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"x"))
        .await
        .expect_err("close-with-outstanding must report uncertain delivery");
    assert_eq!(err, RtcSendError::DataChunkDeliveryUncertain);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_with_no_outstanding_is_terminal_channel_closed() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    let channel = channel_of(&h, "RPC_RELIABLE");
    channel
        .fail_send
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let err = h
        .handle
        .send(LaneKind::RpcReliable, bytes::Bytes::from_static(b"x"))
        .await
        .expect_err("close with no outstanding must retire the stale transport");
    assert_eq!(err, RtcSendError::DataChannelClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_close_closes_each_data_channel_before_the_pc() {
    let mut h = spawn_send_harness(1);
    drive_to_sendable(&mut h).await;

    let channel = channel_of(&h, "RPC_RELIABLE");
    assert_eq!(
        channel
            .close_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "channel not closed before explicit close"
    );

    h.handle.close().await.expect("send close");
    timeout(WAIT, async {
        loop {
            if !h.handle.snapshot().sendable {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("peer did not converge after close");

    assert!(
        channel
            .close_calls
            .load(std::sync::atomic::Ordering::SeqCst)
            >= 1,
        "explicit close must call DataChannel::close() on the lane's channels"
    );
}
