//! Issuer signing public key cache
//!
//! Provides a local SQLite cache for the verifier, avoiding repeated retrieval of Ed25519 public keys from Issuer registration responses.
//! Each Realm has its own issuer key, so entries are indexed by `(realm, key_id)`: a key is
//! only ever looked up for the Realm it belongs to.

use crate::aid::credential::error::AidError;
use base64::prelude::*;
use ed25519_dalek::VerifyingKey;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Issuer signing public key cache manager
///
/// Caches Issuer Ed25519 verifying keys, indexed by `(realm, key_id)`.
/// Used by signaling and other services to look up a Realm's public key by key_id when verifying AIdCredential.
#[derive(Debug, Clone)]
pub struct KeyCache {
    pool: SqlitePool,
    last_cleanup_time: Arc<Mutex<u64>>,
}

impl KeyCache {
    /// Create a new key cache instance and initialize the SQLite table
    pub async fn new<P: AsRef<Path>>(cache_db_file: P) -> Result<Self, AidError> {
        let database_url = format!("sqlite:{}", cache_db_file.as_ref().display());
        let options = SqliteConnectOptions::from_str(&database_url)
            .map_err(|e| AidError::DecodeFailure(format!("invalid database URL: {e}")))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| {
                AidError::DecodeFailure(format!("failed to open key cache database: {e}"))
            })?;

        let cache = Self {
            pool,
            last_cleanup_time: Arc::new(Mutex::new(0)),
        };

        cache.init_tables().await?;

        crate::recording::info!(
            "Issuer public key cache initialized, database: {}",
            cache_db_file.as_ref().display()
        );
        Ok(cache)
    }

    /// Initialize cache database tables
    async fn init_tables(&self) -> Result<(), AidError> {
        // An older cache keyed keys by key_id alone. It is only a cache, so drop
        // it; the Issuer writes each Realm's key again when it next signs.
        let has_realm: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM pragma_table_info('key_cache') WHERE name = 'realm'")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    AidError::DecodeFailure(format!("failed to inspect key_cache table: {e}"))
                })?;
        if has_realm.is_none() {
            sqlx::query("DROP TABLE IF EXISTS key_cache")
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    AidError::DecodeFailure(format!("failed to drop old key_cache table: {e}"))
                })?;
        }

        // key_cache table: stores Ed25519 verifying key (base64-encoded, 32 bytes) and expiration time
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_cache (
                realm      TEXT NOT NULL,
                key_id     INTEGER NOT NULL,
                pubkey     TEXT NOT NULL,
                cached_at  INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (realm, key_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AidError::DecodeFailure(format!("failed to create key_cache table: {e}")))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_cache_expires_at ON key_cache(expires_at)")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AidError::DecodeFailure(format!("failed to create key_cache index: {e}"))
            })?;

        crate::recording::debug!("Issuer public key cache database tables ready");
        Ok(())
    }

    /// Get `realm`'s Ed25519 verifying key `key_id` from the cache
    ///
    /// Returns `(VerifyingKey, expires_at)`; returns `None` if the cache entry does not exist or has expired.
    pub async fn get_cached_key(
        &self,
        realm: &str,
        key_id: u32,
    ) -> Result<Option<(VerifyingKey, u64)>, AidError> {
        self.maybe_cleanup().await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let result = sqlx::query(
            "SELECT pubkey, expires_at FROM key_cache WHERE realm = ?1 AND key_id = ?2",
        )
        .bind(realm)
        .bind(key_id as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            crate::recording::error!(
                "Failed to query Issuer public key cache: key_id={}, {}",
                key_id,
                e
            );
            AidError::DecodeFailure(format!("key cache query error: {e}"))
        })?;

        match result {
            Some(row) => {
                let pubkey_b64: String = row.try_get("pubkey").map_err(|e| {
                    AidError::DecodeFailure(format!("failed to read pubkey column: {e}"))
                })?;
                let expires_at: i64 = row.try_get("expires_at").map_err(|e| {
                    AidError::DecodeFailure(format!("failed to read expires_at column: {e}"))
                })?;

                // Cache entry expired, delete and return None
                if expires_at > 0 && (expires_at as u64) <= now {
                    crate::recording::debug!(
                        "Issuer public key cache expired, deleting: key_id={}",
                        key_id
                    );
                    let _ = sqlx::query("DELETE FROM key_cache WHERE realm = ?1 AND key_id = ?2")
                        .bind(realm)
                        .bind(key_id as i64)
                        .execute(&self.pool)
                        .await;
                    return Ok(None);
                }

                // base64 decode -> 32 bytes -> VerifyingKey
                let pubkey_bytes = BASE64_STANDARD.decode(&pubkey_b64).map_err(|e| {
                    AidError::DecodeFailure(format!("failed to base64 decode pubkey: {e}"))
                })?;

                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    AidError::DecodeFailure(
                        "Invalid pubkey length, expected 32 bytes (Ed25519 VerifyingKey)"
                            .to_string(),
                    )
                })?;

                let verifying_key = VerifyingKey::from_bytes(&pubkey_array).map_err(|e| {
                    AidError::DecodeFailure(format!("invalid Ed25519 verifying key: {e}"))
                })?;

                crate::recording::debug!("Issuer public key cache hit: key_id={}", key_id);
                Ok(Some((verifying_key, expires_at as u64)))
            }
            None => {
                crate::recording::debug!("Issuer public key cache miss: key_id={}", key_id);
                Ok(None)
            }
        }
    }

    /// Write `realm`'s Ed25519 verifying key to the cache
    ///
    /// `expires_at` is a Unix second timestamp; pass 0 to indicate never expires.
    pub async fn cache_key(
        &self,
        realm: &str,
        key_id: u32,
        verifying_key: &VerifyingKey,
        expires_at: u64,
    ) -> Result<(), AidError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let pubkey_b64 = BASE64_STANDARD.encode(verifying_key.as_bytes());

        sqlx::query(
            "REPLACE INTO key_cache (realm, key_id, pubkey, cached_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(realm)
        .bind(key_id as i64)
        .bind(&pubkey_b64)
        .bind(now as i64)
        .bind(expires_at as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| AidError::DecodeFailure(format!("failed to write key cache: {e}")))?;

        crate::recording::debug!(
            "Issuer public key written to cache: key_id={}, expires_at={}",
            key_id,
            expires_at
        );
        Ok(())
    }

    /// Remove entries absent from an accepted Profile, including a mismatched
    /// public key under the same id. Other Realms are unaffected.
    pub async fn retain_profile_keys(
        &self,
        profile: &actr_protocol::realm_profile::RealmProfile,
    ) -> Result<(), AidError> {
        let mut query =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("DELETE FROM key_cache WHERE realm = ");
        query.push_bind(&profile.realm.name);
        for key in &profile.issuer.keys {
            if let Some(id) = actr_protocol::realm_profile::parse_issuer_key_id(&key.id) {
                query
                    .push(" AND NOT (key_id = ")
                    .push_bind(i64::from(id))
                    .push(" AND pubkey = ")
                    .push_bind(&key.public_key)
                    .push(")");
            }
        }
        query
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| AidError::DecodeFailure(format!("failed to prune key cache: {e}")))?;
        Ok(())
    }

    /// Clean up all expired cache entries
    pub async fn cleanup_expired_keys(&self) -> Result<u32, AidError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let result = sqlx::query("DELETE FROM key_cache WHERE expires_at > 0 AND expires_at < ?1")
            .bind(now as i64)
            .execute(&self.pool)
            .await
            .map_err(|e| AidError::DecodeFailure(format!("failed to cleanup expired keys: {e}")))?;

        let deleted = result.rows_affected() as u32;
        if deleted > 0 {
            crate::recording::info!(
                "Issuer public key cache: cleaned up {} expired records",
                deleted
            );
        }
        Ok(deleted)
    }

    /// Returns the total number of entries in the cache (for monitoring/debugging)
    pub async fn get_cached_key_count(&self) -> Result<u32, AidError> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM key_cache")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AidError::DecodeFailure(format!("failed to count cached keys: {e}")))?;

        let count: i64 = row
            .try_get("count")
            .map_err(|e| AidError::DecodeFailure(format!("failed to read count: {e}")))?;

        Ok(count as u32)
    }

    /// Internal: trigger expired entry cleanup once per hour
    async fn maybe_cleanup(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let should_cleanup = {
            let last = self.last_cleanup_time.lock().unwrap();
            now.saturating_sub(*last) >= 3600
        };

        if should_cleanup {
            crate::recording::debug!("Triggering scheduled Issuer public key cache cleanup");
            match self.cleanup_expired_keys().await {
                Ok(n) if n > 0 => crate::recording::info!(
                    "Issuer public key cache cleanup complete, deleted {} entries",
                    n
                ),
                Ok(_) => {}
                Err(e) => crate::recording::error!("Issuer public key cache cleanup failed: {}", e),
            }
            let mut last = self.last_cleanup_time.lock().unwrap();
            *last = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use tempfile::tempdir;

    const REALM: &str = "k7m2q9xh@example.com";

    fn generate_test_key() -> VerifyingKey {
        let signing_key = SigningKey::generate(&mut OsRng);
        signing_key.verifying_key()
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let temp_dir = tempdir().unwrap();
        let cache = KeyCache::new(temp_dir.path().join("test.db")).await;
        assert!(cache.is_ok());
    }

    #[tokio::test]
    async fn test_key_caching_and_retrieval() {
        let temp_dir = tempdir().unwrap();
        let cache = KeyCache::new(temp_dir.path().join("test.db"))
            .await
            .unwrap();

        let verifying_key = generate_test_key();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        cache
            .cache_key(REALM, 1, &verifying_key, expires_at)
            .await
            .unwrap();
        assert_eq!(cache.get_cached_key_count().await.unwrap(), 1);

        let cached = cache.get_cached_key(REALM, 1).await.unwrap();
        assert!(cached.is_some());
        let (retrieved_key, retrieved_expires) = cached.unwrap();
        assert_eq!(verifying_key.as_bytes(), retrieved_key.as_bytes());
        assert_eq!(expires_at, retrieved_expires);
    }

    #[tokio::test]
    async fn keys_are_looked_up_only_in_their_realm() {
        let temp_dir = tempdir().unwrap();
        let cache = KeyCache::new(temp_dir.path().join("test.db"))
            .await
            .unwrap();

        cache
            .cache_key(REALM, 1, &generate_test_key(), 0)
            .await
            .unwrap();
        assert!(cache.get_cached_key(REALM, 1).await.unwrap().is_some());
        assert!(
            cache
                .get_cached_key("other@example.com", 1)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn an_old_cache_keyed_by_key_id_alone_is_replaced() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        {
            let cache = KeyCache::new(&path).await.unwrap();
            sqlx::query("DROP TABLE key_cache")
                .execute(&cache.pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE key_cache (key_id INTEGER PRIMARY KEY, pubkey TEXT NOT NULL, \
                 cached_at INTEGER NOT NULL, expires_at INTEGER NOT NULL)",
            )
            .execute(&cache.pool)
            .await
            .unwrap();
        }

        let cache = KeyCache::new(&path).await.unwrap();
        cache
            .cache_key(REALM, 1, &generate_test_key(), 0)
            .await
            .unwrap();
        assert!(cache.get_cached_key(REALM, 1).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let temp_dir = tempdir().unwrap();
        let cache = KeyCache::new(temp_dir.path().join("test.db"))
            .await
            .unwrap();

        let verifying_key = generate_test_key();
        // Set an expired timestamp
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(1);

        cache
            .cache_key(REALM, 1, &verifying_key, expires_at)
            .await
            .unwrap();
        // Expired entry should return None
        let cached = cache.get_cached_key(REALM, 1).await.unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_cleanup() {
        let temp_dir = tempdir().unwrap();
        let cache = KeyCache::new(temp_dir.path().join("test.db"))
            .await
            .unwrap();

        let verifying_key = generate_test_key();
        let expired_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(1);

        cache
            .cache_key(REALM, 1, &verifying_key, expired_at)
            .await
            .unwrap();
        assert_eq!(cache.get_cached_key_count().await.unwrap(), 1);

        let cleaned = cache.cleanup_expired_keys().await.unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(cache.get_cached_key_count().await.unwrap(), 0);
    }
}
