/// Names that are pre-reserved and do not require registration.
/// - `self`: special syntax referring to the actor itself
/// - `acme`: sandbox/test namespace, available without registration
/// - `actrix`: platform-reserved
const RESERVED: &[&str] = &["self", "acme", "actrix"];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name.to_ascii_lowercase().as_str())
}

/// Convert a domain to reverse domain notation.
/// "myco.com" → "com.myco"
/// "sub.example.com" → "com.example.sub"
pub fn domain_to_name(domain: &str) -> String {
    // Strip port if present
    let domain = domain.split(':').next().unwrap_or(domain);
    // Strip trailing dots
    let domain = domain.trim_matches('.');
    let parts: Vec<&str> = domain.split('.').collect();
    parts.iter().rev().cloned().collect::<Vec<_>>().join(".")
}

/// Validate manufacturer name: reverse domain notation (lowercase alphanumeric, hyphens, dots), 3-128 chars.
pub fn validate_name(name: &str) -> Result<(), crate::MfrError> {
    if is_reserved(name) {
        return Err(crate::MfrError::ReservedName(name.to_string()));
    }
    if name.len() < 3 || name.len() > 128 {
        return Err(crate::MfrError::InvalidName(
            "name must be 3-128 characters".to_string(),
        ));
    }
    // Reverse domain: lowercase alphanum, hyphens, dots; must not start/end with dot
    if name.starts_with('.') || name.ends_with('.') {
        return Err(crate::MfrError::InvalidName(
            "name must not start or end with a dot".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
    {
        return Err(crate::MfrError::InvalidName(
            "name must be lowercase alphanumeric, hyphens, or dots (reverse domain format)"
                .to_string(),
        ));
    }
    Ok(())
}
