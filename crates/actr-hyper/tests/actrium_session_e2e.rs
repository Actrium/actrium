//! Actrium v1 session e2e: two real nodes, a mock relay minting Actor channel
//! certificates, verified-channel Attach, and a session-signed business call.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use actr_framework::{Bytes, Context, Dest, MessageDispatcher, Workload};
use actr_hyper::actrium::{ActriumAttachParams, FrontSessionMetadata};
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{ActrRef, Hyper, HyperConfig, Node, StaticTrust};
use actr_protocol::{ActrId, ActrType, PayloadType, RpcEnvelope, RpcRequest};
use actrium_runtime::{
    AttachmentPolicy, InMemorySessionService, ProgramRelease, ServiceConfig, ServicePolicies,
};
use async_trait::async_trait;
use tempfile::TempDir;

const REALM: &str = "test7@example.com";
const READ: &str = "documents.read";

/// Minimal typed RPC request used for the business call.
#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
pub struct EchoRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
pub struct EchoResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

impl RpcRequest for EchoRequest {
    type Response = EchoResponse;
    fn route_key() -> &'static str {
        "echo"
    }
}

struct EchoWorkload;

#[async_trait]
impl Workload for EchoWorkload {
    type Dispatcher = EchoDispatcher;
}

struct EchoDispatcher;

#[async_trait]
impl MessageDispatcher for EchoDispatcher {
    type Workload = EchoWorkload;

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "echo" => Ok(envelope.payload.unwrap_or_default()),
            other => Err(actr_protocol::ActrError::InvalidArgument(format!(
                "unknown route: {other}"
            ))),
        }
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
        realm: REALM.to_string(),
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
            tracing_service_name: "actrium-session-e2e".to_string(),
            logs_enabled: false,
            ca_cert_path: None,
        },
        config_dir: dir.path().to_path_buf(),
        trust: vec![],
        package_path: None,
        web: None,
        storage_root: None,
        wasi_data_dir: None,
        wasi_data_access: actr_config::WasiDataAccess::default(),
        oci: None,
        actrium: None,
    }
}

/// Allow-everything policy bundle for the session registry.
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
            capabilities: BTreeSet::from([READ.to_string()]),
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

fn service_type() -> ActrType {
    ActrType {
        publisher: "test-publisher".into(),
        name: "ActriumService".into(),
        version: "1.0.0".into(),
    }
}

fn front_type() -> ActrType {
    ActrType {
        publisher: "test-publisher".into(),
        name: "ActriumFront".into(),
        version: "1.0.0".into(),
    }
}

fn attach_params() -> ActriumAttachParams {
    ActriumAttachParams {
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: vec![4; 32],
        guest_digest: vec![5; 32],
        protocol_fingerprint: vec![6; 32],
        runtime: actrium_runtime::Runtime {
            actrium_version: "1".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        },
        requested_capabilities: vec![READ.to_string()],
    }
}

fn make_service_config(
    service_actor_id: &str,
    signing_key: ed25519_dalek::SigningKey,
) -> ServiceConfig {
    ServiceConfig {
        service_actor_id: service_actor_id.into(),
        runtime_instance_id: actr_hyper::process_runtime_instance_id().to_owned(),
        service_signing_key: signing_key,
        allowed_front_actor_types: BTreeSet::from([
            front_type().to_string_repr(),
            service_type().to_string_repr(),
        ]),
        program: ProgramRelease {
            publisher: "example.com".into(),
            name: "documents".into(),
            version: "1.0.0".into(),
        },
        package_digest: vec![4; 32],
        guest_digest: vec![5; 32],
        protocol_fingerprint: vec![6; 32],
        supported_runtimes: vec![actrium_runtime::Runtime {
            actrium_version: "1".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        }],
        program_capabilities: BTreeSet::from([READ.to_string()]),
        routes: BTreeMap::from([(
            "echo".to_string(),
            BTreeSet::from([READ.to_string()]),
        )]),
        attachment_policy: AttachmentPolicy::Exclusive,
        lease_duration: Duration::from_secs(300),
        heartbeat_interval: Duration::from_secs(30),
        grace_period: Duration::from_secs(60),
        replay_retention: ServiceConfig::DEFAULT_REPLAY_RETENTION,
        max_replay_entries: ServiceConfig::DEFAULT_MAX_REPLAY_ENTRIES,
        max_completed_sessions: ServiceConfig::DEFAULT_MAX_COMPLETED_SESSIONS,
    }
}

async fn register_node(
    server: &TestSignalingServer,
    dir: &TempDir,
    actor_type: ActrType,
) -> Node<actr_hyper::Registered> {
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let config = runtime_config(dir, server, actor_type);
    Node::from_hyper(hyper, config)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap()
}

async fn start_service_node(server: &TestSignalingServer, dir: &TempDir) -> (ActrRef, ActrId) {
    let registered = register_node(server, dir, service_type()).await;
    let service_actor_id = registered.actor_id();

    let policies = ServicePolicies {
        membership: Arc::new(AllowPolicies),
        authorize_attach: Arc::new(AllowPolicies),
        authorize_takeover: Arc::new(AllowPolicies),
        authorize_request: Arc::new(AllowPolicies),
        privacy: Arc::new(AllowPolicies),
        quota: Arc::new(AllowPolicies),
    };
    let service = InMemorySessionService::new(
        make_service_config(
            &service_actor_id.to_string_repr(),
            registered.actrium_actor_signing_key().unwrap(),
        ),
        policies,
    )
    .unwrap();
    let actr = registered
        .with_actrium_service(service)
        .start()
        .await
        .unwrap();
    (actr, service_actor_id)
}

async fn start_front_node(server: &TestSignalingServer, dir: &TempDir) -> ActrRef {
    let registered = register_node(server, dir, front_type()).await;
    let front_actor_id = registered.actor_id();

    let policies = ServicePolicies {
        membership: Arc::new(AllowPolicies),
        authorize_attach: Arc::new(AllowPolicies),
        authorize_takeover: Arc::new(AllowPolicies),
        authorize_request: Arc::new(AllowPolicies),
        privacy: Arc::new(AllowPolicies),
        quota: Arc::new(AllowPolicies),
    };
    let service = InMemorySessionService::new(
        make_service_config(
            &front_actor_id.to_string_repr(),
            registered.actrium_actor_signing_key().unwrap(),
        ),
        policies,
    )
    .unwrap();
    registered
        .with_actrium_service(service)
        .start()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_fronts_use_the_same_service_over_ordinary_relay() {
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.pause_forwarding();
    let service_dir = TempDir::new().unwrap();
    let first_dir = TempDir::new().unwrap();
    let second_dir = TempDir::new().unwrap();
    let params = attach_params();
    let service = register_node(&server, &service_dir, service_type())
        .await
        .with_ordinary_service_authorization(FrontSessionMetadata {
            service_type: service_type(),
            front_actor_type: front_type(),
            package_digest: params.package_digest,
            guest_digest: params.guest_digest,
            protocol_fingerprint: params.protocol_fingerprint,
            runtime: params.runtime,
            program_capabilities: BTreeSet::from([READ.to_owned()]),
            routes: BTreeMap::from([("echo".to_owned(), BTreeSet::from([READ.to_owned()]))]),
        })
        .start()
        .await
        .unwrap();
    let first = register_node(&server, &first_dir, front_type())
        .await
        .start()
        .await
        .unwrap();
    let second = register_node(&server, &second_dir, front_type())
        .await
        .start()
        .await
        .unwrap();
    assert_ne!(first.actor_id(), second.actor_id());
    tokio::time::timeout(Duration::from_secs(30), async {
        while server.relay_connection_count() != 3 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all three nodes connect to Relay");

    let service_id = service.actor_id();
    first.prepare_peer_rpc(&service_id, 15_000).await.unwrap();
    let unknown = ActrId {
        serial_number: service_id.serial_number + 10_000,
        ..service_id.clone()
    };
    assert!(first.prepare_peer_rpc(&unknown, 500).await.is_err());

    let target = Dest::Peer(service_id);
    let first_context = first.app_context().await;
    let second_context = second.app_context().await;
    let first_payload = Bytes::from_static(b"first");
    let second_payload = Bytes::from_static(b"second");
    let (first_reply, second_reply) = tokio::join!(
        first_context.call_raw(
            &target,
            "echo".into(),
            PayloadType::RpcReliable,
            first_payload.clone(),
            15_000
        ),
        second_context.call_raw(
            &target,
            "echo".into(),
            PayloadType::RpcReliable,
            second_payload.clone(),
            15_000
        ),
    );
    assert_eq!(first_reply.unwrap(), first_payload);
    assert_eq!(second_reply.unwrap(), second_payload);

    // A route the Service manifest does not grant must be rejected by the
    // session policy even when the caller is otherwise authenticated.
    let denied = first_context
        .call_raw(
            &target,
            "not.granted.Route".into(),
            PayloadType::RpcReliable,
            Bytes::from_static(b"payload"),
            15_000,
        )
        .await;
    assert!(matches!(
        denied,
        Err(actr_protocol::ActrError::PermissionDenied(message))
            if message.contains("Service policy does not grant this caller the requested route")
    ));

    for node in [first, second, service] {
        node.shutdown();
        node.wait_for_shutdown().await;
    }
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn attach_and_session_signed_business_call_over_verified_relay() {
    if !actr_hyper::ACTOR_CHANNEL_AUTHENTICATION_ENABLED {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.enable_actor_certificates();
    // Block SDP offer/answer/ICE forwarding so data can only flow over the
    // verified Actor channel on the relay (no legacy WebRTC bypass).
    server.pause_forwarding();

    let service_dir = TempDir::new().unwrap();
    let front_dir = TempDir::new().unwrap();
    let (service, service_actor_id) = start_service_node(&server, &service_dir).await;
    let front = start_front_node(&server, &front_dir).await;

    tokio::time::timeout(Duration::from_secs(30), async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both nodes authenticate their relay sockets");

    // The peer handshake may still be in flight; Attach retries until the
    // verified Actor channel exists on both ends.
    let session = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match front
                .attach_actrium(&service_actor_id, attach_params())
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
    .expect("Attach succeeds within 20s");

    let payload = b"hello over the verified session".to_vec();
    let response = tokio::time::timeout(
        Duration::from_secs(60),
        session.call("echo", READ, payload.clone(), Duration::from_secs(30)),
    )
    .await
    .expect("business call must not hang")
    .expect("session-signed business call succeeds");
    assert_eq!(response, payload, "echo returns the signed request payload");

    // A fire-and-forget tell is admitted by the same Guard pipeline.
    session
        .tell("echo", READ, b"tell payload".to_vec())
        .await
        .expect("session-signed tell is admitted");

    // A rebuilt verified channel gets a fresh channel generation. Resume the
    // same lease before the next business request, exactly as the macOS Host
    // does for an already-open tab.
    let previous_epoch = session.lease_epoch().await;
    let discovery_before = server.relay_discovery_requests();
    server.disconnect_relay_connections();
    tokio::time::timeout(Duration::from_secs(30), async {
        while server.relay_connection_count() != 2
            || server.relay_discovery_requests() < discovery_before + 2
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both relay channels are rebuilt");
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if session.resume().await.is_ok() && session.lease_epoch().await > previous_epoch {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the same tab resumes over the rebuilt channel");
    let resumed_payload = b"after reconnect".to_vec();
    let response = tokio::time::timeout(
        Duration::from_secs(60),
        session.call(
            "echo",
            READ,
            resumed_payload.clone(),
            Duration::from_secs(30),
        ),
    )
    .await
    .expect("resumed call must not hang")
    .expect("the resumed session makes a signed business call");
    assert_eq!(response, resumed_payload);

    session.detach().await.expect("detach succeeds");
    let after = tokio::time::timeout(
        Duration::from_secs(60),
        session.call("echo", READ, vec![7], Duration::from_secs(30)),
    )
    .await
    .expect("post-detach call must not hang");
    assert!(after.is_err(), "calls after detach are rejected");

    service.shutdown();
    service.wait_for_shutdown().await;
    front.shutdown();
    front.wait_for_shutdown().await;
    server.shutdown().await;
}
