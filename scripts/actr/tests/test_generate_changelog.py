#!/usr/bin/env python3
from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "generate-changelog.py"


def run(command: list[str], cwd: Path, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=check,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


class GenerateChangelogTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp_dir.name)
        run(["git", "init", "-b", "main"], self.repo)
        run(["git", "config", "user.email", "test@example.com"], self.repo)
        run(["git", "config", "user.name", "Test User"], self.repo)

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def commit(self, subject: str, body: str = "") -> str:
        log = self.repo / "log.txt"
        previous = log.read_text() if log.exists() else ""
        log.write_text(previous + subject + "\n")
        run(["git", "add", "log.txt"], self.repo)
        if body:
            run(["git", "commit", "--cleanup=verbatim", "-m", subject, "-m", body], self.repo)
        else:
            run(["git", "commit", "-m", subject], self.repo)
        return run(["git", "rev-parse", "HEAD"], self.repo).stdout.strip()

    def tag(self, name: str) -> None:
        run(["git", "tag", name], self.repo)

    def changelog(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return run(
            [
                "python3",
                str(SCRIPT),
                "--repo",
                str(self.repo),
                "--repository-url",
                "https://github.com/example/project",
                *args,
            ],
            self.repo,
            check=check,
        )

    def test_stable_baseline_skips_prerelease_tags(self) -> None:
        self.commit("feat(core): seed release")
        self.tag("v1.0.0")
        self.commit("feat(cli): add alpha command (#10)")
        self.tag("v1.1.0-alpha.1")
        self.commit("fix(runtime): repair stable path (#11)")

        result = self.changelog("--version", "1.1.0")

        self.assertIn("/compare/v1.0.0...v1.1.0", result.stdout)
        self.assertNotIn("/compare/v1.1.0-alpha.1...v1.1.0", result.stdout)
        self.assertIn("cli: Add alpha command", result.stdout)
        self.assertIn("runtime: Repair stable path", result.stdout)

    def test_prerelease_uses_previous_same_core_prerelease(self) -> None:
        self.commit("feat(core): seed release")
        self.tag("v1.0.0")
        self.commit("feat(cli): add numeric prerelease")
        self.tag("v1.1.0-1")
        self.commit("feat(cli): add alpha command")
        self.tag("v1.1.0-alpha.1")
        self.commit("fix(runtime): repair alpha command")

        result = self.changelog("--version", "1.1.0-alpha.2")

        self.assertIn("/compare/v1.1.0-alpha.1...v1.1.0-alpha.2", result.stdout)
        self.assertIn("runtime: Repair alpha command", result.stdout)
        self.assertNotIn("cli: Add alpha command", result.stdout)
        self.assertNotIn("cli: Add numeric prerelease", result.stdout)

    def test_internal_chore_with_release_word_is_kept_unless_release_scope(self) -> None:
        self.commit("feat(core): seed release")
        self.tag("v1.0.0")
        self.commit("chore(deps): update release tooling")
        self.commit("chore(release): publish v1.1.0")

        result = self.changelog("--version", "1.1.0")

        self.assertIn("deps: Update release tooling", result.stdout)
        self.assertNotIn("Publish v1.1.0", result.stdout)

    def test_maintenance_branch_restricts_stable_baseline_to_same_minor(self) -> None:
        self.commit("feat(core): seed release")
        self.tag("v1.2.0")
        self.commit("feat(core): next minor")
        self.tag("v1.3.0")
        self.commit("fix(runtime): patch old minor")
        self.tag("release-train-v1.2.1")
        self.commit("fix(cli): finish old minor")

        result = self.changelog("--version", "1.2.2", "--branch", "release-1.2")

        self.assertIn("/compare/release-train-v1.2.1...v1.2.2", result.stdout)
        self.assertIn("cli: Finish old minor", result.stdout)
        self.assertNotIn("core: Next minor", result.stdout)

    def test_first_release_has_explicit_no_baseline_summary(self) -> None:
        self.commit("feat(core): first feature")

        result = self.changelog("--version", "0.1.0")

        self.assertIn("Initial release summary.", result.stdout)
        self.assertIn("/releases/tag/v0.1.0", result.stdout)
        self.assertIn("core: First feature", result.stdout)

    def test_target_tag_does_not_compare_against_itself(self) -> None:
        self.commit("feat(core): seed release")
        self.tag("v1.0.0")
        self.commit("fix(cli): tagged fix")
        self.tag("v1.1.0")

        result = self.changelog("--version", "1.1.0", "--to-ref", "v1.1.0")

        self.assertIn("/compare/v1.0.0...v1.1.0", result.stdout)
        self.assertNotIn("/compare/v1.1.0...v1.1.0", result.stdout)
        self.assertIn("cli: Tagged fix", result.stdout)

    def test_stdout_is_default_and_does_not_write_changelog(self) -> None:
        self.commit("feat(core): seed release")
        changelog = self.repo / "CHANGELOG.md"
        changelog.write_text("sentinel\n")

        result = self.changelog("--version", "0.1.0")

        self.assertIn("## v0.1.0", result.stdout)
        self.assertEqual("sentinel\n", changelog.read_text())

    def test_output_and_header_are_optional(self) -> None:
        self.commit("feat(core): seed release")
        header = self.repo / "header.md"
        output = self.repo / "release-notes.md"
        header.write_text("Release header\n")

        result = self.changelog(
            "--version",
            "0.1.0",
            "--header",
            str(header),
            "--output",
            str(output),
        )

        self.assertEqual("", result.stdout)
        self.assertTrue(output.read_text().startswith("Release header\n\n## v0.1.0"))

    def test_from_release_commit_extracts_human_edited_body_only(self) -> None:
        self.commit("feat(core): generated feature (#22)")
        release_body = (
            "## v1.0.0\n\n### Notes\n\n"
            "    preserve indented markdown  \n"
            "- Keep the edited release body."
        )
        release_sha = self.commit("chore(release): basic train v1.0.0", release_body)

        result = self.changelog(
            "--version",
            "1.0.0",
            "--to-ref",
            release_sha,
            "--from-release-commit",
        )

        self.assertEqual(release_body + "\n", result.stdout)
        self.assertNotIn("generated feature", result.stdout)
        self.assertNotIn("Full Changelog", result.stdout)

    def test_from_release_commit_accepts_squash_suffix_and_preserves_notes(self) -> None:
        for version in ("1.0.0", "1.0.0-rc.1"):
            with self.subTest(version=version):
                body = f"## v{version}\n\n    Keep indentation  \n- Reviewed notes"
                sha = self.commit(f"chore(release): basic train v{version} (#239)", body)
                result = self.changelog(
                    "--version", version, "--to-ref", sha, "--from-release-commit",
                )
                self.assertEqual(body + "\n", result.stdout)

    def test_from_release_commit_rejects_malformed_squash_subjects(self) -> None:
        for subject in (
            "chore(release): basic train v1.0.1 (#239)",
            "chore(release): basic train v1.0.0 (#abc)",
            "chore(release): basic train v1.0.0 (#\u0662\u0663\u0669)",
            "chore(release): basic train v1.0.0 (#239) trailing",
            "chore(release): basic train v1.0.0 (#239) (#240)",
            "chore(release): basic train v1.0.0 trailing",
        ):
            with self.subTest(subject=subject):
                sha = self.commit(subject, "## v1.0.0\n\n- Reviewed notes")
                result = self.changelog(
                    "--version", "1.0.0", "--to-ref", sha,
                    "--from-release-commit", check=False,
                )
                self.assertNotEqual(0, result.returncode)
                self.assertIn("Release commit subject must be", result.stderr)

    def test_from_release_commit_can_prepend_header(self) -> None:
        release_body = "## v1.0.0\n\n- Edited notes"
        release_sha = self.commit("chore(release): basic train v1.0.0", release_body)
        header = self.repo / "header.md"
        header.write_text("Release header\n")

        result = self.changelog(
            "--version",
            "1.0.0",
            "--to-ref",
            release_sha,
            "--from-release-commit",
            "--header",
            str(header),
        )

        self.assertEqual(f"Release header\n\n{release_body}\n", result.stdout)

    def test_from_release_commit_rejects_wrong_subject_and_empty_body(self) -> None:
        wrong_subject = self.commit("chore(release): basic train v1.0.1", "## v1.0.0")
        wrong_subject_result = self.changelog(
            "--version",
            "1.0.0",
            "--to-ref",
            wrong_subject,
            "--from-release-commit",
            check=False,
        )
        empty_body = self.commit("chore(release): basic train v1.0.0")
        empty_body_result = self.changelog(
            "--version",
            "1.0.0",
            "--to-ref",
            empty_body,
            "--from-release-commit",
            check=False,
        )

        self.assertNotEqual(0, wrong_subject_result.returncode)
        self.assertIn("Release commit subject must be", wrong_subject_result.stderr)
        self.assertNotEqual(0, empty_body_result.returncode)
        self.assertIn("Release commit body is empty", empty_body_result.stderr)

    def test_from_release_commit_still_validates_version(self) -> None:
        release_sha = self.commit("chore(release): basic train vv1.0.0", "## vv1.0.0")

        result = self.changelog(
            "--version",
            "v1.0.0",
            "--to-ref",
            release_sha,
            "--from-release-commit",
            check=False,
        )

        self.assertNotEqual(0, result.returncode)
        self.assertIn("Invalid semantic version", result.stderr)

    def test_invalid_version_and_ref_fail_clearly(self) -> None:
        self.commit("feat(core): seed release")

        invalid_version = self.changelog("--version", "v1.0.0", check=False)
        invalid_ref = self.changelog(
            "--version", "1.0.0", "--to-ref", "missing-ref", check=False
        )

        self.assertNotEqual(0, invalid_version.returncode)
        self.assertIn("Invalid semantic version", invalid_version.stderr)
        self.assertNotEqual(0, invalid_ref.returncode)
        self.assertIn("Invalid git ref", invalid_ref.stderr)


if __name__ == "__main__":
    unittest.main()
