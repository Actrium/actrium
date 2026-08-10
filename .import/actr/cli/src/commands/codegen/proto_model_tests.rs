use super::*;
use actr_config::ConfigParser;
use tempfile::TempDir;

#[test]
fn classify_proto_side_is_remote_when_first_component_is_remote() {
    assert_eq!(
        classify_proto_side(Path::new("remote/echo.proto")),
        ProtoSide::Remote
    );
    assert_eq!(
        classify_proto_side(Path::new("local/echo.proto")),
        ProtoSide::Local
    );
    assert_eq!(
        classify_proto_side(Path::new("echo.proto")),
        ProtoSide::Local
    );
}

#[test]
fn remote_dependency_alias_comes_from_the_path() {
    assert_eq!(
        remote_dependency_alias(Path::new("remote/echo-server/echo.proto")),
        Some("echo-server")
    );
    assert_eq!(
        remote_dependency_alias(Path::new("remote/echo.proto")),
        None
    );
}

#[test]
fn normalize_proto_type_trims_leading_dot_and_whitespace() {
    assert_eq!(normalize_proto_type("  .EchoRequest "), "EchoRequest");
    assert_eq!(normalize_proto_type("EchoResponse"), "EchoResponse");
    assert_eq!(normalize_proto_type(""), "");
}

#[test]
fn extract_declared_type_name_finds_message_and_enum() {
    assert!(extract_declared_type_name("message EchoRequest {", "message ").is_some());
    assert!(extract_declared_type_name("enum Status {", "enum ").is_some());
    assert!(extract_declared_type_name("not matching", "message ").is_none());
}

#[test]
fn parse_rpc_method_parses_stream_and_unary_signatures() {
    let m = parse_rpc_method(
        "Echo(EchoRequest) returns (EchoResponse);",
        "echo",
        "EchoService",
    )
    .unwrap();
    assert_eq!(m.name, "Echo");
    assert_eq!(m.snake_name, "echo");
    assert_eq!(m.input_type, "EchoRequest");
    assert_eq!(m.output_type, "EchoResponse");
    assert_eq!(m.route_key, "echo.EchoService.Echo");
}

#[test]
fn parse_rejects_proto_path_escaping_proto_root() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    std::fs::create_dir_all(proto_root.join("local")).unwrap();
    let config_path = tmp.path().join("manifest.toml");
    std::fs::write(
        &config_path,
        "edition = 1\nexports = []\n[package]\nname = \"Demo\"\nmanufacturer = \"acme\"\nversion = \"0.1.0\"\n[system.signaling]\nurl = \"ws://127.0.0.1:8080\"\n[system.ais_endpoint]\nurl = \"http://127.0.0.1:8080/ais\"\n[system.deployment]\nrealm_id = 1001\n",
    )
    .unwrap();
    let config = ConfigParser::from_manifest_file(&config_path).unwrap();

    // A proto file path that escapes the proto root via `..` must be rejected
    // so traversal sequences cannot reach generated import/module paths.
    let escaping = proto_root.join("local/../evil.proto");
    let result = ProtoModel::parse(&[escaping], &proto_root, &config);
    assert!(
        result.is_err(),
        "proto path escaping the root should error, got: {:?}",
        result
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("escapes the proto root"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn type_owner_index_does_not_bare_resolve_unmatched_qualified_type() {
    let files = vec![
        ProtoFileModel {
            proto_file: PathBuf::from("local/app.proto"),
            relative_path: PathBuf::from("local/app.proto"),
            package: "app".to_string(),
            side: ProtoSide::Local,
            declared_type_names: vec!["Empty".to_string()],
            services: vec![],
        },
        ProtoFileModel {
            proto_file: PathBuf::from("remote/other.proto"),
            relative_path: PathBuf::from("remote/other.proto"),
            package: "other".to_string(),
            side: ProtoSide::Remote,
            declared_type_names: vec!["Request".to_string()],
            services: vec![],
        },
    ];
    let index = TypeOwnerIndex::from_files(&files);

    assert_eq!(
        index.resolve("google.protobuf.Empty", &files[0]),
        Ok(None),
        "unmatched qualified external types must not resolve by bare name"
    );
    assert_eq!(
        index.resolve("vendor.pkg.Request", &files[0]),
        Ok(None),
        "unmatched qualified external types must not resolve to a same-bare imported owner"
    );
}

// ---------------------------------------------------------------------------
// Raw descriptor payload-type wire scanner
// ---------------------------------------------------------------------------

/// Encode a u64 as a base-128 varint (protobuf wire format).
fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 0x80 {
        bytes.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

fn encode_length_delimited_field(field_number: u64, payload: &[u8]) -> Vec<u8> {
    let mut encoded = encode_varint((field_number << 3) | 2);
    encoded.extend_from_slice(&encode_varint(payload.len() as u64));
    encoded.extend_from_slice(payload);
    encoded
}

/// Build the smallest nested FileDescriptorSet wire payload containing one
/// file, one service, and one method. The method optionally carries custom
/// MethodOptions extension field 50001.
fn descriptor_set_with_payload_type(value: Option<u64>) -> Vec<u8> {
    let raw_method = value.map_or_else(Vec::new, |value| {
        let mut raw_options = encode_varint(50001_u64 << 3);
        raw_options.extend_from_slice(&encode_varint(value));
        encode_length_delimited_field(4, &raw_options)
    });
    let raw_service = encode_length_delimited_field(2, &raw_method);
    let raw_file = encode_length_delimited_field(6, &raw_service);
    encode_length_delimited_field(1, &raw_file)
}

// --- PayloadTypeValue::from_u64 ---

#[test]
fn payload_type_from_u64_maps_all_valid_values() {
    assert_eq!(
        PayloadTypeValue::from_u64(0),
        Some(PayloadTypeValue::RpcReliable)
    );
    assert_eq!(
        PayloadTypeValue::from_u64(1),
        Some(PayloadTypeValue::RpcSignal)
    );
    assert_eq!(
        PayloadTypeValue::from_u64(2),
        Some(PayloadTypeValue::StreamReliable)
    );
    assert_eq!(
        PayloadTypeValue::from_u64(3),
        Some(PayloadTypeValue::StreamLatencyFirst),
    );
    assert_eq!(
        PayloadTypeValue::from_u64(4),
        Some(PayloadTypeValue::MediaRtp)
    );
}

#[test]
fn payload_type_from_u64_returns_none_for_invalid_values() {
    assert_eq!(PayloadTypeValue::from_u64(5), None);
    assert_eq!(PayloadTypeValue::from_u64(u64::MAX), None);
}

#[test]
fn payload_type_as_lower_camel_matches_wire_format_names() {
    assert_eq!(
        PayloadTypeValue::RpcReliable.as_lower_camel(),
        "rpcReliable"
    );
    assert_eq!(PayloadTypeValue::RpcSignal.as_lower_camel(), "rpcSignal");
    assert_eq!(
        PayloadTypeValue::StreamReliable.as_lower_camel(),
        "streamReliable",
    );
    assert_eq!(
        PayloadTypeValue::StreamLatencyFirst.as_lower_camel(),
        "streamLatencyFirst",
    );
    assert_eq!(PayloadTypeValue::MediaRtp.as_lower_camel(), "mediaRtp");
}

// --- read_varint ---

#[test]
fn read_varint_decodes_single_byte_values() {
    for (bytes, expected) in [(&[0x00][..], 0), (&[0x01][..], 1), (&[0x7f][..], 127)] {
        let mut pos = 0;
        assert_eq!(read_varint(bytes, &mut pos), Ok(expected));
        assert_eq!(pos, 1);
    }
}

#[test]
fn read_varint_decodes_multi_byte_values() {
    for (bytes, expected) in [
        (&[0x80, 0x01][..], 128),
        (&[0xac, 0x02][..], 300),
        (&[0x80, 0x80, 0x01][..], 16_384),
    ] {
        let mut pos = 0;
        assert_eq!(read_varint(bytes, &mut pos), Ok(expected));
        assert_eq!(pos, bytes.len());
    }
}

#[test]
fn read_varint_rejects_empty_and_overflowing_buffers() {
    let mut pos = 0;
    assert!(read_varint(&[], &mut pos).is_err());

    let mut pos = 0;
    assert!(read_varint(&[0xff; 10], &mut pos).is_err());
}

#[test]
fn raw_descriptor_scan_preserves_payload_type_extension() {
    let descriptor = descriptor_set_with_payload_type(Some(1));
    let payload_types = extract_payload_types_from_descriptor_set(&descriptor).unwrap();
    assert_eq!(
        payload_types,
        vec![vec![vec![Some(PayloadTypeValue::RpcSignal)]]]
    );
}

#[test]
fn raw_descriptor_scan_leaves_missing_payload_type_unset() {
    let descriptor = descriptor_set_with_payload_type(None);
    let payload_types = extract_payload_types_from_descriptor_set(&descriptor).unwrap();
    assert_eq!(payload_types, vec![vec![vec![None]]]);
}

#[test]
fn raw_descriptor_scan_rejects_unknown_payload_type_value() {
    let descriptor = descriptor_set_with_payload_type(Some(99));
    let error = extract_payload_types_from_descriptor_set(&descriptor).unwrap_err();
    assert!(error.to_string().contains("unknown enum value 99"));
}

#[test]
fn raw_descriptor_scan_rejects_truncated_nested_fields() {
    let mut descriptor = descriptor_set_with_payload_type(Some(1));
    descriptor.pop();
    assert!(extract_payload_types_from_descriptor_set(&descriptor).is_err());
}
