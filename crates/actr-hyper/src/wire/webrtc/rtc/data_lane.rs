//! `RtcDataLane` — a [`DataLane`] implementation that routes sends through the
//! RTC `RtcPeerControllerHandle` send path (LaneHandle + SendPermit).
//!
//! This is the production-wiring bridge: the transport layer's `PeerTransport`
//! calls `DataLane::send()` on the best available connection; this lane forwards
//! to the RTC controller's `send()`/`try_send()` (Snapshot preflight +
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

use crate::transport::{DataLane, NetworkError, NetworkResult, SendAttemptResult};
use crate::wire::webrtc::rtc::peer::handle::RtcPeerControllerHandle;
use crate::wire::webrtc::rtc::peer::send::RtcSendError;
use crate::wire::webrtc::rtc::status::LaneKind;

/// A transport-layer [`DataLane`] backed by the RTC send path.
///
/// Created by [`super::wire_handle::RtcWireHandle`] when the transport layer
/// requests a lane for a `PayloadType`. Sends go through the RTC
/// `RtcPeerControllerHandle`: Snapshot preflight → SendPermit → LaneHandle →
/// `DataChannel.send()`/`try_send()`. Fragment-header encoding is handled
/// inside the controller handle, so this lane is a thin pass-through.
pub(crate) struct RtcDataLane {
    handle: RtcPeerControllerHandle,
    lane: LaneKind,
    /// Whether this lane should use non-blocking `try_send` (latency-first) or
    /// blocking `send` (reliable lanes).
    non_blocking: bool,
}

impl std::fmt::Debug for RtcDataLane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcDataLane")
            .field("lane", &self.lane)
            .finish()
    }
}

impl RtcDataLane {
    pub(crate) fn new(handle: RtcPeerControllerHandle, lane: LaneKind) -> Self {
        let non_blocking = lane == LaneKind::StreamLatencyFirst;
        Self {
            handle,
            lane,
            non_blocking,
        }
    }

    /// Map a RTC send error to a transport `NetworkError`.
    fn map_error(err: RtcSendError) -> NetworkError {
        match err {
            RtcSendError::ConnectionNotReady => {
                NetworkError::ConnectionError("RTC peer not sendable".into())
            }
            RtcSendError::DataChannelClosed => {
                NetworkError::DataChannelClosed("RTC data channel closed".into())
            }
            RtcSendError::DataChannelBufferFull => {
                NetworkError::ConnectionError("RTC data channel buffer full".into())
            }
            RtcSendError::DataChunkDeliveryUncertain => {
                NetworkError::ConnectionError("RTC data chunk delivery uncertain".into())
            }
            RtcSendError::MessageTooLarge => {
                NetworkError::InvalidArgument("RTC message too large for lane fragmentation".into())
            }
        }
    }
}

#[async_trait]
impl DataLane for RtcDataLane {
    async fn send(&self, data: bytes::Bytes) -> NetworkResult<()> {
        self.send_with_progress(data).await.into_result()
    }

    async fn send_with_progress(&self, data: bytes::Bytes) -> SendAttemptResult {
        let result = if self.non_blocking {
            self.handle.send_with_progress(self.lane, data, true).await
        } else {
            self.handle.send_with_progress(self.lane, data, false).await
        };
        let progress = result.progress;
        let proven_pre_send = result.proven_pre_send;
        match result.result {
            Ok(()) => SendAttemptResult::submitted(),
            Err(error) => {
                if matches!(
                    error,
                    RtcSendError::DataChannelClosed | RtcSendError::DataChunkDeliveryUncertain
                ) {
                    tracing::debug!(
                        peer = %self.handle.peer_id().to_string_repr(),
                        lane = ?self.lane,
                        error = ?error,
                        "RTC lane send hit stale data-plane state"
                    );
                }
                SendAttemptResult::failed_with_proof(
                    Self::map_error(error),
                    progress,
                    proven_pre_send,
                )
            }
        }
    }

    fn lane_type(&self) -> &'static str {
        "webrtc"
    }

    fn is_healthy(&self) -> bool {
        self.handle.subscribe().borrow().sendable
    }
}

/// Map a `PayloadType` to a RTC `LaneKind`, returning `None` for non-lane
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

    /// Charter test: prove the RTC send path produces the exact same wire
    /// format as 0.14 `WebRtcDataLane`. Both use the shared codec from
    /// `transport/lane.rs`, so this test pins the contract: a single-fragment
    /// message has the 8-byte header followed by the raw payload.
    #[test]
    fn rtc_fragment_header_matches_014_wire_format() {
        let payload = bytes::Bytes::from_static(b"hello rtc");
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
    fn rtc_multi_fragment_reassembly_matches_original() {
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
            RtcDataLane::map_error(RtcSendError::DataChannelClosed),
            NetworkError::DataChannelClosed(_)
        ));
        assert!(matches!(
            RtcDataLane::map_error(RtcSendError::ConnectionNotReady),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            RtcDataLane::map_error(RtcSendError::DataChunkDeliveryUncertain),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            RtcDataLane::map_error(RtcSendError::DataChannelBufferFull),
            NetworkError::ConnectionError(_)
        ));
        assert!(matches!(
            RtcDataLane::map_error(RtcSendError::MessageTooLarge),
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
