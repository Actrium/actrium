//! A versioned, application-independent subprotocol over RFC 6455 WebSockets.
//!
//! Control messages are JSON text; data messages contain an eight-byte,
//! big-endian channel identifier followed by opaque application bytes. Channel
//! identifiers are minted by the relay and scoped to one relay instance.

use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};

pub const SUBPROTOCOL: &str = "loran-relay.v1";
pub const DATA_HEADER_LEN: usize = 8;
pub const MAX_ENDPOINT_LEN: usize = 512;
pub const MAX_CONTROL_BYTES: usize = 32 * 1024;
pub const MAX_METADATA_BYTES: usize = 16 * 1024;
pub const MAX_IDENTIFIER: u64 = (1 << 53) - 1;

pub fn valid_identifier(identifier: u64) -> bool {
    (1..=MAX_IDENTIFIER).contains(&identifier)
}

/// Public capability discovery from the same origin as a trusted bootstrap
/// endpoint. An omitted URL denotes the local /relay/ws service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayDiscovery {
    pub version: u8,
    pub enabled: bool,
    pub url: Option<String>,
}

/// Authentication is deliberately opaque to the relay protocol. Credentials
/// travel in the first frame, never in a URL or WebSocket subprotocol token.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientControl {
    Hello { endpoint: String, token: String },
    Connect { request: u64, peer: String },
    Accept { channel: u64 },
    Reject { channel: u64 },
    Close { channel: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerControl {
    Welcome {
        session: u64,
    },
    Incoming {
        channel: u64,
        peer: String,
        metadata: String,
    },
    Connected {
        channel: u64,
        request: Option<u64>,
        peer: String,
        metadata: String,
    },
    Closed {
        channel: u64,
        reason: CloseReason,
        /// An unfinished dial has no channel at the initiator yet. Correlate
        /// rejection/timeout so it can fail promptly rather than timing out.
        #[serde(default)]
        request: Option<u64>,
    },
    Error {
        request: Option<u64>,
        code: ErrorCode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    PeerLeft,
    Rejected,
    TimedOut,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    PeerUnavailable,
    Capacity,
    InvalidChannel,
    NotReady,
    InvalidMessage,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("invalid relay endpoint")]
    InvalidEndpoint,
    #[error("invalid relay data frame")]
    InvalidData,
    #[error("relay frame exceeds configured limit")]
    TooLarge,
}

pub fn valid_endpoint(endpoint: &str) -> bool {
    !endpoint.is_empty()
        && endpoint.len() <= MAX_ENDPOINT_LEN
        && !endpoint.chars().any(char::is_control)
}

pub fn encode_data(channel: u64, payload: &[u8]) -> Result<Bytes, ProtocolError> {
    if !valid_identifier(channel) {
        return Err(ProtocolError::InvalidData);
    }
    let mut frame = BytesMut::with_capacity(DATA_HEADER_LEN + payload.len());
    frame.put_u64(channel);
    frame.extend_from_slice(payload);
    Ok(frame.freeze())
}

pub fn decode_data(frame: Bytes, max_bytes: usize) -> Result<(u64, Bytes), ProtocolError> {
    if frame.len() > max_bytes {
        return Err(ProtocolError::TooLarge);
    }
    if frame.len() < DATA_HEADER_LEN {
        return Err(ProtocolError::InvalidData);
    }
    let channel = u64::from_be_bytes(frame[..DATA_HEADER_LEN].try_into().unwrap());
    if !valid_identifier(channel) {
        return Err(ProtocolError::InvalidData);
    }
    Ok((channel, frame.slice(DATA_HEADER_LEN..)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RelayLimits {
    pub max_connections: usize,
    pub max_channels_per_endpoint: usize,
    pub max_frame_bytes: usize,
    pub queue_messages: usize,
    pub queue_bytes: u32,
    pub authentication_timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub write_timeout_secs: u64,
    pub heartbeat_interval_secs: u64,
    pub heartbeat_timeout_secs: u64,
    /// Empty allows browser origins with token authentication. Nonempty
    /// restricts supplied Origin headers; native clients may omit Origin.
    pub allowed_origins: Vec<String>,
}

impl Default for RelayLimits {
    fn default() -> Self {
        Self {
            max_connections: 8192,
            max_channels_per_endpoint: 256,
            max_frame_bytes: 8 * 1024 * 1024 + 64,
            queue_messages: 256,
            queue_bytes: 16 * 1024 * 1024,
            authentication_timeout_secs: 10,
            connect_timeout_secs: 10,
            write_timeout_secs: 5,
            heartbeat_interval_secs: 20,
            heartbeat_timeout_secs: 60,
            allowed_origins: Vec::new(),
        }
    }
}

impl RelayLimits {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_connections == 0
            || self.max_channels_per_endpoint == 0
            || self.max_frame_bytes < MAX_CONTROL_BYTES
            || self.max_frame_bytes > u32::MAX as usize
            || self.queue_messages == 0
            || self.queue_bytes < self.max_frame_bytes as u32
            || self.authentication_timeout_secs == 0
            || self.connect_timeout_secs == 0
            || self.write_timeout_secs == 0
            || self.heartbeat_interval_secs == 0
            || self.heartbeat_timeout_secs <= self.heartbeat_interval_secs
        {
            return Err("invalid relay limits");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_preserves_bytes_and_rejects_invalid_headers() {
        let frame = encode_data(42, b"\0\xffpayload").unwrap();
        assert_eq!(
            decode_data(frame.clone(), 1024).unwrap(),
            (42, Bytes::from_static(b"\0\xffpayload"))
        );
        assert_eq!(decode_data(frame, 8), Err(ProtocolError::TooLarge));
        assert_eq!(
            decode_data(Bytes::from_static(b"short"), 1024),
            Err(ProtocolError::InvalidData)
        );
        assert_eq!(encode_data(0, b""), Err(ProtocolError::InvalidData));
        assert_eq!(
            encode_data(MAX_IDENTIFIER + 1, b""),
            Err(ProtocolError::InvalidData)
        );
    }

    #[test]
    fn version_one_control_rejects_unknown_fields_and_types() {
        assert!(
            serde_json::from_str::<ClientControl>(
                r#"{"type":"accept","channel":7,"endpoint":"spoof"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<ClientControl>(r#"{"type":"v2"}"#).is_err());
        assert!(!valid_endpoint(""));
        assert!(!valid_endpoint("line\nbreak"));
        assert!(valid_endpoint("opaque/endpoint:1"));
    }
}
