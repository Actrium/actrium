//! Actr package hosting service configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackagesConfig {
    /// Absolute paths of local `.actr` files, each served by its SHA-256 digest.
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_retired_target_mappings() {
        let config: PackagesConfig =
            toml::from_str(r#"artifacts = ["/srv/actr/echo.actr"]"#).unwrap();
        assert_eq!(config.artifacts, ["/srv/actr/echo.actr"]);

        let retired = r#"
            [[targets]]
            target = "apps/echo"
            artifact = "/srv/actr/echo.actr"
        "#;
        assert!(toml::from_str::<PackagesConfig>(retired).is_err());
    }
}
