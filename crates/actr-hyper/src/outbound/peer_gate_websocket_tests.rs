//! RFC-0507 acceptance: real WebSocket framing, deterministic partial I/O,
//! and the production lane -> route -> operation-owner error path.
use super::*;
use crate::transport::{
    ConnType, DataLane, NetworkResult, SendAttemptResult, WebSocketDataLane, WireBuilder,
    WireHandle,
};
use async_trait::async_trait;
use futures_util::{Sink, StreamExt};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::{WebSocketStream, tungstenite};

#[derive(Default)]
struct Writes {
    submissions: AtomicUsize,
    lane_calls: AtomicUsize,
    accepted_bytes: AtomicUsize,
    failures: AtomicUsize,
    fallback: AtomicUsize,
    builds: AtomicUsize,
}

/// Accept one byte of the actual encoded frame, then fail. No sleeps, peer
/// shutdown races, or synthesized WebSocket errors determine the outcome.
struct PartialWriteIo(Arc<Writes>);

impl AsyncRead for PartialWriteIo {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

impl AsyncWrite for PartialWriteIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        assert!(!buf.is_empty());
        if self.0.accepted_bytes.load(Ordering::SeqCst) == 0 {
            self.0.accepted_bytes.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Ok(1))
        } else {
            self.0.failures.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "RFC-0507 injected partial WebSocket write",
            )))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Count message submissions separately from the I/O polls within a frame.
struct CountedSink<S> {
    inner: S,
    writes: Arc<Writes>,
}

impl<S> Sink<tungstenite::Message> for CountedSink<S>
where
    S: Sink<tungstenite::Message, Error = tungstenite::Error> + Unpin,
{
    type Error = tungstenite::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: tungstenite::Message) -> Result<(), Self::Error> {
        let this = self.get_mut();
        this.writes.submissions.fetch_add(1, Ordering::SeqCst);
        Pin::new(&mut this.inner).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_close(cx)
    }
}

struct ObservedLane<L> {
    inner: L,
    writes: Arc<Writes>,
}

impl<L> std::fmt::Debug for ObservedLane<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ObservedWebSocketLane")
    }
}

#[async_trait]
impl<L: DataLane + 'static> DataLane for ObservedLane<L> {
    async fn send_with_progress(&self, data: Bytes) -> SendAttemptResult {
        self.writes.lane_calls.fetch_add(1, Ordering::SeqCst);
        let attempt = self.inner.send_with_progress(data).await;
        assert_eq!(attempt.progress, DeliveryProgress::Unknown);
        assert!(!attempt.proven_pre_send);
        assert!(attempt.result.is_err());
        attempt
    }

    fn lane_type(&self) -> &'static str {
        "observed-websocket"
    }
}

struct FallbackLane(Arc<Writes>);

impl std::fmt::Debug for FallbackLane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FallbackLane")
    }
}

#[async_trait]
impl DataLane for FallbackLane {
    async fn send(&self, _data: Bytes) -> NetworkResult<()> {
        self.0.fallback.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn lane_type(&self) -> &'static str {
        "counted-fallback"
    }
}

#[derive(Debug)]
struct TestWire {
    kind: ConnType,
    lane: Arc<dyn DataLane>,
}

#[async_trait]
impl WireHandle for TestWire {
    fn connection_type(&self) -> ConnType {
        self.kind
    }
    fn priority(&self) -> u8 {
        1
    }
    async fn connect(&self) -> NetworkResult<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn close(&self) -> NetworkResult<()> {
        Ok(())
    }
    async fn get_lane(&self, _payload_type: PayloadType) -> NetworkResult<Arc<dyn DataLane>> {
        Ok(self.lane.clone())
    }
}

struct TestBuilder {
    wires: Vec<Arc<dyn WireHandle>>,
    writes: Arc<Writes>,
}

#[async_trait]
impl WireBuilder for TestBuilder {
    async fn create_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.writes.builds.fetch_add(1, Ordering::SeqCst);
        Ok(self.wires.clone())
    }
}

#[tokio::test]
async fn live_websocket_partial_write_is_uncertain_without_replay() {
    // WebSocket is normally the last route. Also install the same real lane
    // in the first route's test slot so a ready later route makes the fallback
    // assertion non-vacuous. Only wire discovery is mocked, not lane sending.
    for with_fallback in [false, true] {
        for operation in ["call", "tell", "stream"] {
            tokio::time::timeout(Duration::from_secs(5), async {
                let writes = Arc::new(Writes::default());
                let websocket = WebSocketStream::from_raw_socket(
                    PartialWriteIo(writes.clone()),
                    tungstenite::protocol::Role::Client,
                    None,
                )
                .await;
                let (sink, _read) = websocket.split();
                let payload_type = if operation == "stream" {
                    PayloadType::StreamReliable
                } else {
                    PayloadType::RpcReliable
                };
                let lane = WebSocketDataLane::new(
                    Arc::new(Mutex::new(Some(CountedSink {
                        inner: sink,
                        writes: writes.clone(),
                    }))),
                    payload_type,
                    mpsc::channel(1).1,
                );
                let mut wires: Vec<Arc<dyn WireHandle>> = vec![Arc::new(TestWire {
                    kind: if with_fallback {
                        ConnType::WebRTC
                    } else {
                        ConnType::WebSocket
                    },
                    lane: Arc::new(ObservedLane {
                        inner: lane,
                        writes: writes.clone(),
                    }),
                })];
                if with_fallback {
                    wires.push(Arc::new(TestWire {
                        kind: ConnType::WebSocket,
                        lane: Arc::new(FallbackLane(writes.clone())),
                    }));
                }
                let actor = |serial_number| ActrId {
                    realm: "test@example.com".into(),
                    serial_number,
                };
                let target = actor(2);
                let transport = Arc::new(PeerTransport::new(
                    actor(1),
                    Arc::new(TestBuilder {
                        wires,
                        writes: writes.clone(),
                    }),
                ));
                // Wait for every candidate before sending, so scheduling cannot
                // choose the successful fallback before the failing lane is ready.
                let dest = Dest::Peer(target.clone());
                let route = transport.get_or_create_transport(&dest).await.unwrap();
                let mut ready = route.watch_ready();
                while ready.borrow_and_update().len() != if with_fallback { 2 } else { 1 } {
                    ready.changed().await.unwrap();
                }
                let gate = PeerGate::with_rtc_manager(
                    transport.clone(),
                    PeerGate::empty_manager_for_test(),
                    Arc::new(RpcCorrelation::new()),
                    None,
                );
                let envelope = RpcEnvelope {
                    request_id: operation.into(),
                    ..RpcEnvelope::default()
                };
                let deadline = RpcDeadline::from_duration(Duration::from_secs(2));
                let error = match operation {
                    "call" => gate
                        .send_request_with_type_until(&target, payload_type, envelope, deadline)
                        .await
                        .expect_err("partial Call write must fail"),
                    "tell" => gate
                        .send_tell_with_type_until(&target, payload_type, envelope, deadline)
                        .await
                        .expect_err("partial Tell write must fail"),
                    _ => gate
                        .send_data_chunk_until(
                            &target,
                            payload_type,
                            "stream-id",
                            Bytes::from_static(b"chunk"),
                            deadline,
                            operation,
                        )
                        .await
                        .expect_err("partial stream write must fail"),
                };
                assert!(
                    matches!(&error, ActrError::DeliveryUncertain { operation_id, cause }
                    if operation_id == operation && cause.contains("RFC-0507 injected")),
                    "{error:?}"
                );
                assert!(!error.is_retryable());
                assert_eq!(writes.accepted_bytes.load(Ordering::SeqCst), 1);
                assert_eq!(writes.failures.load(Ordering::SeqCst), 1);
                assert_eq!(writes.submissions.load(Ordering::SeqCst), 1);
                assert_eq!(writes.lane_calls.load(Ordering::SeqCst), 1);
                assert_eq!(writes.fallback.load(Ordering::SeqCst), 0);
                assert_eq!(writes.builds.load(Ordering::SeqCst), 1);
                assert_eq!(gate.pending_requests.len(), 0);
                transport.close_all().await.unwrap();
            })
            .await
            .unwrap_or_else(|_| panic!("{operation}, fallback={with_fallback}: timed out"));
        }
    }
}
