//! STUN service error types
//!
//! Defines all error types related to the STUN service.

use thiserror::Error;

/// STUN service error enumeration
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum StunError {
    // ========== Server errors ==========
    /// Server start failed
    #[error("Failed to start STUN server: {reason}")]
    ServerStartFailed { reason: String },

    /// Server shutdown failed
    #[error("Failed to shutdown STUN server: {reason}")]
    ServerShutdownFailed { reason: String },

    /// Port binding failed
    #[error("Failed to bind STUN server to port {port}: {reason}")]
    PortBindFailed { port: u16, reason: String },

    // ========== Protocol errors ==========
    /// Invalid STUN message
    #[error("Invalid STUN message: {details}")]
    InvalidMessage { details: String },

    /// STUN message parse failed
    #[error("Failed to parse STUN message: {reason}")]
    MessageParseFailed { reason: String },

    /// STUN message encode failed
    #[error("Failed to encode STUN message: {reason}")]
    MessageEncodeFailed { reason: String },

    /// Unsupported message type
    #[error("Unsupported STUN message type: {message_type}")]
    UnsupportedMessageType { message_type: u16 },

    /// Invalid magic cookie
    #[error("Invalid STUN magic cookie")]
    InvalidMagicCookie,

    /// Invalid message length
    #[error("Invalid message length: expected {expected}, got {actual}")]
    InvalidMessageLength { expected: usize, actual: usize },

    /// Invalid attribute
    #[error("Invalid STUN attribute: {attribute_type}")]
    InvalidAttribute { attribute_type: u16 },

    // ========== Network errors ==========
    /// Network connection error
    #[error("Network connection error: {details}")]
    NetworkConnection { details: String },

    /// Packet send failed
    #[error("Failed to send STUN packet: {reason}")]
    PacketSendFailed { reason: String },

    /// Packet receive failed
    #[error("Failed to receive STUN packet: {reason}")]
    PacketReceiveFailed { reason: String },

    /// Address resolution failed
    #[error("Address resolution failed: {address}")]
    AddressResolutionFailed { address: String },

    /// Socket operation failed
    #[error("Socket operation failed: {operation}")]
    SocketOperationFailed { operation: String },

    // ========== Configuration errors ==========
    /// Configuration error
    #[error("Configuration error: {field} = {value}")]
    Configuration { field: String, value: String },

    /// Missing required configuration
    #[error("Missing required configuration: {field}")]
    MissingConfiguration { field: String },

    /// Invalid IP address
    #[error("Invalid IP address: {address}")]
    InvalidIpAddress { address: String },

    /// Invalid port
    #[error("Invalid port: {port}")]
    InvalidPort { port: String },

    // ========== Resource errors ==========
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    /// Out of memory
    #[error("Out of memory")]
    OutOfMemory,

    /// Connection table full
    #[error("Connection table full")]
    ConnectionTableFull,

    // ========== Timeout errors ==========
    /// Operation timed out
    #[error("Operation timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Response timeout
    #[error("Response timeout for transaction {transaction_id}")]
    ResponseTimeout { transaction_id: String },

    // ========== External error wrappers ==========
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// WebRTC library error
    #[error("WebRTC error: {0}")]
    WebRtc(#[from] webrtc::error::Error),

    /// Address parse error
    #[error("Address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),

    /// General error
    #[error("General error: {message}")]
    General { message: String },
}

/// Result type specific to the STUN service
pub type Result<T> = std::result::Result<T, StunError>;

impl StunError {
    /// Create a general error
    pub fn general(message: impl Into<String>) -> Self {
        Self::General {
            message: message.into(),
        }
    }

    /// Create a message parse error
    pub fn parse_failed(reason: impl Into<String>) -> Self {
        Self::MessageParseFailed {
            reason: reason.into(),
        }
    }

    /// Create a configuration error
    pub fn config_error(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Configuration {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a timeout error
    pub fn timeout(duration_ms: u64) -> Self {
        Self::Timeout { duration_ms }
    }

    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::NetworkConnection { .. }
            | Self::PacketSendFailed { .. }
            | Self::PacketReceiveFailed { .. }
            | Self::Timeout { .. }
            | Self::ResponseTimeout { .. }
            | Self::Io(_) => true,
            Self::ResourceExhausted { .. } | Self::OutOfMemory | Self::ConnectionTableFull => false,
            _ => false,
        }
    }

    /// Get the error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::ServerStartFailed { .. } | Self::ServerShutdownFailed { .. } => {
                ErrorSeverity::Critical
            }
            Self::InvalidMessage { .. }
            | Self::MessageParseFailed { .. }
            | Self::UnsupportedMessageType { .. } => ErrorSeverity::Warning,
            Self::Configuration { .. } | Self::MissingConfiguration { .. } => ErrorSeverity::Error,
            Self::ResourceExhausted { .. } | Self::OutOfMemory | Self::ConnectionTableFull => {
                ErrorSeverity::Critical
            }
            Self::Timeout { .. } | Self::ResponseTimeout { .. } => ErrorSeverity::Warning,
            _ => ErrorSeverity::Error,
        }
    }

    /// Check if this is a protocol-related error
    pub fn is_protocol_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidMessage { .. }
                | Self::MessageParseFailed { .. }
                | Self::MessageEncodeFailed { .. }
                | Self::UnsupportedMessageType { .. }
                | Self::InvalidMagicCookie
                | Self::InvalidMessageLength { .. }
                | Self::InvalidAttribute { .. }
        )
    }

    /// Check if this is a network-related error
    pub fn is_network_error(&self) -> bool {
        matches!(
            self,
            Self::NetworkConnection { .. }
                | Self::PacketSendFailed { .. }
                | Self::PacketReceiveFailed { .. }
                | Self::AddressResolutionFailed { .. }
                | Self::SocketOperationFailed { .. }
                | Self::Io(_)
                | Self::AddrParse(_)
        )
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Info level
    Info,
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical error level
    Critical,
}

// Provide conversion from anyhow::Error for backward compatibility
impl From<anyhow::Error> for StunError {
    fn from(err: anyhow::Error) -> Self {
        Self::general(err.to_string())
    }
}

// Conversion from webrtc-stun library errors
impl From<webrtc_stun::Error> for StunError {
    fn from(err: webrtc_stun::Error) -> Self {
        Self::MessageParseFailed {
            reason: err.to_string(),
        }
    }
}

// Provide conversions for common string types
impl From<String> for StunError {
    fn from(message: String) -> Self {
        Self::general(message)
    }
}

impl From<&str> for StunError {
    fn from(message: &str) -> Self {
        Self::general(message.to_string())
    }
}

// Convert to the unified BaseError
impl From<StunError> for platform::error::BaseError {
    fn from(err: StunError) -> Self {
        platform::error::BaseError::stun_service(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = StunError::general("test error");
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_parse_error() {
        let err = StunError::parse_failed("invalid format");
        assert!(err.to_string().contains("invalid format"));
    }

    #[test]
    fn test_retryable_errors() {
        let timeout_err = StunError::timeout(5000);
        assert!(timeout_err.is_retryable());

        let config_err = StunError::Configuration {
            field: "port".to_string(),
            value: "invalid".to_string(),
        };
        assert!(!config_err.is_retryable());
    }

    #[test]
    fn test_error_severity() {
        let critical_err = StunError::ServerStartFailed {
            reason: "port in use".to_string(),
        };
        assert_eq!(critical_err.severity(), ErrorSeverity::Critical);

        let warning_err = StunError::InvalidMessage {
            details: "bad format".to_string(),
        };
        assert_eq!(warning_err.severity(), ErrorSeverity::Warning);
    }

    #[test]
    fn test_error_categories() {
        let protocol_err = StunError::InvalidMessage {
            details: "test".to_string(),
        };
        assert!(protocol_err.is_protocol_error());
        assert!(!protocol_err.is_network_error());

        let network_err = StunError::NetworkConnection {
            details: "test".to_string(),
        };
        assert!(!network_err.is_protocol_error());
        assert!(network_err.is_network_error());
    }
}
