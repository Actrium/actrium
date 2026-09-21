//! Stream-first DataChunk API types.
//!
//! `DataChunk` remains the single wire item; these types expose sequences of
//! chunks through the standard `futures` ecosystem ([`Stream`] / [`Sink`])
//! instead of callback registration.
//!
//! Backends (native runtime, Component Model guest, dynclib guest) construct
//! [`DataChunkStream`] / [`DataChunkSink`] through the `#[doc(hidden)]`
//! constructors and supply the closures that hook into their transport.
//!
//! Buffering note: the inbound path may hold up to `capacity + 1` items
//! (`futures_channel::mpsc` reserves one slot per sender; exactly one sender
//! clone is kept alive per subscription). Runtimes may add their own bounded
//! inbound queue upstream of this channel; every layer stays bounded.

use actr_protocol::{ActorResult, ActrError, ActrId, DataChunk, PayloadType};
use futures_channel::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use futures_util::future::BoxFuture;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use crate::context::MaybeSendBoxFuture;

/// Default subscription capacity when none is specified.
pub const DEFAULT_DATA_CHUNK_CAPACITY: usize = 64;

/// A single inbound DataChunk together with its sender.
#[derive(Debug, Clone)]
pub struct DataChunkEvent {
    pub chunk: DataChunk,
    pub sender: ActrId,
}

/// Options for [`crate::Context::data_chunks`].
///
/// `capacity` bounds the per-subscription queue. Zero is rejected with
/// [`ActrError::InvalidArgument`]. Note `futures_channel::mpsc` effectively
/// holds `capacity + 1` items (one slot per live sender).
#[derive(Debug, Clone, Copy)]
pub struct DataChunkStreamOptions {
    pub capacity: usize,
}

impl Default for DataChunkStreamOptions {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_DATA_CHUNK_CAPACITY,
        }
    }
}

/// Validate subscription options, returning the usable capacity.
#[doc(hidden)]
pub fn validate_stream_options(options: &DataChunkStreamOptions) -> ActorResult<usize> {
    if options.capacity == 0 {
        return Err(ActrError::InvalidArgument(
            "DataChunkStreamOptions.capacity must be greater than zero".into(),
        ));
    }
    Ok(options.capacity)
}

// Boxed closure aliases. `MaybeSendSync` is a user trait and therefore cannot
// appear as a second `dyn` bound, so the native/WASM split uses cfg'd aliases
// instead.

/// Closure that performs one outbound chunk send.
#[cfg(not(target_arch = "wasm32"))]
pub type SendDataChunkFn =
    Box<dyn FnMut(DataChunk) -> BoxFuture<'static, ActorResult<()>> + Send + Sync>;
/// Closure that performs one outbound chunk send.
#[cfg(target_arch = "wasm32")]
pub type SendDataChunkFn =
    Box<dyn FnMut(DataChunk) -> futures_util::future::LocalBoxFuture<'static, ActorResult<()>>>;

/// Synchronous, idempotent cancellation hook (local unregister + reader wakeup).
#[cfg(not(target_arch = "wasm32"))]
pub type StreamCancelFn = Box<dyn FnOnce() + Send + Sync>;
/// Synchronous, idempotent cancellation hook (local unregister + reader wakeup).
#[cfg(target_arch = "wasm32")]
pub type StreamCancelFn = Box<dyn FnOnce()>;

/// Optional async cleanup run after cancellation (e.g. a host unregister import).
#[cfg(not(target_arch = "wasm32"))]
pub type StreamWaitFn = Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send + Sync>;
/// Optional async cleanup run after cancellation (e.g. a host unregister import).
#[cfg(target_arch = "wasm32")]
pub type StreamWaitFn = Box<dyn FnOnce() -> futures_util::future::LocalBoxFuture<'static, ()>>;

/// Teardown hooks carried by a [`DataChunkStream`].
#[doc(hidden)]
pub struct StreamCloser {
    cancel: Option<StreamCancelFn>,
    wait: Option<StreamWaitFn>,
}

impl StreamCloser {
    pub fn new(cancel: StreamCancelFn) -> Self {
        Self {
            cancel: Some(cancel),
            wait: None,
        }
    }

    pub fn with_wait(cancel: StreamCancelFn, wait: StreamWaitFn) -> Self {
        Self {
            cancel: Some(cancel),
            wait: Some(wait),
        }
    }

    fn cancel_sync(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }

    async fn wait_async(&mut self) {
        if let Some(wait) = self.wait.take() {
            wait().await;
        }
    }
}

/// Stream of inbound DataChunks for one subscription.
///
/// Implements [`futures_core::Stream`] with items of
/// `ActorResult<DataChunkEvent>`; a runtime-originated terminal failure may
/// yield one `Err` item before the stream ends.
///
/// Only one active subscription is allowed per `stream_id` within one
/// actor/context instance; creating a duplicate is rejected by the runtime
/// with [`ActrError::InvalidArgument`].
///
/// Cancellation: dropping the stream or calling [`DataChunkStream::close`]
/// unregisters immediately, discards buffered items, and wakes pending
/// readers. This is an intentional cancellation exception to the graceful
/// drain semantics applied to internal runtime unregister paths.
pub struct DataChunkStream {
    rx: Option<mpsc::Receiver<ActorResult<DataChunkEvent>>>,
    closer: Option<StreamCloser>,
}

impl DataChunkStream {
    #[doc(hidden)]
    pub fn from_parts(
        rx: mpsc::Receiver<ActorResult<DataChunkEvent>>,
        closer: StreamCloser,
    ) -> Self {
        Self {
            rx: Some(rx),
            closer: Some(closer),
        }
    }

    /// Close the subscription: unregister, drop buffered items, and run any
    /// asynchronous backend cleanup.
    pub async fn close(mut self) {
        let closer = self.closer.take();
        drop(self.rx.take());
        if let Some(mut closer) = closer {
            closer.cancel_sync();
            closer.wait_async().await;
        }
    }
}

impl futures_core::Stream for DataChunkStream {
    type Item = ActorResult<DataChunkEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.as_mut() {
            Some(rx) => Pin::new(rx).poll_next(cx),
            None => Poll::Ready(None),
        }
    }
}

impl Drop for DataChunkStream {
    fn drop(&mut self) {
        // Best-effort cancellation only: synchronous unregister runs, any
        // async cleanup is abandoned (backends must tolerate that).
        if let Some(mut closer) = self.closer.take() {
            closer.cancel_sync();
        }
    }
}

/// Sink of outbound DataChunks bound to one destination, stream ID, and lane.
///
/// Created via [`crate::Context::data_chunk_sink`], which fixes the
/// destination, stream ID, and payload type. Every sent chunk's `stream_id`
/// must equal the sink's stream ID; a mismatch returns
/// [`ActrError::InvalidArgument`] — the sink never silently rewrites the wire
/// item.
///
/// Implements [`futures_sink::Sink`]: `poll_ready` reflects the previous
/// in-flight send, `poll_flush` waits for it, and `poll_close` flushes before
/// closing. `start_send` after close returns [`ActrError::InvalidArgument`].
pub struct DataChunkSink {
    stream_id: String,
    payload_type: PayloadType,
    send_fn: SendDataChunkFn,
    in_flight: Option<MaybeSendBoxFuture<'static, ActorResult<()>>>,
    pending: Option<DataChunk>,
    closed: bool,
}

impl DataChunkSink {
    #[doc(hidden)]
    pub fn new(
        stream_id: String,
        payload_type: PayloadType,
        send_fn: SendDataChunkFn,
    ) -> ActorResult<Self> {
        match payload_type {
            PayloadType::StreamReliable | PayloadType::StreamLatencyFirst => {}
            other => {
                return Err(ActrError::InvalidArgument(format!(
                    "payload_type {other:?} is not a DataChunk stream lane"
                )));
            }
        }
        Ok(Self {
            stream_id,
            payload_type,
            send_fn,
            in_flight: None,
            pending: None,
            closed: false,
        })
    }

    /// Stream ID fixed at sink creation.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    /// Lane fixed at sink creation.
    pub fn payload_type(&self) -> PayloadType {
        self.payload_type
    }

    fn poll_in_flight(&mut self, cx: &mut TaskContext<'_>) -> Poll<ActorResult<()>> {
        if let Some(fut) = self.in_flight.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(result) => {
                    self.in_flight = None;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

impl futures_sink::Sink<DataChunk> for DataChunkSink {
    type Error = ActrError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<ActorResult<()>> {
        // Reflect the previous in-flight send before accepting a new item.
        match self.poll_in_flight(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        // Begin the accepted-but-not-yet-started send.
        if let Some(chunk) = self.pending.take() {
            let fut = (self.send_fn)(chunk);
            self.in_flight = Some(fut);
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: DataChunk) -> ActorResult<()> {
        if self.closed {
            return Err(ActrError::InvalidArgument("DataChunkSink is closed".into()));
        }
        if item.stream_id != self.stream_id {
            return Err(ActrError::InvalidArgument(format!(
                "chunk stream_id '{}' does not match sink stream_id '{}'",
                item.stream_id, self.stream_id
            )));
        }
        self.pending = Some(item);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<ActorResult<()>> {
        match self.as_mut().poll_ready(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        self.poll_in_flight(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<ActorResult<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => {
                self.closed = true;
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};

    fn test_chunk(stream_id: &str, sequence: u64) -> DataChunk {
        DataChunk {
            stream_id: stream_id.to_string(),
            sequence,
            payload: vec![sequence as u8].into(),
            metadata: vec![],
            timestamp_ms: None,
        }
    }

    fn ok_send_fn() -> (
        SendDataChunkFn,
        std::sync::Arc<std::sync::Mutex<Vec<DataChunk>>>,
    ) {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent2 = sent.clone();
        let f: SendDataChunkFn = Box::new(move |chunk| {
            let sent = sent2.clone();
            Box::pin(async move {
                sent.lock().unwrap().push(chunk);
                Ok(())
            })
        });
        (f, sent)
    }

    #[tokio::test]
    async fn sink_rejects_non_stream_payload_type() {
        let (send_fn, _) = ok_send_fn();
        let result = DataChunkSink::new("s".into(), PayloadType::RpcReliable, send_fn);
        assert!(matches!(result, Err(ActrError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn sink_rejects_stream_id_mismatch() {
        let (send_fn, _) = ok_send_fn();
        let mut sink =
            DataChunkSink::new("s".into(), PayloadType::StreamReliable, send_fn).unwrap();
        let result = sink.start_send_unpin(test_chunk("other", 1));
        assert!(matches!(result, Err(ActrError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn sink_sends_in_order_and_close_rejects_further_sends() {
        let (send_fn, sent) = ok_send_fn();
        let mut sink =
            DataChunkSink::new("s".into(), PayloadType::StreamReliable, send_fn).unwrap();
        sink.send(test_chunk("s", 1)).await.unwrap();
        sink.send(test_chunk("s", 2)).await.unwrap();
        sink.close().await.unwrap();
        assert!(matches!(
            sink.start_send_unpin(test_chunk("s", 3)),
            Err(ActrError::InvalidArgument(_))
        ));
        let sent = sent.lock().unwrap();
        let seqs: Vec<u64> = sent.iter().map(|c| c.sequence).collect();
        assert_eq!(seqs, vec![1, 2]);
    }

    #[tokio::test]
    async fn sink_send_propagates_backend_error() {
        let f: SendDataChunkFn =
            Box::new(|_| Box::pin(async { Err(ActrError::Internal("boom".into())) }));
        let mut sink = DataChunkSink::new("s".into(), PayloadType::StreamReliable, f).unwrap();
        let result = sink.send(test_chunk("s", 1)).await;
        assert!(matches!(result, Err(ActrError::Internal(_))));
    }

    #[tokio::test]
    async fn validate_stream_options_rejects_zero_capacity() {
        assert!(matches!(
            validate_stream_options(&DataChunkStreamOptions { capacity: 0 }),
            Err(ActrError::InvalidArgument(_))
        ));
        assert_eq!(
            validate_stream_options(&DataChunkStreamOptions::default()).unwrap(),
            DEFAULT_DATA_CHUNK_CAPACITY
        );
    }

    #[tokio::test]
    async fn stream_yields_items_then_terminates() {
        let (mut tx, rx) = mpsc::channel(4);
        let closer = StreamCloser::new(Box::new(|| {}));
        let mut stream = DataChunkStream::from_parts(rx, closer);
        let sender = ActrId {
            serial_number: 1,
            ..Default::default()
        };
        tx.send(Ok(DataChunkEvent {
            chunk: test_chunk("s", 7),
            sender: sender.clone(),
        }))
        .await
        .unwrap();
        drop(tx);
        let event = stream.next().await.unwrap().unwrap();
        assert_eq!(event.chunk.sequence, 7);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn stream_close_runs_cancel_and_wait() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let waited = std::sync::Arc::new(AtomicBool::new(false));
        let c = cancelled.clone();
        let w = waited.clone();
        let (_tx, rx) = mpsc::channel(4);
        let closer = StreamCloser::with_wait(
            Box::new(move || c.store(true, Ordering::SeqCst)),
            Box::new(move || {
                let w = w.clone();
                Box::pin(async move {
                    w.store(true, Ordering::SeqCst);
                })
            }),
        );
        let stream = DataChunkStream::from_parts(rx, closer);
        stream.close().await;
        assert!(cancelled.load(Ordering::SeqCst));
        assert!(waited.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stream_drop_runs_cancel_best_effort() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let c = cancelled.clone();
        let (_tx, rx) = mpsc::channel(4);
        let stream = DataChunkStream::from_parts(
            rx,
            StreamCloser::new(Box::new(move || {
                c.store(true, Ordering::SeqCst);
            })),
        );
        drop(stream);
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
