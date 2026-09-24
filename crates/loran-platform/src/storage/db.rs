//! Database connection and operation management
//!
//! Provides a sqlx-based database connection pool and basic operations

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

const ACTRIX_SCHEMA_VERSION: i64 = 3;

/// Database manager
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database instance
    ///
    /// # Arguments
    /// * `path` - Directory path for database file storage; must already exist
    ///   The main database file will be stored as `{path}/actrix.db`
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db_file = path.as_ref().join("actrix.db");

        // Create connection options and enable WAL mode
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_file.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await?;

        let db = Self { pool };

        // Schema changes are explicit and fail closed. Databases from another
        // schema version must be rebuilt.
        db.verify_schema_version().await?;
        db.initialize_schema().await?;

        Ok(db)
    }

    async fn verify_schema_version(&self) -> Result<()> {
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await?;
        if version == ACTRIX_SCHEMA_VERSION {
            return Ok(());
        }

        if version == 0 {
            let application_table_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .fetch_one(&self.pool)
            .await?;
            if application_table_count == 0 {
                return Ok(());
            }
        }

        anyhow::bail!(
            "unsupported Actrix database schema version {version}; expected \
             {ACTRIX_SCHEMA_VERSION}. Stop Actrix and rebuild the database"
        );
    }

    /// Initialize the database schema.
    ///
    /// All table/index DDL and the `PRAGMA user_version` bump run inside a single
    /// transaction so a mid-initialization crash can never leave a half-built
    /// database with tables present but `user_version = 0` (which the startup gate
    /// rejects and the migration tool cannot repair). SQLite supports
    /// transactional DDL and `PRAGMA user_version` participates in the enclosing
    /// transaction, so the whole schema either lands atomically or rolls back.
    /// Every statement stays idempotent (`IF NOT EXISTS`) so re-running is safe.
    async fn initialize_schema(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Create the Realm table.
        // secret_current carries a CHECK so the non-empty invariant is enforced
        // at the storage layer, not only by the three application entry points.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS realm (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'Active',
                enabled INTEGER NOT NULL DEFAULT 1,
                expires_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER,
                secret_current TEXT NOT NULL DEFAULT ''
                    CHECK(length(trim(secret_current)) > 0),
                secret_previous_hash TEXT,
                secret_previous_valid_until INTEGER
            )",
        )
        .execute(&mut *tx)
        .await?;

        // One Actrix instance is exactly one Realm in the v1 production profile.
        // The expression index enforces that invariant under concurrent writers.
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_realm_singleton
             ON realm((1))",
        )
        .execute(&mut *tx)
        .await?;

        // Set autoincrement start to 2^25 = 33554432
        // Only insert if not already present (fresh database)
        sqlx::query("INSERT OR IGNORE INTO sqlite_sequence(name, seq) VALUES('realm', 33554431)")
            .execute(&mut *tx)
            .await?;

        // Create the access control list table.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS actoracl (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                realm_id INTEGER NOT NULL,
                source_realm_id INTEGER,
                from_type TEXT NOT NULL,
                to_type TEXT NOT NULL,
                access INTEGER NOT NULL CHECK(access IN (0, 1)),
                FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
            )",
        )
        .execute(&mut *tx)
        .await?;

        // Create indexes.
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_realm_name
             ON realm(name)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_actoracl_realm_id
             ON actoracl(realm_id)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_actoracl_lookup
             ON actoracl(realm_id, source_realm_id, from_type, to_type)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_actoracl_policy
             ON actoracl(
                realm_id,
                COALESCE(source_realm_id, -1),
                from_type,
                to_type
             )",
        )
        .execute(&mut *tx)
        .await?;

        // Pending registration data: Issuer writes, signaling reads on WS connect
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS pending_registration (
                realm_id INTEGER NOT NULL,
                serial_number INTEGER NOT NULL,
                service_spec_blob BLOB,
                ws_address TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER,
                PRIMARY KEY(realm_id, serial_number),
                FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
            )",
        )
        .execute(&mut *tx)
        .await?;

        // Signaling reads pending rows with `expires_at IS NULL OR expires_at >
        // unixepoch()`; index the column so the expiry filter stays cheap.
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pending_registration_expiry \
             ON pending_registration(expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // Permanent Actor instance identity ledger. Snowflake worker leases
        // provide distributed uniqueness; this table additionally makes local
        // uniqueness fail closed across process restarts and clock rollback.
        // `target` persists the package target bound at registration so that
        // /renew and /reissue can recheck the exact (type_str, target) package
        // row instead of masking a revoked target behind a sibling that is still
        // active.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issued_actor_identity (
                realm_id INTEGER NOT NULL,
                serial_number INTEGER NOT NULL,
                actor_type TEXT NOT NULL,
                target TEXT,
                auth_mode INTEGER,
                issued_at INTEGER NOT NULL,
                PRIMARY KEY(realm_id, serial_number),
                FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
            )",
        )
        .execute(&mut *tx)
        .await?;

        // Legacy databases (created before the target column existed) lack it;
        // add it idempotently so existing rows get NULL (per-target recheck
        // degrades to type-only for those rows, logged in recheck_existing_actor).
        add_column_if_missing(&mut tx, "issued_actor_identity", "target", "TEXT").await?;
        // NULL preserves the distinction between legacy rows and verified Linked registrations.
        add_column_if_missing(&mut tx, "issued_actor_identity", "auth_mode", "INTEGER").await?;

        // Publisher Registry tables
        // name is the canonical HTTPS publisher domain.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS publisher (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                name         TEXT    NOT NULL UNIQUE,
                public_key   TEXT    NOT NULL DEFAULT '',
                key_id       TEXT    NOT NULL DEFAULT '',
                descriptor_json TEXT NOT NULL DEFAULT '{}',
                contact      TEXT,
                status       TEXT    NOT NULL DEFAULT 'pending',
                created_at   INTEGER NOT NULL,
                updated_at   INTEGER,
                verified_at  INTEGER,
                suspended_at INTEGER,
                revoked_at   INTEGER,
                key_expires_at  INTEGER
            )",
        )
        .execute(&mut *tx)
        .await?;

        // HTTPS publisher descriptor challenge for identity verification.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS publisher_challenge (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                publisher_id      INTEGER NOT NULL REFERENCES publisher(id),
                token       TEXT    NOT NULL,
                verify_url  TEXT    NOT NULL DEFAULT '',
                expires_at  INTEGER NOT NULL,
                verified_at INTEGER,
                created_at  INTEGER NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS publisher_package (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                publisher_id       INTEGER NOT NULL REFERENCES publisher(id),
                publisher TEXT    NOT NULL,
                name         TEXT    NOT NULL,
                version      TEXT    NOT NULL,
                type_str     TEXT    NOT NULL,
                target       TEXT    NOT NULL,
                manifest     TEXT    NOT NULL,
                signature    TEXT    NOT NULL,
                signing_key_id TEXT,
                proto_files  TEXT,
                status       TEXT    NOT NULL DEFAULT 'active',
                published_at INTEGER NOT NULL,
                revoked_at   INTEGER,
                UNIQUE(publisher, name, version, target)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_publisher_package_type ON publisher_package(type_str)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_publisher_package_publisher ON publisher_package(publisher_id, status)",
        )
        .execute(&mut *tx)
        .await?;

        // Publisher Registry key history: stores retired public keys for JWKS-style rotation
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS publisher_key_history (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                publisher_id       INTEGER NOT NULL REFERENCES publisher(id),
                key_id       TEXT    NOT NULL,
                public_key   TEXT    NOT NULL,
                status       TEXT    NOT NULL DEFAULT 'retired',
                created_at   INTEGER NOT NULL,
                retired_at   INTEGER NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;

        // UNIQUE so a given (publisher_id, key_id) can be archived at most once. This
        // backs the application-layer blacklist that forbids reinstating a
        // previously used publisher key: even a code path that skipped the check
        // cannot create a second history row for the same key. Normal forward
        // rotation always archives a distinct outgoing key, so this never fires
        // on the healthy path.
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_publisher_key_history_lookup
             ON publisher_key_history(publisher_id, key_id)",
        )
        .execute(&mut *tx)
        .await?;

        // Publish nonce table for challenge-response authentication on
        // /registry/packages/publish.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS publisher_publish_nonce (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                publisher_id     INTEGER NOT NULL REFERENCES publisher(id),
                nonce      BLOB    NOT NULL UNIQUE,
                status     TEXT    NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_publisher_publish_nonce_expires
             ON publisher_publish_nonce(expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // Issuer unpublished package publisher-proof nonce table. Issuer inserts
        // after publisher_auth_signature verification; the unique index is the
        // replay guard.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issuer_publisher_auth_nonce (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                publisher TEXT    NOT NULL,
                key_id       TEXT    NOT NULL,
                nonce        BLOB    NOT NULL,
                created_at   INTEGER NOT NULL,
                expires_at   INTEGER NOT NULL,
                UNIQUE(publisher, key_id, nonce)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_issuer_publisher_auth_nonce_expires
             ON issuer_publisher_auth_nonce(expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // Issuer renewal token table — only stores SHA-256(token), never the raw token.
        //
        // `delivered_at` marks whether the response carrying this successor token
        // was committed to the reissue response cache (same transaction). NULL
        // means "consumed but not delivered" — a lost response — and the reissue
        // replay guard relaxes to the binding grace window for that case so the
        // client can retry. Non-NULL means the proof is fully spent.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issuer_renewal_token (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id TEXT NOT NULL,
                token_hash BLOB NOT NULL UNIQUE,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                delivered_at INTEGER
            )",
        )
        .execute(&mut *tx)
        .await?;

        add_column_if_missing(&mut tx, "issuer_renewal_token", "delivered_at", "INTEGER").await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_issuer_renewal_token_actor
             ON issuer_renewal_token(actor_id)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_issuer_renewal_token_expires
             ON issuer_renewal_token(expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // /reissue idempotent response cache. Keyed by an authenticated request hash so a lost-
        // response retry of the same attempt decrypts and replays the exact
        // credential Issuer signed on the first attempt instead of re-signing. The
        // nonce consume, this cache write, and the `delivered_at` mark on the
        // successor row all commit in one BEGIN IMMEDIATE transaction, so a
        // response is either durable for replay or none of the side effects land.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issuer_reissue_response_cache (
                nonce_hash BLOB PRIMARY KEY,
                response_ciphertext BLOB NOT NULL,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_issuer_reissue_response_cache_expires
             ON issuer_reissue_response_cache(expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // Durable idempotency journal for Issuer identity-state writes.
        // Response bodies contain bearer credentials and are therefore
        // encrypted by Issuer before being stored in response_ciphertext.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issuer_control_attempt (
                operation           TEXT    NOT NULL,
                attempt_id          BLOB    NOT NULL,
                actor_id            TEXT    NOT NULL,
                request_hash        BLOB    NOT NULL,
                state               INTEGER NOT NULL,
                lease_id            BLOB,
                lease_expires_at    INTEGER,
                response_status     INTEGER,
                response_ciphertext BLOB,
                response_expires_at INTEGER,
                tombstone_expires_at INTEGER NOT NULL,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                PRIMARY KEY(operation, attempt_id)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_issuer_control_attempt_expiry
             ON issuer_control_attempt(state, tombstone_expires_at)",
        )
        .execute(&mut *tx)
        .await?;

        // Explicit actor revocation list. A revoked (realm_id, serial_number)
        // can never be renewed or reissued again, independent of any renewal
        // token it may still hold: /renew and /reissue consult this table before
        // signing. The composite primary key makes revocation idempotent.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS revoked_actor (
                realm_id INTEGER NOT NULL,
                serial_number INTEGER NOT NULL,
                revoked_at INTEGER NOT NULL,
                reason TEXT,
                PRIMARY KEY(realm_id, serial_number),
                FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!("PRAGMA user_version = {ACTRIX_SCHEMA_VERSION}"))
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Get the database connection pool
    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Execute an SQL statement and return the number of affected rows
    pub async fn execute(&self, sql: &str) -> Result<u64> {
        let result = sqlx::query(sql).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
}

/// Add a column to a table if it does not already exist. Used to evolve legacy
/// databases that pre-date a column introduced in a schema bump, without
/// forcing a full offline migration for an additive, nullable column.
async fn add_column_if_missing(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pragma_table_info(?) WHERE name = ?")
            .bind(table)
            .bind(column)
            .fetch_one(&mut **tx)
            .await?;
    if !exists {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

use tokio::sync::OnceCell;

/// Global database instance
static GLOBAL_DATABASE: OnceCell<Database> = OnceCell::const_new();

/// Set the global database path
pub async fn set_db_path(path: &Path) -> Result<()> {
    let database = Database::new(path).await?;
    GLOBAL_DATABASE
        .set(database)
        .map_err(|_| anyhow::anyhow!("Database already initialized"))?;
    Ok(())
}

/// Get the global database instance
pub fn get_database() -> &'static Database {
    GLOBAL_DATABASE
        .get()
        .expect("Database not initialized. Call set_db_path first.")
}

/// Check whether the database has been initialized
pub fn is_database_initialized() -> bool {
    GLOBAL_DATABASE.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_database_is_versioned_and_enforces_single_realm() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(db.get_pool())
            .await
            .unwrap();
        assert_eq!(version, ACTRIX_SCHEMA_VERSION);

        sqlx::query(
            "INSERT INTO realm
             (name, status, enabled, created_at, secret_current)
             VALUES ('primary.example', 'Active', 1, 1, 'hash')",
        )
        .execute(db.get_pool())
        .await
        .unwrap();
        let second = sqlx::query(
            "INSERT INTO realm
             (name, status, enabled, created_at, secret_current)
             VALUES ('second.example', 'Active', 1, 2, 'hash')",
        )
        .execute(db.get_pool())
        .await;
        assert!(second.is_err(), "v1 must enforce one Realm per database");
    }

    #[tokio::test]
    async fn existing_v1_actor_ledger_gains_nullable_auth_mode() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        sqlx::query("ALTER TABLE issued_actor_identity DROP COLUMN auth_mode")
            .execute(db.get_pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO realm (id, name, status, enabled, created_at, secret_current)
             VALUES (1, 'primary', 'Active', 1, 1, 'hash')",
        )
        .execute(db.get_pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO issued_actor_identity (realm_id, serial_number, actor_type, target, issued_at)
             VALUES (1, 42, 'apps.example.com/worker@1.0.0', 'wasm32-wasip1', 1)",
        ).execute(db.get_pool()).await.unwrap();
        db.get_pool().close().await;

        let upgraded = Database::new(dir.path()).await.unwrap();
        let row: (String, Option<i32>) = sqlx::query_as(
            "SELECT actor_type, auth_mode FROM issued_actor_identity WHERE realm_id = 1 AND serial_number = 42",
        ).fetch_one(upgraded.get_pool()).await.unwrap();
        assert_eq!(row, ("apps.example.com/worker@1.0.0".to_string(), None));
        // Repeated initialization preserves the upgraded schema and records.
        upgraded.initialize_schema().await.unwrap();
    }

    #[tokio::test]
    async fn unversioned_existing_database_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("actrix.db");
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_file.display()))
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE realm (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let error = Database::new(dir.path())
            .await
            .err()
            .expect("legacy database must be rejected");
        let message = error.to_string();
        assert!(message.contains("schema version 0"));
        assert!(message.contains("rebuild the database"));
    }

    #[tokio::test]
    async fn previous_schema_version_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        sqlx::query(&format!(
            "PRAGMA user_version = {}",
            ACTRIX_SCHEMA_VERSION - 1
        ))
        .execute(db.get_pool())
        .await
        .unwrap();
        db.get_pool().close().await;

        let error = Database::new(dir.path())
            .await
            .err()
            .expect("previous schema version must be rejected");
        let message = error.to_string();
        assert!(message.contains(&format!("schema version {}", ACTRIX_SCHEMA_VERSION - 1)));
        assert!(message.contains("rebuild the database"));
    }
}
