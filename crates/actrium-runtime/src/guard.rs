use actrium_core::session::AttachErrorCode;

use crate::registry::InMemorySessionRegistry;
use crate::{SessionError, SessionSecret};

/// The kind is authenticated by the request proof; request, response, tell,
/// and stream chunks therefore share exactly the same Guard path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Request,
    Response,
    Tell,
    StreamChunk,
}

#[derive(Debug, Clone)]
pub struct BusinessRequest {
    pub session_id: String,
    pub lease_epoch: u64,
    pub service_actor_id: String,
    pub channel_id: String,
    pub route: String,
    pub capability: String,
    pub request_id: String,
    pub kind: MessageKind,
    pub idempotency_key: Option<String>,
    pub payload: Vec<u8>,
    pub proof: Vec<u8>,
}

impl BusinessRequest {
    /// Signs all authority-bearing host metadata and a digest of the payload.
    pub fn sign(&mut self, secret: &SessionSecret) {
        self.proof = crate::request_proof(secret.as_bytes(), self);
    }
}

/// Proof that a request passed the pre-dispatch Guard. It is not `Clone`, so a
/// queued request has one explicit dequeue/recheck path.
#[derive(Debug)]
pub struct GuardedRequest {
    request: BusinessRequest,
}

impl GuardedRequest {
    pub fn request(&self) -> &BusinessRequest {
        &self.request
    }
}

#[derive(Clone)]
pub struct SessionGuard {
    registry: InMemorySessionRegistry,
}

impl SessionGuard {
    pub fn new(registry: InMemorySessionRegistry) -> Self {
        Self { registry }
    }

    /// Runs before deduplication and business dispatch. Successful admission
    /// consumes the request id for replay purposes.
    pub async fn authorize(
        &self,
        request: BusinessRequest,
    ) -> Result<GuardedRequest, SessionError> {
        self.registry
            .check_business_request(&request, false)
            .await?;
        Ok(GuardedRequest { request })
    }

    /// Rechecks the complete current session and policy state when a previously
    /// admitted request leaves a persistent queue.
    pub async fn dequeue(&self, queued: GuardedRequest) -> Result<BusinessRequest, SessionError> {
        self.registry
            .check_business_request(&queued.request, true)
            .await?;
        Ok(queued.request)
    }

    pub async fn reject_stale_epoch(
        &self,
        request: &BusinessRequest,
        current_epoch: u64,
    ) -> Result<(), SessionError> {
        if request.lease_epoch == current_epoch {
            Ok(())
        } else {
            Err(SessionError::new(
                AttachErrorCode::SessionEpochMismatch,
                "request epoch is stale",
            ))
        }
    }
}
