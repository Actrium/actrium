//! TLS Configuration Implementation
//!
//! Provides TLS server configuration and crypto provider management

use anyhow::Result;
use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::io::BufReader;
use tokio_rustls::TlsAcceptor;

/// TLS configuration utilities
pub struct TlsConfigurer;

impl TlsConfigurer {
    /// Create TLS configuration
    pub fn create_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
        let cert_file = fs::File::open(cert_path)?;
        let key_file = fs::File::open(key_path)?;

        let mut cert_reader = BufReader::new(cert_file);
        let mut key_reader = BufReader::new(key_file);

        let cert_chain: Vec<CertificateDer> =
            rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

        let private_key: PrivateKeyDer = rustls_pemfile::private_key(&mut key_reader)?
            .ok_or_else(|| anyhow::anyhow!("No private key found"))?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)?;

        Ok(config)
    }

    /// Create a Tokio TLS acceptor.
    pub fn create_tokio_tls_config(cert_path: &str, key_path: &str) -> Result<TlsAcceptor> {
        let config = Self::create_tls_config(cert_path, key_path)?;
        Ok(TlsAcceptor::from(std::sync::Arc::new(config)))
    }

    /// Build a mutual-TLS server configuration for the dedicated Signer gRPC
    /// listener (SGN-02).
    ///
    /// Client certificates are verified against the CA bundle in
    /// `ca_cert_path` using rustls' `WebPkiClientVerifier`, so only callers
    /// presenting a certificate signed by that CA can reach the signing oracle.
    /// The server presents the certificate chain / key from `cert_path` /
    /// `key_path`. ALPN advertises `h2` because gRPC runs over HTTP/2.
    ///
    /// `install_crypto_provider` must have been called before invoking this.
    pub fn create_mtls_server_config(
        ca_cert_path: &str,
        cert_path: &str,
        key_path: &str,
    ) -> Result<ServerConfig> {
        use rustls::RootCertStore;
        use rustls::server::WebPkiClientVerifier;
        use std::sync::Arc;

        // Load the CA bundle used to verify client certificates.
        let ca_file = fs::File::open(ca_cert_path)
            .map_err(|e| anyhow::anyhow!("Failed to open Signer mTLS CA '{ca_cert_path}': {e}"))?;
        let mut ca_reader = BufReader::new(ca_file);
        let ca_certs: Vec<CertificateDer> =
            rustls_pemfile::certs(&mut ca_reader).collect::<Result<Vec<_>, _>>()?;
        if ca_certs.is_empty() {
            return Err(anyhow::anyhow!(
                "No CA certificate found in '{ca_cert_path}'"
            ));
        }
        let mut roots = RootCertStore::empty();
        for cert in ca_certs {
            roots
                .add(cert)
                .map_err(|e| anyhow::anyhow!("Invalid Signer mTLS CA certificate: {e}"))?;
        }

        let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build client certificate verifier: {e}"))?;

        // Load the server certificate chain and private key.
        let cert_file = fs::File::open(cert_path).map_err(|e| {
            anyhow::anyhow!("Failed to open Signer mTLS certificate '{cert_path}': {e}")
        })?;
        let key_file = fs::File::open(key_path)
            .map_err(|e| anyhow::anyhow!("Failed to open Signer mTLS key '{key_path}': {e}"))?;
        let mut cert_reader = BufReader::new(cert_file);
        let mut key_reader = BufReader::new(key_file);
        let cert_chain: Vec<CertificateDer> =
            rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;
        let private_key: PrivateKeyDer = rustls_pemfile::private_key(&mut key_reader)?
            .ok_or_else(|| anyhow::anyhow!("No private key found in '{key_path}'"))?;

        let mut config = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cert_chain, private_key)?;
        // gRPC requires HTTP/2; advertise it via ALPN.
        config.alpn_protocols = vec![b"h2".to_vec()];

        Ok(config)
    }

    /// Install the crypto provider
    pub fn install_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
}
