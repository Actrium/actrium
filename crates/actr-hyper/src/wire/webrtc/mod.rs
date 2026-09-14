//! WebRTC subsystem: shared signaling client and native RTC implementation.
//!
//! `rtc` owns peer connection management, per-peer controllers, and sessions.

mod signaling;
#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod test_runtime;
pub(crate) mod trace;
// The `allow(dead_code)` keeps the RTC surface's currently-unused items
// (observation accessors, convenience constructors) out of the warning set
// until the S7B-8 cleanup pass trims them deliberately.
#[allow(dead_code)]
pub(crate) mod rtc;

// Re-export public WebRTC surface from this module boundary; internal hook
// plumbing stays crate-private except under test-utils, where integration
// tests can install recorders without standing up a full node.
pub use actr_config::WebRtcConfig;
#[cfg(not(feature = "test-utils"))]
pub(crate) use signaling::WebSocketSignalingClient;
pub use signaling::{
    AuthConfig, AuthType, ConnectionState, DisconnectReason, InboundQueueConfig, ReconnectConfig,
    SignalingClient, SignalingConfig, SignalingEvent, SignalingStats,
};
#[cfg(not(feature = "test-utils"))]
pub(crate) use signaling::{HookCallback, HookEvent};
#[cfg(feature = "test-utils")]
pub use signaling::{HookCallback, HookEvent, WebSocketSignalingClient};
