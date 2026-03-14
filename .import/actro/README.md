# graphviz-native

Cross-platform prebuilt Graphviz shared libraries, optimized for React Native and Rust shared dependency.

Supports iOS (XCFramework), Android (.so), macOS (.dylib), Linux (.so) & Windows (.dll).

## Architecture

```
graphviz-native/
├── src/                      # C ABI wrapper (graphviz_api.h/.c)
├── scripts/                  # Per-platform build scripts
├── rust/
│   ├── graphviz-sys/         # Raw FFI bindings (graphviz-native-sys)
│   └── graphviz/             # Safe Rust wrapper (graphviz-native)
├── react-native/             # React Native native module
│   ├── ios/                  # iOS bridge (ObjC)
│   ├── android/              # Android bridge (Java + JNI)
│   ├── macos/                # macOS bridge (ObjC)
│   ├── windows/              # Windows bridge (C++/WinRT)
│   └── src/                  # TypeScript API
├── example/
│   ├── rust/                 # Rust usage example
│   └── react-native/         # RN usage example
├── graphviz/                 # Graphviz source (git submodule)
└── .github/workflows/        # CI/CD automation
```

## Quick Start

### Prerequisites

- CMake 3.16+, bison, flex, pkg-config
- Platform-specific toolchains (Xcode, Android NDK, MSVC, etc.)

### Build

```bash
git clone --recursive https://github.com/kookyleo/graphviz-native.git
cd graphviz-native

./scripts/build-linux.sh            # Linux
./scripts/build-macos.sh            # macOS (universal)
./scripts/build-ios.sh              # iOS (XCFramework)
./scripts/build-android.sh          # Android (all ABIs)
./scripts/build-windows.sh          # Windows (x64)
```

Build outputs: `output/<platform>/`

### Prebuilt Binaries

Download from [GitHub Releases](https://github.com/kookyleo/graphviz-native/releases).

## C API

```c
#include "graphviz_api.h"

gv_context_t *ctx = gv_context_new();

char *svg = NULL;
size_t len = 0;
gv_error_t err = gv_render(ctx, "digraph { a -> b }", "dot", "svg", &svg, &len);

if (err == GV_OK) {
    // Use svg (len bytes)
    gv_free_render_data(svg);
}

gv_context_free(ctx);
```

## Rust Integration

Add to `Cargo.toml`:

```toml
[dependencies]
graphviz-native = { path = "rust/graphviz" }
```

```rust
use graphviz_native::{GraphvizContext, Engine, Format};

let ctx = GraphvizContext::new().unwrap();
let svg = ctx.render_to_string(
    "digraph { a -> b -> c }",
    Engine::Dot,
    Format::Svg,
).unwrap();
println!("{svg}");
```

Features:
- Type-safe `Engine` and `Format` enums
- `GraphvizContext` with `Drop` for automatic cleanup
- `Result<T, GraphvizError>` error handling
- `!Send + !Sync` (Graphviz is not thread-safe)

Build with: `GRAPHVIZ_NATIVE_DIR=output/linux-x86_64 cargo build`

## React Native Integration

```bash
npm install react-native-graphviz
# or
yarn add react-native-graphviz
```

```typescript
import { renderDot, getVersion } from 'react-native-graphviz';

const svg = await renderDot('digraph { a -> b }');
const svg2 = await renderDot('graph { a -- b }', 'neato', 'svg');
```

Platform support:

| Platform | Bridge | Min Version |
|----------|--------|-------------|
| iOS | ObjC (dispatch_async) | iOS 15.1 |
| Android | Java + JNI | API 24 |
| macOS | ObjC | macOS 10.13 |
| Windows | C++/WinRT | Windows 10 v1903 |

RN version compatibility: >= 0.71.0 (peer dep), tested with 0.84.x. react-native-windows 0.81.x, react-native-macos 0.81.x.

## Supported Engines & Formats

**Layout engines:** `dot`, `neato`, `fdp`, `sfdp`, `circo`, `twopi`, `osage`, `patchwork`

**Output formats:** `svg`, `png`, `pdf`, `ps`, `json`, `dot`, `xdot`, `plain`

## Platform Build Details

| Platform | Output | Architectures | Min Version |
|----------|--------|---------------|-------------|
| iOS | Graphviz.xcframework | arm64 + x86_64 sim | iOS 12.0 |
| Android | libgraphviz_api.so | arm64-v8a, armeabi-v7a, x86_64 | API 23 |
| macOS | libgraphviz_api.dylib | arm64 + x86_64 (universal) | 10.15 |
| Linux | libgraphviz_api.so | x86_64, aarch64 | glibc 2.27+ |
| Windows | graphviz_api.dll | x86_64 | MSVC 2019+ |

## Graphviz Version

Bundled: **Graphviz 2.44.0** (pinned via git submodule)

## License

Apache License 2.0 - see [LICENSE](LICENSE).

Graphviz itself: [Eclipse Public License 1.0](https://www.eclipse.org/legal/epl-v10.html).
