//! Signer configuration
//!
//! The Signer service is used to generate and manage cryptographic keys, providing key generation and public key query functionality for other services.

use serde::{Deserialize, Serialize};

/// Signer server configuration
///
/// Configures server-side parameters for the Signer service
/// Note: Only stores key_id and public_key, not the secret_key.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SignerServerConfig {}

/// Signer client configuration
///
/// Configuration used by other services when connecting to the Signer service as clients
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignerClientConfig {
    /// Signer service address
    ///
    /// gRPC endpoint, e.g.: "http://127.0.0.1:8080" or "https://ks.example.com:8443"
    pub endpoint: String,

    /// Request timeout (seconds)
    ///
    /// Timeout for connecting to the Signer service
    pub timeout_seconds: u64,

    /// Whether to enable TLS
    ///
    /// Defaults to false (using HTTP). When set to true, uses HTTPS/gRPC over TLS.
    #[serde(default)]
    pub enable_tls: bool,

    /// TLS domain name (required when TLS is enabled)
    ///
    /// Domain name used for TLS certificate validation
    pub tls_domain: Option<String>,

    /// CA certificate path (for verifying server certificate)
    ///
    /// Path to the CA certificate file used to verify the Signer server certificate
    pub ca_cert: Option<String>,

    /// Client certificate path (mTLS)
    ///
    /// Path to the client certificate file used for mutual TLS authentication
    pub client_cert: Option<String>,

    /// Client private key path (mTLS)
    ///
    /// Path to the client private key file used for mutual TLS authentication
    pub client_key: Option<String>,
}

/// Signer configuration (includes server and client configuration)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignerConfig {
    /// Signer server configuration (used when this instance acts as a Signer server)
    pub server: Option<SignerServerConfig>,

    /// Signer client configuration (used when connecting to other Signer services)
    pub client: Option<SignerClientConfig>,
}

impl Default for SignerClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".to_string(), // Default: reuses main HTTP port
            timeout_seconds: 30,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        }
    }
}

impl Default for SignerConfig {
    fn default() -> Self {
        Self {
            server: Some(SignerServerConfig::default()),
            client: Some(SignerClientConfig::default()),
        }
    }
}
