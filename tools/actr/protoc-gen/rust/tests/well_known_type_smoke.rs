use serde_json::{Value, json};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn protoc_passes_well_known_type_descriptors_to_the_plugin() {
    let temp = TempDir::new().expect("create temp directory");
    let proto_root = temp.path().join("proto");
    let local_dir = proto_root.join("local");
    let well_known_dir = proto_root.join("google/protobuf");
    let output_dir = temp.path().join("generated");
    let spec_dir = output_dir.join(".actr/codegen");
    fs::create_dir_all(&local_dir).expect("create local proto directory");
    fs::create_dir_all(&well_known_dir).expect("create well-known proto directory");
    fs::create_dir_all(&output_dir).expect("create generated directory");
    fs::create_dir_all(&spec_dir).expect("create spec directory");

    fs::write(
        well_known_dir.join("empty.proto"),
        r#"syntax = "proto3";
package google.protobuf;
message Empty {}
"#,
    )
    .expect("write empty.proto");
    fs::write(
        local_dir.join("client.proto"),
        r#"syntax = "proto3";
package client;
import "google/protobuf/empty.proto";
service Client {
  rpc Ping(google.protobuf.Empty) returns (google.protobuf.Empty);
}
"#,
    )
    .expect("write client.proto");

    // Write a minimal DispatchSpec v1 with one local route for the service
    // under test. The plugin reads this to determine local/remote
    // classification and to generate the unified `actr_dispatch.rs`.
    let spec = json!({
        "schema_version": "v1",
        "routes": [
            {
                "route_key": "client.Client.Ping",
                "proto_file": "local/client.proto",
                "proto_service": "client.Client",
                "method": "Ping",
                "input_type": "google.protobuf.Empty",
                "output_type": "google.protobuf.Empty",
                "payload_type": "rpcReliable",
                "reply_mode": "return",
                "dependency_alias": null
            }
        ]
    });
    let spec_path = spec_dir.join("actr-dispatch-spec.v1.json");
    fs::write(&spec_path, serde_json::to_string_pretty(&spec).unwrap())
        .expect("write dispatch spec");

    let output = Command::new("protoc")
        .arg(format!("--proto_path={}", proto_root.display()))
        .arg(format!(
            "--plugin=protoc-gen-actrframework={}",
            env!("CARGO_BIN_EXE_protoc-gen-actrframework")
        ))
        .arg(format!(
            "--actrframework_opt=publisher=acme,dispatch_spec={},dispatch_spec_hash=",
            spec_path.display()
        ))
        .arg(format!("--actrframework_out={}", output_dir.display()))
        .arg("local/client.proto")
        .output()
        .expect("run protoc");

    assert!(
        output.status.success(),
        "protoc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output_dir.join("client_actor.rs").is_file(),
        "framework code was not generated"
    );
    assert!(
        output_dir.join("actr_dispatch.rs").is_file(),
        "unified dispatcher was not generated"
    );

    let metadata: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("actr-gen-meta.json"))
            .expect("read generated metadata"),
    )
    .expect("parse generated metadata");
    let method = &metadata["local_services"][0]["methods"][0];

    assert_eq!(method["input_ref"]["proto_type"], "google.protobuf.Empty");
    assert_eq!(
        method["input_ref"]["proto_file"],
        "google/protobuf/empty.proto"
    );
    assert_eq!(method["output_ref"]["proto_type"], "google.protobuf.Empty");

    // The unified dispatcher must not embed ActrType strings or use
    // call_raw_with_timeout / DEFAULT_RPC_TIMEOUT_MS.
    let dispatch_code =
        fs::read_to_string(output_dir.join("actr_dispatch.rs")).expect("read actr_dispatch.rs");
    assert!(
        !dispatch_code.contains("call_raw_with_timeout"),
        "actr_dispatch.rs must not call call_raw_with_timeout"
    );
    assert!(
        !dispatch_code.contains("DEFAULT_RPC_TIMEOUT_MS"),
        "actr_dispatch.rs must not reference DEFAULT_RPC_TIMEOUT_MS"
    );
    assert!(
        !dispatch_code.contains("publisher"),
        "actr_dispatch.rs must not embed ActrType fields"
    );
    assert!(
        dispatch_code.contains("actr_dispatch"),
        "actr_dispatch.rs must define the actr_dispatch function"
    );
    assert!(
        dispatch_code.contains("UnknownRoute"),
        "actr_dispatch.rs must return UnknownRoute for unknown routes"
    );
    // The local route key must appear in the match arms.
    assert!(
        dispatch_code.contains("\"client.Client.Ping\""),
        "actr_dispatch.rs must include the local route key"
    );
    // The metadata must carry the new schema_version and route lists.
    assert_eq!(metadata["schema_version"], "v1");
    assert!(metadata["generated_local_routes"].is_array());
    assert!(
        metadata["generated_local_routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "client.Client.Ping"),
        "generated_local_routes must contain the local route"
    );
    // The metadata must NOT carry actr_type on remote_services.
    if let Some(remote_services) = metadata["remote_services"].as_array() {
        for svc in remote_services {
            assert!(
                svc.get("actr_type").is_none() || svc["actr_type"].as_str().is_none(),
                "remote_services must not carry actr_type"
            );
        }
    }
}
