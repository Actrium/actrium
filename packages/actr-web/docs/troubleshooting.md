# Troubleshooting

This document covers common issues in the current Actor-RTC Web Option U / wasm-bindgen path.

## Build Failures

### Workspace dependencies are missing

Install from the web workspace root:

```bash
cd packages/actr-web
pnpm install
```

### Service Worker host build fails

Check the browser wasm toolchain:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Then rebuild:

```bash
cd packages/actr-web
bash crates/sw-host/build.sh
```

If you need the CLI-embedded runtime assets, run:

```bash
cd packages/actr-web
bash scripts/sync-cli-assets.sh --build
```

### Packaged wasm-bindgen guest is missing

The current Service Worker executes only artifacts extracted from the verified `.actr`. Inspect the package and confirm it contains:

```text
manifest.toml
manifest.sig
<manifest binary path>       # wasm-bindgen *_bg.wasm
resources/glue.js            # signed wasm-bindgen JavaScript glue
```

The manifest must hash the binary and list/hash `resources/glue.js` as a resource. A sibling `.wbg/` directory is not a runtime input.

The echo script lays this out automatically:

```bash
cd packages/actr-web/examples/echo
bash start-mock.sh
```

### CLI serves stale web runtime assets

Refresh the CLI asset copy:

```bash
cd packages/actr-web
bash scripts/sync-cli-assets.sh --build
```

Then rebuild the CLI binary that embeds `crates/actr-cli/assets/web-runtime/`.

## Runtime Failures

### Service Worker does not register

Check:

- the page is served from `http://127.0.0.1`, `http://localhost`, or HTTPS
- `actor.sw.js` is reachable at the configured Service Worker path
- the browser devtools Application panel does not show an old waiting worker
- the page and Service Worker are under the same origin and compatible scope

### Guest registration fails

The current path should call `register_guest_workload` from `actor.sw.js`. If registration fails, confirm:

- package signature and manifest hashes verify
- the package binary is the expected wasm-bindgen `*_bg.wasm`
- `resources/glue.js` is present in the signed resource list and evaluates without a JavaScript exception
- browser devtools show Service Worker logs, not only DOM page logs

### Package verification reports no usable static trust anchor

The browser verifier never skips package verification. Confirm the runtime configuration delivered to the Service Worker contains a static anchor with a non-empty `pubkey_b64`. Registry lookup is not implemented in the browser, and the browser cannot read `pubkey_file` itself; a host using that field must resolve it to `pubkey_b64` before worker initialization.

### Cannot connect to signaling

For local example runs, prefer the mock loran scripts:

```bash
cd packages/actr-web/examples/echo
bash start-mock.sh
```

If connecting manually, verify:

- the WebSocket URL is correct
- the HTTP health endpoint is reachable
- the realm and package identity are registered
- local firewalls or proxies are not blocking WebSocket traffic

### IndexedDB errors

Common causes:

- private browsing mode disables or restricts IndexedDB
- browser storage quota is exhausted
- the page origin changed and the expected database is under a different origin

Basic guard:

```ts
if (!window.indexedDB) {
  console.error('IndexedDB is not supported');
}
```

### WebRTC setup fails

Debug in this order:

1. Signaling WebSocket connects.
2. Offer/answer exchange completes.
3. ICE candidates are exchanged.
4. Data channels open.
5. State-path RPC works before fast-path flows are tested.

## Debugging Strategy

1. Start with `examples/echo/start-mock.sh`.
2. Confirm `actor.sw.js` and the signed `.actr` package return HTTP 200, then inspect Service Worker logs for package verification/extraction failures.
3. Compare DOM-side console logs with Service Worker logs.
4. Clear old Service Worker registrations and reload if the browser keeps stale assets.
5. Do not use the legacy data-stream example as a runtime check until it has migrated to signed `.actr` loading.
