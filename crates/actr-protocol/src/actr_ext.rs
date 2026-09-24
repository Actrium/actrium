//! Actor identity formatting and parsing utilities
//!
//! This module provides string formatting and parsing helpers for `ActrType` and `ActrId`.
//! String forms are stable for logging, configuration, and CLI interactions.
//!
//! ## String formats
//!
//! | Type     | Format                                        | Example                      |
//! |----------|-----------------------------------------------|------------------------------|
//! | Program identity | `publisher/program@version` | `apps.example.com/echo@1.0.0` |
//! | ActrId | `<serial_hex>.<realm_id>@<domain>` | `2af01.k7m2q9xh@acme.com` |

use crate::uri::{ActrUri, ActrUriError};
use crate::{ActrId, ActrType, name::Name};
use thiserror::Error;

fn validate_actor_namespace(value: &str) -> Result<(), ActrIdError> {
    if value.is_empty()
        || value.len() > 253
        || value.starts_with('-')
        || value.starts_with('.')
        || value.ends_with('-')
        || value.ends_with('.')
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
    {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid actor namespace: {value}"
        )));
    }
    Ok(())
}

/// Validate the publisher of an `ActrType` parsed by
/// [`ActrType::from_string_repr`].
///
/// This is deliberately more permissive than [`validate_actor_namespace`]: DNS
/// labels may start with a digit, and linked-workload namespaces may use
/// interior underscores, so the general parser must not reintroduce the old
/// "starts with a letter" rule. A leading or trailing `-`, `_`, or `.` is
/// still rejected, as is any character outside ASCII alphanumeric and `-_.`.
/// Security-sensitive entry points use the strict [`ActrType::validate`]
/// (Name-based) gate rather than this parser-level check.
fn validate_type_publisher(value: &str) -> Result<(), ActrIdError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid actor namespace: {value}"
        )));
    };
    if value.len() > 253
        || !first.is_ascii_alphanumeric()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || !value.ends_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid actor namespace: {value}"
        )));
    }
    Ok(())
}

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
    #[error("Invalid Actor ID format: '{0}'. Expected: <serial_hex>.<realm_id>@<domain>")]
    InvalidFormat(String),

    #[error("Invalid component in actor identity: {0}")]
    InvalidComponent(String),

    #[error("Invalid actor type format: '{0}'. Expected: <publisher>/<program>@<version>")]
    InvalidTypeFormat(String),
}

impl ActrType {
    /// Canonical program identity used by registries:
    /// `publisher/program@version`.
    pub fn to_program_identity(&self) -> String {
        format!("{}/{}@{}", self.publisher, self.name, self.version)
    }

    /// Parse the canonical `publisher/program@version` registry identity.
    pub fn from_program_identity(value: &str) -> Result<Self, ActrIdError> {
        let (publisher, program_version) = value
            .split_once('/')
            .ok_or_else(|| ActrIdError::InvalidTypeFormat(value.to_string()))?;
        let (program, version) = program_version
            .rsplit_once('@')
            .ok_or_else(|| ActrIdError::InvalidTypeFormat(value.to_string()))?;
        if publisher.is_empty() || program.is_empty() || version.is_empty() {
            return Err(ActrIdError::InvalidTypeFormat(value.to_string()));
        }
        validate_actor_namespace(publisher)?;
        if publisher != publisher.to_ascii_lowercase()
            || !publisher.contains('.')
            || publisher.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
            })
        {
            return Err(ActrIdError::InvalidComponent(
                "program publisher must be a canonical lowercase DNS domain".to_string(),
            ));
        }
        Name::new(program.to_string())
            .map_err(|error| ActrIdError::InvalidComponent(error.to_string()))?;
        // This parser feeds from untrusted wire input (WS actor_id query
        // params, TURN usernames); an unvalidated version could smuggle the
        // protocol separators ':' '@' '/' plus control characters into
        // server-side identity strings and logs.
        validate_version(version)?;
        Ok(Self {
            publisher: publisher.to_string(),
            name: program.to_string(),
            version: version.to_string(),
        })
    }

    /// Convert to the stable string representation.
    ///
    /// Always returns the canonical `"publisher/program@version"` form, the
    /// same text as [`ActrType::to_program_identity`].
    pub fn to_string_repr(&self) -> String {
        debug_assert!(
            !self.version.is_empty(),
            "ActrType.version must be non-empty"
        );

        self.to_program_identity()
    }

    /// Validate the identity fields against the protocol naming rules.
    ///
    /// `publisher`/`name` reuse [`Name`]; `version` uses its own 1–64 byte
    /// ASCII allowlist (see [`validate_version`]). Because [`ActrType`] is a
    /// protobuf type with public fields, callers can construct invalid values
    /// without going through a parser — so every security-sensitive entry point
    /// (package verification, registry lookup, Issuer registration, storage
    /// namespace derivation) must call `validate()` before trusting the identity.
    pub fn validate(&self) -> Result<(), ActrIdError> {
        Name::new(self.publisher.clone())
            .map_err(|e| ActrIdError::InvalidComponent(format!("Invalid publisher: {e}")))?;
        Name::new(self.name.clone())
            .map_err(|e| ActrIdError::InvalidComponent(format!("Invalid type name: {e}")))?;
        validate_version(&self.version)
    }

    /// Parse from the string representation `"publisher/program@version"`.
    ///
    /// Unlike [`ActrType::from_program_identity`], the publisher does not have
    /// to be a DNS domain, so linked and local workloads can use it too. The
    /// retired `publisher:name:version` colon form is rejected with a hint.
    pub fn from_string_repr(s: &str) -> Result<Self, ActrIdError> {
        let Some((publisher, program_version)) = s.split_once('/') else {
            if s.contains(':') {
                return Err(ActrIdError::InvalidTypeFormat(format!(
                    "{s} (the publisher:name:version form is no longer supported; \
                     write <publisher>/<program>@<version>)"
                )));
            }
            return Err(ActrIdError::InvalidTypeFormat(s.to_string()));
        };
        let Some((name, version)) = program_version.rsplit_once('@') else {
            return Err(ActrIdError::InvalidTypeFormat(format!(
                "{s} (version is required, expected <publisher>/<program>@<version>)"
            )));
        };
        if publisher.is_empty() || name.is_empty() || name.contains('/') {
            return Err(ActrIdError::InvalidTypeFormat(s.to_string()));
        }

        validate_type_publisher(publisher)?;
        Name::new(name.to_string())
            .map_err(|e| ActrIdError::InvalidComponent(format!("Invalid type name: {e}")))?;
        validate_version(version)?;
        Ok(ActrType {
            publisher: publisher.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })
    }
}

/// Validate the `version` field of an [`ActrType`].
///
/// Rules: 1–=[`MAX_ACTR_VERSION_BYTES`] UTF-8 bytes; first and last characters
/// must be ASCII alphanumeric; interior characters are limited to ASCII
/// alphanumeric plus `.` `_` `-` `+`. This forbids the protocol separators
/// `:` `@` `/` as well as `\`, whitespace, and control characters, so a valid
/// version can never corrupt the `publisher/program@version` string form.
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

/// Largest Actor serial: serials are at most 54 bits.
const MAX_ACTOR_SERIAL: u64 = (1 << 54) - 1;

/// Formats an ActorId `<serial_hex>.<realm_id>@<domain>` from a Realm name and
/// an Actor serial.
pub fn format_actor_id(realm: &str, serial_number: u64) -> String {
    format!("{serial_number:x}.{realm}")
}

/// Parses an ActorId `<serial_hex>.<realm_id>@<domain>` into its Realm name and
/// Actor serial.
///
/// The serial is lowercase hexadecimal without leading zeros and at most 54
/// bits, so the text form is canonical. The legacy
/// `<serial_hex>@<realm_id>/<type>` form is rejected.
pub fn parse_actor_id(s: &str) -> Result<(String, u64), ActrIdError> {
    let expected = "expected <serial_hex>.<realm_id>@<domain>";
    let (local, _) = s
        .split_once('@')
        .ok_or_else(|| ActrIdError::InvalidFormat(format!("{s} ({expected})")))?;
    if s[local.len() + 1..].starts_with(|c: char| c.is_ascii_digit()) && s.contains('/') {
        return Err(ActrIdError::InvalidFormat(format!(
            "{s} (the numeric <serial_hex>@<realm_id>/<type> form is no longer supported; {expected})"
        )));
    }
    let (serial, realm) = s
        .split_once('.')
        .filter(|(serial, _)| !serial.contains('@'))
        .ok_or_else(|| ActrIdError::InvalidFormat(format!("{s} ({expected})")))?;
    let serial_number = (!serial.is_empty()
        && serial.len() <= 14
        && (serial == "0" || !serial.starts_with('0'))
        && serial
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| u64::from_str_radix(serial, 16).ok())
    .flatten()
    .filter(|serial| *serial <= MAX_ACTOR_SERIAL)
    .ok_or_else(|| ActrIdError::InvalidComponent(format!("Invalid Actor serial: {serial}")))?;
    crate::uri::parse_realm_name(realm)
        .map_err(|error| ActrIdError::InvalidComponent(error.to_string()))?;
    Ok((realm.to_string(), serial_number))
}

impl ActrId {
    /// Convert to `"<serial_hex>.<realm_id>@<domain>"`.
    pub fn to_string_repr(&self) -> String {
        format_actor_id(&self.realm, self.serial_number)
    }

    /// Parse from `"<serial_hex>.<realm_id>@<domain>"`; see [`parse_actor_id`].
    pub fn from_string_repr(s: &str) -> Result<Self, ActrIdError> {
        let (realm, serial_number) = parse_actor_id(s)?;
        Ok(ActrId {
            realm,
            serial_number,
        })
    }

    /// RealmId part of the Realm name.
    pub fn realm_id(&self) -> &str {
        self.realm
            .split_once('@')
            .map_or("", |(realm_id, _)| realm_id)
    }

    /// Domain part of the Realm name.
    pub fn domain(&self) -> &str {
        self.realm.split_once('@').map_or("", |(_, domain)| domain)
    }

    /// Actor link `actr://<domain>/<serial_hex>.<realm_id>`.
    pub fn to_uri(&self) -> Result<ActrUri, ActrUriError> {
        format!(
            "actr://{}/{:x}.{}",
            self.domain(),
            self.serial_number,
            self.realm_id()
        )
        .parse()
    }

    /// The Actor named by an Actor link, if the URI names one.
    pub fn from_uri(uri: &ActrUri) -> Option<Self> {
        Self::from_string_repr(&uri.actor_id()?).ok()
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
    fn test_actor_id_roundtrip() {
        let original = ActrId {
            realm: "k7m2q9xh@acme.com".to_string(),
            serial_number: 0x2af01,
        };
        let s = original.to_string_repr();
        assert_eq!(s, "2af01.k7m2q9xh@acme.com");
        assert_eq!(original.to_string(), s);
        assert_eq!(ActrId::from_string_repr(&s).unwrap(), original);
        assert_eq!(original.realm_id(), "k7m2q9xh");
        assert_eq!(original.domain(), "acme.com");

        let uri = original.to_uri().unwrap();
        assert_eq!(uri.to_string(), "actr://acme.com/2af01.k7m2q9xh");
        assert_eq!(ActrId::from_uri(&uri), Some(original));
        assert_eq!(
            ActrId::from_uri(&"actr://acme.com/k7m2q9xh".parse().unwrap()),
            None
        );
    }

    #[test]
    fn actor_id_text_vectors() {
        assert_eq!(
            format_actor_id("k7m2q9xh@acme.com", 0x2af01),
            "2af01.k7m2q9xh@acme.com"
        );
        assert_eq!(
            parse_actor_id("2af01.k7m2q9xh@acme.com").unwrap(),
            ("k7m2q9xh@acme.com".to_string(), 0x2af01)
        );
        let local = "h3x8p2wq@abcdefghijklmnopqrstuvwxyz.actr.alt";
        assert_eq!(
            parse_actor_id(&format!("0.{local}")).unwrap(),
            (local.to_string(), 0)
        );
        assert_eq!(
            parse_actor_id("3fffffffffffff.k7m2q9xh@acme.com")
                .unwrap()
                .1,
            (1 << 54) - 1
        );
    }

    #[test]
    fn actor_id_text_rejects_invalid() {
        for value in [
            "k7m2q9xh@acme.com",
            "2af01k7m2q9xh",
            ".k7m2q9xh@acme.com",
            "2af01.@acme.com",
            "2AF01.k7m2q9xh@acme.com",
            "0x2af01.k7m2q9xh@acme.com",
            "-2af01.k7m2q9xh@acme.com",
            "+2af01.k7m2q9xh@acme.com",
            "02af01.k7m2q9xh@acme.com",
            "40000000000000.k7m2q9xh@acme.com",
            "2af01.K7m2q9xh@acme.com",
            "2af01.k7m2q9xh@ACME.com",
            "2af01.k7m2q9xh@acme.com@acme.com",
            "2af01.k7m.q9xh@acme.com",
            "2af01.k7m2q9xh@acme.com/x",
        ] {
            assert!(parse_actor_id(value).is_err(), "{value}");
        }
        let legacy = parse_actor_id("1a2b3c@101/apps.example.com/echo@1.0.0");
        assert!(
            matches!(&legacy, Err(ActrIdError::InvalidFormat(message)) if message.contains("no longer supported")),
            "{legacy:?}"
        );
    }

    #[test]
    fn test_program_identity_roundtrip() {
        let program =
            ActrType::from_program_identity("apps.example.com/echo-service@1.0.0").unwrap();
        assert_eq!(
            program.to_program_identity(),
            "apps.example.com/echo-service@1.0.0"
        );
    }

    #[test]
    fn test_program_identity_rejects_noncanonical_publisher() {
        for invalid in [
            "EXAMPLE.COM/echo@1.0.0",
            "example..com/echo@1.0.0",
            "-bad.example/echo@1.0.0",
            "localhost/echo@1.0.0",
        ] {
            assert!(
                ActrType::from_program_identity(invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn test_actr_type_roundtrip_with_version() {
        let s = "acme/echo@1.2.3";
        let ty = ActrType::from_string_repr(s).unwrap();
        assert_eq!(ty.publisher, "acme");
        assert_eq!(ty.name, "echo");
        assert_eq!(ty.version.as_str(), "1.2.3");
        assert_eq!(ty.to_string_repr(), s);
        assert_eq!(ty.to_string(), s);
    }

    #[test]
    fn test_actr_type_without_version_is_error() {
        let result = ActrType::from_string_repr("acme/echo-service");
        assert!(
            matches!(result, Err(ActrIdError::InvalidTypeFormat(_))),
            "Expected InvalidTypeFormat, got: {:?}",
            result
        );
    }

    #[test]
    fn test_actr_type_rejects_retired_colon_form() {
        let result = ActrType::from_string_repr("acme:echo:1.2.3");
        match result {
            Err(ActrIdError::InvalidTypeFormat(message)) => {
                assert!(
                    message.contains("<publisher>/<program>@<version>"),
                    "{message}"
                );
            }
            other => panic!("expected InvalidTypeFormat, got {other:?}"),
        }
    }

    #[test]
    fn test_actr_type_invalid_format() {
        // Missing '/' and '@'
        assert!(matches!(
            ActrType::from_string_repr("acme-echo"),
            Err(ActrIdError::InvalidTypeFormat(_))
        ));
        // Extra path segment
        assert!(matches!(
            ActrType::from_string_repr("a/b/c@1"),
            Err(ActrIdError::InvalidTypeFormat(_))
        ));
        // DNS labels may start with a digit; the ActrType namespace parser
        // must not reintroduce the old "starts with a letter" rule.
        assert!(ActrType::from_string_repr("1acme/echo@1.0.0").is_ok());
        // Invalid publisher
        assert!(matches!(
            ActrType::from_string_repr("_acme/echo@1.0.0"),
            Err(ActrIdError::InvalidComponent(_))
        ));
        // Invalid name
        assert!(matches!(
            ActrType::from_string_repr("acme/echo!@1.0.0"),
            Err(ActrIdError::InvalidComponent(_))
        ));
        assert!(matches!(
            ActrType::from_string_repr("acme/echo@"),
            Err(ActrIdError::InvalidComponent(_))
        ));
    }

    fn ty(publisher: &str, name: &str, version: &str) -> ActrType {
        ActrType {
            publisher: publisher.to_string(),
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
    fn test_validate_rejects_bad_publisher_name() {
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
            assert_eq!(parsed.publisher, t.publisher);
            assert_eq!(parsed.name, t.name);
            assert_eq!(parsed.version, t.version);
        }
    }
}
