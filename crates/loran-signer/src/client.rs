//! Signer client - simple HTTP client (for testing only)

use crate::types::{SignRequest, SignResponse, sign_request_payload};
use nonce_auth::CredentialBuilder;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Signer service client
#[derive(Debug, Clone)]
pub struct Client {
    endpoint: String,
    client: reqwest::Client,
    actrix_shared_key: String,
}

/// Signer client configuration
///
/// Configuration used by other services when connecting to the Signer service as a client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
    /// Signer service address
    ///
    /// Full URL of the Signer service, including protocol, host, and port.
    /// e.g. "http://127.0.0.1:8090" or "https://signer.example.com"
    pub endpoint: String,

    /// PSK (Pre-Shared Key) - used for authentication
    ///
    /// Shared key for internal inter-service authentication, typically obtained from the global config's `actrix_shared_key`
    pub psk: String,

    /// Request timeout (seconds)
    ///
    /// Timeout for connecting to the Signer service
    pub timeout_seconds: u64,
}

impl Client {
    /// Create a new Signer client
    pub fn new(config: &ClientConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            endpoint: config.endpoint.clone(),
            client,
            actrix_shared_key: config.psk.clone(),
        }
    }

    /// Sign a message using a key in the Signer service
    ///
    /// Returns a 64-byte Ed25519 signature
    pub async fn sign(
        &self,
        key_id: u32,
        message: &[u8],
    ) -> Result<Vec<u8>, crate::error::SignerError> {
        let url = format!("{}/sign/{}", self.endpoint, key_id);
        let request_data = sign_request_payload(key_id, message);

        let credential = CredentialBuilder::new(self.actrix_shared_key.as_bytes())
            .sign(request_data.as_bytes())?;

        let request = SignRequest {
            key_id,
            message: message.to_vec(),
            credential,
        };

        crate::recording::debug!(
            "Requesting signature for key_id={} from Signer at {}",
            key_id,
            url
        );

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::SignerError::Internal(format!(
                "Signer sign request failed with status {status}: {error_text}"
            )));
        }

        let response: SignResponse = response.json().await?;

        if response.signature.len() != 64 {
            return Err(crate::error::SignerError::Crypto(format!(
                "Invalid signature length: expected 64 bytes, got {}",
                response.signature.len()
            )));
        }

        crate::recording::info!(
            "Successfully obtained signature for key_id={} from Signer",
            key_id
        );
        Ok(response.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ClientConfig {
            endpoint: "http://127.0.0.1:8090".to_string(),
            psk: "test-shared-key".to_string(),
            timeout_seconds: 30,
        };

        let client = Client::new(&config);
        assert_eq!(client.endpoint, "http://127.0.0.1:8090");
    }
}
