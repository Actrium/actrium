//! SQLite Nonce Storage Implementation
//!
//! Provides a SQLite-based nonce storage implementation with async support via sqlx.

use anyhow::Result;
use nonce_auth::NonceError;
use nonce_auth::storage::{NonceEntry, NonceStorage, StorageStats};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

use super::db_nonce_entry::DbNonceEntry;

/// Current on-disk schema version for `nonce.db`, stored in `PRAGMA user_version`.
const NONCE_SCHEMA_VERSION: i64 = 1;

/// Interval between background nonce-table cleanup passes.
///
/// The nonce table is append-only on the write path (`set` only inserts), so
/// without periodic pruning it grows without bound. Five minutes keeps the
/// table small while adding negligible load.
const NONCE_CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

/// A sqlx-based implementation of NonceStorage for nonce-auth
///
/// Cloning is cheap: both the connection pool and the cleanup lock are shared
/// behind `Arc`, so clones operate on the same underlying database and lock.
#[derive(Clone)]
pub struct SqliteNonceStorage {
    pool: Arc<SqlitePool>,
    cleanup_lock: Arc<RwLock<()>>,
}

impl SqliteNonceStorage {
    /// Create a new nonce storage instance (sync, for non-async contexts)
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_file = db_path.as_ref().join("nonce.db");

        // Create a new runtime for initialization (only for non-async contexts)
        let rt = tokio::runtime::Runtime::new()?;
        let pool = rt.block_on(Self::init_pool(&db_file))?;

        Ok(Self {
            pool: Arc::new(pool),
            cleanup_lock: Arc::new(RwLock::new(())),
        })
    }

    /// Create a new nonce storage instance (async, for async contexts)
    pub async fn new_async<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_file = db_path.as_ref().join("nonce.db");
        let pool = Self::init_pool(&db_file).await?;

        Ok(Self {
            pool: Arc::new(pool),
            cleanup_lock: Arc::new(RwLock::new(())),
        })
    }

    async fn init_pool<P: AsRef<Path>>(db_file: P) -> Result<SqlitePool> {
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite:{}", db_file.as_ref().display()))?
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await?;

        // Schema version gate. Databases created before this stamp existed read
        // back as version 0, so accept both 0 (unversioned legacy or freshly
        // created) and the current version, and reject only a newer version this
        // binary does not understand. The stamp is written to the current version
        // after the idempotent DDL below, so fresh and legacy databases converge.
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await?;
        if version > NONCE_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported nonce database schema version {version}; expected at \
                 most {NONCE_SCHEMA_VERSION}"
            );
        }

        // Initialize database tables.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS nonce_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                nonce TEXT NOT NULL,
                context TEXT,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(nonce, context)
            )",
        )
        .execute(&pool)
        .await?;

        // Create indexes.
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_nonce_context ON nonce_entries(nonce, context)",
        )
        .execute(&pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_expires_at ON nonce_entries(expires_at)")
            .execute(&pool)
            .await?;

        // Stamp the schema version once the DDL is in place. Skip the write when
        // the database is already at the current version so a healthy startup
        // performs no header write.
        if version != NONCE_SCHEMA_VERSION {
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("PRAGMA user_version = ")
                .push(NONCE_SCHEMA_VERSION)
                .build()
                .execute(&pool)
                .await?;
        }

        Ok(pool)
    }

    /// Get a nonce entry from storage
    pub async fn get(
        &self,
        nonce: &str,
        context: Option<&str>,
    ) -> Result<Option<DbNonceEntry>, NonceError> {
        let result = if let Some(ctx) = context {
            sqlx::query_as::<_, (i64, String, Option<String>, i64, i64)>(
                "SELECT id, nonce, context, expires_at, created_at FROM nonce_entries WHERE nonce = ? AND context = ?",
            )
            .bind(nonce)
            .bind(ctx)
            .fetch_optional(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, (i64, String, Option<String>, i64, i64)>(
                "SELECT id, nonce, context, expires_at, created_at FROM nonce_entries WHERE nonce = ? AND context IS NULL",
            )
            .bind(nonce)
            .fetch_optional(&*self.pool)
            .await
        };

        match result {
            Ok(Some((id, nonce, context, expires_at, created_at))) => Ok(Some(DbNonceEntry {
                id: Some(id),
                nonce,
                context,
                expires_at,
                created_at,
            })),
            Ok(None) => Ok(None),
            Err(e) => Err(NonceError::from_storage_error(e)),
        }
    }
}

#[async_trait::async_trait]
impl NonceStorage for SqliteNonceStorage {
    async fn get(
        &self,
        nonce: &str,
        context: Option<&str>,
    ) -> Result<Option<NonceEntry>, NonceError> {
        let db_entry = self.get(nonce, context).await?;

        // Check if expired
        if let Some(entry) = db_entry {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            if entry.expires_at <= now {
                // Expired, return None
                return Ok(None);
            }

            Ok(Some(NonceEntry {
                nonce: entry.nonce,
                context: entry.context,
                created_at: entry.created_at,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set(
        &self,
        nonce: &str,
        context: Option<&str>,
        ttl: Duration,
    ) -> Result<(), NonceError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let expires_at = now + ttl.as_secs() as i64;

        let result = sqlx::query(
            "INSERT INTO nonce_entries (nonce, context, expires_at, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(nonce)
        .bind(context)
        .bind(expires_at)
        .bind(now)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE") => {
                Err(NonceError::DuplicateNonce)
            }
            Err(e) => Err(NonceError::from_storage_error(e)),
        }
    }

    async fn exists(&self, nonce: &str, context: Option<&str>) -> Result<bool, NonceError> {
        Ok(self.get(nonce, context).await?.is_some())
    }

    async fn cleanup_expired(&self, current_time: i64) -> Result<usize, NonceError> {
        // Use a lock to prevent concurrent cleanup
        let _lock = self.cleanup_lock.write().await;

        let result = sqlx::query("DELETE FROM nonce_entries WHERE expires_at < ?")
            .bind(current_time)
            .execute(&*self.pool)
            .await
            .map_err(NonceError::from_storage_error)?;

        Ok(result.rows_affected() as usize)
    }

    async fn get_stats(&self) -> Result<StorageStats, NonceError> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM nonce_entries")
            .fetch_one(&*self.pool)
            .await
            .map_err(NonceError::from_storage_error)?;

        Ok(StorageStats {
            total_records: total.0 as usize,
            backend_info: "SQLite (sqlx async)".to_string(),
        })
    }
}

/// Run a cancellation-aware background loop that periodically deletes expired
/// nonce rows from `storage`.
///
/// The loop wakes every [`NONCE_CLEANUP_INTERVAL`] and removes every row whose
/// `expires_at` is already in the past (reusing the [`NonceStorage::cleanup_expired`]
/// DELETE), then waits again. It exits cleanly as soon as `cancel` resolves, so
/// callers can pass any shutdown future (for example
/// `CancellationToken::cancelled_owned()`).
pub async fn run_nonce_cleanup(
    storage: Arc<SqliteNonceStorage>,
    cancel: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let mut ticker = tokio::time::interval(NONCE_CLEANUP_INTERVAL);
    // Drop missed ticks instead of firing a burst after a long stall.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tokio::pin!(cancel);

    loop {
        tokio::select! {
            _ = &mut cancel => {
                crate::recording::debug!("Nonce cleanup loop cancelled");
                break;
            }
            _ = ticker.tick() => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                match NonceStorage::cleanup_expired(&*storage, now).await {
                    Ok(removed) if removed > 0 => {
                        crate::recording::info!(
                            "Nonce cleanup removed {} expired entries",
                            removed
                        );
                    }
                    Ok(_) => {
                        crate::recording::debug!("Nonce cleanup found no expired entries");
                    }
                    Err(e) => {
                        crate::recording::warn!("Nonce cleanup failed: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_basic_storage() {
        let temp_dir = tempdir().unwrap();
        let storage = SqliteNonceStorage::new_async(temp_dir.path())
            .await
            .unwrap();

        // Set nonce with TTL
        storage
            .set(
                "test_nonce",
                Some("test_context"),
                Duration::from_secs(3600),
            )
            .await
            .unwrap();

        // Get should return the nonce
        let result = storage
            .get("test_nonce", Some("test_context"))
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().nonce, "test_nonce");

        // Duplicate should fail
        assert!(matches!(
            storage
                .set(
                    "test_nonce",
                    Some("test_context"),
                    Duration::from_secs(3600)
                )
                .await,
            Err(NonceError::DuplicateNonce)
        ));
    }

    #[tokio::test]
    async fn test_cleanup() {
        let temp_dir = tempdir().unwrap();
        let storage = SqliteNonceStorage::new_async(temp_dir.path())
            .await
            .unwrap();

        let before_insert = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!("Before insert: {before_insert}");

        // Add a nonce about to expire (1 second TTL)
        storage
            .set("expired", None, Duration::from_secs(1))
            .await
            .unwrap();

        // Add a non-expired nonce (1 hour TTL)
        storage
            .set("valid", None, Duration::from_secs(3600))
            .await
            .unwrap();

        // Wait to ensure the first one expires (wait 2 seconds)
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let after_sleep = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!("After sleep: {after_sleep}");
        println!("Time elapsed: {} seconds", after_sleep - before_insert);

        // Call through the NonceStorage trait (checks expiry)
        use nonce_auth::storage::NonceStorage;
        let expired_entry = NonceStorage::get(&storage, "expired", None).await.unwrap();
        println!("Expired entry after trait call: {expired_entry:?}");

        // Verify: expired nonce should not be found (trait method checks expiry internally)
        assert!(
            expired_entry.is_none(),
            "Expected expired nonce to return None"
        );

        let valid_entry = NonceStorage::get(&storage, "valid", None).await.unwrap();
        assert!(valid_entry.is_some(), "Expected valid nonce to be present");
    }

    #[tokio::test]
    async fn test_stats() {
        let temp_dir = tempdir().unwrap();
        let storage = SqliteNonceStorage::new_async(temp_dir.path())
            .await
            .unwrap();

        // Add some nonces
        storage
            .set("n1", None, Duration::from_secs(1))
            .await
            .unwrap();
        storage
            .set("n2", None, Duration::from_secs(3600))
            .await
            .unwrap();

        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.total_records, 2);
        assert!(stats.backend_info.contains("SQLite"));
    }
}
