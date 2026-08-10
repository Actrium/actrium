//! `PayloadType` enum and per-service default-lane fallback.
//!
//! The declared `(actr.payload_type)` method option is carried by the
//! unified dispatch table (sourced from the `DispatchSpec`), not by the
//! per-service generator. This module only defines the `PayloadType` enum
//! mirrored from `core/protocol/proto/actr/options.proto` and the default
//! lane a per-service RPC method emits when no explicit lane applies.

use prost_types::MethodDescriptorProto;

/// PayloadType enum values matching proto definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PayloadType {
    #[default]
    RpcReliable = 0,
    RpcSignal = 1,
    StreamReliable = 2,
    StreamLatencyFirst = 3,
    MediaRtp = 4,
}

impl PayloadType {
    /// Convert from the proto enum value carried on the wire (varint).
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(PayloadType::RpcReliable),
            1 => Some(PayloadType::RpcSignal),
            2 => Some(PayloadType::StreamReliable),
            3 => Some(PayloadType::StreamLatencyFirst),
            4 => Some(PayloadType::MediaRtp),
            _ => None,
        }
    }

    /// Get Rust enum variant name for code generation
    pub(crate) fn as_rust_variant(&self) -> &'static str {
        match self {
            PayloadType::RpcReliable => "PayloadType::RpcReliable",
            PayloadType::RpcSignal => "PayloadType::RpcSignal",
            PayloadType::StreamReliable => "PayloadType::StreamReliable",
            PayloadType::StreamLatencyFirst => "PayloadType::StreamLatencyFirst",
            PayloadType::MediaRtp => "PayloadType::MediaRtp",
        }
    }
}

/// Check if method uses streaming
fn is_streaming(method: &MethodDescriptorProto) -> bool {
    // Check if input or output is streaming
    method.client_streaming.unwrap_or(false) || method.server_streaming.unwrap_or(false)
}

/// Fallback `PayloadType` for a per-service RPC method.
///
/// The declared `(actr.payload_type)` option lives on the unified dispatch
/// table (from the `DispatchSpec`); per-service generated code only needs the
/// default lane for methods that do not select a specific transport.
///
/// # Default Rules
///
/// - Unary RPC (no stream) → `RPC_RELIABLE`
/// - Streaming RPC (has stream keyword) → `STREAM_RELIABLE`
pub(crate) fn extract_payload_type_or_default(method: &MethodDescriptorProto) -> PayloadType {
    if is_streaming(method) {
        return PayloadType::StreamReliable;
    }

    PayloadType::RpcReliable
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::MethodDescriptorProto;

    #[test]
    fn test_payload_type_from_u64() {
        assert_eq!(PayloadType::from_u64(0), Some(PayloadType::RpcReliable));
        assert_eq!(PayloadType::from_u64(1), Some(PayloadType::RpcSignal));
        assert_eq!(PayloadType::from_u64(2), Some(PayloadType::StreamReliable));
        assert_eq!(
            PayloadType::from_u64(3),
            Some(PayloadType::StreamLatencyFirst)
        );
        assert_eq!(PayloadType::from_u64(4), Some(PayloadType::MediaRtp));
        assert_eq!(PayloadType::from_u64(999), None);
    }

    #[test]
    fn test_payload_type_as_rust_variant() {
        assert_eq!(
            PayloadType::RpcReliable.as_rust_variant(),
            "PayloadType::RpcReliable"
        );
        assert_eq!(
            PayloadType::RpcSignal.as_rust_variant(),
            "PayloadType::RpcSignal"
        );
        assert_eq!(
            PayloadType::StreamReliable.as_rust_variant(),
            "PayloadType::StreamReliable"
        );
        assert_eq!(
            PayloadType::StreamLatencyFirst.as_rust_variant(),
            "PayloadType::StreamLatencyFirst"
        );
        assert_eq!(
            PayloadType::MediaRtp.as_rust_variant(),
            "PayloadType::MediaRtp"
        );
    }

    #[test]
    fn test_default_payload_type() {
        assert_eq!(PayloadType::default(), PayloadType::RpcReliable);
    }

    #[test]
    fn test_fallback_unary_rpc() {
        // Unary RPC: no streaming flags
        let method = MethodDescriptorProto {
            name: Some("TestMethod".to_string()),
            input_type: Some(".test.Request".to_string()),
            output_type: Some(".test.Response".to_string()),
            options: None,
            client_streaming: Some(false),
            server_streaming: Some(false),
        };

        assert_eq!(
            extract_payload_type_or_default(&method),
            PayloadType::RpcReliable
        );
    }

    #[test]
    fn test_fallback_client_streaming() {
        // Client streaming: stream Request
        let method = MethodDescriptorProto {
            name: Some("UploadFile".to_string()),
            input_type: Some(".test.FileChunk".to_string()),
            output_type: Some(".test.UploadResponse".to_string()),
            options: None,
            client_streaming: Some(true),
            server_streaming: Some(false),
        };

        assert_eq!(
            extract_payload_type_or_default(&method),
            PayloadType::StreamReliable
        );
    }

    #[test]
    fn test_fallback_server_streaming() {
        // Server streaming: returns stream Response
        let method = MethodDescriptorProto {
            name: Some("DownloadFile".to_string()),
            input_type: Some(".test.DownloadRequest".to_string()),
            output_type: Some(".test.FileChunk".to_string()),
            options: None,
            client_streaming: Some(false),
            server_streaming: Some(true),
        };

        assert_eq!(
            extract_payload_type_or_default(&method),
            PayloadType::StreamReliable
        );
    }

    #[test]
    fn test_fallback_bidirectional_streaming() {
        // Bidirectional streaming: stream Request, returns stream Response
        let method = MethodDescriptorProto {
            name: Some("StreamChat".to_string()),
            input_type: Some(".test.ChatMessage".to_string()),
            output_type: Some(".test.ChatMessage".to_string()),
            options: None,
            client_streaming: Some(true),
            server_streaming: Some(true),
        };

        assert_eq!(
            extract_payload_type_or_default(&method),
            PayloadType::StreamReliable
        );
    }

    #[test]
    fn test_is_streaming_detection() {
        // Unary
        let unary = MethodDescriptorProto {
            client_streaming: Some(false),
            server_streaming: Some(false),
            ..Default::default()
        };
        assert!(!is_streaming(&unary));

        // Client streaming
        let client_stream = MethodDescriptorProto {
            client_streaming: Some(true),
            server_streaming: Some(false),
            ..Default::default()
        };
        assert!(is_streaming(&client_stream));

        // Server streaming
        let server_stream = MethodDescriptorProto {
            client_streaming: Some(false),
            server_streaming: Some(true),
            ..Default::default()
        };
        assert!(is_streaming(&server_stream));

        // Bidirectional
        let bidi = MethodDescriptorProto {
            client_streaming: Some(true),
            server_streaming: Some(true),
            ..Default::default()
        };
        assert!(is_streaming(&bidi));
    }
}
