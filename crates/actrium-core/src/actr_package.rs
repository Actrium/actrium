use thiserror::Error;

use crate::domain::Contract;

pub const SUPPORTED_CONTRACT_NAME: &str = "keel.guest";
pub const SUPPORTED_CONTRACT_VERSION: u32 = 1;
pub const ASSET_EXTENSIONS: [&str; 6] = [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"];
pub const SUPPORTED_INTEGRITY_ALGORITHM: &str = "sha256";

pub type ActrPackageManifest = actr_pack::ActorPackageManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestIntegrityFilePlan {
    pub path: String,
    pub expected_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestIntegrityPlan {
    pub algorithm: String,
    pub files: Vec<ManifestIntegrityFilePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractGate {
    pub status: ContractGateStatus,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractGateStatus {
    Ok,
    Compat,
    Blocked,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("unsupported contract `{actual}`, expected `{expected}`")]
    UnsupportedContract { actual: String, expected: String },
    #[error("contract `{name}` version {actual} is newer than supported version {supported}")]
    ContractTooNew {
        name: String,
        actual: u32,
        supported: u32,
    },
}

pub fn parse_manifest_toml(source: &str) -> Result<ActrPackageManifest, ManifestError> {
    ActrPackageManifest::from_toml(source)
        .map_err(|error| ManifestError::InvalidManifest(error.to_string()))
}

pub fn build_integrity_plan(
    manifest: &ActrPackageManifest,
) -> Result<ManifestIntegrityPlan, ManifestError> {
    manifest
        .validate()
        .map_err(|error| ManifestError::InvalidManifest(error.to_string()))?;
    Ok(ManifestIntegrityPlan {
        algorithm: SUPPORTED_INTEGRITY_ALGORITHM.to_string(),
        files: manifest
            .files
            .iter()
            .map(|file| ManifestIntegrityFilePlan {
                path: file.path.clone(),
                expected_hash: file.sha256.to_ascii_lowercase(),
            })
            .collect(),
    })
}

pub fn evaluate_contract(contract: Option<&Contract>) -> ContractGate {
    match contract {
        None => ContractGate {
            status: ContractGateStatus::Compat,
            blocked_reason: None,
        },
        Some(contract) if contract.name != SUPPORTED_CONTRACT_NAME => ContractGate {
            status: ContractGateStatus::Blocked,
            blocked_reason: Some(format!(
                "unsupported contract: {}; expected {}@v{}",
                contract.name, SUPPORTED_CONTRACT_NAME, SUPPORTED_CONTRACT_VERSION
            )),
        },
        Some(contract) if contract.version > SUPPORTED_CONTRACT_VERSION => ContractGate {
            status: ContractGateStatus::Blocked,
            blocked_reason: Some(format!(
                "contract is too new: {}@v{}; host supports v{}",
                contract.name, contract.version, SUPPORTED_CONTRACT_VERSION
            )),
        },
        Some(_) => ContractGate {
            status: ContractGateStatus::Ok,
            blocked_reason: None,
        },
    }
}

pub fn extract_app_id_from_url(url: &str) -> Option<String> {
    let marker = "/with/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let end = rest.find(['?', '#', '/']).unwrap_or(rest.len());
    let app_id = &rest[..end];
    (!app_id.is_empty()).then(|| app_id.to_string())
}

pub fn normalize_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut previous_was_slash = false;
    for (index, character) in path.chars().enumerate() {
        if character == '/' {
            if index == 0 || !previous_was_slash {
                normalized.push(character);
            }
            previous_was_slash = true;
        } else {
            normalized.push(character);
            previous_was_slash = false;
        }
    }
    normalized
}

pub fn relative_path_from(absolute_path: &str, base_dir: &str) -> String {
    let absolute_path = normalize_path(absolute_path);
    let base_dir = normalize_path(base_dir);
    let base_dir = base_dir.trim_end_matches('/');
    absolute_path
        .strip_prefix(base_dir)
        .unwrap_or(&absolute_path)
        .trim_start_matches('/')
        .to_string()
}

pub fn is_asset_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    ASSET_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

pub fn normalize_integrity_algorithm(algorithm: &str) -> Option<&'static str> {
    let normalized = algorithm.trim().to_ascii_lowercase().replace('-', "");
    (normalized == SUPPORTED_INTEGRITY_ALGORITHM).then_some(SUPPORTED_INTEGRITY_ALGORITHM)
}

pub fn normalize_integrity_path(path: &str) -> Option<String> {
    let normalized = normalize_path(path).trim_start_matches('/').to_string();
    if normalized.is_empty()
        || normalized
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return None;
    }
    Some(normalized)
}

pub fn normalize_sha256_hex(hash: &str) -> Option<String> {
    let normalized = hash.trim().to_ascii_lowercase();
    (normalized.len() == 64
        && normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit()))
    .then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r##"
format = "actr-package/1"

[actor]
manufacturer = "actrium"
name = "echo-ui"
version = "0.1.0"

[workload]
profile = "keel.guest/1"
entry = "workload/guest.js"

[workload.keel]
runtime = "js"
guest-api = "keel.guest/1"

[host]
profile = "actr.host/1"
api = "^1"
required-capabilities = ["ui.render"]

[signature]
algorithm = "ed25519"
key-id = "mfr-test"

[[files]]
path = "workload/guest.js"
role = "workload"
media-type = "text/javascript"
size = 1
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
"##;

    #[test]
    fn parses_versioned_manifest() {
        let manifest = parse_manifest_toml(MANIFEST).unwrap();
        assert_eq!(manifest.workload.profile, "keel.guest/1");
        assert_eq!(build_integrity_plan(&manifest).unwrap().files.len(), 1);
    }
}
