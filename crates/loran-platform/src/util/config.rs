//! TLS configuration implementation
//!
//! Provides TLS server configuration and crypto provider management functionality

use anyhow::Result;
use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::fs;
use tokio_rustls::TlsAcceptor;

/// TLS configuration utilities
pub struct TlsConfigurer;

impl TlsConfigurer {
    /// Create TLS configuration
    pub fn create_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
        let cert_file = fs::File::open(cert_path)?;
        let key_file = fs::File::open(key_path)?;

        let cert_chain: Vec<CertificateDer> =
            CertificateDer::pem_reader_iter(cert_file).collect::<Result<Vec<_>, _>>()?;

        let private_key: PrivateKeyDer = PrivateKeyDer::pem_reader_iter(key_file)
            .next()
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("No private key found"))?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)?;

        Ok(config)
    }

    /// Create Tokio TLS configuration
    pub fn create_tokio_tls_config(cert_path: &str, key_path: &str) -> Result<TlsAcceptor> {
        let config = Self::create_tls_config(cert_path, key_path)?;
        Ok(TlsAcceptor::from(std::sync::Arc::new(config)))
    }

    /// Install crypto provider
    pub fn install_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
}
