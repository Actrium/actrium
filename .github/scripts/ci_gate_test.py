#!/usr/bin/env python3

import sqlite3
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CI_GATE_WORKFLOW = ROOT / ".github/workflows/ci-gate.yml"
CI_E2E_WORKFLOW = ROOT / ".github/workflows/ci-e2e.yml"
CI_CSHARP_WORKFLOW = ROOT / ".github/workflows/ci-csharp.yml"
CI_CSHARP_E2E_WORKFLOW = ROOT / ".github/workflows/ci-csharp-e2e.yml"
CI_TYPESCRIPT_WORKFLOW = ROOT / ".github/workflows/ci-typescript.yml"
RELEASE_TRAIN_WORKFLOW = ROOT / ".github/workflows/release-train.yml"
RELEASE_TRAIN_VERIFY_WORKFLOW = ROOT / ".github/workflows/release-train-verify.yml"
RELEASE_TRAIN_SCRIPT = ROOT / "scripts/actr/release-train.sh"
PUBLISH_ACTRSHARP_WORKFLOW = ROOT / ".github/workflows/publish-actrsharp.yml"
GO_WORKLOAD_BUILD = ROOT / "examples/actr/go/echo-workload/build.sh"
CLI_TEST_SUPPORT = ROOT / "crates/actr-cli/src/test_support.rs"
SWIFT_E2E_READINESS = ROOT / "tests/e2e/actr/swift-echo-app/lib/readiness.sh"
TS_COMPONENTIZE_GUARD = ROOT / ".github/scripts/verify-typescript-componentize-blocker.sh"


def _job(workflow: str, name: str, next_name: str) -> str:
    job_start = workflow.index(f"  {name}:\n")
    next_job_start = workflow.index(f"\n  {next_name}:\n", job_start)
    return workflow[job_start:next_job_start]


def test_gate_aggregates_webrtc_v020_domain() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    # `gate` is the workflow's final job, so slice from its key to EOF.
    gate = workflow[workflow.index("  gate:\n"):]
    # The v0.20 domain must stay in the gate's needs list and in the Python
    # summary dict; dropping either would detach the v0.20 migration from the
    # gating decision without failing any other structure pin.
    assert "webrtc_v020" in gate
    assert "WEBRTC_V020_RESULT" in gate


def test_gate_enforces_anonymous_commit_identities() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    identity_job = _job(workflow, "identity", "determine")
    gate = workflow[workflow.index("  gate:\n") :]

    assert "fetch-depth: 0" in identity_job
    assert "python .github/scripts/check_commit_identities_test.py" in identity_job
    assert "python .github/scripts/check_commit_identities.py" in identity_job
    assert "github.event.pull_request.base.sha || github.event.before" in identity_job
    assert "--base-sha \"$BASE_SHA\"" in identity_job
    assert "--head-sha \"$HEAD_SHA\"" in identity_job
    assert "      - identity\n" in gate
    assert "IDENTITY_RESULT: ${{ needs.identity.result }}" in gate
    assert '"identity": os.environ["IDENTITY_RESULT"]' in gate


def test_rust_gate_avoids_slow_workspace_tests_and_unused_prewarm() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    rust_job = _job(workflow, "rust", "test")

    assert "- name: Run tests" not in rust_job
    assert "cargo test --workspace" not in rust_job
    assert "- name: Prepare Rust codegen plugins" not in rust_job
    assert "cargo install protoc-gen-prost" not in rust_job
    assert "cargo build --quiet -p actr-framework-protoc-codegen" not in rust_job
    assert "- name: Build workspace" not in rust_job
    assert "cargo build --verbose --all-features" not in rust_job
    assert "- name: Build release" not in rust_job
    assert "cargo build --release --verbose --all-features" not in rust_job

    # The separate test job (not inside rust) runs the Rust suite via
    # cargo-nextest (integration retries live in .config/nextest.toml), plus
    # doctests on the libtest harness (nextest skips them). The job shards
    # into realtime (real-PC suites) and rest legs; both run nextest and the
    # rest leg keeps the doctests.
    assert "  test:" in workflow
    test_job = _job(workflow, "test", "rust_msrv")
    assert "shard: [realtime, rest]" in test_job
    assert "- name: Run tests (nextest, ${{ matrix.shard }} shard)" in test_job
    assert "cargo nextest run --workspace" in test_job
    assert "cargo test --doc --workspace" in test_job
    assert 'if: matrix.shard == \'rest\'' in test_job


def test_rust_test_gate_restores_cache_before_installing_tools() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    test_job = _job(workflow, "test", "rust_msrv")

    cache_step = "- uses: Swatinem/rust-cache@v2"
    install_step = "- name: Install prebuilt WebAssembly tools"
    assert test_job.index(cache_step) < test_job.index(install_step)
    assert "cache-targets: false" in test_job


def test_msrv_gate_builds_full_workspace_on_pinned_toolchain() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    msrv_job = _job(workflow, "rust_msrv", "typescript")

    assert "dtolnay/rust-toolchain@1.95.0" in msrv_job
    assert "targets: wasm32-wasip2,wasm32-unknown-unknown" in msrv_job
    assert 'ACTR_REQUIRE_WASM_FIXTURE: "1"' in msrv_job
    assert (
        "cargo build --workspace --all-targets --all-features --locked" in msrv_job
    )


def test_pr_gate_excludes_heavy_root_e2e_jobs() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")

    assert "  ts_stream_e2e:\n" not in workflow
    assert "  web_browser_e2e:\n" not in workflow
    assert "ts_stream_e2e" not in workflow
    assert "web_browser_e2e" not in workflow


def test_pr_gate_compiles_rust_e2e_workspace_with_committed_lockfile() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    # The compile job ends where the targeted scenario job begins.
    e2e_job = _job(workflow, "e2e_rust", "e2e_package_runtime")

    assert "needs.determine.outputs.e2e_rust == 'true'" in e2e_job
    normalized = " ".join(e2e_job.split())
    assert "cargo metadata" in normalized
    assert "--manifest-path tests/e2e/actr/Cargo.toml" in normalized
    assert "--format-version 1" in normalized
    assert "--locked" in normalized
    # e2e_rust runs on the self-hosted image which preinstalls protoc, so
    # no arduino/setup-protoc step is expected here.
    assert "libopus-dev" in e2e_job
    assert "cargo check" in e2e_job
    assert "--workspace" in normalized
    # The compile job must stay a compile job; the scenario lives in the
    # dedicated targeted job below.
    assert "package-runtime-echo/run.sh" not in e2e_job


def test_pr_gate_runs_package_runtime_scenario_when_targeted() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    package_job = _job(workflow, "e2e_package_runtime", "audit")

    # The scenario runs on PRs only when its directory changed (#618), and
    # the gate aggregates its result like every other domain.
    assert "needs.determine.outputs.e2e_package_runtime == 'true'" in package_job
    assert "bash tests/e2e/actr/package-runtime-echo/run.sh" in package_job
    assert "sync-cli-assets.sh" in package_job

    gate = workflow[workflow.index("  gate:\n"):]
    assert "e2e_package_runtime" in gate
    assert "E2E_PACKAGE_RUNTIME_RESULT" in gate


def test_native_workload_gate_builds_c_and_go_with_pinned_tools() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    native_job = _job(workflow, "c_go_workloads", "gate")

    for version in (
        "wit-bindgen-0.59.0-x86_64-linux.tar.gz",
        "wasm-tools-1.253.0-x86_64-linux.tar.gz",
        "wasi-sdk-24.0-x86_64-linux.tar.gz",
        "wasi_snapshot_preview1.reactor.wasm",
    ):
        assert version in native_job

    assert "sha256sum --check --status" in native_job
    assert "make -C examples/actr/c/echo-workload verify" in native_job
    assert "bash examples/actr/go/echo-workload/build.sh" in native_job
    assert "native_workload_smoke" in native_job
    assert "ACTR_C_ECHO_COMPONENT" in native_job
    assert "ACTR_GO_ECHO_COMPONENT" in native_job
    assert "C_GO_WORKLOADS_RESULT" in workflow
    assert '"c_go_workloads": os.environ["C_GO_WORKLOADS_RESULT"]' in workflow


def test_go_workload_build_uses_only_the_hash_pinned_fork() -> None:
    script = GO_WORKLOAD_BUILD.read_text(encoding="utf-8")

    assert "go1.25.5-wasi-on-idle" in script
    assert (
        "63a9efff8601bdf9d3577b1d9d2ddd9b4d7a45dc09d78b73ebf03873e8c23766"
        in script
    )
    assert 'GO_BIN="${GO_ROOT}/bin/go"' in script
    assert 'ACTR_GO="${ACTR_GO:-go}"' not in script
    assert 'echo "ACTR_GO=' not in CI_GATE_WORKFLOW.read_text(encoding="utf-8")


def test_blocked_typescript_component_e2e_jobs_share_disable_gate() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")

    typescript_job = _job(workflow, "typescript-e2e", "web-browser-e2e")
    web_browser_job = _job(workflow, "web-browser-e2e", "swift-echo-app-e2e")
    swift_ts_job = _job(workflow, "swift-ts-workload-e2e", "kotlin-echo-app-e2e")

    for job in (typescript_job, web_browser_job, swift_ts_job):
        assert "if: ${{ false }}" in job
        assert "#431" in job

    assert "bash tests/e2e/actr/typescript-stream/run.sh" in typescript_job
    assert "bash tests/e2e/actr/web-browser/run.sh" in web_browser_job


def test_typescript_componentize_only_allows_known_upstream_failure() -> None:
    gate_workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    gate_job = _job(gate_workflow, "typescript", "python_codegen")
    typescript_workflow = CI_TYPESCRIPT_WORKFLOW.read_text(encoding="utf-8")
    guard = TS_COMPONENTIZE_GUARD.read_text(encoding="utf-8")

    for workflow in (gate_job, typescript_workflow):
        assert "continue-on-error: true" not in workflow
        assert "bash .github/scripts/verify-typescript-componentize-blocker.sh" in workflow

    assert 'grep -Fq "spidermonkey-embedding-splicer"' in guard
    assert 'grep -Fq "not yet implemented"' in guard
    assert "tracked by #427" in guard
    assert 'exit "${componentize_status}"' in guard


def test_python_workload_gate_builds_generated_cli_scaffold() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    python_job = _job(workflow, "python_workload", "swift")

    assert "- name: Build generated Python scaffold" in python_job
    assert "mv workload.py workload.py.handwritten" in python_job
    assert "./build.sh" in python_job


def test_python_template_keeps_project_and_abi_versions_distinct() -> None:
    manifest = (
        ROOT / "crates/actr-cli/fixtures/python/echo/manifest.toml.jinja2"
    ).read_text(encoding="utf-8")
    build_script = (
        ROOT / "crates/actr-cli/fixtures/python/echo/build.sh.jinja2"
    ).read_text(encoding="utf-8")

    assert 'version = "0.1.0"' in manifest
    assert "{{PROJECT_NAME_SNAKE}}-0.1.0-wasm32-wasip2.wasm" in manifest
    assert "{{PROJECT_NAME_SNAKE}}-0.1.0-wasm32-wasip2.wasm" in build_script
    assert 'WORLD="actr-workload-guest-v2"' in build_script


def test_pr_gate_swift_uses_macos_only_xcframework() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    swift_job = _job(workflow, "swift", "kotlin")

    assert "ACTR_XCFRAMEWORK_TARGETS: macos" in swift_job
    assert "targets: aarch64-apple-darwin" in swift_job
    assert "targets: aarch64-apple-ios,aarch64-apple-ios-sim,aarch64-apple-darwin" not in swift_job


def test_release_train_has_valid_publish_steps() -> None:
    raw_workflow = RELEASE_TRAIN_WORKFLOW.read_bytes()
    assert all(byte >= 0x20 or byte in b"\n\r\t" for byte in raw_workflow)

    workflow = raw_workflow.decode("utf-8")
    for stage in (
        "publish-rust",
        "publish-python",
        "publish-swift",
        "publish-kotlin",
        "publish-web",
        "publish-typescript-workload",
        "publish-typescript",
    ):
        assert f"- name: Run {stage} stage" in workflow


def test_release_train_verifies_ci_gate_triggered() -> None:
    workflow = RELEASE_TRAIN_WORKFLOW.read_text(encoding="utf-8")
    gate_job = _job(workflow, "gate", "context")

    assert "actions: read" in workflow
    assert "- name: Verify CI Gate is triggered" in gate_job
    assert "actions/workflows/ci-gate.yml/runs" in gate_job
    assert "head_sha=${RELEASE_SHA}" in gate_job
    assert "we do NOT block on" in gate_job


def test_release_train_reuses_actrsharp_workflow() -> None:
    workflow = RELEASE_TRAIN_WORKFLOW.read_text(encoding="utf-8")
    publish_workflow = PUBLISH_ACTRSHARP_WORKFLOW.read_text(encoding="utf-8")
    verify_workflow = RELEASE_TRAIN_VERIFY_WORKFLOW.read_text(encoding="utf-8")
    publish_job = _job(workflow, "publish-actrsharp", "publish-cli")
    report_job = _job(workflow, "collect-report", "notify-wechat")

    assert "workflow_call:" in publish_workflow
    assert "workflow_dispatch:" not in publish_workflow
    assert '\n  push:\n' not in publish_workflow
    for input_name in ("version", "source_ref", "release_sha", "publish"):
        assert f"      {input_name}:\n" in publish_workflow

    assert "- create-release" in publish_job
    assert "packages: write" in publish_job
    assert "uses: ./.github/workflows/publish-actrsharp.yml" in publish_job
    assert "release_sha: ${{ needs.context.outputs.release_sha }}" in publish_job
    assert "publish: ${{ !fromJSON(needs.context.outputs.dry_run) }}" in publish_job
    assert "- publish-actrsharp" in report_job

    assert "  verify-actrsharp:\n" in verify_workflow
    assert "uses: ./.github/workflows/publish-actrsharp.yml" in verify_workflow
    assert "publish: false" in verify_workflow
    assert "publish-actrsharp.state.tsv" in publish_workflow
    assert "local-name()='license'" in publish_workflow
    assert 'GetAttribute("type") -ne "expression"' in publish_workflow
    assert "<licenseExpression>" not in publish_workflow
    release_script = RELEASE_TRAIN_SCRIPT.read_text(encoding="utf-8")
    assert '"publish-actrsharp"' in release_script
    assert release_script.count("packages/actr-cabi/csharp/ActrSharp/ActrSharp.csproj") >= 2


def test_csharp_pr_gate_uses_light_fixture_and_nightly_keeps_full_matrix() -> None:
    gate_workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    csharp_job = _job(gate_workflow, "csharp", "gate")
    workflow = CI_CSHARP_WORKFLOW.read_text(encoding="utf-8")
    nightly = CI_CSHARP_E2E_WORKFLOW.read_text(encoding="utf-8")
    workload_manifest = (
        ROOT / "tests/e2e/actr/winui-echo-app/workload/Cargo.toml"
    ).read_text(encoding="utf-8")

    assert "needs.determine.outputs.csharp_binding == 'true'" in csharp_job
    assert "rust_core" not in csharp_job
    assert "full_matrix:" in workflow
    assert "default: false" in workflow
    assert "cache-on-failure: true" in workflow
    assert "workspaces: tests/e2e/actr/winui-echo-app/workload" in workflow
    assert workflow.count("https://api.nuget.org/v3/index.json") == 3
    assert workflow.count("<packageSourceMapping>") == 3
    assert workflow.count('<package pattern="ActrSharp.*" />') == 3
    assert workflow.count('<package pattern="*" />') == 3
    assert '@("--profile", "ci")' in workflow
    assert '@("--release")' in workflow
    assert workflow.count("if: inputs.full_matrix") >= 3
    assert 'cron: "0 19 * * *"' in nightly
    assert "uses: ./.github/workflows/ci-csharp.yml" in nightly
    assert "full_matrix: true" in nightly
    assert '[profile.ci]' in workload_manifest
    assert 'inherits = "dev"' in workload_manifest
    assert "incremental = false" in workload_manifest


def test_release_train_forwards_release_context() -> None:
    workflow = RELEASE_TRAIN_WORKFLOW.read_text(encoding="utf-8")
    release_script = RELEASE_TRAIN_SCRIPT.read_text(encoding="utf-8")
    create_tag_job = _job(workflow, "create-tag", "publish-rust")

    assert "EXPECTED_RELEASE_SHA" in create_tag_job
    assert 'needs.context.outputs.release_sha }}' in create_tag_job

    stage_jobs = (
        ("create-tag", "publish-rust"),
        ("publish-rust", "publish-python"),
        ("publish-python", "publish-swift"),
        ("publish-swift", "publish-kotlin"),
        ("publish-kotlin", "publish-web"),
        ("publish-web", "build-typescript-native"),
        ("publish-typescript-workload", "publish-typescript"),
        ("publish-typescript", "collect-report"),
    )
    for job, next_job in stage_jobs:
        job_workflow = _job(workflow, job, next_job)
        assert 'needs.context.outputs.skip_python }}" == "true"' in job_workflow
        assert "args+=(--skip-python)" in job_workflow
        assert 'needs.context.outputs.pre_release }}" == "true"' in job_workflow
        assert "args+=(--pre-release)" in job_workflow

    report_job = workflow[workflow.index("  collect-report:\n") :]
    assert 'needs.context.outputs.skip_python }}" == "true"' in report_job
    assert "args+=(--skip-python)" in report_job
    assert 'needs.context.outputs.pre_release }}" == "true"' in report_job
    assert "args+=(--pre-release)" in report_job

    assert '[[ "$STAGE" == "report" ]]' in release_script
    assert 'current_sha=$(git rev-parse HEAD)' in release_script
    assert 'Release context SHA ${RELEASE_SHA} does not match current HEAD ${current_sha}' in release_script


def test_release_train_supports_generic_maintenance_branches() -> None:
    workflow = RELEASE_TRAIN_WORKFLOW.read_text(encoding="utf-8")
    release_script = RELEASE_TRAIN_SCRIPT.read_text(encoding="utf-8")

    assert '- "release-*"' in workflow
    assert "target_branch: ${{ steps.check.outputs.target_branch }}" in workflow
    assert "ref: main" not in workflow
    assert "--branch main" not in workflow
    assert "--latest=false" in workflow
    assert "--notes-start-tag" in workflow
    assert "needs.context.result == 'success'" in workflow

    assert "^release-([0-9]+)\\.([0-9]+)$" in release_script
    assert 'NPM_DIST_TAG="legacy-${RELEASE_LINE}"' in release_script
    assert 'ensure_versions_prepared\n  set_release_sha' in release_script


def test_swift_echoapp_e2e_job_present() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    swift_job = _job(workflow, "swift-echo-app-e2e", "python-web-e2e")

    assert "runs-on: macos-arm64-larger" in swift_job
    assert "bash tests/e2e/actr/swift-echo-app/run.sh" in swift_job


def test_e2e_actrix_uses_in_tree_install_instead_of_artifact_download() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    pkg_job = _job(workflow, "package-runtime-echo-e2e", "typescript-e2e")
    swift_job = _job(workflow, "swift-echo-app-e2e", "python-web-e2e")

    assert "download-actrix-artifact.sh" not in workflow
    assert "ACTR_E2E_ACTRIX_ARTIFACT" not in workflow
    assert "ACTRIX_READ_TOKEN" not in workflow
    assert "Actrium/actrix" not in workflow
    assert "actions: read" not in workflow

    assert "runs-on: ubuntu-latest-m" in pkg_job
    assert "runs-on: macos-arm64-larger" in swift_job


def test_e2e_no_private_actrix_checkout() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    common = (ROOT / "tests/e2e/actr/package-runtime-echo/lib/common.sh").read_text(encoding="utf-8")
    cli_support = CLI_TEST_SUPPORT.read_text(encoding="utf-8")

    # No private Actrix checkout git config in any job
    assert "insteadOf" not in workflow
    assert "x-access-token" not in workflow
    assert "Configure git for private Actrix checkout" not in workflow
    assert "Actrium/actrix" not in common
    assert "git clone" not in common
    assert 'actrix_crate_dir="$repo_root/crates/loran-server"' in common
    assert "Actrium/actrix" not in cli_support
    assert "DEFAULT_ACTRIX_REPO" not in cli_support
    assert "ACTR_E2E_ACTRIX_REPO" not in cli_support
    assert "ACTR_E2E_ACTRIX_REV" not in cli_support
    assert "ACTR_E2E_ACTRIX_BIN" not in cli_support
    assert "git clone actrix" not in cli_support
    assert "ACTR_E2E_ACTRIX_ARTIFACT" not in cli_support
    assert "DEFAULT_ACTRIX_ARTIFACT" not in cli_support
    assert "latest_successful_actrix_run" not in cli_support
    assert "try_ensure_actrix_artifact_binary" not in cli_support
    assert 'std::env::var("ACTRIX_BIN")' in cli_support
    assert 'workspace_root().join("crates/loran-server/Cargo.toml")' in cli_support


def test_e2e_no_inline_archive_download_url() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")

    # Inline archive_download_url + curl download replaced by shared script
    assert "archive_download_url" not in workflow


def test_e2e_linux_deps_do_not_include_unused_unzip() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    pkg_job = _job(workflow, "package-runtime-echo-e2e", "typescript-e2e")

    # actrix is installed from the in-tree source path; no artifact unzip is needed.
    assert "unzip" not in pkg_job


def test_e2e_linux_job_has_no_ios_rust_targets() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    pkg_job = _job(workflow, "package-runtime-echo-e2e", "typescript-e2e")

    assert "aarch64-apple-ios" not in pkg_job
    assert "aarch64-apple-ios-sim" not in pkg_job


def test_e2e_workspace_prebuilds_use_locked_cargo_resolution() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ts_stream = (ROOT / "tests/e2e/actr/typescript-stream/run.sh").read_text(encoding="utf-8")

    assert "cargo build --release -p actr-cli --bin actr --features wasm-engine" not in workflow
    assert "cargo build --release -p actr-cli --bin actr --features wasm-engine" not in ts_stream
    assert workflow.count(
        "cargo build --locked --release -p actr-cli --bin actr --features wasm-engine"
    ) == 3
    assert workflow.count(
        "cargo build --locked --release -p actr-mock-actrix --bin mock-actrix"
    ) == 2
    assert "cargo build --locked -p actr-framework-protoc-codegen" in ts_stream
    assert 'cp "$REPO_ROOT/Cargo.lock" "$project/Cargo.lock"' in ts_stream
    assert "cargo generate-lockfile --offline" in ts_stream
    assert "cargo run --locked --features host --bin client-app" in ts_stream


def test_swift_e2e_upload_artifact_on_failure() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    swift_jobs = {
        "swift-echo-app-e2e": "tests/e2e/actr/swift-echo-app/.tmp/sanitized-logs/",
        "swift-datastream-app-e2e": "tests/e2e/actr/swift-datastream-app/.tmp/sanitized-logs/",
        "swift-ts-workload-e2e": "tests/e2e/actr/swift-ts-workload/.tmp/sanitized-logs/",
    }

    for job_name, upload_path in swift_jobs.items():
        swift_job = _job(workflow, job_name, "python-web-e2e")
        # Upload step runs even on failure.
        assert "if: always()" in swift_job
        assert "actions/upload-artifact@v4" in swift_job
        assert "retention-days: 7" in swift_job
        # Upload path must match the fixed location used by cleanup trap.
        assert upload_path in swift_job

    for script_path in (
        "tests/e2e/actr/swift-echo-app/run.sh",
        "tests/e2e/actr/swift-datastream-app/run.sh",
        "tests/e2e/actr/swift-ts-workload/run.sh",
    ):
        run_sh = (ROOT / script_path).read_text(encoding="utf-8")
        # Cleanup must output sanitized logs to the same fixed path the workflow uploads.
        assert ".tmp/sanitized-logs" in run_sh


def test_swift_e2e_scripts_capture_app_diagnostics() -> None:
    swift_scripts = {
        "tests/e2e/actr/swift-echo-app/run.sh": "io.actrium.EchoApp",
        "tests/e2e/actr/swift-datastream-app/run.sh": "io.actrium.DataStreamApp",
        "tests/e2e/actr/swift-ts-workload/run.sh": "io.actrium.SwiftTsWorkloadApp",
    }

    for script_path, bundle_id in swift_scripts.items():
        run_sh = (ROOT / script_path).read_text(encoding="utf-8")

        assert f'APP_BUNDLE_ID="{bundle_id}"' in run_sh
        assert 'APP_PID=""' in run_sh
        assert "record_app_pid_from_launch_log" in run_sh
        assert 'echo "APP_PID=${APP_PID:-none}"' in run_sh
        assert 'ps -p "$APP_PID" -o pid,ppid,stat,etime,command' in run_sh
        assert 'sample "$APP_PID" 5 1 -file "$diag_dir/app.sample.txt"' in run_sh
        assert 'xcrun simctl spawn "$DEVICE_UDID" log show --last 10m --style compact' in run_sh
        assert (
            'printf \'\\n\' | xcrun simctl diagnose -b --timeout=60 --output="$diagnose_dir" --no-archive'
            in run_sh
        )
        assert '--udid="$DEVICE_UDID"' in run_sh
        assert "Library/Logs/DiagnosticReports" in run_sh
        assert 'get_app_container "$DEVICE_UDID" "$APP_BUNDLE_ID" data' in run_sh
        assert run_sh.count("fail_if_app_exited_before_result") >= 2
        assert 'find "$src_dir" -type f' in run_sh


def test_swift_e2e_uploads_debug_symbols_separately() -> None:
    debug_symbol_settings = (
        "configs:\n"
        "    Debug:\n"
        "      DEBUG_INFORMATION_FORMAT: dwarf-with-dsym\n"
        "      GCC_GENERATE_DEBUGGING_SYMBOLS: YES\n"
        "      COPY_PHASE_STRIP: NO\n"
        "      STRIP_INSTALLED_PRODUCT: NO"
    )

    for project_path in (
        "tests/e2e/actr/swift-echo-app/project.yml",
        "tests/e2e/actr/swift-datastream-app/project.yml",
    ):
        project_yml = (ROOT / project_path).read_text(encoding="utf-8")
        assert debug_symbol_settings in project_yml

    for script_path in (
        "tests/e2e/actr/swift-echo-app/run.sh",
        "tests/e2e/actr/swift-datastream-app/run.sh",
        "tests/e2e/actr/swift-ts-workload/run.sh",
    ):
        run_sh = (ROOT / script_path).read_text(encoding="utf-8")
        assert 'SYMBOL_DIR="$RUN_DIR/symbols"' in run_sh
        assert 'APP_DSYM="${APP_PATH}.dSYM"' in run_sh
        assert "DEBUG_INFORMATION_FORMAT=dwarf-with-dsym" in run_sh
        assert "GCC_GENERATE_DEBUGGING_SYMBOLS=YES" in run_sh
        assert "COPY_PHASE_STRIP=NO" in run_sh
        assert "STRIP_INSTALLED_PRODUCT=NO" in run_sh
        assert 'xcrun dwarfdump --uuid "$APP_DSYM" >"$SYMBOL_DIR/uuids.txt"' in run_sh
        assert 'cp -R "$APP_DSYM" "$SYMBOL_DIR/"' in run_sh
        assert 'find "$products_dir" -type d -name "*.dSYM"' in run_sh
        assert 'for app_binary in "$APP_BINARY" "$APP_PATH"/*.debug.dylib' in run_sh
        assert (
            'cp "$app_binary" "$SYMBOL_DIR/${APP_PROCESS_NAME}.app/"'
            in run_sh
        )
        assert 'local symbol_upload_dir="$SCRIPT_DIR/.tmp/symbols"' in run_sh
        assert 'mv "$SYMBOL_DIR" "$symbol_upload_dir"' in run_sh
        assert 'sanitize_logs_for_upload "$SYMBOL_DIR"' not in run_sh
        assert 'collect_app_symbols "$derived_data/Build/Products"' in run_sh
        assert 'CAPTURE_CRASH_BACKTRACE:-0' in run_sh
        assert 'launch_args+=(--wait-for-debugger)' in run_sh
        assert "start_lldb_crash_capture" in run_sh
        assert '--attach-pid "$APP_PID"' in run_sh
        assert "settings set target.debug-file-search-paths" in run_sh
        assert "target symbols add" not in run_sh
        assert '-k "thread backtrace all"' in run_sh
        assert '-k "register read"' in run_sh
        assert '-k "image list -o -f"' in run_sh
        assert '"$LOG_DIR/app.lldb.log"' in run_sh
        assert "wait_for_lldb_capture" in run_sh

    for script_path in (
        "tests/e2e/actr/swift-datastream-app/run.sh",
        "tests/e2e/actr/swift-ts-workload/run.sh",
    ):
        run_sh = (ROOT / script_path).read_text(encoding="utf-8")
        assert debug_symbol_settings in run_sh

    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    swift_jobs = (
        (
            "swift-echo-app-e2e",
            "swift-datastream-app-e2e",
            "tests/e2e/actr/swift-echo-app/.tmp/symbols/",
        ),
        (
            "swift-datastream-app-e2e",
            "swift-ts-workload-e2e",
            "tests/e2e/actr/swift-datastream-app/.tmp/symbols/",
        ),
        (
            "swift-ts-workload-e2e",
            "python-web-e2e",
            "tests/e2e/actr/swift-ts-workload/.tmp/symbols/",
        ),
    )
    for job_name, next_job_name, symbol_path in swift_jobs:
        swift_job = _job(workflow, job_name, next_job_name)
        assert symbol_path in swift_job
        assert "-symbols-${{ github.run_id }}-${{ github.run_attempt }}" in swift_job
        assert swift_job.count("actions/upload-artifact@v4") == 2
        assert "DerivedData" not in "\n".join(
            line
            for line in swift_job.splitlines()
            if "path:" in line and ".tmp/" in line
        )


def test_e2e_no_call_remote_in_ffi() -> None:
    ffi_runtime = (ROOT / "crates/actr-ffi/src/runtime.rs").read_text(encoding="utf-8")
    swift_actr_ref = (ROOT / "packages/actr-swift/Sources/Actr/ActrRef.swift").read_text(encoding="utf-8")

    # callRemote was an exploratory API, removed to keep the established
    # local-workload-forwarding path
    assert "call_remote" not in ffi_runtime
    assert "callRemote" not in swift_actr_ref


def test_no_actrix_release_train_artifact_download_script() -> None:
    assert not (ROOT / ".github/scripts/download-actrix-artifact.sh").exists()


def test_run_sh_uses_correct_signaling_cache_table() -> None:
    run_sh = (ROOT / "tests/e2e/actr/swift-echo-app/run.sh").read_text(encoding="utf-8")

    # Must query the actual table name (service_registry, not service_registrations)
    assert "service_registry" in run_sh
    assert "service_registrations" not in run_sh
    assert "actor_registry" not in run_sh


def test_swift_e2e_only_shuts_down_its_selected_simulator() -> None:
    run_sh = (ROOT / "tests/e2e/actr/swift-echo-app/run.sh").read_text(encoding="utf-8")

    assert 'xcrun simctl shutdown "$DEVICE_UDID"' in run_sh
    assert "xcrun simctl shutdown all" not in run_sh


def test_swift_e2e_covers_short_and_long_background_recovery() -> None:
    run_sh = (ROOT / "tests/e2e/actr/swift-echo-app/run.sh").read_text(encoding="utf-8")
    content_view = (ROOT / "tests/e2e/actr/swift-echo-app/EchoApp/ContentView.swift").read_text(
        encoding="utf-8"
    )

    assert 'SHORT_BACKGROUND_SECONDS="${SHORT_BACKGROUND_SECONDS:-5}"' in run_sh
    assert 'LONG_BACKGROUND_SECONDS="${LONG_BACKGROUND_SECONDS:-60}"' in run_sh
    assert 'xcrun simctl launch "$DEVICE_UDID" com.apple.Preferences' in run_sh
    assert 'ACTR_E2E_PHASE:background' in run_sh
    assert 'ACTR_E2E_PHASE:foreground-${cycle}' in run_sh
    assert "ACTR_ECHOAPP_LIFECYCLE_E2E" in content_view
    assert "await sendRecoveryEcho(\"\\(input)-foreground-\\(cycle)\")" in content_view


def test_swift_e2e_delegates_recovery_to_framework() -> None:
    service = (ROOT / "tests/e2e/actr/swift-echo-app/EchoApp/ActrService.swift").read_text(
        encoding="utf-8"
    )
    content_view = (ROOT / "tests/e2e/actr/swift-echo-app/EchoApp/ContentView.swift").read_text(
        encoding="utf-8"
    )

    assert "func sendEcho(_ input: String)" in service
    # The removed retry helper must not be CALLED: a bare identifier ban is
    # collision-prone (a comment or an unrelated helper named after it would
    # fail the gate), so assert no call site remains — an application-side
    # helper definition plus call would match `sendEchoWhenReady(` anywhere;
    # we instead require that the only occurrence, if any, is not a call.
    leftover_calls = [
        line
        for line in service.splitlines()
        if "sendEchoWhenReady" in line and "(" in line.split("sendEchoWhenReady", 1)[1]
    ]
    assert not leftover_calls, f"application-side readiness retry leaked: {leftover_calls}"
    assert "Call owns route readiness and transport retry" in content_view
    assert "ACTR_E2E_RETRY:ConnectionNotReady" not in service


def _create_service_registry(db_path: Path) -> None:
    with sqlite3.connect(db_path) as connection:
        connection.execute(
            """
            CREATE TABLE service_registry (
                actor_realm_id INTEGER NOT NULL,
                actor_manufacturer TEXT NOT NULL,
                actor_device_name TEXT NOT NULL,
                service_name TEXT NOT NULL,
                status TEXT NOT NULL,
                last_heartbeat_at INTEGER NOT NULL
            )
            """
        )


def _run_service_readiness(db_path: Path, timeout: str = "0") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "bash",
            "-c",
            'source "$1"; wait_for_service_registration "$2" 1001 actrium EchoService "$3"',
            "bash",
            str(SWIFT_E2E_READINESS),
            str(db_path),
            timeout,
        ],
        check=False,
        capture_output=True,
        text=True,
    )


def test_service_readiness_rejects_missing_or_unrelated_registration() -> None:
    with tempfile.TemporaryDirectory() as temp_dir:
        db_path = Path(temp_dir) / "signaling_cache.db"
        _create_service_registry(db_path)

        missing = _run_service_readiness(db_path)
        assert missing.returncode != 0

        with sqlite3.connect(db_path) as connection:
            connection.execute(
                """
                INSERT INTO service_registry
                    (actor_realm_id, actor_manufacturer, actor_device_name,
                     service_name, status, last_heartbeat_at)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (1001, "actrium", "OtherService", "actrium:OtherService", "Available", 1),
            )

        unrelated = _run_service_readiness(db_path)
        assert unrelated.returncode != 0


def test_nightly_e2e_includes_swift_datastream() -> None:
    """Verify ci-e2e.yml contains the swift-datastream-app-e2e job."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    assert "swift-datastream-app-e2e:" in workflow


def test_swift_datastream_job_runs_on_macos() -> None:
    """Verify the datastream job uses runs-on: macos-arm64-larger."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    assert "runs-on: macos-arm64-larger" in ds_job


def test_swift_datastream_job_does_not_download_actrix_artifact() -> None:
    """Verify the datastream job builds actrix from the in-tree source path."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    assert "download-actrix-artifact.sh" not in ds_job


def test_swift_datastream_job_builds_xcframework() -> None:
    """Verify the datastream job builds the Swift XCFramework."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    assert "build-xcframework.sh" in ds_job


def test_swift_datastream_timeout_not_less_than_240() -> None:
    """Verify the datastream job has timeout-minutes >= 240."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    import re

    match = re.search(r"timeout-minutes:\s*(\d+)", ds_job)
    assert match is not None, "timeout-minutes not found in swift-datastream-app-e2e job"
    timeout = int(match.group(1))
    assert timeout >= 240, f"timeout-minutes is {timeout}, expected >= 240"


def test_swift_datastream_job_not_in_pr_gate() -> None:
    """Verify ci-gate.yml does NOT contain swift-datastream-app-e2e."""
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    assert "swift-datastream-app-e2e" not in workflow


def test_swift_datastream_cleanup_only_removes_owned_simulator() -> None:
    """Verify local cleanup cannot shut down unrelated iOS Simulators."""
    run_sh = (ROOT / "tests/e2e/actr/swift-datastream-app/run.sh").read_text(encoding="utf-8")

    assert "xcrun simctl shutdown all" not in run_sh
    assert 'DEVICE_CREATED="0"' in run_sh
    assert 'DEVICE_CREATED="1"' in run_sh
    assert 'if [ "$DEVICE_CREATED" = "1" ]' in run_sh
    assert 'xcrun simctl shutdown "$DEVICE_UDID"' in run_sh
    assert 'xcrun simctl delete "$DEVICE_UDID"' in run_sh


def test_datastream_cli_templates_use_empty_scaffolds() -> None:
    """Verify Rust/Swift DataStream init starts from empty templates only."""
    rust_mod = (ROOT / "crates/actr-cli/src/templates/rust/mod.rs").read_text(encoding="utf-8")
    swift_mod = (ROOT / "crates/actr-cli/src/templates/swift/mod.rs").read_text(encoding="utf-8")

    assert "pub mod data_stream;" not in rust_mod
    assert "pub mod data_stream;" not in swift_mod
    assert "ProjectTemplateName::Empty" in rust_mod
    assert "ProjectTemplateName::Empty" in swift_mod
    assert "ProjectTemplateName::DataStream" in rust_mod
    assert "ProjectTemplateName::DataStream" in swift_mod
    assert rust_mod.count("empty::load(&mut files)?;") >= 2
    assert swift_mod.count("empty::load(&mut files)?;") >= 2
    assert not (ROOT / "crates/actr-cli/src/templates/rust/data_stream.rs").exists()
    assert not (ROOT / "crates/actr-cli/src/templates/swift/data_stream.rs").exists()
    assert not any((ROOT / "crates/actr-cli/fixtures/rust/data-stream").glob("*"))
    assert not any((ROOT / "crates/actr-cli/fixtures/swift/data-stream").glob("*"))


def test_swift_datastream_e2e_builds_custom_proto_flow_from_empty() -> None:
    """Verify DataStreamApp E2E creates custom proto/service after empty init."""
    run_sh = (ROOT / "tests/e2e/actr/swift-datastream-app/run.sh").read_text(encoding="utf-8")
    actr_toml = (ROOT / "tests/e2e/actr/swift-datastream-app/actr.toml.tpl").read_text(
        encoding="utf-8"
    )
    actr_service = (
        ROOT / "tests/e2e/actr/swift-datastream-app/DataStreamApp/Services/ActrService.swift"
    ).read_text(encoding="utf-8")

    assert "--template empty" in run_sh
    assert 'rm -f "$TMP_SERVICE_DIR/protos/local/local.proto"' in run_sh
    assert 'rm -f "$TMP_APP_DIR/protos/local/local.proto"' in run_sh
    assert 'write_duplex_stream_proto "$TMP_SERVICE_DIR/protos/local/duplex_stream.proto"' in run_sh
    assert 'write_probe_proto "$TMP_APP_DIR/protos/local/probe.proto"' in run_sh
    assert 'write_duplex_stream_proto "$TMP_APP_DIR/protos/remote' not in run_sh
    assert "use crate::generated::local::{" in run_sh
    assert "use crate::generated::duplex_stream::{" not in run_sh
    assert "run_actr gen -l rust" in run_sh
    assert "run_actr gen -l swift" in run_sh
    assert "rm -f DataStreamApp/ActrService.swift" in run_sh
    assert "SIMCTL_CHILD_ACTR_DATASTREAMAPP_AUTO_STREAM_COUNT=3" in run_sh
    assert "ACTR_E2E_RESULT:3/3" in run_sh
    main_flow = run_sh.split("# ──── Main ────", 1)[1]
    assert main_flow.index("run_server_host") < main_flow.index("build_datastream_app")
    assert main_flow.index("check_service_ready") < main_flow.index("build_datastream_app")
    assert "__MANUFACTURER__:DuplexStreamService:1.0.0" in actr_toml
    assert 'ActrType(manufacturer: manufacturer, name: "DataStreamApp", version: "0.1.0")' in actr_service
    assert 'let expectedLines = (1...count).map { "received: echo: hello \\($0)" }' in actr_service


def test_service_readiness_waits_for_exact_registration() -> None:
    with tempfile.TemporaryDirectory() as temp_dir:
        db_path = Path(temp_dir) / "signaling_cache.db"
        _create_service_registry(db_path)

        script = """
            source "$1"
            (
                sleep 0.2
                sqlite3 "$2" \
                    "INSERT INTO service_registry VALUES \
                    (1001, 'actrium', 'EchoService', 'actrium:EchoService', 'Available', 1);"
            ) &
            SERVICE_READY_POLL_INTERVAL_SECONDS=0.1 \
                wait_for_service_registration "$2" 1001 actrium EchoService 2
        """
        result = subprocess.run(
            ["bash", "-c", script, "bash", str(SWIFT_E2E_READINESS), str(db_path)],
            check=False,
            capture_output=True,
            text=True,
        )

        assert result.returncode == 0, result.stderr


if __name__ == "__main__":
    test_gate_aggregates_webrtc_v020_domain()
    test_gate_enforces_anonymous_commit_identities()
    test_rust_gate_avoids_slow_workspace_tests_and_unused_prewarm()
    test_rust_test_gate_restores_cache_before_installing_tools()
    test_msrv_gate_builds_full_workspace_on_pinned_toolchain()
    test_pr_gate_excludes_heavy_root_e2e_jobs()
    test_pr_gate_compiles_rust_e2e_workspace_with_committed_lockfile()
    test_pr_gate_runs_package_runtime_scenario_when_targeted()
    test_native_workload_gate_builds_c_and_go_with_pinned_tools()
    test_blocked_typescript_component_e2e_jobs_share_disable_gate()
    test_typescript_componentize_only_allows_known_upstream_failure()
    test_python_workload_gate_builds_generated_cli_scaffold()
    test_python_template_keeps_project_and_abi_versions_distinct()
    test_pr_gate_swift_uses_macos_only_xcframework()
    test_release_train_has_valid_publish_steps()
    test_release_train_verifies_ci_gate_triggered()
    test_release_train_reuses_actrsharp_workflow()
    test_csharp_pr_gate_uses_light_fixture_and_nightly_keeps_full_matrix()
    test_release_train_forwards_release_context()
    test_release_train_supports_generic_maintenance_branches()
    test_swift_echoapp_e2e_job_present()
    test_e2e_actrix_uses_in_tree_install_instead_of_artifact_download()
    test_e2e_no_private_actrix_checkout()
    test_e2e_no_inline_archive_download_url()
    test_e2e_linux_deps_do_not_include_unused_unzip()
    test_e2e_linux_job_has_no_ios_rust_targets()
    test_e2e_workspace_prebuilds_use_locked_cargo_resolution()
    test_swift_e2e_upload_artifact_on_failure()
    test_swift_e2e_scripts_capture_app_diagnostics()
    test_swift_e2e_uploads_debug_symbols_separately()
    test_e2e_no_call_remote_in_ffi()
    test_no_actrix_release_train_artifact_download_script()
    test_run_sh_uses_correct_signaling_cache_table()
    test_swift_e2e_only_shuts_down_its_selected_simulator()
    test_swift_e2e_covers_short_and_long_background_recovery()
    test_swift_e2e_delegates_recovery_to_framework()
    test_service_readiness_rejects_missing_or_unrelated_registration()
    test_service_readiness_waits_for_exact_registration()
    test_nightly_e2e_includes_swift_datastream()
    test_swift_datastream_job_runs_on_macos()
    test_swift_datastream_job_does_not_download_actrix_artifact()
    test_swift_datastream_job_builds_xcframework()
    test_swift_datastream_timeout_not_less_than_240()
    test_swift_datastream_job_not_in_pr_gate()
    test_swift_datastream_cleanup_only_removes_owned_simulator()
    test_datastream_cli_templates_use_empty_scaffolds()
    test_swift_datastream_e2e_builds_custom_proto_flow_from_empty()
