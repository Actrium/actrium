#!/usr/bin/env python3
"""Reuse a trusted image's Gradle distribution in its wrapper URL cache slot."""

import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


def cache_key(url):
    # Gradle's wrapper uses base36(MD5(URL)) as a cache identifier, not a digest.
    number = int.from_bytes(hashlib.md5(url.encode("utf-8")).digest(), "big")
    digits = "0123456789abcdefghijklmnopqrstuvwxyz"
    result = ""
    while number:
        number, digit = divmod(number, 36)
        result = digits[digit] + result
    return result or "0"


def distribution_url(properties):
    for line in Path(properties).read_text().splitlines():
        if line.strip().startswith("distributionUrl="):
            return line.strip().split("=", 1)[1].replace("\\:", ":")
    raise SystemExit("Gradle wrapper properties have no distributionUrl")


def valid_home(home, version):
    executable = home / "bin/gradle"
    if not executable.is_file():
        return False
    try:
        result = subprocess.run(
            [str(executable), "--offline", "--version"],
            capture_output=True, text=True, timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0 and f"Gradle {version}" in result.stdout.splitlines()


def link_or_copy(source, destination):
    try:
        os.link(source, destination)
        return destination
    except OSError:
        return shutil.copy2(source, destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--wrapper-properties")
    source.add_argument("--distribution-url")
    parser.add_argument("--print-cache-key", action="store_true")
    parser.add_argument("--require-preseeded", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    url = args.distribution_url or distribution_url(args.wrapper_properties)
    key = cache_key(url)
    if args.print_cache_key:
        print(key)
        return
    match = re.search(r"/gradle-([0-9][^/]*?)-bin\.zip$", url)
    if not match:
        raise SystemExit("Expected a Gradle binary distribution URL")
    version = match.group(1)
    user_home = Path(os.environ.get("GRADLE_USER_HOME", str(Path.home() / ".gradle")))
    root = user_home / "wrapper/dists" / f"gradle-{version}-bin"
    target = root / key
    home = target / f"gradle-{version}"
    marker = target / f"gradle-{version}-bin.zip.ok"
    if args.verify_only:
        if not marker.is_file() or not valid_home(home, version):
            raise SystemExit(f"Missing usable preseeded Gradle {version} wrapper cache")
        print(f"Verified preseeded Gradle {version} ({key})")
        return
    target.mkdir(parents=True, exist_ok=True)
    # Use the wrapper's own lock so concurrent wrappers cannot see partial copies.
    with (target / f"gradle-{version}-bin.zip.lck").open("a") as lock:
        fcntl.lockf(lock, fcntl.LOCK_EX)
        if not valid_home(home, version):
            candidates = sorted(root.glob(f"*/gradle-{version}"))
            cached = next((p for p in candidates if p != home and valid_home(p, version)), None)
            if cached is None:
                if args.require_preseeded:
                    raise SystemExit(f"No trusted image distribution for Gradle {version}")
                print(f"No preseeded Gradle {version}; the wrapper will download it")
                return
            temporary = Path(tempfile.mkdtemp(prefix=".preseed-", dir=target))
            try:
                staged = temporary / f"gradle-{version}"
                shutil.copytree(cached, staged, symlinks=True, copy_function=link_or_copy)
                if home.exists():
                    shutil.rmtree(home)
                staged.rename(home)
            finally:
                shutil.rmtree(temporary)
        # Only trusted image content is reused here; normal wrapper downloads
        # retain the distributionSha256Sum verification in their properties.
        marker.touch()
    print(f"Reusing image Gradle {version} in wrapper cache ({key})")


if __name__ == "__main__":
    main()
