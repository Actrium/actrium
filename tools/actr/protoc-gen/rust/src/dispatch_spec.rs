//! Canonical DispatchSpec v1 — JSON descriptor consumed by the Rust protoc
//! plugin to generate the unified `actr_dispatch.rs` dispatcher.
//!
//! Mirrors the shape defined in `crates/actr-cli/src/commands/codegen/dispatch_spec.rs` so
//! the plugin can deserialize the spec written by the CLI without depending on
//! the CLI crate. The hash is recomputed locally using the same JCS + SHA-256
//! algorithm to detect stale specs.

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    #[serde(default = "default_reply_mode")]
    pub reply_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_alias: Option<String>,
}

fn default_reply_mode() -> String {
    "return".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchSpec {
    pub schema_version: String,
    pub routes: Vec<DispatchRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_hash: Option<String>,
}

impl DispatchSpec {
    /// Load a `DispatchSpec` from a JSON file at `path`.
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read dispatch spec at {}", path.display()))?;
        let spec: DispatchSpec = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse dispatch spec at {}", path.display()))?;
        if spec.schema_version != "v1" {
            bail!(
                "unsupported dispatch spec schema_version `{}`; expected `v1`",
                spec.schema_version
            );
        }
        spec.validate_routes()?;
        Ok(spec)
    }

    fn validate_routes(&self) -> Result<()> {
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
                ("reply_mode", route.reply_mode.as_str()),
            ] {
                if value.is_empty() {
                    bail!("dispatch spec route field `{field}` must be non-empty");
                }
            }
            if !matches!(route.reply_mode.as_str(), "return" | "explicit") {
                bail!(
                    "dispatch spec route `{}` has unsupported reply_mode `{}`",
                    route.route_key,
                    route.reply_mode
                );
            }
            if !route_keys.insert(route.route_key.as_str()) {
                bail!(
                    "dispatch spec contains duplicate route_key `{}`",
                    route.route_key
                );
            }
            let alias = match route.dependency_alias.as_deref() {
                Some(alias) if alias.trim().is_empty() => {
                    bail!(
                        "remote route `{}` has an empty dependency_alias",
                        route.route_key
                    );
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
                bail!(
                    "dispatch spec mixes local/remote routes or dependency aliases for proto service `{service}`"
                );
            }
        }
        Ok(())
    }

    /// Verify the embedded `spec_hash` matches a recomputed JCS + SHA-256 hash
    /// of the spec (without the `spec_hash` field), and that `expected_hash`
    /// (passed via `--actr_opt=dispatch_spec_hash=<hash>`) matches the same
    /// value. Returns an error on any mismatch.
    pub fn verify_hash(&self, expected_hash: &str) -> Result<()> {
        let recomputed = self.recompute_hash();
        let legacy_recomputed = self.recompute_legacy_hash();
        let matches_hash = |hash: &str| {
            hash == recomputed
                || legacy_recomputed
                    .as_deref()
                    .is_some_and(|legacy| hash == legacy)
        };

        if let Some(embedded) = &self.spec_hash
            && !matches_hash(embedded)
        {
            bail!(
                "dispatch spec hash mismatch: embedded `{embedded}` but recomputed \
                 `{recomputed}`; the spec on disk is stale or corrupted"
            );
        }

        if !expected_hash.is_empty() && !matches_hash(expected_hash) {
            bail!(
                "dispatch spec hash mismatch: CLI passed `{expected_hash}` but the spec \
                 on disk hashes to `{recomputed}`; rerun `actr gen` to refresh the spec"
            );
        }

        Ok(())
    }

    /// Recompute the canonical JCS + SHA-256 hash of this spec *without* the
    /// `spec_hash` field, matching `crates/actr-cli/src/commands/codegen/dispatch_spec.rs`.
    fn recompute_hash(&self) -> String {
        let spec_without_hash = DispatchSpec {
            schema_version: self.schema_version.clone(),
            routes: self.routes.clone(),
            spec_hash: None,
        };
        let canonical = canonicalize_json(&serde_json::to_value(&spec_without_hash).unwrap());
        let canonical_bytes = serde_json::to_vec(&canonical).unwrap();
        compute_sha256_hex(&canonical_bytes)
    }

    /// Pre-RFC-0323 DispatchSpec v1 did not serialize `reply_mode`.  Its
    /// semantic default is RETURN, so accept its original digest as well as
    /// the current spelling without weakening explicit-route validation.
    fn recompute_legacy_hash(&self) -> Option<String> {
        if self.routes.iter().any(|route| route.reply_mode != "return") {
            return None;
        }
        let mut value = serde_json::to_value(DispatchSpec {
            schema_version: self.schema_version.clone(),
            routes: self.routes.clone(),
            spec_hash: None,
        })
        .ok()?;
        let routes = value.get_mut("routes")?.as_array_mut()?;
        for route in routes {
            route.as_object_mut()?.remove("reply_mode");
        }
        let canonical = canonicalize_json(&value);
        Some(compute_sha256_hex(&serde_json::to_vec(&canonical).ok()?))
    }

    /// All routes whose `dependency_alias` is `None` — these are local routes
    /// handled by the `ActrDispatchHandler` trait.
    pub fn local_routes(&self) -> Vec<&DispatchRoute> {
        self.routes
            .iter()
            .filter(|route| route.dependency_alias.is_none())
            .collect()
    }

    /// All routes whose `dependency_alias` is `Some(_)` — these are remote
    /// routes handled by dependency-alias resolution + `call_raw`.
    pub fn remote_routes(&self) -> Vec<&DispatchRoute> {
        self.routes
            .iter()
            .filter(|route| route.dependency_alias.is_some())
            .collect()
    }
}

/// Canonicalize a `serde_json::Value` per JCS (RFC 8785):
/// sorted object keys recursively, no whitespace, UTF-8.
fn canonicalize_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map
                .iter()
                .map(|(key, val)| (key.clone(), canonicalize_json(val)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted_map = serde_json::Map::new();
            for (key, val) in entries {
                sorted_map.insert(key, val);
            }
            serde_json::Value::Object(sorted_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        _ => value.clone(),
    }
}

/// Compute a SHA-256 hex digest of the given byte slice.
fn compute_sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Normalize a proto file path the same way the CLI's `ProtoModel` does:
/// backslashes to forward slashes, strip `./` prefixes, and ensure a
/// `.proto` suffix.
#[allow(dead_code)]
pub fn normalize_proto_path(raw: &str) -> String {
    let normalized = raw.replace('\\', "/");
    let mut path = normalized.as_str();
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped;
    }
    if path.ends_with(".proto") {
        path.to_string()
    } else {
        format!("{path}.proto")
    }
}

/// Determine whether a route belongs to a local service by checking the
/// `dependency_alias` field: `None` means local, `Some(_)` means remote.
#[allow(dead_code)]
pub fn route_is_local(route: &DispatchRoute) -> bool {
    route.dependency_alias.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_route(
        route_key: &str,
        proto_file: &str,
        proto_service: &str,
        method: &str,
        input_type: &str,
        output_type: &str,
        dependency_alias: Option<&str>,
    ) -> DispatchRoute {
        DispatchRoute {
            route_key: route_key.to_string(),
            proto_file: proto_file.to_string(),
            proto_service: proto_service.to_string(),
            method: method.to_string(),
            input_type: input_type.to_string(),
            output_type: output_type.to_string(),
            payload_type: "rpcReliable".to_string(),
            reply_mode: "return".to_string(),
            dependency_alias: dependency_alias.map(|s| s.to_string()),
        }
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let json_str = r#"{"b":1,"a":{"d":2,"c":3}}"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let canonical = canonicalize_json(&value);
        let serialized = serde_json::to_string(&canonical).unwrap();
        assert_eq!(serialized, r#"{"a":{"c":3,"d":2},"b":1}"#);
    }

    #[test]
    fn recompute_hash_is_deterministic_and_excludes_spec_hash() {
        let routes = vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        )];
        let spec_a = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: routes.clone(),
            spec_hash: None,
        };
        let spec_b = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: routes.clone(),
            spec_hash: Some("deadbeef".to_string()),
        };

        // Hash must be the same regardless of the spec_hash field value,
        // because the canonical form excludes spec_hash.
        let hash_a = spec_a.recompute_hash();
        let hash_b = spec_b.recompute_hash();
        assert_eq!(hash_a, hash_b);

        // The canonical form of spec_b must not contain "spec_hash".
        let canonical_b = canonicalize_json(
            &serde_json::to_value(&DispatchSpec {
                schema_version: "v1".to_string(),
                routes: routes.clone(),
                spec_hash: None,
            })
            .unwrap(),
        );
        let canonical_str = serde_json::to_string(&canonical_b).unwrap();
        assert!(!canonical_str.contains("spec_hash"));
    }

    #[test]
    fn rejects_duplicate_route_keys_and_mixed_service_aliases() {
        let duplicate = make_route(
            "echo.EchoService.Echo",
            "echo.proto",
            "echo.EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        );
        let duplicate_spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![duplicate.clone(), duplicate],
            spec_hash: None,
        };
        assert!(duplicate_spec.validate_routes().is_err());

        let mut empty_method = duplicate_spec.routes[0].clone();
        empty_method.method.clear();
        let empty_required_field = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![empty_method],
            spec_hash: None,
        };
        assert!(empty_required_field.validate_routes().is_err());

        let mixed_spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![
                make_route(
                    "echo.EchoService.Echo",
                    "echo.proto",
                    "echo.EchoService",
                    "Echo",
                    "EchoRequest",
                    "EchoResponse",
                    None,
                ),
                make_route(
                    "echo.EchoService.Ping",
                    "echo.proto",
                    "echo.EchoService",
                    "Ping",
                    "EchoRequest",
                    "EchoResponse",
                    Some("echo"),
                ),
            ],
            spec_hash: None,
        };
        assert!(mixed_spec.validate_routes().is_err());
    }

    #[test]
    fn verify_hash_accepts_matching_hash() {
        let routes = vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        )];
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: None,
        };
        let recomputed = spec.recompute_hash();
        assert!(spec.verify_hash(&recomputed).is_ok());
    }

    #[test]
    fn verify_hash_accepts_v1_digest_without_reply_mode() {
        let routes = vec![make_route(
            "EchoService.Echo",
            "echo.proto",
            "EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        )];
        let mut spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: None,
        };
        let legacy_hash = spec.recompute_legacy_hash().unwrap();
        spec.spec_hash = Some(legacy_hash.clone());

        assert!(spec.verify_hash(&legacy_hash).is_ok());
    }

    #[test]
    fn verify_hash_rejects_mismatched_expected_hash() {
        let routes = vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        )];
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: None,
        };
        let err = spec.verify_hash("bogus").unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn verify_hash_rejects_stale_embedded_hash() {
        let routes = vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "EchoRequest",
            "EchoResponse",
            None,
        )];
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: Some("stale".to_string()),
        };
        let err = spec.verify_hash("").unwrap_err();
        assert!(err.to_string().contains("stale or corrupted"));
    }

    #[test]
    fn local_and_remote_routes_partition_correctly() {
        let routes = vec![
            make_route(
                "echo.EchoService.Echo",
                "local/echo.proto",
                "echo.EchoService",
                "Echo",
                "EchoRequest",
                "EchoResponse",
                None,
            ),
            make_route(
                "other.OtherService.Ping",
                "remote/other/other.proto",
                "other.OtherService",
                "Ping",
                "PingRequest",
                "PingResponse",
                Some("other"),
            ),
        ];
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: None,
        };
        assert_eq!(spec.local_routes().len(), 1);
        assert_eq!(spec.remote_routes().len(), 1);
    }

    #[test]
    fn normalize_proto_path_handles_prefixes_and_separators() {
        assert_eq!(normalize_proto_path(r".\remote\ask"), "remote/ask.proto");
        assert_eq!(
            normalize_proto_path("./remote/ask.proto"),
            "remote/ask.proto"
        );
        assert_eq!(normalize_proto_path("echo.proto"), "echo.proto");
        assert_eq!(normalize_proto_path("echo"), "echo.proto");
    }
}
