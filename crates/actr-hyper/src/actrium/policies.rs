//! Allow-everything session policies for hosts that delegate authorization to
//! their surrounding environment.
//!
//! The session protocol itself (channel verification, Attach validation,
//! lease, proofs) is still fully enforced. Only the application-level policy
//! dimensions say "yes": membership, Attach, takeover, request, privacy and
//! quota. The capability ceiling is the Service's configured program
//! capabilities plus every capability named by its routes, so a Host cannot
//! accidentally hand out more authority than its `ServiceConfig` declares.

use std::collections::BTreeSet;
use std::sync::Arc;

use actrium_runtime::{
    AttachAuthorization, AttachQuotaContext, AuthorizeAttach, AuthorizeAttachDecision,
    AuthorizeRequest, AuthorizeTakeover, MemberRole, MembershipProvider, Principal, PrivacyContext,
    PrivacyDecision, PrivacyPolicy, PrivacyState, QuotaDecision, RequestAuthorizationDecision,
    RequestContext, RequestQuotaContext, ResourceQuota, ServiceConfig, ServicePolicies,
    SessionError, TakeoverContext,
};

struct AllowAll {
    capabilities: BTreeSet<String>,
}

#[async_trait::async_trait]
impl MembershipProvider for AllowAll {
    async fn principal(
        &self,
        _front_actor_id: &str,
        member: &str,
        client_key: &str,
    ) -> Result<Option<Principal>, SessionError> {
        Ok(Some(Principal {
            member: member.to_string(),
            client_key: client_key.to_string(),
            role: MemberRole::Owner,
            capabilities: self.capabilities.clone(),
        }))
    }
}

#[async_trait::async_trait]
impl AuthorizeAttach for AllowAll {
    async fn authorize_attach(
        &self,
        context: AttachAuthorization,
    ) -> Result<AuthorizeAttachDecision, SessionError> {
        let allowed_capabilities = if context.requested_capabilities.is_empty() {
            self.capabilities.iter().cloned().collect()
        } else {
            context.requested_capabilities
        };
        Ok(AuthorizeAttachDecision {
            allowed: true,
            denial_reason: None,
            allowed_capabilities,
        })
    }
}

#[async_trait::async_trait]
impl AuthorizeTakeover for AllowAll {
    async fn authorize_takeover(&self, _context: TakeoverContext) -> Result<bool, SessionError> {
        Ok(true)
    }
}

#[async_trait::async_trait]
impl AuthorizeRequest for AllowAll {
    async fn authorize_request(
        &self,
        _context: RequestContext,
    ) -> Result<RequestAuthorizationDecision, SessionError> {
        Ok(RequestAuthorizationDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

#[async_trait::async_trait]
impl PrivacyPolicy for AllowAll {
    async fn evaluate(&self, context: PrivacyContext) -> Result<PrivacyDecision, SessionError> {
        Ok(PrivacyDecision {
            state: PrivacyState {
                actor_state_digest: vec![0; 32],
                application_state_digest: vec![0; 32],
                assurance_digest: vec![0; 32],
                expires_at_epoch_secs: u64::MAX,
            },
            allowed_capabilities: context.requested_capabilities,
        })
    }
}

#[async_trait::async_trait]
impl ResourceQuota for AllowAll {
    async fn check_attach(
        &self,
        _context: AttachQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        Ok(QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }

    async fn check_request(
        &self,
        _context: RequestQuotaContext,
    ) -> Result<QuotaDecision, SessionError> {
        Ok(QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

/// Build a policy bundle that admits every caller, bounded by the
/// capabilities `config` declares.
///
/// Use this only when the surrounding environment (workspace membership,
/// lifecycle gate, operator configuration) already authorizes callers. Both
/// the FFI host bridge and `actr run` use it while v1 has no configurable
/// policy backend.
pub fn allow_all_service_policies(config: &ServiceConfig) -> ServicePolicies {
    let mut capabilities = config.program_capabilities.clone();
    capabilities.extend(
        config
            .routes
            .values()
            .flat_map(|route| route.iter().cloned()),
    );
    let policies = Arc::new(AllowAll { capabilities });
    ServicePolicies {
        membership: policies.clone(),
        authorize_attach: policies.clone(),
        authorize_takeover: policies.clone(),
        authorize_request: policies.clone(),
        privacy: policies.clone(),
        quota: policies,
    }
}
