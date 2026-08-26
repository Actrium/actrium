//! `V020DataLane` — a [`DataLane`] implementation that routes sends through the
//! v0.20 `RtcPeerControllerHandle` send path (LaneHandle + SendPermit).
//!
//! This is the production-wiring bridge: the transport layer's `PeerTransport`
//! calls `DataLane::send()` on the best available connection; this lane forwards
//! to the v0.20 controller's `send()`/`try_send()` (Snapshot preflight +
//! SendPermit generation/cancellation re-validation).
//!
//! The inbound direction is push-based (the lane pump routes payloads to the
//! `InboundRouter` callback directly, bypassing the Controller and the
//! transport's `recv()` path), so `recv()` is not supported here.
//!
//! ## Fragment header (wire compatibility with 0.14)
//!
//! The 8-byte fragment header (`[msg_id u32 BE][frag_index u16 BE][total_frags
//! u16 BE]`) is prepended to every DataChannel message inside
//! `RtcPeerControllerHandle::send_with_permit()`, so all send paths — whether
//! through this DataLane or directly via the controller handle — produce the
//! same wire format as 0.14 `WebRtcDataLane`. The receiver (in `lane_pump`)
//! strips the header and reassembles multi-fragment messages before dispatch.

use async_trait::async_trait;

use actr_protocol::PayloadType;

use crate::transport::{DataLane, NetworkError, NetworkResult};
use crate::wire::webrtc::v020::peer::handle::RtcPeerControllerHandle;
use crate::wire::webrtc::v020::peer::send::RtcSendError;
use crate::wire::webrtc::v020::status::LaneKind;

/// A transport-layer [`DataLane`] backed by the v0.20 send path.
///
/// Created by [`super::wire_handle::V020WireHandle`] when the transport layer
/// requests a lane for a `PayloadType`. Sends go through the v0.20
/// `RtcPeerControllerHandle`: Snapshot preflight → SendPermit → LaneHandle →
/// `DataChannel.send()`/`try_send()`. Fragment-header encoding is handled
/// inside the controller handle, so this lane is a thin pass-through.
pub(crate) struct V020DataLane {
    handle: RtcPeerControllerHandle,
    lane: LaneKind,
    /// Whether this lane should use non-blocking `try_send` (latency-first) or
    /// blocking `send` (reliable lanes).
    non_blocking: bool,
}

impl std::fmt::Debug for V020DataLane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V020DataLane")
            .field("lane", &self.lane)
            .finish()
    }
}

impl V020DataLane {
    pub(crate) fn new(handle: RtcPeerControllerHandle, lane: LaneKind) -> Self {
        let non_blocking = lane == LaneKind::StreamLatencyFirst;
        Self {
            handle,
            lane,
            non_blocking,
        }
    }

    /// Map a v0.20 send error to a transport `NetworkError`.
    fn map_error(err: RtcSendError) -> NetworkError {
        match err {
            RtcSendError::ConnectionNotReady => {
                NetworkError::ConnectionError("v0.20 peer not sendable".into())
            }
            RtcSendError::DataChannelClosed => {
                NetworkError::DataChannelClosed("v0.20 data channel closed".into())
            }
            RtcSendError::DataChannelBufferFull => {
                NetworkError::ConnectionError("v0.20 data channel buffer full".into())
            }
            RtcSendError::DataChunkDeliveryUncertain => {
                NetworkError::ConnectionError("v0.20 data chunk delivery uncertain".into())
            }
            RtcSendError::MessageTooLarge => NetworkError::InvalidArgument(
                "v0.20 message too large for lane fragmentation".into(),
            ),
        }
    }
}

#[async_trait]
impl DataLane for V020DataLane {
    async fn send(&self, data: bytes::Bytes) -> NetworkResult<()> {
        let result = if self.non_blocking {
            self.handle.try_send(self.lane, data).await
        } else {
            self.handle.send(self.lane, data).await
        };
        result.map_err(Self::map_error)
    }

    fn lane_type(&self) -> &'static str {
        "webrtc-v020"
    }

    fn is_healthy(&self) -> bool {
        self.handle.subscribe().borrow().sendable
    }
}

/// Map a `PayloadType` to a v0.20 `LaneKind`, returning `None` for non-lane
/// types (e.g. media).
pub(crate) fn payload_type_to_lane(payload_type: PayloadType) -> Option<LaneKind> {
    LaneKind::from_payload_type(payload_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{
        DC_MAX_PAYLOAD_SIZE, FRAGMENT_HEADER_SIZE, ReassemblyBuffer, decode_fragment_header,
        encode_fragment_header,
    };

    /// Charter test: prove the v0.20 send path produces the exact same wire
    /// format as 0.14 `WebRtcDataLane`. Both use the shared codec from
    /// `transport/lane.rs`, so this test pins the contract: a single-fragment
    /// message has the 8-byte header followed by the raw payload.
    #[test]
    fn v020_fragment_header_matches_014_wire_format() {
        let payload = bytes::Bytes::from_static(b"hello v020");
        let msg_id = 42u32;

        // Encode exactly as send_with_permit does for a single-fragment msg.
        let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + payload.len());
        encode_fragment_header(&mut buf, msg_id, 0, 1);
        buf.extend_from_slice(&payload);
        let frame = bytes::Bytes::from(buf);

        // Decode exactly as lane_pump does.
        let (dec_msg_id, dec_frag_index, dec_total_frags, dec_payload) =
            decode_fragment_header(frame).expect("decode must succeed");

        assert_eq!(dec_msg_id, msg_id);
        assert_eq!(dec_frag_index, 0);
        assert_eq!(dec_total_frags, 1);
        assert_eq!(dec_payload, payload);
    }

    /// Charter test: a message split across multiple fragments reassembles to
    /// the original payload, byte-identical.
    #[test]
    fn v020_multi_fragment_reassembly_matches_original() {
        // A payload that exceeds DC_MAX_PAYLOAD_SIZE -> 2 fragments.
        let data = vec![0xABu8; DC_MAX_PAYLOAD_SIZE + 1];
        let msg_id = 7u32;
        let total_frags = 2u16;

        let (part0, part1) = data.split_at(DC_MAX_PAYLOAD_SIZE);

        let mut frame0 = Vec::with_capacity(FRAGMENT_HEADER_SIZE + part0.len());
        encode_fragment_header(&mut frame0, msg_id, 0, total_frags);
        frame0.extend_from_slice(part0);

        let mut frame1 = Vec::with_capacity(FRAGMENT_HEADER_SIZE + part1.len());
        encode_fragment_header(&mut frame1, msg_id, 1, total_frags);
        frame1.extend_from_slice(part1);

        let mut reassembly = ReassemblyBuffer::new();
        let (m, f, t, p) = decode_fragment_header(bytes::Bytes::from(frame0)).unwrap();
        assert!(reassembly.insert(m, f, t, p).is_none());

        let (m, f, t, p) = decode_fragment_header(bytes::Bytes::from(frame1)).unwrap();
        let complete = reassembly.insert(m, f, t, p).expect("should be complete");

        assert_eq!(complete.as_ref(), data.as_slice());
    }

    #[test]
    fn closed_channel_bridge_keeps_data_channel_closed_identity() {
        // A regression back to generic ConnectionError here would bypass
        // stale-transport retirement while the internal mapping tests still
        // pass, so pin the production bridge directly.
        assert!(matches!(
            V020DataLane::map_error(RtcSendError::DataChannelClosed),
            NetworkError::DataChannelClosed(_)
        ));
        assert!(matches!(
            V020DataLane::map_error(RtcSendError::ConnectionNotReady),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            V020DataLane::map_error(RtcSendError::DataChunkDeliveryUncertain),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            V020DataLane::map_error(RtcSendError::DataChannelBufferFull),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            V020DataLane::map_error(RtcSendError::MessageTooLarge),
            NetworkError::InvalidArgument(_)
        ));
    }

    #[test]
    fn payload_type_maps_to_all_four_lanes() {
        assert_eq!(
            payload_type_to_lane(PayloadType::RpcReliable),
            Some(LaneKind::RpcReliable)
        );
        assert_eq!(
            payload_type_to_lane(PayloadType::RpcSignal),
            Some(LaneKind::RpcSignal)
        );
        assert_eq!(
            payload_type_to_lane(PayloadType::StreamReliable),
            Some(LaneKind::StreamReliable)
        );
        assert_eq!(
            payload_type_to_lane(PayloadType::StreamLatencyFirst),
            Some(LaneKind::StreamLatencyFirst)
        );
    }
}
