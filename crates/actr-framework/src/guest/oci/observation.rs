//! Translate OCI observations to the Workload lifecycle API.

use std::time::{Duration, UNIX_EPOCH};

use actr_protocol::ActorResult;
use actr_protocol::oci::{EventKind, Observation};

use crate::{
    BackpressureEvent, CredentialEvent, PeerEvent, WebRtcNetworkPath, WebRtcPeerState,
    WebRtcPeerStatus, WebRtcUnavailableReason, Workload,
};

use super::{GuestContext, protocol_error};

pub(super) async fn deliver<W: Workload>(
    workload: &W,
    context: &GuestContext,
    event: Observation,
) -> ActorResult<()> {
    let kind = event.kind();
    match kind {
        EventKind::SignalingConnecting => workload.on_signaling_connecting(Some(context)).await,
        EventKind::SignalingConnected => workload.on_signaling_connected(Some(context)).await,
        EventKind::SignalingDisconnected => workload.on_signaling_disconnected(context).await,
        EventKind::WebsocketConnecting
        | EventKind::WebsocketConnected
        | EventKind::WebsocketDisconnected
        | EventKind::WebrtcConnecting
        | EventKind::WebrtcConnected
        | EventKind::WebrtcDisconnected => {
            let peer = PeerEvent {
                peer: event.peer.ok_or_else(protocol_error)?,
                relayed: event.relayed,
                status: event.peer_status.map(peer_status).transpose()?,
            };
            match kind {
                EventKind::WebsocketConnecting => {
                    workload.on_websocket_connecting(context, &peer).await
                }
                EventKind::WebsocketConnected => {
                    workload.on_websocket_connected(context, &peer).await
                }
                EventKind::WebsocketDisconnected => {
                    workload.on_websocket_disconnected(context, &peer).await
                }
                EventKind::WebrtcConnecting => workload.on_webrtc_connecting(context, &peer).await,
                EventKind::WebrtcConnected => workload.on_webrtc_connected(context, &peer).await,
                EventKind::WebrtcDisconnected => {
                    workload.on_webrtc_disconnected(context, &peer).await
                }
                _ => unreachable!(),
            }
        }
        EventKind::CredentialRenewed | EventKind::CredentialExpiring => {
            if event.expires_nanos >= 1_000_000_000 {
                return Err(protocol_error());
            }
            let expiry = UNIX_EPOCH
                .checked_add(Duration::new(event.expires_seconds, event.expires_nanos))
                .ok_or_else(protocol_error)?;
            let event = CredentialEvent { new_expiry: expiry };
            if kind == EventKind::CredentialRenewed {
                workload.on_credential_renewed(context, &event).await;
            } else {
                workload.on_credential_expiring(context, &event).await;
            }
        }
        EventKind::MailboxBackpressure => {
            let event = BackpressureEvent {
                queue_len: event.queue_len.try_into().map_err(|_| protocol_error())?,
                threshold: event.threshold.try_into().map_err(|_| protocol_error())?,
            };
            workload.on_mailbox_backpressure(context, &event).await;
        }
        EventKind::WebrtcStateChanged => {
            let network_path = match event.network_path {
                0 => WebRtcNetworkPath::Unknown,
                1 => WebRtcNetworkPath::Direct,
                2 => WebRtcNetworkPath::Relayed,
                _ => return Err(protocol_error()),
            };
            let snapshot = WebRtcPeerState {
                peer: event.peer.ok_or_else(protocol_error)?,
                pc_generation: event.pc_generation,
                revision: event.revision,
                status: peer_status(event.peer_status.ok_or_else(protocol_error)?)?,
                sendable: event.sendable,
                network_path,
                relayed: event.relayed,
                unavailable_reason: event.unavailable_reason.map(reason).transpose()?,
            };
            workload.on_webrtc_state_changed(context, &snapshot).await;
        }
        EventKind::Unspecified => return Err(protocol_error()),
    }
    Ok(())
}

fn peer_status(value: u32) -> ActorResult<WebRtcPeerStatus> {
    match value {
        0 => Ok(WebRtcPeerStatus::Idle),
        1 => Ok(WebRtcPeerStatus::Connecting),
        2 => Ok(WebRtcPeerStatus::Connected),
        3 => Ok(WebRtcPeerStatus::Recovering),
        _ => Err(protocol_error()),
    }
}

fn reason(value: u32) -> ActorResult<WebRtcUnavailableReason> {
    use WebRtcUnavailableReason as Reason;
    match value {
        1 => Ok(Reason::ConnectionNotReady),
        2 => Ok(Reason::PeerConnectionBuildFailed),
        3 => Ok(Reason::NegotiationFailed),
        4 => Ok(Reason::IceRestartFailed),
        5 => Ok(Reason::PeerConnectionRebuildFailed),
        6 => Ok(Reason::UnsupportedRtcCapability),
        7 => Ok(Reason::RtcResourceLimitExceeded),
        8 => Ok(Reason::PeerUnavailable),
        9 => Ok(Reason::RecoveryExhausted),
        10 => Ok(Reason::Closed),
        _ => Err(protocol_error()),
    }
}
