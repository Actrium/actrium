//! SQLite storage backend implementation
//!
//! Provides native async SQLite storage support via sqlx

use crate::crypto::KeyEncryptor;
use crate::error::{SignerError, SignerResult};
use crate::storage::backend::KeyStorageBackend;
use crate::storage::config::SqliteConfig;
use crate::types::{KeyPair, KeyRecord};
use async_trait::async_trait;
use base64::prelude::*;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current on-disk schema version for `signer_keys.db`, stored in
/// `PRAGMA user_version`.
const SIGNER_SCHEMA_VERSION: i64 = 2;

/// SQLite storage backend.
#[derive(Clone)]
pub struct SqliteBackend {
    pool: SqlitePool,
    encryptor: KeyEncryptor,
}

impl std::fmt::Debug for SqliteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteBackend")
            .field("encryption_enabled", &self.encryptor.is_enabled())
            .finish()
    }
}

impl SqliteBackend {
    /// Generate an Ed25519 key pair, returning the Base64 verifying key and
    /// the stored (encrypted when enabled) signing key.
    fn new_key_material(&self) -> SignerResult<(String, String)> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key_b64 = BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes());
        let signing_key_b64 = BASE64_STANDARD.encode(signing_key.as_bytes());
        Ok((verifying_key_b64, self.encryptor.encrypt(&signing_key_b64)?))
    }

    async fn named_key(&self, name: &str) -> SignerResult<Option<KeyPair>> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT k.key_id, k.public_key FROM key_names n JOIN keys k ON k.key_id = n.key_id \
             WHERE n.name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to query named key {name}: {e}")))?;
        Ok(row.map(|(key_id, verifying_key)| KeyPair {
            key_id: key_id as u32,
            verifying_key,
        }))
    }

    /// Create a new SQLite backend instance
    ///
    /// # Arguments
    /// * `_config` - SQLite config (reserved, currently unused)
    /// * `encryptor` - key encryptor
    /// * `db_path` - directory path for the database file (from LoranConfig.sqlite_path)
    pub async fn new(
        _config: &SqliteConfig,
        encryptor: KeyEncryptor,
        db_path: &Path,
    ) -> SignerResult<Self> {
        let file = db_path.join("signer_keys.db");

        // Create connect options and enable WAL mode
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", file.display()))
            .map_err(|e| SignerError::Internal(format!("Failed to parse SQLite URL: {e}")))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5));

        // Create the connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to connect to SQLite: {e}")))?;

        let backend = Self { pool, encryptor };

        // Initialize database tables
        backend.init().await?;

        crate::recording::info!(
            "SQLite storage initialized with sqlx: path={}, encryption={}, WAL mode enabled",
            file.display(),
            backend.encryptor.is_enabled()
        );

        Ok(backend)
    }
}

#[async_trait]
impl KeyStorageBackend for SqliteBackend {
    async fn init(&self) -> SignerResult<()> {
        // Schema version gate. Databases created before this stamp existed read
        // back as version 0, so accept both 0 (unversioned legacy or freshly
        // created) and the current version, and reject only a newer version this
        // binary does not understand. The stamp is written after the idempotent
        // DDL below, so fresh and legacy databases converge to the current version.
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to read schema version: {e}")))?;
        if version > SIGNER_SCHEMA_VERSION {
            return Err(SignerError::Internal(format!(
                "unsupported signer key database schema version {version}; expected \
                 at most {SIGNER_SCHEMA_VERSION}"
            )));
        }

        // Create the keys table (secret_key stores the encrypted Ed25519 signing
        // key, public_key stores the verifying key). `expires_at` is always 0:
        // every key is a named key that never expires.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS keys (
                key_id INTEGER PRIMARY KEY AUTOINCREMENT,
                public_key TEXT NOT NULL,
                secret_key TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to create keys table: {e}")))?;

        // Named keys (schema v2): stable names such as `realm-signing:<domain>`.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_names (
                name TEXT PRIMARY KEY,
                key_id INTEGER NOT NULL UNIQUE REFERENCES keys(key_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to create key_names table: {e}")))?;

        // Keys without a name came from the removed GenerateSigningKey RPC;
        // nothing refers to them any more.
        let unnamed =
            sqlx::query("DELETE FROM keys WHERE key_id NOT IN (SELECT key_id FROM key_names)")
                .execute(&self.pool)
                .await
                .map_err(|e| SignerError::Internal(format!("Failed to delete unnamed keys: {e}")))?
                .rows_affected();
        if unnamed > 0 {
            crate::recording::info!("Deleted {} unnamed signing keys", unnamed);
        }

        // Stamp the schema version once the DDL is in place. Skip the write when
        // the database is already at the current version so a healthy startup
        // performs no header write.
        if version != SIGNER_SCHEMA_VERSION {
            sqlx::query(&format!("PRAGMA user_version = {SIGNER_SCHEMA_VERSION}"))
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    SignerError::Internal(format!("Failed to stamp schema version: {e}"))
                })?;
        }

        crate::recording::debug!("SQLite tables and indexes initialized");
        Ok(())
    }

    async fn ensure_named_key(&self, name: &str) -> SignerResult<KeyPair> {
        if let Some(key_pair) = self.named_key(name).await? {
            return Ok(key_pair);
        }
        let (verifying_key_b64, encrypted_signing_key) = self.new_key_material()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let internal =
            |e: sqlx::Error| SignerError::Internal(format!("Failed to store named key: {e}"));
        let mut tx = self.pool.begin().await.map_err(internal)?;
        let key_id = sqlx::query(
            "INSERT INTO keys (public_key, secret_key, created_at, expires_at) VALUES (?1, ?2, ?3, 0)",
        )
        .bind(&verifying_key_b64)
        .bind(&encrypted_signing_key)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .last_insert_rowid();
        let inserted = sqlx::query(
            "INSERT INTO key_names (name, key_id) VALUES (?1, ?2) ON CONFLICT(name) DO NOTHING",
        )
        .bind(name)
        .bind(key_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();
        if inserted == 0 {
            // Another caller named a key first; drop ours and return theirs.
            tx.rollback().await.map_err(internal)?;
            return self
                .named_key(name)
                .await?
                .ok_or_else(|| SignerError::Internal(format!("Named key {name} vanished")));
        }
        tx.commit().await.map_err(internal)?;

        crate::recording::info!("Generated named key {} with key_id: {}", name, key_id);
        Ok(KeyPair {
            key_id: key_id as u32,
            verifying_key: verifying_key_b64,
        })
    }

    async fn retire_named_key(&self, name: &str) -> SignerResult<u32> {
        let key_id = self.ensure_named_key(name).await?.key_id;
        // An empty secret is the retired marker: the private key is gone.
        sqlx::query("UPDATE keys SET secret_key = '' WHERE key_id = ?")
            .bind(key_id as i64)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                SignerError::Internal(format!("Failed to retire named key {name}: {e}"))
            })?;
        crate::recording::info!("Retired named key {} (key_id: {})", name, key_id);
        Ok(key_id)
    }

    async fn get_public_key(&self, key_id: u32) -> SignerResult<Option<String>> {
        let result = sqlx::query_as::<_, (String,)>("SELECT public_key FROM keys WHERE key_id = ?")
            .bind(key_id as i64)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                SignerError::Internal(format!(
                    "Failed to query public key for key_id {key_id}: {e}"
                ))
            })?;

        if let Some((public_key,)) = result {
            crate::recording::debug!("Found verifying key for key_id: {}", key_id);
            Ok(Some(public_key))
        } else {
            crate::recording::debug!("No verifying key found for key_id: {}", key_id);
            Ok(None)
        }
    }

    async fn sign(&self, key_id: u32, message: &[u8]) -> SignerResult<Vec<u8>> {
        // Read the encrypted signing key from the database
        let result = sqlx::query_as::<_, (String,)>("SELECT secret_key FROM keys WHERE key_id = ?")
            .bind(key_id as i64)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                SignerError::Internal(format!(
                    "Failed to query signing key for key_id {key_id}: {e}"
                ))
            })?;

        let encrypted_signing_key = match result {
            Some((key,)) => key,
            None => {
                crate::recording::warn!("Signing key not found for key_id: {}", key_id);
                return Err(SignerError::NotFound(format!("Key not found: {key_id}")));
            }
        };
        if encrypted_signing_key.is_empty() {
            return Err(SignerError::KeyRetired(key_id));
        }

        // Decrypt the signing key
        let signing_key_b64 = self.encryptor.decrypt(&encrypted_signing_key)?;

        // Base64 decode
        let signing_key_bytes = BASE64_STANDARD
            .decode(&signing_key_b64)
            .map_err(|e| SignerError::Crypto(format!("Failed to decode signing key: {e}")))?;

        let signing_key_array: [u8; 32] = signing_key_bytes
            .try_into()
            .map_err(|_| SignerError::Crypto("Signing key must be exactly 32 bytes".to_string()))?;

        // Rebuild the SigningKey and sign
        let signing_key = SigningKey::from_bytes(&signing_key_array);
        let signature = signing_key.sign(message);

        crate::recording::debug!("Signed message with key_id: {}", key_id);

        Ok(signature.to_bytes().to_vec())
    }

    async fn get_key_record(&self, key_id: u32) -> SignerResult<Option<KeyRecord>> {
        let result = sqlx::query_as::<_, (i64, String, i64)>(
            "SELECT key_id, public_key, created_at FROM keys WHERE key_id = ?",
        )
        .bind(key_id as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            SignerError::Internal(format!(
                "Failed to query key record for key_id {key_id}: {e}"
            ))
        })?;

        if let Some((key_id_db, public_key, created_at)) = result {
            crate::recording::debug!("Found key record for key_id: {}", key_id);
            Ok(Some(KeyRecord {
                key_id: key_id_db as u32,
                public_key,
                created_at: created_at as u64,
            }))
        } else {
            crate::recording::debug!("No key record found for key_id: {}", key_id);
            Ok(None)
        }
    }

    async fn get_key_count(&self) -> SignerResult<u32> {
        let result = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM keys")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to get key count: {e}")))?;

        Ok(result.0 as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn create_test_backend(path: &Path) -> SqliteBackend {
        let config = SqliteConfig::default();
        SqliteBackend::new(&config, crate::crypto::KeyEncryptor::no_encryption(), path)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_sqlite_init() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;
        let count = backend.get_key_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_generate_and_query() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let key_pair = backend.ensure_named_key("test").await.unwrap();
        assert!(key_pair.key_id > 0);
        assert!(!key_pair.verifying_key.is_empty());

        // Verify the verifying key is 32 bytes (after base64 decode)
        let vk_bytes = BASE64_STANDARD.decode(&key_pair.verifying_key).unwrap();
        assert_eq!(vk_bytes.len(), 32, "Ed25519 verifying key must be 32 bytes");

        // Query the public key
        let public_key = backend.get_public_key(key_pair.key_id).await.unwrap();
        assert_eq!(public_key, Some(key_pair.verifying_key.clone()));

        // Query the full record
        let record = backend.get_key_record(key_pair.key_id).await.unwrap();
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key_id, key_pair.key_id);
        assert_eq!(record.public_key, key_pair.verifying_key);
    }

    #[tokio::test]
    async fn test_sign() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let key_pair = backend.ensure_named_key("test").await.unwrap();
        let message = b"hello world";

        let signature = backend.sign(key_pair.key_id, message).await.unwrap();
        assert_eq!(signature.len(), 64, "Ed25519 signature must be 64 bytes");

        // Verify the signature
        let vk_bytes = BASE64_STANDARD.decode(&key_pair.verifying_key).unwrap();
        let vk_array: [u8; 32] = vk_bytes.try_into().unwrap();
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&vk_array).unwrap();
        let sig_array: [u8; 64] = signature.try_into().unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&sig_array);
        use ed25519_dalek::Verifier;
        assert!(
            verifying_key.verify(message, &sig).is_ok(),
            "Signature must be valid"
        );
    }

    #[tokio::test]
    async fn a_retired_named_key_never_signs_again() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let key = backend
            .ensure_named_key("issuer:a@a.example")
            .await
            .unwrap();
        assert_eq!(
            backend
                .retire_named_key("issuer:a@a.example")
                .await
                .unwrap(),
            key.key_id
        );
        assert!(matches!(
            backend.sign(key.key_id, b"m").await,
            Err(SignerError::KeyRetired(id)) if id == key.key_id
        ));
        // Idempotent, and the name keeps its retired key.
        backend
            .retire_named_key("issuer:a@a.example")
            .await
            .unwrap();
        let again = backend
            .ensure_named_key("issuer:a@a.example")
            .await
            .unwrap();
        assert_eq!(again.key_id, key.key_id);
        assert!(backend.sign(again.key_id, b"m").await.is_err());

        // A name retired before it had a key never gets a live one.
        let unused = backend
            .retire_named_key("issuer:b@b.example")
            .await
            .unwrap();
        let later = backend
            .ensure_named_key("issuer:b@b.example")
            .await
            .unwrap();
        assert_eq!(later.key_id, unused);
        assert!(backend.sign(later.key_id, b"m").await.is_err());
    }

    #[tokio::test]
    async fn named_key_is_stable() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let first = backend
            .ensure_named_key("realm-signing:a.example")
            .await
            .unwrap();
        let again = backend
            .ensure_named_key("realm-signing:a.example")
            .await
            .unwrap();
        let other = backend
            .ensure_named_key("realm-signing:b.example")
            .await
            .unwrap();
        assert_eq!(first.key_id, again.key_id);
        assert_eq!(first.verifying_key, again.verifying_key);
        assert_ne!(first.key_id, other.key_id);

        assert_eq!(backend.sign(first.key_id, b"m").await.unwrap().len(), 64);

        // Survives a reopen of the same database.
        drop(backend);
        let backend = create_test_backend(temp_dir.path()).await;
        let reopened = backend
            .ensure_named_key("realm-signing:a.example")
            .await
            .unwrap();
        assert_eq!(reopened.key_id, first.key_id);
        assert_eq!(backend.get_key_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_sign_nonexistent_key() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let result = backend.sign(999, b"message").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SignerError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_query_nonexistent_key() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        let public_key = backend.get_public_key(999).await.unwrap();
        assert_eq!(public_key, None);

        let record = backend.get_key_record(999).await.unwrap();
        assert_eq!(record, None);
    }

    #[tokio::test]
    async fn test_key_count() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;

        assert_eq!(backend.get_key_count().await.unwrap(), 0);

        backend.ensure_named_key("a").await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 1);

        backend.ensure_named_key("b").await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn unnamed_keys_are_deleted_on_open() {
        let temp_dir = tempdir().unwrap();
        let backend = create_test_backend(temp_dir.path()).await;
        let named = backend.ensure_named_key("a").await.unwrap();
        sqlx::query(
            "INSERT INTO keys (public_key, secret_key, created_at, expires_at) \
             VALUES ('pk', 'sk', 1, 2)",
        )
        .execute(&backend.pool)
        .await
        .unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 2);
        drop(backend);

        let backend = create_test_backend(temp_dir.path()).await;
        assert_eq!(backend.get_key_count().await.unwrap(), 1);
        assert!(
            backend
                .get_key_record(named.key_id)
                .await
                .unwrap()
                .is_some()
        );
    }
}
