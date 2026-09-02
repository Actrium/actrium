//! Public authority discovery and package-target resolution data types.

use crate::uri::ActrUri;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const AUTHORITY_DESCRIPTOR_VERSION: u32 = 1;
pub const PACKAGES_PROTOCOL: &str = "actr-packages/1";
pub const ICE_CONFIG_PROTOCOL: &str = "actr-ice-config/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityDescriptor {
    pub version: u32,
    pub authority: String,
    pub services: Vec<ServiceLink>,
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
}

const CORE_RELATIONS: &[&str] = &[
    "signaling",
    "identity",
    "discovery",
    "ice-config",
    "contracts",
    "packages",
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

impl AuthorityDescriptor {
    pub fn validate_for(&self, requested: &ActrUri) -> Result<(), AuthorityDescriptorError> {
        if self.version != AUTHORITY_DESCRIPTOR_VERSION {
            return Err(AuthorityDescriptorError::UnsupportedVersion(self.version));
        }
        if self.services.is_empty() {
            return Err(AuthorityDescriptorError::MissingServices);
        }

        let described = self.authority.parse::<ActrUri>().map_err(|error| {
            AuthorityDescriptorError::InvalidUri(format!("{}: {error}", self.authority))
        })?;
        if described.target().is_some() {
            return Err(AuthorityDescriptorError::InvalidUri(
                "authority member must not contain a target".to_string(),
            ));
        }
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

/// Response from an `actr-packages/1` target resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageResolution {
    pub version: u32,
    pub target: String,
    pub package: ResolvedPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPackage {
    pub href: String,
    pub media_type: String,
    pub sha256: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
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
                ServiceLink::new("packages", "https://packages.example/actr")
                    .with_protocol(PACKAGES_PROTOCOL),
                ServiceLink::new("signaling", "wss://signal.example/actr")
                    .with_protocol("actr-signaling/1"),
            ],
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
    fn finds_package_service_without_interpreting_target() {
        let descriptor = descriptor("actr://example.com");
        let service = descriptor.require_service("packages").unwrap();
        assert_eq!(service.protocol.as_deref(), Some(PACKAGES_PROTOCOL));
        assert_eq!(service.href, "https://packages.example/actr");
    }

    #[test]
    fn rejects_target_in_authority_member() {
        let uri = "actr://example.com/echo".parse::<ActrUri>().unwrap();
        let error = descriptor("actr://example.com/not-an-authority")
            .validate_for(&uri)
            .unwrap_err();
        assert!(matches!(error, AuthorityDescriptorError::InvalidUri(_)));
    }
}
