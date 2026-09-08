#!/usr/bin/env python3
"""Regression checks for deterministic, history-aware protocol auditing."""

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_retired_protocol_names.py")
SPEC = importlib.util.spec_from_file_location("protocol_audit", SCRIPT)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
TOKEN = b"as" + b"ks"


class ProtocolAuditTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.repo = Path(self.directory.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Protocol Audit")
        self.git("config", "user.email", "audit@users.noreply.github.com")
        self.git("config", "commit.gpgsign", "false")
        self.git("commit", "-q", "--allow-empty", "-m", "initial")

    def git(self, *args, data=None):
        return subprocess.check_output(["git", *args], cwd=self.repo, input=data)

    def commit_file(self, name, data):
        (self.repo / name).write_bytes(data)
        self.git("add", "--", name)
        self.git("commit", "-q", "-m", "fixture")

    def audit(self, *args, expected=0, cwd=None):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), *args], cwd=cwd or self.repo,
            text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result.stderr

    def test_unrelated_branch_and_dangling_blob_do_not_affect_head(self):
        self.git("checkout", "-q", "-b", "other-pr")
        self.commit_file("other.txt", b"comment " + TOKEN + b" for work\n")
        self.git("checkout", "-q", "main")
        self.git("hash-object", "-w", "--stdin", data=TOKEN)
        self.audit()
        diagnostic = self.audit("--all-refs", expected=1)
        self.assertIn("other.txt", diagnostic)
        self.assertIn("blob", diagnostic)

    def test_removed_content_is_still_rejected_in_head_history(self):
        self.commit_file("old.txt", TOKEN + b"\n")
        self.git("rm", "-q", "old.txt")
        self.git("commit", "-q", "-m", "remove fixture")
        diagnostic = self.audit(expected=1)
        self.assertIn("old.txt", diagnostic)
        self.assertIn(":1", diagnostic)

    def test_removed_path_is_still_rejected_in_head_history(self):
        name = TOKEN.decode() + ".txt"
        self.commit_file(name, b"clean\n")
        self.git("rm", "-q", name)
        self.git("commit", "-q", "-m", "remove fixture")
        self.assertIn("tree", self.audit(expected=1))

    def test_commit_message_is_rejected(self):
        self.git("commit", "-q", "--allow-empty", "-m", TOKEN.decode())
        self.assertIn("commit", self.audit(expected=1))

    def test_current_tracked_content_and_untracked_path_are_rejected(self):
        self.commit_file("current.txt", b"clean\n")
        (self.repo / "current.txt").write_bytes(b"line one\n" + TOKEN)
        self.assertIn("current.txt':2", self.audit(expected=1))
        (self.repo / "current.txt").write_bytes(b"clean\n")
        (self.repo / TOKEN.decode()).touch()
        self.assertIn("path", self.audit(expected=1))

    def test_binary_blob_and_identifier_substring_are_not_rejected(self):
        self.commit_file("binary.bin", b"\0 " + TOKEN + b"\0")
        self.commit_file("text.txt", b"t" + TOKEN + b"\n")
        self.audit()

    def test_tree_hash_bytes_are_not_scanned_as_names(self):
        failures = []
        raw_hash = b"\0" + TOKEN + b"\0" * 15
        AUDIT.check_tree(b"100644 clean.txt\0" + raw_hash, "fixture", 20, failures)
        self.assertEqual(failures, [])
        AUDIT.check_tree(b"100644 " + TOKEN + b"\0" + raw_hash, "fixture", 20, failures)
        self.assertEqual(len(failures), 1)

    def test_shallow_history_is_rejected(self):
        with tempfile.TemporaryDirectory() as target:
            clone = Path(target) / "clone"
            self.git("clone", "-q", "--depth=1", self.repo.as_uri(), str(clone))
            self.assertIn("full history", self.audit(expected=1, cwd=clone))


if __name__ == "__main__":
    unittest.main()
