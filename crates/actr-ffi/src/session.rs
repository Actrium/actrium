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
use crate::types::{ActrId, ActrType};

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
    /// Leave empty to use the registered Node's own Actor identity.
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

/// Compatibility metadata derived from a verified Front archive and the
/// Realm's selected Service release.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ActriumSessionMetadata {
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
    pub runtime: ActriumRuntime,
    pub program_capabilities: Vec<String>,
    pub routes: Vec<ActriumRoute>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontCapabilityManifest {
    keel_api: String,
    host_modules: BTreeMap<String, FrontHostModule>,
}

#[derive(serde::Deserialize)]
struct FrontHostModule {
    rpc: Option<Vec<String>>,
}

fn invalid_session_metadata(message: impl Into<String>) -> ActrError {
    ActrError::InvalidArgument {
        msg: format!("invalid Actrium Front session metadata: {}", message.into()),
    }
}

/// Verify a Front archive and derive the exact compatibility declarations used
/// by both the local peer runtime and an outbound Attach. The Realm-selected
/// Service digest is a lowercase/uppercase hex SHA-256, while FFI records carry
/// the raw 32-byte representation.
#[uniffi::export]
pub fn actrium_session_metadata_for_front(
    front_archive: Vec<u8>,
    trusted_front_key: Vec<u8>,
    service_type: ActrType,
    service_package_digest_hex: String,
) -> ActrResult<ActriumSessionMetadata> {
    use sha2::Digest as _;

    const ACTRIUM_VERSION_V1: &str = "1.0.0";
    const FRONT_HOST_API_V1: &str = "1.0";
    const SERVICE_CAPABILITY: &str = "service";

    let key_bytes: [u8; 32] = trusted_front_key
        .as_slice()
        .try_into()
        .map_err(|_| invalid_session_metadata("the Front Ed25519 key must contain 32 bytes"))?;
    let trusted_front_key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
        .map_err(|error| invalid_session_metadata(error.to_string()))?;
    let inspected = actr_pack::verify_actor_package(&front_archive, &trusted_front_key)
        .map_err(|error| invalid_session_metadata(error.to_string()))?;
    inspected
        .manifest
        .validate_workload_payload(&front_archive)
        .map_err(|error| invalid_session_metadata(error.to_string()))?;

    let package_digest = hex::decode(service_package_digest_hex)
        .map_err(|error| invalid_session_metadata(format!("Service digest: {error}")))?;
    if package_digest.len() != 32 {
        return Err(invalid_session_metadata(
            "Service digest must contain 32 bytes",
        ));
    }

    let service_version = semver::Version::parse(&service_type.version)
        .map_err(|error| invalid_session_metadata(format!("Service version: {error}")))?;
    let dependency = inspected
        .manifest
        .dependencies
        .get("service")
        .ok_or_else(|| invalid_session_metadata("[dependencies.service] is required"))?;
    let expected_actor = format!("{}/{}", service_type.publisher, service_type.name);
    if !dependency.required || dependency.actor != expected_actor {
        return Err(invalid_session_metadata(format!(
            "the selected Service {expected_actor} does not match the Front dependency"
        )));
    }
    let dependency_requirement = semver::VersionReq::parse(&dependency.version)
        .map_err(|error| invalid_session_metadata(format!("Service dependency: {error}")))?;
    if !dependency_requirement.matches(&service_version) {
        return Err(invalid_session_metadata(format!(
            "Service version {service_version} does not satisfy {}",
            dependency.version
        )));
    }

    // The manifest carries a version requirement, whereas Attach carries the
    // concrete Host implementation version selected by this v1 native Host.
    let host_requirement = semver::VersionReq::parse(&inspected.manifest.host.api)
        .map_err(|error| invalid_session_metadata(format!("Host API requirement: {error}")))?;
    if !host_requirement.matches(&semver::Version::parse("1.0.0").expect("valid Host version")) {
        return Err(invalid_session_metadata(format!(
            "Host API {FRONT_HOST_API_V1} does not satisfy {}",
            inspected.manifest.host.api
        )));
    }

    let guest = actr_pack::read_actor_workload(&front_archive, &inspected)
        .map_err(|error| invalid_session_metadata(error.to_string()))?;
    let guest_digest = sha2::Sha256::digest(&guest).to_vec();

    let capabilities_path = inspected
        .manifest
        .workload
        .keel
        .as_ref()
        .and_then(|keel| keel.capabilities.as_deref())
        .ok_or_else(|| invalid_session_metadata("the Keel capability manifest is required"))?;
    let capability_bytes = actr_pack::read_actor_package_file(
        &front_archive,
        &inspected,
        capabilities_path,
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )
    .map_err(|error| invalid_session_metadata(error.to_string()))?;
    let capabilities: FrontCapabilityManifest = serde_json::from_slice(&capability_bytes)
        .map_err(|error| invalid_session_metadata(format!("capability manifest: {error}")))?;
    let rpc_routes = capabilities
        .host_modules
        .get(SERVICE_CAPABILITY)
        .or_else(|| capabilities.host_modules.get("host:service"))
        .and_then(|module| module.rpc.as_ref())
        .ok_or_else(|| invalid_session_metadata("hostModules.service.rpc is required"))?;
    let routes: BTreeSet<String> = rpc_routes.iter().cloned().collect();
    if routes.is_empty() || routes.iter().any(|route| route.is_empty()) {
        return Err(invalid_session_metadata(
            "hostModules.service.rpc must contain nonempty routes",
        ));
    }
    let routes = routes
        .into_iter()
        .map(|route| ActriumRoute {
            route,
            capabilities: vec![SERVICE_CAPABILITY.to_owned()],
        })
        .collect();

    let contract_path = dependency
        .contract
        .as_deref()
        .ok_or_else(|| invalid_session_metadata("the Service Proto contract is required"))?;
    let contract_bytes = actr_pack::read_actor_package_file(
        &front_archive,
        &inspected,
        contract_path,
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )
    .map_err(|error| invalid_session_metadata(error.to_string()))?;
    let contract = String::from_utf8(contract_bytes).map_err(|error| {
        invalid_session_metadata(format!("Service Proto is not UTF-8: {error}"))
    })?;
    let contract_name = contract_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid_session_metadata("the Service Proto basename is empty"))?;
    let fingerprint = actr_service_compat::Fingerprint::calculate_service_semantic_fingerprint(&[
        actr_service_compat::ProtoFile {
            name: contract_name.to_owned(),
            content: contract,
            path: None,
        },
    ])
    .map_err(|error| invalid_session_metadata(error.to_string()))?;
    let fingerprint_hex = fingerprint
        .strip_prefix("service_semantic:")
        .ok_or_else(|| invalid_session_metadata("unexpected Service fingerprint encoding"))?;
    let protocol_fingerprint = hex::decode(fingerprint_hex)
        .map_err(|error| invalid_session_metadata(format!("Service fingerprint: {error}")))?;

    Ok(ActriumSessionMetadata {
        program: ProgramRelease {
            publisher: service_type.publisher,
            name: service_type.name,
            version: service_type.version,
        },
        package_digest,
        guest_digest,
        protocol_fingerprint,
        runtime: ActriumRuntime {
            actrium_version: ACTRIUM_VERSION_V1.to_owned(),
            front_host_api: FRONT_HOST_API_V1.to_owned(),
            keel_api: capabilities.keel_api,
        },
        program_capabilities: vec![SERVICE_CAPABILITY.to_owned()],
        routes,
    })
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
    mut config: ActriumServiceConfig,
    policies: Arc<ActriumServicePolicies>,
) -> ActrResult<actrium_runtime::InMemorySessionService> {
    if config.service_actor_id.is_empty() {
        config.service_actor_id = node.actor_id().to_string_repr();
    }
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

    fn front_package_for_test(host_api: &str) -> (Vec<u8>, ed25519_dalek::SigningKey) {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
        let manifest = actr_pack::ActorPackageManifest {
            format: actr_pack::ACTOR_PACKAGE_FORMAT_V2.to_owned(),
            actor: actr_pack::ActorDescriptor {
                publisher: "example.com".into(),
                name: "documents-front".into(),
                version: "1.0.0".into(),
            },
            workload: actr_pack::WorkloadDescriptor {
                profile: actr_pack::KEEL_GUEST_PROFILE_V1.to_owned(),
                entry: "workload/guest.js".into(),
                rpc_contract: None,
                keel: Some(actr_pack::KeelGuestDescriptor {
                    runtime: "js".into(),
                    guest_api: actr_pack::KEEL_GUEST_PROFILE_V1.to_owned(),
                    capabilities: Some("workload/capabilities.json".into()),
                    contract: None,
                }),
            },
            host: actr_pack::HostRequirements {
                profile: "actr.host/1".into(),
                api: host_api.into(),
                required_capabilities: Vec::new(),
                optional_capabilities: Vec::new(),
            },
            dependencies: BTreeMap::from([(
                "service".into(),
                actr_pack::ActorDependency {
                    actor: "example.com/documents".into(),
                    version: "=1.0.0".into(),
                    contract: Some("contracts/service.proto".into()),
                    required: true,
                },
            )]),
            extensions: actr_pack::PackageExtensions { actrium: None },
            signature: actr_pack::SignatureDescriptor {
                algorithm: "ed25519".into(),
                key_id: "filled-by-pack".into(),
            },
            files: [
                ("workload/guest.js", "workload", "text/javascript"),
                ("workload/capabilities.json", "profile", "application/json"),
                ("contracts/service.proto", "contract", "text/plain"),
            ]
            .into_iter()
            .map(|(path, role, media_type)| actr_pack::PackageFileEntry {
                path: path.into(),
                role: role.into(),
                media_type: media_type.into(),
                size: 0,
                sha256: "0".repeat(64),
            })
            .collect(),
        };
        let payloads = BTreeMap::from([
            (
                "workload/guest.js".into(),
                b"globalThis.documents = true;".to_vec(),
            ),
            (
                "workload/capabilities.json".into(),
                br#"{"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js","imports":["host:service"],"hostModules":{"service":{"rpc":["documents/write","documents/read"]}}}"#.to_vec(),
            ),
            (
                "contracts/service.proto".into(),
                br#"syntax = "proto3";
package documents.v1;
message ReadRequest {}
message ReadResponse {}
service Documents { rpc Read(ReadRequest) returns (ReadResponse); }
"#
                .to_vec(),
            ),
        ]);
        let package = actr_pack::pack_actor_package(&actr_pack::ActorPackOptions {
            manifest,
            payloads,
            signing_key: signing_key.clone(),
        })
        .expect("pack the Front fixture");
        (package, signing_key)
    }

    fn selected_service_type() -> ActrType {
        ActrType {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        }
    }

    #[test]
    fn front_session_metadata_uses_verified_package_payloads() {
        use sha2::Digest as _;

        let (package, signing_key) = front_package_for_test("^1.0.0");
        let metadata = actrium_session_metadata_for_front(
            package,
            signing_key.verifying_key().to_bytes().to_vec(),
            selected_service_type(),
            "04".repeat(32),
        )
        .expect("derive verified Front metadata");

        assert_eq!(metadata.package_digest, vec![4; 32]);
        assert_eq!(
            metadata.guest_digest,
            sha2::Sha256::digest(b"globalThis.documents = true;").to_vec()
        );
        assert_eq!(metadata.protocol_fingerprint.len(), 32);
        assert_eq!(metadata.runtime.actrium_version, "1.0.0");
        assert_eq!(metadata.runtime.front_host_api, "1.0");
        assert_eq!(metadata.runtime.keel_api, "1.0");
        assert_eq!(metadata.program_capabilities, vec!["service"]);
        assert_eq!(
            metadata
                .routes
                .iter()
                .map(|route| route.route.as_str())
                .collect::<Vec<_>>(),
            vec!["documents/read", "documents/write"]
        );
        assert!(
            metadata
                .routes
                .iter()
                .all(|route| route.capabilities == vec!["service"])
        );
    }

    #[test]
    fn front_session_metadata_rejects_an_unsupported_host_api() {
        let (package, signing_key) = front_package_for_test("^2.0.0");
        let error = actrium_session_metadata_for_front(
            package,
            signing_key.verifying_key().to_bytes().to_vec(),
            selected_service_type(),
            "04".repeat(32),
        )
        .expect_err("the concrete v1 Host cannot satisfy a v2-only requirement");
        assert!(error.to_string().contains("does not satisfy"));
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

    #[test]
    fn metadata_from_the_real_counterapp_front() {
        use base64::Engine as _;
        use sha2::Digest as _;

        let (Some(front_path), Some(service_path), Some(key_path)) = (
            std::env::var_os("COUNTERAPP_FRONT"),
            std::env::var_os("COUNTERAPP_SERVICE"),
            std::env::var_os("COUNTERAPP_PUBKEY"),
        ) else {
            return;
        };
        let front = std::fs::read(front_path).expect("read front.actr");
        let service = std::fs::read(service_path).expect("read service.actr");
        let key_document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(key_path).expect("read the public key"))
                .expect("the public key file is JSON");
        let trusted_key = base64::engine::general_purpose::STANDARD
            .decode(key_document["public_key"].as_str().expect("public_key"))
            .expect("the public key is base64");
        let service_digest = hex::encode(sha2::Sha256::digest(&service));

        let metadata = actrium_session_metadata_for_front(
            front,
            trusted_key,
            ActrType {
                publisher: "publisher.develenv.com".into(),
                name: "counterapp-service".into(),
                version: "0.1.0".into(),
            },
            service_digest,
        )
        .expect("the real CounterApp Front yields valid session metadata");

        assert_eq!(metadata.program.publisher, "publisher.develenv.com");
        assert_eq!(metadata.program.name, "counterapp-service");
        assert_eq!(metadata.program.version, "0.1.0");
        assert_eq!(metadata.runtime.actrium_version, "1.0.0");
        assert_eq!(metadata.runtime.front_host_api, "1.0");
        assert_eq!(metadata.runtime.keel_api, "1.0");
        assert_eq!(metadata.program_capabilities, vec!["service"]);
        assert_eq!(metadata.routes.len(), 1);
        assert_eq!(metadata.routes[0].route, "echo.v1.EchoService.Echo");
        assert_eq!(metadata.routes[0].capabilities, vec!["service".to_string()]);
        assert_eq!(
            hex::encode(&metadata.guest_digest),
            "f0118a1e4ab675ceb56315bba0056feb9b7aad08aa500fae555ebec1f97dce96"
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
