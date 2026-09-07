//! Canonical publisher identity validation.
//!
//! A publisher ID is a DNS domain, without scheme, user info, path or port.
//! HTTPS is implicit and is the discovery transport, not part of the ID.

use url::Host;

pub fn canonical_publisher_domain(input: &str) -> Result<String, crate::MfrError> {
    let candidate = input.trim().trim_end_matches('.');
    if candidate.is_empty() || candidate.contains(['/', '@', ':']) {
        return Err(crate::MfrError::InvalidName(
            "publisher must be a DNS domain without scheme, port, user info or path".to_string(),
        ));
    }

    let Host::Domain(domain) = Host::parse(candidate).map_err(|error| {
        crate::MfrError::InvalidName(format!("invalid publisher domain: {error}"))
    })?
    else {
        return Err(crate::MfrError::InvalidName(
            "publisher identity must be a DNS domain, not an IP address".to_string(),
        ));
    };

    let domain = domain.to_ascii_lowercase();
    if !domain.contains('.') {
        return Err(crate::MfrError::InvalidName(
            "publisher domain must contain at least one dot".to_string(),
        ));
    }
    if domain.len() > 253 {
        return Err(crate::MfrError::InvalidName(
            "publisher domain exceeds 253 characters".to_string(),
        ));
    }
    if domain.split('.').any(|label| {
        label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return Err(crate::MfrError::InvalidName(
            "publisher contains an invalid DNS label".to_string(),
        ));
    }
    Ok(domain)
}

#[cfg(test)]
mod tests {
    use super::canonical_publisher_domain;

    #[test]
    fn canonicalizes_domain_identity() {
        assert_eq!(
            canonical_publisher_domain("EXAMPLE.COM.").unwrap(),
            "example.com"
        );
        assert_eq!(
            // language-allow: IDN input required to test punycode canonicalization
            canonical_publisher_domain("münchen.example").unwrap(),
            "xn--mnchen-3ya.example"
        );
    }

    #[test]
    fn rejects_non_domain_identity_components() {
        for invalid in [
            "",
            "localhost",
            "https://example.com",
            "example.com:8443",
            "user@example.com",
            "example.com/path",
            "_service.example.com",
            "-bad.example.com",
            "bad-.example.com",
            "two..dots.example.com",
            "127.0.0.1",
        ] {
            assert!(
                canonical_publisher_domain(invalid).is_err(),
                "{invalid} must be rejected"
            );
        }
    }
}
