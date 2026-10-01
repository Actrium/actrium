#!/usr/bin/env python3
"""Tests for check_ios_jsbundle_name.py."""

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_ios_jsbundle_name.py")
SPEC = importlib.util.spec_from_file_location("jsbundle_name_audit", SCRIPT)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class IosJsbundleNameTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "ios" / "App").mkdir(parents=True)
        (self.root / "ios" / "Scripts").mkdir(parents=True)

    def write_loader(self, name, file="ios/App/Host.swift"):
        (self.root / file).write_text(
            f'        return Bundle.main.url(forResource: "{name}", '
            'withExtension: "jsbundle")\n'
        )

    def write_producer(self, name, file="ios/Scripts/build.sh", raw=None):
        output = raw if raw is not None else name + ".jsbundle"
        (self.root / file).write_text(
            f'node bundle.js --bundle-output "$OUT/{output}" \\\n'
        )

    def run_check(self, expected=0):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root)],
            text=True, capture_output=True,
        )
        self.assertEqual(
            result.returncode, expected,
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )
        return result.stderr

    def test_matching_names_pass(self):
        self.write_loader("runtime")
        self.write_producer("runtime")
        self.write_producer("runtime", file="ios/Scripts/upload.sh")
        self.run_check(expected=0)

    def test_issue_318_loader_producer_drift_fails(self):
        # The shipped bug: loader asked for actr.jsbundle while scripts
        # produced runtime.jsbundle.
        self.write_loader("actr")
        self.write_producer("runtime")
        stderr = self.run_check(expected=1)
        self.assertIn("disagree", stderr)

    def test_producers_disagreeing_fails(self):
        self.write_loader("runtime")
        self.write_producer("runtime")
        self.write_producer("actr", file="ios/Scripts/upload.sh")
        self.run_check(expected=1)

    def test_missing_loader_fails(self):
        self.write_producer("runtime")
        self.run_check(expected=1)

    def test_missing_producer_fails(self):
        self.write_loader("runtime")
        self.run_check(expected=1)

    def test_non_jsbundle_output_fails(self):
        self.write_loader("runtime")
        self.write_producer("runtime", raw="runtime.bundle")
        self.run_check(expected=1)

    def test_comment_lines_are_ignored(self):
        self.write_loader("runtime")
        self.write_producer("runtime")
        (self.root / "ios" / "Scripts" / "upload.sh").write_text(
            '# --bundle-output Output To Root directory, matching bundleURL\n'
        )
        self.run_check(expected=0)

    def test_extraction_helpers_on_fixture(self):
        self.write_loader("runtime")
        self.write_producer("runtime")
        loaders = AUDIT.loader_names(self.root / "ios")
        producers, problems = AUDIT.producer_names(self.root / "ios" / "Scripts")
        self.assertEqual([name for name, _ in loaders], ["runtime"])
        self.assertEqual([name for name, _ in producers], ["runtime"])
        self.assertEqual(problems, [])


if __name__ == "__main__":
    unittest.main()
