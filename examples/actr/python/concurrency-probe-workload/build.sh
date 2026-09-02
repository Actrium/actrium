#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTR_ROOT="$(cd "${HERE}/../../.." && pwd)"
WORLD="actr-workload-guest-v2"
WORLD_MODULE="actr_workload_bindings"
BINDINGS_DIR="${HERE}/bindings"
DIST_DIR="${HERE}/dist"
OUT_WASM="${DIST_DIR}/concurrency-probe-python-0.2.0-wasm32-wasip2.wasm"
VENV_DIR="${HERE}/.venv"

ensure_cli_web_runtime_assets() {
    local cli_sw_host_wasm="${ACTR_ROOT}/crates/actr-cli/assets/web-runtime/actr_sw_host_bg.wasm"
    local cli_sw_host_js="${ACTR_ROOT}/crates/actr-cli/assets/web-runtime/actr_sw_host.js"
    if [[ ! -f "${cli_sw_host_wasm}" || ! -f "${cli_sw_host_js}" ]]; then
        (
            cd "${ACTR_ROOT}"
            bash packages/actr-web/scripts/sync-cli-assets.sh --build
        )
    fi
}

if [[ ! -d "${VENV_DIR}" ]]; then
    python3 -m venv "${VENV_DIR}"
fi

# shellcheck disable=SC1091
source "${VENV_DIR}/bin/activate"
python -m pip install --upgrade pip >/dev/null
python -m pip install -r "${HERE}/requirements.txt"

if [[ -n "${ACTR_CMD:-}" ]]; then
    ACTR=( "${ACTR_CMD}" )
elif command -v actr >/dev/null 2>&1; then
    ACTR=( actr )
else
    ensure_cli_web_runtime_assets
    ACTR=( cargo run --manifest-path "${ACTR_ROOT}/Cargo.toml" -p actr-cli -- )
fi

(
    cd "${HERE}"
    "${ACTR[@]}" deps install
    rm -rf generated
    "${ACTR[@]}" gen -l python --input protos --output generated
)

rm -rf "${BINDINGS_DIR}"
actr-workload bindings "${BINDINGS_DIR}" \
    --world "${WORLD}" \
    --world-module "${WORLD_MODULE}"

mkdir -p "${DIST_DIR}"
actr-workload componentize workload \
    -o "${OUT_WASM}" \
    --project-dir "${HERE}" \
    --bindings-dir "${BINDINGS_DIR}" \
    --world "${WORLD}" \
    --world-module "${WORLD_MODULE}" \
    --python-path "${ACTR_ROOT}/packages/actr-python/actr-workload/src"

if command -v wasm-tools >/dev/null 2>&1; then
    wasm-tools component wit "${OUT_WASM}" > "${DIST_DIR}/concurrency-probe-python.wit.txt"
    grep -q "actr:workload" "${DIST_DIR}/concurrency-probe-python.wit.txt"
fi

echo "Done. Component at: ${OUT_WASM}"
