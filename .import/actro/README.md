# graphviz-anywhere

Graphviz for native runtimes and the web.

This repository now covers three delivery paths:

- Native Graphviz shared libraries for Rust and React Native
- A safe Rust crate on top of the native C ABI
- A WebAssembly-powered web package for browsers, Web Workers, and edge runtimes

Native builds target iOS (XCFramework), Android (`.so`), macOS (`.dylib`), Linux (`.so`) and Windows (`.dll`). Web builds are powered by compiled WebAssembly.

## Architecture

```
graphviz-anywhere/
├── src/                      # C ABI wrapper (graphviz_api.h/.c)
├── scripts/                  # Per-platform native build scripts
├── rust/
│   ├── graphviz-sys/         # Raw FFI bindings (graphviz-anywhere-sys)
│   └── graphviz/             # Safe Rust wrapper (graphviz-anywhere)
├── react-native/             # React Native native module
│   ├── ios/                  # iOS bridge (ObjC)
│   ├── android/              # Android bridge (Java + JNI)
│   ├── macos/                # macOS bridge (ObjC)
│   ├── windows/              # Windows bridge (C++/WinRT)
│   └── src/                  # TypeScript API
├── web/                      # WebAssembly renderer package
│   └── src/                  # Lazy / worker / server deployment presets
├── example/
│   ├── rust/                 # Rust usage example
│   ├── react-native/         # RN usage example
│   └── web/                  # Web usage notes
├── graphviz/                 # Graphviz source (git submodule)
└── .github/workflows/        # CI/CD automation
```

## Quick Start

### Native prerequisites

- CMake 3.16+, bison, flex, pkg-config
- Platform-specific toolchains (Xcode, Android NDK, MSVC, etc.)

### Native build

```bash
git clone --recursive https://github.com/kookyleo/graphviz-anywhere.git
cd graphviz-anywhere

./scripts/build-linux.sh
./scripts/build-macos.sh
./scripts/build-ios.sh
./scripts/build-android.sh
./scripts/build-windows.sh
```

Build outputs land in `output/<platform>/`.

Prebuilt native binaries are published from the current repository namespace:
[GitHub Releases](https://github.com/kookyleo/graphviz-anywhere/releases).

## C API

```c
#include "graphviz_api.h"

gv_context_t *ctx = gv_context_new();

char *svg = NULL;
size_t len = 0;
gv_error_t err = gv_render(ctx, "digraph { a -> b }", "dot", "svg", &svg, &len);

if (err == GV_OK) {
    gv_free_render_data(svg);
}

gv_context_free(ctx);
```

## Rust

```toml
[dependencies]
graphviz-anywhere = { path = "rust/graphviz" }
```

```rust
use graphviz_anywhere::{Engine, Format, GraphvizContext};

let ctx = GraphvizContext::new().unwrap();
let svg = ctx
    .render_to_string("digraph { a -> b -> c }", Engine::Dot, Format::Svg)
    .unwrap();
println!("{svg}");
```

Highlights:

- Type-safe `Engine` and `Format` enums
- `GraphvizContext` with `Drop` for automatic cleanup
- `Result<T, GraphvizError>` error handling
- `!Send + !Sync` because Graphviz is not thread-safe
- `GRAPHVIZ_ANYWHERE_DIR` as the preferred native lookup variable, with `GRAPHVIZ_NATIVE_DIR` kept for compatibility

Build with:

```bash
GRAPHVIZ_ANYWHERE_DIR=output/linux-x86_64 cargo build
```

## React Native

The package name stays `react-native-graphviz` for ecosystem discoverability, while the repository and Rust surface move under the `graphviz-anywhere` umbrella.

```bash
npm install react-native-graphviz
# or
yarn add react-native-graphviz
```

```ts
import { renderDot, getVersion } from 'react-native-graphviz';

const svg = await renderDot('digraph { mobile -> native }');
const svg2 = await renderDot('graph { a -- b }', 'neato', 'svg');
```

Platform support:

| Platform | Bridge | Min Version |
|----------|--------|-------------|
| iOS | ObjC (`dispatch_async`) | iOS 15.1 |
| Android | Java + JNI | API 24 |
| macOS | ObjC | macOS 11.0 |
| Windows | C++/WinRT | Windows 10 v1903 |

RN compatibility: `react-native >= 0.71.0`, tested with `0.84.x`. `react-native-macos` and `react-native-windows` remain optional peer dependencies.

## Web Wasm

The new `web/` package adds Graphviz rendering in browsers and edge runtimes through WebAssembly.

```bash
cd web
npm install
```

```ts
import { createLazyWasmRenderer } from 'graphviz-anywhere-web';

const renderer = createLazyWasmRenderer();
const svg = await renderer.render('digraph { web -> wasm }');
```

The runtime can self-report supported engines and formats:

```ts
const capabilities = await renderer.getCapabilities();
console.log(capabilities.engines, capabilities.formats);
```

## Three Deployment Profiles

### 1. Lazy client rendering

Use `createLazyWasmRenderer()` for docs sites, product pages, or pages where diagrams are occasional and startup size matters more than first-render latency.

### 2. Worker-backed rendering

Use `createWorkerWasmRenderer()` for diagram editors, whiteboards, or large-graph exploration so layout work stays off the main thread.

```ts
import { createWorkerWasmRenderer } from 'graphviz-anywhere-web';

const renderer = createWorkerWasmRenderer({ timeoutMs: 8000 });
const svg = await renderer.render('digraph { editor -> worker -> svg }');
```

### 3. Warm server / edge renderer

Use `createServerWasmRenderer()` for SSR, preview APIs, or edge handlers that render repeatedly and want to amortize Wasm startup cost across requests.

```ts
import { createServerWasmRenderer } from 'graphviz-anywhere-web';

const renderer = createServerWasmRenderer();
await renderer.preload();

const outputs = await renderer.renderMany(
  'digraph { api -> cache -> client }',
  ['svg', 'json']
);
```

## Supported Engines and Formats

### Native (C / Rust / React Native)

- Engines: `dot`, `neato`, `fdp`, `sfdp`, `circo`, `twopi`, `osage`, `patchwork`
- Formats: `svg`, `png`, `pdf`, `ps`, `json`, `dot`, `xdot`, `plain`

### Web (Wasm)

The exact engine and format list depends on the Wasm build and should be queried at runtime through `getCapabilities()`.

## Naming

Renaming the project to `graphviz-anywhere` is reasonable because the repository now spans:

- native shared libraries
- Rust bindings
- React Native bindings
- WebAssembly delivery for the web

The migration strategy in this repository is:

- Rust crates are renamed to `graphviz-anywhere` and `graphviz-anywhere-sys`
- the React Native npm package stays `react-native-graphviz`
- the repository path and release URLs can be renamed later without forcing another API rename right now

## Testing

### Web Unit Tests

Run vitest for the web package:

```bash
cd web
npm install
npm test -- --run          # Run once
npm test                   # Watch mode
npm test:ui                # Open test UI
```

Test coverage includes:
- 30 tests for shared utilities and error handling
- 25 tests for renderer creation (lazy, server, worker)
- 20 tests for worker protocol and messaging

All tests use mock VizWasmInstance (no real Wasm dependency).

### Rust Unit Tests

Run cargo tests for Rust bindings:

```bash
# Build native C library first (macOS example)
./scripts/build-macos.sh

# Run tests with environment variables
cd rust/graphviz
GRAPHVIZ_NATIVE_DIR=/path/to/native/output \
  DYLD_LIBRARY_PATH=/path/to/native/output/lib \
  cargo test --lib

# Or for graphviz-sys
cd rust/graphviz-sys
GRAPHVIZ_NATIVE_DIR=/path/to/native/output \
  DYLD_LIBRARY_PATH=/path/to/native/output/lib \
  cargo test --lib
```

Test coverage includes:
- 31 tests for graphviz crate (Engine, Format, Error, GraphvizContext)
- 6 tests for graphviz-sys crate (FFI constants, error codes)

All Rust tests verify type safety, error handling, and trait implementations.

## Graphviz Version

Native builds bundle **Graphviz 2.44.0** from the pinned submodule.

## License

Apache License 2.0 - see [LICENSE](LICENSE).

Graphviz itself: [Eclipse Public License 1.0](https://www.eclipse.org/legal/epl-v10.html).
