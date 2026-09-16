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

use crate::transport::DeliveryProgress;
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

/// One RTC send result with the delivery fact and adapter-level pre-send proof
/// observed by the controller. `DataChunkDeliveryUncertain` is the only
/// current outcome that may have entered the channel; all other errors are
/// proven pre-send failures.
#[derive(Debug)]
pub(crate) struct RtcSendAttempt {
    pub(crate) result: Result<(), RtcSendError>,
    pub(crate) progress: DeliveryProgress,
    pub(crate) proven_pre_send: bool,
}

impl RtcSendAttempt {
    pub(crate) fn from_result(result: Result<(), RtcSendError>) -> Self {
        let (progress, proven_pre_send) = match &result {
            Ok(()) => (DeliveryProgress::SubmittedLocally, false),
            Err(RtcSendError::DataChunkDeliveryUncertain) => (DeliveryProgress::Unknown, false),
            Err(
                RtcSendError::ConnectionNotReady
                | RtcSendError::DataChannelClosed
                | RtcSendError::DataChannelBufferFull
                | RtcSendError::MessageTooLarge,
            ) => (DeliveryProgress::NotAttempted, true),
        };
        Self {
            result,
            progress,
            proven_pre_send,
        }
    }
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

    #[test]
    fn send_attempt_preserves_delivery_progress() {
        assert_eq!(
            RtcSendAttempt::from_result(Ok(())).progress,
            DeliveryProgress::SubmittedLocally
        );
        assert_eq!(
            RtcSendAttempt::from_result(Err(RtcSendError::ConnectionNotReady)).progress,
            DeliveryProgress::NotAttempted
        );
        assert_eq!(
            RtcSendAttempt::from_result(Err(RtcSendError::DataChunkDeliveryUncertain)).progress,
            DeliveryProgress::Unknown
        );
        assert!(RtcSendAttempt::from_result(Err(RtcSendError::ConnectionNotReady)).proven_pre_send);
        assert!(
            !RtcSendAttempt::from_result(Err(RtcSendError::DataChunkDeliveryUncertain))
                .proven_pre_send
        );
    }

    #[test]
    fn every_non_uncertain_error_is_a_proven_pre_send_failure() {
        // The invariant documented on `RtcSendAttempt`: `DataChunkDeliveryUncertain`
        // is the only error that may have entered the channel. Every other
        // variant — including buffer-full and message-too-large, which are
        // detected before fragmentation hands bytes to the channel — must
        // classify as a proven pre-send failure so route recovery may replay.
        //
        // The match below is exhaustiveness-checked on purpose (same-crate
        // enum, no wildcard arm): adding a new `RtcSendError` variant breaks
        // this function's compilation, forcing an explicit classification
        // decision instead of silently omitting the variant from a
        // hand-maintained list.
        fn classify(error: RtcSendError) -> DeliveryProgress {
            let attempt = RtcSendAttempt::from_result(Err(error));
            match error {
                RtcSendError::ConnectionNotReady
                | RtcSendError::DataChannelClosed
                | RtcSendError::DataChannelBufferFull
                | RtcSendError::MessageTooLarge => {
                    assert_eq!(
                        attempt.progress,
                        DeliveryProgress::NotAttempted,
                        "{error:?} must remain a proven pre-send failure"
                    );
                    assert!(
                        attempt.proven_pre_send,
                        "{error:?} must carry adapter-level pre-send proof"
                    );
                    DeliveryProgress::NotAttempted
                }
                RtcSendError::DataChunkDeliveryUncertain => {
                    assert_eq!(
                        attempt.progress,
                        DeliveryProgress::Unknown,
                        "{error:?} must stay the only ambiguous outcome"
                    );
                    DeliveryProgress::Unknown
                }
            }
        }

        classify(RtcSendError::ConnectionNotReady);
        classify(RtcSendError::DataChannelClosed);
        classify(RtcSendError::DataChannelBufferFull);
        classify(RtcSendError::MessageTooLarge);
        classify(RtcSendError::DataChunkDeliveryUncertain);
    }
}
