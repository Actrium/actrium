//! WebRTC P2P Connection implementation

use crate::transport::lane::classify_peer_connection_error;
use crate::transport::session::ConnectionSession;
use crate::transport::{
    ConnType, DataChannelStateSignal, DataLane, NetworkError, NetworkResult, SecureCell,
    SecureChannelState, SecureDataLane, WebRtcDataLane, WireHandle,
};
use crate::transport::{ConnectionEvent, ConnectionState};
use crate::wire::webrtc::WebRtcInboundMessage;
use crate::wire::webrtc::adapter::{
    RTCDataChannel, RTCDataChannelInit, RTCDataChannelMessage, RTCDataChannelState,
    RTCPeerConnection, RTCPeerConnectionState,
};
use actr_protocol::prost::Message;
use actr_protocol::{ActrId, PayloadType};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, broadcast, mpsc};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::rtp_transceiver::RtpSender;

/// Type alias for media track storage (track_id → (Track, Sender))
type MediaTracks = Arc<RwLock<HashMap<String, (Arc<TrackLocalStaticRTP>, Arc<dyn RtpSender>)>>>;

/// Type alias for lane cache array (PayloadType index → cached DataLane)
type LaneCache<const N: usize> = Arc<RwLock<[Option<Arc<dyn DataLane>>; N]>>;

/// Test-seam slot pausing one received-channel registration before its
/// first cache commit (payload type + rendezvous barrier).
#[cfg(any(test, feature = "test-utils"))]
type RegistrationCommitBarrier =
    Arc<std::sync::Mutex<Option<(PayloadType, Arc<tokio::sync::Barrier>)>>>;

/// Bound on `RTCPeerConnection::close`. rtc 0.20's close takes the driver
/// handle and polls it for up to 2 s before aborting a wedged driver (so its
/// reactor task and sockets are reclaimed). Cancelling before that 2 s bound
/// drops the already-taken handle and leaves the stalled driver running, so
/// this ceiling must clear the upstream 2 s wait. The loop returns early once
/// the driver finishes, so a non-wedged shutdown pays no extra latency.
const PEER_CONNECTION_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const DATA_CHANNEL_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Shared bound on closing every cached DataChannel during connection
/// teardown. `RTCDataChannel::close` flips the state to `Closing` before its
/// first suspension point, so a timed-out close still leaves lane waiters a
/// terminal state to observe once the signals below are published.
const DATA_CHANNEL_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebRtcCloseMode {
    Graceful,
    Immediate,
}

fn should_wait_for_data_channel_drain(
    close_mode: WebRtcCloseMode,
    connection_state: RTCPeerConnectionState,
    buffered_amount: usize,
) -> bool {
    close_mode == WebRtcCloseMode::Graceful
        && buffered_amount > 0
        && connection_state == RTCPeerConnectionState::Connected
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DataChannelDrainReport {
    notified: usize,
    timed_out: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataChannelDrainOutcome {
    AlreadyDrained,
    Notified,
    TimedOut,
}

async fn wait_for_data_channel_drain(
    channel: &RTCDataChannel,
    timeout: Duration,
) -> DataChannelDrainOutcome {
    let drained = Arc::new(tokio::sync::Notify::new());
    let drained_callback = drained.clone();
    channel.set_buffered_amount_low_threshold(0).await;
    channel
        .on_buffered_amount_low(Box::new(move || {
            let drained = drained_callback.clone();
            Box::pin(async move {
                drained.notify_one();
            })
        }))
        .await;

    // Close the callback-install race: the buffer may have reached zero
    // immediately before the handler became visible.
    if channel.buffered_amount().await == 0 {
        return DataChannelDrainOutcome::AlreadyDrained;
    }

    if crate::timer::timeout(
        crate::timer::ids::DATA_CHANNEL_DRAIN,
        timeout,
        drained.notified(),
    )
    .await
    .is_err()
    {
        DataChannelDrainOutcome::TimedOut
    } else {
        DataChannelDrainOutcome::Notified
    }
}

/// WebRtcConnection - WebRTC P2P Connect
#[derive(Clone)]
pub(crate) struct WebRtcConnection {
    /// Peer ID for event identification
    peer_id: ActrId,

    /// underlying RTCPeerConnection
    peer_connection: Arc<RTCPeerConnection>,

    // TODO: useless property, remove this
    /// DataChannel Cache: PayloadType -> DataChannel (4 types use DataChannel)
    /// index reference mapping: RpcReliable(0), RpcSignal(1), StreamReliable(2), StreamLatencyFirst(3)
    data_channels: Arc<RwLock<[Option<Arc<RTCDataChannel>>; 4]>>,

    /// MediaTrack Cache: track_id -> (Track, RtpSender)
    media_tracks: MediaTracks,

    /// RTP sequence numbers per track (track_id → sequence_number)
    track_sequence_numbers: Arc<RwLock<HashMap<String, Arc<AtomicU16>>>>,

    /// RTP SSRC per track (track_id → ssrc)
    track_ssrcs: Arc<RwLock<HashMap<String, u32>>>,

    /// Lane Cache: PayloadType -> Lane (4 types use DataChannel)
    /// index mapping: RpcReliable(0), RpcSignal(1), StreamReliable(2), StreamLatencyFirst(3)
    /// MediaTrack not cached in array, uses HashMap
    lane_cache: LaneCache<4>,

    /// State signals stored by production lanes, one per PayloadType slot.
    ///
    /// `close_with_mode` publishes to these before replacing the DataChannel
    /// callbacks with no-ops, so lane waiters armed on a still-Connecting
    /// channel observe a terminal transition instead of parking until their
    /// failure timeout.
    data_channel_state_signals: Arc<RwLock<[Option<DataChannelStateSignal>; 4]>>,

    /// Test seam: pauses a received-channel registration after its handlers
    /// are installed and before the first cache commit, so tests can
    /// interleave a full teardown into the exact commit window.
    #[cfg(any(test, feature = "test-utils"))]
    registration_commit_barrier_for_test: RegistrationCommitBarrier,

    /// Event broadcaster for connection state changes
    event_tx: broadcast::Sender<ConnectionEvent>,

    /// Connection session (session_id + cancel_token + close-once)
    session: ConnectionSession,

    /// connection status (legacy, will be replaced by session.is_closed())
    connected: Arc<RwLock<bool>>,

    /// Secure-channel state owned by this physical WebRTC session (RFC-0563).
    /// Phase 2 constructs `LegacyBypass`; phase 3 drives the real handshake.
    secure: SecureCell,
}

impl std::fmt::Debug for WebRtcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebRtcConnection")
            .field("peer_id", &self.peer_id)
            .field("peer_connection", &"<RTCPeerConnection>")
            .field("data_channels", &"<[Option<Arc<RTCDataChannel>>; 4]>")
            .field("media_tracks", &"<HashMap<String, Arc<Track>>>")
            .field("connected", &self.connected)
            .finish()
    }
}

impl WebRtcConnection {
    /// Create WebRtcConnection from RTCPeerConnection
    ///
    /// # Arguments
    /// - `peer_id`: Peer identity for event identification
    /// - `peer_connection`: Arc wrapped RTCPeerConnection
    /// - `event_tx`: Broadcast sender for connection events
    pub fn new(
        peer_id: ActrId,
        peer_connection: Arc<RTCPeerConnection>,
        event_tx: broadcast::Sender<ConnectionEvent>,
    ) -> Self {
        Self {
            peer_id,
            peer_connection,
            data_channels: Arc::new(RwLock::new([None, None, None, None])),
            media_tracks: Arc::new(RwLock::new(HashMap::new())),
            track_sequence_numbers: Arc::new(RwLock::new(HashMap::new())),
            track_ssrcs: Arc::new(RwLock::new(HashMap::new())),
            lane_cache: Arc::new(RwLock::new([None, None, None, None])),
            data_channel_state_signals: Arc::new(RwLock::new([None, None, None, None])),
            #[cfg(any(test, feature = "test-utils"))]
            registration_commit_barrier_for_test: Arc::new(std::sync::Mutex::new(None)),
            event_tx,
            session: ConnectionSession::new(),
            connected: Arc::new(RwLock::new(true)),
            secure: SecureCell::new(SecureChannelState::LegacyBypass),
        }
    }

    /// Get session ID
    pub(crate) fn session_id(&self) -> u64 {
        self.session.session_id
    }

    /// Return the signal actually stored inside the production lane.
    ///
    /// Reading it back from the constructed lane, rather than retaining the
    /// constructor argument, makes callback-to-lane wiring regressions visible:
    /// a callback publishing to a different signal will not wake test waiters.
    #[cfg(feature = "test-utils")]
    pub(crate) async fn data_channel_state_signal_for_test(
        &self,
        payload_type: PayloadType,
    ) -> Option<DataChannelStateSignal> {
        self.data_channel_state_signals.read().await[payload_type as usize].clone()
    }

    /// Pause the next received-channel registration for `payload_type`
    /// immediately before its first cache commit.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn set_registration_commit_barrier_for_test(
        &self,
        payload_type: PayloadType,
        barrier: Arc<tokio::sync::Barrier>,
    ) {
        let mut slot = self
            .registration_commit_barrier_for_test
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            slot.replace((payload_type, barrier)).is_none(),
            "registration commit barrier should only be installed once"
        );
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn hit_registration_commit_barrier_for_test(&self, payload_type: PayloadType) {
        let barrier = {
            let mut slot = self
                .registration_commit_barrier_for_test
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.as_ref().is_some_and(|(pt, _)| *pt == payload_type) {
                slot.take().map(|(_, barrier)| barrier)
            } else {
                None
            }
        };
        if let Some(barrier) = barrier {
            barrier.wait().await;
            barrier.wait().await;
        }
    }

    /// Whether this connection has entered its terminal close path.
    ///
    /// `ConnectionSession::try_close` flips this flag before any asynchronous
    /// teardown, so it covers both closing and fully closed connections.
    pub(crate) fn is_closing_or_closed(&self) -> bool {
        self.session.is_closed()
    }

    /// Install a state-change handler on the underlying RTCPeerConnection.
    ///
    /// This keeps `connected` in sync with the WebRTC connection state and
    /// broadcasts state change events for upper layers to handle.
    pub(crate) async fn handle_state_change(&self, state: RTCPeerConnectionState) {
        if self.session.is_cancelled() {
            // Preserve the terminal state event for session-guarded cleanup
            // observers, but skip hooks and recursive close side effects.
            if matches!(state, RTCPeerConnectionState::Closed) {
                let _ = self.event_tx.send(ConnectionEvent::StateChanged {
                    peer_id: self.peer_id.clone(),
                    session_id: self.session.session_id,
                    state: ConnectionState::Closed,
                });
            }
            tracing::debug!(
                "🚫 handle_state_change session {} cancelled, ignoring {:?}",
                self.session.session_id,
                state
            );
            return;
        }

        // Treat New/Connecting/Connected as "connected"; others as disconnected.
        let is_connected = matches!(
            state,
            RTCPeerConnectionState::New
                | RTCPeerConnectionState::Connecting
                | RTCPeerConnectionState::Connected
        );

        // Update flag and detect transitions from connected -> disconnected.
        let was_connected = {
            let mut flag = self.connected.write().await;
            let prev = *flag;
            *flag = is_connected;
            prev
        };

        // Convert WebRTC state to our ConnectionState
        let connection_state = match state {
            RTCPeerConnectionState::New => ConnectionState::New,
            RTCPeerConnectionState::Connecting => ConnectionState::Connecting,
            RTCPeerConnectionState::Connected => ConnectionState::Connected,
            RTCPeerConnectionState::Disconnected => ConnectionState::Disconnected,
            RTCPeerConnectionState::Failed => ConnectionState::Failed,
            RTCPeerConnectionState::Closed => ConnectionState::Closed,
            _ => ConnectionState::Closed, // Unspecified maps to Closed
        };

        tracing::info!(
            "🔄 WebRtcConnection peer state changed: {:?}, connected={}",
            state,
            is_connected
        );

        // Broadcast state change event for upper layers
        let _ = self.event_tx.send(ConnectionEvent::StateChanged {
            peer_id: self.peer_id.clone(),
            session_id: self.session.session_id,
            state: connection_state.clone(),
        });

        // For Closed state, proactively close the connection and let
        // `close()` perform all resource cleanup. Only trigger when we
        // transition from connected -> disconnected to avoid loops.
        if was_connected && matches!(state, RTCPeerConnectionState::Closed) {
            tracing::info!(
                "🔻 WebRtcConnection entering terminal state {:?}, calling close()",
                state
            );

            if let Err(e) = self.close().await {
                tracing::warn!("⚠️ WebRtcConnection::close() failed: {}", e);
            }
        }
    }

    /// Mark the connection as connected.
    ///
    /// The underlying WebRTC connection has already been established via
    /// signaling; this call only records the local "connected" flag.
    pub(crate) async fn connect(&self) -> NetworkResult<()> {
        *self.connected.write().await = true;
        Ok(())
    }

    /// Return a snapshot of the current DataChannel cache.
    ///
    /// Used by the coordinator to query `buffered_amount` on abnormal disconnect.
    pub async fn data_channels(&self) -> [Option<Arc<RTCDataChannel>>; 4] {
        self.data_channels.read().await.clone()
    }

    /// Check if any DataChannel is open
    pub async fn has_open_data_channel(&self) -> bool {
        let channels = self.data_channels.read().await;
        for channel in channels.iter().flatten() {
            if channel.ready_state() == RTCDataChannelState::Open {
                return true;
            }
        }
        false
    }

    /// Drain all open DataChannel send buffers before closing (graceful shutdown).
    ///
    /// Waits on the DataChannel's buffered-amount-low event, with a five-second
    /// failure bound. The successful path has no polling interval.
    async fn drain_data_channels(&self, close_mode: WebRtcCloseMode) {
        let _ = self
            .drain_data_channels_with_timeout(close_mode, DATA_CHANNEL_DRAIN_TIMEOUT)
            .await;
    }

    async fn drain_data_channels_with_timeout(
        &self,
        close_mode: WebRtcCloseMode,
        drain_timeout: Duration,
    ) -> DataChannelDrainReport {
        let mut report = DataChannelDrainReport::default();

        if close_mode == WebRtcCloseMode::Immediate {
            tracing::debug!(
                peer_id = %self.peer_id,
                "Skipping DataChannel send-buffer drain for immediate WebRTC close"
            );
            return report;
        }

        // Snapshot open channels first, then release the lock before any async waits.
        let open_channels: Vec<(usize, Arc<RTCDataChannel>)> = {
            let channels = self.data_channels.read().await;
            channels
                .iter()
                .enumerate()
                .filter_map(|(idx, opt)| {
                    opt.as_ref().and_then(|ch| {
                        if ch.ready_state() == RTCDataChannelState::Open {
                            Some((idx, Arc::clone(ch)))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };

        for (idx, channel) in open_channels {
            let label = channel.label().to_owned();
            let buffered = channel.buffered_amount().await;
            if buffered == 0 {
                continue;
            }

            let connection_state = self.peer_connection.connection_state();
            if !should_wait_for_data_channel_drain(close_mode, connection_state, buffered) {
                tracing::warn!(
                    peer_id = %self.peer_id,
                    channel = %label,
                    channel_idx = idx,
                    connection_state = ?connection_state,
                    buffered_bytes = buffered,
                    "Skipping DataChannel send-buffer drain because PeerConnection is not connected; \
                     data may be lost for the peer",
                );
                return report;
            }

            match wait_for_data_channel_drain(&channel, drain_timeout).await {
                DataChannelDrainOutcome::AlreadyDrained => {}
                DataChannelDrainOutcome::Notified => {
                    report.notified += 1;
                    tracing::debug!(
                        peer_id = %self.peer_id,
                        channel = %label,
                        channel_idx = idx,
                        "DataChannel send buffer drained",
                    );
                }
                DataChannelDrainOutcome::TimedOut => {
                    report.timed_out += 1;
                    let buffered_bytes = channel.buffered_amount().await;
                    tracing::warn!(
                        peer_id = %self.peer_id,
                        channel = %label,
                        channel_idx = idx,
                        buffered_bytes,
                        "DataChannel send buffer not fully drained before close; \
                         data may be lost for the peer",
                    );
                }
            }
        }

        report
    }

    /// Close connection and broadcast ConnectionClosed event
    ///
    /// This method is idempotent: only the first call performs the actual close.
    /// Subsequent calls return Ok(()) immediately.
    async fn close_with_mode(&self, close_mode: WebRtcCloseMode) -> NetworkResult<()> {
        // Idempotent: only execute once per session
        if !self.session.try_close() {
            tracing::debug!(
                "🔒 [close] serial={} already closed (session_id={}), skipping",
                self.peer_id,
                self.session.session_id
            );
            return Ok(());
        }

        // Close admission: drive the secure session terminal now, before the
        // graceful drain. Every path into this physical close (inherent
        // call sites in the coordinator bypass the trait method — RFC-0563
        // review point on shadowed terminal driving) marks the session
        // `Closed` and invalidates all wrapped `SecureDataLane`s. Idempotent
        // no-op if already terminal.
        self.secure.drive_terminal(SecureChannelState::Closed);

        // Cancel the session token — all callbacks holding a clone will notice
        self.session.cancel();

        tracing::debug!(
            "🔒 [close] serial={} session_id={} step 1: marking closed",
            self.peer_id,
            self.session.session_id
        );
        *self.connected.write().await = false;

        // Forced recovery must not infer transport liveness from a potentially
        // stale PeerConnection::Connected snapshot. Only explicitly graceful
        // shutdowns wait for DataChannel buffers to drain.
        self.drain_data_channels(close_mode).await;

        // Notify upper layers before awaiting RTCPeerConnection::close().
        // Mobile background/resume paths can stall inside the lower-level close
        // after SCTP/ICE has already become unusable; send recovery must not
        // depend on that await completing.
        let _ = self.event_tx.send(ConnectionEvent::ConnectionClosed {
            peer_id: self.peer_id.clone(),
            session_id: self.session.session_id,
        });

        // Fail armed lane waiters fast, before the lower-level close. A lane
        // send waits exclusively on the DataChannel state signal (bounded by
        // INITIAL_CONNECTION_TIMEOUT), and the on_close callback — the only
        // terminal publisher — is replaced with a no-op below; when the
        // lower-level close times out and the channel reaches Closed only
        // afterward, waiters would otherwise park until that timeout. Closing
        // each channel flips its state toward Closing (terminal for lane
        // waits), and publishing the stored signal wakes every armed waiter
        // immediately. All channels close concurrently under ONE shared
        // deadline: `RTCDataChannel::close` flips `Closing` synchronously
        // before its first suspension point, so even a wedged lower-level
        // close leaves every waiter a terminal state once the signals below
        // are published.
        {
            let channels = self.data_channels.read().await.clone();
            let signals = self.data_channel_state_signals.read().await.clone();
            let close_all = futures_util::future::join_all(channels.into_iter().flatten().map(
                |channel| async move {
                    let _ = channel.close().await;
                },
            ));
            let _ = crate::timer::timeout(
                crate::timer::ids::DATA_CHANNEL_CLOSE,
                DATA_CHANNEL_CLOSE_TIMEOUT,
                close_all,
            )
            .await;
            for signal in signals.into_iter().flatten() {
                signal.publish();
            }
        }

        tracing::debug!(
            "🔒 [close] serial={} step 2: closing peer_connection",
            self.peer_id
        );
        let close_result = crate::timer::timeout(
            crate::timer::ids::PEER_CONNECTION_CLOSE,
            PEER_CONNECTION_CLOSE_TIMEOUT,
            self.peer_connection.close(),
        )
        .await;
        let close_error = match close_result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e),
            Err(_) => {
                tracing::warn!(
                    peer_id = %self.peer_id,
                    session_id = self.session.session_id,
                    "RTCPeerConnection close timed out",
                );
                None
            }
        };

        // Break the RTCPeerConnection <-> handler reference cycle so the peer
        // connection can actually be dropped after cleanup. The state-change and
        // data-channel handlers installed on the peer connection each capture a
        // strong `WebRtcConnection` clone, which itself owns an
        // `Arc<RTCPeerConnection>` -- so the connection keeps itself alive through
        // its own callbacks. `RTCPeerConnection::close()` closes the ICE transport
        // but does NOT clear these handlers. Under webrtc-rs 0.20 the driver
        // releases its UDP sockets once the `closing` flag is observed or the
        // Close event is processed — independent of whether this handle's
        // close() future survives — but the driver task keeps the handler (and
        // thus the whole cycle) alive until it exits. Replacing the handlers
        // with no-ops drops the captured clones and breaks the cycle, so
        // releasing the last `Arc<RTCPeerConnection>` lets the driver task and
        // its sockets actually be freed.
        //
        // Handlers are cleared only AFTER the pc.close() await above, so the
        // final state-change event may or may not be observed. Downstream
        // consumers do not depend on it: lane waiters were already released by
        // the explicit channel close + signal publish above, the
        // ConnectionClosed broadcast drives coordinator cleanup (the peers-map
        // entry is removed first), and the health check reconciles its cached
        // state against connection_state() instead of trusting the callback
        // alone.
        self.peer_connection
            .on_peer_connection_state_change(Box::new(move |_state| Box::pin(async move {})));
        self.peer_connection
            .on_data_channel(Box::new(move |_dc| Box::pin(async move {})));

        // Also clear the handlers installed on every cached data channel. These
        // closures are stored on the channel itself: on_close holds connection
        // state (and historically a whole WebRtcConnection clone), and
        // on_message holds the lane's receive-channel sender. Replacing them
        // with capture-free no-ops breaks the channel self-cycles and lets the
        // receive loops terminate, so the channels (and anything they pin) can
        // actually be dropped once the caches below are cleared. Lane waiters
        // were already released above, so suppressing a late on_close here
        // cannot strand them.
        {
            let channels = self.data_channels.read().await.clone();
            for channel in channels.into_iter().flatten() {
                channel.on_close(Box::new(|| Box::pin(async {})));
                channel.on_open(Box::new(|| Box::pin(async {})));
                channel.on_message(Box::new(|_msg| Box::pin(async {})));
                channel.on_error(Box::new(|_err| Box::pin(async {})));
            }
        }

        // Clear each cache under a dedicated lock scope
        {
            let mut cache = self.lane_cache.write().await;
            *cache = [None, None, None, None];
        }
        {
            let mut channels = self.data_channels.write().await;
            *channels = [None, None, None, None];
        }
        {
            let mut signals = self.data_channel_state_signals.write().await;
            *signals = [None, None, None, None];
        }
        {
            let mut tracks = self.media_tracks.write().await;
            tracks.clear();
        }
        {
            let mut seq_nums = self.track_sequence_numbers.write().await;
            seq_nums.clear();
        }
        {
            let mut ssrcs = self.track_ssrcs.write().await;
            ssrcs.clear();
        }

        tracing::info!(
            "🔌 WebRtcConnection closed for peer {:?} (session_id={})",
            self.peer_id,
            self.session.session_id
        );

        if let Some(error) = close_error {
            return Err(error.into());
        }

        Ok(())
    }

    /// Gracefully close the WebRTC connection, draining queued DataChannel data
    /// while the PeerConnection still reports a usable transport.
    pub async fn close(&self) -> NetworkResult<()> {
        self.close_with_mode(WebRtcCloseMode::Graceful).await
    }

    /// Close without waiting for DataChannel buffers.
    ///
    /// Recovery and abnormal cleanup paths use this because WebRTC state can
    /// remain `Connected` after the underlying mobile network path is gone.
    pub(crate) async fn close_immediately(&self) -> NetworkResult<()> {
        self.close_with_mode(WebRtcCloseMode::Immediate).await
    }

    /// based on PayloadType configuration DataChannel
    fn get_data_channel_config(payload_type: &PayloadType) -> RTCDataChannelInit {
        match payload_type {
            PayloadType::StreamLatencyFirst => {
                // partial reliable transmission (low latency priority)
                RTCDataChannelInit {
                    ordered: false,
                    max_retransmits: Some(3),
                    max_packet_life_time: None,
                    protocol: String::new(),
                    negotiated: None,
                }
            }
            _ => {
                // default reliable transmission
                RTCDataChannelInit {
                    ordered: true,
                    max_retransmits: None,
                    max_packet_life_time: None,
                    protocol: String::new(),
                    negotiated: None,
                }
            }
        }
    }
}

impl WebRtcConnection {
    /// Get or create DataLane (with caching)
    pub async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        self.get_lane_internal(payload_type).await
    }

    /// Internal implementation of get_lane
    async fn get_lane_internal(
        &self,
        payload_type: PayloadType,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        // MediaTrack not supported in this method (needs stream_id)
        if payload_type == PayloadType::MediaRtp {
            return Err(NetworkError::NotImplemented(
                "MediaTrack Lane requires stream_id, use get_media_lane() instead".to_string(),
            ));
        }

        // A closing connection must not hand out new lanes: teardown cleared
        // (or is about to clear) every cache and replaces DataChannel
        // callbacks with no-ops, so a lane registered now would outlive its
        // channel's terminal publication and park its sender until the
        // failure timeout.
        if self.is_closing_or_closed() {
            return Err(NetworkError::ConnectionClosed(format!(
                "connection to {} is closing; refusing to create a {:?} lane",
                self.peer_id.to_string_repr(),
                payload_type,
            )));
        }

        let idx = payload_type as usize;

        // 1. Check cache
        let mut need_recreate = false;
        {
            let cache = self.lane_cache.read().await;
            if let Some(lane) = &cache[idx] {
                // Check if the cached lane's transport is still healthy
                if !lane.is_healthy() {
                    tracing::warn!(
                        "Cached lane for {:?} is unhealthy, recreating",
                        payload_type,
                    );
                    need_recreate = true;
                } else {
                    tracing::debug!("Reuse cached DataLane: {:?}", payload_type);
                    return Ok(Arc::clone(lane));
                }
            }
        }

        if need_recreate {
            // Clear stale cache entries before recreating.
            let mut cache = self.lane_cache.write().await;
            cache[idx] = None;
            let mut channels = self.data_channels.write().await;
            channels[idx] = None;
        }

        // 2. Create new DataLane
        let lane = self.create_lane_internal(payload_type).await?;

        // 3. Cache — the session check lives INSIDE the lane-cache critical
        // section: teardown cancels before any clear, so an insert attempted
        // after the cancel observes the closed session here and can never
        // repopulate a cleared cache. The channel/signal entries created
        // above are still swept by teardown's post-cancel snapshot, so the
        // fresh lane is released correctly.
        {
            let mut cache = self.lane_cache.write().await;
            if self.is_closing_or_closed() {
                return Err(NetworkError::ConnectionClosed(format!(
                    "connection to {} closed during {:?} lane creation",
                    self.peer_id.to_string_repr(),
                    payload_type,
                )));
            }
            cache[idx] = Some(Arc::clone(&lane));
        }

        tracing::info!("✨ WebRtcConnection Createnew DataLane: {:?}", payload_type);

        Ok(lane)
    }

    /// Internal implementation of invalidate_lane (see the `WireHandle` impl):
    /// invalidates the cached lane/DataChannel for the given payload type when
    /// the underlying DataChannel has transitioned to Closed and needs to be
    /// recreated on the next `get_lane` call.
    async fn invalidate_lane_internal(&self, payload_type: PayloadType) {
        let idx = payload_type as usize;
        let mut cache = self.lane_cache.write().await;
        cache[idx] = None;
        let mut channels = self.data_channels.write().await;
        channels[idx] = None;
    }

    /// Internal: Create DataChannel Lane (without cache)
    async fn create_lane_internal(
        &self,
        payload_type: PayloadType,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        // Media tracks use a different code path
        if payload_type == PayloadType::MediaRtp {
            return Err(NetworkError::NotImplemented(
                "MediaTrack Lane not implemented in this method".to_string(),
            ));
        }

        // Create new DataChannel
        let mut channels = self.data_channels.write().await;

        let label = payload_type.as_str_name();

        let dc_config = Self::get_data_channel_config(&payload_type);
        let data_channel = self
            .peer_connection
            .create_data_channel(label, Some(dc_config))
            .await
            .map_err(|e| {
                classify_peer_connection_error(e, self.peer_connection.connection_state())
            })?;

        // Register on_open callback to send DataChannelOpened event
        let event_tx_for_open = self.event_tx.clone();
        let peer_id_for_open = self.peer_id.clone();
        let session_id_for_open = self.session.session_id;
        let payload_type_for_open = payload_type;
        let data_channel_state_changed = DataChannelStateSignal::new();
        let state_changed_for_open = data_channel_state_changed.clone();

        data_channel.on_open(Box::new(move || {
            let event_tx = event_tx_for_open.clone();
            let peer_id = peer_id_for_open.clone();
            let payload_type = payload_type_for_open;
            let state_changed = state_changed_for_open.clone();

            tracing::info!("🔄 WebRTC DataChannel opened: {:?}", payload_type);

            Box::pin(async move {
                state_changed.publish();
                let _ = event_tx.send(ConnectionEvent::DataChannelOpened {
                    peer_id,
                    session_id: session_id_for_open,
                    payload_type,
                });
                tracing::debug!("📣 DataChannelOpened event sent for {:?}", payload_type);
            })
        }));

        let channel_id = data_channel.id();
        let payload_type_for_error = payload_type;
        let label_for_error = label;
        data_channel.on_error(Box::new(move |error| {
            let payload_type = payload_type_for_error;
            let label = label_for_error;
            let channel_id = channel_id;
            tracing::warn!(
                "⚠️ WebRTC DataChannel error [{}] (payload_type={:?}, channel_id={}): {:?}",
                label,
                payload_type,
                channel_id,
                error
            );
            Box::pin(async move {})
        }));

        let session_for_close = self.session.clone();
        let lane_cache_for_close = self.lane_cache.clone();
        let data_channels_for_close = self.data_channels.clone();
        let event_tx_for_close = self.event_tx.clone();
        let peer_id_for_close = self.peer_id.clone();
        let sid_for_close = self.session.session_id;
        let payload_type_for_close = payload_type;
        let label_for_close = label;
        let channel_id_for_close = channel_id;
        let state_changed_for_close = data_channel_state_changed.clone();
        // Weak: this closure is stored on the data channel itself, so a strong
        // clone would form a self-cycle that keeps the channel (and its SCTP
        // stream state) alive forever.
        let dc_for_close = Arc::downgrade(&data_channel);
        data_channel.on_close(Box::new(move || {
            let session = session_for_close.clone();
            let lane_cache = lane_cache_for_close.clone();
            let data_channels = data_channels_for_close.clone();
            let event_tx = event_tx_for_close.clone();
            let peer_id = peer_id_for_close.clone();
            let payload_type = payload_type_for_close;
            let label = label_for_close;
            let channel_id = channel_id_for_close;
            let dc_weak = dc_for_close.clone();
            let state_changed = state_changed_for_close.clone();
            Box::pin(async move {
                state_changed.publish();
                // Guard: if session is cancelled (connection already cleaned up),
                // skip all side effects to avoid corrupting a new connection
                if session.is_cancelled() {
                    tracing::debug!(
                        "🚫 DC.on_close session {} cancelled, ignoring for {:?}",
                        sid_for_close,
                        payload_type
                    );
                    return;
                }

                // Query buffered_amount at the moment of close to surface potential
                // data loss. If the channel is already gone, nothing was buffered
                // from our point of view.
                let buffered = match dc_weak.upgrade() {
                    Some(dc) => dc.buffered_amount().await,
                    None => 0,
                };
                if buffered > 0 {
                    tracing::warn!(
                        channel = %label,
                        channel_id = channel_id,
                        payload_type = ?payload_type,
                        buffered_bytes = buffered,
                        "DataChannel closed with non-empty send buffer",
                    );
                } else {
                    tracing::warn!(
                        "DataChannel closed [{}] (payload_type={:?}, channel_id={})",
                        label,
                        payload_type,
                        channel_id,
                    );
                }
                // Invalidate cached lane when DataChannel closes
                let idx = payload_type as usize;
                {
                    let mut cache = lane_cache.write().await;
                    cache[idx] = None;
                }
                {
                    let mut channels = data_channels.write().await;
                    channels[idx] = None;
                }
                // Broadcast DataChannelClosed event
                let _ = event_tx.send(ConnectionEvent::DataChannelClosed {
                    peer_id,
                    session_id: sid_for_close,
                    payload_type,
                });
            })
        }));

        // CreateReceive channel (using Bytes)
        let (tx, rx) = mpsc::channel(100);

        // Set onmessage return adjust
        let tx_clone = tx.clone();
        data_channel.on_message(Box::new(move |msg: RTCDataChannelMessage| {
            // zero-copy: directly using msg.data (Bytes)
            let data = msg.data.freeze();
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let Err(e) = tx.send(data).await {
                    tracing::warn!("❌ WebRTC DataChannel messageSend to Lane failure: {}", e);
                }
            })
        }));

        // Re-check the session before caching: teardown cancels it before
        // taking its close snapshot, so a channel refused here can never be
        // missed by teardown's explicit close/publish, and an accepted one is
        // always swept by it.
        if self.is_closing_or_closed() {
            drop(channels);
            let _ = crate::timer::timeout(
                crate::timer::ids::DATA_CHANNEL_CLOSE,
                DATA_CHANNEL_CLOSE_TIMEOUT,
                data_channel.close(),
            )
            .await;
            return Err(NetworkError::ConnectionClosed(format!(
                "connection to {} closed during {:?} channel creation",
                self.peer_id.to_string_repr(),
                payload_type,
            )));
        }

        // Cache DataChannel (index reference directly using PayloadType value)
        let idx = payload_type as usize;
        channels[idx] = Some(Arc::clone(&data_channel));

        // Returns Lane
        let lane = Arc::new(WebRtcDataLane::new(
            data_channel,
            rx,
            data_channel_state_changed,
        ));
        {
            let mut signals = self.data_channel_state_signals.write().await;
            if self.is_closing_or_closed() {
                // The channel committed above is inside teardown's
                // post-cancel snapshot and is closed/published there; only
                // the signal and lane-cache publication must be skipped.
                return Err(NetworkError::ConnectionClosed(format!(
                    "connection to {} closed during {:?} lane creation",
                    self.peer_id.to_string_repr(),
                    payload_type,
                )));
            }
            signals[idx] = Some(lane.state_signal());
        }
        // Wrap in `SecureDataLane` so the cached lane gates both outbound
        // `send()` and the inbound reader's `recv()` on this session's
        // `SecureCell` (RFC-0563 review point 3).
        let lane: Arc<dyn DataLane> = lane;
        let lane = Arc::new(SecureDataLane::new(lane, self.secure.clone()));
        Ok(lane)
    }

    /// Add media track to PeerConnection
    ///
    /// # Arguments
    /// - `track_id`: Unique track identifier
    /// - `codec`: Codec name (e.g., "H264", "VP8", "opus")
    /// - `media_type`: "video" or "audio"
    ///
    /// # Returns
    /// Reference to the created TrackLocalStaticRTP
    ///
    /// # Note
    /// Must be called BEFORE create_offer/create_answer for track to appear in SDP
    pub async fn add_media_track(
        &self,
        track_id: String,
        codec: &str,
        media_type: &str,
    ) -> NetworkResult<Arc<TrackLocalStaticRTP>> {
        use rtc::media_stream::MediaStreamTrack;
        use rtc::peer_connection::configuration::media_engine::{
            MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8,
        };
        use rtc::rtp_transceiver::rtp_sender::{
            RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
        };

        // Reuse existing track so repeated start/stop flows can safely retry.
        if let Some((track, _sender)) = self.media_tracks.read().await.get(&track_id).cloned() {
            tracing::info!("♻️ Reusing existing media track: {}", track_id);
            return Ok(track);
        }

        // Determine MIME type and media kind based on codec and media_type.
        let (mime_type, kind, clock_rate, channels) =
            match (media_type, codec.to_uppercase().as_str()) {
                ("video", "H264") => (MIME_TYPE_H264, RtpCodecKind::Video, 90_000, 0),
                ("video", "VP8") => (MIME_TYPE_VP8, RtpCodecKind::Video, 90_000, 0),
                ("audio", "OPUS") => (MIME_TYPE_OPUS, RtpCodecKind::Audio, 48_000, 2),
                _ => {
                    return Err(NetworkError::WebRtcError(format!(
                        "Unsupported codec: {codec} for {media_type}"
                    )));
                }
            };

        let ssrc = rand::random::<u32>();
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            format!("actr-{media_type}"),
            track_id.clone(),
            track_id.clone(),
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
                active: true,
                ..Default::default()
            }],
        )));

        // Add track to PeerConnection
        let rtp_sender = self
            .peer_connection
            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
            .await?;

        // Cache track and sender
        let mut tracks = self.media_tracks.write().await;
        tracks.insert(track_id.clone(), (Arc::clone(&track), rtp_sender));

        // Initialize sequence number for this track
        let mut seq_nums = self.track_sequence_numbers.write().await;
        seq_nums.insert(track_id.clone(), Arc::new(AtomicU16::new(0)));

        let mut ssrcs = self.track_ssrcs.write().await;
        ssrcs.insert(track_id.clone(), ssrc);

        tracing::info!(
            "✨ Added media track: id={}, codec={}, type={}, ssrc=0x{:08x}",
            track_id,
            codec,
            media_type,
            ssrc
        );

        Ok(track)
    }

    /// Remove a media track and its RTP sender from the PeerConnection
    pub async fn remove_media_track(&self, track_id: &str) -> NetworkResult<()> {
        let removed = self.media_tracks.write().await.remove(track_id);
        if let Some((_track, rtp_sender)) = removed {
            self.peer_connection.remove_track(&rtp_sender).await?;
            self.track_sequence_numbers.write().await.remove(track_id);
            self.track_ssrcs.write().await.remove(track_id);
            tracing::info!("🗑️ Removed media track: {}", track_id);
        }
        Ok(())
    }

    /// Get existing media track by ID
    pub async fn get_media_track(&self, track_id: &str) -> Option<Arc<TrackLocalStaticRTP>> {
        let tracks = self.media_tracks.read().await;
        tracks
            .get(track_id)
            .map(|(track, _sender)| Arc::clone(track))
    }

    /// Get next RTP sequence number for track (atomically increments)
    ///
    /// # Arguments
    /// - `track_id`: Track identifier
    ///
    /// # Returns
    /// Next sequence number (wraps at 65535)
    pub async fn next_sequence_number(&self, track_id: &str) -> Option<u16> {
        let seq_nums = self.track_sequence_numbers.read().await;
        seq_nums
            .get(track_id)
            .map(|atomic_seq| atomic_seq.fetch_add(1, Ordering::SeqCst))
    }

    /// Get SSRC for track
    ///
    /// # Arguments
    /// - `track_id`: Track identifier
    ///
    /// # Returns
    /// SSRC value for this track
    pub async fn get_ssrc(&self, track_id: &str) -> Option<u32> {
        let ssrcs = self.track_ssrcs.read().await;
        ssrcs.get(track_id).copied()
    }

    /// Register received DataChannel (for passive side)
    ///
    /// When receiving an Offer, the passive side should register DataChannels
    /// received via on_data_channel callback instead of creating new ones.
    pub async fn register_received_data_channel(
        &self,
        data_channel: Arc<RTCDataChannel>,
        payload_type: PayloadType,
        message_tx: mpsc::Sender<WebRtcInboundMessage>,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        // Check if it's MediaTrack type
        if payload_type == PayloadType::MediaRtp {
            return Err(NetworkError::NotImplemented(
                "MediaTrack Lane not supported in this method".to_string(),
            ));
        }

        // A closing connection must not accept late channels: teardown
        // already replaced (or is about to replace) every DataChannel
        // callback with a no-op and cleared the caches, so a registration now
        // would miss the terminal publication, repopulate cleared caches, and
        // pin a receive task on a dead connection.
        if self.is_closing_or_closed() {
            let _ = crate::timer::timeout(
                crate::timer::ids::DATA_CHANNEL_CLOSE,
                DATA_CHANNEL_CLOSE_TIMEOUT,
                data_channel.close(),
            )
            .await;
            return Err(NetworkError::ConnectionClosed(format!(
                "connection to {} is closing; refusing received {:?} channel",
                self.peer_id.to_string_repr(),
                payload_type,
            )));
        }

        let idx = payload_type as usize;
        tracing::debug!(
            "🔄 WebRTC DataChannel registered received: {:?}, idx={}",
            payload_type,
            idx
        );
        let label = format!("{payload_type:?}");

        // Register on_open callback to send DataChannelOpened event
        let event_tx_for_open = self.event_tx.clone();
        let peer_id_for_open = self.peer_id.clone();
        let session_id_for_open = self.session.session_id;
        let payload_type_for_open = payload_type;
        let data_channel_state_changed = DataChannelStateSignal::new();
        let state_changed_for_open = data_channel_state_changed.clone();

        data_channel.on_open(Box::new(move || {
            let event_tx = event_tx_for_open.clone();
            let peer_id = peer_id_for_open.clone();
            let payload_type = payload_type_for_open;
            let state_changed = state_changed_for_open.clone();

            tracing::info!(
                "🔄 WebRTC DataChannel opened (received): {:?}",
                payload_type
            );

            Box::pin(async move {
                state_changed.publish();
                let _ = event_tx.send(ConnectionEvent::DataChannelOpened {
                    peer_id,
                    session_id: session_id_for_open,
                    payload_type,
                });
                tracing::debug!("📣 DataChannelOpened event sent for {:?}", payload_type);
            })
        }));

        // Set error handler
        let payload_type_for_error = payload_type;
        let label_for_error = label.clone();
        data_channel.on_error(Box::new(move |error| {
            let payload_type = payload_type_for_error;
            let label = label_for_error.clone();
            tracing::warn!(
                "⚠️ WebRTC DataChannel error [{}] (payload_type={:?} ): {:?}",
                label,
                payload_type,
                error
            );
            Box::pin(async move {})
        }));

        // Set close handler. Capture individual fields instead of `self`: this
        // closure is stored on the data channel, and a strong `WebRtcConnection`
        // clone owns the `Arc<RTCPeerConnection>`, so capturing `self` would keep
        // the whole peer-connection graph (including its ICE UDP sockets) alive
        // through the channel's own callback. Mirrors create_lane_internal.
        let session_for_close = self.session.clone();
        let lane_cache_for_close = self.lane_cache.clone();
        let data_channels_for_close = self.data_channels.clone();
        let event_tx_for_close = self.event_tx.clone();
        let peer_id_for_close = self.peer_id.clone();
        let sid_for_close = self.session.session_id;
        let payload_type_for_close = payload_type;
        let label_for_close = label.clone();
        let state_changed_for_close = data_channel_state_changed.clone();
        // Weak: a strong clone of the channel inside its own handler would be a
        // self-cycle that keeps the channel alive forever.
        let dc_for_close = Arc::downgrade(&data_channel);

        data_channel.on_close(Box::new(move || {
            let session = session_for_close.clone();
            let lane_cache = lane_cache_for_close.clone();
            let data_channels = data_channels_for_close.clone();
            let event_tx = event_tx_for_close.clone();
            let peer_id = peer_id_for_close.clone();
            let payload_type = payload_type_for_close;
            let label = label_for_close.clone();
            let dc_weak = dc_for_close.clone();
            let state_changed = state_changed_for_close.clone();

            Box::pin(async move {
                state_changed.publish();
                // Guard: if session is cancelled (connection already cleaned up),
                // skip all side effects to avoid corrupting a new connection
                if session.is_cancelled() {
                    tracing::debug!(
                        "🚫 DC.on_close (received) session {} cancelled, ignoring for {:?}",
                        sid_for_close,
                        payload_type
                    );
                    return;
                }

                // Query buffered_amount at the moment of close to surface potential
                // data loss. If the channel is already gone, nothing was buffered
                // from our point of view.
                let buffered = match dc_weak.upgrade() {
                    Some(dc) => dc.buffered_amount().await,
                    None => 0,
                };
                if buffered > 0 {
                    tracing::warn!(
                        peer_id = %peer_id,
                        channel = %label,
                        payload_type = ?payload_type,
                        buffered_bytes = buffered,
                        "DataChannel (received) closed with non-empty send buffer; \
                         buffered data was likely not delivered to peer",
                    );
                } else {
                    tracing::warn!(
                        "DataChannel (received) closed [{}] (payload_type={:?})",
                        label,
                        payload_type,
                    );
                }
                // Invalidate cached lane when DataChannel closes
                let idx = payload_type as usize;
                {
                    let mut cache = lane_cache.write().await;
                    cache[idx] = None;
                }
                {
                    let mut channels = data_channels.write().await;
                    channels[idx] = None;
                }
                // Broadcast DataChannelClosed event
                let _ = event_tx.send(ConnectionEvent::DataChannelClosed {
                    peer_id,
                    session_id: sid_for_close,
                    payload_type,
                });
            })
        }));

        // Create receive channel
        let (tx, rx) = mpsc::channel(100);

        // Set on_message callback
        let tx_clone = tx.clone();
        data_channel.on_message(Box::new(move |msg: RTCDataChannelMessage| {
            let data = msg.data.freeze();
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let Err(e) = tx.send(data).await {
                    tracing::warn!("❌ WebRTC DataChannel message send to Lane failed: {}", e);
                }
            })
        }));

        #[cfg(any(test, feature = "test-utils"))]
        self.hit_registration_commit_barrier_for_test(payload_type)
            .await;

        // Commit under each cache lock itself, never check-then-lock:
        // teardown cancels the session BEFORE taking its close snapshot, so
        // an insert accepted before the snapshot is always swept (closed +
        // published), and one attempted after the cancel observes the closed
        // session inside the same critical section and is refused. No insert
        // can slip between a passed check and the cache clear. Refusing here
        // skips every later insert and the receive task below.
        {
            let mut channels = self.data_channels.write().await;
            if self.is_closing_or_closed() {
                // Not swept by any snapshot: the fresh channel exists only in
                // this frame, so close it ourselves.
                drop(channels);
                let _ = crate::timer::timeout(
                    crate::timer::ids::DATA_CHANNEL_CLOSE,
                    DATA_CHANNEL_CLOSE_TIMEOUT,
                    data_channel.close(),
                )
                .await;
                return Err(NetworkError::ConnectionClosed(format!(
                    "connection to {} closed while registering received {:?} channel",
                    self.peer_id.to_string_repr(),
                    payload_type,
                )));
            }
            channels[idx] = Some(Arc::clone(&data_channel));
        }

        // Create and cache Lane
        let lane = Arc::new(WebRtcDataLane::new(
            data_channel,
            rx,
            data_channel_state_changed,
        ));
        {
            let mut signals = self.data_channel_state_signals.write().await;
            if self.is_closing_or_closed() {
                // The channel committed above is inside teardown's
                // post-cancel snapshot and is closed/published there; only
                // the signal and lane publication must be skipped.
                return Err(NetworkError::ConnectionClosed(format!(
                    "connection to {} closed during received {:?} lane registration",
                    self.peer_id.to_string_repr(),
                    payload_type,
                )));
            }
            signals[idx] = Some(lane.state_signal());
        }
        // Wrap in `SecureDataLane` so the cached lane gates both the inbound
        // reader's `recv()` (spawned below) and outbound `send()` on this
        // session's `SecureCell` (RFC-0563 review point 3). Wrapping before the
        // cache commit and reader spawn ensures the reader holds the gated lane.
        let lane: Arc<dyn DataLane> = lane;
        let lane: Arc<dyn DataLane> = Arc::new(SecureDataLane::new(lane, self.secure.clone()));
        {
            let mut cache = self.lane_cache.write().await;
            if self.is_closing_or_closed() {
                return Err(NetworkError::ConnectionClosed(format!(
                    "connection to {} closed during received {:?} lane registration",
                    self.peer_id.to_string_repr(),
                    payload_type,
                )));
            }
            cache[idx] = Some(Arc::clone(&lane));
        }

        tracing::info!(
            "✨ WebRtcConnection registered received DataChannel: {:?}",
            payload_type
        );
        let peer_id_clone = self.peer_id.clone();
        let lane_clone = lane.clone();
        tokio::spawn(async move {
            // Continuously receive messages
            loop {
                match lane_clone.recv().await {
                    Ok(data) => {
                        tracing::debug!(
                            "📨 Received message from {:?} (PayloadType: {:?}): {} bytes",
                            peer_id_clone,
                            payload_type,
                            data.len()
                        );

                        // Serialize peer_id as bytes
                        let peer_id_bytes = peer_id_clone.encode_to_vec();

                        // Send to the traffic-class aggregation channel (include PayloadType).
                        if let Err(e) = message_tx.send((peer_id_bytes, data, payload_type)).await {
                            tracing::error!("❌ Message aggregation failed: {:?}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "❌ Peer {:?} message receive failed (PayloadType: {:?}): {}",
                            peer_id_clone,
                            payload_type,
                            e
                        );
                        break;
                    }
                }
            }
        });

        Ok(lane)
    }
}

#[async_trait]
impl WireHandle for WebRtcConnection {
    fn connection_type(&self) -> ConnType {
        ConnType::WebRTC
    }

    fn priority(&self) -> u8 {
        1 // WebRTC has higher priority
    }

    async fn connect(&self) -> NetworkResult<()> {
        // TODO(phase3): drive `LegacyBypass → New → Handshaking → Verified`
        // once the authenticated handshake lands. For now connect() returns
        // only when transport-ready (`LegacyBypass` is sendable).
        //
        // No per-attempt `Failed` drive: a terminal state would make a later
        // successful `WirePool` retry on this same object permanently
        // unsendable. Failure is declared only at retry exhaustion
        // (`mark_secure_failed`).
        Self::connect(self).await
    }

    fn is_connected(&self) -> bool {
        !self.session.is_closed()
    }

    async fn close(&self) -> NetworkResult<()> {
        // The inherent `Self::close` (close_with_mode) owns terminal driving.
        Self::close(self).await
    }

    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        self.get_lane_internal(payload_type).await
    }

    async fn invalidate_lane(&self, payload_type: PayloadType) {
        self.invalidate_lane_internal(payload_type).await;
    }

    fn identity(&self) -> Option<crate::transport::WireIdentity> {
        Some(crate::transport::WireIdentity::WebRtc {
            peer_id: self.peer_id.clone(),
            session_id: self.session_id(),
        })
    }

    fn secure_channel_state(&self) -> SecureChannelState {
        self.secure.state()
    }

    fn secure_epoch(&self) -> u64 {
        self.secure.epoch()
    }

    async fn await_secure_state_change(&self, observed_epoch: u64) {
        self.secure.wait_after(observed_epoch).await;
    }

    fn mark_secure_failed(&self) {
        self.secure.drive_terminal(SecureChannelState::Failed);
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
