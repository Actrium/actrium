//! Opaque UniFFI handles for the stream-first DataChunk API.
//!
//! Rust `Stream` / `Sink` traits cannot cross UniFFI, so foreign consumers get
//! pull/send handles instead. Lifecycle contract:
//!
//! - Only one `next()` may be pending per stream handle; a concurrent call
//!   returns [`ActrError::InvalidArgument`].
//! - Stream `close()` is idempotent, never waits behind a pending `next()`,
//!   wakes it with `Ok(None)`, and unregisters synchronously (no
//!   duplicate-subscription window for an immediate re-subscribe).
//! - Dropping a handle performs best-effort cancellation, but language
//!   wrappers must close deterministically.
//! - Concurrent sink `send()` calls are serialized in bridge-acceptance order.
//! - Sink `close()` stops accepting new sends, waits for already accepted
//!   sends to finish, sends the in-band `end_of_stream` marker, and is
//!   idempotent. Calls after close return
//!   [`ActrError::InvalidArgument`].

use std::sync::atomic::{AtomicBool, Ordering};

use actr_framework::{DataChunkSink, DataChunkStream};
use futures_util::{SinkExt, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::{ActrError, ActrResult};

/// One inbound DataChunk with its sender, as returned by
/// [`DataChunkStreamBridge::next`].
#[derive(Clone, uniffi::Record)]
pub struct DataChunkEventBridge {
    pub chunk: crate::types::DataChunk,
    pub sender: crate::types::ActrId,
}

impl From<actr_framework::DataChunkEvent> for DataChunkEventBridge {
    fn from(event: actr_framework::DataChunkEvent) -> Self {
        Self {
            chunk: event.chunk.into(),
            sender: event.sender.into(),
        }
    }
}

/// Opaque pull handle for one DataChunk subscription.
#[derive(uniffi::Object)]
pub struct DataChunkStreamBridge {
    // `None` once the stream has been taken for closing.
    inner: std::sync::Arc<tokio::sync::Mutex<Option<DataChunkStream>>>,
    closed: AtomicBool,
    wake: CancellationToken,
    // Shared cancel hook captured at construction: lets `close` unregister
    // synchronously instead of waiting on the teardown task.
    cancel: Option<actr_framework::StreamCancelHandle>,
    // Copied out of the stream when it ends on the peer's marker, so the
    // terminating sender stays inspectable after close() consumes the
    // stream. `None` for a stream that ended any other way.
    end_marker: std::sync::Mutex<Option<DataChunkEventBridge>>,
}

impl DataChunkStreamBridge {
    pub(crate) fn new(stream: DataChunkStream) -> Self {
        let cancel = stream.cancel_handle();
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(Some(stream))),
            closed: AtomicBool::new(false),
            wake: CancellationToken::new(),
            cancel,
            end_marker: std::sync::Mutex::new(None),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl DataChunkStreamBridge {
    /// Wait for the next inbound chunk. Returns `None` at end of stream and
    /// after `close()`. Only one call may be pending at a time.
    ///
    /// Cancellation safety: dropping this future (e.g. foreign coroutine
    /// cancellation) releases the internal lock guard via RAII, so the handle
    /// stays usable.
    pub async fn next(&self) -> ActrResult<Option<DataChunkEventBridge>> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(None);
        }
        let mut guard = self
            .inner
            .try_lock()
            .map_err(|_| ActrError::InvalidArgument {
                msg: "concurrent next() on DataChunkStreamBridge".into(),
            })?;
        let Some(stream) = guard.as_mut() else {
            return Ok(None);
        };
        tokio::select! {
            // close() must win over a blocked poll so a pending next() wakes
            // with Ok(None) instead of hanging until the next chunk arrives.
            biased;
            _ = self.wake.cancelled() => {
                let Some(stream) = guard.take() else {
                    return Ok(None);
                };
                drop(guard);
                stream.close().await;
                Ok(None)
            }
            item = stream.next() => {
                match item {
                    Some(Ok(event)) => Ok(Some(event.into())),
                    Some(Err(error)) => Err(error.into()),
                    None => {
                        // Preserve the terminating marker (sender included)
                        // for post-end inspection; the stream object itself
                        // may be consumed by a later close().
                        if let Some(marker) = stream.end_marker() {
                            *self.end_marker.lock().unwrap() = Some(marker.clone().into());
                        }
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Whether the stream ended on the peer's in-band end-of-stream marker
    /// (as opposed to a local `close()` or runtime teardown). Meaningful
    /// after `next()` returned `None`; survives `close()`.
    pub fn ended_by_marker(&self) -> bool {
        self.end_marker.lock().unwrap().is_some()
    }

    /// The end-of-stream marker event that terminated the stream, if any.
    /// The sender is the transport-authenticated peer that sent it, so a
    /// consumer that pins a stream to one expected producer can detect a
    /// foreign terminator here.
    pub fn end_marker(&self) -> Option<DataChunkEventBridge> {
        self.end_marker.lock().unwrap().clone()
    }

    /// Close the subscription: idempotent, never waits behind a pending
    /// `next()`, and wakes any pending `next()` with `Ok(None)`. The
    /// registry unregister runs synchronously, so a re-subscription issued
    /// right after this call cannot be rejected as a duplicate.
    pub async fn close(&self) -> ActrResult<()> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        // Synchronous unregister first: the remaining teardown (async
        // cleanup) is idempotent and runs through the normal close path.
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        self.wake.cancel();
        // Never wait behind a pending next(): if it holds the lock, its
        // cancellation arm performs the close instead.
        if let Ok(mut guard) = self.inner.try_lock() {
            if let Some(stream) = guard.take() {
                drop(guard);
                stream.close().await;
            }
        } else {
            // Fallback: a pending next() holding the lock will normally run
            // its cancellation arm, but a foreign runtime may drop that
            // future without ever re-polling it. Queue a detached closer that
            // runs as soon as the lock is free so the subscription is always
            // torn down (bounded: one task per close call).
            let inner = self.inner.clone();
            tokio::spawn(async move {
                let mut guard = inner.lock().await;
                if let Some(stream) = guard.take() {
                    drop(guard);
                    stream.close().await;
                }
            });
        }
        Ok(())
    }
}

/// Opaque send handle bound to one destination, stream ID, and lane.
#[derive(uniffi::Object)]
pub struct DataChunkSinkBridge {
    // Serializes send()/close() in bridge-acceptance order.
    inner: tokio::sync::Mutex<DataChunkSink>,
    closed: AtomicBool,
}

impl DataChunkSinkBridge {
    pub(crate) fn new(sink: DataChunkSink) -> Self {
        Self {
            inner: tokio::sync::Mutex::new(sink),
            closed: AtomicBool::new(false),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl DataChunkSinkBridge {
    /// Send one chunk. The chunk's `stream_id` must equal the sink's stream
    /// ID. Calls are serialized; a call after `close()` returns
    /// [`ActrError::InvalidArgument`].
    pub async fn send(&self, chunk: crate::types::DataChunk) -> ActrResult<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ActrError::InvalidArgument {
                msg: "DataChunkSinkBridge is closed".into(),
            });
        }
        let mut guard = self.inner.lock().await;
        // Taking a place in the FIFO lock queue accepts this send. A later
        // close must drain it even if it has not acquired the lock yet.
        guard.send(chunk.into()).await.map_err(Into::into)
    }

    /// Stop accepting new sends, wait for already accepted sends to finish,
    /// and close. Idempotent.
    pub async fn close(&self) -> ActrResult<()> {
        self.closed.store(true, Ordering::Release);
        // Every close waits for teardown, including concurrent callers and a
        // retry after a previous close future was cancelled.
        // Waits behind all accepted sends, then flushes.
        let mut guard = self.inner.lock().await;
        guard.close().await.map_err(Into::into)
    }

    /// Abort the sink: stop accepting new sends and wait for accepted sends
    /// to finish — WITHOUT emitting the end-of-stream marker. Use on failure
    /// paths (e.g. a transient send error) where the peer must not observe a
    /// graceful end. Idempotent; a later `close()` is a no-op because the
    /// marker window has passed.
    pub async fn abort(&self) -> ActrResult<()> {
        self.closed.store(true, Ordering::Release);
        let mut guard = self.inner.lock().await;
        guard.abort().await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_framework::{DataChunkEvent, StreamCloser};
    use actr_protocol::{ActorResult, DataChunk, PayloadType};
    use std::sync::Arc;

    fn test_stream() -> (
        DataChunkStream,
        futures_channel::mpsc::Sender<ActorResult<DataChunkEvent>>,
    ) {
        let (tx, rx) = futures_channel::mpsc::channel(4);
        let stream = DataChunkStream::from_parts(rx, StreamCloser::new(Box::new(|| {})));
        (stream, tx)
    }

    fn test_sink(sent: Arc<std::sync::Mutex<Vec<u64>>>) -> DataChunkSink {
        let send_fn: actr_framework::SendDataChunkFn = Box::new(move |chunk| {
            let sent = sent.clone();
            Box::pin(async move {
                sent.lock().unwrap().push(chunk.sequence);
                Ok(())
            })
        });
        DataChunkSink::new("s".into(), PayloadType::StreamReliable, send_fn).unwrap()
    }

    fn bridge_chunk(stream_id: &str, sequence: u64) -> crate::types::DataChunk {
        DataChunk {
            stream_id: stream_id.to_string(),
            sequence,
            payload: vec![sequence as u8].into(),
            metadata: vec![],
            timestamp_ms: None,
            end_of_stream: None,
        }
        .into()
    }

    // current_thread: yield_now() deterministically runs the spawned next()
    // to its first Pending point (holding the lock) before the caller's
    // concurrent next() runs — no lock-acquisition race.
    #[tokio::test(flavor = "current_thread")]
    async fn next_rejects_concurrent_call() {
        let (stream, _tx) = test_stream();
        let bridge = Arc::new(DataChunkStreamBridge::new(stream));

        let pending = {
            let bridge = bridge.clone();
            tokio::spawn(async move { bridge.next().await })
        };
        // Let the spawned next() acquire the lock.
        tokio::task::yield_now().await;
        let concurrent = bridge.next().await;
        assert!(matches!(concurrent, Err(ActrError::InvalidArgument { .. })));

        bridge.close().await.unwrap();
        let first = pending.await.unwrap();
        assert!(matches!(first, Ok(None)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_wakes_pending_next_with_none_and_is_idempotent() {
        let (stream, _tx) = test_stream();
        let bridge = Arc::new(DataChunkStreamBridge::new(stream));

        let pending = {
            let bridge = bridge.clone();
            tokio::spawn(async move { bridge.next().await })
        };
        tokio::task::yield_now().await;
        bridge.close().await.unwrap();
        bridge.close().await.unwrap();
        let first = pending.await.unwrap();
        assert!(matches!(first, Ok(None)));
        assert!(matches!(bridge.next().await, Ok(None)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn next_yields_events_in_order() {
        let (stream, mut tx) = test_stream();
        let bridge = DataChunkStreamBridge::new(stream);
        for seq in 1..=3 {
            tx.send(Ok(DataChunkEvent {
                chunk: DataChunk {
                    stream_id: "s".into(),
                    sequence: seq,
                    payload: vec![].into(),
                    metadata: vec![],
                    timestamp_ms: None,
                    end_of_stream: None,
                },
                sender: Default::default(),
            }))
            .await
            .unwrap();
        }
        for seq in 1..=3 {
            let event = bridge.next().await.unwrap().unwrap();
            assert_eq!(event.chunk.sequence, seq);
        }
    }

    #[tokio::test]
    async fn sink_close_drains_queued_sends_and_survives_cancelled_close() {
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (release, wait) = tokio::sync::oneshot::channel::<()>();
        let mut wait = Some(wait);
        let send_fn: actr_framework::SendDataChunkFn = {
            let sent = sent.clone();
            Box::new(move |chunk| {
                let wait = wait.take();
                let sent = sent.clone();
                Box::pin(async move {
                    if let Some(wait) = wait {
                        wait.await.unwrap();
                    }
                    sent.lock().unwrap().push(chunk.sequence);
                    Ok(())
                })
            })
        };
        let bridge = DataChunkSinkBridge::new(
            DataChunkSink::new("s".into(), PayloadType::StreamReliable, send_fn).unwrap(),
        );
        let mut first = Box::pin(bridge.send(bridge_chunk("s", 1)));
        let mut queued = Box::pin(bridge.send(bridge_chunk("s", 2)));
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        assert!(futures_util::poll!(queued.as_mut()).is_pending());
        let mut closing = Box::pin(bridge.close());
        assert!(futures_util::poll!(closing.as_mut()).is_pending());
        let mut repeated = Box::pin(bridge.close());
        assert!(futures_util::poll!(repeated.as_mut()).is_pending());
        drop(closing);
        assert!(matches!(
            bridge.send(bridge_chunk("s", 3)).await,
            Err(ActrError::InvalidArgument { .. })
        ));
        release.send(()).unwrap();
        first.await.unwrap();
        queued.await.unwrap();
        repeated.await.unwrap();
        // Chunks 1 and 2 plus the close-time end-of-stream marker (seq 3).
        assert_eq!(*sent.lock().unwrap(), vec![1, 2, 3]);
        bridge.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sink_sends_are_serialized_and_close_rejects_later_sends() {
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let bridge = Arc::new(DataChunkSinkBridge::new(test_sink(sent.clone())));

        let mut handles = Vec::new();
        for seq in 0..4u64 {
            let bridge = bridge.clone();
            handles.push(tokio::spawn(async move {
                bridge.send(bridge_chunk("s", seq)).await
            }));
        }
        for handle in handles {
            handle.await.unwrap().unwrap();
        }
        bridge.close().await.unwrap();
        bridge.close().await.unwrap();
        assert!(matches!(
            bridge.send(bridge_chunk("s", 99)).await,
            Err(ActrError::InvalidArgument { .. })
        ));

        let mut seen = sent.lock().unwrap().clone();
        seen.sort_unstable();
        // Chunks 0..=3 plus the close-time end-of-stream marker (seq 4).
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    }
}
