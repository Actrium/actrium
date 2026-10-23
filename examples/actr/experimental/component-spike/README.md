# component-spike (Phase 0)

Standalone proof-of-concept validating that **WASM Component Model + WIT + wit-bindgen + wasmtime**
can serve as the substrate for a future actr workload contract.

This spike is isolated from the main workspace: each sub-crate declares its own `[workspace]`,
so `cargo build` at the repo root does not see it and the root `Cargo.lock` is never touched.

The host pins Wasmtime/WASI 48.0.7 for the current security fixes. `REPORT.md`
records the original 43.0.1 experiment and is a historical measurement, not
an instruction to restore that dependency version.

## Layout

```
examples/actr/experimental/component-spike/
  wit/actr-spike.wit    minimal WIT contract (records, variants, nested records, result types)
  guest/                Rust workload compiled to a Component (wasm32-wasip2)
  host/                 Rust host using wasmtime::component + wit-bindgen to load + dispatch
  build.sh              one-shot: build guest, print Component WIT, build + run host
  REPORT.md             findings + recommendations
```

## Prerequisites

- Rust 1.95+
- `rustup target add wasm32-wasip2`
- `cargo install wasm-tools --version 1.253.0`
- Optional for Q14/Q15 probes:
  - Node 20+ for jco (`npm i @bytecodealliance/jco`)
  - `cargo install wit-bindgen-cli --version 0.57.1`

## Run

```
./build.sh
```

Expected tail:

```
dispatch reply: "echo: hello"
1000 dispatches: ~6 ms total, ~6 us/call
=== spike OK ===
```

## See also

- `REPORT.md` — technical findings and recommendations for Phase 1.
