//! TURN authenticator
//!
//! Implements authentication and authorization for the TURN server,
//! based on HMAC-SHA1 time-limited credentials (coturn --use-auth-secret compatible format).
//!
//! # Authentication flow
//!
//! 1. AIS generates TurnCredential { username = "<expires_at>:<actor_id>", password = base64(HMAC-SHA1(turn_secret, username)) }
//! 2. Client authenticates to the TURN server using (username, password)
//! 3. TURN server computes MD5(username:realm:password) as the long-term credential key
//! 4. turn_crate uses this key to verify HMAC-SHA1 message integrity in requests
//!
//! # Security invariants
//!
//! - **Validate before cache lookup**: every request parses the username and checks
//!   `expires_at > now`; expired or malformed credentials are always rejected.
//! - **Cache entries never extend credential lifetime**: entries retain `expires_at`,
//!   and cache hits do not refresh it.
//! - **Secret rotations are isolated**: each authenticator instance owns its cache and
//!   immutable `turn_secret`, so a new secret starts with a new empty cache.

use base64::prelude::*;
use hmac::{Hmac, Mac};
use lru::LruCache;
use sha1::Sha1;
use std::hash::Hasher;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use turn_crate::Error;
use turn_crate::auth::AuthHandler;
use twox_hash::XxHash64;

type HmacSha1 = Hmac<Sha1>;

// Per-authenticator LRU cache capacity.
// Cache key: hash of (username, realm) as u128.
// Cache value: CacheEntry { key = MD5(username:realm:password), expires_at }.
const AUTH_CACHE_CAPACITY: usize = 4096;

/// Clock abstraction (Unix seconds) for deterministic tests.
type NowFn = Arc<dyn Fn() -> u64 + Send + Sync>;

/// System clock (Unix seconds).
fn system_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Cached long-term credential key and its expiration time.
///
/// `expires_at` comes from the username and is rechecked on cache hits so an
/// expired credential can never be accepted from the cache.
#[derive(Clone)]
struct CacheEntry {
    key: Vec<u8>,
    expires_at: u64,
}

/// TURN authenticator (HMAC time-limited credential mode).
///
/// Each instance owns its credential cache and has an immutable `turn_secret`.
/// A secret rotation therefore creates a new instance with an empty cache.
pub struct Authenticator {
    turn_secret: String,
    cache: Mutex<LruCache<u128, CacheEntry>>,
    now: NowFn,
}

impl Authenticator {
    pub fn new(turn_secret: String) -> Result<Self, Error> {
        Self::build(turn_secret, Arc::new(system_now))
    }

    /// Build an authenticator with an injected clock for deterministic tests.
    #[cfg(test)]
    fn with_test_clock(turn_secret: String, now: NowFn) -> Result<Self, Error> {
        Self::build(turn_secret, now)
    }

    fn build(turn_secret: String, now: NowFn) -> Result<Self, Error> {
        platform::recording::info!(
            "TURN authenticator initialized (HMAC time-limited credential mode, LRU cache)"
        );
        let cap =
            NonZeroUsize::new(AUTH_CACHE_CAPACITY).expect("AUTH_CACHE_CAPACITY must be non-zero");
        Ok(Self {
            turn_secret,
            cache: Mutex::new(LruCache::new(cap)),
            now,
        })
    }

    /// Get cache statistics for monitoring and debugging.
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.lock().expect("auth cache poisoned");
        (cache.len(), cache.cap().get())
    }

    /// Clear this authenticator's cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().expect("auth cache poisoned");
        cache.clear();
        platform::recording::info!("TURN auth key cache cleared");
    }

    /// Compute HMAC-SHA1 password (base64 encoded)
    ///
    /// `password = base64(HMAC-SHA1(turn_secret, username))`
    fn compute_password(&self, username: &str) -> String {
        let mut mac = HmacSha1::new_from_slice(self.turn_secret.as_bytes())
            .expect("HMAC-SHA1 accepts any key length");
        mac.update(username.as_bytes());
        BASE64_STANDARD.encode(mac.finalize().into_bytes())
    }
}

/// Compute cache key
#[inline]
fn compute_cache_key(username: &str, realm: &str) -> u128 {
    let mut h1 = XxHash64::with_seed(0);
    h1.write(username.as_bytes());
    let k1 = h1.finish();

    let mut h2 = XxHash64::with_seed(0);
    h2.write(realm.as_bytes());
    let k2 = h2.finish();

    ((k1 as u128) << 64) | (k2 as u128)
}

impl AuthHandler for Authenticator {
    fn auth_handle(
        &self,
        username: &str,
        server_realm: &str,
        src_addr: SocketAddr,
    ) -> Result<Vec<u8>, Error> {
        platform::recording::debug!(
            "Processing TURN auth: username={}, realm={}, src={}",
            username,
            server_realm,
            src_addr
        );

        // 1. Parse username = "<expires_at>:<actor_id>" before checking the cache.
        // Malformed usernames are rejected even if a matching cache entry could exist.
        let (expires_at_str, actor_id) = username.split_once(':').ok_or_else(|| {
            platform::recording::warn!(
                "TURN username format invalid (expected '<expires_at>:<actor_id>'): username={}",
                username
            );
            Error::Other("invalid username format".to_string())
        })?;

        let expires_at: u64 = expires_at_str.parse().map_err(|e| {
            platform::recording::warn!(
                "TURN username expires_at parse failed: username={}, error={}",
                username,
                e
            );
            Error::Other(format!("invalid expires_at in username: {e}"))
        })?;

        let now = (self.now)();
        let cache_key = compute_cache_key(username, server_realm);

        // 2. Check expiration before the cache. Expired credentials are rejected and
        // any matching entry is removed so a hot cache cannot extend their lifetime.
        if expires_at <= now {
            self.cache
                .lock()
                .expect("auth cache poisoned")
                .pop(&cache_key);
            platform::recording::warn!(
                "TURN credential expired: username={}, expires_at={}, now={}",
                username,
                expires_at,
                now
            );
            return Err(Error::Other("TURN credential expired".to_string()));
        }

        // 3. On a cache hit, defensively recheck the entry's expiration without
        // refreshing it, then return the previously derived key.
        {
            let mut cache = self.cache.lock().expect("auth cache poisoned");
            if let Some(entry) = cache.get(&cache_key) {
                if entry.expires_at > now {
                    platform::recording::debug!("TURN auth cache hit: username={}", username);
                    return Ok(entry.key.clone());
                }
                // Remove stale entries before deriving a fresh key. This is defensive:
                // the username expiration check above should normally reject first.
                cache.pop(&cache_key);
            }
        }

        // 4. Compute HMAC-SHA1 password
        let password = self.compute_password(username);

        // 5. Compute long-term credential key: MD5(username:realm:password)
        let integrity_text = format!("{username}:{server_realm}:{password}");
        let digest = md5::compute(integrity_text.as_bytes());
        let result = digest.to_vec();

        // 6. Store the key together with its fixed expiration time.
        self.cache.lock().expect("auth cache poisoned").put(
            cache_key,
            CacheEntry {
                key: result.clone(),
                expires_at,
            },
        );

        platform::recording::debug!(
            "TURN auth succeeded: actor_id={}, expires_at={}, cache_size={}/{}",
            actor_id,
            expires_at,
            self.cache_stats().0,
            self.cache_stats().1
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    const TEST_START: u64 = 1_000_000;

    fn make_auth() -> Authenticator {
        Authenticator::new("test-secret".to_string()).expect("authenticator should initialize")
    }

    fn src_addr() -> SocketAddr {
        "127.0.0.1:3478".parse().expect("valid socket addr")
    }

    /// Build a manually controlled test clock and return its shared current time.
    fn test_clock(initial: u64) -> (NowFn, Arc<AtomicU64>) {
        let now = Arc::new(AtomicU64::new(initial));
        let clock = {
            let now = Arc::clone(&now);
            Arc::new(move || now.load(Ordering::SeqCst)) as NowFn
        };
        (clock, now)
    }

    fn make_auth_with_clock(initial: u64) -> (Authenticator, Arc<AtomicU64>) {
        let (clock, now) = test_clock(initial);
        let auth = Authenticator::with_test_clock("test-secret".to_string(), clock)
            .expect("authenticator should initialize");
        (auth, now)
    }

    #[test]
    fn test_authenticator_creation() {
        let _auth = make_auth();
    }

    #[test]
    fn test_cache_key_computation() {
        let key1 = compute_cache_key("user1", "realm1");
        let key2 = compute_cache_key("user1", "realm1");
        let key3 = compute_cache_key("user2", "realm1");
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_cache_stats() {
        let auth = make_auth();
        let (size, capacity) = auth.cache_stats();
        assert_eq!(size, 0);
        assert_eq!(capacity, AUTH_CACHE_CAPACITY);
    }

    #[test]
    fn test_md5_computation() {
        let integrity_text = "testuser:testrealm:testpass";
        let result = md5::compute(integrity_text.as_bytes());
        assert_eq!(result.len(), 16);
        let result2 = md5::compute(integrity_text.as_bytes());
        assert_eq!(result.to_vec(), result2.to_vec());
    }

    #[test]
    fn test_auth_handle_rejects_invalid_username_format() {
        let auth = make_auth();

        // Username validation runs before cache lookup, so malformed values are rejected.
        let err = auth
            .auth_handle("invalid-no-colon", "actrix.local", src_addr())
            .expect_err("invalid username should be rejected");

        assert!(
            err.to_string().contains("invalid username format"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_auth_handle_rejects_expired_credential() {
        let auth = make_auth();

        // Expired username (expires_at in the past)
        let username = "1000000000:test-actor"; // timestamp in the past
        let err = auth
            .auth_handle(username, "actrix.local", src_addr())
            .expect_err("expired credential should be rejected");

        assert!(
            err.to_string().contains("expired"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_auth_handle_accepts_valid_credential() {
        let auth = make_auth();

        // Non-expired credential
        let now = system_now();
        let expires_at = now + 3600;
        let username = format!("{expires_at}:test-actor");

        let result = auth.auth_handle(&username, "actrix.local", src_addr());
        assert!(result.is_ok(), "valid credential should succeed");
        assert_eq!(result.unwrap().len(), 16, "should return 16-byte MD5 key");
    }

    #[test]
    fn test_auth_handle_uses_cache() {
        let (auth, _now) = make_auth_with_clock(TEST_START);
        let username = format!("{}:test-actor", TEST_START + 3600);

        // First authentication derives and caches the key.
        let key1 = auth
            .auth_handle(&username, "actrix.local", src_addr())
            .expect("first auth should succeed");
        assert_eq!(auth.cache_stats().0, 1, "one entry should be cached");

        // Second authentication returns the cached key without inserting another entry.
        let key2 = auth
            .auth_handle(&username, "actrix.local", src_addr())
            .expect("cached auth should succeed");
        assert_eq!(key1, key2, "cache hit should return the same key");
        assert_eq!(
            auth.cache_stats().0,
            1,
            "cache hit should not insert a new entry"
        );
    }

    #[test]
    fn test_cache_hit_does_not_extend_expiry() {
        let (auth, now) = make_auth_with_clock(TEST_START);
        let expires_at = TEST_START + 100;
        let username = format!("{expires_at}:test-actor");

        // Authentication succeeds before expiration.
        assert!(
            auth.auth_handle(&username, "actrix.local", src_addr())
                .is_ok(),
            "auth should succeed before expiry"
        );
        assert_eq!(auth.cache_stats().0, 1, "credential should be cached");

        // A cache hit still succeeds immediately before expiration.
        now.store(expires_at - 1, Ordering::SeqCst);
        assert!(
            auth.auth_handle(&username, "actrix.local", src_addr())
                .is_ok(),
            "cache hit should succeed just before expiry"
        );

        // At the expiration boundary (expires_at <= now), the credential is rejected.
        now.store(expires_at, Ordering::SeqCst);
        let err = auth
            .auth_handle(&username, "actrix.local", src_addr())
            .expect_err("cached credential must be rejected once expired");
        assert!(
            err.to_string().contains("expired"),
            "unexpected error: {err}"
        );

        // The expired entry is removed instead of extending the credential lifetime.
        assert_eq!(
            auth.cache_stats().0,
            0,
            "expired entry should be evicted from cache"
        );

        // The credential remains rejected after moving farther past the boundary.
        now.store(expires_at + 3600, Ordering::SeqCst);
        assert!(
            auth.auth_handle(&username, "actrix.local", src_addr())
                .is_err(),
            "credential must remain rejected well past expiry"
        );
    }

    #[test]
    fn test_secret_rotation_does_not_reuse_cached_key() {
        let expires_at = system_now() + 3600;
        let username = format!("{expires_at}:test-actor");

        // Derive and cache a key with the old secret.
        let auth_old = Authenticator::new("secret-one".to_string()).expect("init");
        let key_old = auth_old
            .auth_handle(&username, "actrix.local", src_addr())
            .expect("auth with old secret should succeed");

        // Rotate the secret by creating a new authenticator with an empty cache.
        let auth_new = Authenticator::new("secret-two".to_string()).expect("init");
        let key_new = auth_new
            .auth_handle(&username, "actrix.local", src_addr())
            .expect("auth with rotated secret should succeed");

        // The new authenticator cannot reuse a key derived with the old secret.
        assert_ne!(
            key_old, key_new,
            "rotated secret must not reuse the previously cached key"
        );
    }
}
