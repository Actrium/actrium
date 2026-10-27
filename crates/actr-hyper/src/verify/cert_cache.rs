//! Production mode Publisher Registry public key cache
//!
//! `PublisherCertCache` fetches publisher Ed25519 public keys on demand from
//! Issuer `GET /registry/publishers/{name}/verifying-key`, caching locally (TTL 1 hour).
//!
//! Uses `std::sync::RwLock` (not tokio) internally because:
//! - Cache reads/writes are extremely short memory operations that won't block the tokio executor
//! - Provides a synchronous read path for `RegistryTrust::verify_package` to call directly

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use base64::Engine;
use ed25519_dalek::VerifyingKey;

use crate::error::{HyperError, HyperResult};

/// Publisher Registry public key cache entry
struct CacheEntry {
    key: VerifyingKey,
    fetched_at: Instant,
}

/// Structured cert cache key: `(publisher, signing_key_id)`.
///
/// A tuple avoids the `publisher:key_id` string-concatenation collision, where
/// e.g. `("a", "b:c")` and `("a:b", "c")` would otherwise map to the same key.
type CertCacheKey = (String, Option<String>);

fn cert_cache_key(publisher: &str, key_id: Option<&str>) -> CertCacheKey {
    (publisher.to_string(), key_id.map(str::to_string))
}

/// Production mode Publisher Registry Ed25519 public key cache
///
/// Fetches publisher public keys on demand from the Issuer endpoint, cache TTL defaults to 1 hour.
/// Shared across tasks via `Arc<PublisherCertCache>`.
pub struct PublisherCertCache {
    issuer_endpoint: String,
    http: reqwest::Client,
    ttl: Duration,
    cache: RwLock<HashMap<CertCacheKey, CacheEntry>>,
}

impl std::fmt::Debug for PublisherCertCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublisherCertCache")
            .field("issuer_endpoint", &self.issuer_endpoint)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl PublisherCertCache {
    pub fn new(issuer_endpoint: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            issuer_endpoint: issuer_endpoint.into(),
            http: reqwest::Client::new(),
            ttl: Duration::from_secs(3600),
            cache: RwLock::new(HashMap::new()),
        })
    }

    /// Used in `RegistryTrust::verify_package` synchronous path;
    /// caller must ensure the cache has been warmed via `get_or_fetch` beforehand.
    pub fn get_from_cache(&self, publisher: &str, key_id: Option<&str>) -> Option<VerifyingKey> {
        let cache_key = cert_cache_key(publisher, key_id);
        let cache = self.cache.read().expect("cert_cache read lock poisoned");
        cache.get(&cache_key).and_then(|entry| {
            // Instant-based TTL is a deliberate in-process freshness policy:
            // it cannot be skewed by wall-clock changes, but it freezes while
            // the process is suspended, so after a long suspend entries look
            // fresher than they really are. Known and accepted trade-off.
            if entry.fetched_at.elapsed() < self.ttl {
                Some(entry.key)
            } else {
                None
            }
        })
    }

    /// Get the Ed25519 verifying key for the specified publisher
    ///
    /// Reads from cache first (if not expired); on miss, fetches from Issuer and updates cache.
    pub async fn get_or_fetch(
        &self,
        publisher: &str,
        key_id: Option<&str>,
    ) -> HyperResult<VerifyingKey> {
        // fast path: read cache
        if let Some(key) = self.get_from_cache(publisher, key_id) {
            tracing::debug!(publisher, ?key_id, "Publisher Registry pubkey cache hit");
            return Ok(key);
        }

        tracing::debug!(
            publisher,
            ?key_id,
            "Publisher Registry pubkey cache miss, fetching from Issuer"
        );

        // slow path: HTTP fetch
        let key = self.fetch_from_issuer(publisher, key_id).await?;

        // write to cache (brief blocking lock, just a HashMap insert)
        let cache_key = cert_cache_key(publisher, key_id);
        {
            let mut cache = self.cache.write().expect("cert_cache write lock poisoned");
            cache.insert(
                cache_key,
                CacheEntry {
                    key,
                    // Monotonic stamp; see the TTL note in `get_from_cache`.
                    fetched_at: Instant::now(),
                },
            );
        }

        tracing::info!(
            publisher,
            ?key_id,
            "Publisher Registry pubkey fetched from Issuer and cached"
        );
        Ok(key)
    }

    /// Fetch public key from Issuer `GET /registry/publishers/{publisher}/verifying-key`
    async fn fetch_from_issuer(
        &self,
        publisher: &str,
        key_id: Option<&str>,
    ) -> HyperResult<VerifyingKey> {
        // Build the URL with the url crate (not `format!`) so the publisher
        // and key_id are percent-encoded as path segments / query pairs and can
        // never inject extra path or query structure into the Issuer request.
        let mut url = reqwest::Url::parse(&self.issuer_endpoint).map_err(|e| {
            HyperError::Config(format!(
                "invalid Issuer endpoint `{}`: {e}",
                self.issuer_endpoint
            ))
        })?;
        url.path_segments_mut()
            .map_err(|_| HyperError::Config("Issuer endpoint cannot be a base URL".to_string()))?
            .extend(["registry", "publishers", publisher, "verifying-key"]);
        if let Some(id) = key_id {
            url.query_pairs_mut().append_pair("key_id", id);
        }
        tracing::debug!(url = %url, "fetching Publisher Registry pubkey from Issuer");

        let resp = self.http.get(url).send().await.map_err(|e| {
            HyperError::UntrustedPublisher(format!(
                "failed to fetch Publisher Registry pubkey ({publisher}): {e}"
            ))
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(
                publisher,
                status = status.as_u16(),
                body,
                "Issuer returned non-2xx, Publisher Registry pubkey fetch failed"
            );
            return Err(HyperError::UntrustedPublisher(format!(
                "Issuer refused to provide Publisher Registry pubkey ({publisher}), status={status}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct VerifyingKeyResp {
            /// Base64-encoded Ed25519 verifying key (32 bytes)
            public_key: String,
        }

        let body: VerifyingKeyResp = resp.json().await.map_err(|e| {
            HyperError::UntrustedPublisher(format!(
                "failed to parse Publisher Registry pubkey response ({publisher}): {e}"
            ))
        })?;

        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&body.public_key)
            .map_err(|e| {
                HyperError::UntrustedPublisher(format!(
                    "Publisher Registry pubkey base64 decode failed ({publisher}): {e}"
                ))
            })?;

        let key_arr: [u8; 32] = key_bytes.try_into().map_err(|v: Vec<u8>| {
            HyperError::UntrustedPublisher(format!(
                "Publisher Registry pubkey length incorrect ({publisher}), expected 32 bytes, got {} bytes",
                v.len()
            ))
        })?;

        VerifyingKey::from_bytes(&key_arr).map_err(|e| {
            HyperError::UntrustedPublisher(format!(
                "Publisher Registry pubkey format invalid ({publisher}): {e}"
            ))
        })
    }
}

#[cfg(test)]
#[path = "cert_cache_tests.rs"]
mod tests;
