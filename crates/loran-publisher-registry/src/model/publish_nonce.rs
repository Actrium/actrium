//! Publish nonce for Challenge-Response authentication on `/registry/packages/publish`.
//!
//! Flow:
//! 1. CLI requests a nonce via `POST /registry/packages/nonce`
//! 2. Server generates a 32-byte random nonce, stores it with status=pending and TTL=5min
//! 3. CLI signs `"ACTR-PUBLISH:{publisher}:{hex(nonce)}:{sha256(manifest)}"` with Publisher Registry private key
//! 4. CLI sends the nonce + nonce_sig in the publish request body
//! 5. Server verifies: nonce exists + pending + not expired → verify signature → mark used

use crate::PublisherRegistryError;
use chrono::Utc;
use sqlx::SqlitePool;

/// Nonce TTL in seconds (5 minutes).
const NONCE_TTL_SECS: i64 = 300;

/// Publish nonce record stored in the database.
#[derive(Debug, Clone)]
pub struct PublishNonce {
    pub id: i64,
    pub publisher_id: i64,
    pub nonce: Vec<u8>,
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for PublishNonce {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(PublishNonce {
            id: row.try_get("id")?,
            publisher_id: row.try_get("publisher_id")?,
            nonce: row.try_get("nonce")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            expires_at: row.try_get("expires_at")?,
        })
    }
}

impl PublishNonce {
    /// Create a new pending nonce for the given Publisher Registry.
    ///
    /// Returns the raw 32-byte nonce value.
    pub async fn create(
        pool: &SqlitePool,
        publisher_id: i64,
    ) -> Result<Vec<u8>, PublisherRegistryError> {
        use rand::RngCore;

        let mut nonce_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let now = Utc::now().timestamp();
        let expires_at = now + NONCE_TTL_SECS;

        sqlx::query(
            "INSERT INTO publisher_publish_nonce (publisher_id, nonce, status, created_at, expires_at)
             VALUES (?, ?, 'pending', ?, ?)",
        )
        .bind(publisher_id)
        .bind(&nonce_bytes[..])
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await?;

        Ok(nonce_bytes.to_vec())
    }

    /// Look up a pending, non-expired nonce without consuming it.
    ///
    /// Returns the nonce record if found, pending, and not expired.
    /// Does NOT modify the nonce status — call `consume()` after signature verification.
    pub async fn find_pending(
        pool: &SqlitePool,
        nonce_bytes: &[u8],
    ) -> Result<PublishNonce, PublisherRegistryError> {
        let now = Utc::now().timestamp();

        let entry = sqlx::query_as::<_, PublishNonce>(
            "SELECT * FROM publisher_publish_nonce WHERE nonce = ? LIMIT 1",
        )
        .bind(nonce_bytes)
        .fetch_optional(pool)
        .await?;

        let entry = match entry {
            Some(e) => e,
            None => {
                platform::recording::warn!("publish nonce not found");
                return Err(PublisherRegistryError::Unauthorized);
            }
        };

        if entry.status != "pending" {
            platform::recording::warn!(
                "publish nonce already consumed: nonce_id={}, status={}",
                entry.id,
                entry.status
            );
            return Err(PublisherRegistryError::Unauthorized);
        }

        if now > entry.expires_at {
            platform::recording::warn!("publish nonce expired: nonce_id={}", entry.id);
            return Err(PublisherRegistryError::Unauthorized);
        }

        Ok(entry)
    }

    /// Atomically consume a pending nonce.
    ///
    /// Uses `UPDATE ... WHERE status = 'pending'` as an optimistic lock.
    /// Returns Ok(()) if exactly one row was updated, or Err(Unauthorized) if
    /// the nonce was already consumed by a concurrent request.
    pub async fn consume(pool: &SqlitePool, nonce_id: i64) -> Result<(), PublisherRegistryError> {
        let result = sqlx::query(
            "UPDATE publisher_publish_nonce SET status = 'used' WHERE id = ? AND status = 'pending'",
        )
        .bind(nonce_id)
        .execute(pool)
        .await?;

        if result.rows_affected() != 1 {
            platform::recording::warn!("publish nonce consume race: nonce_id={}", nonce_id);
            return Err(PublisherRegistryError::Unauthorized);
        }

        Ok(())
    }

    /// Clean up old nonce records.
    ///
    /// Deletes records that have been expired for longer than `retain_secs`.
    /// - `retain_secs`: how long to keep expired/used records for auditing (default: 86400 = 24h)
    pub async fn cleanup(
        pool: &SqlitePool,
        retain_secs: i64,
    ) -> Result<u64, PublisherRegistryError> {
        let cutoff = Utc::now().timestamp() - retain_secs;
        let result = sqlx::query("DELETE FROM publisher_publish_nonce WHERE expires_at < ?")
            .bind(cutoff)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
