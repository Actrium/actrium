//! Lane (DataChannel) model for the RTC data plane (RFC §"DataChannel model"
//! and §"DataChannel APIs and lane policies").
//!
//! The four logical lanes share one configuration function so callers cannot
//! assemble them inconsistently (RFC: "A single Lane configuration function MUST
//! construct the four Lane variants"). The QoS contract matches the pinned 0.14
//! behavior (`connection_tests.rs::charter_lane_data_channel_config_matches_qos_contract`):
//! only `StreamLatencyFirst` is partial-reliable (unordered, bounded retransmits);
//! every other lane is ordered and fully reliable.

use webrtc::data_channel::RTCDataChannelInit;

use crate::wire::webrtc::rtc::status::LaneKind;

/// Build the `RTCDataChannelInit` for a lane (RTC field shapes: `ordered:
/// bool`, `protocol: String`, `negotiated: Option<u16>`).
///
/// All lanes use in-band DCEP (`negotiated: None`) and an empty sub-protocol
/// label — matching the 0.14 wire contract. `StreamLatencyFirst` is
/// partial-reliable (`ordered: false`, `max_retransmits: Some(3)`); the rest are
/// ordered and fully reliable (`ordered: true`, no retransmit/lifetime cap).
pub(crate) fn lane_config(lane: LaneKind) -> RTCDataChannelInit {
    let mut init = RTCDataChannelInit {
        ordered: true,
        max_packet_life_time: None,
        max_retransmits: None,
        protocol: String::new(),
        negotiated: None,
    };
    if lane == LaneKind::StreamLatencyFirst {
        init.ordered = false;
        init.max_retransmits = Some(3);
    }
    init
}

/// The reliability policy a lane advertises on the wire, used to verify a
/// remote-created channel maps to a known lane (RFC: "unknown, duplicate or
/// semantically unmatched Lanes must be closed and cannot overwrite the current
/// active Lane").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LanePolicy {
    pub(crate) ordered: bool,
    pub(crate) max_retransmits: Option<u16>,
}

/// The expected on-wire policy for a lane.
pub(crate) fn lane_policy(lane: LaneKind) -> LanePolicy {
    match lane {
        LaneKind::StreamLatencyFirst => LanePolicy {
            ordered: false,
            max_retransmits: Some(3),
        },
        _ => LanePolicy {
            ordered: true,
            max_retransmits: None,
        },
    }
}

/// Verify that a remote-created channel maps to a known lane. Returns the lane,
/// or `None` if the channel is NOT a known application lane (unknown label, or
/// an externally-negotiated channel). Mirrors the 0.14 acceptance rule: the wire
/// label is the lane identity; the receiving side does not re-derive the
/// reliability policy (RTC reports `max_retransmits: None` for an incoming
/// partial-reliable channel, so it cannot be validated from `max_retransmits()`).
///
/// The duplicate/unmatched guarantee is enforced separately by the Controller
/// (a second channel with the same label as an already-active lane is closed,
/// never allowed to overwrite it) — this function only decides "is this a known
/// lane at all".
pub(crate) fn verify_channel_metadata(label: &str, negotiated: bool) -> Option<LaneKind> {
    if negotiated {
        // An out-of-band (externally negotiated) channel is not one of our
        // application lanes.
        return None;
    }
    LaneKind::from_label(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_config_matches_014_qos_charter() {
        // Latency-first: unordered + bounded retransmits.
        let latency = lane_config(LaneKind::StreamLatencyFirst);
        assert!(!latency.ordered);
        assert_eq!(latency.max_retransmits, Some(3));
        assert_eq!(latency.max_packet_life_time, None);
        assert_eq!(latency.protocol, "");
        assert_eq!(latency.negotiated, None);

        // All other lanes: ordered + fully reliable.
        for lane in [
            LaneKind::RpcReliable,
            LaneKind::RpcSignal,
            LaneKind::StreamReliable,
        ] {
            let init = lane_config(lane);
            assert!(init.ordered, "{lane:?} must be ordered");
            assert_eq!(init.max_retransmits, None, "{lane:?} must be reliable");
            assert_eq!(init.max_packet_life_time, None);
            assert_eq!(init.protocol, "");
            assert_eq!(init.negotiated, None);
        }
    }

    #[test]
    fn verify_accepts_known_labels_inband() {
        for (label, lane) in [
            ("RPC_RELIABLE", LaneKind::RpcReliable),
            ("RPC_SIGNAL", LaneKind::RpcSignal),
            ("STREAM_RELIABLE", LaneKind::StreamReliable),
            ("STREAM_LATENCY_FIRST", LaneKind::StreamLatencyFirst),
        ] {
            assert_eq!(verify_channel_metadata(label, false), Some(lane));
        }
    }

    #[test]
    fn verify_rejects_unknown_labels_and_negotiated_channels() {
        // Unknown label.
        assert_eq!(verify_channel_metadata("NO_SUCH_LANE", false), None);
        // Externally-negotiated channel (not one of our lanes).
        assert_eq!(verify_channel_metadata("RPC_RELIABLE", true), None);
    }
}
