//! Storage backend configuration
//!
//! Defines configuration structures for various storage backends

use serde::{Deserialize, Serialize};

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type
    pub backend: StorageBackend,

    /// Key TTL (seconds)
    ///
    /// Validity period for generated keys; keys older than this are considered expired
    /// Set to 0 for no expiration
    pub key_ttl_seconds: u64,

    /// SQLite configuration (required when backend = "sqlite")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite: Option<SqliteConfig>,

    /// PostgreSQL configuration (required when backend = "postgres")
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

/// Storage backend type enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// SQLite database
    Sqlite,
    /// PostgreSQL database
    Postgres,
}

/// SQLite configuration
///
/// Note: The database path is passed via the db_path parameter of KeyStorage::from_config
/// TODO: define grain config for sqlite
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SqliteConfig {}

/// PostgreSQL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Database host address
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password
    pub password: String,

    /// Connection pool size
    #[serde(default = "default_postgres_pool_size")]
    pub pool_size: u32,

    /// Connection maximum lifetime (seconds)
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
            sqlite: Some(SqliteConfig {}),
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
}
