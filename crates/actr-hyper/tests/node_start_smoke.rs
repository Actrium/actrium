//! Smoke test: start a fully-linked Node against a mock signaling/Issuer server.
//!
//! Drives the `Node::from_hyper → link → register → start` typestate chain,
//! which is the only path that exercises `lifecycle::node::Inner::start` (the
//! ~750-line async runtime bring-up). Existing integration tests bypass Node
//! and construct coordinators directly, so `start` was previously uncovered.

use std::collections::HashMap;
use std::sync::Arc;

use actr_framework::{Bytes, Context, Dest, MessageDispatcher, Workload};
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{ActrRef, Hyper, HyperConfig, Node, StaticTrust};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrError, ActrId, ActrType, PayloadType, RpcEnvelope, RpcRequest};
use async_trait::async_trait;
use tempfile::TempDir;

/// Minimal typed RPC request used for cross-node calls.
#[derive(Clone, PartialEq, ProstMessage)]
pub struct EchoRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, ProstMessage)]
pub struct SmokeEchoResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

impl RpcRequest for EchoRequest {
    type Response = SmokeEchoResponse;
    fn route_key() -> &'static str {
        "echo"
    }
}

/// Request whose route_key no workload handles — drives the inbound dispatch
/// error path (workload returns InvalidArgument → propagated to caller).
#[derive(Clone, PartialEq, ProstMessage)]
pub struct UnknownRouteRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

impl RpcRequest for UnknownRouteRequest {
    type Response = SmokeEchoResponse;
    fn route_key() -> &'static str {
        "no/such/route"
    }
}

/// Minimal linked workload whose dispatcher echoes the payload back.
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
    name: &str,
) -> actr_config::RuntimeConfig {
    runtime_config_with_type(
        dir,
        server,
        name,
        ActrType {
            publisher: "test-publisher".to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
        },
    )
}

fn runtime_config_with_type(
    dir: &TempDir,
    server: &TestSignalingServer,
    name: &str,
    actr_type: ActrType,
) -> actr_config::RuntimeConfig {
    actr_config::RuntimeConfig {
        package: actr_config::PackageInfo {
            name: name.to_string(),
            actr_type,
            description: None,
            authors: vec![],
            license: None,
        },
        signaling_url: url::Url::parse(&server.url()).unwrap(),
        realm: "test7@example.com".to_string(),
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
            tracing_service_name: "node-start-smoke".to_string(),
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

async fn discover_required(caller: &ActrRef, target_type: &ActrType, expected: &ActrId) -> ActrId {
    let candidates = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        caller.discover_route_candidates(target_type, 1),
    )
    .await
    .expect("discover must not hang")
    .expect("discover must not error");

    assert!(
        candidates.iter().any(|candidate| candidate == expected),
        "discovery did not return expected actor {expected:?}; candidates: {candidates:?}"
    );

    candidates
        .into_iter()
        .next()
        .expect("discovery returned no candidates")
}

async fn assert_echo_rpc(caller: &ActrRef, target: ActrId, payload: &[u8]) {
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        caller.call_remote::<EchoRequest>(
            target,
            EchoRequest {
                payload: payload.to_vec(),
            },
            30_000,
        ),
    )
    .await
    .expect("echo RPC must not hang")
    .expect("echo RPC must succeed");

    assert_eq!(response.payload, payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_starts_and_shuts_down_cleanly() {
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let node = Node::from_hyper(hyper, runtime_config(&dir, &server, "SmokeNode"))
        .link(EchoWorkload)
        .await
        .expect("link should attach the workload");

    // register hits the mock Issuer (MockLoranServer serves /issuer/register).
    let mut registered = node
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .expect("register should succeed against mock Issuer");
    let _network_events = registered.create_gated_network_event_handle(25);

    let actr = registered.start().await.expect("node should start");
    // Node is live: it has an actor id and is not yet shutting down.
    let _id = actr.actor_id();
    assert!(!actr.is_shutting_down());

    // Graceful shutdown.
    actr.shutdown();
    actr.wait_for_shutdown().await;

    server.shutdown().await;
}

#[tokio::test]
async fn attached_node_exposes_gated_network_event_constructor() {
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let mut node = Node::from_hyper(hyper, runtime_config(&dir, &server, "GatedAttached"))
        .link(EchoWorkload)
        .await
        .expect("link should attach the workload");

    let handle = node.create_gated_network_event_handle(25);
    handle.notify_session_activated(3);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_starts_without_peer_listener_configuration() {
    // Peer data channels require only outbound connections.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let cfg = runtime_config(&dir, &server, "OutboundOnly");

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let registered = Node::from_hyper(hyper, cfg)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .expect("register should succeed");

    let actr = registered.start().await.expect("node should start");
    assert!(!actr.is_shutting_down());

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn register_against_unreachable_issuer_errors() {
    // A garbage Issuer endpoint must surface a registration error rather than
    // hanging or starting the node.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let node = Node::from_hyper(hyper, runtime_config(&dir, &server, "IssuerFail"))
        .link(EchoWorkload)
        .await
        .unwrap();

    // Point Issuer at a closed port → register must fail.
    let result = node.register("http://127.0.0.1:1/issuer").await;
    assert!(
        result.is_err(),
        "register against an unreachable Issuer must error"
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_runs_background_tasks_before_shutdown() {
    // Start the node, let background tasks (signaling heartbeat, mailbox poll,
    // coordinator idle) run briefly, then shut down. This exercises the
    // long-running spawned tasks inside `start` rather than only the bring-up.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "BgRun"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .expect("register")
        .start()
        .await
        .expect("start");

    // Let the runtime settle so spawned background loops make progress.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!actr.is_shutting_down());

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn double_shutdown_is_idempotent() {
    // Calling shutdown twice and then waiting must not panic or hang.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "DoubleStop"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    actr.shutdown();
    actr.shutdown(); // idempotent
    actr.wait_for_shutdown().await;
    assert!(actr.is_shutting_down());
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn discover_unknown_type_returns_no_candidates() {
    // discover_route_candidates against a type no peer registered must return
    // an empty list (or error) without hanging — exercises the signaling
    // route-candidates request path inside the running node.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "Discover"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    let unknown_type = ActrType {
        publisher: "nobody".to_string(),
        name: "NoSuchActor".to_string(),
        version: "0.0.0".to_string(),
    };
    let err = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        actr.discover_route_candidates(&unknown_type, 1),
    )
    .await
    .expect("discover must not hang")
    .expect_err("unknown type should not return route candidates");
    assert!(
        matches!(err, ActrError::NotFound(_)),
        "unknown type should map to NotFound, got {err:?}"
    );

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_connect_and_exchange_rpc() {
    // Two linked nodes on the same signaling server: the caller discovers the
    // echo node by ActrType and issues a typed RPC. This drives coordinator
    // connection establishment, signaling route-candidates, and the inbound
    // handle_incoming dispatch path — the largest remaining uncovered surface.
    let mut server = TestSignalingServer::start().await.expect("server start");

    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let echo_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "EchoService".to_string(),
        version: "1.0.0".to_string(),
    };

    // Echo node (responder).
    let hyper_b = Hyper::new(HyperConfig::new(
        dir_b.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_b = runtime_config_with_type(&dir_b, &server, "EchoNode", echo_type.clone());
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    // Caller node.
    let hyper_a = Hyper::new(HyperConfig::new(
        dir_a.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let caller_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "Caller".to_string(),
        version: "1.0.0".to_string(),
    };
    let cfg_a = runtime_config_with_type(&dir_a, &server, "CallerNode", caller_type);
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    let target = discover_required(&caller_actr, &echo_type, &echo_id).await;
    assert_echo_rpc(&caller_actr, target, b"ping").await;

    caller_actr.shutdown();
    caller_actr.wait_for_shutdown().await;
    echo_actr.shutdown();
    echo_actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_rpc_unknown_route_returns_error() {
    // A call to a route_key no workload handles must propagate an error back
    // to the caller — exercises the inbound dispatch error path.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let echo_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "EchoService".to_string(),
        version: "1.0.0".to_string(),
    };

    let hyper_b = Hyper::new(HyperConfig::new(
        dir_b.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_b = runtime_config_with_type(&dir_b, &server, "EchoNode", echo_type.clone());
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    let hyper_a = Hyper::new(HyperConfig::new(
        dir_a.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let caller_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "Caller2".to_string(),
        version: "1.0.0".to_string(),
    };
    let cfg_a = runtime_config_with_type(&dir_a, &server, "CallerNode2", caller_type);
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    // First discover to warm the connection, then issue the unknown-route call.
    let target = discover_required(&caller_actr, &echo_type, &echo_id).await;

    let err = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        caller_actr.call_remote::<UnknownRouteRequest>(
            target,
            UnknownRouteRequest::default(),
            30_000,
        ),
    )
    .await
    .expect("unknown-route RPC must not hang")
    .expect_err("unknown route should propagate an error");
    assert!(
        err.to_string().contains("unknown route"),
        "unexpected unknown-route error: {err:?}"
    );

    caller_actr.shutdown();
    caller_actr.wait_for_shutdown().await;
    echo_actr.shutdown();
    echo_actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_connect_over_webrtc() {
    // This fixture has no relay service, so the caller must use WebRTC.
    // Exercise the coordinator's offer/answer/ICE connection path.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let echo_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "EchoWebrtc".to_string(),
        version: "1.0.0".to_string(),
    };

    // Echo node on a WebRTC-only fixture.
    let hyper_b = Hyper::new(HyperConfig::new(
        dir_b.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_b = runtime_config_with_type(&dir_b, &server, "EchoWebrtcNode", echo_type.clone());
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    let hyper_a = Hyper::new(HyperConfig::new(
        dir_a.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let caller_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "CallerWebrtc".to_string(),
        version: "1.0.0".to_string(),
    };
    let cfg_a = runtime_config_with_type(&dir_a, &server, "CallerWebrtcNode", caller_type);
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    let target = discover_required(&caller_actr, &echo_type, &echo_id).await;
    assert_echo_rpc(&caller_actr, target, b"hi").await;

    caller_actr.shutdown();
    caller_actr.wait_for_shutdown().await;
    echo_actr.shutdown();
    echo_actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_remains_running_during_signaling_pause_and_resume_smoke() {
    // Pause then resume signaling forwarding while the node runs. This is a
    // bounded smoke test for the signaling disruption path; it does not assert
    // detailed reconnect state transitions.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "Reconnect"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    // Let the node connect to signaling, then disrupt forwarding and restore.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    server.pause_forwarding();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    server.resume_forwarding();
    // Give background signaling logic time to run.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(!actr.is_shutting_down());

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn headless_node_reconnects_after_signaling_endpoint_restart() {
    let mut server = TestSignalingServer::start().await.expect("server start");
    let signaling_port = server.port();
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{signaling_port}/issuer");
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "HeadlessReconnect"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    assert_eq!(
        server.get_connection_count(),
        1,
        "node startup should establish the initial signaling socket"
    );
    server.shutdown().await;

    let restart_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    server = loop {
        match TestSignalingServer::start_on_port(signaling_port).await {
            Ok(server) => break server,
            Err(error) => {
                assert!(
                    tokio::time::Instant::now() < restart_deadline,
                    "signaling endpoint did not release its original port: {error}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
    };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while server.get_connection_count() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "headless node did not reconnect through its always-on supervisor"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(!actr.is_shutting_down());
    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_handles_signaling_blackhole_io_without_panic_smoke() {
    // Blackhole WebSocket I/O (drop packets without closing) to exercise the
    // heartbeat-timeout disruption path as a bounded smoke test.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "Blackhole"))
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    server.blackhole_websocket_io();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    server.restore_websocket_io();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_drop_ice_candidates_returns_bounded_rpc_result() {
    // Dropping ICE candidates between two connected nodes disrupts connectivity;
    // this test asserts the RPC resolves with either success or a transport-class
    // error instead of hanging.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let echo_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "EchoIce".to_string(),
        version: "1.0.0".to_string(),
    };

    let hyper_b = Hyper::new(HyperConfig::new(
        dir_b.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_b = runtime_config_with_type(&dir_b, &server, "EchoIceNode", echo_type.clone());
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    let hyper_a = Hyper::new(HyperConfig::new(
        dir_a.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_a = runtime_config_with_type(
        &dir_a,
        &server,
        "CallerIceNode",
        ActrType {
            publisher: "test-publisher".into(),
            name: "CallerIce".into(),
            version: "1.0.0".into(),
        },
    );
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    // Drop ICE candidates to disrupt connectivity, then attempt a bounded call.
    server.drop_next_ice_candidates(3);
    let target = discover_required(&caller_actr, &echo_type, &echo_id).await;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        caller_actr.call_remote::<EchoRequest>(
            target.clone(),
            EchoRequest {
                payload: b"x".to_vec(),
            },
            30_000,
        ),
    )
    .await
    .expect("ICE-disrupted call must resolve instead of hanging");
    match result {
        Ok(response) => assert_eq!(response.payload, b"x"),
        Err(err) => assert!(
            matches!(
                err,
                ActrError::ConnectionNotReady(_) | ActrError::Unavailable(_) | ActrError::TimedOut
            ),
            "ICE disruption should return a transport-class error, got {err:?}"
        ),
    }

    caller_actr.shutdown();
    caller_actr.wait_for_shutdown().await;
    echo_actr.shutdown();
    echo_actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_registers_with_a_realm_client_key() {
    // Setting realm_client_key exercises the linked-registration auth path
    // that loads the key file and signs the x-actr-client-proof header, a
    // distinct branch from the default linked register flow.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let mut cfg = runtime_config(&dir, &server, "RealmClientKeyNode");
    let key_path = dir.path().join("realm-client.json");
    let key_file = actr_protocol::realm_membership::RealmClientKeyFile::new(
        &cfg.realm,
        "127.0.0.1",
        &cfg.issuer_endpoint,
        &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
    );
    std::fs::write(&key_path, serde_json::to_string(&key_file).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    cfg.realm_client_key = Some(key_path);

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, cfg)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .expect("register with a Realm client key should succeed")
        .start()
        .await
        .expect("start");

    // Let the node run briefly so background tasks progress.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_run_with_acl_and_scripts_config() {
    // Non-default ACL and scripts config exercises the runtime config plumbing.
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let mut cfg = runtime_config(&dir, &server, "AclNode");
    cfg.acl = Some(actr_protocol::Acl { rules: vec![] });
    cfg.scripts
        .insert("greet".to_string(), "echo hi".to_string());

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, cfg)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

/// A second linked workload used purely as a host-side hook observer.
struct NoopObserver;

#[async_trait]
impl Workload for NoopObserver {
    type Dispatcher = NoopDispatcher;
}

struct NoopDispatcher;

#[async_trait]
impl MessageDispatcher for NoopDispatcher {
    type Workload = NoopObserver;
    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        _envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        Err(actr_protocol::ActrError::NotImplemented("observer".into()))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linked_node_accepts_extra_hook_observer() {
    // `with_hook_observer` chains a second observer onto an attached node —
    // exercises Node<Attached>::with_hook_observer (lib.rs L842-856).
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir = TempDir::new().unwrap();

    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();

    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let actr = Node::from_hyper(hyper, runtime_config(&dir, &server, "ObsNode"))
        .link(EchoWorkload)
        .await
        .unwrap()
        // Chain a second (no-op) host observer alongside the linked observer.
        .with_hook_observer(NoopObserver)
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    actr.shutdown();
    actr.wait_for_shutdown().await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn peer_disconnect_triggers_coordinator_cleanup() {
    // Two nodes connect, then the echo node shuts down. The caller observes
    // the peer going away, driving the coordinator's peer-disconnect cleanup
    // path (close peer, remove session, emit disconnected hook).
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let echo_type = ActrType {
        publisher: "test-publisher".to_string(),
        name: "EchoDisconnect".to_string(),
        version: "1.0.0".to_string(),
    };

    let hyper_b = Hyper::new(HyperConfig::new(
        dir_b.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_b = runtime_config_with_type(&dir_b, &server, "EchoDiscNode", echo_type.clone());
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    let hyper_a = Hyper::new(HyperConfig::new(
        dir_a.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let cfg_a = runtime_config_with_type(
        &dir_a,
        &server,
        "CallerDiscNode",
        ActrType {
            publisher: "test-publisher".into(),
            name: "CallerDisc".into(),
            version: "1.0.0".into(),
        },
    );
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(EchoWorkload)
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    // Establish a connection via discovery + a call.
    let target = discover_required(&caller_actr, &echo_type, &echo_id).await;
    assert_echo_rpc(&caller_actr, target.clone(), b"hi").await;

    // Now shut down the echo peer — caller should detect the disconnect and
    // clean up its side of the connection.
    echo_actr.shutdown();
    echo_actr.wait_for_shutdown().await;

    // Give the caller time to observe the peer loss and run cleanup.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let context = caller_actr.app_context().await;
    let err = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        context.call_raw(
            &Dest::Peer(target),
            EchoRequest::route_key().to_string(),
            PayloadType::RpcReliable,
            EchoRequest {
                payload: b"after-disconnect".to_vec(),
            }
            .encode_to_vec()
            .into(),
            1_000,
        ),
    )
    .await
    .expect("post-disconnect call must resolve")
    .expect_err("call to a shut down peer should fail");
    assert!(
        matches!(
            err,
            ActrError::ConnectionNotReady(_)
                | ActrError::Unavailable(_)
                | ActrError::TimedOut
                | ActrError::DeliveryUncertain { .. }
        ),
        "post-disconnect call should return a transport-class error, got {err:?}"
    );

    caller_actr.shutdown();
    caller_actr.wait_for_shutdown().await;
    server.shutdown().await;
}

struct RelayObserver(tokio::sync::mpsc::UnboundedSender<ActrId>);
struct RelayObserverDispatcher;

#[async_trait]
impl Workload for RelayObserver {
    type Dispatcher = RelayObserverDispatcher;
    async fn on_websocket_connected<C: Context>(
        &self,
        _ctx: &C,
        event: &actr_framework::PeerEvent,
    ) {
        let _ = self.0.send(event.peer.clone());
    }
}
#[async_trait]
impl MessageDispatcher for RelayObserverDispatcher {
    type Workload = RelayObserver;
    async fn dispatch<C: Context>(
        _workload: &RelayObserver,
        _envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        Ok(Bytes::new())
    }
}

async fn relay_node(
    dir: &TempDir,
    server: &TestSignalingServer,
    name: &str,
    ready: tokio::sync::mpsc::UnboundedSender<ActrId>,
) -> ActrRef {
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    Node::from_hyper(hyper, runtime_config(dir, server, name))
        .link(EchoWorkload)
        .await
        .unwrap()
        .with_hook_observer(RelayObserver(ready))
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap()
        .start()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relay_only_rpc_streams_and_warm_reconnection_need_no_peer_listener() {
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    // Registration and pings continue, but no offer/answer/ICE can reach a peer.
    server.pause_forwarding();
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    let (ready_a, mut events_a) = tokio::sync::mpsc::unbounded_channel();
    let (ready_b, mut events_b) = tokio::sync::mpsc::unbounded_channel();
    let a = relay_node(&dir_a, &server, "RelayA", ready_a).await;
    let wait = std::time::Duration::from_secs(15);
    tokio::time::timeout(wait, async {
        while server.relay_connection_count() != 1 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("first relay socket authenticates");
    // The second certificate must expire later than the first node's cached
    // issuer key lease, as happens when peers start at different times.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let b = relay_node(&dir_b, &server, "RelayB", ready_b).await;
    tokio::time::timeout(wait, async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both outbound relay sockets authenticate");
    assert_echo_rpc(&a, b.actor_id(), b"relay request and response").await;
    assert_echo_rpc(&b, a.actor_id(), b"reverse RPC").await;
    assert_eq!(
        tokio::time::timeout(wait, events_a.recv())
            .await
            .unwrap()
            .unwrap(),
        b.actor_id()
    );
    assert_eq!(
        tokio::time::timeout(wait, events_b.recv())
            .await
            .unwrap()
            .unwrap(),
        a.actor_id()
    );

    let ctx_a = a.app_context().await;
    let ctx_b = b.app_context().await;
    let (chunks, mut received) = tokio::sync::mpsc::unbounded_channel();
    for ctx in [ctx_a.clone(), ctx_b.clone()] {
        let chunks = chunks.clone();
        let mut stream = ctx
            .data_chunks("relay-stream".into(), Default::default())
            .await
            .unwrap();
        tokio::spawn(async move {
            use futures_util::StreamExt as _;
            while let Some(event) = stream.next().await {
                let event = event.unwrap();
                chunks.send((event.chunk, event.sender)).unwrap();
            }
        });
    }
    for (ctx, target, source) in [
        (&ctx_a, b.actor_id(), a.actor_id()),
        (&ctx_b, a.actor_id(), b.actor_id()),
    ] {
        for (sequence, lane) in [
            (1, PayloadType::StreamReliable),
            (2, PayloadType::StreamLatencyFirst),
        ] {
            let payload = Bytes::from(vec![sequence as u8; 64 * 1024]);
            let mut sink = ctx
                .data_chunk_sink(&Dest::Peer(target.clone()), "relay-stream".into(), lane)
                .await
                .unwrap();
            futures_util::SinkExt::send(
                &mut sink,
                actr_protocol::DataChunk {
                    stream_id: "relay-stream".into(),
                    sequence,
                    payload: payload.clone(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let _ = sink.abort().await;
            let (chunk, sender) = tokio::time::timeout(wait, received.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(sender, source);
            assert_eq!(chunk.payload, payload);
            assert_eq!(chunk.sequence, sequence);
        }
    }

    // Unexpired certified keys survive a transient AIS outage. Reconnection
    // still uses new Noise ephemeral keys and requires mutual confirmation.
    server.pause_relay_enrollment();
    server.disconnect_relay_connections();
    // Wait for actual peer-acknowledged replacement sessions, not socket OPEN.
    tokio::time::timeout(wait, events_a.recv())
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(wait, events_b.recv())
        .await
        .unwrap()
        .unwrap();
    assert_echo_rpc(&a, b.actor_id(), b"after relay reconnect").await;
    assert_echo_rpc(&b, a.actor_id(), b"reverse after reconnect").await;
    for node in [&a, &b] {
        node.shutdown();
        tokio::time::timeout(wait, node.wait_for_shutdown())
            .await
            .unwrap();
    }
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relay_discovery_recovers_after_being_enabled_without_restarting_nodes() {
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.set_relay_discovery_enabled(false);
    server.pause_forwarding();
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    let (ready_a, _) = tokio::sync::mpsc::unbounded_channel();
    let (ready_b, _) = tokio::sync::mpsc::unbounded_channel();
    let a = relay_node(&dir_a, &server, "DiscoveryA", ready_a).await;
    let b = relay_node(&dir_b, &server, "DiscoveryB", ready_b).await;
    let deadline = std::time::Duration::from_secs(15);
    tokio::time::timeout(deadline, async {
        while server.relay_discovery_requests() < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both nodes must discover the disabled relay");
    assert_eq!(server.relay_connection_count(), 0);
    server.set_relay_discovery_enabled(true);
    tokio::time::timeout(deadline, async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both nodes must reconnect after the relay is enabled");
    assert_echo_rpc(&a, b.actor_id(), b"relay became available").await;

    let requests = server.relay_discovery_requests();
    server.set_relay_discovery_enabled(false);
    server.disconnect_relay_connections();
    tokio::time::timeout(deadline, async {
        while server.relay_connection_count() != 0
            || server.relay_discovery_requests() < requests + 2
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("nodes must observe the disabled relay after disconnecting");
    server.set_relay_discovery_enabled(true);
    tokio::time::timeout(deadline, async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("nodes must rediscover the re-enabled relay");
    assert_echo_rpc(&b, a.actor_id(), b"relay available again").await;
    for node in [&a, &b] {
        node.shutdown();
        tokio::time::timeout(deadline, node.wait_for_shutdown())
            .await
            .unwrap();
    }
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relay_upgrades_to_real_direct_rtc_with_ordered_streams_and_survives_relay_loss() {
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.delay_ice_candidates_for(std::time::Duration::from_secs(2));
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    let (ready_a, _events_a) = tokio::sync::mpsc::unbounded_channel();
    let (ready_b, _events_b) = tokio::sync::mpsc::unbounded_channel();
    let a = relay_node(&dir_a, &server, "UpgradeA", ready_a).await;
    let b = relay_node(&dir_b, &server, "UpgradeB", ready_b).await;
    let deadline = std::time::Duration::from_secs(15);
    tokio::time::timeout(deadline, async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let before = server.relay_frame_count();
    assert_echo_rpc(&a, b.actor_id(), b"relay wins while ICE is delayed").await;
    assert!(server.relay_frame_count() > before);
    let ctx = a.app_context().await;
    let (chunks, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut upgrade_stream = b
        .app_context()
        .await
        .data_chunks("upgrade-stream".into(), Default::default())
        .await
        .unwrap();
    tokio::spawn(async move {
        use futures_util::StreamExt as _;
        while let Some(event) = upgrade_stream.next().await {
            let Ok(event) = event else { break };
            chunks.send(event.chunk.sequence).unwrap();
        }
    });
    let mut upgrade_sink = ctx
        .data_chunk_sink(
            &Dest::Peer(b.actor_id()),
            "upgrade-stream".into(),
            PayloadType::StreamReliable,
        )
        .await
        .unwrap();
    futures_util::SinkExt::send(
        &mut upgrade_sink,
        actr_protocol::DataChunk {
            stream_id: "upgrade-stream".into(),
            sequence: 0,
            payload: Bytes::from_static(b"old relay route"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(deadline, async {
        for (node, peer) in [(&a, b.actor_id()), (&b, a.actor_id())] {
            loop {
                if node.get_rtc_peer_status(&peer).await.is_some_and(|state| {
                    state.sendable
                        && state.network_path == actr_framework::WebRtcNetworkPath::Direct
                }) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("both real RTC selected pairs become direct");
    for sequence in 1..32 {
        futures_util::SinkExt::send(
            &mut upgrade_sink,
            actr_protocol::DataChunk {
                stream_id: "upgrade-stream".into(),
                sequence,
                payload: Bytes::from(vec![sequence as u8; 64 * 1024]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let _ = upgrade_sink.abort().await;
    for sequence in 0..32 {
        assert_eq!(
            tokio::time::timeout(deadline, received.recv())
                .await
                .unwrap(),
            Some(sequence)
        );
    }
    // Count actual server frames to prove established RPCs use direct RTC.
    let frames = server.relay_frame_count();
    assert_echo_rpc(&a, b.actor_id(), b"direct RPC").await;
    assert_echo_rpc(&b, a.actor_id(), b"reverse direct RPC").await;
    assert_eq!(server.relay_frame_count(), frames);
    server.disconnect_relay_connections();
    assert_echo_rpc(&a, b.actor_id(), b"direct survives relay loss").await;
    for node in [&a, &b] {
        node.shutdown();
        tokio::time::timeout(deadline, node.wait_for_shutdown())
            .await
            .unwrap();
    }
    server.shutdown().await;
}
