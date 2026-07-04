# Actro + Rill Architecture Overview

## System Overview

Actro is a React Native macOS application with a sandboxed plugin system based on the **Rill** dynamic UI engine.

**Core Components:**
- **Host (actr-host)**: React Native macOS app, responsible for rendering and secure execution
- **Guest (.actr)**: Independently bundled plugin apps running in QuickJS sandbox
- **Rill**: Bridge layer connecting Host and Guest with secure communication
- **Askit**: Isomorphic UI component library providing unified APIs for Host and Guest

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     Actro actr-host                        │
│  (React Native macOS App)                                   │
│                                                             │
│  ┌──────────────┐         ┌──────────────────┐             │
│  │  index.ts    │────────►│  ActrApp.tsx     │             │
│  │  (Entry)     │         │  (3-Panel Layout)│             │
│  └──────────────┘         └────────┬─────────┘             │
│                                    │                        │
│                           ┌────────▼─────────┐             │
│                           │ GuestAppView.tsx │             │
│                           │ (Plugin Loader)  │             │
│                           └────────┬─────────┘             │
│                                    │                        │
│       ┌────────────────────────────┼────────────────┐      │
│       │                            │                │      │
│  ┌────▼──────┐          ┌─────────▼──────┐   ┌─────▼────┐ │
│  │  Engine   │          │FunctionQuickJS │   │ Component│ │
│  │ (rill)    │          │   Provider     │   │ Registry │ │
│  └───────────┘          └────────────────┘   └──────────┘ │
└─────────────────────────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│                    Rill Core Library                        │
│                                                             │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────┐    │
│  │  Engine.ts  │───►│ Receiver.ts  │───►│ Registry   │    │
│  │  (Sandbox)  │    │ (Ops→React)  │    │ (Components)│   │
│  └─────────────┘    └──────────────┘    └────────────┘    │
│                                                             │
│  SDK: View, Text, Image, ScrollView, useHostEvent, etc.    │
└─────────────────────────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│                    askit                                    │
│                                                             │
│  APIs: EventEmitter, Toast, Haptic, Bridge                 │
│  Components: ChatBubble, StepList, ThemeView, UserAvatar   │
└─────────────────────────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│              Guest App (.actr package)                      │
│                                                             │
│  manifest.json + left-panel.js + right-panel.js            │
└─────────────────────────────────────────────────────────────┘
```

## Data Flow

```
Guest Sandbox (QuickJS)          Host (React Native)
─────────────────────           ──────────────────────
React Component
      ↓
React.createElement()
      ↓
Rill Reconciler
      ↓
OperationBatch (JSON)  ──────►  Receiver.applyBatch()
                                       ↓
                                NodeMap + Registry
                                       ↓
                                React.createElement()
                                       ↓
                                Native UI Rendering
```

## Key Files

**Host App:**
- `actr-host/index.ts` - Entry point
- `actr-host/src/ActrApp.tsx` - Main app with 3-panel layout
- `actr-host/src/GuestAppView.tsx` - Plugin loader
- `actr-host/metro.config.js` - Metro bundler config

**Rill Core:**
- `rill/packages/core/src/runtime/engine.ts` - Sandbox engine
- `rill/packages/core/src/runtime/receiver.ts` - Operation receiver
- `rill/packages/core/src/reconciler/index.ts` - React reconciler

**Askit:**
- `askit/src/api/` - EventEmitter, Toast, Haptic
- `askit/src/ui/` - UI components
- `askit/src/core/` - Bridge and registry

**Guest App:**
- `packages/actro-runtime/actr-app-examples/counterapp/` - Example app
- `packages/actro-runtime/actr-app-examples/pack-actr.sh` - Packaging script

## Build Process

**Guest App → .actr:**
```bash
# 1. Build with rill CLI
rill build src/left-panel.tsx --outfile dist/left-panel.js

# 2. Package as .actr
zip -r app.actr manifest.json left-panel.js right-panel.js
```

**Host App:**
```bash
# Metro bundles the host app
cd actr-host && npm start
```

## Security

- **Module Whitelist**: Only `react`, `react-native`, `rill/reconciler`, `rill/sdk` allowed
- **No eval()**: Build-time analysis rejects any eval usage
- **Execution Timeout**: 5s default timeout with force destroy
- **Callback Cleanup**: Auto-cleanup on node deletion

## Development

```bash
# Start log server (optional)
node actr-host/log-server.js

# Run Metro bundler
cd actr-host && npm start

# Build Xcode project
open Actro.xcworkspace
```

## Related Documentation

- [Manifest Specification](docs/manifest-specification.md)
- [Guest Development Guide](docs/rill-askit-guest-guide.md)
