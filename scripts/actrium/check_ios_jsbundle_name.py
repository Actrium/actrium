#!/usr/bin/env python3
"""Guard the iOS Release JS bundle name against producer/loader drift.

The Release loader (`bundleURL()`) looks the bundle up by resource name, while
the packaging scripts produce it under a file name. Issue #318: these drifted
apart ("actr" vs "runtime") and every Release/TestFlight build white-screened.
This check fails whenever any producer or loader disagrees on the name.
"""

import argparse
import re
import sys
from pathlib import Path

LOADER = re.compile(
    r'forResource:\s*"(?P<name>[^"]+)"\s*,\s*withExtension:\s*"jsbundle"'
)
PRODUCER = re.compile(r'--bundle-output\s+(?:"(?P<quoted>[^"]+)"|(?P<bare>\S+))')


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def loader_names(ios_dir: Path):
    found = []
    for swift in sorted(ios_dir.rglob("*.swift")):
        for number, line in enumerate(swift.read_text(errors="replace").splitlines(), 1):
            match = LOADER.search(line)
            if match:
                found.append((match.group("name"), f"{swift}:{number}"))
    return found


def producer_names(script_dir: Path):
    found = []
    problems = []
    for script in sorted(p for p in script_dir.iterdir() if p.is_file()):
        for number, line in enumerate(script.read_text(errors="replace").splitlines(), 1):
            if line.lstrip().startswith("#"):
                continue
            match = PRODUCER.search(line)
            if not match:
                continue
            raw = match.group("quoted") or match.group("bare")
            base = Path(raw).name
            location = f"{script}:{number}"
            stem, dot, ext = base.rpartition(".")
            if not stem or dot == "" or ext != "jsbundle":
                problems.append(f"bundle output is not a *.jsbundle: {base} ({location})")
                continue
            found.append((stem, location))
    return found, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=repo_root(),
        help="repository root (default: inferred from this script)",
    )
    args = parser.parse_args()

    ios_dir = args.root / "ios"
    script_dir = ios_dir / "Scripts"
    failures = []

    if not ios_dir.is_dir() or not script_dir.is_dir():
        print(f"error: expected {ios_dir} and {script_dir} to exist", file=sys.stderr)
        return 1

    loaders = loader_names(ios_dir)
    producers, problems = producer_names(script_dir)
    failures.extend(problems)

    if not loaders:
        failures.append("no forResource:…withExtension:\"jsbundle\" loader found under ios/")
    if not producers:
        failures.append("no --bundle-output …jsbundle producer found under ios/Scripts/")

    for name, location in loaders:
        print(f"loader  {name}.jsbundle ({location})", file=sys.stderr)
    for name, location in producers:
        print(f"producer {name}.jsbundle ({location})", file=sys.stderr)

    loader_set = {name for name, _ in loaders}
    producer_set = {name for name, _ in producers}
    if len(loader_set) > 1:
        failures.append(f"loaders disagree on the bundle name: {sorted(loader_set)}")
    if len(producer_set) > 1:
        failures.append(f"producers disagree on the bundle name: {sorted(producer_set)}")
    if loader_set and producer_set and loader_set != producer_set:
        failures.append(
            "loader and producers disagree on the bundle name: "
            f"loader={sorted(loader_set)} producers={sorted(producer_set)}"
        )

    if failures:
        print(f"\ncheck_ios_jsbundle_name: {len(failures)} problem(s)", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("check_ios_jsbundle_name: ok", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
