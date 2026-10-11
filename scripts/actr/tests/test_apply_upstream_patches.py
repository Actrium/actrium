#!/usr/bin/env python3
"""Check that temporary workspaces apply legacy server overrides safely."""

import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "apply-upstream-patches.py"
spec = importlib.util.spec_from_file_location("apply_upstream_patches", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ApplyUpstreamPatchesTest(unittest.TestCase):
    def test_repository_workspaces_keep_the_same_upstream_sources(self):
        source = tomllib.loads((module.REPO / "Cargo.toml").read_text())["patch"]
        for relative in (
            "packages/actr-typescript/Cargo.toml",
            "examples/actr/rust/Cargo.toml",
            "tests/e2e/actr/Cargo.toml",
            "tests/e2e/actr/winui-echo-app/workload/Cargo.toml",
        ):
            with self.subTest(manifest=relative):
                patches = tomllib.loads((module.REPO / relative).read_text())["patch"]
                for registry, entries in source.items():
                    for name, expected in entries.items():
                        self.assertEqual(patches[registry][name], expected)

    def test_merges_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "Cargo.toml"
            manifest.write_text('[package]\nname = "demo"\nversion = "0.1.0"\n\n[patch.crates-io]\nactr = { path = "../actr" }\n')
            module.apply(module.REPO / "Cargo.toml", manifest)
            updated = manifest.read_text()
            parsed = tomllib.loads(updated)
            self.assertEqual(parsed["patch"]["crates-io"]["actr"]["path"], "../actr")
            source = tomllib.loads((module.REPO / "Cargo.toml").read_text())["patch"]
            for registry, entries in source.items():
                for name, expected in entries.items():
                    self.assertEqual(parsed["patch"][registry][name], expected)
            module.apply(module.REPO / "Cargo.toml", manifest)
            self.assertEqual(manifest.read_text(), updated)

    def test_rejects_conflicting_patch_without_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "Cargo.toml"
            original = '[package]\nname = "demo"\nversion = "0.1.0"\n\n[patch.crates-io]\nturn = { path = "../other" }\n'
            manifest.write_text(original)
            with self.assertRaisesRegex(ValueError, "conflicting patch for turn"):
                module.apply(module.REPO / "Cargo.toml", manifest)
            self.assertEqual(manifest.read_text(), original)


if __name__ == "__main__":
    unittest.main()
