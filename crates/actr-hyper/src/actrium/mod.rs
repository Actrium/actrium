//! Actrium Front–Service session wiring for the Hyper host.
//!
//! This module connects `actrium-runtime` (Attach, exclusive lease, request
//! proofs) to the transport's verified channels and inbound delivery:
//!
//! - [`ActriumServiceRegistry`] dispatches the exact `actrium.session.v1`
//!   system routes to an [`InMemorySessionService`].
//! - [`ActriumServiceGuard`] authorizes business requests, tells, and stream
//!   chunks with the session's current policy and signs actual responses.
//!
//! Everything is opt-in through `Node<Attached>::with_actrium_service`; a node
//! without it keeps the existing behavior. Session methods and business RPC
//! require a verified Actor channel: legacy or in-process transports have no
//! authenticated channel generation and are rejected rather than trusted.
//!
//! Wire payloads for the session system routes are serde JSON envelopes of
//! the `actrium-core` domain types, matching the JSON envelope already used
//! for `ActorChannelCertificate`. Business proofs travel in envelope/chunk
//! metadata as hex-encoded HMAC-SHA-256 under [`metadata::PROOF`].

use actrium_runtime::{AttachErrorCode, MemberRole, SessionError, VerifiedChannel};

use crate::lifecycle::{IncomingGuardError, IncomingGuardErrorCode};
use crate::transport::VerifiedPeerContext;

/// Envelope and stream-chunk metadata keys carrying session proof inputs.
/// Every value is covered by the request HMAC; the transport-authenticated
/// channel context is always compared against these claims.
pub mod metadata {
    pub const SESSION_ID: &str = "actrium.session_id";
    pub const LEASE_EPOCH: &str = "actrium.lease_epoch";
    pub const SERVICE_ACTOR_ID: &str = "actrium.service_actor_id";
    pub const CHANNEL_ID: &str = "actrium.channel_id";
    pub const CAPABILITY: &str = "actrium.capability";
    /// Stream chunks carry no envelope route, so their route is explicit
    /// metadata. Envelope requests use `RpcEnvelope.route_key` instead.
    pub const ROUTE: &str = "actrium.route";
    pub const ISSUED_AT: &str = "actrium.issued_at";
    pub const DEADLINE: &str = "actrium.deadline";
    pub const IDEMPOTENCY_KEY: &str = "actrium.idempotency_key";
    /// Hex-encoded HMAC-SHA-256 proof of the complete business request.
    pub const PROOF: &str = "actrium.request.proof";
}

pub mod front;
pub mod service;

pub use front::{ActriumAttachParams, ActriumFrontSession};
pub use service::{ActriumServiceGuard, ActriumServiceRegistry};

#[cfg(test)]
mod tests;

pub(crate) fn member_role(role: &str) -> Result<MemberRole, IncomingGuardError> {
    match role {
        "owner" => Ok(MemberRole::Owner),
        "editor" => Ok(MemberRole::Editor),
        "viewer" => Ok(MemberRole::Viewer),
        other => Err(IncomingGuardError::new(
            IncomingGuardErrorCode::PermissionDenied,
            format!("verified channel carries an unknown member role `{other}`"),
        )),
    }
}

/// Project the transport-authenticated peer context into the session
/// protocol's channel view. The context is produced by the verified relay
/// handshake, never reconstructed from caller-declared metadata.
///
/// Hyper does not retain a separate transport negotiation digest; the policy
/// binding captured by the same verification fills that slot so complete
/// generation comparisons stay stable within one verified channel.
pub(crate) fn verified_channel(
    context: &VerifiedPeerContext,
) -> Result<VerifiedChannel, IncomingGuardError> {
    Ok(VerifiedChannel {
        channel_id: context.channel_id.clone(),
        channel_transcript_digest: context.transcript_digest.clone(),
        peer_actor_id: context.peer_actor_id.to_string_repr(),
        peer_actor_type: context.peer_actor_type.to_string_repr(),
        peer_member: context.peer_member.clone(),
        peer_client_key: context.peer_client_key.clone(),
        peer_role: member_role(&context.peer_role)?,
        peer_actor_public_key: context.peer_actor_public_key.clone(),
        peer_credential_expires_at_epoch_secs: context.peer_credential_expires_at_epoch_secs,
        local_runtime_instance_id: context.local_runtime_instance_id.clone(),
        peer_runtime_instance_id: context.peer_runtime_instance_id.clone(),
        negotiation_digest: context.current_policy_binding.clone(),
    })
}

pub(crate) fn missing_verified_channel() -> IncomingGuardError {
    IncomingGuardError::new(
        IncomingGuardErrorCode::PermissionDenied,
        "Actrium sessions require a verified Actor channel".to_string(),
    )
}

pub(crate) fn invalid_metadata(message: impl Into<String>) -> IncomingGuardError {
    IncomingGuardError::new(IncomingGuardErrorCode::InvalidMetadata, message)
}

pub(crate) fn guard_error(error: SessionError) -> IncomingGuardError {
    let code = match error.code {
        AttachErrorCode::SessionExpired | AttachErrorCode::ChannelExpired => {
            IncomingGuardErrorCode::Revoked
        }
        AttachErrorCode::RateLimited | AttachErrorCode::ServiceOffline => {
            IncomingGuardErrorCode::Unavailable
        }
        _ => IncomingGuardErrorCode::PermissionDenied,
    };
    IncomingGuardError::new(code, format!("{:?}: {}", error.code, error.message))
}

/// Stable session-protocol error for the RPC wire. The `AttachErrorCode`
/// discriminant is preserved in the message so callers can distinguish
/// occupancy, expiry, and permission failures instead of collapsing them into
/// a generic connection error.
pub(crate) fn session_actr_error(error: SessionError) -> actr_protocol::ActrError {
    let message = format!("actrium.session/v1:{:?}:{}", error.code, error.message);
    match error.code {
        AttachErrorCode::RateLimited
        | AttachErrorCode::ServiceOffline
        | AttachErrorCode::SessionOccupied => actr_protocol::ActrError::Unavailable(message),
        _ => actr_protocol::ActrError::PermissionDenied(message),
    }
}
