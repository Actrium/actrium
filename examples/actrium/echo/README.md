# Echo at Actrium

Actrium can open the published Echo Realm and call its running Service over
WebRTC. This directory also contains a loopback demo that starts a local stack.

## Run the published Echo in Actrium on macOS

The [Echo Realm Profile](https://realm.develenv.com/realm/echo-0-1-4) selects
`publisher.develenv.com/echo-front@0.1.7` and `publisher.develenv.com/echo@0.1.7`.
The Realm address is `actr://realm.develenv.com/echo-0-1-4`. The name is stable
when the Profile changes package versions.

### Start the Service

If the Realm owner already has the Service running, skip to opening Actrium
and use a browser invite. To run your own Service, create a test Realm you own,
select the same published packages, and use your Realm's name and provider
endpoints in the config below.

Build `actr` from this checkout if needed. Install the repository's Rust
and Protobuf build dependencies and `wasm-pack` before running these commands
from the repository root.

```bash
bash packages/actr-web/scripts/sync-cli-assets.sh --build
cargo build --locked -p actr-cli --bin actr
export PATH="$PWD/target/debug:$PATH"
```

The Service requires a Realm owner client key. To enroll a new Service host,
the owner can issue an invite for their own member ID with
`actr realm invite --key <owner-key-file> --member <owner-member-id>`.
On the Service host, run `actr realm join - --output service-owner-client.json`
and pass the invite on stdin.

With the owner client key ready, start the Service from the repository root:

```bash
run_dir="$(mktemp -d)"
cp examples/actrium/echo/realm-service.toml "$run_dir/actr.toml"
install -m 600 /path/to/service-owner-client.json "$run_dir/service-owner-client.json"
curl --fail --show-error --location \
  https://publisher.develenv.com/releases/c6230aeda879e39d322c0e68d30fe8decb356f5ab7d1b00992341f53302a4458.actr \
  --output "$run_dir/echo-service.actr"
actr run --config "$run_dir/actr.toml" --hyper-dir "$run_dir/hyper"
```

Wait for `ActrNode started` and leave this terminal running. The runtime verifies
that the package was signed by the configured Publisher. The example ACL allows
only the selected Echo Front type in the same Realm.

### Open Actrium

On an Apple silicon Mac with the Xcode SDK required by the project, install
Node.js, pnpm, CocoaPods, CMake, Protobuf, and `uniffi-bindgen` 0.31.1. Build
both sets of Swift bindings from the same checkout as the browser.

```bash
pnpm install --frozen-lockfile --ignore-scripts
node packages/actrium/scripts/patch-react-native-macos.js
(cd macos && pod install)
scripts/actrium/build-actrium-rust-bindings.sh apple-macos
ACTR_BUILD_PROFILE=debug ACTR_XCFRAMEWORK_TARGETS=macos \
  CMAKE_POLICY_VERSION_MINIMUM=3.5 packages/actr-swift/build-xcframework.sh
open macos/Actrium.xcworkspace
```

In Xcode, select **Actrium** and **My Mac**, configure your Apple Development
signing identity, and run the app. Keep the bundle ID and signing identity
stable between builds so the browser can reuse its Keychain-backed Realm
identity. If macOS requests Keychain access for your signed build, authorize
it before sending a message.

Enter `actr://realm.develenv.com/echo-0-1-4` in the address bar and press Return.
On first use, ask the Realm owner for a separate browser invite, then enter
it and select **Redeem**.

Enter `hello` in Echo Chat and select **Send**. The conversation should show
`echo: hello`. The browser fetches the Front from the signed Realm Profile and
resolves the running Service Actor of the selected type over the Realm network.

## Run the loopback demo on macOS

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
