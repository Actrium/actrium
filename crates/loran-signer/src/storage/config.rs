//! Storage backend configuration.
//!
//! Defines the configuration structures for the various storage backends.

use serde::{Deserialize, Serialize};

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Storage backend type.
    pub backend: StorageBackend,

    /// Key lifetime, in seconds.
    ///
    /// How long a generated key remains valid; keys older than this are treated
    /// as expired. A value of 0 means the key never expires.
    pub key_ttl_seconds: u64,

    /// SQLite configuration (required when `backend = "sqlite"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite: Option<SqliteConfig>,

    /// PostgreSQL configuration (required when `backend = "postgres"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<PostgresConfig>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            key_ttl_seconds: 3600,
            sqlite: Some(SqliteConfig::default()),
            postgres: None,
        }
    }
}

/// Storage backend type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// SQLite database.
    Sqlite,
    /// PostgreSQL database.
    Postgres,
}

/// SQLite configuration.
///
/// Note: the effective database file is currently `signer_keys.db` inside the
/// instance data directory (`ActrixConfig.sqlite_path`), supplied via the
/// `db_path` argument of `KeyStorage::from_config`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SqliteConfig {
    /// Optional database path override.
    ///
    /// Accepted for configuration compatibility — generated, example, and
    /// deployment templates all emit `path = "ks.db"` under this section, and
    /// silently rejecting it would break existing configs on upgrade. It is
    /// currently informational only: the key database location is derived from
    /// the instance data directory, not from this field. Honouring it would
    /// relocate the key store, so wiring is deferred to a deliberate change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// PostgreSQL configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    /// Database host address.
    pub host: String,

    /// Database port.
    pub port: u16,

    /// Database name.
    pub database: String,

    /// Username.
    pub username: String,

    /// Password.
    pub password: String,

    /// Connection pool size.
    #[serde(default = "default_postgres_pool_size")]
    pub pool_size: u32,

    /// Maximum connection lifetime, in seconds.
    #[serde(default = "default_max_lifetime_secs")]
    pub max_lifetime_secs: u64,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "actrix_ks".to_string(),
            username: "actrix".to_string(),
            password: "".to_string(),
            pool_size: default_postgres_pool_size(),
            max_lifetime_secs: default_max_lifetime_secs(),
        }
    }
}

fn default_postgres_pool_size() -> u32 {
    20
}

fn default_max_lifetime_secs() -> u64 {
    3600
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_storage_config() {
        let config = StorageConfig::default();
        assert_eq!(config.backend, StorageBackend::Sqlite);
        assert_eq!(config.key_ttl_seconds, 3600);
        assert!(config.sqlite.is_some());
    }

    #[test]
    fn test_serialize_sqlite_config() {
        let config = StorageConfig {
            backend: StorageBackend::Sqlite,
            key_ttl_seconds: 7200,
            sqlite: Some(SqliteConfig::default()),
            postgres: None,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("backend = \"sqlite\""));
        assert!(toml.contains("key_ttl_seconds = 7200"));
    }

    #[test]
    fn test_deserialize_postgres_config() {
        let toml_str = r#"
            backend = "postgres"
            key_ttl_seconds = 1800

            [postgres]
            host = "localhost"
            port = 5432
            database = "actrix"
            username = "actrix"
            password = "secret"
            pool_size = 30
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.backend, StorageBackend::Postgres);
        assert_eq!(config.key_ttl_seconds, 1800);

        let postgres = config.postgres.unwrap();
        assert_eq!(postgres.host, "localhost");
        assert_eq!(postgres.port, 5432);
        assert_eq!(postgres.database, "actrix");
        assert_eq!(postgres.username, "actrix");
        assert_eq!(postgres.password, "secret");
        assert_eq!(postgres.pool_size, 30);
    }

    #[test]
    fn test_reject_unknown_storage_field() {
        let toml_str = r#"
            backend = "sqlite"
            key_ttl_seconds = 3600
            unknown_field = "typo"
        "#;

        let result: Result<StorageConfig, _> = toml::from_str(toml_str);
        assert!(
            result.is_err(),
            "unknown top-level storage field must be rejected"
        );
    }

    #[test]
    fn test_reject_unknown_postgres_field() {
        let toml_str = r#"
            backend = "postgres"
            key_ttl_seconds = 1800

            [postgres]
            host = "localhost"
            port = 5432
            database = "actrix"
            username = "actrix"
            password = "secret"
            typo_field = 1
        "#;

        let result: Result<StorageConfig, _> = toml::from_str(toml_str);
        assert!(
            result.is_err(),
            "unknown nested postgres field must be rejected"
        );
    }
}
