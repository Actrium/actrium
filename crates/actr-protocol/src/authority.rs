//! Public authority discovery data types.

use crate::uri::{ActrUri, is_link_prefix};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const AUTHORITY_DESCRIPTOR_VERSION: u32 = 1;
pub const ICE_CONFIG_PROTOCOL: &str = "actr-ice-config/1";
pub const PUBLISHER_PROFILE_SCHEMA: &str = "actr.publisher/v1";
pub const SIGNATURE_ALGORITHM: &str = "Ed25519";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityDescriptor {
    pub version: u32,
    pub authority: String,
    pub services: Vec<ServiceLink>,
    /// Path prefixes under `https://<domain>` that are Realm links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,
    /// Realm signing keys for Realms named `<realm_id>@<domain>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm_signing_keys: Option<RealmSigningKeys>,
    /// Signed Publisher Profile of this authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<PublisherProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmSigningKeys {
    pub sequence: u64,
    pub keys: Vec<RealmSigningKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotations: Option<Vec<KeyRotation>>,
    pub signature: DetachedSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmSigningKey {
    pub id: String,
    pub algorithm: String,
    /// Base64 Ed25519 public key.
    pub public_key: String,
    pub status: KeyStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStatus {
    Active,
    Retired,
}

/// Signed by the `from` key over the JCS form of
/// `{from, to, public_key, sequence}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRotation {
    pub from: String,
    pub to: String,
    pub sequence: u64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetachedSignature {
    pub key_id: String,
    pub algorithm: String,
    pub value: String,
}

/// Publisher Profile carried inline. Its full format belongs to the Actor
/// Package specification; only the members checked here are modeled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublisherProfile {
    pub schema: String,
    pub domain: String,
    pub signature: DetachedSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLink {
    pub rel: String,
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

impl ServiceLink {
    pub fn new(rel: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            rel: rel.into(),
            href: href.into(),
            protocol: None,
        }
    }

    pub fn with_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AuthorityDescriptorError {
    #[error("unsupported authority descriptor version {0}")]
    UnsupportedVersion(u32),
    #[error("authority descriptor contains no services")]
    MissingServices,
    #[error("authority descriptor is bound to '{actual}', expected '{expected}'")]
    AuthorityMismatch { expected: String, actual: String },
    #[error("invalid authority descriptor URI: {0}")]
    InvalidUri(String),
    #[error("unsupported service relation '{0}'")]
    InvalidRelation(String),
    #[error("required service relation '{0}' is missing")]
    MissingService(String),
    #[error("invalid Realm link prefix '{0}'")]
    InvalidLink(String),
    #[error("invalid realm_signing_keys: {0}")]
    InvalidRealmSigningKeys(String),
    #[error("invalid publisher: {0}")]
    InvalidPublisher(String),
}

const CORE_RELATIONS: &[&str] = &[
    "signaling",
    "identity",
    "discovery",
    "ice-config",
    "contracts",
    "realms",
    "realm-management",
];

fn is_absolute_uri(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && !remainder.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
        })
}

fn decodes_to(value: &str, len: usize) -> bool {
    BASE64.decode(value).is_ok_and(|bytes| bytes.len() == len)
}

fn check_signature(signature: &DetachedSignature) -> Result<(), String> {
    if signature.key_id.is_empty() {
        return Err("signature key_id is empty".into());
    }
    if signature.algorithm != SIGNATURE_ALGORITHM {
        return Err(format!(
            "unsupported signature algorithm '{}'",
            signature.algorithm
        ));
    }
    if !decodes_to(&signature.value, 64) {
        return Err("signature value is not a base64 Ed25519 signature".into());
    }
    Ok(())
}

impl RealmSigningKeys {
    /// Structural checks only; signature verification and key pinning are
    /// left to the caller.
    pub fn validate(&self) -> Result<(), AuthorityDescriptorError> {
        self.check()
            .map_err(AuthorityDescriptorError::InvalidRealmSigningKeys)
    }

    fn check(&self) -> Result<(), String> {
        if self.keys.is_empty() {
            return Err("keys is empty".into());
        }
        for (index, key) in self.keys.iter().enumerate() {
            if key.id.is_empty() {
                return Err("key id is empty".into());
            }
            if self.keys[..index].iter().any(|other| other.id == key.id) {
                return Err(format!("duplicate key id '{}'", key.id));
            }
            if key.algorithm != SIGNATURE_ALGORITHM {
                return Err(format!(
                    "key '{}' has unsupported algorithm '{}'",
                    key.id, key.algorithm
                ));
            }
            if !decodes_to(&key.public_key, 32) {
                return Err(format!(
                    "key '{}' public_key is not a base64 Ed25519 key",
                    key.id
                ));
            }
        }
        if !self.keys.iter().any(|key| key.status == KeyStatus::Active) {
            return Err("no active key".into());
        }
        check_signature(&self.signature)?;
        if !self
            .keys
            .iter()
            .any(|key| key.id == self.signature.key_id && key.status == KeyStatus::Active)
        {
            return Err(format!(
                "signature key '{}' is not an active key",
                self.signature.key_id
            ));
        }
        for rotation in self.rotations.as_deref().unwrap_or_default() {
            if rotation.from.is_empty() || rotation.to.is_empty() {
                return Err("rotation from/to is empty".into());
            }
            if !decodes_to(&rotation.signature, 64) {
                return Err(format!(
                    "rotation '{}' -> '{}' signature is not a base64 Ed25519 signature",
                    rotation.from, rotation.to
                ));
            }
        }
        Ok(())
    }
}

impl PublisherProfile {
    /// Checks the descriptor-level rules: schema, `domain` equal to the
    /// authority host, and signature shape. The profile signature itself is
    /// verified under the Actor Package specification.
    pub fn validate_for(&self, domain: &str) -> Result<(), AuthorityDescriptorError> {
        let invalid = AuthorityDescriptorError::InvalidPublisher;
        if self.schema != PUBLISHER_PROFILE_SCHEMA {
            return Err(invalid(format!("unsupported schema '{}'", self.schema)));
        }
        if self.domain != domain {
            return Err(invalid(format!(
                "domain '{}' does not match authority host '{domain}'",
                self.domain
            )));
        }
        check_signature(&self.signature).map_err(invalid)
    }
}

/// Parses an `authority` member, which must be `actr://<domain>` with no
/// port, path, trailing slash, query, fragment, or user information.
pub fn parse_authority_member(value: &str) -> Result<ActrUri, AuthorityDescriptorError> {
    let invalid = |reason: &str| AuthorityDescriptorError::InvalidUri(format!("{value}: {reason}"));
    let domain = value
        .strip_prefix("actr://")
        .ok_or_else(|| invalid("expected actr://<domain>"))?;
    if domain.contains(['@', '/', '?', '#', ':']) {
        return Err(invalid("expected actr://<domain> without port or path"));
    }
    let uri = ActrUri::from_domain(domain).map_err(|error| invalid(&error.to_string()))?;
    if uri.is_local() {
        return Err(invalid("a local Realm domain has no authority descriptor"));
    }
    Ok(uri)
}

impl AuthorityDescriptor {
    pub fn validate_for(&self, requested: &ActrUri) -> Result<(), AuthorityDescriptorError> {
        if self.version != AUTHORITY_DESCRIPTOR_VERSION {
            return Err(AuthorityDescriptorError::UnsupportedVersion(self.version));
        }
        if self.services.is_empty() {
            return Err(AuthorityDescriptorError::MissingServices);
        }

        let described = parse_authority_member(&self.authority)?;
        if described.authority_uri() != requested.authority_uri() {
            return Err(AuthorityDescriptorError::AuthorityMismatch {
                expected: requested.authority_uri(),
                actual: described.authority_uri(),
            });
        }

        for service in &self.services {
            if !is_absolute_uri(&service.href) {
                return Err(AuthorityDescriptorError::InvalidUri(service.href.clone()));
            }
            if !CORE_RELATIONS.contains(&service.rel.as_str()) && !is_absolute_uri(&service.rel) {
                return Err(AuthorityDescriptorError::InvalidRelation(
                    service.rel.clone(),
                ));
            }
        }

        let links = self.links.as_deref().unwrap_or_default();
        for (index, link) in links.iter().enumerate() {
            if !is_link_prefix(link) || links[..index].contains(link) {
                return Err(AuthorityDescriptorError::InvalidLink(link.clone()));
            }
        }

        if let Some(keys) = &self.realm_signing_keys {
            keys.validate()?;
        }
        if let Some(publisher) = &self.publisher {
            publisher.validate_for(described.domain())?;
        }
        Ok(())
    }

    pub fn service(&self, relation: &str) -> Option<&ServiceLink> {
        self.services.iter().find(|service| service.rel == relation)
    }

    pub fn require_service(
        &self,
        relation: &str,
    ) -> Result<&ServiceLink, AuthorityDescriptorError> {
        self.service(relation)
            .ok_or_else(|| AuthorityDescriptorError::MissingService(relation.to_string()))
    }
}

/// Public, non-credential ICE information. Authorization-bearing deployments
/// can return short-lived credentials from the same endpoint after auth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceConfiguration {
    pub version: u32,
    pub ice_servers: Vec<IceServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(authority: &str) -> AuthorityDescriptor {
        AuthorityDescriptor {
            version: 1,
            authority: authority.to_string(),
            services: vec![
                ServiceLink::new("signaling", "wss://signal.example/actr")
                    .with_protocol("actr-signaling/1"),
            ],
            links: None,
            realm_signing_keys: None,
            publisher: None,
        }
    }

    fn b64(len: usize) -> String {
        BASE64.encode(vec![7u8; len])
    }

    fn signature(key_id: &str) -> DetachedSignature {
        DetachedSignature {
            key_id: key_id.to_string(),
            algorithm: "Ed25519".to_string(),
            value: b64(64),
        }
    }

    fn signing_key(id: &str, status: KeyStatus) -> RealmSigningKey {
        RealmSigningKey {
            id: id.to_string(),
            algorithm: "Ed25519".to_string(),
            public_key: b64(32),
            status,
        }
    }

    fn signing_keys() -> RealmSigningKeys {
        RealmSigningKeys {
            sequence: 3,
            keys: vec![
                signing_key("rs-2026-1", KeyStatus::Active),
                signing_key("rs-2025-1", KeyStatus::Retired),
            ],
            rotations: Some(vec![KeyRotation {
                from: "rs-2025-1".to_string(),
                to: "rs-2026-1".to_string(),
                sequence: 2,
                signature: b64(64),
            }]),
            signature: signature("rs-2026-1"),
        }
    }

    fn publisher(domain: &str) -> PublisherProfile {
        PublisherProfile {
            schema: "actr.publisher/v1".to_string(),
            domain: domain.to_string(),
            signature: signature("pub-1"),
        }
    }

    #[test]
    fn validates_exact_authority_binding() {
        let uri = "actr://example.com/echo".parse::<ActrUri>().unwrap();
        descriptor("actr://example.com").validate_for(&uri).unwrap();

        let error = descriptor("actr://other.example")
            .validate_for(&uri)
            .unwrap_err();
        assert!(matches!(
            error,
            AuthorityDescriptorError::AuthorityMismatch { .. }
        ));
    }

    #[test]
    fn finds_service_by_relation() {
        let descriptor = descriptor("actr://example.com");
        let service = descriptor.require_service("signaling").unwrap();
        assert_eq!(service.protocol.as_deref(), Some("actr-signaling/1"));
        assert!(matches!(
            descriptor.require_service("contracts"),
            Err(AuthorityDescriptorError::MissingService(_))
        ));
    }

    #[test]
    fn rejects_retired_packages_relation() {
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        let mut descriptor = descriptor("actr://example.com");
        descriptor.services.push(ServiceLink::new(
            "packages",
            "https://packages.example/actr",
        ));
        assert!(matches!(
            descriptor.validate_for(&uri),
            Err(AuthorityDescriptorError::InvalidRelation(_))
        ));
    }

    #[test]
    fn rejects_noncanonical_authority_member() {
        let uri = "actr://example.com/echo".parse::<ActrUri>().unwrap();
        for authority in [
            "actr://example.com/not-an-authority",
            "actr://example.com/",
            "actr://example.com:443",
            "actr://example.com:8443",
            "actr://user@example.com",
            "actr://abcdefghijklmnopqrstuvwxyz.actr.alt",
            "actr://",
            "https://example.com",
        ] {
            let error = descriptor(authority).validate_for(&uri).unwrap_err();
            assert!(
                matches!(error, AuthorityDescriptorError::InvalidUri(_)),
                "{authority}: {error:?}"
            );
        }
    }

    #[test]
    fn validates_realm_link_prefixes() {
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        let mut descriptor = descriptor("actr://example.com");
        descriptor.links = Some(vec!["/i/".to_string(), "/apps/realms/".to_string()]);
        descriptor.validate_for(&uri).unwrap();

        for link in ["/", "i/", "/i", "//", "/i//", "/i?x/", "/i#x/"] {
            descriptor.links = Some(vec![link.to_string()]);
            assert!(
                matches!(
                    descriptor.validate_for(&uri),
                    Err(AuthorityDescriptorError::InvalidLink(_))
                ),
                "{link}"
            );
        }

        descriptor.links = Some(vec!["/i/".to_string(), "/i/".to_string()]);
        assert!(matches!(
            descriptor.validate_for(&uri),
            Err(AuthorityDescriptorError::InvalidLink(_))
        ));
    }

    #[test]
    fn accepts_realm_relations() {
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        let mut descriptor = descriptor("actr://example.com");
        descriptor.services.push(
            ServiceLink::new("realms", "https://realms.example/actr")
                .with_protocol("actr-realm-profile/1"),
        );
        descriptor.services.push(
            ServiceLink::new("realm-management", "https://realms.example/actr/manage")
                .with_protocol("actr-realm-management/1"),
        );
        descriptor.validate_for(&uri).unwrap();
    }

    #[test]
    fn ignores_unknown_members() {
        let parsed: AuthorityDescriptor = serde_json::from_value(serde_json::json!({
            "version": 1,
            "authority": "actr://example.com",
            "services": [{
                "rel": "signaling",
                "href": "wss://signal.example/actr",
                "x-extra": true
            }],
            "realm_signing_keys": {
                "sequence": 3,
                "keys": [{
                    "id": "rs-2026-1",
                    "algorithm": "Ed25519",
                    "public_key": b64(32),
                    "status": "active",
                    "x-extra": true
                }],
                "signature": { "key_id": "rs-2026-1", "algorithm": "Ed25519", "value": b64(64) },
                "x-extra": true
            },
            "publisher": {
                "schema": "actr.publisher/v1",
                "domain": "example.com",
                "name": "Example",
                "signature": { "key_id": "pub-1", "algorithm": "Ed25519", "value": b64(64) }
            },
            "x-future": 1
        }))
        .unwrap();
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        assert_eq!(parsed.validate_for(&uri), Ok(()));
        assert_eq!(parsed.publisher.unwrap().domain, "example.com");
    }

    #[test]
    fn validates_realm_signing_keys() {
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        let mut descriptor = descriptor("actr://example.com");
        descriptor.realm_signing_keys = Some(signing_keys());
        descriptor.validate_for(&uri).unwrap();

        let cases: Vec<fn(&mut RealmSigningKeys)> = vec![
            |keys| keys.keys.clear(),
            |keys| keys.keys[1].id = "rs-2026-1".to_string(),
            |keys| keys.keys[0].id.clear(),
            |keys| keys.keys[0].algorithm = "RS256".to_string(),
            |keys| keys.keys[0].public_key = b64(31),
            |keys| keys.keys[0].public_key = "not base64!".to_string(),
            |keys| keys.keys[0].status = KeyStatus::Retired,
            |keys| keys.signature.key_id = "rs-2025-1".to_string(),
            |keys| keys.signature.key_id = "unknown".to_string(),
            |keys| keys.signature.algorithm = "RS256".to_string(),
            |keys| keys.signature.value = b64(32),
            |keys| keys.rotations.as_mut().unwrap()[0].to.clear(),
            |keys| keys.rotations.as_mut().unwrap()[0].signature = String::new(),
        ];
        for (index, mutate) in cases.into_iter().enumerate() {
            let mut keys = signing_keys();
            mutate(&mut keys);
            descriptor.realm_signing_keys = Some(keys);
            assert!(
                matches!(
                    descriptor.validate_for(&uri),
                    Err(AuthorityDescriptorError::InvalidRealmSigningKeys(_))
                ),
                "case {index}"
            );
        }
    }

    #[test]
    fn rotation_from_unknown_key_is_accepted() {
        let mut keys = signing_keys();
        keys.rotations.as_mut().unwrap()[0].from = "rs-2019-1".to_string();
        keys.validate().unwrap();
    }

    #[test]
    fn realm_signing_keys_reject_negative_sequence() {
        let mut json = serde_json::to_value(signing_keys()).unwrap();
        json["sequence"] = serde_json::json!(-1);
        assert!(serde_json::from_value::<RealmSigningKeys>(json).is_err());
    }

    #[test]
    fn publisher_domain_must_equal_authority_host() {
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        let mut descriptor = descriptor("actr://example.com");
        descriptor.publisher = Some(publisher("example.com"));
        descriptor.validate_for(&uri).unwrap();

        for profile in [
            publisher("other.example"),
            publisher("sub.example.com"),
            publisher(""),
            PublisherProfile {
                schema: "actr.publisher/v2".to_string(),
                ..publisher("example.com")
            },
            PublisherProfile {
                signature: DetachedSignature {
                    value: "x".to_string(),
                    ..signature("pub-1")
                },
                ..publisher("example.com")
            },
        ] {
            descriptor.publisher = Some(profile);
            assert!(matches!(
                descriptor.validate_for(&uri),
                Err(AuthorityDescriptorError::InvalidPublisher(_))
            ));
        }
    }

    #[test]
    fn signed_members_are_optional_on_the_wire() {
        let json = serde_json::to_value(descriptor("actr://example.com")).unwrap();
        assert!(json.get("realm_signing_keys").is_none());
        assert!(json.get("publisher").is_none());

        let mut with_keys = descriptor("actr://example.com");
        with_keys.realm_signing_keys = Some(signing_keys());
        let json = serde_json::to_value(&with_keys).unwrap();
        assert_eq!(json["realm_signing_keys"]["keys"][0]["status"], "active");
        let parsed: AuthorityDescriptor = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, with_keys);
    }

    #[test]
    fn links_member_is_optional_on_the_wire() {
        let json = serde_json::to_value(descriptor("actr://example.com")).unwrap();
        assert!(json.get("links").is_none());

        let parsed: AuthorityDescriptor = serde_json::from_value(serde_json::json!({
            "version": 1,
            "authority": "actr://example.com",
            "services": [{ "rel": "signaling", "href": "wss://signal.example/actr" }],
            "links": ["/i/"]
        }))
        .unwrap();
        assert_eq!(parsed.links.as_deref(), Some(&["/i/".to_string()][..]));
    }
}
