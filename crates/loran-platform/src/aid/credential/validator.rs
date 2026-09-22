//! AId Credential verifier
//!
//! Verifies AId Credential based on Ed25519 signatures:
//! - claims are plaintext proto bytes, signed by the Issuer Ed25519 private key
//! - The verifier looks up the corresponding VerifyingKey by key_id via `key_cache` to complete verification

use super::error::AidError;
use super::verifier::AIdCredentialVerifier;
use crate::aid::key_cache::KeyCache;
use actr_protocol::{AIdCredential, IdentityClaims};
use ed25519_dalek::VerifyingKey;
use once_cell::sync::OnceCell;
use std::sync::Arc;

static KEY_CACHE: OnceCell<Arc<KeyCache>> = OnceCell::new();

/// AId Token verifier - verifies AIdCredential via static methods (Ed25519 signature)
pub struct AIdCredentialValidator;

impl AIdCredentialValidator {
    /// Initialize the global KeyCache instance (typically called once at service startup)
    ///
    /// Can be called multiple times; the first call takes effect, subsequent calls are ignored (idempotent).
    pub async fn init(sqlite_path: &std::path::Path) -> Result<(), AidError> {
        if KEY_CACHE.get().is_some() {
            return Ok(());
        }
        let cache_db = sqlite_path.join("signaling_key_cache.db");
        let cache = KeyCache::new(&cache_db).await?;
        // Best-effort set; concurrent callers may race but both produce valid caches
        let _ = KEY_CACHE.set(Arc::new(cache));
        Ok(())
    }

    /// Get the global KeyCache instance
    fn get_cache() -> Result<Arc<KeyCache>, AidError> {
        KEY_CACHE.get().cloned().ok_or(AidError::InvalidFormat)
    }

    /// Verify AIdCredential: Ed25519 signature verification + realm_id check
    ///
    /// # Returns
    /// `Ok((IdentityClaims, false))` — Ed25519 mode has no grace period concept, the second value is always `false`
    pub async fn check(
        credential: &AIdCredential,
        realm_id: u32,
        audience: &str,
    ) -> Result<(IdentityClaims, bool), AidError> {
        let cache = Self::get_cache()?;
        let key_id = credential.key_id;

        let (verifying_key, _expires_at) = cache
            .get_cached_key(key_id)
            .await?
            .ok_or(AidError::InvalidFormat)?; // key_id not found in cache

        let claims = AIdCredentialVerifier::verify(credential, &verifying_key)?;

        if claims.realm_id != realm_id {
            return Err(AidError::InvalidFormat);
        }
        if claims.protocol_version != 1
            || claims.issuer != format!("actr:realm:{realm_id}:authority")
            || claims.provider != "actrix-official"
            || claims.audience != audience
        {
            return Err(AidError::InvalidFormat);
        }

        Ok((claims, false))
    }

    /// Returns the Ed25519 public key bytes for the given key_id (used for GetSigningKeyRequest responses)
    pub async fn get_key_bytes(key_id: u32) -> Result<Option<Vec<u8>>, AidError> {
        let cache = Self::get_cache()?;
        let result = cache.get_cached_key(key_id).await?;
        Ok(result.map(|(verifying_key, _expires_at)| verifying_key.as_bytes().to_vec()))
    }

    /// Write the Ed25519 verifying key into the global KeyCache (called by the issuer)
    ///
    /// Issuer calls this method after loading or refreshing the signing key, so that signaling and other services can verify credentials.
    /// If the KeyCache is not yet initialized (`init` not called), this method is a no-op (non-fatal).
    pub async fn populate_key(
        key_id: u32,
        verifying_key: &VerifyingKey,
        expires_at: u64,
    ) -> Result<(), AidError> {
        let cache = Self::get_cache()?;
        cache.cache_key(key_id, verifying_key, expires_at).await
    }

    /// Persistently write the Ed25519 verifying key to the SQLite DB; does not require KeyCache to be initialized.
    ///
    /// Used by the issuer to write keys before `init` is called, ensuring that subsequent `init` calls can read the key from the DB.
    /// If the global KeyCache is already initialized, also updates the in-memory cache.
    pub async fn persist_key(
        sqlite_path: &std::path::Path,
        key_id: u32,
        verifying_key: &VerifyingKey,
        expires_at: u64,
    ) -> Result<(), AidError> {
        let cache_db = sqlite_path.join("signaling_key_cache.db");
        let cache = KeyCache::new(&cache_db).await?;
        cache.cache_key(key_id, verifying_key, expires_at).await?;

        // If the global cache is initialized, sync the in-memory cache
        if let Some(global_cache) = KEY_CACHE.get() {
            let _ = global_cache
                .cache_key(key_id, verifying_key, expires_at)
                .await;
        }

        Ok(())
    }

    /// Synchronous verification (for non-async contexts, e.g. TURN authentication)
    pub fn check_sync(
        credential: &AIdCredential,
        realm_id: u32,
        audience: &str,
    ) -> Result<IdentityClaims, AidError> {
        tokio::task::block_in_place(|| {
            let handle =
                tokio::runtime::Handle::try_current().map_err(|_| AidError::InvalidFormat)?;
            let (claims, _) = handle.block_on(Self::check(credential, realm_id, audience))?;
            Ok(claims)
        })
    }
}
