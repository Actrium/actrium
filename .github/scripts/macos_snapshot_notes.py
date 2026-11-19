"""Refresh the generated block in a macOS alpha release's Markdown body."""

import json
import sys
from datetime import datetime, timezone

START = "<!-- actrium-macos-build:start -->"
END = "<!-- actrium-macos-build:end -->"


def update_notes(body, metadata):
    block = f"{START}\n{metadata}\n{END}"
    if START not in body and END not in body:
        if not body:
            return block + "\n"
        separator = "\n" if body.endswith("\n") else "\n\n"
        return body + separator + block + "\n"

    prefix, _, remainder = body.partition(START)
    _, delimiter, suffix = remainder.partition(END)
    if not delimiter or body.count(START) != 1 or body.count(END) != 1:
        raise SystemExit("Invalid macOS build metadata markers in the existing release notes")
    return prefix + block + suffix


if __name__ == "__main__":
    existing = json.load(sys.stdin)["body"] or ""
    built = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
    metadata = (
        f"Commit: {sys.argv[1]}\n\n"
        f"Built: {built} (UTC)\n\n"
        "Apple Silicon, macOS 26.1 or later. Unsigned and not notarized."
    )
    sys.stdout.write(update_notes(existing, metadata))
