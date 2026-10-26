//! Edition 1 configuration parser

use crate::config::ObservabilityConfig;
use crate::config::{
    BinaryConfig, BuildArtifact, BuildConfig, BuildProfile, BuildTool, Dependency, IceServer,
    IceTransportPolicy, ManifestConfig, PackageInfo, ProtoFile, RuntimeConfig, ServiceRef,
    TrustAnchor, WebRtcAdvancedConfig, WebRtcConfig,
};

use crate::actr_raw::RuntimeRawConfig;
use crate::error::{ConfigError, Result};
use crate::{ManifestRawConfig, RawBuildConfig, RawDependency, RawPackageConfig, WebConfig};
use actr_protocol::uri::parse_realm_name;
use actr_protocol::{Acl, ActrType, Name};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

const DEFAULT_TRACING_ENDPOINT: &str = "http://localhost:4317";

/// Edition 1 format parser
pub struct ParserV1 {
    base_dir: PathBuf,
}

impl ParserV1 {
    pub fn new(config_path: impl AsRef<Path>) -> Self {
        let base_dir = config_path
            .as_ref()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self { base_dir }
    }

    pub fn parse_manifest(&self, mut raw: ManifestRawConfig) -> Result<ManifestConfig> {
        // 1. Process inheritance
        let raw = if let Some(parent_path) = raw.inherit.take() {
            self.merge_inheritance(raw, parent_path)?
        } else {
            raw
        };

        // 2. Parse package
        let package = self.parse_package(&raw.package)?;

        // 3. Parse exports (prefer [package].exports, fallback to top-level exports)
        let export_paths = if !raw.package.exports.is_empty() {
            &raw.package.exports
        } else {
            &raw.exports
        };
        let exports = self.parse_exports(export_paths)?;

        // 4. Parse dependencies. The Realm comes from actr.toml at runtime, so a
        //    dependency without an explicit Realm gets an empty placeholder.
        let dependencies = self.parse_dependencies(&raw.dependencies, "")?;

        // 5. Parse ACL
        let acl = if let Some(acl_value) = raw.acl {
            // Empty placeholder: the Realm is not known at manifest parse time
            Some(self.parse_acl(acl_value, "")?)
        } else {
            None
        };

        // 6. Determine config_dir
        let config_dir = if let Some(dir) = raw.config_dir {
            self.base_dir.join(dir)
        } else {
            self.base_dir.clone()
        };

        let binary = raw
            .binary
            .as_ref()
            .map(|raw_binary| self.parse_binary(raw_binary));
        let build = raw
            .build
            .as_ref()
            .map(|raw_build| self.parse_build(raw_build))
            .transpose()?;

        if raw.build.is_some() && raw.binary.is_none() {
            return Err(ConfigError::InvalidConfig(
                "[build] requires [binary] to declare the final packaged artifact path".to_string(),
            ));
        }

        // 7. Build manifest config — runtime fields are NOT included
        Ok(ManifestConfig {
            package,
            exports,
            dependencies,
            acl,
            tags: raw.package.tags,
            scripts: raw.scripts,
            binary,
            build,
            config_dir,
        })
    }

    /// Parse a `RuntimeRawConfig` (from actr.toml) into a `RuntimeConfig`.
    ///
    /// Since the runtime configuration has no `[package]` section, the caller must provide
    /// package info separately (typically read from the .actr package).
    pub fn parse_runtime(
        &self,
        raw: RuntimeRawConfig,
        package: PackageInfo,
    ) -> Result<RuntimeConfig> {
        // Validate required runtime fields
        let signaling_url_str = raw
            .signaling
            .url
            .as_ref()
            .ok_or(ConfigError::MissingField("signaling.url"))?;
        let signaling_url = Url::parse(signaling_url_str).map_err(ConfigError::InvalidUrl)?;

        let issuer_endpoint = raw
            .issuer_endpoint
            .url
            .as_ref()
            .ok_or(ConfigError::MissingField("issuer_endpoint.url"))
            .and_then(|url| Url::parse(url).map_err(ConfigError::InvalidUrl))?;

        let self_realm = raw
            .deployment
            .realm
            .clone()
            .ok_or(ConfigError::MissingField("deployment.realm"))?;
        check_realm_name(&self_realm, "deployment.realm")?;

        // Build observability from flat config
        let observability = ObservabilityConfig {
            filter_level: raw
                .observability
                .filter_level
                .unwrap_or_else(|| "info".to_string()),
            tracing_enabled: raw.observability.tracing_enabled.unwrap_or(false),
            tracing_endpoint: raw
                .observability
                .tracing_endpoint
                .unwrap_or_else(|| DEFAULT_TRACING_ENDPOINT.to_string()),
            tracing_service_name: raw
                .observability
                .tracing_service_name
                .unwrap_or_else(|| package.name.clone()),
            logs_enabled: raw.observability.logs_enabled.unwrap_or(false),
            ca_cert_path: raw
                .observability
                .ca_cert_path
                .map(|path| self.resolve_manifest_path(&path)),
        };

        // Parse ACL
        let acl = if let Some(acl_value) = raw.acl {
            Some(self.parse_acl(acl_value, &self_realm)?)
        } else {
            None
        };

        let package_path = raw
            .package
            .and_then(|pkg| pkg.path)
            .map(|path| self.resolve_manifest_path(&path));

        // Surface the removed `[hyper].storage_path_template` as a migration error
        // instead of silently ignoring it, and resolve `[hyper].storage_root`
        // against the config dir so `actr run` honours it (CWE-22 / CWE-73).
        if raw.deployment.removed_realm_secret.is_some() {
            return Err(ConfigError::InvalidConfig(
                "`deployment.realm_secret` was removed; join the Realm with `actr realm join` \
                 and set `deployment.realm_client_key` to the key file it writes."
                    .to_string(),
            ));
        }

        let storage_root = match raw.hyper {
            Some(hyper) => {
                if hyper.removed_storage_path_template.is_some() {
                    return Err(ConfigError::InvalidConfig(
                        "`storage_path_template` was removed; actor storage now uses a fixed \
                         hashed layout. Configure `[hyper].storage_root` instead."
                            .to_string(),
                    ));
                }
                hyper.storage_root.map(|p| {
                    let expanded =
                        crate::user_config::expand_tilde(p.to_string_lossy().into_owned());
                    self.resolve_manifest_path(&expanded)
                })
            }
            None => None,
        };

        let mut websocket_security = raw.websocket.security;
        for path in [
            &mut websocket_security.tls_cert,
            &mut websocket_security.tls_key,
            &mut websocket_security.ca_cert,
        ]
        .into_iter()
        .flatten()
        {
            *path = self.resolve_manifest_path(path);
        }

        Ok(RuntimeConfig {
            package,
            signaling_url,
            realm: self_realm,
            issuer_endpoint: issuer_endpoint.to_string(),
            realm_client_key: raw
                .deployment
                .realm_client_key
                .map(|path| self.resolve_manifest_path(&path)),
            visible_in_discovery: raw.discovery.visible.unwrap_or(true),
            acl,
            mailbox_path: None,
            scripts: raw.scripts,
            webrtc: self.parse_webrtc(&raw.webrtc)?,
            websocket_listen_port: raw.websocket.listen_port,
            websocket_security,
            websocket_advertised_host: raw.websocket.advertised_host,
            observability,
            config_dir: self.base_dir.clone(),
            trust: raw
                .trust
                .into_iter()
                .map(|anchor| resolve_trust_paths(anchor, &self.base_dir))
                .collect(),
            package_path,
            web: raw.web.map(|w| WebConfig {
                port: w.port,
                host: w.host,
                static_dir: self.base_dir.join(&w.static_dir),
                package_url: w.package_url,
                runtime_wasm_url: w.runtime_wasm_url,
            }),
            storage_root,
        })
    }

    fn parse_package(&self, raw: &RawPackageConfig) -> Result<PackageInfo> {
        ActrType::validate_publisher(&raw.publisher).map_err(|e| {
            ConfigError::InvalidActrType(format!(
                "Invalid publisher name '{}': {}",
                raw.publisher, e
            ))
        })?;

        Name::new(raw.name.clone()).map_err(|e| {
            ConfigError::InvalidActrType(format!("Invalid actor type name '{}': {}", raw.name, e))
        })?;

        let actr_type = ActrType {
            publisher: raw.publisher.clone(),
            name: raw.name.clone(),
            version: raw.version.clone(),
        };
        // `version` defaults to "" when omitted (an empty version contributes
        // nothing unsafe to the filename). When present it is interpolated into
        // the default `actr build` filename, so validate the whole identity via
        // the centralized protocol rule — this rejects traversal/separator content
        // in `version` (CWE-22 / CWE-73), which was previously unchecked.
        if !actr_type.version.is_empty() {
            actr_type
                .validate()
                .map_err(|e| ConfigError::InvalidActrType(e.to_string()))?;
        }

        Ok(PackageInfo {
            name: raw.name.clone(),
            actr_type,
            description: raw.description.clone(),
            authors: raw.authors.clone().unwrap_or_default(),
            license: raw.license.clone(),
        })
    }

    fn parse_binary(&self, raw: &crate::RawBinaryConfig) -> BinaryConfig {
        BinaryConfig {
            path: self.resolve_manifest_path(&raw.path),
            target: raw.target.clone(),
        }
    }

    fn parse_build(&self, raw: &RawBuildConfig) -> Result<BuildConfig> {
        let tool = match raw.tool.as_deref().unwrap_or("cargo") {
            "cargo" => BuildTool::Cargo,
            other => {
                return Err(ConfigError::InvalidConfig(format!(
                    "Unsupported [build].tool '{other}'; v1 only supports 'cargo'"
                )));
            }
        };

        let artifact = match raw.artifact.as_deref().unwrap_or("lib") {
            "lib" => BuildArtifact::Lib,
            "bin" => BuildArtifact::Bin,
            other => {
                return Err(ConfigError::InvalidConfig(format!(
                    "Unsupported [build].artifact '{other}'; expected 'lib' or 'bin'"
                )));
            }
        };

        let profile = match raw.profile.as_deref().unwrap_or("release") {
            "release" => BuildProfile::Release,
            "dev" => BuildProfile::Dev,
            other => {
                return Err(ConfigError::InvalidConfig(format!(
                    "Unsupported [build].profile '{other}'; expected 'release' or 'dev'"
                )));
            }
        };

        Ok(BuildConfig {
            tool,
            manifest_path: self.resolve_manifest_path(
                raw.manifest_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new("Cargo.toml")),
            ),
            artifact,
            target: raw.target.clone(),
            profile,
            features: raw.features.clone(),
            no_default_features: raw.no_default_features,
            post_build: raw.post_build.clone(),
        })
    }

    fn parse_exports(&self, paths: &[PathBuf]) -> Result<Vec<ProtoFile>> {
        paths
            .iter()
            .map(|path| {
                let full_path = self.base_dir.join(path);
                let content = std::fs::read_to_string(&full_path)
                    .map_err(|e| ConfigError::ProtoFileNotFound(full_path.clone(), e))?;
                Ok(ProtoFile {
                    path: full_path,
                    content,
                })
            })
            .collect()
    }

    fn resolve_manifest_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        }
    }

    fn parse_dependencies(
        &self,
        deps: &HashMap<String, RawDependency>,
        self_realm: &str,
    ) -> Result<Vec<Dependency>> {
        deps.iter()
            .map(|(alias, raw_dep)| match raw_dep {
                RawDependency::Empty {} => Ok(Dependency {
                    alias: alias.clone(),
                    realm: self_realm.to_string(),
                    actr_type: None,
                    service: None,
                }),
                RawDependency::Specified {
                    actr_type: actr_type_str,
                    service,
                    realm,
                } => {
                    let actr_type = self.parse_actr_type(actr_type_str)?;
                    let service_ref = service
                        .as_deref()
                        .map(|s| self.parse_service_ref(s))
                        .transpose()?;
                    let realm = match realm {
                        Some(realm) => {
                            check_realm_name(realm, &format!("dependencies.{alias}.realm"))?;
                            realm.clone()
                        }
                        None => self_realm.to_string(),
                    };
                    Ok(Dependency {
                        alias: alias.clone(),
                        realm,
                        actr_type: Some(actr_type),
                        service: service_ref,
                    })
                }
            })
            .collect()
    }

    /// Parse `"ServiceName:fingerprint"` into a `ServiceRef`.
    fn parse_service_ref(&self, s: &str) -> Result<ServiceRef> {
        let (name, fingerprint) = s.split_once(':').ok_or_else(|| {
            ConfigError::InvalidActrType(format!(
                "Invalid service reference '{}': expected 'ServiceName:fingerprint'",
                s
            ))
        })?;
        Ok(ServiceRef {
            name: name.to_string(),
            fingerprint: fingerprint.to_string(),
        })
    }

    /// Parse an ActrType string: `"publisher/program@version"`.
    ///
    /// Note: `version` is required.
    fn parse_actr_type(&self, s: &str) -> Result<ActrType> {
        let expected = || {
            ConfigError::InvalidActrType(format!(
                "Invalid actor type '{}': expected 'publisher/program@version'",
                s
            ))
        };
        let (publisher, name_version) = s.split_once('/').ok_or_else(expected)?;
        let (name, version) = name_version.rsplit_once('@').ok_or_else(expected)?;
        if name.contains('/') {
            return Err(expected());
        }

        ActrType::validate_publisher(publisher).map_err(|e| {
            ConfigError::InvalidActrType(format!(
                "Invalid publisher '{}' in '{}': {}",
                publisher, s, e
            ))
        })?;
        Name::new(name.to_string()).map_err(|e| {
            ConfigError::InvalidActrType(format!("Invalid type name '{}' in '{}': {}", name, s, e))
        })?;
        if version.is_empty() {
            return Err(ConfigError::InvalidActrType(format!(
                "Invalid actor type '{}': version must not be empty",
                s
            )));
        }

        Ok(ActrType {
            publisher: publisher.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })
    }

    fn parse_acl(&self, value: toml::Value, self_realm: &str) -> Result<Acl> {
        use actr_protocol::AclRule;
        use actr_protocol::acl_rule::{Permission, SourceRealm};

        let rules_array = value
            .get("rules")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ConfigError::InvalidAcl("ACL must have 'rules' array".to_string()))?;

        let mut rules = Vec::new();

        for (idx, rule_value) in rules_array.iter().enumerate() {
            let rule_table = rule_value.as_table().ok_or_else(|| {
                ConfigError::InvalidAcl(format!("ACL rule {} must be a table", idx))
            })?;

            // permission (required)
            let permission_str = rule_table
                .get("permission")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ConfigError::InvalidAcl(format!("ACL rule {} missing 'permission' field", idx))
                })?;
            let permission = match permission_str.to_uppercase().as_str() {
                "ALLOW" => Permission::Allow as i32,
                "DENY" => Permission::Deny as i32,
                _ => {
                    return Err(ConfigError::InvalidAcl(format!(
                        "ACL rule {}: invalid permission '{}', expected 'ALLOW' or 'DENY'",
                        idx, permission_str
                    )));
                }
            };

            // type (required): "publisher/program@version"
            let type_str = rule_table
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ConfigError::InvalidAcl(format!("ACL rule {} missing 'type' field", idx))
                })?;
            let from_type = self.parse_actr_type(type_str)?;

            // realm (optional): omitted/"self" → own Realm, "*" → any, Realm name → specific
            let source_realm = match rule_table.get("realm") {
                None => Some(SourceRealm::Realm(self_realm.to_string())),
                Some(v) => match v.as_str() {
                    Some("self") => Some(SourceRealm::Realm(self_realm.to_string())),
                    Some("*") => Some(SourceRealm::AnyRealm(true)),
                    Some(other) if parse_realm_name(other).is_ok() => {
                        Some(SourceRealm::Realm(other.to_string()))
                    }
                    _ => {
                        return Err(ConfigError::InvalidAcl(format!(
                            "ACL rule {idx}: realm must be 'self', '*', or a Realm name <realm_id>@<domain>"
                        )));
                    }
                },
            };

            rules.push(AclRule {
                permission,
                from_type,
                source_realm,
            });
        }

        Ok(Acl { rules })
    }

    fn parse_webrtc(&self, raw: &crate::raw::RawWebRtcConfig) -> Result<WebRtcConfig> {
        // Reject before the port-range mapping below, which silently drops
        // public_ips when no range is set: rtc 0.20 never applies the NAT
        // 1:1 mapping, so the option must fail startup loudly no matter how
        // it is combined.
        if !raw.public_ips.is_empty() {
            return Err(ConfigError::InvalidConfig(format!(
                "public_ips = {:?} is not supported with webrtc-rs 0.20: rtc \
                 stores nat_1to1_ips but never applies it, so SDP would keep \
                 advertising internal addresses while the mapping appears \
                 configured. Remove the option; front the node with TURN or \
                 run it on a public interface",
                raw.public_ips
            )));
        }

        let mut ice_servers = Vec::new();

        // Parse STUN URLs
        if !raw.stun_urls.is_empty() {
            ice_servers.push(IceServer {
                urls: raw.stun_urls.clone(),
                username: None,
                credential: None,
            });
        }

        // Parse TURN URLs (credentials are dynamically generated at runtime)
        if !raw.turn_urls.is_empty() {
            ice_servers.push(IceServer {
                urls: raw.turn_urls.clone(),
                username: None,
                credential: None,
            });
        }

        // Parse ICE transport policy
        let ice_transport_policy = if raw.force_relay {
            IceTransportPolicy::Relay
        } else {
            IceTransportPolicy::All
        };

        // Parse port configuration
        let (udp_ports, public_ips) = match (raw.port_range_start, raw.port_range_end) {
            (Some(start), Some(end)) => {
                // Port range configured, enable fixed port mode
                if start >= end {
                    // Invalid port range, return error
                    return Err(ConfigError::InvalidConfig(format!(
                        "Invalid port range: start ({}) must be less than end ({})",
                        start, end
                    )));
                } else {
                    // Port range mode
                    (Some((start, end)), raw.public_ips.clone())
                }
            }
            (None, None) => {
                // No port range configured, use default mode (random ports)
                (None, Vec::new())
            }
            _ => {
                return Err(ConfigError::InvalidConfig(
                    "port_range_start and port_range_end must be configured together".to_string(),
                ));
            }
        };

        // Parse ICE acceptance wait times
        let ice_host_acceptance_min_wait = raw.ice_host_acceptance_min_wait.unwrap_or(0);
        let ice_srflx_acceptance_min_wait = raw.ice_srflx_acceptance_min_wait.unwrap_or(20);
        let ice_prflx_acceptance_min_wait = raw.ice_prflx_acceptance_min_wait.unwrap_or(40);
        let ice_relay_acceptance_min_wait = raw.ice_relay_acceptance_min_wait.unwrap_or(100);

        // `ice_udp_mux_port` is rejected below and `udp_ports` is rejected by
        // the v0.20 node capability guard: upstream exposes neither the shared
        // mux nor an atomic port-range allocator/pre-bound-socket seam.
        let advanced = WebRtcAdvancedConfig {
            udp_ports,
            ice_udp_mux_port: raw.ice_udp_mux_port,
            public_ips,
            ice_host_acceptance_min_wait,
            ice_srflx_acceptance_min_wait,
            ice_prflx_acceptance_min_wait,
            ice_relay_acceptance_min_wait,
        };
        advanced.validate()?;

        Ok(WebRtcConfig {
            ice_servers,
            ice_transport_policy,
            advanced,
        })
    }

    fn merge_inheritance(
        &self,
        child: ManifestRawConfig,
        parent_path: PathBuf,
    ) -> Result<ManifestRawConfig> {
        let parent_full_path = self.base_dir.join(&parent_path);
        let mut parent = ManifestRawConfig::from_file(&parent_full_path)?;

        // Check edition consistency
        if parent.edition != child.edition {
            return Err(ConfigError::EditionMismatch {
                parent: parent.edition,
                child: child.edition,
            });
        }

        // Recursively process parent config inheritance
        let parent = if let Some(grandparent) = parent.inherit.take() {
            self.merge_inheritance(parent, grandparent)?
        } else {
            parent
        };

        // Merge logic — system config is no longer part of manifest.toml
        Ok(ManifestRawConfig {
            edition: child.edition, // Verified consistent
            inherit: None,
            config_dir: child.config_dir,
            package: child.package, // Package is not inherited
            exports: {
                let mut p = parent.exports;
                p.extend(child.exports);
                p
            },
            dependencies: {
                let mut d = parent.dependencies;
                d.extend(child.dependencies);
                d
            },
            acl: child.acl.or(parent.acl),
            scripts: {
                let mut s = parent.scripts;
                s.extend(child.scripts);
                s
            },
            binary: child.binary.or(parent.binary),
            build: child.build.or(parent.build),
        })
    }
}

// (ActrMode removed) previous execution mode parsing no longer needed.

/// Reject a value that is not a Realm name `<realm_id>@<domain>`.
fn check_realm_name(realm: &str, field: &str) -> Result<()> {
    parse_realm_name(realm)
        .map(|_| ())
        .map_err(|error| ConfigError::InvalidConfig(format!("{field}: {error}")))
}

/// Resolve a trust anchor's relative paths against the config dir.
fn resolve_trust_paths(anchor: TrustAnchor, base_dir: &Path) -> TrustAnchor {
    match anchor {
        TrustAnchor::Static {
            pubkey_file,
            pubkey_b64,
        } => TrustAnchor::Static {
            pubkey_file: pubkey_file.map(|p| if p.is_absolute() { p } else { base_dir.join(p) }),
            pubkey_b64,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ManifestRawConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn publisher_domain_rules_apply_to_manifest_and_dependency_types() {
        let parser = ParserV1::new("manifest.toml");
        for publisher in [
            "1acme.example",
            "a-very-long-publisher-domain-name.example.com",
        ] {
            let raw: ManifestRawConfig = toml::from_str(&format!(
                "edition = 1\n[package]\nname = \"echo\"\npublisher = \"{publisher}\"\nversion = \"1.0.0\"\n"
            )).unwrap();
            parser
                .parse_package(&raw.package)
                .unwrap()
                .actr_type
                .validate()
                .unwrap();
            parser
                .parse_actr_type(&format!("{publisher}/echo@1.0.0"))
                .unwrap()
                .validate()
                .unwrap();
        }
        for publisher in ["acme_example.com", "a..example"] {
            assert!(
                parser
                    .parse_actr_type(&format!("{publisher}/echo@1.0.0"))
                    .is_err()
            );
        }
    }

    #[test]
    fn test_parse_basic_config() {
        let toml_content = r#"
edition = 1
exports = ["proto/test.proto"]

[package]
name = "test-service"
publisher = "acme"

[dependencies]
user-service = {}

[scripts]
run = "cargo run"
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");

        // Create proto file
        let proto_dir = tmpdir.path().join("proto");
        fs::create_dir_all(&proto_dir).unwrap();
        fs::write(proto_dir.join("test.proto"), "syntax = \"proto3\";").unwrap();

        // Write config
        fs::write(&config_path, toml_content).unwrap();

        // Parse
        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let config = parser.parse_manifest(raw).unwrap();

        assert_eq!(config.package.name, "test-service");
        // ManifestConfig carries only manifest-level fields
        assert_eq!(config.dependencies.len(), 1);
        assert_eq!(config.exports.len(), 1);
    }

    #[test]
    fn test_parse_cross_realm_dependency() {
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "acme"
version = "1.0.0"

[dependencies]
shared = { actr_type = "acme/logging-service@1.0.0", service = "LoggingService:abc123", realm = "other@example.com" }
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let config = parser.parse_manifest(raw).unwrap();

        let dep = config.get_dependency("shared").unwrap();
        assert_eq!(dep.alias, "shared");
        assert_eq!(dep.realm, "other@example.com");
        assert_eq!(dep.actr_type.as_ref().unwrap().name, "logging-service");
        assert_eq!(dep.service.as_ref().unwrap().name, "LoggingService");
        assert_eq!(dep.service.as_ref().unwrap().fingerprint, "abc123");
        // ManifestConfig does not carry runtime fields (realm, signaling_url, issuer_endpoint)
        // cross_realm_dependencies() is a ManifestConfig method based on dependency realm IDs
        assert!(!config.dependencies.is_empty());
    }

    #[test]
    fn test_parse_package_fields() {
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "acme"
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let config = parser.parse_manifest(raw).unwrap();

        assert_eq!(config.package.name, "test");
        assert_eq!(config.package.actr_type.publisher, "acme");
        // ManifestConfig does not carry runtime fields (realm, signaling_url, issuer_endpoint)
    }

    #[test]
    fn test_validate_actr_type_name() {
        // Test invalid publisher name (starts with number)
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "1acme"
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let result = parser.parse_manifest(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::InvalidActrType(_)
        ));
    }

    #[test]
    fn test_validate_actr_type_name_invalid() {
        // Test invalid actor type name (ends with hyphen)
        let toml_content = r#"
edition = 1

[package]
name = "test-"
publisher = "acme"
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let result = parser.parse_manifest(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::InvalidActrType(_)
        ));
    }

    #[test]
    fn test_parse_binary_and_build_config() {
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "acme"
version = "1.0.0"

[binary]
path = "dist/test.wasm"
target = "wasm32-wasip2"

[build]
tool = "cargo"
manifest_path = "Cargo.toml"
artifact = "lib"
profile = "release"
target = "wasm32-wasip2"
features = ["feature-a", "feature-b"]
no_default_features = true
post_build = ["echo build"]
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let config = parser.parse_manifest(raw).unwrap();

        let binary = config.binary.expect("binary config should exist");
        assert_eq!(binary.path, tmpdir.path().join("dist/test.wasm"));
        assert_eq!(binary.target.as_deref(), Some("wasm32-wasip2"));

        let build = config.build.expect("build config should exist");
        assert_eq!(build.manifest_path, tmpdir.path().join("Cargo.toml"));
        assert_eq!(build.artifact, BuildArtifact::Lib);
        assert_eq!(build.profile, BuildProfile::Release);
        assert_eq!(build.features, vec!["feature-a", "feature-b"]);
        assert!(build.no_default_features);
        assert_eq!(build.post_build, vec!["echo build"]);
    }

    #[test]
    fn test_build_requires_binary_config() {
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "acme"

[build]
tool = "cargo"
"#;

        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        fs::write(&config_path, toml_content).unwrap();

        let raw = ManifestRawConfig::from_file(&config_path).unwrap();
        let parser = ParserV1::new(&config_path);
        let result = parser.parse_manifest(raw);

        assert!(matches!(result, Err(ConfigError::InvalidConfig(_))));
    }

    #[test]
    fn test_parse_execution_mode_not_in_manifest() {
        // manifest.toml does not carry execution mode — that belongs to RuntimeConfig (actr.toml).
        // Verify that parsing manifest.toml succeeds and produces a valid ManifestConfig.
        let toml_content = r#"
edition = 1

[package]
name = "test"
publisher = "acme"
"#;

        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("manifest.toml");
        fs::write(&path, toml_content).unwrap();

        let config = ParserV1::new(&path)
            .parse_manifest(ManifestRawConfig::from_file(&path).unwrap())
            .unwrap();
        // ManifestConfig fields are present; no execution_mode field exists on ManifestConfig
        assert_eq!(config.package.name, "test");
        assert_eq!(config.package.actr_type.publisher, "acme");
    }

    #[test]
    fn test_parse_webrtc_ice_udp_mux_port() {
        let tmpdir = TempDir::new().unwrap();
        let parser = ParserV1::new(tmpdir.path().join("actr.toml"));

        // Default: unset (per-connection ephemeral UDP), parses fine
        let raw = crate::raw::RawWebRtcConfig::default();
        let config = parser.parse_webrtc(&raw).unwrap();
        assert_eq!(config.advanced.ice_udp_mux_port, None);

        // Explicitly set: rejected — webrtc-rs 0.20 does not expose the
        // shared ICE UDP mux, so the option would silently bind an ephemeral
        // port instead of the configured one.
        let raw = crate::raw::RawWebRtcConfig {
            ice_udp_mux_port: Some(19302),
            ..Default::default()
        };
        let err = parser.parse_webrtc(&raw).unwrap_err();
        assert!(
            err.to_string().contains("ice_udp_mux_port"),
            "unexpected error: {err}"
        );

        // Also rejected when combined with a port range: the legacy "mux
        // wins, range ignored" precedence must not silently disable both
        // fixed-port mechanisms.
        let raw = crate::raw::RawWebRtcConfig {
            ice_udp_mux_port: Some(19302),
            port_range_start: Some(50000),
            port_range_end: Some(50100),
            ..Default::default()
        };
        let err = parser.parse_webrtc(&raw).unwrap_err();
        assert!(
            err.to_string().contains("ice_udp_mux_port"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_webrtc_udp_port_range_rejected() {
        let tmpdir = TempDir::new().unwrap();
        let parser = ParserV1::new(tmpdir.path().join("actr.toml"));
        let raw = crate::raw::RawWebRtcConfig {
            port_range_start: Some(50000),
            port_range_end: Some(50100),
            ..Default::default()
        };
        let err = parser.parse_webrtc(&raw).unwrap_err();
        assert!(
            err.to_string().contains("udp_ports"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_webrtc_partial_udp_port_range_rejected() {
        let tmpdir = TempDir::new().unwrap();
        let parser = ParserV1::new(tmpdir.path().join("actr.toml"));
        for raw in [
            crate::raw::RawWebRtcConfig {
                port_range_start: Some(50000),
                ..Default::default()
            },
            crate::raw::RawWebRtcConfig {
                port_range_end: Some(50100),
                ..Default::default()
            },
        ] {
            let err = parser.parse_webrtc(&raw).unwrap_err();
            assert!(
                err.to_string().contains("must be configured together"),
                "unexpected error: {err}"
            );
        }
    }

    #[test]
    fn test_parse_webrtc_public_ips_rejected() {
        let tmpdir = TempDir::new().unwrap();
        let parser = ParserV1::new(tmpdir.path().join("actr.toml"));

        // With a port range: rejected — rtc 0.20 stores nat_1to1_ips but
        // never applies it, so the mapping would silently not take effect.
        let raw = crate::raw::RawWebRtcConfig {
            port_range_start: Some(50000),
            port_range_end: Some(50100),
            public_ips: vec!["203.0.113.10".to_string()],
            ..Default::default()
        };
        let err = parser.parse_webrtc(&raw).unwrap_err();
        assert!(
            err.to_string().contains("public_ips"),
            "unexpected error: {err}"
        );

        // Without a port range: also rejected. The resolved config used to
        // drop public_ips silently in this combination, so the raw check must
        // fire before that mapping.
        let raw = crate::raw::RawWebRtcConfig {
            public_ips: vec!["203.0.113.10".to_string()],
            ..Default::default()
        };
        let err = parser.parse_webrtc(&raw).unwrap_err();
        assert!(
            err.to_string().contains("public_ips"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_ice_udp_mux_port_deserializes_from_toml() {
        let raw: crate::raw::RawWebRtcConfig = toml::from_str(
            r#"
ice_udp_mux_port = 3478
"#,
        )
        .unwrap();
        assert_eq!(raw.ice_udp_mux_port, Some(3478));

        // Absent field defaults to None (backward compatible)
        let raw: crate::raw::RawWebRtcConfig = toml::from_str("").unwrap();
        assert_eq!(raw.ice_udp_mux_port, None);
    }

    #[test]
    fn test_runtime_ca_cert_path_resolves_from_config_directory() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config").join("actr.toml");
        let raw: RuntimeRawConfig = toml::from_str(
            r#"
[signaling]
url = "ws://localhost:8081/signaling/ws"

[issuer_endpoint]
url = "http://localhost:8081/issuer"

[deployment]
realm = "k7m2q9xh@example.com"

[observability]
ca_cert_path = "certs/collector-ca.pem"
"#,
        )
        .unwrap();
        let package = PackageInfo {
            name: "test".to_string(),
            actr_type: ActrType {
                publisher: "acme".to_string(),
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            description: None,
            authors: Vec::new(),
            license: None,
        };

        let config = ParserV1::new(&config_path)
            .parse_runtime(raw, package)
            .unwrap();

        assert_eq!(
            config.observability.ca_cert_path,
            Some(tmpdir.path().join("config/certs/collector-ca.pem"))
        );
    }

    fn runtime_test_package() -> PackageInfo {
        PackageInfo {
            name: "test".to_string(),
            actr_type: ActrType {
                publisher: "acme".to_string(),
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            description: None,
            authors: Vec::new(),
            license: None,
        }
    }

    fn parse_runtime_str(
        config_path: &Path,
        toml_body: &str,
    ) -> crate::error::Result<RuntimeConfig> {
        let raw: RuntimeRawConfig = toml::from_str(toml_body).unwrap();
        ParserV1::new(config_path).parse_runtime(raw, runtime_test_package())
    }

    #[test]
    fn direct_websocket_defaults_and_flat_tls_paths() {
        let config_path = Path::new("/deployment/actr.toml");
        let defaults = parse_runtime_str(config_path, RUNTIME_BASE).unwrap();
        assert!(defaults.websocket_listen_port.is_none());
        assert!(defaults.websocket_security.bind_address.is_loopback());
        assert!(!defaults.websocket_security.allow_insecure);
        let body = format!(
            r#"{RUNTIME_BASE}
[websocket]
listen_port = 9443
bind_address = "0.0.0.0"
advertised_host = "peer.example.com"
tls_cert = "certs/server.pem"
tls_key = "/secrets/key.pem"
ca_cert = "certs/realm.pem"
"#
        );
        let parsed = parse_runtime_str(config_path, &body).unwrap();
        assert_eq!(parsed.websocket_listen_port, Some(9443));
        assert!(parsed.websocket_security.bind_address.is_unspecified());
        assert_eq!(
            parsed.websocket_security.tls_cert.unwrap(),
            Path::new("/deployment/certs/server.pem")
        );
        assert_eq!(
            parsed.websocket_security.tls_key.unwrap(),
            Path::new("/secrets/key.pem")
        );
        assert_eq!(
            parsed.websocket_security.ca_cert.unwrap(),
            Path::new("/deployment/certs/realm.pem")
        );
        assert!(toml::from_str::<RuntimeRawConfig>(&body.replace("0.0.0.0", "localhost")).is_err());
        assert!(
            toml::from_str::<RuntimeRawConfig>(&format!(
                "{RUNTIME_BASE}\n[websocket]\nallow_insecure = \"true\"\n"
            ))
            .is_err()
        );
    }

    const RUNTIME_BASE: &str = r#"
[signaling]
url = "ws://localhost:8081/signaling/ws"

[issuer_endpoint]
url = "http://localhost:8081/issuer"

[deployment]
realm = "k7m2q9xh@example.com"
"#;

    #[test]
    fn test_runtime_deployment_realm() {
        let path = Path::new("actr.toml");
        let config = parse_runtime_str(path, RUNTIME_BASE).unwrap();
        assert_eq!(config.realm, "k7m2q9xh@example.com");

        let missing = RUNTIME_BASE.replace("realm = \"k7m2q9xh@example.com\"\n", "");
        assert!(matches!(
            parse_runtime_str(path, &missing),
            Err(ConfigError::MissingField("deployment.realm"))
        ));
        for realm in ["k7m2q9xh", "K7m2q9xh@example.com", "1001"] {
            let invalid = RUNTIME_BASE.replace("k7m2q9xh@example.com", realm);
            assert!(parse_runtime_str(path, &invalid).is_err(), "{realm}");
        }
    }

    #[test]
    fn test_runtime_acl_source_realm() {
        use actr_protocol::acl_rule::SourceRealm;

        let rules = r#"
[[acl.rules]]
permission = "allow"
type = "acme/a@1.0.0"

[[acl.rules]]
permission = "allow"
type = "acme/b@1.0.0"
realm = "self"

[[acl.rules]]
permission = "allow"
type = "acme/c@1.0.0"
realm = "*"

[[acl.rules]]
permission = "deny"
type = "acme/d@1.0.0"
realm = "other@example.com"
"#;
        let config =
            parse_runtime_str(Path::new("actr.toml"), &format!("{RUNTIME_BASE}{rules}")).unwrap();
        let realms: Vec<_> = config
            .acl
            .unwrap()
            .rules
            .into_iter()
            .map(|rule| rule.source_realm)
            .collect();
        assert_eq!(
            realms,
            vec![
                Some(SourceRealm::Realm("k7m2q9xh@example.com".to_string())),
                Some(SourceRealm::Realm("k7m2q9xh@example.com".to_string())),
                Some(SourceRealm::AnyRealm(true)),
                Some(SourceRealm::Realm("other@example.com".to_string())),
            ]
        );

        for realm in ["realm = 9999", "realm = \"other\""] {
            let invalid = format!(
                "{RUNTIME_BASE}[[acl.rules]]\npermission = \"allow\"\ntype = \"acme/a@1.0.0\"\n{realm}\n"
            );
            assert!(
                matches!(
                    parse_runtime_str(Path::new("actr.toml"), &invalid),
                    Err(ConfigError::InvalidAcl(_))
                ),
                "{realm}"
            );
        }
    }

    #[test]
    fn test_runtime_hyper_storage_root_resolves_from_config_directory() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config").join("actr.toml");
        let config = parse_runtime_str(
            &config_path,
            &format!("{RUNTIME_BASE}\n[hyper]\nstorage_root = \"store\"\n"),
        )
        .unwrap();

        // A relative storage_root resolves against the config file's directory.
        assert_eq!(
            config.storage_root,
            Some(tmpdir.path().join("config/store"))
        );
    }

    #[test]
    fn test_runtime_hyper_storage_root_expands_tilde() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config").join("actr.toml");
        let config = parse_runtime_str(
            &config_path,
            &format!("{RUNTIME_BASE}\n[hyper]\nstorage_root = \"~/actr-test-store\"\n"),
        )
        .unwrap();

        // `~` expands to the home dir instead of a literal `~` component.
        let expected = crate::user_config::expand_tilde("~/actr-test-store".to_string());
        assert!(!expected.starts_with("~"));
        assert_eq!(config.storage_root, Some(expected));
    }

    #[test]
    fn test_runtime_hyper_storage_path_template_is_a_migration_error() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config").join("actr.toml");
        let err = parse_runtime_str(
            &config_path,
            &format!("{RUNTIME_BASE}\n[hyper]\nstorage_path_template = \"{{data_dir}}/custom\"\n"),
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("storage_path_template") && msg.contains("storage_root"),
            "error should point to the storage_root migration, got: {msg}"
        );
    }

    #[test]
    fn test_runtime_realm_client_key_is_relative_to_the_config() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config").join("actr.toml");
        let config = parse_runtime_str(
            &config_path,
            &format!("{RUNTIME_BASE}realm_client_key = \"keys/realm.json\"\n"),
        )
        .unwrap();
        assert_eq!(
            config.realm_client_key,
            Some(tmpdir.path().join("config").join("keys/realm.json"))
        );

        let err = parse_runtime_str(
            &config_path,
            &format!("{RUNTIME_BASE}realm_secret = \"rs_old\"\n"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("actr realm join"), "{err}");
    }

    #[test]
    fn test_parse_package_rejects_traversal_version() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("manifest.toml");
        // `version` is interpolated into the default `actr build` filename, so any
        // path-traversal / separator content must be rejected at manifest parsing.
        // (An omitted/empty version is allowed — it contributes nothing unsafe.)
        for bad_version in ["../../../outside", "1/0", "a\\b", "1 0"] {
            let toml_content = format!(
                "edition = 1\n\n[package]\nname = \"Echo\"\npublisher = \"acme\"\nversion = \"{bad_version}\"\n"
            );
            fs::write(&config_path, &toml_content).unwrap();
            let raw = ManifestRawConfig::from_file(&config_path).unwrap();
            let parser = ParserV1::new(&config_path);
            assert!(
                parser.parse_manifest(raw).is_err(),
                "version {bad_version:?} must be rejected"
            );
        }
    }
}
