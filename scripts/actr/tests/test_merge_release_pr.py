#!/usr/bin/env python3
"""Verify release merge payloads without contacting GitHub."""

import contextlib
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "merge-release-pr.py"
SPEC = importlib.util.spec_from_file_location("merge_release_pr", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class MergeReleaseTests(unittest.TestCase):
    def setUp(self):
        self.pr = {
            "number": 123, "title": "chore(release): basic train v0.10.1",
            "body": "## v0.10.1\n\n### Bug Fixes\n\n- Reviewed `$(example)` text.\n",
            "headRefName": "release-prepare/v0.10.1", "headRefOid": "a" * 40,
            "baseRefName": "main", "state": "OPEN", "isDraft": False,
        }

    def test_merge_preserves_exact_reviewed_body_and_head(self):
        def merge(command, check):
            self.assertTrue(check)
            self.assertIn("--squash", command)
            self.assertEqual(self.pr["title"], command[command.index("--subject") + 1])
            self.assertEqual(self.pr["headRefOid"], command[command.index("--match-head-commit") + 1])
            body = Path(command[command.index("--body-file") + 1]).read_text()
            self.assertEqual(self.pr["body"], body)

        with patch.object(MODULE, "read_pr", return_value=self.pr), \
             patch.object(MODULE.subprocess, "run", side_effect=merge) as run, \
             patch("sys.argv", [str(SCRIPT), "123", "--merge"]):
            MODULE.main()
        run.assert_called_once()

    def test_preview_does_not_merge(self):
        output = io.StringIO()
        with patch.object(MODULE, "read_pr", return_value=self.pr), \
             patch.object(MODULE.subprocess, "run") as run, \
             patch("sys.argv", [str(SCRIPT), "123"]), contextlib.redirect_stdout(output):
            MODULE.main()
        run.assert_not_called()
        self.assertIn(self.pr["body"], output.getvalue())

    def test_missing_notes_or_mismatched_version_is_rejected(self):
        for body in ("", "## v0.10.1\n", "## v0.10.2\n\n- Fix"):
            with self.subTest(body=body), self.assertRaises(ValueError):
                MODULE.validate_pr(dict(self.pr, body=body))

    def test_changed_pr_does_not_merge(self):
        changed = dict(self.pr, body=self.pr["body"] + "- Another change\n")
        with patch.object(MODULE, "read_pr", side_effect=[self.pr, changed]), \
             patch.object(MODULE.subprocess, "run") as run, \
             patch("sys.argv", [str(SCRIPT), "123", "--merge"]), \
             self.assertRaises(ValueError):
            MODULE.main()
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
