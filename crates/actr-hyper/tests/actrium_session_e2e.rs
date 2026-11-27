//! Actrium v1 session e2e: two real nodes, a mock relay minting Actor channel
//! certificates, verified-channel Attach, and a session-signed business call.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use actr_framework::{Bytes, Context, Dest, MessageDispatcher, WebRtcNetworkPath, Workload};
use actr_hyper::actrium::{ActriumAttachParams, FrontSessionMetadata};
use actr_hyper::test_support::{TestRtcControl, TestSignalingServer};
use actr_hyper::{ActrRef, Hyper, HyperConfig, Node, StaticTrust};
use actr_protocol::{
    ActrId, ActrType, DataChunk, PayloadType, RpcEnvelope, RpcRequest, prost::Message as _,
};
use actrium_runtime::{
    AttachmentPolicy, InMemorySessionService, ProgramRelease, ServiceConfig, ServicePolicies,
};
use async_trait::async_trait;
use tempfile::TempDir;

const REALM: &str = "test7@example.com";
const READ: &str = "documents.read";
const EXPLICIT_ECHO: &str = "echo.explicit";
const PREPARE_ATTACHMENT_ROUTE: &str = "ask.AskService.PrepareAttachmentUpload";
const ATTACHMENT_UPLOAD_ROUTE: &str = "uploadAttachment";

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

#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
pub struct PrepareAttachmentRequest {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "8")]
    pub result_stream_id: String,
}

#[derive(Clone, PartialEq, actr_protocol::prost::Message)]
pub struct PrepareAttachmentResponse {
    #[prost(int32, tag = "2")]
    pub status_code: i32,
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

    fn reply_mode_for_route(route: &str) -> actr_protocol::ReplyMode {
        if route == EXPLICIT_ECHO {
            actr_protocol::ReplyMode::Explicit
        } else {
            actr_protocol::ReplyMode::Return
        }
    }

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "echo" => Ok(envelope.payload.unwrap_or_default()),
            EXPLICIT_ECHO => {
                let request = EchoRequest::decode(envelope.payload.unwrap_or_default())
                    .map_err(|error| actr_protocol::ActrError::DecodeFailure(error.to_string()))?;
                let deliver = ctx
                    .reply_deliver()
                    .expect("explicit route has reply delivery");
                let reply = actr_framework::Reply::new(
                    ctx.request_id().to_string(),
                    ctx.reply_deadline(),
                    deliver,
                    ctx.expects_response(),
                );
                reply.send(Ok(EchoResponse {
                    payload: request.payload,
                }));
                Ok(Bytes::new())
            }
            PREPARE_ATTACHMENT_ROUTE => {
                let payload = envelope.payload.unwrap_or_default();
                let request =
                    PrepareAttachmentRequest::decode(payload.clone()).map_err(|error| {
                        actr_protocol::ActrError::InvalidArgument(format!(
                            "invalid attachment prepare payload: {error}"
                        ))
                    })?;
                if request.id == "reject" {
                    Ok(Bytes::from(
                        PrepareAttachmentResponse { status_code: 13 }.encode_to_vec(),
                    ))
                } else {
                    Ok(payload)
                }
            }
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
        routes: actr_hyper::actrium::service_routes(&BTreeMap::from([
            ("echo".to_string(), BTreeSet::from([READ.to_string()])),
            (
                EXPLICIT_ECHO.to_string(),
                BTreeSet::from([READ.to_string()]),
            ),
            (
                ATTACHMENT_UPLOAD_ROUTE.to_string(),
                BTreeSet::from([READ.to_string()]),
            ),
        ])),
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
    register_node_with_webrtc(
        server,
        dir,
        actor_type,
        actr_config::WebRtcConfig::default(),
    )
    .await
}

async fn register_node_with_webrtc(
    server: &TestSignalingServer,
    dir: &TempDir,
    actor_type: ActrType,
    webrtc: actr_config::WebRtcConfig,
) -> Node<actr_hyper::Registered> {
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let mut config = runtime_config(dir, server, actor_type);
    config.webrtc = webrtc;
    Node::from_hyper(hyper, config)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap()
}

async fn start_service_node(
    server: &TestSignalingServer,
    dir: &TempDir,
    webrtc: actr_config::WebRtcConfig,
    heartbeat_interval: Duration,
) -> (ActrRef, ActrId, InMemorySessionService) {
    let registered = register_node_with_webrtc(server, dir, service_type(), webrtc).await;
    let service_actor_id = registered.actor_id();

    let policies = ServicePolicies {
        membership: Arc::new(AllowPolicies),
        authorize_attach: Arc::new(AllowPolicies),
        authorize_takeover: Arc::new(AllowPolicies),
        authorize_request: Arc::new(AllowPolicies),
        privacy: Arc::new(AllowPolicies),
        quota: Arc::new(AllowPolicies),
    };
    let mut config = make_service_config(
        &service_actor_id.to_string_repr(),
        registered.actrium_actor_signing_key().unwrap(),
    );
    config.heartbeat_interval = heartbeat_interval;
    // Give heartbeat renewal room before the Actor certificate's hard expiry.
    config.lease_duration = Duration::from_secs(120);
    let service = InMemorySessionService::new(config, policies).unwrap();
    let actr = registered
        .with_actrium_service(service.clone())
        .start()
        .await
        .unwrap();
    (actr, service_actor_id, service)
}

async fn start_front_node(
    server: &TestSignalingServer,
    dir: &TempDir,
    webrtc: actr_config::WebRtcConfig,
) -> ActrRef {
    let registered = register_node_with_webrtc(server, dir, front_type(), webrtc).await;
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

    let denied = first_context
        .call_raw(
            &target,
            PREPARE_ATTACHMENT_ROUTE.into(),
            PayloadType::RpcReliable,
            Bytes::from(PrepareAttachmentRequest::default().encode_to_vec()),
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
    // Block SDP offer/answer/ICE forwarding so data can only flow over the
    // verified Actor channel using the relay transport.
    server.pause_forwarding();

    let service_dir = TempDir::new().unwrap();
    let front_dir = TempDir::new().unwrap();
    let (service, service_actor_id, _) = start_service_node(
        &server,
        &service_dir,
        Default::default(),
        Duration::from_secs(30),
    )
    .await;
    let front = start_front_node(&server, &front_dir, Default::default()).await;

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

    // The same authenticated session carries the attachment prepare RPC and
    // the stream chunks. Both paths are admitted by the receiving Guard.
    let prepare = PrepareAttachmentRequest {
        id: String::new(),
        result_stream_id: "attachment-result".to_string(),
    }
    .encode_to_vec();
    let prepared = session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            prepare.clone(),
            Duration::from_secs(30),
        )
        .await
        .expect("prepare RPC is bound to the Attach session");
    let prepared = PrepareAttachmentRequest::decode(prepared.as_slice()).unwrap();
    let transport_result_stream = prepared.result_stream_id;
    assert_ne!(transport_result_stream, "attachment-result");
    assert!(transport_result_stream.starts_with("actrium-result-"));

    // An application-level rejection is still a successful, authenticated RPC.
    // It must release the Front reservation immediately; explicit cancellation
    // through unregister_stream must do the same for an accepted Prepare.
    let rejected = session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            PrepareAttachmentRequest {
                id: "reject".into(),
                result_stream_id: "reused-attachment-result".into(),
            }
            .encode_to_vec(),
            Duration::from_secs(30),
        )
        .await
        .expect("the rejected Prepare response is authenticated");
    assert_eq!(
        PrepareAttachmentResponse::decode(rejected.as_slice())
            .unwrap()
            .status_code,
        13
    );
    let accepted = PrepareAttachmentRequest {
        id: String::new(),
        result_stream_id: "reused-attachment-result".into(),
    }
    .encode_to_vec();
    session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            accepted.clone(),
            Duration::from_secs(30),
        )
        .await
        .expect("a rejected Prepare releases its result stream for immediate reuse");
    front
        .unregister_stream("reused-attachment-result")
        .await
        .expect("explicit cancellation unregisters the result stream");
    session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            accepted,
            Duration::from_secs(30),
        )
        .await
        .expect("explicit cancellation releases its result stream for immediate reuse");
    front
        .unregister_stream("reused-attachment-result")
        .await
        .expect("clean up the reused result stream");

    // Reusing the caller-visible stream must not allow abandoned upload work
    // to complete a newer Prepare. The Host gives each attempt a distinct
    // transport stream and restores the public ID only after verification.
    let old_prepare = session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            PrepareAttachmentRequest {
                id: "late-old".into(),
                result_stream_id: "late-attachment-result".into(),
            }
            .encode_to_vec(),
            Duration::from_secs(30),
        )
        .await
        .expect("prepare the abandoned upload");
    let old_transport_stream = PrepareAttachmentRequest::decode(old_prepare.as_slice())
        .unwrap()
        .result_stream_id;
    front
        .unregister_stream("late-attachment-result")
        .await
        .expect("cancel the abandoned upload");
    let new_prepare = session
        .call(
            PREPARE_ATTACHMENT_ROUTE,
            READ,
            PrepareAttachmentRequest {
                id: "late-new".into(),
                result_stream_id: "late-attachment-result".into(),
            }
            .encode_to_vec(),
            Duration::from_secs(30),
        )
        .await
        .expect("reuse the public result stream for a new upload");
    let new_transport_stream = PrepareAttachmentRequest::decode(new_prepare.as_slice())
        .unwrap()
        .result_stream_id;
    assert_ne!(old_transport_stream, new_transport_stream);

    let (late_results, mut late_result_rx) = tokio::sync::mpsc::unbounded_channel();
    front
        .app_context()
        .await
        .register_stream("late-attachment-result".into(), move |chunk, _| {
            let late_results = late_results.clone();
            Box::pin(async move {
                late_results.send(chunk).unwrap();
                Ok(())
            })
        })
        .await
        .expect("register the replacement result callback");
    for (stream_id, payload) in [
        (old_transport_stream, b"abandoned result".as_slice()),
        (new_transport_stream, b"replacement result".as_slice()),
    ] {
        service
            .app_context()
            .await
            .send_data_chunk(
                &Dest::Peer(front.actor_id()),
                DataChunk {
                    stream_id,
                    sequence: 0,
                    payload: Bytes::copy_from_slice(payload),
                    ..Default::default()
                },
                PayloadType::StreamReliable,
            )
            .await
            .expect("send an attempt-specific terminal result");
    }
    let replacement = tokio::time::timeout(Duration::from_secs(10), late_result_rx.recv())
        .await
        .expect("the replacement attempt receives its own result")
        .expect("the replacement result receiver is open");
    assert_eq!(replacement.stream_id, "late-attachment-result");
    assert_eq!(
        replacement.payload,
        Bytes::from_static(b"replacement result")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), late_result_rx.recv())
            .await
            .is_err(),
        "the abandoned attempt must not deliver to the replacement callback"
    );

    let (chunks, mut received) = tokio::sync::mpsc::unbounded_channel();
    service
        .app_context()
        .await
        .register_stream("attachment-chunk".into(), move |chunk, _| {
            let chunks = chunks.clone();
            Box::pin(async move {
                chunks.send(chunk).unwrap();
                Ok(())
            })
        })
        .await
        .expect("register the attachment chunk stream");
    let chunk_payload = b"attachment bytes".to_vec();
    session
        .send_data_chunk(
            ATTACHMENT_UPLOAD_ROUTE,
            READ,
            DataChunk {
                stream_id: "attachment-chunk".into(),
                sequence: 0,
                payload: Bytes::from(chunk_payload.clone()),
                ..Default::default()
            },
            PayloadType::StreamReliable,
            Duration::from_secs(30),
        )
        .await
        .expect("session-signed DataChunk is admitted");
    let chunk = tokio::time::timeout(Duration::from_secs(10), received.recv())
        .await
        .expect("the Service receives the attachment chunk")
        .expect("the chunk receiver is open");
    assert_eq!(chunk.payload, Bytes::from(chunk_payload));
    assert!(
        chunk
            .metadata
            .iter()
            .any(|entry| entry.key == actr_hyper::actrium::metadata::PROOF)
    );

    // The prepare RPC reserves its result stream. A raw workload-originated
    // terminal chunk is signed inside the trusted Host, and the Front's Guard
    // verifies it before invoking the registered completion callback.
    let (results, mut result_rx) = tokio::sync::mpsc::unbounded_channel();
    front
        .app_context()
        .await
        .register_stream("attachment-result".into(), move |chunk, _| {
            let results = results.clone();
            Box::pin(async move {
                results.send(chunk).unwrap();
                Ok(())
            })
        })
        .await
        .expect("register the Front attachment result stream");
    service
        .app_context()
        .await
        .send_data_chunk(
            &Dest::Peer(front.actor_id()),
            DataChunk {
                stream_id: transport_result_stream,
                sequence: 0,
                payload: Bytes::from_static(b"terminal upload result"),
                ..Default::default()
            },
            PayloadType::StreamReliable,
        )
        .await
        .expect("the Host signs the terminal result on the prepared session");
    let result = tokio::time::timeout(Duration::from_secs(10), result_rx.recv())
        .await
        .expect("the Front Guard admits the authenticated terminal result")
        .expect("the result receiver is open");
    assert_eq!(
        result.payload,
        Bytes::from_static(b"terminal upload result")
    );
    assert!(
        result
            .metadata
            .iter()
            .any(|entry| entry.key == actr_hyper::actrium::metadata::PROOF)
    );

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actrium_session_over_direct_ice_and_replacement() {
    webrtc_session_lifecycle(false, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actrium_session_over_forced_turn() {
    webrtc_session_lifecycle(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actrium_session_over_forced_turn_with_verified_wss_available() {
    webrtc_session_lifecycle(true, true).await;
}

async fn webrtc_session_lifecycle(turn_only: bool, with_wss: bool) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let mut server = if with_wss {
        TestSignalingServer::start_with_relay().await.unwrap()
    } else {
        TestSignalingServer::start().await.unwrap()
    };
    let mut webrtc = actr_config::WebRtcConfig::default();
    let turn_socket = if turn_only {
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        webrtc.ice_transport_policy = actr_config::IceTransportPolicy::Relay;
        webrtc.ice_servers = vec![actr_config::IceServer {
            urls: vec![format!(
                "turn:{}?transport=udp",
                socket.local_addr().unwrap()
            )],
            ..Default::default()
        }];
        Some(socket)
    } else {
        None
    };
    let expected_path = if turn_only {
        WebRtcNetworkPath::Relayed
    } else {
        WebRtcNetworkPath::Direct
    };

    let service_dir = TempDir::new().unwrap();
    let front_dir = TempDir::new().unwrap();
    let (service, service_id, registry) = start_service_node(
        &server,
        &service_dir,
        webrtc.clone(),
        Duration::from_secs(1),
    )
    .await;
    let front = start_front_node(&server, &front_dir, webrtc).await;
    let front_id = front.actor_id();
    let turn_server = if let Some(socket) = turn_socket {
        let mut passwords = BTreeMap::new();
        for actor in [&front, &service] {
            let credential = actor.session_state().unwrap().turn_credential().await;
            passwords.insert(credential.username, credential.password);
        }
        let auth = turn::StaticAuthenticator::new(passwords).unwrap();
        Some(
            turn::create_turn_server(
                socket,
                "127.0.0.1",
                REALM,
                (49_152, 65_535),
                Arc::new(auth),
                true,
            )
            .await
            .expect("start local TURN"),
        )
    } else {
        None
    };

    // Warm and prove an authenticated WSS channel before enabling ICE.
    let wss_channel = if with_wss {
        server.pause_forwarding();
        let wss = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match front.attach_actrium(&service_id, attach_params()).await {
                    Ok(session) => break session,
                    Err(error) => {
                        eprintln!("waiting for verified WSS: {error}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        })
        .await
        .expect("authenticated WSS Attach");
        let channel = registry
            .session(&wss.session_id().await)
            .await
            .unwrap()
            .channel_id;
        wss.detach().await.expect("release the WSS lease");
        assert_eq!(server.relay_connection_count(), 2);
        server.resume_forwarding();
        Some(channel)
    } else {
        assert_eq!(server.relay_connection_count(), 0);
        None
    };
    let relay_frames = server.relay_frame_count();

    let session = if with_wss {
        front
            .attach_actrium_over_webrtc(&service_id, attach_params())
            .await
    } else {
        front.attach_actrium(&service_id, attach_params()).await
    }
    .expect("Attach over the selected authenticated WebRTC wire");
    let front_rtc = TestRtcControl::from_actor(&front).unwrap();
    let service_rtc = TestRtcControl::from_actor(&service).unwrap();
    let session_id = session.session_id().await;
    let initial = registry
        .session(&session_id)
        .await
        .expect("lease established");
    let front_channel = front_rtc.verified_channel(&service_id).await.unwrap();
    let service_channel = service_rtc.verified_channel(&front_id).await.unwrap();
    assert_eq!(initial.channel_id, front_channel.channel_id);
    assert_eq!(initial.channel_id, service_channel.channel_id);
    assert_eq!(
        initial.channel_transcript_digest,
        service_channel.transcript_digest
    );
    assert_ne!(wss_channel.as_ref(), Some(&initial.channel_id));
    for (rtc, peer) in [(&front_rtc, &service_id), (&service_rtc, &front_id)] {
        rtc.wait_state(peer, Duration::from_secs(10), |state| {
            state.sendable && state.network_path == expected_path
        })
        .await
        .expect("selected ICE candidate pair matches the test configuration");
    }

    tokio::time::timeout(Duration::from_secs(10), async {
        while session.lease_expiry_for_test().await <= initial.lease_expires_at_epoch_secs {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Front receives a successful heartbeat response extending the lease");
    let renewed = registry.session(&session_id).await.unwrap();
    assert!(renewed.lease_expires_at_epoch_secs > initial.lease_expires_at_epoch_secs);
    assert_eq!(renewed.channel_id, initial.channel_id);

    let payload = b"returned over the selected WebRTC wire".to_vec();
    assert_eq!(
        session
            .call("echo", READ, payload.clone(), Duration::from_secs(8))
            .await
            .unwrap(),
        payload
    );
    let explicit_request = EchoRequest {
        payload: payload.clone(),
    }
    .encode_to_vec();
    let explicit_response = session
        .call(
            EXPLICIT_ECHO,
            READ,
            explicit_request,
            Duration::from_secs(8),
        )
        .await
        .expect("explicit reply uses the ingress WebRTC wire");
    assert_eq!(
        EchoResponse::decode(explicit_response.as_slice())
            .unwrap()
            .payload,
        payload
    );

    if !turn_only {
        let generation = front_rtc
            .fail_peer(&service_id)
            .await
            .expect("retire the Front PC");
        service_rtc
            .fail_peer(&front_id)
            .await
            .expect("retire the Service PC");
        front_rtc
            .wait_state(&service_id, Duration::from_secs(25), |state| {
                state.pc_generation > generation && state.sendable
            })
            .await
            .expect("a new physical PeerConnection becomes sendable");
        session
            .resume()
            .await
            .expect("resume on the newly verified WebRTC generation");
        let resumed = registry.session(&session_id).await.unwrap();
        let replacement = front_rtc.verified_channel(&service_id).await.unwrap();
        assert!(replacement.channel_generation > front_channel.channel_generation);
        assert_eq!(resumed.channel_id, replacement.channel_id);
        assert_ne!(resumed.channel_id, initial.channel_id);
        assert!(resumed.lease_epoch > initial.lease_epoch);
        assert_eq!(
            session
                .call("echo", READ, payload.clone(), Duration::from_secs(8))
                .await
                .unwrap(),
            payload
        );
    }

    session.detach().await.expect("detach over WebRTC");
    assert_eq!(
        registry.status(&session_id).await.unwrap().state,
        actrium_runtime::SessionState::Closed
    );
    assert_eq!(
        server.relay_frame_count(),
        relay_frames,
        "session exchanges must not use WSS"
    );

    service.shutdown();
    front.shutdown();
    service.wait_for_shutdown().await;
    front.wait_for_shutdown().await;
    if let Some(server) = turn_server {
        turn::shutdown_turn_server(&server).await.unwrap();
    }
    server.shutdown().await;
}
