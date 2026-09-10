#!/usr/bin/env python3
"""Reject newly introduced commits that expose non-anonymous email addresses.

Two layers are enforced:

* Privacy: author, committer, and commit-message email addresses must be
  GitHub noreply addresses (plus the fixed GitHub service committer).
* Attribution: the GitHub login embedded in every `users.noreply` address
  -- for the author, the committer, and each `Co-authored-by` trailer --
  must be on the allowed identity list. The privacy regex alone accepts any
  `<login>@users.noreply.github.com` string, so a tool that stamps its own
  noreply-shaped address passes unnoticed; an OpenClaw-authored commit and
  the matching squash trailer landed on main through exactly that gap in
  #222.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


USER_NOREPLY_RE = re.compile(r"^[^@\s<>]+@users\.noreply\.github\.com$", re.IGNORECASE)
# Captures the login of both noreply spellings: "login@..." and "42+login@...".
NOREPLY_LOGIN_RE = re.compile(r"^(?:\d+\+)?([^+@\s]+)@users\.noreply\.github\.com$", re.IGNORECASE)
CO_AUTHOR_EMAIL_RE = re.compile(r"^Co-authored-by:[^<\n]*<([^<>]*)>", re.IGNORECASE | re.MULTILINE)
EMAIL_RE = re.compile(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", re.IGNORECASE)
ANONYMOUS_ROOT_NAME = "root"
GITHUB_SERVICE_COMMITTER_NAME = "GitHub"
GITHUB_SERVICE_COMMITTER_EMAIL = "noreply@github.com"
ZERO_SHA = "0" * 40

# GitHub logins allowed to appear in noreply identities of new commits:
# repository members observed in history plus the service bot GitHub
# stamps on squash merges. Extend this set when a new member joins.
ALLOWED_MEMBER_LOGINS = frozenset(
    {
        "kaito-harry",
        "zhanghongjun",
        "alinejun",
        "kookyleo",
        "mybee",
        "g65537",
        "harry",
        "kaito",
        "github-actions[bot]",
    }
)

# Coding assistants whose attribution the team has explicitly accepted.
# Trailers and commits signed by anything else (for example the OpenClaw
# organization account) are rejected by the attribution layer. Keep this
# list empty until the team decides which assistant logins to allow.
ALLOWED_ASSISTANT_LOGINS: frozenset[str] = frozenset()


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
    # GitHub.com stamps every commit it creates on a user's behalf -- merge
    # commits, squash merges, and web/API edits alike -- with this fixed
    # service identity (#216). The exact name/email pair names no human, so
    # no parent-count, PR-suffix, or release-prefix condition is needed to
    # keep the exception narrow for an ordinary human commit.
    return (
        commit.committer_name == GITHUB_SERVICE_COMMITTER_NAME
        and commit.committer_email == GITHUB_SERVICE_COMMITTER_EMAIL
    )


def noreply_login(email: str) -> str | None:
    """Return the GitHub login of a users.noreply address, else None."""
    match = NOREPLY_LOGIN_RE.fullmatch(email.strip())
    return match.group(1) if match else None


def co_author_emails(message: str) -> list[str]:
    """Return the email of every Co-authored-by trailer that carries one."""
    return CO_AUTHOR_EMAIL_RE.findall(message)


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
        # A force-push can make the event's `before` commit unreachable from
        # every advertised ref even with fetch-depth: 0. GitHub still permits
        # fetching that exact object id, so recover it before calculating the
        # introduced range instead of silently checking only HEAD.
        try:
            run_git(
                repo,
                "fetch",
                "--no-tags",
                "--no-recurse-submodules",
                "origin",
                base_sha,
            )
            base_sha = run_git(repo, "rev-parse", f"{base_sha}^{{commit}}").strip()
        except subprocess.CalledProcessError as fetch_error:
            raise RuntimeError(
                "base commit is unavailable after an explicit object-id fetch"
            ) from fetch_error

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


def attribution_errors(commit: CommitIdentity) -> list[str]:
    """Reject noreply identities whose login is not on the allow lists."""
    errors: list[str] = []
    short_sha = commit.sha[:12]
    members = {login.casefold() for login in ALLOWED_MEMBER_LOGINS}
    trailers = members | {login.casefold() for login in ALLOWED_ASSISTANT_LOGINS}

    for role, email in (
        ("author", commit.author_email),
        ("committer", commit.committer_email),
    ):
        login = noreply_login(email)
        if login is not None and login.casefold() not in members:
            errors.append(
                f"{short_sha}: {role} noreply login '{login}' is not on the "
                "allowed identity list; extend ALLOWED_MEMBER_LOGINS or commit "
                "with a member identity"
            )

    for email in co_author_emails(commit.message):
        login = noreply_login(email)
        if login is not None and login.casefold() not in trailers:
            errors.append(
                f"{short_sha}: co-author noreply login '{login}' is not on "
                "the allowed identity list; extend ALLOWED_MEMBER_LOGINS or "
                "ALLOWED_ASSISTANT_LOGINS"
            )

    return errors


def validate_identity(commit: CommitIdentity, check_attribution: bool = True) -> list[str]:
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

    if check_attribution:
        errors.extend(attribution_errors(commit))

    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base-sha", default="")
    parser.add_argument("--head-sha", default="HEAD")
    parser.add_argument(
        "--no-attribution",
        action="store_true",
        help="skip the noreply login attribution layer (privacy checks still run)",
    )
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
        for error in validate_identity(
            read_identity(args.repo, sha),
            check_attribution=not args.no_attribution,
        )
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
