# Guest Lines: JS Guest vs Native Guest

Keel runs guest apps in **two distinct execution lines**. They used to be told apart by
the old "keel vs wasm" shorthand, which is misleading — see the axis below. This page
fixes the canonical terminology so docs, issues, and code all say the same thing.

## The real axis: interpreted-JS vs compiled-native

The distinguishing axis is **not** "wasm or not" — on the web *both* lines compile to
wasm (QuickJS ships as `quickjs-sandbox.wasm` for `sandbox: 'wasm-quickjs'`, and native
guests are Rust→wasm). The axis that actually separates them is **how the guest app is
authored and executed**: interpreted JavaScript, or compiled-native code.

| | **JS guest** | **native guest** |
|---|---|---|
| App written in | JavaScript / React | Rust / C / Zig → wasm |
| Executed by | a JS engine (interpreted) | `NativeGuestHost` (direct wasm) |
| Selected via | `EngineOptions.sandbox` — `'node-vm' \| 'jsc' \| 'hermes' \| 'quickjs' \| 'wasm-quickjs' \| 'tenant-manager'` (`src/host/engine/types.ts`) | the `NativeGuestHost` path (`src/host/native-guest/`) |
| SDK | **`keel/guest`** — JS package (`package.json` exports `"./guest" → ./src/sdk`) | **`keel-native-guest`** — Rust crate (`crates/keel-native-guest`) |
| Isolation | JS engine context (QuickJS/JSC/Hermes/Node VM); on web, WASM linear-memory | WASM linear-memory boundary |

The `keel-native-guest` crate describes itself as *"Rust SDK for writing keel **native
(non-JS)** WASM guests"* — the code already knows what it is; only the name failed to
say so.

## Canonical terms — use these everywhere

- **JS guest** — the interpreted-JavaScript line: an app written in JS/React, run by any
  JS engine (including `wasm-quickjs`), via the **`keel/guest`** SDK.
- **native guest** — the compiled-native line: an app written in Rust (or C/Zig),
  compiled to wasm and run directly by `NativeGuestHost`, via the **`keel-native-guest`** crate.
  Docs also call this the *native (direct-wasm)* line.

When you must distinguish the two in prose, say **"JS-guest line"** and
**"native-guest (direct-wasm) line"**.

## Deprecated shorthand — do not use

- ❌ **"the keel line" vs "the wasm line."** "wasm" is not a distinguishing axis (both
  lines are wasm on the web), and "keel" names the whole project, not one line.
- ❌ **`runtime: "keel"` / `runtime: "wasm"`.** This is **not a real code field** — it is
  conceptual only. The real selector is `EngineOptions.sandbox` (JS line) versus the
  `NativeGuestHost` path (native line).

## The naming wart, and how it was resolved

The Rust crate used to be called `keel-guest`, one character away from the JS SDK's
`keel/guest` (`-` versus `/`) while naming the opposite line — error-prone to read and
to write. Decided 2026-07-25, shipped since:

| was | is now |
|---|---|
| `keel-guest` crate (`crates/keel-guest`) | **`keel-native-guest`** (`crates/keel-native-guest`) |
| `keel_guest::keel_guest_main!` | `keel_native_guest::guest_main!` |
| `WasmGuestHost` / `WasmGuestView` / `WasmGuestEngine` (+ their `*Options`) | **`NativeGuestHost`** / `NativeGuestView` / `NativeGuestEngine` |
| `createWasmGuestEngine` | `createNativeGuestEngine` |
| `src/host/wasm-guest/` | `src/host/native-guest/` |

The five TypeScript names were exported from `keel/host`, so the old spellings remain
exported as `@deprecated` aliases rather than breaking downstream silently. The Rust
side took a clean break: the crate is `publish = false` and every caller is in this
repository, so there is no downstream to alias for.

Two things deliberately kept their names. The public JS export stays **`keel/guest`** —
it is the primary line and a published entry point, and `keel/js-guest` would buy
symmetry at the cost of churning the API everyone actually uses. The C guest SDK stays
`sdk/c/keel_guest.h`: renaming a language-neutral header and its `keel_guest_*` symbols
is a separate breaking change that the naming issue never asked for.

See also: [Sandbox Comparison](./sandbox-comparison.md) (the JS-guest engine backends)
and the native-guest design docs under `docs/native-guest*.zh.md`.
