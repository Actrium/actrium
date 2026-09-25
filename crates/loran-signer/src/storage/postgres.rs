//! PostgreSQL storage backend implementation
//!
//! Uses sqlx to provide PostgreSQL storage support

use crate::crypto::KeyEncryptor;
use crate::error::{SignerError, SignerResult};
use crate::storage::backend::KeyStorageBackend;
use crate::storage::config::PostgresConfig;
use crate::types::{KeyPair, KeyRecord};
use async_trait::async_trait;
use base64::prelude::*;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Per-row ciphertext format marker prefix.
///
/// `secret_key` column values are either:
/// - `v0` (legacy): raw `BASE64_STANDARD.encode(signing_key_32B)` — plaintext, ~44 chars
/// - `v1`: `aes256gcm:v1:` + `BASE64_STANDARD.encode(nonce[12] || ciphertext || tag[16])`
///
/// The prefix lets [`unwrap_secret`] distinguish formats and fail closed instead of
/// silently treating ciphertext as plaintext (or vice versa).
const V1_PREFIX: &str = "aes256gcm:v1:";

/// Wrap a plaintext Base64 signing key for storage using `encryptor`.
///
/// With a KEK configured → `aes256gcm:v1:<ciphertext>`; without → plaintext (legacy `v0`).
fn wrap_secret(encryptor: &KeyEncryptor, signing_key_b64: &str) -> SignerResult<String> {
    if encryptor.is_enabled() {
        Ok(format!(
            "{V1_PREFIX}{}",
            encryptor.encrypt(signing_key_b64)?
        ))
    } else {
        Ok(signing_key_b64.to_string())
    }
}

/// Unwrap a stored `secret_key` value back to the plaintext Base64 signing key.
///
/// Fail-closed matrix:
/// | stored         | KEK on | KEK off                          |
/// |----------------|--------|----------------------------------|
/// | `v1:` prefix   | decrypt| error: encrypted without KEK     |
/// | `v0` plaintext | error  | passthrough (backward compatible)|
///
/// Never falls back from ciphertext to plaintext (or the reverse): that is the
/// fail-open anti-pattern this module exists to prevent.
fn unwrap_secret(encryptor: &KeyEncryptor, stored: &str) -> SignerResult<String> {
    if let Some(rest) = stored.strip_prefix(V1_PREFIX) {
        if !encryptor.is_enabled() {
            return Err(SignerError::Crypto(
                "Encrypted (v1) signing key present but no KEK configured".into(),
            ));
        }
        return encryptor.decrypt(rest);
    }
    if encryptor.is_enabled() {
        return Err(SignerError::Crypto(
            "Refusing to load unencrypted (v0) signing key while KEK is configured; \
             the v0→v1 migration should have converted it. Restart with the KEK to retry migration."
                .into(),
        ));
    }
    Ok(stored.to_string())
}

/// PostgreSQL storage backend
#[derive(Clone)]
pub struct PostgresBackend {
    pool: PgPool,
    key_ttl: u64,
    encryptor: KeyEncryptor,
}

impl std::fmt::Debug for PostgresBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresBackend")
            .field("key_ttl", &self.key_ttl)
            .field("encryption_enabled", &self.encryptor.is_enabled())
            .finish()
    }
}

impl PostgresBackend {
    /// Create a new PostgreSQL backend instance
    ///
    /// # Arguments
    /// * `config` - PostgreSQL configuration
    /// * `key_ttl` - Key TTL (seconds); 0 means never expires
    /// * `encryptor` - Key encryptor used to protect stored Ed25519 signing keys
    pub async fn new(
        config: &PostgresConfig,
        key_ttl: u64,
        encryptor: KeyEncryptor,
    ) -> SignerResult<Self> {
        // Build connection URL
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );

        // Create connection pool
        let pool = PgPoolOptions::new()
            .max_connections(config.pool_size)
            .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
            .connect(&url)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to connect to PostgreSQL: {e}")))?;

        let backend = Self {
            pool,
            key_ttl,
            encryptor,
        };

        // Initialize database tables. When a KEK is configured this also runs the
        // v0→v1 migration; any failure aborts startup so the service never serves
        // with plaintext keys it was asked to protect.
        backend.init().await?;

        crate::recording::info!(
            "PostgreSQL storage initialized: host={}:{}, db={}, key_ttl={}s, encryption={}",
            config.host,
            config.port,
            config.database,
            key_ttl,
            backend.encryptor.is_enabled()
        );

        Ok(backend)
    }

    async fn named_key(&self, name: &str) -> SignerResult<Option<KeyPair>> {
        let row = sqlx::query_as::<_, (i32, String)>(
            "SELECT k.key_id, k.public_key FROM key_names n JOIN keys k ON k.key_id = n.key_id \
             WHERE n.name = $1",
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

    /// Prepare a plaintext Base64 signing key for the `secret_key` column.
    fn store_secret(&self, signing_key_b64: &str) -> SignerResult<String> {
        wrap_secret(&self.encryptor, signing_key_b64)
    }

    /// Recover the plaintext Base64 signing key from a stored `secret_key` value.
    fn load_secret(&self, stored: &str) -> SignerResult<String> {
        unwrap_secret(&self.encryptor, stored)
    }

    /// Transactionally migrate legacy `v0` plaintext rows to `v1` ciphertext.
    ///
    /// Runs at init time only when a KEK is configured. Idempotent: rows already
    /// carrying the `v1:` prefix are skipped, so concurrent/repeated starts are safe.
    /// `SELECT ... FOR UPDATE` serializes multi-replica migrations. Any malformed row
    /// aborts the transaction and fails startup (no half-migrated state is left behind).
    async fn migrate_v0_to_v1(&self) -> SignerResult<u64> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SignerError::Internal(format!("migrate_v0_to_v1 begin: {e}")))?;

        let rows = sqlx::query_as::<_, (i32, String)>(
            "SELECT key_id, secret_key FROM keys ORDER BY key_id FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| SignerError::Internal(format!("migrate_v0_to_v1 select: {e}")))?;

        let mut migrated = 0u64;
        for (key_id, stored) in &rows {
            if stored.starts_with(V1_PREFIX) {
                continue; // already v1 — idempotent
            }
            // v0 is plaintext base64(32B). Validate before re-encrypting so we never
            // stamp a malformed row as a valid v1 key.
            let raw = BASE64_STANDARD.decode(stored).map_err(|e| {
                SignerError::Crypto(format!(
                    "migrate_v0_to_v1: key_id={key_id} invalid v0 base64: {e}"
                ))
            })?;
            if raw.len() != 32 {
                return Err(SignerError::Crypto(format!(
                    "migrate_v0_to_v1: key_id={key_id} expected 32-byte signing key, got {} bytes",
                    raw.len()
                )));
            }
            let v1 = format!("{V1_PREFIX}{}", self.encryptor.encrypt(stored)?);
            sqlx::query("UPDATE keys SET secret_key = $1 WHERE key_id = $2")
                .bind(&v1)
                .bind(*key_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    SignerError::Internal(format!("migrate_v0_to_v1 update key_id={key_id}: {e}"))
                })?;
            migrated += 1;
        }

        tx.commit()
            .await
            .map_err(|e| SignerError::Internal(format!("migrate_v0_to_v1 commit: {e}")))?;

        if migrated > 0 {
            crate::recording::info!(
                "Migrated {migrated} plaintext signing key(s) to v1 (AES-256-GCM) in PostgreSQL"
            );
        } else {
            crate::recording::debug!("No v0 plaintext signing keys to migrate in PostgreSQL");
        }
        Ok(migrated)
    }
}

#[async_trait]
impl KeyStorageBackend for PostgresBackend {
    async fn init(&self) -> SignerResult<()> {
        // Create keys table (secret_key column stores the encrypted Ed25519 signing key, public_key stores the verifying key)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS keys (
                key_id SERIAL PRIMARY KEY,
                public_key TEXT NOT NULL,
                secret_key TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to create keys table: {e}")))?;

        // Create index to improve expired key query performance
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_keys_expires_at ON keys(expires_at) WHERE expires_at > 0",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to create index: {e}")))?;

        // Named keys: stable names such as `realm-signing:<domain>` for keys
        // that never expire.
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

        crate::recording::debug!("PostgreSQL tables and indexes initialized");

        // When a KEK is configured, fail closed: convert any legacy plaintext (v0)
        // rows to v1 ciphertext before serving traffic. Aborts startup on failure.
        if self.encryptor.is_enabled() {
            self.migrate_v0_to_v1().await?;
        } else {
            crate::recording::warn!(
                "PostgreSQL storing Ed25519 signing keys WITHOUT encryption (no KEK configured)"
            );
        }
        Ok(())
    }

    async fn generate_and_store_key(&self) -> SignerResult<KeyPair> {
        // Generate Ed25519 key pair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Encode as Base64
        let verifying_key_b64 = BASE64_STANDARD.encode(verifying_key.as_bytes());
        let signing_key_b64 = BASE64_STANDARD.encode(signing_key.as_bytes());

        // Encrypt (if a KEK is configured) and tag with the v1 format prefix so the
        // stored value is never the recoverable Base64 private key.
        let stored_secret = self.store_secret(&signing_key_b64)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Calculate expiration time
        let expires_at = if self.key_ttl == 0 {
            0 // Never expires
        } else {
            now + self.key_ttl as i64
        };

        // Insert key and get the auto-generated key_id (public_key stores the verifying
        // key, secret_key stores the AES-256-GCM ciphertext or legacy plaintext)
        let row = sqlx::query_as::<_, (i32,)>(
            r#"
            INSERT INTO keys (public_key, secret_key, created_at, expires_at)
            VALUES ($1, $2, $3, $4)
            RETURNING key_id
            "#,
        )
        .bind(&verifying_key_b64)
        .bind(&stored_secret)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| SignerError::Internal(format!("Failed to insert key: {e}")))?;

        let key_id = row.0 as u32;

        crate::recording::info!(
            "Generated and stored new Ed25519 key pair in PostgreSQL: key_id={}, expires_at={}",
            key_id,
            expires_at
        );

        Ok(KeyPair {
            key_id,
            verifying_key: verifying_key_b64,
        })
    }

    async fn ensure_named_key(&self, name: &str) -> SignerResult<KeyPair> {
        if let Some(key_pair) = self.named_key(name).await? {
            return Ok(key_pair);
        }
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key_b64 = BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes());
        let stored_secret = self.store_secret(&BASE64_STANDARD.encode(signing_key.as_bytes()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let internal =
            |e: sqlx::Error| SignerError::Internal(format!("Failed to store named key: {e}"));
        let mut tx = self.pool.begin().await.map_err(internal)?;
        let (key_id,) = sqlx::query_as::<_, (i32,)>(
            "INSERT INTO keys (public_key, secret_key, created_at, expires_at) \
             VALUES ($1, $2, $3, 0) RETURNING key_id",
        )
        .bind(&verifying_key_b64)
        .bind(&stored_secret)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
        let inserted = sqlx::query(
            "INSERT INTO key_names (name, key_id) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING",
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

    async fn get_public_key(&self, key_id: u32) -> SignerResult<Option<String>> {
        let result =
            sqlx::query_scalar::<_, String>("SELECT public_key FROM keys WHERE key_id = $1")
                .bind(key_id as i32)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    SignerError::Internal(format!(
                        "Failed to query public key for key_id {key_id}: {e}"
                    ))
                })?;

        if result.is_some() {
            crate::recording::debug!("Found verifying key for key_id: {} in PostgreSQL", key_id);
        } else {
            crate::recording::debug!(
                "No verifying key found for key_id: {} in PostgreSQL",
                key_id
            );
        }

        Ok(result)
    }

    async fn retire_named_key(&self, name: &str) -> SignerResult<u32> {
        let key_id = self.ensure_named_key(name).await?.key_id;
        // An empty secret is the retired marker: the private key is gone.
        sqlx::query("UPDATE keys SET secret_key = '' WHERE key_id = $1")
            .bind(key_id as i32)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                SignerError::Internal(format!("Failed to retire named key {name}: {e}"))
            })?;
        crate::recording::info!("Retired named key {} (key_id: {})", name, key_id);
        Ok(key_id)
    }

    async fn sign(&self, key_id: u32, message: &[u8]) -> SignerResult<Vec<u8>> {
        // Read stored signing key (AES-256-GCM v1 ciphertext, or legacy v0 plaintext)
        let result =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(key_id as i32)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    SignerError::Internal(format!(
                        "Failed to query signing key for key_id {key_id}: {e}"
                    ))
                })?;

        let stored = match result {
            Some(key) => key,
            None => {
                crate::recording::warn!(
                    "Signing key not found for key_id: {} in PostgreSQL",
                    key_id
                );
                return Err(SignerError::NotFound(format!("Key not found: {key_id}")));
            }
        };
        if stored.is_empty() {
            return Err(SignerError::KeyRetired(key_id));
        }

        // Decrypt / decode; fails closed on format/KEK mismatch (see load_secret)
        let signing_key_b64 = self.load_secret(&stored)?;

        // Base64 decode
        let signing_key_bytes = BASE64_STANDARD
            .decode(&signing_key_b64)
            .map_err(|e| SignerError::Crypto(format!("Failed to decode signing key: {e}")))?;

        let signing_key_array: [u8; 32] = signing_key_bytes
            .try_into()
            .map_err(|_| SignerError::Crypto("Signing key must be exactly 32 bytes".to_string()))?;

        // Reconstruct SigningKey and sign
        let signing_key = SigningKey::from_bytes(&signing_key_array);
        let signature = signing_key.sign(message);

        crate::recording::debug!("Signed message with key_id: {} in PostgreSQL", key_id);

        Ok(signature.to_bytes().to_vec())
    }

    async fn get_key_record(&self, key_id: u32) -> SignerResult<Option<KeyRecord>> {
        let result = sqlx::query_as::<_, (i32, String, i64, i64)>(
            "SELECT key_id, public_key, created_at, expires_at FROM keys WHERE key_id = $1",
        )
        .bind(key_id as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            SignerError::Internal(format!(
                "Failed to query key record for key_id {key_id}: {e}"
            ))
        })?;

        match result {
            Some((id, public_key, created_at, expires_at)) => {
                crate::recording::debug!("Found key record for key_id: {} in PostgreSQL", key_id);
                Ok(Some(KeyRecord {
                    key_id: id as u32,
                    public_key,
                    created_at: created_at as u64,
                    expires_at: expires_at as u64,
                }))
            }
            None => {
                crate::recording::debug!(
                    "No key record found for key_id: {} in PostgreSQL",
                    key_id
                );
                Ok(None)
            }
        }
    }

    async fn get_key_count(&self) -> SignerResult<u32> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM keys")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to get key count: {e}")))?;

        Ok(count as u32)
    }

    async fn cleanup_expired_keys(&self, tolerance_seconds: u64) -> SignerResult<u32> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let cutoff = now - tolerance_seconds as i64;

        let result = sqlx::query("DELETE FROM keys WHERE expires_at > 0 AND expires_at < $1")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to cleanup expired keys: {e}")))?;

        let deleted_count = result.rows_affected() as u32;

        if deleted_count > 0 {
            crate::recording::info!(
                "Cleaned up {} expired keys from PostgreSQL (tolerance {}s)",
                deleted_count,
                tolerance_seconds
            );
        }

        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> PostgresConfig {
        PostgresConfig {
            host: std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("POSTGRES_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5432),
            database: std::env::var("POSTGRES_DB").unwrap_or_else(|_| "signer_test".to_string()),
            username: std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
            password: std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
            pool_size: 5,
            max_lifetime_secs: 3600,
        }
    }

    /// Fixed 32-byte test KEK (hex). Test-only; never used in production.
    fn test_kek() -> String {
        "0011223344556677889900112233445566778899001122334455667788990011".to_string()
    }

    async fn create_test_backend() -> PostgresBackend {
        let enc = KeyEncryptor::no_encryption();
        PostgresBackend::new(&test_config(), 3600, enc)
            .await
            .unwrap()
    }

    /// Backend with a configured KEK (encryption enabled). Used by the encryption
    /// and migration integration tests below.
    async fn create_test_backend_with_encryption() -> PostgresBackend {
        let enc = KeyEncryptor::from_kek(&test_kek()).unwrap();
        PostgresBackend::new(&test_config(), 3600, enc)
            .await
            .unwrap()
    }

    async fn cleanup_test_data(backend: &PostgresBackend) {
        sqlx::query("TRUNCATE TABLE keys RESTART IDENTITY")
            .execute(&backend.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_postgres_init() {
        let backend = create_test_backend().await;
        cleanup_test_data(&backend).await;

        let count = backend.get_key_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_generate_and_query() {
        let backend = create_test_backend().await;
        cleanup_test_data(&backend).await;

        // Generate key
        let key_pair = backend.generate_and_store_key().await.unwrap();
        assert!(key_pair.key_id > 0);
        assert!(!key_pair.verifying_key.is_empty());

        // Verify verifying key is 32 bytes
        let vk_bytes = BASE64_STANDARD.decode(&key_pair.verifying_key).unwrap();
        assert_eq!(vk_bytes.len(), 32);

        // Query public key
        let public_key = backend.get_public_key(key_pair.key_id).await.unwrap();
        assert_eq!(public_key, Some(key_pair.verifying_key.clone()));

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_sign() {
        let backend = create_test_backend().await;
        cleanup_test_data(&backend).await;

        let key_pair = backend.generate_and_store_key().await.unwrap();
        let message = b"test message";
        let signature = backend.sign(key_pair.key_id, message).await.unwrap();
        assert_eq!(signature.len(), 64);

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_query_nonexistent_key() {
        let backend = create_test_backend().await;
        cleanup_test_data(&backend).await;

        let result = backend.get_public_key(99999).await.unwrap();
        assert_eq!(result, None);

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_key_count() {
        let backend = create_test_backend().await;
        cleanup_test_data(&backend).await;

        assert_eq!(backend.get_key_count().await.unwrap(), 0);

        backend.generate_and_store_key().await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 1);

        backend.generate_and_store_key().await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 2);

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_cleanup_expired_keys() {
        let config = PostgresConfig {
            host: std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("POSTGRES_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5432),
            database: std::env::var("POSTGRES_DB").unwrap_or_else(|_| "signer_test".to_string()),
            username: std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
            password: std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
            pool_size: 5,
            max_lifetime_secs: 3600,
        };

        // Create backend with 1-second TTL
        let backend = PostgresBackend::new(&config, 1, KeyEncryptor::no_encryption())
            .await
            .unwrap();
        cleanup_test_data(&backend).await;

        // Generate key
        backend.generate_and_store_key().await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 1);

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Clean up expired keys
        let cleaned = backend.cleanup_expired_keys(0).await.unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(backend.get_key_count().await.unwrap(), 0);

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_zero_ttl_never_expires() {
        let config = PostgresConfig {
            host: std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("POSTGRES_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5432),
            database: std::env::var("POSTGRES_DB").unwrap_or_else(|_| "signer_test".to_string()),
            username: std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
            password: std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
            pool_size: 5,
            max_lifetime_secs: 3600,
        };

        // TTL is 0 (never expires)
        let backend = PostgresBackend::new(&config, 0, KeyEncryptor::no_encryption())
            .await
            .unwrap();
        cleanup_test_data(&backend).await;

        backend.generate_and_store_key().await.unwrap();
        assert_eq!(backend.get_key_count().await.unwrap(), 1);

        // Cleanup should not delete keys that never expire
        let cleaned = backend.cleanup_expired_keys(0).await.unwrap();
        assert_eq!(cleaned, 0);
        assert_eq!(backend.get_key_count().await.unwrap(), 1);

        cleanup_test_data(&backend).await;
    }

    // ===== Encryption format unit tests (no PostgreSQL required) =====
    // These exercise the fail-closed matrix directly so CI can catch regressions
    // without a live database; the #[ignore] tests below cover the DB path.

    #[test]
    fn test_wrap_then_unwrap_with_kek() {
        let enc = KeyEncryptor::from_kek(&test_kek()).unwrap();
        let plain = BASE64_STANDARD.encode([0xABu8; 32]);

        let stored = wrap_secret(&enc, &plain).unwrap();
        assert!(stored.starts_with(V1_PREFIX), "v1 must carry the prefix");
        assert_ne!(stored, plain, "stored value must not equal plaintext key");

        // Decoded stored bytes must not be the raw 32-byte signing key.
        let stored_bytes = BASE64_STANDARD.decode(&stored[V1_PREFIX.len()..]).unwrap();
        assert_ne!(stored_bytes, vec![0xABu8; 32]);

        let recovered = unwrap_secret(&enc, &stored).unwrap();
        assert_eq!(recovered, plain);
    }

    #[test]
    fn test_wrap_without_kek_is_passthrough_v0() {
        let enc = KeyEncryptor::no_encryption();
        let plain = BASE64_STANDARD.encode([0x11u8; 32]);

        let stored = wrap_secret(&enc, &plain).unwrap();
        assert_eq!(stored, plain, "no KEK → stored as legacy v0 plaintext");

        let recovered = unwrap_secret(&enc, &stored).unwrap();
        assert_eq!(recovered, plain);
    }

    #[test]
    fn test_unwrap_v1_without_kek_fails_closed() {
        let enc_on = KeyEncryptor::from_kek(&test_kek()).unwrap();
        let plain = BASE64_STANDARD.encode([0xCDu8; 32]);
        let stored = wrap_secret(&enc_on, &plain).unwrap();

        let enc_off = KeyEncryptor::no_encryption();
        let err = unwrap_secret(&enc_off, &stored).unwrap_err();
        assert!(
            err.to_string().contains("no KEK configured"),
            "expected fail-closed error, got: {err}"
        );
    }

    #[test]
    fn test_unwrap_v0_with_kek_fails_closed() {
        let enc_on = KeyEncryptor::from_kek(&test_kek()).unwrap();
        let plain = BASE64_STANDARD.encode([0xEEu8; 32]); // legacy v0 plaintext

        let err = unwrap_secret(&enc_on, &plain).unwrap_err();
        assert!(
            err.to_string().contains("Refusing to load unencrypted"),
            "expected fail-closed error, got: {err}"
        );
    }

    #[test]
    fn test_unwrap_v1_wrong_kek_fails() {
        let enc1 = KeyEncryptor::from_kek(&test_kek()).unwrap();
        let plain = BASE64_STANDARD.encode([0x42u8; 32]);
        let stored = wrap_secret(&enc1, &plain).unwrap();

        let other_kek = crate::crypto::KeyEncryptor::generate_kek();
        let enc2 = KeyEncryptor::from_kek(&other_kek).unwrap();
        assert!(
            unwrap_secret(&enc2, &stored).is_err(),
            "wrong KEK must fail"
        );
    }

    // ===== Encryption integration tests (require a PostgreSQL server) =====

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_pg_secret_not_plaintext_when_kek_configured() {
        let backend = create_test_backend_with_encryption().await;
        cleanup_test_data(&backend).await;

        let kp = backend.generate_and_store_key().await.unwrap();

        // The raw column value must be v1 ciphertext, never the recoverable base64 key.
        let stored: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(kp.key_id as i32)
                .fetch_one(&backend.pool)
                .await
                .unwrap();
        assert!(
            stored.starts_with(V1_PREFIX),
            "secret_key must be v1 ciphertext"
        );

        // Signing must still succeed end-to-end through the decrypt path.
        let sig = backend.sign(kp.key_id, b"message").await.unwrap();
        assert_eq!(sig.len(), 64);

        cleanup_test_data(&backend).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_pg_migration_v0_to_v1_then_sign() {
        // Phase 1: insert a v0 plaintext row via a no-KEK backend.
        let plain_backend = create_test_backend().await;
        cleanup_test_data(&plain_backend).await;
        let kp = plain_backend.generate_and_store_key().await.unwrap();
        let stored_before: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(kp.key_id as i32)
                .fetch_one(&plain_backend.pool)
                .await
                .unwrap();
        assert!(
            !stored_before.starts_with(V1_PREFIX),
            "pre-migration row must be v0 plaintext"
        );
        drop(plain_backend);

        // Phase 2: open with KEK → init migrates v0→v1 transactionally.
        let enc_backend = create_test_backend_with_encryption().await;
        let stored_after: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(kp.key_id as i32)
                .fetch_one(&enc_backend.pool)
                .await
                .unwrap();
        assert!(
            stored_after.starts_with(V1_PREFIX),
            "migration must produce v1"
        );

        // Phase 3: signing works post-migration.
        let sig = enc_backend
            .sign(kp.key_id, b"after-migration")
            .await
            .unwrap();
        assert_eq!(sig.len(), 64);

        // Phase 4: re-open with the same KEK → idempotent (still v1, still signs).
        drop(enc_backend);
        let enc_backend2 = create_test_backend_with_encryption().await;
        let stored_2: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(kp.key_id as i32)
                .fetch_one(&enc_backend2.pool)
                .await
                .unwrap();
        assert!(stored_2.starts_with(V1_PREFIX));
        assert!(enc_backend2.sign(kp.key_id, b"idempotent").await.is_ok());

        cleanup_test_data(&enc_backend2).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_pg_migration_aborts_on_malformed_v0() {
        // Phase 1: insert a malformed v0 row (valid base64 but != 32 bytes) via a
        // no-KEK backend.
        let plain_backend = create_test_backend().await;
        cleanup_test_data(&plain_backend).await;

        let malformed_b64 = BASE64_STANDARD.encode([0u8; 16]); // 16 bytes, not 32
        let key_id: i32 = sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO keys (public_key, secret_key, created_at, expires_at)
               VALUES ($1, $2, $3, $4) RETURNING key_id"#,
        )
        .bind(BASE64_STANDARD.encode([0u8; 32]))
        .bind(&malformed_b64)
        .bind(0i64)
        .bind(0i64)
        .fetch_one(&plain_backend.pool)
        .await
        .unwrap();
        drop(plain_backend);

        // Phase 2: open with KEK → migration must abort and new() must return Err.
        let enc = KeyEncryptor::from_kek(&test_kek()).unwrap();
        let result = PostgresBackend::new(&test_config(), 3600, enc).await;
        assert!(
            result.is_err(),
            "migration of a malformed v0 row must abort startup (fail closed)"
        );

        // Phase 3: the malformed row must be unchanged (transaction rolled back).
        let probe = create_test_backend().await;
        let stored: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(key_id)
                .fetch_one(&probe.pool)
                .await
                .unwrap();
        assert_eq!(
            stored, malformed_b64,
            "rollback must leave the malformed v0 row untouched"
        );

        cleanup_test_data(&probe).await;
    }

    #[tokio::test]
    #[ignore] // Requires a PostgreSQL server
    async fn test_pg_from_config_encrypts_with_kek() {
        use crate::storage::{KeyStorage, StorageBackend, StorageConfig};

        // Clean slate via a throwaway no-KEK backend sharing the same DB.
        let cleaner = create_test_backend().await;
        cleanup_test_data(&cleaner).await;
        drop(cleaner);

        let storage_config = StorageConfig {
            backend: StorageBackend::Postgres,
            key_ttl_seconds: 3600,
            sqlite: None,
            postgres: Some(test_config()),
        };
        let encryptor = KeyEncryptor::from_kek(&test_kek()).unwrap();

        // Construct through from_config — the exact path that previously dropped
        // the encryptor on the Postgres branch (issue #344 root cause).
        let storage = KeyStorage::from_config(&storage_config, encryptor, std::path::Path::new(""))
            .await
            .expect("from_config must succeed on Postgres+KEK");

        let kp = storage.generate_and_store_key().await.unwrap();

        // Read the raw column via the underlying pool to confirm it is v1
        // ciphertext, not the recoverable base64 key.
        let pool = match &storage {
            KeyStorage::Postgres(b) => b.pool.clone(),
            _ => unreachable!("backend=Postgres"),
        };
        let stored: String =
            sqlx::query_scalar::<_, String>("SELECT secret_key FROM keys WHERE key_id = $1")
                .bind(kp.key_id as i32)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            stored.starts_with(V1_PREFIX),
            "from_config Postgres path must encrypt (v1), got: {stored}"
        );

        // Signing must still work end-to-end through the decrypt path.
        let sig = storage.sign(kp.key_id, b"from-config").await.unwrap();
        assert_eq!(sig.len(), 64);

        let cleaner = create_test_backend().await;
        cleanup_test_data(&cleaner).await;
    }
}
