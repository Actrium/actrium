//! DataLane - Business data transport channel (trait-based abstraction)
//!
//! DataLane is the core trait of the transport layer for message/data transmission.
//! Note: MediaTrack uses a separate MediaFrameRegistry path, not DataLane.
//!
//! ## Design Philosophy
//!
//! ```text
//! DataLane trait:
//!   ✓ trait object for cross-platform extensibility (Arc<dyn DataLane>)
//!   ✓ 3 built-in implementations: MpscLane, WebRtcDataLane, WebSocketDataLane
//!   ✓ Unified send/recv API for data messages
//!   ✓ Platform-specific implementations can be provided externally
//! ```
//!
//! ## WebRTC DataChannel Fragmentation
//!
//! WebRTC DataChannel has a 64KB per-message limit. The `WebRtcDataLane`
//! transparently fragments outgoing messages and reassembles incoming fragments.
//! Upper layers are unaware of this mechanism.
//!
//! ### Fragment header format (8 bytes, prepended to each DataChannel message)
//!
//! ```text
//! [4 bytes: msg_id       u32 big-endian]
//! [2 bytes: frag_index   u16 big-endian]
//! [2 bytes: total_frags  u16 big-endian]
//! ```
//!
//! When `total_frags == 1` the message fits in a single DataChannel message;
//! the receiver strips the header and returns the payload directly.

use super::error::{NetworkError, NetworkResult, is_tungstenite_closed};
use actr_protocol::PayloadType;
use async_trait::async_trait;
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// ── WebRTC DataChannel fragmentation constants ────────────────────────────────

/// Maximum size of a single WebRTC DataChannel message.
///
/// The SCTP transport layer uses a buffer of `u16::MAX` (65535) bytes.
/// Using `64 * 1024` (65536) would exceed this by 1 byte, causing
/// `ErrShortBuffer` on the SCTP layer.
const DC_MAX_MESSAGE_SIZE: usize = 65535;

/// Size of the fragment header prepended to every DataChannel message.
pub(crate) const FRAGMENT_HEADER_SIZE: usize = 8;

/// Stale fragment entries older than this are evicted from ReassemblyBuffer.
///
/// Loss semantics: the TTL is anchored at the first fragment. A sender that
/// stalls mid-message for longer than this bound loses its original partial
/// entry; any trailing fragments then open a fresh, uncompletable "zombie"
/// entry that consumes budget until its own TTL expires (bounded, self-healing
/// churn). The previous integration TTL was six hours; 30 s trades rare
/// very-slow senders for prompt reclamation of reassembly budget.
pub(crate) const REASSEMBLY_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum payload bytes that fit in one DataChannel message after the header.
pub(crate) const DC_MAX_PAYLOAD_SIZE: usize = DC_MAX_MESSAGE_SIZE - FRAGMENT_HEADER_SIZE;

// Keep the historical integration-test seam at the fragmentation boundary.
// The v0.20 data plane moved fragmentation from `WebRtcDataLane` into
// `RtcPeerControllerHandle`, but the legacy mobile interruption tests still
// need to pause a concrete multi-fragment send at the same point in time.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRtcFragmentSendEvent {
    pub msg_id: u32,
    pub frag_index: u16,
    pub total_frags: u16,
    pub fragment_payload_len: usize,
    pub message_len: usize,
}

#[cfg(feature = "test-utils")]
pub type WebRtcFragmentSendHookFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

#[cfg(feature = "test-utils")]
pub type WebRtcFragmentSendHook =
    Arc<dyn Fn(WebRtcFragmentSendEvent) -> WebRtcFragmentSendHookFuture + Send + Sync + 'static>;

#[cfg(feature = "test-utils")]
static WEBRTC_FRAGMENT_SEND_HOOK: std::sync::OnceLock<
    std::sync::Mutex<Option<WebRtcFragmentSendHook>>,
> = std::sync::OnceLock::new();

#[cfg(feature = "test-utils")]
fn webrtc_fragment_send_hook_slot() -> &'static std::sync::Mutex<Option<WebRtcFragmentSendHook>> {
    WEBRTC_FRAGMENT_SEND_HOOK.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(feature = "test-utils")]
pub struct WebRtcFragmentSendHookGuard {
    previous: Option<WebRtcFragmentSendHook>,
}

#[cfg(feature = "test-utils")]
impl Drop for WebRtcFragmentSendHookGuard {
    fn drop(&mut self) {
        let mut hook = webrtc_fragment_send_hook_slot()
            .lock()
            .expect("fragment send hook mutex poisoned");
        *hook = self.previous.take();
    }
}

#[cfg(feature = "test-utils")]
pub fn install_webrtc_fragment_send_hook_for_test(
    hook: WebRtcFragmentSendHook,
) -> WebRtcFragmentSendHookGuard {
    let mut slot = webrtc_fragment_send_hook_slot()
        .lock()
        .expect("fragment send hook mutex poisoned");
    let previous = slot.replace(hook);
    WebRtcFragmentSendHookGuard { previous }
}

#[cfg(feature = "test-utils")]
pub(crate) async fn notify_webrtc_fragment_sent_for_test(event: WebRtcFragmentSendEvent) {
    let hook = {
        webrtc_fragment_send_hook_slot()
            .lock()
            .expect("fragment send hook mutex poisoned")
            .clone()
    };

    if let Some(hook) = hook {
        hook(event).await;
    }
}

// ── Reassembly types ──────────────────────────────────────────────────────────

/// Holds the pieces of a multi-fragment message while waiting for all fragments.
struct FragmentEntry {
    total: u16,
    /// Timestamp when the first fragment arrived (TTL eviction is anchored
    /// here).
    created_at: Instant,
    /// Timestamp of the last accepted fragment. Pressure eviction ranks by
    /// this so an actively progressing transfer is not starved by quiet stale
    /// entries (LRU-style fairness).
    last_activity: Instant,
    fragments: HashMap<u16, bytes::Bytes>,
    bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReassemblyLimits {
    pub(crate) max_messages: usize,
    pub(crate) max_bytes: usize,
    pub(crate) max_message_bytes: usize,
    pub(crate) max_fragments_per_message: u16,
}

impl Default for ReassemblyLimits {
    fn default() -> Self {
        Self {
            max_messages: 64,
            max_bytes: 16 * 1024 * 1024,
            max_message_bytes: 8 * 1024 * 1024,
            max_fragments_per_message: (8usize * 1024 * 1024)
                .div_ceil(DC_MAX_PAYLOAD_SIZE)
                .min(u16::MAX as usize) as u16,
        }
    }
}

impl ReassemblyLimits {
    #[cfg(feature = "webrtc-v020")]
    pub(crate) fn from_buffer_config(
        config: &crate::wire::webrtc::v020::config::RtcBufferConfig,
    ) -> Self {
        let fragments = config
            .max_inbound_message_bytes
            .div_ceil(DC_MAX_PAYLOAD_SIZE)
            .min(u16::MAX as usize) as u16;
        Self {
            max_messages: config.max_reassembly_messages,
            max_bytes: config.max_reassembly_bytes,
            max_message_bytes: config.max_inbound_message_bytes,
            max_fragments_per_message: fragments.max(1),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ReassemblyError {
    #[error("invalid fragment index {index} for total {total}")]
    InvalidIndex { index: u16, total: u16 },
    #[error("fragment count {total} exceeds configured maximum {max}")]
    TooManyFragments { total: u16, max: u16 },
    #[error("fragment total changed for message {msg_id}: expected {expected}, got {actual}")]
    InconsistentTotal {
        msg_id: u32,
        expected: u16,
        actual: u16,
    },
    #[error("reassembly byte budget exhausted")]
    ByteBudgetExhausted,
    #[error("reassembled message exceeds configured maximum")]
    MessageTooLarge,
}

/// Accumulates in-flight fragmented messages keyed by `msg_id`.
///
/// ## Safety features
///
/// - **TTL eviction**: Stale entries older than [`REASSEMBLY_TTL`] are removed
///   on each `insert()` call to prevent unbounded memory growth.
pub(crate) struct ReassemblyBuffer {
    pending: HashMap<u32, FragmentEntry>,
    pending_bytes: usize,
    limits: ReassemblyLimits,
    /// Correlation context (peer/generation/lane) for eviction diagnostics.
    label: String,
}

impl ReassemblyBuffer {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_limits(ReassemblyLimits::default())
    }

    #[cfg(test)]
    pub(crate) fn with_limits(limits: ReassemblyLimits) -> Self {
        Self::with_limits_and_label(limits, String::new())
    }

    /// Like [`with_limits`], with correlation context for eviction warnings.
    pub(crate) fn with_limits_and_label(limits: ReassemblyLimits, label: String) -> Self {
        Self {
            pending: HashMap::new(),
            pending_bytes: 0,
            limits,
            label,
        }
    }

    /// Insert a fragment.
    ///
    /// Returns the fully reassembled payload when all fragments for `msg_id`
    /// have arrived, or `None` if more fragments are still missing.
    #[cfg(test)]
    pub(crate) fn insert(
        &mut self,
        msg_id: u32,
        frag_index: u16,
        total_frags: u16,
        payload: bytes::Bytes,
    ) -> Option<bytes::Bytes> {
        self.try_insert(msg_id, frag_index, total_frags, payload)
            .ok()
            .flatten()
    }

    /// Bounded insertion used by the v0.20 production pump. Every malformed or
    /// over-budget fragment is reported explicitly; partial state for the
    /// affected message is discarded so an attacker cannot permanently poison
    /// a message id or retain its bytes until the TTL.
    pub(crate) fn try_insert(
        &mut self,
        msg_id: u32,
        frag_index: u16,
        total_frags: u16,
        payload: bytes::Bytes,
    ) -> Result<Option<bytes::Bytes>, ReassemblyError> {
        // Evict stale entries
        self.evict_stale();

        if total_frags == 0 || frag_index >= total_frags {
            self.discard(msg_id);
            return Err(ReassemblyError::InvalidIndex {
                index: frag_index,
                total: total_frags,
            });
        }
        if total_frags > self.limits.max_fragments_per_message {
            self.discard(msg_id);
            return Err(ReassemblyError::TooManyFragments {
                total: total_frags,
                max: self.limits.max_fragments_per_message,
            });
        }
        if let Some(entry) = self.pending.get(&msg_id) {
            if entry.total != total_frags {
                let expected = entry.total;
                self.discard(msg_id);
                return Err(ReassemblyError::InconsistentTotal {
                    msg_id,
                    expected,
                    actual: total_frags,
                });
            }
            if entry.fragments.contains_key(&frag_index) {
                // Retransmitted duplicate: it consumes no additional budget.
                return Ok(None);
            }
        } else if self.pending.len() >= self.limits.max_messages {
            // A bounded buffer must remain live under abandoned fragmented
            // messages. Prefer dropping the oldest incomplete message over
            // rejecting every new multi-fragment API call until a long TTL.
            self.evict_oldest(None, "message_count");
        }

        let entry_bytes = self
            .pending
            .get(&msg_id)
            .map(|entry| entry.bytes)
            .unwrap_or(0);
        if entry_bytes
            .checked_add(payload.len())
            .is_none_or(|bytes| bytes > self.limits.max_message_bytes)
        {
            self.discard(msg_id);
            return Err(ReassemblyError::MessageTooLarge);
        }
        while self
            .pending_bytes
            .checked_add(payload.len())
            .is_none_or(|bytes| bytes > self.limits.max_bytes)
        {
            if !self.evict_oldest(Some(msg_id), "byte_budget") {
                self.discard(msg_id);
                return Err(ReassemblyError::ByteBudgetExhausted);
            }
        }

        let entry = self.pending.entry(msg_id).or_insert_with(|| FragmentEntry {
            total: total_frags,
            created_at: Instant::now(),
            last_activity: Instant::now(),
            fragments: HashMap::new(),
            bytes: 0,
        });
        entry.bytes += payload.len();
        self.pending_bytes += payload.len();
        entry.fragments.insert(frag_index, payload);
        entry.last_activity = Instant::now();

        if entry.fragments.len() == entry.total as usize {
            // All fragments arrived – reassemble in order.
            let entry = self.pending.remove(&msg_id).unwrap();
            self.pending_bytes = self.pending_bytes.saturating_sub(entry.bytes);
            let mut ordered: Vec<(u16, bytes::Bytes)> = entry.fragments.into_iter().collect();
            ordered.sort_by_key(|(idx, _)| *idx);

            let total_len: usize = ordered.iter().map(|(_, b)| b.len()).sum();
            let mut out = bytes::BytesMut::with_capacity(total_len);
            for (_, frag) in ordered {
                out.extend_from_slice(&frag);
            }
            Ok(Some(out.freeze()))
        } else {
            Ok(None)
        }
    }

    fn discard(&mut self, msg_id: u32) {
        if let Some(entry) = self.pending.remove(&msg_id) {
            self.pending_bytes = self.pending_bytes.saturating_sub(entry.bytes);
        }
    }

    fn evict_oldest(&mut self, exclude: Option<u32>, pressure: &str) -> bool {
        // Rank by last accepted fragment rather than first arrival: a stalled
        // partial message loses its slot to genuinely quiet entries, so churn
        // cannot repeatedly starve an older but actively progressing transfer
        // just before it completes.
        let oldest = self
            .pending
            .iter()
            .filter(|(msg_id, _)| Some(**msg_id) != exclude)
            .min_by_key(|(_, entry)| entry.last_activity)
            .map(|(msg_id, _)| *msg_id);
        if let Some(msg_id) = oldest {
            let evicted_bytes = self.pending.get(&msg_id).map_or(0, |entry| entry.bytes);
            self.discard(msg_id);
            tracing::warn!(
                lane_context = %self.label,
                msg_id,
                evicted_bytes,
                pressure,
                "evicting least-recently-active incomplete reassembly message to preserve ingress liveness"
            );
            true
        } else {
            false
        }
    }

    /// Remove entries that have been pending longer than [`REASSEMBLY_TTL`].
    fn evict_stale(&mut self) {
        let now = Instant::now();
        let before = self.pending.len();
        let mut evicted_bytes = 0usize;
        self.pending.retain(|msg_id, entry| {
            let age = now.duration_since(entry.created_at);
            if age > REASSEMBLY_TTL {
                evicted_bytes = evicted_bytes.saturating_add(entry.bytes);
                tracing::warn!(
                    "evicting stale reassembly entry: msg_id={} \
                     (age={:.1}s, {}/{} fragments received)",
                    msg_id,
                    age.as_secs_f64(),
                    entry.fragments.len(),
                    entry.total,
                );
                false
            } else {
                true
            }
        });
        self.pending_bytes = self.pending_bytes.saturating_sub(evicted_bytes);
        let evicted = before - self.pending.len();
        if evicted > 0 {
            tracing::info!(
                "evicted {} stale reassembly entries ({} remaining)",
                evicted,
                self.pending.len()
            );
        }
    }
}

// ── Fragment header encode / decode ───────────────────────────────────────────

/// Encode a fragment header into `buf` (8 bytes).
#[inline]
pub(crate) fn encode_fragment_header(
    buf: &mut Vec<u8>,
    msg_id: u32,
    frag_index: u16,
    total_frags: u16,
) {
    buf.extend_from_slice(&msg_id.to_be_bytes());
    buf.extend_from_slice(&frag_index.to_be_bytes());
    buf.extend_from_slice(&total_frags.to_be_bytes());
}

/// Decode a fragment header from a raw DataChannel message.
///
/// Returns `(msg_id, frag_index, total_frags, payload)` on success.
///
/// # Errors
/// Returns [`NetworkError::DataChannelError`] when the message is shorter than
/// [`FRAGMENT_HEADER_SIZE`].
#[inline]
pub(crate) fn decode_fragment_header(
    raw: bytes::Bytes,
) -> NetworkResult<(u32, u16, u16, bytes::Bytes)> {
    if raw.len() < FRAGMENT_HEADER_SIZE {
        return Err(NetworkError::DataChannelError(format!(
            "fragment too short: {} bytes (minimum {})",
            raw.len(),
            FRAGMENT_HEADER_SIZE
        )));
    }
    let msg_id = u32::from_be_bytes(raw[0..4].try_into().unwrap());
    let frag_index = u16::from_be_bytes(raw[4..6].try_into().unwrap());
    let total_frags = u16::from_be_bytes(raw[6..8].try_into().unwrap());
    let payload = raw.slice(FRAGMENT_HEADER_SIZE..);
    Ok((msg_id, frag_index, total_frags, payload))
}

// ── Type alias ────────────────────────────────────────────────────────────────

/// Type alias for WebSocket sink (shared across all PayloadTypes)
pub(crate) type WsSink =
    Arc<Mutex<Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, WsMessage>>>>;

// ── DataLane trait ────────────────────────────────────────────────────────────

/// How far an outbound payload progressed before a send returned an error.
///
/// This is deliberately crate-private.  The distinction is used by the
/// recovery layer to avoid replaying bytes that may already have entered a
/// transport; public callers continue to receive the existing error surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryProgress {
    /// No transport write was attempted for this payload.
    NotAttempted,
    /// The local transport accepted the payload (remote processing is not
    /// implied).
    SubmittedLocally,
    /// The send may have entered the transport, but completion is unknown.
    Unknown,
}

/// Result of one low-level lane submission, including progress that must not be
/// inferred later from a coarse `NetworkError`.
#[derive(Debug)]
pub struct SendAttemptResult {
    pub(crate) result: NetworkResult<()>,
    pub(crate) progress: DeliveryProgress,
    /// The adapter proved that no byte could have reached its transport.
    /// This is separate from `progress`: a custom adapter may report
    /// `NotAttempted` without satisfying the framework's replay proof.
    pub(crate) proven_pre_send: bool,
}

impl SendAttemptResult {
    /// Construct an attempt with an explicit delivery classification.
    ///
    /// This constructor is exposed only through the `test-utils` transport
    /// surface so deterministic integration lanes can model pre-send and
    /// ambiguous failures without weakening the production default.
    #[cfg(feature = "test-utils")]
    pub fn for_test(result: NetworkResult<()>, progress: DeliveryProgress) -> Self {
        Self::for_test_with_proof(result, progress, false)
    }

    /// Construct a test attempt with an explicit adapter-level pre-send proof.
    /// Keeping this bit separate from `DeliveryProgress::NotAttempted` lets
    /// tests prove that framework replay requires both facts.
    #[cfg(feature = "test-utils")]
    pub fn for_test_with_proof(
        result: NetworkResult<()>,
        progress: DeliveryProgress,
        proven_pre_send: bool,
    ) -> Self {
        Self {
            result,
            progress,
            proven_pre_send: proven_pre_send && progress == DeliveryProgress::NotAttempted,
        }
    }

    pub(crate) fn submitted() -> Self {
        Self {
            result: Ok(()),
            progress: DeliveryProgress::SubmittedLocally,
            proven_pre_send: false,
        }
    }

    pub(crate) fn failed(error: NetworkError, progress: DeliveryProgress) -> Self {
        Self::failed_with_proof(error, progress, false)
    }

    pub(crate) fn failed_pre_send(error: NetworkError) -> Self {
        Self::failed_with_proof(error, DeliveryProgress::NotAttempted, true)
    }

    pub(crate) fn failed_with_proof(
        error: NetworkError,
        progress: DeliveryProgress,
        proven_pre_send: bool,
    ) -> Self {
        Self {
            result: Err(error),
            progress,
            proven_pre_send: proven_pre_send && progress == DeliveryProgress::NotAttempted,
        }
    }

    pub fn into_result(self) -> NetworkResult<()> {
        self.result
    }
}

/// The progress recorded for one outbound operation, kept separately from
/// the in-flight flag so a lane write in progress never overwrites the last
/// authoritative classification of a completed attempt.
#[derive(Default)]
pub(crate) struct RecordedDeliveryProgress {
    /// Progress of the most recent completed lane attempt (`None` = no
    /// attempt ever completed).
    completed: Option<DeliveryProgress>,
    /// True between "a lane write is about to start" and "that write
    /// resolved". A cancellation inside this window cannot be classified
    /// any other way than `Unknown`.
    write_in_flight: bool,
}

/// Shared sink receiving the delivery progress of one in-flight operation.
///
/// The transport marks the write in flight immediately before each lane
/// write and records the authoritative classification when the lane
/// resolves. A deadline wrapper that cancels the send future can therefore
/// still distinguish "never reached a transport" (retryable) from "may have
/// entered a transport" (uncertain) — and a quick proven-pre-send failure is
/// never misread as ambiguity.
pub(crate) type DeliveryProgressCell = Arc<std::sync::Mutex<RecordedDeliveryProgress>>;

/// Create an empty progress cell for one outbound operation.
pub(crate) fn new_delivery_progress_cell() -> DeliveryProgressCell {
    Arc::new(std::sync::Mutex::new(RecordedDeliveryProgress::default()))
}

/// Mark that a lane write is about to start. A cancellation observed while
/// this flag is set conservatively classifies as `Unknown`, regardless of
/// any earlier completed attempt.
pub(crate) fn mark_delivery_write_in_flight(cell: &DeliveryProgressCell) {
    if let Ok(mut guard) = cell.lock() {
        guard.write_in_flight = true;
    }
}

/// Record the authoritative classification of a completed lane attempt.
pub(crate) fn record_delivery_progress(cell: &DeliveryProgressCell, progress: DeliveryProgress) {
    if let Ok(mut guard) = cell.lock() {
        guard.completed = Some(progress);
        guard.write_in_flight = false;
    }
}

/// Read the progress recorded so far. `None` means no lane write was ever
/// approached, so a cancellation is provably pre-send; a write still in
/// flight classifies conservatively as `Unknown`.
pub(crate) fn observed_delivery_progress(cell: &DeliveryProgressCell) -> Option<DeliveryProgress> {
    cell.lock().ok().and_then(|guard| {
        if guard.write_in_flight {
            Some(DeliveryProgress::Unknown)
        } else {
            guard.completed
        }
    })
}

/// DataLane - Data transport channel trait
///
/// Each DataLane represents a specific transport path for data/message transmission.
/// MediaTrack uses a separate path via MediaFrameRegistry, not DataLane.
///
/// Platform-specific implementations (native WebRTC, browser WebRTC, etc.)
/// implement this trait to provide concrete transport behavior.
#[async_trait]
pub trait DataLane: Send + Sync + std::fmt::Debug {
    /// Send raw bytes (network lanes).
    ///
    /// Default implementation returns `InvalidOperation` error.
    /// Override for network-based lanes (WebRTC, WebSocket).
    async fn send(&self, _data: bytes::Bytes) -> NetworkResult<()> {
        Err(NetworkError::InvalidOperation(
            "send(bytes) not supported on this lane type".to_string(),
        ))
    }

    /// Send raw bytes and retain the delivery-progress fact for the recovery
    /// layer.
    ///
    /// The default is fail-safe: a lane that only implements `send` cannot
    /// prove where an error happened relative to the wire, so every closed-like
    /// error is classified `Unknown` — a send that may have partially entered
    /// the transport must never be auto-replayed. Structural validation and
    /// `SecureChannelNotReady` are pre-send by construction (no lane bytes
    /// were framed or written), so those cases also carry the returned
    /// `proven_pre_send` bit. A lane-specific closed/not-open check must
    /// override this method and set the bit explicitly.
    ///
    /// # Override checklist (all items are REQUIRED to report `NotAttempted` with proof)
    ///
    /// An adapter may override this method to keep route-level self-healing
    /// for typed closed/not-open errors only if, for every error it maps to
    /// `NotAttempted` plus `proven_pre_send`:
    ///
    /// 1. the state check runs **before any byte can reach the wire**
    ///    (`WebSocketDataLane`'s missing-sink arm is the reference pattern;
    ///    its post-write arm reports `Unknown`);
    /// 2. the adapter documents where that check happens in its `send`; and
    /// 3. a test pins the classification (see the `ClosedLikePreSend*` lanes
    ///    in `dest_transport_tests` and the trait-default tests in
    ///    `lane_tests`).
    /// 4. the adapter constructs the result with `failed_pre_send` or
    ///    `failed_with_proof`, rather than relying on `NotAttempted` alone.
    ///
    /// An override that cannot satisfy all four must classify `Unknown`
    /// instead.
    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        match self.send(data).await {
            Ok(()) => SendAttemptResult::submitted(),
            Err(error) => {
                // Exhaustive per-variant classification (same-crate enum,
                // no wildcard arm): adding a `NetworkError` variant breaks
                // this match, forcing an explicit delivery-progress decision
                // instead of silently inheriting `Unknown` — or worse,
                // silently joining the pre-send allowlist and enabling
                // replay of a partially written send.
                let progress = match &error {
                    // Provably pre-send by construction.
                    NetworkError::SecureChannelNotReady(_)
                    | NetworkError::InvalidArgument(_)
                    | NetworkError::InvalidOperation(_) => DeliveryProgress::NotAttempted,
                    // Transport and genuinely ambiguous failures: the write
                    // may have partially entered the wire.
                    NetworkError::ConnectionError(_)
                    | NetworkError::SignalingReplyDropped
                    | NetworkError::SignalingError(_)
                    | NetworkError::WebRtcError(_)
                    | NetworkError::ProtocolError(_)
                    | NetworkError::SerializationError(_)
                    | NetworkError::DeserializationError(_)
                    | NetworkError::TimeoutError(_)
                    | NetworkError::AuthenticationError(_)
                    | NetworkError::CredentialExpired(_)
                    | NetworkError::PermissionError(_)
                    | NetworkError::ConfigurationError(_)
                    | NetworkError::ResourceExhaustedError(_)
                    | NetworkError::NetworkUnreachableError(_)
                    | NetworkError::ServiceDiscoveryError(_)
                    | NetworkError::NatTraversalError(_)
                    | NetworkError::DataChannelError(_)
                    | NetworkError::DataChannelClosed(_)
                    | NetworkError::DataChannelNotOpen(_)
                    | NetworkError::BroadcastError(_)
                    | NetworkError::IceError(_)
                    | NetworkError::DtlsError(_)
                    | NetworkError::StunTurnError(_)
                    | NetworkError::WebSocketError(_)
                    | NetworkError::WebSocketClosed(_)
                    | NetworkError::ConnectionNotFound(_)
                    | NetworkError::ConnectionClosed(_)
                    | NetworkError::PeerConnectionClosed(_)
                    | NetworkError::NotImplemented(_)
                    | NetworkError::ChannelClosed(_)
                    | NetworkError::SendError(_)
                    | NetworkError::NoRoute(_)
                    | NetworkError::ChannelNotFound(_)
                    | NetworkError::IoError(_)
                    | NetworkError::UrlParseError(_)
                    | NetworkError::JsonError(_)
                    | NetworkError::Other(_) => DeliveryProgress::Unknown,
                };
                match progress {
                    DeliveryProgress::NotAttempted
                        if matches!(
                            &error,
                            NetworkError::SecureChannelNotReady(_)
                                | NetworkError::InvalidArgument(_)
                                | NetworkError::InvalidOperation(_)
                        ) =>
                    {
                        SendAttemptResult::failed_pre_send(error)
                    }
                    _ => SendAttemptResult::failed(error, progress),
                }
            }
        }
    }

    /// Receive raw bytes (blocking until available).
    ///
    /// Default implementation returns `InvalidOperation` error.
    /// Override for network-based lanes (WebRTC, WebSocket).
    async fn recv(&self) -> NetworkResult<bytes::Bytes> {
        Err(NetworkError::InvalidOperation(
            "recv() not supported on this lane type".to_string(),
        ))
    }

    /// Try receive raw bytes (non-blocking).
    ///
    /// Default implementation returns `InvalidOperation` error.
    /// Override for network-based lanes (WebRTC, WebSocket).
    #[allow(dead_code)]
    async fn try_recv(&self) -> NetworkResult<Option<bytes::Bytes>> {
        Err(NetworkError::InvalidOperation(
            "try_recv() not supported on this lane type".to_string(),
        ))
    }

    /// Send RpcEnvelope directly (inproc lanes, zero-copy).
    ///
    /// Default implementation returns `InvalidOperation` error.
    /// Override for inproc lanes (Mpsc).
    async fn send_envelope(&self, _envelope: actr_protocol::RpcEnvelope) -> NetworkResult<()> {
        Err(NetworkError::InvalidOperation(
            "send_envelope() not supported on this lane type".to_string(),
        ))
    }

    /// Receive RpcEnvelope directly (inproc lanes, zero-copy).
    ///
    /// Default implementation returns `InvalidOperation` error.
    /// Override for inproc lanes (Mpsc).
    async fn recv_envelope(&self) -> NetworkResult<actr_protocol::RpcEnvelope> {
        Err(NetworkError::InvalidOperation(
            "recv_envelope() not supported on this lane type".to_string(),
        ))
    }

    /// Get DataLane type name (for logging)
    #[allow(dead_code)]
    fn lane_type(&self) -> &'static str;

    /// Whether the lane's backing transport is still usable. Stateless lanes
    /// default to healthy; v0.20 lanes project their controller sendability so
    /// the secure wrapper does not hide a retired RTC generation.
    #[allow(dead_code)]
    fn is_healthy(&self) -> bool {
        true
    }
}

// ── MpscLane ──────────────────────────────────────────────────────────────────

/// Mpsc Lane - Intra-process communication (zero serialization)
///
/// Directly passes RpcEnvelope objects via tokio mpsc channels.
/// Used for Guest <-> Shell communication within a single process.
#[derive(Clone, Debug)]
pub(crate) struct MpscLane {
    /// PayloadType identifier
    #[allow(dead_code)]
    payload_type: PayloadType,

    /// Send channel (directly passes RpcEnvelope)
    tx: mpsc::Sender<actr_protocol::RpcEnvelope>,

    /// Receive channel (shared)
    rx: Arc<Mutex<mpsc::Receiver<actr_protocol::RpcEnvelope>>>,
}

impl MpscLane {
    /// Create MpscLane (accepts plain Receiver, wraps in Arc<Mutex<>>).
    /// Only used by in-crate tests; production paths build `MpscLane` via
    /// `MpscLane::new_shared`.
    #[cfg(test)]
    #[inline]
    pub(crate) fn new(
        payload_type: PayloadType,
        tx: mpsc::Sender<actr_protocol::RpcEnvelope>,
        rx: mpsc::Receiver<actr_protocol::RpcEnvelope>,
    ) -> Self {
        Self {
            payload_type,
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Create MpscLane (accepts shared Receiver)
    #[inline]
    pub(crate) fn new_shared(
        payload_type: PayloadType,
        tx: mpsc::Sender<actr_protocol::RpcEnvelope>,
        rx: Arc<Mutex<mpsc::Receiver<actr_protocol::RpcEnvelope>>>,
    ) -> Self {
        Self {
            payload_type,
            tx,
            rx,
        }
    }
}

#[async_trait]
impl DataLane for MpscLane {
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(skip_all, name = "MpscLane.send_envelope")
    )]
    async fn send_envelope(&self, envelope: actr_protocol::RpcEnvelope) -> NetworkResult<()> {
        self.tx
            .send(envelope)
            .await
            .map_err(|_| NetworkError::ChannelClosed("Mpsc channel closed".to_string()))?;

        tracing::trace!("Mpsc sent RpcEnvelope");
        Ok(())
    }

    async fn recv_envelope(&self) -> NetworkResult<actr_protocol::RpcEnvelope> {
        let mut receiver = self.rx.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| NetworkError::ChannelClosed("Mpsc channel closed".to_string()))
    }

    #[inline]
    fn lane_type(&self) -> &'static str {
        "Mpsc"
    }
}

// ── WebSocketDataLane ─────────────────────────────────────────────────────────

/// WebSocket lane for business data transmission over WebSocket transport
///
/// All PayloadTypes share the same underlying WebSocket connection,
/// distinguished by a message header.
#[derive(Clone)]
pub(crate) struct WebSocketDataLane {
    /// Shared Sink (all PayloadTypes share the same WebSocket connection)
    pub(crate) sink: WsSink,

    /// PayloadType identifier (used to add message header when sending)
    payload_type: PayloadType,

    /// Receive channel (independent, routed by dispatcher, uses Bytes for zero-copy)
    rx: Arc<Mutex<mpsc::Receiver<bytes::Bytes>>>,
}

impl std::fmt::Debug for WebSocketDataLane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WebSocketDataLane(type={:?})", self.payload_type)
    }
}

impl WebSocketDataLane {
    /// Create WebSocket DataLane
    #[inline]
    pub(crate) fn new(
        sink: WsSink,
        payload_type: PayloadType,
        rx: mpsc::Receiver<bytes::Bytes>,
    ) -> Self {
        Self {
            sink,
            payload_type,
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

/// Map the result of a WebSocket sink write to delivery progress.
///
/// Once `SinkExt::send` has been invoked, a returned error cannot prove that
/// zero bytes reached the peer: the sink may have flushed part of the frame
/// before the underlying connection failed.  Keep that distinction at the
/// adapter boundary so route-level recovery cannot replay the payload.
fn map_websocket_send_result(
    result: Result<(), tokio_tungstenite::tungstenite::Error>,
) -> SendAttemptResult {
    match result {
        Ok(()) => SendAttemptResult::submitted(),
        Err(error) => {
            let error = if is_tungstenite_closed(&error) {
                NetworkError::WebSocketClosed(error.to_string())
            } else {
                NetworkError::SendError(format!("WebSocket send failed: {error}"))
            };
            SendAttemptResult::failed(error, DeliveryProgress::Unknown)
        }
    }
}

#[async_trait]
impl DataLane for WebSocketDataLane {
    async fn send(&self, data: bytes::Bytes) -> NetworkResult<()> {
        self.send_with_progress(data).await.into_result()
    }

    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        // 1. Encapsulate message (add PayloadType header)
        let mut buf = Vec::with_capacity(5 + data.len());

        // 1 byte: payload_type
        buf.push(self.payload_type as u8);

        // 4 bytes: data length (big-endian)
        let len = data.len() as u32;
        buf.extend_from_slice(&len.to_be_bytes());

        // N bytes: data (copy from Bytes to Vec)
        buf.extend_from_slice(&data);

        // 2. Send to WebSocket
        let mut sink_opt = self.sink.lock().await;
        if let Some(s) = sink_opt.as_mut() {
            let attempt = map_websocket_send_result(s.send(WsMessage::Binary(buf.into())).await);
            if attempt.result.is_ok() {
                tracing::trace!(
                    "WebSocket sent {} bytes (type={:?})",
                    data.len(),
                    self.payload_type
                );
            }
            attempt
        } else {
            SendAttemptResult::failed_pre_send(NetworkError::WebSocketClosed(
                "WebSocket not connected".to_string(),
            ))
        }
    }

    async fn recv(&self) -> NetworkResult<bytes::Bytes> {
        let mut receiver = self.rx.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| NetworkError::ChannelClosed("DataLane receiver closed".to_string()))
    }

    async fn try_recv(&self) -> NetworkResult<Option<bytes::Bytes>> {
        let mut receiver = self.rx.lock().await;
        match receiver.try_recv() {
            Ok(data) => Ok(Some(data)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(NetworkError::ChannelClosed(
                "Lane receiver closed".to_string(),
            )),
        }
    }

    #[inline]
    fn lane_type(&self) -> &'static str {
        "WebSocket"
    }
}

#[cfg(test)]
#[path = "lane_tests.rs"]
mod tests;
