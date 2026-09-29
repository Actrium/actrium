#!/usr/bin/env python3
"""Verify CLI release-note publishing without contacting GitHub."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]


class PublishNotesTests(unittest.TestCase):
    def test_create_and_retry_use_the_same_generated_body(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(repo), *args], text=True
                ).strip()

            git("init", "-q")
            for subject in ("chore: initial", "fix(runtime): recover calls (#12)"):
                git(
                    "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                    "commit", "-q", "--allow-empty", "-m", subject,
                )
                if subject == "chore: initial":
                    git("tag", "v0.9.0")
            release_body = "## v0.9.1\n\n### Bug Fixes\n\n- Human edited release notes."
            git(
                "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-q", "--allow-empty",
                "-m", "chore(release): basic train v0.9.1",
                "-m", release_body,
            )
            sha = git("rev-parse", "HEAD")
            git("tag", "v0.9.1")
            expected = release_body + "\n"
            script = r'''
source "$CLI_SCRIPT"
curl() {
  local method=GET payload='' url='' arg
  while (($#)); do
    arg=$1; shift
    case "$arg" in
      -X) method=$1; shift ;;
      -d) payload=$1; shift ;;
      https://*) url=$arg ;;
    esac
  done
  if [[ "$method" == GET ]]; then
    if [[ "$EXISTING" == yes ]]; then printf '{"id":7}'; else return 22; fi
  else
    [[ "$method" == "$EXPECTED_METHOD" ]] || return 1
    printf '%s' "$payload"
  fi
}
resolve_release Actrium/actrium v0.9.1 unused true
'''
            for existing, method in (("no", "POST"), ("yes", "PATCH")):
                env = dict(os.environ, ACTR_REPO_ROOT=directory, RELEASE_SHA=sha,
                           RELEASE_BRANCH="main", EXISTING=existing,
                           EXPECTED_METHOD=method,
                           CLI_SCRIPT=str(ROOT / "scripts/actr/release-actr-cli.sh"))
                result = subprocess.run(
                    ["bash", "-c", script], env=env, check=True,
                    capture_output=True, text=True,
                )
                payload = json.loads(result.stdout)
                self.assertEqual(payload["body"], expected)
                self.assertEqual(payload["tag_name"], "v0.9.1")
                self.assertFalse(payload["prerelease"])
                self.assertNotIn("assets", payload)


if __name__ == "__main__":
    unittest.main()
