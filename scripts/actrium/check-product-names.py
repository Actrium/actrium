#!/usr/bin/env python3
"""Reject retired product names outside the pending runtime cutover inventory."""

import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path


ALLOWLIST = "scripts/actrium/product-name-allowlist.json"
ARCHIVE = "docs/migration/source-archive-readiness.md"
RETIRED = re.compile(rb"act" + rb"ro|Act" + rb"ro|ACT" + rb"RO")
RETIRED_SERVICE = re.compile(rb"act" + rb"rix", re.IGNORECASE)
IDENTIFIER = re.compile(rb"[A-Za-z0-9_./:@+%\\-]+")


def load_allowlist(root):
    inventory = json.loads((root / ALLOWLIST).read_text(encoding="utf-8"))
    allowances = inventory["references"]
    for path, identifiers in allowances.items():
        if (
            Path(path).is_absolute()
            or ".." in Path(path).parts
            or any(character in path for character in "*?[]")
            or path in (ALLOWLIST, ARCHIVE)
            or not identifiers
        ):
            raise ValueError(f"Invalid allowlist path: {path}")
        for identifier, count in identifiers.items():
            encoded = identifier.encode("ascii")
            if (
                not IDENTIFIER.fullmatch(encoded)
                or not RETIRED.search(encoded)
                or type(count) is not int
                or count < 1
            ):
                raise ValueError(f"Invalid allowlist entry in {path}")
    return allowances


def main():
    root = Path(__file__).resolve().parents[2]
    allowances = load_allowlist(root)
    listing = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
    )
    failures = []
    actual = {}
    for raw_path in sorted(set(listing.split(b"\0")) - {b""}):
        path = os.fsdecode(raw_path)
        if RETIRED.search(raw_path) or RETIRED_SERVICE.search(raw_path):
            failures.append(f"Retired product name in path: {path}")
        file = root / path
        if file.is_symlink():
            data = os.fsencode(os.readlink(file))
        elif file.is_file():
            with file.open("rb") as source:
                prefix = source.read(8192)
                if b"\0" in prefix:
                    continue
                data = prefix + source.read()
        else:
            continue

        for number, line in enumerate(data.split(b"\n"), 1):
            if RETIRED_SERVICE.search(line):
                failures.append(f"Retired service name at {path}:{number}")
        if path in (ALLOWLIST, ARCHIVE):
            continue

        counts = Counter()
        expected = allowances.get(path, {})
        for number, line in enumerate(data.split(b"\n"), 1):
            if not RETIRED.search(line):
                continue
            for match in IDENTIFIER.finditer(line):
                token = match.group()
                if not RETIRED.search(token):
                    continue
                identifier = token.decode("ascii")
                counts[identifier] += 1
                if counts[identifier] > expected.get(identifier, 0):
                    failures.append(f"Unexpected retired product name at {path}:{number}")
        actual[path] = counts

    # Removed references must also lose their allowance so they cannot return.
    for path, identifiers in allowances.items():
        for identifier, expected in identifiers.items():
            if actual.get(path, {}).get(identifier, 0) < expected:
                failures.append(f"Remove or reduce a stale product-name allowance in {path}")

    if failures:
        print("Product name check failed:", file=sys.stderr)
        print("\n".join(sorted(set(failures))), file=sys.stderr)
        return 1
    print("Product name check passed; pending runtime references match their exact allowances.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"Product name check failed: {error}", file=sys.stderr)
        sys.exit(1)
