#!/bin/sh
# Runtime entrypoint for the Actrium ARC runner container.
#
# Everything that can be prepared ahead of time is baked into the image at
# build time: wasm-opt, sccache, the hosted-toolcache PATH entry and the
# Actions-cache worker patch. This script only performs the work that must
# happen per Pod:
#
#   1. Start the sccache daemon while the Pod is still idle. RUSTC_WRAPPER
#      would start it lazily on the first compile, and a cold start in the
#      middle of a compile burst exceeded the client startup timeout under
#      fleet load ("Timed out waiting for server startup"). A failure here is
#      deliberately tolerated: a broken cache must never stop a job from
#      running, and sccache is started lazily as a fallback anyway.
#   2. exec the runner agent. ARC injects the registration as
#      ACTIONS_RUNNER_INPUT_JITCONFIG, which run.sh consumes. exec keeps the
#      agent as PID 1, so Kubernetes SIGTERM/SIGINT reach it directly and it
#      can deregister and drain gracefully (RUNNER_MANUALLY_TRAP_SIG=1 is set
#      in the Dockerfile for that reason).
#
# An explicit command (for example `docker run <image> <cmd>`, or the
# `container:` workflow mode) bypasses the default agent startup below.
set -eu

if command -v sccache >/dev/null 2>&1; then
    sccache --start-server || true
fi

if [ "$#" -gt 0 ]; then
    exec "$@"
fi

exec /home/runner/actions-runner/run.sh
