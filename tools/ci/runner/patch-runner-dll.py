#!/usr/bin/env python3
"""Patch the Actions runner worker for the in-cluster Actions cache server.

The runner normally writes GitHub's hosted results endpoint into the job's
ACTIONS_RESULTS_URL environment key. Renaming that worker literal to
ACTIONS_RESULTS_ORL moves the generated assignment to a separate key, preserving
the externally configured ACTIONS_RESULTS_URL inherited by cache clients.
The ConfigMap therefore retains the original key. The cache server must support
the protocol selected by the job; verify actual restores/saves after upgrades.

The rename keeps the UTF-16LE byte length identical, so the surrounding .NET
metadata is untouched and the replacement is a pure in-place substitution. The
script asserts that exactly one occurrence exists: a runner version bump that
changes the string must fail the image build loudly rather than silently
produce an unpatched runner.

Usage: python3 patch-runner-dll.py  (the DLL path is fixed to the layout of
the runner agent installed in the image).
"""

from pathlib import Path

DLL = Path("/home/runner/actions-runner/bin/Runner.Worker.dll")
OLD = "ACTIONS_RESULTS_URL"
NEW = "ACTIONS_RESULTS_ORL"


def main() -> None:
    """Rename the single ACTIONS_RESULTS_URL literal in the worker assembly."""
    data = DLL.read_bytes()
    old = OLD.encode("utf-16-le")
    new = NEW.encode("utf-16-le")
    if len(old) != len(new):
        raise SystemExit(f"replacement must keep the byte length: {OLD} -> {NEW}")

    count = data.count(old)
    if count != 1:
        raise SystemExit(
            f"expected exactly 1 occurrence of {OLD} in {DLL}, found {count}; "
            "the runner agent version probably changed - re-verify the cache patch"
        )

    DLL.write_bytes(data.replace(old, new))
    print(f"patched {DLL}: {OLD} -> {NEW} (1 occurrence)")


if __name__ == "__main__":
    main()
