# component-spike-async (Phase 0.5)

Standalone validation of the **async** end-to-end path for WASM Component Model
+ WIT + wit-bindgen + wasmtime, building on Phase 0 (`examples/actr/experimental/component-spike/`).

Phase 0 proved the synchronous path. This spike answers: **can wasmtime's
`Component::instantiate_async` + `Store::run_concurrent` replace
actr's handwritten cooperative-suspend (asyncify) machinery in
`crates/actr-hyper/src/wasm/host.rs`?**

Each sub-crate declares its own `[workspace]` so the repo-root `Cargo.lock` is
untouched.

The host pins Wasmtime/WASI 48.0.8 and the guest uses wit-bindgen 0.59.0.
All suspendable imports and exports are explicit WIT `async func` declarations.
The host uses `Accessor` for scoped state access and `Store::run_concurrent`
to drive guest tasks. Its helper methods intentionally hold a mutable Store
borrow for each complete dispatch, preserving sequential calls per instance.
The ABI itself supports concurrency; serialization is the harness's policy.

`REPORT.md` records the original 43.0.1 / 0.57.1 experiment. Its ABI discussion
and timings are historical, not claims about this migrated implementation.

## Layout

```
examples/actr/experimental/component-spike-async/
  wit/actr-spike-async.wit   WIT with explicit async function types
  guest/                     Rust workload built to a Component (wasm32-wasip2)
                             with bindgen following WIT async annotations
  host/                      Rust host using wasmtime::component::bindgen
                             with `imports: { default: async | trappable }`
                             and `Store::run_concurrent`
  build.sh                   one-shot: build guest, print Component WIT,
                             build + run host (8 tests)
  REPORT.md                  findings + Phase 1 planning notes
```

## Prerequisites

- Rust 1.95+
- `rustup target add wasm32-wasip2`
- `cargo install wasm-tools --version 1.253.0` (for `wasm-tools validate/strip`)
- `cargo install wasm-component-ld --version 0.5.26` — `build.sh` selects this
  Component Model linker explicitly for the guest.

## Run

```
./build.sh
```

## Tests

The host runs eight runtime probes. The report describes the original run;
use the current output to assess the migrated ABI.

1. basic async dispatch round-trip (guest awaits a host import)
2. concurrent dispatches on DIFFERENT instances (expect ~50ms, not 100ms)
3. two dispatches on the SAME instance — harness helpers enforce serial calls
4. host thread free during guest await (tokio ticker test)
5. guest-side `.await` ergonomics for explicit WIT async functions
6. throughput: 100 sequential dispatches + overhead per call
7. error variant propagation across async boundary
8. guest panic AFTER a suspension point -> Trap

## See also

- `REPORT.md` — full findings, versions, WIT/bindgen syntax gotchas, and
  recommendations for Phase 1.
- `examples/actr/experimental/component-spike/REPORT.md` — Phase 0 baseline.
- `crates/actr-hyper/src/wasm/host.rs` — the handwritten cooperative-suspend driver
  this spike aims to replace.
