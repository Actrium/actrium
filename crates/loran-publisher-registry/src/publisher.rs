//! HTTPS publisher identity discovery.

use crate::{PublisherRegistryError, crypto, reserved::canonical_publisher_domain};
use serde::{Deserialize, Serialize};

pub const PUBLISHER_DESCRIPTOR_SCHEMA: &str = "actr.publisher/v1";
pub const PUBLISHER_DESCRIPTOR_PATH: &str = "/.well-known/actr/publisher.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublisherDescriptor {
    pub schema: String,
    pub publisher: String,
    pub key_id: String,
    /// Base64-encoded Ed25519 verifying key.
    pub public_key: String,
    /// HTTPS registry endpoints that resolve `publisher/program@version`.
    #[serde(default)]
    pub registries: Vec<String>,
    /// Permitted HTTPS package origins/CDNs.
    #[serde(default)]
    pub origins: Vec<String>,
    /// Short-lived ownership proof issued by this Actrix instance.
    pub verification_challenge: String,
}

impl PublisherDescriptor {
    pub fn validate_for(
        &self,
        expected_publisher: &str,
        expected_challenge: &str,
    ) -> Result<(), PublisherRegistryError> {
        if self.schema != PUBLISHER_DESCRIPTOR_SCHEMA {
            return Err(PublisherRegistryError::VerificationFailed(format!(
                "unsupported publisher descriptor schema '{}'",
                self.schema
            )));
        }
        let publisher = canonical_publisher_domain(&self.publisher)?;
        if publisher != expected_publisher {
            return Err(PublisherRegistryError::VerificationFailed(
                "descriptor publisher does not match the requested domain".to_string(),
            ));
        }
        if self.verification_challenge != expected_challenge {
            return Err(PublisherRegistryError::VerificationFailed(
                "descriptor challenge does not match the active challenge".to_string(),
            ));
        }
        if self.key_id.trim().is_empty() {
            return Err(PublisherRegistryError::VerificationFailed(
                "descriptor key_id must not be empty".to_string(),
            ));
        }
        crypto::validate_public_key(&self.public_key)?;
        for endpoint in self.registries.iter().chain(&self.origins) {
            validate_https_endpoint(endpoint)?;
        }
        Ok(())
    }
}

fn validate_https_endpoint(endpoint: &str) -> Result<(), PublisherRegistryError> {
    let url = url::Url::parse(endpoint).map_err(|error| {
        PublisherRegistryError::InvalidRequest(format!("invalid HTTPS endpoint: {error}"))
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(PublisherRegistryError::InvalidRequest(
            "publisher registry/origin must be an HTTPS URL without user info".to_string(),
        ));
    }
    Ok(())
}

/// Fetch a descriptor without redirects through the shared public fetcher,
/// which pins the request to a preflighted public DNS answer. This prevents
/// the verification endpoint becoming a generic SSRF primitive and closes the
/// usual DNS-rebinding gap.
pub async fn fetch_publisher_descriptor(
    publisher: &str,
) -> Result<(PublisherDescriptor, String), PublisherRegistryError> {
    let publisher = canonical_publisher_domain(publisher)?;
    let descriptor_url = format!("https://{publisher}{PUBLISHER_DESCRIPTOR_PATH}");
    let bytes = platform::net::fetch_public_json(&descriptor_url, 0)
        .await
        .map_err(|error| PublisherRegistryError::PublisherDiscovery(error.to_string()))?;
    let descriptor = serde_json::from_slice(&bytes).map_err(|error| {
        PublisherRegistryError::PublisherDiscovery(format!("invalid JSON: {error}"))
    })?;
    Ok((descriptor, descriptor_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use ed25519_dalek::SigningKey;

    fn descriptor() -> PublisherDescriptor {
        PublisherDescriptor {
            schema: PUBLISHER_DESCRIPTOR_SCHEMA.to_string(),
            publisher: "apps.example.com".to_string(),
            key_id: "publisher-2026".to_string(),
            public_key: STANDARD.encode(
                SigningKey::from_bytes(&[7_u8; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
            registries: vec!["https://registry.example.com/v1".to_string()],
            origins: vec!["https://cdn.example.com/packages/".to_string()],
            verification_challenge: "challenge".to_string(),
        }
    }

    #[test]
    fn validates_descriptor_contract() {
        descriptor()
            .validate_for("apps.example.com", "challenge")
            .unwrap();
    }

    #[test]
    fn rejects_descriptor_identity_or_transport_confusion() {
        let mut wrong = descriptor();
        wrong.publisher = "other.example.com".to_string();
        assert!(wrong.validate_for("apps.example.com", "challenge").is_err());

        let mut insecure = descriptor();
        insecure.origins = vec!["http://cdn.example.com".to_string()];
        assert!(
            insecure
                .validate_for("apps.example.com", "challenge")
                .is_err()
        );
    }
}
