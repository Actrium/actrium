use crate::{
    MfrError, crypto,
    model::{
        ActrPackage, Manufacturer, MfrKeyHistory, MfrStatus, PkgStatus, PublishNonce,
        PublisherChallenge, key_history::KeyHistoryStatus,
    },
    publisher::fetch_publisher_descriptor,
    reserved::canonical_publisher_domain,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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

fn signable_publish_body_bytes(req: &PublishRequest, nonce: &str) -> Result<Vec<u8>, MfrError> {
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
        MfrError::InvalidRequest(format!("failed to serialize signable publish body: {e}"))
    })
}

fn build_publish_auth_payload(
    manufacturer: &str,
    nonce_hex: &str,
    body_sha256_hex: &str,
) -> String {
    format!(
        "ACTR-PUBLISH-V2\npublisher={manufacturer}\nmethod=POST\npath=/registry/packages/publish\nnonce={nonce_hex}\nbody_sha256={body_sha256_hex}"
    )
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublisherPublicInfo {
    pub id: i64,
    pub key_id: String,
    pub name: String,
    pub public_key: String,
    pub certificate: PublisherCertificate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<crate::publisher::PublisherDescriptor>,
}

pub struct MfrManager {
    pool: SqlitePool,
    /// How long (seconds) to retain expired/used nonce records for auditing.
    /// Default: 86400 (24 hours).
    nonce_retain_secs: i64,
}

impl MfrManager {
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

    fn validate_publish_authority(mfr: &Manufacturer) -> Result<(), MfrError> {
        if mfr.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                mfr.name
            )));
        }

        let key_expires_at = mfr.key_expires_at.ok_or(MfrError::CertificateExpired)?;
        if chrono::Utc::now().timestamp() >= key_expires_at {
            return Err(MfrError::CertificateExpired);
        }

        Ok(())
    }

    // ========== Publish Nonce Challenge-Response ==========

    /// Request a one-time nonce for publish authentication.
    ///
    /// Also performs lazy cleanup of old nonce records.
    pub async fn request_nonce(&self, manufacturer: &str) -> Result<Vec<u8>, MfrError> {
        let mfr = Manufacturer::get_by_name(&self.pool, manufacturer)
            .await?
            .ok_or(MfrError::NotFound)?;
        Self::validate_publish_authority(&mfr)?;

        // Lazy cleanup of old nonces
        let cleaned = PublishNonce::cleanup(&self.pool, self.nonce_retain_secs).await?;
        if cleaned > 0 {
            platform::recording::debug!("cleaned up {} expired publish nonces", cleaned);
        }

        // Create new nonce
        let nonce = PublishNonce::create(&self.pool, mfr.id).await?;
        platform::recording::info!(
            "publish nonce issued for publisher '{}' (publisher_id={})",
            manufacturer,
            mfr.id
        );
        Ok(nonce)
    }

    /// Step 1: apply for a canonical HTTPS publisher identity.
    ///
    /// The challenge must be placed in the publisher descriptor at
    /// `https://<publisher>/.well-known/actr/publisher.json`.
    pub async fn apply(
        &self,
        publisher: &str,
        contact: Option<&str>,
    ) -> Result<(Manufacturer, PublisherChallenge), MfrError> {
        let publisher = canonical_publisher_domain(publisher)?;
        let mfr = Manufacturer::create(&self.pool, &publisher, contact).await?;
        let challenge = PublisherChallenge::create(&self.pool, mfr.id).await?;
        platform::recording::info!("Publisher application received: publisher={}", publisher);
        Ok((mfr, challenge))
    }

    /// Step 2: verify domain ownership and bind the descriptor's signing key.
    pub async fn verify_publisher_domain(
        &self,
        mfr_id: i64,
    ) -> Result<PublisherVerificationResponse, MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;

        if !matches!(mfr.status, MfrStatus::Pending | MfrStatus::Active) {
            return Err(MfrError::InvalidStatus(format!(
                "cannot verify publisher with status: {}",
                mfr.status
            )));
        }

        let challenge = PublisherChallenge::get_active(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::ChallengeNotFound)?;

        let (descriptor, descriptor_url) = fetch_publisher_descriptor(&mfr.name).await?;
        descriptor.validate_for(&mfr.name, &challenge.token)?;
        let computed_key_id = crypto::compute_key_id_from_b64(&descriptor.public_key)?;
        if descriptor.key_id != computed_key_id {
            return Err(MfrError::VerificationFailed(format!(
                "descriptor key_id '{}' does not match public key fingerprint '{}'",
                descriptor.key_id, computed_key_id
            )));
        }
        if mfr.status == MfrStatus::Active && descriptor.key_id == mfr.key_id {
            return Err(MfrError::InvalidRequest(
                "publisher rotation descriptor must present a new signing key".to_string(),
            ));
        }

        let descriptor_json = serde_json::to_string(&descriptor)
            .map_err(|error| MfrError::InvalidRequest(error.to_string()))?;
        let response = self
            .commit_verified_descriptor(
                &mut mfr,
                &descriptor.public_key,
                &descriptor_json,
                &challenge,
                &descriptor_url,
            )
            .await?;
        mfr.descriptor_json = descriptor_json;
        platform::recording::info!(
            "Publisher verified via HTTPS descriptor: publisher_id={}, name={}, key_id={}",
            mfr_id,
            mfr.name,
            response.certificate.key_id
        );
        Ok(response)
    }

    /// Atomically bind the discovered key and descriptor, archive the previous
    /// key when rotating, and consume the exact ownership challenge.
    async fn commit_verified_descriptor(
        &self,
        mfr: &mut Manufacturer,
        public_key: &str,
        descriptor_json: &str,
        challenge: &PublisherChallenge,
        descriptor_url: &str,
    ) -> Result<PublisherVerificationResponse, MfrError> {
        crypto::validate_public_key(public_key)?;
        let now = chrono::Utc::now().timestamp();
        let new_key_id = crypto::compute_key_id_from_b64(public_key)?;

        // Historical-key blacklist: a key_id that was ever archived (retired or
        // revoked) must never be reinstated as the current key. Without this an
        // attacker who briefly controlled key A could ride A -> B -> revoke(A) ->
        // B -> A back into a trusted current-key slot. The check covers retired
        // keys too, so normal forward rotation to a fresh key is unaffected while
        // any backward move to a used key is refused.
        if MfrKeyHistory::key_id_ever_used(&self.pool, mfr.id, &new_key_id).await? {
            platform::recording::warn!(
                "rejected reinstatement of previously used publisher key: \
                 publisher_id={}, key_id={}",
                mfr.id,
                new_key_id
            );
            return Err(MfrError::InvalidRequest(
                "a previously used publisher key must not be reinstated".to_string(),
            ));
        }

        let key_expires_at = now + 365 * 24 * 3600;
        let mut tx = self.pool.begin().await?;

        let updated = match mfr.status {
            MfrStatus::Pending => sqlx::query(
                "UPDATE mfr
                     SET status = 'active', public_key = ?, key_id = ?,
                         verified_at = ?, updated_at = ?, key_expires_at = ?,
                         descriptor_json = ?
                     WHERE id = ? AND status = 'pending'",
            )
            .bind(public_key)
            .bind(&new_key_id)
            .bind(now)
            .bind(now)
            .bind(key_expires_at)
            .bind(descriptor_json)
            .bind(mfr.id)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            MfrStatus::Active => {
                if !mfr.public_key.is_empty() && !mfr.key_id.is_empty() {
                    sqlx::query(
                        "INSERT INTO mfr_key_history
                         (mfr_id, key_id, public_key, status, created_at, retired_at)
                         VALUES (?, ?, ?, 'retired', ?, ?)",
                    )
                    .bind(mfr.id)
                    .bind(&mfr.key_id)
                    .bind(&mfr.public_key)
                    .bind(mfr.verified_at.unwrap_or(mfr.created_at))
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                }
                sqlx::query(
                    "UPDATE mfr
                     SET public_key = ?, key_id = ?, key_expires_at = ?,
                         updated_at = ?, descriptor_json = ?
                     WHERE id = ? AND status = 'active' AND key_id = ?",
                )
                .bind(public_key)
                .bind(&new_key_id)
                .bind(key_expires_at)
                .bind(now)
                .bind(descriptor_json)
                .bind(mfr.id)
                .bind(&mfr.key_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
            }
            _ => {
                return Err(MfrError::InvalidStatus(format!(
                    "cannot verify publisher with status: {}",
                    mfr.status
                )));
            }
        };
        if updated != 1 {
            return Err(MfrError::InvalidStatus(
                "publisher changed while domain verification was in progress".to_string(),
            ));
        }

        let challenge_updated = sqlx::query(
            "UPDATE mfr_challenge
             SET verified_at = ?, verify_url = ?
             WHERE id = ? AND mfr_id = ? AND verified_at IS NULL AND expires_at > ?",
        )
        .bind(now)
        .bind(descriptor_url)
        .bind(challenge.id)
        .bind(mfr.id)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if challenge_updated != 1 {
            return Err(MfrError::ChallengeNotFound);
        }
        tx.commit().await?;

        mfr.status = MfrStatus::Active;
        mfr.public_key = public_key.to_string();
        mfr.key_id = new_key_id.clone();
        mfr.verified_at.get_or_insert(now);
        mfr.updated_at = Some(now);
        mfr.key_expires_at = Some(key_expires_at);

        Ok(PublisherVerificationResponse {
            certificate: PublisherCertificate {
                key_id: new_key_id,
                publisher: mfr.name.clone(),
                public_key: public_key.to_string(),
                issued_at: now,
                expires_at: key_expires_at,
            },
        })
    }

    /// Start a publisher-controlled key rotation. The new key is accepted only
    /// after the active HTTPS descriptor presents this fresh challenge.
    pub async fn begin_key_rotation(&self, mfr_id: i64) -> Result<PublisherChallenge, MfrError> {
        let mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        if mfr.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "cannot rotate publisher key from status: {}",
                mfr.status
            )));
        }
        PublisherChallenge::create(&self.pool, mfr_id).await
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
        mfr_id: i64,
        user_public_key: Option<&str>,
    ) -> Result<PublisherVerificationResponse, MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;

        let generated_public_key;
        let public_key = match user_public_key {
            Some(public_key) => public_key,
            None => {
                generated_public_key = crypto::generate_keypair().1;
                &generated_public_key
            }
        };
        let response = self.activate_with_key(&mut mfr, public_key).await?;
        platform::recording::info!(
            "test publisher approved: publisher_id={}, name={}, key_id={}",
            mfr_id,
            mfr.name,
            response.certificate.key_id
        );
        Ok(response)
    }

    /// Bind a publisher-supplied public key after ownership verification.
    #[cfg(test)]
    async fn activate_with_key(
        &self,
        mfr: &mut Manufacturer,
        public_key: &str,
    ) -> Result<PublisherVerificationResponse, MfrError> {
        crypto::validate_public_key(public_key)?;

        mfr.activate(&self.pool, public_key.to_string()).await?;

        let expires_at = mfr.key_expires_at.ok_or(MfrError::CertificateExpired)?;
        Ok(PublisherVerificationResponse {
            certificate: PublisherCertificate {
                key_id: mfr.key_id.clone(),
                publisher: mfr.name.clone(),
                public_key: mfr.public_key.clone(),
                issued_at: mfr.verified_at.unwrap_or(mfr.created_at),
                expires_at,
            },
        })
    }

    /// Get an active challenge for initial verification or key rotation.
    pub async fn get_challenge(&self, mfr_id: i64) -> Result<PublisherChallenge, MfrError> {
        let mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        if !matches!(mfr.status, MfrStatus::Pending | MfrStatus::Active) {
            return Err(MfrError::InvalidStatus(format!(
                "publisher cannot hold a verification challenge (status: {})",
                mfr.status
            )));
        }
        PublisherChallenge::get_active(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::ChallengeNotFound)
    }

    pub async fn get_status(&self, mfr_id: i64) -> Result<Manufacturer, MfrError> {
        Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)
    }

    pub async fn resolve_by_name(&self, name: &str) -> Result<PublisherPublicInfo, MfrError> {
        let mfr = Manufacturer::get_by_name(&self.pool, name)
            .await?
            .ok_or(MfrError::NotFound)?;
        if mfr.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                name
            )));
        }
        let expires_at = mfr.key_expires_at.ok_or(MfrError::CertificateExpired)?;
        let cert = PublisherCertificate {
            key_id: mfr.key_id.clone(),
            publisher: mfr.name.clone(),
            public_key: mfr.public_key.clone(),
            issued_at: mfr.verified_at.unwrap_or(mfr.created_at),
            expires_at,
        };
        let descriptor = serde_json::from_str(&mfr.descriptor_json).ok();
        Ok(PublisherPublicInfo {
            id: mfr.id,
            key_id: mfr.key_id.clone(),
            name: mfr.name,
            public_key: mfr.public_key,
            certificate: cert,
            descriptor,
        })
    }

    /// Resolve a specific historical (or active) key by exact key_id.
    pub async fn resolve_key_by_id(
        &self,
        name: &str,
        key_id: &str,
    ) -> Result<PublisherPublicInfo, MfrError> {
        let mfr = Manufacturer::get_by_name(&self.pool, name)
            .await?
            .ok_or(MfrError::NotFound)?;
        if mfr.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                name
            )));
        }

        // Fail closed even on the current-key fast path: if this exact key_id
        // carries a revoked history row, refuse to serve it before the
        // short-circuit below could hand back the live public key.
        if MfrKeyHistory::is_key_id_revoked(&self.pool, mfr.id, key_id).await? {
            platform::recording::warn!(
                "rejected resolution of revoked key on current-key path: \
                 publisher_id={}, key_id={}",
                mfr.id,
                key_id
            );
            return Err(MfrError::KeyRevoked(key_id.to_string()));
        }

        // Is it the current key?
        if mfr.key_id == key_id {
            return self.resolve_by_name(name).await;
        }

        let history = MfrKeyHistory::get_by_key_id(&self.pool, mfr.id, key_id)
            .await?
            .ok_or(MfrError::NotFound)?;

        let cert = PublisherCertificate {
            key_id: history.key_id.clone(),
            publisher: mfr.name.clone(),
            public_key: history.public_key.clone(),
            issued_at: history.created_at,
            expires_at: history.retired_at, // Old keys technically "expired" at retirement time
        };
        let descriptor = serde_json::from_str(&mfr.descriptor_json).ok();

        Ok(PublisherPublicInfo {
            id: mfr.id,
            key_id: history.key_id.clone(),
            name: mfr.name,
            public_key: history.public_key,
            certificate: cert,
            descriptor,
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
    ) -> Result<String, MfrError> {
        let mfr = Manufacturer::get(&self.pool, package.mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        if mfr.name != package.manufacturer {
            return Err(MfrError::InvalidRequest(
                "package publisher does not match its registry record".to_string(),
            ));
        }
        if mfr.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "Publisher '{}' is not active",
                mfr.name
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
            return Err(MfrError::InvalidSignature);
        }

        if let Some(key_id) = package
            .signing_key_id
            .as_deref()
            .or(manifest_key_id.as_deref())
        {
            let key = self.resolve_key_by_id(&mfr.name, key_id).await?;
            let valid = crypto::verify_signature(
                package.manifest.as_bytes(),
                &package.signature,
                &key.public_key,
            )?;
            return if valid {
                Ok(key.key_id)
            } else {
                Err(MfrError::InvalidSignature)
            };
        }

        // Legacy package: identify the signing key by verifying the stored
        // signature against the current key and all non-revoked historical keys.
        if !mfr.key_id.is_empty() && !mfr.public_key.is_empty() {
            let valid = crypto::verify_signature(
                package.manifest.as_bytes(),
                &package.signature,
                &mfr.public_key,
            )?;
            if valid {
                return Ok(mfr.key_id);
            }
        }

        for history in MfrKeyHistory::list_by_mfr(&self.pool, mfr.id).await? {
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

        Err(MfrError::InvalidSignature)
    }

    /// Admin: rotate the signing key to a publisher-supplied public key.
    ///
    /// Actrix never generates or handles publisher private keys in production.
    pub async fn renew_key(
        &self,
        mfr_id: i64,
        user_public_key: &str,
    ) -> Result<PublisherVerificationResponse, MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;

        crypto::validate_public_key(user_public_key)?;
        let public_key = user_public_key.to_string();

        // Historical-key blacklist on the renew path too. Without this, renew
        // could be abused to ride A -> B -> revoke(A) -> A back into the current
        // key slot: rotating away archives the outgoing key, but nothing stopped
        // a later renew from reinstating a previously retired or revoked key.
        // Mirror the identical guard on the HTTPS verification path so any key_id
        // ever archived (retired or revoked) is refused as an incoming key.
        let incoming_key_id = crypto::compute_key_id_from_b64(&public_key)?;
        if MfrKeyHistory::key_id_ever_used(&self.pool, mfr.id, &incoming_key_id).await? {
            platform::recording::warn!(
                "rejected reinstatement of previously used publisher key on renew: \
                 publisher_id={}, key_id={}",
                mfr.id,
                incoming_key_id
            );
            return Err(MfrError::InvalidRequest(
                "a previously used publisher key must not be reinstated".to_string(),
            ));
        }

        mfr.renew_key(&self.pool, public_key).await?;

        let expires_at = mfr.key_expires_at.ok_or(MfrError::CertificateExpired)?;

        platform::recording::info!(
            "Publisher key renewed: publisher_id={}, name={}, new_key_id={}",
            mfr_id,
            mfr.name,
            mfr.key_id
        );

        Ok(PublisherVerificationResponse {
            certificate: PublisherCertificate {
                key_id: mfr.key_id.clone(),
                publisher: mfr.name.clone(),
                public_key: mfr.public_key.clone(),
                issued_at: mfr.updated_at.unwrap_or(mfr.created_at),
                expires_at,
            },
        })
    }

    pub async fn publish_package(&self, req: PublishRequest) -> Result<ActrPackage, MfrError> {
        let mfr = Manufacturer::get_by_name(&self.pool, &req.publisher)
            .await?
            .ok_or(MfrError::NotFound)?;
        Self::validate_publish_authority(&mfr)?;

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
                    .map_err(|_| MfrError::InvalidRequest("invalid nonce base64".to_string()))?;

                // Step 1: Read-only lookup — does NOT consume the nonce
                let entry = PublishNonce::find_pending(&self.pool, &nonce_bytes).await?;

                // Ensure nonce was issued for this manufacturer
                if entry.mfr_id != mfr.id {
                    platform::recording::warn!(
                        "publish nonce publisher_id mismatch: nonce={}, request={}",
                        entry.mfr_id,
                        mfr.id
                    );
                    return Err(MfrError::Unauthorized);
                }

                // Step 2: Verify nonce signature (before consuming).
                // The signature authorizes this exact publish request body (excluding nonce_sig).
                let body_bytes = signable_publish_body_bytes(&req, nonce_b64)?;
                let body_sha256 = hex::encode(Sha256::digest(&body_bytes));
                let nonce_hex = hex::encode(&nonce_bytes);
                let payload = build_publish_auth_payload(&req.publisher, &nonce_hex, &body_sha256);

                let valid =
                    crypto::verify_signature(payload.as_bytes(), nonce_sig_b64, &mfr.public_key)?;
                if !valid {
                    platform::recording::warn!(
                        "publish nonce signature invalid: publisher={}",
                        req.publisher
                    );
                    return Err(MfrError::Unauthorized);
                }

                entry
            }
            _ => {
                platform::recording::warn!(
                    "publish request missing nonce/nonce_sig: publisher={}",
                    req.publisher
                );
                return Err(MfrError::Unauthorized);
            }
        };

        // Step 3: Verify manifest signature
        let valid =
            crypto::verify_signature(req.manifest.as_bytes(), &req.signature, &mfr.public_key)?;
        if !valid {
            return Err(MfrError::InvalidSignature);
        }

        // Step 4: Atomically consume nonce (only after both signatures verified)
        PublishNonce::consume(&self.pool, nonce_entry.id).await?;
        platform::recording::info!(
            "publish nonce verified and consumed: publisher={}",
            req.publisher
        );

        // Step 5: Verify outer fields match manifest content (parse as TOML)
        let manifest_toml: toml::Value = toml::from_str(&req.manifest)
            .map_err(|e| MfrError::InvalidRequest(format!("manifest is not valid TOML: {}", e)))?;

        // Check Publisher
        let m = manifest_toml
            .get("publisher")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MfrError::InvalidRequest("manifest missing required field 'publisher'".into())
            })?;
        if m != req.publisher {
            return Err(MfrError::InvalidRequest(format!(
                "manifest publisher '{}' != request '{}'",
                m, req.publisher
            )));
        }

        // Check name
        let n = manifest_toml
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MfrError::InvalidRequest("manifest missing required field 'name'".into())
            })?;
        if n != req.name {
            return Err(MfrError::InvalidRequest(format!(
                "manifest name '{}' != request '{}'",
                n, req.name
            )));
        }

        // Check version
        let v = manifest_toml
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MfrError::InvalidRequest("manifest missing required field 'version'".into())
            })?;
        if v != req.version {
            return Err(MfrError::InvalidRequest(format!(
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
            && manifest_key_id != mfr.key_id
        {
            return Err(MfrError::InvalidRequest(format!(
                "manifest signing_key_id '{}' != current Publisher key '{}'",
                manifest_key_id, mfr.key_id
            )));
        }

        // Check target (nested under [binary])
        let t = manifest_toml
            .get("binary")
            .and_then(|b| b.get("target"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MfrError::InvalidRequest("manifest missing required field 'binary.target'".into())
            })?;
        if t != req.target {
            return Err(MfrError::InvalidRequest(format!(
                "manifest binary.target '{}' != request '{}'",
                t, req.target
            )));
        }

        // Serialize proto_files JSON to string for storage
        let proto_files_str = req.proto_files.as_ref().map(|v| v.to_string());

        let pkg = ActrPackage::publish(
            &self.pool,
            mfr.id,
            &req.publisher,
            &req.name,
            &req.version,
            &req.target,
            &req.manifest,
            &req.signature,
            &mfr.key_id,
            proto_files_str.as_deref(),
        )
        .await?;

        if req.proto_files.is_some() {
            platform::recording::info!(
                "actor package published with proto filing: type_str={}, publisher_id={}",
                pkg.type_str,
                mfr.id
            );
        } else {
            platform::recording::info!(
                "actor package published: type_str={}, publisher_id={}",
                pkg.type_str,
                mfr.id
            );
        }
        Ok(pkg)
    }

    pub async fn get_package(&self, type_str: &str) -> Result<ActrPackage, MfrError> {
        ActrPackage::get_by_type(&self.pool, type_str)
            .await?
            .ok_or(MfrError::NotFound)
    }

    pub async fn list_packages(
        &self,
        publisher: Option<&str>,
    ) -> Result<Vec<ActrPackage>, MfrError> {
        if let Some(name) = publisher {
            let mfr = Manufacturer::get_by_name(&self.pool, name)
                .await?
                .ok_or(MfrError::NotFound)?;
            ActrPackage::list_by_mfr(&self.pool, mfr.id).await
        } else {
            Ok(sqlx::query_as::<_, ActrPackage>(
                "SELECT * FROM mfr_package ORDER BY published_at DESC",
            )
            .fetch_all(&self.pool)
            .await?)
        }
    }

    pub async fn revoke_package(&self, pkg_id: i64) -> Result<(), MfrError> {
        let mut pkg = ActrPackage::get_by_id(&self.pool, pkg_id)
            .await?
            .ok_or(MfrError::NotFound)?;
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
        status: Option<MfrStatus>,
    ) -> Result<Vec<Manufacturer>, MfrError> {
        Manufacturer::list(&self.pool, status).await
    }

    pub async fn admin_suspend(&self, mfr_id: i64) -> Result<(), MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        mfr.suspend(&self.pool).await?;
        platform::recording::warn!(
            "Publisher suspended by admin: publisher_id={}, name={}",
            mfr_id,
            mfr.name
        );
        Ok(())
    }

    pub async fn admin_reinstate(&self, mfr_id: i64) -> Result<(), MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        mfr.reinstate(&self.pool).await?;
        platform::recording::info!(
            "Publisher reinstated by admin: publisher_id={}, name={}",
            mfr_id,
            mfr.name
        );
        Ok(())
    }

    pub async fn admin_list_keys(
        &self,
        mfr_id: i64,
    ) -> Result<Vec<crate::model::key_history::MfrKeyHistory>, MfrError> {
        // Ensure the Publisher exists.
        let _ = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        crate::model::key_history::MfrKeyHistory::list_by_mfr(&self.pool, mfr_id).await
    }

    pub async fn admin_revoke_historical_key(&self, history_id: i64) -> Result<(), MfrError> {
        crate::model::key_history::MfrKeyHistory::revoke(&self.pool, history_id).await?;
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
    /// revoked one. See `Manufacturer::revoke_active_key` for the exact post-state.
    pub async fn admin_revoke_active_key(&self, mfr_id: i64) -> Result<(), MfrError> {
        let mut mfr = Manufacturer::get(&self.pool, mfr_id)
            .await?
            .ok_or(MfrError::NotFound)?;
        mfr.revoke_active_key(&self.pool).await?;
        platform::recording::warn!(
            "Publisher active signing key revoked by admin: publisher_id={}, name={}",
            mfr_id,
            mfr.name
        );
        Ok(())
    }

    pub async fn admin_delete(&self, mfr_id: i64) -> Result<(), MfrError> {
        Manufacturer::delete(&self.pool, mfr_id).await?;
        platform::recording::warn!("Publisher deleted by admin: publisher_id={}", mfr_id);
        Ok(())
    }
}

/// Public API for AIS integration: check if a type_str is a valid, active package.
/// Reserved names always return true.
///
/// When `target` is provided, lookup is narrowed to that specific platform.
/// When `manifest_hash` is provided, the stored manifest is SHA-256 compared for content integrity.
pub async fn lookup_package(
    pool: &SqlitePool,
    type_str: &str,
    target: Option<&str>,
    manifest_hash: Option<&[u8]>,
) -> Result<bool, MfrError> {
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
