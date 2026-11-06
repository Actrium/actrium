//! FileTransfer Service Implementation - Receiver

use crate::generated::file_transfer::*;
use crate::generated::file_transfer_actor::FileTransferServiceHandler;
use actr_protocol::ActorResult;
use actr_hyper::prelude::*;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// FileTransfer service[...] - receive[...]
///
/// [...]:
/// 1. [...] StartTransfer RPC - register DataChunk [...]
/// 2. receive DataChunk data[...]
/// 3. [...] EndTransfer RPC - [...]
pub struct FileTransferService {
    /// receive[...]data[...]
    chunks_received: Arc<Mutex<u32>>,
    /// receive[...]
    bytes_received: Arc<Mutex<u64>>,
    /// Per-stream consumer tasks; each ends on its own when the peer's
    /// end-of-stream marker drains the stream. `end_transfer` joins the task
    /// with a bounded wait and aborts it only if the marker never arrived
    /// (lost on a lossy lane, or a vanished sender).
    stream_tasks: Arc<Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl FileTransferService {
    pub fn new() -> Self {
        Self {
            chunks_received: Arc::new(Mutex::new(0)),
            bytes_received: Arc::new(Mutex::new(0)),
            stream_tasks: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl FileTransferServiceHandler for FileTransferService {
    /// StartTransfer - start[...]
    ///
    /// 1. register DataChunk [...]
    /// 2. [...] ready=true [...] sender [...]send
    async fn start_transfer<C: Context>(
        &self,
        req: StartTransferRequest,
        ctx: &C,
    ) -> ActorResult<StartTransferResponse> {
        tracing::info!("📥 StartTransfer RPC received:");
        tracing::info!("   stream_id: {}", req.stream_id);
        tracing::info!("   filename: {}", req.filename);
        tracing::info!("   total_size: {} bytes", req.total_size);
        tracing::info!("   chunk_count: {}", req.chunk_count);

        // [...]
        *self.chunks_received.lock().await = 0;
        *self.bytes_received.lock().await = 0;

        // subscribe to DataChunks for the stream
        let stream_id = req.stream_id.clone();
        let chunks_counter = self.chunks_received.clone();
        let bytes_counter = self.bytes_received.clone();
        let total_size = req.total_size;
        let chunk_count = req.chunk_count;

        let mut data_chunks = ctx
            .data_chunks(stream_id.clone(), Default::default())
            .await?;
        let stats_stream_id = stream_id.clone();
        let handle = tokio::spawn(async move {
            use futures_util::StreamExt;
            // The loop ends when the sender's sink close arrives as the
            // in-band end-of-stream marker (or on teardown/abort).
            while let Some(event) = data_chunks.next().await {
                let Ok(event) = event else { break };
                let data_chunk = event.chunk;
                let sender_id = event.sender;
                let mut chunks = chunks_counter.lock().await;
                let mut bytes = bytes_counter.lock().await;

                *chunks += 1;
                *bytes += data_chunk.payload.len() as u64;

                let progress = (*bytes as f64 / total_size as f64 * 100.0) as u32;

                tracing::info!(
                    "📦 Received chunk #{}/{} from {:?}: sequence={}, size={} bytes, progress={}%, total_bytes={}",
                    *chunks,
                    chunk_count,
                    sender_id,
                    data_chunk.sequence,
                    data_chunk.payload.len(),
                    progress,
                    *bytes
                );

                // [...]data, [...]
                if let Ok(text) = String::from_utf8(data_chunk.payload.to_vec()) {
                    let preview = &text[..text.len().min(80)];
                    tracing::debug!("   Content preview: {}", preview);
                }
            }
            let chunks = *chunks_counter.lock().await;
            let bytes = *bytes_counter.lock().await;
            tracing::info!(
                "🏁 Stream '{stats_stream_id}' ended: {chunks} chunks, {bytes} bytes received"
            );
        });
        self.stream_tasks
            .lock()
            .await
            .insert(stream_id.clone(), handle);

        tracing::info!("✅ DataChunk subscription active for '{}'", stream_id);
        tracing::info!(
            "   Ready to receive {} chunks ({} bytes)",
            chunk_count,
            total_size
        );

        Ok(StartTransferResponse {
            ready: true,
            message: format!("Ready to receive {} chunks", chunk_count),
        })
    }

    /// EndTransfer - [...]
    ///
    /// 1. [...]register DataChunk [...]
    /// 2. [...]receive[...]
    async fn end_transfer<C: Context>(
        &self,
        req: EndTransferRequest,
        _ctx: &C,
    ) -> ActorResult<EndTransferResponse> {
        tracing::info!("🏁 EndTransfer RPC received:");
        tracing::info!("   stream_id: {}", req.stream_id);
        tracing::info!("   success: {}", req.success);

        // The consumer task ends itself when the sender's end-of-stream
        // marker drains the stream. Wait for that with a bounded grace
        // period; on a lossy lane the marker can be dropped like any other
        // chunk, so a vanished marker falls back to aborting the task (which
        // drops the stream and unregisters the subscription).
        if let Some(mut handle) = self.stream_tasks.lock().await.remove(&req.stream_id) {
            if tokio::time::timeout(std::time::Duration::from_secs(5), &mut handle)
                .await
                .is_err()
            {
                tracing::warn!(
                    "⏱️ Timed out waiting for the end-of-stream marker; aborting the consumer task"
                );
                handle.abort();
            }
        }
        tracing::info!(
            "✅ DataChunk subscription closed for '{}'",
            req.stream_id
        );

        // [...]
        let chunks = *self.chunks_received.lock().await;
        let bytes = *self.bytes_received.lock().await;

        tracing::info!("📊 Transfer complete:");
        tracing::info!("   Chunks received: {}", chunks);
        tracing::info!("   Bytes received: {}", bytes);

        Ok(EndTransferResponse {
            acknowledged: true,
            bytes_received: bytes,
            chunks_received: chunks,
        })
    }
}
