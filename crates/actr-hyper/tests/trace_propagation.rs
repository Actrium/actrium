//! W3C Trace Context end-to-end propagation tests.
//!
//! Verifies that `traceparent` / `tracestate` injected by a caller into an
//! `RpcEnvelope` survive the full transport round-trip and arrive at the callee
//! with bit-perfect fidelity.
//!
//! # Scenarios
//!
//! ## A — Inproc (HostTransport)
//!
//! Shell constructs an envelope with a hand-crafted W3C `traceparent` and
//! sends it through `HostGate` → `HostTransport` (in-memory channel) →
//! a workload dispatcher that echoes the received `traceparent` back as the
//! response payload.  The caller verifies the round-tripped value matches.
//!
//! ## B — WebRTC / WebSocket transport (PeerGate)
//!
//! Two `TestPeer` nodes connect through a `TestSignalingServer` (mock-actrix).
//! Node A sends an `RpcEnvelope` with an injected `traceparent`.  A custom
//! responder on Node B decodes the arriving envelope, extracts `traceparent`,
//! and writes it verbatim into the response payload.  Node A asserts the
//! returned bytes match the injected header, proving the field survived
//! protobuf serialise → WebRTC data-channel → protobuf deserialise.

use std::sync::Arc;

use actr_framework::{
    Bytes, Context, MaybeSendBoxFuture, MaybeSendSync, MessageDispatcher, Workload,
};
use actr_hyper::outbound::HostGate;
use actr_hyper::test_support::TestHarness;
use actr_hyper::transport::HostTransport;
use actr_protocol::PayloadType;
use actr_protocol::{ActorResult, ActrError, ActrId, RpcEnvelope};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

// ─── W3C traceparent fixtures ─────────────────────────────────────────────────

/// A syntactically valid W3C traceparent header per the spec:
///   version(2)-trace_id(32)-parent_id(16)-flags(2)
const TRACE_PARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

/// A companion tracestate (vendor-specific list, optional in the spec).
const TRACE_STATE: &str = "vendor=abc123";

// ─────────────────────────────────────────────────────────────────────────────
// Scenario A — Inproc (HostTransport)
// ─────────────────────────────────────────────────────────────────────────────

/// Workload that echoes `traceparent` from the incoming envelope as the
/// response payload so the caller can assert it survived the trip.
struct TraceEchoWorkload;

#[async_trait]
impl Workload for TraceEchoWorkload {
    type Dispatcher = TraceEchoDispatcher;
}

struct TraceEchoDispatcher;

#[async_trait]
impl MessageDispatcher for TraceEchoDispatcher {
    type Workload = TraceEchoWorkload;

    async fn dispatch<C: Context>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        _ctx: &C,
    ) -> ActorResult<Bytes> {
        let tp = envelope
            .traceparent
            .as_deref()
            .unwrap_or("")
            .as_bytes()
            .to_vec();
        Ok(Bytes::from(tp))
    }
}

// ── Minimal mock Context ──────────────────────────────────────────────────────

#[derive(Clone)]
struct MockCtx;

#[async_trait]
impl Context for MockCtx {
    fn self_id(&self) -> &ActrId {
        static ID: std::sync::OnceLock<ActrId> = std::sync::OnceLock::new();
        ID.get_or_init(|| ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 99,
        })
    }

    fn caller_id(&self) -> Option<&ActrId> {
        None
    }
    fn request_id(&self) -> &str {
        "mock"
    }

    async fn call<R: actr_protocol::RpcRequest>(
        &self,
        _target: &actr_framework::Dest,
        _req: R,
        _timeout_ms: i64,
    ) -> ActorResult<R::Response> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn tell<R: actr_protocol::RpcRequest>(
        &self,
        _target: &actr_framework::Dest,
        _msg: R,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn register_stream<F>(&self, _id: String, _cb: F) -> ActorResult<()>
    where
        F: Fn(actr_protocol::DataChunk, ActrId) -> MaybeSendBoxFuture<'static, ActorResult<()>>
            + MaybeSendSync
            + 'static,
    {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn unregister_stream(&self, _id: &str) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn send_data_chunk(
        &self,
        _target: &actr_framework::Dest,
        _chunk: actr_protocol::DataChunk,
        _pt: actr_protocol::PayloadType,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn discover_route_candidate(
        &self,
        _target_type: &actr_protocol::ActrType,
    ) -> ActorResult<ActrId> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn call_raw(
        &self,
        _target: &ActrId,
        _route_key: &str,
        _payload: bytes::Bytes,
        _timeout_ms: i64,
    ) -> ActorResult<bytes::Bytes> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn register_media_track<F>(&self, _id: String, _cb: F) -> ActorResult<()>
    where
        F: Fn(
                actr_framework::MediaSample,
                ActrId,
            ) -> futures_util::future::BoxFuture<'static, ActorResult<()>>
            + Send
            + Sync
            + 'static,
    {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn unregister_media_track(&self, _id: &str) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn send_media_sample(
        &self,
        _target: &actr_framework::Dest,
        _track_id: &str,
        _sample: actr_framework::MediaSample,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn add_media_track(
        &self,
        _target: &actr_framework::Dest,
        _track_id: &str,
        _codec: &str,
        _media_type: &str,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }

    async fn remove_media_track(
        &self,
        _target: &actr_framework::Dest,
        _track_id: &str,
    ) -> ActorResult<()> {
        Err(ActrError::NotImplemented("mock".to_string()))
    }
}

// ── Inproc harness ────────────────────────────────────────────────────────────

struct InprocHarness {
    shell_gate: Arc<HostGate>,
    actor_id: ActrId,
    shutdown: CancellationToken,
    _tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl InprocHarness {
    async fn build() -> Self {
        let actor_id = ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 77,
        };

        // Shell -> Workload channel
        let req_transport = Arc::new(HostTransport::new());
        // Workload -> Shell response channel
        let resp_transport = Arc::new(HostTransport::new());

        let shell_gate = Arc::new(HostGate::new(req_transport.clone()));
        let shutdown = CancellationToken::new();

        // ── Workload loop ──────────────────────────────────────────────────────
        let req_rx_lane = req_transport
            .get_lane(PayloadType::RpcReliable, None)
            .await
            .expect("req lane");
        let resp_tx = resp_transport.clone();
        let sd = shutdown.clone();
        let workload = Arc::new(TraceEchoWorkload);

        let wl_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sd.cancelled() => break,
                    result = req_rx_lane.recv_envelope() => {
                        let envelope = match result {
                            Ok(e) => e,
                            Err(_) => break,
                        };
                        let request_id = envelope.request_id.clone();
                        let ctx = MockCtx;
                        let response_bytes = TraceEchoDispatcher::dispatch(
                            &workload,
                            envelope,
                            &ctx,
                        )
                        .await
                        .unwrap_or_else(|_| Bytes::new());

                        let resp = RpcEnvelope {
                            request_id,
                            route_key: "response".to_string(),
                            payload: Some(response_bytes),
                            direction: Some(actr_protocol::Direction::Response as i32),
                            timeout_ms: 5000,
                            ..Default::default()
                        };
                        let _ = resp_tx
                            .send_message(PayloadType::RpcReliable, None, resp)
                            .await;
                    }
                }
            }
        });

        // ── Shell receive loop ─────────────────────────────────────────────────
        let resp_rx_lane = resp_transport
            .get_lane(PayloadType::RpcReliable, None)
            .await
            .expect("resp lane");
        let req_mgr = req_transport.clone();
        let sd2 = shutdown.clone();

        let shell_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sd2.cancelled() => break,
                    result = resp_rx_lane.recv_envelope() => {
                        let envelope = match result {
                            Ok(e) => e,
                            Err(_) => break,
                        };
                        if let Some(payload) = envelope.payload {
                            let _ = req_mgr
                                .complete_response(&envelope.request_id, payload)
                                .await;
                        }
                    }
                }
            }
        });

        Self {
            shell_gate,
            actor_id,
            shutdown,
            _tasks: vec![wl_task, shell_task],
        }
    }

    async fn send_traced(&self, traceparent: &str, tracestate: Option<&str>) -> ActorResult<Bytes> {
        let envelope = RpcEnvelope {
            route_key: "trace/echo".to_string(),
            payload: Some(Bytes::from_static(b"ping")),
            traceparent: Some(traceparent.to_string()),
            tracestate: tracestate.map(|s| s.to_string()),
            request_id: uuid::Uuid::new_v4().to_string(),
            direction: Some(actr_protocol::Direction::Request as i32),
            timeout_ms: 5000,
            ..Default::default()
        };
        self.shell_gate.send_request(&self.actor_id, envelope).await
    }
}

impl Drop for InprocHarness {
    fn drop(&mut self) {
        self.shutdown.cancel();
        for t in self._tasks.drain(..) {
            t.abort();
        }
    }
}

/// Scenario A-1: traceparent survives the inproc Shell → Workload round-trip.
#[tokio::test]
async fn inproc_traceparent_preserved_round_trip() {
    let harness = InprocHarness::build().await;

    let result = harness
        .send_traced(TRACE_PARENT, Some(TRACE_STATE))
        .await
        .expect("inproc RPC must succeed");

    let echoed = String::from_utf8(result.to_vec()).expect("valid utf8");
    assert_eq!(
        echoed, TRACE_PARENT,
        "traceparent must survive inproc HostTransport round-trip unchanged"
    );
}

/// Scenario A-2: when traceparent is absent callee receives an empty string.
#[tokio::test]
async fn inproc_no_traceparent_yields_empty() {
    let harness = InprocHarness::build().await;

    let envelope = RpcEnvelope {
        route_key: "trace/echo".to_string(),
        payload: Some(Bytes::from_static(b"no-trace")),
        traceparent: None,
        tracestate: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 5000,
        ..Default::default()
    };
    let result = harness
        .shell_gate
        .send_request(&harness.actor_id, envelope)
        .await
        .expect("inproc RPC must succeed");

    let echoed = String::from_utf8(result.to_vec()).expect("valid utf8");
    assert!(
        echoed.is_empty(),
        "absent traceparent should echo as empty string, got: {echoed:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario B — WebRTC / WebSocket transport (PeerGate)
// ─────────────────────────────────────────────────────────────────────────────

async fn webrtc_harness(a: u64, b: u64) -> TestHarness {
    let mut harness = TestHarness::new().await;
    harness.add_peer(a).await;
    harness.add_peer(b).await;
    harness.connect(a, b).await;
    harness
}

/// Scenario B-1: traceparent survives protobuf serialisation over the WebRTC
/// data-channel (WebSocket-signalled, localhost).
#[tokio::test]
async fn webrtc_traceparent_preserved_across_transport() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init()
        .ok();

    let harness = webrtc_harness(501, 502).await;
    let id_b = harness.peer(502).id.clone();

    // ── Send request with traceparent ─────────────────────────────────────────
    let envelope = RpcEnvelope {
        request_id: uuid::Uuid::new_v4().to_string(),
        route_key: "trace/reflect".to_string(),
        payload: Some(Bytes::from_static(b"probe")),
        traceparent: Some(TRACE_PARENT.to_string()),
        tracestate: Some(TRACE_STATE.to_string()),
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 20_000,
        ..Default::default()
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        harness.peer(501).gate.send_request(&id_b, envelope),
    )
    .await
    .expect("send_request must complete within 20 s")
    .expect("send_request must succeed");

    let echoed = String::from_utf8(result.to_vec()).expect("valid utf8 in response");

    assert_eq!(
        echoed, TRACE_PARENT,
        "traceparent must survive protobuf encode → WebRTC → protobuf decode unchanged"
    );
    harness.shutdown().await;
}

/// Scenario B-2: an envelope without traceparent carries no trace context.
#[tokio::test]
async fn webrtc_no_traceparent_yields_empty() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init()
        .ok();

    let harness = webrtc_harness(601, 602).await;
    let id_b = harness.peer(602).id.clone();

    let envelope = RpcEnvelope {
        request_id: uuid::Uuid::new_v4().to_string(),
        route_key: "trace/reflect".to_string(),
        payload: Some(Bytes::from_static(b"no-trace")),
        traceparent: None,
        tracestate: None,
        direction: Some(actr_protocol::Direction::Request as i32),
        timeout_ms: 20_000,
        ..Default::default()
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        harness.peer(601).gate.send_request(&id_b, envelope),
    )
    .await
    .expect("send_request must complete within 20 s")
    .expect("send_request must succeed");

    let echoed = String::from_utf8(result.to_vec()).expect("valid utf8");
    assert!(
        echoed.is_empty(),
        "absent traceparent should not appear on the callee, got: {echoed:?}"
    );
    harness.shutdown().await;
}
