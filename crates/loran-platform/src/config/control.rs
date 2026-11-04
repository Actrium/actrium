use serde::{Deserialize, Serialize};

/// Control head type.
///
/// - `admin_ui`: provides a local admin UI (HTTP).
/// - `grpc_api`: provides a gRPC API for the cluster supervisor (reuses the main HTTP port).
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ControlHead {
    #[default]
    AdminUi,
    GrpcApi,
}

/// gRPC control plane configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ControlGrpcApiConfig {
    /// Whether to enable the NodeAdminService gRPC API.
    #[serde(default)]
    pub enabled: bool,

    /// Node ID (used in authentication payloads)
    #[serde(default = "default_grpc_node_id")]
    pub node_id: String,

    /// Node display name (falls back to node_id when empty)
    #[serde(default = "default_grpc_node_name")]
    pub node_name: String,

    /// nonce-auth shared secret (hex, at least 64 characters)
    #[serde(default)]
    pub shared_secret: String,

    /// Maximum allowed clock skew (seconds)
    #[serde(default = "default_max_clock_skew_secs")]
    pub max_clock_skew_secs: u64,
}

/// Admin UI configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AdminUiConfig {
    /// Whether to enable the local Admin UI.
    #[serde(default = "default_admin_ui_enabled")]
    pub enabled: bool,

    /// Argon2id PHC password hash. Plaintext passwords are never accepted in
    /// production configuration.
    #[serde(default)]
    pub password_hash: String,

    /// JWT session expiry in seconds. Default: 3600 (1 hour).
    #[serde(default = "default_session_expiry_secs")]
    pub session_expiry_secs: u64,

    /// Login admission limits, applied before expensive password verification.
    #[serde(default)]
    pub login_limits: AdminLoginLimits,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct AdminLoginLimits {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub global_per_minute: u32,
    pub global_burst: u32,
    pub max_concurrent: usize,
}

impl Default for AdminLoginLimits {
    fn default() -> Self {
        Self {
            per_ip_per_minute: 30,
            per_ip_burst: 10,
            global_per_minute: 120,
            global_burst: 20,
            max_concurrent: 2,
        }
    }
}

impl AdminUiConfig {
    /// ADM-07: Short, stable fingerprint of the current password hash.
    ///
    /// Embedded into admin session tokens as the `pwd_ver` claim so that
    /// rotating `password_hash` (followed by a reload) invalidates every
    /// previously issued token, effectively logging out all sessions. Returns
    /// the first 16 hex characters of the SHA-256 digest of the PHC hash
    /// string. This never exposes the password or the raw hash — only a stable
    /// derived fingerprint of it.
    pub fn password_version(&self) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(self.password_hash.as_bytes());
        hex::encode(&digest[..8])
    }
}

/// Control persistent configuration.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ControlConfig {
    /// Legacy either-or head mode. New configurations should prefer admin_ui.enabled/grpc_api.enabled.
    #[serde(default)]
    pub head: Option<ControlHead>,

    /// gRPC head parameters (used only in grpc_api mode)
    #[serde(default)]
    pub grpc_api: ControlGrpcApiConfig,

    /// Admin UI parameters (used only in admin_ui mode)
    #[serde(default)]
    pub admin_ui: AdminUiConfig,
}

fn default_grpc_node_id() -> String {
    "loran-node".to_string()
}

fn default_grpc_node_name() -> String {
    "loran-node".to_string()
}

fn default_max_clock_skew_secs() -> u64 {
    300
}

fn default_session_expiry_secs() -> u64 {
    // ADM-07: Lowered from 86400 (24h) to reduce the window a leaked admin
    // token remains valid. Combined with the pwd_ver claim, password rotation
    // still invalidates tokens immediately regardless of this TTL.
    3600
}

fn default_admin_ui_enabled() -> bool {
    true
}

impl Default for AdminUiConfig {
    fn default() -> Self {
        Self {
            enabled: default_admin_ui_enabled(),
            password_hash: String::new(),
            session_expiry_secs: default_session_expiry_secs(),
            login_limits: AdminLoginLimits::default(),
        }
    }
}

impl Default for ControlGrpcApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_id: default_grpc_node_id(),
            node_name: default_grpc_node_name(),
            shared_secret: String::new(),
            max_clock_skew_secs: default_max_clock_skew_secs(),
        }
    }
}

impl ControlGrpcApiConfig {
    pub fn effective_node_name(&self) -> String {
        let trimmed = self.node_name.trim();
        if trimmed.is_empty() {
            self.node_id.trim().to_string()
        } else {
            trimmed.to_string()
        }
    }
}

impl ControlConfig {
    pub fn admin_ui_enabled(&self) -> bool {
        match self.head {
            Some(ControlHead::AdminUi) => true,
            Some(ControlHead::GrpcApi) => false,
            None => self.admin_ui.enabled,
        }
    }

    pub fn grpc_api_enabled(&self) -> bool {
        match self.head {
            Some(ControlHead::AdminUi) => false,
            Some(ControlHead::GrpcApi) => true,
            None => self.grpc_api.enabled,
        }
    }

    pub fn superv_managed(&self) -> bool {
        self.grpc_api_enabled()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.admin_ui_enabled() {
            self.validate_admin_ui()?;
        }

        if self.grpc_api_enabled() {
            self.validate_grpc_api()?;
        }

        Ok(())
    }

    fn validate_admin_ui(&self) -> Result<(), String> {
        let cfg = &self.admin_ui;

        let limits = &cfg.login_limits;
        if limits.per_ip_per_minute == 0
            || limits.per_ip_burst == 0
            || limits.global_per_minute == 0
            || limits.global_burst == 0
            || limits.max_concurrent == 0
        {
            return Err("control.admin_ui.login_limits values must be greater than 0".into());
        }

        if cfg.password_hash.is_empty() {
            return Err(
                "control.admin_ui.password_hash is required when admin_ui is enabled".to_string(),
            );
        }
        let parsed = argon2::PasswordHash::new(&cfg.password_hash).map_err(|error| {
            format!("control.admin_ui.password_hash is not a valid PHC string: {error}")
        })?;
        if parsed.algorithm.as_str() != "argon2id" {
            return Err(
                "control.admin_ui.password_hash must use the Argon2id algorithm".to_string(),
            );
        }

        if cfg.session_expiry_secs == 0 {
            return Err("control.admin_ui.session_expiry_secs must be greater than 0".to_string());
        }

        Ok(())
    }

    fn validate_grpc_api(&self) -> Result<(), String> {
        let cfg = &self.grpc_api;

        if cfg.node_id.trim().is_empty() {
            return Err("control.grpc_api.node_id cannot be empty".to_string());
        }

        if cfg.shared_secret.trim().is_empty() {
            return Err(
                "control.grpc_api.shared_secret is required when grpc_api is enabled".to_string(),
            );
        }

        if cfg.shared_secret.len() < 64 {
            return Err(
                "control.grpc_api.shared_secret must be at least 64 hex characters (32 bytes)"
                    .to_string(),
            );
        }

        if hex::decode(&cfg.shared_secret).is_err() {
            return Err("control.grpc_api.shared_secret must be a valid hex string".to_string());
        }

        if cfg.max_clock_skew_secs == 0 {
            return Err("control.grpc_api.max_clock_skew_secs must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_control_head_is_admin_ui() {
        let cfg = ControlConfig::default();
        assert!(cfg.admin_ui_enabled());
        assert!(!cfg.grpc_api_enabled());
    }

    #[test]
    fn admin_ui_requires_password() {
        let cfg = ControlConfig {
            head: Some(ControlHead::AdminUi),
            admin_ui: AdminUiConfig {
                password_hash: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn admin_ui_rejects_plaintext_password() {
        let cfg = ControlConfig {
            head: Some(ControlHead::AdminUi),
            admin_ui: AdminUiConfig {
                password_hash: "short".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn legacy_plaintext_password_field_is_not_silently_ignored() {
        let error = toml::from_str::<ControlConfig>(
            r#"
            [admin_ui]
            enabled = true
            password = "plaintext"
            "#,
        )
        .expect_err("unknown plaintext password field must fail deserialization");
        assert!(error.to_string().contains("unknown field"));
        assert!(error.to_string().contains("password"));
    }

    #[test]
    fn admin_ui_accepts_valid_password_hash() {
        let cfg = ControlConfig {
            head: Some(ControlHead::AdminUi),
            admin_ui: AdminUiConfig {
                password_hash: "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn admin_ui_rejects_non_argon2id_hash() {
        let cfg = ControlConfig {
            head: Some(ControlHead::AdminUi),
            admin_ui: AdminUiConfig {
                password_hash: "$argon2i$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn grpc_api_requires_shared_secret() {
        let cfg = ControlConfig {
            head: Some(ControlHead::GrpcApi),
            grpc_api: ControlGrpcApiConfig {
                shared_secret: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn grpc_api_accepts_valid_secret() {
        let cfg = ControlConfig {
            head: Some(ControlHead::GrpcApi),
            grpc_api: ControlGrpcApiConfig {
                shared_secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn enabled_flags_allow_admin_ui_and_grpc_together() {
        let cfg = ControlConfig {
            head: None,
            admin_ui: AdminUiConfig {
                enabled: true,
                password_hash: "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E".to_string(),
                ..Default::default()
            },
            grpc_api: ControlGrpcApiConfig {
                enabled: true,
                shared_secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                ..Default::default()
            },
        };

        assert!(cfg.admin_ui_enabled());
        assert!(cfg.grpc_api_enabled());
        assert!(cfg.superv_managed());
        assert!(cfg.validate().is_ok());
    }
}
