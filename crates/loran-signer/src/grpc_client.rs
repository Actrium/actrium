//! Signer gRPC client

use crate::error::SignerError;
use crate::types::sign_request_payload;
use actrix_proto::admin::v1::NonceCredential;
use actrix_proto::signer::v1::{
    GenerateSigningKeyRequest, GetVerifyingKeyRequest, HealthCheckRequest, SignRequest,
    signer_client::SignerClient,
};
use base64::prelude::*;
use ed25519_dalek::VerifyingKey;
use nonce_auth::CredentialBuilder;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

/// Signer gRPC client configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GrpcClientConfig {
    /// Signer service address (gRPC endpoint)
    ///
    /// Example: "http://127.0.0.1:8080" or "https://signer.example.com:8443"
    pub endpoint: String,

    /// Actrix shared key (for authentication)
    pub actrix_shared_key: String,

    /// Request timeout (seconds)
    pub timeout_seconds: u64,

    /// Whether TLS is enabled
    pub enable_tls: bool,

    /// TLS domain name (required when TLS is enabled)
    pub tls_domain: Option<String>,

    /// CA certificate path (for server verification)
    pub ca_cert: Option<String>,

    /// Client certificate path (mTLS)
    pub client_cert: Option<String>,

    /// Client private key path (mTLS)
    pub client_key: Option<String>,
}

/// Signer gRPC client
pub struct GrpcClient {
    client: SignerClient<Channel>,
    actrix_shared_key: String,
}

impl GrpcClient {
    /// Create a new Signer gRPC client
    pub async fn new(config: &GrpcClientConfig) -> Result<Self, SignerError> {
        let mut endpoint = Endpoint::from_shared(config.endpoint.clone())
            .map_err(|e| SignerError::Internal(format!("Invalid endpoint: {e}")))?
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(config.timeout_seconds));

        // If TLS is enabled, configure TLS/mTLS
        if config.enable_tls {
            let tls_config = Self::build_tls_config(config)?;
            endpoint = endpoint
                .tls_config(tls_config)
                .map_err(|e| SignerError::Internal(format!("TLS configuration error: {e}")))?;
            crate::recording::info!("TLS enabled for Signer gRPC client");
        }

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| SignerError::Internal(format!("Failed to connect to Signer: {e}")))?;

        let client = SignerClient::new(channel);

        Ok(Self {
            client,
            actrix_shared_key: config.actrix_shared_key.clone(),
        })
    }

    /// Build TLS configuration
    fn build_tls_config(config: &GrpcClientConfig) -> Result<ClientTlsConfig, SignerError> {
        let tls_domain = config.tls_domain.as_ref().ok_or_else(|| {
            SignerError::Config("tls_domain is required when enable_tls is true".to_string())
        })?;

        let mut tls_config = ClientTlsConfig::new().domain_name(tls_domain);

        crate::recording::debug!("Configuring TLS with domain: {}", tls_domain);

        // Load trust roots: explicit ca_cert as certificate pin, otherwise fall back to OS native root store
        if let Some(ca_cert_path) = &config.ca_cert {
            crate::recording::debug!("Loading CA certificate from: {}", ca_cert_path);
            let ca_cert_pem = std::fs::read(ca_cert_path).map_err(|e| {
                SignerError::Config(format!(
                    "Failed to read CA certificate from {ca_cert_path}: {e}"
                ))
            })?;

            let ca_cert = Certificate::from_pem(ca_cert_pem);
            tls_config = tls_config.ca_certificate(ca_cert);
            crate::recording::info!("CA certificate loaded for server verification");
        } else {
            // No explicit pin: trust platform native root store (relies on tonic's tls-native-roots feature).
            // If that feature is not enabled, with_native_roots() does not exist and the root store is empty, causing TLS verification to fail.
            tls_config = tls_config.with_native_roots();
            crate::recording::info!("Using platform native root store for server verification");
        }

        // Load client certificate and private key (mTLS)
        if let (Some(cert_path), Some(key_path)) = (&config.client_cert, &config.client_key) {
            crate::recording::debug!("Loading client certificate from: {}", cert_path);
            crate::recording::debug!("Loading client private key from: {}", key_path);

            let client_cert_pem = std::fs::read(cert_path).map_err(|e| {
                SignerError::Config(format!(
                    "Failed to read client certificate from {cert_path}: {e}"
                ))
            })?;

            let client_key_pem = std::fs::read(key_path).map_err(|e| {
                SignerError::Config(format!(
                    "Failed to read client private key from {key_path}: {e}"
                ))
            })?;

            let identity = Identity::from_pem(client_cert_pem, client_key_pem);
            tls_config = tls_config.identity(identity);
            crate::recording::info!("mTLS enabled: client certificate and key loaded");
        } else if config.client_cert.is_some() || config.client_key.is_some() {
            return Err(SignerError::Config(
                "Both client_cert and client_key must be provided for mTLS".to_string(),
            ));
        }

        Ok(tls_config)
    }

    /// Generate a new Ed25519 signing key pair from the Signer service
    ///
    /// Returns (key_id, verifying_key_bytes[32], expires_at, tolerance_seconds)
    /// Private key remains on the Signer service side, not returned to the caller
    pub async fn generate_signing_key(&mut self) -> Result<(u32, [u8; 32], u64, u64), SignerError> {
        let request_data = "generate_signing_key";

        // Create nonce credential
        let nonce_credential = CredentialBuilder::new(self.actrix_shared_key.as_bytes())
            .sign(request_data.as_bytes())?;

        // Convert to protobuf NonceCredential
        let credential = NonceCredential {
            timestamp: nonce_credential.timestamp,
            nonce: nonce_credential.nonce,
            signature: nonce_credential.signature,
        };

        let request = tonic::Request::new(GenerateSigningKeyRequest { credential });

        crate::recording::debug!("Requesting Ed25519 signing key generation from Signer via gRPC");

        let response = self
            .client
            .generate_signing_key(request)
            .await
            .map_err(|e| SignerError::Internal(format!("gRPC GenerateSigningKey failed: {e}")))?;

        let resp = response.into_inner();

        // Decode verifying key (32-byte Ed25519 public key)
        let verifying_key_bytes = BASE64_STANDARD
            .decode(&resp.verifying_key)
            .map_err(|e| SignerError::Crypto(format!("Failed to decode verifying key: {e}")))?;

        if verifying_key_bytes.len() != 32 {
            return Err(SignerError::Crypto(format!(
                "Invalid verifying key length: expected 32 bytes, got {}",
                verifying_key_bytes.len()
            )));
        }

        // Validate verifying key legitimacy
        let vk_array: [u8; 32] = verifying_key_bytes.try_into().map_err(|_| {
            SignerError::Crypto("Failed to convert verifying key to array".to_string())
        })?;

        VerifyingKey::from_bytes(&vk_array)
            .map_err(|e| SignerError::Crypto(format!("Invalid Ed25519 verifying key: {e}")))?;

        crate::recording::info!(
            "Successfully generated Ed25519 signing key via gRPC: key_id={}, expires_at={}, tolerance_seconds={}",
            resp.key_id,
            resp.expires_at,
            resp.tolerance_seconds
        );

        Ok((
            resp.key_id,
            vk_array,
            resp.expires_at,
            resp.tolerance_seconds,
        ))
    }

    /// Sign a message with a key stored in the Signer service (Ed25519)
    ///
    /// Returns 64-byte Ed25519 signature
    /// Private key does not leave the Signer service
    pub async fn sign(&mut self, key_id: u32, message: &[u8]) -> Result<Vec<u8>, SignerError> {
        let request_data = sign_request_payload(key_id, message);

        // Create nonce credential
        let nonce_credential = CredentialBuilder::new(self.actrix_shared_key.as_bytes())
            .sign(request_data.as_bytes())?;

        // Convert to protobuf NonceCredential
        let credential = NonceCredential {
            timestamp: nonce_credential.timestamp,
            nonce: nonce_credential.nonce,
            signature: nonce_credential.signature,
        };

        let request = tonic::Request::new(SignRequest {
            key_id,
            message: message.to_vec(),
            credential,
        });

        crate::recording::debug!(
            "Requesting signature for key_id={} from Signer via gRPC",
            key_id
        );

        let response = self
            .client
            .sign(request)
            .await
            .map_err(|e| SignerError::Internal(format!("gRPC Sign failed: {e}")))?;

        let resp = response.into_inner();

        if resp.signature.len() != 64 {
            return Err(SignerError::Crypto(format!(
                "Invalid signature length: expected 64 bytes, got {}",
                resp.signature.len()
            )));
        }

        crate::recording::info!(
            "Successfully obtained signature for key_id={} via gRPC",
            key_id
        );

        Ok(resp.signature)
    }

    /// Get the verifying public key for the specified key_id
    ///
    /// Returns (verifying_key_bytes [u8;32], expires_at, tolerance_seconds)
    pub async fn get_verifying_key(
        &mut self,
        key_id: u32,
    ) -> Result<([u8; 32], u64, u64), SignerError> {
        let request_data = format!("get_verifying_key:{key_id}");
        let nonce_credential = CredentialBuilder::new(self.actrix_shared_key.as_bytes())
            .sign(request_data.as_bytes())?;

        let credential = NonceCredential {
            timestamp: nonce_credential.timestamp,
            nonce: nonce_credential.nonce,
            signature: nonce_credential.signature,
        };

        let request = tonic::Request::new(GetVerifyingKeyRequest { key_id, credential });

        let response = self
            .client
            .get_verifying_key(request)
            .await
            .map_err(|e| SignerError::Internal(format!("gRPC GetVerifyingKey failed: {e}")))?;

        let resp = response.into_inner();

        let key_bytes = BASE64_STANDARD
            .decode(&resp.verifying_key)
            .map_err(|e| SignerError::Internal(format!("Invalid verifying key base64: {e}")))?;

        let key_array: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| SignerError::Internal("Verifying key must be 32 bytes".to_string()))?;

        // Validate as a valid Ed25519 public key
        VerifyingKey::from_bytes(&key_array)
            .map_err(|e| SignerError::Internal(format!("Invalid verifying key: {e}")))?;

        Ok((key_array, resp.expires_at, resp.tolerance_seconds))
    }

    /// Health check
    pub async fn health_check(&mut self) -> Result<String, SignerError> {
        let request_data = "health_check";
        let nonce_credential = CredentialBuilder::new(self.actrix_shared_key.as_bytes())
            .sign(request_data.as_bytes())?;

        let credential = NonceCredential {
            timestamp: nonce_credential.timestamp,
            nonce: nonce_credential.nonce,
            signature: nonce_credential.signature,
        };

        let request = tonic::Request::new(HealthCheckRequest { credential });

        let response = self
            .client
            .health_check(request)
            .await
            .map_err(|e| SignerError::Internal(format!("gRPC HealthCheck failed: {e}")))?;

        let resp = response.into_inner();
        Ok(resp.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_client_config() {
        let config = GrpcClientConfig {
            endpoint: "http://127.0.0.1:8080".to_string(),
            actrix_shared_key: "test-key".to_string(),
            timeout_seconds: 30,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        };

        assert_eq!(config.endpoint, "http://127.0.0.1:8080");
        assert!(!config.enable_tls);
    }

    #[test]
    fn test_tls_config_validation() {
        let config = GrpcClientConfig {
            endpoint: "https://signer.example.com:8443".to_string(),
            actrix_shared_key: "test-key".to_string(),
            timeout_seconds: 30,
            enable_tls: true,
            tls_domain: None, // missing tls_domain
            ca_cert: None,
            client_cert: None,
            client_key: None,
        };

        let result = GrpcClient::build_tls_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_mtls_partial_config() {
        let config = GrpcClientConfig {
            endpoint: "https://signer.example.com:8443".to_string(),
            actrix_shared_key: "test-key".to_string(),
            timeout_seconds: 30,
            enable_tls: true,
            tls_domain: Some("signer.example.com".to_string()),
            ca_cert: None,
            client_cert: Some("/path/to/cert.pem".to_string()),
            client_key: None, // missing client_key
        };

        let result = GrpcClient::build_tls_config(&config);
        assert!(result.is_err());
    }
}
