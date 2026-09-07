//! Signer configuration.
//!
//! The Signer service generates and manages cryptographic keys, offering key
//! generation and public-key lookup to other services.

use crate::crypto::KekSource;
use crate::storage::StorageConfig;
use serde::{Deserialize, Serialize};

/// Network exposure policy for the Signer gRPC surface.
///
/// The Signer server is currently merged into the shared primary HTTP listener.
/// This policy records the operator's intent for how far that surface should
/// reach (SGN-02): default to loopback-only and require an explicit opt-in
/// (plus mTLS in prod) before serving key operations to the public internet.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SignerExpose {
    /// Serve the Signer gRPC surface to loopback callers only (default).
    #[default]
    Loopback,
    /// Serve the Signer gRPC surface on the public listener. In `prod` this
    /// requires mTLS (see [`SignerServiceConfig::validate_exposure`]).
    Public,
}

/// Signer service configuration.
///
/// Service enable/disable is controlled by the bitmask in ActrixConfig.enable.
/// The ENABLE_SIGNER bit (bit 4) must be set to enable this service.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SignerServiceConfig {
    /// Storage configuration.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Tolerance period, in seconds.
    ///
    /// For a while after a key expires, the private key can still be fetched to
    /// allow a smooth transition. Default: 3600 (1 hour).
    #[serde(default = "default_tolerance")]
    pub tolerance_seconds: u64,

    /// KEK (Key Encryption Key) - inline value.
    ///
    /// Used to encrypt stored private keys. Two formats are accepted:
    /// - a 64-character hexadecimal string (32 bytes)
    /// - a 44-character Base64 string (32 bytes)
    ///
    /// Note: storing the KEK inline in the config file is not secure; in
    /// production prefer kek_env or kek_file.
    #[serde(default)]
    pub kek: Option<String>,

    /// KEK environment variable name.
    ///
    /// Reads the KEK from the named environment variable, which is safer than
    /// an inline value. For example: kek_env = "ACTRIX_KS_KEK".
    #[serde(default)]
    pub kek_env: Option<String>,

    /// KEK file path.
    ///
    /// Reads the KEK from a file that should contain a 64-character hexadecimal
    /// string or a 44-character Base64 string. File permissions should be set to
    /// 600 (owner read/write only).
    #[serde(default)]
    pub kek_file: Option<String>,

    /// Network exposure policy. Defaults to [`SignerExpose::Loopback`], so
    /// existing configs that omit this field keep loopback-only intent and
    /// behave exactly as before.
    #[serde(default)]
    pub expose: SignerExpose,

    /// Require mutual TLS (client-certificate authentication) for Signer
    /// requests. Defaults to `false` for backward compatibility. mTLS is only
    /// considered active once the certificate material below is also present
    /// (see [`SignerServiceConfig::mtls_enabled`]).
    #[serde(default)]
    pub require_mtls: bool,

    /// Path to the CA certificate used to verify client certificates (mTLS).
    #[serde(default)]
    pub mtls_ca_cert: Option<String>,

    /// Path to the server certificate presented for mTLS.
    #[serde(default)]
    pub mtls_cert: Option<String>,

    /// Path to the server private key used for mTLS.
    #[serde(default)]
    pub mtls_key: Option<String>,
}

fn default_tolerance() -> u64 {
    3600
}

impl Default for SignerServiceConfig {
    fn default() -> Self {
        Self {
            storage: Default::default(),
            tolerance_seconds: default_tolerance(),
            kek: None,
            kek_env: None,
            kek_file: None,
            expose: SignerExpose::default(),
            require_mtls: false,
            mtls_ca_cert: None,
            mtls_cert: None,
            mtls_key: None,
        }
    }
}

impl SignerServiceConfig {
    /// Whether mutual TLS is effectively enabled: the flag is set and all the
    /// certificate material required to establish it is present.
    pub fn mtls_enabled(&self) -> bool {
        self.require_mtls
            && self.mtls_ca_cert.is_some()
            && self.mtls_cert.is_some()
            && self.mtls_key.is_some()
    }

    /// Validate the network-exposure policy against the deployment environment.
    ///
    /// `env` is the top-level deployment environment (for example `"prod"`); it
    /// is not part of this local config, so the top-level
    /// `ActrixConfig::validate` owns the value and threads it through.
    ///
    /// Rule (SGN-02): exposing the Signer gRPC surface to the public internet in
    /// `prod` without mTLS is rejected (fail-closed).
    pub fn validate_exposure(&self, env: &str) -> Result<(), String> {
        if self.expose == SignerExpose::Public && env == "prod" && !self.mtls_enabled() {
            return Err(
                "services.signer.expose = \"public\" requires mTLS in prod: set \
                 require_mtls = true together with mtls_ca_cert, mtls_cert and mtls_key"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Return the KEK source.
    ///
    /// Priority: kek_file > kek_env > kek. Returns None when none is configured
    /// (encryption is disabled).
    pub fn get_kek_source(&self) -> Option<KekSource> {
        if let Some(path) = &self.kek_file {
            return Some(KekSource::File(path.clone()));
        }

        if let Some(env_var) = &self.kek_env {
            return Some(KekSource::Environment(env_var.clone()));
        }

        if let Some(kek) = &self.kek {
            return Some(KekSource::Direct(kek.clone()));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{SqliteConfig, StorageBackend};

    #[test]
    fn test_default_ks_service_config() {
        let config = SignerServiceConfig::default();
        assert_eq!(config.storage.backend, StorageBackend::Sqlite);
    }

    #[test]
    fn test_serialize_ks_service_config() {
        let config = SignerServiceConfig {
            storage: StorageConfig {
                backend: StorageBackend::Sqlite,
                key_ttl_seconds: 7200,
                sqlite: Some(SqliteConfig::default()),
                postgres: None,
            },
            kek: None,
            kek_env: None,
            kek_file: None,
            tolerance_seconds: 3600,
            ..Default::default()
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(!toml.contains("enabled")); // enabled field should not be present
        assert!(toml.contains("key_ttl_seconds = 7200"));
    }

    #[test]
    fn test_kek_source_priority() {
        // No KEK configured.
        let config = SignerServiceConfig::default();
        assert!(config.get_kek_source().is_none());

        // Only kek configured.
        let config = SignerServiceConfig {
            kek: Some("test_kek".to_string()),
            ..Default::default()
        };
        match config.get_kek_source() {
            Some(KekSource::Direct(k)) => assert_eq!(k, "test_kek"),
            _ => panic!("Expected Direct KEK source"),
        }

        // kek and kek_env configured: kek_env takes priority.
        let config = SignerServiceConfig {
            kek: Some("test_kek".to_string()),
            kek_env: Some("TEST_ENV".to_string()),
            ..Default::default()
        };
        match config.get_kek_source() {
            Some(KekSource::Environment(e)) => assert_eq!(e, "TEST_ENV"),
            _ => panic!("Expected Environment KEK source"),
        }

        // All three configured: kek_file takes priority.
        let config = SignerServiceConfig {
            kek: Some("test_kek".to_string()),
            kek_env: Some("TEST_ENV".to_string()),
            kek_file: Some("/path/to/kek".to_string()),
            ..Default::default()
        };
        match config.get_kek_source() {
            Some(KekSource::File(f)) => assert_eq!(f, "/path/to/kek"),
            _ => panic!("Expected File KEK source"),
        }
    }
}
