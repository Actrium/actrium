//! Actrium v1 session e2e: two real nodes, a mock relay minting Actor channel
//! certificates, verified-channel Attach, and a session-signed business call.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use actr_framework::{Bytes, Context, Dest, MessageDispatcher, WebRtcNetworkPath, Workload};
use actr_hyper::actrium::ActriumAttachParams;
use actr_hyper::test_support::TestSignalingServer;
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

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "echo" => Ok(envelope.payload.unwrap_or_default()),
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

async fn start_service_node(server: &TestSignalingServer, dir: &TempDir) -> (ActrRef, ActrId) {
    let service_type = service_type();
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let config = runtime_config(dir, server, service_type.clone());
    let registered = Node::from_hyper(hyper, config)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap();
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
    let front_type = front_type();
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let config = runtime_config(dir, server, front_type.clone());
    let registered = Node::from_hyper(hyper, config)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap();
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
async fn session_traffic_stays_on_verified_relay_with_sendable_direct_rtc() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.enable_actor_certificates();

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

    // Start peer negotiation without creating an Actrium session. The remote
    // Guard may reject this unsigned Tell; its only purpose is to materialize
    // both routes before Attach chooses the channel for its signed request.
    front
        .app_context()
        .await
        .tell_raw(
            &Dest::Peer(service_actor_id.clone()),
            "transport.probe".into(),
            PayloadType::RpcReliable,
            Bytes::new(),
        )
        .await
        .expect("start peer route negotiation");
    let mut rtc_status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(status) = front.subscribe_rtc_peer_status(&service_actor_id).await {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the direct RTC controller exists");
    tokio::time::timeout(Duration::from_secs(30), async move {
        loop {
            let snapshot = rtc_status.borrow_and_update().clone();
            if snapshot.sendable && snapshot.network_path == WebRtcNetworkPath::Direct {
                break;
            }
            rtc_status
                .changed()
                .await
                .expect("RTC status stays available");
        }
    })
    .await
    .expect("higher-priority direct RTC is sendable before Attach");

    // The peer handshake may still be in flight; Attach retries until the
    // verified Actor channel exists on both ends. Every attempt must stay on
    // that channel even though direct RTC can accept ordinary RPC bytes.
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
