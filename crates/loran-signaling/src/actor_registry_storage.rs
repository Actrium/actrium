//! ActorRegistry SQLite persistent cache layer
//!
//! ## Design Principles
//!
//! 1. **SQLite as cache**: Not the primary data source; used for restart recovery
//! 2. **Data has TTL**: Default 1-hour expiration with automatic cleanup
//! 3. **Memory first**: HashMap is the primary store, SQLite is the backup
//!
//! ## Data Flow
//!
//! - Startup: SQLite -> HashMap (cache recovery)
//! - Register: HashMap + SQLite (dual write)
//! - Query: HashMap (fast path)
//! - Heartbeat: HashMap + SQLite (update TTL)
//! - Cleanup: Periodic cleanup of expired data

use crate::actor_registry::{ServiceCapabilities, ServiceInfo, ServiceLocation, ServiceStatus};
use actr_protocol::{Acl, ActrId, ServiceSpec};
use anyhow::{Context, Result};
use prost::Message as ProstMessage;
use serde_json;
use sqlx::{
    Row,
    sqlite::{SqlitePool, SqlitePoolOptions},
};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// ActorRegistry persistent storage
#[derive(Debug)]
pub struct ActorRegistryStorage {
    pool: SqlitePool,
    /// TTL (seconds), default 3600 seconds (1 hour)
    default_ttl_secs: u64,
}

/// Default service TTL (1 hour)
pub const DEFAULT_SERVICE_TTL_SECS: u64 = 12 * 3600; // temporary workaround

impl ActorRegistryStorage {
    /// Create a storage instance
    pub async fn new(database_file: impl AsRef<Path>, ttl_secs: Option<u64>) -> Result<Self> {
        let db_path = database_file.as_ref();

        // Ensure the database directory exists
        if let Some(parent) = db_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create database directory: {}", parent.display())
            })?;
            platform::recording::info!("📁 Created database directory: {}", parent.display());
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .with_context(|| format!("Failed to connect to database: {}", db_path.display()))?;

        let storage = Self {
            pool,
            default_ttl_secs: ttl_secs.unwrap_or(DEFAULT_SERVICE_TTL_SECS),
        };

        storage.init_schema().await?;
        platform::recording::info!(
            "✅ ActorRegistryStorage initialized with TTL={}s",
            storage.default_ttl_secs
        );

        Ok(storage)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        // Migration: tables keyed by the retired numeric Realm id cannot be
        // mapped to Realm names. The table is a cache, so drop them and let
        // Actors re-register.
        sqlx::query("DROP TABLE IF EXISTS service_registry")
            .execute(&self.pool)
            .await?;
        let legacy_realm_column: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM pragma_table_info('actor_registry') WHERE name = 'actor_realm_id'",
        )
        .fetch_optional(&self.pool)
        .await?;
        if legacy_realm_column.is_some() {
            sqlx::query("DROP TABLE actor_registry")
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS actor_registry (
                -- ActorId fields
                actor_serial_number INTEGER NOT NULL,
                actor_realm TEXT NOT NULL,
                actor_publisher TEXT NOT NULL,
                actor_device_name TEXT NOT NULL,
                actor_version TEXT NOT NULL,

                -- Service basic info
                service_name TEXT NOT NULL,
                message_types TEXT NOT NULL,  -- JSON array

                -- Capabilities and status
                capabilities_json TEXT,  -- JSON, nullable
                status TEXT NOT NULL,  -- Available/Busy/Maintenance/Unavailable

                -- ServiceSpec (protobuf BLOB)
                service_spec_blob BLOB,

                -- ACL (protobuf BLOB)
                acl_blob BLOB,

                -- Load metrics
                service_availability_state INTEGER,
                power_reserve REAL,
                mailbox_backlog REAL,
                worst_dependency_health_state INTEGER,

                -- Geographic location
                geo_region TEXT,
                geo_longitude REAL,
                geo_latitude REAL,

                -- Sticky clients
                sticky_client_ids TEXT,  -- JSON array

                -- WebSocket address (optional, for direct connection)
                ws_address TEXT,

                -- Timestamps (Unix timestamp)
                registered_at INTEGER NOT NULL,
                last_heartbeat_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,  -- TTL expiration time

                PRIMARY KEY (actor_serial_number, actor_realm, service_name)
            );

            CREATE INDEX IF NOT EXISTS idx_expires_at ON actor_registry(expires_at);
            CREATE INDEX IF NOT EXISTS idx_service_name ON actor_registry(service_name);
            "#,
        )
        .execute(&self.pool)
        .await
        .with_context(|| "Failed to create actor_registry table")?;

        platform::recording::info!("Database schema initialized");
        Ok(())
    }

    /// Save service info
    pub async fn save_service(&self, service: &ServiceInfo) -> Result<()> {
        let now = current_timestamp();
        let expires_at = now + self.default_ttl_secs;

        // Serialize complex fields
        let message_types_json = serde_json::to_string(&service.message_types)?;
        let capabilities_json = service
            .capabilities
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let sticky_client_ids_json = serde_json::to_string(&service.sticky_client_ids)?;

        // Serialize ServiceSpec to protobuf bytes
        let service_spec_blob = service.service_spec.as_ref().and_then(|spec| {
            let mut buf = Vec::new();
            spec.encode(&mut buf).ok()?;
            Some(buf)
        });

        // Serialize ACL to protobuf bytes
        let acl_blob = service.acl.as_ref().and_then(|acl| {
            let mut buf = Vec::new();
            acl.encode(&mut buf).ok()?;
            Some(buf)
        });

        let actor_type = &service.actor_type;

        sqlx::query(
            r#"
            INSERT INTO actor_registry (
                actor_serial_number, actor_realm, actor_publisher, actor_device_name,
                actor_version,
                service_name, message_types, capabilities_json, status,
                service_spec_blob, acl_blob,
                service_availability_state, power_reserve, mailbox_backlog,
                worst_dependency_health_state,
                geo_region, geo_longitude, geo_latitude,
                sticky_client_ids, ws_address,
                registered_at, last_heartbeat_at, expires_at
            ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11,
                ?12, ?13, ?14,
                ?15,
                ?16, ?17, ?18,
                ?19, ?20,
                ?21, ?22, ?23
            )
            ON CONFLICT(actor_serial_number, actor_realm, service_name)
            DO UPDATE SET
                actor_publisher = excluded.actor_publisher,
                actor_device_name = excluded.actor_device_name,
                actor_version = excluded.actor_version,
                message_types = excluded.message_types,
                capabilities_json = excluded.capabilities_json,
                status = excluded.status,
                service_spec_blob = excluded.service_spec_blob,
                acl_blob = excluded.acl_blob,
                service_availability_state = excluded.service_availability_state,
                power_reserve = excluded.power_reserve,
                mailbox_backlog = excluded.mailbox_backlog,
                worst_dependency_health_state = excluded.worst_dependency_health_state,
                geo_region = excluded.geo_region,
                geo_longitude = excluded.geo_longitude,
                geo_latitude = excluded.geo_latitude,
                sticky_client_ids = excluded.sticky_client_ids,
                ws_address = excluded.ws_address,
                last_heartbeat_at = excluded.last_heartbeat_at,
                expires_at = excluded.expires_at
            "#,
        )
        .bind(service.actor_id.serial_number as i64)
        .bind(&service.actor_id.realm)
        .bind(&actor_type.publisher)
        .bind(&actor_type.name)
        .bind(&actor_type.version)
        .bind(&service.service_name)
        .bind(&message_types_json)
        .bind(capabilities_json.as_deref())
        .bind(status_to_string(&service.status))
        .bind(service_spec_blob.as_deref())
        .bind(acl_blob.as_deref())
        .bind(service.service_availability_state.map(|v| v as i64))
        .bind(service.power_reserve.map(|v| v as f64))
        .bind(service.mailbox_backlog.map(|v| v as f64))
        .bind(service.worst_dependency_health_state.map(|v| v as i64))
        .bind(service.geo_location.as_ref().map(|g| g.region.as_str()))
        .bind(service.geo_location.as_ref().and_then(|g| g.longitude))
        .bind(service.geo_location.as_ref().and_then(|g| g.latitude))
        .bind(&sticky_client_ids_json)
        .bind(service.ws_address.as_deref())
        .bind(now as i64)
        .bind(service.last_heartbeat_time_secs as i64)
        .bind(expires_at as i64)
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to save service: {}", service.service_name))?;

        platform::recording::debug!(
            "Saved service to cache: {} (Actor {}, expires in {}s)",
            service.service_name,
            service.actor_id.to_string_repr(),
            self.default_ttl_secs
        );

        Ok(())
    }

    /// Update heartbeat time and TTL
    pub async fn update_heartbeat(&self, actor_id: &ActrId, service_name: &str) -> Result<()> {
        let now = current_timestamp();
        let expires_at = now + self.default_ttl_secs;

        let rows_affected = sqlx::query(
            r#"
            UPDATE actor_registry
            SET last_heartbeat_at = ?1, expires_at = ?2
            WHERE actor_serial_number = ?3 AND actor_realm = ?4 AND service_name = ?5
            "#,
        )
        .bind(now as i64)
        .bind(expires_at as i64)
        .bind(actor_id.serial_number as i64)
        .bind(&actor_id.realm)
        .bind(service_name)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected > 0 {
            platform::recording::debug!(
                "Updated heartbeat: {} (Actor {}, TTL extended to {})",
                service_name,
                actor_id.to_string_repr(),
                expires_at
            );
        }

        Ok(())
    }

    /// Delete service
    pub async fn delete_service(&self, actor_id: &ActrId, service_name: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM actor_registry
            WHERE actor_serial_number = ?1 AND actor_realm = ?2 AND service_name = ?3
            "#,
        )
        .bind(actor_id.serial_number as i64)
        .bind(&actor_id.realm)
        .bind(service_name)
        .execute(&self.pool)
        .await?;

        platform::recording::debug!(
            "Deleted service from cache: {} (Actor {})",
            service_name,
            actor_id.to_string_repr()
        );
        Ok(())
    }

    /// Load all valid services (recovery on startup)
    pub async fn load_all_services(&self) -> Result<Vec<ServiceInfo>> {
        let now = current_timestamp();

        let rows = sqlx::query(
            r#"
            SELECT
                actor_serial_number, actor_realm, actor_publisher, actor_device_name,
                actor_version,
                service_name, message_types, capabilities_json, status,
                service_spec_blob, acl_blob,
                service_availability_state, power_reserve, mailbox_backlog,
                worst_dependency_health_state,
                geo_region, geo_longitude, geo_latitude,
                sticky_client_ids, ws_address,
                last_heartbeat_at
            FROM actor_registry
            WHERE expires_at > ?1
            ORDER BY service_name, actor_serial_number
            "#,
        )
        .bind(now as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut services = Vec::new();

        for row in rows {
            match self.row_to_service_info(row) {
                Ok(service) => services.push(service),
                Err(e) => {
                    platform::recording::error!(
                        "Failed to deserialize service from cache: {:?}",
                        e
                    );
                }
            }
        }

        platform::recording::info!("Loaded {} services from cache", services.len());
        Ok(services)
    }

    /// Load services by ActorId (for heartbeat recovery)
    ///
    /// When a heartbeat is received but the service is not found in memory,
    /// attempt to restore it from the database.
    /// Only returns non-expired services (expires_at > now).
    ///
    /// # Arguments
    ///
    /// * `actor_id` - Actor ID
    ///
    /// # Returns
    ///
    /// All non-expired services for this Actor
    pub async fn load_services_by_actor_id(&self, actor_id: &ActrId) -> Result<Vec<ServiceInfo>> {
        let now = current_timestamp();

        let rows = sqlx::query(
            r#"
            SELECT
                actor_serial_number, actor_realm, actor_publisher, actor_device_name,
                actor_version,
                service_name, message_types, capabilities_json, status,
                service_spec_blob, acl_blob,
                service_availability_state, power_reserve, mailbox_backlog,
                worst_dependency_health_state,
                geo_region, geo_longitude, geo_latitude,
                sticky_client_ids, ws_address,
                last_heartbeat_at
            FROM actor_registry
            WHERE actor_serial_number = ?1
              AND actor_realm = ?2
              AND expires_at > ?3
            ORDER BY service_name
            "#,
        )
        .bind(actor_id.serial_number as i64)
        .bind(&actor_id.realm)
        .bind(now as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut services = Vec::new();

        for row in rows {
            match self.row_to_service_info(row) {
                Ok(service) => services.push(service),
                Err(e) => {
                    platform::recording::error!(
                        "Failed to deserialize service from cache for Actor {}: {:?}",
                        actor_id.to_string_repr(),
                        e
                    );
                }
            }
        }

        if !services.is_empty() {
            platform::recording::debug!(
                "Loaded {} services from cache for Actor {}",
                services.len(),
                actor_id.to_string_repr()
            );
        }

        Ok(services)
    }

    /// Delete every cached service of a Realm
    pub async fn delete_realm(&self, realm: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM actor_registry WHERE actor_realm = ?1")
            .bind(realm)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Clean up expired data
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = current_timestamp();

        let result = sqlx::query(
            r#"
            DELETE FROM actor_registry WHERE expires_at <= ?1
            "#,
        )
        .bind(now as i64)
        .execute(&self.pool)
        .await?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            platform::recording::info!("Cleaned up {} expired services from cache", deleted_count);
        }

        Ok(deleted_count)
    }

    /// Convert a database row to ServiceInfo
    fn row_to_service_info(&self, row: sqlx::sqlite::SqliteRow) -> Result<ServiceInfo> {
        let actor_id = ActrId {
            serial_number: row.get::<i64, _>("actor_serial_number") as u64,
            realm: row.get("actor_realm"),
        };
        let actor_type = actr_protocol::ActrType {
            publisher: row.get("actor_publisher"),
            name: row.get("actor_device_name"),
            version: row.get("actor_version"),
        };

        // Basic fields
        let service_name: String = row.get("service_name");
        let message_types: Vec<String> = serde_json::from_str(row.get("message_types"))?;
        let status = string_to_status(row.get("status"))?;

        // Optional fields
        let capabilities: Option<ServiceCapabilities> = row
            .get::<Option<String>, _>("capabilities_json")
            .map(|s| serde_json::from_str(&s))
            .transpose()?;

        // ServiceSpec (protobuf BLOB)
        let service_spec: Option<ServiceSpec> = row
            .get::<Option<Vec<u8>>, _>("service_spec_blob")
            .map(|bytes| ServiceSpec::decode(&bytes[..]))
            .transpose()
            .ok()
            .flatten();

        // ACL (protobuf BLOB)
        let acl: Option<Acl> = row
            .get::<Option<Vec<u8>>, _>("acl_blob")
            .map(|bytes| Acl::decode(&bytes[..]))
            .transpose()
            .ok()
            .flatten();

        // Geographic location
        let geo_location =
            row.get::<Option<String>, _>("geo_region")
                .map(|region| ServiceLocation {
                    region,
                    longitude: row.get("geo_longitude"),
                    latitude: row.get("geo_latitude"),
                });

        // Sticky clients
        let sticky_client_ids: Vec<String> = serde_json::from_str(row.get("sticky_client_ids"))?;

        Ok(ServiceInfo {
            actor_id,
            actor_type,
            service_name,
            message_types,
            capabilities,
            status,
            last_heartbeat_time_secs: row.get::<i64, _>("last_heartbeat_at") as u64,
            service_spec,
            acl,
            service_availability_state: row
                .get::<Option<i64>, _>("service_availability_state")
                .map(|v| v as i32),
            power_reserve: row.get::<Option<f64>, _>("power_reserve").map(|v| v as f32),
            mailbox_backlog: row
                .get::<Option<f64>, _>("mailbox_backlog")
                .map(|v| v as f32),
            worst_dependency_health_state: row
                .get::<Option<i64>, _>("worst_dependency_health_state")
                .map(|v| v as i32),
            geo_location,
            is_exact_match: false,
            sticky_client_ids,
            ws_address: row.get::<Option<String>, _>("ws_address"),
            is_restored_from_storage: false, // Set by the caller of restore_from_storage
        })
    }

    /// Get statistics
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let now = current_timestamp();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_registry")
            .fetch_one(&self.pool)
            .await?;

        let expired: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM actor_registry WHERE expires_at <= ?1")
                .bind(now as i64)
                .fetch_one(&self.pool)
                .await?;

        Ok(CacheStats {
            total_services: total as u64,
            expired_services: expired as u64,
            valid_services: (total - expired) as u64,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_services: u64,
    pub expired_services: u64,
    pub valid_services: u64,
}

/// Convert ServiceStatus to string
fn status_to_string(status: &ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Available => "Available",
        ServiceStatus::Busy => "Busy",
        ServiceStatus::Maintenance => "Maintenance",
        ServiceStatus::Unavailable => "Unavailable",
    }
}

/// Convert string to ServiceStatus
fn string_to_status(s: &str) -> Result<ServiceStatus> {
    match s {
        "Available" => Ok(ServiceStatus::Available),
        "Busy" => Ok(ServiceStatus::Busy),
        "Maintenance" => Ok(ServiceStatus::Maintenance),
        "Unavailable" => Ok(ServiceStatus::Unavailable),
        _ => Err(anyhow::anyhow!("Invalid service status: {s}")),
    }
}

/// Get current Unix timestamp (seconds)
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::ActrType;

    fn create_test_actor_id(serial: u64) -> ActrId {
        ActrId {
            serial_number: serial,
            realm: "test@example.com".to_string(),
        }
    }

    fn create_test_service(serial: u64, name: &str) -> ServiceInfo {
        ServiceInfo {
            actor_id: create_test_actor_id(serial),
            actor_type: ActrType {
                publisher: "test-mfg".to_string(),
                name: "test-device".to_string(),
                version: "1.0.0".to_string(),
            },
            service_name: name.to_string(),
            message_types: vec!["test.Message".to_string()],
            capabilities: None,
            status: ServiceStatus::Available,
            last_heartbeat_time_secs: current_timestamp(),
            service_spec: None,
            acl: None,
            service_availability_state: None,
            power_reserve: Some(0.8),
            mailbox_backlog: Some(0.2),
            worst_dependency_health_state: None,
            geo_location: None,
            is_exact_match: false,
            sticky_client_ids: vec![],
            ws_address: None,
            is_restored_from_storage: false,
        }
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let storage = ActorRegistryStorage::new(":memory:", Some(3600))
            .await
            .unwrap();

        let service = create_test_service(1, "test-service");
        storage.save_service(&service).await.unwrap();

        let loaded = storage.load_all_services().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].service_name, "test-service");
        assert_eq!(loaded[0].actor_id, service.actor_id);
        assert_eq!(loaded[0].actor_type, service.actor_type);

        let loaded_by_actor = storage
            .load_services_by_actor_id(&service.actor_id)
            .await
            .unwrap();
        assert_eq!(loaded_by_actor.len(), 1);
        assert_eq!(loaded_by_actor[0].actor_id, service.actor_id);
    }

    #[tokio::test]
    async fn test_load_by_actor_requires_same_realm() {
        let storage = ActorRegistryStorage::new(":memory:", Some(3600))
            .await
            .unwrap();

        let service = create_test_service(1, "test-service");
        storage.save_service(&service).await.unwrap();

        let mut other_realm = service.actor_id.clone();
        other_realm.realm = "other@example.com".to_string();

        let loaded = storage
            .load_services_by_actor_id(&other_realm)
            .await
            .unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let storage = ActorRegistryStorage::new(":memory:", Some(1))
            .await
            .unwrap();

        let service = create_test_service(1, "expiring-service");
        storage.save_service(&service).await.unwrap();

        // Loading immediately should find the service
        let loaded = storage.load_all_services().await.unwrap();
        assert_eq!(loaded.len(), 1);

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // After expiration, load returns empty
        let loaded = storage.load_all_services().await.unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let storage = ActorRegistryStorage::new(":memory:", Some(1))
            .await
            .unwrap();

        let service = create_test_service(1, "test-service");
        storage.save_service(&service).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Cleanup
        let deleted = storage.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 1);

        // Verify deletion
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.total_services, 0);
    }

    #[tokio::test]
    async fn test_drops_legacy_table_keyed_by_numeric_realm() {
        let storage = ActorRegistryStorage::new(":memory:", Some(3600))
            .await
            .unwrap();

        // Recreate the schema keyed by the retired numeric Realm id
        sqlx::query("DROP TABLE actor_registry")
            .execute(&storage.pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE actor_registry (
                actor_serial_number INTEGER NOT NULL,
                actor_realm_id INTEGER NOT NULL,
                actor_publisher TEXT NOT NULL,
                actor_device_name TEXT NOT NULL,
                actor_version TEXT NOT NULL,
                service_name TEXT NOT NULL,
                message_types TEXT NOT NULL,
                capabilities_json TEXT,
                status TEXT NOT NULL,
                service_spec_blob BLOB,
                acl_blob BLOB,
                service_availability_state INTEGER,
                power_reserve REAL,
                mailbox_backlog REAL,
                worst_dependency_health_state INTEGER,
                geo_region TEXT,
                geo_longitude REAL,
                geo_latitude REAL,
                sticky_client_ids TEXT,
                ws_address TEXT,
                registered_at INTEGER NOT NULL,
                last_heartbeat_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (actor_serial_number, actor_realm_id, service_name)
            );
            "#,
        )
        .execute(&storage.pool)
        .await
        .unwrap();

        // Re-run schema init as if reopening a legacy database
        storage.init_schema().await.unwrap();

        let service = create_test_service(1, "legacy-service");
        storage.save_service(&service).await.unwrap();

        let loaded = storage.load_all_services().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].actor_id, service.actor_id);

        let loaded_by_actor = storage
            .load_services_by_actor_id(&service.actor_id)
            .await
            .unwrap();
        assert_eq!(loaded_by_actor.len(), 1);
        assert_eq!(loaded_by_actor[0].actor_id, service.actor_id);
    }

    #[tokio::test]
    async fn test_update_heartbeat() {
        let storage = ActorRegistryStorage::new(":memory:", Some(10))
            .await
            .unwrap();

        let service = create_test_service(1, "test-service");
        storage.save_service(&service).await.unwrap();

        // Update heartbeat
        storage
            .update_heartbeat(&service.actor_id, &service.service_name)
            .await
            .unwrap();

        // Verify update succeeded
        let loaded = storage.load_all_services().await.unwrap();
        assert_eq!(loaded.len(), 1);
    }
}
