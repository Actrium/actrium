#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
python3 - "$repo_root" <<'PY'
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

repo = Path(sys.argv[1])
script = (repo / "scripts/actr/release-train.sh").read_text()
groups = ["FOUNDATION_CRATES", "PROTOC_CRATES", "SDK_CRATES", "CLI_CRATES", "LORAN_CRATES"]
order = []
for group in groups:
    block = re.search(r"readonly " + group + r"=\((.*?)\n\)", script, re.S).group(1)
    order.extend(re.findall(r'^\s*"([^"\n]+)"', block, re.M))
assert len(set(order)) == len(order), "duplicate release packages"
positions = {name: index for index, name in enumerate(order)}
new_packages = {"actr-relay-security", "loran-relay-protocol", "loran-relay"}
assert new_packages <= set(order), "relay crates missing from release train"

manifest_paths = []
for pattern in ["crates/*/Cargo.toml", "tools/actr/protoc-gen/*/Cargo.toml", "packages/actr-web/crates/*/Cargo.toml"]:
    manifest_paths.extend(repo.glob(pattern))
packages = {}
for path in manifest_paths:
    manifest = tomllib.loads(path.read_text())
    if "package" in manifest:
        packages[manifest["package"]["name"]] = (path, manifest)


def check_dependencies(root, expected_version=None):
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["dependencies"]
    for name in order:
        original_path, _ = packages[name]
        path = root / original_path.relative_to(repo)
        manifest = tomllib.loads(path.read_text())
        version = manifest["package"]["version"]
        if expected_version and name in new_packages:
            if isinstance(version, dict):
                version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
            assert version == expected_version, (name, version)
        sections = [manifest, *manifest.get("target", {}).values()]
        for section in sections:
            for table in ["dependencies", "build-dependencies"]:
                for alias, dependency in section.get(table, {}).items():
                    if not isinstance(dependency, dict):
                        continue
                    if dependency.get("workspace"):
                        dependency = workspace.get(alias, dependency)
                    if not isinstance(dependency, dict):
                        continue
                    dep_name = dependency.get("package", alias)
                    if dep_name not in positions:
                        continue
                    assert dependency.get("version"), (name, alias, "missing publishable version")
                    assert positions[dep_name] < positions[name], (name, "must follow", dep_name)
                    if expected_version and (dep_name in new_packages or name == "actr-hyper"):
                        assert dependency["version"] == expected_version, (name, alias, dependency["version"])


check_dependencies(repo)
upstream_names = ("webrtc", "rtc", "rtc-rtp", "turn_crate", "webrtc-stun", "stun-protocol", "webrtc-util")
workspace_deps = tomllib.loads((repo / "Cargo.toml").read_text())["workspace"]["dependencies"]
upstream_sources = {name: (workspace_deps[name]["git"], workspace_deps[name]["rev"]) for name in upstream_names}
# Execute the real release version updater against isolated copies, never the checkout.
updater = script.split("update_versions() {", 1)[1].split("<<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
stage_function = script.split("stage_release_version_files() {", 1)[1].split("\n}", 1)[0]
stage_paths = stage_function.removeprefix("\n  git add ").replace("\\\n", "").split()
nested_manifests = [
    "examples/actr/rust/Cargo.toml",
    "examples/actr/audio-capture/rust-receiver/Cargo.toml",
    "crates/actr-hyper/tests/dynclib_actor_fixture/Cargo.toml",
]
with tempfile.TemporaryDirectory(prefix="actr-relay-release-") as directory:
    copied = Path(directory)
    for relative in set(re.findall(r'repo / "([^"]+)"', updater)) | set(stage_paths):
        source = repo / relative
        if source.is_file():
            destination = copied / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
    for name in order:
        source, _ = packages[name]
        destination = copied / source.relative_to(repo)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    version = "0.11.0-review-test"
    # Exercise both the real updater and staging helper. Only Cargo is stubbed;
    # dependency resolution is verified separately with the committed lockfiles.
    helper = copied / "release-train-test.sh"
    helper.write_text(script.replace('main "$@"\n', ''))
    subprocess.run(["git", "init", "-q", str(copied)], check=True)
    subprocess.run(["git", "-C", str(copied), "add", "."], check=True)
    subprocess.run([
        "git", "-C", str(copied), "-c", "user.name=Fixture",
        "-c", "user.email=fixture@example.invalid", "commit", "-qm", "Fixture",
    ], check=True)
    subprocess.run(["bash", "-c", r'''
set -euo pipefail
source "$1"
WORK_REPO_ROOT=$2
VERSION=$3
SKIP_PYTHON=true
cargo() {
  printf '%s\n' "$*" >> "$WORK_REPO_ROOT/cargo-calls"
  if [[ "$1 $2" == "update --workspace" ]]; then
    printf '\n# Refreshed by release train\n' >> "${4%Cargo.toml}Cargo.lock"
  fi
}
cd "$WORK_REPO_ROOT"
update_versions
stage_release_version_files
''', "release-sync-test", str(helper), str(copied), version], check=True)
    check_dependencies(copied, version)
    updated_deps = tomllib.loads((copied / "Cargo.toml").read_text())["workspace"]["dependencies"]
    assert updated_deps["actr-hlc"]["version"] == version
    examples = tomllib.loads((copied / nested_manifests[0]).read_text())["workspace"]
    original_examples = tomllib.loads((repo / nested_manifests[0]).read_text())
    assert examples["package"]["version"] == original_examples["workspace"]["package"]["version"]
    for name in original_examples["patch"]["crates-io"]:
        assert examples["dependencies"][name]["version"] == version, name
    calls = (copied / "cargo-calls").read_text().splitlines()
    staged = subprocess.check_output(["git", "-C", str(copied), "diff", "--cached", "--name-only"], text=True).splitlines()
    for manifest in nested_manifests:
        assert f"update --workspace --manifest-path {manifest}" in calls, manifest
        assert manifest.replace("Cargo.toml", "Cargo.lock") in staged, manifest
    assert nested_manifests[0] in staged
    for name, source in upstream_sources.items():
        assert (updated_deps[name]["git"], updated_deps[name]["rev"]) == source, (name, "release bump changed upstream source")
print("Release dependency order, version bump, nested lock refresh and staging passed.")
PY
