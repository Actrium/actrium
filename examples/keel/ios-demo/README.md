# ios-demo

iOS example app demonstrating **keel sandbox** across different JS engine configurations.

**Bridgeless (New Architecture) only** — React Native 0.83+.

## Configurations

| Scheme | Sandbox Engine | Notes |
|--------|---------------|-------|
| `ios-demo (Bridgeless+Hermes)` | Hermes | Default, supports AOT bytecode |
| `ios-demo (Bridgeless+JSC)` | JavaScriptCore | Apple's built-in engine |
| `ios-demo (Bridgeless+QuickJS)` | QuickJS | Lightweight engine |

## Quick Start

```bash
cd examples/keel/ios-demo

# 1. Install JS dependencies
npm install

# 2. Install pods (default: Hermes sandbox)
KEEL_SANDBOX=hermes pod install

# 3. Open in Xcode
open ios-demo.xcworkspace

# 4. Select scheme and run
```

## Switch Sandbox Engine

```bash
# Switch to a different sandbox engine
./switch-config.sh hermes
./switch-config.sh jsc
./switch-config.sh quickjs
```

## Install All Configurations

```bash
# Build and install all 3 configurations to the simulator
./install.sh

# Install a specific configuration
./install.sh bridgeless-hermes
```

## Architecture

```
ios-demo/
├── package.json               # keel: "file:../../../packages/keel"
├── metro.config.js            # Metro bundler config
├── babel.config.js            # Babel config
├── Podfile                    # CocoaPods config (Bridgeless only)
├── switch-config.sh           # Switch sandbox engine
├── install.sh                 # Build & install to simulator
├── ios-demo.xcodeproj/        # Xcode project (3 Bridgeless schemes)
├── ios-demo/                  # Swift/ObjC source
│   ├── KeelDemoApp.swift      # App entry point
│   ├── ContentView.swift      # UI with performance dashboard
│   ├── PerformanceMonitor.swift
│   ├── ReactNativeFactory.h/.mm
│   ├── KeelConfiguration.h/.mm
│   └── KeelSandboxBridge.h/.mm
├── RCTNApps/                  # React Native JS source
│   ├── index.tsx
│   └── src/App.tsx
└── TestCode/                  # JS performance benchmarks
    ├── fib.js
    ├── array.js
    ├── json.js
    └── string.js
```

## Dependencies

- Xcode 16+
- CocoaPods
- Node.js / npm
- `keel` (resolved via `file:../../../packages/keel` symlink)
