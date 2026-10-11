#!/usr/bin/env python3
"""Merge the root workspace's upstream patches into a test project manifest."""

import argparse
import json
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError as error:
    raise SystemExit("Python 3.11 or newer is required to apply Cargo patches") from error


REPO = Path(__file__).resolve().parents[2]


def apply(source: Path, target: Path) -> None:
    patches = tomllib.loads(source.read_text())["patch"]
    original = target.read_text()
    current = tomllib.loads(original).get("patch", {})
    additions = []
    for registry, entries in patches.items():
        for name, expected in entries.items():
            existing = current.get(registry, {}).get(name)
            if existing is not None:
                if existing != expected:
                    raise ValueError(f"conflicting patch for {name} in {target}")
                continue
            additions.append(f"\n[patch.{json.dumps(registry)}.{json.dumps(name)}]\n")
            for key, value in expected.items():
                if not isinstance(value, str):
                    raise ValueError(f"unsupported patch value for {registry}.{name}.{key}")
                additions.append(f"{key} = {json.dumps(value)}\n")
    if additions:
        target.write_text(original + "".join(additions))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args()
    apply(REPO / "Cargo.toml", args.manifest)


if __name__ == "__main__":
    main()
