//! DispatchSpec v1 — JSON descriptor of all RPC routes.
//!
//! The spec is produced by `actr-cli` and consumed by this plugin to generate
//! the unified TS dispatcher. The spec file is written to
//! `<output>/.actr/codegen/actr-dispatch-spec.v1.json` alongside a SHA-256
//! hash of the canonical JSON (RFC 8785 / JCS) of the spec *without* the
//! `spec_hash` field.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const DISPATCH_SPEC_SCHEMA_VERSION: &str = "v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRoute {
    pub route_key: String,
    pub proto_file: String,
    pub proto_service: String,
    pub method: String,
    pub input_type: String,
    pub output_type: String,
    pub payload_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_alias: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchSpec {
    pub schema_version: String,
    pub routes: Vec<DispatchRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_hash: Option<String>,
}

impl DispatchSpec {
    /// Load and validate a DispatchSpec from a JSON file.
    ///
    /// If `expected_hash` is provided, the spec's `spec_hash` is verified
    /// against it, and the canonical hash is recomputed to ensure integrity.
    pub fn load_and_validate(path: &Path, expected_hash: Option<&str>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read dispatch spec {}: {e}", path.display()))?;

        let spec: DispatchSpec = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse dispatch spec JSON: {e}"))?;

        if spec.schema_version != DISPATCH_SPEC_SCHEMA_VERSION {
            return Err(format!(
                "Unexpected dispatch spec schema_version: expected '{}', got '{}'",
                DISPATCH_SPEC_SCHEMA_VERSION, spec.schema_version
            ));
        }

        spec.validate_routes()?;

        // If an expected hash is provided, verify it matches the spec's hash.
        if let Some(expected) = expected_hash {
            let actual = spec.spec_hash.as_deref().unwrap_or("");
            if actual != expected {
                return Err(format!(
                    "Dispatch spec hash mismatch: expected '{expected}', got '{actual}'"
                ));
            }
        }

        // Recompute the canonical hash and verify.
        let spec_without_hash = DispatchSpec {
            schema_version: spec.schema_version.clone(),
            routes: spec.routes.clone(),
            spec_hash: None,
        };
        let canonical = canonicalize_json(&spec_without_hash);
        let recomputed = compute_sha256_hex(&canonical);

        if let Some(ref stored) = spec.spec_hash {
            if stored != &recomputed {
                return Err(format!(
                    "Dispatch spec hash integrity check failed: stored '{stored}' != recomputed '{recomputed}'"
                ));
            }
        }

        Ok(spec)
    }

    fn validate_routes(&self) -> Result<(), String> {
        let mut route_keys = BTreeSet::new();
        let mut service_aliases: BTreeMap<&str, BTreeSet<Option<&str>>> = BTreeMap::new();
        for route in &self.routes {
            for (field, value) in [
                ("route_key", route.route_key.as_str()),
                ("proto_file", route.proto_file.as_str()),
                ("proto_service", route.proto_service.as_str()),
                ("method", route.method.as_str()),
                ("input_type", route.input_type.as_str()),
                ("output_type", route.output_type.as_str()),
                ("payload_type", route.payload_type.as_str()),
            ] {
                if value.is_empty() {
                    return Err(format!(
                        "Dispatch spec route field '{field}' must be non-empty"
                    ));
                }
            }
            if !route_keys.insert(route.route_key.as_str()) {
                return Err(format!(
                    "Dispatch spec contains duplicate route_key '{}'",
                    route.route_key
                ));
            }
            let alias = match route.dependency_alias.as_deref() {
                Some(alias) if alias.trim().is_empty() => {
                    return Err(format!(
                        "Remote route '{}' has an empty dependency_alias",
                        route.route_key
                    ));
                }
                value => value,
            };
            service_aliases
                .entry(route.proto_service.as_str())
                .or_default()
                .insert(alias);
        }
        for (service, aliases) in service_aliases {
            if aliases.len() != 1 {
                return Err(format!(
                    "Dispatch spec mixes local/remote routes or dependency aliases for proto service '{service}'"
                ));
            }
        }
        Ok(())
    }

    /// Return all local routes (routes without `dependency_alias`).
    pub fn local_routes(&self) -> Vec<&DispatchRoute> {
        self.routes
            .iter()
            .filter(|r| r.dependency_alias.is_none())
            .collect()
    }

    /// Return all remote routes (routes with `dependency_alias`).
    pub fn remote_routes(&self) -> Vec<&DispatchRoute> {
        self.routes
            .iter()
            .filter(|r| r.dependency_alias.is_some())
            .collect()
    }
}

/// Compute a SHA-256 hex digest of the given byte slice.
fn compute_sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Canonicalize a JSON-serializable value per JCS (RFC 8785):
/// sorted object keys recursively, no whitespace, UTF-8.
fn canonicalize_json<T: Serialize>(value: &T) -> Vec<u8> {
    let json_value = serde_json::to_value(value).unwrap_or_else(|e| {
        panic!("failed to convert to serde_json::Value: {e}");
    });
    let canonical = canonicalize_value(&json_value);
    serde_json::to_vec(&canonical).unwrap_or_else(|e| {
        panic!("failed to serialize canonical JSON: {e}");
    })
}

/// Recursively canonicalize a [`serde_json::Value`]:
/// - Objects: sort keys alphabetically (UTF-8 byte order), recurse on values.
/// - Arrays: recurse on each element in order.
/// - Primitives: returned as-is.
fn canonicalize_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map
                .iter()
                .map(|(key, val)| (key.clone(), canonicalize_value(val)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted_map = serde_json::Map::new();
            for (key, val) in entries {
                sorted_map.insert(key, val);
            }
            serde_json::Value::Object(sorted_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_value).collect())
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let json_str = r#"{"b":1,"a":{"d":2,"c":3}}"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let canonical = canonicalize_value(&value);
        let serialized = serde_json::to_string(&canonical).unwrap();
        assert_eq!(serialized, r#"{"a":{"c":3,"d":2},"b":1}"#);
    }

    #[test]
    fn local_and_remote_route_partition() {
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![
                DispatchRoute {
                    route_key: "local.Svc.Method".to_string(),
                    proto_file: "local/svc.proto".to_string(),
                    proto_service: "local.Svc".to_string(),
                    method: "Method".to_string(),
                    input_type: "Request".to_string(),
                    output_type: "Response".to_string(),
                    payload_type: "rpcReliable".to_string(),
                    dependency_alias: None,
                },
                DispatchRoute {
                    route_key: "remote.Svc.Method".to_string(),
                    proto_file: "remote/alias/remote.proto".to_string(),
                    proto_service: "remote.Svc".to_string(),
                    method: "Method".to_string(),
                    input_type: "Request".to_string(),
                    output_type: "Response".to_string(),
                    payload_type: "rpcReliable".to_string(),
                    dependency_alias: Some("alias".to_string()),
                },
            ],
            spec_hash: None,
        };

        assert_eq!(spec.local_routes().len(), 1);
        assert_eq!(spec.remote_routes().len(), 1);
    }

    #[test]
    fn route_validation_rejects_duplicates_and_empty_aliases() {
        let route = DispatchRoute {
            route_key: "remote.Svc.Method".to_string(),
            proto_file: "remote/alias/remote.proto".to_string(),
            proto_service: "remote.Svc".to_string(),
            method: "Method".to_string(),
            input_type: "Request".to_string(),
            output_type: "Response".to_string(),
            payload_type: "rpcReliable".to_string(),
            dependency_alias: Some("alias".to_string()),
        };
        let duplicate = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![route.clone(), route.clone()],
            spec_hash: None,
        };
        assert!(duplicate.validate_routes().is_err());

        let mut empty_method = route.clone();
        empty_method.method.clear();
        let empty_required_field = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![empty_method],
            spec_hash: None,
        };
        assert!(empty_required_field.validate_routes().is_err());

        let mut empty_alias_route = route;
        empty_alias_route.dependency_alias = Some(" ".to_string());
        let empty_alias = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![empty_alias_route],
            spec_hash: None,
        };
        assert!(empty_alias.validate_routes().is_err());
    }
}
