//! In-memory Front–Service Attach, exclusive lease, and request authorization.
//!
//! The transport remains responsible for producing [`VerifiedChannel`]. This
//! crate starts at that trust boundary and implements the application session
//! protocol. No Service workload lifecycle hooks are used.

mod config;
mod crypto;
mod error;
mod guard;
mod lifecycle;
mod policy;
mod registry;
mod service;

pub use config::{AttachmentPolicy, ServiceConfig};
pub use crypto::{
    attach_request_digest, attach_request_signing_bytes, attach_response_signing_bytes,
    request_proof, request_signing_bytes, resume_digest, resume_proof, resume_signing_bytes,
    sign_attach_request, sign_attach_response, takeover_signing_bytes, verify_attach_request,
    verify_attach_response, verify_request_proof,
};
pub use error::{ConfigError, SessionError};
pub use guard::{BusinessRequest, GuardedRequest, MessageKind, SessionGuard};
pub use lifecycle::{
    AttachmentState, CreateInstanceRequest, CreationIntent, CreationStage, CredentialState,
    DesiredState, Health, InstanceTombstone, LifecycleError, LifecycleService, LifecycleState,
    Placement, Reachability, ServiceInstance, TombstoneReason, UpgradeOutcome, WorkspaceBinding,
    actor_id_hash,
};
pub use policy::{
    AttachQuotaContext, AuthorizeAttach, AuthorizeRequest, AuthorizeTakeover, Clock,
    MembershipProvider, Principal, PrivacyContext, PrivacyDecision, PrivacyPolicy, QuotaDecision,
    RequestAuthorizationDecision, RequestContext, RequestQuotaContext, ResourceQuota,
    ServicePolicies, SystemClock, TakeoverContext,
};
pub use registry::{
    AttachOutcome, HeartbeatOutcome, InMemorySessionRegistry, RetentionStats, SessionSecret,
};
pub use service::InMemorySessionService;

pub use actrium_core::session::{
    AttachErrorCode, AttachRequest, AttachResponse, AuthorizeAttach as AttachAuthorization,
    AuthorizeAttachDecision, DetachRequest, FrontServiceSession, GetStatusRequest,
    HeartbeatRequest, Lease, LeaseMode, MemberRole, PrivacyState, ProgramRelease, ResumeRequest,
    Runtime, ServiceInfo, SessionState, SessionStatus, TakeoverRequest, VerifiedChannel,
};
