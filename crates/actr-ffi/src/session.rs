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
        let routes = actr_hyper::actrium::service_routes(&routes);
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
        actr_hyper::actrium::allow_all_service_policies(config)
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
    let metadata = actr_hyper::actrium::front_session_metadata(
        &front_archive,
        &inspected.manifest,
        &service_type.into(),
        &package_digest,
    )
    .map_err(|error| invalid_session_metadata(error.to_string()))?;

    Ok(ActriumSessionMetadata {
        program: ProgramRelease {
            publisher: metadata.service_type.publisher,
            name: metadata.service_type.name,
            version: metadata.service_type.version,
        },
        package_digest: metadata.package_digest,
        guest_digest: metadata.guest_digest,
        protocol_fingerprint: metadata.protocol_fingerprint,
        runtime: ActriumRuntime {
            actrium_version: metadata.runtime.actrium_version,
            front_host_api: metadata.runtime.front_host_api,
            keel_api: metadata.runtime.keel_api,
        },
        program_capabilities: metadata.program_capabilities.into_iter().collect(),
        routes: metadata
            .routes
            .into_iter()
            .map(|(route, capabilities)| ActriumRoute {
                route,
                capabilities: capabilities.into_iter().collect(),
            })
            .collect(),
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

    /// Resume this lease after the verified transport channel is rebuilt.
    /// Returns a SessionExpired error when a fresh Attach is required.
    pub async fn resume(&self) -> ActrResult<()> {
        self.inner.resume().await.map_err(Into::into)
    }

    /// Send a DataChunk with proof metadata bound to this Attach session.
    pub async fn send_data_chunk(
        &self,
        route: String,
        capability: String,
        chunk: crate::types::DataChunk,
        payload_type: crate::types::PayloadType,
        timeout_ms: u64,
    ) -> ActrResult<()> {
        self.inner
            .send_data_chunk(
                &route,
                &capability,
                chunk.into(),
                payload_type.into(),
                Duration::from_millis(timeout_ms),
            )
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
        front_package_for_test_with_capabilities(
            host_api,
            br#"{"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js","imports":["host:service"],"hostModules":{"service":{"rpc":["documents/write","documents/read"]}}}"#,
        )
    }

    fn front_package_for_test_with_capabilities(
        host_api: &str,
        capabilities: &[u8],
    ) -> (Vec<u8>, ed25519_dalek::SigningKey) {
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
            ("workload/capabilities.json".into(), capabilities.to_vec()),
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
    fn front_session_metadata_accepts_an_event_only_service() {
        let (package, signing_key) = front_package_for_test_with_capabilities(
            "^1.0.0",
            br#"{"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js","imports":["host:service"],"hostModules":{"service":{"events":["documents/changed"]}}}"#,
        );
        let metadata = actrium_session_metadata_for_front(
            package,
            signing_key.verifying_key().to_bytes().to_vec(),
            selected_service_type(),
            "04".repeat(32),
        )
        .expect("event-only Service declarations retain their Attach binding");

        assert!(metadata.routes.is_empty());
        assert_eq!(metadata.program_capabilities, vec!["service"]);
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
