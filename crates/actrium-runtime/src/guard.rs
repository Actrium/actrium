use actrium_core::session::AttachErrorCode;

use crate::registry::InMemorySessionRegistry;
use crate::{SessionError, SessionSecret, VerifiedChannel};

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
    /// Front-signed issuance time and absolute execution deadline. Both are
    /// covered by the request HMAC and checked against replay_retention.
    pub issued_at: u64,
    pub deadline: u64,
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
    channel: VerifiedChannel,
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
    /// consumes the request id for replay purposes. `channel` is the trusted
    /// transport context on which this message actually arrived, never a copy
    /// reconstructed from request metadata.
    pub async fn authorize(
        &self,
        request: BusinessRequest,
        channel: &VerifiedChannel,
    ) -> Result<GuardedRequest, SessionError> {
        self.registry
            .check_business_request(&request, channel, false)
            .await?;
        Ok(GuardedRequest {
            request,
            channel: channel.clone(),
        })
    }

    /// Rechecks the complete current session and policy state when a previously
    /// admitted request leaves a persistent queue. The delivery channel must be
    /// the same verified generation that was admitted; a newer channel requires
    /// a fresh request and proof rather than migrating an old queue entry.
    pub async fn dequeue(
        &self,
        queued: GuardedRequest,
        channel: &VerifiedChannel,
    ) -> Result<BusinessRequest, SessionError> {
        if !same_channel_generation(&queued.channel, channel) {
            return Err(SessionError::new(
                AttachErrorCode::ChannelIdentityMismatch,
                "queued request changed its verified channel generation",
            ));
        }
        self.registry
            .check_business_request(&queued.request, channel, true)
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

/// Compare the complete authenticated generation, not just its caller-visible
/// id. In particular expiry and runtime identities cannot be forged upward on
/// an old channel without changing this comparison.
pub(crate) fn same_channel_generation(left: &VerifiedChannel, right: &VerifiedChannel) -> bool {
    left.channel_id == right.channel_id
        && left.channel_transcript_digest == right.channel_transcript_digest
        && left.peer_actor_id == right.peer_actor_id
        && left.peer_actor_type == right.peer_actor_type
        && left.peer_member == right.peer_member
        && left.peer_client_key == right.peer_client_key
        && left.peer_role == right.peer_role
        && left.peer_actor_public_key == right.peer_actor_public_key
        && left.peer_credential_expires_at_epoch_secs == right.peer_credential_expires_at_epoch_secs
        && left.local_runtime_instance_id == right.local_runtime_instance_id
        && left.peer_runtime_instance_id == right.peer_runtime_instance_id
        && left.negotiation_digest == right.negotiation_digest
}
