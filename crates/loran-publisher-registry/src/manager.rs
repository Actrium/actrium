use crate::{
    PublisherRegistryError, crypto,
    model::{
        ActrPackage, PkgStatus, PublishNonce, Publisher, PublisherKeyHistory, PublisherStatus,
        key_history::KeyHistoryStatus,
    },
    reserved::canonical_publisher_domain,
};
use actr_protocol::realm_profile::jcs;
use actr_protocol::{KeyPinOutcome, KeyPins, KeyStatus, PublisherKeys, PublisherProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

/// How long a package signing key without `not_after` may publish.
const DEFAULT_KEY_LIFETIME_SECS: i64 = 365 * 24 * 3600;

#[cfg(test)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PublisherVerificationResponse {
    pub certificate: PublisherCertificate,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublisherCertificate {
    pub key_id: String,
    pub publisher: String,
    pub public_key: String,
    pub issued_at: i64,
    /// End of this key's authority to publish new packages. Natural expiry
    /// does not invalidate signatures made by this key.
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishRequest {
    pub publisher: String,
    pub name: String,
    pub version: String,
    /// Target platform (e.g. "wasm32-wasip1", "x86_64-unknown-linux-gnu")
    #[serde(default = "default_target")]
    pub target: String,
    /// Full content of actr.toml (with binary_hash field populated)
    pub manifest: String,
    /// base64 Ed25519 signature by the Publisher private key over manifest bytes
    pub signature: String,
    /// Proto files JSON for filing/audit (optional)
    #[serde(default)]
    pub proto_files: Option<serde_json::Value>,
    /// Challenge-Response nonce from POST /registry/packages/nonce.
    #[serde(default)]
    pub nonce: Option<String>,
    /// Ed25519 signature over the request authorization payload:
    /// ACTR-PUBLISH-V2 + publisher + method + path + hex(nonce) + sha256(signable_body)
    #[serde(default)]
    pub nonce_sig: Option<String>,
}

/// Request body for POST /registry/packages/nonce.
#[derive(Debug, Serialize, Deserialize)]
pub struct NonceRequest {
    pub publisher: String,
}

fn default_target() -> String {
    "wasm32-wasip1".to_string()
}

#[derive(Serialize)]
struct SignablePublishBody<'a> {
    publisher: &'a str,
    name: &'a str,
    version: &'a str,
    target: &'a str,
    manifest: &'a str,
    signature: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    proto_files: Option<&'a serde_json::Value>,
    nonce: &'a str,
}

fn signable_publish_body_bytes(
    req: &PublishRequest,
    nonce: &str,
) -> Result<Vec<u8>, PublisherRegistryError> {
    serde_json::to_vec(&SignablePublishBody {
        publisher: &req.publisher,
        name: &req.name,
        version: &req.version,
        target: &req.target,
        manifest: &req.manifest,
        signature: &req.signature,
        proto_files: req.proto_files.as_ref(),
        nonce,
    })
    .map_err(|e| {
        PublisherRegistryError::InvalidRequest(format!(
            "failed to serialize signable publish body: {e}"
        ))
    })
}

fn build_publish_auth_payload(publisher: &str, nonce_hex: &str, body_sha256_hex: &str) -> String {
    format!(
        "ACTR-PUBLISH-V2\npublisher={publisher}\nmethod=POST\npath=/registry/packages/publish\nnonce={nonce_hex}\nbody_sha256={body_sha256_hex}"
    )
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublisherPublicInfo {
    pub id: i64,
    pub key_id: String,
    pub name: String,
    pub public_key: String,
    pub certificate: PublisherCertificate,
    /// The accepted Publisher Profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Value>,
}

pub struct PublisherRegistryManager {
    pool: SqlitePool,
    /// How long (seconds) to retain expired/used nonce records for auditing.
    /// Default: 86400 (24 hours).
    nonce_retain_secs: i64,
    /// Most publishers `register_publisher` registers; `None`: no limit.
    max_publishers: Option<u32>,
}

impl PublisherRegistryManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            nonce_retain_secs: 86400, // 24 hours
            max_publishers: None,
        }
    }

    /// Refuse to register a new publisher once `max` are registered.
    pub fn with_max_publishers(mut self, max: u32) -> Self {
        self.max_publishers = Some(max);
        self
    }

    /// Set nonce retention duration in seconds (for cleanup of expired/used nonces).
    pub fn with_nonce_retain_secs(mut self, secs: i64) -> Self {
        self.nonce_retain_secs = secs;
        self
    }

    /// Access the underlying database pool (used by tests for nonce setup).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn validate_publish_authority(
        publisher_record: &Publisher,
    ) -> Result<(), PublisherRegistryError> {
        if publisher_record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                publisher_record.name
            )));
        }

        if publisher_record
            .publishing_keys(chrono::Utc::now().timestamp())
            .is_empty()
        {
            return Err(PublisherRegistryError::CertificateExpired);
        }

        Ok(())
    }

    /// The `(key_id, public_key)` pairs of `name`'s active package signing
    /// keys: the current one and, while it rotates, the second one. Natural
    /// expiry ends their authority to publish, not to prove possession.
    pub async fn active_keys(
        &self,
        name: &str,
    ) -> Result<Vec<(String, String)>, PublisherRegistryError> {
        let record = Publisher::get_by_name(&self.pool, name)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        if record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{name}' is not active"
            )));
        }
        let mut keys = Vec::new();
        if !record.key_id.is_empty() {
            keys.push((record.key_id, record.public_key));
        }
        keys.extend(record.next_key_id.zip(record.next_public_key));
        if keys.is_empty() {
            return Err(PublisherRegistryError::CertificateExpired);
        }
        Ok(keys)
    }

    // ========== Publish Nonce Challenge-Response ==========

    /// Request a one-time nonce for publish authentication.
    ///
    /// Also performs lazy cleanup of old nonce records.
    pub async fn request_nonce(&self, publisher: &str) -> Result<Vec<u8>, PublisherRegistryError> {
        let publisher_record = Publisher::get_by_name(&self.pool, publisher)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        Self::validate_publish_authority(&publisher_record)?;

        // Lazy cleanup of old nonces
        let cleaned = PublishNonce::cleanup(&self.pool, self.nonce_retain_secs).await?;
        if cleaned > 0 {
            platform::recording::debug!("cleaned up {} expired publish nonces", cleaned);
        }

        // Create new nonce
        let nonce = PublishNonce::create(&self.pool, publisher_record.id).await?;
        platform::recording::info!(
            "publish nonce issued for publisher '{}' (publisher_id={})",
            publisher,
            publisher_record.id
        );
        Ok(nonce)
    }

    /// Register `publisher` from `profile`, its Publisher Profile verified
    /// by [`PublisherProfile::verify`] with `keys`, or accept a later profile
    /// of a registered publisher (`register_publisher`,
    /// `actr/protocols/authority-discovery.md` section 4.5).
    ///
    /// The profile's `profile_signing_keys` go through the acceptance
    /// procedure of section 4.4 against the pins of the accepted profile:
    /// the first profile pins them, later ones may retire, remove or, along
    /// a rotation chain, introduce keys; an identity change is refused.
    ///
    /// The profile lists one or two `active` package signing keys, each
    /// within its `not_before`/`not_after`; all of them publish. The current
    /// key stays current while it is active; a key that is no longer active
    /// is archived as retired; a key the profile no longer lists is revoked,
    /// and a key used before is never reinstated.
    pub async fn register_publisher(
        &self,
        publisher: &str,
        profile: &PublisherProfile,
        keys: &PublisherKeys,
    ) -> Result<Publisher, PublisherRegistryError> {
        let invalid = PublisherRegistryError::InvalidRequest;
        if canonical_publisher_domain(publisher)? != publisher || profile.domain != publisher {
            return Err(invalid(format!(
                "'{publisher}' is not the normalized domain of this Publisher Profile"
            )));
        }
        let now = chrono::Utc::now().timestamp();
        let time = |value: &Option<String>, member: &str| {
            value
                .as_deref()
                .map(|value| {
                    chrono::DateTime::parse_from_rfc3339(value)
                        .map(|time| time.timestamp())
                        .map_err(|error| invalid(format!("{member}: {error}")))
                })
                .transpose()
        };
        // (key_id, public_key, expires_at) of each active package signing key.
        let mut active = Vec::new();
        for key in keys
            .keys
            .iter()
            .filter(|key| key.status == KeyStatus::Active)
        {
            crypto::validate_public_key(&key.public_key)
                .map_err(|error| invalid(format!("package signing key: {error}")))?;
            let key_id = crypto::compute_key_id_from_b64(&key.public_key)?;
            let not_before = time(&key.not_before, "not_before")?;
            let expires_at =
                time(&key.not_after, "not_after")?.unwrap_or(now + DEFAULT_KEY_LIFETIME_SECS);
            if not_before.is_some_and(|not_before| now < not_before) || now >= expires_at {
                return Err(invalid(format!(
                    "package signing key '{}' is outside its validity period",
                    key.id
                )));
            }
            active.push((key_id, key.public_key.clone(), expires_at));
        }
        if active.is_empty() {
            return Err(invalid(
                "a Publisher Profile must list an active package signing key".into(),
            ));
        }
        let document = serde_json::to_value(profile)
            .map_err(|error| invalid(format!("Publisher Profile: {error}")))?;
        let profile_json = jcs(&document);
        let contact = document["security_contact"].as_str();

        // IMMEDIATE takes the write lock before the read, so concurrent
        // registrations of one publisher run one after another instead of
        // failing on a stale WAL snapshot.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing = sqlx::query_as::<_, Publisher>("SELECT * FROM publisher WHERE name = ?")
            .bind(publisher)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(record) = &existing {
            if record.status != PublisherStatus::Active {
                return Err(PublisherRegistryError::InvalidStatus(format!(
                    "Publisher '{publisher}' is {}",
                    record.status
                )));
            }
            if record.profile_json == profile_json {
                return Ok(record.clone());
            }
        }
        let pinned = existing
            .as_ref()
            .and_then(|record| record.profile_pins.as_deref())
            .map(serde_json::from_str::<KeyPins>)
            .transpose()
            .map_err(|error| invalid(format!("stored key pins: {error}")))?;
        let rejected = |message: String| PublisherRegistryError::VerificationFailed(message);
        let pins = match profile
            .accept(publisher, chrono::Utc::now(), pinned.as_ref())
            .map_err(|error| rejected(error.to_string()))?
        {
            (_, KeyPinOutcome::Accepted(pins)) => pins,
            (_, KeyPinOutcome::IdentityChange(_)) => {
                return Err(rejected(
                    "the Publisher Profile is not signed by a key chained to a pinned key".into(),
                ));
            }
        };
        let pins_json =
            serde_json::to_string(&pins).map_err(|error| invalid(format!("key pins: {error}")))?;

        // The current key stays current while it is active.
        let held = existing.as_ref().map_or(Vec::new(), |record| {
            let mut held = vec![record.key_id.clone()];
            held.extend(record.next_key_id.clone());
            held
        });
        if let Some(position) = active.iter().position(|(id, ..)| held.first() == Some(id)) {
            active.swap(0, position);
        } else if let Some(position) = active.iter().position(|(id, ..)| held.contains(id)) {
            active.swap(0, position);
        }
        let (key_id, public_key, key_expires_at) = active[0].clone();
        let next = active.get(1).cloned();

        let Some(record) = existing else {
            if let Some(max) = self.max_publishers {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM publisher")
                    .fetch_one(&mut *tx)
                    .await?;
                if count >= i64::from(max) {
                    return Err(PublisherRegistryError::QuotaExceeded(format!(
                        "no more publishers: max_publishers = {max}"
                    )));
                }
            }
            sqlx::query(
                "INSERT INTO publisher
                    (name, public_key, key_id, profile_json, contact, status,
                     created_at, updated_at, verified_at, key_expires_at,
                     next_public_key, next_key_id, next_key_expires_at, profile_pins)
                 VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(publisher)
            .bind(&public_key)
            .bind(&key_id)
            .bind(&profile_json)
            .bind(contact)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(key_expires_at)
            .bind(next.as_ref().map(|(_, key, _)| key))
            .bind(next.as_ref().map(|(id, ..)| id))
            .bind(next.as_ref().map(|(.., expires)| expires))
            .bind(&pins_json)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            platform::recording::info!("Publisher registered: name={publisher}, key_id={key_id}");
            return Publisher::get_by_name(&self.pool, publisher)
                .await?
                .ok_or(PublisherRegistryError::NotFound);
        };

        // A key archived or revoked before never becomes active again.
        for (id, ..) in &active {
            if !held.contains(id)
                && PublisherKeyHistory::key_id_ever_used(&mut *tx, record.id, id).await?
            {
                return Err(invalid(
                    "a previously used publisher key must not be reinstated".into(),
                ));
            }
        }
        // Keys that stop being active: retired while the profile still lists
        // them, revoked once it does not.
        let leaving = std::iter::once((&record.key_id, &record.public_key)).chain(
            record
                .next_key_id
                .as_ref()
                .zip(record.next_public_key.as_ref()),
        );
        for (old_id, old_key) in leaving {
            if old_id.is_empty() || active.iter().any(|(id, ..)| id == old_id) {
                continue;
            }
            let listed = keys.keys.iter().any(|key| &key.public_key == old_key);
            sqlx::query(
                "INSERT INTO publisher_key_history
                 (publisher_id, key_id, public_key, status, created_at, retired_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(record.id)
            .bind(old_id)
            .bind(old_key)
            .bind(if listed { "retired" } else { "revoked" })
            .bind(record.verified_at.unwrap_or(record.created_at))
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        // A retired key the profile no longer lists is revoked.
        for history in sqlx::query_as::<_, PublisherKeyHistory>(
            "SELECT * FROM publisher_key_history WHERE publisher_id = ? AND status = 'retired'",
        )
        .bind(record.id)
        .fetch_all(&mut *tx)
        .await?
        {
            if !keys
                .keys
                .iter()
                .any(|key| key.public_key == history.public_key)
            {
                sqlx::query("UPDATE publisher_key_history SET status = 'revoked' WHERE id = ?")
                    .bind(history.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let updated = sqlx::query(
            "UPDATE publisher
             SET public_key = ?, key_id = ?, key_expires_at = ?, next_public_key = ?,
                 next_key_id = ?, next_key_expires_at = ?, profile_json = ?,
                 profile_pins = ?, contact = ?, updated_at = ?
             WHERE id = ? AND status = 'active' AND profile_json = ?",
        )
        .bind(&public_key)
        .bind(&key_id)
        .bind(key_expires_at)
        .bind(next.as_ref().map(|(_, key, _)| key))
        .bind(next.as_ref().map(|(id, ..)| id))
        .bind(next.as_ref().map(|(.., expires)| expires))
        .bind(&profile_json)
        .bind(&pins_json)
        .bind(contact)
        .bind(now)
        .bind(record.id)
        .bind(&record.profile_json)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(PublisherRegistryError::InvalidStatus(
                "publisher changed while its profile was registered".to_string(),
            ));
        }
        tx.commit().await?;
        platform::recording::info!(
            "Publisher Profile accepted: name={publisher}, sequence={}, key_id={key_id}",
            keys.sequence
        );
        Publisher::get(&self.pool, record.id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)
    }

    /// Test-only shortcut for provisioning publisher records without external
    /// HTTPS discovery.
    ///
    /// If no public key is supplied, the helper creates a disposable test key
    /// and immediately discards its private half. Production code cannot call
    /// this path.
    #[cfg(test)]
    pub async fn admin_approve(
        &self,
        publisher_id: i64,
        user_public_key: Option<&str>,
    ) -> Result<PublisherVerificationResponse, PublisherRegistryError> {
        let mut publisher_record = Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;

        let generated_public_key;
        let public_key = match user_public_key {
            Some(public_key) => public_key,
            None => {
                generated_public_key = crypto::generate_keypair().1;
                &generated_public_key
            }
        };
        let response = self
            .activate_with_key(&mut publisher_record, public_key)
            .await?;
        platform::recording::info!(
            "test publisher approved: publisher_id={}, name={}, key_id={}",
            publisher_id,
            publisher_record.name,
            response.certificate.key_id
        );
        Ok(response)
    }

    /// Bind a publisher-supplied public key after ownership verification.
    #[cfg(test)]
    async fn activate_with_key(
        &self,
        publisher_record: &mut Publisher,
        public_key: &str,
    ) -> Result<PublisherVerificationResponse, PublisherRegistryError> {
        crypto::validate_public_key(public_key)?;

        publisher_record
            .activate(&self.pool, public_key.to_string())
            .await?;

        let expires_at = publisher_record
            .key_expires_at
            .ok_or(PublisherRegistryError::CertificateExpired)?;
        Ok(PublisherVerificationResponse {
            certificate: PublisherCertificate {
                key_id: publisher_record.key_id.clone(),
                publisher: publisher_record.name.clone(),
                public_key: publisher_record.public_key.clone(),
                issued_at: publisher_record
                    .verified_at
                    .unwrap_or(publisher_record.created_at),
                expires_at,
            },
        })
    }

    pub async fn get_status(&self, publisher_id: i64) -> Result<Publisher, PublisherRegistryError> {
        Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)
    }

    pub async fn resolve_by_name(
        &self,
        name: &str,
    ) -> Result<PublisherPublicInfo, PublisherRegistryError> {
        let publisher_record = Publisher::get_by_name(&self.pool, name)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        if publisher_record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                name
            )));
        }
        let expires_at = publisher_record
            .key_expires_at
            .ok_or(PublisherRegistryError::CertificateExpired)?;
        let cert = PublisherCertificate {
            key_id: publisher_record.key_id.clone(),
            publisher: publisher_record.name.clone(),
            public_key: publisher_record.public_key.clone(),
            issued_at: publisher_record
                .verified_at
                .unwrap_or(publisher_record.created_at),
            expires_at,
        };
        let profile = accepted_profile(&publisher_record.profile_json);
        Ok(PublisherPublicInfo {
            id: publisher_record.id,
            key_id: publisher_record.key_id.clone(),
            name: publisher_record.name,
            public_key: publisher_record.public_key,
            certificate: cert,
            profile,
        })
    }

    /// Resolve a specific historical (or active) key by exact key_id.
    pub async fn resolve_key_by_id(
        &self,
        name: &str,
        key_id: &str,
    ) -> Result<PublisherPublicInfo, PublisherRegistryError> {
        let publisher_record = Publisher::get_by_name(&self.pool, name)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        if publisher_record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                name
            )));
        }

        // Fail closed even on the current-key fast path: if this exact key_id
        // carries a revoked history row, refuse to serve it before the
        // short-circuit below could hand back the live public key.
        if PublisherKeyHistory::is_key_id_revoked(&self.pool, publisher_record.id, key_id).await? {
            platform::recording::warn!(
                "rejected resolution of revoked key on current-key path: \
                 publisher_id={}, key_id={}",
                publisher_record.id,
                key_id
            );
            return Err(PublisherRegistryError::KeyRevoked(key_id.to_string()));
        }

        // Is it the current key?
        if publisher_record.key_id == key_id {
            return self.resolve_by_name(name).await;
        }
        // The second active key while the publisher rotates.
        if publisher_record.next_key_id.as_deref() == Some(key_id)
            && let (Some(public_key), Some(expires_at)) = (
                publisher_record.next_public_key.clone(),
                publisher_record.next_key_expires_at,
            )
        {
            let profile = accepted_profile(&publisher_record.profile_json);
            return Ok(PublisherPublicInfo {
                id: publisher_record.id,
                key_id: key_id.to_string(),
                certificate: PublisherCertificate {
                    key_id: key_id.to_string(),
                    publisher: publisher_record.name.clone(),
                    public_key: public_key.clone(),
                    issued_at: publisher_record
                        .updated_at
                        .unwrap_or(publisher_record.created_at),
                    expires_at,
                },
                name: publisher_record.name,
                public_key,
                profile,
            });
        }

        let history = PublisherKeyHistory::get_by_key_id(&self.pool, publisher_record.id, key_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;

        let cert = PublisherCertificate {
            key_id: history.key_id.clone(),
            publisher: publisher_record.name.clone(),
            public_key: history.public_key.clone(),
            issued_at: history.created_at,
            expires_at: history.retired_at, // Old keys technically "expired" at retirement time
        };
        let profile = accepted_profile(&publisher_record.profile_json);

        Ok(PublisherPublicInfo {
            id: publisher_record.id,
            key_id: history.key_id.clone(),
            name: publisher_record.name,
            public_key: history.public_key,
            certificate: cert,
            profile,
        })
    }

    /// Verify the Publisher key that authenticated a published package.
    ///
    /// Natural expiration and normal retirement do not invalidate an existing
    /// package. Explicitly revoked keys do. New packages store signing_key_id
    /// directly; legacy rows fall back to the signed manifest or, if needed,
    /// cryptographic identification across the Publisher Registry's non-revoked keys.
    pub async fn verify_published_package_signing_key(
        &self,
        package: &ActrPackage,
    ) -> Result<String, PublisherRegistryError> {
        let publisher_record = Publisher::get(&self.pool, package.publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        if publisher_record.name != package.publisher {
            return Err(PublisherRegistryError::InvalidRequest(
                "package publisher does not match its registry record".to_string(),
            ));
        }
        if publisher_record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                publisher_record.name
            )));
        }

        let manifest_key_id = package
            .manifest
            .parse::<toml::Value>()
            .ok()
            .and_then(|manifest| {
                manifest
                    .get("signing_key_id")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            });

        if let (Some(stored), Some(manifest)) = (
            package.signing_key_id.as_deref(),
            manifest_key_id.as_deref(),
        ) && stored != manifest
        {
            return Err(PublisherRegistryError::InvalidSignature);
        }

        if let Some(key_id) = package
            .signing_key_id
            .as_deref()
            .or(manifest_key_id.as_deref())
        {
            let key = self
                .resolve_key_by_id(&publisher_record.name, key_id)
                .await?;
            let valid = crypto::verify_signature(
                package.manifest.as_bytes(),
                &package.signature,
                &key.public_key,
            )?;
            return if valid {
                Ok(key.key_id)
            } else {
                Err(PublisherRegistryError::InvalidSignature)
            };
        }

        // Legacy package: identify the signing key by verifying the stored
        // signature against the current key and all non-revoked historical keys.
        if !publisher_record.key_id.is_empty() && !publisher_record.public_key.is_empty() {
            let valid = crypto::verify_signature(
                package.manifest.as_bytes(),
                &package.signature,
                &publisher_record.public_key,
            )?;
            if valid {
                return Ok(publisher_record.key_id);
            }
        }

        for history in
            PublisherKeyHistory::list_by_publisher(&self.pool, publisher_record.id).await?
        {
            if history.status == KeyHistoryStatus::Revoked {
                continue;
            }
            let valid = crypto::verify_signature(
                package.manifest.as_bytes(),
                &package.signature,
                &history.public_key,
            )?;
            if valid {
                return Ok(history.key_id);
            }
        }

        Err(PublisherRegistryError::InvalidSignature)
    }

    pub async fn publish_package(
        &self,
        req: PublishRequest,
    ) -> Result<ActrPackage, PublisherRegistryError> {
        let publisher_record = Publisher::get_by_name(&self.pool, &req.publisher)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        Self::validate_publish_authority(&publisher_record)?;

        // --- Challenge-Response nonce verification ---
        // Flow: find_pending → verify nonce_sig over the full signable request body
        //       → verify manifest_sig → atomic consume.
        let (nonce_entry, (signing_key_id, signing_public_key)) = match (&req.nonce, &req.nonce_sig)
        {
            (Some(nonce_b64), Some(nonce_sig_b64)) => {
                use base64::Engine as _;
                use sha2::{Digest, Sha256};

                // Decode nonce
                let nonce_bytes = base64::engine::general_purpose::STANDARD
                    .decode(nonce_b64)
                    .map_err(|_| {
                        PublisherRegistryError::InvalidRequest("invalid nonce base64".to_string())
                    })?;

                // Step 1: Read-only lookup — does NOT consume the nonce
                let entry = PublishNonce::find_pending(&self.pool, &nonce_bytes).await?;

                // Ensure nonce was issued for this publisher
                if entry.publisher_id != publisher_record.id {
                    platform::recording::warn!(
                        "publish nonce publisher_id mismatch: nonce={}, request={}",
                        entry.publisher_id,
                        publisher_record.id
                    );
                    return Err(PublisherRegistryError::Unauthorized);
                }

                // Step 2: Verify nonce signature (before consuming).
                // The signature authorizes this exact publish request body (excluding nonce_sig).
                let body_bytes = signable_publish_body_bytes(&req, nonce_b64)?;
                let body_sha256 = hex::encode(Sha256::digest(&body_bytes));
                let nonce_hex = hex::encode(&nonce_bytes);
                let payload = build_publish_auth_payload(&req.publisher, &nonce_hex, &body_sha256);

                // Any key that may publish; it must also sign the manifest.
                let mut signer = None;
                for (key_id, public_key) in
                    publisher_record.publishing_keys(chrono::Utc::now().timestamp())
                {
                    if crypto::verify_signature(payload.as_bytes(), nonce_sig_b64, public_key)? {
                        signer = Some((key_id.to_string(), public_key.to_string()));
                        break;
                    }
                }
                let Some(signer) = signer else {
                    platform::recording::warn!(
                        "publish nonce signature invalid: publisher={}",
                        req.publisher
                    );
                    return Err(PublisherRegistryError::Unauthorized);
                };

                (entry, signer)
            }
            _ => {
                platform::recording::warn!(
                    "publish request missing nonce/nonce_sig: publisher={}",
                    req.publisher
                );
                return Err(PublisherRegistryError::Unauthorized);
            }
        };

        // Step 3: Verify manifest signature
        let valid =
            crypto::verify_signature(req.manifest.as_bytes(), &req.signature, &signing_public_key)?;
        if !valid {
            return Err(PublisherRegistryError::InvalidSignature);
        }

        // Step 4: Atomically consume nonce (only after both signatures verified)
        PublishNonce::consume(&self.pool, nonce_entry.id).await?;
        platform::recording::info!(
            "publish nonce verified and consumed: publisher={}",
            req.publisher
        );

        // Step 5: Verify outer fields match manifest content (parse as TOML)
        let manifest_toml: toml::Value = toml::from_str(&req.manifest).map_err(|e| {
            PublisherRegistryError::InvalidRequest(format!("manifest is not valid TOML: {}", e))
        })?;

        // Check Publisher
        let m = manifest_toml
            .get("publisher")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PublisherRegistryError::InvalidRequest(
                    "manifest missing required field 'publisher'".into(),
                )
            })?;
        if m != req.publisher {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest publisher '{}' != request '{}'",
                m, req.publisher
            )));
        }

        // Check name
        let n = manifest_toml
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PublisherRegistryError::InvalidRequest(
                    "manifest missing required field 'name'".into(),
                )
            })?;
        if n != req.name {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest name '{}' != request '{}'",
                n, req.name
            )));
        }

        // Check version
        let v = manifest_toml
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PublisherRegistryError::InvalidRequest(
                    "manifest missing required field 'version'".into(),
                )
            })?;
        if v != req.version {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest version '{}' != request '{}'",
                v, req.version
            )));
        }

        // If a modern manifest declares its signing key, it must identify the
        // key that signed this request. The database field remains
        // authoritative so legacy manifests without this field stay publishable.
        if let Some(manifest_key_id) = manifest_toml
            .get("signing_key_id")
            .and_then(|value| value.as_str())
            && manifest_key_id != signing_key_id
        {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest signing_key_id '{}' != signing Publisher key '{}'",
                manifest_key_id, signing_key_id
            )));
        }

        // Check target (nested under [binary])
        let t = manifest_toml
            .get("binary")
            .and_then(|b| b.get("target"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PublisherRegistryError::InvalidRequest(
                    "manifest missing required field 'binary.target'".into(),
                )
            })?;
        if t != req.target {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest binary.target '{}' != request '{}'",
                t, req.target
            )));
        }

        // Serialize proto_files JSON to string for storage
        let proto_files_str = req.proto_files.as_ref().map(|v| v.to_string());

        let pkg = ActrPackage::publish(
            &self.pool,
            publisher_record.id,
            &req.publisher,
            &req.name,
            &req.version,
            &req.target,
            &req.manifest,
            &req.signature,
            &signing_key_id,
            proto_files_str.as_deref(),
        )
        .await?;

        if req.proto_files.is_some() {
            platform::recording::info!(
                "actor package published with proto filing: type_str={}, publisher_id={}",
                pkg.type_str,
                publisher_record.id
            );
        } else {
            platform::recording::info!(
                "actor package published: type_str={}, publisher_id={}",
                pkg.type_str,
                publisher_record.id
            );
        }
        Ok(pkg)
    }

    pub async fn get_package(&self, type_str: &str) -> Result<ActrPackage, PublisherRegistryError> {
        ActrPackage::get_by_type(&self.pool, type_str)
            .await?
            .ok_or(PublisherRegistryError::NotFound)
    }

    pub async fn list_packages(
        &self,
        publisher: Option<&str>,
    ) -> Result<Vec<ActrPackage>, PublisherRegistryError> {
        if let Some(name) = publisher {
            let publisher_record = Publisher::get_by_name(&self.pool, name)
                .await?
                .ok_or(PublisherRegistryError::NotFound)?;
            ActrPackage::list_by_publisher(&self.pool, publisher_record.id).await
        } else {
            Ok(sqlx::query_as::<_, ActrPackage>(
                "SELECT * FROM publisher_package ORDER BY published_at DESC",
            )
            .fetch_all(&self.pool)
            .await?)
        }
    }

    pub async fn revoke_package(&self, pkg_id: i64) -> Result<(), PublisherRegistryError> {
        let mut pkg = ActrPackage::get_by_id(&self.pool, pkg_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        pkg.revoke(&self.pool).await?;
        platform::recording::warn!(
            "actor package revoked: pkg_id={}, type_str={}",
            pkg_id,
            pkg.type_str
        );
        Ok(())
    }

    // Admin methods
    pub async fn admin_list(
        &self,
        status: Option<PublisherStatus>,
    ) -> Result<Vec<Publisher>, PublisherRegistryError> {
        Publisher::list(&self.pool, status).await
    }

    pub async fn admin_suspend(&self, publisher_id: i64) -> Result<(), PublisherRegistryError> {
        let mut publisher_record = Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        publisher_record.suspend(&self.pool).await?;
        platform::recording::warn!(
            "Publisher suspended by admin: publisher_id={}, name={}",
            publisher_id,
            publisher_record.name
        );
        Ok(())
    }

    pub async fn admin_reinstate(&self, publisher_id: i64) -> Result<(), PublisherRegistryError> {
        let mut publisher_record = Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        publisher_record.reinstate(&self.pool).await?;
        platform::recording::info!(
            "Publisher reinstated by admin: publisher_id={}, name={}",
            publisher_id,
            publisher_record.name
        );
        Ok(())
    }

    pub async fn admin_list_keys(
        &self,
        publisher_id: i64,
    ) -> Result<Vec<crate::model::key_history::PublisherKeyHistory>, PublisherRegistryError> {
        // Ensure the Publisher exists.
        let _ = Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        crate::model::key_history::PublisherKeyHistory::list_by_publisher(&self.pool, publisher_id)
            .await
    }

    pub async fn admin_revoke_historical_key(
        &self,
        history_id: i64,
    ) -> Result<(), PublisherRegistryError> {
        crate::model::key_history::PublisherKeyHistory::revoke(&self.pool, history_id).await?;
        platform::recording::warn!(
            "Publisher historical key revoked: history_id={}",
            history_id
        );
        Ok(())
    }

    /// Emergency-revoke a publisher's CURRENTLY ACTIVE signing key.
    ///
    /// `admin_revoke_historical_key` only covers keys already rotated away; this
    /// handles a compromised live key. Afterwards the publisher holds no usable
    /// key (publish and resolution fail closed) and must register a later
    /// Publisher Profile with a fresh key - the rollback blacklist prevents
    /// reinstating the revoked one. See `Publisher::revoke_active_key` for the exact post-state.
    pub async fn admin_revoke_active_key(
        &self,
        publisher_id: i64,
    ) -> Result<(), PublisherRegistryError> {
        let mut publisher_record = Publisher::get(&self.pool, publisher_id)
            .await?
            .ok_or(PublisherRegistryError::NotFound)?;
        publisher_record.revoke_active_key(&self.pool).await?;
        platform::recording::warn!(
            "Publisher active signing key revoked by admin: publisher_id={}, name={}",
            publisher_id,
            publisher_record.name
        );
        Ok(())
    }

    pub async fn admin_delete(&self, publisher_id: i64) -> Result<(), PublisherRegistryError> {
        Publisher::delete(&self.pool, publisher_id).await?;
        platform::recording::warn!("Publisher deleted by admin: publisher_id={}", publisher_id);
        Ok(())
    }
}

/// The stored Publisher Profile, if one was accepted.
fn accepted_profile(profile_json: &str) -> Option<Value> {
    serde_json::from_str::<Value>(profile_json)
        .ok()
        .filter(|profile| profile.get("signature").is_some())
}

/// Public API for Issuer integration: check if a type_str is a valid, active package.
/// Reserved names always return true.
///
/// When `target` is provided, lookup is narrowed to that specific platform.
/// When `manifest_hash` is provided, the stored manifest is SHA-256 compared for content integrity.
pub async fn lookup_package(
    pool: &SqlitePool,
    type_str: &str,
    target: Option<&str>,
    manifest_hash: Option<&[u8]>,
) -> Result<bool, PublisherRegistryError> {
    let pkg = if let Some(t) = target {
        ActrPackage::get_by_type_and_target(pool, type_str, t).await?
    } else {
        ActrPackage::get_by_type(pool, type_str).await?
    };

    match pkg {
        Some(p) if p.status == PkgStatus::Active => {
            // C1: manifest hash comparison (defense-in-depth)
            if let Some(expected_hash) = manifest_hash {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(p.manifest.as_bytes());
                let stored_hash = hasher.finalize();
                if stored_hash.as_slice() != expected_hash {
                    platform::recording::warn!(
                        "manifest hash mismatch for type_str={}, target={:?}",
                        type_str,
                        target
                    );
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
