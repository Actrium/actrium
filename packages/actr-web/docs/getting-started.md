# Getting Started

This guide covers the current Actor-RTC Web browser path: Option U / wasm-bindgen guests loaded by `actor.sw.js`.

## Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` for wasm-bindgen builds.
- `wasm-pack` for browser wasm packages.
- Node.js and `pnpm`.
- A modern browser with Service Worker, WebRTC, WebSocket, and IndexedDB support.

Install dependencies:

```bash
cd packages/actr-web
pnpm install
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Native Component Model packaging still uses the repository-level `wasm32-wasip2` toolchain described in the root `AGENTS.md`. The browser package instead embeds the wasm-bindgen output described below.

## Runtime Artifacts

The current browser runtime expects:

- a signed `.actr` package whose binary is the wasm-bindgen `*_bg.wasm`
- signed JavaScript glue inside that package at `resources/glue.js`
- `actor.sw.js`
- at least one usable static trust anchor whose 32-byte Ed25519 public key reaches the Service Worker as `pubkey_b64`

`actor.sw.js` verifies the `.actr`, extracts its binary and `resources/glue.js`, instantiates that authenticated wasm-bindgen guest, and calls `register_guest_workload`. It does not fetch or execute a sibling `.wbg/` directory. jco-transpiled bundles are not part of the current browser runtime path.

Package verification fails closed. Empty trust configuration, registry-only trust, or a browser-visible `pubkey_file` that has not been resolved to `pubkey_b64` aborts initialization; none of these cases skips verification.

## Run the Echo Example

```bash
cd packages/actr-web
pnpm install
cd examples/echo
bash start-mock.sh
```

The script builds the browser guest, packages its wasm and glue into signed `.actr` files, starts the in-repo mock actrix service, serves the web client/server through `actr run --web`, and drives the browser smoke suite.

## Legacy Data-Stream Example

`examples/data-stream-peer-concurrent` is not part of the current setup or release smoke flow. Its custom Service Workers still execute loose, unverified wasm-bindgen JavaScript and WASM, so `start.sh` refuses to launch by default. Use the signed-package echo example above until the data-stream example is migrated to the canonical `.actr` loader.

## Rebuild CLI-Embedded Web Runtime Assets

When `crates/sw-host` or `packages/web-sdk/src/actor.sw.js` changes, refresh the assets embedded by `actr-cli`:

```bash
cd packages/actr-web
bash crates/sw-host/build.sh
bash scripts/sync-cli-assets.sh --build
```

The sync script copies canonical web runtime assets into `crates/actr-cli/assets/web-runtime/`. Rebuild `actr-cli` after syncing if you need the CLI binary to embed the new bytes.

## React Integration

`@actrium/actr-web-react` currently re-exports the hooks defined in `packages/actr-web/packages/web-react/src/index.ts`:

- `useActorClient`
- `useServiceCall`
- `useSubscription`

Use those hooks directly or wrap them in application-specific context. Do not rely on a generic actor hook unless your application defines it locally.

## Next Steps

- Read [troubleshooting.md](./troubleshooting.md) for environment and runtime checks.
- Read [error-handling.md](./error-handling.md) for error categories and reporting guidance.
- Read [requirements.md](./requirements.md) for current goals and platform constraints.
- Use [docs/README.md](./README.md) to find historical architecture notes.
