//! HTTPS publisher identity discovery.

use crate::{PublisherRegistryError, crypto, reserved::canonical_publisher_domain};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub const PUBLISHER_DESCRIPTOR_SCHEMA: &str = "actr.publisher/v1";
pub const PUBLISHER_DESCRIPTOR_PATH: &str = "/.well-known/actr/publisher.json";
const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;

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

fn is_forbidden_discovery_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (18..=19).contains(&b))
        }
        IpAddr::V6(ip) => {
            // IPv4-mapped/compatible addresses connect to IPv4 destinations at
            // the socket layer, so they must be judged by the IPv4 rules;
            // otherwise ::ffff:169.254.169.254 etc. would bypass the blacklist.
            if let Some(v4) = ip.to_ipv4_mapped().or_else(|| ip.to_ipv4()) {
                return is_forbidden_discovery_address(IpAddr::V4(v4));
            }
            let seg = ip.segments();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || seg[0] == 0x2002 // 6to4 (2002::/16)
                || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64 well-known 64:ff9b::/96
        }
    }
}

/// Fetch a descriptor without redirects and pin the request to a preflighted
/// public DNS answer. This prevents the verification endpoint becoming a
/// generic SSRF primitive and closes the usual DNS-rebinding gap.
pub async fn fetch_publisher_descriptor(
    publisher: &str,
) -> Result<(PublisherDescriptor, String), PublisherRegistryError> {
    let publisher = canonical_publisher_domain(publisher)?;
    let mut addresses = tokio::net::lookup_host((publisher.as_str(), 443))
        .await
        .map_err(|error| {
            PublisherRegistryError::PublisherDiscovery(format!("DNS lookup failed: {error}"))
        })?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(PublisherRegistryError::PublisherDiscovery(
            "publisher domain has no address records".to_string(),
        ));
    }
    if addresses
        .iter()
        .any(|address| is_forbidden_discovery_address(address.ip()))
    {
        return Err(PublisherRegistryError::PublisherDiscovery(
            "publisher domain resolves to a non-public address".to_string(),
        ));
    }

    let pinned = SocketAddr::new(addresses[0].ip(), 443);
    // Bound every phase of the request. Without these an unresponsive or
    // slow-loris endpoint would hang the fetch indefinitely. `.no_proxy()`
    // stops an ambient HTTPS_PROXY/ALL_PROXY from silently tunnelling the
    // request through a proxy, which would bypass the pinned `.resolve()` DNS
    // answer and the private-address preflight above (SSRF re-entry).
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&publisher, pinned)
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| PublisherRegistryError::PublisherDiscovery(error.to_string()))?;
    let descriptor_url = format!("https://{publisher}{PUBLISHER_DESCRIPTOR_PATH}");
    let mut response = client
        .get(&descriptor_url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|error| {
            PublisherRegistryError::PublisherDiscovery(format!("HTTPS fetch failed: {error}"))
        })?;
    if !response.status().is_success() {
        return Err(PublisherRegistryError::PublisherDiscovery(format!(
            "descriptor endpoint returned HTTP {}",
            response.status()
        )));
    }
    // Require a JSON content type. A wrong type usually means the pinned host is
    // not actually serving a descriptor, so reject before buffering the body.
    let is_json = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("application/json")
        });
    if !is_json {
        return Err(PublisherRegistryError::PublisherDiscovery(
            "publisher descriptor must be served as application/json".to_string(),
        ));
    }
    // A declared Content-Length lets us reject oversize bodies early, but it is
    // advisory: chunked responses omit it, so the streaming loop below is the
    // real enforcement point.
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DESCRIPTOR_BYTES as u64)
    {
        return Err(PublisherRegistryError::PublisherDiscovery(
            "publisher descriptor exceeds 64 KiB".to_string(),
        ));
    }
    // Stream the body chunk by chunk and abort the moment the accumulated size
    // exceeds the cap. Buffering the whole response first (`.bytes()`) would let
    // a chunked reply with no Content-Length grow without bound (OOM).
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| PublisherRegistryError::PublisherDiscovery(error.to_string()))?
    {
        if bytes.len() + chunk.len() > MAX_DESCRIPTOR_BYTES {
            return Err(PublisherRegistryError::PublisherDiscovery(
                "publisher descriptor exceeds 64 KiB".to_string(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
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

    #[test]
    fn rejects_private_discovery_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            // IPv4-mapped / compatible IPv6 that reach forbidden IPv4 targets.
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            "::7f00:1",
            // 6to4 and NAT64 well-known prefixes.
            "2002::1",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(
                is_forbidden_discovery_address(ip.parse().unwrap()),
                "{ip} should be forbidden"
            );
        }
        assert!(!is_forbidden_discovery_address("1.1.1.1".parse().unwrap()));
        // A genuine global IPv6 address must still be allowed.
        assert!(!is_forbidden_discovery_address(
            "2606:4700:4700::1111".parse().unwrap()
        ));
    }
}
