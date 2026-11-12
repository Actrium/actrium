use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use actrium_core::session::{
    AuthorizeAttach as AttachAuthorization, AuthorizeAttachDecision, FrontServiceSession,
    MemberRole, PrivacyState, VerifiedChannel,
};
use async_trait::async_trait;

use crate::SessionError;

/// Current Realm/application principal. Role capabilities are evaluated again
/// on renewal and on every Guard check; the Attach snapshot is never enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub member: String,
    pub client_key: String,
    pub role: MemberRole,
    pub capabilities: BTreeSet<String>,
}

#[async_trait]
pub trait MembershipProvider: Send + Sync {
    async fn principal(
        &self,
        front_actor_id: &str,
        member: &str,
        client_key: &str,
    ) -> Result<Option<Principal>, SessionError>;
}

/// Service workload's per-Attach application authorization hook.
#[async_trait]
pub trait AuthorizeAttach: Send + Sync {
    async fn authorize_attach(
        &self,
        context: AttachAuthorization,
    ) -> Result<AuthorizeAttachDecision, SessionError>;
}

#[derive(Debug, Clone)]
pub struct TakeoverContext {
    pub existing_session: FrontServiceSession,
    pub requesting: AttachAuthorization,
}

/// Independent authorization decision for exclusive-lease takeover.
#[async_trait]
pub trait AuthorizeTakeover: Send + Sync {
    async fn authorize_takeover(&self, context: TakeoverContext) -> Result<bool, SessionError>;
}

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub session: FrontServiceSession,
    pub principal: Principal,
    pub route: String,
    pub capability: String,
    pub request_id: String,
}

/// Current application permission, independent of role and capability checks.
#[async_trait]
pub trait AuthorizeRequest: Send + Sync {
    async fn authorize_request(
        &self,
        context: RequestContext,
    ) -> Result<RequestAuthorizationDecision, SessionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestAuthorizationDecision {
    pub allowed: bool,
    pub denial_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrivacyContext {
    pub front_actor_id: String,
    pub member: String,
    pub client_key: String,
    pub role: MemberRole,
    pub requested_capabilities: BTreeSet<String>,
    pub channel: VerifiedChannel,
    pub session_id: Option<String>,
    pub route: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrivacyDecision {
    pub state: PrivacyState,
    pub allowed_capabilities: BTreeSet<String>,
}

/// Evaluates current state/assurance both at Attach/Resume and at execution.
#[async_trait]
pub trait PrivacyPolicy: Send + Sync {
    async fn evaluate(&self, context: PrivacyContext) -> Result<PrivacyDecision, SessionError>;
}

#[derive(Debug, Clone)]
pub struct AttachQuotaContext {
    pub front_actor_id: String,
    pub active_session_count: usize,
    pub retained_session_count: usize,
    pub requested_capabilities: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct RequestQuotaContext {
    pub session_id: String,
    pub route: String,
    pub request_id: String,
    pub payload_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaDecision {
    pub allowed: bool,
    pub denial_reason: Option<String>,
}

/// Enforces Attach rate/memory/session quotas and per-request resource limits.
#[async_trait]
pub trait ResourceQuota: Send + Sync {
    async fn check_attach(
        &self,
        context: AttachQuotaContext,
    ) -> Result<QuotaDecision, SessionError>;
    async fn check_request(
        &self,
        context: RequestQuotaContext,
    ) -> Result<QuotaDecision, SessionError>;
}

/// All callbacks are mandatory; there is no implicit allow-all implementation.
#[derive(Clone)]
pub struct ServicePolicies {
    pub membership: Arc<dyn MembershipProvider>,
    pub authorize_attach: Arc<dyn AuthorizeAttach>,
    pub authorize_takeover: Arc<dyn AuthorizeTakeover>,
    pub authorize_request: Arc<dyn AuthorizeRequest>,
    pub privacy: Arc<dyn PrivacyPolicy>,
    pub quota: Arc<dyn ResourceQuota>,
}

pub trait Clock: Send + Sync {
    fn now_epoch_secs(&self) -> u64;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
