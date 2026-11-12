//! Validate native v2 package requirements before instantiating the workload.

use std::str::FromStr;

use actr_config::lock::LockFile;
use actr_pack::ActorPackageManifest;
use actr_protocol::ActrType;
use semver::{Version, VersionReq};

use crate::error::{HyperError, HyperResult};

pub(crate) fn validate_component_host(manifest: &ActorPackageManifest) -> HyperResult<()> {
    if manifest.workload.profile != "actr.component/1" || manifest.host.profile != "actr.host/1" {
        return Err(HyperError::InvalidManifest(format!(
            "unsupported native workload/host profile combination: {} / {}",
            manifest.workload.profile, manifest.host.profile
        )));
    }
    let host_api = Version::new(1, 0, 0);
    let requirement = VersionReq::parse(&manifest.host.api)
        .map_err(|e| HyperError::InvalidManifest(format!("invalid host.api: {e}")))?;
    if !requirement.matches(&host_api) {
        return Err(HyperError::InvalidManifest(format!(
            "native host API {host_api} does not satisfy {}",
            manifest.host.api
        )));
    }
    if !manifest.host.required_capabilities.is_empty() {
        return Err(HyperError::InvalidManifest(format!(
            "native Component packages cannot require capabilities: {}",
            manifest.host.required_capabilities.join(", ")
        )));
    }
    Ok(())
}

pub(crate) fn parse_lock(bytes: &[u8]) -> HyperResult<LockFile> {
    let raw = std::str::from_utf8(bytes).map_err(|e| {
        HyperError::InvalidManifest(format!("manifest.lock.toml is not valid UTF-8: {e}"))
    })?;
    LockFile::from_str(raw).map_err(|e| {
        HyperError::InvalidManifest(format!("failed to parse manifest.lock.toml: {e}"))
    })
}

pub(crate) fn read_v2_lock(
    bytes: &[u8],
    manifest: &ActorPackageManifest,
    max_bytes: usize,
) -> HyperResult<Option<LockFile>> {
    const LOCK_PATH: &str = "manifest.lock.toml";
    let mut lock = None;
    for file in &manifest.files {
        if file.path == LOCK_PATH && file.role == "lock" {
            let raw = manifest
                .read_declared_file(bytes, LOCK_PATH, max_bytes)
                .map_err(|e| HyperError::InvalidManifest(e.to_string()))?;
            lock = Some(parse_lock(&raw)?);
        } else if file.path == LOCK_PATH || file.role == "lock" {
            return Err(HyperError::InvalidManifest(
                "v2 dependency lock must be manifest.lock.toml with role `lock`".into(),
            ));
        }
    }
    if let Some(lock) = &lock {
        lock.validate_aliases()
            .map_err(|e| HyperError::InvalidManifest(format!("invalid packaged lock: {e}")))?;
        for entry in &lock.dependencies {
            let declared = manifest.dependencies.get(&entry.alias).ok_or_else(|| {
                HyperError::InvalidManifest(format!(
                    "undeclared dependency alias `{}` in lock",
                    entry.alias
                ))
            })?;
            let actor_type = ActrType::from_string_repr(&entry.actr_type)
                .map_err(|e| HyperError::InvalidManifest(e.to_string()))?;
            let version = Version::parse(&actor_type.version)
                .map_err(|e| HyperError::InvalidManifest(e.to_string()))?;
            let constraint = VersionReq::parse(&declared.version).map_err(|e| {
                HyperError::InvalidManifest(format!(
                    "invalid dependency `{}` version: {e}",
                    entry.alias
                ))
            })?;
            if declared.actor != format!("{}/{}", actor_type.publisher, actor_type.name)
                || !constraint.matches(&version)
            {
                return Err(HyperError::InvalidManifest(format!(
                    "locked actor `{}` does not satisfy dependency `{}`",
                    entry.actr_type, entry.alias
                )));
            }
        }
    }
    for (alias, dependency) in &manifest.dependencies {
        if dependency.required
            && !lock
                .as_ref()
                .is_some_and(|lock| lock.dependencies.iter().any(|entry| &entry.alias == alias))
        {
            return Err(HyperError::InvalidManifest(format!(
                "required dependency `{alias}` is missing from the signed package lock"
            )));
        }
    }
    Ok(lock)
}
