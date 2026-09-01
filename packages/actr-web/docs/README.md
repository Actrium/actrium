# Actor-RTC Web Documentation

Actor-RTC Web currently uses the Option U / wasm-bindgen browser path. The wasm-bindgen `*_bg.wasm` is the signed `.actr` package binary, and its JavaScript glue is the signed package resource `resources/glue.js`. The browser Service Worker entry `actor.sw.js` verifies the package, extracts both artifacts directly from it, and installs the guest dispatch function through `register_guest_workload`. No sibling `.wbg/` bundle is executed.

Component Model and jco documents are retained as historical design context only.

## Current User Documentation

- [Getting started](./getting-started.md): setup, current commands, example entry points, and React hooks.
- [Troubleshooting](./troubleshooting.md): build, signed package contents, Service Worker, signaling, WebRTC, and storage checks.
- [Error handling](./error-handling.md): how browser runtime errors should be categorized and reported.
- [Requirements](./requirements.md): current goals, non-goals, usage modes, and platform constraints.

## Current Examples

- [Echo example](../examples/echo/README.md): primary browser smoke path. Run with `bash examples/echo/start-mock.sh` from `packages/actr-web`.

## Legacy Examples

- [Data-stream peer concurrent example](../examples/data-stream-peer-concurrent/README.zh.md): retained for historical Fast Path debugging. Its custom Service Workers execute loose, unverified wasm-bindgen assets, so its launcher refuses to run by default. It is not a current security or release smoke path.

## Current Build and Asset Commands

```bash
cd packages/actr-web
pnpm install
```

```bash
cd packages/actr-web
bash crates/sw-host/build.sh
bash scripts/sync-cli-assets.sh --build
```

## Architecture and Historical Notes

Use these when you need implementation background. They are not the canonical user setup path.

- [Architecture notes](./architecture/README.zh.md)
- [Option U WIT compile web notes](./option-u-wit-compile-web.zh.md)
- [2026-04 architecture change notes](./architecture-changes-2026-04.zh.md)
- [Historical jco async-lift investigation](./t18-jco-async-lift-hang.zh.md)
- [Tech debt notes](./tech-debt.zh.md)

## Source-of-Truth Pointers

- `packages/web-sdk/src/actor.sw.js`: Service Worker entry used by the current wasm-bindgen browser path.
- `crates/sw-host/src/guest_bridge.rs`: `register_guest_workload` and guest dispatch bridge.
- `crates/sw-host/build.sh`: builds the Service Worker host wasm-bindgen artifacts.
- `scripts/sync-cli-assets.sh`: syncs canonical web assets into `crates/actr-cli/assets/web-runtime/`.
- `packages/web-react/src/index.ts`: public React hook exports.
