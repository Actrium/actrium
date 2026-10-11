//! End-to-end proof that conflict-key dispatch concurrency (B2) lets a native
//! `Linked` workload run distinct-key invocations concurrently through the real
//! node mailbox path.
//!
//! The echo node enables `dispatch_concurrency` and declares a conflict key on
//! the request payload's field 1, so two requests with distinct payloads project
//! to distinct keys. Its handler blocks on a `tokio::sync::Barrier` sized for two
//! parties: the barrier only releases once *both* dispatches are simultaneously
//! in flight. If dispatch were serial (as it is gate-off), the first handler
//! would block forever waiting for the second, which would never be dispatched —
//! the caller would time out. Success therefore *is* the concurrency proof, with
//! no wall-clock sleeps.
//!
//! A companion test enables the gate but leaves the key undeclared, so both
//! requests project to the global `Serial` key and must NOT run concurrently —
//! the barrier is proven to stay unreleased while the first handler is in flight.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use actr_framework::{Bytes, Context, MessageDispatcher, Workload};
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{
    ActrRef, ConflictKeySpec, DispatchConcurrency, Hyper, HyperConfig, KeySource, Node,
    PayloadFieldKind, StaticTrust,
};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, ActrType, RpcEnvelope, RpcRequest};
use async_trait::async_trait;
use tempfile::TempDir;

/// Install a tracing subscriber so the caller→signaling→`admit_incoming`→
/// scheduler→handler trail is visible if a dispatch hangs. nextest runs each
/// test in its own process, so `try_init` is safe. The `EnvFilter` keeps the
/// noisy transport layers (tungstenite/webrtc) at `warn` while exposing the
/// dispatch path at `debug`; `RUST_LOG` overrides this when set. Without this
/// the test has no subscriber at all, so every `tracing` event is dropped and
/// a hang leaves nothing to diagnose (the failure mode behind issue #500).
fn init_tracing() {
    use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};
    let directive = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        "warn,actr_hyper::lifecycle=debug,actr_hyper::wire::webrtc=debug,actr_hyper::dispatch=debug,actr_hyper::outbound=debug".to_string()
    });
    let filter = EnvFilter::try_new(&directive).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_test_writer(),
        )
        .with(filter)
        .try_init();
}

/// Bound node teardown so a stuck shutdown cannot hang the whole test binary —
/// the failure mode behind issue #500's 5h+ CI run. `ActrRef::shutdown` is
/// non-blocking; the `wait_for_shutdown` is the part that can stall.
async fn bounded_shutdown(node: &ActrRef, label: &str) {
    node.shutdown();
    if tokio::time::timeout(std::time::Duration::from_secs(15), node.wait_for_shutdown())
        .await
        .is_err()
    {
        panic!("{label} did not shut down within 15s");
    }
}

const ROUTE: &str = "echo";

#[derive(Clone, PartialEq, ProstMessage)]
pub struct EchoRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, ProstMessage)]
pub struct EchoResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

impl RpcRequest for EchoRequest {
    type Response = EchoResponse;
    fn route_key() -> &'static str {
        ROUTE
    }
}

/// Workload whose dispatch waits on a shared barrier before echoing, so a caller
/// can observe whether two handlers are ever in flight at the same time.
struct BarrierWorkload {
    barrier: Arc<tokio::sync::Barrier>,
    entered: Arc<AtomicUsize>,
}

#[async_trait]
impl Workload for BarrierWorkload {
    type Dispatcher = BarrierDispatcher;
}

struct BarrierDispatcher;

#[async_trait]
impl MessageDispatcher for BarrierDispatcher {
    type Workload = BarrierWorkload;

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        workload.entered.fetch_add(1, Ordering::SeqCst);
        tracing::debug!(
            "barrier dispatch handler entered: route_key={}",
            envelope.route_key
        );
        // Blocks until the required number of parties are simultaneously here.
        workload.barrier.wait().await;
        Ok(envelope.payload.unwrap_or_default())
    }
}

/// Workload that hands the test a *release handle* on every dispatch and then
/// parks until the test releases it. This lets the test observe — without any
/// wall-clock sleeps and without ever leaving a permanently-stuck handler that
/// would hang shutdown — exactly how many handlers are simultaneously in
/// flight. Under serial dispatch the second handler is never even started while
/// the first is held open.
struct GatedWorkload {
    entry_tx: tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Sender<()>>,
    entered: Arc<AtomicUsize>,
}

#[async_trait]
impl Workload for GatedWorkload {
    type Dispatcher = GatedDispatcher;
}

struct GatedDispatcher;

#[async_trait]
impl MessageDispatcher for GatedDispatcher {
    type Workload = GatedWorkload;

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        _ctx: &C,
    ) -> actr_protocol::ActorResult<Bytes> {
        workload.entered.fetch_add(1, Ordering::SeqCst);
        tracing::debug!(
            "gated dispatch handler entered: route_key={}",
            envelope.route_key
        );
        // Publish a release handle to the test and park until it fires. If the
        // test dropped the receiver (teardown), we simply complete.
        let (rel_tx, rel_rx) = tokio::sync::oneshot::channel::<()>();
        let _ = workload.entry_tx.send(rel_tx);
        let _ = rel_rx.await;
        Ok(envelope.payload.unwrap_or_default())
    }
}

fn runtime_config(
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
            tracing_service_name: "dispatch-concurrency-e2e".to_string(),
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

fn echo_type() -> ActrType {
    ActrType {
        publisher: "test-publisher".to_string(),
        name: "BarrierEcho".to_string(),
        version: "1.0.0".to_string(),
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
    candidates
        .into_iter()
        .find(|c| c == expected)
        .expect("discovery returned expected actor")
}

/// Build the caller node and a barrier-gated echo node. `gate` toggles dispatch
/// concurrency; `declare_key` toggles whether the route has a conflict key
/// (needed for distinct-key concurrency — undeclared projects to Serial).
async fn setup(
    server: &TestSignalingServer,
    dir_a: &TempDir,
    dir_b: &TempDir,
    barrier: Arc<tokio::sync::Barrier>,
    entered: Arc<AtomicUsize>,
    gate: bool,
    declare_key: bool,
) -> (ActrRef, ActrRef, ActrId) {
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());

    // Echo (responder) node with the barrier workload.
    let mut echo_cfg = HyperConfig::new(dir_b.path(), Arc::new(StaticTrust::dev_only()));
    if gate {
        echo_cfg = echo_cfg.with_dispatch_concurrency(Some(DispatchConcurrency {
            enabled: true,
            budget: 8,
            queue_cap: 256,
            dispatch_timeout: None,
        }));
    }
    let hyper_b = Hyper::new(echo_cfg).await.unwrap();
    let cfg_b = runtime_config(dir_b, server, "EchoNode", echo_type());

    let mut node_b = Node::from_hyper(hyper_b, cfg_b);
    if declare_key {
        node_b = node_b.with_conflict_keys(
            ConflictKeySpec::builder()
                .method(
                    ROUTE,
                    KeySource::PayloadField {
                        tag: 1,
                        kind: PayloadFieldKind::Bytes,
                    },
                )
                .build()
                .unwrap(),
        );
    }
    let echo_actr = node_b
        .link(BarrierWorkload { barrier, entered })
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
    let cfg_a = runtime_config(dir_a, server, "CallerNode", caller_type);
    // The caller also links a barrier workload it never uses (it only makes
    // outbound calls); a fresh 1-party barrier keeps the type simple.
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(BarrierWorkload {
            barrier: Arc::new(tokio::sync::Barrier::new(1)),
            entered: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    (caller_actr, echo_actr, echo_id)
}

/// Build the caller + a gate-ON, key-UNDECLARED echo node whose handlers are
/// gated on a release channel (see `GatedWorkload`). The `entry_tx` is the echo
/// node's; the test uses the paired receiver to observe/release handlers.
async fn setup_serial(
    server: &TestSignalingServer,
    dir_a: &TempDir,
    dir_b: &TempDir,
    entry_tx: tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Sender<()>>,
    entered: Arc<AtomicUsize>,
) -> (ActrRef, ActrRef, ActrId) {
    let issuer = format!("http://127.0.0.1:{}/issuer", server.port());

    // Echo node: dispatch concurrency ON but NO conflict key declared, so every
    // request projects to the global `Serial` key.
    let echo_cfg = HyperConfig::new(dir_b.path(), Arc::new(StaticTrust::dev_only()))
        .with_dispatch_concurrency(Some(DispatchConcurrency {
            enabled: true,
            budget: 8,
            queue_cap: 256,
            dispatch_timeout: None,
        }));
    let hyper_b = Hyper::new(echo_cfg).await.unwrap();
    let cfg_b = runtime_config(dir_b, server, "EchoNode", echo_type());
    // NOTE: no `with_conflict_keys` → the route is undeclared → Serial.
    let echo_actr = Node::from_hyper(hyper_b, cfg_b)
        .link(GatedWorkload { entry_tx, entered })
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
    let echo_id = echo_actr.actor_id();

    // Caller node with a dummy gated workload it never exercises.
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
    let cfg_a = runtime_config(dir_a, server, "CallerNode", caller_type);
    let (dummy_tx, _dummy_rx) = tokio::sync::mpsc::unbounded_channel();
    let caller_actr = Node::from_hyper(hyper_a, cfg_a)
        .link(GatedWorkload {
            entry_tx: dummy_tx,
            entered: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .unwrap()
        .register(&issuer)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();

    (caller_actr, echo_actr, echo_id)
}

/// Gate ON + distinct keys declared → the two distinct-payload requests must run
/// concurrently. The 2-party barrier releases only if both handlers are in
/// flight together, so both calls returning is the concurrency proof.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn distinct_keys_dispatch_concurrently_end_to_end() {
    init_tracing();
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let entered = Arc::new(AtomicUsize::new(0));

    let (caller, echo, echo_id) = setup(
        &server,
        &dir_a,
        &dir_b,
        barrier.clone(),
        entered.clone(),
        true,
        true,
    )
    .await;

    let target = discover_required(&caller, &echo_type(), &echo_id).await;

    // Two concurrent calls with distinct payloads → distinct conflict keys.
    let c1 = {
        let caller = caller.clone();
        let target = target.clone();
        tokio::spawn(async move {
            caller
                .call_remote::<EchoRequest>(
                    target,
                    EchoRequest {
                        payload: b"room-A".to_vec(),
                    },
                    30_000,
                )
                .await
        })
    };
    let c2 = {
        let caller = caller.clone();
        let target = target.clone();
        tokio::spawn(async move {
            caller
                .call_remote::<EchoRequest>(
                    target,
                    EchoRequest {
                        payload: b"room-B".to_vec(),
                    },
                    30_000,
                )
                .await
        })
    };

    // If dispatch were serial, the first handler would block on the 2-party
    // barrier forever → this timeout would fire. Success = concurrency.
    let (r1, r2) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        (c1.await.unwrap(), c2.await.unwrap())
    })
    .await
    .expect("both concurrent dispatches must complete (proves concurrency)");

    assert_eq!(r1.expect("call A ok").payload, b"room-A");
    assert_eq!(r2.expect("call B ok").payload, b"room-B");
    assert_eq!(
        entered.load(Ordering::SeqCst),
        2,
        "both handlers must have entered"
    );

    bounded_shutdown(&caller, "caller").await;
    bounded_shutdown(&echo, "echo").await;
    tokio::time::timeout(std::time::Duration::from_secs(15), server.shutdown())
        .await
        .expect("signaling server did not shut down within 15s");
}

/// Gate ON but NO key declared → both requests project to the global `Serial`
/// key and must NOT overlap. This is the *falsifiable* form of that claim: we
/// fire **two** concurrent undeclared requests and prove the second handler is
/// never started while the first is still in flight. A regression that let the
/// undeclared route dispatch concurrently would surface a second handler here
/// and fail the test.
///
/// No wall-clock sleeps: handlers are held open via a per-invocation release
/// channel, so the test drives completion order directly. The one bounded
/// observation window (`OBSERVE`) is the only sound way to assert the *negative*
/// — "no second handler appeared while the first was in flight" — and stands in
/// for (and forbids) sleep-based coordination.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn undeclared_route_stays_serial_end_to_end() {
    init_tracing();
    let mut server = TestSignalingServer::start().await.expect("server start");
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let (entry_tx, mut entry_rx) =
        tokio::sync::mpsc::unbounded_channel::<tokio::sync::oneshot::Sender<()>>();
    let entered = Arc::new(AtomicUsize::new(0));

    let (caller, echo, echo_id) =
        setup_serial(&server, &dir_a, &dir_b, entry_tx, entered.clone()).await;

    let target = discover_required(&caller, &echo_type(), &echo_id).await;

    // Two concurrent undeclared requests with distinct payloads. Distinct
    // payloads would project to distinct keys *if a key were declared* — but it
    // is not, so both must land on the global Serial key.
    let c1 = {
        let caller = caller.clone();
        let target = target.clone();
        tokio::spawn(async move {
            caller
                .call_remote::<EchoRequest>(
                    target,
                    EchoRequest {
                        payload: b"room-A".to_vec(),
                    },
                    30_000,
                )
                .await
        })
    };
    let c2 = {
        let caller = caller.clone();
        let target = target.clone();
        tokio::spawn(async move {
            caller
                .call_remote::<EchoRequest>(
                    target,
                    EchoRequest {
                        payload: b"room-B".to_vec(),
                    },
                    30_000,
                )
                .await
        })
    };

    // The first handler enters and parks on its release channel. This `recv`
    // is the point that hung unbounded in CI (issue #500): if the first
    // dispatch never reached the handler — e.g. a session race dropped the
    // envelope before `admit_incoming`, or the scheduler never started the
    // job — this would block forever and nextest had no per-test cap to kill
    // it. Bound it so a stuck dispatch fails loudly with the in-flight calls'
    // status instead of stalling the runner, and so the tracing trail is
    // captured rather than lost to a 5h+ timeout.
    let rel1 = match tokio::time::timeout(std::time::Duration::from_secs(30), entry_rx.recv()).await
    {
        Ok(Some(rel)) => rel,
        Ok(None) => panic!(
            "entry channel closed before the first handler published its release \
             handle (entered_count={}); the echo node dispatch path exited \
             without entering the handler — see tracing output for the \
             '📨 Handling incoming' / scheduler / 'dispatch handler entered' trail",
            entered.load(Ordering::SeqCst)
        ),
        Err(_) => {
            // First handler never entered within 30s. Probe the in-flight
            // calls' status (non-blocking `is_finished`, so the handles stay
            // usable on the success path): a finished call means it already
            // resolved — almost certainly with an error, since the handler
            // never entered — pointing at a session/transport race; a pending
            // call means the envelope produced no reply at all. The actual
            // error (TimedOut / ConnectionNotReady / Unavailable) is in the
            // tracing output emitted by the caller's `PeerGate`.
            let entered_now = entered.load(Ordering::SeqCst);
            let c1_status = if c1.is_finished() {
                "finished"
            } else {
                "pending"
            };
            let c2_status = if c2.is_finished() {
                "finished"
            } else {
                "pending"
            };
            panic!(
                "first dispatch handler never entered within 30s \
                 (entered_count={entered_now}); call status: c1={c1_status}, \
                 c2={c2_status}. Inspect the tracing output for the '📨 Handling \
                 incoming' → 'admitting inbound RPC to dispatch scheduler' → \
                 'dispatch scheduler started job' → 'dispatch handler entered' \
                 trail to locate where the request stalled."
            );
        }
    };

    // While the first handler is held in flight, a serial runner must NOT have
    // started the second one. A concurrent dispatch would surface a second entry
    // within this window; its absence is the serial proof.
    const OBSERVE: std::time::Duration = std::time::Duration::from_secs(2);
    match tokio::time::timeout(OBSERVE, entry_rx.recv()).await {
        Ok(Some(rel2)) => {
            // Concurrency leak: release both handlers so teardown is clean, then
            // fail loudly.
            let _ = rel1.send(());
            let _ = rel2.send(());
            panic!(
                "undeclared route dispatched a second handler while the first was \
                 in flight; gate-on + undeclared must stay fully serial"
            );
        }
        Ok(None) => panic!("entry channel closed before the serial proof completed"),
        Err(_) => { /* timed out: no second handler while the first was held → serial */ }
    }
    assert_eq!(
        entered.load(Ordering::SeqCst),
        1,
        "exactly one handler may be in flight under serial dispatch"
    );

    // Release the first handler; the runner may now start the second.
    let _ = rel1.send(());
    let rel2 = tokio::time::timeout(std::time::Duration::from_secs(30), entry_rx.recv())
        .await
        .expect("second handler must start once the first completes")
        .expect("entry channel still open");
    let _ = rel2.send(());

    // Both calls complete correctly, one after the other.
    let (r1, r2) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        (c1.await.unwrap(), c2.await.unwrap())
    })
    .await
    .expect("both calls complete after serial dispatch");
    let mut got = [
        r1.expect("call A ok").payload,
        r2.expect("call B ok").payload,
    ];
    got.sort();
    assert_eq!(
        got,
        [b"room-A".to_vec(), b"room-B".to_vec()],
        "both distinct payloads must be echoed back"
    );
    assert_eq!(
        entered.load(Ordering::SeqCst),
        2,
        "both handlers must eventually have run"
    );

    bounded_shutdown(&caller, "caller").await;
    bounded_shutdown(&echo, "echo").await;
    tokio::time::timeout(std::time::Duration::from_secs(15), server.shutdown())
        .await
        .expect("signaling server did not shut down within 15s");
}
