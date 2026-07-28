# Go echo workload

<!-- SPDX-License-Identifier: Apache-2.0 -->

This example implements the async
[`actr-workload-guest-v2`](../../../core/framework/wit-v2/actr-workload.wit)
world (`actr:workload@0.2.0`) in Go. `dispatch` returns the inbound payload
with an `echo: ` prefix, and every lifecycle or observation export accepts the
explicit V2 invocation context.

The previous TinyGo binding generator is no longer maintained. This example
uses the Go backend built into `wit-bindgen` and the build procedure documented
by that project.

## Pinned toolchain

| Tool | Version |
| --- | --- |
| `wit-bindgen` | 0.59.0 |
| patched Go | `go1.25.5-wasi-on-idle` |
| `go.bytecodealliance.org/pkg` | 0.2.2 (generated pin) |
| `wasm-tools` | 1.253.0 |
| Wasmtime Preview 1 reactor adapter | 46.0.1 |

Async Go components currently require the
[`go1.25.5-wasi-on-idle`](https://github.com/dicej/go/releases/tag/go1.25.5-wasi-on-idle)
toolchain. A stock Go 1.25.5 binary has the same version string but lacks the
scheduler integration required by the Component Model async ABI. The build
script provisions only this fork and verifies its pinned archive hash; it never
falls back to a Go binary from `PATH`. The pinned archive currently supports
Linux x86_64 only.

## Build

```bash
WIT_BINDGEN=/path/to/wit-bindgen \
WASM_TOOLS=/path/to/wasm-tools \
./build.sh
```

The script:

1. Generates Go bindings for `actr-workload-guest-v2`.
2. Copies the checked-in implementation into the generated module.
3. Builds a wasm32-wasip1 reactor with the patched Go compiler.
4. Embeds the V2 WIT and adapts Preview 1 into a component.
5. Validates the component, checks its async V2 workload export and invocation
   context, and rejects any V1 interface.

The fork Go toolchain and pinned Wasmtime adapter are downloaded into `.cache`
and hash-checked automatically. `WASI_ADAPTER` may point to an existing adapter
copy, which is still verified before use. The final component is:

```text
dist/echo-go-0.1.0-wasm32-wasip2.wasm
```

The implementation does not call a host function, so componentization may
trim the unused `host` import from the inferred component world.
CI additionally loads the component through actr's Wasmtime V2 host, executes
`on-start`, dispatches an echo request, and checks the returned payload.

Pass `package` to `build.sh` to additionally invoke
`actr build --no-compile`.

Generated bindings, intermediate core Wasm, and final artifacts are excluded
from version control and reproduced by CI.
