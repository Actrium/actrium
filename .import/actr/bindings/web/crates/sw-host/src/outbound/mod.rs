//! Outbound layer for sending messages.
//!
//! Mirrors actr's outbound layer and provides a unified sending interface.
//!
//! # Outbound path
//!
//! ```text
//! Actor ctx.call()/tell()
//!   -> Gate::Peer
//!     -> PeerGate (ActrId->Dest mapping + pending_requests)
//!       -> PeerTransport (Dest->DestTransport mapping)
//!         -> DestTransport (event-driven send loop)
//!           -> WirePool (priority selection: WebRTC > WebSocket)
//!             -> WireHandle::WebRTC.get_lane()
//!               -> DataLane::PostMessage { port: dedicated MessagePort }
//!                 -> port.postMessage(data)  [zero-copy, no command protocol]
//!                   -> DOM bridge -> RtcDataChannel.send() -> Remote
//! ```

mod host_gate;
mod peer_gate;

pub use host_gate::HostGate;
pub use peer_gate::PeerGate;

use actr_protocol::{ActorResult, ActrError, ActrId, PayloadType, RpcEnvelope};
use bytes::Bytes;
use std::sync::Arc;

/// Validate a caller-supplied RPC timeout for a REQUEST envelope.
///
/// Wire contract (see `package.proto`): `timeout_ms` MUST be > 0 for
/// `DIRECTION_REQUEST`. Rejecting `<= 0` here prevents web callers from
/// producing the invalid zero/negative REQUEST envelopes that #254 guards
/// against on the native side. Mirrors
/// `actr_hyper::transport::validate_rpc_timeout_ms`.
pub(crate) fn validate_rpc_timeout_ms(timeout_ms: i64) -> ActorResult<u32> {
    if timeout_ms <= 0 {
        return Err(ActrError::InvalidArgument(format!(
            "RPC call timeout_ms must be > 0, got {timeout_ms} (fire-and-forget messaging must use tell, not a zero timeout)"
        )));
    }
    if timeout_ms > i64::from(i32::MAX) {
        return Err(ActrError::InvalidArgument(format!(
            "RPC call timeout_ms must be <= {}, got {timeout_ms}",
            i32::MAX
        )));
    }
    Ok(timeout_ms as u32)
}

pub(crate) fn validate_rpc_payload_type(payload_type: PayloadType) -> ActorResult<()> {
    if matches!(
        payload_type,
        PayloadType::RpcReliable | PayloadType::RpcSignal
    ) {
        Ok(())
    } else {
        Err(ActrError::InvalidArgument(format!(
            "RPC calls and tells require an RPC payload type, got {payload_type:?}"
        )))
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn web_timeout_fits_browser_timer_range() {
        assert_eq!(validate_rpc_timeout_ms(1).unwrap(), 1);
        assert_eq!(
            validate_rpc_timeout_ms(i64::from(i32::MAX)).unwrap(),
            i32::MAX as u32
        );
        assert!(matches!(
            validate_rpc_timeout_ms(i64::from(i32::MAX) + 1),
            Err(ActrError::InvalidArgument(_))
        ));
    }

    #[test]
    fn web_rpc_lane_validation_rejects_stream_and_media_types() {
        assert!(validate_rpc_payload_type(PayloadType::RpcReliable).is_ok());
        assert!(validate_rpc_payload_type(PayloadType::RpcSignal).is_ok());
        assert!(matches!(
            validate_rpc_payload_type(PayloadType::StreamReliable),
            Err(ActrError::InvalidArgument(_))
        ));
        assert!(matches!(
            validate_rpc_payload_type(PayloadType::MediaRtp),
            Err(ActrError::InvalidArgument(_))
        ));
    }
}

/// Gate enum for outbound messaging.
///
/// # Variants
///
/// - **Host**: communication between actors inside the SW with zero serialization
/// - **Peer**: cross-node transport through a dedicated MessagePort and the full transport stack
#[derive(Clone)]
pub enum Gate {
    /// Host gate for in-SW communication with zero serialization.
    Host(Arc<HostGate>),

    /// Peer gate for cross-node transport.
    ///
    /// PeerGate -> PeerTransport -> DestTransport
    ///   -> WirePool -> WireHandle -> DataLane::PostMessage (direct send through a dedicated MessagePort)
    Peer(Arc<PeerGate>),
}

impl Gate {
    /// Create a Host gate.
    pub fn host(gate: Arc<HostGate>) -> Self {
        Self::Host(gate)
    }

    /// Create a Peer gate.
    pub fn peer(gate: Arc<PeerGate>) -> Self {
        Self::Peer(gate)
    }

    /// Send a request and wait for the response.
    pub async fn send_request(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<Bytes> {
        self.send_request_with_type(target, PayloadType::RpcReliable, envelope)
            .await
    }

    pub async fn send_request_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<Bytes> {
        validate_rpc_payload_type(payload_type)?;
        match self {
            Gate::Host(gate) => {
                gate.send_request_with_type(target, payload_type, envelope)
                    .await
            }
            Gate::Peer(gate) => {
                gate.send_request_with_type(target, payload_type, envelope)
                    .await
            }
        }
    }

    /// Send a one-way tell without waiting for a response.
    pub async fn send_message(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<()> {
        self.send_message_with_type(target, PayloadType::RpcSignal, envelope)
            .await
    }

    pub async fn send_message_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<()> {
        validate_rpc_payload_type(payload_type)?;
        match self {
            Gate::Host(gate) => {
                gate.send_message_with_type(target, payload_type, envelope)
                    .await
            }
            Gate::Peer(gate) => {
                gate.send_message_with_type(target, payload_type, envelope)
                    .await
            }
        }
    }

    /// Relay an envelope that already carries an explicit routing direction.
    ///
    /// Unlike `send_message` (which stamps `Direction::Tell`), this preserves
    /// the sender's Request/Response/Tell label so a relayed request still
    /// expects a reply on the remote peer.
    ///
    /// Only `Gate::Peer` supports relay: `HostGate`'s only send path is
    /// `send_message`, which would wrongly downgrade a Request to Tell, so
    /// Host returns `InvalidArgument`. The sole caller
    /// (`System::init_message_handler`) always runs against a Peer outgate, so
    /// the Host arm is a defensive guard against misuse, not a live path.
    pub async fn relay_envelope(&self, target: &ActrId, envelope: RpcEnvelope) -> ActorResult<()> {
        self.relay_envelope_with_type(target, PayloadType::RpcSignal, envelope)
            .await
    }

    pub async fn relay_envelope_with_type(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        envelope: RpcEnvelope,
    ) -> ActorResult<()> {
        validate_rpc_payload_type(payload_type)?;
        match self {
            Gate::Host(_) => Err(ActrError::InvalidArgument(
                "relay_envelope requires a Peer gate; HostGate send_message is tell-only"
                    .to_string(),
            )),
            Gate::Peer(gate) => {
                gate.relay_envelope_with_type(target, payload_type, envelope)
                    .await
            }
        }
    }

    /// Send a DataChunk through the Fast Path.
    pub async fn send_data_chunk(
        &self,
        target: &ActrId,
        payload_type: PayloadType,
        data: Bytes,
    ) -> ActorResult<()> {
        match self {
            Gate::Host(gate) => gate.send_data_chunk(target, payload_type, data).await,
            Gate::Peer(gate) => gate.send_data_chunk(target, payload_type, data).await,
        }
    }

    /// Try to handle a remote response.
    ///
    /// Checks whether this gate has a pending request for `request_id`.
    /// If so, resolves it and returns true; otherwise returns false.
    pub fn try_handle_response(&self, request_id: &str, response: Bytes) -> bool {
        match self {
            Gate::Host(_) => false,
            Gate::Peer(gate) => gate.handle_response(request_id.to_string(), response),
        }
    }
}
