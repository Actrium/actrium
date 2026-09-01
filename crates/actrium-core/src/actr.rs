use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{Contract, SystemAppType};

pub const SUPPORTED_CONTRACT_NAME: &str = "ask";
pub const SUPPORTED_CONTRACT_VERSION: u32 = 1;
pub const ASSET_EXTENSIONS: [&str; 6] = [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"];
pub const SUPPORTED_INTEGRITY_ALGORITHM: &str = "sha256";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestIntegrity {
    pub algorithm: String,
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActrManifest {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "bundleId", skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub entry: String,
    #[serde(default)]
    pub left_panel_default_visible: bool,
    #[serde(default)]
    pub right_panel_default_visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<Contract>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<ManifestIntegrity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_app_type: Option<SystemAppType>,
    #[serde(
        default,
        rename = "themeColor",
        skip_serializing_if = "Option::is_none"
    )]
    pub theme_color: Option<String>,
    #[serde(
        default,
        rename = "backgroundColor",
        skip_serializing_if = "Option::is_none"
    )]
    pub background_color: Option<String>,
}

impl ActrManifest {
    pub fn app_id(&self) -> Option<&str> {
        self.id.as_deref().or(self.bundle_id.as_deref())
    }

    pub fn effective_theme_color(&self) -> Option<&str> {
        self.theme_color
            .as_deref()
            .or(self.background_color.as_deref())
    }
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
    #[error("manifest JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("manifest is missing required field `{0}`")]
    MissingField(&'static str),
    #[error("unsupported contract `{actual}`, expected `{expected}`")]
    UnsupportedContract { actual: String, expected: String },
    #[error("contract `{name}` version {actual} is newer than supported version {supported}")]
    ContractTooNew {
        name: String,
        actual: u32,
        supported: u32,
    },
    #[error("unsupported integrity algorithm `{actual}`, expected `{expected}`")]
    UnsupportedIntegrityAlgorithm { actual: String, expected: String },
    #[error("manifest integrity path `{0}` is invalid")]
    InvalidIntegrityPath(String),
    #[error("manifest integrity hash for `{path}` is invalid")]
    InvalidIntegrityHash { path: String },
}

pub fn parse_manifest_json(json: &str) -> Result<ActrManifest, ManifestError> {
    let manifest: ActrManifest =
        serde_json::from_str(json).map_err(|err| ManifestError::InvalidJson(err.to_string()))?;

    if manifest.name.trim().is_empty() {
        return Err(ManifestError::MissingField("name"));
    }
    if manifest.version.trim().is_empty() {
        return Err(ManifestError::MissingField("version"));
    }
    if manifest.entry.trim().is_empty() {
        return Err(ManifestError::MissingField("entry"));
    }

    validate_contract(manifest.contract.as_ref())?;
    Ok(manifest)
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
                "Unsupported contract: {}(Support only) {}@v{})",
                contract.name, SUPPORTED_CONTRACT_NAME, SUPPORTED_CONTRACT_VERSION
            )),
        },
        Some(contract) if contract.version > SUPPORTED_CONTRACT_VERSION => ContractGate {
            status: ContractGateStatus::Blocked,
            blocked_reason: Some(format!(
                "contractVersion Too new:{}@v{}(The host only supports) v{})",
                contract.name, contract.version, SUPPORTED_CONTRACT_VERSION
            )),
        },
        Some(_) => ContractGate {
            status: ContractGateStatus::Ok,
            blocked_reason: None,
        },
    }
}

pub fn validate_contract(contract: Option<&Contract>) -> Result<(), ManifestError> {
    if let Some(contract) = contract {
        if contract.name != SUPPORTED_CONTRACT_NAME {
            return Err(ManifestError::UnsupportedContract {
                actual: contract.name.clone(),
                expected: SUPPORTED_CONTRACT_NAME.to_string(),
            });
        }
        if contract.version > SUPPORTED_CONTRACT_VERSION {
            return Err(ManifestError::ContractTooNew {
                name: contract.name.clone(),
                actual: contract.version,
                supported: SUPPORTED_CONTRACT_VERSION,
            });
        }
    }
    Ok(())
}

pub fn extract_app_id_from_url(url: &str) -> Option<String> {
    let marker = "/with/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let end = rest.find(['?', '#', '/']).unwrap_or(rest.len());
    let app_id = &rest[..end];
    if app_id.is_empty() {
        None
    } else {
        Some(app_id.to_string())
    }
}

pub fn normalize_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut previous_was_slash = false;

    for (idx, ch) in path.chars().enumerate() {
        if ch == '/' {
            if idx == 0 || !previous_was_slash {
                normalized.push(ch);
            }
            previous_was_slash = true;
        } else {
            normalized.push(ch);
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
    if normalized == SUPPORTED_INTEGRITY_ALGORITHM {
        Some(SUPPORTED_INTEGRITY_ALGORITHM)
    } else {
        None
    }
}

pub fn normalize_integrity_path(path: &str) -> Option<String> {
    let normalized = normalize_path(path).trim_start_matches('/').to_string();
    if normalized.is_empty() {
        return None;
    }

    if normalized
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return None;
    }

    Some(normalized)
}

pub fn normalize_sha256_hex(hash: &str) -> Option<String> {
    let normalized = hash.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(normalized)
}

pub fn build_integrity_plan(
    integrity: Option<&ManifestIntegrity>,
) -> Result<Option<ManifestIntegrityPlan>, ManifestError> {
    let Some(integrity) = integrity else {
        return Ok(None);
    };

    let algorithm = normalize_integrity_algorithm(&integrity.algorithm).ok_or_else(|| {
        ManifestError::UnsupportedIntegrityAlgorithm {
            actual: integrity.algorithm.clone(),
            expected: SUPPORTED_INTEGRITY_ALGORITHM.to_string(),
        }
    })?;

    let mut files = Vec::with_capacity(integrity.files.len());
    for (path, hash) in &integrity.files {
        let path = normalize_integrity_path(path)
            .ok_or_else(|| ManifestError::InvalidIntegrityPath(path.clone()))?;
        let expected_hash = normalize_sha256_hex(hash)
            .ok_or_else(|| ManifestError::InvalidIntegrityHash { path: path.clone() })?;

        files.push(ManifestIntegrityFilePlan {
            path,
            expected_hash,
        });
    }

    Ok(Some(ManifestIntegrityPlan {
        algorithm: algorithm.to_string(),
        files,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_normalizes_aliases() {
        let manifest = parse_manifest_json(
            r##"{
                "name": "Demo",
                "version": "1.0.0",
                "bundleId": "app.demo",
                "entry": "dist/unified.js",
                "themeColor": "#223344",
                "contract": { "name": "ask", "version": 1 },
                "permissions": ["network"],
                "systemAppType": "history"
            }"##,
        )
        .unwrap();

        assert_eq!(manifest.app_id(), Some("app.demo"));
        assert_eq!(manifest.effective_theme_color(), Some("#223344"));
        assert_eq!(manifest.system_app_type, Some(SystemAppType::History));
    }

    #[test]
    fn blocks_too_new_contracts() {
        let error = parse_manifest_json(
            r#"{"name":"Demo","version":"1","entry":"main.js","contract":{"name":"ask","version":2}}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            ManifestError::ContractTooNew {
                name: "ask".to_string(),
                actual: 2,
                supported: 1,
            }
        );
    }

    #[test]
    fn matches_runtime_app_id_extraction() {
        assert_eq!(
            extract_app_id_from_url("https://go.actrium-dev.example/with/app-123?x=1"),
            Some("app-123".to_string())
        );
        assert_eq!(
            extract_app_id_from_url("https://go.actrium-dev.example/app/app-123"),
            None
        );
    }

    #[test]
    fn path_helpers_match_loader_semantics() {
        assert_eq!(normalize_path("/tmp//actro///app"), "/tmp/actro/app");
        assert_eq!(
            relative_path_from("/tmp/app/assets/icon.png", "/tmp/app/"),
            "assets/icon.png"
        );
        assert!(is_asset_path("assets/icon.WEBP"));
        assert!(!is_asset_path("dist/main.js"));
    }

    #[test]
    fn builds_integrity_plan_with_normalized_paths_and_hashes() {
        let integrity = ManifestIntegrity {
            algorithm: "SHA-256".to_string(),
            files: std::collections::BTreeMap::from([(
                "assets//icon.png".to_string(),
                "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789".to_string(),
            )]),
        };

        let plan = build_integrity_plan(Some(&integrity)).unwrap().unwrap();

        assert_eq!(plan.algorithm, "sha256");
        assert_eq!(plan.files[0].path, "assets/icon.png");
        assert_eq!(
            plan.files[0].expected_hash,
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
    }

    #[test]
    fn rejects_integrity_path_traversal_and_bad_hashes() {
        assert_eq!(normalize_integrity_path("../secret"), None);
        assert_eq!(normalize_integrity_path("assets/../secret"), None);
        assert_eq!(normalize_sha256_hex("not-a-sha256"), None);
    }
}
