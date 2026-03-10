/// Names that are pre-reserved and do not require registration.
/// - `self`: special syntax referring to the actor itself
/// - `acme`: sandbox/test namespace, available without registration
/// - `actrix`: platform-reserved
const RESERVED: &[&str] = &["self", "acme", "actrix"];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name.to_ascii_lowercase().as_str())
}

/// Validate a GitHub login as manufacturer name.
///
/// GitHub usernames: 1-39 chars, alphanumeric or hyphens, cannot start/end with hyphen,
/// no consecutive hyphens. We lower-case for storage.
pub fn validate_github_login(login: &str) -> Result<(), crate::MfrError> {
    let lower = login.to_ascii_lowercase();

    if is_reserved(&lower) {
        return Err(crate::MfrError::ReservedName(lower));
    }
    if lower.is_empty() || lower.len() > 39 {
        return Err(crate::MfrError::InvalidName(
            "name must be 1-39 characters".to_string(),
        ));
    }
    if lower.starts_with('-') || lower.ends_with('-') {
        return Err(crate::MfrError::InvalidName(
            "name must not start or end with a hyphen".to_string(),
        ));
    }
    if lower.contains("--") {
        return Err(crate::MfrError::InvalidName(
            "name must not contain consecutive hyphens".to_string(),
        ));
    }
    if !lower
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(crate::MfrError::InvalidName(
            "name must only contain alphanumeric characters or hyphens".to_string(),
        ));
    }
    Ok(())
}
