//! Verify the Service and Front chosen by a signed Realm Profile.
use actr_protocol::PackageSigningKey;
use actr_protocol::realm_profile::RealmPackage;
use semver::{Version, VersionReq};

use crate::actor_package::{
    ActorPackageManifest, InspectedActorPackage, KEEL_GUEST_PROFILE_V1,
    inspect_actor_package_bounded,
};
use crate::{PackError, util::sha256_hex};

/// `keys` must come from the accepted Publisher Profile for `reference.type`.
pub fn verify_realm_archive(
    reference: &RealmPackage,
    bytes: &[u8],
    keys: &[PackageSigningKey],
    limit: usize,
) -> Result<InspectedActorPackage, PackError> {
    if bytes.len() > limit || sha256_hex(bytes) != reference.sha256 {
        return Err(PackError::InvalidPackage(
            "Realm archive size or digest mismatch".into(),
        ));
    }
    let verified = inspect_actor_package_bounded(bytes, limit, limit)?;
    let invalid = |message: &str| PackError::SignatureVerificationFailed(message.into());
    let key = keys
        .iter()
        .find(|k| k.id == verified.manifest.signature.key_id)
        .ok_or_else(|| invalid("key is not listed by the package Publisher"))?;
    let key = key
        .verification_key()
        .map_err(PackError::SignatureVerificationFailed)?;
    verified.verify_signature(&key)?;
    if verified.manifest.actor_type() != reference.r#type {
        return Err(PackError::InvalidPackage(
            "Realm archive Actor type mismatch".into(),
        ));
    }
    Ok(verified)
}

/// Check the signed manifests after both archives have passed `verify_realm_archive`.
/// Platform capabilities and permission grants are still enforced by the loading Host.
pub fn check_realm_pair(
    service: &ActorPackageManifest,
    front: &ActorPackageManifest,
) -> Result<(), PackError> {
    let invalid = |reason: &str| PackError::InvalidPackage(reason.into());
    if !matches!(
        service.workload.profile.as_str(),
        "actr.component/1" | "actr.native/1" | "actr.linked/1"
    ) || front.workload.profile != KEEL_GUEST_PROFILE_V1
    {
        return Err(invalid(
            "unsupported Realm Service or Front workload profile",
        ));
    }
    let dep = front
        .dependencies
        .get("service")
        .filter(|d| d.required)
        .ok_or_else(|| invalid("Front must declare a required service dependency"))?;
    let expected = format!("{}/{}", service.actor.publisher, service.actor.name);
    let constraint = VersionReq::parse(&dep.version)
        .map_err(|_| invalid("invalid Front Service version requirement"))?;
    let version =
        Version::parse(&service.actor.version).map_err(|_| invalid("invalid Service version"))?;
    if dep.actor != expected || !constraint.matches(&version) {
        return Err(invalid(
            "Front service dependency does not match selected Service",
        ));
    }
    let front_path = dep
        .contract
        .as_deref()
        .filter(|p| p.ends_with(".proto"))
        .ok_or_else(|| invalid("Front service dependency needs an RPC Proto contract"))?;
    let front_contract = front
        .files
        .iter()
        .find(|file| file.role == "contract" && file.path == front_path)
        .ok_or_else(|| invalid("Front does not declare its service dependency's RPC Proto"))?;
    let matches_export = if service.workload.profile == "actr.component/1" {
        let export = service
            .component_rpc_contract()?
            .ok_or_else(|| invalid("Service component has no exported RPC Proto"))?;
        service
            .files
            .iter()
            .any(|file| file.path == export && file.sha256 == front_contract.sha256)
    } else {
        service
            .local_rpc_contracts()
            .any(|file| file.sha256 == front_contract.sha256)
    };
    if !matches_export {
        return Err(invalid("Service and Front RPC contracts differ"));
    }
    Ok(())
}

/// Authenticate the optional contract carried by an already verified Service archive.
/// This returns evidence for policy evaluation; it does not grant any capability.
pub fn privacy_contract_digest(
    manifest: &ActorPackageManifest,
    archive: &[u8],
    limit: usize,
) -> Result<Option<String>, PackError> {
    const PATH: &str = "contracts/privacy.toml";
    let Some(entry) = manifest.files.iter().find(|f| f.path == PATH) else {
        return Ok(None);
    };
    if entry.role != "contract" {
        return Err(PackError::InvalidPackage(
            "privacy.toml must have role contract".into(),
        ));
    }
    let bytes = manifest.read_declared_file(archive, PATH, limit)?;
    if sha256_hex(&bytes) != entry.sha256 {
        return Err(PackError::InvalidPackage(
            "Privacy Contract digest mismatch".into(),
        ));
    }
    #[derive(serde::Deserialize)]
    struct Document {
        privacy_contract: Contract,
    }
    #[derive(serde::Deserialize)]
    struct Contract {
        schema_version: u32,
        release_id: String,
    }
    let doc: Document = toml::from_str(
        std::str::from_utf8(&bytes).map_err(|e| PackError::InvalidPackage(e.to_string()))?,
    )
    .map_err(|e| PackError::InvalidPackage(format!("invalid Privacy Contract: {e}")))?;
    if doc.privacy_contract.schema_version != 1
        || doc.privacy_contract.release_id != manifest.actor_type()
    {
        return Err(PackError::InvalidPackage(
            "Privacy Contract does not match selected Service".into(),
        ));
    }
    Ok(Some(entry.sha256.clone()))
}
