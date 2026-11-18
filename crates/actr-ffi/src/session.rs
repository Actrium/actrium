//! FFI bridge for the Actrium v1 peer-to-peer session protocol.
//!
//! Lets Swift / Kotlin hosts participate as an Actrium peer: install a Service,
//! attach to another peer, make session-signed calls/tells, and detach cleanly
//! on lifecycle events (new tab, close, background, crash, restart).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{ActrError, ActrResult};
use crate::lifecycle::ProgramRelease;
use crate::types::ActrId;

/// Service runtime compatibility tuple.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActriumRuntime {
    pub actrium_version: String,
    pub front_host_api: String,
    pub keel_api: String,
}

impl From<ActriumRuntime> for actrium_core::session::Runtime {
    fn from(value: ActriumRuntime) -> Self {
        Self {
            actrium_version: value.actrium_version,
            front_host_api: value.front_host_api,
            keel_api: value.keel_api,
        }
    }
}

/// Service attachment policy. v1 only supports `Exclusive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AttachmentPolicy {
    Exclusive,
}

impl From<AttachmentPolicy> for actrium_runtime::AttachmentPolicy {
    fn from(_value: AttachmentPolicy) -> Self {
        Self::Exclusive
    }
}

/// Route admission entry for a Service config.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ActriumRoute {
    pub route: String,
    pub capabilities: Vec<String>,
}

/// Configuration for an Actrium Service host.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ActriumServiceConfig {
    pub service_actor_id: String,
    pub allowed_front_actor_types: Vec<String>,
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
    pub supported_runtimes: Vec<ActriumRuntime>,
    pub program_capabilities: Vec<String>,
    /// Routes and their allowed capabilities.
    pub routes: Vec<ActriumRoute>,
    pub attachment_policy: AttachmentPolicy,
    pub lease_duration_secs: u64,
    pub heartbeat_interval_secs: u64,
    pub grace_period_secs: u64,
}

impl ActriumServiceConfig {
    fn into_runtime(
        self,
        service_signing_key: ed25519_dalek::SigningKey,
    ) -> actrium_runtime::ServiceConfig {
        let routes: BTreeMap<String, BTreeSet<String>> = self
            .routes
            .into_iter()
            .map(|entry| (entry.route, entry.capabilities.into_iter().collect()))
            .collect();
        actrium_runtime::ServiceConfig {
            service_actor_id: self.service_actor_id,
            runtime_instance_id: actr_hyper::process_runtime_instance_id().to_owned(),
            service_signing_key,
            allowed_front_actor_types: self.allowed_front_actor_types.into_iter().collect(),
            program: self.program.into(),
            package_digest: self.package_digest,
            guest_digest: self.guest_digest,
            protocol_fingerprint: self.protocol_fingerprint,
            supported_runtimes: self
                .supported_runtimes
                .into_iter()
                .map(Into::into)
                .collect(),
            program_capabilities: self.program_capabilities.into_iter().collect(),
            routes,
            attachment_policy: self.attachment_policy.into(),
            lease_duration: Duration::from_secs(self.lease_duration_secs),
            heartbeat_interval: Duration::from_secs(self.heartbeat_interval_secs),
            grace_period: Duration::from_secs(self.grace_period_secs),
            replay_retention: actrium_runtime::ServiceConfig::DEFAULT_REPLAY_RETENTION,
            max_replay_entries: actrium_runtime::ServiceConfig::DEFAULT_MAX_REPLAY_ENTRIES,
            max_completed_sessions: actrium_runtime::ServiceConfig::DEFAULT_MAX_COMPLETED_SESSIONS,
        }
    }
}

struct AllowAllPolicies {
    capabilities: BTreeSet<String>,
}

#[async_trait::async_trait]
impl actrium_runtime::MembershipProvider for AllowAllPolicies {
    async fn principal(
        &self,
        _front_actor_id: &str,
        member: &str,
        client_key: &str,
    ) -> Result<Option<actrium_runtime::Principal>, actrium_runtime::SessionError> {
        Ok(Some(actrium_runtime::Principal {
            member: member.to_string(),
            client_key: client_key.to_string(),
            role: actrium_runtime::MemberRole::Owner,
            capabilities: self.capabilities.clone(),
        }))
    }
}

#[async_trait::async_trait]
impl actrium_runtime::AuthorizeAttach for AllowAllPolicies {
    async fn authorize_attach(
        &self,
        context: actrium_runtime::AttachAuthorization,
    ) -> Result<actrium_runtime::AuthorizeAttachDecision, actrium_runtime::SessionError> {
        let allowed_capabilities = if context.requested_capabilities.is_empty() {
            self.capabilities.iter().cloned().collect()
        } else {
            context.requested_capabilities
        };
        Ok(actrium_runtime::AuthorizeAttachDecision {
            allowed: true,
            denial_reason: None,
            allowed_capabilities,
        })
    }
}

#[async_trait::async_trait]
impl actrium_runtime::AuthorizeTakeover for AllowAllPolicies {
    async fn authorize_takeover(
        &self,
        _context: actrium_runtime::TakeoverContext,
    ) -> Result<bool, actrium_runtime::SessionError> {
        Ok(true)
    }
}

#[async_trait::async_trait]
impl actrium_runtime::AuthorizeRequest for AllowAllPolicies {
    async fn authorize_request(
        &self,
        _context: actrium_runtime::RequestContext,
    ) -> Result<actrium_runtime::RequestAuthorizationDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::RequestAuthorizationDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

#[async_trait::async_trait]
impl actrium_runtime::PrivacyPolicy for AllowAllPolicies {
    async fn evaluate(
        &self,
        context: actrium_runtime::PrivacyContext,
    ) -> Result<actrium_runtime::PrivacyDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::PrivacyDecision {
            state: actrium_runtime::PrivacyState {
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
impl actrium_runtime::ResourceQuota for AllowAllPolicies {
    async fn check_attach(
        &self,
        _context: actrium_runtime::AttachQuotaContext,
    ) -> Result<actrium_runtime::QuotaDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }

    async fn check_request(
        &self,
        _context: actrium_runtime::RequestQuotaContext,
    ) -> Result<actrium_runtime::QuotaDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::QuotaDecision {
            allowed: true,
            denial_reason: None,
        })
    }
}

/// FFI wrapper for service session policies.
///
/// v1 exposes a simple allow-all bundle for hosts that delegate policy to
/// lifecycle / workspace layers. Future versions can add callback interfaces
/// for each policy dimension.
#[derive(uniffi::Object)]
pub struct ActriumServicePolicies;

#[uniffi::export]
impl ActriumServicePolicies {
    /// Construct an allow-all policy bundle. Suitable only when the host
    /// environment already authorizes callers and workloads. The effective
    /// capability set is bounded by each installed Service's configuration.
    #[uniffi::constructor]
    pub fn allow_all() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl ActriumServicePolicies {
    fn for_service(
        &self,
        config: &actrium_runtime::ServiceConfig,
    ) -> actrium_runtime::ServicePolicies {
        let mut capabilities = config.program_capabilities.clone();
        capabilities.extend(
            config
                .routes
                .values()
                .flat_map(|route| route.iter().cloned()),
        );
        let policies = Arc::new(AllowAllPolicies { capabilities });
        actrium_runtime::ServicePolicies {
            membership: policies.clone(),
            authorize_attach: policies.clone(),
            authorize_takeover: policies.clone(),
            authorize_request: policies.clone(),
            privacy: policies.clone(),
            quota: policies,
        }
    }
}

/// Parameters for a peer to Attach to another peer's Service.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ActriumAttachParams {
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
    pub runtime: ActriumRuntime,
    pub requested_capabilities: Vec<String>,
}

impl ActriumAttachParams {
    fn into_runtime(self) -> actr_hyper::actrium::ActriumAttachParams {
        actr_hyper::actrium::ActriumAttachParams {
            program: self.program.into(),
            package_digest: self.package_digest,
            guest_digest: self.guest_digest,
            protocol_fingerprint: self.protocol_fingerprint,
            runtime: self.runtime.into(),
            requested_capabilities: self.requested_capabilities,
        }
    }
}

/// FFI wrapper for an Actrium peer session.
#[derive(uniffi::Object)]
pub struct ActrPeerSessionWrapper {
    inner: Arc<actr_hyper::actrium::ActriumPeerSession>,
}

#[uniffi::export(async_runtime = "tokio")]
impl ActrPeerSessionWrapper {
    /// Make a session-signed request and verify the Service's response proof.
    pub async fn call(
        &self,
        route: String,
        capability: String,
        payload: Vec<u8>,
        timeout_ms: u64,
    ) -> ActrResult<Vec<u8>> {
        self.inner
            .call(
                &route,
                &capability,
                payload,
                Duration::from_millis(timeout_ms),
            )
            .await
            .map_err(Into::into)
    }

    /// Fire-and-forget session-signed tell.
    pub async fn tell(
        &self,
        route: String,
        capability: String,
        payload: Vec<u8>,
    ) -> ActrResult<()> {
        self.inner
            .tell(&route, &capability, payload)
            .await
            .map_err(Into::into)
    }

    /// Detach the session. Subsequent calls fail.
    pub async fn detach(&self) -> ActrResult<()> {
        self.inner.detach().await.map_err(Into::into)
    }
}

// The Node/ActrRef methods are re-exported on the existing types in runtime.rs.
// They are kept here as free-function helpers so the implementation detail does
// not leak into the larger runtime module.

pub(crate) fn actrium_service_for_node(
    node: &actr_hyper::Node<actr_hyper::Registered>,
    config: ActriumServiceConfig,
    policies: Arc<ActriumServicePolicies>,
) -> ActrResult<actrium_runtime::InMemorySessionService> {
    let service_signing_key =
        node.actrium_actor_signing_key()
            .map_err(|error| ActrError::Internal {
                msg: format!("failed to load the node Actor key: {error}"),
            })?;
    let config = config.into_runtime(service_signing_key);
    let policies = policies.for_service(&config);
    actrium_runtime::InMemorySessionService::new(config, policies).map_err(|error| {
        ActrError::Internal {
            msg: format!("invalid service config: {error}"),
        }
    })
}

pub(crate) async fn actr_ref_attach_actrium(
    actr_ref: &actr_hyper::ActrRef,
    service: ActrId,
    params: ActriumAttachParams,
) -> ActrResult<Arc<ActrPeerSessionWrapper>> {
    let service: actr_protocol::ActrId = service.into();
    let params = params.into_runtime();
    let session = actr_ref.attach_actrium(&service, params).await?;
    Ok(Arc::new(ActrPeerSessionWrapper { inner: session }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn service_config_for_test() -> ActriumServiceConfig {
        ActriumServiceConfig {
            service_actor_id: "service@example.com/1".into(),
            allowed_front_actor_types: vec!["example.com/Front@1.0.0".into()],
            program: ProgramRelease {
                publisher: "example.com".into(),
                name: "documents".into(),
                version: "1.0.0".into(),
            },
            package_digest: vec![4; 32],
            guest_digest: vec![5; 32],
            protocol_fingerprint: vec![6; 32],
            supported_runtimes: vec![ActriumRuntime {
                actrium_version: "1".into(),
                front_host_api: "1".into(),
                keel_api: "1".into(),
            }],
            program_capabilities: vec!["read".into(), "write".into()],
            routes: vec![ActriumRoute {
                route: "echo".into(),
                capabilities: vec!["read".into(), "write".into()],
            }],
            attachment_policy: AttachmentPolicy::Exclusive,
            lease_duration_secs: 300,
            heartbeat_interval_secs: 30,
            grace_period_secs: 60,
        }
    }

    #[test]
    fn service_config_uses_the_process_identity_and_node_key() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        let expected_public_key = key.verifying_key();
        let config = service_config_for_test().into_runtime(key);

        assert_eq!(
            config.runtime_instance_id,
            actr_hyper::process_runtime_instance_id()
        );
        assert_eq!(
            config.service_signing_key.verifying_key(),
            expected_public_key
        );
    }

    #[tokio::test]
    async fn allow_all_uses_service_capabilities_for_attach_and_membership() {
        let config =
            service_config_for_test().into_runtime(ed25519_dalek::SigningKey::from_bytes(&[7; 32]));
        let expected = BTreeSet::from(["read".to_string(), "write".to_string()]);
        let policies = ActriumServicePolicies::allow_all().for_service(&config);

        let principal = policies
            .membership
            .principal("front", "member", "client")
            .await
            .expect("membership lookup succeeds")
            .expect("allow-all returns a principal");
        assert_eq!(principal.capabilities, expected);

        // An empty request means the full configured ceiling, not an empty
        // application grant that would make every business request fail.
        let decision = policies
            .authorize_attach
            .authorize_attach(actrium_runtime::AttachAuthorization {
                front_actor_id: "front".into(),
                member: "member".into(),
                client_key: "client".into(),
                role: actrium_runtime::MemberRole::Owner,
                program: config.program.clone(),
                requested_capabilities: Vec::new(),
            })
            .await
            .expect("allow-all authorizes Attach");
        assert_eq!(
            decision.allowed_capabilities,
            expected.into_iter().collect::<Vec<_>>()
        );

        let decision = policies
            .authorize_attach
            .authorize_attach(actrium_runtime::AttachAuthorization {
                front_actor_id: "front".into(),
                member: "member".into(),
                client_key: "client".into(),
                role: actrium_runtime::MemberRole::Owner,
                program: config.program.clone(),
                requested_capabilities: vec!["read".into()],
            })
            .await
            .expect("allow-all authorizes an explicit capability");
        assert_eq!(decision.allowed_capabilities, vec!["read".to_string()]);
    }
}
