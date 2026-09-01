//! AIS (Actor Identity Service) configuration

use crate::config::signer::SignerClientConfig;
use serde::{Deserialize, Serialize};

/// AIS service configuration
///
/// Service enable/disable is controlled by the bitmask in ActrixConfig.enable.
/// The ENABLE_AIS bit (bit 3) must be set to enable this service.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AisConfig {
    /// AIS server configuration
    #[serde(default)]
    pub server: AisServerConfig,

    /// AIS dependency service configuration
    #[serde(default)]
    pub dependencies: AisDependencies,
}

/// AIS server configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AisServerConfig {
    /// Signaling Server heartbeat interval (seconds)
    ///
    /// Returned in RegisterResponse, guides the client heartbeat frequency after connecting to the Signaling Server
    #[serde(default = "default_signaling_heartbeat_interval_secs")]
    pub signaling_heartbeat_interval_secs: u32,

    /// Token validity period (seconds)
    ///
    /// Expiration time of the generated AIdCredential
    #[serde(default = "default_token_ttl_secs")]
    pub token_ttl_secs: u64,

    /// Renewal token validity period (seconds)
    ///
    /// TTL for the initial issuance of the renewal token. When the rotation window is reached,
    /// a successor token is deterministically derived; the old token remains valid until its own expiry.
    #[serde(default = "default_renewal_token_ttl_secs")]
    pub renewal_token_ttl_secs: u64,

    /// Renewal rotation window (seconds)
    ///
    /// When the remaining validity of the renewal token is less than this value, the rotation window begins,
    /// and a successor token is deterministically derived.
    #[serde(default = "default_renewal_rotation_window_secs")]
    pub renewal_rotation_window_secs: u64,

    /// Renewal token secret (base64-encoded, at least 32 bytes after decoding)
    ///
    /// Used for deterministic derivation of the successor token (HMAC-SHA256).
    /// Must be consistent within the same actrix cluster. Must not use `actrix_shared_key` as a fallback.
    #[serde(default)]
    pub renewal_token_secret: String,
}

/// External services that AIS depends on
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AisDependencies {
    /// Signer client configuration
    ///
    /// If not configured, AIS will auto-detect the local Signer service:
    /// - If the Signer service is enabled (ENABLE_SIGNER bit is set), use localhost:SIGNER_PORT
    /// - Otherwise returns a configuration error
    #[serde(default)]
    pub signer: Option<SignerClientConfig>,
}

impl Default for AisServerConfig {
    fn default() -> Self {
        Self {
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

/// Default Token validity period: 1 hour (3600 seconds)
fn default_token_ttl_secs() -> u64 {
    3600
}

/// Default Renewal Token validity period: 24 hours (86400 seconds)
fn default_renewal_token_ttl_secs() -> u64 {
    86400
}

/// Default Renewal Rotation Window: 6 hours (21600 seconds)
fn default_renewal_rotation_window_secs() -> u64 {
    21600
}

impl AisConfig {
    /// Get the Signer client configuration
    ///
    /// Supports smart defaults:
    /// 1. If dependencies.signer is explicitly configured, return it directly
    /// 2. If the local Signer service is enabled, return configuration pointing to the local Signer
    /// 3. Otherwise return None
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
}
