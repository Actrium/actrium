//! Generic authorization hook for inbound RPC and stream delivery.
//!
//! Request and tell admission runs after the existing ACL and before request
//! deduplication. Responses pass through the same hook before waking a waiting
//! caller, and stream chunks are checked immediately before their callback runs.
//! The durable mailbox rechecks requests at dequeue time, so all decisions use
//! decisions use current policy rather than only arrival-time state.
//! Exact Actrium session system routes are dispatched to a configured
//! [`SessionRegistry`]; unknown protocol routes never inherit that exemption.

use actr_protocol::{
    ActorResult, ActrError, ActrId, ActrType, DataChunk, MetadataEntry, RpcEnvelope,
};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

use crate::transport::VerifiedPeerContext;

/// The caller authenticated by the transport and recorded with the message.
pub type IncomingCaller<'a> = (&'a ActrId, Option<&'a ActrType>);

/// Namespace reserved for the Actrium session registry, not workload methods.
pub const ACTRIUM_SESSION_V1_ROUTE_PREFIX: &str = "actrium.session.v1.";

/// Protocol-defined Front–Service session methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionMethod {
    Attach,
    Resume,
    Detach,
    Heartbeat,
    Takeover,
    GetStatus,
}

impl SessionMethod {
    fn from_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "Attach" => Some(Self::Attach),
            "Resume" => Some(Self::Resume),
            "Detach" => Some(Self::Detach),
            "Heartbeat" => Some(Self::Heartbeat),
            "Takeover" => Some(Self::Takeover),
            "GetStatus" => Some(Self::GetStatus),
            _ => None,
        }
    }
}

/// Match only protocol-defined session routes. Both the documented package
/// form and the generated `SessionRegistry` service form name the same registry;
/// arbitrary descendants of the reserved namespace are not system methods.
pub fn actrium_session_v1_method(route_key: &str) -> Option<SessionMethod> {
    let suffix = route_key
        .strip_prefix("actrium.session.v1.SessionRegistry.")
        .or_else(|| route_key.strip_prefix(ACTRIUM_SESSION_V1_ROUTE_PREFIX))?;
    SessionMethod::from_suffix(suffix)
}

/// Whether the generic incoming guard applies to a request/tell `route_key`.
/// Exact session methods are authorized by [`SessionRegistry`] instead. Every
/// other route, including unknown names beneath the reserved prefix, requires
/// the guard. Responses always use the guard regardless of their route.
pub fn route_requires_incoming_guard(route_key: &str) -> bool {
    actrium_session_v1_method(route_key).is_none()
}

/// Trusted dispatch boundary for session protocol methods. Hyper never falls
/// back to a workload handler for these routes: no configured registry is a
/// fail-closed configuration error.
#[async_trait]
pub trait SessionRegistry: Send + Sync + 'static {
    async fn dispatch(
        &self,
        method: SessionMethod,
        request: IncomingDispatchRequest<'_>,
    ) -> ActorResult<Bytes>;
}

/// Typed, read-only access to the metadata carried by an RPC envelope.
///
/// Values remain strings on the existing wire. Helpers reject ambiguous
/// duplicate keys instead of silently choosing an attacker-controlled value.
#[derive(Clone, Copy, Debug)]
pub struct IncomingMetadata<'a> {
    entries: &'a [MetadataEntry],
}

impl<'a> IncomingMetadata<'a> {
    pub fn new(entries: &'a [MetadataEntry]) -> Self {
        Self { entries }
    }

    pub fn entries(self) -> &'a [MetadataEntry] {
        self.entries
    }

    pub fn is_empty(self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(self, key: &str) -> Option<&'a str> {
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
    }

    /// Read one optional unsigned integer, rejecting duplicate or malformed
    /// values. Security metadata must never use first-value-wins semantics.
    pub fn get_u64(self, key: &str) -> Result<Option<u64>, IncomingMetadataError> {
        let mut values = self
            .entries
            .iter()
            .filter(|entry| entry.key == key)
            .map(|entry| entry.value.as_str());
        let Some(value) = values.next() else {
            return Ok(None);
        };
        if values.next().is_some() {
            return Err(IncomingMetadataError {
                key: key.to_string(),
                kind: IncomingMetadataErrorKind::Duplicate,
            });
        }
        value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| IncomingMetadataError {
                key: key.to_string(),
                kind: IncomingMetadataErrorKind::InvalidU64,
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingMetadataErrorKind {
    Duplicate,
    InvalidU64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingMetadataError {
    pub key: String,
    pub kind: IncomingMetadataErrorKind,
}

impl std::fmt::Display for IncomingMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            IncomingMetadataErrorKind::Duplicate => {
                write!(f, "duplicate incoming metadata key `{}`", self.key)
            }
            IncomingMetadataErrorKind::InvalidU64 => {
                write!(f, "incoming metadata `{}` is not a u64", self.key)
            }
        }
    }
}

impl std::error::Error for IncomingMetadataError {}

/// One inbound RPC presented to [`IncomingGuard`] or [`SessionRegistry`].
#[derive(Clone, Debug)]
pub struct IncomingDispatchRequest<'a> {
    pub envelope: &'a RpcEnvelope,
    pub caller: Option<IncomingCaller<'a>>,
    pub metadata: IncomingMetadata<'a>,
    /// Authenticated channel generation on which this message actually arrived.
    /// `None` denotes a legacy or in-process transport, never a verified peer.
    pub peer_context: Option<Arc<VerifiedPeerContext>>,
}

impl<'a> IncomingDispatchRequest<'a> {
    pub fn new(envelope: &'a RpcEnvelope, caller: Option<IncomingCaller<'a>>) -> Self {
        Self {
            envelope,
            caller,
            metadata: IncomingMetadata::new(&envelope.metadata),
            peer_context: None,
        }
    }

    pub fn with_peer_context(mut self, context: Option<Arc<VerifiedPeerContext>>) -> Self {
        self.peer_context = context;
        self
    }
}

/// One inbound stream chunk presented to [`IncomingGuard`]. The sender comes
/// from the authenticated transport record, not from chunk metadata.
#[derive(Clone, Debug)]
pub struct IncomingStreamChunk<'a> {
    pub chunk: &'a DataChunk,
    pub sender: &'a ActrId,
    pub metadata: IncomingMetadata<'a>,
    pub peer_context: Option<Arc<VerifiedPeerContext>>,
}

impl<'a> IncomingStreamChunk<'a> {
    pub fn new(chunk: &'a DataChunk, sender: &'a ActrId) -> Self {
        Self {
            chunk,
            sender,
            metadata: IncomingMetadata::new(&chunk.metadata),
            peer_context: None,
        }
    }

    pub fn with_peer_context(mut self, context: Option<Arc<VerifiedPeerContext>>) -> Self {
        self.peer_context = context;
        self
    }
}

/// Stable machine-readable rejection codes beneath the existing RPC wire code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingGuardErrorCode {
    PermissionDenied,
    Revoked,
    StaleChannelGeneration,
    InvalidMetadata,
    Unavailable,
}

impl IncomingGuardErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::Revoked => "revoked",
            Self::StaleChannelGeneration => "stale_channel_generation",
            Self::InvalidMetadata => "invalid_metadata",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Authorization rejection returned by an [`IncomingGuard`].
///
/// `ActrError` intentionally remains the outer RPC error type. The versioned
/// message prefix plus `code` preserves a stable subcode without coupling Hyper
/// to Actrium's role or session error enums.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingGuardError {
    pub code: IncomingGuardErrorCode,
    pub message: String,
}

impl IncomingGuardError {
    const WIRE_PREFIX: &'static str = "incoming_guard/v1:";

    pub fn new(code: IncomingGuardErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_metadata(error: IncomingMetadataError) -> Self {
        Self::new(IncomingGuardErrorCode::InvalidMetadata, error.to_string())
    }

    /// Stable message carried by the existing `ErrorResponse.message` field.
    pub fn wire_message(&self) -> String {
        format!(
            "{}{}:{}",
            Self::WIRE_PREFIX,
            self.code.as_str(),
            self.message
        )
    }

    pub fn into_actr_error(self) -> ActrError {
        let message = self.wire_message();
        match self.code {
            IncomingGuardErrorCode::InvalidMetadata => ActrError::InvalidArgument(message),
            IncomingGuardErrorCode::Unavailable => ActrError::Unavailable(message),
            IncomingGuardErrorCode::PermissionDenied
            | IncomingGuardErrorCode::Revoked
            | IncomingGuardErrorCode::StaleChannelGeneration => {
                ActrError::PermissionDenied(message)
            }
        }
    }
}

/// Current-policy authorization for inbound delivery.
///
/// Implementations may consult an Actrium session registry and interpret the
/// verified peer context there. Hyper supplies the authenticated transport
/// context separately from caller-declared envelope metadata.
/// `check` covers request, tell, and response envelopes. Implementations must
/// inspect `envelope.direction` when their proof binds the message kind.
#[async_trait]
pub trait IncomingGuard: Send + Sync + 'static {
    async fn check(&self, request: IncomingDispatchRequest<'_>) -> Result<(), IncomingGuardError>;

    /// Authenticate an actual outbound response after the workload has returned.
    /// This must replace, not reuse, any proof inherited from the request.
    async fn authenticate_response(
        &self,
        response: &mut RpcEnvelope,
    ) -> Result<(), IncomingGuardError>;

    /// Authorize a stream chunk immediately before callback delivery.
    async fn check_stream(
        &self,
        request: IncomingStreamChunk<'_>,
    ) -> Result<(), IncomingGuardError>;

    /// Release any per-request state kept by the guard after the request has
    /// been dispatched and its response (if any) has been sent. The default
    /// no-op keeps optional-state guards from needing an empty implementation.
    async fn finish(&self, _request_id: &str) -> Result<(), IncomingGuardError> {
        Ok(())
    }
}
