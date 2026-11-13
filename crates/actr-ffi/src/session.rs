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
    pub runtime_instance_id: String,
    /// 32-byte Ed25519 signing key for the Service Actor.
    pub service_signing_key: Vec<u8>,
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
    fn into_runtime(self) -> ActrResult<actrium_runtime::ServiceConfig> {
        let key_bytes: [u8; 32] =
            self.service_signing_key
                .try_into()
                .map_err(|_| ActrError::InvalidArgument {
                    msg: "service_signing_key must be 32 bytes".into(),
                })?;
        let routes: BTreeMap<String, BTreeSet<String>> = self
            .routes
            .into_iter()
            .map(|entry| (entry.route, entry.capabilities.into_iter().collect()))
            .collect();
        Ok(actrium_runtime::ServiceConfig {
            service_actor_id: self.service_actor_id,
            runtime_instance_id: self.runtime_instance_id,
            service_signing_key: ed25519_dalek::SigningKey::from_bytes(&key_bytes),
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
        })
    }
}

struct AllowAllPolicies;

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
            capabilities: BTreeSet::new(),
        }))
    }
}

#[async_trait::async_trait]
impl actrium_runtime::AuthorizeAttach for AllowAllPolicies {
    async fn authorize_attach(
        &self,
        context: actrium_runtime::AttachAuthorization,
    ) -> Result<actrium_runtime::AuthorizeAttachDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::AuthorizeAttachDecision {
            allowed: true,
            denial_reason: None,
            allowed_capabilities: context.requested_capabilities,
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
pub struct ActriumServicePolicies {
    inner: actrium_runtime::ServicePolicies,
}

#[uniffi::export]
impl ActriumServicePolicies {
    /// Construct an allow-all policy bundle. Suitable only when the host
    /// environment already authorizes callers and workloads.
    #[uniffi::constructor]
    pub fn allow_all() -> Arc<Self> {
        let policies = actrium_runtime::ServicePolicies {
            membership: Arc::new(AllowAllPolicies),
            authorize_attach: Arc::new(AllowAllPolicies),
            authorize_takeover: Arc::new(AllowAllPolicies),
            authorize_request: Arc::new(AllowAllPolicies),
            privacy: Arc::new(AllowAllPolicies),
            quota: Arc::new(AllowAllPolicies),
        };
        Arc::new(Self { inner: policies })
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
    /// 32-byte Ed25519 signing key for the attaching peer.
    pub actor_signing_key: Vec<u8>,
}

impl ActriumAttachParams {
    fn into_runtime(self) -> ActrResult<actr_hyper::actrium::ActriumAttachParams> {
        let key_bytes: [u8; 32] =
            self.actor_signing_key
                .try_into()
                .map_err(|_| ActrError::InvalidArgument {
                    msg: "actor_signing_key must be 32 bytes".into(),
                })?;
        Ok(actr_hyper::actrium::ActriumAttachParams {
            program: self.program.into(),
            package_digest: self.package_digest,
            guest_digest: self.guest_digest,
            protocol_fingerprint: self.protocol_fingerprint,
            runtime: self.runtime.into(),
            requested_capabilities: self.requested_capabilities,
            actor_signing_key: ed25519_dalek::SigningKey::from_bytes(&key_bytes),
        })
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

pub(crate) fn registered_node_with_actrium_service(
    node: actr_hyper::Node<actr_hyper::Registered>,
    config: ActriumServiceConfig,
    policies: Arc<ActriumServicePolicies>,
) -> ActrResult<actr_hyper::Node<actr_hyper::Registered>> {
    let config = config.into_runtime()?;
    let service = actrium_runtime::InMemorySessionService::new(config, policies.inner.clone())
        .map_err(|error| ActrError::Internal {
            msg: format!("invalid service config: {error}"),
        })?;
    Ok(node.with_actrium_service(service))
}

pub(crate) async fn actr_ref_attach_actrium(
    actr_ref: &actr_hyper::ActrRef,
    service: ActrId,
    params: ActriumAttachParams,
) -> ActrResult<Arc<ActrPeerSessionWrapper>> {
    let service: actr_protocol::ActrId = service.into();
    let params = params.into_runtime()?;
    let session = actr_ref.attach_actrium(&service, params).await?;
    Ok(Arc::new(ActrPeerSessionWrapper { inner: session }))
}
