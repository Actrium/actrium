#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("check_commit_identities.py")
SPEC = importlib.util.spec_from_file_location("check_commit_identities", SCRIPT)
assert SPEC and SPEC.loader
identity_check = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = identity_check
SPEC.loader.exec_module(identity_check)

MEMBER_EMAIL = "128583280+kaito-harry@users.noreply.github.com"


def make_commit(sha_letter: str, **overrides: object) -> identity_check.CommitIdentity:
    fields: dict[str, object] = dict(
        sha=sha_letter * 40,
        author_name="kaito-harry",
        author_email=MEMBER_EMAIL,
        committer_name="kaito-harry",
        committer_email=MEMBER_EMAIL,
        parent_count=1,
        message="feat: example change",
    )
    fields.update(overrides)
    return identity_check.CommitIdentity(**fields)  # type: ignore[arg-type]


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
        allowed = make_commit(
            "a",
            message="Co-authored-by: alinejun <28826657+alinejun@users.noreply.github.com>",
        )
        exposed = make_commit(
            "b",
            message="Signed-off-by: Developer <dev@example.com>",
        )
        self.assertEqual(identity_check.validate_identity(allowed), [])
        self.assertEqual(len(identity_check.validate_identity(exposed)), 1)

    def test_github_service_identity_does_not_require_multiple_parents(self) -> None:
        base = dict(
            author_name="kaito-harry",
            author_email=MEMBER_EMAIL,
            committer_name="GitHub",
            committer_email="noreply@github.com",
            message="Merge pull request #1",
        )
        merge = identity_check.CommitIdentity(parent_count=2, sha="c" * 40, **base)
        squash = identity_check.CommitIdentity(
            parent_count=1,
            sha="c" * 40,
            **{**base, "message": "chore(release): basic train v0.10.0"},
        )
        wrong_name = identity_check.CommitIdentity(
            parent_count=2,
            sha="c" * 40,
            **{**base, "committer_name": "kaito-harry"},
        )
        wrong_email = identity_check.CommitIdentity(
            parent_count=1,
            sha="c" * 40,
            **{**base, "committer_email": "noreply@example.com"},
        )

        self.assertEqual(identity_check.validate_identity(merge), [])
        self.assertEqual(identity_check.validate_identity(squash), [])
        self.assertEqual(len(identity_check.validate_identity(wrong_name)), 1)
        self.assertEqual(len(identity_check.validate_identity(wrong_email)), 1)

    def test_member_noreply_author_and_committer_pass(self) -> None:
        commit = make_commit("d")
        self.assertEqual(identity_check.validate_identity(commit), [])

    def test_legacy_noreply_spelling_of_member_passes(self) -> None:
        commit = make_commit(
            "e",
            author_name="zhanghongjun",
            author_email="zhanghongjun@users.noreply.github.com",
            committer_name="zhanghongjun",
            committer_email="zhanghongjun@users.noreply.github.com",
        )
        self.assertEqual(identity_check.validate_identity(commit), [])

    def test_foreign_noreply_author_is_rejected(self) -> None:
        # Regression test for #222: an OpenClaw-authored commit reached main
        # because the privacy regex accepts any users.noreply address.
        commit = make_commit(
            "f",
            author_name="OpenClaw",
            author_email="openclaw@users.noreply.github.com",
            committer_name="OpenClaw",
            committer_email="openclaw@users.noreply.github.com",
        )
        errors = identity_check.validate_identity(commit)
        self.assertEqual(len(errors), 2)
        self.assertIn("author noreply login 'openclaw'", errors[0])
        self.assertIn("committer noreply login 'openclaw'", errors[1])

    def test_attribution_layer_can_be_disabled(self) -> None:
        commit = make_commit(
            "g",
            author_email="openclaw@users.noreply.github.com",
            committer_email="openclaw@users.noreply.github.com",
        )
        self.assertEqual(
            identity_check.validate_identity(commit, check_attribution=False), []
        )

    def test_member_co_author_trailer_passes(self) -> None:
        commit = make_commit(
            "h",
            message=(
                "feat: example change\n\n"
                "Co-authored-by: alinejun <28826657+alinejun@users.noreply.github.com>\n"
            ),
        )
        self.assertEqual(identity_check.validate_identity(commit), [])

    def test_service_bot_co_author_trailer_passes(self) -> None:
        commit = make_commit(
            "i",
            message=(
                "chore: automated bump\n\n"
                "Co-authored-by: github-actions[bot] "
                "<41898282+github-actions[bot]@users.noreply.github.com>\n"
            ),
        )
        self.assertEqual(identity_check.validate_identity(commit), [])

    def test_foreign_co_author_trailer_is_rejected(self) -> None:
        commit = make_commit(
            "j",
            message=(
                "feat: example change\n\n"
                "Co-authored-by: OpenClaw <openclaw@users.noreply.github.com>\n"
            ),
        )
        errors = identity_check.validate_identity(commit)
        self.assertEqual(len(errors), 1)
        self.assertIn("co-author noreply login 'openclaw'", errors[0])

    def test_assistant_login_passes_only_when_allowlisted(self) -> None:
        commit = make_commit(
            "k",
            message=(
                "feat: example change\n\n"
                "Co-authored-by: Claude <123456+claude@users.noreply.github.com>\n"
            ),
        )
        self.assertEqual(len(identity_check.validate_identity(commit)), 1)

        allowed = identity_check.ALLOWED_ASSISTANT_LOGINS | {"claude"}
        with mock.patch.object(identity_check, "ALLOWED_ASSISTANT_LOGINS", allowed):
            self.assertEqual(identity_check.validate_identity(commit), [])

    def test_trailer_without_email_is_ignored_by_attribution(self) -> None:
        commit = make_commit(
            "l",
            message="feat: example change\n\nCo-authored-by: Claude\n",
        )
        self.assertEqual(identity_check.validate_identity(commit), [])

    def test_non_noreply_trailer_email_keeps_privacy_error(self) -> None:
        commit = make_commit(
            "m",
            message=(
                "feat: example change\n\n"
                "Co-authored-by: Claude <noreply@anthropic.com>\n"
            ),
        )
        errors = identity_check.validate_identity(commit)
        self.assertEqual(len(errors), 1)
        self.assertIn("non-anonymous email", errors[0])

    def test_single_parent_github_service_committer_passes_end_to_end(self) -> None:
        # Regression test for #216: a squash merge carries the GitHub service
        # committer on a single-parent commit, with or without a PR suffix or
        # release prefix in the subject.
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            base_sha = self._commit(
                repo,
                MEMBER_EMAIL,
                "base",
            )
            squash_sha = self._commit(
                repo,
                MEMBER_EMAIL,
                "chore(release): basic train v0.10.0",
                committer_name="GitHub",
                committer_email="noreply@github.com",
            )
            pr_suffixed_sha = self._commit(
                repo,
                MEMBER_EMAIL,
                "fix(ci): stabilize protocol audits (#202)",
                committer_name="GitHub",
                committer_email="noreply@github.com",
            )

            commits = identity_check.commit_range(repo, base_sha, pr_suffixed_sha)
            self.assertEqual(commits, [squash_sha, pr_suffixed_sha])
            for sha in commits:
                self.assertEqual(
                    identity_check.validate_identity(
                        identity_check.read_identity(repo, sha)
                    ),
                    [],
                )

    def test_foreign_author_fails_end_to_end(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            base_sha = self._commit(repo, MEMBER_EMAIL, "base")
            foreign_sha = self._commit(
                repo,
                "openclaw@users.noreply.github.com",
                "fix: drafted elsewhere",
                author_name="OpenClaw",
            )

            commits = identity_check.commit_range(repo, base_sha, foreign_sha)
            self.assertEqual(commits, [foreign_sha])
            errors = identity_check.validate_identity(
                identity_check.read_identity(repo, foreign_sha)
            )
            self.assertEqual(len(errors), 2)

    def test_only_commits_after_base_are_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            old_sha = self._commit(repo, "old@example.com", "old")
            new_sha = self._commit(
                repo,
                MEMBER_EMAIL,
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
                MEMBER_EMAIL,
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
                MEMBER_EMAIL,
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
    def _commit(
        repo: Path,
        email: str,
        message: str,
        committer_name: str = "kaito-harry",
        committer_email: str = "",
        author_name: str = "kaito-harry",
    ) -> str:
        env = os.environ.copy()
        env.update(
            {
                "GIT_AUTHOR_NAME": author_name,
                "GIT_AUTHOR_EMAIL": email,
                "GIT_COMMITTER_NAME": committer_name,
                "GIT_COMMITTER_EMAIL": committer_email or email,
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
