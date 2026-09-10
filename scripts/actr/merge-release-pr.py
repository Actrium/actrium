#!/usr/bin/env python3
"""Preview or squash-merge a release PR while preserving its reviewed notes."""

import argparse
import json
import re
import subprocess
import tempfile
from pathlib import Path


def read_pr(repository: str, number: int) -> dict:
    return json.loads(subprocess.check_output(
        ["gh", "pr", "view", str(number), "--repo", repository, "--json",
         "number,title,body,headRefName,headRefOid,baseRefName,state,isDraft"],
        text=True,
    ))


def validate_pr(pr: dict) -> None:
    match = re.fullmatch(
        r"chore\(release\): basic train v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)",
        pr["title"],
    )
    if not match:
        raise ValueError("Expected an exact release-train PR title")
    version = match.group(1)
    if pr["state"] != "OPEN" or pr["isDraft"]:
        raise ValueError("Release PR must be open and ready for review")
    if pr["baseRefName"] != "main":
        raise ValueError("This helper only merges Release Prepare PRs into main")
    if pr["headRefName"] != f"release-prepare/v{version}":
        raise ValueError("Release PR branch and title versions must match")
    body = pr["body"] or ""
    lines = body.strip().splitlines()
    if not lines or lines[0] != f"## v{version}" or not any(
        line.strip() for line in lines[1:]
    ):
        raise ValueError("Release PR body must contain matching version notes")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("number", type=int)
    parser.add_argument("--repo", default="Actrium/actrium")
    parser.add_argument("--merge", action="store_true", help="perform the squash merge")
    args = parser.parse_args()
    pr = read_pr(args.repo, args.number)
    validate_pr(pr)
    if not args.merge:
        print(pr["title"] + "\n\n" + pr["body"])
        return

    # Use a fresh snapshot and require the same reviewed head at merge time.
    if read_pr(args.repo, args.number) != pr:
        raise ValueError("Release PR changed during preparation; review it again")
    with tempfile.TemporaryDirectory(prefix="actrium-release-merge-") as directory:
        body_file = Path(directory) / "body.md"
        body_file.write_text(pr["body"], encoding="utf-8")
        subprocess.run(
            ["gh", "pr", "merge", str(args.number), "--repo", args.repo,
             "--squash", "--match-head-commit", pr["headRefOid"],
             "--subject", pr["title"], "--body-file", str(body_file)],
            check=True,
        )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
