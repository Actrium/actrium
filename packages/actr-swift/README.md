# Actr

Swift bindings and the repository-local Swift package for ACTR (Actrium).

## Overview

- **Monorepo `Package.swift`**: Uses an XCFramework built from the current
  checkout for local development and CI validation. It never falls back to a
  published binary.
- **Published package**: The external `actr-swift-package-sync` repository
  distributes a prebuilt iOS/macOS XCFramework through a remote `binaryTarget`.
- **Actr**: Swift API that includes UniFFI-generated bindings and high-level helpers.

## Relationship to the Rust Node Typestate

The native host exposes a typestate chain
`Node<Init> → Node<Attached> → Node<Registered> → ActrRef`
(`from_config_file` → `attach_*` → `register` → `start`) so Rust-side
system code can hook into each transition. The Swift API collapses the
pipeline into a one-shot `ActrNode.from(packageConfig:packagePath:)`
followed by `start()`: iOS/macOS app developers only see the node and
the live `ActrRef`. The `Node<S>` typestate is intentionally Rust-layer
power-user territory — bindings do not re-export it. If fine-grained
control is required (custom `TrustProvider`, pre-built `Hyper`,
attaching a Rust `Workload`, etc.), use the `actr_hyper::{Hyper, Node}`
API directly from native Rust.

Package-backed Swift hosts can pass `RuntimeObservers` to
`ActrNode.from(packageConfig:packagePath:observers:)` to receive signaling and
target-scoped WebRTC readiness callbacks. Treat signaling connection as
service availability only; retry saved user intent with a fresh send after the
matching `WebRTCObserver.onConnected(ctx:event:)` target callback.

Application code importing `Actr` uses Swift-facing names such as `Context`,
`Workload`, `RpcEnvelope`, `PeerEvent`, `WebRTCObserver`, and
`WebRTCPeerStatus`. The corresponding transport-oriented types remain available
through the low-level `ActrBindings` module.

## Workspace Layout

The Swift build scripts build `libactr` from the monorepo workspace root.

```text
actrium/
├── Cargo.toml                # Rust workspace root
├── crates/
│   └── actr-ffi/             # libactr crate
└── packages/
    └── actr-swift/           # Swift package sources and build scripts
```

The package-sync repository owns its own workflow definitions.
The monorepo release train only dispatches that external release workflow with `version`, `source_sha`, and `source_tag`.

## Consume via SwiftPM

```swift
dependencies: [
    .package(url: "https://github.com/Actrium/actr-swift-package-sync.git", from: "0.1.0")
]
```

Targets that need the SDK should depend on `Actr`.

### Local development without a published binary

For source-tree validation, generate the bindings and XCFramework together from
the current checkout. Both are ignored build artifacts under `dist/`; generated
bindings are not committed in the monorepo. Swift commands fail fast when
required artifacts are missing. External SwiftPM consumers should
depend on the published `actr-swift` package instead of this monorepo package.

Build the artifacts, then use the package defaults:

```bash
./build-xcframework.sh
swift build
```

For custom output locations, pass both overrides to the build script and Swift
commands. For example, the following explicitly selects the default locations:

```bash
ACTR_BINDINGS_PATH=dist/ActrBindings ACTR_BINARY_PATH=dist/ActrFFI.xcframework ./build-xcframework.sh
```

Then consume the package with the same environment variables:

```bash
ACTR_BINDINGS_PATH=dist/ActrBindings ACTR_BINARY_PATH=dist/ActrFFI.xcframework swift build
ACTR_BINDINGS_PATH=dist/ActrBindings ACTR_BINARY_PATH=dist/ActrFFI.xcframework swift test
```

Rebuild both artifacts after changing the Rust FFI or switching revisions.
The package does not fall back to root-level bindings or binaries. Keep any
generated-code adjustments in `build-xcframework.sh` so regeneration preserves
them. The published package-sync repository still commits generated bindings
alongside its manifest referencing the matching released binary.

## Build (maintainers)

Prerequisites:
- Rust 1.95+
- Xcode Command Line Tools
- UniFFI CLI: `cargo install uniffi --features "cli"`

Steps:

```bash
./build-xcframework.sh
```

This generates Swift bindings at `dist/ActrBindings/` and the multi-platform
XCFramework at `dist/ActrFFI.xcframework/`.

## Package for release

Swift publishing is owned by the monorepo release train. Do not add a remote
binary URL, release tag, or checksum to this directory's `Package.swift`; that
manifest must remain local-only so repository validation cannot silently use a
stale published binary.

The release train's `publish-swift` stage dispatches the
`Actrium/actr-swift-package-sync` release workflow. The external workflow checks
out the tagged monorepo source, rebuilds the XCFramework, updates its own
consumer-facing `Package.swift`, and publishes the archive.

For manual artifact inspection:

1. Build the XCFramework: `./build-xcframework.sh`
2. Package it and compute its checksum:
   `./scripts/package-binary.sh v0.1.0`
3. Inspect `dist/ActrFFI.xcframework.zip` and `dist/release.txt`.

The root release workflows consume the same artifact metadata when validating
and publishing releases.

## Project Structure

- `../../crates/actr-ffi/`: Rust FFI crate used to build the XCFramework and generate UniFFI bindings
- `dist/ActrBindings/`: UniFFI-generated Swift bindings (ignored)
- `build-xcframework.sh`: Build script
- `scripts/package-binary.sh`: Zip + checksum helper for Release assets
- `dist/`: Local release artifacts (ignored)
- `Package.swift`: Swift Package manifest

## Configuration

UniFFI configuration lives in `../../crates/actr-ffi/uniffi.toml`.

## Documentation

- **[API Reference](docs/api.md)** - Comprehensive API documentation covering both Low Level and High Level APIs

## License

Apache-2.0
