#!/usr/bin/env python3
"""Test the historical service-name exception without permitting runtime drift."""

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKER = Path("scripts/actrium/check-product-names.py")
RFC = Path("rfcs/actr/0014-kubernetes-multi-pod-soak-testing.md")
OLD_SERVICE = "Act" + "rix"


class ProductNamePolicyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="loran-name-policy-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / CHECKER).parent.mkdir(parents=True)
        shutil.copyfile(ROOT / CHECKER, self.root / CHECKER)
        (self.root / CHECKER.parent / "product-name-allowlist.json").write_text(
            json.dumps({"references": {}}), encoding="utf-8"
        )
        self.definition = next(
            line
            for line in (ROOT / RFC).read_text(encoding="utf-8").splitlines()
            if line.startswith("| **Loran** |")
        )
        self.write(RFC, self.definition + "\n")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    def check(self):
        return subprocess.run(
            [sys.executable, str(self.root / CHECKER)],
            cwd=self.root,
            capture_output=True,
            text=True,
            check=False,
        )

    def assert_rejected(self, message):
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(message, result.stderr)

    def test_exact_historical_definition_is_allowed(self):
        self.assertIn(OLD_SERVICE, self.definition)
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_historical_definition_allows_crlf_line_endings(self):
        (self.root / RFC).write_bytes(self.definition.encode("utf-8") + b"\r\n")
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_extra_reference_in_the_same_rfc_is_rejected(self):
        self.write(RFC, self.definition + f"\ncargo build -p {OLD_SERVICE.lower()}\n")
        self.assert_rejected("Retired service name")

    def test_duplicate_historical_definition_is_rejected(self):
        self.write(RFC, (self.definition + "\n") * 2)
        self.assert_rejected("Retired service name")

    def test_definition_in_runtime_source_is_rejected(self):
        self.write(Path("src/runtime.rs"), self.definition + "\n")
        self.assert_rejected("Retired service name")

    def test_runtime_references_are_rejected_in_any_case(self):
        for spelling in (OLD_SERVICE, OLD_SERVICE.lower(), OLD_SERVICE.upper()):
            with self.subTest(spelling=spelling):
                self.write(Path("src/runtime.rs"), f'const SERVICE: &str = "{spelling}";\n')
                self.assert_rejected("Retired service name")

    def test_changed_definition_requires_allowance_update(self):
        self.write(RFC, "The service suite is Loran.\n")
        self.assert_rejected("Update the exact historical service definition allowance")

    def test_retired_service_filename_is_rejected(self):
        self.write(Path(f"{OLD_SERVICE.lower()}.json"), '{}\n')
        self.assert_rejected("Retired product name in path")


if __name__ == "__main__":
    unittest.main()
