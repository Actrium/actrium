#!/usr/bin/env python3
"""Fail mobile packaging on Realm private keys/invites without printing secrets.

Accept source asset/resource directories or APK/IPA zip files. This is a
packaging gate for structured credentials, not a general secret scanner.
"""
import json
import pathlib
import re
import sys
import zipfile


def sensitive(data):
    text = data.decode("utf-8", errors="ignore")
    if re.search(r"(?m)^\s*realm_client_key\s*=", text):
        return True
    if re.search(r"\bri_[A-Za-z0-9_-]{40,}", text):
        return True
    if "-----BEGIN PRIVATE KEY-----" in text:
        return True
    try:
        value = json.loads(text)
    except (ValueError, RecursionError):
        return False

    def walk(value):
        if isinstance(value, dict):
            if value.get("schema") in ("actr.realm-client-key/v1", "actr.realm-invite/v1"):
                return True
            if any(key in value for key in ("private_key", "secret_key", "signing_key")):
                return True
            return any(walk(child) for child in value.values())
        return isinstance(value, list) and any(walk(child) for child in value)

    return walk(value)


def main(paths):
    failed = False
    for name in paths:
        path = pathlib.Path(name)
        if path.suffix.lower() in (".apk", ".ipa"):
            with zipfile.ZipFile(path) as archive:
                entries = ((entry.filename, archive.read(entry)) for entry in archive.infolist()
                           if not entry.is_dir() and entry.file_size < 16 * 1024 * 1024
                           and ("/assets/" in "/" + entry.filename or entry.filename.endswith((".json", ".toml", ".plist"))))
                for entry, data in entries:
                    if sensitive(data):
                        print(f"Unsafe mobile resource: {path.name}/{entry}", file=sys.stderr)
                        failed = True
        else:
            for entry in path.rglob("*"):
                if entry.is_file() and entry.stat().st_size < 16 * 1024 * 1024 and sensitive(entry.read_bytes()):
                    print(f"Unsafe mobile resource: {entry}", file=sys.stderr)
                    failed = True
    return int(failed)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
