#!/usr/bin/env python3
"""Package a release dependency graph without requiring unpublished crates online."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib


def run_cargo(repo: Path, arguments: list[str], **kwargs):
    # Honor Cargo configuration and the caller's environment directly.
    return subprocess.run(
        ["cargo", *arguments],
        cwd=repo,
        check=True,
        **kwargs,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    options = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    offline = ["--offline"] if options.offline else []
    host = next(line.split(": ", 1)[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines() if line.startswith("host: "))
    metadata = json.loads(run_cargo(repo, ["metadata", "--locked", "--format-version", "1", "--filter-platform", host, *offline], capture_output=True, text=True).stdout)
    manifests = {Path(package["manifest_path"]) for package in metadata["packages"] if package["source"] is None}
    # This optional dependency belongs to the separate browser workspace.
    manifests.add(repo / "packages/actr-web/crates/actr-web-abi/Cargo.toml")
    release_script = (repo / "scripts/actr/release-train.sh").read_text()
    packages = []
    for group in ["FOUNDATION_CRATES", "PROTOC_CRATES", "SDK_CRATES", "CLI_CRATES", "LORAN_CRATES"]:
        block = re.search(r"readonly " + group + r"=\((.*?)\n\)", release_script, re.S).group(1)
        packages.extend(re.findall(r'^\s*"([^"\n]+)"', block, re.M))
    options.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="actr-package-graph-") as directory:
        staging = Path(directory)
        browser_package = tomllib.loads((repo / "packages/actr-web/Cargo.toml").read_text())["workspace"]["package"]
        for manifest in manifests:
            source = manifest.parent
            destination = staging / source.relative_to(repo)
            destination.mkdir(parents=True, exist_ok=True)
            shutil.copy2(manifest, destination / "Cargo.toml")
            if manifest.parent.name == "actr-web-abi":
                text = manifest.read_text()
                for key, value in browser_package.items():
                    text = re.sub(r"(?m)^" + re.escape(key) + r"\.workspace = true$", lambda _: f"{key} = {json.dumps(value)}", text)
                (destination / "Cargo.toml").write_text(text)
            for item in source.iterdir():
                if item.name in {"Cargo.toml", "Cargo.lock", "target", "node_modules", "dist", ".git"}:
                    continue
                (destination / item.name).symlink_to(item, target_is_directory=item.is_dir())
        workspace = (repo / "Cargo.toml").read_text()
        members = sorted(manifest.parent.relative_to(repo).as_posix() for manifest in manifests)
        workspace = re.sub(r"(?ms)^members = \[.*?^\]", "members = " + json.dumps(members, indent=4), workspace, count=1)
        workspace = re.sub(r"(?ms)^exclude = \[.*?^\]", "exclude = []", workspace, count=1)
        browser = (repo / "packages/actr-web/Cargo.toml").read_text()
        bindings = [re.search(r"(?m)^" + re.escape(name) + r" = .+$", browser).group() for name in ["wasm-bindgen", "wasm-bindgen-futures", "js-sys", "serde-wasm-bindgen"]]
        workspace = workspace.replace("[workspace.dependencies]", "[workspace.dependencies]\n" + "\n".join(bindings), 1)
        (staging / "Cargo.toml").write_text(workspace)
        shutil.copy2(repo / "Cargo.lock", staging / "Cargo.lock")
        arguments = ["package", "--manifest-path", str(staging / "Cargo.toml"), "--allow-dirty", "--no-verify", *offline]
        for package in packages:
            arguments.extend(["-p", package])
        # Cargo packages interdependent members together using a temporary registry.
        # The sources, versions and features remain those of the real checkout.
        run_cargo(repo, arguments)
        package_metadata = json.loads(run_cargo(repo, ["metadata", "--manifest-path", str(staging / "Cargo.toml"), "--no-deps", "--format-version", "1", *offline], capture_output=True, text=True).stdout)
        target = Path(package_metadata["target_directory"])
        version = tomllib.loads((repo / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        for package in packages:
            artifact = target / "package" / f"{package}-{version}.crate"
            if not artifact.is_file():
                raise RuntimeError(f"Missing packaged artifact: {artifact}")
            shutil.copy2(artifact, options.output / artifact.name)
        (options.output / "manifest.json").write_text(json.dumps({"version": version, "packages": packages, "build_verified": False}, indent=2) + "\n")
        print(f"Packaged {len(packages)} release crates into {options.output}")


if __name__ == "__main__":
    main()
