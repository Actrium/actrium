//! Unified configuration management system
//!
//! This module is the "single source of truth" for Actrix auxiliary service configuration.
//! All configuration item definitions, documentation, and default values are managed here.

pub mod ais;
pub mod bind;
pub mod config_store;
pub mod control;
pub mod registry;
pub mod resolver;
pub mod services;
pub mod signaling;
pub mod signer;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
pub mod turn;

pub use crate::config::ais::AisConfig;
pub use crate::config::bind::BindConfig;
pub use crate::config::control::{AdminUiConfig, ControlConfig, ControlHead};
pub use crate::config::services::ServicesConfig;
pub use crate::config::signaling::SignalingConfig;
pub use crate::config::turn::TurnConfig;
use ::signer::storage::StorageBackend;
use std::path::{Path, PathBuf};
use url::Url;

/// Main configuration structure for Actrix auxiliary services
///
/// This is the core configuration of the system, containing all service configuration information.
/// Configuration files use TOML format with full type-safe loading.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActrixConfig {
    /// Service enable flags (bitmask) - Primary switch for business services
    ///
    /// This is the primary control mechanism for enabling business services.
    /// `control` is a resident control plane capability, not controlled by this bitmask.
    ///
    /// Bit positions (business services only):
    /// - Bit 0 (1): Signaling service
    /// - Bit 1 (2): STUN service
    /// - Bit 2 (4): TURN service
    /// - Bit 3 (8): AIS (Actor Identity Service)
    /// - Bit 4 (16): Signer
    ///
    /// Examples:
    /// - `enable = 31` enables all services (1+2+4+8+16=31)
    /// - `enable = 6` enables STUN + TURN (2+4=6)
    /// - `enable = 7` enables Signaling + STUN + TURN (1+2+4=7)
    #[serde(default = "default_enable")]
    pub enable: u8,

    /// Server instance name
    ///
    /// Used to identify different server instances, distinguishing nodes in cluster deployments.
    /// Recommended to use meaningful naming conventions, e.g.: actrix-01, actrix-prod-east-1.
    pub name: String,

    /// Environment identifier
    ///
    /// Specifies the current running environment, affecting security policies and default behavior:
    /// - "dev": Development environment, allows HTTP, relaxed certificate checks
    /// - "prod": Production environment, enforces HTTPS, strict security checks
    /// - "test": Test environment, used for automated testing
    pub env: String,

    /// Run user (optional)
    ///
    /// Specifies the system user the service runs as. The service will switch to this user after binding ports,
    /// to improve security. Leave empty to keep the current user.
    pub user: Option<String>,

    /// Run user group (optional)
    ///
    /// Specifies the system user group the service runs as. Used in conjunction with the user config.
    pub group: Option<String>,

    /// PID file path (optional)
    ///
    /// File path used to store the process ID. System management tools can use this file
    /// to monitor and manage the service process.
    pub pid: Option<String>,

    /// Network binding configuration
    ///
    /// Defines bind addresses and port configurations for various network services.
    pub bind: BindConfig,

    /// TURN service-specific configuration
    ///
    /// Dedicated configuration for the TURN relay service, including public address, port range, authentication realm, etc.
    pub turn: TurnConfig,

    /// Location tag
    ///
    /// Used to identify the geographic location or logical grouping of the server, for operations management and monitoring.
    /// e.g.: us-west-1, office-beijing, edge-node-01
    pub location_tag: String,

    /// Control plane configuration (resident)
    ///
    /// The control plane is always available and not controlled by the `enable` bitmask.
    #[serde(default)]
    pub control: ControlConfig,

    /// Service configuration collection
    ///
    /// Contains all business service configurations; each service can independently configure its own parameters and dependencies.
    /// Uses a service-level configuration structure to achieve high cohesion and low coupling.
    #[serde(default)]
    pub services: ServicesConfig,

    /// SQLite database file storage directory path
    ///
    /// Specifies the directory path for storing all SQLite database files.
    /// The main database file will be stored as `{sqlite_path}/actrix.db`.
    /// Includes Realm information, access control lists, nonce cache, etc.
    #[serde(
        serialize_with = "serialize_pathbuf",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub sqlite_path: PathBuf,

    /// Actrix internal service communication shared secret
    ///
    /// Used for internal communication authentication between Actrix services, such as between AIS and Signer.
    /// This is a system-level internal authentication key, used only for inter-service communication, not for external business.
    ///
    /// Note:
    /// - This key is for Actrix internal services only
    /// - Not for Realm business or external API access
    /// - A strong random key should be used in production environments
    /// - Field name retains actrix_shared_key for backward compatibility
    pub actrix_shared_key: String,

    /// Recording pipeline configuration (logging + tracing)
    ///
    /// Merges logging and OpenTelemetry tracing configuration into a unified recording section for centralized management.
    #[serde(default)]
    pub recording: RecordingConfig,

    /// Monitoring endpoints configuration
    ///
    /// Controls authentication for `/health` and `/metrics` endpoints.
    /// Supports IP whitelist (CIDR) and/or HTTP Basic Auth (htpasswd file).
    /// When both are configured, a request matching either is allowed (OR logic).
    /// Per-service overrides replace the global config for that service.
    #[serde(default)]
    pub monitoring: MonitoringConfig,
}

/// Recording pipeline configuration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RecordingConfig {
    /// Global recording sink URI (single sink)
    ///
    /// Serves as the default sink for all channels; can be overridden by [recording.<channel>].
    ///
    /// Supported:
    /// - file://...
    /// - otlp+http://...
    /// - otlp+grpc://...
    #[serde(default)]
    pub sink: Option<String>,

    /// OTLP reporting service.name
    #[serde(default = "default_recording_service_name")]
    pub service_name: String,

    /// observability channel configuration
    #[serde(default = "default_observability_channel")]
    pub observability: RecordingChannelConfig,
    /// audit channel configuration
    #[serde(default = "default_audit_channel")]
    pub audit: RecordingChannelConfig,
    /// security channel configuration
    #[serde(default = "default_security_channel")]
    pub security: RecordingChannelConfig,
    /// operations channel configuration
    #[serde(default = "default_operations_channel")]
    pub operations: RecordingChannelConfig,

    /// OTLP TLS CA certificate path (PEM). When set, traces and logs exporters
    /// use TLS with this CA to verify the collector.
    #[serde(default)]
    pub ca_cert_path: Option<String>,

    /// Enable OTLP logs export (same OTLP endpoint as traces; the collector
    /// routes log records to the logs backend).
    #[serde(default)]
    pub logs_enabled: Option<bool>,
}

/// Single channel configuration: semantic filter + sink override
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct RecordingChannelConfig {
    /// Channel semantic filter
    ///
    /// Each channel has its own filter vocabulary:
    /// - observability: off / digest / detailed / full
    /// - audit: off / mutations / all
    /// - security: off / critical / high / medium / all
    /// - operations: off / lifecycle / detailed
    #[serde(default)]
    pub filter: String,

    /// Recording sink URI (single sink)
    ///
    /// Supported:
    /// - file://...
    /// - otlp+http://...
    /// - otlp+grpc://...
    #[serde(default)]
    pub sink: Option<String>,
}

fn default_audit_channel() -> RecordingChannelConfig {
    RecordingChannelConfig {
        filter: "mutations".to_string(),
        sink: None,
    }
}

fn default_security_channel() -> RecordingChannelConfig {
    RecordingChannelConfig {
        filter: "all".to_string(),
        sink: None,
    }
}

fn default_operations_channel() -> RecordingChannelConfig {
    RecordingChannelConfig {
        filter: "lifecycle".to_string(),
        sink: None,
    }
}

fn default_observability_channel() -> RecordingChannelConfig {
    RecordingChannelConfig {
        filter: "digest".to_string(),
        sink: None,
    }
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            sink: None,
            service_name: default_recording_service_name(),
            observability: default_observability_channel(),
            audit: default_audit_channel(),
            security: default_security_channel(),
            operations: default_operations_channel(),
            ca_cert_path: None,
            logs_enabled: None,
        }
    }
}

fn normalized_optional_uri(uri: &Option<String>) -> Option<String> {
    uri.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn any_recording_file_sink(recording: &RecordingConfig) -> bool {
    [
        &recording.sink,
        &recording.observability.sink,
        &recording.audit.sink,
        &recording.security.sink,
        &recording.operations.sink,
    ]
    .into_iter()
    .filter_map(normalized_optional_uri)
    .any(|sink| sink.starts_with("file://"))
}

fn validate_recording_sink_field(field: &str, value: &Option<String>, errors: &mut Vec<String>) {
    let Some(uri) = normalized_optional_uri(value) else {
        return;
    };

    let parsed = match Url::parse(&uri) {
        Ok(parsed) => parsed,
        Err(error) => {
            errors.push(format!(
                "Invalid URI in {field}: {uri} (parse error: {error})"
            ));
            return;
        }
    };

    match parsed.scheme() {
        "file" => {
            if parsed.to_file_path().is_err() {
                errors.push(format!(
                    "Invalid file URI in {field}: {uri} (cannot convert to local file path)"
                ));
            }
        }
        "otlp+http" | "otlp+grpc" => {
            if parsed.host_str().is_none() {
                errors.push(format!(
                    "Invalid OTLP URI in {field}: {uri} (host is required)"
                ));
            }
        }
        _ => {
            errors.push(format!(
                "Invalid URI scheme in {field}: expected file://, otlp+http:// or otlp+grpc://, got {}",
                parsed.scheme()
            ));
        }
    }
}

/// Monitoring endpoint auth configuration.
///
/// Both `allowed_ips` and `htpasswd_file` can be used simultaneously.
/// When both are configured, a request matching either is allowed (OR logic).
/// When neither is configured, endpoints are publicly accessible.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitoringConfig {
    /// IP whitelist in CIDR notation (e.g. "10.0.0.0/8", "127.0.0.1").
    /// A bare IP without prefix length implies /32 (IPv4) or /128 (IPv6).
    #[serde(default)]
    pub allowed_ips: Vec<String>,

    /// Path to an htpasswd-format file for HTTP Basic Auth.
    /// Each line: `username:password` (plaintext).
    #[serde(default)]
    pub htpasswd_file: String,

    /// Per-service overrides. When a service key is present here,
    /// it completely replaces the global config for that service.
    #[serde(default)]
    pub services: std::collections::HashMap<String, MonitoringServiceOverride>,
}

/// Per-service monitoring auth override.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitoringServiceOverride {
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    #[serde(default)]
    pub htpasswd_file: String,
}

impl MonitoringConfig {
    /// Returns true if no auth is configured (globally).
    pub fn is_open(&self) -> bool {
        self.allowed_ips.is_empty() && self.htpasswd_file.is_empty()
    }

    /// Get the effective auth config for a given service.
    /// Per-service override replaces global when present.
    pub fn effective_for(&self, service: &str) -> (&[String], &str) {
        if let Some(ovr) = self.services.get(service) {
            (ovr.allowed_ips.as_slice(), ovr.htpasswd_file.as_str())
        } else {
            (self.allowed_ips.as_slice(), self.htpasswd_file.as_str())
        }
    }

    /// Returns true if no auth is configured for the given service.
    pub fn is_open_for(&self, service: &str) -> bool {
        let (ips, htpasswd) = self.effective_for(service);
        ips.is_empty() && htpasswd.is_empty()
    }
}

fn default_enable() -> u8 {
    31 // Signaling(1) + STUN(2) + TURN(4) + AIS(8) + Signer(16)
}

fn default_recording_service_name() -> String {
    "actrix".to_string()
}

fn serialize_pathbuf<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    path.display().to_string().serialize(serializer)
}

fn deserialize_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PathBuf::from(s))
}

impl Default for ActrixConfig {
    fn default() -> Self {
        Self {
            enable: default_enable(), // Default: enable STUN + TURN
            name: "actrix-default".to_string(),
            env: "dev".to_string(),
            user: None,
            group: None,
            pid: Some("logs/actrix.pid".to_string()),
            bind: BindConfig::default(),
            turn: TurnConfig::default(),
            location_tag: "default-location".to_string(),
            control: ControlConfig::default(),
            services: ServicesConfig::default(),
            sqlite_path: PathBuf::from("database"),
            actrix_shared_key: "XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg=".to_string(),
            recording: RecordingConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

// Service enable flag bit constants
pub const ENABLE_SIGNALING: u8 = 0b00001;
pub const ENABLE_STUN: u8 = 0b00010;
pub const ENABLE_TURN: u8 = 0b00100;
pub const ENABLE_AIS: u8 = 0b01000;
pub const ENABLE_SIGNER: u8 = 0b10000;

impl ActrixConfig {
    /// Check if the signaling service is enabled
    ///
    /// Service is enabled if the ENABLE_SIGNALING bit is set in the enable bitmask.
    pub fn is_signaling_enabled(&self) -> bool {
        self.enable & ENABLE_SIGNALING != 0
    }

    /// Check if the STUN service is enabled
    pub fn is_stun_enabled(&self) -> bool {
        self.enable & ENABLE_STUN != 0
    }

    /// Check if the TURN service is enabled
    pub fn is_turn_enabled(&self) -> bool {
        self.enable & ENABLE_TURN != 0
    }

    /// Check if the AIS (AId Issue Service) identity service is enabled
    ///
    /// Service is enabled if the ENABLE_AIS bit is set in the enable bitmask.
    pub fn is_ais_enabled(&self) -> bool {
        self.enable & ENABLE_AIS != 0
    }

    /// Check if the Signer key service is enabled
    ///
    /// Service is enabled if the ENABLE_SIGNER bit is set in the enable bitmask.
    pub fn is_signer_enabled(&self) -> bool {
        self.enable & ENABLE_SIGNER != 0
    }

    /// Check if the ICE service (STUN or TURN) is enabled
    pub fn is_ice_enabled(&self) -> bool {
        self.is_stun_enabled() || self.is_turn_enabled()
    }

    /// Current control plane head type
    pub fn control_head(&self) -> ControlHead {
        if self.control.grpc_api_enabled() && !self.control.admin_ui_enabled() {
            ControlHead::GrpcApi
        } else {
            ControlHead::AdminUi
        }
    }

    /// Whether Admin UI is enabled.
    pub fn admin_ui_enabled(&self) -> bool {
        self.control.admin_ui_enabled()
    }

    /// Whether NodeAdminService gRPC API is enabled.
    pub fn grpc_api_enabled(&self) -> bool {
        self.control.grpc_api_enabled()
    }

    /// Whether in superv takeover mode.
    pub fn superv_managed(&self) -> bool {
        self.control.superv_managed()
    }

    /// Get the PID file path, using the default if not configured
    pub fn get_pid_path(&self) -> Option<String> {
        self.pid.clone().or_else(|| {
            // If pid is not configured, use the default value logs/actrix.pid
            Some("logs/actrix.pid".to_string())
        })
    }

    /// Get the Actrix internal service communication shared secret
    ///
    /// This key is used for authenticated communication between Actrix internal services,
    /// such as service calls between AIS and Signer.
    ///
    /// Note: This key is for internal service communication only, not for external business
    pub fn get_actrix_shared_key(&self) -> &str {
        &self.actrix_shared_key
    }

    /// Returns a reference to the recording pipeline configuration
    pub fn recording_config(&self) -> &RecordingConfig {
        &self.recording
    }

    /// Returns the OTLP service.name
    pub fn recording_service_name(&self) -> &str {
        &self.recording.service_name
    }

    /// Get the composite log/trace filter level; prefers RUST_LOG.
    ///
    /// With semantic filters, channel targets are set to TRACE so that
    /// all events reach the recording layer (which applies its own gate).
    /// The base level stays `info` to suppress third-party noise.
    pub fn get_filter_level(&self) -> String {
        if let Ok(rust_log) = std::env::var("RUST_LOG") {
            let trimmed = rust_log.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }

        "info,actrix::observability=trace,actrix::audit=trace,actrix::security=trace,actrix::operations=trace".to_string()
    }

    /// Load configuration from file
    pub fn from_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let path_ref = path.as_ref();

        // Check if file exists
        if !path_ref.exists() {
            return Err(format!("Configuration file does not exist: {path_ref:?}").into());
        }

        // Check if path is a file, not a directory
        if !path_ref.is_file() {
            return Err(format!("Path is not a valid file: {path_ref:?}").into());
        }

        // Read file content
        let content = std::fs::read_to_string(path_ref)?;

        // Parse TOML content
        let config: ActrixConfig = toml::from_str(&content)?;

        Ok(config)
    }

    /// Load configuration from a TOML string
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize configuration to a TOML string
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    /// Validate configuration
    ///
    /// Checks the validity of all configuration items, including:
    /// - Whether required fields are present
    /// - Whether value ranges are reasonable
    /// - Whether file paths are valid
    /// - Whether service configurations are consistent
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate bitmask value range (0-31, 5 bits)
        if self.enable > 31 {
            errors.push(format!(
                "Invalid enable bitmask value: {}. Must be between 0 and 31 (5 bits)",
                self.enable
            ));
        }

        // Validate instance name
        if self.name.trim().is_empty() {
            errors.push("Instance name cannot be empty".to_string());
        }

        // Validate environment
        if !["dev", "prod", "test"].contains(&self.env.as_str()) {
            errors.push(format!(
                "Invalid environment '{}', must be one of: dev, prod, test",
                self.env
            ));
        }

        // Control is always mounted on the main HTTP port; binding must exist.
        if self.bind.http.is_none() {
            errors.push(
                "Control plane requires bind.http because /admin is always served".to_string(),
            );
        }

        // Validate channel semantic filters (empty string = use channel default, skip validation)
        {
            let check = |name: &str, value: &str, valid: &[&str]| -> Option<String> {
                if value.is_empty() || valid.contains(&value) {
                    None
                } else {
                    Some(format!(
                        "Invalid recording.{name}.filter '{value}', must be one of: {}",
                        valid.join(", ")
                    ))
                }
            };
            if let Some(e) = check(
                "observability",
                &self.recording.observability.filter,
                &["off", "digest", "detailed", "full"],
            ) {
                errors.push(e);
            }
            if let Some(e) = check(
                "audit",
                &self.recording.audit.filter,
                &["off", "mutations", "all"],
            ) {
                errors.push(e);
            }
            if let Some(e) = check(
                "security",
                &self.recording.security.filter,
                &["off", "critical", "high", "medium", "all"],
            ) {
                errors.push(e);
            }
            if let Some(e) = check(
                "operations",
                &self.recording.operations.filter,
                &["off", "lifecycle", "detailed"],
            ) {
                errors.push(e);
            }
        }

        if self.recording.service_name.trim().is_empty() {
            errors.push("recording.service_name cannot be empty".to_string());
        }

        // Validate new URI recording sink configuration (single sink, global + per-channel)
        validate_recording_sink_field("recording.sink", &self.recording.sink, &mut errors);
        validate_recording_sink_field(
            "recording.observability.sink",
            &self.recording.observability.sink,
            &mut errors,
        );
        validate_recording_sink_field(
            "recording.audit.sink",
            &self.recording.audit.sink,
            &mut errors,
        );
        validate_recording_sink_field(
            "recording.security.sink",
            &self.recording.security.sink,
            &mut errors,
        );
        validate_recording_sink_field(
            "recording.operations.sink",
            &self.recording.operations.sink,
            &mut errors,
        );

        // Validate actrix_shared_key
        if self.actrix_shared_key.contains("default") || self.actrix_shared_key.contains("change") {
            errors.push("Security warning: actrix_shared_key appears to be a default value. Please change it!".to_string());
        }
        if self.actrix_shared_key.len() < 16 {
            errors.push("Security warning: actrix_shared_key is too short, recommend at least 16 characters".to_string());
        }

        // Validate SQLite path
        if self
            .sqlite_path
            .to_str()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            errors.push("SQLite database path cannot be empty".to_string());
        }

        // Validate ICE configuration (if STUN or TURN is enabled)
        if self.is_ice_enabled() {
            if self.bind.ice.advertised_ip.trim().is_empty() {
                errors.push(
                    "bind.ice.advertised_ip is required when ICE services are enabled".to_string(),
                );
            }
            // Validate advertised_ip format
            if self
                .bind
                .ice
                .advertised_ip
                .parse::<std::net::IpAddr>()
                .is_err()
            {
                errors.push(format!(
                    "Invalid bind.ice.advertised_ip '{}', must be a valid IP address",
                    self.bind.ice.advertised_ip
                ));
            }
        }

        // Validate TURN configuration (if enabled)
        if self.is_turn_enabled() && self.turn.realm.trim().is_empty() {
            errors.push("TURN realm is required when TURN is enabled".to_string());
        }

        // Validate Signer configuration (if enabled)
        if self.is_signer_enabled() {
            if let Some(ref signer_cfg) = self.services.signer {
                // Validate storage configuration
                match signer_cfg.storage.backend {
                    StorageBackend::Sqlite => {}
                    StorageBackend::Postgres => {
                        if signer_cfg.storage.postgres.is_none() {
                            errors.push(
                                "Signer is configured to use PostgreSQL but postgres config is missing"
                                    .to_string(),
                            );
                        }
                    }
                }
            } else {
                // Signer bitmask is set but services.signer configuration is missing
                errors.push(
                    "Signer service is enabled (ENABLE_SIGNER bit is set) but services.signer configuration is missing".to_string(),
                );
            }
        }

        // Validate AIS configuration (if enabled)
        if self.is_ais_enabled() {
            // Check if Signer config can be obtained (explicit config or auto default)
            if let Some(ref ais) = self.services.ais {
                if ais.get_signer_client_config(self).is_none() {
                    errors.push(
                        "AIS service is enabled but no Signer available: \
                        either configure services.ais.dependencies.signer or enable local Signer service"
                            .to_string(),
                    );
                }
                // Validate renewal_token_secret configuration
                if ais.server.renewal_token_secret.is_empty() {
                    errors.push("services.ais.server.renewal_token_secret is required".to_string());
                } else {
                    match base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        &ais.server.renewal_token_secret,
                    ) {
                        Ok(decoded) if decoded.len() >= 32 => {}
                        Ok(decoded) => {
                            errors.push(format!(
                                "services.ais.server.renewal_token_secret must decode to \
                                 at least 32 bytes, got {} byte(s)",
                                decoded.len()
                            ));
                        }
                        Err(e) => {
                            errors.push(format!(
                                "services.ais.server.renewal_token_secret is not valid \
                                 base64: {e}"
                            ));
                        }
                    }
                }
            } else {
                // AIS bitmask is set but services.ais configuration is missing
                errors.push(
                    "AIS service is enabled (ENABLE_AIS bit is set) but services.ais configuration is missing".to_string(),
                );
            }
        }

        // Validate Signaling configuration (if enabled)
        if self.is_signaling_enabled() {
            if let Some(ref signaling) = self.services.signaling {
                if signaling.dependencies.signer.is_none()
                    && !(self.is_signer_enabled() && self.services.signer.is_some())
                {
                    errors.push(
                    "Signaling dependencies.signer is not configured; enable Signer (bitmask + services.signer) to use local defaults"
                        .to_string(),
                );
                }

                if signaling.dependencies.ais.is_none()
                    && !(self.is_ais_enabled() && self.services.ais.is_some())
                {
                    errors.push(
                    "Signaling dependencies.ais is not configured; enable AIS (bitmask + services.ais) to use local defaults"
                        .to_string(),
                );
                }
            } else {
                errors.push(
                    "Signaling service is enabled (ENABLE_SIGNALING bit is set) but services.signaling configuration is missing"
                        .to_string(),
                );
            }
        }

        // Production environment additional checks
        if self.env == "prod" {
            // Production environment should use HTTPS (bind.http must exist and is_tls() == true)
            if let Some(ref http) = self.bind.http {
                if !http.is_tls() {
                    errors.push(
                        "Production environment should enable HTTPS (configure cert and key in bind.http)".to_string(),
                    );
                }
            } else {
                errors.push("Production environment requires bind.http configuration".to_string());
            }

            // Production environment should configure at least one file:// sink
            if !any_recording_file_sink(&self.recording) {
                errors.push("Warning: Production environment should configure recording.sink (file://...) or channel-specific recording.<channel>.sink".to_string());
            }
        }

        if let Err(e) = self.control.validate() {
            errors.push(format!("Control configuration error: {e}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::signer::SignerServiceConfig;

    fn valid_ais_server_config() -> ais::AisServerConfig {
        ais::AisServerConfig {
            renewal_token_secret: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn example_config_loads_and_validates() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config.example.toml");
        let cfg = ActrixConfig::from_file(&path).expect("example config should parse");
        cfg.validate().expect("example config should validate");
        assert!(
            cfg.admin_ui_enabled(),
            "example should have admin_ui enabled"
        );
        assert!(
            !cfg.grpc_api_enabled(),
            "example should have grpc_api disabled by default"
        );
    }

    #[test]
    fn test_default_config() {
        let config = ActrixConfig::default();
        assert_eq!(config.enable, 31); // Default: all services enabled
        assert_eq!(config.name, "actrix-default");
        assert_eq!(config.env, "dev");
        assert!(config.is_signaling_enabled());
        assert!(config.is_stun_enabled());
        assert!(config.is_turn_enabled());
        assert!(config.is_ais_enabled());
        assert!(config.is_signer_enabled());
        assert_eq!(config.recording.service_name, "actrix");
        assert!(config.recording.sink.is_none());
        assert_eq!(config.recording.observability.filter, "digest");
        assert_eq!(config.recording.audit.filter, "mutations");
        assert_eq!(config.recording.security.filter, "all");
        assert_eq!(config.recording.operations.filter, "lifecycle");
    }

    #[test]
    fn test_recording_global_plus_spec_deserialize() {
        let mut config = ActrixConfig::default();
        config.recording.sink = Some("file:///tmp/actrix.log".to_string());
        config.recording.audit.sink = Some("otlp+http://127.0.0.1:4318/v1/logs".to_string());
        config.recording.security.sink = Some("otlp+grpc://127.0.0.1:4317".to_string());

        let toml = config.to_toml().expect("config should serialize");
        let parsed = ActrixConfig::from_toml(&toml).expect("config should deserialize");
        assert_eq!(
            parsed.recording.sink.as_deref(),
            Some("file:///tmp/actrix.log")
        );
        assert_eq!(
            parsed.recording.audit.sink.as_deref(),
            Some("otlp+http://127.0.0.1:4318/v1/logs")
        );
        assert_eq!(
            parsed.recording.security.sink.as_deref(),
            Some("otlp+grpc://127.0.0.1:4317")
        );
    }

    #[test]
    fn test_recording_sink_validation_rejects_invalid_scheme() {
        let mut config = ActrixConfig::default();
        config.recording.sink = Some("http://127.0.0.1:4317".to_string());

        let errors = config.validate().expect_err("validation should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid URI scheme in recording.sink")),
            "expected invalid scheme error, got: {errors:?}"
        );
    }

    #[test]
    fn test_recording_sink_validation_accepts_supported_schemes() {
        let mut config = ActrixConfig {
            enable: 0,
            ..Default::default()
        };
        config.control.admin_ui.password = "testpassword123".to_string();
        config.recording.sink = Some("file:///tmp/actrix.log".to_string());
        config.recording.audit.sink = Some("otlp+http://127.0.0.1:4318/v1/logs".to_string());
        config.recording.security.sink = Some("otlp+grpc://127.0.0.1:4317".to_string());

        assert!(
            config.validate().is_ok(),
            "supported sink schemes should pass validation"
        );
    }

    #[test]
    fn test_recording_sink_validation_rejects_otlp_uri_without_host() {
        let mut config = ActrixConfig::default();
        config.recording.sink = Some("otlp+grpc:///v1/traces".to_string());

        let errors = config.validate().expect_err("validation should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid OTLP URI in recording.sink")),
            "expected missing-host error, got: {errors:?}"
        );
    }

    #[test]
    fn test_recording_sink_validation_rejects_invalid_file_uri() {
        let mut config = ActrixConfig::default();
        config.recording.sink = Some("file://relative-path.log".to_string());

        let errors = config.validate().expect_err("validation should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid file URI in recording.sink")),
            "expected invalid file-uri error, got: {errors:?}"
        );
    }

    #[test]
    fn test_toml_serialization() {
        let config = ActrixConfig::default();
        let toml_str = config.to_toml().unwrap();
        assert!(toml_str.contains("enable = 31")); // all services
        assert!(toml_str.contains("name = \"actrix-default\""));
        assert!(
            toml_str
                .contains("actrix_shared_key = \"XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg=\"")
        );

        let parsed_config = ActrixConfig::from_toml(&toml_str).unwrap();
        assert_eq!(parsed_config.enable, config.enable);
        assert_eq!(parsed_config.name, config.name);
        assert_eq!(parsed_config.actrix_shared_key, config.actrix_shared_key);
    }

    #[test]
    fn test_actrix_shared_key() {
        let config = ActrixConfig::default();
        assert_eq!(
            config.get_actrix_shared_key(),
            "XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg="
        );

        // Test custom shared secret
        let mut custom_config = config;
        custom_config.actrix_shared_key = "custom-shared-key".to_string();
        assert_eq!(custom_config.get_actrix_shared_key(), "custom-shared-key");
    }

    #[test]
    fn test_service_flags() {
        let mut config = ActrixConfig {
            enable: 0,
            ..ActrixConfig::default()
        };

        // Test Signaling service: bitmask-only control
        // Case 1: Bitmask not set, service not enabled
        config.enable = 0;
        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies::default(),
        });
        assert!(!config.is_signaling_enabled());

        // Case 2: Bitmask set -> enabled (regardless of services.* config)
        config.enable = ENABLE_SIGNALING;
        assert!(config.is_signaling_enabled());

        // Case 3: Bitmask set, with services.* config -> still enabled
        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies::default(),
        });
        assert!(config.is_signaling_enabled());

        // Case 4: Bitmask set, no services.* config -> enabled
        config.services.signaling = None;
        assert!(config.is_signaling_enabled());

        // Test AIS service: bitmask-only control
        config.enable = 0;
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies::default(),
        });
        assert!(!config.is_ais_enabled());

        // Case 2: Bitmask set -> enabled (regardless of services.* config)
        config.enable = ENABLE_AIS;
        assert!(config.is_ais_enabled());

        // Case 3: Bitmask set, with services.* config -> still enabled
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies::default(),
        });
        assert!(config.is_ais_enabled());

        // Case 4: Bitmask set, no services.* config -> enabled
        config.services.ais = None;
        assert!(config.is_ais_enabled());

        // Test KS service: bitmask-only control
        config.enable = 0;
        config.services.signer = Some(SignerServiceConfig {
            ..Default::default()
        });
        assert!(!config.is_signer_enabled());

        // Case 2: Bitmask set -> enabled (regardless of services.* config)
        config.enable = ENABLE_SIGNER;
        assert!(config.is_signer_enabled());

        // Case 3: Bitmask set, with services.* config -> still enabled
        config.services.signer = Some(SignerServiceConfig {
            ..Default::default()
        });
        assert!(config.is_signer_enabled());

        // Case 4: Bitmask set, no services.* config -> enabled
        config.services.signer = None;
        assert!(config.is_signer_enabled());

        // Test ICE services (STUN/TURN use bitmask only)
        config.enable = ENABLE_STUN;
        assert!(config.is_stun_enabled());
        assert!(config.is_ice_enabled());

        config.enable = ENABLE_TURN;
        assert!(config.is_turn_enabled());
        assert!(config.is_ice_enabled());
    }

    #[test]
    fn test_ais_auto_ks_config() {
        let mut config = ActrixConfig {
            enable: ENABLE_SIGNER | ENABLE_AIS,
            ..ActrixConfig::default()
        };

        // Scenario 1: Local Signer enabled, AIS has no Signer client config, should auto-use local Signer
        config.enable = ENABLE_SIGNER | ENABLE_AIS; // Enable both KS and AIS via bitmask
        config.services.signer = Some(SignerServiceConfig {
            ..Default::default()
        });

        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies { signer: None }, // Signer not configured
        });

        // Should be able to get the auto-generated Signer config
        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_signer_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        // Signer gRPC reuses the main HTTP port (default 8080)
        assert_eq!(ks_config.endpoint, "http://127.0.0.1:8080");

        // Scenario 2: Explicit Signer client config, should use the explicit config
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies {
                signer: Some(crate::config::signer::SignerClientConfig {
                    endpoint: "http://remote-ks:8080".to_string(),
                    timeout_seconds: 10,
                    enable_tls: false,
                    tls_domain: None,
                    ca_cert: None,
                    client_cert: None,
                    client_key: None,
                }),
            },
        });

        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_signer_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        assert_eq!(ks_config.endpoint, "http://remote-ks:8080"); // Uses explicit config

        // Scenario 3: No local Signer, no explicit config, should return None
        config.services.signer = None;
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies { signer: None },
        });
        config.enable = ENABLE_AIS; // Enable AIS via bitmask

        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_signer_client_config(&config);
        assert!(ks_config.is_none());
    }

    #[test]
    fn test_signaling_auto_ks_config() {
        let mut config = ActrixConfig {
            enable: ENABLE_SIGNER | ENABLE_SIGNALING,
            ..ActrixConfig::default()
        };

        // Enable local Signer and Signaling
        config.enable = ENABLE_SIGNER | ENABLE_SIGNALING; // Enable both via bitmask
        config.services.signer = Some(SignerServiceConfig {
            ..Default::default()
        });

        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies {
                signer: None,
                ais: None,
            },
        });

        // Signaling should be able to get the auto-generated Signer config
        let ks_config = config
            .services
            .signaling
            .as_ref()
            .unwrap()
            .get_signer_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        // Signer gRPC reuses the main HTTP port (default 8080)
        assert_eq!(ks_config.endpoint, "http://127.0.0.1:8080");
    }

    #[test]
    fn test_validate_bitmask_consistency() {
        let mut config = ActrixConfig {
            enable: ENABLE_AIS,
            ..ActrixConfig::default()
        };

        // Case 1: AIS bitmask set but services.ais config missing - should warn
        config.enable = ENABLE_AIS;
        config.services.ais = None;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains(
            "AIS service is enabled (ENABLE_AIS bit is set) but services.ais configuration is missing"
        )));

        // Case 2: Signer bitmask set but services.signer config missing - should warn
        config.enable = ENABLE_SIGNER;
        config.services.ais = None;
        config.services.signer = None;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.contains("Signer service is enabled (ENABLE_SIGNER bit is set) but services.signer configuration is missing")
        }));

        // Case 3: Signaling bitmask set but services.signaling config missing - should error
        config.enable = ENABLE_SIGNALING;
        config.services.signaling = None;
        config.services.signer = None;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.contains("Signaling service is enabled (ENABLE_SIGNALING bit is set) but services.signaling configuration is missing")
        }));

        // Case 4: Signaling enabled with config present - should pass (if other validations pass)
        // Note: Unlike AIS, Signaling validation does not check KS availability
        config.enable = ENABLE_SIGNALING;
        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies {
                signer: None,
                ais: None,
            },
        });
        config.services.signer = None; // No local KS

        let result = config.validate();
        // Signaling with config should not have bitmask consistency errors
        // (may have other validation errors like sqlite_path, etc.)
        if let Err(errors) = result {
            assert!(
                !errors
                    .iter()
                    .any(|e| e.contains("Signaling service is enabled (ENABLE_SIGNALING bit is set) but services.signaling configuration is missing"))
            );
            assert!(
                !errors
                    .iter()
                    .any(|e| e.contains("bit is not set in enable bitmask"))
            );
        }

        // Case 5: Bitmask set and services.* config present - should pass (if other validations pass)
        config.enable = ENABLE_AIS | ENABLE_SIGNER;
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies {
                signer: Some(crate::config::signer::SignerClientConfig {
                    endpoint: "http://127.0.0.1:8080".to_string(),
                    timeout_seconds: 10,
                    enable_tls: false,
                    tls_domain: None,
                    ca_cert: None,
                    client_cert: None,
                    client_key: None,
                }),
            },
        });
        config.services.signer = Some(SignerServiceConfig {
            storage: ::signer::storage::StorageConfig {
                backend: ::signer::storage::StorageBackend::Sqlite,
                key_ttl_seconds: 3600,
                sqlite: Some(::signer::storage::SqliteConfig {}),
                postgres: None,
            },
            kek: None,
            kek_env: None,
            kek_file: None,
            tolerance_seconds: 3600,
        });

        // Should not have bitmask consistency errors (may have other validation errors)
        let result = config.validate();
        if let Err(errors) = result {
            assert!(
                !errors
                    .iter()
                    .any(|e| e.contains("bit is not set in enable bitmask"))
            );
        }
    }

    #[test]
    fn test_signaling_dependencies_require_local_services_when_missing_clients() {
        let mut config = ActrixConfig {
            enable: ENABLE_SIGNALING,
            ..ActrixConfig::default()
        };
        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies::default(),
        });
        config.services.signer = None;
        config.services.ais = None;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Signaling dependencies.signer is not configured"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Signaling dependencies.ais is not configured"))
        );
    }

    #[test]
    fn test_signaling_dependencies_accept_local_services_when_enabled() {
        let mut config = ActrixConfig {
            enable: ENABLE_SIGNALING | ENABLE_SIGNER | ENABLE_AIS,
            ..ActrixConfig::default()
        };
        config.control.admin_ui.password = "testpassword123".to_string();
        config.services.signaling = Some(SignalingConfig {
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies::default(),
        });
        config.services.signer = Some(SignerServiceConfig {
            ..Default::default()
        });
        config.services.ais = Some(AisConfig {
            server: valid_ais_server_config(),
            dependencies: ais::AisDependencies::default(),
        });

        let result = config.validate();
        assert!(result.is_ok());
    }
}
