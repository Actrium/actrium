//! End-to-end Attach session against the real CounterApp packages.
//!
//! Unlike `actrium_session_e2e` (which links a Rust mock workload), this test
//! loads the signed CounterApp Front and Service archives and runs the real
//! `actr.component/1` Service component. It then creates a Front node through
//! `Node::attach_front`, derives the v1 Attach metadata from the verified Front
//! payloads, attaches over a verified Actor channel, and receives the echo.
//!
//! The packages are external fixtures, so the test only runs when the paths
//! are supplied:
//!
//! ```sh
//! COUNTERAPP_FRONT=/path/front.actr \
//! COUNTERAPP_SERVICE=/path/service.actr \
//! COUNTERAPP_PUBKEY=/path/front-public-key.json \
//! cargo test -p actr-hyper --features wasm-engine --test counterapp_realm_e2e -- --nocapture
//! ```

#![cfg(all(feature = "wasm-engine", not(target_arch = "wasm32")))]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use actr_hyper::actrium::ActriumAttachParams;
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{Hyper, HyperConfig, Node, StaticTrust, WorkloadPackage};
use actr_protocol::prost::Message as _;
use actr_protocol::{ActrType, prost};
use actrium_runtime::{
    AttachmentPolicy, InMemorySessionService, ProgramRelease, Runtime, ServiceConfig,
    ServicePolicies,
};
use async_trait::async_trait;
use base64::Engine;
use tempfile::TempDir;

const SERVICE_CAPABILITY: &str = "service";
const ROUTE: &str = "echo.v1.EchoService.Echo";

#[derive(Clone, PartialEq, prost::Message)]
struct EchoRequest {
    #[prost(string, tag = "1")]
    message: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct EchoReply {
    #[prost(string, tag = "1")]
    reply: String,
}

/// Allow-everything policy bundle; the session protocol itself is still fully
/// exercised (verification, lease, signing, response proofs).
struct AllowPolicies;

#[async_trait]
impl actrium_runtime::MembershipProvider for AllowPolicies {
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
            capabilities: BTreeSet::from([SERVICE_CAPABILITY.to_string()]),
        }))
    }
}

#[async_trait]
impl actrium_runtime::AuthorizeAttach for AllowPolicies {
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

#[async_trait]
impl actrium_runtime::AuthorizeTakeover for AllowPolicies {
    async fn authorize_takeover(
        &self,
        _context: actrium_runtime::TakeoverContext,
    ) -> Result<bool, actrium_runtime::SessionError> {
        Ok(true)
    }
}

#[async_trait]
impl actrium_runtime::AuthorizeRequest for AllowPolicies {
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

#[async_trait]
impl actrium_runtime::PrivacyPolicy for AllowPolicies {
    async fn evaluate(
        &self,
        context: actrium_runtime::PrivacyContext,
    ) -> Result<actrium_runtime::PrivacyDecision, actrium_runtime::SessionError> {
        Ok(actrium_runtime::PrivacyDecision {
            state: actrium_runtime::PrivacyState {
                actor_state_digest: vec![1; 32],
                application_state_digest: vec![2; 32],
                assurance_digest: vec![3; 32],
                expires_at_epoch_secs: u64::MAX,
            },
            allowed_capabilities: context.requested_capabilities,
        })
    }
}

#[async_trait]
impl actrium_runtime::ResourceQuota for AllowPolicies {
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

fn policies() -> ServicePolicies {
    ServicePolicies {
        membership: Arc::new(AllowPolicies),
        authorize_attach: Arc::new(AllowPolicies),
        authorize_takeover: Arc::new(AllowPolicies),
        authorize_request: Arc::new(AllowPolicies),
        privacy: Arc::new(AllowPolicies),
        quota: Arc::new(AllowPolicies),
    }
}

fn runtime_config(
    dir: &TempDir,
    server: &TestSignalingServer,
    actr_type: ActrType,
) -> actr_config::RuntimeConfig {
    actr_config::RuntimeConfig {
        package: actr_config::PackageInfo {
            name: actr_type.name.clone(),
            actr_type,
            description: None,
            authors: vec![],
            license: None,
        },
        signaling_url: url::Url::parse(&server.url()).unwrap(),
        realm: "counterapp-e2e@example.com".to_string(),
        issuer_endpoint: format!("http://127.0.0.1:{}/issuer", server.port()),
        realm_client_key: None,
        visible_in_discovery: true,
        acl: None,
        mailbox_path: None,
        scripts: HashMap::new(),
        webrtc: actr_config::WebRtcConfig::default(),
        observability: actr_config::ObservabilityConfig {
            filter_level: "warn".to_string(),
            tracing_enabled: false,
            tracing_endpoint: String::new(),
            tracing_service_name: "counterapp-realm-e2e".to_string(),
            logs_enabled: false,
            ca_cert_path: None,
        },
        config_dir: dir.path().to_path_buf(),
        trust: vec![],
        package_path: None,
        web: None,
        storage_root: None,
    }
}

/// The exact declarations the macOS Front derives through
/// `actrium_session_metadata_for_front`.
struct Metadata {
    service_type: ActrType,
    front_type: ActrType,
    package_digest: Vec<u8>,
    guest_digest: Vec<u8>,
    protocol_fingerprint: Vec<u8>,
    runtime: Runtime,
    routes: BTreeMap<String, BTreeSet<String>>,
}

fn hex_decode(value: &str) -> Vec<u8> {
    assert!(value.len() % 2 == 0, "odd-length hex digest");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("valid hex"))
        .collect()
}

fn derive_metadata(front_bytes: &[u8], service_bytes: &[u8], trusted_key: &[u8; 32]) -> Metadata {
    use sha2::Digest as _;

    let verifying_key =
        ed25519_dalek::VerifyingKey::from_bytes(trusted_key).expect("valid Front signing key");
    let inspected = actr_pack::verify_actor_package(front_bytes, &verifying_key)
        .expect("the Front archive verifies against its publisher key");
    let actor = &inspected.manifest.actor;
    let front_type = ActrType {
        publisher: actor.publisher.clone(),
        name: actor.name.clone(),
        version: actor.version.clone(),
    };

    let dependency = inspected
        .manifest
        .dependencies
        .get("service")
        .expect("[dependencies.service] is present");
    let (service_publisher, service_name) = dependency
        .actor
        .split_once('/')
        .expect("dependency actor is publisher/name");
    let service_version = dependency.version.trim_start_matches('=').to_string();
    let service_type = ActrType {
        publisher: service_publisher.to_string(),
        name: service_name.to_string(),
        version: service_version,
    };

    let package_digest = sha2::Sha256::digest(service_bytes).to_vec();
    let guest = actr_pack::read_actor_workload(front_bytes, &inspected)
        .expect("the Front guest payload is readable");
    let guest_digest = sha2::Sha256::digest(&guest).to_vec();

    let capabilities_path = inspected
        .manifest
        .workload
        .keel
        .as_ref()
        .and_then(|keel| keel.capabilities.as_deref())
        .expect("the Keel capability manifest is declared");
    let capabilities_bytes = actr_pack::read_actor_package_file(
        front_bytes,
        &inspected,
        capabilities_path,
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )
    .expect("the capability manifest is readable");
    let capabilities: serde_json::Value =
        serde_json::from_slice(&capabilities_bytes).expect("the capability manifest is JSON");
    let keel_api = capabilities["keelApi"]
        .as_str()
        .expect("keelApi is present")
        .to_string();
    let rpc_routes = capabilities["hostModules"]["service"]["rpc"]
        .as_array()
        .expect("hostModules.service.rpc is present");
    let routes: BTreeMap<String, BTreeSet<String>> = rpc_routes
        .iter()
        .map(|route| {
            (
                route.as_str().expect("route is a string").to_string(),
                BTreeSet::from([SERVICE_CAPABILITY.to_string()]),
            )
        })
        .collect();
    assert!(
        routes.contains_key(ROUTE),
        "the CounterApp Guest calls {ROUTE}"
    );

    let contract_path = dependency
        .contract
        .as_deref()
        .expect("the Service Proto contract is declared");
    let contract_bytes = actr_pack::read_actor_package_file(
        front_bytes,
        &inspected,
        contract_path,
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )
    .expect("the Service Proto is readable");
    let contract_name = contract_path.rsplit('/').next().unwrap();
    let fingerprint = actr_service_compat::Fingerprint::calculate_service_semantic_fingerprint(&[
        actr_service_compat::ProtoFile {
            name: contract_name.to_string(),
            content: String::from_utf8(contract_bytes).expect("the Proto is UTF-8"),
            path: None,
        },
    ])
    .expect("the Service Proto fingerprint is computable");
    let protocol_fingerprint = hex_decode(
        fingerprint
            .strip_prefix("service_semantic:")
            .expect("the fingerprint uses the semantic encoding"),
    );

    Metadata {
        service_type,
        front_type,
        package_digest,
        guest_digest,
        protocol_fingerprint,
        runtime: Runtime {
            actrium_version: "1.0.0".to_string(),
            front_host_api: "1.0".to_string(),
            keel_api,
        },
        routes,
    }
}

fn service_config(
    actor_id: &str,
    signing_key: ed25519_dalek::SigningKey,
    metadata: &Metadata,
) -> ServiceConfig {
    ServiceConfig {
        service_actor_id: actor_id.to_string(),
        runtime_instance_id: actr_hyper::process_runtime_instance_id().to_owned(),
        service_signing_key: signing_key,
        allowed_front_actor_types: BTreeSet::from([metadata.front_type.to_string_repr()]),
        program: ProgramRelease {
            publisher: metadata.service_type.publisher.clone(),
            name: metadata.service_type.name.clone(),
            version: metadata.service_type.version.clone(),
        },
        package_digest: metadata.package_digest.clone(),
        guest_digest: metadata.guest_digest.clone(),
        protocol_fingerprint: metadata.protocol_fingerprint.clone(),
        supported_runtimes: vec![metadata.runtime.clone()],
        program_capabilities: BTreeSet::from([SERVICE_CAPABILITY.to_string()]),
        routes: metadata.routes.clone(),
        attachment_policy: AttachmentPolicy::Exclusive,
        lease_duration: Duration::from_secs(300),
        heartbeat_interval: Duration::from_secs(30),
        grace_period: Duration::from_secs(60),
        replay_retention: ServiceConfig::DEFAULT_REPLAY_RETENTION,
        max_replay_entries: ServiceConfig::DEFAULT_MAX_REPLAY_ENTRIES,
        max_completed_sessions: ServiceConfig::DEFAULT_MAX_COMPLETED_SESSIONS,
    }
}

fn read_public_key(path: &std::path::Path) -> [u8; 32] {
    let body = std::fs::read(path).expect("read the publisher public key");
    let document: serde_json::Value =
        serde_json::from_slice(&body).expect("the public key file is JSON");
    let encoded = document["public_key"]
        .as_str()
        .expect("the public key file has a public_key field");
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("the public key is base64")
        .as_slice()
        .try_into()
        .expect("the Ed25519 public key is 32 bytes")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn counterapp_front_attaches_to_the_real_service_package() {
    let (Some(front_path), Some(service_path), Some(key_path)) = (
        std::env::var_os("COUNTERAPP_FRONT"),
        std::env::var_os("COUNTERAPP_SERVICE"),
        std::env::var_os("COUNTERAPP_PUBKEY"),
    ) else {
        eprintln!(
            "skipping: set COUNTERAPP_FRONT, COUNTERAPP_SERVICE and COUNTERAPP_PUBKEY \
             to the CounterApp dist/ artifacts"
        );
        return;
    };

    let front_bytes = std::fs::read(&front_path).expect("read front.actr");
    let service_bytes = std::fs::read(&service_path).expect("read service.actr");
    let trusted_key = read_public_key(std::path::Path::new(&key_path));
    let metadata = derive_metadata(&front_bytes, &service_bytes, &trusted_key);
    eprintln!(
        "fixtures: front={} service={} guest_digest={} service_digest={}",
        metadata.front_type.to_string_repr(),
        metadata.service_type.to_string_repr(),
        metadata
            .guest_digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        metadata
            .package_digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    );

    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.enable_actor_certificates();
    // Only the verified Actor channel may carry data.
    server.pause_forwarding();

    let service_dir = TempDir::new().unwrap();
    let front_dir = TempDir::new().unwrap();
    let trust = Arc::new(StaticTrust::new(trusted_key).expect("valid trust key"));
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());

    // ── Service host: run the packaged Component and accept Attach ──────────
    let service_hyper = Hyper::new(HyperConfig::new(service_dir.path(), trust.clone()))
        .await
        .unwrap();
    let service_registered = Node::from_hyper(
        service_hyper,
        runtime_config(&service_dir, &server, metadata.service_type.clone()),
    )
    .attach(&WorkloadPackage::new(service_bytes))
    .await
    .expect("the packaged CounterApp Service attaches")
    .register(&issuer)
    .await
    .expect("the CounterApp Service registers");
    let service_actor_id = service_registered.actor_id();
    let service_key = service_registered
        .actrium_actor_signing_key()
        .expect("the Service node holds its Actor key");
    let service = InMemorySessionService::new(
        service_config(&service_actor_id.to_string_repr(), service_key, &metadata),
        policies(),
    )
    .expect("valid Service session config");
    let service_ref = service_registered
        .with_actrium_service(service)
        .start()
        .await
        .expect("start the CounterApp Service host");
    eprintln!("service ready: {service_actor_id:?}");

    // ── Front host: the same node shape the macOS app creates ───────────────
    let front_hyper = Hyper::new(HyperConfig::new(front_dir.path(), trust))
        .await
        .unwrap();
    let front_registered = Node::from_hyper(
        front_hyper,
        runtime_config(&front_dir, &server, metadata.front_type.clone()),
    )
    .attach_front(&WorkloadPackage::new(front_bytes), &metadata.service_type)
    .await
    .expect("the packaged CounterApp Front attaches")
    .register(&issuer)
    .await
    .expect("the CounterApp Front registers");
    let front_actor_id = front_registered.actor_id();
    let front_key = front_registered
        .actrium_actor_signing_key()
        .expect("the Front node holds its Actor key");
    let front_service = InMemorySessionService::new(
        service_config(&front_actor_id.to_string_repr(), front_key, &metadata),
        policies(),
    )
    .expect("valid Front session config");
    let front_ref = front_registered
        .with_actrium_service(front_service)
        .start()
        .await
        .expect("start the CounterApp Front host");
    eprintln!("front ready: {front_actor_id:?}");

    tokio::time::timeout(Duration::from_secs(30), async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both nodes authenticate their relay sockets");

    // ── Discover the Service, then Attach ───────────────────────────────────
    let discovered = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match front_ref
                .discover_route_candidates(&metadata.service_type, 1)
                .await
            {
                Ok(candidates) if !candidates.is_empty() => return candidates[0].clone(),
                Ok(_) => {}
                Err(error) => eprintln!("discovery attempt failed: {error}"),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("the Front discovers the CounterApp Service");
    assert_eq!(
        discovered, service_actor_id,
        "discovery resolves the Service node"
    );

    let attach_params = ActriumAttachParams {
        program: ProgramRelease {
            publisher: metadata.service_type.publisher.clone(),
            name: metadata.service_type.name.clone(),
            version: metadata.service_type.version.clone(),
        },
        package_digest: metadata.package_digest.clone(),
        guest_digest: metadata.guest_digest.clone(),
        protocol_fingerprint: metadata.protocol_fingerprint.clone(),
        runtime: metadata.runtime.clone(),
        requested_capabilities: vec![SERVICE_CAPABILITY.to_string()],
    };
    let session = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match front_ref
                .attach_actrium(&discovered, attach_params.clone())
                .await
            {
                Ok(session) => return session,
                Err(error) => {
                    eprintln!("attach attempt failed: {error}");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    })
    .await
    .expect("Attach succeeds within 30s");

    // ── Business call: the real packaged Service executes Echo ──────────────
    let request = EchoRequest {
        message: "hello".to_string(),
    }
    .encode_to_vec();
    let response = tokio::time::timeout(
        Duration::from_secs(60),
        session.call(ROUTE, SERVICE_CAPABILITY, request, Duration::from_secs(30)),
    )
    .await
    .expect("the session call must not hang")
    .expect("the session-signed call succeeds");
    let reply = EchoReply::decode(response.as_slice()).expect("decode EchoReply");
    assert_eq!(reply.reply, "echo: hello");

    session.detach().await.expect("detach succeeds");
    front_ref.shutdown();
    front_ref.wait_for_shutdown().await;
    service_ref.shutdown();
    service_ref.wait_for_shutdown().await;
    server.shutdown().await;
}
