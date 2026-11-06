//! Authentication boundary shared by native package installers (#366).
//!
//! Network and trust UI belong to the embedding Host. Realm selections must
//! come from F02's accepted Profile; an AppInfo hash is not a Realm acceptance.
//! Publisher descriptors arrive only through their Web-PKI authenticated
//! bootstrap. No Registry row or Publisher private key is involved.

use std::sync::Arc;

use actr_pack::program::{ProgramHostPolicy, verify_program_release};
use actr_protocol::authority::{AuthorityDescriptor, KeyPinOutcome, KeyPins, PublisherKeys};
use actr_protocol::uri::ActrUri;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};

use crate::CoreError;

pub const MAX_PACKAGE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;

/// A source explicitly selected by the Host user. For a Realm, both expected
/// fields are mandatory and are copied from its already accepted Profile.
#[derive(Debug, Clone)]
pub struct PackageSourceInfo {
    pub href: String,
    pub expected_sha256: Option<String>,
    pub expected_actor_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackagePublisherInfo {
    pub domain: String,
    pub bootstrap_url: String,
    pub digest: String,
    pub actor_type: String,
}

#[derive(Debug, Clone)]
pub struct VerifiedProgramFileInfo {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// All bytes and metadata are from one authenticated archive. Permission
/// requests are metadata, not runtime grants. The caller must still enforce
/// the Front/session/capability policy before exposing business capabilities.
#[derive(Debug, Clone)]
pub struct VerifiedProgramInfo {
    pub publisher: String,
    pub program: String,
    pub version: String,
    pub digest: String,
    pub signing_key_id: String,
    pub title: String,
    pub description: Option<String>,
    pub guest_code: String,
    pub service_bytes: Vec<u8>,
    pub resources: Vec<VerifiedProgramFileInfo>,
    pub metadata_json: String,
}

fn fail(code: &str, message: impl std::fmt::Display) -> CoreError {
    CoreError::PackageVerification(format!("{code}: {message}"))
}

pub fn validate_package_source(source: PackageSourceInfo) -> Result<(), CoreError> {
    let url = url::Url::parse(&source.href).map_err(|e| fail("PACKAGE_SOURCE_INVALID", e))?;
    let authority = source
        .href
        .strip_prefix("https://")
        .unwrap_or_default()
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if !source.href.starts_with("https://")
        || authority.is_empty()
        || authority.contains('@')
        || source.href.contains('\\')
        || source
            .href
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(fail(
            "PACKAGE_SOURCE_INVALID",
            "expected an HTTPS URL without credentials or fragment",
        ));
    }
    if let Some(hash) = &source.expected_sha256
        && (hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(fail("PACKAGE_DIGEST_INVALID", "expected lowercase SHA-256"));
    }
    if let Some(identity) = &source.expected_actor_type {
        actr_protocol::ActrType::from_program_identity(identity)
            .map_err(|e| fail("PACKAGE_IDENTITY_INVALID", e))?;
        if source.expected_sha256.is_none() {
            return Err(fail(
                "PACKAGE_DIGEST_INVALID",
                "a Realm package selection requires its digest",
            ));
        }
    }
    Ok(())
}

fn identity(source: &PackageSourceInfo, archive: &[u8]) -> Result<PackagePublisherInfo, CoreError> {
    validate_package_source(source.clone())?;
    if archive.is_empty() || archive.len() > MAX_PACKAGE_BYTES {
        return Err(fail("PACKAGE_SIZE_LIMIT", "empty or oversized archive"));
    }
    let digest = format!("{:x}", Sha256::digest(archive));
    if source
        .expected_sha256
        .as_ref()
        .is_some_and(|expected| expected != &digest)
    {
        return Err(fail(
            "PACKAGE_DIGEST_MISMATCH",
            "download differs from selection",
        ));
    }
    // Bounded inspection authenticates nothing; only the identity leaves this
    // function. Payloads stay inside Rust until signature and policy succeed.
    let inspected =
        actr_pack::inspect_actor_package(archive).map_err(|e| fail("PACKAGE_INVALID", e))?;
    let actor_type = inspected.manifest.actor_type();
    if source
        .expected_actor_type
        .as_ref()
        .is_some_and(|expected| expected != &actor_type)
    {
        return Err(fail(
            "PACKAGE_IDENTITY_MISMATCH",
            "manifest differs from accepted Realm package type",
        ));
    }
    let domain = &inspected.manifest.actor.publisher;
    let uri = ActrUri::from_domain(domain).map_err(|e| fail("PACKAGE_PUBLISHER_INVALID", e))?;
    if uri.domain() != domain || uri.is_local() || !domain.contains('.') {
        return Err(fail(
            "PACKAGE_PUBLISHER_INVALID",
            "expected canonical Publisher DNS domain",
        ));
    }
    Ok(PackagePublisherInfo {
        domain: domain.clone(),
        bootstrap_url: format!("https://{domain}/.well-known/actr"),
        digest,
        actor_type,
    })
}

pub fn inspect_downloaded_program(
    source: PackageSourceInfo,
    archive: Vec<u8>,
) -> Result<PackagePublisherInfo, CoreError> {
    identity(&source, &archive)
}

/// Opaque accepted Publisher state. Its constructor verifies the descriptor
/// and rotation chain. A serialized record cannot masquerade as this object.
pub struct PackagePublisher {
    domain: String,
    keys: PublisherKeys,
    pins: KeyPins,
    expires_at: DateTime<Utc>,
}

pub fn accept_package_publisher(
    domain: String,
    descriptor_json: String,
    pins_json: Option<String>,
) -> Result<Arc<PackagePublisher>, CoreError> {
    if descriptor_json.len() > MAX_DESCRIPTOR_BYTES
        || pins_json
            .as_ref()
            .is_some_and(|p| p.len() > MAX_DESCRIPTOR_BYTES)
    {
        return Err(fail(
            "PUBLISHER_SIZE_LIMIT",
            "descriptor or saved pins exceeds limit",
        ));
    }
    let uri = ActrUri::from_domain(&domain).map_err(|e| fail("PUBLISHER_INVALID", e))?;
    if uri.domain() != domain || uri.is_local() || !domain.contains('.') {
        return Err(fail("PUBLISHER_INVALID", "noncanonical Publisher domain"));
    }
    let descriptor: AuthorityDescriptor =
        serde_json::from_str(&descriptor_json).map_err(|e| fail("PUBLISHER_INVALID", e))?;
    descriptor
        .validate_for(&uri)
        .map_err(|e| fail("PUBLISHER_INVALID", e))?;
    let profile = descriptor
        .publisher
        .ok_or_else(|| fail("PUBLISHER_INVALID", "missing publisher member"))?;
    let pinned: Option<KeyPins> = pins_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| fail("PUBLISHER_PINS_INVALID", e))?;
    let (keys, outcome) = profile
        .accept(&domain, Utc::now(), pinned.as_ref())
        .map_err(|e| fail("PUBLISHER_UNTRUSTED", e))?;
    let pins = match outcome {
        KeyPinOutcome::Accepted(pins) => pins,
        KeyPinOutcome::IdentityChange(_) => {
            return Err(fail(
                "PUBLISHER_IDENTITY_CHANGED",
                "explicit Host trust decision required",
            ));
        }
    };
    let expires_at = DateTime::parse_from_rfc3339(&keys.profile_expires_at)
        .map_err(|e| fail("PUBLISHER_INVALID", e))?
        .with_timezone(&Utc);
    Ok(Arc::new(PackagePublisher {
        domain,
        keys,
        pins,
        expires_at,
    }))
}

impl PackagePublisher {
    /// The Host persists this atomically immediately after accepting the
    /// descriptor, independently of whether an untrusted package verifies.
    pub fn pins_json(&self) -> String {
        serde_json::to_string(&self.pins).expect("KeyPins serialization is infallible")
    }

    pub fn verify_program(
        &self,
        source: PackageSourceInfo,
        archive: Vec<u8>,
    ) -> Result<VerifiedProgramInfo, CoreError> {
        let identity = identity(&source, &archive)?;
        if identity.domain != self.domain {
            return Err(fail(
                "PACKAGE_PUBLISHER_MISMATCH",
                "descriptor belongs to another Publisher",
            ));
        }
        let now = Utc::now();
        if now >= self.expires_at {
            return Err(fail(
                "PUBLISHER_EXPIRED",
                "refresh Publisher Profile before verifying",
            ));
        }
        let inspected =
            actr_pack::inspect_actor_package(&archive).map_err(|e| fail("PACKAGE_INVALID", e))?;
        let key_id = &inspected.manifest.signature.key_id;
        let key = self
            .keys
            .keys
            .iter()
            .find(|key| &key.id == key_id)
            .ok_or_else(|| {
                fail(
                    "PACKAGE_KEY_REVOKED",
                    "signing key is absent from current Publisher Profile",
                )
            })?;
        for (date, before) in [(&key.not_before, true), (&key.not_after, false)] {
            if let Some(value) = date {
                let time = DateTime::parse_from_rfc3339(value)
                    .map_err(|e| fail("PACKAGE_KEY_INVALID", e))?
                    .with_timezone(&Utc);
                if (before && now < time) || (!before && now >= time) {
                    return Err(fail(
                        "PACKAGE_KEY_EXPIRED",
                        "package signing key is outside validity period",
                    ));
                }
            }
        }
        // Both active and retired listed keys verify historic packages;
        // removal from this signed Profile is revocation.
        let raw = base64::engine::general_purpose::STANDARD
            .decode(&key.public_key)
            .map_err(|e| fail("PACKAGE_KEY_INVALID", e))?;
        let raw: [u8; 32] = raw
            .try_into()
            .map_err(|_| fail("PACKAGE_KEY_INVALID", "expected Ed25519 key"))?;
        let public_key =
            VerifyingKey::from_bytes(&raw).map_err(|e| fail("PACKAGE_KEY_INVALID", e))?;
        let verified = verify_program_release(
            &archive,
            &public_key,
            &format!("sha256:{}", identity.digest),
            &ProgramHostPolicy::default(),
        )
        .map_err(|e| fail("PROGRAM_VERIFICATION_FAILED", e))?;
        let manifest = verified.manifest();
        let display = manifest.extensions.actrium.as_ref();
        let guest_code = String::from_utf8(verified.guest_bytes().to_vec())
            .map_err(|e| fail("PROGRAM_GUEST_INVALID", e))?;
        let resources = manifest
            .files
            .iter()
            .filter(|f| {
                f.path != manifest.workload.entry
                    && f.path != verified.declaration().front.guest_entry
            })
            .map(|f| VerifiedProgramFileInfo {
                path: f.path.clone(),
                bytes: verified
                    .resource(&f.path)
                    .expect("verified payload set is complete")
                    .to_vec(),
            })
            .collect();
        Ok(VerifiedProgramInfo {
            publisher: manifest.actor.publisher.clone(),
            program: manifest.actor.name.clone(),
            version: manifest.actor.version.clone(),
            digest: identity.digest,
            signing_key_id: key.id.clone(),
            title: display
                .and_then(|x| x.title.clone())
                .unwrap_or_else(|| manifest.actor.name.clone()),
            description: display.and_then(|x| x.description.clone()),
            guest_code,
            service_bytes: verified.service_bytes().to_vec(),
            resources,
            metadata_json: serde_json::to_string(verified.declaration())
                .map_err(|e| fail("PROGRAM_METADATA_INVALID", e))?,
        })
    }
}
