//! Service configuration collection

use super::issuer::IssuerServiceConfig;
use super::packages::PackagesConfig;
use super::signaling::SignalingConfig;
use serde::{Deserialize, Serialize};

/// Publisher Registry configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PublisherRegistryConfig {
    /// How long (in seconds) to retain expired/used publish nonces for auditing.
    /// Default: 86400 (24 hours).
    #[serde(default = "default_nonce_retain_secs")]
    pub nonce_retain_secs: i64,
}

fn default_nonce_retain_secs() -> i64 {
    86400 // 24 hours
}

impl Default for PublisherRegistryConfig {
    fn default() -> Self {
        Self {
            nonce_retain_secs: default_nonce_retain_secs(),
        }
    }
}

/// Collection of all service configurations
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ServicesConfig {
    /// Optional content-addressed `.actr` artifact hosting.
    #[serde(default)]
    pub packages: Option<PackagesConfig>,

    /// Signer service configuration
    #[serde(default)]
    pub signer: Option<signer::SignerServiceConfig>,

    /// Issuer (Actor Identity Service) configuration
    #[serde(default)]
    pub issuer: Option<IssuerServiceConfig>,

    /// Signaling service configuration
    #[serde(default)]
    pub signaling: Option<SignalingConfig>,
    // Note: STUN/TURN do not depend on Signer, keeping their original configuration approach
    /// Publisher Registry service configuration
    #[serde(default)]
    pub publisher_registry: PublisherRegistryConfig,
}
