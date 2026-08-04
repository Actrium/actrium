//! Service configuration collection

use super::ais::AisConfig;
use super::signaling::SignalingConfig;
use serde::{Deserialize, Serialize};

/// MFR (Manufacturer Registry) configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MfrConfig {
    /// How long (in seconds) to retain expired/used publish nonces for auditing.
    /// Default: 86400 (24 hours).
    #[serde(default = "default_nonce_retain_secs")]
    pub nonce_retain_secs: i64,
}

fn default_nonce_retain_secs() -> i64 {
    86400 // 24 hours
}

impl Default for MfrConfig {
    fn default() -> Self {
        Self {
            nonce_retain_secs: default_nonce_retain_secs(),
        }
    }
}

/// Collection of all service configurations
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ServicesConfig {
    /// Signer service configuration
    #[serde(default)]
    pub signer: Option<signer::SignerServiceConfig>,

    /// AIS (Actor Identity Service) service configuration
    #[serde(default)]
    pub ais: Option<AisConfig>,

    /// Signaling service configuration
    #[serde(default)]
    pub signaling: Option<SignalingConfig>,
    // Note: STUN/TURN do not depend on Signer, keeping the original configuration approach
    /// MFR (Manufacturer Registry) service configuration
    #[serde(default)]
    pub mfr: MfrConfig,
}
