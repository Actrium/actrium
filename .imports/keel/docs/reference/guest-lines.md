# Guest Lines: JS Guest vs Native Guest

Rill runs guest apps in **two distinct execution lines**. Their SDK names differ by a
single character (`rill/guest` vs `rill-guest`) yet mean opposite things, and the old
"rill vs wasm" shorthand is misleading. This page fixes the canonical terminology and
records the naming wart so docs, issues, and code all say the same thing.

## The real axis: interpreted-JS vs compiled-native

The distinguishing axis is **not** "wasm or not" — on the web *both* lines compile to
wasm (QuickJS ships as `quickjs-sandbox.wasm` for `sandbox: 'wasm-quickjs'`, and native
guests are Rust→wasm). The axis that actually separates them is **how the guest app is
authored and executed**: interpreted JavaScript, or compiled-native code.

| | **JS guest** | **native guest** |
|---|---|---|
| App written in | JavaScript / React | Rust / C / Zig → wasm |
| Executed by | a JS engine (interpreted) | `WasmGuestHost` (direct wasm) |
| Selected via | `EngineOptions.sandbox` — `'node-vm' \| 'jsc' \| 'hermes' \| 'quickjs' \| 'wasm-quickjs' \| 'tenant-manager'` (`src/host/engine/types.ts`) | the `WasmGuestHost` path (`src/host/wasm-guest/`) |
| SDK | **`rill/guest`** — JS package (`package.json` exports `"./guest" → ./src/sdk`) | **`rill-guest`** — Rust crate (`crates/rill-guest`) |
| Isolation | JS engine context (QuickJS/JSC/Hermes/Node VM); on web, WASM linear-memory | WASM linear-memory boundary |

The `rill-guest` crate describes itself as *"Rust SDK for writing rill **native
(non-JS)** WASM guests"* — the code already knows what it is; only the name failed to
say so.

## Canonical terms — use these everywhere

- **JS guest** — the interpreted-JavaScript line: an app written in JS/React, run by any
  JS engine (including `wasm-quickjs`), via the **`rill/guest`** SDK.
- **native guest** — the compiled-native line: an app written in Rust (or C/Zig),
  compiled to wasm and run directly by `WasmGuestHost`, via the **`rill-guest`** crate.
  Docs also call this the *native (direct-wasm)* line.

When you must distinguish the two in prose, say **"JS-guest line"** and
**"native-guest (direct-wasm) line"**.

## Deprecated shorthand — do not use

- ❌ **"the rill line" vs "the wasm line."** "wasm" is not a distinguishing axis (both
  lines are wasm on the web), and "rill" names the whole project, not one line.
- ❌ **`runtime: "rill"` / `runtime: "wasm"`.** This is **not a real code field** — it is
  conceptual only. The real selector is `EngineOptions.sandbox` (JS line) versus the
  `WasmGuestHost` path (native line).

## Known naming wart

`rill/guest` (JS SDK) and `rill-guest` (Rust/native crate) differ by one character
(`/` vs `-`) yet name opposite lines — error-prone to read and to write.

**Decided (2026-07-25):** the crate becomes **`rill-native-guest`** and the host class
becomes **`NativeGuestHost`** (the whole `NativeGuest*` family and the
`src/host/native-guest/` dir), with `@deprecated` back-compat aliases so downstream
isn't broken silently. Implementation is deferred until the in-flight feature PRs that
touch the same files land, then done as one clean rename; it is tracked in the naming
issue. Until it ships, this glossary is the source of truth for the terms.

See also: [Sandbox Comparison](./sandbox-comparison.md) (the JS-guest engine backends)
and the native-guest design docs under `docs/native-guest*.zh.md`.
