//! Tests for the descriptor-based proto model and dispatch spec.
//!
//! These tests require `protoc` to be available in PATH because the model
//! builder invokes `protoc --descriptor_set_out` to obtain
//! `FileDescriptorSet` descriptors. Tests that don't need protoc are in
//! `proto_model_tests.rs`.

use crate::commands::codegen::dispatch_spec::{DISPATCH_SPEC_SCHEMA_VERSION, DispatchRoute};
use crate::commands::codegen::{
    DispatchSpec, MethodModel, ProtoModel, ProtoSide, ReplyMode, ServiceModel,
};
use actr_config::ConfigParser;
use std::path::PathBuf;
use tempfile::TempDir;

fn write_manifest_with_dependency(root: &std::path::Path) -> actr_config::ManifestConfig {
    let config_path = root.join("manifest.toml");
    std::fs::write(
        &config_path,
        r#"edition = 1
exports = []

[dependencies]
echo-server = { actr_type = "acme/Echo@1.0.0" }

[package]
name = "TestApp"
publisher = "acme"
version = "0.1.0"

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.issuer_endpoint]
url = "http://127.0.0.1:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
    ConfigParser::from_manifest_file(&config_path).unwrap()
}

fn write_manifest_no_deps(root: &std::path::Path) -> actr_config::ManifestConfig {
    let config_path = root.join("manifest.toml");
    std::fs::write(
        &config_path,
        r#"edition = 1
exports = []

[package]
name = "TestApp"
publisher = "acme"
version = "0.1.0"

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.issuer_endpoint]
url = "http://127.0.0.1:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
    ConfigParser::from_manifest_file(&config_path).unwrap()
}

fn write_simple_echo_proto(proto_root: &std::path::Path) -> std::path::PathBuf {
    let local_dir = proto_root.join("local");
    std::fs::create_dir_all(&local_dir).unwrap();
    let proto = local_dir.join("echo.proto");
    std::fs::write(&proto,
        "syntax = \"proto3\";\npackage echo;\n\nmessage EchoRequest {}\nmessage EchoResponse {}\n\nservice EchoService {\n  rpc Echo(EchoRequest) returns (EchoResponse);\n  rpc Echo2(EchoRequest) returns (EchoResponse);\n}\n",
    ).unwrap();
    proto
}

fn write_remote_proto(proto_root: &std::path::Path) -> std::path::PathBuf {
    let remote_dir = proto_root.join("remote/echo-server");
    std::fs::create_dir_all(&remote_dir).unwrap();
    let proto = remote_dir.join("echo.proto");
    std::fs::write(&proto,
        "syntax = \"proto3\";\npackage remote_echo;\n\nmessage EchoRequest {}\nmessage EchoResponse {}\n\nservice EchoService {\n  rpc Echo(EchoRequest) returns (EchoResponse);\n}\n",
    ).unwrap();
    proto
}

fn write_streaming_proto(proto_root: &std::path::Path) -> std::path::PathBuf {
    let local_dir = proto_root.join("local");
    std::fs::create_dir_all(&local_dir).unwrap();
    let proto = local_dir.join("stream.proto");
    std::fs::write(&proto,
        "syntax = \"proto3\";\npackage stream;\n\nmessage Chunk {}\nmessage Response {}\n\nservice StreamService {\n  rpc Download(Response) returns (stream Chunk);\n}\n",
    ).unwrap();
    proto
}

#[test]
fn dispatch_spec_builds_sorted_deduped_spec_from_proto_model() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let proto = write_simple_echo_proto(&proto_root);
    let config = write_manifest_no_deps(tmp.path());

    let model = ProtoModel::parse(&[proto], &proto_root, &config).unwrap();
    let spec = DispatchSpec::build(&model).unwrap();

    assert_eq!(spec.schema_version, DISPATCH_SPEC_SCHEMA_VERSION);
    assert_eq!(spec.routes.len(), 2);
    assert!(spec.routes[0].route_key < spec.routes[1].route_key);
    assert!(spec.routes.iter().all(|r| r.dependency_alias.is_none()));
    assert!(spec.spec_hash.is_some());
    assert!(!spec.spec_hash.as_ref().unwrap().is_empty());
}

#[test]
fn dispatch_spec_remote_routes_carry_dependency_alias() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let local_proto = write_simple_echo_proto(&proto_root);
    let remote_proto = write_remote_proto(&proto_root);
    let config = write_manifest_with_dependency(tmp.path());

    let model = ProtoModel::parse(&[local_proto, remote_proto], &proto_root, &config).unwrap();
    let spec = DispatchSpec::build(&model).unwrap();

    let remote_routes: Vec<&DispatchRoute> = spec
        .routes
        .iter()
        .filter(|r| r.dependency_alias.is_some())
        .collect();
    assert!(
        !remote_routes.is_empty(),
        "at least one remote route should exist"
    );
    for route in &remote_routes {
        assert_eq!(route.dependency_alias.as_deref(), Some("echo-server"));
    }
}

#[test]
fn dispatch_spec_remote_routes_preserve_declared_payload_type() {
    let model = ProtoModel {
        files: vec![],
        local_services: vec![],
        remote_services: vec![ServiceModel {
            name: "SignalService".to_string(),
            package: "signal".to_string(),
            proto_file: PathBuf::from("remote/signal-service/signal.proto"),
            relative_path: PathBuf::from("remote/signal-service/signal.proto"),
            side: ProtoSide::Remote,
            methods: vec![MethodModel {
                name: "Notify".to_string(),
                snake_name: "notify".to_string(),
                input_type: "Request".to_string(),
                output_type: "Response".to_string(),
                payload_type: "rpcSignal".to_string(),
                reply_mode: ReplyMode::Return,
                route_key: "signal.SignalService.Notify".to_string(),
            }],
            actr_type: Some("acme/SignalService@1.0.0".to_string()),
        }],
    };

    let spec = DispatchSpec::build(&model).expect("non-default remote payload type is supported");
    assert_eq!(spec.routes[0].payload_type, "rpcSignal");
}

#[test]
fn dispatch_spec_rejects_non_rpc_payload_type_for_remote_route() {
    let model = ProtoModel {
        files: vec![],
        local_services: vec![],
        remote_services: vec![ServiceModel {
            name: "StreamService".to_string(),
            package: "stream".to_string(),
            proto_file: PathBuf::from("remote/stream-service/stream.proto"),
            relative_path: PathBuf::from("remote/stream-service/stream.proto"),
            side: ProtoSide::Remote,
            methods: vec![MethodModel {
                name: "Send".to_string(),
                snake_name: "send".to_string(),
                input_type: "Request".to_string(),
                output_type: "Response".to_string(),
                payload_type: "dataStream".to_string(),
                reply_mode: ReplyMode::Return,
                route_key: "stream.StreamService.Send".to_string(),
            }],
            actr_type: Some("acme/StreamService@1.0.0".to_string()),
        }],
    };

    let error = DispatchSpec::build(&model)
        .expect_err("remote callRaw routes must reject non-RPC payload types")
        .to_string();
    assert!(error.contains("dataStream"), "unexpected error: {error}");
    assert!(error.contains("rpcSignal"), "unexpected error: {error}");
}

#[test]
fn proto_model_rejects_remote_alias_missing_from_manifest() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let remote_proto = write_remote_proto(&proto_root);
    let config = write_manifest_no_deps(tmp.path());

    let error = ProtoModel::parse(&[remote_proto], &proto_root, &config)
        .expect_err("an undeclared remote dependency alias must fail codegen");
    let message = error.to_string();
    assert!(message.contains("echo-server"), "{message}");
    assert!(message.contains("complete actr_type"), "{message}");
}

#[test]
fn dispatch_spec_streaming_rpc_errors() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let proto = write_streaming_proto(&proto_root);
    let config = write_manifest_no_deps(tmp.path());

    let result = ProtoModel::parse(&[proto], &proto_root, &config);
    let err = result.expect_err("streaming RPC should error");
    let msg = err.to_string();
    assert!(
        msg.contains("v1 dispatch spec does not support streaming RPC"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn dispatch_spec_hash_is_stable_across_runs() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let proto = write_simple_echo_proto(&proto_root);
    let config = write_manifest_no_deps(tmp.path());

    let model1 = ProtoModel::parse(std::slice::from_ref(&proto), &proto_root, &config).unwrap();
    let spec1 = DispatchSpec::build(&model1).unwrap();

    let model2 = ProtoModel::parse(&[proto], &proto_root, &config).unwrap();
    let spec2 = DispatchSpec::build(&model2).unwrap();

    assert_eq!(spec1.spec_hash, spec2.spec_hash, "spec_hash must be stable");
}

#[test]
fn dispatch_spec_to_json_string_is_valid_json() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let proto = write_simple_echo_proto(&proto_root);
    let config = write_manifest_no_deps(tmp.path());

    let model = ProtoModel::parse(&[proto], &proto_root, &config).unwrap();
    let spec = DispatchSpec::build(&model).unwrap();
    let json = spec.to_json_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["schema_version"], "v1");
    assert!(parsed["routes"].is_array());
    assert!(parsed["spec_hash"].is_string());
}

fn make_local_service(
    name: &str,
    package: &str,
    proto_file: &str,
    methods: Vec<(&str, &str)>,
) -> ServiceModel {
    let methods = methods
        .into_iter()
        .map(|(method_name, route_key)| MethodModel {
            name: method_name.to_string(),
            snake_name: method_name.to_lowercase(),
            input_type: "Req".to_string(),
            output_type: "Resp".to_string(),
            payload_type: "rpcReliable".to_string(),
            reply_mode: ReplyMode::Return,
            route_key: route_key.to_string(),
        })
        .collect();
    ServiceModel {
        name: name.to_string(),
        package: package.to_string(),
        proto_file: PathBuf::from(proto_file),
        relative_path: PathBuf::from(proto_file),
        side: ProtoSide::Local,
        methods,
        actr_type: None,
    }
}

#[test]
fn dispatch_spec_rejects_colliding_local_handler_method_names() {
    // Two LOCAL services with the SAME service name from different proto
    // packages share the method name "Echo". Their <service>_<method>
    // identifiers collide (`echo_service_echo`) for every generator, so this
    // must be caught at build time regardless of target language.
    let model = ProtoModel {
        files: vec![],
        local_services: vec![
            make_local_service(
                "EchoService",
                "svc.a",
                "local/a.proto",
                vec![("Echo", "svc.a.EchoService.Echo")],
            ),
            make_local_service(
                "EchoService",
                "svc.b",
                "local/b.proto",
                vec![("Echo", "svc.b.EchoService.Echo")],
            ),
        ],
        remote_services: vec![],
    };

    let err = DispatchSpec::build(&model)
        .expect_err("colliding local handler method names must be rejected at build time");
    let msg = err.to_string();
    assert!(
        msg.contains("collision") && msg.contains("Echo"),
        "error must name the collision and method: {msg}",
    );
    assert!(
        msg.contains("svc.a.EchoService.Echo") && msg.contains("svc.b.EchoService.Echo"),
        "error must name both colliding routes: {msg}",
    );
}

#[test]
fn dispatch_spec_allows_same_rpc_name_across_services() {
    // Two local services with DIFFERENT names sharing the RPC name "Ask":
    // legal for Rust/Python/TypeScript, whose dispatchers use the
    // `<service>_<method>` prefix (`ask_service_ask` / `chat_service_ask`).
    // Only the bare-method-name generators (Kotlin/Swift) reject this, via
    // `reject_bare_method_name_collisions`.
    let model = ProtoModel {
        files: vec![],
        local_services: vec![
            make_local_service(
                "AskService",
                "ask",
                "local/ask.proto",
                vec![("Ask", "ask.AskService.Ask")],
            ),
            make_local_service(
                "ChatService",
                "chat",
                "local/chat.proto",
                vec![("Ask", "chat.ChatService.Ask")],
            ),
        ],
        remote_services: vec![],
    };

    let spec =
        DispatchSpec::build(&model).expect("same RPC name across services must build a spec");
    assert_eq!(spec.routes.len(), 2);
}

#[test]
fn bare_method_name_collision_check_targets_kotlin_swift_dispatch() {
    let model = ProtoModel {
        files: vec![],
        local_services: vec![
            make_local_service(
                "AskService",
                "ask",
                "local/ask.proto",
                vec![("Ask", "ask.AskService.Ask")],
            ),
            make_local_service(
                "ChatService",
                "chat",
                "local/chat.proto",
                vec![("Ask", "chat.ChatService.Ask")],
            ),
        ],
        remote_services: vec![],
    };

    // Kotlin/Swift dispatch by the bare method name → rejected with an
    // actionable error.
    let err = super::reject_bare_method_name_collisions(&model)
        .expect_err("bare method name collision must be rejected for Kotlin/Swift");
    let msg = err.to_string();
    assert!(msg.contains("`ask`"), "error must name the method: {msg}");
    assert!(
        msg.contains("ask.AskService.Ask") && msg.contains("chat.ChatService.Ask"),
        "error must name both routes: {msg}"
    );

    // Distinct method names pass the bare-name check (the universal
    // <service>_<method> constraint is enforced by DispatchSpec::build).
    let ok_model = ProtoModel {
        files: vec![],
        local_services: vec![
            make_local_service(
                "AskService",
                "ask",
                "local/ask.proto",
                vec![("Query", "ask.AskService.Query")],
            ),
            make_local_service(
                "ChatService",
                "chat",
                "local/chat.proto",
                vec![("Ask", "chat.ChatService.Ask")],
            ),
        ],
        remote_services: vec![],
    };
    super::reject_bare_method_name_collisions(&ok_model)
        .expect("distinct method names must pass the bare-name check");
}

#[test]
fn dispatch_spec_rejects_handler_names_that_collide_after_normalization() {
    let model = ProtoModel {
        files: vec![],
        local_services: vec![make_local_service(
            "EchoService",
            "svc.a",
            "local/a.proto",
            vec![
                ("FooBar", "svc.a.EchoService.FooBar"),
                ("Foo_Bar", "svc.a.EchoService.Foo_Bar"),
            ],
        )],
        remote_services: vec![],
    };

    let error = DispatchSpec::build(&model)
        .expect_err("normalized handler identifiers must remain unique")
        .to_string();
    assert!(error.contains("foo_bar"), "unexpected error: {error}");
    assert!(error.contains("FooBar") && error.contains("Foo_Bar"));
}

#[test]
fn dispatch_spec_allows_same_method_name_within_same_service() {
    // A single service can repeat a method name across different route keys
    // (unusual but not a cross-service collision).
    let model = ProtoModel {
        files: vec![],
        local_services: vec![make_local_service(
            "EchoService",
            "svc.a",
            "local/a.proto",
            vec![("Echo", "svc.a.EchoService.Echo")],
        )],
        remote_services: vec![],
    };

    let spec = DispatchSpec::build(&model)
        .expect("single-service dispatch spec must build without collision errors");
    assert_eq!(spec.routes.len(), 1);
}

#[test]
fn dispatch_spec_allows_colliding_method_names_for_remote_routes() {
    // Remote routes use a different dispatch path (dependency alias
    // qualifies the handler), so the cross-service collision check does
    // not apply to them.
    let remote = ServiceModel {
        name: "RemoteService".to_string(),
        package: "svc.remote".to_string(),
        proto_file: PathBuf::from("remote/dep/remote.proto"),
        relative_path: PathBuf::from("remote/dep/remote.proto"),
        side: ProtoSide::Remote,
        methods: vec![MethodModel {
            name: "Echo".to_string(),
            snake_name: "echo".to_string(),
            input_type: "Req".to_string(),
            output_type: "Resp".to_string(),
            payload_type: "rpcReliable".to_string(),
            reply_mode: ReplyMode::Return,
            route_key: "svc.remote.RemoteService.Echo".to_string(),
        }],
        actr_type: Some("acme/Echo@1.0.0".to_string()),
    };

    let model = ProtoModel {
        files: vec![],
        local_services: vec![make_local_service(
            "EchoService",
            "svc.local",
            "local/a.proto",
            vec![("Echo", "svc.local.EchoService.Echo")],
        )],
        remote_services: vec![remote],
    };

    let spec = DispatchSpec::build(&model)
        .expect("remote route with same method name as a local route must not trigger collision");
    assert_eq!(spec.routes.len(), 2);
}
