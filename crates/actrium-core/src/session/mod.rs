//! Front–Service session domain types and protocol contract.
//!
//! Corresponds to `contracts/actrium/session/v1.proto`.
//!
//! # Relationship to Tab and Service lifecycles
//!
//! From `docs/product/actrium-v1/08-workspace-lifecycle-and-storage.md` §5
//! ("Tab and Service lifecycle separation"), the invariant is: **Tab events operate on
//! Front sessions; they do not change the Service lifecycle state.**
//!
//! | Tab event         | Front operation   | Service lifecycle |
//! |-------------------|-------------------|-------------------|
//! | New Tab (create)  | Create + Attach   | (Create if needed)|
//! | New Tab (open)    | Attach            | (unchanged)       |
//! | Refresh           | Recreate + Attach | (unchanged)       |
//! | Tab background    | Pause / Suspend   | (unchanged)       |
//! | Tab close         | Detach + Destroy  | (unchanged)       |
//! | Front crash       | Destroy/recreate  | (unchanged)       |
//! | Actrium restart   | Recreate + Attach | Recover same ActorId |
//! | Service stop      | Detach            | Stopped           |
//! | Service trash     | Detach            | Trashed           |
//! | Service purge     | Detach            | Deleted           |
//!
//! Therefore:
//! - `Attach` is NOT a `Workload::on_start` / `on_stop` hook.
//! - `Attach` may happen many times over one Service lifetime.
//! - Service restart invalidates old sessions; Fronts re-Attach.
//! - The attachment lease is independent of the Workspace writer lease.
//!
//! # Composition (see proposal §2–§9)
//!
//! ```text
//! VerifiedChannel  →  Attach  →  FrontServiceSession  →  Business RPC
//!    (Actr Host)     (actrium)   (actrium)                (Service workload)
//! ```
//!
//! `VerifiedChannel` provides authenticated peer identity and channel binding.
//! `FrontServiceSession` adds application authorization, contract compatibility,
//! exclusive lease, and session capability ceiling.

use serde::{Deserialize, Serialize};

pub mod channel;
pub mod integration;

// Re-export channel types for convenience.
pub use channel::{
    ActorChannelCertificate, ChannelState, HandshakeStep1, HandshakeStep2, HandshakeStep3,
    HandshakeStep4, HandshakeTranscript, NegotiationParams, PeerBinding, TransportKind,
    VerifiedChannel,
};

// Re-export integration traits for convenience.
pub use integration::{FrontSessionClient, ServiceSessionHandler, TabLifecycleIntegration};

// ---------------------------------------------------------------------------
// Session state machine
// ---------------------------------------------------------------------------

/// Lease lifecycle state for a Service's front attachment.
///
/// Transitions:
/// ```text
/// Idle → Active     :  Attach passes, lease granted atomically
/// Active → Grace    :  Transient disconnect
/// Grace → Active    :  Resume while lease remains valid
/// Grace → Idle      :  Lease or credential expires
/// Active → Idle     :  Detach, credential expiry, or Service restart
/// Active → Takeover :  Another Front requests takeover
/// Takeover → Active :  Denied – original session continues
/// Takeover → Idle   :  Approved – old lease, epoch, and channel invalidated
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// No active Front attachment. Service may still be Running.
    Idle,
    /// Exclusive lease is held. Only state that accepts business RPC.
    Active,
    /// Transient disconnect; lease still valid. Heartbeat may restore.
    Grace,
    /// Policy, assurance, or revocation gate. No business RPC.
    Suspended,
    /// Session ended (Detach, expiry, or restart). Terminal.
    Closed,
    /// Takeover in progress; old session being invalidated.
    Takeover,
}

// ---------------------------------------------------------------------------
// Lease
// ---------------------------------------------------------------------------

/// Exclusive lease granted as part of a successful Attach.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub mode: LeaseMode,
    /// Unix epoch seconds.
    pub expires_at_epoch_secs: u64,
    pub heartbeat_interval_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaseMode {
    Exclusive,
}

// ---------------------------------------------------------------------------
// Program identity
// ---------------------------------------------------------------------------

/// Logical program version. Does not encode a specific package digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramRelease {
    pub publisher: String,
    pub name: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// FrontServiceSession (authoritative record, Service Host only)
// ---------------------------------------------------------------------------

/// Immutable session record created on a successful Attach.
///
/// The `session_secret` is kept in Host memory / secure IPC only. It is never
/// exposed to Guest, JavaScript, logs, URIs, or crash reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontServiceSession {
    pub session_id: String,
    /// Monotonic; advances on Takeover and Resume. Stale epochs are rejected.
    pub lease_epoch: u64,

    // --- Identity bindings ---
    pub front_actor_id: String,
    pub service_actor_id: String,
    pub channel_id: String,
    pub channel_transcript_digest: Vec<u8>,

    // --- Authorization snapshot (from credential, verified by Host) ---
    pub member: String,
    pub client_key: String,
    pub role: MemberRole,

    // --- Compatibility ---
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,

    // --- Capability ---
    /// Service-authorized ceiling. The local Front Host computes the final
    /// effective capabilities as the intersection with user/org/platform grants.
    pub session_capability_ceiling: Vec<String>,

    // --- Lease ---
    pub lease: Lease,
    pub state: SessionState,

    // --- Hard expiries ---
    pub credential_expires_at_epoch_secs: u64,
    pub lease_expires_at_epoch_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberRole {
    Owner,
    Editor,
    Viewer,
}

// ---------------------------------------------------------------------------
// Attach messages
// ---------------------------------------------------------------------------

/// AttachRequest as constructed by the Front Host.
///
/// Program, digest, runtime and fingerprint fields are compatibility
/// declarations, not proof of remote code. The Guest MUST NOT supply or
/// override any authority-bearing field; those come from verified local state
/// and the authenticated channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachRequest {
    pub attach_id: String,
    pub front_actor_id: String,
    pub service_actor_id: String,
    pub channel_id: String,
    pub channel_transcript_digest: Vec<u8>,

    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub runtime: Runtime,

    pub protocol_fingerprint: Vec<u8>,
    pub tab_session_nonce: Vec<u8>,

    pub requested_lease: LeaseMode,
    /// Optional least-privilege request; not proof of authorization.
    pub requested_capabilities: Vec<String>,

    /// Actor signature over canonical request + channel transcript.
    pub challenge_response: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub actrium_version: String,
    pub front_host_api: String,
    pub keel_api: String,
}

/// AttachResponse returned by the Service Host after all checks pass.
///
/// `session_capability_ceiling` is the Service-authorized upper bound; the
/// local Front Host computes the final effective capabilities as:
/// `ceiling ∩ program-declared ∩ host-supported ∩ user/org grant ∩ platform
/// policy ∩ session scope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachResponse {
    pub session_id: String,
    pub lease_epoch: u64,
    pub lease: Lease,
    pub service: ServiceInfo,
    pub session_capability_ceiling: Vec<String>,
    pub privacy: PrivacyState,
    pub service_challenge: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub actor_id: String,
    pub program: String,
    pub package_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyState {
    pub actor_state_digest: Vec<u8>,
    pub application_state_digest: Vec<u8>,
    pub assurance_digest: Vec<u8>,
    pub expires_at_epoch_secs: u64,
}

// ---------------------------------------------------------------------------
// System protocol messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub session_id: String,
    pub lease_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeRequest {
    pub session_id: String,
    pub lease_epoch: u64,
    pub channel_id: String,
    pub channel_transcript_digest: Vec<u8>,
    pub session_proof: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachRequest {
    pub session_id: String,
    pub lease_epoch: u64,
}

/// Takeover requires independent permission and conflict-policy approval.
/// On success the old session, epoch and channel are atomically invalidated
/// before the new attachment becomes active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeoverRequest {
    pub session_id: String,
    pub lease_epoch: u64,
    pub requesting_front_actor_id: String,
    pub takeover_proof: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Service workload authorization hook
// ---------------------------------------------------------------------------

/// Application-level authorization context for a single Attach attempt.
///
/// All identity and credential claims have been verified by the Service Host
/// before the Service workload sees this struct. The Service only decides
/// whether the application authorizes the session under these parameters.
///
/// This is a **per-Attach** request-scoped hook, not a lifecycle method.
/// It can fire many times during one Service lifetime (one per Attach).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizeAttach {
    pub front_actor_id: String,
    pub member: String,
    pub client_key: String,
    pub role: MemberRole,
    pub program: ProgramRelease,
    pub requested_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizeAttachDecision {
    pub allowed: bool,
    pub denial_reason: Option<String>,
    /// Application-level capability scope for this session.
    pub allowed_capabilities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Stable error codes for Attach and session protocol failures.
///
/// User-facing errors MUST preserve these distinctions; they MUST NOT be
/// collapsed into a generic connection failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachErrorCode {
    ChannelIdentityMismatch,
    ChannelTargetMismatch,
    ChannelTranscriptMismatch,
    ChannelExpired,
    ServiceActorTypeMismatch,
    ProgramReleaseMismatch,
    PackageDigestMismatch,
    ProtocolMismatch,
    PermissionDenied,
    SessionOccupied,
    PrivacyPolicyUnsatisfied,
    RateLimited,
    SessionExpired,
    SessionEpochMismatch,
    TakeoverDenied,
    ServiceOffline,
}

// ---------------------------------------------------------------------------
// Traits (to be implemented in the future Actrium runtime crate)
// ---------------------------------------------------------------------------
//
// The SessionRegistry and VerifiedChannelContext traits sketched in the
// proposal belong in the platform-trusted Service Host. They are not defined
// here to keep `actrium-core` free of async runtime and transport dependencies.
//
// ```text
// trait SessionRegistry {
//     async fn attach(...)   -> Result<AttachResponse, AttachErrorCode>;
//     async fn heartbeat(...);
//     async fn resume(...);
//     async fn detach(...);
//     async fn takeover(...) -> Result<AttachResponse, AttachErrorCode>;
//     async fn status(...)   -> Result<SessionStatus, AttachErrorCode>;
//     async fn invalidate_all();
// }
//
// trait VerifiedChannelContext {
//     fn peer_actor_id(&self)            -> &str;
//     fn peer_actor_type(&self)          -> &str;
//     fn peer_member(&self)              -> &str;
//     fn peer_client_key(&self)          -> &str;
//     fn peer_role(&self)                -> MemberRole;
//     fn peer_credential_expires_at(&self) -> u64;
//     fn channel_id(&self)               -> &str;
//     fn transcript_digest(&self)        -> &[u8];
// }
// ```
//
// Concrete implementations belong in a future `crates/actrium-runtime` crate
// that depends on `actr-hyper` for transport integration and `actrium-core` for
// domain types. See the proposal §9 for the full architecture map.

/// Snapshot of a single session for status queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub state: SessionState,
    pub lease_epoch: u64,
    pub expires_at_epoch_secs: u64,
    pub occupant: Option<String>,
}
