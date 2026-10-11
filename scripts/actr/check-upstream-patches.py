#!/usr/bin/env python3
"""Verify the single RTC Git snapshot and legacy TURN source overrides."""

import argparse
import json
from pathlib import Path
import subprocess
import tomllib


REPO = Path(__file__).resolve().parents[2]
CLIENT_CRATES = ("webrtc", "rtc", "rtc-turn", "rtc-rtp")
SERVER_CRATES = ("turn", "stun", "webrtc-util")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=REPO / "Cargo.toml")
    parser.add_argument("--client-only", action="store_true")
    args = parser.parse_args()
    manifest = args.manifest.resolve()
    config = tomllib.loads((REPO / "Cargo.toml").read_text())
    overrides = config["patch"]["crates-io"]
    client_source = config["workspace"]["dependencies"]["webrtc"]
    for name in ("rtc", "rtc-rtp"):
        dependency = config["workspace"]["dependencies"][name]
        if (dependency.get("git"), dependency.get("rev")) != (client_source["git"], client_source["rev"]):
            raise SystemExit(f"{name} must use the same Git snapshot as webrtc")
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--manifest-path", str(manifest)],
        cwd=manifest.parent,
        text=True,
    )
    metadata = json.loads(output)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    names = CLIENT_CRATES if args.client_only else CLIENT_CRATES + SERVER_CRATES
    resolved = {}
    for name in names:
        matching = [package for package in packages.values() if package["name"] == name and package["id"] in nodes]
        if len(matching) != 1:
            raise SystemExit(f"expected one resolved {name}, found {len(matching)}")
        package = matching[0]
        override = client_source if name in CLIENT_CRATES else overrides[name]
        expected = f'git+{override["git"]}?rev={override["rev"]}#{override["rev"]}'
        if package["source"] != expected:
            raise SystemExit(f'{name} resolved to {package["source"]}, expected {expected}')
        resolved[name] = package["id"]

    consumers = ("actr-hyper",) if args.client_only else ("actr-hyper", "actrix-turn")
    for name in consumers:
        matching = [package for package in packages.values() if package["name"] == name and package["id"] in nodes]
        if len(matching) != 1:
            raise SystemExit(f"expected one resolved {name}, found {len(matching)}")
        resolved[name] = matching[0]["id"]

    edges = [
        ("actr-hyper", "webrtc"),
        ("actr-hyper", "rtc"),
        ("actr-hyper", "rtc-rtp"),
        ("webrtc", "rtc"),
        ("rtc", "rtc-turn"),
        ("rtc", "rtc-rtp"),
    ]
    if not args.client_only:
        edges += [
            ("actrix-turn", "turn"),
            ("actrix-turn", "webrtc-util"),
            ("turn", "stun"),
            ("turn", "webrtc-util"),
        ]
    for parent, dependency in edges:
        actual = {dep["pkg"] for dep in nodes[resolved[parent]]["deps"]}
        if resolved[dependency] not in actual:
            raise SystemExit(f"{parent} does not use the expected {dependency} source")
    print(f"Verified RTC sources and TURN overrides for {manifest}")


if __name__ == "__main__":
    main()
