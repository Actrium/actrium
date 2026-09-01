//! Runtime strategy for the v0.20 path (RFC §"Runtime strategy").
//!
//! The RFC's default policy:
//!
//! * Android/iOS use the Tokio runtime driver and, by default, do **not** enable
//!   a dedicated reactor pool (extra threads cost energy on mobile).
//! * Server / high-connection deployments MAY enable a shared, bounded reactor
//!   pool so a peer-connection driver is pinned to one reactor thread instead of
//!   migrating across the async runtime's worker pool (webrtc-rs issue #101).
//! * Reactor-pool sizing is **process-level** configuration: webrtc-rs builds
//!   the pool once, lazily, on first use, and only the first value takes effect,
//!   so Actr must not set conflicting sizes from different instances.
//!
//! The factory applies this profile explicitly to every fresh generation; T2
//! validation covers the runtime/task/socket boundary.

use std::sync::Arc;

use webrtc::runtime::{Runtime, default_runtime};

/// Per-process runtime policy for v0.20 peer connections.
///
/// Clone-cheap; the factory reads the knobs it needs at build time.
#[derive(Debug, Clone)]
pub(crate) struct RtcRuntimeConfig {
    /// Pin each connection's driver to the shared bounded reactor pool
    /// (`with_dedicated_reactor_thread`). Default `false` (mobile-friendly).
    dedicated_reactor: bool,
    /// Size of the shared reactor pool (`with_reactor_pool_size`). Only used
    /// when `dedicated_reactor` is true. `0` lets webrtc-rs clamp to a single
    /// shared reactor thread.
    reactor_pool_size: usize,
    /// Validated stack-wide buffer policy. Keeping this next to the runtime
    /// knobs makes the Factory apply the exact same values that the Manager and
    /// Session data plane enforce.
    buffer_config: super::config::RtcBufferConfig,
    /// Test/debug-only runtime override (Stage 5 item 10): the fault probe
    /// (`RuntimeFaultProbe`) wraps the production runtime and is injected here.
    runtime_override: Option<Arc<dyn Runtime>>,
}

impl RtcRuntimeConfig {
    /// Desktop/server default: Tokio driver on the general runtime (no
    /// dedicated reactor), a browser-like 16 MiB per-channel send-buffer limit
    /// (the value webrtc-rs docs cite as Chromium's `MaxSendQueueSize`), and the
    /// default 1 MiB SCTP receive window.
    pub(crate) fn desktop_default() -> Self {
        Self {
            dedicated_reactor: false,
            reactor_pool_size: 0,
            buffer_config: super::config::RtcBufferConfig::default(),
            runtime_override: None,
        }
    }

    /// Mobile default (Android/iOS): identical to desktop — the
    /// dedicated reactor pool stays off to avoid extra threads and energy use.
    /// It can diverge only after platform profiling justifies another budget.
    #[allow(dead_code)]
    pub(crate) fn mobile_default() -> Self {
        Self::desktop_default()
    }

    /// Opt into the shared bounded reactor pool (server/high-connection).
    #[allow(dead_code)]
    pub(crate) fn with_dedicated_reactor(mut self, enabled: bool, pool_size: usize) -> Self {
        self.dedicated_reactor = enabled;
        self.reactor_pool_size = pool_size;
        self
    }

    /// Override the per-channel send-buffer limit.
    pub(crate) fn with_data_channel_send_buffer_limit(mut self, bytes: usize) -> Self {
        self.buffer_config.per_channel_send_bytes = bytes;
        self
    }

    /// Override the SCTP receive window.
    #[allow(dead_code)]
    pub(crate) fn with_sctp_receive_buffer_size(mut self, bytes: u32) -> Self {
        self.buffer_config.sctp_receive_window_bytes = bytes;
        self
    }

    /// Apply the complete, shared RTC buffer policy.
    pub(crate) fn with_buffer_config(mut self, config: super::config::RtcBufferConfig) -> Self {
        self.buffer_config = config;
        self
    }

    /// The compiled-in runtime. With the `runtime-tokio` feature (the only
    /// runtime this crate enables) this is the Tokio driver.
    ///
    /// The returned runtime only *spawns* onto / *wraps sockets for* the ambient
    /// Tokio runtime, so `build()` must be awaited from inside a Tokio context.
    pub(crate) fn runtime(&self) -> Arc<dyn Runtime> {
        if let Some(runtime) = &self.runtime_override {
            return runtime.clone();
        }
        default_runtime().expect("webrtc-v020 is built with the runtime-tokio feature")
    }

    /// Inject a custom runtime (Stage 5 item 10: the fault probe wraps the
    /// production runtime). Test / explicit debug builds only — the RFC
    /// forbids runtime fault injection in production configuration.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn with_runtime_override(mut self, runtime: Arc<dyn Runtime>) -> Self {
        self.runtime_override = Some(runtime);
        self
    }

    pub(crate) fn dedicated_reactor(&self) -> bool {
        self.dedicated_reactor
    }

    pub(crate) fn reactor_pool_size(&self) -> usize {
        self.reactor_pool_size
    }

    pub(crate) fn data_channel_send_buffer_limit(&self) -> usize {
        self.buffer_config.per_channel_send_bytes
    }

    pub(crate) fn sctp_receive_buffer_size(&self) -> u32 {
        self.buffer_config.sctp_receive_window_bytes
    }

    pub(crate) fn buffer_config(&self) -> &super::config::RtcBufferConfig {
        &self.buffer_config
    }

    pub(crate) fn validate(&self) -> Result<(), super::config::RtcConfigError> {
        self.buffer_config.validate()
    }
}
