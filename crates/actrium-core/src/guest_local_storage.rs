use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use thiserror::Error;

pub(crate) const MAX_KEY_BYTES: usize = 1024;
pub(crate) const MAX_VALUE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_ORIGIN_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ORIGIN_ENTRIES: usize = 1024;

#[derive(Debug, Error)]
pub enum GuestLocalStorageError {
    #[error("Local storage keys are limited to 1024 UTF-8 bytes.")]
    InvalidKey,
    #[error("Local storage values are limited to 64 KiB.")]
    InvalidValue,
    #[error("The application has exceeded its local storage quota.")]
    QuotaExceeded,
    #[error("Local storage database operation failed: {0}")]
    DatabaseFailure(String),
    #[error("Publisher verification failed: {0}")]
    InvalidPublisher(String),
    #[error("The Publisher identity changed without a verified rotation chain.")]
    IdentityChanged,
    #[error(
        "Existing storage has no verified Publisher history and requires explicit migration approval."
    )]
    MigrationRequired,
    #[error("Storage migration failed: {0}")]
    MigrationConflict(String),
    #[error("Local storage changed before the replacement could be saved.")]
    Conflict,
}

impl From<rusqlite::Error> for GuestLocalStorageError {
    fn from(error: rusqlite::Error) -> Self {
        database_failure(error)
    }
}

/// Owns a lazily opened SQLite connection for one local profile.
pub struct GuestLocalStorage {
    path: PathBuf,
    connection: Mutex<Option<Connection>>,
}

#[derive(Clone)]
pub struct GuestLocalStorageItem {
    pub key: String,
    pub value: String,
}

impl GuestLocalStorage {
    pub fn new(path: String) -> Self {
        Self {
            path: path.into(),
            connection: Mutex::new(None),
        }
    }

    pub fn get_item(
        &self,
        origin: String,
        key: String,
    ) -> Result<Option<String>, GuestLocalStorageError> {
        validate_key(&key)?;
        self.with_connection(|connection| {
            Ok(connection
                .query_row(
                    "SELECT value FROM guest_local_storage WHERE origin = ?1 AND key = ?2",
                    params![origin, key],
                    |row| row.get(0),
                )
                .optional()?)
        })
    }

    pub fn set_item(
        &self,
        origin: String,
        key: String,
        value: String,
    ) -> Result<(), GuestLocalStorageError> {
        validate_key(&key)?;
        if value.len() > MAX_VALUE_BYTES {
            return Err(GuestLocalStorageError::InvalidValue);
        }
        let size = key.len() + value.len();
        // The quota check and the write share one statement, including across connections.
        let changed = self.with_connection(|connection| {
            Ok(connection.execute(
                "INSERT INTO guest_local_storage (origin, key, value, size)
                 SELECT ?1, ?2, ?3, ?4
                 FROM (
                     SELECT COALESCE(SUM(size), 0) AS total,
                            COALESCE(MAX(CASE WHEN key = ?2 THEN size END), 0) AS previous,
                            COUNT(*) AS entries,
                            COALESCE(MAX(key = ?2), 0) AS replacing
                     FROM guest_local_storage WHERE origin = ?1
                 )
                 WHERE total - previous + ?4 <= ?5 AND (entries < ?6 OR replacing)
                 ON CONFLICT (origin, key) DO UPDATE SET value = excluded.value, size = excluded.size",
                params![origin, key, value, size, MAX_ORIGIN_BYTES, MAX_ORIGIN_ENTRIES],
            )?)
        })?;
        if changed == 0 {
            return Err(GuestLocalStorageError::QuotaExceeded);
        }
        Ok(())
    }

    pub fn remove_item(&self, origin: String, key: String) -> Result<(), GuestLocalStorageError> {
        validate_key(&key)?;
        self.with_connection(|connection| {
            connection.execute(
                "DELETE FROM guest_local_storage WHERE origin = ?1 AND key = ?2",
                params![origin, key],
            )?;
            Ok(())
        })
    }

    pub fn replace_items(
        &self,
        origin: String,
        key: String,
        expected: Option<String>,
        remove: Vec<String>,
        set: Vec<GuestLocalStorageItem>,
    ) -> Result<(), GuestLocalStorageError> {
        validate_key(&key)?;
        for key in remove.iter().chain(set.iter().map(|item| &item.key)) {
            validate_key(key)?;
        }
        if expected
            .iter()
            .chain(set.iter().map(|item| &item.value))
            .any(|value| value.len() > MAX_VALUE_BYTES)
        {
            return Err(GuestLocalStorageError::InvalidValue);
        }
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current: Option<String> = transaction.query_row(
                "SELECT value FROM guest_local_storage WHERE origin = ?1 AND key = ?2",
                params![origin, key],
                |row| row.get(0),
            ).optional()?;
            if current != expected {
                return Err(GuestLocalStorageError::Conflict);
            }
            for key in remove {
                transaction.execute(
                    "DELETE FROM guest_local_storage WHERE origin = ?1 AND key = ?2",
                    params![origin, key],
                )?;
            }
            for GuestLocalStorageItem { key, value } in set {
                let size = key.len() + value.len();
                transaction.execute(
                    "INSERT INTO guest_local_storage (origin, key, value, size) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (origin, key) DO UPDATE SET value = excluded.value, size = excluded.size",
                    params![origin, key, value, size],
                )?;
            }
            enforce_quota(&transaction, &origin)?;
            Ok(transaction.commit()?)
        })
    }

    fn open(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS guest_local_storage (
                origin TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                size INTEGER NOT NULL,
                PRIMARY KEY (origin, key)
            ) WITHOUT ROWID",
        )?;
        crate::publisher_storage::initialize(&connection)?;
        Ok(connection)
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, GuestLocalStorageError>,
    ) -> Result<T, GuestLocalStorageError> {
        let mut slot = self.connection.lock().map_err(|_| {
            GuestLocalStorageError::DatabaseFailure("connection lock is poisoned".into())
        })?;
        let connection = match &mut *slot {
            Some(connection) => connection,
            None => slot.insert(self.open().map_err(database_failure)?),
        };
        operation(connection)
    }
}

fn enforce_quota(connection: &Connection, origin: &str) -> Result<(), GuestLocalStorageError> {
    let (bytes, entries): (usize, usize) = connection.query_row(
        "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM guest_local_storage WHERE origin = ?1",
        [origin],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if bytes > MAX_ORIGIN_BYTES || entries > MAX_ORIGIN_ENTRIES {
        return Err(GuestLocalStorageError::QuotaExceeded);
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), GuestLocalStorageError> {
    if key.len() > MAX_KEY_BYTES {
        return Err(GuestLocalStorageError::InvalidKey);
    }
    Ok(())
}

fn database_failure(error: rusqlite::Error) -> GuestLocalStorageError {
    GuestLocalStorageError::DatabaseFailure(error.to_string())
}
