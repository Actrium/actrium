//! Lock file management for manifest.lock.toml
//!
//! This module provides lock file structures for dependency locking.
//! Proto content is cached to the project's `proto/` folder, not in the lock file.
//! The lock file only records metadata (fingerprints, paths) for version locking.

use crate::error::{ConfigError, Result};
use actr_protocol::{ActrType, ServiceSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

/// Lock file structure for manifest.lock.toml
///
/// Format matches documentation spec:
/// ```toml
/// [metadata]
/// version = 2
/// generated_at = "2024-01-15T10:30:00Z"
///
/// [[dependency]]
/// name = "user-service"
/// alias = "user-service"
/// actr_type = "acme:user-service:1.0.0"
/// description = "User management service"
/// fingerprint = "service_semantic:a1b2c3d4e5f6..."
/// published_at = 1705315800
/// tags = ["latest", "stable"]
/// cached_at = "2024-01-15T10:30:00Z"
///
///   [[dependency.files]]
///   path = "user-service/user.v1.proto"
///   fingerprint = "semantic:abc123..."
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LockFile {
    /// Lock file metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<LockMetadata>,

    /// Locked dependencies (ordered array for deterministic output)
    #[serde(rename = "dependency", default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<LockedDependency>,
}

/// Lock file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockMetadata {
    /// Lock file format version. Runtime alias resolution requires v2.
    pub version: u32,
    /// Generation timestamp (ISO 8601)
    pub generated_at: String,
}

/// Current lock file format version.
pub const LOCK_FILE_VERSION: u32 = 2;

/// A locked dependency entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDependency {
    /// Optional service-reference/display name carried by the CLI.
    ///
    /// This is not the dependency identity, package identity, or signaling
    /// registry key. Runtime resolution must use [`alias`](Self::alias).
    pub name: String,

    /// Dependency alias — the key under which this dependency is referenced in
    /// `manifest.toml` and the value used by `DispatchSpec` remote routes to
    /// resolve the dependency's `ActrType`. Required in v2 lock files.
    ///
    /// The default exists only so v1 files can be parsed and rejected with an
    /// actionable migration error; it is never used as a runtime fallback.
    #[serde(default)]
    pub alias: String,

    /// Actor type (e.g., "acme:user-service:1.0.0")
    pub actr_type: String,

    /// Service description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Service-level semantic fingerprint (format: "service_semantic:hash")
    pub fingerprint: String,

    /// Publication timestamp (Unix epoch seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,

    /// Tags like "latest", "stable"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// When this dependency was cached (ISO 8601)
    pub cached_at: String,

    /// Proto file references (path + fingerprint only, no content)
    #[serde(rename = "files")]
    pub files: Vec<LockedProtoFile>,
}

impl LockedDependency {
    /// Return the explicit dependency alias.
    ///
    /// Kept for source compatibility with callers of the v1-era API. It no
    /// longer falls back to `name`; unsupported v1 locks remain invalid.
    pub fn effective_alias(&self) -> &str {
        &self.alias
    }
}

/// Proto file reference in lock file (NO content, just path and fingerprint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedProtoFile {
    /// Relative path from project's proto/ folder (e.g., "user-service/user.v1.proto")
    pub path: String,

    /// Semantic fingerprint of the file (format: "semantic:hash")
    pub fingerprint: String,
}

/// Service specification metadata (for backward compatibility and conversions)
/// Holds proto file references for conversions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpecMeta {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Service-level semantic fingerprint
    pub fingerprint: String,

    /// Proto files referenced by path
    #[serde(rename = "files")]
    pub protobufs: Vec<ProtoFileMeta>,

    /// Publication timestamp (Unix epoch seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,

    /// Tags like "latest", "stable"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Package-level protobuf entry in lock file
/// Note: References a local file instead of embedding content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoFileMeta {
    /// Relative path to the proto file (e.g., "remote/user-service/user.v1.proto")
    pub path: String,

    /// Semantic fingerprint of the package content
    pub fingerprint: String,
}

// ============================================================================
// Bidirectional Conversion: ServiceSpec ↔ ServiceSpecMeta
// ============================================================================

impl From<ServiceSpec> for ServiceSpecMeta {
    fn from(spec: ServiceSpec) -> Self {
        Self {
            name: spec.name,
            description: spec.description,
            fingerprint: spec.fingerprint,
            protobufs: spec
                .protobufs
                .into_iter()
                .map(|proto| ProtoFileMeta {
                    path: format!("{}.proto", proto.package),
                    fingerprint: proto.fingerprint,
                })
                .collect(),
            published_at: spec.published_at,
            tags: spec.tags,
        }
    }
}

impl From<ServiceSpecMeta> for ServiceSpec {
    fn from(meta: ServiceSpecMeta) -> Self {
        Self {
            name: meta.name,
            description: meta.description,
            fingerprint: meta.fingerprint,
            protobufs: meta
                .protobufs
                .into_iter()
                .map(|proto| actr_protocol::service_spec::Protobuf {
                    package: package_from_path(&proto.path),
                    content: String::new(), // Content is no longer in lock file
                    fingerprint: proto.fingerprint,
                })
                .collect(),
            published_at: meta.published_at,
            tags: meta.tags,
        }
    }
}

// ============================================================================
// LockFile Operations
// ============================================================================

impl LockFile {
    /// Create a new empty lock file with current timestamp
    pub fn new() -> Self {
        Self {
            metadata: Some(LockMetadata {
                version: LOCK_FILE_VERSION,
                generated_at: chrono::Utc::now().to_rfc3339(),
            }),
            dependencies: Vec::new(),
        }
    }

    /// Load lock file from disk
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        content.parse()
    }

    /// Save lock file to disk
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let existing_permissions = std::fs::metadata(path)
            .ok()
            .map(|metadata| metadata.permissions());
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        if let Some(permissions) = existing_permissions {
            std::fs::set_permissions(temporary.path(), permissions)?;
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o644))?;
            }
        }
        temporary
            .persist(path)
            .map_err(|error| ConfigError::IoError(error.error))?;
        Ok(())
    }

    /// Add or update a dependency
    pub fn add_dependency(&mut self, dep: LockedDependency) {
        // Alias is the only update key. Keep duplicate actor types visible so
        // validation can reject them instead of silently deleting another
        // manifest alias.
        self.dependencies
            .retain(|existing| existing.alias != dep.alias);

        // Add new entry
        self.dependencies.push(dep);

        // Sort by alias for deterministic output.
        self.dependencies.sort_by(|a, b| a.alias.cmp(&b.alias));
    }

    /// Get a dependency by its service-reference/display name.
    ///
    /// This is a metadata lookup only. Runtime dependency identity uses
    /// [`Self::get_dependency_by_alias`] or [`Self::get_dependency_by_actr_type`].
    pub fn get_dependency(&self, name: &str) -> Option<&LockedDependency> {
        self.dependencies.iter().find(|d| d.name == name)
    }

    /// Get a dependency by its explicit v2 alias.
    pub fn get_dependency_by_alias(&self, alias: &str) -> Option<&LockedDependency> {
        self.dependencies.iter().find(|d| d.alias == alias)
    }

    /// Get a mutable dependency by its explicit v2 alias.
    pub fn get_dependency_by_alias_mut(&mut self, alias: &str) -> Option<&mut LockedDependency> {
        self.dependencies.iter_mut().find(|d| d.alias == alias)
    }

    /// Get a dependency by its canonical actor type.
    pub fn get_dependency_by_actr_type(&self, actr_type: &ActrType) -> Option<&LockedDependency> {
        self.dependencies.iter().find(|dependency| {
            ActrType::from_string_repr(&dependency.actr_type)
                .is_ok_and(|locked_type| locked_type == *actr_type)
        })
    }

    /// Remove a dependency by its explicit v2 alias.
    pub fn remove_dependency_by_alias(&mut self, alias: &str) -> bool {
        let before = self.dependencies.len();
        self.dependencies.retain(|d| d.alias != alias);
        self.dependencies.len() != before
    }

    /// Remove a dependency by its service-reference/display name.
    pub fn remove_dependency(&mut self, name: &str) -> bool {
        let before = self.dependencies.len();
        self.dependencies.retain(|d| d.name != name);
        self.dependencies.len() != before
    }

    /// Update generation timestamp
    pub fn update_timestamp(&mut self) {
        if let Some(ref mut metadata) = self.metadata {
            metadata.generated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    /// Return the declared format version, treating files without metadata as
    /// legacy v1 files.
    pub fn format_version(&self) -> u32 {
        self.metadata
            .as_ref()
            .map_or(1, |metadata| metadata.version)
    }

    /// Validate aliases and actor types, plus required proto file references.
    ///
    /// Only v2 is accepted. Both aliases and canonical actor types must be
    /// unique; `name` is never treated as an alias fallback.
    pub fn validate_aliases(&self) -> Result<()> {
        let version = self.format_version();
        if version != LOCK_FILE_VERSION {
            let migration = if version == 1 {
                "; run `actr deps install` to regenerate a v2 lock with explicit aliases"
            } else {
                ""
            };
            return Err(ConfigError::InvalidConfig(format!(
                "unsupported lock file version {version}; expected {LOCK_FILE_VERSION}{migration}"
            )));
        }

        let mut aliases = HashSet::new();
        let mut actr_types = HashSet::new();

        for dep in &self.dependencies {
            let alias = dep.alias.trim();
            if alias.is_empty() {
                return Err(ConfigError::InvalidConfig(format!(
                    "dependency `{}` is missing its alias",
                    dep.name
                )));
            }
            if alias != dep.alias.as_str() {
                return Err(ConfigError::InvalidConfig(format!(
                    "dependency alias `{}` contains leading or trailing whitespace",
                    dep.alias
                )));
            }
            if !aliases.insert(alias) {
                return Err(ConfigError::InvalidConfig(format!(
                    "dependency alias `{alias}` is declared more than once"
                )));
            }

            let actr_type = ActrType::from_string_repr(&dep.actr_type).map_err(|error| {
                ConfigError::InvalidConfig(format!(
                    "dependency `{}` has invalid actr_type `{}`: {error}",
                    dep.name, dep.actr_type
                ))
            })?;
            let canonical_type = actr_type.to_string_repr();
            if !actr_types.insert(canonical_type.clone()) {
                return Err(ConfigError::InvalidConfig(format!(
                    "dependency actr_type `{canonical_type}` is declared more than once"
                )));
            }

            if dep.files.is_empty() {
                return Err(ConfigError::InvalidConfig(format!(
                    "dependency `{}` declares no proto files",
                    dep.name
                )));
            }
        }
        Ok(())
    }
}

impl FromStr for LockFile {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(ConfigError::from)
    }
}

impl LockedDependency {
    /// Create a new locked dependency entry.
    ///
    /// `alias` is the dependency key from `manifest.toml` and must be non-empty
    /// in every v2 lock entry.
    pub fn new(alias: String, actr_type: String, spec: ServiceSpecMeta) -> Self {
        Self {
            name: spec.name,
            alias,
            actr_type,
            description: spec.description,
            fingerprint: spec.fingerprint,
            published_at: spec.published_at,
            tags: spec.tags,
            cached_at: chrono::Utc::now().to_rfc3339(),
            files: spec
                .protobufs
                .iter()
                .map(|p| LockedProtoFile {
                    path: p.path.clone(),
                    fingerprint: p.fingerprint.clone(),
                })
                .collect(),
        }
    }

    /// Get service-level fingerprint
    pub fn service_fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Get file fingerprints
    pub fn file_fingerprints(&self) -> &[LockedProtoFile] {
        &self.files
    }
}

fn package_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| path.trim_end_matches(".proto").to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_spec_conversion() {
        let spec = ServiceSpec {
            name: "test-service".to_string(),
            description: Some("Test service".to_string()),
            fingerprint: "service_semantic:abc123".to_string(),
            protobufs: vec![actr_protocol::service_spec::Protobuf {
                package: "user.v1".to_string(),
                content: "syntax = \"proto3\";".to_string(),
                fingerprint: "semantic:xyz".to_string(),
            }],
            published_at: Some(1705315800),
            tags: vec!["latest".to_string(), "stable".to_string()],
        };

        // Convert to meta
        let meta: ServiceSpecMeta = spec.clone().into();
        assert_eq!(meta.protobufs.len(), 1);
        assert_eq!(meta.protobufs[0].path, "user.v1.proto");
        assert_eq!(meta.protobufs[0].fingerprint, "semantic:xyz");
        assert_eq!(meta.published_at, Some(1705315800));
        assert_eq!(meta.tags.len(), 2);

        // Convert back to ServiceSpec
        let restored: ServiceSpec = meta.into();
        assert_eq!(restored.fingerprint, spec.fingerprint);
        assert_eq!(restored.protobufs.len(), 1);
        assert_eq!(restored.protobufs[0].package, spec.protobufs[0].package);
        // Note: content is lost during conversion back to ServiceSpec
        assert_eq!(restored.protobufs[0].content, "");
    }

    #[test]
    fn test_lock_file_operations() {
        let mut lock_file = LockFile::new();
        assert!(lock_file.dependencies.is_empty());

        let spec_meta = ServiceSpecMeta {
            name: "test-service".to_string(),
            description: None,
            fingerprint: "service_semantic:test".to_string(),
            protobufs: vec![],
            published_at: None,
            tags: vec![],
        };

        let dep = LockedDependency::new(
            "test-service".to_string(),
            "acme:test-service:1.0.0".to_string(),
            spec_meta,
        );

        lock_file.add_dependency(dep);
        assert_eq!(lock_file.dependencies.len(), 1);

        let found = lock_file.get_dependency_by_alias("test-service");
        assert!(found.is_some());
        assert_eq!(found.unwrap().actr_type, "acme:test-service:1.0.0");

        let removed = lock_file.remove_dependency_by_alias("test-service");
        assert!(removed);
        assert!(lock_file.dependencies.is_empty());
    }

    #[test]
    fn test_lock_file_serialization() {
        let mut lock_file = LockFile::new();

        let spec_meta = ServiceSpecMeta {
            name: "user-service".to_string(),
            description: Some("User service".to_string()),
            fingerprint: "service_semantic:abc123".to_string(),
            protobufs: vec![ProtoFileMeta {
                path: "user-service/user.v1.proto".to_string(),
                fingerprint: "semantic:xyz".to_string(),
            }],
            published_at: Some(1705315800),
            tags: vec!["latest".to_string()],
        };

        let dep = LockedDependency::new(
            "user-service".to_string(),
            "acme:user-service:1.0.0".to_string(),
            spec_meta,
        );

        lock_file.add_dependency(dep);

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&lock_file).unwrap();
        assert!(toml_str.contains("user-service"));
        assert!(toml_str.contains("user-service/user.v1.proto"));
        // Lock file should NOT contain proto content anymore
        assert!(!toml_str.contains("syntax = \"proto3\""));
        assert!(toml_str.contains("service_semantic:abc123"));

        // Deserialize back
        let restored: LockFile = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.dependencies.len(), 1);
        assert_eq!(restored.dependencies[0].name, "user-service");
        assert_eq!(
            restored.dependencies[0].files[0].path,
            "user-service/user.v1.proto"
        );
        assert_eq!(
            restored.dependencies[0].files[0].fingerprint,
            "semantic:xyz"
        );
    }

    #[test]
    fn test_path_serialization() {
        let spec_meta = ServiceSpecMeta {
            name: "user-service".to_string(),
            description: None,
            fingerprint: "service_semantic:test".to_string(),
            protobufs: vec![ProtoFileMeta {
                path: "user-service/user.v1.proto".to_string(),
                fingerprint: "semantic:abc".to_string(),
            }],
            published_at: None,
            tags: vec![],
        };

        // Serialize
        let toml_str = toml::to_string_pretty(&spec_meta).unwrap();

        // Should contain path
        assert!(toml_str.contains("path = \"user-service/user.v1.proto\""));

        // Deserialize back
        let restored: ServiceSpecMeta = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.protobufs[0].path, "user-service/user.v1.proto");
    }

    fn locked_dependency(name: &str, alias: &str, actr_type: &str) -> LockedDependency {
        LockedDependency {
            name: name.to_string(),
            alias: alias.to_string(),
            actr_type: actr_type.to_string(),
            description: None,
            fingerprint: format!("service_semantic:{name}"),
            published_at: None,
            tags: Vec::new(),
            cached_at: "2024-01-15T10:30:00Z".to_string(),
            files: vec![LockedProtoFile {
                path: format!("{name}/{name}.proto"),
                fingerprint: "semantic:test".to_string(),
            }],
        }
    }

    #[test]
    fn v1_is_rejected_instead_of_treating_name_as_an_alias() {
        let lock = LockFile {
            metadata: Some(LockMetadata {
                version: 1,
                generated_at: "2024-01-15T10:30:00Z".to_string(),
            }),
            dependencies: vec![locked_dependency(
                "echo-service",
                "",
                "acme:echo-service:1.0.0",
            )],
        };

        let error = lock.validate_aliases().unwrap_err().to_string();
        assert!(error.contains("unsupported lock file version 1"), "{error}");
        assert!(error.contains("actr deps install"), "{error}");
        assert!(lock.get_dependency_by_alias("echo-service").is_none());
        assert_eq!(lock.dependencies[0].effective_alias(), "");
    }

    #[test]
    fn v2_requires_an_explicit_non_empty_alias() {
        for alias in ["", "   "] {
            let lock = LockFile {
                metadata: Some(LockMetadata {
                    version: LOCK_FILE_VERSION,
                    generated_at: "2024-01-15T10:30:00Z".to_string(),
                }),
                dependencies: vec![locked_dependency(
                    "echo-service",
                    alias,
                    "acme:echo-service:1.0.0",
                )],
            };

            let error = lock.validate_aliases().unwrap_err().to_string();
            assert!(error.contains("missing its alias"), "{error}");
            assert!(lock.get_dependency_by_alias("echo-service").is_none());
        }
    }

    #[test]
    fn v2_rejects_alias_whitespace() {
        let mut lock = LockFile::new();
        lock.dependencies.push(locked_dependency(
            "echo-service",
            " echo ",
            "acme:echo-service:1.0.0",
        ));

        let error = lock.validate_aliases().unwrap_err().to_string();
        assert!(error.contains("leading or trailing whitespace"), "{error}");
    }

    #[test]
    fn validation_rejects_duplicate_aliases_and_actor_types() {
        let mut lock = LockFile::new();
        lock.dependencies = vec![
            locked_dependency("echo-service", "shared", "acme:echo-service:1.0.0"),
            locked_dependency("other-service", "shared", "acme:other-service:1.0.0"),
        ];
        let alias_error = lock.validate_aliases().unwrap_err().to_string();
        assert!(alias_error.contains("alias `shared` is declared more than once"));

        lock.dependencies[1].alias = "other".to_string();
        lock.dependencies[1].actr_type = "acme:echo-service:1.0.0".to_string();
        let type_error = lock.validate_aliases().unwrap_err().to_string();
        assert!(
            type_error.contains("actr_type `acme:echo-service:1.0.0` is declared more than once")
        );
    }

    #[test]
    fn validation_rejects_unparseable_actor_type() {
        let mut lock = LockFile::new();
        lock.dependencies.push(locked_dependency(
            "echo-service",
            "echo",
            "acme+echo-service",
        ));

        let error = lock.validate_aliases().unwrap_err().to_string();
        assert!(error.contains("invalid actr_type `acme+echo-service`"));
    }

    #[test]
    fn alias_is_the_only_source_level_dependency_identity() {
        let mut lock = LockFile::new();
        lock.add_dependency(locked_dependency(
            "EchoService",
            "echo",
            "acme:EchoActor:1.0.0",
        ));

        assert!(lock.get_dependency_by_alias("echo").is_some());
        assert!(lock.get_dependency_by_alias("EchoService").is_none());
        assert!(lock.get_dependency_by_alias("EchoActor").is_none());
        assert!(lock.get_dependency_by_alias("acme:EchoActor").is_none());
        assert!(
            lock.get_dependency_by_actr_type(
                &ActrType::from_string_repr("acme:EchoActor:1.0.0").unwrap()
            )
            .is_some()
        );
        assert!(lock.validate_aliases().is_ok());
    }

    #[test]
    fn add_dependency_replaces_by_alias_but_preserves_type_conflicts() {
        let mut lock = LockFile::new();
        lock.add_dependency(locked_dependency(
            "EchoService",
            "echo",
            "acme:EchoActor:1.0.0",
        ));
        lock.add_dependency(locked_dependency(
            "DifferentServiceRef",
            "echo",
            "acme:NewEchoActor:2.0.0",
        ));
        assert_eq!(lock.dependencies.len(), 1);
        assert_eq!(lock.dependencies[0].name, "DifferentServiceRef");

        lock.add_dependency(locked_dependency(
            "AnotherServiceRef",
            "renamed-alias",
            "acme:NewEchoActor:2.0.0",
        ));
        assert_eq!(lock.dependencies.len(), 2);
        let error = lock.validate_aliases().unwrap_err().to_string();
        assert!(error.contains("actr_type `acme:NewEchoActor:2.0.0`"));
    }
}
