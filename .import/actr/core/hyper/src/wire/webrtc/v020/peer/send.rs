//! `SendPermit` and the stable send errors for the v0.20 send path
//! (RFC §"Send path and SendPermit").
//!
//! A `SendPermit` is created per send, binding the lane, its PC generation, and
//! the session's cancellation token. It is the *final* race guard before
//! touching a DataChannel: even a `sendable` snapshot can go stale in the last
//! moment, so the permit re-validates generation + cancellation before
//! `send`/`try_send`/`writable`.

use std::sync::Arc;

use actr_protocol::ActrId;
use tokio_util::sync::CancellationToken;
use webrtc::data_channel::DataChannel;
use webrtc::error::Error as RtcError;

use crate::wire::webrtc::v020::status::LaneKind;

use super::lane_handle::LaneHandle;

/// A one-send permit: lane + generation + channel + cancellation (RFC §"SendPermit").
pub(crate) struct SendPermit {
    peer: ActrId,
    generation: u64,
    lane: LaneKind,
    channel: Arc<dyn DataChannel>,
    cancellation: CancellationToken,
}

impl SendPermit {
    pub(crate) fn from_handle(peer: &ActrId, generation: u64, handle: &LaneHandle) -> Self {
        Self {
            peer: peer.clone(),
            generation,
            lane: handle.lane(),
            channel: handle.channel().clone(),
            cancellation: handle.cancellation().clone(),
        }
    }

    /// Final race guard: the permit's generation is still the current one and
    /// the session has not been cancelled (RFC: "Check cancellation/generation
    /// again before calling DataChannel").
    pub(crate) fn is_valid(&self, current_generation: u64) -> bool {
        self.generation == current_generation && !self.cancellation.is_cancelled()
    }

    pub(crate) fn channel(&self) -> &Arc<dyn DataChannel> {
        &self.channel
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn lane(&self) -> LaneKind {
        self.lane
    }
}

/// Stable Actr send errors (RFC §"Error model"). `ConnectionNotReady` covers a
/// not-sendable snapshot and a stale generation/cancellation; the buffer and
/// delivery variants map the v0.20 send outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RtcSendError {
    #[error("connection not ready")]
    ConnectionNotReady,
    #[error("data channel closed")]
    DataChannelClosed,
    #[error("data channel buffer full")]
    DataChannelBufferFull,
    #[error("data chunk delivery uncertain")]
    DataChunkDeliveryUncertain,
    /// The payload exceeds what lane fragmentation can address (more than
    /// `u16::MAX` fragments); retrying the same payload can never succeed.
    #[error("message too large for lane fragmentation")]
    MessageTooLarge,
}

/// An internal/uncategorized send failure (kept out of the stable enum so the
/// public surface stays small; logged with the peer/generation/lane).
#[derive(Debug, thiserror::Error)]
#[error("rtc send failed: {0}")]
pub(crate) struct RtcSendFailed(pub(crate) String);

/// Map a v0.20 send error to a stable Actr send error.
pub(crate) fn map_send_error(error: &RtcError) -> RtcSendError {
    match error {
        RtcError::ErrDataChannelClosed => RtcSendError::DataChannelClosed,
        RtcError::ErrSendBufferFull => RtcSendError::DataChannelBufferFull,
        _ => RtcSendError::ConnectionNotReady,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_data_channel_keeps_a_terminal_error_identity() {
        assert_eq!(
            map_send_error(&RtcError::ErrDataChannelClosed),
            RtcSendError::DataChannelClosed
        );
    }
}
