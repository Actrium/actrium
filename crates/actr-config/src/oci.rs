//! Host-only deployment configuration for OCI workloads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct OciRuntimeConfig {
    pub syd_oci: PathBuf,
    pub umoci: PathBuf,
    pub bubblewrap: PathBuf,
    pub supervisor: PathBuf,
    pub state_dir: PathBuf,
    pub image_dir: PathBuf,
    pub policy: PathBuf,
    pub cgroup_parent: PathBuf,
    pub package_sha256: String,
    #[serde(default)]
    pub allowed_capabilities: Vec<String>,
    #[serde(default)]
    pub data: Option<OciDataConfig>,
    #[serde(default)]
    pub https: Vec<OciHttpsGrant>,
    #[serde(default)]
    pub actors: Vec<OciActorGrant>,
    #[serde(default = "default_package_bytes")]
    pub max_package_bytes: u64,
    #[serde(default = "default_unpack_bytes")]
    pub max_unpack_bytes: u64,
    #[serde(default = "default_unpack_inodes")]
    pub max_unpack_inodes: u64,
    /// Hard cgroup memory limit for the Guest, in bytes.
    #[serde(default = "default_memory_limit_bytes")]
    pub memory_limit_bytes: i64,
    /// Swap-only limit in bytes. Zero disables Guest swap.
    #[serde(default)]
    pub swap_limit_bytes: Option<u64>,
}

/// Provisioned data volume granted to one OCI Actor instance.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct OciDataConfig {
    pub path: PathBuf,
    pub access: crate::WasiDataAccess,
    pub max_bytes: u64,
    pub max_inodes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct OciHttpsGrant {
    pub alias: String,
    pub url: String,
    pub method: String,
    pub data_classes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct OciActorGrant {
    pub alias: String,
    pub target: String,
    pub actor_type: String,
    pub package_sha256: String,
    pub contract_fingerprint: String,
    pub routes: Vec<String>,
    pub data_classes: Vec<String>,
    #[serde(default)]
    pub allow_tell: bool,
}

impl OciRuntimeConfig {
    /// Resolve host paths relative to the runtime configuration file.
    pub fn resolve_paths(mut self, base: &Path) -> Self {
        for path in [
            &mut self.syd_oci,
            &mut self.umoci,
            &mut self.bubblewrap,
            &mut self.supervisor,
            &mut self.state_dir,
            &mut self.image_dir,
            &mut self.policy,
            &mut self.cgroup_parent,
        ] {
            if path.is_relative() {
                *path = base.join(&*path);
            }
        }
        if let Some(data) = self.data.as_mut() {
            if data.path.is_relative() {
                data.path = base.join(&data.path);
            }
        }
        self
    }
}

const fn default_memory_limit_bytes() -> i64 {
    256 * 1024 * 1024
}

const fn default_package_bytes() -> u64 {
    8 * 1024 * 1024 * 1024
}

const fn default_unpack_bytes() -> u64 {
    16 * 1024 * 1024 * 1024
}

const fn default_unpack_inodes() -> u64 {
    1_000_000
}
