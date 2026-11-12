//! Generic authorization hook for inbound business RPC dispatch.
//!
//! The hook runs after the existing ACL and before request deduplication. The
//! durable mailbox calls the same admission path when a record is dequeued, so
//! a decision is based on current policy rather than only on enqueue-time state.
//! Actrium session system routes are deliberately excluded: their registry owns
//! the session protocol, while this hook authorizes business RPC only.

use actr_protocol::{ActrError, ActrId, ActrType, MetadataEntry, RpcEnvelope};
use async_trait::async_trait;

/// The caller authenticated by the transport and recorded with the message.
pub type IncomingCaller<'a> = (&'a ActrId, Option<&'a ActrType>);

/// Routes handled by the Actrium session registry, not business dispatch.
pub const ACTRIUM_SESSION_V1_ROUTE_PREFIX: &str = "actrium.session.v1.";

/// Whether the generic incoming guard applies to `route_key`.
///
/// The trailing dot is significant: `actrium.session.v10.*` is not a v1 system
/// route and must not bypass business authorization.
pub fn route_requires_incoming_guard(route_key: &str) -> bool {
    !route_key.starts_with(ACTRIUM_SESSION_V1_ROUTE_PREFIX)
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

/// One business RPC presented to [`IncomingGuard`].
#[derive(Clone, Copy, Debug)]
pub struct IncomingDispatchRequest<'a> {
    pub envelope: &'a RpcEnvelope,
    pub caller: Option<IncomingCaller<'a>>,
    pub metadata: IncomingMetadata<'a>,
}

impl<'a> IncomingDispatchRequest<'a> {
    pub fn new(envelope: &'a RpcEnvelope, caller: Option<IncomingCaller<'a>>) -> Self {
        Self {
            envelope,
            caller,
            metadata: IncomingMetadata::new(&envelope.metadata),
        }
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

/// Current-policy authorization for business RPC dispatch.
///
/// Implementations may consult an Actrium session registry and interpret the
/// verified peer context there. Hyper deliberately supplies only generic Actor
/// identity and typed envelope metadata; it does not interpret member roles.
#[async_trait]
pub trait IncomingGuard: Send + Sync + 'static {
    async fn check(&self, request: IncomingDispatchRequest<'_>) -> Result<(), IncomingGuardError>;
}
