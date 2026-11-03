#!/usr/bin/env python3
"""Audit the checked-out tree and reachable history for retired identifiers."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


TOKENS = (b"as" + b"kc", b"as" + b"ks")
PATTERN = re.compile(
    rb"(?<![A-Za-z0-9])(?:" + b"|".join(TOKENS) + rb")(?![A-Za-z0-9])",
    re.IGNORECASE,
)


# Exact reviewed upstream comment lines, rather than a directory exemption.
# A changed line or a real identifier elsewhere remains subject to the audit,
# including historical blobs and commit messages. The retained hashes cover
# reviewed upstream prose that remains reachable in repository history.
PROSE_LINE_HASHES = frozenset(
    item["line_sha256"]
    for item in json.loads(
        Path(__file__).with_name("retired-protocol-prose-exceptions.json").read_text()
    )
)


def git(*args):
    return subprocess.check_output(["git", *args])


def check_text(data, location, failures):
    # Match the binary exclusion used by the history migration script.
    if b"\0" in data[:8192]:
        return
    for number, line in enumerate(data.split(b"\n"), 1):
        if PATTERN.search(line) and hashlib.sha256(line.rstrip(b"\r")).hexdigest() not in PROSE_LINE_HASHES:
            # Report locations, not arbitrary historical content or secrets.
            failures.append(f"retired identifier in {location}:{number}")


def check_tree(data, oid, hash_size, failures):
    offset = 0
    while offset < len(data):
        separator = data.index(b" ", offset)
        end = data.index(b"\0", separator)
        name = data[separator + 1 : end]
        if PATTERN.search(name):
            failures.append(f"retired identifier in tree {oid}, entry {os.fsdecode(name)!r}")
        # Tree entries contain raw object IDs; these bytes are not identifiers.
        offset = end + 1 + hash_size


def check_history(all_refs, failures):
    scope = ["--all", "HEAD"] if all_refs else ["HEAD"]
    listing = git("-c", "core.quotePath=false", "rev-list", "--objects", *scope)
    objects = {}
    for line in listing.split(b"\n"):
        if line:
            oid, _, name = line.partition(b" ")
            objects[oid] = name
    hash_size = len(git("rev-parse", "HEAD").strip()) // 2

    # File-backed input prevents stdin/stdout pipe deadlocks on large repos.
    with tempfile.TemporaryFile() as requests:
        requests.write(b"\n".join(objects) + b"\n")
        requests.seek(0)
        with subprocess.Popen(
            ["git", "cat-file", "--batch"], stdin=requests, stdout=subprocess.PIPE
        ) as process:
            for expected_oid, name in objects.items():
                header = process.stdout.readline().split()
                if len(header) != 3 or header[0] != expected_oid:
                    raise RuntimeError(f"Cannot read Git object {expected_oid.decode()}")
                oid, kind, size = header
                data = process.stdout.read(int(size))
                if len(data) != int(size) or process.stdout.read(1) != b"\n":
                    raise RuntimeError(f"Truncated Git object {oid.decode()}")
                label = oid.decode()
                if kind == b"blob":
                    check_text(data, f"blob {label} ({os.fsdecode(name)!r})", failures)
                elif kind == b"commit":
                    check_text(data.partition(b"\n\n")[2], f"commit {label}", failures)
                elif kind == b"tree":
                    check_tree(data, label, hash_size, failures)
            if process.wait() != 0:
                raise RuntimeError("git cat-file failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--all-refs", action="store_true", help="Also audit history reachable from other local refs"
    )
    args = parser.parse_args()
    os.chdir(os.fsdecode(git("rev-parse", "--show-toplevel").rstrip(b"\n")))
    failures = []
    if git("rev-parse", "--is-shallow-repository").strip() == b"true":
        failures.append("full history is required for the retired protocol audit")

    # Preserve the current-tree checks, including untracked, non-ignored paths.
    for name in set(git("ls-files", "-co", "--exclude-standard", "-z").split(b"\0")) - {b""}:
        filename = os.fsdecode(name)
        if PATTERN.search(name):
            failures.append(f"retired identifier in path: {filename!r}")
    for name in git("ls-files", "-z").split(b"\0"):
        if not name:
            continue
        filename = os.fsdecode(name)
        file = Path(filename)
        if file.is_symlink():
            check_text(os.fsencode(os.readlink(file)), repr(filename), failures)
        elif file.is_file():
            check_text(file.read_bytes(), repr(filename), failures)

    if not failures:
        check_history(args.all_refs, failures)
    if failures:
        print("Retired package protocol identifiers are not allowed:", file=sys.stderr)
        print("\n".join(sorted(set(failures))), file=sys.stderr)
        return 1
    print("Retired package protocol identifier check passed.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Retired protocol audit failed: {error}", file=sys.stderr)
        sys.exit(1)
