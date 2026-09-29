#!/usr/bin/env python3
"""Patch the Actions runner worker for the in-cluster Actions cache server.

The Actrium ARC fleet stores Actions file caches in an in-cluster
gha-cache-server (falcondev-oss/github-actions-cache-server, MinIO bucket
`gha-cache`) instead of GitHub's hosted cache service. That server implements
the v1 cache API, which `actions/cache` addresses through ACTIONS_CACHE_URL
(falling back to ACTIONS_RESULTS_URL), while the v2 cache service is addressed
through ACTIONS_RESULTS_URL.

The cache-enabled production image therefore ships a worker assembly whose
ACTIONS_RESULTS_URL literal is renamed to ACTIONS_RESULTS_ORL, which keeps the
worker from binding the job's cache traffic to the hosted v2 results endpoint
and lets the fleet's cache requests resolve to the in-cluster v1 server. The
runtime behaviour is what the deployment's cache restore/save check verifies.

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
