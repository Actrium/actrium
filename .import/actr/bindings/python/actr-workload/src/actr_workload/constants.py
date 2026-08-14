from __future__ import annotations

DEFAULT_WORLD = "actr-workload-guest-v2"
DEFAULT_WORLD_MODULE = "actr_workload_bindings"
COMPONENTIZE_PY_VERSION = "0.25.0"
DEFAULT_WIT_RESOURCE = "wit/actr-workload.wit"
ENV_WIT_PATH = "ACTR_WORKLOAD_WIT"
REPO_WIT_PATH = "core/framework/wit-v2/actr-workload.wit"

#: Default RPC timeout (milliseconds) applied by the four public `ActrContext`
#: APIs when the caller does not pass an explicit deadline. Keep this value
#: aligned with the other language runtimes. Generated dispatch code MUST NOT
#: inline it.
DEFAULT_RPC_TIMEOUT_MS: int = 30_000
