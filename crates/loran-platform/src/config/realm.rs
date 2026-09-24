//! Realm hosting limits.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RealmConfig {
    /// Maximum number of pending and active Realms this instance hosts.
    /// Retired Realms do not count. Unset means no limit; `1` gives a
    /// dedicated single-Realm deployment.
    #[serde(default)]
    pub max_realms: Option<u32>,

    /// Seconds a pending Realm waits for its first Realm Profile before it
    /// is removed.
    #[serde(default = "default_pending_ttl_secs")]
    pub pending_ttl_secs: u64,

    /// Maximum number of pending Realms per Realm domain.
    #[serde(default = "default_max_pending_per_domain")]
    pub max_pending_per_domain: u32,
}

fn default_pending_ttl_secs() -> u64 {
    3600
}

fn default_max_pending_per_domain() -> u32 {
    10
}

impl Default for RealmConfig {
    fn default() -> Self {
        Self {
            max_realms: None,
            pending_ttl_secs: default_pending_ttl_secs(),
            max_pending_per_domain: default_max_pending_per_domain(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let config: RealmConfig = toml::from_str("").unwrap();
        assert_eq!(config, RealmConfig::default());
        assert_eq!(config.max_realms, None);

        let config: RealmConfig = toml::from_str("max_realms = 1").unwrap();
        assert_eq!(config.max_realms, Some(1));
        assert!(toml::from_str::<RealmConfig>("max_realm = 1").is_err());
    }
}
