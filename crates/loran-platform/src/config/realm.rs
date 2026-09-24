//! Realm hosting limits.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RealmConfig {
    /// Maximum number of pending and active Realms this instance hosts.
    /// Retired Realms do not count. Unset means no limit; `1` gives a
    /// dedicated single-Realm deployment.
    #[serde(default)]
    pub max_realms: Option<u32>,
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
