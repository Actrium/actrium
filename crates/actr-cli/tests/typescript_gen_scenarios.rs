//! Integration tests for TypeScript code generation scenarios.
//!
//! These tests verify the file structure and content of generated TypeScript code
//! for three main scenarios:
//! 1. Local service only
//! 2. Remote service only
//! 3. Both local and remote services
//!
//! Run with:
//! `cargo test --test typescript_gen_scenarios -- --test-threads=1`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use tempfile::TempDir;

fn actr_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_actr"))
}

fn framework_codegen_typescript_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|candidate| candidate.join("tools/actr/protoc-gen/typescript").is_dir())
        .expect("actr-cli should live under the workspace root")
        .join("tools/actr/protoc-gen/typescript")
}

fn prepare_typescript_codegen_tools() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = framework_codegen_typescript_dir();

        // Ensure we are in the right directory
        assert!(
            dir.exists(),
            "tools/actr/protoc-gen/typescript dir not found at {}",
            dir.display()
        );

        // nextest runs each test in its own process, so the OnceLock does not
        // stop sibling test processes from running npm install concurrently
        // into the same node_modules (observed in CI as ENOTEMPTY rename
        // races inside node_modules). Serialize across processes with an
        // exclusive lock file plus a done marker: the winner runs
        // install/bundle, the losers wait for the marker. A lock left behind
        // by a killed run is reclaimed after a staleness deadline.
        let lock = dir.join(".npm-prepare.lock");
        let done = dir.join(".npm-prepare.done");
        let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
        let mut installed = done.exists();
        while !installed {
            let acquired = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
                .is_ok();
            if acquired {
                // Re-check: a winner may have finished while we raced the lock.
                installed = done.exists();
                if !installed {
                    run_npm_prepare(&dir);
                    fs::write(&done, b"ok").expect("write npm-prepare done marker");
                }
                let _ = fs::remove_file(&lock);
                break;
            }
            // Somebody else is preparing; the marker is all we need.
            if done.exists() {
                break;
            }
            // Reclaim a stale lock (holder died mid-install).
            if let Ok(metadata) = fs::metadata(&lock)
                && let Ok(modified) = metadata.modified()
                && modified.elapsed().unwrap_or_default() > std::time::Duration::from_secs(10 * 60)
            {
                let _ = fs::remove_file(&lock);
                continue;
            }
            if std::time::Instant::now() > wait_deadline {
                panic!(
                    "timed out waiting for a sibling test process to finish npm prepare in {}",
                    dir.display()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        dir
    })
}

fn run_npm_prepare(dir: &Path) {
    println!("Running npm install in {}...", dir.display());
    let npm_install = Command::new("npm")
        .args(["install"])
        .current_dir(dir)
        .output()
        .expect("failed to run npm install");
    if !npm_install.status.success() {
        panic!(
            "npm install failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&npm_install.stdout),
            String::from_utf8_lossy(&npm_install.stderr)
        );
    }

    println!("Running npm run bundle in {}...", dir.display());
    let npm_bundle = Command::new("npm")
        .args(["run", "bundle"])
        .current_dir(dir)
        .output()
        .expect("failed to run npm run bundle");
    if !npm_bundle.status.success() {
        panic!(
            "npm bundle failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&npm_bundle.stdout),
            String::from_utf8_lossy(&npm_bundle.stderr)
        );
    }
}

fn run_actr(args: &[&str], cwd: &Path) -> Output {
    let tool_dir = prepare_typescript_codegen_tools();

    // Construct PATH to include the local plugin and its dependencies
    let mut path_entries = vec![tool_dir.join("scripts"), tool_dir.join("node_modules/.bin")];
    if let Some(existing) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(path_entries).expect("failed to construct PATH");

    Command::new(actr_bin())
        .args(args)
        .current_dir(cwd)
        .env("PATH", path)
        .output()
        .expect("failed to run actr binary")
}

fn assert_success(out: &Output, context: &str) {
    if !out.status.success() {
        panic!(
            "{context} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// Project construction helpers

fn write_ts_project_files(root: &Path) {
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "test-gen",
  "dependencies": {
    "@actrium/actr": "*"
  }
}"#,
    )
    .unwrap();

    fs::write(
        root.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true,
    "skipLibCheck": true
  }
}"#,
    )
    .unwrap();
}

fn write_actr_toml_local_only(root: &Path) {
    fs::write(
        root.join("manifest.toml"),
        r#"edition = 1
[package]
name = "LocalService"
publisher = "acme"
version="0.0.1"
[dependencies]

[system.signaling]
url = "wss://localhost:8080"

[system.issuer_endpoint]
url = "https://localhost:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
}

fn write_actr_toml_with_remote(root: &Path) {
    fs::write(
        root.join("manifest.toml"),
        r#"edition = 1
[package]
name = "RemoteApp"
publisher = "acme"
version="0.0.1"
[dependencies]
echo-service = { actr_type = "acme/EchoService@0.0.1" }

[system.signaling]
url = "wss://localhost:8080"

[system.issuer_endpoint]
url = "https://localhost:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
}

fn write_actr_toml_both(root: &Path) {
    fs::write(
        root.join("manifest.toml"),
        r#"edition = 1
[package]
name = "BothService"
publisher = "acme"
version = "0.0.1"
[dependencies]
echo-service = { actr_type = "acme/EchoService@0.0.1" }

[system.signaling]
url = "wss://localhost:8080"

[system.issuer_endpoint]
url = "https://localhost:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
}

fn write_actr_toml_both_with_two_remotes(root: &Path) {
    fs::write(
        root.join("manifest.toml"),
        r#"edition = 1
[package]
name = "BothTwoRemotesService"
publisher = "acme"
version="0.0.1"
[dependencies]
echo-service = { actr_type = "acme/EchoService@0.0.1" }
profile-service = { actr_type = "acme/ProfileService@0.0.1" }

[system.signaling]
url = "wss://localhost:8080"

[system.issuer_endpoint]
url = "https://localhost:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
}

fn write_local_greeter_proto(root: &Path) {
    let proto_dir = root.join("protos");
    fs::create_dir_all(&proto_dir).unwrap();
    fs::write(
        proto_dir.join("greeter.proto"),
        r#"syntax = "proto3";
package greeter;

message HelloRequest {
  string name = 1;
}

message HelloResponse {
  string message = 1;
}

service Greeter {
  rpc SayHello(HelloRequest) returns (HelloResponse);
}
"#,
    )
    .unwrap();
}

fn write_remote_echo_proto(root: &Path) {
    let remote_proto_dir = root.join("protos/remote/echo-service");
    fs::create_dir_all(&remote_proto_dir).unwrap();
    fs::write(
        remote_proto_dir.join("echo.proto"),
        r#"syntax = "proto3";
package echo;

message EchoRequest {
  string message = 1;
}

message EchoResponse {
  string message = 1;
}

service EchoService {
  rpc Echo(EchoRequest) returns (EchoResponse);
}
"#,
    )
    .unwrap();
}

fn write_remote_profile_proto(root: &Path) {
    let remote_proto_dir = root.join("protos/remote/profile-service");
    fs::create_dir_all(&remote_proto_dir).unwrap();
    fs::write(
        remote_proto_dir.join("profile.proto"),
        r#"syntax = "proto3";
package profile;

message GetProfileRequest {
  string user_id = 1;
}

message GetProfileResponse {
  string nickname = 1;
}

service ProfileService {
  rpc GetProfile(GetProfileRequest) returns (GetProfileResponse);
}
"#,
    )
    .unwrap();
}

#[test]
fn test_local_service_only() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    write_ts_project_files(root);
    write_actr_toml_local_only(root);
    write_local_greeter_proto(root);

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "actr gen -l typescript (local only)");

    let gen_dir = root.join("src/generated");
    assert!(gen_dir.join("greeter_pb.ts").exists());
    assert!(gen_dir.join("actr_dispatch.ts").exists());
    assert!(!gen_dir.join("greeter_workload.ts").exists());
    assert!(!gen_dir.join("greeter_client.ts").exists());
    assert!(
        !gen_dir.join("local_actor.ts").exists(),
        "TypeScript local_actor.ts dispatcher should not be generated"
    );

    assert!(
        !gen_dir.join("local").exists(),
        "local/ directory should have been flattened"
    );
    assert!(
        !gen_dir.join("remote").exists(),
        "remote/ directory should not exist"
    );

    let dispatcher_content = fs::read_to_string(gen_dir.join("actr_dispatch.ts")).unwrap();
    assert!(dispatcher_content.contains("export interface ActrDispatchHandler"));
    assert!(dispatcher_content.contains(
        "greeterSayHello(req: HelloRequest, ctx: ActrContext): HelloResponse | Promise<HelloResponse>;"
    ));
    assert!(dispatcher_content.contains("case \"greeter.Greeter.SayHello\":"));
    assert!(dispatcher_content.contains("fromBinary(HelloRequestSchema, payload)"));
    assert!(dispatcher_content.contains("await ctx.replyRaw("));
    assert!(dispatcher_content.contains("return new Uint8Array();"));
    assert!(!dispatcher_content.contains("GreeterDispatcher"));

    let actr_service_content = fs::read_to_string(root.join("src/actr_service.ts")).unwrap();
    assert!(
        actr_service_content
            .contains("import { ActrContext, defineWorkload } from '@actrium/actr-workload';")
    );
    assert!(actr_service_content.contains("class ActrHandler {"));
    assert!(
        actr_service_content
            .contains("const { actrDispatch } = await import('./generated/actr_dispatch.js');")
    );
    assert!(actr_service_content.contains("const handler = new ActrHandler();"));
    assert!(actr_service_content.contains("export default defineWorkload({"));
    assert!(actr_service_content.contains("return actrDispatch(actrCtx, handler, envelope.routeKey ?? envelope.method, envelope.payload ?? new Uint8Array(), envelope.timeoutMs);"));
    assert!(!actr_service_content.contains("Local RPC methods:', 1"));
    assert!(actr_service_content.contains("Remote RPC methods:', 0"));
    assert!(!actr_service_content.contains("Received workload RPC:', envelope.method"));
    assert!(
        !actr_service_content.contains("// - Greeter.SayHello (HelloRequest -> HelloResponse)")
    );
    assert!(!actr_service_content.contains("from './generated/local_actor'"));
    assert!(!actr_service_content.contains("dispatchLocalActor"));
}

#[test]
fn test_remote_service_only() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    write_ts_project_files(root);
    write_actr_toml_with_remote(root);
    write_remote_echo_proto(root);

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "actr gen -l typescript (remote only)");

    let gen_dir = root.join("src/generated");
    // Remote files are lifted: remote/echo-service/echo.proto -> src/generated/echo-service/echo_pb.ts
    assert!(gen_dir.join("echo-service/echo_pb.ts").exists());
    assert!(gen_dir.join("echo-service/echo_client.ts").exists());
    assert!(!gen_dir.join("echo_workload.ts").exists());
    assert!(
        !gen_dir.join("local_actor.ts").exists(),
        "TypeScript remote-only generation should emit client stubs only"
    );

    // Verify directory lifting
    assert!(
        !gen_dir.join("remote").exists(),
        "remote/ directory should have been lifted"
    );

    let client_content = fs::read_to_string(gen_dir.join("echo-service/echo_client.ts")).unwrap();
    assert!(client_content.contains("export const EchoRequest = {"));
    assert!(client_content.contains("routeKey: \"echo.EchoService.Echo\""));
    // payload_type now lives in the unified DispatchSpec (actr_dispatch.ts),
    // not the per-message companion — no companion field to assert here.
    assert!(client_content.contains("response: {"));

    let actr_service_content = fs::read_to_string(root.join("src/actr_service.ts")).unwrap();
    assert!(actr_service_content.contains("defineWorkload"));
    assert!(
        actr_service_content
            .contains("const { actrDispatch } = await import('./generated/actr_dispatch.js');")
    );
    assert!(!actr_service_content.contains("class ActrHandler {"));
    assert!(actr_service_content.contains("Remote RPC methods:', 1"));
    assert!(actr_service_content.contains("Remote RPC quick-start examples"));
    assert!(actr_service_content.contains("EchoRequest.routeKey"));
    assert!(actr_service_content.contains("EchoRequest.encode"));
    assert!(actr_service_content.contains("EchoRequest.response.decode"));
    assert!(!actr_service_content.contains("HelloRequest.decode"));
}

#[test]
fn test_local_and_remote_services() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    write_ts_project_files(root);
    write_actr_toml_both(root);
    write_local_greeter_proto(root);
    write_remote_echo_proto(root);

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "actr gen -l typescript (both)");

    let gen_dir = root.join("src/generated");

    // Local files
    assert!(gen_dir.join("greeter_pb.ts").exists());
    assert!(gen_dir.join("actr_dispatch.ts").exists());
    assert!(!gen_dir.join("greeter_workload.ts").exists());
    assert!(!gen_dir.join("greeter_client.ts").exists());

    // Remote files
    assert!(gen_dir.join("echo-service/echo_pb.ts").exists());
    assert!(gen_dir.join("echo-service/echo_client.ts").exists());

    assert!(
        !gen_dir.join("local_actor.ts").exists(),
        "TypeScript generation should not emit a local dispatcher"
    );

    let client_content = fs::read_to_string(gen_dir.join("echo-service/echo_client.ts")).unwrap();
    assert!(client_content.contains("export const EchoRequest = {"));
    assert!(client_content.contains("routeKey: \"echo.EchoService.Echo\""));
    // payload_type now lives in the unified DispatchSpec (actr_dispatch.ts),
    // not the per-message companion — no companion field to assert here.

    let actr_service_content = fs::read_to_string(root.join("src/actr_service.ts")).unwrap();
    assert!(actr_service_content.contains("export default defineWorkload({"));
    assert!(
        actr_service_content
            .contains("const { actrDispatch } = await import('./generated/actr_dispatch.js');")
    );
    assert!(actr_service_content.contains("class ActrHandler {"));
    assert!(actr_service_content.contains("const handler = new ActrHandler();"));
    assert!(actr_service_content.contains("return actrDispatch(actrCtx, handler, envelope.routeKey ?? envelope.method, envelope.payload ?? new Uint8Array(), envelope.timeoutMs);"));
    assert!(!actr_service_content.contains("Local RPC methods:', 1"));
    assert!(
        !actr_service_content.contains("// - Greeter.SayHello (HelloRequest -> HelloResponse)")
    );
    assert!(actr_service_content.contains("from './generated/echo-service/echo_client';"));
    assert!(actr_service_content.contains("EchoRequest.routeKey"));
    assert!(!actr_service_content.contains("from './generated/local_actor'"));
}

#[test]
fn test_remote_clients_are_generated_without_local_dispatcher() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    write_ts_project_files(root);
    write_actr_toml_both_with_two_remotes(root);
    write_local_greeter_proto(root);
    write_remote_echo_proto(root);
    write_remote_profile_proto(root);

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "actr gen -l typescript (local + two remotes)");

    let gen_dir = root.join("src/generated");
    assert!(gen_dir.join("actr_dispatch.ts").exists());
    assert!(!gen_dir.join("greeter_workload.ts").exists());
    assert!(
        !gen_dir.join("local_actor.ts").exists(),
        "TypeScript remote client generation should not emit local_actor.ts"
    );

    let echo_client = fs::read_to_string(gen_dir.join("echo-service/echo_client.ts")).unwrap();
    let profile_client =
        fs::read_to_string(gen_dir.join("profile-service/profile_client.ts")).unwrap();
    assert!(echo_client.contains("routeKey: \"echo.EchoService.Echo\""));
    assert!(profile_client.contains("routeKey: \"profile.ProfileService.GetProfile\""));

    let actr_service_content = fs::read_to_string(root.join("src/actr_service.ts")).unwrap();
    assert!(actr_service_content.contains("Remote RPC methods:', 2"));
    assert!(actr_service_content.contains("EchoRequest.routeKey"));
    assert!(actr_service_content.contains("GetProfileRequest.routeKey"));
}

#[test]
fn test_regeneration_removes_stale_legacy_workload_modules() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    write_ts_project_files(root);
    write_actr_toml_local_only(root);
    write_local_greeter_proto(root);

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "initial actr gen -l typescript");

    let stale_workload = root.join("src/generated/greeter_workload.ts");
    fs::write(
        &stale_workload,
        "// DO NOT EDIT.\n// Generated by actr gen -l typescript.\n",
    )
    .unwrap();

    let out = run_actr(&["gen", "-l", "typescript"], root);
    assert_success(&out, "actr gen -l typescript after centralized migration");
    assert!(
        !stale_workload.exists(),
        "regeneration must remove legacy per-service workload artifacts"
    );
}
