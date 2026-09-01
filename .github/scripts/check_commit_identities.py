#!/usr/bin/env python3
"""Reject newly introduced commits that expose non-anonymous email addresses."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


USER_NOREPLY_RE = re.compile(r"^[^@\s<>]+@users\.noreply\.github\.com$", re.IGNORECASE)
EMAIL_RE = re.compile(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", re.IGNORECASE)
ANONYMOUS_ROOT_NAME = "root"
GITHUB_MERGE_COMMITTER_NAME = "GitHub"
GITHUB_MERGE_COMMITTER_EMAIL = "noreply@github.com"
ZERO_SHA = "0" * 40


@dataclass(frozen=True)
class CommitIdentity:
    sha: str
    author_name: str
    author_email: str
    committer_name: str
    committer_email: str
    parent_count: int
    message: str


def run_git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ).stdout


def is_allowed_identity(name: str, email: str) -> bool:
    if USER_NOREPLY_RE.fullmatch(email):
        return True
    return name == ANONYMOUS_ROOT_NAME and not email


def is_allowed_message_email(email: str) -> bool:
    return bool(USER_NOREPLY_RE.fullmatch(email))


def is_allowed_committer(commit: CommitIdentity) -> bool:
    if is_allowed_identity(commit.committer_name, commit.committer_email):
        return True
    # GitHub.com signs PR merge commits with this fixed service identity. Keep
    # the exception narrow so it cannot be used for an ordinary human commit.
    return (
        commit.parent_count >= 2
        and commit.committer_name == GITHUB_MERGE_COMMITTER_NAME
        and commit.committer_email == GITHUB_MERGE_COMMITTER_EMAIL
    )


def commit_range(repo: Path, base_sha: str, head_sha: str) -> list[str]:
    head_sha = run_git(repo, "rev-parse", f"{head_sha}^{{commit}}").strip()
    base_sha = base_sha.strip()

    if not base_sha or base_sha == ZERO_SHA:
        # workflow_dispatch and creation of a new branch do not provide a
        # meaningful before SHA. Check HEAD rather than rescanning the retained
        # pre-policy history.
        return [head_sha]

    try:
        base_sha = run_git(repo, "rev-parse", f"{base_sha}^{{commit}}").strip()
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            "base commit is unavailable; CI must checkout with fetch-depth: 0"
        ) from error

    if base_sha == head_sha:
        return []

    output = run_git(repo, "rev-list", "--reverse", f"{base_sha}..{head_sha}")
    return [line for line in output.splitlines() if line]


def read_identity(repo: Path, sha: str) -> CommitIdentity:
    output = run_git(
        repo,
        "show",
        "-s",
        "--format=%an%x00%ae%x00%cn%x00%ce%x00%P%x00%B",
        sha,
    )
    author_name, author_email, committer_name, committer_email, parents, message = output.split(
        "\0", 5
    )
    return CommitIdentity(
        sha=sha,
        author_name=author_name,
        author_email=author_email,
        committer_name=committer_name,
        committer_email=committer_email,
        parent_count=len(parents.split()),
        message=message,
    )


def validate_identity(commit: CommitIdentity) -> list[str]:
    errors: list[str] = []
    short_sha = commit.sha[:12]

    if not is_allowed_identity(commit.author_name, commit.author_email):
        errors.append(
            f"{short_sha}: author must use a GitHub noreply identity; "
            "the configured address is not printed to avoid repeating it in CI logs"
        )
    if not is_allowed_committer(commit):
        errors.append(
            f"{short_sha}: committer must use a GitHub noreply identity; "
            "the configured address is not printed to avoid repeating it in CI logs"
        )

    exposed_message_emails = {
        email for email in EMAIL_RE.findall(commit.message) if not is_allowed_message_email(email)
    }
    if exposed_message_emails:
        errors.append(
            f"{short_sha}: commit message contains a non-anonymous email address; "
            "the address is not printed to avoid repeating it in CI logs"
        )

    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base-sha", default="")
    parser.add_argument("--head-sha", default="HEAD")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        commits = commit_range(args.repo, args.base_sha, args.head_sha)
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"commit identity check could not determine its range: {error}", file=sys.stderr)
        return 2

    errors = [
        error
        for sha in commits
        for error in validate_identity(read_identity(args.repo, sha))
    ]
    if errors:
        for error in errors:
            print(f"::error::{error}", file=sys.stderr)
        print(
            "Configure Git with a GitHub noreply address, for example: "
            "git config user.email '<id>+<username>@users.noreply.github.com'",
            file=sys.stderr,
        )
        return 1

    noun = "commit" if len(commits) == 1 else "commits"
    print(f"commit identity privacy check passed for {len(commits)} {noun}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
