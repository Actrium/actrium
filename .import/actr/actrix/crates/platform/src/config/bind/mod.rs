pub mod http;
pub mod ice;

pub use crate::config::bind::http::HttpBindConfig;
pub use crate::config::bind::ice::IceBindConfig;
use serde::{Deserialize, Serialize};

/// Network binding configuration
///
/// Defines network binding parameters for different types of services.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BindConfig {
    /// HTTP/HTTPS service binding configuration (optional)
    ///
    /// Unified HTTP binding. Configuring cert+key enables HTTPS; otherwise HTTP.
    pub http: Option<HttpBindConfig>,

    /// ICE service binding configuration
    ///
    /// UDP binding configuration for STUN/TURN services.
    pub ice: IceBindConfig,
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            http: Some(HttpBindConfig::default()),
            ice: IceBindConfig::default(),
        }
    }
}
