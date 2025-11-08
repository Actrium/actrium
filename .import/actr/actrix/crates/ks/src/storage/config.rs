//! 存储后端配置
//!
//! 定义各种存储后端的配置结构

use serde::{Deserialize, Serialize};

/// 存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 存储后端类型
    pub backend: StorageBackend,

    /// 密钥有效期（秒）
    ///
    /// 生成的密钥的有效期时间，超过此时间的密钥将被视为过期
    /// 设置为 0 表示永不过期
    pub key_ttl_seconds: u64,

    /// SQLite 配置（当 backend = "sqlite" 时必需）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite: Option<SqliteConfig>,

    /// Redis 配置（当 backend = "redis" 时必需）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis: Option<RedisConfig>,

    /// PostgreSQL 配置（当 backend = "postgres" 时必需）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<PostgresConfig>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            key_ttl_seconds: 3600,
            sqlite: Some(SqliteConfig::default()),
            redis: None,
            postgres: None,
        }
    }
}

/// 存储后端类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// SQLite 数据库
    Sqlite,
    /// Redis 内存数据库
    Redis,
    /// PostgreSQL 数据库
    Postgres,
}

/// SQLite 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// 数据库文件路径
    pub path: String,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "ks_keys.db".to_string(),
        }
    }
}

/// Redis 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis 连接 URL
    ///
    /// 格式：redis://[username:password@]host[:port][/database]
    /// 示例：redis://localhost:6379/0
    pub url: String,

    /// 连接池大小
    #[serde(default = "default_redis_pool_size")]
    pub pool_size: usize,

    /// 超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379/0".to_string(),
            pool_size: default_redis_pool_size(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

fn default_redis_pool_size() -> usize {
    20
}

fn default_timeout_ms() -> u64 {
    5000
}

/// PostgreSQL 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// 数据库主机地址
    pub host: String,

    /// 数据库端口
    pub port: u16,

    /// 数据库名称
    pub database: String,

    /// 用户名
    pub username: String,

    /// 密码
    pub password: String,

    /// 连接池大小
    #[serde(default = "default_postgres_pool_size")]
    pub pool_size: u32,

    /// 连接最大生命周期（秒）
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
            sqlite: Some(SqliteConfig {
                path: "test.db".to_string(),
            }),
            redis: None,
            postgres: None,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("backend = \"sqlite\""));
        assert!(toml.contains("key_ttl_seconds = 7200"));
    }

    #[test]
    fn test_deserialize_redis_config() {
        let toml_str = r#"
            backend = "redis"
            key_ttl_seconds = 1800

            [redis]
            url = "redis://localhost:6379/1"
            pool_size = 30
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.backend, StorageBackend::Redis);
        assert_eq!(config.key_ttl_seconds, 1800);

        let redis = config.redis.unwrap();
        assert_eq!(redis.url, "redis://localhost:6379/1");
        assert_eq!(redis.pool_size, 30);
    }
}
