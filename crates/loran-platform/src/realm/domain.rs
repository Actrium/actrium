//! Accepted `realm_signing_keys` of Realm domains (the key pin).
//!
//! Minimal pinning per `actr/protocols/authority-discovery.md` section 4.4:
//! the member must carry a valid self-signature, the first accepted member is
//! pinned, and later members must be signed by a pinned `active` key, never
//! lower the `sequence`, and list only pinned keys. Rotation chains are not
//! followed yet, so a member that introduces a new key is rejected.

use actr_protocol::realm_profile::jcs;
use actr_protocol::uri::ActrUri;
use actr_protocol::{AuthorityDescriptor, KeyStatus, RealmSigningKeys};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::error::RealmError;
use crate::storage::db::get_database;

/// A Realm domain's accepted `realm_signing_keys` member.
#[derive(Debug, Clone)]
pub struct RealmDomain {
    pub domain: String,
    pub keys: RealmSigningKeys,
    pub pinned_at: u64,
    pub checked_at: u64,
    digest: String,
}

fn invalid(message: impl Into<String>) -> RealmError {
    RealmError::ValidationError(message.into())
}

/// SHA-256 hex of `bytes`.
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Check a `realm_signing_keys` member and its self-signature: an `active`
/// listed key signs the RFC 8785 JCS form of the member without `signature`.
pub fn verify_member(member: &Value) -> Result<RealmSigningKeys, RealmError> {
    let keys: RealmSigningKeys = serde_json::from_value(member.clone())
        .map_err(|error| invalid(format!("malformed realm_signing_keys: {error}")))?;
    keys.validate()
        .map_err(|error| invalid(error.to_string()))?;
    let key = keys
        .keys
        .iter()
        .find(|key| key.id == keys.signature.key_id)
        .ok_or_else(|| invalid("realm_signing_keys signature key is not listed"))?;
    let public_key: [u8; 32] = BASE64
        .decode(&key.public_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid(format!("key '{}' is not an Ed25519 key", key.id)))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|error| invalid(format!("key '{}': {error}", key.id)))?;
    let signature: [u8; 64] = BASE64
        .decode(&keys.signature.value)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("realm_signing_keys signature is not Ed25519"))?;

    let mut payload = member.clone();
    if let Value::Object(members) = &mut payload {
        members.remove("signature");
    }
    verifying_key
        .verify(jcs(&payload).as_bytes(), &Signature::from_bytes(&signature))
        .map_err(|_| invalid("realm_signing_keys signature verification failed"))?;
    Ok(keys)
}

/// The `realm_signing_keys` member of `domain`'s authority descriptor, as
/// served. The descriptor must be valid for `actr://<domain>`.
pub fn member_from_descriptor(domain: &str, document: &[u8]) -> Result<Value, RealmError> {
    let mut value: Value = serde_json::from_slice(document)
        .map_err(|error| invalid(format!("descriptor is not JSON: {error}")))?;
    let descriptor: AuthorityDescriptor = serde_json::from_value(value.clone())
        .map_err(|error| invalid(format!("malformed descriptor: {error}")))?;
    let authority = ActrUri::from_domain(domain).map_err(|error| invalid(error.to_string()))?;
    descriptor
        .validate_for(&authority)
        .map_err(|error| invalid(error.to_string()))?;
    value
        .get_mut("realm_signing_keys")
        .map(Value::take)
        .ok_or_else(|| invalid(format!("{domain} publishes no realm_signing_keys")))
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for RealmDomain {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let keys_json: String = row.try_get("keys_json")?;
        let keys = serde_json::from_str(&keys_json)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        Ok(Self {
            domain: row.try_get("domain")?,
            keys,
            pinned_at: row.try_get::<i64, _>("pinned_at")? as u64,
            checked_at: row.try_get::<i64, _>("checked_at")? as u64,
            digest: row.try_get("keys_digest")?,
        })
    }
}

impl RealmDomain {
    pub async fn get(domain: &str) -> Result<Option<Self>, RealmError> {
        Ok(sqlx::query_as::<_, Self>(
            "SELECT domain, keys_json, keys_digest, pinned_at, checked_at
             FROM realm_domain WHERE domain = ?",
        )
        .bind(domain)
        .fetch_optional(get_database().get_pool())
        .await?)
    }

    /// Accept `member` for `domain`. The first accepted member is pinned;
    /// later ones must pass the pin checks in the module documentation.
    pub async fn accept(domain: &str, member: &Value) -> Result<Self, RealmError> {
        let keys = verify_member(member)?;
        let keys_json = jcs(member);
        let keys_digest = digest(keys_json.as_bytes());
        let now = Utc::now().timestamp();

        if let Some(pinned) = Self::get(domain).await? {
            pinned.check_successor(&keys, &keys_digest)?;
            let updated = sqlx::query(
                "UPDATE realm_domain
                 SET keys_json = ?, keys_sequence = ?, keys_digest = ?, checked_at = ?
                 WHERE domain = ? AND keys_digest = ?",
            )
            .bind(&keys_json)
            .bind(keys.sequence as i64)
            .bind(&keys_digest)
            .bind(now)
            .bind(domain)
            .bind(&pinned.digest)
            .execute(get_database().get_pool())
            .await?;
            if updated.rows_affected() == 0 {
                return Err(invalid(format!("{domain} keys changed concurrently")));
            }
        } else {
            sqlx::query(
                "INSERT INTO realm_domain
                    (domain, keys_json, keys_sequence, keys_digest, pinned_at, checked_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(domain)
            .bind(&keys_json)
            .bind(keys.sequence as i64)
            .bind(&keys_digest)
            .bind(now)
            .bind(now)
            .execute(get_database().get_pool())
            .await
            .map_err(|error| match error {
                sqlx::Error::Database(e) if e.is_unique_violation() => {
                    invalid(format!("{domain} keys changed concurrently"))
                }
                error => error.into(),
            })?;
        }
        Self::get(domain).await?.ok_or(RealmError::NotFound)
    }

    fn check_successor(&self, keys: &RealmSigningKeys, digest: &str) -> Result<(), RealmError> {
        let pinned = |id: &str, public_key: &str| {
            self.keys
                .keys
                .iter()
                .find(|key| key.id == id && key.public_key == public_key)
        };
        let signer = keys
            .keys
            .iter()
            .find(|key| key.id == keys.signature.key_id)
            .and_then(|key| pinned(&key.id, &key.public_key));
        if signer.is_none_or(|key| key.status != KeyStatus::Active) {
            return Err(invalid(format!(
                "{} realm_signing_keys is not signed by a pinned active key",
                self.domain
            )));
        }
        if keys.sequence < self.keys.sequence
            || (keys.sequence == self.keys.sequence && digest != self.digest)
        {
            return Err(invalid(format!(
                "{} realm_signing_keys sequence {} rolls back {}",
                self.domain, keys.sequence, self.keys.sequence
            )));
        }
        if let Some(key) = keys
            .keys
            .iter()
            .find(|key| pinned(&key.id, &key.public_key).is_none())
        {
            return Err(invalid(format!(
                "{} lists new Realm signing key '{}'; key rotation chains are not supported",
                self.domain, key.id
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::util::test_utils::utils::setup_test_db;
    use actr_protocol::{DetachedSignature, RealmSigningKey};
    use ed25519_dalek::{Signer, SigningKey};
    use serial_test::serial;

    /// A self-signed member listing `keys`, signed by the first one.
    pub(crate) fn signed_member(sequence: u64, keys: &[(&str, &SigningKey, KeyStatus)]) -> Value {
        let member = RealmSigningKeys {
            sequence,
            keys: keys
                .iter()
                .map(|(id, key, status)| RealmSigningKey {
                    id: id.to_string(),
                    algorithm: "Ed25519".into(),
                    public_key: BASE64.encode(key.verifying_key().to_bytes()),
                    status: *status,
                })
                .collect(),
            rotations: None,
            signature: DetachedSignature {
                key_id: keys[0].0.to_string(),
                algorithm: "Ed25519".into(),
                value: String::new(),
            },
        };
        let mut value = serde_json::to_value(member).unwrap();
        value.as_object_mut().unwrap().remove("signature");
        let signature = keys[0].1.sign(jcs(&value).as_bytes());
        value["signature"] = serde_json::json!({
            "key_id": keys[0].0,
            "algorithm": "Ed25519",
            "value": BASE64.encode(signature.to_bytes()),
        });
        value
    }

    #[test]
    fn verifies_member_self_signature() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let member = signed_member(1, &[("rs-1", &key, KeyStatus::Active)]);
        assert_eq!(verify_member(&member).unwrap().sequence, 1);

        let mut tampered = member.clone();
        tampered["sequence"] = 2.into();
        assert!(verify_member(&tampered).is_err());

        let other = SigningKey::from_bytes(&[2; 32]);
        let mut forged = signed_member(1, &[("rs-1", &other, KeyStatus::Active)]);
        forged["keys"] = member["keys"].clone();
        assert!(verify_member(&forged).is_err());
    }

    #[test]
    fn reads_member_from_a_valid_descriptor_only() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let member = signed_member(1, &[("rs-1", &key, KeyStatus::Active)]);
        let descriptor = |authority: &str, member: Option<&Value>| {
            let mut value = serde_json::json!({
                "version": 1,
                "authority": authority,
                "services": [{ "rel": "identity", "href": "https://acme.com/id" }],
            });
            if let Some(member) = member {
                value["realm_signing_keys"] = member.clone();
            }
            serde_json::to_vec(&value).unwrap()
        };
        let found =
            member_from_descriptor("acme.com", &descriptor("actr://acme.com", Some(&member)))
                .unwrap();
        assert_eq!(found, member);
        for document in [
            descriptor("actr://other.com", Some(&member)),
            descriptor("actr://acme.com", None),
            b"not json".to_vec(),
        ] {
            assert!(member_from_descriptor("acme.com", &document).is_err());
        }
    }

    #[tokio::test]
    #[serial]
    async fn pins_first_member_and_checks_successors() -> anyhow::Result<()> {
        setup_test_db().await?;
        let one = SigningKey::from_bytes(&[1; 32]);
        let two = SigningKey::from_bytes(&[2; 32]);
        let stranger = SigningKey::from_bytes(&[9; 32]);
        use KeyStatus::{Active, Retired};

        let first = signed_member(3, &[("rs-1", &one, Active), ("rs-2", &two, Active)]);
        let pinned = RealmDomain::accept("acme.com", &first).await?;
        assert_eq!(pinned.keys.sequence, 3);
        // The same member again only refreshes the check time.
        RealmDomain::accept("acme.com", &first).await?;

        let rejected = [
            // Lower sequence, or different content at the same sequence.
            signed_member(2, &[("rs-1", &one, Active), ("rs-2", &two, Active)]),
            signed_member(3, &[("rs-1", &one, Active)]),
            // Not chained to a pinned key: an identity change.
            signed_member(4, &[("rs-9", &stranger, Active)]),
            // Same id, different key.
            signed_member(4, &[("rs-1", &stranger, Active)]),
            // A new key needs a rotation chain.
            signed_member(4, &[("rs-1", &one, Active), ("rs-9", &stranger, Active)]),
        ];
        for member in &rejected {
            assert!(RealmDomain::accept("acme.com", member).await.is_err());
        }

        // Retiring and removing keys moves forward.
        let next = signed_member(4, &[("rs-2", &two, Active), ("rs-1", &one, Retired)]);
        RealmDomain::accept("acme.com", &next).await?;
        // A key retired by an accepted member no longer signs members.
        let by_retired = signed_member(5, &[("rs-1", &one, Active), ("rs-2", &two, Active)]);
        assert!(RealmDomain::accept("acme.com", &by_retired).await.is_err());
        let revoked = signed_member(5, &[("rs-2", &two, Active)]);
        let accepted = RealmDomain::accept("acme.com", &revoked).await?;
        assert_eq!(accepted.keys.keys.len(), 1);
        assert_eq!(accepted.pinned_at, pinned.pinned_at);
        Ok(())
    }
}
