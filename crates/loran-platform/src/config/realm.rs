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
    /// Maximum pending and active Realms per Realm domain created by Realm
    /// management requests. Unset means no limit.
    #[serde(default)]
    pub max_realms_per_domain: Option<u32>,
    /// Maximum pending Realms per Realm domain. Unset means 10.
    #[serde(default)]
    pub max_pending_per_domain: Option<u32>,
    /// Seconds a pending Realm waits for its first Realm Profile before it
    /// is removed. Unset means 3600; values are kept within 60..=86400.
    #[serde(default)]
    pub pending_ttl_secs: Option<u64>,
    /// Seconds between refreshes of other domains' Realm signing keys and
    /// Realm Profiles. Unset means 3600; values are kept within 60..=86400.
    #[serde(default)]
    pub profile_refresh_secs: Option<u64>,
    /// Maximum number of publishers `register_publisher` requests may
    /// register, whatever their status. Unset means 1000.
    #[serde(default)]
    pub max_publishers: Option<u32>,
    /// Maximum active members per Realm. Unset means 1000.
    #[serde(default)]
    pub max_members_per_realm: Option<u32>,
    /// Maximum unrevoked Realm client keys per member. Unset means 20.
    #[serde(default)]
    pub max_clients_per_member: Option<u32>,
    /// Maximum unused, unexpired, unrevoked invites per Realm. Unset means
    /// 100.
    #[serde(default)]
    pub max_open_invites_per_realm: Option<u32>,
}

impl RealmConfig {
    pub fn max_pending_per_domain(&self) -> u32 {
        self.max_pending_per_domain.unwrap_or(10)
    }

    pub fn pending_ttl_secs(&self) -> u64 {
        self.pending_ttl_secs.unwrap_or(3600).clamp(60, 86_400)
    }

    pub fn profile_refresh_secs(&self) -> u64 {
        self.profile_refresh_secs.unwrap_or(3600).clamp(60, 86_400)
    }

    pub fn max_publishers(&self) -> u32 {
        self.max_publishers.unwrap_or(1000)
    }

    pub fn max_members_per_realm(&self) -> u32 {
        self.max_members_per_realm.unwrap_or(1000)
    }

    pub fn max_clients_per_member(&self) -> u32 {
        self.max_clients_per_member.unwrap_or(20)
    }

    pub fn max_open_invites_per_realm(&self) -> u32 {
        self.max_open_invites_per_realm.unwrap_or(100)
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
        assert_eq!(config.profile_refresh_secs(), 3600);
        assert_eq!(config.max_pending_per_domain(), 10);
        assert_eq!(config.pending_ttl_secs(), 3600);
        assert_eq!(config.max_publishers(), 1000);
        assert_eq!(config.max_members_per_realm(), 1000);
        assert_eq!(config.max_clients_per_member(), 20);
        assert_eq!(config.max_open_invites_per_realm(), 100);

        let config: RealmConfig = toml::from_str("max_realms = 1").unwrap();
        assert_eq!(config.max_realms, Some(1));
        assert!(toml::from_str::<RealmConfig>("max_realm = 1").is_err());

        let config: RealmConfig = toml::from_str("profile_refresh_secs = 999999").unwrap();
        assert_eq!(config.profile_refresh_secs(), 86_400);
    }
}
