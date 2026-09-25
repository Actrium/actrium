use crate::{
    PublisherRegistryError, crypto,
    model::{
        ActrPackage, PkgStatus, PublishNonce, Publisher, PublisherKeyHistory, PublisherStatus,
        key_history::KeyHistoryStatus,
    },
    reserved::canonical_publisher_domain,
};
use actr_protocol::realm_profile::jcs;
use actr_protocol::{KeyStatus, PublisherKeys, PublisherProfile};
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
}

impl PublisherRegistryManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            nonce_retain_secs: 86400, // 24 hours
        }
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

        let key_expires_at = publisher_record
            .key_expires_at
            .ok_or(PublisherRegistryError::CertificateExpired)?;
        if chrono::Utc::now().timestamp() >= key_expires_at {
            return Err(PublisherRegistryError::CertificateExpired);
        }

        Ok(())
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
    /// The first accepted profile pins its `profile_signing_keys`. A later
    /// profile must be signed by a pinned `active` key, list only pinned keys,
    /// never make a retired key active again, and never lower `sequence`; the
    /// same `sequence` must be the same profile. Rotation chains are not
    /// followed yet. The profile's one `active` package signing key becomes
    /// the publisher's current key; a replaced key is archived as retired and
    /// a key used before is never reinstated.
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
        let mut active = keys
            .keys
            .iter()
            .filter(|key| key.status == KeyStatus::Active);
        let (Some(package_key), None) = (active.next(), active.next()) else {
            return Err(invalid(
                "a Publisher Profile must list exactly one active package signing key".into(),
            ));
        };
        crypto::validate_public_key(&package_key.public_key)?;
        let key_id = crypto::compute_key_id_from_b64(&package_key.public_key)?;
        let now = chrono::Utc::now().timestamp();
        let key_expires_at = match &package_key.not_after {
            Some(not_after) => chrono::DateTime::parse_from_rfc3339(not_after)
                .map_err(|error| invalid(format!("not_after: {error}")))?
                .timestamp(),
            None => now + DEFAULT_KEY_LIFETIME_SECS,
        };
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
        let Some(record) = existing else {
            sqlx::query(
                "INSERT INTO publisher
                    (name, public_key, key_id, profile_json, contact, status,
                     created_at, updated_at, verified_at, key_expires_at)
                 VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?)",
            )
            .bind(publisher)
            .bind(&package_key.public_key)
            .bind(&key_id)
            .bind(&profile_json)
            .bind(contact)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(key_expires_at)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            platform::recording::info!("Publisher registered: name={publisher}, key_id={key_id}");
            return Publisher::get_by_name(&self.pool, publisher)
                .await?
                .ok_or(PublisherRegistryError::NotFound);
        };

        if record.status != PublisherStatus::Active {
            return Err(PublisherRegistryError::InvalidStatus(format!(
                "Publisher '{publisher}' is {}",
                record.status
            )));
        }
        if record.profile_json == profile_json {
            return Ok(record);
        }
        check_successor(&record.profile_json, profile, keys)?;

        if record.key_id != key_id {
            // A key archived or revoked before never becomes current again.
            if PublisherKeyHistory::key_id_ever_used(&mut *tx, record.id, &key_id).await? {
                return Err(invalid(
                    "a previously used publisher key must not be reinstated".into(),
                ));
            }
            if !record.key_id.is_empty() {
                sqlx::query(
                    "INSERT INTO publisher_key_history
                     (publisher_id, key_id, public_key, status, created_at, retired_at)
                     VALUES (?, ?, ?, 'retired', ?, ?)",
                )
                .bind(record.id)
                .bind(&record.key_id)
                .bind(&record.public_key)
                .bind(record.verified_at.unwrap_or(record.created_at))
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
        let updated = sqlx::query(
            "UPDATE publisher
             SET public_key = ?, key_id = ?, key_expires_at = ?, profile_json = ?,
                 contact = ?, updated_at = ?
             WHERE id = ? AND status = 'active' AND profile_json = ?",
        )
        .bind(&package_key.public_key)
        .bind(&key_id)
        .bind(key_expires_at)
        .bind(&profile_json)
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
        let nonce_entry = match (&req.nonce, &req.nonce_sig) {
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

                let valid = crypto::verify_signature(
                    payload.as_bytes(),
                    nonce_sig_b64,
                    &publisher_record.public_key,
                )?;
                if !valid {
                    platform::recording::warn!(
                        "publish nonce signature invalid: publisher={}",
                        req.publisher
                    );
                    return Err(PublisherRegistryError::Unauthorized);
                }

                entry
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
        let valid = crypto::verify_signature(
            req.manifest.as_bytes(),
            &req.signature,
            &publisher_record.public_key,
        )?;
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
        // current publish-authority key. The database field remains
        // authoritative so legacy manifests without this field stay publishable.
        if let Some(manifest_key_id) = manifest_toml
            .get("signing_key_id")
            .and_then(|value| value.as_str())
            && manifest_key_id != publisher_record.key_id
        {
            return Err(PublisherRegistryError::InvalidRequest(format!(
                "manifest signing_key_id '{}' != current Publisher key '{}'",
                manifest_key_id, publisher_record.key_id
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
            &publisher_record.key_id,
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
    /// key (publish and resolution fail closed) and must call `renew_key` to
    /// rotate to a fresh key - the rollback blacklist prevents reinstating the
    /// revoked one. See `Publisher::revoke_active_key` for the exact post-state.
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

/// Check `candidate` against the accepted profile `accepted_json` (see
/// [`PublisherRegistryManager::register_publisher`]).
fn check_successor(
    accepted_json: &str,
    candidate: &PublisherProfile,
    keys: &PublisherKeys,
) -> Result<(), PublisherRegistryError> {
    let Some(accepted) = accepted_profile(accepted_json) else {
        return Ok(());
    };
    let rejected = |message: String| PublisherRegistryError::VerificationFailed(message);
    let accepted: PublisherKeys = serde_json::from_value(accepted)
        .map_err(|error| rejected(format!("accepted Publisher Profile: {error}")))?;
    let pinned = |id: &str, public_key: &str| {
        accepted
            .profile_signing_keys
            .iter()
            .find(|key| key.id == id && key.public_key == public_key)
    };
    let signer = keys
        .profile_signing_keys
        .iter()
        .find(|key| key.id == candidate.signature.key_id)
        .and_then(|key| pinned(&key.id, &key.public_key));
    if signer.is_none_or(|key| key.status != KeyStatus::Active) {
        return Err(rejected(
            "the Publisher Profile is not signed by a pinned active key".into(),
        ));
    }
    // The same sequence with other content was ruled out by the caller.
    if keys.sequence <= accepted.sequence {
        return Err(rejected(format!(
            "Publisher Profile sequence {} does not follow {}",
            keys.sequence, accepted.sequence
        )));
    }
    for key in &keys.profile_signing_keys {
        let Some(pinned) = pinned(&key.id, &key.public_key) else {
            return Err(rejected(format!(
                "the Publisher Profile lists new signing key '{}'; key rotation chains are not supported",
                key.id
            )));
        };
        if pinned.status == KeyStatus::Retired && key.status == KeyStatus::Active {
            return Err(rejected(format!(
                "the Publisher Profile reactivates retired signing key '{}'",
                key.id
            )));
        }
    }
    Ok(())
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
