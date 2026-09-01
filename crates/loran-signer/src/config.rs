//! Signer configuration
//!
//! The Signer service generates and manages cryptographic keys, providing key generation and public key query capabilities for other services

use crate::crypto::KekSource;
use crate::storage::StorageConfig;
use serde::{Deserialize, Serialize};

/// Signer service configuration
///
/// Service enable/disable is controlled by the bitmask in ActrixConfig.enable.
/// The ENABLE_SIGNER bit (bit 4) must be set to enable this service.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignerServiceConfig {
    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,

    /// Tolerance period (seconds)
    ///
    /// A period after key expiration during which private key retrieval is still allowed, for smooth transitions
    /// Default: 3600 (1 hour)
    #[serde(default = "default_tolerance")]
    pub tolerance_seconds: u64,

    /// KEK (Key Encryption Key) - direct configuration
    ///
    /// Used to encrypt stored private keys. Supports two formats:
    /// - 64-character hex string (32 bytes)
    /// - 44-character Base64 string (32 bytes)
    ///
    /// Note: storing the KEK directly in the config file is not secure; production environments should use kek_env or kek_file
    #[serde(default)]
    pub kek: Option<String>,

    /// KEK environment variable name
    ///
    /// Read the KEK from the specified environment variable, more secure than direct configuration
    /// Example: kek_env = "ACTRIX_KS_KEK"
    #[serde(default)]
    pub kek_env: Option<String>,

    /// KEK file path
    ///
    /// Read the KEK from a file; the file should contain a 64-character hex string or a 44-character Base64 string
    /// File permissions should be set to 600 (owner read/write only)
    #[serde(default)]
    pub kek_file: Option<String>,
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
        }
    }
}

impl SignerServiceConfig {
    /// Get the KEK source
    ///
    /// Priority: kek_file > kek_env > kek
    /// Returns None if none configured (uses no-encryption mode)
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
                sqlite: Some(SqliteConfig {}),
                postgres: None,
            },
            kek: None,
            kek_env: None,
            kek_file: None,
            tolerance_seconds: 3600,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(!toml.contains("enabled")); // enabled field should not be present
        assert!(toml.contains("key_ttl_seconds = 7200"));
    }

    #[test]
    fn test_kek_source_priority() {
        // No KEK configured
        let config = SignerServiceConfig::default();
        assert!(config.get_kek_source().is_none());

        // Only kek configured
        let config = SignerServiceConfig {
            kek: Some("test_kek".to_string()),
            ..Default::default()
        };
        match config.get_kek_source() {
            Some(KekSource::Direct(k)) => assert_eq!(k, "test_kek"),
            _ => panic!("Expected Direct KEK source"),
        }

        // kek and kek_env configured; kek_env takes priority
        let config = SignerServiceConfig {
            kek: Some("test_kek".to_string()),
            kek_env: Some("TEST_ENV".to_string()),
            ..Default::default()
        };
        match config.get_kek_source() {
            Some(KekSource::Environment(e)) => assert_eq!(e, "TEST_ENV"),
            _ => panic!("Expected Environment KEK source"),
        }

        // All three configured; kek_file takes priority
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
