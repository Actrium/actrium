//! `actr:` URI parsing and normalization.
//!
//! The public URI identifies an authority and an opaque target. Runtime Actor
//! identities (realm, publisher, type, version, and instance) are resolved
//! later and are deliberately not encoded into fixed URI positions.

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use thiserror::Error;

/// Actor-RTC URI parse error.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActrUriError {
    #[error("invalid URI scheme, expected 'actr' but got '{0}'")]
    InvalidScheme(String),

    #[error("missing Actr authority in URI")]
    MissingAuthority,

    #[error("userinfo is not permitted in an Actr URI")]
    UserInfoNotAllowed,

    #[error("query and fragment components are not permitted in an Actr URI")]
    ExtraComponentNotAllowed,

    #[error("invalid target reference: {0}")]
    InvalidTarget(String),

    #[error("URI parse error: {0}")]
    ParseError(String),
}

/// A normalized public Actr URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActrUri {
    /// Canonical `host[:port]` authority. An explicit default port 443 is removed.
    authority: String,
    /// Opaque normalized target reference without a leading slash.
    target: Option<String>,
}

impl ActrUri {
    /// Construct and validate an Actr URI from its public components.
    pub fn new(authority: impl AsRef<str>, target: Option<&str>) -> Result<Self, ActrUriError> {
        let mut value = format!("actr://{}", authority.as_ref());
        if let Some(target) = target {
            value.push('/');
            value.push_str(target);
        }
        value.parse()
    }

    pub fn scheme(&self) -> &'static str {
        "actr"
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Canonical authority-only URI used to bind an authority descriptor.
    pub fn authority_uri(&self) -> String {
        format!("actr://{}", self.authority)
    }

    /// HTTPS discovery URL for this authority.
    pub fn discovery_url(&self) -> String {
        format!("https://{}/.well-known/actr", self.authority)
    }
}

impl Display for ActrUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "actr://{}", self.authority)?;
        if let Some(target) = &self.target {
            write!(f, "/{target}")?;
        }
        Ok(())
    }
}

fn is_unreserved(octet: u8) -> bool {
    octet.is_ascii_alphanumeric() || matches!(octet, b'-' | b'.' | b'_' | b'~')
}

fn is_target_ascii(octet: u8) -> bool {
    is_unreserved(octet)
        || matches!(
            octet,
            b'!' | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

fn normalize_raw_target(value: &str) -> Result<String, ActrUriError> {
    if value.is_empty() {
        return Ok(String::new());
    }

    let mut normalized_segments = Vec::new();
    for segment in value.split('/') {
        if segment.is_empty() {
            return Err(ActrUriError::InvalidTarget(
                "empty path segments are not permitted".to_string(),
            ));
        }

        let mut decoded = Vec::with_capacity(segment.len());
        let mut normalized = String::with_capacity(segment.len());
        let bytes = segment.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' {
                if index + 2 >= bytes.len() {
                    return Err(ActrUriError::InvalidTarget(
                        "incomplete percent-encoding".to_string(),
                    ));
                }
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|_| {
                    ActrUriError::InvalidTarget("invalid percent-encoding".to_string())
                })?;
                let octet = u8::from_str_radix(hex, 16).map_err(|_| {
                    ActrUriError::InvalidTarget("invalid percent-encoding".to_string())
                })?;
                decoded.push(octet);
                if is_unreserved(octet) {
                    normalized.push(char::from(octet));
                } else {
                    use std::fmt::Write as _;
                    write!(&mut normalized, "%{octet:02X}")
                        .expect("writing to a String cannot fail");
                }
                index += 3;
            } else {
                if !bytes[index].is_ascii() || !is_target_ascii(bytes[index]) {
                    return Err(ActrUriError::InvalidTarget(
                        "characters outside the target grammar must be percent-encoded".to_string(),
                    ));
                }
                decoded.push(bytes[index]);
                normalized.push(char::from(bytes[index]));
                index += 1;
            }
        }

        if decoded == b"." || decoded == b".." {
            return Err(ActrUriError::InvalidTarget(
                "dot segments are not permitted".to_string(),
            ));
        }
        normalized_segments.push(normalized);
    }
    Ok(normalized_segments.join("/"))
}

fn normalize_port(raw: &str) -> Result<Option<u16>, ActrUriError> {
    if raw.is_empty() {
        return Err(ActrUriError::ParseError("empty port".to_string()));
    }
    let port = raw
        .parse::<u16>()
        .map_err(|_| ActrUriError::ParseError(format!("invalid port '{raw}'")))?;
    Ok((port != 443).then_some(port))
}

fn normalize_authority(raw: &str) -> Result<String, ActrUriError> {
    if raw.is_empty() {
        return Err(ActrUriError::MissingAuthority);
    }
    if raw.contains('@') {
        return Err(ActrUriError::UserInfoNotAllowed);
    }
    if !raw.is_ascii()
        || raw
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(ActrUriError::ParseError(
            "authority must use its ASCII form".to_string(),
        ));
    }

    if let Some(bracketed) = raw.strip_prefix('[') {
        let close = bracketed
            .find(']')
            .ok_or_else(|| ActrUriError::ParseError("unterminated IPv6 literal".to_string()))?;
        let address = bracketed[..close]
            .parse::<Ipv6Addr>()
            .map_err(|error| ActrUriError::ParseError(error.to_string()))?;
        let suffix = &bracketed[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            normalize_port(suffix.strip_prefix(':').ok_or_else(|| {
                ActrUriError::ParseError("invalid text after IPv6 literal".to_string())
            })?)?
        };
        return Ok(port.map_or_else(
            || format!("[{address}]"),
            |port| format!("[{address}]:{port}"),
        ));
    }

    let (host, port) = match raw.rsplit_once(':') {
        Some((host, port)) => (host, normalize_port(port)?),
        None => (raw, None),
    };
    if host.is_empty() || host.contains(':') {
        return Err(ActrUriError::ParseError("invalid host".to_string()));
    }

    let host = if let Ok(address) = host.parse::<Ipv4Addr>() {
        address.to_string()
    } else {
        if host
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return Err(ActrUriError::ParseError("invalid IPv4 address".to_string()));
        }
        if host.len() > 253
            || host.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err(ActrUriError::ParseError("invalid DNS host".to_string()));
        }
        host.to_ascii_lowercase()
    };

    Ok(port.map_or_else(|| host.clone(), |port| format!("{host}:{port}")))
}

impl FromStr for ActrUri {
    type Err = ActrUriError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (scheme, remainder) = input.split_once("://").ok_or_else(|| {
            ActrUriError::InvalidScheme(input.split(':').next().unwrap_or_default().to_string())
        })?;
        if !scheme.eq_ignore_ascii_case("actr") {
            return Err(ActrUriError::InvalidScheme(scheme.to_string()));
        }
        if remainder.contains('?') || remainder.contains('#') {
            return Err(ActrUriError::ExtraComponentNotAllowed);
        }
        let (raw_authority, raw_target) = remainder
            .split_once('/')
            .map_or((remainder, ""), |(authority, target)| (authority, target));
        let authority = normalize_authority(raw_authority)?;
        let target = normalize_raw_target(raw_target)?;

        let normalized_target = if target.is_empty() {
            None
        } else {
            Some(target)
        };

        Ok(Self {
            authority,
            target: normalized_target,
        })
    }
}

/// Builder for a public Actr URI.
#[derive(Debug, Default)]
pub struct ActrUriBuilder {
    authority: Option<String>,
    target: Option<String>,
}

impl ActrUriBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authority(mut self, authority: impl Into<String>) -> Self {
        self.authority = Some(authority.into());
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn build(self) -> Result<ActrUri, ActrUriError> {
        let authority = self.authority.ok_or(ActrUriError::MissingAuthority)?;
        ActrUri::new(authority, self.target.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_authority_and_opaque_target() {
        let uri = "actr://Example.COM:8443/apps/echo%2Fstable"
            .parse::<ActrUri>()
            .unwrap();
        assert_eq!(uri.authority(), "example.com:8443");
        assert_eq!(uri.target(), Some("apps/echo%2Fstable"));
        assert_eq!(
            uri.to_string(),
            "actr://example.com:8443/apps/echo%2Fstable"
        );
    }

    #[test]
    fn authority_uri_has_no_target() {
        let uri = "actr://example.com/echo".parse::<ActrUri>().unwrap();
        assert_eq!(uri.authority_uri(), "actr://example.com");
        assert_eq!(uri.discovery_url(), "https://example.com/.well-known/actr");
    }

    #[test]
    fn strips_default_discovery_port() {
        let uri = "ACTR://EXAMPLE.COM:443/echo".parse::<ActrUri>().unwrap();
        assert_eq!(uri.to_string(), "actr://example.com/echo");
    }

    #[test]
    fn normalizes_ip_authorities() {
        let uri = "actr://[2001:0db8:0:0:0:0:0:1]:8443/echo"
            .parse::<ActrUri>()
            .unwrap();
        assert_eq!(uri.authority(), "[2001:db8::1]:8443");
        assert!("actr://999.1.1.1/echo".parse::<ActrUri>().is_err());
        assert!("actr://example..com/echo".parse::<ActrUri>().is_err());
    }

    #[test]
    fn builder_uses_public_components() {
        let uri = ActrUriBuilder::new()
            .authority("example.com")
            .target("echo")
            .build()
            .unwrap();
        assert_eq!(uri.to_string(), "actr://example.com/echo");
    }

    #[test]
    fn rejects_internal_identity_shaped_authority() {
        let result = "actr://101:acme+echo@1.0.0".parse::<ActrUri>();
        assert!(result.is_err());
    }

    #[test]
    fn rejects_userinfo_query_fragment_and_invalid_paths() {
        assert!(matches!(
            "actr://user@example.com/echo".parse::<ActrUri>(),
            Err(ActrUriError::UserInfoNotAllowed)
        ));
        assert!(matches!(
            "actr://example.com/echo?token=x".parse::<ActrUri>(),
            Err(ActrUriError::ExtraComponentNotAllowed)
        ));
        assert!("actr://example.com/a//b".parse::<ActrUri>().is_err());
        assert!("actr://example.com/a/%2e%2e/b".parse::<ActrUri>().is_err());
        assert!("actr://example.com/a/%ZZ/b".parse::<ActrUri>().is_err());
    }

    #[test]
    fn preserves_target_case() {
        let uri = "actr://EXAMPLE.com/Apps/Echo".parse::<ActrUri>().unwrap();
        assert_eq!(uri.to_string(), "actr://example.com/Apps/Echo");
    }

    #[test]
    fn normalizes_percent_encoding_without_changing_target_semantics() {
        let uri = "actr://example.com/%65cho/%e4%b8%ad"
            .parse::<ActrUri>()
            .unwrap();
        assert_eq!(uri.to_string(), "actr://example.com/echo/%E4%B8%AD");
        assert!(
            "actr://example.com/echo/\u{4e2d}"
                .parse::<ActrUri>()
                .is_err()
        );
    }
}
