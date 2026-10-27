//! Signer configuration.
//!
//! The Signer service generates and manages cryptographic keys, offering key
//! generation and public-key lookup to other services.

use serde::{Deserialize, Serialize};

// Note (SGN-02): the exposure / mTLS policy (`SignerExpose`, `expose`,
// `require_mtls`, `mtls_*`, `validate_exposure`) lives on the live runtime type
// `signer::config::SignerServiceConfig` (see the `signer` crate), which is what
// `services.signer` actually deserializes into. The previous server-side struct
// here was never wired into the runtime, so keeping a second copy of those
// fields would only create drift; it has been removed.

/// Signer client configuration.
///
/// Configuration used by other services when connecting to the Signer service
/// as a client.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SignerClientConfig {
    /// Signer service address.
    ///
    /// gRPC endpoint, e.g. "http://127.0.0.1:8080" or "https://signer.example.com:8443".
    pub endpoint: String,

    /// Request timeout, in seconds.
    ///
    /// Timeout for connecting to the Signer service.
    pub timeout_seconds: u64,

    /// Whether to enable TLS.
    ///
    /// Defaults to false (plain HTTP). When true, uses HTTPS / gRPC over TLS.
    #[serde(default)]
    pub enable_tls: bool,

    /// TLS domain name (required when TLS is enabled).
    ///
    /// Domain name used for TLS certificate verification.
    pub tls_domain: Option<String>,

    /// CA certificate path (used to verify the server certificate).
    ///
    /// Path to the CA certificate file used to verify the Signer server certificate.
    pub ca_cert: Option<String>,

    /// Client certificate path (mTLS).
    ///
    /// Path to the client certificate file used for mutual TLS authentication.
    pub client_cert: Option<String>,

    /// Client private key path (mTLS).
    ///
    /// Path to the client private key file used for mutual TLS authentication.
    pub client_key: Option<String>,
}

impl Default for SignerClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".to_string(), // default: reuse the primary HTTP port
            timeout_seconds: 30,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        }
    }
}
