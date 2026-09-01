# Migrating from `rill` to `keel`

The project was renamed from **rill** to **keel**. The rename is mechanical — no
behaviour changed, no capability was added or removed — but it is a **breaking**
change on every named surface: the npm package, the guest ABI symbols, the three
binary wire magics, the native module and Android package names, and every
`RILL_*` environment variable.

There is no compatibility shim. A host and a guest must be on the same side of
the rename.

## 1. Package and entry points

| was | is now |
|---|---|
| `rill` | `keel` |
| `rill/host` | `keel/host` |
| `rill/guest` | `keel/guest` |
| `rill/host/preset` | `keel/host/preset` |
| `rill/host/web` | `keel/host/web` |
| `rill/wire` | `keel/wire` |
| `rill/contract` | `keel/contract` |
| `rill/devtools` | `keel/devtools` |
| `rill/cli` | `keel/cli` |
| `rill` CLI binary | `keel` |

The Rust guest SDK crate moved with it: `rill-native-guest` →
`keel-native-guest` (`crates/keel-native-guest`), and its entry macro is
`keel_native_guest::guest_main!`. The C guest header is `sdk/c/keel_guest.h`.

## 2. TypeScript API

Every exported identifier that carried the product name moved:

| was | is now |
|---|---|
| `defineRillContract` | `defineKeelContract` |
| `RillContractShape` | `KeelContractShape` |
| `RillCapabilitiesManifest` | `KeelCapabilitiesManifest` |
| `RillErrorBoundary` / `RillErrorBoundaryProps` | `KeelErrorBoundary` / `KeelErrorBoundaryProps` |
| `RillKeyEvent` | `KeelKeyEvent` |
| `RillPlatformInfo` | `KeelPlatformInfo` |

The generated capability manifest file is now `keel-capabilities.json`.

## 3. Guest ABI (breaking for every prebuilt guest)

A native guest compiled before the rename **will not instantiate** — its import
and export names no longer match. Recompile it against `keel-native-guest`.

Host → guest imports (module `env`):

| was | is now |
|---|---|
| `rill_host_call` | `keel_host_call` |
| `rill_send_batch` | `keel_send_batch` |
| `rill_log` | `keel_log` |

Guest → host exports:

| was | is now |
|---|---|
| `rill_init` | `keel_init` |
| `rill_alloc` | `keel_alloc` |
| `rill_resolve` | `keel_resolve` |
| `rill_on_event` | `keel_on_event` |
| `rill_abi_version` | `keel_abi_version` |
| `rill_wire_caps` | `keel_wire_caps` |

`KEEL_ABI_VERSION` is **unchanged at 1**. The rename does not alter the wire
layout, the calling convention, or the fail-closed semantics — only the spelling
of the symbols — so the ABI version number does not move.

## 4. JS sandbox globals

Every `__rill*` global inside the sandbox is now `__keel*` — `__keel_sendBatch`,
`__keel_emitEvent`, `__keel_onHostEvent`, `__keel_getConfig`,
`__keel_handleMessage`, `__keel_importHostModule`, `__keel_stageBinary`,
`__keel_takeBinary`, and the rest of the family. A guest bundle built against the
old globals must be rebuilt.

## 5. Binary wire magics

All three sibling wires re-spell their 4-byte magic. The framing, field layout,
limits and fail-closed reason vocabulary are **byte-for-byte unchanged** apart
from those four leading bytes.

| contract | was | is now |
|---|---|---|
| op-batch (`contracts/op-batch-wire.json`) | `RILL` `0x4c4c4952` | `KEEL` `0x4c45454b` |
| canvas (`contracts/canvas-wire.json`) | `RCNV` `0x564e4352` | `KCNV` `0x564e434b` |
| store/net bytes (`contracts/store-net-bytes.json`) | `RBS1` `0x31534252` | `KBS1` `0x3153424b` |

The routing discipline survives intact: all three still share byte 0 (now
`0x4B` `'K'`, formerly `0x52` `'R'`) and still differ at byte 1 — `'E'` `0x45`,
`'C'` `0x43`, `'B'` `0x42` — so a host fork that compares the **full `u32`**
routes each frame to exactly one decoder, and a cross-fed buffer still fails
`bad-magic` before any field is read. A `null`/no-bytes call remains plain JSON
and never enters an envelope.

Every golden vector was regenerated from the Rust oracle, so
`op-batch-wire.golden.json`, `op-batch-wire.matrix.json`,
`canvas-wire.golden.json` and `store-net-bytes.golden.json` now lock the new
magics. A decoder that hard-codes the old value rejects the new goldens
immediately, which is the intended fail-closed behaviour.

## 6. Native integration

| was | is now |
|---|---|
| `RillSandboxNative.podspec` / pod `RillSandboxNative` | `KeelSandboxNative.podspec` / pod `KeelSandboxNative` |
| `RillSandboxNativeTurboModule` | `KeelSandboxNativeTurboModule` |
| `RillQuickJSNativeTurboModule` | `KeelQuickJSNativeTurboModule` |
| `RillTenantManager` | `KeelTenantManager` |
| Android package `com.rill.sandbox` | `com.keel.sandbox` |
| Android package `com.rill.demo` (demo app) | `com.keel.demo` |
| JNI entry points `Java_com_rill_sandbox_*` | `Java_com_keel_sandbox_*` |
| JNI shared library `rillsandbox` | `keelsandbox` |
| postinstall `scripts/rill_postinstall.js` / `rill_post_install.rb` | `scripts/keel_postinstall.js` / `keel_post_install.rb` |

Autolinking picks up the new names from `react-native.config.js`
(`com.keel.sandbox.KeelSandboxNativePackage`), so an app that relies on
autolinking only needs the dependency name changed. An app that names the pod or
the package explicitly must update both.

## 7. Environment variables

Every `RILL_*` variable is now `KEEL_*`, with no other change in meaning. The
ones most likely to appear in an integrator's build:

- `RILL_SANDBOX_ENGINE` → `KEEL_SANDBOX_ENGINE` (`jsc` / `hermes` / `quickjs`),
  set in the Podfile to pick the native sandbox engine.
- `RILL_WIP_CDP_DEVTOOLS`, `RILL_WIP_NATIVE_SECURITY`,
  `RILL_WIP_BINARY_PROTOCOL`, `RILL_WIP_JSC_INSPECTOR` → `KEEL_WIP_*`.
- `RILL_QJS_*`, `RILL_WASM_*`, `RILL_IOS_E2E_*`, `RILL_ANDROID_E2E_*` → `KEEL_*`.

## 8. What did NOT change

- The ABI version (`1`), the protocol version (`1`), every limit, every opcode,
  every value tag, every `reason` token.
- The shape of the capability contract, the dispatch surface, the fail-closed
  posture.
- The Apache-2.0 license, the repository history.

Only the repository URL moved with the name:
`https://github.com/Actrium/keel`.
