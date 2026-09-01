# Echo at Actrium

This example exercises a complete Actor call from a sandboxed Keel Guest in
Actrium to an Echo Actor:

```text
Echo Keel Guest
  -> Actrium `actor.call` Host capability
  -> loopback-only client Actor Host
  -> Loran discovery and signaling
  -> WebRTC
  -> Wasmtime Echo Actor
```

The loopback gateway is a Host boundary, not an application-facing network
API. It accepts generic Actor route keys and protobuf payload bytes, listens
only on `127.0.0.1`, and dispatches into the signed client workload. The Keel
Guest cannot access signaling credentials, Actor IDs, or WebRTC directly.

## Run on macOS

From this directory:

```bash
./run-macos.sh
```

The runner builds all three signed `.actr` packages, starts Loran and the two Actor
Hosts, verifies one real echo call, builds Actrium, and opens the Echo UI package.
Use `--no-open` for a non-interactive build and end-to-end verification.

The Guest package declares the `actor.call` Host capability in `app/manifest.toml`.
Its only Actor-specific knowledge is the Echo protobuf wire format and the
route key `echo.EchoService.Echo`.

## Components

- `app/`: the Echo Keel Guest loaded by Actrium.
- `../../actr/rust/package-echo/client-guest/`: the client workload that discovers
  and calls the remote Echo service.
- `../../actr/rust/package-echo/client/`: the client Actor Host and Actrium's
  loopback capability gateway.
- `../../actr/rust/echo-actr/`: the Wasmtime Echo Actor workload.
- `../../../crates/loran-server/`: Loran infrastructure used for discovery, signaling,
  identity, STUN, and TURN in this example.

The HTTP gateway address defaults to `127.0.0.1:8391`. A Host embedding can
override the endpoint with `globalThis.__ACTRIUM_ACTR_GATEWAY_URL__` before the
Actrium runtime starts.
