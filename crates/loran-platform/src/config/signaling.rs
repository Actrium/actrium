//! Signaling service configuration

use crate::config::signer::SignerClientConfig;
use serde::{Deserialize, Serialize};

/// Signaling service configuration
///
/// Service enable/disable is controlled by the bitmask in ActrixConfig.enable.
/// The ENABLE_SIGNALING bit (bit 0) must be set to enable this service.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SignalingConfig {
    /// Signaling server configuration
    #[serde(default)]
    pub server: SignalingServerConfig,

    /// Signaling dependency service configuration
    #[serde(default)]
    pub dependencies: SignalingDependencies,
}

/// Signaling server configuration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SignalingServerConfig {
    /// WebSocket path
    pub ws_path: String,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,

    /// Per-client egress queue bound configuration (issue #351)
    #[serde(default)]
    pub egress: EgressQueueConfig,
}

/// Per-client outbound (egress) queue bound configuration (issue #351)
///
/// Bound the number of WebSocket messages and bytes the server queues for a single client, and apply enqueue/write deadlines for slow-consuming/non-reading
/// clients; on timeout the connection is closed and removed (healthy clients are unaffected).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EgressQueueConfig {
    /// Max number of data (Binary) messages queued per client (message-count bound)
    #[serde(default = "default_egress_max_messages")]
    pub max_messages: u32,

    /// Max number of encoded bytes queued per client (byte budget)
    #[serde(default = "default_egress_max_bytes")]
    pub max_bytes: u32,

    /// Max number of control messages (Ping/Pong/Close) queued per client
    #[serde(default = "default_egress_max_control_messages")]
    pub max_control_messages: u32,

    /// Max size of a single inbound signaling frame (rejected before protobuf decode/alloc when exceeded)
    #[serde(default = "default_egress_max_frame_bytes")]
    pub max_frame_bytes: u32,

    /// Deadline (ms) for enqueueing onto a full client queue; on timeout the client is treated as slow-consuming and disconnected
    #[serde(default = "default_egress_enqueue_timeout_ms")]
    pub enqueue_timeout_ms: u32,

    /// Deadline (ms) for a single WebSocket write; on timeout the connection is closed
    #[serde(default = "default_egress_write_timeout_ms")]
    pub write_timeout_ms: u32,

    /// Per-target aggregate message rate (messages/sec, across all senders)
    #[serde(default = "default_target_rate_per_second")]
    pub target_rate_per_second: u32,

    /// Per-target aggregate byte budget (bytes/sec, across all senders)
    #[serde(default = "default_target_bytes_per_second")]
    pub target_bytes_per_second: u32,
}

fn default_egress_max_messages() -> u32 {
    256
}

fn default_egress_max_bytes() -> u32 {
    1 << 20 // 1 MiB
}

fn default_egress_max_control_messages() -> u32 {
    32
}

fn default_egress_max_frame_bytes() -> u32 {
    256 << 10 // 256 KiB
}

fn default_egress_enqueue_timeout_ms() -> u32 {
    2_000
}

fn default_egress_write_timeout_ms() -> u32 {
    5_000
}

fn default_target_rate_per_second() -> u32 {
    200
}

fn default_target_bytes_per_second() -> u32 {
    1 << 20 // 1 MiB/s
}

impl Default for EgressQueueConfig {
    fn default() -> Self {
        Self {
            max_messages: default_egress_max_messages(),
            max_bytes: default_egress_max_bytes(),
            max_control_messages: default_egress_max_control_messages(),
            max_frame_bytes: default_egress_max_frame_bytes(),
            enqueue_timeout_ms: default_egress_enqueue_timeout_ms(),
            write_timeout_ms: default_egress_write_timeout_ms(),
            target_rate_per_second: default_target_rate_per_second(),
            target_bytes_per_second: default_target_bytes_per_second(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct RateLimitConfig {
    /// Connection rate limiting configuration
    #[serde(default)]
    pub connection: ConnectionRateLimit,

    /// Message rate limiting configuration
    #[serde(default)]
    pub message: MessageRateLimit,
}

/// Connection rate limiting configuration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ConnectionRateLimit {
    /// Whether to enable connection rate limiting
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Allowed new connections per minute
    #[serde(default = "default_connections_per_minute")]
    pub per_minute: u32,

    /// Burst allowance for connections
    #[serde(default = "default_connection_burst")]
    pub burst_size: u32,

    /// Maximum concurrent connections per IP
    #[serde(default = "default_max_concurrent_connections")]
    pub max_concurrent_per_ip: u32,
}

/// Message rate limiting configuration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MessageRateLimit {
    /// Whether to enable message rate limiting
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Allowed messages per second
    #[serde(default = "default_messages_per_second")]
    pub per_second: u32,

    /// Burst allowance for messages
    #[serde(default = "default_message_burst")]
    pub burst_size: u32,
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_connections_per_minute() -> u32 {
    5
}

fn default_connection_burst() -> u32 {
    10
}

fn default_max_concurrent_connections() -> u32 {
    100
}

fn default_messages_per_second() -> u32 {
    10
}

fn default_message_burst() -> u32 {
    50
}

/// External services that Signaling depends on
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SignalingDependencies {
    /// Signer client configuration (optional, needed if encryption is required)
    ///
    /// If not configured but Signer is needed, it will auto-detect the local Signer service:
    /// - If the Signer service is enabled (ENABLE_SIGNER bit is set), use localhost:SIGNER_PORT
    /// - Otherwise return None (Signaling can operate without Signer)
    #[serde(default)]
    pub signer: Option<SignerClientConfig>,

    /// Issuer client configuration (optional, for Credential refresh)
    ///
    /// If not configured but Issuer is needed, it will auto-detect the local Issuer service:
    /// - If the Issuer service is enabled (ENABLE_ISSUER bit is set), use localhost:ISSUER_PORT
    /// - Otherwise return None
    #[serde(default)]
    pub issuer: Option<IssuerClientConfig>,
}

/// Issuer client configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IssuerClientConfig {
    /// Issuer service endpoint URL
    pub endpoint: String,
    /// Request timeout (seconds)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 {
    30
}

impl Default for SignalingServerConfig {
    fn default() -> Self {
        Self {
            ws_path: "/signaling".to_string(),
            rate_limit: RateLimitConfig::default(),
            egress: EgressQueueConfig::default(),
        }
    }
}

impl Default for ConnectionRateLimit {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            per_minute: default_connections_per_minute(),
            burst_size: default_connection_burst(),
            max_concurrent_per_ip: default_max_concurrent_connections(),
        }
    }
}

impl Default for MessageRateLimit {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            per_second: default_messages_per_second(),
            burst_size: default_message_burst(),
        }
    }
}

impl SignalingConfig {
    /// Get the Signer client configuration
    ///
    /// Supports smart defaults:
    /// 1. If dependencies.signer is explicitly configured, return it directly
    /// 2. If the local Signer service is enabled, return configuration pointing to the local Signer
    /// 3. Otherwise return None (Signaling can operate without Signer)
    pub fn get_signer_client_config(
        &self,
        global_config: &super::ActrixConfig,
    ) -> Option<SignerClientConfig> {
        // Prefer explicit configuration
        if let Some(ref signer_config) = self.dependencies.signer {
            return Some(signer_config.clone());
        }

        // Fallback: check if local Signer service is enabled
        if global_config.is_signer_enabled() && global_config.services.signer.is_some() {
            // SGN-02: the Signer gRPC surface runs on its own dedicated listener,
            // not the shared HTTP port. Dial the resolved Signer endpoint.
            let use_tls = global_config.signer_listener_tls();
            let tls_domain = if use_tls {
                Some(global_config.signer_tls_domain())
            } else {
                None
            };

            return Some(SignerClientConfig {
                endpoint: global_config.signer_client_endpoint(),
                timeout_seconds: 30,
                enable_tls: use_tls,
                tls_domain,
                ca_cert: None,
                client_cert: None,
                client_key: None,
            });
        }

        None
    }

    /// Get the Issuer client configuration
    ///
    /// Supports smart defaults:
    /// 1. If dependencies.issuer is explicitly configured, return it directly
    /// 2. If the local Issuer service is enabled, return configuration pointing to the local Issuer
    /// 3. Otherwise return None
    pub fn get_issuer_client_config(
        &self,
        global_config: &super::ActrixConfig,
    ) -> Option<IssuerClientConfig> {
        // Prefer explicit configuration
        if let Some(ref issuer_config) = self.dependencies.issuer {
            return Some(issuer_config.clone());
        }

        // Fallback: check if local Issuer service is enabled
        if global_config.is_issuer_enabled() && global_config.services.issuer.is_some() {
            // Auto-generate client configuration pointing to local Issuer
            // Issuer as an HTTP router service shares the same HTTP/HTTPS port
            let http_cfg = global_config.bind.http.as_ref();
            let port = http_cfg.map(|h| h.port).unwrap_or(8080);
            let protocol = if http_cfg.is_some_and(|h| h.is_tls()) {
                "https"
            } else {
                "http"
            };

            return Some(IssuerClientConfig {
                endpoint: format!("{protocol}://127.0.0.1:{port}"),
                timeout_seconds: 30,
            });
        }

        None
    }
}
