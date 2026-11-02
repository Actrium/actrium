#!/usr/bin/env python3
"""Exercise release packaging with Cargo's resolved artifact directory."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("package_release_crates", Path(__file__).resolve().parents[1] / "package-release-crates.py")
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class PackageReleaseCratesTests(unittest.TestCase):
    def check_artifact_directory(self, environment, configured_target):
        with tempfile.TemporaryDirectory(prefix="actr-package-test-") as directory:
            repo = Path(directory).resolve()
            crate = repo / "crates/example"
            abi = repo / "packages/actr-web/crates/actr-web-abi"
            for path in [crate, abi, repo / "scripts/actr"]:
                path.mkdir(parents=True)
            (repo / "Cargo.toml").write_text('[workspace]\nmembers = [\n]\nexclude = [\n]\n[workspace.package]\nversion = "0.10.5"\nauthors = ["Root Author"]\n[workspace.dependencies]\n')
            (repo / "Cargo.lock").write_text("version = 4\n")
            (crate / "Cargo.toml").write_text('[package]\nname = "example"\nversion.workspace = true\n')
            (crate / "src").mkdir()
            (crate / "src/lib.rs").write_text("pub fn example() {}\n")
            (abi / "Cargo.toml").write_text('[package]\nname = "actr-web-abi"\nversion.workspace = true\nauthors.workspace = true\n')
            (abi.parents[1] / "Cargo.toml").write_text('[workspace.package]\nversion = "0.10.5"\nauthors = ["Browser Author"]\n[workspace.dependencies]\nwasm-bindgen = "0.2"\nwasm-bindgen-futures = "0.4"\njs-sys = "0.3"\nserde-wasm-bindgen = "0.6"\n')
            groups = ["FOUNDATION_CRATES", "PROTOC_CRATES", "SDK_CRATES", "CLI_CRATES", "ACTRIX_CRATES"]
            (repo / "scripts/actr/release-train.sh").write_text("".join(f'readonly {group}=(\n' + ('  "example"\n' if index == 0 else "") + ")\n" for index, group in enumerate(groups)))
            stale = repo / "target/package/example-0.10.5.crate"
            stale.parent.mkdir(parents=True)
            stale.write_bytes(b"stale artifact")

            def fake_cargo(cwd, arguments, **kwargs):
                self.assertEqual(cwd, repo)
                if "--manifest-path" not in arguments:
                    return subprocess.CompletedProcess(arguments, 0, json.dumps({"packages": [{"source": None, "manifest_path": str(crate / "Cargo.toml") }]}))
                staging = Path(arguments[arguments.index("--manifest-path") + 1]).parent
                target = repo / "configured-build" if configured_target else staging / "target"
                if arguments[0] == "metadata":
                    return subprocess.CompletedProcess(arguments, 0, json.dumps({"target_directory": str(target)}))
                self.assertEqual(arguments[0], "package")
                manifest = tomllib.loads((staging / abi.relative_to(repo) / "Cargo.toml").read_text())
                self.assertEqual(manifest["package"]["authors"], ["Browser Author"])
                artifact = target / "package/example-0.10.5.crate"
                artifact.parent.mkdir(parents=True)
                artifact.write_bytes(b"current artifact")
                return subprocess.CompletedProcess(arguments, 0)

            output = repo / "artifacts"
            with patch.object(packager, "__file__", str(repo / "scripts/actr/package-release-crates.py")), patch.object(packager, "run_cargo", side_effect=fake_cargo), patch.object(packager.subprocess, "check_output", return_value="host: x86_64-unknown-linux-gnu\n"), patch.dict(os.environ, environment, clear=True), patch.object(sys, "argv", ["package-release-crates.py", "--offline", "--output", str(output)]):
                packager.main()
            self.assertEqual((output / stale.name).read_bytes(), b"current artifact")
            self.assertFalse(json.loads((output / "manifest.json").read_text())["build_verified"])

    def test_default_staging_target_does_not_copy_stale_repository_artifact(self):
        self.check_artifact_directory({}, False)

    def test_resolved_target_overrides_relative_environment_value(self):
        self.check_artifact_directory({"CARGO_TARGET_DIR": "relative-target"}, True)


if __name__ == "__main__":
    unittest.main()
