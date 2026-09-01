//! Integration tests for `actr init -l rust --template echo`
//!
//! These tests verify scaffold generation only (fast).
//! For end-to-end tests with real service communication, see `e2e_rust_echo.rs`.
//!
//! Run with: `cargo test --test rust_echo`

use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

fn actr_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_actr"))
}

fn run_actr(args: &[&str], cwd: &std::path::Path) -> Output {
    Command::new(actr_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run actr binary")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli crate should live under workspace root")
        .to_path_buf()
}

fn append_workspace_patch(project_dir: &std::path::Path) {
    let workspace = workspace_root();
    let cargo_toml = project_dir.join("Cargo.toml");
    // Adding an explicit `[workspace]` marker pins this project as its own
    // workspace root and stops cargo from walking up into unrelated parent
    // `Cargo.toml` files (e.g. a stray `/tmp/Cargo.toml`).
    let patch = format!(
        r#"

[workspace]

[patch.crates-io]
actr = {{ path = "{}" }}
actr-protocol = {{ path = "{}" }}
actr-framework = {{ path = "{}" }}
actr-hyper = {{ path = "{}" }}
actr-runtime = {{ path = "{}" }}
actr-config = {{ path = "{}" }}
actr-service-compat = {{ path = "{}" }}
actr-runtime-mailbox = {{ path = "{}" }}
"#,
        workspace.display(),
        workspace.join("crates/actr-protocol").display(),
        workspace.join("crates/actr-framework").display(),
        workspace.join("crates/actr-hyper").display(),
        workspace.join("crates/actr-runtime").display(),
        workspace.join("crates/actr-config").display(),
        workspace.join("crates/actr-service-compat").display(),
        workspace.join("crates/actr-runtime-mailbox").display(),
    );
    let mut content = std::fs::read_to_string(&cargo_toml).unwrap();
    content.push_str(&patch);
    std::fs::write(&cargo_toml, content).unwrap();
}

fn cargo_check(project_dir: &std::path::Path) -> Output {
    // Use an explicit `--manifest-path` so cargo does not walk up the directory
    // tree and accidentally pick up an unrelated Cargo.toml (for example when
    // tempdirs live under `/tmp` and the user has a stray `/tmp/Cargo.toml`).
    let manifest_path = project_dir.join("Cargo.toml");
    Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .current_dir(project_dir)
        .output()
        .expect("failed to run cargo check")
}

fn assert_actr_success(out: &Output, context: &str) {
    assert!(
        out.status.success(),
        "{context} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn init_rust_echo_app(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let out = run_actr(
        &[
            "init",
            "-l",
            "rust",
            "--template",
            "echo",
            "--role",
            "app",
            "--signaling",
            "wss://actrix1.develenv.com",
            "--manufacturer",
            "acme",
            name,
        ],
        parent,
    );
    assert_actr_success(&out, "actr init (app)");
    parent.join(name)
}

fn init_rust_echo_service(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let out = run_actr(
        &[
            "init",
            "-l",
            "rust",
            "--template",
            "echo",
            "--role",
            "service",
            "--signaling",
            "wss://actrix1.develenv.com",
            "--manufacturer",
            "acme",
            name,
        ],
        parent,
    );
    assert_actr_success(&out, "actr init (service)");
    parent.join(name)
}

fn init_rust_empty_service(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let out = run_actr(
        &[
            "init",
            "-l",
            "rust",
            "--template",
            "empty",
            "--role",
            "service",
            "--signaling",
            "wss://actrix1.develenv.com",
            "--manufacturer",
            "acme",
            name,
        ],
        parent,
    );
    assert_actr_success(&out, "actr init (empty service)");
    parent.join(name)
}

fn init_rust_echo_both(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let out = run_actr(
        &[
            "init",
            "-l",
            "rust",
            "--template",
            "echo",
            "--role",
            "both",
            "--signaling",
            "wss://actrix1.develenv.com",
            "--manufacturer",
            "example",
            name,
        ],
        parent,
    );
    assert_actr_success(&out, "actr init (both)");
    parent.join(name)
}

// ---------------------------------------------------------------------------
// App role: scaffold validation
// ---------------------------------------------------------------------------

/// Verify all generated files and their content for an app-role project.
#[test]
fn rust_echo_app_scaffold() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_app(tmp.path(), "my-echo-app");

    // -- files exist --
    for path in &[
        "Cargo.toml",
        "manifest.toml",
        "actr.toml",
        "src/main.rs",
        "src/lib.rs",
        "README.md",
        "protos/local/local.proto",
    ] {
        assert!(dir.join(path).exists(), "{path} should exist");
    }
    assert!(
        !dir.join(".protoc-plugin.toml").exists(),
        ".protoc-plugin.toml should not be generated"
    );

    // -- Cargo.toml --
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains(r#"name = "my-echo-app""#), "package name");
    assert!(cargo.contains(r#"edition = "2024""#), "edition");
    assert!(
        cargo.contains("actr-framework = "),
        "missing actr-framework dependency"
    );
    assert!(
        cargo.contains("actr-hyper = "),
        "missing actr-hyper dependency"
    );

    // -- manifest.toml --
    let actr = std::fs::read_to_string(dir.join("manifest.toml")).unwrap();
    assert!(
        actr.contains(r#"EchoService = { actr_type = "acme:EchoService:1.0.0" }"#),
        "app should point to the real EchoService actr_type"
    );
    assert!(
        actr.contains(r#"path = "dist/app.wasm""#),
        "app should define a guest binary path"
    );
    assert!(actr.contains("[build]"), "app should define build settings");

    // -- actr.toml --
    let runtime = std::fs::read_to_string(dir.join("actr.toml")).unwrap();
    assert!(
        runtime.contains("wss://actrix1.develenv.com/signaling/ws"),
        "runtime config should contain signaling URL"
    );
    assert!(
        runtime.contains(r#"path = "dist/app.actr""#),
        "runtime config should point to the local app package"
    );
    assert!(
        runtime.contains("visible = false"),
        "app runtime should not advertise itself for discovery"
    );

    // -- local.proto should contain the generated bridge service --
    let proto = std::fs::read_to_string(dir.join("protos/local/local.proto")).unwrap();
    assert!(
        proto.contains("service MyEchoAppClientApp {}"),
        "app local.proto must define the bridge service"
    );
    assert!(
        !dir.join("src/echo_app.rs").exists(),
        "app scaffold should no longer generate src/echo_app.rs"
    );

    // -- main.rs --
    let main = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(
        main.contains("ensure_package_built(Path::new(PACKAGE_PATH))"),
        "main.rs should build the local guest package on demand"
    );
    assert!(
        main.contains("Node::from_hyper(hyper, runtime.clone())"),
        "main.rs should build Node from the constructed Hyper handle"
    );
    assert!(
        main.contains(".attach(&package)"),
        "main.rs should attach the local guest package through Node"
    );
    assert!(
        main.contains(".register(&ais_endpoint)"),
        "main.rs should register with AIS"
    );
    assert!(
        main.contains("let response = actr_ref"),
        "main.rs should call EchoService through the local guest"
    );
    assert!(
        main.contains("30_000"),
        "main.rs should pass an explicit call timeout"
    );
    assert!(
        main.contains(r#"println!("Echo reply: {}""#),
        "main.rs should print the echo response"
    );

    // -- lib.rs --
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("pub mod generated;"),
        "app lib should include generated modules"
    );
    // The bridge proto has no local RPC routes, so the unified workload
    // registers the unit handler and forwards remote calls via dispatch.
    assert!(
        lib.contains("actr_dispatch::ActrWorkload"),
        "app lib should register the unified workload"
    );
    assert!(
        lib.contains("entry!(ActrWorkload<()>, ActrWorkload::new(()))"),
        "app lib should register the unit-handler unified workload"
    );
}

// ---------------------------------------------------------------------------
// Service role: scaffold validation
// ---------------------------------------------------------------------------

/// Verify all generated files and their content for a service-role project.
#[test]
fn rust_echo_service_scaffold() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_service(tmp.path(), "my-echo-svc");

    // -- files exist --
    for path in &[
        "Cargo.toml",
        "manifest.toml",
        "build.rs",
        "src/lib.rs",
        "src/echo_service.rs",
    ] {
        assert!(dir.join(path).exists(), "{path} should exist");
    }
    assert!(
        !dir.join("src/generated/mod.rs").exists(),
        "init should not preseed src/generated; it must come from actr gen"
    );

    // -- Cargo.toml --
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(
        cargo.contains("crate-type = [\"rlib\", \"cdylib\"]"),
        "service template should build as a workload library"
    );
    assert!(
        cargo.contains("actr-framework = "),
        "service template should depend on actr-framework directly"
    );

    // -- manifest.toml --
    let actr = std::fs::read_to_string(dir.join("manifest.toml")).unwrap();
    assert!(
        actr.contains(r#"exports = ["protos/local/echo.proto"]"#),
        "should export echo.proto"
    );
    assert!(
        actr.contains(r#"name = "EchoService""#),
        "actr_type.name should be EchoService"
    );
    assert!(
        actr.contains("[binary]") && actr.contains("[build]"),
        "service manifest should declare package build inputs"
    );

    // -- lib.rs --
    let main = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        main.contains("actr_handler::ActrHandler"),
        "service library should compose the generated ActrHandler adapter"
    );
    assert!(
        main.contains("ActrWorkload<ActrHandler<EchoServiceImpl>>"),
        "service library should wire the unified workload through ActrHandler"
    );
    assert!(
        main.contains("entry!("),
        "service library should expose the package entry point"
    );
    assert!(
        !dir.join("src/main.rs").exists(),
        "service package template should no longer generate a host main.rs"
    );

    // -- echo_service.rs --
    let svc = std::fs::read_to_string(dir.join("src/echo_service.rs")).unwrap();
    assert!(
        svc.contains("echo_actor::EchoServiceHandler"),
        "import EchoServiceHandler"
    );
    assert!(
        svc.contains("echo::"),
        "import message types from echo module"
    );
    assert!(
        svc.contains("_ctx: &C") || svc.contains("ctx: &C"),
        "handler ctx param"
    );

    // -- local echo.proto --
    let proto = std::fs::read_to_string(dir.join("protos/local/echo.proto")).unwrap();
    assert!(
        proto.contains("service EchoService"),
        "should define EchoService"
    );
    assert!(proto.contains("rpc Echo"), "should declare Echo rpc");

    append_workspace_patch(&dir);
    let check = cargo_check(&dir);
    assert_actr_success(&check, "cargo check (service init)");
}

#[test]
fn rust_echo_service_build_requires_gen_first() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_service(tmp.path(), "build-needs-gen");

    let out = run_actr(&["build"], &dir);
    assert!(
        !out.status.success(),
        "actr build should fail before actr gen"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Run `actr gen -l rust` before `actr build`."),
        "missing gen-first guidance, stderr:\n{stderr}"
    );
}

#[test]
fn rust_echo_both_app_uses_local_service_dependency() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_both(tmp.path(), "echo-pair");

    let app_actr = std::fs::read_to_string(dir.join("echo-app/manifest.toml")).unwrap();
    assert!(
        app_actr.contains(r#"EchoService = { actr_type = "example:EchoService:1.0.0" }"#),
        "role=both app should directly target the generated service actr_type, got:\n{app_actr}"
    );
    assert!(
        !app_actr.contains("EchoService = {}"),
        "role=both app should not use a placeholder dependency"
    );
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[test]
fn rust_echo_init_fails_if_directory_exists() {
    let tmp = TempDir::new().unwrap();
    init_rust_echo_app(tmp.path(), "duplicate-svc");

    let out = run_actr(
        &[
            "init",
            "-l",
            "rust",
            "--template",
            "echo",
            "--role",
            "app",
            "--signaling",
            "wss://actrix1.develenv.com",
            "duplicate-svc",
        ],
        tmp.path(),
    );
    assert!(!out.status.success(), "second init should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("already exists") || stderr.contains("exist"),
        "should mention existing directory, got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Rust codegen: bridge local_actor generation
// ---------------------------------------------------------------------------

const ECHO_PROTO: &str = r#"syntax = "proto3";
package echo;
message EchoRequest { string message = 1; }
message EchoResponse { string reply = 1; uint64 timestamp = 2; }
service EchoService {
  rpc Echo(EchoRequest) returns (EchoResponse);
}
"#;

const CHAT_PROTO: &str = r#"syntax = "proto3";
package chat;
message SendMessageRequest { string room_id = 1; string content = 2; }
message SendMessageResponse { string message_id = 1; }
message JoinRoomRequest { string room_id = 1; string user_id = 2; }
message JoinRoomResponse { bool success = 1; }
service ChatService {
  rpc SendMessage(SendMessageRequest) returns (SendMessageResponse);
  rpc JoinRoom(JoinRoomRequest) returns (JoinRoomResponse);
}
"#;

const EMPTY_SERVICE_PROTO: &str = r#"syntax = "proto3";
package empty_shell;
service EmptyShell {}
"#;

/// Helper: write a remote proto under protos/remote/<dependency-alias>/<stem>.proto.
fn write_remote_proto(dir: &std::path::Path, alias: &str, stem: &str, content: &str) {
    let proto_dir = dir.join("protos/remote").join(alias);
    std::fs::create_dir_all(&proto_dir).unwrap();
    std::fs::write(proto_dir.join(format!("{stem}.proto")), content).unwrap();
}

fn add_manifest_dependency(dir: &std::path::Path, alias: &str, actr_type: &str) {
    let path = dir.join("manifest.toml");
    let mut manifest = std::fs::read_to_string(&path).unwrap();
    let dependency = format!("{alias} = {{ actr_type = \"{actr_type}\" }}\n");
    let marker = "[dependencies]\n";
    let insert_at = manifest.find(marker).expect("manifest dependencies table") + marker.len();
    manifest.insert_str(insert_at, &dependency);
    std::fs::write(path, manifest).unwrap();
}

fn set_manifest_exports(dir: &std::path::Path, exports: &[&str]) {
    let path = dir.join("manifest.toml");
    let manifest = std::fs::read_to_string(&path).unwrap();
    let quoted = exports
        .iter()
        .map(|export| format!("\"{export}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let updated = manifest.replacen(
        "exports = [\"protos/local/local.proto\"]",
        &format!("exports = [{quoted}]"),
        1,
    );
    std::fs::write(path, updated).unwrap();
}

#[test]
fn rust_gen_empty_only_local_service_uses_unit_workload_and_checks() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_empty_service(tmp.path(), "empty-only-service");
    std::fs::write(dir.join("protos/local/local.proto"), EMPTY_SERVICE_PROTO).unwrap();

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (empty-only local service)");

    assert!(
        !dir.join("src/generated/actr_handler.rs").exists(),
        "empty-only local services have no local routes, so actr_handler.rs must not be generated"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("ActrWorkload<()>") && lib.contains("ActrWorkload::new(())"),
        "empty-only entry must use the unit handler:\n{lib}"
    );
    assert!(
        !lib.contains("actr_handler::ActrHandler") && !lib.contains("EmptyShellImpl"),
        "empty-only entry must not compose missing ActrHandler services:\n{lib}"
    );

    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("src/generated/actr-gen-meta.json")).unwrap(),
    )
    .unwrap();
    let local_service = &metadata["local_services"][0];
    assert!(local_service.get("workload_type").is_none());
    assert!(local_service.get("dispatcher_type").is_none());

    append_workspace_patch(&dir);
    let check = cargo_check(&dir);
    assert_actr_success(&check, "cargo check (empty-only local service)");
}

#[test]
fn rust_gen_empty_and_nonempty_local_services_compose_only_routed_service_and_check() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_empty_service(tmp.path(), "mixed-local-service");
    std::fs::remove_file(dir.join("protos/local/local.proto")).unwrap();
    std::fs::write(dir.join("protos/local/empty.proto"), EMPTY_SERVICE_PROTO).unwrap();
    std::fs::write(dir.join("protos/local/echo.proto"), ECHO_PROTO).unwrap();
    set_manifest_exports(
        &dir,
        &["protos/local/empty.proto", "protos/local/echo.proto"],
    );

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (empty + nonempty local services)");

    let adapter = std::fs::read_to_string(dir.join("src/generated/actr_handler.rs")).unwrap();
    assert!(
        adapter.contains("EchoServiceHandler") && !adapter.contains("EmptyShellHandler"),
        "adapter must include only services with local routes:\n{adapter}"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("ActrWorkload<ActrHandler<EchoServiceImpl>>"),
        "entry composes the nonempty service:\n{lib}"
    );
    assert!(
        !lib.contains("EmptyShellImpl"),
        "entry must not include the empty service as a handler generic or constructor arg:\n{lib}"
    );

    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("src/generated/actr-gen-meta.json")).unwrap(),
    )
    .unwrap();
    for service in metadata["local_services"].as_array().unwrap() {
        assert!(service.get("workload_type").is_none());
        assert!(service.get("dispatcher_type").is_none());
    }

    append_workspace_patch(&dir);
    let check = cargo_check(&dir);
    assert_actr_success(&check, "cargo check (empty + nonempty local services)");
}

/// Verify that `actr gen` emits a unified dispatcher for one remote service.
#[test]
fn rust_echo_app_gen_single_remote_service() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_app(tmp.path(), "single-svc-app");

    write_remote_proto(&dir, "EchoService", "echo", ECHO_PROTO);

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (single remote service)");

    let dispatch = std::fs::read_to_string(dir.join("src/generated/actr_dispatch.rs")).unwrap();

    assert!(
        dispatch.contains("pub trait ActrDispatchHandler"),
        "should generate the unified handler trait"
    );
    assert!(
        dispatch.contains("pub struct ActrWorkload"),
        "should generate the unified workload"
    );
    assert!(
        dispatch.contains("\"echo.EchoService.Echo\""),
        "should forward the Echo route key"
    );
    assert!(
        dispatch.contains("resolve_dependency(\"EchoService\")"),
        "should resolve the manifest dependency alias at runtime"
    );
    assert!(
        !dispatch.contains("ActrType {"),
        "should not embed a remote ActrType"
    );
    assert!(
        !dir.join("src/local_service.rs").exists(),
        "empty bridge proto should not generate local_service.rs"
    );
}

/// Verify that `actr gen` merges multiple remote services into the unified dispatcher.
#[test]
fn rust_echo_app_gen_two_remote_services() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_app(tmp.path(), "two-svc-app");

    add_manifest_dependency(&dir, "ChatService", "acme:ChatService:1.0.0");
    write_remote_proto(&dir, "EchoService", "echo", ECHO_PROTO);
    write_remote_proto(&dir, "ChatService", "chat", CHAT_PROTO);

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (two remote services)");

    let dispatch = std::fs::read_to_string(dir.join("src/generated/actr_dispatch.rs")).unwrap();

    assert!(
        dispatch.contains("\"echo.EchoService.Echo\""),
        "should forward EchoService"
    );
    assert!(
        dispatch.contains("\"chat.ChatService.SendMessage\""),
        "should forward ChatService.SendMessage"
    );
    assert!(
        dispatch.contains("\"chat.ChatService.JoinRoom\""),
        "should forward ChatService.JoinRoom"
    );
    assert!(
        dispatch.contains("resolve_dependency(\"EchoService\")"),
        "should resolve EchoService by alias"
    );
    assert!(
        dispatch.contains("resolve_dependency(\"ChatService\")"),
        "should resolve ChatService by alias"
    );
    assert!(
        !dispatch.contains("ActrType {"),
        "should not embed remote ActrTypes"
    );
    assert!(
        !dir.join("src/local_service.rs").exists(),
        "empty bridge proto should still skip local_service.rs"
    );
}

// ---------------------------------------------------------------------------
// Rust codegen: two local services sharing an RPC name
// ---------------------------------------------------------------------------

const ASK_PROTO: &str = r#"syntax = "proto3";
package ask;
message AskRequest { string question = 1; }
message AskResponse { string answer = 1; }
service AskService {
  rpc Ask(AskRequest) returns (AskResponse);
}
"#;

const CHAT_ASK_PROTO: &str = r#"syntax = "proto3";
package chat;
message ChatRequest { string question = 1; }
message ChatResponse { string answer = 1; }
service ChatService {
  rpc Ask(ChatRequest) returns (ChatResponse);
}
"#;

/// End-to-end: two local services with the SAME RPC name must generate
/// successfully for Rust. The unified dispatcher keeps unique internal
/// routes (`ask_service_ask` / `chat_service_ask`) while the business
/// handlers keep the protobuf-named `ask` methods.
#[test]
fn rust_gen_two_local_services_same_rpc_name() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_app(tmp.path(), "same-rpc-app");

    // Replace the empty bridge proto with two local services that share
    // the RPC name `Ask`.
    std::fs::write(dir.join("protos/local/ask.proto"), ASK_PROTO).unwrap();
    std::fs::write(dir.join("protos/local/chat.proto"), CHAT_ASK_PROTO).unwrap();
    std::fs::remove_file(dir.join("protos/local/local.proto")).unwrap();
    let manifest_path = dir.join("manifest.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "exports = []",
        "exports = [\"protos/local/ask.proto\", \"protos/local/chat.proto\"]",
    );
    std::fs::write(&manifest_path, manifest).unwrap();

    // The init template ships its own user-owned lib.rs (a unit-handler
    // bridge entry); `actr gen` must preserve user entries by design. Remove
    // it so this test exercises the generated entry composition path.
    std::fs::remove_file(dir.join("src/lib.rs")).unwrap();

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (two local services, same RPC name)");

    // The sealed dispatcher trait carries the prefixed internal routes.
    let dispatch = std::fs::read_to_string(dir.join("src/generated/actr_dispatch.rs")).unwrap();
    assert!(
        dispatch.contains("async fn ask_service_ask"),
        "dispatcher must expose the ask_service_ask route:\n{dispatch}"
    );
    assert!(
        dispatch.contains("async fn chat_service_ask"),
        "dispatcher must expose the chat_service_ask route:\n{dispatch}"
    );
    assert!(
        dispatch.contains("\"ask.AskService.Ask\"")
            && dispatch.contains("\"chat.ChatService.Ask\""),
        "dispatcher must route both proto route keys:\n{dispatch}"
    );

    // The generated adapter forwards each prefixed route to the
    // protobuf-named business method on the matching service field.
    let adapter = std::fs::read_to_string(dir.join("src/generated/actr_handler.rs")).unwrap();
    assert!(
        adapter.contains("self.ask_service.ask"),
        "adapter forwards AskService::ask:\n{adapter}"
    );
    assert!(
        adapter.contains("self.chat_service.ask"),
        "adapter forwards ChatService::ask:\n{adapter}"
    );

    // Per-service business scaffolds keep the same protobuf method name.
    let ask_svc = std::fs::read_to_string(dir.join("src/ask_service.rs")).unwrap();
    let chat_svc = std::fs::read_to_string(dir.join("src/chat_service.rs")).unwrap();
    assert!(
        ask_svc.contains("async fn ask<")
            && ask_svc.contains("impl AskServiceHandler for AskServiceImpl"),
        "AskService scaffold keeps the protobuf-named ask method:\n{ask_svc}"
    );
    assert!(
        chat_svc.contains("async fn ask<")
            && chat_svc.contains("impl ChatServiceHandler for ChatServiceImpl"),
        "ChatService scaffold keeps the protobuf-named ask method:\n{chat_svc}"
    );

    // The lib.rs entry composes both implementations positionally.
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("ActrWorkload<ActrHandler<AskServiceImpl, ChatServiceImpl>>"),
        "entry composes both service impls:\n{lib}"
    );
}
// ---------------------------------------------------------------------------
// Rust codegen: stale artifacts after losing local routes
// ---------------------------------------------------------------------------

/// End-to-end P1 repro: regenerate WITHOUT `--clean` after the workload
/// loses its local routes. The stale `actr_handler.rs` (and stale
/// `*_actor.rs`) from the previous run must be removed instead of being
/// re-exported by mod.rs against the regenerated marker-trait dispatcher.
#[test]
fn rust_gen_stale_adapter_removed_when_local_routes_disappear() {
    let tmp = TempDir::new().unwrap();
    let dir = init_rust_echo_app(tmp.path(), "stale-adapter-app");

    // Run 1: two local services (shared RPC name) → adapter generated.
    std::fs::write(dir.join("protos/local/ask.proto"), ASK_PROTO).unwrap();
    std::fs::write(dir.join("protos/local/chat.proto"), CHAT_ASK_PROTO).unwrap();
    std::fs::remove_file(dir.join("protos/local/local.proto")).unwrap();
    let manifest_path = dir.join("manifest.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "exports = []",
        "exports = [\"protos/local/ask.proto\", \"protos/local/chat.proto\"]",
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    std::fs::remove_file(dir.join("src/lib.rs")).unwrap();

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (local services)");
    assert!(dir.join("src/generated/actr_handler.rs").exists());

    // Run 2: the workload becomes remote-only — no `--clean`.
    std::fs::remove_file(dir.join("protos/local/ask.proto")).unwrap();
    std::fs::remove_file(dir.join("protos/local/chat.proto")).unwrap();
    std::fs::create_dir_all(dir.join("protos/remote/svc")).unwrap();
    std::fs::write(dir.join("protos/remote/svc/ask.proto"), ASK_PROTO).unwrap();
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace(
            "exports = [\"protos/local/ask.proto\", \"protos/local/chat.proto\"]",
            "exports = []",
        )
        .replace(
            "[dependencies]",
            "[dependencies]\nsvc = { actr_type = \"acme:AskService:1.0.0\" }",
        );
    std::fs::write(&manifest_path, manifest).unwrap();
    // Drop user-owned files so run 2 recreates the remote-only entry.
    let _ = std::fs::remove_file(dir.join("src/lib.rs"));
    let _ = std::fs::remove_file(dir.join("src/ask_service.rs"));
    let _ = std::fs::remove_file(dir.join("src/chat_service.rs"));

    let out = run_actr(&["gen", "-l", "rust"], &dir);
    assert_actr_success(&out, "actr gen (remote-only, no --clean)");

    // Stale tool-owned artifacts are gone...
    assert!(
        !dir.join("src/generated/actr_handler.rs").exists(),
        "stale actr_handler.rs must be removed when no local routes remain"
    );
    assert!(
        !dir.join("src/generated/chat_actor.rs").exists(),
        "stale chat_actor.rs must be removed with its proto"
    );
    // ...and mod.rs no longer exports them.
    let mod_rs = std::fs::read_to_string(dir.join("src/generated/mod.rs")).unwrap();
    assert!(
        !mod_rs.contains("actr_handler"),
        "mod.rs must drop the adapter:\n{mod_rs}"
    );
    assert!(
        !mod_rs.contains("chat_actor"),
        "mod.rs must drop stale actor modules:\n{mod_rs}"
    );

    // The dispatcher is the remote-only marker-trait shape with the unit
    // handler, so the generated tree compiles without the adapter.
    let dispatch = std::fs::read_to_string(dir.join("src/generated/actr_dispatch.rs")).unwrap();
    assert!(dispatch.contains("impl ActrDispatchHandler for ()"));
    assert!(dispatch.contains("resolve_dependency"));

    // The regenerated entry uses the unit handler (the scaffold template
    // renders the entry! invocation across multiple lines).
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("ActrWorkload<()>") && lib.contains("ActrWorkload::new(())"),
        "regenerated entry should use the unit handler, got:\n{lib}"
    );
}
