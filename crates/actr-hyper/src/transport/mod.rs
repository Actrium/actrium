//! Transport Layer 1: Transport layer
//!
//! Core Lane abstraction and transport management:
//! - Lane: Physical embodiment of PayloadType, unified bidirectional channel abstraction
//! - HostTransport: Intra-process transport management (Guest <-> Shell)
//! - PeerTransport: Cross-process transport management (WebRTC + WebSocket)
//! - WireHandle: Unified handle for Wire layer components
//! - WirePool: Wire connection pool manager (strategy layer)
//! - WireBuilder: Wire layer component builder

mod backoff;
pub(crate) mod correlation;
mod dest_transport;
mod error;
mod host_transport;
pub(crate) mod lane;
mod peer_transport;
mod route_table;
pub(crate) mod secure_channel;
mod wire_builder;
mod wire_handle;
pub(crate) mod wire_pool;

// Re-export Dest from actr-framework (unified API layer).
pub use actr_framework::Dest;

// Submodule-internal types (lanes, wire pool, sessions, dest_transport,
// connection events broadcasters) stay reachable via their module paths
// without duplicating re-exports here.

// Secure-channel seam (RFC-0563): state, physical-session cell, and lane gate.
// `SecureChannelState` is `pub` (returned by the `WireHandle::secure_channel_state`
// trait method); the cell and lane decorator are `pub(crate)`.
pub(crate) use secure_channel::{SecureCell, SecureChannelState, SecureDataLane};

// DataLane core abstraction (trait kept pub for sw-host/peer_transport).
#[cfg(feature = "test-utils")]
pub use lane::DataLane;
#[cfg(not(feature = "test-utils"))]
pub(crate) use lane::DataLane;
#[cfg(feature = "test-utils")]
pub(crate) use lane::notify_webrtc_fragment_sent_for_test;
pub(crate) use lane::{MpscLane, WebSocketDataLane, WsSink};
#[cfg(feature = "test-utils")]
pub use lane::{
    WebRtcFragmentSendEvent, WebRtcFragmentSendHook, WebRtcFragmentSendHookGuard,
    install_webrtc_fragment_send_hook_for_test,
};
// Fragment codec: shared between 0.14 WebRtcDataLane and v0.20 V020DataLane
// so both produce byte-identical wire format on the DataChannel. Only
// re-exported when the v0.20 feature is active — 0.14 uses them internally
// within `lane.rs` and does not need the re-export.
#[cfg(feature = "webrtc-v020")]
pub(crate) use lane::{
    DC_MAX_PAYLOAD_SIZE, FRAGMENT_HEADER_SIZE, ReassemblyBuffer, ReassemblyLimits,
    decode_fragment_header, encode_fragment_header,
};
pub(crate) use route_table::{PayloadTypeExt, RetryPolicy};

// ConnType leaks through the public `WireHandle::connection_type` method,
// so it must stay pub even though the `wire_pool` module itself is private.
#[cfg(feature = "test-utils")]
pub use wire_pool::ConnType;
#[cfg(not(feature = "test-utils"))]
pub(crate) use wire_pool::ConnType;

// Transport management
#[cfg(feature = "test-utils")]
pub use host_transport::HostTransport;
#[cfg(not(feature = "test-utils"))]
pub(crate) use host_transport::HostTransport;
#[cfg(not(feature = "test-utils"))]
pub(crate) use peer_transport::PeerTransport;
pub(crate) use peer_transport::{DestTransportRef, SentTransportSlot};
#[cfg(feature = "test-utils")]
pub use peer_transport::{PeerTransport, WireBuilder};

// Wire layer management
#[cfg(feature = "test-utils")]
pub use wire_builder::{DefaultWireBuilder, DefaultWireBuilderConfig};
#[cfg(not(feature = "test-utils"))]
pub(crate) use wire_builder::{DefaultWireBuilder, DefaultWireBuilderConfig};
#[cfg(feature = "test-utils")]
pub use wire_handle::{WebRtcTransportLifecycle, WireHandle, WireIdentity};
#[cfg(not(feature = "test-utils"))]
pub(crate) use wire_handle::{WebRtcTransportLifecycle, WireHandle, WireIdentity};

// Error types
pub use error::{AuthVerdict, NetworkError, NetworkResult};

// Retry and backoff strategies
pub use backoff::ExponentialBackoff;

// Connection events are re-exported at the transport module boundary; the
// broadcaster stays crate-internal.

// Connection session

/// Validate a caller-supplied RPC timeout at a call entry point and convert
/// it into a [`std::time::Duration`].
///
/// The wire contract (see `package.proto`) requires `timeout_ms > 0` iff
/// `direction == DIRECTION_REQUEST`. Rejecting non-positive values here also
/// prevents the `Duration::from_millis(timeout_ms as u64)` sign wrap that
/// would silently turn a negative timeout into an effectively infinite one.
///
/// Sender-side only: this helper guards call entry points
/// (`RuntimeContext::call_raw`, typed call construction,
/// `PeerGate::send_request_with_type`, `HostTransport::send_request`).
/// Receivers stay permissive — an inbound REQUEST carrying `timeout_ms == 0`
/// from a buggy sender is still dispatched and answered.
pub(crate) fn validate_rpc_timeout_ms(
    timeout_ms: i64,
) -> actr_protocol::ActorResult<std::time::Duration> {
    if timeout_ms <= 0 {
        return Err(actr_protocol::ActrError::InvalidArgument(format!(
            "RPC call timeout_ms must be > 0, got {timeout_ms} (fire-and-forget messaging must use tell, not a zero timeout)"
        )));
    }
    Ok(std::time::Duration::from_millis(timeout_ms as u64))
}
