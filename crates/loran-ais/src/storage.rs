//! AIS key storage
//!
//! # Functionality
//!
//! Provides a local cache of public keys fetched from the KS service, reducing network request frequency.
//!
//! # Data Model
//!
//! ```sql
//! CREATE TABLE current_key (
//!     id INTEGER PRIMARY KEY CHECK (id = 1),  -- single-row constraint
//!     key_id INTEGER NOT NULL,
//!     public_key TEXT NOT NULL,               -- Base64 encoded
//!     fetched_at INTEGER NOT NULL,            -- Unix timestamp
//!     expires_at INTEGER NOT NULL             -- Unix timestamp
//! )
//! ```
//!
//! # Refresh Strategy
//!
//! - **Early refresh**: trigger refresh 10 minutes before key expiry
//! - **Minting deadline**: an expired key is never usable for minting — the
//!   issuer rotates as soon as `expires_at` passes (minus a small clock-skew
//!   margin), because signaling drops verifying keys at `expires_at` with no
//!   tolerance
//! - **Tolerance window**: only governs how long KS keeps the key material
//!   readable; it does not extend minting usability on the AIS side
//!
//! # Thread Safety
//!
//! Uses a sqlx connection pool for native async concurrent access, with WAL mode enabled to improve read performance.
//!
//! # Example
//!
//! ```ignore
//! use ais::storage::{KeyStorage, KeyRecord};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let storage = KeyStorage::new("keys.db").await?;
//!
//! // Check if refresh is needed
//! if storage.should_refresh().await? {
//!     // Fetch a new key from KS and update
//!     let record = KeyRecord {
//!         key_id: 123,
//!         public_key: "base64_encoded_key".to_string(),
//!         fetched_at: 1234567890,
//!         expires_at: 1234571490,
//!     };
//!     storage.update_current_key(&record).await?;
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ========== Constants ==========

/// Key refresh advance time (seconds)
///
/// Triggers refresh this many seconds before key expiry to avoid availability issues at the moment of expiry
const KEY_REFRESH_ADVANCE_SECS: u64 = 600; // 10 minutes

/// Key record
#[derive(Debug, Clone)]
pub struct KeyRecord {
    /// Key ID
    pub key_id: u32,
    /// Public key (Base64 encoded)
    pub public_key: String,
    /// Fetched time (Unix timestamp)
    pub fetched_at: u64,
    /// Expiry time (Unix timestamp)
    pub expires_at: u64,
    /// Tolerance window (seconds)
    pub tolerance_seconds: u64,
}

/// Key storage (uses sqlx connection pool)
#[derive(Clone)]
pub struct KeyStorage {
    pool: SqlitePool,
}

impl KeyStorage {
    /// Create or open key storage
    ///
    /// Uses a sqlx connection pool with the following configuration:
    /// - Max connections: 10
    /// - WAL mode: improves concurrent read performance (4x)
    /// - Synchronous mode: NORMAL (balances performance and safety)
    pub async fn new<P: AsRef<Path>>(db_file: P) -> Result<Self> {
        // Create connection options and enable WAL mode
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite:{}", db_file.as_ref().display()))
                .context("Failed to parse SQLite URL")?
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(5));

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .context("Failed to connect to SQLite")?;

        // Initialize database schema
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS current_key (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                key_id INTEGER NOT NULL,
                public_key TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                tolerance_seconds INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .context("Failed to create current_key table")?;

        platform::recording::info!(
            "Key storage initialized with sqlx (max_connections=10, WAL mode enabled)"
        );
        Ok(Self { pool })
    }

    /// Get the current key
    pub async fn get_current_key(&self) -> Result<Option<KeyRecord>> {
        let row = sqlx::query_as::<_, (i64, String, i64, i64, i64)>(
            "SELECT key_id, public_key, fetched_at, expires_at, tolerance_seconds FROM current_key WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to query current key")?;

        let record = row.map(
            |(key_id, public_key, fetched_at, expires_at, tolerance_seconds)| KeyRecord {
                key_id: key_id as u32,
                public_key,
                fetched_at: fetched_at as u64,
                expires_at: expires_at as u64,
                tolerance_seconds: tolerance_seconds as u64,
            },
        );

        if let Some(ref key) = record {
            platform::recording::debug!(
                "Retrieved key record: key_id={}, expires_at={}",
                key.key_id,
                key.expires_at
            );
        }

        Ok(record)
    }

    /// Update the current key
    pub async fn update_current_key(&self, record: &KeyRecord) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO current_key (id, key_id, public_key, fetched_at, expires_at, tolerance_seconds)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        )
        .bind(record.key_id as i64)
        .bind(&record.public_key)
        .bind(record.fetched_at as i64)
        .bind(record.expires_at as i64)
        .bind(record.tolerance_seconds as i64)
        .execute(&self.pool)
        .await
        .context("Failed to update current key")?;

        platform::recording::debug!(
            "Updated current key: key_id={}, expires_at={}",
            record.key_id,
            record.expires_at
        );

        Ok(())
    }

    /// Check whether the key needs refreshing
    ///
    /// Returns true if:
    /// - No key exists
    /// - The key will expire within 10 minutes
    pub async fn should_refresh(&self) -> Result<bool> {
        let key = match self.get_current_key().await? {
            Some(key) => key,
            None => {
                platform::recording::debug!("No current key found, refresh needed");
                return Ok(true);
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Early refresh
        let refresh_threshold = key.expires_at.saturating_sub(KEY_REFRESH_ADVANCE_SECS);

        if now >= refresh_threshold {
            platform::recording::debug!(
                "Key refresh needed: now={}, expires_at={}, threshold={}",
                now,
                key.expires_at,
                refresh_threshold
            );
            Ok(true)
        } else {
            platform::recording::debug!(
                "Key still valid: now={}, expires_at={}, remaining={}s",
                now,
                key.expires_at,
                key.expires_at.saturating_sub(now)
            );
            Ok(false)
        }
    }

    /// Check whether the key has fully expired (beyond the tolerance window)
    ///
    /// Returns true if the key expired more than `tolerance_seconds` ago.
    ///
    /// Note: this reflects KS's key-*material* retention window, not minting
    /// usability. The issuer refuses to mint with any
    /// key past `expires_at` and rotates instead, regardless of the tolerance
    /// window; this method is kept for observability of the retention window.
    pub async fn is_expired_beyond_tolerance(&self) -> Result<bool> {
        let key = match self.get_current_key().await? {
            Some(key) => key,
            None => return Ok(true),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check if beyond the tolerance window (using the tolerance time fetched from KS)
        let expired_beyond = now > key.expires_at + key.tolerance_seconds;

        if expired_beyond {
            platform::recording::warn!(
                "Key expired beyond tolerance: now={}, expires_at={}, tolerance={}s",
                now,
                key.expires_at,
                key.tolerance_seconds
            );
        }

        Ok(expired_beyond)
    }

    /// Health check: execute a simple database query to validate the connection pool
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .context("Database health check failed")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_key_storage_basic() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = KeyStorage::new(temp_file.path()).await.unwrap();

        // Initial state: no key
        assert!(storage.get_current_key().await.unwrap().is_none());
        assert!(storage.should_refresh().await.unwrap());

        // Add key
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let record = KeyRecord {
            key_id: 123,
            public_key: "test_public_key".to_string(),
            fetched_at: now,
            expires_at: now + 3600,
            tolerance_seconds: 24 * 3600,
        };

        storage.update_current_key(&record).await.unwrap();

        // Verify
        let retrieved = storage.get_current_key().await.unwrap().unwrap();
        assert_eq!(retrieved.key_id, 123);
        assert_eq!(retrieved.public_key, "test_public_key");
        assert_eq!(retrieved.tolerance_seconds, 24 * 3600);
        assert!(!storage.should_refresh().await.unwrap());
        assert!(!storage.is_expired_beyond_tolerance().await.unwrap());
    }

    #[tokio::test]
    async fn test_key_refresh_threshold() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = KeyStorage::new(temp_file.path()).await.unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Key will expire in 5 minutes (less than the 10-minute threshold)
        let record = KeyRecord {
            key_id: 1,
            public_key: "test_key".to_string(),
            fetched_at: now,
            expires_at: now + 300, // 5 minutes
            tolerance_seconds: 24 * 3600,
        };

        storage.update_current_key(&record).await.unwrap();

        // Should need refresh
        assert!(storage.should_refresh().await.unwrap());
    }

    #[tokio::test]
    async fn test_expired_beyond_tolerance() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = KeyStorage::new(temp_file.path()).await.unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Key expired 25 hours ago
        let record = KeyRecord {
            key_id: 1,
            public_key: "expired_key".to_string(),
            fetched_at: now - 30 * 3600,
            expires_at: now - 25 * 3600,
            tolerance_seconds: 24 * 3600,
        };

        storage.update_current_key(&record).await.unwrap();

        // Should be beyond the tolerance window
        assert!(storage.is_expired_beyond_tolerance().await.unwrap());
    }
}
