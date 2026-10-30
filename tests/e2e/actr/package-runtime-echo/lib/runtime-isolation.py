#!/usr/bin/env python3
"""Run an E2E Host without access to the Publisher's signing directory.

This protects the test's signing material, not arbitrary host resources. Linux
uses Landlock (or a mount/PID namespace); macOS uses a Seatbelt rule. Both fail
closed if the OS cannot establish the boundary. No Publisher key is passed in
the child environment.
"""

import argparse
import ctypes
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import sys


RUNTIME_ENV = {
    "HOME", "PATH", "LANG", "LC_ALL", "TZ", "TMPDIR", "RUST_LOG",
    "ECHO_ACTR_VERSION", "ISSUER_ENDPOINT", "CLIENT_RUNTIME_CONFIG_PATH",
    "CLIENT_GUEST_PACKAGE_PATH", "CLIENT_GUEST_PUBLIC_KEY_PATH", "TRUST_MODE",
    "SSL_CERT_FILE",
}


def restrict_linux_files(private_dir):
    """Allow filesystem siblings, excluding the signing subtree and its aliases.

    Landlock ABI 2 is needed for rename/hard-link restrictions. ABI 3 adds
    truncate protection. The policy is inherited by exec and future threads;
    it changes no host-wide setting and needs no mount privilege.
    See https://docs.kernel.org/userspace-api/landlock.html.
    """
    if platform.machine() not in {"x86_64", "aarch64"}:
        raise OSError("Landlock syscall numbers are unsupported on this architecture")
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long

    def syscall(number, *arguments):
        result = libc.syscall(number, *arguments)
        if result < 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))
        return result

    abi = syscall(444, None, 0, 1)  # landlock_create_ruleset(VERSION)
    if abi < 2:
        raise OSError("Landlock ABI 2 or newer is required")

    class Ruleset(ctypes.Structure):
        _fields_ = [("handled_access_fs", ctypes.c_uint64)]

    class PathRule(ctypes.Structure):
        _pack_ = 1
        _fields_ = [("allowed_access", ctypes.c_uint64), ("parent_fd", ctypes.c_int32)]

    rights = (1 << (15 if abi >= 3 else 14)) - 1
    file_rights = 7 | ((1 << 14) if abi >= 3 else 0)
    attribute = Ruleset(rights)
    ruleset = syscall(444, ctypes.byref(attribute), ctypes.sizeof(attribute), 0)
    try:
        def allow(path, access):
            descriptor = os.open(path, os.O_PATH | os.O_CLOEXEC)
            try:
                if not stat.S_ISDIR(os.fstat(descriptor).st_mode):
                    access &= file_rights
                rule = PathRule(access, descriptor)
                syscall(445, ruleset, 1, ctypes.byref(rule), 0)  # PATH_BENEATH
            finally:
                os.close(descriptor)

        # Ancestors can be listed, but cannot be modified or used to read the
        # signing files. Grant other rights only to siblings outside that chain.
        current = Path("/")
        allow(current, 1 << 3)  # READ_DIR
        for component in private_dir.parts[1:]:
            child = current / component
            for sibling in current.iterdir():
                if sibling == child:
                    continue
                try:
                    target = sibling.resolve(strict=True)
                except OSError:
                    continue  # A dangling/unreadable alias grants no access.
                if target.is_relative_to(private_dir) or private_dir.is_relative_to(target):
                    continue
                allow(target, rights)
            current = child
        if libc.prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))
        syscall(446, ruleset, 0)  # landlock_restrict_self
    finally:
        os.close(ruleset)


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
        try:
            restrict_linux_files(private_dir)
        except OSError as error:
            print(f"Landlock unavailable ({error}); trying mount/PID isolation", file=sys.stderr)
        else:
            os.execvpe(command[0], command, environment)
        sandbox = shutil.which("bwrap")
        if sandbox is None:
            parser.error("Landlock ABI 2+ or bubblewrap with working namespaces is required")
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
