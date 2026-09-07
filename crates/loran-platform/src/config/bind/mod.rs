pub mod http;
pub mod ice;

pub use crate::config::bind::http::HttpBindConfig;
pub use crate::config::bind::ice::IceBindConfig;
use serde::{Deserialize, Serialize};

/// Network binding configuration.
///
/// Defines the network binding parameters for the different service types.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct BindConfig {
    /// HTTP/HTTPS service binding (optional).
    ///
    /// The unified HTTP binding. Configuring `cert` + `key` enables HTTPS;
    /// otherwise the listener serves plain HTTP.
    pub http: Option<HttpBindConfig>,

    /// Dedicated Signer gRPC listener binding (optional, SGN-02).
    ///
    /// The Signer signing oracle is served on its own dedicated listener rather
    /// than merged into the shared `bind.http` listener. When omitted the Signer
    /// defaults to loopback-only on `bind.http.port + 1`, matching the
    /// `expose = "loopback"` default. Set this to override the bind address; a
    /// non-loopback `ip` is treated as public exposure by the config validator.
    #[serde(default)]
    pub signer: Option<HttpBindConfig>,

    /// ICE service binding.
    ///
    /// UDP binding for the STUN/TURN services.
    pub ice: IceBindConfig,
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            http: Some(HttpBindConfig::default()),
            signer: None,
            ice: IceBindConfig::default(),
        }
    }
}
