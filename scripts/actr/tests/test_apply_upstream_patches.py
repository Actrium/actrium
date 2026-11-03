#!/usr/bin/env python3
"""Check that registry consumers can apply upstream overrides safely."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "apply-upstream-patches.py"
spec = importlib.util.spec_from_file_location("apply_upstream_patches", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ApplyUpstreamPatchesTest(unittest.TestCase):
    def test_source_consumer_resolves_without_workspace_overrides(self):
        with tempfile.TemporaryDirectory(prefix="actr-source-consumer-") as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/main.rs").write_text("fn main() {}\n")
            manifest = root / "Cargo.toml"
            manifest.write_text(f'''[package]
name = "actr-source-consumer-check"
version = "0.1.0"
edition = "2024"

[workspace]

[dependencies]
actr-hyper = {{ path = {json.dumps((module.REPO / 'crates/actr-hyper').as_posix())}, features = ["test-utils"] }}
actrix-stun = {{ path = {json.dumps((module.REPO / 'crates/loran-stun').as_posix())} }}
actrix-turn = {{ path = {json.dumps((module.REPO / 'crates/loran-turn').as_posix())} }}
''')
            shutil.copyfile(module.REPO / "Cargo.lock", root / "Cargo.lock")
            subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(manifest)],
                stdout=subprocess.DEVNULL,
                check=True,
            )
            subprocess.run(
                [sys.executable, str(module.REPO / "scripts/actr/check-upstream-patches.py"), "--manifest", str(manifest)],
                check=True,
            )

    def test_generated_overrides_follow_workspace_dependencies(self):
        patches = tomllib.loads(module.PATCH_SOURCE.read_text())["patch"]["crates-io"]
        dependencies = tomllib.loads((module.REPO / "Cargo.toml").read_text())["workspace"]["dependencies"]
        for package, dependency in (
            ("webrtc", "webrtc"), ("rtc", "rtc"), ("rtc-turn", "rtc"),
            ("rtc-rtp", "rtc-rtp"), ("turn", "turn_crate"),
            ("stun", "webrtc-stun"), ("webrtc-util", "webrtc-util"),
        ):
            with self.subTest(package=package):
                self.assertEqual(patches[package]["git"], dependencies[dependency]["git"])
                self.assertEqual(patches[package]["rev"], dependencies[dependency]["rev"])

    def test_merges_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "Cargo.toml"
            manifest.write_text('[package]\nname = "demo"\nversion = "0.1.0"\n\n[patch.crates-io]\nactr = { path = "../actr" }\n')
            module.apply(module.PATCH_SOURCE, manifest)
            updated = manifest.read_text()
            parsed = tomllib.loads(updated)
            self.assertEqual(parsed["patch"]["crates-io"]["actr"]["path"], "../actr")
            source = tomllib.loads(module.PATCH_SOURCE.read_text())["patch"]
            for registry, entries in source.items():
                for name, expected in entries.items():
                    self.assertEqual(parsed["patch"][registry][name], expected)
            module.apply(module.PATCH_SOURCE, manifest)
            self.assertEqual(manifest.read_text(), updated)

    def test_rejects_conflicting_patch_without_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "Cargo.toml"
            original = '[package]\nname = "demo"\nversion = "0.1.0"\n\n[patch.crates-io]\nturn = { path = "../other" }\n'
            manifest.write_text(original)
            with self.assertRaisesRegex(ValueError, "conflicting patch for turn"):
                module.apply(module.PATCH_SOURCE, manifest)
            self.assertEqual(manifest.read_text(), original)


if __name__ == "__main__":
    unittest.main()
