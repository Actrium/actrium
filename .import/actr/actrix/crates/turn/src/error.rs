//! TURN service error types
//!
//! Defines all error types related to the TURN service.

use thiserror::Error;

/// TURN service error enumeration
#[derive(Error, Debug)]
pub enum TurnError {
    // ========== Server errors ==========
    /// Server start failed
    #[error("Failed to start TURN server: {reason}")]
    ServerStartFailed { reason: String },

    /// Server shutdown failed
    #[error("Failed to shutdown TURN server: {reason}")]
    ServerShutdownFailed { reason: String },

    /// Port binding failed
    #[error("Failed to bind TURN server to port {port}: {reason}")]
    PortBindFailed { port: u16, reason: String },

    // ========== Authentication errors ==========
    /// Authentication failed
    #[error("TURN authentication failed for user '{username}': {reason}")]
    AuthenticationFailed { username: String, reason: String },

    /// Authenticator creation failed
    #[error("Failed to create authenticator: {reason}")]
    AuthenticatorCreationFailed { reason: String },

    /// Invalid credentials
    #[error("Invalid credentials: {details}")]
    InvalidCredentials { details: String },

    /// Token validation failed
    #[error("Token validation failed: {reason}")]
    TokenValidationFailed { reason: String },

    // ========== Protocol errors ==========
    /// Invalid TURN message
    #[error("Invalid TURN message: {details}")]
    InvalidMessage { details: String },

    /// Unsupported message type
    #[error("Unsupported message type: {message_type}")]
    UnsupportedMessageType { message_type: String },

    /// Allocation failed
    #[error("Allocation failed: {reason}")]
    AllocationFailed { reason: String },

    /// Permission denied
    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },

    // ========== Network errors ==========
    /// Network connection error
    #[error("Network connection error: {details}")]
    NetworkConnection { details: String },

    /// Packet send failed
    #[error("Failed to send packet: {reason}")]
    PacketSendFailed { reason: String },

    /// Packet receive failed
    #[error("Failed to receive packet: {reason}")]
    PacketReceiveFailed { reason: String },

    /// Address resolution failed
    #[error("Address resolution failed: {address}")]
    AddressResolutionFailed { address: String },

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

    /// Invalid port range
    #[error("Invalid port range: {range}")]
    InvalidPortRange { range: String },

    // ========== Resource errors ==========
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    /// Allocation table full
    #[error("Allocation table full")]
    AllocationTableFull,

    /// Port pool exhausted
    #[error("Port pool exhausted")]
    PortPoolExhausted,

    /// Out of memory
    #[error("Out of memory")]
    OutOfMemory,

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

/// Result type specific to the TURN service
pub type Result<T> = std::result::Result<T, TurnError>;

impl TurnError {
    /// Create a general error
    pub fn general(message: impl Into<String>) -> Self {
        Self::General {
            message: message.into(),
        }
    }

    /// Create an authentication failure error
    pub fn auth_failed(username: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::AuthenticationFailed {
            username: username.into(),
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

    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::NetworkConnection { .. }
            | Self::PacketSendFailed { .. }
            | Self::PacketReceiveFailed { .. }
            | Self::Io(_) => true,
            Self::ResourceExhausted { .. }
            | Self::AllocationTableFull
            | Self::PortPoolExhausted
            | Self::OutOfMemory => false,
            _ => false,
        }
    }

    /// Get the error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::ServerStartFailed { .. } | Self::ServerShutdownFailed { .. } => {
                ErrorSeverity::Critical
            }
            Self::AuthenticationFailed { .. } | Self::PermissionDenied { .. } => {
                ErrorSeverity::Warning
            }
            Self::Configuration { .. } | Self::MissingConfiguration { .. } => ErrorSeverity::Error,
            Self::ResourceExhausted { .. }
            | Self::AllocationTableFull
            | Self::PortPoolExhausted
            | Self::OutOfMemory => ErrorSeverity::Critical,
            _ => ErrorSeverity::Error,
        }
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
impl From<anyhow::Error> for TurnError {
    fn from(err: anyhow::Error) -> Self {
        Self::general(err.to_string())
    }
}

// Provide conversions for common string types
impl From<String> for TurnError {
    fn from(message: String) -> Self {
        Self::general(message)
    }
}

impl From<&str> for TurnError {
    fn from(message: &str) -> Self {
        Self::general(message.to_string())
    }
}

// Convert to the unified BaseError
impl From<TurnError> for platform::error::BaseError {
    fn from(err: TurnError) -> Self {
        platform::error::BaseError::turn_service(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = TurnError::general("test error");
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_auth_error() {
        let err = TurnError::auth_failed("user1", "invalid password");
        assert!(err.to_string().contains("user1"));
        assert!(err.to_string().contains("invalid password"));
    }

    #[test]
    fn test_retryable_errors() {
        let network_err = TurnError::NetworkConnection {
            details: "timeout".to_string(),
        };
        assert!(network_err.is_retryable());

        let config_err = TurnError::Configuration {
            field: "port".to_string(),
            value: "invalid".to_string(),
        };
        assert!(!config_err.is_retryable());
    }

    #[test]
    fn test_error_severity() {
        let critical_err = TurnError::ServerStartFailed {
            reason: "port in use".to_string(),
        };
        assert_eq!(critical_err.severity(), ErrorSeverity::Critical);

        let warning_err = TurnError::AuthenticationFailed {
            username: "user".to_string(),
            reason: "bad password".to_string(),
        };
        assert_eq!(warning_err.severity(), ErrorSeverity::Warning);
    }
}
