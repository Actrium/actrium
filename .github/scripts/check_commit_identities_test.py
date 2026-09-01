#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_commit_identities.py")
SPEC = importlib.util.spec_from_file_location("check_commit_identities", SCRIPT)
assert SPEC and SPEC.loader
identity_check = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = identity_check
SPEC.loader.exec_module(identity_check)


class CommitIdentityTest(unittest.TestCase):
    def test_user_noreply_identity_is_allowed(self) -> None:
        self.assertTrue(
            identity_check.is_allowed_identity(
                "kookyleo", "87705880+kookyleo@users.noreply.github.com"
            )
        )
        self.assertTrue(
            identity_check.is_allowed_identity(
                "Camel9527", "Camel9527@users.noreply.github.com"
            )
        )

    def test_anonymous_root_identity_is_allowed(self) -> None:
        self.assertTrue(identity_check.is_allowed_identity("root", ""))
        self.assertFalse(
            identity_check.is_allowed_identity("GitHub", "noreply@github.com")
        )

    def test_personal_and_blank_emails_are_rejected(self) -> None:
        self.assertFalse(identity_check.is_allowed_identity("Developer", "dev@example.com"))
        self.assertFalse(identity_check.is_allowed_identity("Developer", ""))

    def test_message_email_privacy(self) -> None:
        allowed = identity_check.CommitIdentity(
            sha="a" * 40,
            author_name="Developer",
            author_email="developer@users.noreply.github.com",
            committer_name="Developer",
            committer_email="developer@users.noreply.github.com",
            parent_count=1,
            message="Co-authored-by: Peer <123+peer@users.noreply.github.com>",
        )
        exposed = identity_check.CommitIdentity(
            sha="b" * 40,
            author_name=allowed.author_name,
            author_email=allowed.author_email,
            committer_name=allowed.committer_name,
            committer_email=allowed.committer_email,
            parent_count=allowed.parent_count,
            message="Signed-off-by: Developer <dev@example.com>",
        )
        self.assertEqual(identity_check.validate_identity(allowed), [])
        self.assertEqual(len(identity_check.validate_identity(exposed)), 1)

    def test_github_service_identity_is_limited_to_merge_commits(self) -> None:
        base = dict(
            sha="c" * 40,
            author_name="Developer",
            author_email="developer@users.noreply.github.com",
            committer_name="GitHub",
            committer_email="noreply@github.com",
            message="Merge pull request #1",
        )
        merge = identity_check.CommitIdentity(parent_count=2, **base)
        ordinary = identity_check.CommitIdentity(parent_count=1, **base)
        wrong_name = identity_check.CommitIdentity(
            parent_count=2,
            **{**base, "committer_name": "Developer"},
        )

        self.assertEqual(identity_check.validate_identity(merge), [])
        self.assertEqual(len(identity_check.validate_identity(ordinary)), 1)
        self.assertEqual(len(identity_check.validate_identity(wrong_name)), 1)

    def test_only_commits_after_base_are_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            old_sha = self._commit(repo, "old@example.com", "old")
            new_sha = self._commit(
                repo,
                "101+developer@users.noreply.github.com",
                "new",
            )

            commits = identity_check.commit_range(repo, old_sha, new_sha)
            self.assertEqual(commits, [new_sha])
            self.assertEqual(
                identity_check.validate_identity(
                    identity_check.read_identity(repo, new_sha)
                ),
                [],
            )

    def test_unadvertised_force_push_base_is_fetched_by_object_id(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            remote = root / "remote.git"
            writer = root / "writer"
            checker = root / "checker"
            subprocess.run(["git", "init", "-q", "--bare", str(remote)], check=True)
            subprocess.run(
                ["git", "-C", str(remote), "config", "uploadpack.allowAnySHA1InWant", "true"],
                check=True,
            )
            subprocess.run(["git", "init", "-q", str(writer)], check=True)
            subprocess.run(
                ["git", "-C", str(writer), "remote", "add", "origin", str(remote)],
                check=True,
            )
            old_sha = self._commit(
                writer,
                "101+developer@users.noreply.github.com",
                "old history",
            )
            subprocess.run(
                ["git", "-C", str(writer), "push", "-q", "origin", "HEAD:main"],
                check=True,
            )
            subprocess.run(
                ["git", "-C", str(writer), "checkout", "-q", "--orphan", "replacement"],
                check=True,
            )
            new_sha = self._commit(
                writer,
                "101+developer@users.noreply.github.com",
                "replacement history",
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(writer),
                    "push",
                    "-q",
                    "--force",
                    "origin",
                    "HEAD:main",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "clone",
                    "-q",
                    "--no-local",
                    "--branch",
                    "main",
                    str(remote),
                    str(checker),
                ],
                check=True,
            )

            with self.assertRaises(subprocess.CalledProcessError):
                identity_check.run_git(checker, "rev-parse", f"{old_sha}^{{commit}}")
            self.assertEqual(identity_check.commit_range(checker, old_sha, new_sha), [new_sha])

    @staticmethod
    def _commit(repo: Path, email: str, message: str) -> str:
        env = os.environ.copy()
        env.update(
            {
                "GIT_AUTHOR_NAME": "Developer",
                "GIT_AUTHOR_EMAIL": email,
                "GIT_COMMITTER_NAME": "Developer",
                "GIT_COMMITTER_EMAIL": email,
            }
        )
        subprocess.run(
            ["git", "-C", str(repo), "commit", "-q", "--allow-empty", "-m", message],
            check=True,
            env=env,
        )
        return subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        ).stdout.strip()


if __name__ == "__main__":
    unittest.main()
