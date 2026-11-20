//! Derive Actrium v1 session compatibility declarations from a verified Front
//! package.
//!
//! A Service Host must know the exact Front it accepts: the Front Actor type,
//! its Guest digest, the Service protocol fingerprint, the business routes and
//! the runtime tuple. All of them are declared by the Front package, so the
//! same derivation is shared by the FFI host bridge, the packaged runtime
//! (`actr run`) and tests. Callers are responsible for verifying the archive
//! signature first; this module only reads declared package entries.

use std::collections::{BTreeMap, BTreeSet};

use actr_pack::ActorPackageManifest;
use actr_protocol::ActrType;
use actrium_runtime::Runtime;

use crate::error::HyperError;

pub const ACTRIUM_VERSION_V1: &str = "1.0.0";
pub const FRONT_HOST_API_V1: &str = "1.0";
pub const SERVICE_CAPABILITY: &str = "service";
pub const ATTACHMENT_UPLOAD_TRANSPORT: &str = "uploadAttachment";
pub const ATTACHMENT_PREPARE_ROUTE: &str = "ask.AskService.PrepareAttachmentUpload";
pub const ATTACHMENT_CANCEL_ROUTE: &str =
    "actrium.session.v1.SessionRegistry.CancelAttachmentResult";

/// Add the fixed wire routes for a declared reserved upload transport. The
/// transport alias remains configured for signed DataChunk metadata, while the
/// Prepare RPC and Host-only cancellation use their exact routes.
pub fn service_routes(
    declared: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut routes = declared.clone();
    if let Some(capabilities) = declared.get(ATTACHMENT_UPLOAD_TRANSPORT) {
        routes.insert(ATTACHMENT_PREPARE_ROUTE.to_owned(), capabilities.clone());
        routes.insert(ATTACHMENT_CANCEL_ROUTE.to_owned(), capabilities.clone());
    }
    routes
}

/// Exact compatibility declarations extracted from one verified Front package.
#[derive(Debug, Clone)]
pub struct FrontSessionMetadata {
    /// The Service Actor type the Front declares as `[dependencies.service]`.
    pub service_type: ActrType,
    /// The Front Actor type itself.
    pub front_actor_type: ActrType,
    /// SHA-256 of the whole Service archive.
    pub package_digest: Vec<u8>,
    /// SHA-256 of the Front workload payload.
    pub guest_digest: Vec<u8>,
    /// Semantic fingerprint of the Service Proto contract.
    pub protocol_fingerprint: Vec<u8>,
    /// The runtime tuple this Host implements.
    pub runtime: Runtime,
    /// Capabilities the Front may exercise on the Service.
    pub program_capabilities: BTreeSet<String>,
    /// Business routes declared by the Front capability manifest.
    pub routes: BTreeMap<String, BTreeSet<String>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontCapabilityManifest {
    keel_api: String,
    host_modules: BTreeMap<String, FrontHostModule>,
}

#[derive(serde::Deserialize)]
struct FrontHostModule {
    rpc: Option<Vec<String>>,
}

fn invalid(message: impl Into<String>) -> HyperError {
    HyperError::InvalidManifest(format!(
        "invalid Actrium Front session metadata: {}",
        message.into()
    ))
}

/// Derive the v1 session metadata for `service_type` from a verified Front
/// archive.
///
/// `front_manifest` must be the manifest verified against `front_archive`.
/// `service_package_digest` is the SHA-256 of the Service package bytes the
/// Host is about to run.
pub fn front_session_metadata(
    front_archive: &[u8],
    front_manifest: &ActorPackageManifest,
    service_type: &ActrType,
    service_package_digest: &[u8],
) -> Result<FrontSessionMetadata, HyperError> {
    use sha2::Digest as _;

    if service_package_digest.len() != 32 {
        return Err(invalid("the Service digest must be a SHA-256"));
    }

    let service_version = semver::Version::parse(&service_type.version)
        .map_err(|error| invalid(format!("Service version: {error}")))?;
    let dependency = front_manifest
        .dependencies
        .get("service")
        .ok_or_else(|| invalid("[dependencies.service] is required"))?;
    let expected_actor = format!("{}/{}", service_type.publisher, service_type.name);
    if !dependency.required || dependency.actor != expected_actor {
        return Err(invalid(format!(
            "the selected Service {expected_actor} does not match the Front dependency"
        )));
    }
    let dependency_requirement = semver::VersionReq::parse(&dependency.version)
        .map_err(|error| invalid(format!("Service dependency: {error}")))?;
    if !dependency_requirement.matches(&service_version) {
        return Err(invalid(format!(
            "Service version {service_version} does not satisfy {}",
            dependency.version
        )));
    }

    // The manifest carries a version requirement, whereas Attach carries the
    // concrete Host implementation version selected by this v1 native Host.
    let host_requirement = semver::VersionReq::parse(&front_manifest.host.api)
        .map_err(|error| invalid(format!("Host API requirement: {error}")))?;
    if !host_requirement.matches(&semver::Version::parse("1.0.0").expect("valid Host version")) {
        return Err(invalid(format!(
            "Host API {FRONT_HOST_API_V1} does not satisfy {}",
            front_manifest.host.api
        )));
    }

    let guest = read_declared(
        front_archive,
        front_manifest,
        &front_manifest.workload.entry,
    )?;
    let guest_digest = sha2::Sha256::digest(&guest).to_vec();

    let capabilities_path = front_manifest
        .workload
        .keel
        .as_ref()
        .and_then(|keel| keel.capabilities.as_deref())
        .ok_or_else(|| invalid("the Keel capability manifest is required"))?;
    let capability_bytes = read_declared(front_archive, front_manifest, capabilities_path)?;
    let capabilities: FrontCapabilityManifest = serde_json::from_slice(&capability_bytes)
        .map_err(|error| invalid(format!("capability manifest: {error}")))?;
    let rpc_routes = capabilities
        .host_modules
        .get(SERVICE_CAPABILITY)
        .or_else(|| capabilities.host_modules.get("host:service"))
        .and_then(|module| module.rpc.as_ref())
        .ok_or_else(|| invalid("hostModules.service.rpc is required"))?;
    let route_names: BTreeSet<&String> = rpc_routes.iter().collect();
    if route_names.is_empty() || route_names.iter().any(|route| route.is_empty()) {
        return Err(invalid(
            "hostModules.service.rpc must contain nonempty routes",
        ));
    }
    let routes: BTreeMap<String, BTreeSet<String>> = route_names
        .into_iter()
        .map(|route| {
            (
                route.clone(),
                BTreeSet::from([SERVICE_CAPABILITY.to_owned()]),
            )
        })
        .collect();

    let contract_path = dependency
        .contract
        .as_deref()
        .ok_or_else(|| invalid("the Service Proto contract is required"))?;
    let contract_bytes = read_declared(front_archive, front_manifest, contract_path)?;
    let contract = String::from_utf8(contract_bytes)
        .map_err(|error| invalid(format!("Service Proto is not UTF-8: {error}")))?;
    let contract_name = contract_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("the Service Proto basename is empty"))?;
    let fingerprint = actr_service_compat::Fingerprint::calculate_service_semantic_fingerprint(&[
        actr_service_compat::ProtoFile {
            name: contract_name.to_owned(),
            content: contract,
            path: None,
        },
    ])
    .map_err(|error| invalid(error.to_string()))?;
    let fingerprint_hex = fingerprint
        .strip_prefix("service_semantic:")
        .ok_or_else(|| invalid("unexpected Service fingerprint encoding"))?;
    let protocol_fingerprint = hex::decode(fingerprint_hex)
        .map_err(|error| invalid(format!("Service fingerprint: {error}")))?;

    Ok(FrontSessionMetadata {
        service_type: service_type.clone(),
        front_actor_type: ActrType {
            publisher: front_manifest.actor.publisher.clone(),
            name: front_manifest.actor.name.clone(),
            version: front_manifest.actor.version.clone(),
        },
        package_digest: service_package_digest.to_vec(),
        guest_digest,
        protocol_fingerprint,
        runtime: Runtime {
            actrium_version: ACTRIUM_VERSION_V1.to_owned(),
            front_host_api: FRONT_HOST_API_V1.to_owned(),
            keel_api: capabilities.keel_api,
        },
        program_capabilities: BTreeSet::from([SERVICE_CAPABILITY.to_owned()]),
        routes,
    })
}

fn read_declared(
    archive: &[u8],
    manifest: &ActorPackageManifest,
    path: &str,
) -> Result<Vec<u8>, HyperError> {
    manifest
        .read_declared_file(archive, path, actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES)
        .map_err(|error| invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_transport_adds_its_fixed_prepare_route() {
        let capabilities = BTreeSet::from([SERVICE_CAPABILITY.to_owned()]);
        let declared = BTreeMap::from([
            ("notes.save".to_owned(), capabilities.clone()),
            (ATTACHMENT_UPLOAD_TRANSPORT.to_owned(), capabilities.clone()),
        ]);

        let routes = service_routes(&declared);
        assert_eq!(routes.get("notes.save"), Some(&capabilities));
        assert_eq!(routes.get(ATTACHMENT_UPLOAD_TRANSPORT), Some(&capabilities));
        assert_eq!(routes.get(ATTACHMENT_PREPARE_ROUTE), Some(&capabilities));
        assert_eq!(routes.get(ATTACHMENT_CANCEL_ROUTE), Some(&capabilities));
    }

    #[test]
    fn business_routes_do_not_gain_the_upload_transport() {
        let capabilities = BTreeSet::from([SERVICE_CAPABILITY.to_owned()]);
        let declared = BTreeMap::from([("notes.save".to_owned(), capabilities.clone())]);
        assert_eq!(service_routes(&declared), declared);
    }
}
