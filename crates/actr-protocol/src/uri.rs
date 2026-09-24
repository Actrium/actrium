//! `actr:` URI parsing and normalization.
//!
//! An `actr:` URI is a Realm link (`actr/protocols/uri.md`):
//!
//! ```text
//! actr://<domain>[/<realm_id>|/<serial>.<realm_id>[/path]][?query][#fragment]
//! ```
//!
//! The first path segment names a Realm (`<realm_id>@<domain>`) or an Actor
//! (`<serial>.<realm_id>@<domain>`). The rest of the path, the query, and the
//! fragment belong to the application and carry no protocol meaning.

use std::fmt::{Display, Formatter, Write as _};
use std::net::Ipv4Addr;
use std::str::FromStr;
use thiserror::Error;

/// Pseudo-domain suffix of a local Realm: `<installation-id>.actr.alt`.
pub const LOCAL_REALM_SUFFIX: &str = ".actr.alt";

/// `actr:` URI error. Each variant maps to one URI error code; see [`ActrUriError::code`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActrUriError {
    #[error("invalid URI scheme, expected 'actr' but got '{0}'")]
    InvalidScheme(String),

    #[error("invalid Realm domain: {0}")]
    InvalidDomain(String),

    #[error("invalid RealmId or ActorId: {0}")]
    InvalidLabel(String),

    #[error("invalid path, query, or fragment: {0}")]
    InvalidPath(String),

    #[error("legacy actr URI form is not supported: {0}")]
    LegacyForm(String),

    #[error("local Realm '{0}' does not belong to this installation")]
    LocalRealmForeign(String),
}

impl ActrUriError {
    /// Stable error code for logs, tests, and developer tools.
    ///
    /// The URI error codes have no separate entry for a malformed path, query,
    /// or fragment, so those report `URI_LABEL_INVALID`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidScheme(_) => "URI_SCHEME_INVALID",
            Self::InvalidDomain(_) => "URI_DOMAIN_INVALID",
            Self::InvalidLabel(_) | Self::InvalidPath(_) => "URI_LABEL_INVALID",
            Self::LegacyForm(_) => "URI_LEGACY_FORM",
            Self::LocalRealmForeign(_) => "URI_LOCAL_REALM_FOREIGN",
        }
    }
}

/// A normalized `actr:` URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActrUri {
    /// Lowercase IDNA A-label domain, or `<installation-id>.actr.alt`.
    domain: String,
    /// RealmId from the first path segment.
    realm_id: Option<String>,
    /// Actor serial when the first path segment is `<serial>.<realm_id>`.
    serial: Option<String>,
    /// Remaining path after the first segment: empty or `/segment...`.
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl ActrUri {
    /// Construct the domain-only URI `actr://<domain>`.
    pub fn from_domain(domain: &str) -> Result<Self, ActrUriError> {
        Ok(Self {
            domain: normalize_domain(domain)?,
            realm_id: None,
            serial: None,
            path: String::new(),
            query: None,
            fragment: None,
        })
    }

    pub fn scheme(&self) -> &'static str {
        "actr"
    }

    /// Realm domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn realm_id(&self) -> Option<&str> {
        self.realm_id.as_deref()
    }

    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    /// Path after the first segment: empty or starting with `/`.
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// Realm name `<realm_id>@<domain>`, when the URI names a Realm or an Actor.
    pub fn realm_name(&self) -> Option<String> {
        let realm_id = self.realm_id.as_ref()?;
        Some(format!("{realm_id}@{}", self.domain))
    }

    /// ActorId `<serial>.<realm_id>@<domain>`, when the URI names an Actor.
    pub fn actor_id(&self) -> Option<String> {
        let serial = self.serial.as_ref()?;
        let realm_id = self.realm_id.as_ref()?;
        Some(format!("{serial}.{realm_id}@{}", self.domain))
    }

    /// Whether the domain is a local Realm pseudo-domain `<installation-id>.actr.alt`.
    pub fn is_local(&self) -> bool {
        self.domain.ends_with(LOCAL_REALM_SUFFIX)
    }

    /// Reject a local Realm that belongs to another installation.
    pub fn check_local(&self, installation_id: &str) -> Result<(), ActrUriError> {
        if self.is_local() && self.domain != format!("{installation_id}{LOCAL_REALM_SUFFIX}") {
            return Err(ActrUriError::LocalRealmForeign(self.domain.clone()));
        }
        Ok(())
    }

    /// Canonical domain-only URI `actr://<domain>`, used to bind an authority descriptor.
    pub fn authority_uri(&self) -> String {
        format!("actr://{}", self.domain)
    }

    /// HTTPS discovery URL of the domain. A local Realm has none.
    pub fn discovery_url(&self) -> Option<String> {
        (!self.is_local()).then(|| format!("https://{}/.well-known/actr", self.domain))
    }

    /// https form of a Realm link under a declared `links` prefix.
    ///
    /// Returns `None` for a local Realm, a domain-only URI, or an invalid prefix.
    pub fn to_https(&self, prefix: &str) -> Option<String> {
        if self.is_local() || self.realm_id.is_none() || !is_link_prefix(prefix) {
            return None;
        }
        let uri = self.to_string();
        let rest = &uri["actr://".len() + self.domain.len() + 1..];
        Some(format!("https://{}{prefix}{rest}", self.domain))
    }

    /// Convert an https address to a Realm link using the domain's declared
    /// `links` prefixes, longest prefix first.
    ///
    /// Returns `None` when no prefix matches or the result is not a Realm link;
    /// such an address is an ordinary web page.
    pub fn from_https(url: &str, links: &[String]) -> Option<Self> {
        let (scheme, rest) = url.split_once("://")?;
        if !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (host, rest) = rest.split_at(end);
        let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
        let (path, suffix) = rest.split_at(path_end);
        let prefix = links
            .iter()
            .filter(|prefix| is_link_prefix(prefix) && path.starts_with(prefix.as_str()))
            .max_by_key(|prefix| prefix.len())?;
        let uri = format!("actr://{host}/{}{suffix}", &path[prefix.len()..])
            .parse::<Self>()
            .ok()?;
        (uri.realm_id.is_some() && !uri.is_local()).then_some(uri)
    }
}

impl Display for ActrUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "actr://{}", self.domain)?;
        if let Some(realm_id) = &self.realm_id {
            f.write_char('/')?;
            if let Some(serial) = &self.serial {
                write!(f, "{serial}.")?;
            }
            f.write_str(realm_id)?;
        }
        f.write_str(&self.path)?;
        if let Some(query) = &self.query {
            write!(f, "?{query}")?;
        }
        if let Some(fragment) = &self.fragment {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

impl FromStr for ActrUri {
    type Err = ActrUriError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (scheme, rest) = input.split_once(':').unwrap_or((input, ""));
        if !scheme.eq_ignore_ascii_case("actr") {
            return Err(ActrUriError::InvalidScheme(scheme.to_string()));
        }
        let rest = rest
            .strip_prefix("//")
            .ok_or_else(|| ActrUriError::InvalidDomain("expected actr://<domain>".to_string()))?;
        let (rest, fragment) = rest
            .split_once('#')
            .map_or((rest, None), |(rest, fragment)| (rest, Some(fragment)));
        let (rest, query) = rest
            .split_once('?')
            .map_or((rest, None), |(rest, query)| (rest, Some(query)));
        let (raw_domain, raw_path) = rest.split_once('/').unwrap_or((rest, ""));

        if is_legacy(raw_domain, raw_path) {
            return Err(ActrUriError::LegacyForm(input.to_string()));
        }
        let query = query.map(|q| normalize_component(q, true)).transpose()?;
        let fragment = fragment.map(|f| normalize_component(f, true)).transpose()?;
        let domain = normalize_domain(raw_domain)?;

        let mut uri = Self {
            domain,
            realm_id: None,
            serial: None,
            path: String::new(),
            query,
            fragment,
        };
        if raw_path.is_empty() {
            return Ok(uri);
        }

        let mut segments = raw_path.split('/').map(normalize_segment);
        let target = segments.next().expect("split yields one segment")?;
        let (serial, realm_id) = match target.split_once('.') {
            Some((serial, realm_id)) => (Some(serial), realm_id),
            None => (None, target.as_str()),
        };
        if serial.is_some_and(|serial| !is_serial(serial)) || !is_realm_id(realm_id) {
            return Err(ActrUriError::InvalidLabel(target));
        }
        uri.serial = serial.map(str::to_string);
        uri.realm_id = Some(realm_id.to_string());
        for segment in segments {
            uri.path.push('/');
            uri.path.push_str(&segment?);
        }
        Ok(uri)
    }
}

/// Early forms named in the URI spec's legacy section:
/// `actr://<realm>:<publisher>+<name>@<version>` and
/// `actr://<publisher>/<program>@<version>`.
fn is_legacy(raw_domain: &str, raw_path: &str) -> bool {
    // Both forms end in `@<version>`; a version starts with a digit.
    let versioned = |value: &str| {
        value
            .split_once('@')
            .is_some_and(|(_, version)| version.starts_with(|c: char| c.is_ascii_digit()))
    };
    let prototype = raw_domain
        .split_once(':')
        .and_then(|(_, rest)| rest.split_once('+'))
        .is_some_and(|(_, rest)| versioned(rest));
    let product_draft = raw_path.split('/').next().is_some_and(versioned);
    prototype || product_draft
}

fn normalize_domain(raw: &str) -> Result<String, ActrUriError> {
    let invalid = |reason: &str| ActrUriError::InvalidDomain(format!("'{raw}': {reason}"));
    if raw.is_empty() {
        return Err(invalid("missing domain"));
    }
    if raw.contains('@') {
        return Err(invalid("userinfo is not permitted"));
    }
    if raw.contains([':', '[', ']']) {
        return Err(invalid("port and IP literal are not permitted"));
    }
    let domain = idna::domain_to_ascii_strict(raw).map_err(|_| invalid("not a valid DNS name"))?;
    let last = domain.rsplit('.').next().unwrap_or_default();
    if domain.parse::<Ipv4Addr>().is_ok() || last.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("IP literal is not permitted"));
    }
    if domain == LOCAL_REALM_SUFFIX[1..] || domain.ends_with(LOCAL_REALM_SUFFIX) {
        let id = domain.strip_suffix(LOCAL_REALM_SUFFIX).unwrap_or_default();
        let valid = id.len() == 26 && id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'));
        if !valid {
            return Err(invalid("expected <installation-id>.actr.alt"));
        }
    }
    Ok(domain)
}

/// Parses a Realm name `<realm_id>@<domain>`.
///
/// The name must already be in normalized form: a lowercase IDNA A-label
/// domain and no percent-encoding. The result names the Realm and nothing else.
pub fn parse_realm_name(name: &str) -> Result<ActrUri, ActrUriError> {
    let invalid = || ActrUriError::InvalidLabel(format!("'{name}' is not <realm_id>@<domain>"));
    let (realm_id, domain) = name.split_once('@').ok_or_else(invalid)?;
    let uri: ActrUri = format!("actr://{domain}/{realm_id}").parse()?;
    if uri.domain() != domain
        || uri.realm_id() != Some(realm_id)
        || uri.serial().is_some()
        || !uri.path().is_empty()
        || uri.query().is_some()
        || uri.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(uri)
}

/// RealmId: 1 to 63 lowercase letters, digits, or `-`, not starting or ending with `-`.
pub fn is_realm_id(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Serial: one or more lowercase hexadecimal digits.
fn is_serial(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A `links` prefix starts and ends with `/` and has at least one segment.
pub(crate) fn is_link_prefix(value: &str) -> bool {
    value.len() > 1
        && value.starts_with('/')
        && value.ends_with('/')
        && value[1..value.len() - 1]
            .split('/')
            .all(|segment| !segment.is_empty() && !segment.contains(['?', '#']))
}

fn is_unreserved(octet: u8) -> bool {
    octet.is_ascii_alphanumeric() || matches!(octet, b'-' | b'.' | b'_' | b'~')
}

/// RFC 3986 `pchar` without percent-encoding.
fn is_pchar(octet: u8) -> bool {
    is_unreserved(octet) || b"!$&'()*+,;=:@".contains(&octet)
}

fn normalize_segment(segment: &str) -> Result<String, ActrUriError> {
    let normalized = normalize_component(segment, false)?;
    if normalized.is_empty() || normalized == "." || normalized == ".." {
        return Err(ActrUriError::InvalidPath(
            "empty and dot segments are not permitted".to_string(),
        ));
    }
    Ok(normalized)
}

/// Uppercase percent-encoding hex digits and decode unreserved characters.
/// Query and fragment also allow `/` and `?`.
fn normalize_component(value: &str, query: bool) -> Result<String, ActrUriError> {
    let invalid = |reason: &str| ActrUriError::InvalidPath(format!("'{value}': {reason}"));
    let bytes = value.as_bytes();
    let mut normalized = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        let octet = bytes[index];
        if octet == b'%' {
            let octet = value
                .get(index + 1..index + 3)
                .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| invalid("invalid percent-encoding"))?;
            if is_unreserved(octet) {
                normalized.push(char::from(octet));
            } else {
                write!(normalized, "%{octet:02X}").expect("writing to a String cannot fail");
            }
            index += 3;
        } else if is_pchar(octet) || (query && matches!(octet, b'/' | b'?')) {
            normalized.push(char::from(octet));
            index += 1;
        } else {
            return Err(invalid(
                "characters outside RFC 3986 must be percent-encoded",
            ));
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> ActrUri {
        value.parse().unwrap()
    }

    fn code(value: &str) -> &'static str {
        value.parse::<ActrUri>().unwrap_err().code()
    }

    #[test]
    fn parses_realm_link() {
        let uri = parse("actr://acme.com/k7m2q9xh");
        assert_eq!(uri.domain(), "acme.com");
        assert_eq!(uri.realm_id(), Some("k7m2q9xh"));
        assert_eq!(uri.serial(), None);
        assert_eq!(uri.realm_name().as_deref(), Some("k7m2q9xh@acme.com"));
        assert_eq!(uri.actor_id(), None);
        assert_eq!(uri.to_string(), "actr://acme.com/k7m2q9xh");
    }

    #[test]
    fn passes_page_path_query_and_fragment_through() {
        let uri = parse("actr://acme.com/k7m2q9xh/today?view=week#item-3");
        assert_eq!(uri.realm_name().as_deref(), Some("k7m2q9xh@acme.com"));
        assert_eq!(uri.path(), "/today");
        assert_eq!(uri.query(), Some("view=week"));
        assert_eq!(uri.fragment(), Some("item-3"));
        assert_eq!(
            uri.to_string(),
            "actr://acme.com/k7m2q9xh/today?view=week#item-3"
        );
    }

    #[test]
    fn parses_actor_link() {
        let uri = parse("actr://acme.com/2af01.k7m2q9xh");
        assert_eq!(uri.serial(), Some("2af01"));
        assert_eq!(uri.realm_name().as_deref(), Some("k7m2q9xh@acme.com"));
        assert_eq!(uri.actor_id().as_deref(), Some("2af01.k7m2q9xh@acme.com"));
        assert_eq!(uri.to_string(), "actr://acme.com/2af01.k7m2q9xh");
    }

    #[test]
    fn domain_only_uri_names_no_realm() {
        let uri = parse("actr://acme.com/");
        assert_eq!(uri.realm_id(), None);
        assert_eq!(uri.to_string(), "actr://acme.com");
        assert_eq!(uri.authority_uri(), "actr://acme.com");
        assert_eq!(
            uri.discovery_url().as_deref(),
            Some("https://acme.com/.well-known/actr")
        );
        assert_eq!(ActrUri::from_domain("ACME.com").unwrap(), uri);
    }

    #[test]
    fn normalizes_scheme_domain_and_percent_encoding() {
        let uri = parse("ACTR://ACME.Com/k7m2q9xh/%7euser/%e4%b8%ad?q=%2f#%41");
        assert_eq!(
            uri.to_string(),
            "actr://acme.com/k7m2q9xh/~user/%E4%B8%AD?q=%2F#A"
        );
        assert_eq!(
            parse("actr://例子.中国/k7m2q9xh").domain(),
            "xn--fsqu00a.xn--fiqs8s"
        );
    }

    #[test]
    fn page_path_is_case_sensitive() {
        assert_ne!(
            parse("actr://acme.com/k7m2q9xh/Today"),
            parse("actr://acme.com/k7m2q9xh/today")
        );
    }

    #[test]
    fn local_realm() {
        let id = "abcdefghijklmnopqrstuvwxyz";
        let uri = parse(&format!("actr://{id}.actr.alt/h3x8p2wq"));
        assert!(uri.is_local());
        assert_eq!(uri.discovery_url(), None);
        assert_eq!(uri.to_https("/i/"), None);
        uri.check_local(id).unwrap();
        let error = uri.check_local("bcdefghijklmnopqrstuvwxyza").unwrap_err();
        assert_eq!(error.code(), "URI_LOCAL_REALM_FOREIGN");
        assert!(!parse("actr://acme.com/k7m2q9xh").is_local());
        parse("actr://acme.com/k7m2q9xh").check_local(id).unwrap();
    }

    #[test]
    fn https_form_uses_declared_prefix() {
        let links = vec!["/i/".to_string()];
        let uri = ActrUri::from_https("https://acme.com/i/k7m2q9xh/today?view=week", &links);
        assert_eq!(
            uri.as_ref().map(ToString::to_string).as_deref(),
            Some("actr://acme.com/k7m2q9xh/today?view=week")
        );
        assert_eq!(
            uri.unwrap().to_https("/i/").as_deref(),
            Some("https://acme.com/i/k7m2q9xh/today?view=week")
        );
    }

    #[test]
    fn https_form_prefers_longest_prefix() {
        let links = vec!["/i/".to_string(), "/i/x/".to_string()];
        let uri = ActrUri::from_https("https://acme.com/i/x/k7m2q9xh", &links).unwrap();
        assert_eq!(uri.to_string(), "actr://acme.com/k7m2q9xh");
    }

    #[test]
    fn undeclared_https_is_an_ordinary_page() {
        let links = vec!["/i/".to_string()];
        for url in [
            "https://acme.com/k7m2q9xh",
            "https://acme.com/i/",
            "https://acme.com/i/Bad_Id",
            "https://acme.com:8443/i/k7m2q9xh",
            "http://acme.com/i/k7m2q9xh",
            "https://abcdefghijklmnopqrstuvwxyz.actr.alt/i/h3x8p2wq",
        ] {
            assert_eq!(ActrUri::from_https(url, &links), None, "{url}");
        }
        assert_eq!(
            ActrUri::from_https("https://acme.com/i/k7m2q9xh", &[]),
            None
        );
        assert_eq!(parse("actr://acme.com").to_https("/i/"), None);
        assert_eq!(parse("actr://acme.com/k7m2q9xh").to_https("/"), None);
    }

    #[test]
    fn rejects_invalid_scheme() {
        assert_eq!(code("https://acme.com/k7m2q9xh"), "URI_SCHEME_INVALID");
        assert_eq!(code("actro://acme.com"), "URI_SCHEME_INVALID");
    }

    #[test]
    fn rejects_invalid_domain() {
        for value in [
            "actr:acme.com",
            "actr://",
            "actr://acme.com:443/k7m2q9xh",
            "actr://acme.com:8443",
            "actr://user@acme.com/k7m2q9xh",
            "actr://192.168.1.1/k7m2q9xh",
            "actr://[2001:db8::1]/k7m2q9xh",
            "actr://acme..com",
            "actr://-acme.com",
            "actr://acme.com.",
            "actr://actr.alt",
            "actr://short.actr.alt/h3x8p2wq",
            "actr://abcdefghijklmnopqrstuvwxy1.actr.alt/h3x8p2wq",
        ] {
            assert_eq!(code(value), "URI_DOMAIN_INVALID", "{value}");
        }
    }

    #[test]
    fn rejects_invalid_label() {
        for value in [
            "actr://acme.com/K7m2q9xh",
            "actr://acme.com/-k7m",
            "actr://acme.com/k7m-",
            "actr://acme.com/k7_m",
            "actr://acme.com/2AF01.k7m2q9xh",
            "actr://acme.com/2ag01.k7m2q9xh",
            "actr://acme.com/.k7m2q9xh",
            "actr://acme.com/2af01.",
            "actr://acme.com/1.2.k7m",
            "actr://acme.com/k7m2q9xh/a//b",
            "actr://acme.com/k7m2q9xh/",
            "actr://acme.com/k7m2q9xh/%2e%2e/b",
            "actr://acme.com/k7m2q9xh/./b",
            "actr://acme.com/k7m2q9xh/%ZZ",
            "actr://acme.com/k7m2q9xh/\u{4e2d}",
            "actr://acme.com/k7m2q9xh?a b",
            "actr://acme.com/2af01.k7m2q9xh@acme.com",
        ] {
            assert_eq!(code(value), "URI_LABEL_INVALID", "{value}");
        }
        assert_eq!(
            code(&format!("actr://acme.com/{}", "a".repeat(64))),
            "URI_LABEL_INVALID"
        );
    }

    #[test]
    fn parses_realm_name() {
        let uri = parse_realm_name("k7m2q9xh@acme.com").unwrap();
        assert_eq!(uri.realm_name().as_deref(), Some("k7m2q9xh@acme.com"));
        let local = "h3x8p2wq@abcdefghijklmnopqrstuvwxyz.actr.alt";
        assert!(parse_realm_name(local).unwrap().is_local());
        for name in [
            "k7m2q9xh",
            "@acme.com",
            "k7m2q9xh@",
            "K7m2q9xh@acme.com",
            "k7m2q9xh@ACME.com",
            "k7m2q9xh@acme.com:443",
            "k7m2q9xh@acme.com/x",
            "k7m2q9xh@acme.com?x",
            "k7m%32q9xh@acme.com",
            "2af01.k7m2q9xh@acme.com",
            "a@b@acme.com",
            "k7m2q9xh@\u{4f8b}\u{5b50}.\u{4e2d}\u{56fd}",
            "k7m2q9xh@short.actr.alt",
        ] {
            assert!(parse_realm_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn realm_id_rules() {
        assert!(is_realm_id("k7m2q9xh"));
        assert!(is_realm_id("a-1"));
        for value in ["", "-a", "a-", "A", "a.b", "a_b"] {
            assert!(!is_realm_id(value), "{value}");
        }
        assert!(!is_realm_id(&"a".repeat(64)));
    }

    #[test]
    fn rejects_legacy_forms() {
        for value in [
            "actr://101:acme+echo@1.0.0",
            "actr://acme.com/notes@1.2.0",
            "actr://acme.com/notes@1.2.0?service=2af01.k7m2q9xh@acme.com",
        ] {
            assert_eq!(code(value), "URI_LEGACY_FORM", "{value}");
        }
    }
}
