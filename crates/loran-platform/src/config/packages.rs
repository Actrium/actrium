//! Actr package distribution service configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct PackagesConfig {
    /// Deploy-time mappings from opaque URI targets to root Actor packages.
    #[serde(default)]
    pub targets: Vec<PackageTargetConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PackageTargetConfig {
    /// Opaque target reference, without a leading slash.
    pub target: String,
    /// Local `.actr` artifact served by this Loran instance.
    pub artifact: String,
    /// Optional workload profile hint used for early Host selection.
    #[serde(default)]
    pub profile: Option<String>,
}
