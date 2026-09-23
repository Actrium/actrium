//! Public Actr authority deployment configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AuthorityConfig {
    /// Enables `/.well-known/actr` and authority-owned bootstrap endpoints.
    /// Package hosting is enabled independently under `services.packages`.
    #[serde(default)]
    pub enabled: bool,

    /// Canonical public authority URI, `actr://<domain>` without a port,
    /// for example `actr://example.com`.
    #[serde(default)]
    pub uri: Option<String>,
}
