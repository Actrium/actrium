//! Signer gRPC service implementation

use crate::{error::SignerError, storage::KeyStorage};
use nonce_auth::{CredentialVerifier, NonceError, storage::NonceStorage};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Import generated protobuf code
use actrix_proto::admin::v1::NonceCredential;
use actrix_proto::signer::v1::signer_server::{Signer, SignerServer};
use actrix_proto::signer::v1::*;

/// Signer gRPC service state
#[derive(Clone)]
pub struct SignerGrpcService {
    pub storage: KeyStorage,
    pub nonce_storage: Arc<dyn NonceStorage + Send + Sync>,
    pub psk: String,
    pub tolerance_seconds: u64,
}

impl SignerGrpcService {
    /// Create a new gRPC service instance
    pub fn new<N: NonceStorage + Send + Sync + 'static>(
        storage: KeyStorage,
        nonce_storage: N,
        psk: String,
        tolerance_seconds: u64,
    ) -> Self {
        Self {
            storage,
            nonce_storage: Arc::new(nonce_storage),
            psk,
            tolerance_seconds,
        }
    }

    /// Verify the nonce credential of a request
    async fn verify_credential(
        &self,
        credential: &NonceCredential,
        request_payload: &str,
    ) -> Result<(), SignerError> {
        // Convert protobuf NonceCredential to nonce_auth::NonceCredential
        let nonce_credential = nonce_auth::NonceCredential {
            timestamp: credential.timestamp,
            nonce: credential.nonce.clone(),
            signature: credential.signature.clone(),
        };

        let verify_result = CredentialVerifier::new(self.nonce_storage.clone())
            .with_secret(self.psk.as_bytes())
            .verify(&nonce_credential, request_payload.as_bytes())
            .await;

        verify_result.map_err(|e| match e {
            NonceError::DuplicateNonce => {
                SignerError::ReplayAttack("Nonce already used".to_string())
            }
            NonceError::TimestampOutOfWindow => {
                SignerError::Authentication("Request timestamp out of range".to_string())
            }
            NonceError::InvalidSignature => {
                SignerError::Authentication("Invalid signature".to_string())
            }
            _ => SignerError::Internal(format!("Authentication error: {e}")),
        })?;

        Ok(())
    }
}

#[tonic::async_trait]
impl Signer for SignerGrpcService {
    /// Generate a new Ed25519 signing key pair
    async fn generate_signing_key(
        &self,
        request: Request<GenerateSigningKeyRequest>,
    ) -> Result<Response<GenerateSigningKeyResponse>, Status> {
        crate::recording::info!("Received gRPC GenerateSigningKey request");

        let req = request.into_inner();

        let request_data = "generate_signing_key";
        self.verify_credential(&req.credential, request_data)
            .await
            .map_err(|e| Status::unauthenticated(format!("Authentication failed: {e}")))?;

        // Generate Ed25519 key pair (private key stored in backend)
        let key_pair = self
            .storage
            .generate_and_store_key()
            .await
            .map_err(|e| Status::internal(format!("Failed to generate signing key: {e}")))?;

        // Get the key record to obtain the expiration time
        let key_record = self
            .storage
            .get_key_record(key_pair.key_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get key record: {e}")))?
            .ok_or_else(|| Status::internal("Failed to get key record after creation"))?;

        crate::recording::info!(
            "Generated Ed25519 signing key with key_id: {}",
            key_pair.key_id
        );

        let response = GenerateSigningKeyResponse {
            key_id: key_pair.key_id,
            verifying_key: key_pair.verifying_key,
            expires_at: key_record.expires_at,
            tolerance_seconds: self.tolerance_seconds,
        };

        Ok(Response::new(response))
    }

    /// Sign a message with the specified key (Ed25519)
    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let req = request.into_inner();
        let key_id = req.key_id;

        crate::recording::info!("Received gRPC Sign request for key_id: {}", key_id);

        // nonce payload is bound to a specific key_id to prevent reuse
        let request_data = format!("sign:{key_id}");
        self.verify_credential(&req.credential, &request_data)
            .await
            .map_err(|e| Status::unauthenticated(format!("Authentication failed: {e}")))?;

        // Check whether the key is still valid within the tolerance period
        let key_record = self
            .storage
            .get_key_record(key_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get key record: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Key not found: {key_id}")))?;

        if key_record.expires_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if key_record.expires_at + self.tolerance_seconds < now {
                crate::recording::warn!("Key {} has expired beyond tolerance period", key_id);
                return Err(Status::not_found(format!("Key {key_id} has expired")));
            }

            if key_record.expires_at < now {
                crate::recording::warn!("Key {} is in tolerance period, signing allowed", key_id);
            }
        }

        // Signing is performed internally in the backend; private key does not leave Signer
        let signature = self
            .storage
            .sign(key_id, &req.message)
            .await
            .map_err(|e| match e {
                crate::error::SignerError::NotFound(_) => {
                    Status::not_found(format!("Key not found: {key_id}"))
                }
                other => Status::internal(format!("Sign failed: {other}")),
            })?;

        crate::recording::info!("Signed message for key_id: {}", key_id);

        Ok(Response::new(SignResponse { signature }))
    }

    /// Get the verifying public key for the specified key_id
    async fn get_verifying_key(
        &self,
        request: Request<GetVerifyingKeyRequest>,
    ) -> Result<Response<GetVerifyingKeyResponse>, Status> {
        let req = request.into_inner();
        let key_id = req.key_id;

        crate::recording::info!(
            "Received gRPC GetVerifyingKey request for key_id: {}",
            key_id
        );

        let request_data = format!("get_verifying_key:{key_id}");
        self.verify_credential(&req.credential, &request_data)
            .await
            .map_err(|e| Status::unauthenticated(format!("Authentication failed: {e}")))?;

        let key_record = self
            .storage
            .get_key_record(key_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get key record: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Key not found: {key_id}")))?;

        // Check whether the tolerance period has been exceeded
        if key_record.expires_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if key_record.expires_at + self.tolerance_seconds < now {
                crate::recording::warn!("Key {} has expired beyond tolerance period", key_id);
                return Err(Status::not_found(format!("Key {key_id} has expired")));
            }
        }

        crate::recording::debug!("Returning verifying key for key_id: {}", key_id);

        Ok(Response::new(GetVerifyingKeyResponse {
            key_id,
            verifying_key: key_record.public_key,
            expires_at: key_record.expires_at,
            tolerance_seconds: self.tolerance_seconds,
        }))
    }

    /// Health check
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        crate::recording::debug!("gRPC health check requested");
        let req = request.into_inner();

        // Consistent with other methods: health also requires nonce-auth
        self.verify_credential(&req.credential, "health_check")
            .await
            .map_err(|e| Status::unauthenticated(format!("Authentication failed: {e}")))?;

        let key_count = self
            .storage
            .get_key_count()
            .await
            .map_err(|e| Status::internal(format!("Failed to get key count: {e}")))?;

        let response = HealthCheckResponse {
            status: "healthy".to_string(),
            service: "signer".to_string(),
            backend: self.storage.backend_name().to_string(),
            key_count,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        Ok(Response::new(response))
    }
}

/// Create a gRPC server
pub fn create_grpc_service<N: NonceStorage + Send + Sync + 'static>(
    storage: KeyStorage,
    nonce_storage: N,
    psk: String,
    tolerance_seconds: u64,
) -> SignerServer<SignerGrpcService> {
    let service = SignerGrpcService::new(storage, nonce_storage, psk, tolerance_seconds);
    SignerServer::new(service)
}
