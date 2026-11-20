#!/usr/bin/env python3
"""Apply an explicitly provisioned Host identity to the signing entitlements."""

from datetime import datetime
from pathlib import Path
import plistlib
import sys


def host_entitlements(profile: dict, info: dict, entitlements: dict) -> dict:
    if "OSX" not in profile.get("Platform", []):
        raise ValueError("Host provisioning profile is for a different platform")
    expires = profile.get("ExpirationDate")
    if not isinstance(expires, datetime) or expires <= datetime.utcnow():
        raise ValueError("Host provisioning profile is expired or missing an expiration date")

    teams = profile.get("TeamIdentifier", [])
    if len(teams) != 1 or not isinstance(teams[0], str) or not teams[0]:
        raise ValueError("Host provisioning profile must identify one signing team")
    bundle_id = info["CFBundleIdentifier"]
    app_id = profile.get("Entitlements", {}).get("com.apple.application-identifier")
    if not isinstance(app_id, str):
        raise ValueError("Host provisioning profile is missing its application identifier")
    prefix, _, profiled_bundle = app_id.partition(".")
    if profiled_bundle != bundle_id or prefix not in profile.get("ApplicationIdentifierPrefix", teams):
        raise ValueError("Host provisioning profile does not match the explicit app bundle identifier")
    return {
        **entitlements,
        "com.apple.application-identifier": app_id,
        "com.apple.developer.team-identifier": teams[0],
    }


def main() -> None:
    profile, info, entitlements, output = map(Path, sys.argv[1:])
    try:
        checked = host_entitlements(
            plistlib.loads(profile.read_bytes()),
            plistlib.loads(info.read_bytes()),
            plistlib.loads(entitlements.read_bytes()),
        )
        output.write_bytes(plistlib.dumps(checked))
    except (KeyError, OSError, ValueError) as error:
        sys.exit(f"ERROR: {error}")


if __name__ == "__main__":
    main()
