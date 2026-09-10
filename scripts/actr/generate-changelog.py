#!/usr/bin/env python3
"""Generate release notes for an Actrium release."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


CATEGORY_ORDER = (
    "Breaking Changes",
    "New Features",
    "Improvements",
    "Bug Fixes",
    "Internal Changes",
)

TYPE_TO_CATEGORY = {
    "feat": "New Features",
    "fix": "Bug Fixes",
    "perf": "Improvements",
    "refactor": "Improvements",
    "docs": "Improvements",
    "ci": "Internal Changes",
    "build": "Internal Changes",
    "test": "Internal Changes",
    "chore": "Internal Changes",
}

CONVENTIONAL_COMMIT = re.compile(
    r"^(?P<type>[a-z]+)(?:\((?P<scope>[^)]*)\))?(?P<breaking>!)?:\s*(?P<description>.+)$"
)
PR_NUMBER = re.compile(r"(?:\(#|\b#)(\d+)\)?")
SEMVER = re.compile(
    r"^(?P<major>0|[1-9]\d*)\."
    r"(?P<minor>0|[1-9]\d*)\."
    r"(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<prerelease>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
TAG_VERSION = re.compile(r"^(?:v|release-train-v)(?P<version>.+)$")


@dataclass(frozen=True)
class SemVer:
    major: int
    minor: int
    patch: int
    prerelease: tuple[int | str, ...] = ()

    @property
    def core(self) -> tuple[int, int, int]:
        return self.major, self.minor, self.patch

    @property
    def is_prerelease(self) -> bool:
        return bool(self.prerelease)

    def __lt__(self, other: "SemVer") -> bool:
        if self.core != other.core:
            return self.core < other.core
        if not self.prerelease and other.prerelease:
            return False
        if self.prerelease and not other.prerelease:
            return True
        return prerelease_is_less(self.prerelease, other.prerelease)


@dataclass(frozen=True)
class Tag:
    name: str
    version: SemVer


@dataclass(frozen=True)
class Baseline:
    name: str
    revision: str


@dataclass(frozen=True)
class Entry:
    category: str
    description: str
    identifier: str
    link: str

    @property
    def line(self) -> str:
        return f"- {self.description} ([{self.identifier}]({self.link}))"


def prerelease_is_less(
    left: tuple[int | str, ...], right: tuple[int | str, ...]
) -> bool:
    for left_part, right_part in zip(left, right):
        if left_part == right_part:
            continue
        if isinstance(left_part, int) and isinstance(right_part, str):
            return True
        if isinstance(left_part, str) and isinstance(right_part, int):
            return False
        return left_part < right_part
    return len(left) < len(right)


def parse_version(value: str) -> SemVer:
    match = SEMVER.match(value)
    if match is None:
        raise ValueError(f"Invalid semantic version: {value}")
    prerelease_text = match.group("prerelease") or ""
    prerelease: list[int | str] = []
    for identifier in prerelease_text.split(".") if prerelease_text else []:
        if identifier.isdigit():
            if len(identifier) > 1 and identifier.startswith("0"):
                raise ValueError(f"Invalid semantic version: {value}")
            prerelease.append(int(identifier))
        else:
            prerelease.append(identifier)
    return SemVer(
        int(match.group("major")),
        int(match.group("minor")),
        int(match.group("patch")),
        tuple(prerelease),
    )


def run_git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout


def repository_url(repo: Path) -> str:
    try:
        remote = run_git(repo, "config", "--get", "remote.origin.url").strip()
    except subprocess.CalledProcessError:
        return "https://github.com/Actrium/actrium"

    remote = remote.removesuffix(".git")
    if remote.startswith("git@github.com:"):
        return "https://github.com/" + remote.removeprefix("git@github.com:")
    if remote.startswith("https://github.com/"):
        return remote
    if remote.startswith("http://github.com/"):
        return "https://github.com/" + remote.removeprefix("http://github.com/")
    return "https://github.com/Actrium/actrium"


def validate_ref(repo: Path, ref: str) -> str:
    try:
        return run_git(
            repo, "rev-parse", "--verify", "--end-of-options", f"{ref}^{{commit}}"
        ).strip()
    except subprocess.CalledProcessError as error:
        raise ValueError(f"Invalid git ref: {ref}") from error


def release_commit_subject(version: str) -> str:
    return f"chore(release): basic train v{version}"


def release_commit_body(repo: Path, ref: str, version: str) -> str:
    target = validate_ref(repo, ref)
    raw = run_git(repo, "show", "-s", "--format=%s%x00%b", "--end-of-options", target)
    subject, _, body = raw.partition("\x00")
    expected = release_commit_subject(version)
    if subject.strip() != expected:
        raise ValueError(
            f"Release commit subject must be exactly {expected!r}: {subject.strip()!r}"
        )
    if not body.strip():
        raise ValueError(f"Release commit body is empty: {target}")
    return body.rstrip("\n") + "\n"


def validate_ancestor(repo: Path, base: str, target: str) -> None:
    try:
        run_git(repo, "merge-base", "--is-ancestor", "--end-of-options", base, target)
    except subprocess.CalledProcessError as error:
        raise ValueError(f"Base ref is not reachable from target ref: {base}") from error


def release_minor(branch: str) -> tuple[int, int] | None:
    match = re.search(r"(?:^|/)release-(\d+)\.(\d+)$", branch)
    if match is None:
        return None
    return int(match.group(1)), int(match.group(2))


def reachable_release_tags(repo: Path, to_ref: str) -> list[Tag]:
    raw = run_git(
        repo,
        "for-each-ref",
        "--merged",
        to_ref,
        "--format=%(refname:short)",
        "refs/tags",
    )
    tags: list[Tag] = []
    for name in raw.splitlines():
        match = TAG_VERSION.match(name)
        if match is None:
            continue
        try:
            tags.append(Tag(name, parse_version(match.group("version"))))
        except ValueError:
            continue
    return tags


def select_baseline(repo: Path, version: SemVer, target: str, branch: str) -> Baseline:
    tags = reachable_release_tags(repo, target)
    maintenance_minor = release_minor(branch)

    if version.is_prerelease:
        prereleases = [
            tag
            for tag in tags
            if tag.version.is_prerelease
            and tag.version.core == version.core
            and tag.version < version
        ]
        if prereleases:
            tag = max(prereleases, key=lambda tag: tag.version)
            return Baseline(tag.name, validate_ref(repo, tag.name))

    stable_tags = [
        tag
        for tag in tags
        if not tag.version.is_prerelease and tag.version < version
    ]
    if maintenance_minor is not None:
        stable_tags = [
            tag
            for tag in stable_tags
            if (tag.version.major, tag.version.minor) == maintenance_minor
        ]
    if not stable_tags:
        return Baseline("", "")
    tag = max(stable_tags, key=lambda tag: tag.version)
    return Baseline(tag.name, validate_ref(repo, tag.name))


def resolve_base(
    repo: Path, version: SemVer, to_ref: str, branch: str, explicit: str | None
) -> tuple[Baseline, str]:
    target = validate_ref(repo, to_ref)
    if explicit:
        base = validate_ref(repo, explicit)
        validate_ancestor(repo, base, target)
        return Baseline(explicit, base), target
    return select_baseline(repo, version, target, branch), target


def commits_between(repo: Path, base: str, target: str) -> list[tuple[str, str, str]]:
    revision = f"{base}..{target}" if base else target
    raw = run_git(
        repo,
        "log",
        "--reverse",
        "--format=%H%x00%s%x00%b%x00",
        "--end-of-options",
        revision,
    )
    fields = raw.split("\x00")
    commits: list[tuple[str, str, str]] = []
    for index in range(0, len(fields) - 2, 3):
        commit, subject, body = (field.strip() for field in fields[index : index + 3])
        if commit:
            commits.append((commit, subject, body))
    return commits


def classify(subject: str, body: str) -> tuple[str, str] | None:
    match = CONVENTIONAL_COMMIT.match(subject)
    if match is None:
        category = "Internal Changes"
        description = subject
        scope = ""
        breaking = False
        commit_type = ""
    else:
        commit_type = match.group("type")
        if commit_type == "style":
            return None
        category = TYPE_TO_CATEGORY.get(commit_type, "Internal Changes")
        description = match.group("description").strip()
        scope = (match.group("scope") or "").strip()
        breaking = bool(match.group("breaking"))

    if commit_type == "chore" and scope == "release":
        return None
    if subject.startswith("chore: basic train"):
        return None
    if "BREAKING CHANGE" in body or breaking:
        category = "Breaking Changes"

    description = re.sub(r"\s+\(#\d+\)$", "", description).strip()
    if description:
        description = description[0].upper() + description[1:]
    if scope:
        description = f"{scope}: {description}"
    return category, description


def make_entries(repo: Path, base: str, target: str, root_url: str) -> dict[str, list[Entry]]:
    entries = {category: [] for category in CATEGORY_ORDER}
    for commit, subject, body in commits_between(repo, base, target):
        classified = classify(subject, body)
        if classified is None:
            continue
        category, description = classified
        pr_match = PR_NUMBER.search(subject)
        if pr_match:
            identifier = f"#{pr_match.group(1)}"
            link = f"{root_url}/pull/{pr_match.group(1)}"
        else:
            identifier = commit[:8]
            link = f"{root_url}/commit/{commit}"
        entries[category].append(Entry(category, description, identifier, link))
    return entries


def compare_link(root_url: str, base: str, version: str) -> str:
    target = f"v{version}"
    if base:
        return f"{root_url}/compare/{base}...{target}"
    return f"{root_url}/releases/tag/{target}"


def render_notes(
    version: str, base: str, entries: dict[str, list[Entry]], full_link: str
) -> str:
    lines = [f"## v{version}", ""]
    if not base:
        lines.extend(["Initial release summary.", ""])
    for category in CATEGORY_ORDER:
        category_entries = entries[category]
        if not category_entries:
            continue
        lines.extend([f"### {category}", ""])
        lines.extend(entry.line for entry in category_entries)
        lines.append("")
    lines.extend(["### Full Changelog", "", f"[Compare changes]({full_link})", ""])
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--version", required=True, help="release version without the leading v")
    parser.add_argument("--from-tag", default=None)
    parser.add_argument("--to-ref", default="HEAD")
    parser.add_argument("--branch", default="main")
    parser.add_argument("--output", type=Path, default=None, help="write release notes to this path")
    parser.add_argument("--header", type=Path, default=None)
    parser.add_argument("--repository-url", default=None)
    parser.add_argument(
        "--from-release-commit",
        action="store_true",
        help="extract release notes from the resolved --to-ref release commit body",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.resolve()
    version = parse_version(args.version)
    if args.from_release_commit:
        body = release_commit_body(repo, args.to_ref, args.version)
    else:
        baseline, target = resolve_base(repo, version, args.to_ref, args.branch, args.from_tag)
        root_url = args.repository_url or repository_url(repo)
        entries = make_entries(repo, baseline.revision, target, root_url)
        body = render_notes(
            args.version,
            baseline.name,
            entries,
            compare_link(root_url, baseline.name, args.version),
        )
    if args.header and args.header.is_file():
        body = args.header.read_text().rstrip() + "\n\n" + body
    if args.output:
        args.output.write_text(body)
    else:
        print(body, end="")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(error.stderr.strip() or str(error), file=sys.stderr)
        raise SystemExit(1)
    except (ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
