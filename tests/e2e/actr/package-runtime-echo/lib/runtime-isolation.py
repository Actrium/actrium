#!/usr/bin/env python3
"""Run an E2E Host without access to the Publisher's signing directory.

This protects the test's signing material, not arbitrary host resources. Linux
uses a mount/PID namespace; macOS uses a Seatbelt file-access rule. Both fail
closed if the OS cannot establish the boundary. No Publisher key is passed in
the child environment.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import sys


RUNTIME_ENV = {
    "HOME", "PATH", "LANG", "LC_ALL", "TZ", "TMPDIR", "RUST_LOG",
    "ECHO_ACTR_VERSION", "ISSUER_ENDPOINT", "CLIENT_RUNTIME_CONFIG_PATH",
    "CLIENT_GUEST_PACKAGE_PATH", "CLIENT_GUEST_PUBLIC_KEY_PATH", "TRUST_MODE",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--private-dir", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a runtime command is required")
    private_dir = args.private_dir.resolve(strict=True)
    if not private_dir.is_dir() or private_dir == Path("/"):
        parser.error("--private-dir must name a dedicated signing directory")

    environment = {key: value for key, value in os.environ.items() if key in RUNTIME_ENV}
    if sys.platform == "darwin":
        sandbox = shutil.which("sandbox-exec")
        if sandbox is None:
            parser.error("sandbox-exec is required for Publisher-key isolation")
        profile = "(version 1)(allow default)(deny file-read* file-write* (subpath {}))".format(
            json.dumps(str(private_dir), ensure_ascii=False)
        )
        launch = [sandbox, "-p", profile, *command]
    elif sys.platform == "linux":
        sandbox = shutil.which("bwrap")
        if sandbox is None:
            parser.error("bubblewrap is required; install it and enable unprivileged namespaces")
        # Hide the signing directory and host processes, including their /proc
        # root/fd aliases. Keep host networking for the real Issuer and TURN.
        launch = [
            sandbox, "--die-with-parent", "--new-session", "--unshare-user",
            "--unshare-pid",
            "--bind", "/", "/", "--proc", "/proc", "--tmpfs", str(private_dir),
            "--chdir", os.getcwd(), "--", *command,
        ]
    else:
        parser.error("Publisher-key isolation is supported on Linux and macOS only")
    os.execve(launch[0], launch, environment)


if __name__ == "__main__":
    main()
