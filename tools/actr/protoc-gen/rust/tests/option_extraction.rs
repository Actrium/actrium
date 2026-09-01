//! Integration test: protoc → descriptor set → `prost-reflect` recovers the
//! `actr.reply_mode` and `actr.payload_type` custom method options.
//!
//! This is the proof that the protoc plugin can extract these options: the
//! plugin receives a `CodeGeneratorRequest` whose `proto_file` entries are
//! `FileDescriptorProto`s. `prost` drops unknown extension fields on decode,
//! but `prost-reflect`'s `DescriptorPool` decodes the descriptor tree itself
//! and surfaces custom options via `MethodDescriptor::options()`. If this
//! test passes, the extractor in `modern_generator` can read `reply_mode`
//! from the pool instead of the stub `payload_type_extractor`.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// The canonical `actr/options.proto` content, inlined so the test is
/// self-contained.
const OPTIONS_PROTO: &str = include_str!("../../../../core/protocol/proto/actr/options.proto");

const SERVICE_PROTO: &str = r#"syntax = "proto2";
package demo;
import "actr/options.proto";
message Req { optional string s = 1; }
message Resp { optional string r = 1; }
service Svc {
  rpc Probe(Req) returns (Resp);
  rpc Transcode(Req) returns (Resp) {
    option (actr.reply_mode) = REPLY_MODE_EXPLICIT;
  }
  rpc Signal(Req) returns (Resp) {
    option (actr.payload_type) = RPC_SIGNAL;
  }
}
"#;

#[test]
fn prost_reflect_recovers_actr_method_options() {
    let temp = TempDir::new().expect("temp dir");
    let proto_root = temp.path().join("proto");
    let actr_dir = proto_root.join("actr");
    fs::create_dir_all(&actr_dir).expect("actr dir");
    fs::write(actr_dir.join("options.proto"), OPTIONS_PROTO).expect("options.proto");
    fs::write(proto_root.join("svc.proto"), SERVICE_PROTO).expect("svc.proto");

    let desc_set = temp.path().join("desc.bin");
    let output = Command::new("protoc")
        .arg(format!("--proto_path={}", proto_root.display()))
        .arg("--include_imports")
        .arg(format!("--descriptor_set_out={}", desc_set.display()))
        .arg(proto_root.join("svc.proto"))
        .output()
        .expect("run protoc");
    assert!(
        output.status.success(),
        "protoc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = fs::read(&desc_set).expect("read descriptor set");

    // `DescriptorPool::decode` decodes the raw bytes with prost-reflect's own
    // types, whose `Options<T>` wrapper retains unknown extension fields —
    // the doc explicitly notes extension options are preserved, unlike
    // `from_file_descriptor_set` (which re-encodes from the lossy prost struct
    // and drops them). This is the path the protoc plugin must use.
    let pool = prost_reflect::DescriptorPool::decode(bytes.as_slice()).expect("build pool");

    let svc = pool
        .get_service_by_name("demo.Svc")
        .expect("service exists");

    // Discover the extension descriptor from the pool, then read it off each
    // method's options message.
    let reply_mode_ext = pool
        .get_extension_by_name("actr.reply_mode")
        .expect("actr.reply_mode extension registered");
    let payload_type_ext = pool
        .get_extension_by_name("actr.payload_type")
        .expect("actr.payload_type extension registered");

    let reply_mode_of = |method_name: &str| -> i64 {
        let m = svc
            .methods()
            .find(|m| m.name() == method_name)
            .unwrap_or_else(|| panic!("method {method_name} not found"));
        let opts = m.options();
        let val = opts.get_extension(&reply_mode_ext);
        let prost_reflect::Value::EnumNumber(num) = &*val else {
            panic!("reply_mode for {method_name} was not an enum: {val:?}");
        };
        *num as i64
    };

    // Probe: no option set → default RETURN (0)
    assert_eq!(reply_mode_of("Probe"), 0);
    // Transcode: EXPLICIT (1)
    assert_eq!(reply_mode_of("Transcode"), 1);

    // payload_type round-trips too (the existing extractor stub is unblocked
    // by the same pool-based path).
    let signal = svc
        .methods()
        .find(|m| m.name() == "Signal")
        .expect("Signal method");
    let signal_opts = signal.options();
    let pt = signal_opts.get_extension(&payload_type_ext);
    let prost_reflect::Value::EnumNumber(num) = &*pt else {
        panic!("payload_type was not an enum: {pt:?}");
    };
    assert_eq!(*num, 1); // RPC_SIGNAL
}
