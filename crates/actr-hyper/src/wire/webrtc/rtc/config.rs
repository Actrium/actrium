//! Validated resource configuration for the RTC stack.
//!
//! webrtc-rs bounds a single DataChannel send queue when Actr opts in; the RFC
//! also requires finite receive and reassembly budgets.  This type is the
//! single source of truth shared by the factory, Manager, Session pumps, and
//! inbound bridge.  Invalid combinations are rejected before a PeerConnection
//! is built; none of the accessors silently substitute an unlimited value.

/// One SCTP user message, including Actr's fragment header.
pub(crate) const DEFAULT_MAX_INBOUND_FRAME_BYTES: usize = 65_535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RtcBufferConfig {
    pub(crate) per_channel_send_bytes: usize,
    pub(crate) sctp_receive_window_bytes: u32,
    pub(crate) max_inbound_frame_bytes: usize,
    pub(crate) max_inbound_message_bytes: usize,
    pub(crate) max_reassembly_messages: usize,
    pub(crate) max_reassembly_bytes: usize,
    pub(crate) max_inbound_dispatch_messages: usize,
    pub(crate) max_inbound_dispatch_bytes: usize,
    pub(crate) max_session_resources: usize,
    pub(crate) max_structural_event_bytes: usize,
}

impl Default for RtcBufferConfig {
    fn default() -> Self {
        Self {
            // Browser-like finite send queue (enforced by the webrtc crate's
            // per-channel send-buffer limit; there is no Actr-level aggregate
            // send budget).
            per_channel_send_bytes: 16 * 1024 * 1024,
            sctp_receive_window_bytes: 1024 * 1024,
            max_inbound_frame_bytes: DEFAULT_MAX_INBOUND_FRAME_BYTES,
            // Existing Actr tests require at least 1 MiB application messages;
            // 8 MiB leaves headroom while remaining a hard, finite boundary.
            max_inbound_message_bytes: 8 * 1024 * 1024,
            max_reassembly_messages: 64,
            max_reassembly_bytes: 16 * 1024 * 1024,
            max_inbound_dispatch_messages: 256,
            max_inbound_dispatch_bytes: 32 * 1024 * 1024,
            max_session_resources: 256,
            max_structural_event_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum RtcConfigError {
    #[error("RTC resource setting `{0}` must be non-zero and finite")]
    Zero(&'static str),
    #[error(
        "SCTP receive window ({window} bytes) is smaller than the maximum inbound frame ({frame} bytes)"
    )]
    SctpWindowTooSmall { window: u32, frame: usize },
    #[error(
        "maximum inbound message ({message} bytes) is smaller than one inbound frame payload ({frame_payload} bytes)"
    )]
    InboundMessageTooSmall {
        message: usize,
        frame_payload: usize,
    },
    #[error(
        "reassembly byte budget ({budget} bytes) cannot hold one maximum inbound message ({message} bytes)"
    )]
    ReassemblyBudgetTooSmall { budget: usize, message: usize },
    #[error(
        "inbound dispatch byte budget ({budget} bytes) cannot hold one maximum inbound message ({message} bytes)"
    )]
    InboundDispatchBudgetTooSmall { budget: usize, message: usize },
    #[error("RTC resource budget arithmetic overflowed")]
    ArithmeticOverflow,
}

impl RtcBufferConfig {
    pub(crate) fn validate(&self) -> Result<(), RtcConfigError> {
        for (name, value) in [
            ("per_channel_send_bytes", self.per_channel_send_bytes),
            (
                "sctp_receive_window_bytes",
                self.sctp_receive_window_bytes as usize,
            ),
            ("max_inbound_frame_bytes", self.max_inbound_frame_bytes),
            ("max_inbound_message_bytes", self.max_inbound_message_bytes),
            ("max_reassembly_messages", self.max_reassembly_messages),
            ("max_reassembly_bytes", self.max_reassembly_bytes),
            (
                "max_inbound_dispatch_messages",
                self.max_inbound_dispatch_messages,
            ),
            (
                "max_inbound_dispatch_bytes",
                self.max_inbound_dispatch_bytes,
            ),
            ("max_session_resources", self.max_session_resources),
            (
                "max_structural_event_bytes",
                self.max_structural_event_bytes,
            ),
        ] {
            if value == 0 || value == usize::MAX {
                return Err(RtcConfigError::Zero(name));
            }
        }

        if (self.sctp_receive_window_bytes as usize) < self.max_inbound_frame_bytes {
            return Err(RtcConfigError::SctpWindowTooSmall {
                window: self.sctp_receive_window_bytes,
                frame: self.max_inbound_frame_bytes,
            });
        }
        let frame_payload = self
            .max_inbound_frame_bytes
            .checked_sub(crate::transport::FRAGMENT_HEADER_SIZE)
            .ok_or(RtcConfigError::ArithmeticOverflow)?;
        if self.max_inbound_message_bytes < frame_payload {
            return Err(RtcConfigError::InboundMessageTooSmall {
                message: self.max_inbound_message_bytes,
                frame_payload,
            });
        }
        if self.max_reassembly_bytes < self.max_inbound_message_bytes {
            return Err(RtcConfigError::ReassemblyBudgetTooSmall {
                budget: self.max_reassembly_bytes,
                message: self.max_inbound_message_bytes,
            });
        }
        if self.max_inbound_dispatch_bytes < self.max_inbound_message_bytes {
            return Err(RtcConfigError::InboundDispatchBudgetTooSmall {
                budget: self.max_inbound_dispatch_bytes,
                message: self.max_inbound_message_bytes,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_defaults_are_finite_and_consistent() {
        RtcBufferConfig::default().validate().unwrap();
    }

    #[test]
    fn zero_send_limit_cannot_restore_upstream_unbounded_mode() {
        let config = RtcBufferConfig {
            per_channel_send_bytes: 0,
            ..RtcBufferConfig::default()
        };
        assert_eq!(
            config.validate(),
            Err(RtcConfigError::Zero("per_channel_send_bytes"))
        );
    }

    #[test]
    fn receive_window_must_fit_one_wire_frame() {
        let config = RtcBufferConfig {
            sctp_receive_window_bytes: 32 * 1024,
            ..RtcBufferConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(RtcConfigError::SctpWindowTooSmall { .. })
        ));
    }

    #[test]
    fn receive_and_aggregate_budgets_fail_fast_when_inconsistent() {
        let mut config = RtcBufferConfig::default();
        config.max_reassembly_bytes = config.max_inbound_message_bytes - 1;
        assert!(matches!(
            config.validate(),
            Err(RtcConfigError::ReassemblyBudgetTooSmall { .. })
        ));
    }
}
