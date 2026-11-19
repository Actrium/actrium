"""Choose the numeric alpha tag for a workspace commit from fetched repository tags."""

import re
import subprocess
import tomllib
from pathlib import Path


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def main():
    with Path("Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["workspace"]["package"]["version"]
    match = re.fullmatch(r"(\d+\.\d+\.\d+)(?:[-+][0-9A-Za-z.+-]+)?", version)
    if not match:
        raise SystemExit(f"Invalid workspace version: {version}")
    base = match[1]
    prefix = f"{base}-alpha."
    commit = git("rev-parse", "HEAD")
    highest = 0
    reuse = []
    for tag in git("tag", "--list", f"{prefix}*").splitlines():
        suffix = tag.removeprefix(prefix)
        if not re.fullmatch(r"[1-9][0-9]*", suffix):
            continue
        highest = max(highest, int(suffix))
        if git("rev-parse", f"refs/tags/{tag}^{{commit}}") == commit:
            reuse.append(tag)
    if len(reuse) > 1:
        raise SystemExit(f"Ambiguous snapshot tags for {commit}: {reuse}")
    print(f"APP_VERSION={base}")
    print(f"TAG={reuse[0] if reuse else f'{prefix}{highest + 1}'}")


if __name__ == "__main__":
    main()
