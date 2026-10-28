//! Background and registration-time freshness of Publisher Profiles
//! (`actr/protocols/actor-package.zh.md` section 6.8): the package signing
//! key a registration resolves in this Loran's Publisher Registry must still
//! be listed by the publisher domain's current Publisher Profile.
//!
//! The registry row is a projection captured by `register_publisher`. This
//! refresher re-checks the domain within [`DEFAULT_TTL`] and accepts the
//! fetched Profile into the registry, which enforces the pins stored when the
//! publisher registered: a key the domain has removed is revoked here on the
//! next check, even without another Actor registration.
//! Checks fail closed — a registration whose Publisher Profile cannot be
//! re-checked is refused rather than verified against a stale projection.

use actr_protocol::{PublisherKeys, PublisherProfile};
use platform::net::FetchError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a successful check stands before the domain is fetched again.
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 3600);

/// Shortest spacing of fetch attempts for one publisher after a failure.
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);

/// Check which registered publishers are due once per minute. Successful
/// checks still have a 24-hour TTL; failures retain the shared retry spacing.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const SWEEP_PAGE_SIZE: i64 = 64;
const SWEEP_CONCURRENCY: usize = 8;

/// Fetches a public document over HTTPS. Production passes the SSRF-checked
/// [`platform::net::CachedFetcher`]; tests substitute a stub.
#[async_trait::async_trait]
pub trait PublicDocumentFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<Arc<[u8]>, FetchError>;
}

#[async_trait::async_trait]
impl PublicDocumentFetcher for platform::net::CachedFetcher {
    async fn fetch(&self, url: &str) -> Result<Arc<[u8]>, FetchError> {
        self.get(url).await
    }
}

/// Why a Publisher Profile could not be re-checked. Every variant fails the
/// registration closed; the endpoint maps them to a retryable status.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("cannot fetch the Publisher Profile of {domain}: {message}")]
    Fetch { domain: String, message: String },
    #[error("the Publisher Profile of {domain} was refused: {message}")]
    Profile { domain: String, message: String },
    #[error("the Publisher Profile of {domain} failed its last check too recently")]
    Throttled { domain: String },
}

#[derive(Default)]
struct Entry {
    /// The last successful check.
    checked_at: Option<Instant>,
    /// A cache hit never extends the signed document's validity period.
    profile_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The last attempt, success or failure, for the retry spacing.
    attempted_at: Option<Instant>,
}

impl Entry {
    fn is_current(&self, now: Instant, ttl: Duration) -> bool {
        self.checked_at
            .is_some_and(|checked| now.duration_since(checked) < ttl)
            && self
                .profile_expires_at
                .is_some_and(|expires| chrono::Utc::now() < expires)
    }
}

/// Re-checks registered publishers' Publisher Profiles in the background and
/// during registration ([`crate::issuer::AIdIssuer`]'s package verification).
pub struct PublisherProfileRefresher {
    fetcher: Arc<dyn PublicDocumentFetcher>,
    ttl: Duration,
    retry_after_failure: Duration,
    entries: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Entry>>>>,
}

impl PublisherProfileRefresher {
    pub fn new(
        fetcher: Arc<dyn PublicDocumentFetcher>,
        ttl: Duration,
        retry_after_failure: Duration,
    ) -> Self {
        Self {
            fetcher,
            ttl,
            retry_after_failure,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_defaults(fetcher: Arc<dyn PublicDocumentFetcher>) -> Self {
        Self::new(fetcher, DEFAULT_TTL, RETRY_AFTER_FAILURE)
    }

    /// Start an immediate sweep, then repeat until the Issuer shuts down.
    /// The same refresher is shared with registration: neither path bypasses
    /// pins, successful TTLs, signed expiry, or failed-check backoff.
    pub fn spawn(
        self: &Arc<Self>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.clone().run(SWEEP_INTERVAL, cancel))
    }

    async fn run(self: Arc<Self>, interval: Duration, cancel: tokio_util::sync::CancellationToken) {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                result = self.sweep() => {
                    if let Err(error) = result {
                        platform::recording::warn!("Publisher Profile sweep failed: {error}");
                    }
                }
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }

    async fn sweep(self: &Arc<Self>) -> Result<(), sqlx::Error> {
        let pool = platform::storage::db::get_database().get_pool();
        // A fixed high-water mark makes a pass finite even during ongoing
        // registrations. Keyset pages avoid loading every Profile into memory
        // and do not skip rows when an administrator removes a publisher.
        let end: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM publisher")
            .fetch_one(pool)
            .await?;
        let mut after = 0_i64;
        loop {
            let page: Vec<(i64, String)> = sqlx::query_as(
                "SELECT id, name FROM publisher
                 WHERE id > ? AND id <= ? AND status = 'active' AND profile_pins IS NOT NULL
                 ORDER BY id LIMIT ?",
            )
            .bind(after)
            .bind(end)
            .bind(SWEEP_PAGE_SIZE)
            .fetch_all(pool)
            .await?;
            if page.is_empty() {
                return Ok(());
            }
            // JoinSet aborts its children when cancellation drops the sweep,
            // including a fetch or a wait for a registration's domain lock.
            let mut checks = tokio::task::JoinSet::new();
            for (id, domain) in page {
                after = id;
                if checks.len() == SWEEP_CONCURRENCY {
                    Self::join_check(&mut checks).await;
                }
                let refresher = self.clone();
                checks.spawn(async move {
                    if let Err(error) = refresher.ensure_current(&domain).await {
                        platform::recording::warn!("Publisher Profile check failed: {error}");
                    }
                });
            }
            while !checks.is_empty() {
                Self::join_check(&mut checks).await;
            }
        }
    }

    async fn join_check(checks: &mut tokio::task::JoinSet<()>) {
        if let Some(Err(error)) = checks.join_next().await {
            platform::recording::warn!("Publisher Profile check task failed: {error}");
        }
    }

    /// Ensure `domain`'s Publisher Profile was checked recently, fetching and
    /// accepting it again when it has not been. A failed check is retried at
    /// most every [`RETRY_AFTER_FAILURE`]; in between it stays failed.
    pub async fn ensure_current(&self, domain: &str) -> Result<(), RefreshError> {
        self.refresh(domain, false).await
    }

    /// A missing key requests a refresh even inside the successful TTL, but
    /// still observes the per-publisher minimum spacing between attempts.
    pub async fn refresh_missing_key(&self, domain: &str) -> Result<(), RefreshError> {
        self.refresh(domain, true).await
    }

    async fn refresh(&self, domain: &str, missing_key: bool) -> Result<(), RefreshError> {
        let entry = {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            entries.entry(domain.to_string()).or_default().clone()
        };
        // Serialize checks of one publisher across registrations. Other
        // publishers do not wait for this domain's network request.
        let mut entry = entry.lock().await;
        let now = Instant::now();
        if !missing_key && entry.is_current(now, self.ttl) {
            return Ok(());
        }
        if entry
            .attempted_at
            .is_some_and(|attempted| now.duration_since(attempted) < self.retry_after_failure)
        {
            return if entry.is_current(now, self.ttl) {
                // A recent accepted Profile still lacks this key: the caller
                // re-resolves it and refuses it without another fetch.
                Ok(())
            } else {
                Err(RefreshError::Throttled {
                    domain: domain.to_string(),
                })
            };
        }
        entry.attempted_at = Some(now);
        entry.checked_at = None;
        entry.profile_expires_at = None;
        let expires_at = self.check(domain).await?;
        if chrono::Utc::now() >= expires_at {
            return Err(RefreshError::Profile {
                domain: domain.to_string(),
                message: "Publisher Profile expired during refresh".to_string(),
            });
        }
        entry.checked_at = Some(Instant::now());
        entry.profile_expires_at = Some(expires_at);
        Ok(())
    }

    /// Fetch `domain`'s authority descriptor, verify its Publisher Profile,
    /// and accept it into this Loran's Publisher Registry.
    async fn check(&self, domain: &str) -> Result<chrono::DateTime<chrono::Utc>, RefreshError> {
        let url = format!("https://{domain}/.well-known/actr");
        let document = self
            .fetcher
            .fetch(&url)
            .await
            .map_err(|error| RefreshError::Fetch {
                domain: domain.to_string(),
                message: error.to_string(),
            })?;
        let (profile, keys) = verify_descriptor(domain, &document)?;
        let expires_at = chrono::DateTime::parse_from_rfc3339(&keys.profile_expires_at)
            .map_err(|error| RefreshError::Profile {
                domain: domain.to_string(),
                message: error.to_string(),
            })?
            .with_timezone(&chrono::Utc);
        let pool = platform::storage::db::get_database().get_pool().clone();
        actrix_publisher_registry::PublisherRegistryManager::new(pool)
            .refresh_publisher(domain, &profile, &keys)
            .await
            .map(|_| expires_at)
            .map_err(|error| RefreshError::Profile {
                domain: domain.to_string(),
                message: error.to_string(),
            })
    }
}

#[cfg(test)]
mod maintenance_tests;

/// The verified `publisher` member of `domain`'s authority descriptor: its
/// own signature, domain binding and validity period; pinning against the
/// accepted baseline happens when the registry accepts it.
fn verify_descriptor(
    domain: &str,
    document: &[u8],
) -> Result<(PublisherProfile, PublisherKeys), RefreshError> {
    let invalid = |message: String| RefreshError::Profile {
        domain: domain.to_string(),
        message,
    };
    let profile =
        crate::realm_management::publisher_from_descriptor(domain, document).map_err(invalid)?;
    let keys = profile
        .verify(domain, chrono::Utc::now())
        .map_err(|error| invalid(error.to_string()))?;
    Ok((profile, keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::realm_profile::sign_document;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    fn key_member(id: &str, key: &SigningKey, status: &str) -> serde_json::Value {
        json!({
            "id": id,
            "algorithm": "Ed25519",
            "status": status,
            "public_key": BASE64.encode(key.verifying_key().to_bytes()),
        })
    }

    fn descriptor(
        domain: &str,
        root: &SigningKey,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Vec<u8> {
        let package = SigningKey::from_bytes(&[9; 32]);
        let now = chrono::Utc::now();
        let mut profile = json!({
            "schema": "actr.publisher/v1",
            "sequence": 1,
            "domain": domain,
            "profile_signing_keys": [key_member("root-1", root, "active")],
            "keys": [key_member(
                &actr_protocol::package_key_id(package.verifying_key().as_bytes()),
                &package,
                "active",
            )],
            "profile_issued_at": (now - chrono::Duration::hours(1)).to_rfc3339(),
            "profile_expires_at": expires_at.to_rfc3339(),
        });
        sign_document(&mut profile, "root-1", root);
        serde_json::to_vec(&json!({
            "version": 1,
            "authority": format!("actr://{domain}"),
            "services": [{ "rel": "contracts", "href": format!("https://{domain}/contracts") }],
            "publisher": profile,
        }))
        .unwrap()
    }

    #[test]
    fn accepts_a_well_formed_descriptor() {
        let root = SigningKey::from_bytes(&[3; 32]);
        let document = descriptor(
            "acme.com",
            &root,
            chrono::Utc::now() + chrono::Duration::days(1),
        );
        let (profile, keys) = verify_descriptor("acme.com", &document).unwrap();
        assert_eq!(profile.domain, "acme.com");
        assert_eq!(keys.sequence, 1);
        assert_eq!(keys.keys.len(), 1);
    }

    #[test]
    fn refuses_another_domains_profile() {
        let root = SigningKey::from_bytes(&[4; 32]);
        let document = descriptor(
            "acme.com",
            &root,
            chrono::Utc::now() + chrono::Duration::days(1),
        );
        let error = verify_descriptor("other.example", &document).unwrap_err();
        assert!(matches!(error, RefreshError::Profile { .. }), "{error}");
    }

    #[test]
    fn refuses_a_tampered_descriptor() {
        let root = SigningKey::from_bytes(&[5; 32]);
        let document = descriptor(
            "acme.com",
            &root,
            chrono::Utc::now() + chrono::Duration::days(1),
        );
        let mut value: serde_json::Value = serde_json::from_slice(&document).unwrap();
        value["publisher"]["keys"][0]["status"] = json!("retired");
        let error =
            verify_descriptor("acme.com", &serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(matches!(error, RefreshError::Profile { .. }), "{error}");
    }

    #[test]
    fn refuses_an_expired_profile() {
        let root = SigningKey::from_bytes(&[6; 32]);
        let document = descriptor(
            "acme.com",
            &root,
            chrono::Utc::now() - chrono::Duration::hours(1),
        );
        let error = verify_descriptor("acme.com", &document).unwrap_err();
        assert!(matches!(error, RefreshError::Profile { .. }), "{error}");
    }
}
