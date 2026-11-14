#!/usr/bin/env python3

import importlib.util
import re
import sqlite3
import subprocess
import tempfile
from textwrap import dedent
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CI_GATE_WORKFLOW = ROOT / ".github/workflows/ci-gate.yml"
CI_E2E_WORKFLOW = ROOT / ".github/workflows/ci-e2e.yml"
CI_CSHARP_WORKFLOW = ROOT / ".github/workflows/ci-csharp.yml"
CI_CSHARP_E2E_WORKFLOW = ROOT / ".github/workflows/ci-csharp-e2e.yml"
CI_TYPESCRIPT_WORKFLOW = ROOT / ".github/workflows/ci-typescript.yml"
RELEASE_TRAIN_WORKFLOW = ROOT / ".github/workflows/release-train.yml"
RELEASE_TRAIN_VERIFY_WORKFLOW = ROOT / ".github/workflows/release-train-verify.yml"
RELEASE_PREPARE_WORKFLOW = ROOT / ".github/workflows/release-prepare.yml"
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


def test_gate_aggregates_webrtc_domain() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    # `gate` is the workflow's final job, so slice from its key to EOF.
    gate = workflow[workflow.index("  gate:\n"):]
    # The RTC domain must stay in the gate's needs list and in the Python
    # summary dict; dropping either would detach the RTC implementation from the
    # gating decision without failing any other structure pin.
    assert "      - webrtc\n" in gate
    assert 'WEBRTC_RESULT: ${{ needs.webrtc.result }}' in gate
    assert '"webrtc": os.environ["WEBRTC_RESULT"]' in gate
    rtc_job = _job(workflow, "webrtc", "cjk")
    assert "test(/^wire::webrtc::rtc/)" in rtc_job


def test_macos_host_checks_are_affected_only_and_gate_failures() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    job = _job(workflow, "macos_host", "kotlin_native")
    gate = workflow[workflow.index("  gate:\n"):]
    assert "macos_host: ${{ steps.targets.outputs.macos_host }}" in workflow
    assert "if: needs.determine.outputs.macos_host == 'true'" in job
    assert "runs-on: macos-15" in job
    assert "bash macos/scripts/test-host.sh" in job
    assert "      - macos_host\n" in gate
    assert "MACOS_HOST_RESULT: ${{ needs.macos_host.result }}" in gate
    assert '"macos_host": os.environ["MACOS_HOST_RESULT"]' in gate


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


def test_gate_aggregates_kotlin_native_domain() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    gate = workflow[workflow.index("  gate:\n"):]
    assert "      - kotlin_native\n" in gate
    assert '"kotlin_native": os.environ["KOTLIN_NATIVE_RESULT"]' in gate
    kotlin = (ROOT / ".github/workflows/ci-kotlin.yml").read_text(encoding="utf-8")
    assert "ACTR_ANDROID_TARGETS: ${{ matrix.target }}" in kotlin
    assert "abi: arm64-v8a" in kotlin and "abi: x86_64" in kotlin


def test_rust_gate_avoids_slow_workspace_tests_and_unused_prewarm() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    rust_job = _job(workflow, "rust", "test")

    assert "dtolnay/rust-toolchain@1.99.0" in rust_job
    assert "targets: wasm32-wasip2,wasm32-unknown-unknown" in rust_job
    assert 'ACTR_REQUIRE_WASM_FIXTURE: "1"' in rust_job
    assert "- name: Run tests" not in rust_job
    assert "cargo test --workspace" not in rust_job
    assert "- name: Prepare Rust codegen plugins" not in rust_job
    assert "cargo install protoc-gen-prost" not in rust_job
    assert "cargo build --quiet -p actr-framework-protoc-codegen" not in rust_job
    assert "- name: Build workspace" not in rust_job
    assert "cargo build --verbose --all-features" not in rust_job
    assert "- name: Build release" not in rust_job
    assert "cargo build --release --verbose --all-features" not in rust_job

    # The separate job runs the same full workspace inventory in four
    # disjoint shards, including the original realtime and doctest coverage.
    assert "  test:" in workflow
    test_job = _job(workflow, "test", "rust_msrv")
    assert "shard: [build-heavy, network, rest-1, rest-2]" in test_job
    assert "- name: Run tests (nextest, ${{ matrix.shard }} shard)" in test_job
    assert "cargo nextest list --workspace --all-features" in test_job
    assert "assert-ci-test-partitions.py" in test_job
    assert "assert-ci-timer.py" in test_job
    assert "cargo test --doc --workspace" in test_job
    assert "if: matrix.shard == 'rest-2'" in test_job


def test_test_partition_guard_rejects_missing_and_overlapping_tests() -> None:
    path = ROOT / ".github/scripts/assert-ci-test-partitions.py"
    spec = importlib.util.spec_from_file_location("ci_test_partitions", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    first = {("binary-a", "test-a")}
    second = {("binary-b", "test-b")}
    module.verify(first | second, first, second)
    for parts in ((first, first | second), (first, {("binary-c", "extra")}), (first, set())):
        try:
            module.verify(first | second, *parts)
        except ValueError:
            continue
        raise AssertionError(f"Invalid partition accepted: {parts}")


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


def test_gha_cache_consumers_always_initialize_credentials() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    for name, next_name in [
        ("cli_web_assets", "e2e_rust"),
        ("rust", "test"),
        ("test", "rust_msrv"),
        ("rust_msrv", "typescript"),
    ]:
        job = _job(workflow, name, next_name)
        setup = re.search(
            r"(?ms)^      - name: Run sccache-cache\n(.*?)(?=^      - |\Z)", job
        )
        assert setup is not None, name
        assert "mozilla-actions/sccache-action@v0.0.9" in setup[1], name
        assert "version: v0.18.0" in setup[1], name
        conditions = re.findall(r"^        if: (.+)$", setup[1], re.M)
        expected = (
            ["steps.web-runtime.outputs.ready != 'true'"]
            if name == "cli_web_assets" else []
        )
        assert conditions == expected, name


def test_cli_web_runtime_cache_requires_complete_build_output() -> None:
    job = _job(CI_GATE_WORKFLOW.read_text(), "cli_web_assets", "e2e_rust")
    probe = re.search(
        r"(?ms)^      - name: Check cached Web runtime\n.*?^        run: \|\n(.*?)(?=^      - )", job
    )
    assert probe is not None
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        dist = root / "packages/actr-web/dist/sw"
        dist.mkdir(parents=True)
        files = [dist / name for name in (
            "actr_sw_host_bg.wasm", "actr_sw_host.js", "actr_sw_host.d.ts", "package.json"
        )]
        for file in files:
            file.write_bytes(b"built")

        def ready(hit: str) -> bool:
            output = root / "output"
            output.write_text("")
            subprocess.run(
                ["bash", "-eu", "-c", dedent(probe[1])], cwd=root, check=True,
                env={"CACHE_HIT": hit, "GITHUB_OUTPUT": str(output)},
            )
            result = output.read_text().strip()
            assert result in {"ready=true", "ready=false"}
            return result == "ready=true"

        assert ready("true")
        assert not ready("false")
        for file in files:
            file.unlink()
            assert not ready("true"), file
            file.touch()
            assert not ready("true"), file
            file.write_bytes(b"built")


def test_cli_web_assets_installs_pinned_wasm_toolchain() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    job = _job(workflow, "cli_web_assets", "e2e_rust")
    assert "runs-on: ubuntu-24.04" in job
    assert "RUSTUP_TOOLCHAIN: 1.99.0" in job
    assert "targets: wasm32-unknown-unknown" in job
    assert "dtolnay/rust-toolchain@1.99.0" in job
    assert "arduino/setup-protoc@v3" in job


def test_pr_gate_compiles_rust_e2e_workspace_with_committed_lockfile() -> None:
    workflow = CI_GATE_WORKFLOW.read_text(encoding="utf-8")
    # The compile job ends where the targeted scenario job begins.
    e2e_job = _job(workflow, "e2e_rust", "e2e_package_runtime")

    assert "needs.determine.outputs.e2e_rust == 'true'" in e2e_job
    normalized = " ".join(e2e_job.split())
    assert "cargo metadata" in normalized
    assert e2e_job.count("working-directory: tests/e2e/actr\n") == 2
    assert "workspaces: tests/e2e/actr -> ../target/e2e" in e2e_job
    assert "--format-version 1" in normalized
    assert "--locked" in normalized
    assert "dtolnay/rust-toolchain@1.99.0" in e2e_job
    assert "arduino/setup-protoc@v3" in e2e_job
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
    assert "scenario: [echo, realms, rotation]" in package_job
    for scenario in ("echo", "realms", "rotation"):
        assert f"if: matrix.scenario == '{scenario}'" in package_job


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
        assert "#50" in job

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
    assert "tracked by #54" in guard
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


def _workflow_grants_pull_request_write(workflow: Path) -> bool:
    """Return whether a workflow grants the token write access to pull requests.

    GitHub accepts scalar and inline-map permission syntax in addition to the
    usual indented map. Keep this check dependency-free so CI can run it before
    installing any Python packages.
    """

    lines = workflow.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        match = re.match(
            r"^(?P<indent>\s*)['\"]?permissions['\"]?:\s*(?P<value>.*)$", line
        )
        if not match:
            continue

        value = match.group("value").split("#", 1)[0].strip()
        if re.fullmatch(r"['\"]?write-all['\"]?", value):
            return True

        inline = re.fullmatch(r"\{(?P<body>.*)\}", value)
        if inline and re.search(
            r"(?:^|,)\s*['\"]?pull-requests['\"]?\s*:\s*['\"]?write['\"]?\s*(?:,|$)",
            inline.group("body"),
        ):
            return True

        permission_indent = len(match.group("indent"))
        for child in lines[index + 1 :]:
            if not child.strip() or child.lstrip().startswith("#"):
                continue
            child_indent = len(child) - len(child.lstrip())
            if child_indent <= permission_indent:
                break
            if re.match(
                r"^\s*['\"]?pull-requests['\"]?\s*:\s*['\"]?write['\"]?\s*(?:#.*)?$",
                child,
            ):
                return True

    return False


def test_only_release_prepare_can_write_pull_requests() -> None:
    workflow_files = sorted(
        path
        for suffix in ("*.yml", "*.yaml")
        for path in (ROOT / ".github/workflows").glob(suffix)
    )
    writers = sorted(
        workflow.name
        for workflow in workflow_files
        if _workflow_grants_pull_request_write(workflow)
    )

    assert writers == [RELEASE_PREPARE_WORKFLOW.name]


def test_pull_request_permission_scanner_handles_supported_syntax() -> None:
    variants = {
        "scalar.yml": "permissions: write-all\n",
        "inline.yaml": "permissions: { contents: read, pull-requests: write }\n",
        "job.yml": "jobs:\n  release:\n    permissions:\n      pull-requests: write\n",
        "quoted.yaml": '"permissions": {"pull-requests": "write"}\n',
    }

    with tempfile.TemporaryDirectory() as directory:
        for filename, contents in variants.items():
            workflow = Path(directory) / filename
            workflow.write_text(contents, encoding="utf-8")
            assert _workflow_grants_pull_request_write(workflow)

        safe_workflow = Path(directory) / "safe.yml"
        safe_workflow.write_text(
            "permissions:\n"
            "  contents: read\n"
            "jobs:\n"
            "  check:\n"
            "    steps:\n"
            "      - run: echo 'pull-requests: write'\n",
            encoding="utf-8",
        )
        assert not _workflow_grants_pull_request_write(safe_workflow)


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
    assert '--from-release-commit' in workflow
    assert 'scripts/actr/generate-changelog.py' in workflow
    assert "needs.context.result == 'success'" in workflow

    assert "^release-([0-9]+)\\.([0-9]+)$" in release_script
    assert 'NPM_DIST_TAG="legacy-${RELEASE_LINE}"' in release_script
    assert 'ensure_versions_prepared\n  set_release_sha' in release_script


def test_swift_echoapp_e2e_job_present() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    swift_job = _job(workflow, "swift-echo-app-e2e", "python-web-e2e")

    assert "runs-on: macos-15" in swift_job
    assert "bash tests/e2e/actr/swift-echo-app/run.sh" in swift_job


def test_e2e_loran_uses_in_tree_install_instead_of_artifact_download() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    pkg_job = _job(workflow, "package-runtime-echo-e2e", "typescript-e2e")
    swift_job = _job(workflow, "swift-echo-app-e2e", "python-web-e2e")

    assert "download-loran-artifact.sh" not in workflow
    assert "ACTR_E2E_LORAN_ARTIFACT" not in workflow
    assert "LORAN_READ_TOKEN" not in workflow
    assert "Actrium/loran" not in workflow
    assert "actions: read" not in workflow

    assert "runs-on: ubuntu-24.04" in pkg_job
    assert "runs-on: macos-15" in swift_job


def test_e2e_no_private_loran_checkout() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    common = (ROOT / "tests/e2e/actr/package-runtime-echo/lib/common.sh").read_text(encoding="utf-8")
    cli_support = CLI_TEST_SUPPORT.read_text(encoding="utf-8")

    # No private Loran checkout git config in any job
    assert "insteadOf" not in workflow
    assert "x-access-token" not in workflow
    assert "Configure git for private Loran checkout" not in workflow
    assert "Actrium/loran" not in common
    assert "git clone" not in common
    assert 'loran_crate_dir="$repo_root/crates/loran-server"' in common
    assert "Actrium/loran" not in cli_support
    assert "DEFAULT_LORAN_REPO" not in cli_support
    assert "ACTR_E2E_LORAN_REPO" not in cli_support
    assert "ACTR_E2E_LORAN_REV" not in cli_support
    assert "ACTR_E2E_LORAN_BIN" not in cli_support
    assert "git clone loran" not in cli_support
    assert "ACTR_E2E_LORAN_ARTIFACT" not in cli_support
    assert "DEFAULT_LORAN_ARTIFACT" not in cli_support
    assert "latest_successful_loran_run" not in cli_support
    assert "try_ensure_loran_artifact_binary" not in cli_support
    assert 'std::env::var("LORAN_BIN")' in cli_support
    assert 'workspace_root().join("crates/loran-server/Cargo.toml")' in cli_support


def test_e2e_no_inline_archive_download_url() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")

    # Inline archive_download_url + curl download replaced by shared script
    assert "archive_download_url" not in workflow


def test_e2e_linux_deps_do_not_include_unused_unzip() -> None:
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    pkg_job = _job(workflow, "package-runtime-echo-e2e", "typescript-e2e")

    # loran is installed from the in-tree source path; no artifact unzip is needed.
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
        "cargo build --locked --release -p actr-mock-loran --bin mock-loran"
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


def test_no_loran_release_train_artifact_download_script() -> None:
    assert not (ROOT / ".github/scripts/download-loran-artifact.sh").exists()


def test_run_sh_uses_correct_signaling_cache_table() -> None:
    run_sh = (ROOT / "tests/e2e/actr/swift-echo-app/run.sh").read_text(encoding="utf-8")

    # Must query the actual table name (actor_registry, not service_registrations)
    assert "actor_registry" in run_sh
    assert "service_registrations" not in run_sh
    assert "service_registry" not in run_sh


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
    # The removed retry helper must not be CALLED. A regex tolerates
    # whitespace/comments between the identifier and its argument list, so
    # formatting or line wrapping cannot produce a false pass/fail.
    import re

    assert not re.search(r"sendEchoWhenReady\s*\(", service), (
        "application-side readiness retry leaked"
    )
    assert "Call owns route readiness and transport retry" in content_view
    assert "ACTR_E2E_RETRY:ConnectionNotReady" not in service


def _create_actor_registry(db_path: Path) -> None:
    with sqlite3.connect(db_path) as connection:
        connection.execute(
            """
            CREATE TABLE actor_registry (
                actor_realm TEXT NOT NULL,
                actor_publisher TEXT NOT NULL,
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
            'source "$1"; wait_for_service_registration "$2" e2e@e2e.test actrium EchoService "$3"',
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
        _create_actor_registry(db_path)

        missing = _run_service_readiness(db_path)
        assert missing.returncode != 0

        with sqlite3.connect(db_path) as connection:
            connection.execute(
                """
                INSERT INTO actor_registry
                    (actor_realm, actor_publisher, actor_device_name,
                     service_name, status, last_heartbeat_at)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                ("e2e@e2e.test", "actrium", "OtherService", "actrium:OtherService", "Available", 1),
            )

        unrelated = _run_service_readiness(db_path)
        assert unrelated.returncode != 0


def test_nightly_e2e_includes_swift_datastream() -> None:
    """Verify ci-e2e.yml contains the swift-datastream-app-e2e job."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    assert "swift-datastream-app-e2e:" in workflow


def test_swift_datastream_job_runs_on_macos() -> None:
    """Verify the datastream job uses runs-on: macos-15."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    assert "runs-on: macos-15" in ds_job


def test_swift_datastream_job_does_not_download_loran_artifact() -> None:
    """Verify the datastream job builds loran from the in-tree source path."""
    workflow = CI_E2E_WORKFLOW.read_text(encoding="utf-8")
    ds_job = _job(workflow, "swift-datastream-app-e2e", "python-web-e2e")
    assert "download-loran-artifact.sh" not in ds_job


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
    assert "__PUBLISHER__/DuplexStreamService@1.0.0" in actr_toml
    assert 'ActrType(publisher: publisher, name: "DataStreamApp", version: "0.1.0")' in actr_service
    assert 'let expectedLines = (1...count).map { "received: echo: hello \\($0)" }' in actr_service


def test_service_readiness_waits_for_exact_registration() -> None:
    with tempfile.TemporaryDirectory() as temp_dir:
        db_path = Path(temp_dir) / "signaling_cache.db"
        _create_actor_registry(db_path)

        script = """
            source "$1"
            (
                sleep 0.2
                sqlite3 "$2" \
                    "INSERT INTO actor_registry VALUES \
                    ('e2e@e2e.test', 'actrium', 'EchoService', 'actrium:EchoService', 'Available', 1);"
            ) &
            SERVICE_READY_POLL_INTERVAL_SECONDS=0.1 \
                wait_for_service_registration "$2" e2e@e2e.test actrium EchoService 2
        """
        result = subprocess.run(
            ["bash", "-c", script, "bash", str(SWIFT_E2E_READINESS), str(db_path)],
            check=False,
            capture_output=True,
            text=True,
        )

        assert result.returncode == 0, result.stderr


if __name__ == "__main__":
    test_gate_aggregates_webrtc_domain()
    test_macos_host_checks_are_affected_only_and_gate_failures()
    test_gate_enforces_anonymous_commit_identities()
    test_gate_aggregates_kotlin_native_domain()
    test_rust_gate_avoids_slow_workspace_tests_and_unused_prewarm()
    test_test_partition_guard_rejects_missing_and_overlapping_tests()
    test_rust_test_gate_restores_cache_before_installing_tools()
    test_msrv_gate_builds_full_workspace_on_pinned_toolchain()
    test_pr_gate_excludes_heavy_root_e2e_jobs()
    test_gha_cache_consumers_always_initialize_credentials()
    test_cli_web_runtime_cache_requires_complete_build_output()
    test_cli_web_assets_installs_pinned_wasm_toolchain()
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
    test_only_release_prepare_can_write_pull_requests()
    test_pull_request_permission_scanner_handles_supported_syntax()
    test_release_train_reuses_actrsharp_workflow()
    test_csharp_pr_gate_uses_light_fixture_and_nightly_keeps_full_matrix()
    test_release_train_forwards_release_context()
    test_release_train_supports_generic_maintenance_branches()
    test_swift_echoapp_e2e_job_present()
    test_e2e_loran_uses_in_tree_install_instead_of_artifact_download()
    test_e2e_no_private_loran_checkout()
    test_e2e_no_inline_archive_download_url()
    test_e2e_linux_deps_do_not_include_unused_unzip()
    test_e2e_linux_job_has_no_ios_rust_targets()
    test_e2e_workspace_prebuilds_use_locked_cargo_resolution()
    test_swift_e2e_upload_artifact_on_failure()
    test_swift_e2e_scripts_capture_app_diagnostics()
    test_swift_e2e_uploads_debug_symbols_separately()
    test_e2e_no_call_remote_in_ffi()
    test_no_loran_release_train_artifact_download_script()
    test_run_sh_uses_correct_signaling_cache_table()
    test_swift_e2e_only_shuts_down_its_selected_simulator()
    test_swift_e2e_covers_short_and_long_background_recovery()
    test_swift_e2e_delegates_recovery_to_framework()
    test_service_readiness_rejects_missing_or_unrelated_registration()
    test_service_readiness_waits_for_exact_registration()
    test_nightly_e2e_includes_swift_datastream()
    test_swift_datastream_job_runs_on_macos()
    test_swift_datastream_job_does_not_download_loran_artifact()
    test_swift_datastream_job_builds_xcframework()
    test_swift_datastream_timeout_not_less_than_240()
    test_swift_datastream_job_not_in_pr_gate()
    test_swift_datastream_cleanup_only_removes_owned_simulator()
    test_datastream_cli_templates_use_empty_scaffolds()
    test_swift_datastream_e2e_builds_custom_proto_flow_from_empty()
