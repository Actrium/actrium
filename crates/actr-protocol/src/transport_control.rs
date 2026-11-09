//! Private SDK controls for ordered handover between peer transports.
//!
//! A barrier shares the ordered reliable stream lane with application chunks.
//! Its acknowledgement is sent only after earlier chunks were admitted to the
//! receiver. Older SDKs discard the unregistered stream and the unspecified RPC
//! direction; they never dispatch these controls as application RPCs.

use crate::{DataChunk, Direction, RpcEnvelope};

const STREAM_PREFIX: &str = "__actr_transport_barrier_v1__/";
const ACK_ROUTE: &str = "__actr.transport.v1.BarrierAck";
const MAGIC: &[u8] = b"actr-ordered-handover-v1";

fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Encode an SDK barrier using a fresh 128-bit hexadecimal nonce.
pub fn barrier(nonce: &str) -> DataChunk {
    assert!(
        valid_nonce(nonce),
        "handover nonce must be 32 lowercase hex digits"
    );
    DataChunk {
        stream_id: format!("{STREAM_PREFIX}{nonce}"),
        sequence: 0,
        payload: MAGIC.to_vec().into(),
        metadata: Vec::new(),
        timestamp_ms: None,
        end_of_stream: None,
    }
}

/// Recognize the complete reserved barrier shape, never just a stream prefix.
pub fn barrier_nonce(chunk: &DataChunk) -> Option<&str> {
    let nonce = chunk.stream_id.strip_prefix(STREAM_PREFIX)?;
    (valid_nonce(nonce)
        && chunk.sequence == 0
        && &chunk.payload[..] == MAGIC
        && chunk.metadata.is_empty()
        && chunk.timestamp_ms.is_none())
    .then_some(nonce)
}

/// Build an acknowledgement that legacy RPC dispatchers safely discard.
pub fn acknowledgement(nonce: &str) -> RpcEnvelope {
    assert!(
        valid_nonce(nonce),
        "handover nonce must be 32 lowercase hex digits"
    );
    RpcEnvelope {
        route_key: ACK_ROUTE.into(),
        payload: Some(MAGIC.to_vec().into()),
        error: None,
        direction: Some(Direction::Unspecified as i32),
        traceparent: None,
        tracestate: None,
        request_id: nonce.into(),
        metadata: Vec::new(),
        timeout_ms: 0,
    }
}

/// Recognize a complete SDK acknowledgement before normal RPC validation.
pub fn acknowledgement_nonce(envelope: &RpcEnvelope) -> Option<&str> {
    (envelope.route_key == ACK_ROUTE
        && valid_nonce(&envelope.request_id)
        && envelope
            .payload
            .as_ref()
            .is_some_and(|payload| &payload[..] == MAGIC)
        && envelope.direction == Some(Direction::Unspecified as i32)
        && envelope.error.is_none()
        && envelope.metadata.is_empty()
        && envelope.traceparent.is_none()
        && envelope.tracestate.is_none()
        && envelope.timeout_ms == 0)
        .then_some(envelope.request_id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prost::Message;

    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn controls_round_trip_and_require_the_entire_reserved_shape() {
        let chunk = DataChunk::decode(barrier(NONCE).encode_to_vec().as_slice()).unwrap();
        assert_eq!(barrier_nonce(&chunk), Some(NONCE));
        let mut application_chunk = chunk.clone();
        application_chunk.sequence = 1;
        assert_eq!(barrier_nonce(&application_chunk), None);
        application_chunk = chunk;
        application_chunk.payload = b"application data".to_vec().into();
        assert_eq!(barrier_nonce(&application_chunk), None);

        let mut ack =
            RpcEnvelope::decode(acknowledgement(NONCE).encode_to_vec().as_slice()).unwrap();
        assert_eq!(acknowledgement_nonce(&ack), Some(NONCE));
        ack.direction = Some(Direction::Tell as i32);
        assert_eq!(acknowledgement_nonce(&ack), None);
        ack.direction = Some(Direction::Unspecified as i32);
        ack.request_id = "a different request".into();
        assert_eq!(acknowledgement_nonce(&ack), None);
    }
}
