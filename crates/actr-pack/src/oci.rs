//! Signed metadata for the OCI workload profile.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ActorPackageManifest, PackError, egress};

#[cfg(not(target_arch = "wasm32"))]
mod archive;
#[cfg(not(target_arch = "wasm32"))]
mod image;

#[cfg(not(target_arch = "wasm32"))]
pub use archive::{
    OciArchive, OciLimits, VerifiedOciPackage, pack_oci_package, read_file_manifest,
    read_open_file_manifest,
};

pub const OCI_PROFILE: &str = "actr.oci/1";
pub const OCI_HOST_PROFILE: &str = "actr.oci-host/1";
pub const OCI_IPC_PROFILE: &str = "actr.oci-ipc/1";
pub const OCI_AMD64_PLATFORM: &str = "linux/amd64";
pub const OCI_ARM64_PLATFORM: &str = "linux/arm64";
pub const DATA_READ_ONLY: &str = "fs.data.read-only";
pub const DATA_READ_WRITE: &str = "fs.data.read-write";
pub const ACTOR_RECEIVE: &str = "actor.receive";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OciWorkloadDescriptor {
    pub digest: String,
    pub platform: String,
    pub ipc: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

pub(crate) fn validate_oci_manifest(manifest: &ActorPackageManifest) -> Result<(), PackError> {
    let workload = &manifest.workload;
    if workload.profile != OCI_PROFILE {
        return require(
            workload.oci.is_none(),
            "[workload.oci] requires the actr.oci/1 profile",
        );
    }

    let oci = workload
        .oci
        .as_ref()
        .ok_or_else(|| invalid("actr.oci/1 requires [workload.oci]"))?;
    require(
        manifest.host.profile == OCI_HOST_PROFILE && manifest.host.api == "1.0.0",
        "actr.oci/1 requires actr.oci-host/1 API 1.0.0",
    )?;
    require(
        workload.entry == "index.json" || workload.entry.ends_with("/index.json"),
        "OCI workload entry must name the layout's index.json",
    )?;
    require(
        matches!(
            oci.platform.as_str(),
            OCI_AMD64_PLATFORM | OCI_ARM64_PLATFORM
        ) && oci.ipc == OCI_IPC_PROFILE,
        "unsupported OCI platform or IPC protocol",
    )?;
    parse_digest(&oci.digest)?;
    require(
        oci.args.first().is_some_and(|arg| {
            arg.strip_prefix('/').is_some_and(|path| {
                path.split('/')
                    .all(|part| !matches!(part, "" | "." | "..") && !part.contains('\\'))
            })
        }) && oci.args.iter().all(|arg| !arg.contains('\0')),
        "OCI args must start with an absolute, normalized executable path",
    )?;
    require(
        oci.env.iter().all(|(name, value)| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
                && !name.starts_with("ACTR_OCI_")
                && !value.contains('\0')
        }),
        "OCI env contains invalid or reserved entries",
    )?;
    require(
        manifest.host.optional_capabilities.is_empty(),
        "actr.oci/1 supports required capabilities only",
    )?;
    let mut capabilities = BTreeSet::new();
    for capability in &manifest.host.required_capabilities {
        if !matches!(
            capability.as_str(),
            DATA_READ_ONLY
                | DATA_READ_WRITE
                | ACTOR_RECEIVE
                | egress::ACTOR_CALL_CAPABILITY
                | egress::HTTPS_CAPABILITY
        ) || !capabilities.insert(capability.as_str())
        {
            return Err(invalid(format!(
                "unknown or repeated OCI capability `{capability}`"
            )));
        }
    }
    require(
        !(capabilities.contains(DATA_READ_ONLY) && capabilities.contains(DATA_READ_WRITE)),
        "OCI data directory access modes are mutually exclusive",
    )?;
    require(
        capabilities.contains(ACTOR_RECEIVE) == workload.rpc_contract.is_some(),
        "OCI actor.receive and workload.rpc-contract must be declared together",
    )?;
    require(
        manifest
            .files
            .iter()
            .filter(|file| file.role == "workload")
            .count()
            == 1,
        "OCI packages require one workload entry",
    )
}

pub(crate) fn parse_digest(digest: &str) -> Result<&str, PackError> {
    digest
        .strip_prefix("sha256:")
        .filter(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
        .ok_or_else(|| invalid("OCI digest must be a lowercase SHA-256 digest"))
}

pub(crate) fn invalid(message: impl Into<String>) -> PackError {
    PackError::InvalidPackage(message.into())
}

pub(crate) fn require(condition: bool, message: &str) -> Result<(), PackError> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}
