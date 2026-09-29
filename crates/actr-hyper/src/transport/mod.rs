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
pub(crate) use lane::notify_webrtc_fragment_sent_for_test;
#[cfg(feature = "test-utils")]
pub use lane::{DataLane, DeliveryProgress, SendAttemptResult};
#[cfg(not(feature = "test-utils"))]
pub(crate) use lane::{DataLane, DeliveryProgress, SendAttemptResult};
pub(crate) use lane::{
    DeliveryProgressCell, MpscLane, WebSocketDataLane, WsSink, new_delivery_progress_cell,
    observed_delivery_progress,
};
#[cfg(feature = "test-utils")]
pub use lane::{
    WebRtcFragmentSendEvent, WebRtcFragmentSendHook, WebRtcFragmentSendHookGuard,
    install_webrtc_fragment_send_hook_for_test,
};
// Fragment codec for the RTC DataLane's DataChannel wire format. It lives in
// the transport layer because both the send boundary and the inbound lane
// pump use the same framing and reassembly rules.
pub(crate) use lane::{
    DC_MAX_PAYLOAD_SIZE, FRAGMENT_HEADER_SIZE, REASSEMBLY_TTL, ReassemblyBuffer, ReassemblyLimits,
    decode_fragment_header, encode_fragment_header,
};
pub(crate) use route_table::{PayloadTypeExt, RetryPolicy};

/// Default send budgets for fire-and-forget RPC and DataStream operations.
///
/// These operations do not carry a caller-provided RPC timeout in their wire
/// envelope.  The budget is still created once at the API boundary and then
/// passed down as an absolute deadline, so readiness, lane acquisition, and
/// the actual send share one bounded operation.
pub(crate) const TELL_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(13);
pub(crate) const DATA_CHUNK_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

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
pub(crate) use peer_transport::{DestTransportRef, PeerSendAttempt, SentTransportSlot};
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

/// One caller-owned absolute deadline shared by every phase of an outbound
/// operation.
///
/// The value is intentionally crate-private: the public API continues to
/// expose the existing Call/Tell/DataStream methods, while each lower layer
/// consumes the same monotonic deadline rather than starting another full
/// relative timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RpcDeadline(tokio::time::Instant);

impl RpcDeadline {
    /// Start a deadline from a validated RPC wire timeout.
    pub(crate) fn from_timeout_ms(timeout_ms: i64) -> actr_protocol::ActorResult<Self> {
        let timeout = validate_rpc_timeout_ms(timeout_ms)?;
        Ok(Self(tokio::time::Instant::now() + timeout))
    }

    /// Start a deadline for an operation whose wire envelope has no timeout
    /// field (Tell and DataStream).
    pub(crate) fn from_duration(timeout: std::time::Duration) -> Self {
        Self(tokio::time::Instant::now() + timeout)
    }

    pub(crate) fn instant(self) -> tokio::time::Instant {
        self.0
    }

    /// Guard admission points against a deadline that elapsed between two
    /// asynchronous stages.  This prevents a lower layer from registering or
    /// sending work during the one final poll of an already-expired timeout.
    pub(crate) fn ensure_remaining(self) -> actr_protocol::ActorResult<()> {
        if tokio::time::Instant::now() >= self.0 {
            Err(actr_protocol::ActrError::TimedOut)
        } else {
            Ok(())
        }
    }

    /// Guard an admission point that must keep at least `min` of budget after
    /// admission.  Committing work (pending registration + send) with less
    /// than `min` remaining guarantees the operation fails mid-flight while
    /// its bytes may already be delivered — refuse before anything is
    /// committed so the caller may retry with a fresh budget.
    pub(crate) fn ensure_min_remaining(
        self,
        min: std::time::Duration,
    ) -> actr_protocol::ActorResult<()> {
        if self
            .0
            .saturating_duration_since(tokio::time::Instant::now())
            < min
        {
            Err(actr_protocol::ActrError::TimedOut)
        } else {
            Ok(())
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::RpcDeadline;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn rpc_deadline_is_shared_across_stages() {
        let deadline = RpcDeadline::from_duration(Duration::from_secs(1));
        let instant = deadline.instant();

        tokio::time::advance(Duration::from_millis(400)).await;
        deadline
            .ensure_remaining()
            .expect("the shared deadline should still be active");
        assert_eq!(deadline.instant(), instant);

        // A later stage observes the same absolute instant rather than
        // receiving another full one-second timeout.
        tokio::time::advance(Duration::from_millis(600)).await;
        assert!(matches!(
            deadline.ensure_remaining(),
            Err(actr_protocol::ActrError::TimedOut)
        ));
    }
}
