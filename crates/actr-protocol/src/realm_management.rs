//! Realm management requests (`actr/protocols/realm-profile.md` section 8).
//!
//! A request is an Actr management envelope: the payload members plus a
//! `signature` member, a detached Ed25519 signature by an `active` Realm
//! signing key of the Realm's domain over the RFC 8785 JCS form of the
//! payload (the request without `signature`).

use crate::authority::{DetachedSignature, KeyStatus, RealmSigningKeys};
use crate::realm_profile::{RealmPackage, jcs, sign_document, verify_signature};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const REALM_MANAGEMENT_PROTOCOL: &str = "actr-realm-management/1";

/// Longest validity window, `expires_at - issued_at`, in seconds.
pub const MAX_VALIDITY_SECS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementOp {
    CreateRealm,
    PublishProfile,
    ReissueAdminToken,
}

/// A Realm management request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagementRequest {
    pub op: ManagementOp,
    /// Full Realm name `<realm_id>@<domain>`.
    pub realm: String,
    /// `create_realm`: the package of the first Realm Profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<RealmPackage>,
    /// `publish_profile`: the signed Realm Profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Value>,
    /// Realm Provider ID of the receiver.
    pub provider_id: String,
    /// Opaque id of whom the key holder's signing service signs for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester: Option<String>,
    pub nonce: String,
    /// RFC 3339.
    pub issued_at: String,
    /// RFC 3339, at most [`MAX_VALIDITY_SECS`] after `issued_at`.
    pub expires_at: String,
    /// Absent before signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<DetachedSignature>,
    /// JCS form of the received request without `signature`, unknown
    /// members included; what the signature covers.
    #[serde(skip)]
    signed_payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid Realm management request: {0}")]
pub struct ManagementRequestError(pub String);

fn invalid(message: impl Into<String>) -> ManagementRequestError {
    ManagementRequestError(message.into())
}

impl ManagementRequest {
    /// An unsigned request valid for five minutes from now. `nonce` must be
    /// random and never reused.
    pub fn new(op: ManagementOp, realm: &str, provider_id: &str, nonce: &str) -> Self {
        let now = Utc::now();
        let format = |time: DateTime<Utc>| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        Self {
            op,
            realm: realm.to_string(),
            package: None,
            profile: None,
            provider_id: provider_id.to_string(),
            requester: None,
            nonce: nonce.to_string(),
            issued_at: format(now),
            expires_at: format(now + chrono::Duration::seconds(MAX_VALIDITY_SECS)),
            signature: None,
            signed_payload: String::new(),
        }
    }

    /// The signed envelope.
    pub fn sign(&self, key_id: &str, key: &SigningKey) -> Value {
        let mut document = serde_json::to_value(self).expect("request serializes");
        sign_document(&mut document, key_id, key);
        document
    }

    /// Parse a signed envelope. Its signature is checked by [`Self::verify`]
    /// once the Realm domain's keys are known.
    pub fn parse(document: &[u8]) -> Result<Self, ManagementRequestError> {
        let mut value: Value = serde_json::from_slice(document)
            .map_err(|error| invalid(format!("not valid JSON: {error}")))?;
        let request: Self = serde_json::from_value(value.clone())
            .map_err(|error| invalid(format!("malformed request: {error}")))?;
        if request.signature.is_none() {
            return Err(invalid("request is not signed"));
        }
        if let Value::Object(members) = &mut value {
            members.remove("signature");
        }
        Ok(Self {
            signed_payload: jcs(&value),
            ..request
        })
    }

    /// Check the validity window: at most five minutes long and containing
    /// `now`.
    pub fn check_window(&self, now: DateTime<Utc>) -> Result<(), ManagementRequestError> {
        let parse = |value: &str, member: &str| {
            DateTime::parse_from_rfc3339(value)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|error| invalid(format!("{member}: {error}")))
        };
        let issued_at = parse(&self.issued_at, "issued_at")?;
        let expires_at = parse(&self.expires_at, "expires_at")?;
        let length = (expires_at - issued_at).num_seconds();
        if !(0..=MAX_VALIDITY_SECS).contains(&length) {
            return Err(invalid(format!(
                "validity window of {length} seconds exceeds {MAX_VALIDITY_SECS}"
            )));
        }
        if now < issued_at || now > expires_at {
            return Err(invalid("request is outside its validity window"));
        }
        Ok(())
    }

    /// Verify the signature of a request from [`Self::parse`] with an
    /// `active` key of `keys`, the accepted `realm_signing_keys` member of the
    /// Realm's domain.
    pub fn verify(&self, keys: &RealmSigningKeys) -> Result<(), ManagementRequestError> {
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| invalid("request is not signed"))?;
        let key = verify_signature(keys, signature, self.signed_payload.as_bytes())
            .map_err(|error| invalid(error.to_string()))?;
        if key.status != KeyStatus::Active {
            return Err(invalid(format!(
                "Realm signing key '{}' is not active",
                key.id
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::RealmSigningKey;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;

    fn keys(listed: &[(&str, &SigningKey, KeyStatus)]) -> RealmSigningKeys {
        RealmSigningKeys {
            sequence: 1,
            keys: listed
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
                key_id: listed[0].0.to_string(),
                algorithm: "Ed25519".into(),
                value: String::new(),
            },
        }
    }

    fn request() -> ManagementRequest {
        let mut request = ManagementRequest::new(
            ManagementOp::CreateRealm,
            "k7m2q9xh@acme.com",
            "p.example",
            "n-1",
        );
        request.package = Some(RealmPackage {
            href: "https://cdn.example/a.actr".into(),
            sha256: "ab".repeat(32),
        });
        request
    }

    #[test]
    fn signed_request_round_trips_and_verifies() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let document = request().sign("rs-1", &key);
        assert_eq!(document["op"], "create_realm");
        let parsed = ManagementRequest::parse(document.to_string().as_bytes()).unwrap();
        parsed
            .verify(&keys(&[("rs-1", &key, KeyStatus::Active)]))
            .unwrap();
        parsed.check_window(Utc::now()).unwrap();

        // A retired key no longer signs requests.
        assert!(
            parsed
                .verify(&keys(&[("rs-1", &key, KeyStatus::Retired)]))
                .is_err()
        );
        // Another key, or a changed member, fails.
        let other = SigningKey::from_bytes(&[2; 32]);
        assert!(
            parsed
                .verify(&keys(&[("rs-1", &other, KeyStatus::Active)]))
                .is_err()
        );
        let mut tampered = document.clone();
        tampered["provider_id"] = "other.example".into();
        let tampered = ManagementRequest::parse(tampered.to_string().as_bytes()).unwrap();
        assert!(
            tampered
                .verify(&keys(&[("rs-1", &key, KeyStatus::Active)]))
                .is_err()
        );
    }

    #[test]
    fn rejects_unsigned_requests_and_bad_windows() {
        let unsigned = serde_json::to_vec(&request()).unwrap();
        assert!(ManagementRequest::parse(&unsigned).is_err());

        let mut long = request();
        long.expires_at = "2100-01-01T00:00:00Z".into();
        assert!(long.check_window(Utc::now()).is_err());
        let mut expired = request();
        expired.issued_at = "2020-01-01T00:00:00Z".into();
        expired.expires_at = "2020-01-01T00:05:00Z".into();
        assert!(expired.check_window(Utc::now()).is_err());
        assert!(
            request()
                .check_window(Utc::now() - chrono::Duration::seconds(60))
                .is_err()
        );
    }
}
