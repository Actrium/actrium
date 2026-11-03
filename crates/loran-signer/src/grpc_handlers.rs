//! Signer gRPC service implementation

use crate::{
    error::SignerError,
    storage::KeyStorage,
    types::{ensure_named_key_payload, retire_named_key_payload, sign_request_payload},
};
use nonce_auth::{CredentialVerifier, NonceError, storage::NonceStorage};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Import the generated protobuf code
use loran_proto::admin::v1::NonceCredential;
use loran_proto::signer::v1::signer_server::{Signer, SignerServer};
use loran_proto::signer::v1::*;

/// Signer gRPC service state
#[derive(Clone)]
pub struct SignerGrpcService {
    pub storage: KeyStorage,
    pub nonce_storage: Arc<dyn NonceStorage + Send + Sync>,
    pub psk: String,
}

impl SignerGrpcService {
    /// Create a new gRPC service instance
    pub fn new<N: NonceStorage + Send + Sync + 'static>(
        storage: KeyStorage,
        nonce_storage: N,
        psk: String,
    ) -> Self {
        Self {
            storage,
            nonce_storage: Arc::new(nonce_storage),
            psk,
        }
    }

    /// Verify the request's nonce credential
    async fn verify_credential(
        &self,
        credential: &NonceCredential,
        request_payload: &str,
    ) -> Result<(), SignerError> {
        // Convert the protobuf NonceCredential into nonce_auth::NonceCredential
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
    /// Sign a message with the given key using Ed25519
    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let req = request.into_inner();
        let key_id = req.key_id;

        crate::recording::info!("Received gRPC Sign request for key_id: {}", key_id);

        // Bind authentication to both the selected key and the exact message.
        let request_data = sign_request_payload(key_id, &req.message);
        self.verify_credential(&req.credential, &request_data)
            .await
            .map_err(Status::from)?;

        // Sign inside the backend; the private key never leaves the Signer
        let signature = self
            .storage
            .sign(key_id, &req.message)
            .await
            .map_err(Status::from)?;

        crate::recording::info!("Signed message for key_id: {}", key_id);

        Ok(Response::new(SignResponse { signature }))
    }

    /// Get the verifying public key for the given key_id
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
            .map_err(Status::from)?;

        let key_record = self
            .storage
            .get_key_record(key_id)
            .await
            .map_err(Status::from)?
            .ok_or_else(|| Status::not_found("not found"))?;

        crate::recording::debug!("Returning verifying key for key_id: {key_id}");

        Ok(Response::new(GetVerifyingKeyResponse {
            key_id,
            verifying_key: key_record.public_key,
        }))
    }

    /// Return the key with the given name, generating it if missing
    async fn ensure_named_key(
        &self,
        request: Request<EnsureNamedKeyRequest>,
    ) -> Result<Response<EnsureNamedKeyResponse>, Status> {
        let req = request.into_inner();
        self.verify_credential(&req.credential, &ensure_named_key_payload(&req.name))
            .await
            .map_err(Status::from)?;
        if req.name.is_empty() {
            return Err(Status::invalid_argument("key name is empty"));
        }

        let key_pair = self
            .storage
            .ensure_named_key(&req.name)
            .await
            .map_err(Status::from)?;
        crate::recording::info!(
            "Named key '{}' resolved to key_id: {}",
            req.name,
            key_pair.key_id
        );

        Ok(Response::new(EnsureNamedKeyResponse {
            key_id: key_pair.key_id,
            verifying_key: key_pair.verifying_key,
        }))
    }

    /// Retire the named key; it never signs again
    async fn retire_named_key(
        &self,
        request: Request<RetireNamedKeyRequest>,
    ) -> Result<Response<RetireNamedKeyResponse>, Status> {
        let req = request.into_inner();
        self.verify_credential(&req.credential, &retire_named_key_payload(&req.name))
            .await
            .map_err(Status::from)?;
        if req.name.is_empty() {
            return Err(Status::invalid_argument("key name is empty"));
        }

        let key_id = self
            .storage
            .retire_named_key(&req.name)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(RetireNamedKeyResponse { key_id }))
    }

    /// Health check
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        crate::recording::debug!("gRPC health check requested");
        let req = request.into_inner();

        // Like every other method, health also requires nonce-auth
        self.verify_credential(&req.credential, "health_check")
            .await
            .map_err(Status::from)?;

        let key_count = self.storage.get_key_count().await.map_err(Status::from)?;

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

/// Create the gRPC server
pub fn create_grpc_service<N: NonceStorage + Send + Sync + 'static>(
    storage: KeyStorage,
    nonce_storage: N,
    psk: String,
) -> SignerServer<SignerGrpcService> {
    let service = SignerGrpcService::new(storage, nonce_storage, psk);
    SignerServer::new(service)
}
