//! Accepted `realm_signing_keys` of Realm domains (the key pin).
//!
//! Each member runs the acceptance procedure of
//! `actr/protocols/authority-discovery.md` section 4.4 against the domain's
//! [`KeyPins`]: the first member is pinned, later ones may retire, remove or,
//! through a rotation chain, introduce keys. A member whose signing key does
//! not chain to a pinned key is an identity change and is rejected; there is
//! no re-confirmation.

use actr_protocol::realm_profile::jcs;
use actr_protocol::uri::ActrUri;
use actr_protocol::{AuthorityDescriptor, KeyPinOutcome, KeyPins, RealmSigningKeys};
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::error::RealmError;
use crate::storage::db::get_database;

/// A Realm domain's accepted `realm_signing_keys` member.
#[derive(Debug, Clone)]
pub struct RealmDomain {
    pub domain: String,
    /// The accepted member, listing only the pinned keys it still lists.
    pub keys: RealmSigningKeys,
    pub pins: KeyPins,
    pub pinned_at: u64,
    pub checked_at: u64,
    pins_json: String,
}

fn invalid(message: impl Into<String>) -> RealmError {
    RealmError::ValidationError(message.into())
}

/// SHA-256 hex of `bytes`.
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Check a `realm_signing_keys` member and its self-signature: an `active`
/// listed key signs the RFC 8785 JCS form of the member without `signature`,
/// and every rotation between listed keys verifies.
pub fn verify_member(member: &Value) -> Result<RealmSigningKeys, RealmError> {
    RealmSigningKeys::accept(member, None)
        .map(|(keys, _)| keys)
        .map_err(|error| invalid(error.to_string()))
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

/// The accepted member restricted to the keys `pins` still lists, so a key
/// listed without a chain to a pinned key signs nothing here.
fn pinned_keys(mut keys: RealmSigningKeys, pins: &KeyPins) -> RealmSigningKeys {
    keys.keys = pins.signing_keys();
    keys
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for RealmDomain {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let decode = |error: serde_json::Error| sqlx::Error::Decode(Box::new(error));
        let keys_json: String = row.try_get("keys_json")?;
        let pins_json: String = row.try_get("pins_json")?;
        let pins: KeyPins = serde_json::from_str(&pins_json).map_err(decode)?;
        let keys = serde_json::from_str(&keys_json).map_err(decode)?;
        Ok(Self {
            domain: row.try_get("domain")?,
            keys: pinned_keys(keys, &pins),
            pins,
            pinned_at: row.try_get::<i64, _>("pinned_at")? as u64,
            checked_at: row.try_get::<i64, _>("checked_at")? as u64,
            pins_json,
        })
    }
}

impl RealmDomain {
    pub async fn get(domain: &str) -> Result<Option<Self>, RealmError> {
        Ok(sqlx::query_as::<_, Self>(
            "SELECT domain, keys_json, pins_json, pinned_at, checked_at
             FROM realm_domain WHERE domain = ?",
        )
        .bind(domain)
        .fetch_optional(get_database().get_pool())
        .await?)
    }

    /// Accept `member` for `domain` under the domain's pins; see the module
    /// documentation.
    pub async fn accept(domain: &str, member: &Value) -> Result<Self, RealmError> {
        let pinned = Self::get(domain).await?;
        let (_, outcome) = RealmSigningKeys::accept(member, pinned.as_ref().map(|p| &p.pins))
            .map_err(|error| invalid(format!("{domain}: {error}")))?;
        let pins = match outcome {
            KeyPinOutcome::Accepted(pins) => pins,
            KeyPinOutcome::IdentityChange(_) => {
                return Err(invalid(format!(
                    "{domain} realm_signing_keys is not signed by a key chained to a pinned \
                     key: an identity change"
                )));
            }
        };
        let keys_json = jcs(member);
        let pins_json =
            serde_json::to_string(&pins).map_err(|error| invalid(format!("key pins: {error}")))?;
        let now = Utc::now().timestamp();

        if let Some(pinned) = pinned {
            let updated = sqlx::query(
                "UPDATE realm_domain SET keys_json = ?, pins_json = ?, checked_at = ?
                 WHERE domain = ? AND pins_json = ?",
            )
            .bind(&keys_json)
            .bind(&pins_json)
            .bind(now)
            .bind(domain)
            .bind(&pinned.pins_json)
            .execute(get_database().get_pool())
            .await?;
            if updated.rows_affected() == 0 {
                return Err(invalid(format!("{domain} keys changed concurrently")));
            }
        } else {
            sqlx::query(
                "INSERT INTO realm_domain (domain, keys_json, pins_json, pinned_at, checked_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(domain)
            .bind(&keys_json)
            .bind(&pins_json)
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
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::util::test_utils::utils::setup_test_db;
    use actr_protocol::{DetachedSignature, KeyStatus, RealmSigningKey};
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
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
        ];
        for member in &rejected {
            assert!(RealmDomain::accept("acme.com", member).await.is_err());
        }
        // A new key listed without a rotation is not pinned and signs nothing.
        let unchained = signed_member(
            4,
            &[
                ("rs-1", &one, Active),
                ("rs-2", &two, Active),
                ("rs-9", &stranger, Active),
            ],
        );
        let accepted = RealmDomain::accept("acme.com", &unchained).await?;
        assert!(accepted.keys.keys.iter().all(|key| key.id != "rs-9"));
        let by_unchained = signed_member(
            5,
            &[
                ("rs-9", &stranger, Active),
                ("rs-1", &one, Active),
                ("rs-2", &two, Active),
            ],
        );
        assert!(
            RealmDomain::accept("acme.com", &by_unchained)
                .await
                .is_err()
        );

        // Retiring and removing keys moves forward.
        let next = signed_member(5, &[("rs-2", &two, Active), ("rs-1", &one, Retired)]);
        RealmDomain::accept("acme.com", &next).await?;
        // A key retired by an accepted member no longer signs members, and
        // no member can make it active again.
        let by_retired = signed_member(6, &[("rs-1", &one, Active), ("rs-2", &two, Active)]);
        assert!(RealmDomain::accept("acme.com", &by_retired).await.is_err());
        let reactivated = signed_member(6, &[("rs-2", &two, Active), ("rs-1", &one, Active)]);
        assert!(RealmDomain::accept("acme.com", &reactivated).await.is_err());
        let revoked = signed_member(6, &[("rs-2", &two, Active)]);
        let accepted = RealmDomain::accept("acme.com", &revoked).await?;
        assert_eq!(accepted.keys.keys.len(), 1);
        assert_eq!(accepted.pinned_at, pinned.pinned_at);
        // A removed key never comes back.
        let back = signed_member(7, &[("rs-2", &two, Active), ("rs-1", &one, Retired)]);
        assert!(RealmDomain::accept("acme.com", &back).await.is_err());
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn follows_a_rotation_chain() -> anyhow::Result<()> {
        setup_test_db().await?;
        let old = SigningKey::from_bytes(&[1; 32]);
        let new = SigningKey::from_bytes(&[2; 32]);
        use KeyStatus::{Active, Retired};
        RealmDomain::accept("acme.com", &signed_member(1, &[("rs-1", &old, Active)])).await?;

        // Switched to rs-2 at sequence 3 without seeing sequence 2.
        let rotation = actr_protocol::KeyRotation {
            from: "rs-1".into(),
            to: "rs-2".into(),
            sequence: 2,
            signature: String::new(),
        };
        let public_key = BASE64.encode(new.verifying_key().to_bytes());
        let signature =
            old.sign(actr_protocol::rotation_payload(&rotation, &public_key).as_bytes());
        let mut member = signed_member(3, &[("rs-2", &new, Active), ("rs-1", &old, Retired)]);
        member["rotations"] = serde_json::json!([{
            "from": "rs-1", "to": "rs-2", "sequence": 2,
            "signature": BASE64.encode(signature.to_bytes()),
        }]);
        let mut payload = member.clone();
        payload.as_object_mut().unwrap().remove("signature");
        member["signature"]["value"] = BASE64
            .encode(new.sign(jcs(&payload).as_bytes()).to_bytes())
            .into();

        let accepted = RealmDomain::accept("acme.com", &member).await?;
        assert_eq!(accepted.pins.sequence, 3);
        assert_eq!(accepted.keys.keys.len(), 2);
        Ok(())
    }
}
