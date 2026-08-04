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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignalingServerConfig {
    /// WebSocket path
    pub ws_path: String,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

/// Rate limiting configuration
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RateLimitConfig {
    /// Connection rate limiting configuration
    #[serde(default)]
    pub connection: ConnectionRateLimit,

    /// Message rate limiting configuration
    #[serde(default)]
    pub message: MessageRateLimit,
}

/// Connection rate limiting configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
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

    /// AIS client configuration (optional, for Credential refresh)
    ///
    /// If not configured but AIS is needed, it will auto-detect the local AIS service:
    /// - If the AIS service is enabled (ENABLE_AIS bit is set), use localhost:AIS_PORT
    /// - Otherwise return None
    #[serde(default)]
    pub ais: Option<AisClientConfig>,
}

/// AIS client configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AisClientConfig {
    /// AIS service endpoint URL
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
            // Auto-generate client configuration pointing to local Signer
            // Signer gRPC reuses the instance's main HTTP/HTTPS port
            let http_cfg = global_config.bind.http.as_ref();
            let port = http_cfg.map(|h| h.port).unwrap_or(8080);
            let use_tls = http_cfg.is_some_and(|h| h.is_tls());
            let protocol = if use_tls { "https" } else { "http" };
            let tls_domain = if use_tls {
                http_cfg.map(|h| h.domain_name.clone())
            } else {
                None
            };

            return Some(SignerClientConfig {
                endpoint: format!("{protocol}://127.0.0.1:{port}"),
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

    /// Get the AIS client configuration
    ///
    /// Supports smart defaults:
    /// 1. If dependencies.ais is explicitly configured, return it directly
    /// 2. If the local AIS service is enabled, return configuration pointing to the local AIS
    /// 3. Otherwise return None
    pub fn get_ais_client_config(
        &self,
        global_config: &super::ActrixConfig,
    ) -> Option<AisClientConfig> {
        // Prefer explicit configuration
        if let Some(ref ais_config) = self.dependencies.ais {
            return Some(ais_config.clone());
        }

        // Fallback: check if local AIS service is enabled
        if global_config.is_ais_enabled() && global_config.services.ais.is_some() {
            // Auto-generate client configuration pointing to local AIS
            // AIS as an HTTP router service shares the same HTTP/HTTPS port
            let http_cfg = global_config.bind.http.as_ref();
            let port = http_cfg.map(|h| h.port).unwrap_or(8080);
            let protocol = if http_cfg.is_some_and(|h| h.is_tls()) {
                "https"
            } else {
                "http"
            };

            return Some(AisClientConfig {
                endpoint: format!("{protocol}://127.0.0.1:{port}"),
                timeout_seconds: 30,
            });
        }

        None
    }
}
