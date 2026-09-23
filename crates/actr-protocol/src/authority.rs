//! Public authority discovery data types.

use crate::uri::ActrUri;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const AUTHORITY_DESCRIPTOR_VERSION: u32 = 1;
pub const ICE_CONFIG_PROTOCOL: &str = "actr-ice-config/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityDescriptor {
    pub version: u32,
    pub authority: String,
    pub services: Vec<ServiceLink>,
    /// Path prefixes under `https://<domain>` that are Realm links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,
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

/// Parses an `authority` member, which must be `actr://<domain>` with no
/// port, path, trailing slash, query, fragment, or user information.
pub fn parse_authority_member(value: &str) -> Result<ActrUri, AuthorityDescriptorError> {
    let invalid = |reason: &str| AuthorityDescriptorError::InvalidUri(format!("{value}: {reason}"));
    let domain = value
        .strip_prefix("actr://")
        .ok_or_else(|| invalid("expected actr://<domain>"))?;
    if domain.is_empty() || domain.contains(['@', '/', '?', '#', ':']) {
        return Err(invalid("expected actr://<domain> without port or path"));
    }
    value
        .parse::<ActrUri>()
        .map_err(|error| invalid(&error.to_string()))
}

/// A Realm link prefix starts and ends with `/` and has at least one segment.
fn is_link_prefix(value: &str) -> bool {
    value.len() > 1
        && value.starts_with('/')
        && value.ends_with('/')
        && value[1..value.len() - 1]
            .split('/')
            .all(|segment| !segment.is_empty() && !segment.contains(['?', '#']))
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
            "realm_signing_keys": { "keys": [] },
            "publisher": { "name": "Example" },
            "x-future": 1
        }))
        .unwrap();
        let uri = "actr://example.com".parse::<ActrUri>().unwrap();
        assert_eq!(parsed.validate_for(&uri), Ok(()));
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
