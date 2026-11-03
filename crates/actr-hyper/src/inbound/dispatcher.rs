//! `InboundDispatcher` — the shared inbound message dispatch logic.
//!
//! The RTC inbound bridge routes messages through this dispatch path:
//!
//! * RPC envelopes (`RpcReliable`/`RpcSignal`): `Request`/`Tell` → Mailbox;
//!   `Response` → pending_requests wake; invalid direction → drop + warn.
//! * Data chunks (`StreamReliable`/`StreamLatencyFirst`): decode →
//!   `DataChunkRegistry.dispatch`.
//!

use std::sync::{Arc, OnceLock};

use actr_framework::Bytes;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{ActrId, ActrType, DataChunk, Direction, PayloadType, RpcEnvelope};
use actr_runtime_mailbox::{Mailbox, MessagePriority};

use crate::lifecycle::node::PeerTypes;
use crate::transport::correlation::PendingRequestsMap;
use crate::transport::handover::HandoverBook;
use crate::wire::{DirectionError, direction_for_routing};
use actr_protocol::transport_control;

use super::DataChunkRegistry;

/// Shared inbound dispatcher: routes decoded inbound messages to the Mailbox
/// (RPC), the pending-requests map (RPC responses), or the `DataChunkRegistry`
/// (stream chunks). Used by the RTC inbound bridge.
pub(crate) struct InboundDispatcher {
    handover: OnceLock<Arc<HandoverBook>>,
    pending_requests: PendingRequestsMap,
    data_chunk_registry: Arc<DataChunkRegistry>,
    /// Sender types learned from Signaling; recorded with each enqueued
    /// request for the inbound ACL.
    peer_types: PeerTypes,
}

impl InboundDispatcher {
    pub(crate) fn new(
        pending_requests: PendingRequestsMap,
        data_chunk_registry: Arc<DataChunkRegistry>,
        peer_types: PeerTypes,
    ) -> Self {
        Self {
            handover: OnceLock::new(),
            pending_requests,
            data_chunk_registry,
            peer_types,
        }
    }

    pub(crate) fn set_handover(&self, book: Arc<HandoverBook>) {
        let _ = self.handover.set(book);
    }

    /// Dispatch one inbound message by its payload type.
    ///
    /// RPC lanes decode the `RpcEnvelope` and route by direction; stream lanes
    /// decode the `DataChunk` and dispatch to the registry. Misclassified or
    /// undecodable payloads are logged and dropped.
    pub(crate) async fn dispatch(
        &self,
        from_bytes: Vec<u8>,
        data: Bytes,
        payload_type: PayloadType,
        mailbox: &Arc<dyn Mailbox>,
    ) {
        match payload_type {
            PayloadType::RpcReliable | PayloadType::RpcSignal => {
                match RpcEnvelope::decode(&data[..]) {
                    Ok(envelope) => {
                        if payload_type == PayloadType::RpcSignal
                            && let Some(nonce) = transport_control::acknowledgement_nonce(&envelope)
                        {
                            if let (Some(book), Ok(peer)) =
                                (self.handover.get(), ActrId::decode(from_bytes.as_slice()))
                            {
                                book.acknowledge(&peer, nonce);
                            }
                            return;
                        }
                        let from_type = ActrId::decode(&from_bytes[..])
                            .ok()
                            .and_then(|from| self.peer_types.get(&from));
                        Self::handle_envelope(
                            envelope,
                            from_bytes,
                            from_type,
                            data,
                            payload_type,
                            self.pending_requests.clone(),
                            mailbox.clone(),
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!("Failed to deserialize RpcEnvelope: {:?}", e);
                    }
                }
            }
            PayloadType::StreamReliable | PayloadType::StreamLatencyFirst => {
                Self::dispatch_data_chunk(
                    &self.data_chunk_registry,
                    self.handover.get(),
                    &from_bytes,
                    &data,
                    payload_type,
                )
                .await;
            }
            unexpected => {
                tracing::warn!(
                    payload_type = ?unexpected,
                    "unexpected payload type in inbound dispatch"
                );
            }
        }
    }

    /// Handle an RPC envelope: route by direction.
    ///
    /// * `Request`/`Tell`: enqueue to Mailbox, skip pending lookup.
    /// * `Response`: wake the waiting caller if a pending entry exists;
    ///   otherwise drop as an orphan late response — never enqueue (#255).
    /// * Missing/`Unspecified`/unknown: warn and drop.
    ///
    /// Exposed pub(crate) so the 0.14 gate can create an opentelemetry span
    /// from the decoded envelope before dispatching.
    pub(crate) async fn handle_envelope(
        envelope: RpcEnvelope,
        from_bytes: Vec<u8>,
        from_type: Option<ActrType>,
        data: Bytes,
        payload_type: PayloadType,
        pending_requests: PendingRequestsMap,
        mailbox: Arc<dyn Mailbox>,
    ) {
        let request_id = envelope.request_id.clone();

        let direction = match direction_for_routing(envelope.direction) {
            Ok(direction) => direction,
            Err(error) => {
                let peer = Self::peer_for_log(&from_bytes);
                let reason = match error {
                    DirectionError::Missing => "missing",
                    DirectionError::Unspecified => "unspecified",
                    DirectionError::Unknown => "unknown",
                };
                tracing::warn!(
                    request_id = %request_id,
                    peer = %peer,
                    route_key = %envelope.route_key,
                    direction = ?envelope.direction,
                    reason = %reason,
                    "rpc.invalid_direction_dropped: invalid RpcEnvelope.direction; dropping"
                );
                return;
            }
        };

        if matches!(direction, Direction::Request | Direction::Tell) {
            Self::enqueue_request(
                from_bytes,
                from_type,
                data,
                payload_type,
                mailbox,
                &request_id,
            )
            .await;
            return;
        }

        // Direction::Response: complete the waiting caller via the shared
        // correlation map (orphan late replies warn and drop).
        pending_requests.complete_from_envelope(envelope, || Self::peer_for_log(&from_bytes));
    }

    /// Enqueue an explicit inbound RPC request or tell to the Mailbox with
    /// priority derived from the inbound `PayloadType`.
    async fn enqueue_request(
        from_bytes: Vec<u8>,
        from_type: Option<ActrType>,
        data: Bytes,
        payload_type: PayloadType,
        mailbox: Arc<dyn Mailbox>,
        request_id: &str,
    ) {
        tracing::debug!("Received RPC Request: request_id={}", request_id);

        let priority = match payload_type {
            PayloadType::RpcSignal => MessagePriority::High,
            _ => MessagePriority::Normal,
        };

        tracing::info!(request_id = %request_id, "rpc.mailbox.enqueue");
        let from_type = from_type.map(|actor_type| actor_type.encode_to_vec());
        match mailbox
            .enqueue(from_bytes, from_type, data.to_vec(), priority)
            .await
        {
            Ok(msg_id) => {
                tracing::debug!(
                    "RPC message enqueued to Mailbox: msg_id={}, priority={:?}",
                    msg_id,
                    priority
                );
            }
            Err(e) => {
                tracing::error!("Mailbox enqueue failed: {:?}", e);
            }
        }
    }

    /// Decode a DataChunk and dispatch it to the per-stream registry worker.
    async fn dispatch_data_chunk(
        registry: &DataChunkRegistry,
        handover: Option<&Arc<HandoverBook>>,
        from_bytes: &[u8],
        data: &[u8],
        payload_type: PayloadType,
    ) {
        let chunk = match DataChunk::decode(data) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to deserialize DataChunk: {:?}", e);
                return;
            }
        };
        tracing::debug!(
            "Received DataChunk: stream_id={}, seq={}, {} bytes",
            chunk.stream_id,
            chunk.sequence,
            chunk.payload.len()
        );
        let sender_id = match ActrId::decode(from_bytes) {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to decode sender ActrId: {:?}", e);
                return;
            }
        };
        if payload_type == PayloadType::StreamReliable
            && let Some(nonce) = transport_control::barrier_nonce(&chunk)
        {
            if let Some(book) = handover {
                book.receive_barrier(sender_id, nonce).await;
            }
            return;
        }
        registry.dispatch(chunk, sender_id, payload_type).await;
    }

    fn peer_for_log(from_bytes: &[u8]) -> String {
        if from_bytes.is_empty() {
            return "unavailable".to_string();
        }
        ActrId::decode(from_bytes)
            .map(|peer| peer.to_string_repr())
            .unwrap_or_else(|e| format!("decode_failed:{e}"))
    }
}
