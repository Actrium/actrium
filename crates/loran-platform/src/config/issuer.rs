//! Issuer (Actor Identity Service) config

use crate::config::signer::SignerClientConfig;
use serde::{Deserialize, Serialize};

/// Issuer service config
///
/// Service enable/disable is controlled by the bitmask in ActrixConfig.enable.
/// The ENABLE_ISSUER bit (bit 3) must be set to enable this service.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct IssuerServiceConfig {
    /// Issuer server config
    #[serde(default)]
    pub server: IssuerServerConfig,

    /// Issuer dependency service config
    #[serde(default)]
    pub dependencies: IssuerDependencies,
}

/// Issuer server config
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct IssuerServerConfig {
    /// Stable Snowflake worker identity. Concurrent Issuer issuers must use
    /// different values in the range 0..=31.
    #[serde(default)]
    pub worker_id: u8,

    /// Signaling Server heartbeat interval (seconds)
    ///
    /// Returned in the RegisterResponse to guide the client's heartbeat frequency after connecting to the Signaling Server
    #[serde(default = "default_signaling_heartbeat_interval_secs")]
    pub signaling_heartbeat_interval_secs: u32,

    /// Token TTL (seconds)
    ///
    /// Expiry time of the generated AIdCredential
    #[serde(default = "default_token_ttl_secs")]
    pub token_ttl_secs: u64,

    /// Renewal token TTL (seconds)
    ///
    /// TTL when initially issuing a renewal token. When the rotation window is reached,
    /// a successor token is deterministically derived; the old token remains valid until its own expiry.
    #[serde(default = "default_renewal_token_ttl_secs")]
    pub renewal_token_ttl_secs: u64,

    /// Renewal rotation window (seconds)
    ///
    /// When the remaining validity of the renewal token is less than this value, it enters the rotation window
    /// and begins deterministically deriving a successor token.
    #[serde(default = "default_renewal_rotation_window_secs")]
    pub renewal_rotation_window_secs: u64,

    /// Renewal token secret (base64-encoded, at least 32 bytes after decoding)
    ///
    /// Used to deterministically derive the successor token (HMAC-SHA256).
    /// Must be consistent within the same actrix cluster. Must not use `actrix_shared_key` as a fallback.
    #[serde(default)]
    pub renewal_token_secret: String,
}

/// External services Issuer depends on
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct IssuerDependencies {
    /// Signer client config
    ///
    /// If not configured, Issuer will auto-discover the local Signer service:
    /// - If the Signer service is enabled (ENABLE_SIGNER bit set), uses localhost:SIGNER_PORT
    /// - Otherwise returns a config error
    #[serde(default)]
    pub signer: Option<SignerClientConfig>,
}

impl Default for IssuerServerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            signaling_heartbeat_interval_secs: default_signaling_heartbeat_interval_secs(),
            token_ttl_secs: default_token_ttl_secs(),
            renewal_token_ttl_secs: default_renewal_token_ttl_secs(),
            renewal_rotation_window_secs: default_renewal_rotation_window_secs(),
            renewal_token_secret: String::new(),
        }
    }
}

/// Default Signaling Server heartbeat interval: 30 seconds
fn default_signaling_heartbeat_interval_secs() -> u32 {
    30
}

/// Default Token TTL: 1 hour (3600 seconds)
fn default_token_ttl_secs() -> u64 {
    3600
}

/// Default Renewal Token TTL: 24 hours (86400 seconds)
fn default_renewal_token_ttl_secs() -> u64 {
    86400
}

/// Default Renewal Rotation Window: 6 hours (21600 seconds)
fn default_renewal_rotation_window_secs() -> u64 {
    21600
}

impl IssuerServiceConfig {
    /// Get the Signer client config
    ///
    /// Supports smart defaults:
    /// 1. If dependencies.signer is explicitly configured, return it directly
    /// 2. If the local Signer service is enabled, return config pointing to the local Signer
    /// 3. Otherwise return None
    pub fn get_signer_client_config(
        &self,
        global_config: &super::ActrixConfig,
    ) -> Option<SignerClientConfig> {
        // Explicit configuration always wins.
        if let Some(ref signer_config) = self.dependencies.signer {
            return Some(signer_config.clone());
        }

        // Fallback: dial the local Signer service if it is enabled.
        if global_config.is_signer_enabled() && global_config.services.signer.is_some() {
            // SGN-02: the Signer gRPC surface runs on its own dedicated listener,
            // not the shared HTTP port. Dial the resolved Signer address. For the
            // loopback default this is http://127.0.0.1:{signer_port} with TLS
            // off; when the listener terminates mTLS the operator must configure
            // client certificates explicitly via dependencies.signer, so this
            // fallback only fully covers the loopback default.
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
}
