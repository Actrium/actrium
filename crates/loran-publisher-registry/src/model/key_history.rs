use crate::PublisherRegistryError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Status of a historical key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyHistoryStatus {
    /// Normal retirement: key rotated as part of regular lifecycle. Still valid for verifying old packages.
    Retired,
    /// Emergency revocation: private key compromised. Reject ALL verification attempts with this key.
    Revoked,
}

impl std::fmt::Display for KeyHistoryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retired => write!(f, "retired"),
            Self::Revoked => write!(f, "revoked"),
        }
    }
}

impl KeyHistoryStatus {
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "revoked" => Self::Revoked,
            _ => Self::Retired,
        }
    }
}

/// A retired Publisher signing key, preserved for historical verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherKeyHistory {
    pub id: i64,
    #[serde(rename = "publisher_id")]
    pub publisher_id: i64,
    pub key_id: String,
    pub public_key: String,
    pub status: KeyHistoryStatus,
    pub created_at: i64,
    pub retired_at: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for PublisherKeyHistory {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let status_str: String = row
            .try_get("status")
            .unwrap_or_else(|_| "retired".to_string());
        Ok(PublisherKeyHistory {
            id: row.try_get("id")?,
            publisher_id: row.try_get("publisher_id")?,
            key_id: row.try_get("key_id")?,
            public_key: row.try_get("public_key")?,
            status: KeyHistoryStatus::from_str_lossy(&status_str),
            created_at: row.try_get("created_at")?,
            retired_at: row.try_get("retired_at")?,
        })
    }
}

impl PublisherKeyHistory {
    /// Archive a retired key into the history table.
    pub async fn archive(
        pool: &SqlitePool,
        publisher_id: i64,
        key_id: &str,
        public_key: &str,
        created_at: i64,
    ) -> Result<(), PublisherRegistryError> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO publisher_key_history (publisher_id, key_id, public_key, status, created_at, retired_at)
             VALUES (?, ?, ?, 'retired', ?, ?)",
        )
        .bind(publisher_id)
        .bind(key_id)
        .bind(public_key)
        .bind(created_at)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Revoke a historical key (emergency: private key compromised).
    /// After revocation, verification requests using this key_id will be rejected.
    pub async fn revoke(pool: &SqlitePool, history_id: i64) -> Result<(), PublisherRegistryError> {
        let result =
            sqlx::query("UPDATE publisher_key_history SET status = 'revoked' WHERE id = ?")
                .bind(history_id)
                .execute(pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(PublisherRegistryError::NotFound);
        }
        Ok(())
    }

    /// Look up a specific historical key by publisher_id + key_id.
    /// Returns None if the key is not found OR if it has been revoked.
    ///
    /// `ORDER BY id DESC LIMIT 1` resolves the newest archival row unambiguously
    /// even if legacy data holds more than one row for the same (publisher_id, key_id).
    pub async fn get_by_key_id(
        pool: &SqlitePool,
        publisher_id: i64,
        key_id: &str,
    ) -> Result<Option<Self>, PublisherRegistryError> {
        let entry = sqlx::query_as::<_, PublisherKeyHistory>(
            "SELECT * FROM publisher_key_history WHERE publisher_id = ? AND key_id = ?
             ORDER BY id DESC LIMIT 1",
        )
        .bind(publisher_id)
        .bind(key_id)
        .fetch_optional(pool)
        .await?;
        // Reject revoked keys: treat as if they don't exist
        match entry {
            Some(ref e) if e.status == KeyHistoryStatus::Revoked => {
                platform::recording::warn!(
                    "rejected lookup for revoked historical key: key_id={}, publisher_id={}",
                    key_id,
                    publisher_id
                );
                Err(PublisherRegistryError::KeyRevoked(key_id.to_string()))
            }
            other => Ok(other),
        }
    }

    /// Look up a specific historical key by publisher_id + key_id, regardless of status.
    /// Used by admin endpoints that need to see revoked keys.
    pub async fn get_by_key_id_unfiltered(
        pool: &SqlitePool,
        publisher_id: i64,
        key_id: &str,
    ) -> Result<Option<Self>, PublisherRegistryError> {
        Ok(sqlx::query_as::<_, PublisherKeyHistory>(
            "SELECT * FROM publisher_key_history WHERE publisher_id = ? AND key_id = ?
             ORDER BY id DESC LIMIT 1",
        )
        .bind(publisher_id)
        .bind(key_id)
        .fetch_optional(pool)
        .await?)
    }

    /// Blacklist check: has this (publisher_id, key_id) ever been archived into the
    /// history table, in any status (retired or revoked)?
    ///
    /// Unlike `get_by_key_id`, this does NOT treat revoked rows as absent, so it
    /// is the correct primitive for refusing to reinstate a previously used
    /// publisher key. A key that was rotated away from (retired) or emergency
    /// revoked must never become the current key again.
    pub async fn key_id_ever_used(
        executor: impl sqlx::SqliteExecutor<'_>,
        publisher_id: i64,
        key_id: &str,
    ) -> Result<bool, PublisherRegistryError> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM publisher_key_history WHERE publisher_id = ? AND key_id = ? LIMIT 1",
        )
        .bind(publisher_id)
        .bind(key_id)
        .fetch_optional(executor)
        .await?;
        Ok(found.is_some())
    }

    /// Is the most recent history row for this (publisher_id, key_id) revoked?
    ///
    /// Returns false when the key_id has no history row at all. Used to fail an
    /// exact-key resolution closed even on the current-key fast path.
    pub async fn is_key_id_revoked(
        pool: &SqlitePool,
        publisher_id: i64,
        key_id: &str,
    ) -> Result<bool, PublisherRegistryError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM publisher_key_history WHERE publisher_id = ? AND key_id = ?
             ORDER BY id DESC LIMIT 1",
        )
        .bind(publisher_id)
        .bind(key_id)
        .fetch_optional(pool)
        .await?;
        Ok(matches!(status.as_deref(), Some("revoked")))
    }

    /// List all historical keys for an Publisher Registry (most recent first).
    pub async fn list_by_publisher(
        pool: &SqlitePool,
        publisher_id: i64,
    ) -> Result<Vec<Self>, PublisherRegistryError> {
        Ok(sqlx::query_as::<_, PublisherKeyHistory>(
            "SELECT * FROM publisher_key_history WHERE publisher_id = ? ORDER BY retired_at DESC",
        )
        .bind(publisher_id)
        .fetch_all(pool)
        .await?)
    }

    /// Delete all history for an Publisher Registry (called when Publisher Registry is deleted).
    pub async fn delete_by_publisher(
        pool: &SqlitePool,
        publisher_id: i64,
    ) -> Result<(), PublisherRegistryError> {
        sqlx::query("DELETE FROM publisher_key_history WHERE publisher_id = ?")
            .bind(publisher_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
