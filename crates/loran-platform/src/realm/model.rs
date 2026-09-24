//! Realm core data structures and database operations
//!
//! Defines the core data structures and database CRUD operations for the Realm entity

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum::{Display, EnumString};

use super::error::RealmError;
use crate::storage::db::get_database;

/// Realm lifecycle status.
///
/// `pending` waits for its first Realm Profile, `active` serves Actors, and
/// `retired` is final: the row stays as the Realm Provider tombstone so the
/// name is never reused. The operator's local suspend switch is `enabled`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Display, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum RealmStatus {
    Pending,
    #[default]
    Active,
    Retired,
}

/// Realm is an administrative and cryptographic trust boundary hosted by
/// this Actrix instance, identified by its Realm name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Realm {
    /// Realm name `<realm_id>@<domain>`, the primary key; never changes
    pub name: String,
    pub status: RealmStatus,
    /// Operator suspend switch; a suspended Realm serves no Actor
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
    /// Create a new active realm.
    ///
    /// `secret_hash` is the SHA256 hash of the realm secret (must be non-empty).
    /// `name` must be a normalized Realm name `<realm_id>@<domain>`.
    pub async fn create(name: String, secret_hash: String) -> Result<Self, RealmError> {
        Self {
            name,
            secret_current: secret_hash,
            ..Default::default()
        }
        .insert(None)
        .await
    }

    /// Insert this realm as a new active Realm.
    ///
    /// `max_realms` bounds the number of pending and active Realms. A name that
    /// exists, retired or not, is rejected: Realm names are never reused.
    pub async fn insert(mut self, max_realms: Option<u32>) -> Result<Self, RealmError> {
        validate_name(&self.name)?;
        if self.secret_current.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "realm secret hash must not be empty".to_string(),
            ));
        }
        let now = Utc::now().timestamp();
        let (prev_hash, prev_valid_until) = previous_columns(&self.secret_previous);

        let result = sqlx::query(
            "INSERT INTO realm (
                name, status, enabled, expires_at, created_at,
                secret_current, secret_previous_hash, secret_previous_valid_until
             )
             SELECT ?1, 'active', ?2, ?3, ?4, ?5, ?6, ?7
             WHERE ?8 IS NULL
                OR (SELECT COUNT(*) FROM realm WHERE status != 'retired') < ?8",
        )
        .bind(&self.name)
        .bind(self.enabled as i32)
        .bind(self.expires_at.map(|v| v as i64))
        .bind(now)
        .bind(&self.secret_current)
        .bind(prev_hash)
        .bind(prev_valid_until)
        .bind(max_realms.map(i64::from))
        .execute(get_database().get_pool())
        .await;

        match result {
            // The limit filtered the row out before the primary key was
            // checked; an existing name still reports as never reusable.
            Ok(done) if done.rows_affected() == 0 => match Self::get(&self.name).await? {
                Some(_) => Err(RealmError::AlreadyExists),
                None => Err(RealmError::ValidationError(format!(
                    "this instance already hosts max_realms = {} realms",
                    max_realms.unwrap_or_default()
                ))),
            },
            Ok(_) => {
                self.status = RealmStatus::Active;
                self.created_at = now as u64;
                self.updated_at = None;
                Ok(self)
            }
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(RealmError::AlreadyExists)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Create or update a superv-managed realm by name.
    ///
    /// Only the enabled switch, expiry and secret hashes are synced; a retired
    /// realm is never touched.
    pub async fn upsert_managed(
        name: String,
        enabled: bool,
        expires_at: Option<u64>,
        secret_current: String,
        secret_previous: Option<(String, u64)>,
        max_realms: Option<u32>,
    ) -> Result<Self, RealmError> {
        if secret_current.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "managed realm secret_current must not be empty".to_string(),
            ));
        }
        let secret_previous = secret_previous.filter(|(hash, _)| !hash.trim().is_empty());

        let Some(mut realm) = Self::get(&name).await? else {
            return Self {
                name,
                enabled,
                expires_at,
                secret_current,
                secret_previous,
                ..Default::default()
            }
            .insert(max_realms)
            .await;
        };
        realm.enabled = enabled;
        realm.expires_at = expires_at;
        realm.secret_current = secret_current;
        realm.secret_previous = secret_previous;
        realm.save().await?;
        Ok(realm)
    }

    /// Save the mutable fields of an existing, non-retired realm.
    ///
    /// Name and status never change here: the name is immutable and status
    /// moves only through [`Realm::retire`].
    pub async fn save(&mut self) -> Result<(), RealmError> {
        // Enforce the non-empty secret invariant here as well, so no code path
        // can silently persist an empty secret. This mirrors the storage-layer
        // CHECK constraint and the validation in `insert` / `upsert_managed`.
        if self.secret_current.trim().is_empty() {
            return Err(RealmError::ValidationError(
                "realm secret_current must not be empty".to_string(),
            ));
        }

        let now = Utc::now().timestamp();
        let (prev_hash, prev_valid_until) = previous_columns(&self.secret_previous);

        let result = sqlx::query(
            "UPDATE realm SET enabled = ?, expires_at = ?,
             updated_at = ?, secret_current = ?,
             secret_previous_hash = ?, secret_previous_valid_until = ?
             WHERE name = ? AND status != 'retired'",
        )
        .bind(self.enabled as i32)
        .bind(self.expires_at.map(|v| v as i64))
        .bind(now)
        .bind(&self.secret_current)
        .bind(prev_hash)
        .bind(prev_valid_until)
        .bind(&self.name)
        .execute(get_database().get_pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(match Self::get(&self.name).await? {
                Some(_) => RealmError::Retired,
                None => RealmError::NotFound,
            });
        }

        self.updated_at = Some(now as u64);
        Ok(())
    }

    /// Get a realm by its Realm name.
    pub async fn get(name: &str) -> Result<Option<Self>, RealmError> {
        let result = sqlx::query_as::<_, Realm>(
            "SELECT name, status, enabled, expires_at, created_at, updated_at,
                    secret_current, secret_previous_hash, secret_previous_valid_until
             FROM realm WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(get_database().get_pool())
        .await?;

        Ok(result)
    }

    /// Get all realms.
    pub async fn get_all() -> Result<Vec<Self>, RealmError> {
        let realms = sqlx::query_as::<_, Realm>(
            "SELECT name, status, enabled, expires_at, created_at, updated_at,
                    secret_current, secret_previous_hash, secret_previous_valid_until
             FROM realm ORDER BY created_at, name",
        )
        .fetch_all(get_database().get_pool())
        .await?;

        Ok(realms)
    }

    /// Suspend (`false`) or resume (`true`) a realm. Retired realms are final.
    pub async fn set_enabled(name: &str, enabled: bool) -> Result<Self, RealmError> {
        let mut realm = Self::get(name).await?.ok_or(RealmError::NotFound)?;
        realm.enabled = enabled;
        realm.save().await?;
        Ok(realm)
    }

    /// Retire a realm in one transaction.
    ///
    /// The row stays as the tombstone that keeps the name from being reused.
    /// Connection ACL rules, pending registrations and renewal tokens of the
    /// realm are removed; the issued identity ledger and revocation list stay.
    pub async fn retire(name: &str) -> Result<Self, RealmError> {
        let mut tx = get_database().get_pool().begin().await?;
        let now = Utc::now().timestamp();

        let retired = sqlx::query(
            "UPDATE realm SET status = 'retired', enabled = 0, updated_at = ?,
                pending_expires_at = NULL, admin_token_hash = NULL
             WHERE name = ? AND status != 'retired'",
        )
        .bind(now)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        if retired.rows_affected() == 0 {
            let exists: Option<String> =
                sqlx::query_scalar("SELECT name FROM realm WHERE name = ?")
                    .bind(name)
                    .fetch_optional(&mut *tx)
                    .await?;
            return Err(match exists {
                Some(_) => RealmError::Retired,
                None => RealmError::NotFound,
            });
        }

        sqlx::query("DELETE FROM actoracl WHERE realm = ?1 OR source_realm = ?1")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM pending_registration WHERE realm = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        // ActorId text is `<serial_hex>.<realm>`; the serial has no dot.
        sqlx::query(
            "DELETE FROM issuer_renewal_token
             WHERE substr(actor_id, instr(actor_id, '.') + 1) = ?",
        )
        .bind(name)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Self::get(name).await?.ok_or(RealmError::NotFound)
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
    let uri = actr_protocol::uri::parse_realm_name(name)
        .map_err(|e| RealmError::ValidationError(format!("invalid realm name: {e}")))?;
    if uri.is_local() {
        return Err(RealmError::ValidationError(
            "a local Realm (.actr.alt) is not hosted by a Realm Provider".to_string(),
        ));
    }
    Ok(())
}

fn previous_columns(previous: &Option<(String, u64)>) -> (Option<&str>, Option<i64>) {
    match previous {
        Some((hash, valid_until)) => (Some(hash.as_str()), Some(*valid_until as i64)),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test_utils::utils::setup_test_db;
    use serial_test::serial;

    #[test]
    fn test_realm_status_text() {
        for (status, text) in [
            (RealmStatus::Pending, "pending"),
            (RealmStatus::Active, "active"),
            (RealmStatus::Retired, "retired"),
        ] {
            assert_eq!(status.to_string(), text);
            assert_eq!(RealmStatus::from_str(text).unwrap(), status);
        }
        assert!(RealmStatus::from_str("suspended").is_err());
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

    #[test]
    fn test_pending_retired_and_disabled_are_not_active() {
        for realm in [
            Realm {
                status: RealmStatus::Pending,
                ..Default::default()
            },
            Realm {
                status: RealmStatus::Retired,
                ..Default::default()
            },
            Realm {
                enabled: false,
                ..Default::default()
            },
        ] {
            assert!(!realm.is_active());
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_many_realms_and_max_realms() -> anyhow::Result<()> {
        setup_test_db().await?;

        Realm::create("one@example.com".to_string(), "hash".to_string()).await?;
        Realm::create("two@acme.com".to_string(), "hash".to_string()).await?;
        assert_eq!(Realm::get_all().await?.len(), 2);

        let third = Realm {
            name: "three@example.com".to_string(),
            secret_current: "hash".to_string(),
            ..Default::default()
        };
        let limited = third.clone().insert(Some(2)).await;
        assert!(matches!(limited, Err(RealmError::ValidationError(_))));
        let existing = Realm {
            name: "one@example.com".to_string(),
            secret_current: "hash".to_string(),
            ..Default::default()
        };
        assert!(matches!(
            existing.insert(Some(2)).await,
            Err(RealmError::AlreadyExists)
        ));

        // Retired realms do not count toward max_realms.
        Realm::retire("one@example.com").await?;
        third.insert(Some(2)).await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_retired_name_is_never_reused() -> anyhow::Result<()> {
        setup_test_db().await?;

        Realm::create("gone@example.com".to_string(), "hash".to_string()).await?;
        let duplicate = Realm::create("gone@example.com".to_string(), "hash".to_string()).await;
        assert!(matches!(duplicate, Err(RealmError::AlreadyExists)));

        let retired = Realm::retire("gone@example.com").await?;
        assert_eq!(retired.status, RealmStatus::Retired);
        assert!(!retired.enabled);

        let reuse = Realm::create("gone@example.com".to_string(), "hash".to_string()).await;
        assert!(matches!(reuse, Err(RealmError::AlreadyExists)));
        assert!(matches!(
            Realm::retire("gone@example.com").await,
            Err(RealmError::Retired)
        ));
        assert!(matches!(
            Realm::set_enabled("gone@example.com", true).await,
            Err(RealmError::Retired)
        ));
        assert!(matches!(
            Realm::retire("missing@example.com").await,
            Err(RealmError::NotFound)
        ));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_retire_removes_realm_scoped_state() -> anyhow::Result<()> {
        setup_test_db().await?;
        let pool = get_database().get_pool();

        for name in ["old@example.com", "keep@example.com"] {
            Realm::create(name.to_string(), "hash".to_string()).await?;
            sqlx::query(
                "INSERT INTO actoracl (realm, source_realm, from_type, to_type, access)
                 VALUES (?1, ?1, 'acme/a@1', 'acme/b@1', 1)",
            )
            .bind(name)
            .execute(pool)
            .await?;
            sqlx::query(
                "INSERT INTO pending_registration (realm, serial_number, created_at)
                 VALUES (?, 1, 1)",
            )
            .bind(name)
            .execute(pool)
            .await?;
            sqlx::query(
                "INSERT INTO issued_actor_identity (realm, serial_number, actor_type, issued_at)
                 VALUES (?, 1, 'acme/a@1', 1)",
            )
            .bind(name)
            .execute(pool)
            .await?;
            sqlx::query(
                "INSERT INTO issuer_renewal_token (actor_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, 9999999999, 1)",
            )
            .bind(format!("1a.{name}"))
            .bind(name.as_bytes())
            .execute(pool)
            .await?;
        }

        Realm::retire("old@example.com").await?;

        for (table, column) in [
            ("actoracl", "realm"),
            ("pending_registration", "realm"),
            ("issued_actor_identity", "realm"),
        ] {
            let rows: Vec<String> =
                sqlx::query_scalar(&format!("SELECT {column} FROM {table} ORDER BY 1"))
                    .fetch_all(pool)
                    .await?;
            let expected: &[&str] = if table == "issued_actor_identity" {
                &["keep@example.com", "old@example.com"]
            } else {
                &["keep@example.com"]
            };
            assert_eq!(rows, expected, "{table}");
        }
        let tokens: Vec<String> = sqlx::query_scalar("SELECT actor_id FROM issuer_renewal_token")
            .fetch_all(pool)
            .await?;
        assert_eq!(tokens, ["1a.keep@example.com"]);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_suspend_and_resume() -> anyhow::Result<()> {
        setup_test_db().await?;

        Realm::create("pause@example.com".to_string(), "hash".to_string()).await?;
        let suspended = Realm::set_enabled("pause@example.com", false).await?;
        assert!(!suspended.enabled);
        assert_eq!(suspended.status, RealmStatus::Active);
        assert!(!suspended.is_active());
        let resumed = Realm::set_enabled("pause@example.com", true).await?;
        assert!(resumed.is_active());
        assert!(matches!(
            Realm::set_enabled("missing@example.com", false).await,
            Err(RealmError::NotFound)
        ));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_upsert_managed_updates_by_name() -> anyhow::Result<()> {
        setup_test_db().await?;

        let realm = Realm::upsert_managed(
            "managed-one@example.com".to_string(),
            true,
            Some(1_900_000_000),
            "hash-current".to_string(),
            None,
            None,
        )
        .await?;
        assert_eq!(realm.name, "managed-one@example.com");
        assert_eq!(realm.secret_current, "hash-current");

        let updated = Realm::upsert_managed(
            "managed-one@example.com".to_string(),
            false,
            Some(1_900_000_001),
            "hash-current-2".to_string(),
            Some(("hash-prev".to_string(), 1_800_000_000)),
            None,
        )
        .await?;
        assert_eq!(updated.status, RealmStatus::Active);
        assert!(!updated.enabled);
        assert_eq!(updated.secret_current, "hash-current-2");
        assert_eq!(
            updated.secret_previous,
            Some(("hash-prev".to_string(), 1_800_000_000))
        );

        // A second realm is allowed; a retired one is never touched again.
        Realm::upsert_managed(
            "managed-two@example.com".to_string(),
            true,
            None,
            "hash-other".to_string(),
            None,
            None,
        )
        .await?;
        Realm::retire("managed-two@example.com").await?;
        let revived = Realm::upsert_managed(
            "managed-two@example.com".to_string(),
            true,
            None,
            "hash-other".to_string(),
            None,
            None,
        )
        .await;
        assert!(matches!(revived, Err(RealmError::Retired)));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_rejects_invalid_realm_name() -> anyhow::Result<()> {
        setup_test_db().await?;

        for name in [
            "",
            "managed",
            "Managed@example.com",
            "a@example.com:443",
            "a@abcdefghijklmnopqrstuvwxyz.actr.alt",
        ] {
            let result = Realm::create(name.to_string(), "hash".to_string()).await;
            assert!(
                matches!(result, Err(RealmError::ValidationError(_))),
                "{name} should be rejected"
            );
        }

        Ok(())
    }
}
