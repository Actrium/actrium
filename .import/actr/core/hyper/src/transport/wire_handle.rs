//! Wire Handle - Trait-based abstraction for Wire layer connections
//!
//! WireHandle trait provides a unified interface for different wire connection
//! types (WebRTC, WebSocket, etc.). Platform-specific implementations provide
//! the concrete connection behavior.

use super::error::NetworkResult;
use super::lane::DataLane;
use super::secure_channel::SecureChannelState;
use super::wire_pool::ConnType;
use actr_protocol::{ActrId, PayloadType};
use async_trait::async_trait;
use std::sync::Arc;

/// Wire identity — distinguishes connection sessions even for the same peer.
///
/// Used for stale detection: if a close event arrives with a session_id that
/// no longer matches the active wire, the event is stale and should be ignored.
///
/// WebSocket connections carry an identity too so the same compare-and-swap
/// machinery (`mark_connection_closed_if_same`) applies to them — otherwise a
/// passively-closed WebSocket stays in the ready set forever on the tell/stream
/// paths. The `session_id` is a monotonically increasing generation minted by
/// the wire builder, so a stale close event for a replaced wire never matches
/// the fresh one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireIdentity {
    WebRtc { peer_id: ActrId, session_id: u64 },
    WebSocket { peer_id: ActrId, session_id: u64 },
}

/// Stable identity of one logical WebRTC transport attempt.
///
/// Unlike [`WireIdentity`], this identity survives PC generation replacement
/// during Hard Recovery. The controller lifecycle id prevents an event from a
/// removed controller matching its replacement; `transport_epoch` prevents a
/// delayed terminal event from an earlier attempt matching a later `Connect`
/// on the same long-lived controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcTransportLifecycle {
    pub peer_id: ActrId,
    pub controller_lifecycle_id: u64,
    pub transport_epoch: u64,
}

/// WireHandle - Unified interface for Wire layer connections
///
/// # Design Philosophy
/// - Uses trait objects for cross-platform extensibility
/// - Supports connection priority comparison (WebRTC > WebSocket)
/// - Platform-specific implementations (native, web) implement this trait
#[async_trait]
pub trait WireHandle: Send + Sync + std::fmt::Debug {
    /// Get connection type
    fn connection_type(&self) -> ConnType;

    /// Connection priority (higher number = higher priority)
    #[allow(dead_code)]
    fn priority(&self) -> u8;

    /// Establish connection.
    ///
    /// Contract (RFC-0563): returns only when the wire's
    /// `secure_channel_state()` is sendable. In phase 2 that means
    /// `LegacyBypass` (transport ready, no crypto); phase 3 will drive
    /// `New → Handshaking → Verified` here once the real handshake lands.
    async fn connect(&self) -> NetworkResult<()>;

    /// Check if connected
    #[allow(dead_code)]
    fn is_connected(&self) -> bool;

    /// Close connection
    async fn close(&self) -> NetworkResult<()>;

    /// Get or create DataLane (with caching)
    async fn get_lane(&self, payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>>;

    /// Invalidate cached lane (no-op by default).
    ///
    /// Used when the underlying transport (e.g. DataChannel) has closed
    /// and needs to be recreated on next `get_lane` call.
    async fn invalidate_lane(&self, _payload_type: PayloadType) {}

    /// Connection identity for stale detection (default: None).
    ///
    /// WebRTC connections return `WireIdentity::WebRtc { peer_id, session_id }`
    /// so that upper layers can detect whether a close event belongs to the
    /// current active session or a stale one. WebSocket connections return
    /// `WireIdentity::WebSocket { peer_id, session_id }` for the same reason.
    /// Implementations without a comparable session concept return `None`.
    fn identity(&self) -> Option<WireIdentity> {
        None
    }

    /// Secure-channel state. Default `LegacyBypass` — an honest "transport
    /// ready, no crypto" label — so unwrapped wires (tests, web bindings,
    /// future transports) are sendable **without** claiming cryptographic
    /// verification. Physical `WireHandle` impls that own a [`SecureCell`]
    /// override this with their real state. See RFC-0563.
    ///
    /// [`SecureCell`]: super::secure_channel::SecureCell
    fn secure_channel_state(&self) -> SecureChannelState {
        SecureChannelState::LegacyBypass
    }

    /// Current secure epoch (monotonic, bumps on every validated transition).
    /// Default `0` for wires without a [`SecureCell`]. `WirePool` uses this to
    /// detect that a state change has occurred.
    ///
    /// [`SecureCell`]: super::secure_channel::SecureCell
    fn secure_epoch(&self) -> u64 {
        0
    }

    /// Wait until the secure epoch advances past `observed_epoch`, signalling
    /// that the secure state may have changed (e.g. `Handshaking → Verified`,
    /// rekey, or a terminal transition). Default: never resolves — unwrapped
    /// wires have no transitions to observe, so a `WirePool` listener parked on
    /// this simply stays inert. Physical impls override to delegate to their
    /// [`SecureCell`].
    ///
    /// [`SecureCell`]: super::secure_channel::SecureCell
    async fn await_secure_state_change(&self, _observed_epoch: u64) {
        std::future::pending::<()>().await;
    }

    /// Declare the secure session terminally `Failed`. Called by `WirePool`
    /// **only after connect retries are exhausted** — a single failed attempt
    /// must stay non-terminal so a later retry on the same object can still
    /// become sendable. Default no-op for wires without a [`SecureCell`].
    ///
    /// [`SecureCell`]: super::secure_channel::SecureCell
    fn mark_secure_failed(&self) {}

    /// Stable WebRTC connection-flight identity used for terminal cache
    /// retirement. Non-WebRTC handles return `None`.
    fn webrtc_transport_lifecycle(&self) -> Option<WebRtcTransportLifecycle> {
        None
    }
}

/// Wire connection status
#[derive(Debug)]
pub enum WireStatus {
    /// Connecting
    Connecting(Arc<dyn WireHandle>),

    /// Connection ready
    Ready(Arc<dyn WireHandle>),

    /// Connection failed
    Failed(Arc<dyn WireHandle>),
}

impl Clone for WireStatus {
    fn clone(&self) -> Self {
        match self {
            WireStatus::Connecting(handle) => WireStatus::Connecting(Arc::clone(handle)),
            WireStatus::Ready(handle) => WireStatus::Ready(Arc::clone(handle)),
            WireStatus::Failed(handle) => WireStatus::Failed(Arc::clone(handle)),
        }
    }
}
