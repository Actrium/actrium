//! PayloadType option extractor
//!
//! Extracts `option (actr.payload_type)` from protobuf method descriptors.
//!
//! prost discards unknown fields when decoding `MethodOptions`, so the
//! extension (field 50001) never reaches `MethodDescriptorProto.options`.
//! The explicit values are therefore recovered by re-decoding the raw
//! `CodeGeneratorRequest` bytes with rust-protobuf, which retains unknown
//! fields — see [`build_explicit_payload_types`]. The prost-side
//! [`extract_payload_type_or_default`] only provides the fallback for
//! methods that do not set the option.

use std::collections::HashMap;

use anyhow::{Context as _, bail};
use prost_types::MethodDescriptorProto;
use protobuf::plugin::CodeGeneratorRequest as PbCodeGeneratorRequest;
use protobuf::{Message as _, UnknownValueRef};

/// Extension field number of `(actr.payload_type)` on `MethodOptions`.
const PAYLOAD_TYPE_FIELD_NUMBER: u32 = 50001;

/// Structured method identity used to correlate the extraction side (raw
/// request bytes) with the generation side (prost descriptors). Both sides
/// build the key from the same fields, decoupling it from the emitted
/// `route_key` string format.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct MethodKey {
    package: String,
    service: String,
    method: String,
}

impl MethodKey {
    pub(crate) fn new(package: &str, service: &str, method: &str) -> Self {
        Self {
            package: package.to_string(),
            service: service.to_string(),
            method: method.to_string(),
        }
    }
}

/// Explicitly declared `(actr.payload_type)` values keyed by method.
///
/// Only methods that actually set the option appear here; everything else
/// falls back per [`extract_payload_type_or_default`].
pub type ExplicitPayloadTypeMap = HashMap<MethodKey, PayloadType>;

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

/// Recover explicitly declared `(actr.payload_type)` values from the raw
/// `CodeGeneratorRequest` bytes.
///
/// prost does not retain unknown fields, so the same bytes protoc sent to
/// the plugin are decoded a second time with rust-protobuf, and extension
/// field 50001 is read from each method's `MethodOptions.unknown_fields`.
///
/// # Option Format
///
/// ```protobuf
/// extend google.protobuf.MethodOptions {
///     optional PayloadType payload_type = 50001;
/// }
///
/// rpc SendUrgentMessage(Request) returns (Response) {
///     option (actr.payload_type) = RPC_SIGNAL;
/// }
/// ```
///
/// # Errors
///
/// A method that simply does not set the option is *not* an error — it is
/// absent from the returned map and the caller applies the fallback. Every
/// "the generator is broken" condition is an error, with the method named
/// in the message: undecodable request bytes, the extension carried with a
/// wrong wire type, or an enum value outside the `PayloadType` range.
/// Silently degrading any of those to `RpcReliable` would disguise a
/// codegen defect as a deliberate lane choice.
pub fn build_explicit_payload_types(buf: &[u8]) -> anyhow::Result<ExplicitPayloadTypeMap> {
    let request = PbCodeGeneratorRequest::parse_from_bytes(buf)
        .context("failed to decode CodeGeneratorRequest for payload_type extraction")?;

    let mut map = HashMap::new();
    for file in &request.proto_file {
        let package = file.package.clone().unwrap_or_default();
        for service in &file.service {
            let service_name = service.name.clone().unwrap_or_default();
            for method in &service.method {
                let method_name = method.name.clone().unwrap_or_default();
                let key = MethodKey::new(&package, &service_name, &method_name);

                let Some(options) = method.options.as_ref() else {
                    continue;
                };
                match options.unknown_fields().get(PAYLOAD_TYPE_FIELD_NUMBER) {
                    // Option not set: legal. The method is absent from the
                    // map and the generation side applies the fallback.
                    None => {}
                    Some(UnknownValueRef::Varint(value)) => {
                        let payload_type = PayloadType::from_u64(value).with_context(|| {
                            format!(
                                "unsupported payload_type value {value} for \
                                 {package}.{service_name}.{method_name}"
                            )
                        })?;
                        map.insert(key, payload_type);
                    }
                    Some(other) => bail!(
                        "invalid payload_type wire value on \
                         {package}.{service_name}.{method_name}: {other:?}"
                    ),
                }
            }
        }
    }
    Ok(map)
}

/// Check if method uses streaming
fn is_streaming(method: &MethodDescriptorProto) -> bool {
    // Check if input or output is streaming
    method.client_streaming.unwrap_or(false) || method.server_streaming.unwrap_or(false)
}

/// Fallback PayloadType for methods that do not set `(actr.payload_type)`.
///
/// Explicit option values never reach this function — prost drops the
/// extension during decode — they are recovered by
/// [`build_explicit_payload_types`] and consulted by the caller first.
///
/// # Default Rules
///
/// - Unary RPC (no stream) → `RPC_RELIABLE`
/// - Streaming RPC (has stream keyword) → `STREAM_RELIABLE`
///
/// # Example
///
/// ```rust,ignore
/// let payload_type = extract_payload_type_or_default(method);
/// // Unary RPC: PayloadType::RpcReliable
/// // Streaming RPC: PayloadType::StreamReliable
/// ```
pub(crate) fn extract_payload_type_or_default(method: &MethodDescriptorProto) -> PayloadType {
    if is_streaming(method) {
        return PayloadType::StreamReliable;
    }

    PayloadType::RpcReliable
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::descriptor::{
        FileDescriptorProto, MethodDescriptorProto as PbMethodDescriptorProto,
        ServiceDescriptorProto,
    };

    /// Build a serialized CodeGeneratorRequest whose single method carries
    /// field 50001 as a raw unknown varint (None = option unset).
    fn request_bytes_with_payload_type(payload_type: Option<u64>) -> Vec<u8> {
        let mut method = PbMethodDescriptorProto::new();
        method.name = Some("Echo".to_string());
        if let Some(value) = payload_type {
            let mut options = protobuf::descriptor::MethodOptions::new();
            options
                .mut_unknown_fields()
                .add_varint(PAYLOAD_TYPE_FIELD_NUMBER, value);
            method.options = protobuf::MessageField::some(options);
        }

        let mut service = ServiceDescriptorProto::new();
        service.name = Some("EchoService".to_string());
        service.method.push(method);

        let mut file = FileDescriptorProto::new();
        file.name = Some("echo.proto".to_string());
        file.package = Some("echo.v1".to_string());
        file.service.push(service);

        let mut request = PbCodeGeneratorRequest::new();
        request.proto_file.push(file);
        request.write_to_bytes().expect("serialize request")
    }

    #[test]
    fn test_build_explicit_payload_types_reads_set_option() {
        let bytes = request_bytes_with_payload_type(Some(1));
        let map = build_explicit_payload_types(&bytes).expect("extract");
        assert_eq!(
            map.get(&MethodKey::new("echo.v1", "EchoService", "Echo")),
            Some(&PayloadType::RpcSignal)
        );
    }

    #[test]
    fn test_build_explicit_payload_types_omits_unset_option() {
        let bytes = request_bytes_with_payload_type(None);
        let map = build_explicit_payload_types(&bytes).expect("extract");
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_explicit_payload_types_rejects_unknown_value() {
        let bytes = request_bytes_with_payload_type(Some(99));
        let err = build_explicit_payload_types(&bytes).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unsupported payload_type value 99")
                && msg.contains("echo.v1.EchoService.Echo"),
            "error must name the value and method: {msg}"
        );
    }

    #[test]
    fn test_build_explicit_payload_types_rejects_wrong_wire_type() {
        let mut method = PbMethodDescriptorProto::new();
        method.name = Some("Echo".to_string());
        let mut options = protobuf::descriptor::MethodOptions::new();
        options
            .mut_unknown_fields()
            .add_fixed32(PAYLOAD_TYPE_FIELD_NUMBER, 1);
        method.options = protobuf::MessageField::some(options);

        let mut service = ServiceDescriptorProto::new();
        service.name = Some("EchoService".to_string());
        service.method.push(method);

        let mut file = FileDescriptorProto::new();
        file.package = Some("echo.v1".to_string());
        file.service.push(service);

        let mut request = PbCodeGeneratorRequest::new();
        request.proto_file.push(file);
        let bytes = request.write_to_bytes().expect("serialize request");

        let err = build_explicit_payload_types(&bytes).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid payload_type wire value")
                && msg.contains("echo.v1.EchoService.Echo"),
            "error must name the wire-type problem and method: {msg}"
        );
    }

    #[test]
    fn test_build_explicit_payload_types_rejects_garbage_bytes() {
        let err = build_explicit_payload_types(&[0xff, 0xff, 0xff]).unwrap_err();
        assert!(
            format!("{err:#}").contains("failed to decode CodeGeneratorRequest"),
            "error must describe the decode failure: {err:#}"
        );
    }

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
