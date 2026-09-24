//! Realm core data structures and database operations
//!
//! Defines the core data structures and database CRUD operations for the Realm entity

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum::{Display, EnumString};

use super::error::RealmError;
use crate::storage::db::get_database;

/// Realm status enum
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Display, EnumString)]
pub enum RealmStatus {
    #[default]
    Active,
    Inactive,
    Suspended,
}

/// Realm is the administrative and cryptographic trust boundary of one Actrix
/// production instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Realm {
    /// Realm name `<realm_id>@<domain>`, the primary key
    pub name: String,
    pub status: RealmStatus,
    pub enabled: bool,
    pub expires_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: Option<u64>,
    /// SHA256 hash of current secret (required)
    pub secret_current: String,
    /// (hash, valid_until) for previous secret during rotation grace window
    pub secret_previous: Option<(String, u64)>,
}

impl Default for Realm {
    fn default() -> Self {
        let now = Utc::now().timestamp() as u64;
        Self {
            name: String::new(),
            status: RealmStatus::Active,
            enabled: true,
            expires_at: None,
            created_at: now,
            updated_at: None,
            secret_current: String::new(),
            secret_previous: None,
        }
    }
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Realm {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let name: String = row.try_get("name")?;
        let status_str: String = row.try_get("status")?;
        let enabled: bool = row.try_get::<i32, _>("enabled")? != 0;
        let expires_at: Option<i64> = row.try_get("expires_at")?;
        let created_at: i64 = row.try_get("created_at")?;
        let updated_at: Option<i64> = row.try_get("updated_at")?;
        let secret_current: String = row.try_get("secret_current")?;
        let secret_previous_hash: Option<String> = row.try_get("secret_previous_hash")?;
        let secret_previous_valid_until: Option<i64> =
            row.try_get("secret_previous_valid_until")?;

        let secret_previous = match (secret_previous_hash, secret_previous_valid_until) {
            (Some(hash), Some(valid_until)) if !hash.is_empty() => Some((hash, valid_until as u64)),
            _ => None,
        };

        let status = RealmStatus::from_str(&status_str).map_err(|_| {
            sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown realm status in database: {status_str}"),
            )))
        })?;

        Ok(Self {
            name,
            status,
            enabled,
            expires_at: expires_at.map(|v| v as u64),
            created_at: created_at as u64,
            updated_at: updated_at.map(|v| v as u64),
            secret_current,
            secret_previous,
        })
    }
}

impl Realm {
    /// Create a new realm in the database.
    ///
    /// `secret_hash` is the SHA256 hash of the realm secret (must be non-empty).
    /// `name` must be a normalized Realm name `<realm_id>@<domain>`.
    pub async fn create(name: String, secret_hash: String) -> Result<Self, RealmError> {
        validate_name(&name)?;
        if secret_hash.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "realm secret hash must not be empty".to_string(),
            ));
        }

        let db = get_database();
        let pool = db.get_pool();
        if !Self::get_all().await?.is_empty() {
            return Err(RealmError::ValidationError(
                "this Actrix instance already has its single realm".to_string(),
            ));
        }
        let now = Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO realm (name, status, enabled, created_at, secret_current)
             VALUES (?, 'Active', 1, ?, ?)",
        )
        .bind(&name)
        .bind(now)
        .bind(&secret_hash)
        .execute(pool)
        .await?;

        Ok(Self {
            name,
            status: RealmStatus::Active,
            enabled: true,
            expires_at: None,
            created_at: now as u64,
            updated_at: None,
            secret_current: secret_hash,
            secret_previous: None,
        })
    }

    /// Upsert a superv-managed realm by name.
    pub async fn upsert_managed(
        name: String,
        status: RealmStatus,
        enabled: bool,
        expires_at: Option<u64>,
        secret_current: String,
        secret_previous: Option<(String, u64)>,
    ) -> Result<Self, RealmError> {
        validate_name(&name)?;
        if secret_current.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "managed realm secret_current must not be empty".to_string(),
            ));
        }

        let db = get_database();
        let pool = db.get_pool();
        if let Some(existing) = Self::get_all().await?.first()
            && existing.name != name
        {
            return Err(RealmError::ValidationError(format!(
                "this Actrix instance is already bound to realm {}",
                existing.name
            )));
        }
        let now = Utc::now().timestamp();
        let (prev_hash, prev_valid_until) = match &secret_previous {
            Some((hash, valid_until)) if !hash.trim().is_empty() => {
                (Some(hash.as_str()), Some(*valid_until as i64))
            }
            _ => (None, None),
        };

        sqlx::query(
            "INSERT INTO realm (
                name, status, enabled, expires_at, created_at, updated_at,
                secret_current, secret_previous_hash, secret_previous_valid_until
             )
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
                status = excluded.status,
                enabled = excluded.enabled,
                expires_at = excluded.expires_at,
                updated_at = ?,
                secret_current = excluded.secret_current,
                secret_previous_hash = excluded.secret_previous_hash,
                secret_previous_valid_until = excluded.secret_previous_valid_until",
        )
        .bind(&name)
        .bind(status.to_string())
        .bind(enabled as i32)
        .bind(expires_at.map(|v| v as i64))
        .bind(now)
        .bind(&secret_current)
        .bind(prev_hash)
        .bind(prev_valid_until)
        .bind(now)
        .execute(pool)
        .await?;

        Self::get(&name).await?.ok_or(RealmError::NotFound)
    }

    /// Save (UPDATE) an existing realm to the database.
    pub async fn save(&mut self) -> Result<(), RealmError> {
        // Enforce the non-empty secret invariant here as well, so no code path
        // can silently persist an empty secret. This mirrors the storage-layer
        // CHECK constraint and the validation in `create` / `upsert_managed`.
        if self.secret_current.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "realm secret_current must not be empty".to_string(),
            ));
        }

        let db = get_database();
        let pool = db.get_pool();
        let now = Utc::now().timestamp();
        self.updated_at = Some(now as u64);

        let (prev_hash, prev_valid_until) = match &self.secret_previous {
            Some((hash, valid_until)) => (Some(hash.as_str()), Some(*valid_until as i64)),
            None => (None, None),
        };

        sqlx::query(
            "UPDATE realm SET status = ?, enabled = ?, expires_at = ?,
             updated_at = ?, secret_current = ?,
             secret_previous_hash = ?, secret_previous_valid_until = ?
             WHERE name = ?",
        )
        .bind(self.status.to_string())
        .bind(self.enabled as i32)
        .bind(self.expires_at.map(|v| v as i64))
        .bind(now)
        .bind(&self.secret_current)
        .bind(prev_hash)
        .bind(prev_valid_until)
        .bind(&self.name)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get a realm by its Realm name.
    pub async fn get(name: &str) -> Result<Option<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query_as::<_, Realm>(
            "SELECT name, status, enabled, expires_at, created_at, updated_at,
                    secret_current, secret_previous_hash, secret_previous_valid_until
             FROM realm WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Get all realms.
    pub async fn get_all() -> Result<Vec<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let realms = sqlx::query_as::<_, Realm>(
            "SELECT name, status, enabled, expires_at, created_at, updated_at,
                    secret_current, secret_previous_hash, secret_previous_valid_until
             FROM realm",
        )
        .fetch_all(pool)
        .await?;

        Ok(realms)
    }

    /// Delete a realm by name.
    pub async fn delete(name: &str) -> Result<u64, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query("DELETE FROM realm WHERE name = ?")
            .bind(name)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Disable a realm without physically removing it.
    pub async fn soft_delete(name: &str) -> Result<bool, RealmError> {
        let Some(mut realm) = Self::get(name).await? else {
            return Ok(false);
        };

        realm.enabled = false;
        realm.status = RealmStatus::Inactive;
        realm.save().await?;
        Ok(true)
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = Utc::now().timestamp() as u64;
            now > expires_at
        } else {
            false
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == RealmStatus::Active && self.enabled && !self.is_expired()
    }
}

fn validate_name(name: &str) -> Result<(), RealmError> {
    actr_protocol::uri::parse_realm_name(name)
        .map(|_| ())
        .map_err(|e| RealmError::ValidationError(format!("invalid realm name: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test_utils::utils::setup_test_db;
    use serial_test::serial;

    #[test]
    fn test_realm_status_display() {
        assert_eq!(RealmStatus::Active.to_string(), "Active");
        assert_eq!(RealmStatus::Inactive.to_string(), "Inactive");
        assert_eq!(RealmStatus::Suspended.to_string(), "Suspended");
    }

    #[test]
    fn test_realm_status_parse() {
        assert_eq!(
            RealmStatus::from_str("Active").unwrap(),
            RealmStatus::Active
        );
        assert_eq!(
            RealmStatus::from_str("Inactive").unwrap(),
            RealmStatus::Inactive
        );
        assert_eq!(
            RealmStatus::from_str("Suspended").unwrap(),
            RealmStatus::Suspended
        );
    }

    #[test]
    fn test_realm_default() {
        let realm = Realm::default();
        assert!(realm.name.is_empty());
        assert_eq!(realm.status, RealmStatus::Active);
        assert!(realm.enabled);
        assert!(realm.secret_current.is_empty());
        assert!(realm.secret_previous.is_none());
    }

    #[test]
    fn test_realm_expired() {
        let mut realm = Realm::default();
        let past_time = Utc::now().timestamp() as u64 - 3600;
        realm.expires_at = Some(past_time);
        assert!(realm.is_expired());
        assert!(!realm.is_active());
    }

    #[test]
    fn test_realm_active() {
        let mut realm = Realm::default();
        let future_time = Utc::now().timestamp() as u64 + 3600;
        realm.expires_at = Some(future_time);
        assert!(!realm.is_expired());
        assert!(realm.is_active());
    }

    #[tokio::test]
    #[serial]
    async fn test_upsert_managed_updates_by_name() -> anyhow::Result<()> {
        setup_test_db().await?;

        let realm = Realm::upsert_managed(
            "managed-one@example.com".to_string(),
            RealmStatus::Active,
            true,
            Some(1_900_000_000),
            "hash-current".to_string(),
            None,
        )
        .await?;
        assert_eq!(realm.name, "managed-one@example.com");
        assert_eq!(realm.secret_current, "hash-current");

        let updated = Realm::upsert_managed(
            "managed-one@example.com".to_string(),
            RealmStatus::Suspended,
            false,
            Some(1_900_000_001),
            "hash-current-2".to_string(),
            Some(("hash-prev".to_string(), 1_800_000_000)),
        )
        .await?;
        assert_eq!(updated.name, "managed-one@example.com");
        assert_eq!(updated.status, RealmStatus::Suspended);
        assert!(!updated.enabled);
        assert_eq!(updated.secret_current, "hash-current-2");
        assert_eq!(
            updated.secret_previous,
            Some(("hash-prev".to_string(), 1_800_000_000))
        );

        let other = Realm::upsert_managed(
            "managed-two@example.com".to_string(),
            RealmStatus::Active,
            true,
            None,
            "hash-other".to_string(),
            None,
        )
        .await;
        assert!(matches!(other, Err(RealmError::ValidationError(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_rejects_invalid_realm_name() -> anyhow::Result<()> {
        setup_test_db().await?;

        for name in ["", "managed", "Managed@example.com", "a@example.com:443"] {
            let result = Realm::create(name.to_string(), "hash".to_string()).await;
            assert!(
                matches!(result, Err(RealmError::ValidationError(_))),
                "{name} should be rejected"
            );
        }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_soft_delete_marks_realm_inactive() -> anyhow::Result<()> {
        setup_test_db().await?;

        Realm::upsert_managed(
            "managed-delete@example.com".to_string(),
            RealmStatus::Active,
            true,
            None,
            "delete-current".to_string(),
            None,
        )
        .await?;

        assert!(Realm::soft_delete("managed-delete@example.com").await?);
        let realm = Realm::get("managed-delete@example.com")
            .await?
            .expect("realm should remain");
        assert_eq!(realm.status, RealmStatus::Inactive);
        assert!(!realm.enabled);
        assert!(!Realm::soft_delete("missing@example.com").await?);

        Ok(())
    }

    #[test]
    fn test_realm_suspended_not_active() {
        let realm = Realm {
            status: RealmStatus::Suspended,
            ..Default::default()
        };
        assert!(!realm.is_active());
    }

    #[test]
    fn test_realm_disabled_not_active() {
        let realm = Realm {
            enabled: false,
            ..Default::default()
        };
        assert!(!realm.is_active());
    }
}
