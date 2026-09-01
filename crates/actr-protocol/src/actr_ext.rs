//! Actor identity formatting and parsing utilities
//!
//! This module provides string formatting and parsing helpers for `ActrType` and `ActrId`.
//! String forms are stable for logging, configuration, and CLI interactions.
//!
//! ## String formats
//!
//! | Type     | Format                                        | Example                      |
//! |----------|-----------------------------------------------|------------------------------|
//! | ActrType | `manufacturer:name:version` | `acme:echo-service:1.0.0` |
//! | ActrId   | `<serial_hex>@<realm_id>/<actr_type>`         | `1a2b3c@101/acme:echo:1.0.0` |

use crate::{ActrId, ActrType, Realm, name::Name};
use std::str::FromStr;
use thiserror::Error;

/// Maximum byte length of the `version` field in an [`ActrType`].
///
/// `version` must be 1–=`MAX_ACTR_VERSION_BYTES` UTF-8 bytes; see
/// [`ActrType::validate`].
pub const MAX_ACTR_VERSION_BYTES: usize = 64;

/// Errors for actor identity parsing and formatting.
///
/// Covers only syntactic/structural validity of `ActrId` and `ActrType` strings.
/// Runtime and RPC errors belong to `ActrIdError` in the `error` module.
#[derive(Error, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActrIdError {
    #[error(
        "Invalid Actor ID format: '{0}'. Expected: <serial_hex>@<realm_id>/<manufacturer>:<name>:<version>"
    )]
    InvalidFormat(String),

    #[error("Invalid component in actor identity: {0}")]
    InvalidComponent(String),

    #[error("Invalid actor type format: '{0}'. Expected: <manufacturer>:<name>:<version>")]
    InvalidTypeFormat(String),
}

impl ActrType {
    /// Convert to stable string representation.
    ///
    /// Always returns `"manufacturer:name:version"`.
    pub fn to_string_repr(&self) -> String {
        debug_assert!(
            !self.version.is_empty(),
            "ActrType.version must be non-empty"
        );

        format!("{}:{}:{}", self.manufacturer, self.name, self.version)
    }

    /// Validate the identity fields against the protocol naming rules.
    ///
    /// `manufacturer`/`name` reuse [`Name`]; `version` uses its own 1–64 byte
    /// ASCII allowlist (see [`validate_version`]). Because [`ActrType`] is a
    /// protobuf type with public fields, callers can construct invalid values
    /// without going through a parser — so every security-sensitive entry point
    /// (package verification, registry lookup, AIS registration, storage
    /// namespace derivation) must call `validate()` before trusting the identity.
    pub fn validate(&self) -> Result<(), ActrIdError> {
        Name::new(self.manufacturer.clone())
            .map_err(|e| ActrIdError::InvalidComponent(format!("Invalid manufacturer: {e}")))?;
        Name::new(self.name.clone())
            .map_err(|e| ActrIdError::InvalidComponent(format!("Invalid type name: {e}")))?;
        validate_version(&self.version)
    }

    /// Parse from string representation.
    ///
    /// Accepts:
    /// - `"manufacturer:name:version"` — with version (required)
    ///
    /// Returns `Err(ActrIdError::InvalidTypeFormat)` if version is absent,
    /// as `ActrType.version` is now a required field.
    pub fn from_string_repr(s: &str) -> Result<Self, ActrIdError> {
        // Require exactly 3 segments: manufacturer, name, version
        let parts: Vec<&str> = s.splitn(4, ':').collect();
        let (manufacturer, name, version) = match parts.as_slice() {
            [_, _] => {
                return Err(ActrIdError::InvalidTypeFormat(format!(
                    "{s} (version is required, expected <manufacturer>:<name>:<version>)"
                )));
            }
            [m, n, v] => (*m, *n, *v),
            _ => return Err(ActrIdError::InvalidTypeFormat(s.to_string())),
        };

        let actr_type = ActrType {
            manufacturer: manufacturer.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        };
        actr_type.validate()?;
        Ok(actr_type)
    }
}

/// Validate the `version` field of an [`ActrType`].
///
/// Rules: 1–=[`MAX_ACTR_VERSION_BYTES`] UTF-8 bytes; first and last characters
/// must be ASCII alphanumeric; interior characters are limited to ASCII
/// alphanumeric plus `.` `_` `-` `+`. This forbids the protocol separators
/// `:` `@` `/` as well as `\`, whitespace, and control characters, so a valid
/// version can never corrupt the `manufacturer:name:version` or
/// `<serial>@<realm>/<type>` string forms.
fn validate_version(version: &str) -> Result<(), ActrIdError> {
    if version.is_empty() {
        return Err(ActrIdError::InvalidComponent(
            "Invalid version: version cannot be empty".to_string(),
        ));
    }
    if version.len() > MAX_ACTR_VERSION_BYTES {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid version: exceeds {MAX_ACTR_VERSION_BYTES} UTF-8 bytes"
        )));
    }

    let first = version.chars().next().expect("checked non-empty");
    let last = version.chars().next_back().expect("checked non-empty");
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return Err(ActrIdError::InvalidComponent(
            "Invalid version: must start and end with an ASCII alphanumeric character".to_string(),
        ));
    }
    if let Some(ch) = version
        .chars()
        .find(|ch| !ch.is_ascii_alphanumeric() && !matches!(ch, '.' | '_' | '-' | '+'))
    {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid version character: {ch:?}"
        )));
    }
    Ok(())
}

impl std::fmt::Display for ActrType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_repr())
    }
}

impl ActrId {
    /// Convert to `"<serial_hex>@<realm_id>/<actr_type>"`.
    ///
    /// The `/` separates the realm ID from the ActrType string, avoiding
    /// ambiguity with the `:` separators used inside ActrType.
    pub fn to_string_repr(&self) -> String {
        format!(
            "{:x}@{}/{}",
            self.serial_number,
            self.realm.realm_id,
            self.r#type.to_string_repr()
        )
    }

    /// Parse from string representation.
    pub fn from_string_repr(s: &str) -> Result<Self, ActrIdError> {
        // Format: "<serial_hex>@<realm_id>/<actr_type>"
        let (serial_part, rest) = s
            .split_once('@')
            .ok_or_else(|| ActrIdError::InvalidFormat("Missing '@' separator".to_string()))?;

        let serial_number = u64::from_str_radix(serial_part, 16).map_err(|_| {
            ActrIdError::InvalidComponent(format!("Invalid serial number hex: {serial_part}"))
        })?;

        let (realm_part, type_part) = rest
            .split_once('/')
            .ok_or_else(|| ActrIdError::InvalidFormat("Missing '/' separator".to_string()))?;

        let realm_id = u32::from_str(realm_part).map_err(|_| {
            ActrIdError::InvalidComponent(format!("Invalid realm ID: {realm_part}"))
        })?;

        let actr_type = ActrType::from_string_repr(type_part)?;

        Ok(ActrId {
            realm: Realm { realm_id },
            serial_number,
            r#type: actr_type,
        })
    }
}

impl std::fmt::Display for ActrId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_repr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_id_roundtrip_with_version() {
        let original = ActrId {
            realm: Realm { realm_id: 101 },
            serial_number: 0x1a2b3c,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "echo-service".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let s = original.to_string_repr();
        assert_eq!(s, "1a2b3c@101/acme:echo-service:1.0.0");

        let parsed = ActrId::from_string_repr(&s).unwrap();
        assert_eq!(parsed.realm.realm_id, original.realm.realm_id);
        assert_eq!(parsed.serial_number, original.serial_number);
        assert_eq!(parsed.r#type.manufacturer, original.r#type.manufacturer);
        assert_eq!(parsed.r#type.name, original.r#type.name);
        assert_eq!(parsed.r#type.version, original.r#type.version);
    }

    #[test]
    fn test_actor_id_roundtrip_without_version_errors() {
        // Parsing a string without version should now return an error
        let result = ActrId::from_string_repr("1a2b3c@101/acme:echo-service");
        assert!(
            matches!(result, Err(ActrIdError::InvalidTypeFormat(_))),
            "Expected InvalidTypeFormat error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_invalid_actor_id_format() {
        assert!(matches!(
            ActrId::from_string_repr("invalid-string"),
            Err(ActrIdError::InvalidFormat(_))
        ));
        // Missing '/' between realm and type
        assert!(matches!(
            ActrId::from_string_repr("123@101"),
            Err(ActrIdError::InvalidFormat(_))
        ));
        // Invalid hex serial
        assert!(matches!(
            ActrId::from_string_repr("xyz@101/acme:echo"),
            Err(ActrIdError::InvalidComponent(_))
        ));
    }

    #[test]
    fn test_actr_type_roundtrip_with_version() {
        let s = "acme:echo:1.2.3";
        let ty = ActrType::from_string_repr(s).unwrap();
        assert_eq!(ty.manufacturer, "acme");
        assert_eq!(ty.name, "echo");
        assert_eq!(ty.version.as_str(), "1.2.3");
        assert_eq!(ty.to_string_repr(), s);
    }

    #[test]
    fn test_actr_type_without_version_is_error() {
        // Version is now required; two-segment strings must fail
        let s = "acme:echo-service";
        let result = ActrType::from_string_repr(s);
        assert!(
            matches!(result, Err(ActrIdError::InvalidTypeFormat(_))),
            "Expected InvalidTypeFormat, got: {:?}",
            result
        );
    }

    #[test]
    fn test_actr_type_invalid_format() {
        // Single segment (no colon) — invalid
        assert!(matches!(
            ActrType::from_string_repr("acme-echo"),
            Err(ActrIdError::InvalidTypeFormat(_))
        ));
        // Too many segments (4+)
        assert!(matches!(
            ActrType::from_string_repr("a:b:c:d"),
            Err(ActrIdError::InvalidTypeFormat(_))
        ));
        // Invalid manufacturer
        assert!(matches!(
            ActrType::from_string_repr("1acme:echo:1.0.0"),
            Err(ActrIdError::InvalidComponent(_))
        ));
        // Invalid name
        assert!(matches!(
            ActrType::from_string_repr("acme:echo!:1.0.0"),
            Err(ActrIdError::InvalidComponent(_))
        ));
        assert!(matches!(
            ActrType::from_string_repr("acme:echo:"),
            Err(ActrIdError::InvalidComponent(_))
        ));
    }

    fn ty(manufacturer: &str, name: &str, version: &str) -> ActrType {
        ActrType {
            manufacturer: manufacturer.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    #[test]
    fn test_validate_accepts_valid_identities() {
        assert!(ty("acme", "echo-service", "1.0.0").validate().is_ok());
        assert!(ty("acme", "x..y", "1.0.0").validate().is_ok()); // internal dots allowed by Name
        assert!(ty("Acme_Corp", "Echo", "1.0.0-rc.1").validate().is_ok());
        assert!(ty("acme", "echo", "1.0.0+build").validate().is_ok()); // semver build metadata
        assert!(ty("acme", "echo", "v1").validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_bad_manufacturer_name() {
        assert!(ty("", "echo", "1.0.0").validate().is_err());
        assert!(ty("1acme", "echo", "1.0.0").validate().is_err()); // digit start (Name)
        assert!(ty("acme", "echo!", "1.0.0").validate().is_err());
        // `Name` limit is 32 UTF-8 bytes
        assert!(ty(&"a".repeat(33), "echo", "1.0.0").validate().is_err());
    }

    #[test]
    fn test_validate_rejects_bad_version() {
        assert!(ty("acme", "echo", "").validate().is_err()); // empty
        assert!(ty("acme", "echo", &"1".repeat(65)).validate().is_err()); // >64 bytes
        // protocol separators, backslash, whitespace, non-ASCII
        for v in [
            "1:0",
            "1/0",
            "1@0",
            "1\\0",
            "1 0",
            "1.0 ",
            " 1.0",
            "caf\u{e9}",
        ] {
            assert!(
                ty("acme", "echo", v).validate().is_err(),
                "version {v:?} must be rejected"
            );
        }
        // first/last must be ASCII alphanumeric
        assert!(ty("acme", "echo", "-1.0").validate().is_err());
        assert!(ty("acme", "echo", "1.0-").validate().is_err());
        assert!(ty("acme", "echo", ".1.0").validate().is_err());
    }

    #[test]
    fn test_validate_roundtrip_consistency() {
        for (m, n, v) in [
            ("acme", "echo-service", "1.0.0"),
            ("acme", "x..y", "2.0.0-rc.1"),
            ("Acme_Corp", "Echo", "1.0.0+build"),
        ] {
            let t = ty(m, n, v);
            t.validate().expect("identity should validate");
            let s = t.to_string_repr();
            let parsed = ActrType::from_string_repr(&s)
                .unwrap_or_else(|e| panic!("round-trip failed for {s}: {e}"));
            assert_eq!(parsed.manufacturer, t.manufacturer);
            assert_eq!(parsed.name, t.name);
            assert_eq!(parsed.version, t.version);
        }
    }
}
