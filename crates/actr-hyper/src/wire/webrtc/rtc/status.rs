//! Peer status vocabulary for the RTC architecture (Stage 2).
//!
//! These types define the *authoritative* per-peer snapshot the
//! `RtcPeerController` publishes (RFC §"Sendability and public state") plus the
//! lane/status enums shared across the controller, handle, and notification hub.
//! The controller is the sole producer of [`RtcPeerSnapshot`] (invariant #7);
//! everything else (handle, gate, business, notification hub) reads it.

use actr_framework::WebRtcPeerStatus;
use actr_protocol::{ActrId, PayloadType};

use crate::transport::WebRtcTransportLifecycle;

/// The four logical data lanes (RFC §"DataChannel model").
///
/// `RpcReliable` is created during the initial connection and is the peer-level
/// *readiness anchor*; the others may be lazily created. The wire label is the
/// exact `PayloadType` name the two peers negotiate over — pinned by the Stage 1
/// characterization tests (`connection_tests.rs`) and unchanged from 0.14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LaneKind {
    RpcReliable,
    RpcSignal,
    StreamReliable,
    StreamLatencyFirst,
}

impl LaneKind {
    /// All four lanes, in `PayloadType` slot order.
    pub(crate) const ALL: [LaneKind; 4] = [
        LaneKind::RpcReliable,
        LaneKind::RpcSignal,
        LaneKind::StreamReliable,
        LaneKind::StreamLatencyFirst,
    ];

    /// The lane that anchors peer-level readiness (RFC: "`RpcReliable` MUST be
    /// created during the initial connection and serve as the peer-level
    /// readiness anchor").
    pub(crate) const READINESS_ANCHOR: LaneKind = LaneKind::RpcReliable;

    /// The exact DataChannel label negotiated on the wire.
    pub(crate) fn wire_label(self) -> &'static str {
        self.to_payload_type().as_str_name()
    }

    pub(crate) fn to_payload_type(self) -> PayloadType {
        match self {
            LaneKind::RpcReliable => PayloadType::RpcReliable,
            LaneKind::RpcSignal => PayloadType::RpcSignal,
            LaneKind::StreamReliable => PayloadType::StreamReliable,
            LaneKind::StreamLatencyFirst => PayloadType::StreamLatencyFirst,
        }
    }

    /// Map a `PayloadType` to a data lane, or `None` for non-lane types (e.g.
    /// `MediaRtp`, which is a media track, not a data channel).
    pub(crate) fn from_payload_type(payload_type: PayloadType) -> Option<LaneKind> {
        match payload_type {
            PayloadType::RpcReliable => Some(LaneKind::RpcReliable),
            PayloadType::RpcSignal => Some(LaneKind::RpcSignal),
            PayloadType::StreamReliable => Some(LaneKind::StreamReliable),
            PayloadType::StreamLatencyFirst => Some(LaneKind::StreamLatencyFirst),
            _ => None,
        }
    }

    /// Map a wire DataChannel label back to a lane, or `None` if unknown.
    pub(crate) fn from_label(label: &str) -> Option<LaneKind> {
        Self::from_payload_type(PayloadType::from_str_name(label)?)
    }
}

/// Whether the selected ICE candidate pair is direct or relayed (RFC §"RtcPeerSnapshot").
///
/// Derived asynchronously from stats; never blocks `Connected`/`sendable`
/// (RFC: "`network_path` is updated asynchronously via stats and does not
/// participate in sendability").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkPath {
    Unknown,
    Direct,
    Relayed,
}

/// Stable, FFI-safe reason a peer is unavailable. Aliased to the public
/// framework enum (single source of truth; the FFI-facing snapshot conversion
/// is then a plain field copy).
pub(crate) use actr_framework::WebRtcUnavailableReason as RtcUnavailableReason;

/// Authoritative, subscribable snapshot of one logical RTC peer (RFC §"RtcPeerSnapshot").
///
/// Published by the controller on every relevant state change with a bumped
/// `revision`; subscribers replay the latest immediately on subscribe. Equality
/// is structural so the notification hub can de-duplicate repeat publications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RtcPeerSnapshot {
    pub(crate) peer: ActrId,
    /// Controller-owned negotiation role. Kept internal because role is a
    /// protocol/testing observation, not a business send-admission signal.
    pub(crate) role: super::peer::state::Role,
    /// Monotonic logical connection-flight epoch within one Controller.
    /// Hard Recovery keeps this value; a fresh attempt from terminal `Idle`
    /// increments it so delayed retirement cannot target the replacement.
    pub(crate) transport_epoch: u64,
    pub(crate) pc_generation: u64,
    /// Current ICE restart generation within `pc_generation`. Soft recovery
    /// restarts ICE in place, so this is the third component of the per-peer
    /// identity tuple (peer, `pc_generation`, `ice_generation`) that
    /// peer-scoped recovery coverage validates terminal facts against
    /// (RFC-0400 "Per-peer coverage for peer-scoped recovery").
    pub(crate) ice_generation: u64,
    pub(crate) revision: u64,
    pub(crate) status: WebRtcPeerStatus,
    pub(crate) sendable: bool,
    pub(crate) network_path: NetworkPath,
    pub(crate) unavailable_reason: Option<RtcUnavailableReason>,
    /// Signaling messages this peer dropped because the shared outbound queue
    /// was full (N2 backpressure metric).
    pub(crate) signaling_dropped: u64,
    /// Mailbox overflow events that triggered Hard Recovery (O1). The
    /// structural-event handler dropped critical events; the controller
    /// detected the gap and rebuilt the PeerConnection (RFC: "Handler
    /// mailbox depth and overflow [must be observable]").
    pub(crate) mailbox_overflows: u64,
}

impl RtcPeerSnapshot {
    /// The initial snapshot for a freshly created controller: public `Idle`,
    /// not sendable, no session yet.
    pub(crate) fn idle(peer: ActrId) -> Self {
        Self {
            peer,
            role: super::peer::state::Role::Undetermined,
            transport_epoch: 0,
            pc_generation: 0,
            ice_generation: 0,
            revision: 0,
            status: WebRtcPeerStatus::Idle,
            sendable: false,
            network_path: NetworkPath::Unknown,
            unavailable_reason: None,
            signaling_dropped: 0,
            mailbox_overflows: 0,
        }
    }

    /// Project to the public, FFI-safe peer state for the node's status
    /// APIs and the `on_webrtc_state_changed` hook (S6-4/S6-5). `network_path`
    /// and the derived compat `relayed` stay `Unknown`/`None` until stats
    /// resolve the selected pair (S6-3).
    pub(crate) fn to_public(&self) -> actr_framework::WebRtcPeerState {
        actr_framework::WebRtcPeerState {
            peer: self.peer.clone(),
            pc_generation: self.pc_generation,
            revision: self.revision,
            status: self.status,
            sendable: self.sendable,
            network_path: match self.network_path {
                NetworkPath::Unknown => actr_framework::WebRtcNetworkPath::Unknown,
                NetworkPath::Direct => actr_framework::WebRtcNetworkPath::Direct,
                NetworkPath::Relayed => actr_framework::WebRtcNetworkPath::Relayed,
            },
            relayed: match self.network_path {
                NetworkPath::Direct => Some(false),
                NetworkPath::Relayed => Some(true),
                NetworkPath::Unknown => None,
            },
            unavailable_reason: self.unavailable_reason,
        }
    }
}

/// Terminal logical-transport edge emitted by the Controller and consumed by
/// PeerGate/PeerTransport. Physical generation retirement is a separate edge.
#[derive(Debug, Clone)]
pub(crate) struct PeerTransportRetirement {
    pub(crate) lifecycle: WebRtcTransportLifecycle,
    pub(crate) reason: RtcUnavailableReason,
}

/// Spawn a forwarder mapping an internal snapshot watch channel to the public
/// `WebRtcPeerState` type, seeded with the current value (S6-4/S6-5: "mobile
/// late subscribe immediately gets current state"). The forwarder task ends
/// when the controller drops its snapshot sender.
#[cfg(test)]
pub(crate) fn subscribe_public(
    mut rx: tokio::sync::watch::Receiver<RtcPeerSnapshot>,
) -> tokio::sync::watch::Receiver<actr_framework::WebRtcPeerState> {
    let (tx, out) = tokio::sync::watch::channel(rx.borrow().to_public());
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            if tx.send(rx.borrow_and_update().to_public()).is_err() {
                // The business subscriber was dropped. Do not retain one
                // forwarding task per historical subscribe call until the
                // underlying peer itself eventually closes.
                break;
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_wire_labels_match_pinned_014_contract() {
        // Mirror of the Stage 1 characterization test, kept here so the RTC
        // lane model cannot drift from the wire contract it must reproduce.
        assert_eq!(LaneKind::RpcReliable.wire_label(), "RPC_RELIABLE");
        assert_eq!(LaneKind::RpcSignal.wire_label(), "RPC_SIGNAL");
        assert_eq!(LaneKind::StreamReliable.wire_label(), "STREAM_RELIABLE");
        assert_eq!(
            LaneKind::StreamLatencyFirst.wire_label(),
            "STREAM_LATENCY_FIRST"
        );
    }

    #[test]
    fn lane_round_trips_through_payload_type() {
        for lane in LaneKind::ALL {
            assert_eq!(
                LaneKind::from_payload_type(lane.to_payload_type()),
                Some(lane)
            );
        }
        assert_eq!(LaneKind::from_payload_type(PayloadType::MediaRtp), None);
    }

    #[test]
    fn readiness_anchor_is_rpc_reliable() {
        assert_eq!(LaneKind::READINESS_ANCHOR, LaneKind::RpcReliable);
    }

    /// S6-4/S6-5: the public projection maps the stats-resolved network path
    /// to the tri-state relayed flag.
    #[test]
    fn to_public_maps_network_path_to_relayed() {
        let peer = ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 1,
        };
        let mut snap = RtcPeerSnapshot::idle(peer);
        assert_eq!(snap.to_public().relayed, None);
        snap.network_path = NetworkPath::Direct;
        assert_eq!(snap.to_public().relayed, Some(false));
        snap.network_path = NetworkPath::Relayed;
        assert_eq!(snap.to_public().relayed, Some(true));
    }

    /// S6-4: a public subscribe replays the current state immediately and
    /// forwards later changes (RFC: "SUBSCRIBE MUST replay the latest
    /// Snapshot immediately").
    #[tokio::test]
    async fn subscribe_public_replays_initial_then_forwards() {
        let peer = ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 2,
        };
        let mut snap = RtcPeerSnapshot::idle(peer);
        snap.revision = 5;
        let (tx, rx) = tokio::sync::watch::channel(snap.clone());

        let mut public_rx = subscribe_public(rx);
        // Initial replay: the current (revision-5) state is immediately visible.
        assert_eq!(public_rx.borrow().revision, 5);

        snap.revision = 6;
        snap.sendable = true;
        snap.status = WebRtcPeerStatus::Connected;
        tx.send(snap).unwrap();

        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(5),
            public_rx.changed(),
        )
        .await
        .expect("update did not arrive")
        .expect("snapshot channel closed unexpectedly");
        assert_eq!(public_rx.borrow().revision, 6);
        assert!(public_rx.borrow().sendable);
    }

    #[tokio::test]
    async fn subscribe_public_releases_forwarder_after_business_drop() {
        let peer = ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 3,
        };
        let mut snap = RtcPeerSnapshot::idle(peer);
        let (tx, rx) = tokio::sync::watch::channel(snap.clone());
        let public_rx = subscribe_public(rx);
        assert_eq!(tx.receiver_count(), 1);

        drop(public_rx);
        snap.revision += 1;
        tx.send(snap).expect("source watch remains open");
        crate::timer::timeout(
            crate::timer::ids::WEBRTC_PEER_SENDABLE,
            std::time::Duration::from_secs(5),
            async {
                while tx.receiver_count() != 0 {
                    tokio::task::yield_now().await;
                }
            },
        )
        .await
        .expect("dropped business subscription retained its forwarding task");
    }
}
