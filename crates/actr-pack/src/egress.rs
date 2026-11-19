//! Outbound capabilities scoped by a signed package Privacy Contract.

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::{ActorPackageManifest, PackError, util::sha256_hex};

pub const PRIVACY_CONTRACT_PATH: &str = "contracts/privacy.toml";
pub const HTTPS_CAPABILITY: &str = "net.https";
pub const ACTOR_CALL_CAPABILITY: &str = "actor.call";

/// Validated rules for the Actor release that owns the signed package.
#[derive(Debug)]
pub struct PackagePrivacyContract {
    pub sha256: String,
    pub egress: EgressPermissions,
    pub dependency_digests: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct EgressPermissions {
    pub https: BTreeMap<String, HttpsPermission>,
    pub actor: BTreeMap<String, ActorPermission>,
}

impl EgressPermissions {
    pub fn is_empty(&self) -> bool {
        self.https.is_empty() && self.actor.is_empty()
    }
}

#[derive(Debug, Deserialize)]
pub struct HttpsPermission {
    pub url: String,
    pub method: String,
    pub required: bool,
    pub data_classes: Vec<String>,
}

#[derive(Debug)]
pub struct ActorPermission {
    pub rpc_routes: Vec<String>,
    pub data_classes: Vec<String>,
    pub target_actor_type: String,
}

#[derive(Deserialize)]
struct ContractDocument {
    privacy_contract: Contract,
}

#[derive(Deserialize)]
struct Contract {
    schema_version: u32,
    release_id: String,
    #[serde(rename = "actor_role", default)]
    roles: Vec<ActorRole>,
    #[serde(rename = "edge", default)]
    edges: Vec<ActorEdge>,
    #[serde(rename = "terminal", default)]
    terminals: Vec<HttpsTerminal>,
}

#[derive(Deserialize)]
struct ActorRole {
    role: String,
    actr_type: Option<String>,
}

#[derive(Deserialize)]
struct ActorEdge {
    source: String,
    target: String,
    #[serde(default)]
    data_classes: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    dependency_alias: Option<String>,
    rpc_routes: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct HttpsTerminal {
    alias: String,
    source: String,
    #[serde(flatten)]
    permission: HttpsPermission,
}

pub(crate) fn validate_privacy_contract<B: AsRef<[u8]>>(
    manifest: &ActorPackageManifest,
    read: impl Fn(&str) -> Result<B, PackError>,
) -> Result<Option<PackagePrivacyContract>, PackError> {
    let entry = manifest
        .files
        .iter()
        .find(|entry| entry.path == PRIVACY_CONTRACT_PATH);
    let component = manifest.workload.profile == "actr.component/1";
    let mut privacy = None;
    if let Some(entry) = entry {
        if entry.role != "contract" {
            return Err(invalid("privacy.toml must have role contract"));
        }
        let raw = read(PRIVACY_CONTRACT_PATH)?;
        let raw = raw.as_ref();
        let sha256 = sha256_hex(raw);
        if raw.len() as u64 != entry.size || sha256 != entry.sha256 {
            return Err(invalid("Privacy Contract size or digest mismatch"));
        }
        let source = std::str::from_utf8(raw)
            .map_err(|error| invalid(format!("invalid Privacy Contract: {error}")))?;
        let doc: ContractDocument = toml::from_str(source)
            .map_err(|error| invalid(format!("invalid Privacy Contract: {error}")))?;
        let contract = doc.privacy_contract;
        if contract.schema_version != 1 || contract.release_id != manifest.actor_type() {
            return Err(invalid("Privacy Contract does not match selected Service"));
        }
        let egress = contract.into_permissions()?;
        if !egress.is_empty() && !component {
            return Err(invalid("scoped egress requires actr.component/1"));
        }
        let mut dependency_digests = BTreeMap::new();
        if !egress.actor.is_empty() {
            const LOCK: &str = "manifest.lock.toml";
            if !manifest
                .files
                .iter()
                .any(|file| file.path == LOCK && file.role == "lock")
            {
                return Err(invalid("Actor egress requires a signed manifest.lock.toml"));
            }
            dependency_digests = validate_lock(manifest, &egress, read(LOCK)?.as_ref())?;
            validate_actor_contracts(manifest, &egress, &read)?;
        }
        privacy = Some(PackagePrivacyContract {
            sha256,
            egress,
            dependency_digests,
        });
    }
    if component {
        for (capability, rules_present) in [
            (
                ACTOR_CALL_CAPABILITY,
                privacy
                    .as_ref()
                    .is_some_and(|contract| !contract.egress.actor.is_empty()),
            ),
            (
                HTTPS_CAPABILITY,
                privacy
                    .as_ref()
                    .is_some_and(|contract| !contract.egress.https.is_empty()),
            ),
        ] {
            let declared = manifest
                .host
                .required_capabilities
                .iter()
                .any(|id| id == capability);
            if declared != rules_present {
                return Err(invalid(format!(
                    "host capability {capability} and its local Privacy Contract rules must be declared together"
                )));
            }
        }
    }
    Ok(privacy)
}

impl Contract {
    fn into_permissions(self) -> Result<EgressPermissions, PackError> {
        let mut roles = BTreeMap::new();
        for role in self.roles {
            if roles.insert(role.role.clone(), role.actr_type).is_some() {
                return Err(invalid(format!(
                    "duplicate Privacy Contract role {}",
                    role.role
                )));
            }
        }

        let mut permissions = EgressPermissions::default();
        for edge in self.edges {
            if edge.dependency_alias.is_none() && edge.rpc_routes.is_none() {
                continue;
            }
            let (Some(alias), Some(rpc_routes)) = (edge.dependency_alias, edge.rpc_routes) else {
                return Err(invalid(
                    "scoped Actor edges require both dependency_alias and rpc_routes",
                ));
            };
            let source = role_actor_type(&roles, &edge.source)?;
            let target = role_actor_type(&roles, &edge.target)?;
            let mut distinct = BTreeSet::new();
            if !valid_alias(&alias)
                || !edge
                    .capabilities
                    .iter()
                    .any(|id| id == ACTOR_CALL_CAPABILITY)
                || !valid_data_classes(&edge.data_classes)
                || rpc_routes.is_empty()
                || !rpc_routes.iter().all(|route| {
                    route.contains('.')
                        && route.split('.').all(valid_proto_name)
                        && distinct.insert(route)
                })
            {
                return Err(invalid(format!("invalid scoped Actor edge {alias}")));
            }
            if source == self.release_id
                && permissions
                    .actor
                    .insert(
                        alias.clone(),
                        ActorPermission {
                            rpc_routes,
                            data_classes: edge.data_classes,
                            target_actor_type: target.to_owned(),
                        },
                    )
                    .is_some()
            {
                return Err(invalid(format!("duplicate Actor dependency alias {alias}")));
            }
        }

        for terminal in self.terminals {
            let source = role_actor_type(&roles, &terminal.source)?;
            validate_https(&terminal.alias, &terminal.permission)?;
            if source == self.release_id
                && permissions
                    .https
                    .insert(terminal.alias.clone(), terminal.permission)
                    .is_some()
            {
                return Err(invalid(format!(
                    "duplicate HTTPS terminal alias {}",
                    terminal.alias
                )));
            }
        }
        if !permissions.is_empty()
            && roles
                .values()
                .filter(|actor_type| actor_type.as_deref() == Some(self.release_id.as_str()))
                .count()
                != 1
        {
            return Err(invalid(
                "scoped egress requires exactly one local Actor role",
            ));
        }
        Ok(permissions)
    }
}

fn role_actor_type<'a>(
    roles: &'a BTreeMap<String, Option<String>>,
    role: &str,
) -> Result<&'a str, PackError> {
    let actor_type = roles
        .get(role)
        .and_then(|actor_type| actor_type.as_deref())
        .ok_or_else(|| {
            invalid(format!(
                "scoped egress role {role} requires an exact actr_type"
            ))
        })?;
    actr_protocol::ActrType::from_string_repr(actor_type)
        .map_err(|error| invalid(format!("invalid actr_type for role {role}: {error}")))?;
    Ok(actor_type)
}

fn valid_data_classes(classes: &[String]) -> bool {
    !classes.is_empty() && classes.iter().all(|value| !value.trim().is_empty())
}

fn validate_https(alias: &str, rule: &HttpsPermission) -> Result<(), PackError> {
    let url = url::Url::parse(&rule.url)
        .map_err(|error| invalid(format!("invalid HTTPS URL for {alias}: {error}")))?;
    let authority = rule
        .url
        .strip_prefix("https://")
        .and_then(|value| value.split(['/', '?', '#']).next());
    if !valid_alias(alias)
        || !valid_data_classes(&rule.data_classes)
        || !authority.is_some_and(|value| !value.is_empty() && !value.contains('@'))
        || !matches!(url.host(), Some(url::Host::Domain(_)))
        || url.port_or_known_default() != Some(443)
        || url.fragment().is_some()
        || rule.url.contains(['*', '\\'])
        || rule
            .url
            .chars()
            .any(|value| value.is_whitespace() || value.is_control())
        || !matches!(
            rule.method.as_str(),
            "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
        )
    {
        return Err(invalid(format!("invalid HTTPS terminal {alias}")));
    }
    Ok(())
}

#[derive(Deserialize)]
struct DigestLock {
    metadata: DigestLockMetadata,
    #[serde(rename = "dependency", default)]
    dependencies: Vec<DigestDependency>,
}

#[derive(Deserialize)]
struct DigestLockMetadata {
    version: u32,
}

#[derive(Deserialize)]
struct DigestDependency {
    alias: String,
    actr_type: String,
    fingerprint: String,
    #[serde(rename = "package-sha256")]
    package_sha256: Option<String>,
}

/// Validate the signed Unary routes at both signing and verification boundaries.
#[cfg(not(target_arch = "wasm32"))]
fn validate_actor_contracts<B: AsRef<[u8]>>(
    manifest: &ActorPackageManifest,
    permissions: &EgressPermissions,
    read: impl Fn(&str) -> Result<B, PackError>,
) -> Result<(), PackError> {
    for (alias, permission) in &permissions.actor {
        let path = manifest
            .dependencies
            .get(alias)
            .and_then(|dependency| dependency.contract.as_deref())
            .filter(|path| path.ends_with(".proto"))
            .ok_or_else(|| invalid(format!("Actor permission `{alias}` has no contract")))?;
        let bytes = read(path)?;
        let proto = std::str::from_utf8(bytes.as_ref())
            .map_err(|error| invalid(format!("{path}: {error}")))?;
        let routes = actr_service_compat::spec_builder::self_contained_unary_routes(proto)
            .map_err(|error| invalid(format!("{path}: {error}")))?;
        if let Some(route) = permission
            .rpc_routes
            .iter()
            .find(|route| !routes.contains(*route))
        {
            return Err(invalid(format!("{alias} has no unary RPC route {route}")));
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn validate_actor_contracts<B: AsRef<[u8]>>(
    _manifest: &ActorPackageManifest,
    permissions: &EgressPermissions,
    _read: impl Fn(&str) -> Result<B, PackError>,
) -> Result<(), PackError> {
    if permissions.actor.is_empty() {
        Ok(())
    } else {
        Err(invalid(
            "Actor permission contracts require a native verifier",
        ))
    }
}

fn validate_lock(
    manifest: &ActorPackageManifest,
    permissions: &EgressPermissions,
    bytes: &[u8],
) -> Result<BTreeMap<String, String>, PackError> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| invalid(format!("invalid manifest.lock.toml: {error}")))?;
    let lock: DigestLock = toml::from_str(source)
        .map_err(|error| invalid(format!("invalid egress dependency lock: {error}")))?;
    if lock.metadata.version != 2 {
        return Err(invalid("egress dependency locks must use version 2"));
    }
    let mut entries = BTreeMap::new();
    for entry in lock.dependencies {
        if entry.actr_type == manifest.actor_type() && entry.package_sha256.is_some() {
            return Err(invalid(format!(
                "dependency {} creates a self-referencing package digest",
                entry.alias
            )));
        }
        let alias = entry.alias.clone();
        if entries.insert(alias.clone(), entry).is_some() {
            return Err(invalid(format!("duplicate dependency lock alias {alias}")));
        }
    }
    let mut digests = BTreeMap::new();
    for (alias, permission) in &permissions.actor {
        let entry = entries
            .get(alias)
            .ok_or_else(|| invalid(format!("Actor permission {alias} requires a lock entry")))?;
        let declared = manifest.dependencies.get(alias).ok_or_else(|| {
            invalid(format!(
                "Actor edge alias {alias} is not a declared dependency"
            ))
        })?;
        let (actor, version) = entry.actr_type.rsplit_once('@').ok_or_else(|| {
            invalid(format!(
                "Actor permission {alias} requires a locked Actor type"
            ))
        })?;
        let matches_version = VersionReq::parse(&declared.version)
            .ok()
            .zip(Version::parse(version).ok())
            .is_some_and(|(requirement, version)| requirement.matches(&version));
        if entry.actr_type != permission.target_actor_type
            || actor != declared.actor
            || !matches_version
            || entry.fingerprint.trim().is_empty()
        {
            return Err(invalid(format!(
                "dependency lock does not match Actor permission {alias}"
            )));
        }
        if !entry.package_sha256.as_deref().is_some_and(is_digest) {
            return Err(invalid(format!(
                "dependency {alias} requires a lowercase package-sha256"
            )));
        }
        digests.insert(
            alias.clone(),
            entry
                .package_sha256
                .clone()
                .ok_or_else(|| invalid("missing package-sha256"))?,
        );
    }
    Ok(digests)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_proto_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn invalid(message: impl Into<String>) -> PackError {
    PackError::InvalidPackage(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorPackOptions, pack_actor_package, verify_actor_package};
    use ed25519_dalek::SigningKey;

    #[test]
    fn fixed_https_rules_require_the_signed_capability_and_reject_invalid_targets() {
        let mut manifest =
            ActorPackageManifest::from_toml(include_str!("../tests/fixtures/registration-v2.toml"))
                .unwrap();
        manifest.files.push(crate::PackageFileEntry {
            path: "contracts/privacy.toml".into(),
            role: "contract".into(),
            media_type: "application/toml".into(),
            size: 0,
            sha256: "0".repeat(64),
        });
        let fixed = "https://api.open-meteo.com/v1/forecast?latitude=35.6762&longitude=139.6503";
        let privacy = |actor: &str, url: &str| {
            format!(
                r#"
[privacy_contract]
schema_version = 1
release_id = {actor:?}
[[privacy_contract.actor_role]]
role = "notebook"
actr_type = {actor:?}
[[privacy_contract.actor_role]]
role = "remote"
actr_type = "egress.example/weather@1.0.0"
[[privacy_contract.terminal]]
alias = "tokyo-weather"
source = "notebook"
url = {url:?}
method = "GET"
required = false
data_classes = ["public"]
"#
            )
        };
        let actor_type = manifest.actor_type();
        let contract = privacy(&actor_type, fixed);
        let mut options = ActorPackOptions {
            payloads: manifest
                .files
                .iter()
                .map(|entry| (entry.path.clone(), Vec::new()))
                .collect(),
            manifest,
            signing_key: SigningKey::from_bytes(&[23; 32]),
        };
        options
            .payloads
            .insert(PRIVACY_CONTRACT_PATH.into(), contract.as_bytes().to_vec());
        assert!(
            pack_actor_package(&options)
                .unwrap_err()
                .to_string()
                .contains(HTTPS_CAPABILITY)
        );
        options
            .manifest
            .host
            .required_capabilities
            .push(HTTPS_CAPABILITY.into());
        let archive = pack_actor_package(&options).unwrap();
        let verified =
            verify_actor_package(&archive, &options.signing_key.verifying_key()).unwrap();
        let verified = verified
            .manifest
            .read_privacy_contract(&archive, 4096)
            .unwrap()
            .unwrap();
        assert_eq!(verified.egress.https["tokyo-weather"].url, fixed);
        assert_eq!(verified.sha256, sha256_hex(contract.as_bytes()));

        for invalid_url in [
            "http://api.open-meteo.com/v1/forecast",
            "https://user@api.open-meteo.com/v1/forecast",
            "https://api.open-meteo.com/v1/*",
            "https://api.open-meteo.com/v1/forecast#fragment",
            "https://api.open-meteo.com:444/v1/forecast",
        ] {
            options.payloads.insert(
                PRIVACY_CONTRACT_PATH.into(),
                privacy(&actor_type, invalid_url).into_bytes(),
            );
            assert!(pack_actor_package(&options).is_err());
        }

        let remote = contract.replace("source = \"notebook\"", "source = \"remote\"");
        options
            .payloads
            .insert(PRIVACY_CONTRACT_PATH.into(), remote.into_bytes());
        assert!(
            pack_actor_package(&options)
                .unwrap_err()
                .to_string()
                .contains(HTTPS_CAPABILITY)
        );
        options.manifest.host.required_capabilities.clear();
        let archive = pack_actor_package(&options).unwrap();
        let verified =
            verify_actor_package(&archive, &options.signing_key.verifying_key()).unwrap();
        let verified = verified
            .manifest
            .read_privacy_contract(&archive, 4096)
            .unwrap()
            .unwrap();
        assert!(verified.egress.https.is_empty());
    }

    #[test]
    fn signed_egress_lock_requires_exact_target_digest() {
        let manifest = format!(
            r#"
format = "actr-package/2"
[actor]
publisher = "egress.example"
name = "notebook"
version = "1.0.0"
[workload]
profile = "actr.component/1"
entry = "notebook.wasm"
[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["actor.call"]
[dependencies.index]
actor = "egress.example/index"
version = "^1.0.0"
contract = "contracts/index.proto"
required = false
[signature]
algorithm = "ed25519"
key-id = "placeholder"
{}
"#,
            [
                ("notebook.wasm", "workload"),
                ("contracts/index.proto", "contract"),
                ("contracts/privacy.toml", "contract"),
                ("manifest.lock.toml", "lock"),
            ]
            .map(|(path, role)| format!(
                "[[files]]\npath = \"{path}\"\nrole = \"{role}\"\nmedia-type = \"application/octet-stream\"\nsize = 0\nsha256 = \"{}\"\n", "0".repeat(64),
            ))
            .join("\n")
        );
        let manifest = ActorPackageManifest::from_toml(&manifest).unwrap();
        let mut payloads: BTreeMap<_, _> = manifest
            .files
            .iter()
            .map(|file| (file.path.clone(), Vec::new()))
            .collect();
        payloads.insert(
            "contracts/index.proto".into(),
            br#"syntax = "proto3";
package notes;
message SearchRequest {}
message SearchResponse {}
service Index { rpc Search(SearchRequest) returns (SearchResponse); }
"#
            .to_vec(),
        );
        payloads.insert(
            PRIVACY_CONTRACT_PATH.into(),
            br#"
[privacy_contract]
schema_version = 1
release_id = "egress.example/notebook@1.0.0"
[[privacy_contract.actor_role]]
role = "notebook"
actr_type = "egress.example/notebook@1.0.0"
[[privacy_contract.actor_role]]
role = "index"
actr_type = "egress.example/index@1.0.2"
[[privacy_contract.edge]]
source = "notebook"
target = "index"
data_classes = ["private"]
capabilities = ["actor.call"]
dependency_alias = "index"
rpc_routes = ["notes.Index.Search"]
"#
            .to_vec(),
        );
        let lock = r#"
[metadata]
version = 2
[[dependency]]
alias = "index"
actr_type = "egress.example/index@1.0.2"
fingerprint = "service_semantic:example"
"#;
        payloads.insert("manifest.lock.toml".into(), lock.as_bytes().to_vec());
        let mut options = ActorPackOptions {
            manifest,
            payloads,
            signing_key: SigningKey::from_bytes(&[19; 32]),
        };
        assert!(
            pack_actor_package(&options)
                .unwrap_err()
                .to_string()
                .contains("package-sha256")
        );
        let digest = "bc".repeat(32);
        options.payloads.insert(
            "manifest.lock.toml".into(),
            format!("{lock}package-sha256 = \"{digest}\"\n").into_bytes(),
        );
        let bytes = pack_actor_package(&options).unwrap();
        let verified = verify_actor_package(&bytes, &options.signing_key.verifying_key()).unwrap();
        let contract = verified
            .manifest
            .read_privacy_contract(&bytes, 4096)
            .unwrap()
            .unwrap();
        assert_eq!(contract.dependency_digests.get("index"), Some(&digest));
    }
}
