# Guest Runtime Architecture

This document describes the architecture of Rill's guest runtime - how user code is bundled, injected into the sandbox, and executed.

## Overview

Rill uses a **two-phase architecture** to run guest code in a sandboxed environment:

1. **Build Phase**: Compiles the guest runtime (React + reconciler + bridge protocol) into a single minified string
2. **Runtime Phase**: Injects code segments into the sandbox to build up a state machine

```
┌─────────────────────────────────────────────────────────────────┐
│                        BUILD PHASE                               │
│  src/guest/bundle.ts  ──[bun build]──>  src/guest/build/bundle.ts│
│   (React + SDK + Reconciler + Bridge)          (injectable string)│
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                       RUNTIME PHASE                              │
│  Engine.loadBundle()                                             │
│    1. injectPolyfills()                → timers/require/module/exports │
│    2. evalCode(GUEST_BUNDLE_CODE)      → React/RillSDK/RillReconciler  │
│    3. injectRuntimeAPI()               → __sendToHost/__getConfig/etc.│
│    4. evalCode(userBundle)             → User's guest component        │
└─────────────────────────────────────────────────────────────────┘
```

## Directory Structure

```
src/
├── sdk/                       # Guest SDK (rill/sdk)
│   ├── index.ts               # Public exports (components, hooks, types)
│   ├── sdk.ts                 # Components + hooks implementation
│   └── types.ts               # Guest-facing types
│
├── guest/                     # Guest runtime bundle (auto-build)
│   ├── bundle.ts              # Bundle entry point (source)
│   ├── runtime/               # Guest runtime modules
│   │   ├── init.ts            # Environment initialization
│   │   ├── globals-setup.ts   # console + host event helpers + callbacks
│   │   ├── react-global.ts    # exposes React/JSX runtimes on globalThis
│   │   └── reconciler/        # React reconciler implementation
│   │       ├── index.ts       # Public API (render, unmount, etc.)
│   │       ├── host-config.ts # react-reconciler host configuration
│   │       ├── reconciler-manager.ts  # Reconciler instance management
│   │       ├── operation-collector.ts # Operation batching
│   │       ├── element-transform.ts   # Guest element transformation
│   │       ├── guest-encoder.ts       # Props serialization
│   │       ├── devtools.ts            # DevTools integration
│   │       └── types.ts               # Reconciler types
│   └── build/
│       └── bundle.ts          # Auto-generated `GUEST_BUNDLE_CODE` (DO NOT EDIT)
│
├── shared/                    # Shared protocol layer
│   ├── index.ts               # Protocol exports
│   ├── types.ts               # Operation and message types
│   ├── TypeRules.ts           # Serialization rules
│   ├── serialization.ts       # Encoder/decoder utilities
│   ├── bridge/                # Bridge layer
│   │   └── Bridge.ts          # Core communication
│   └── CallbackRegistry.ts    # Cross-boundary function management
│
└── host/                      # Host runtime
    ├── Engine.ts              # Engine (loads and executes guest)
    └── receiver/              # Receives operations, renders UI
```

## Build Phase

### Build Script: `scripts/build-guest-bundle.ts`

```bash
bun scripts/build-guest-bundle.ts
```

**Input**: `src/guest/bundle.ts`
**Output**: `src/guest/build/bundle.ts`

The build process:
1. Bundles `src/guest/bundle.ts` with all dependencies (React, rill/sdk, reconciler, shared protocol)
2. Minifies to IIFE format
3. Transpiles to ES5 (for wider guest-engine compatibility)
4. Wraps in TypeScript export as `GUEST_BUNDLE_CODE`

### What Gets Bundled

```
bundle.ts
├── runtime/init.ts            # Sets up __RILL_GUEST_ENV__, __callbacks
├── runtime/globals-setup.ts   # console + host event helpers
├── runtime/react-global.ts    # React/JSX globals
├── ../sdk/*                   # rill/sdk (Guest SDK)
├── runtime/reconciler/*       # Reconciler implementation
├── ../shared/*                # Shared protocol (CallbackRegistry, TypeRules)
└── react / react-reconciler   # React runtime + renderer foundation
```

## Runtime Phase: The State Machine

Each `evalCode()` call adds to the sandbox's global state. The order is critical:

### 1. Console Setup (`CONSOLE_SETUP_CODE`)

```javascript
globalThis.console = {
  log: function() { __console_log.apply(null, arguments); },
  warn: function() { __console_warn.apply(null, arguments); },
  // ...
};
```

### 2. Runtime Helpers (`RUNTIME_HELPERS_CODE`)

```javascript
globalThis.__callbacks = new Map();
globalThis.__callbackId = 0;
globalThis.__registerCallback = function(fn) { /* ... */ };
globalThis.__invokeCallback = function(fnId, args) { /* ... */ };
globalThis.__useHostEvent = function(eventName, callback) { /* ... */ };
globalThis.__handleHostEvent = function(eventName, payload) { /* ... */ };
```

### 3. React/JSX Shims (`ALL_SHIMS`)

```javascript
globalThis.__rillHooks = { states: [], effects: [], /* ... */ };
globalThis.React = { createElement, useState, useEffect, /* ... */ };
globalThis.require = function(id) {
  if (id === 'react') return globalThis.React;
  if (id === 'react-native') return { View: 'View', Text: 'Text', /* ... */ };
  // ...
};
```

### 4. Guest Bundle (`GUEST_BUNDLE_CODE`)

```javascript
globalThis.RillReconciler = {
  render: function(element, sendToHost) { /* ... */ },
  unmount: function(sendToHost) { /* ... */ },
  unmountAll: function() { /* ... */ },
  invokeCallback: function(fnId, args) { /* ... */ },
  releaseCallback: function(fnId) { /* ... */ },
  getCallbackCount: function() { /* ... */ },
};
```

### 5. Component Name Globals (injectRuntimeAPI)

Engine injects each registered component name as a global variable, with the value being the name string itself:

```javascript
// For each component registered in ComponentRegistry:
globalThis.View = 'View';
globalThis.Text = 'Text';
globalThis.Image = 'Image';
// ... etc.
```

This allows user bundles to use **variable mode** instead of **string mode**:

```javascript
// Variable mode (recommended) - validated at compile time
h(View, { style: styles.container }, children);

// String mode - only validated at runtime
h('View', { style: styles.container }, children);
```

The `rill/cli build` command automatically transforms JSX to variable mode.

### 6. User Bundle

```javascript
// User's compiled guest code
var MyComponent = function() {
  var config = useConfig();
  useHostEvent('REFRESH', function() { /* ... */ });
  return React.createElement(View, null, /* ... */);
};

// Auto-render
RillReconciler.render(
  React.createElement(MyComponent),
  globalThis.__sendToHost
);
```

## Data Flow

### Guest → Host (Render Operations)

```
User Component
     │
     ▼
React Reconciler (host-config.ts)
     │ createInstance, appendChild, commitUpdate, etc.
     ▼
Operation Collector (operation-collector.ts)
     │ batches operations
     ▼
sendToHost(batch)  ──[serialization]──>  Host Engine
                                              │
                                              ▼
                                         Receiver
                                              │
                                              ▼
                                      Native Component Tree
```

### Host → Guest (Events)

```
Host App
     │ engine.sendEvent('REFRESH', payload)
     ▼
Engine
     │ evalCode("__handleHostEvent('REFRESH', payload)")
     ▼
Sandbox
     │ __hostEventListeners.get('REFRESH').forEach(cb => cb(payload))
     ▼
useHostEvent callbacks in user components
```

### Host → Guest (Callback Invocation)

```
Native Button onPress
     │
     ▼
Receiver.handleCallback(fnId, args)
     │
     ▼
Engine.evalCode("RillReconciler.invokeCallback(fnId, args)")
     │
     ▼
CallbackRegistry.invoke(fnId, args)
     │
     ▼
Original function in user component
```

## Key Design Decisions

### Why Pre-bundle the Reconciler?

1. **Startup Performance**: Single eval vs multiple script loads
2. **Version Consistency**: React and reconciler versions are locked together
3. **Size Optimization**: Tree-shaking and minification at build time

### Why Separate `src/sdk/` and `src/guest/`?

- **`src/sdk/`**: User-facing API - what developers import in their guest code
- **`src/guest/`**: Runtime internals - bundled and injected by the engine

Users import from `rill/sdk`:
```tsx
import { View, Text, useHostEvent } from 'rill/sdk';
```

They never directly use `render()`, `CallbackRegistry`, etc. - those are runtime internals.

### Shared Bridge Protocol

`src/shared/` contains the serialization protocol shared by both sides:
- **Guest side**: Bundled into `GUEST_BUNDLE_CODE`
- **Host side**: Imported directly by `src/host/bridge/Bridge.ts`

This ensures both sides use identical serialization logic.

## File Responsibilities

| File | Role |
|------|------|
| `src/guest/bundle.ts` | Guest runtime entry: exposes `React`/`RillSDK`/`RillReconciler` globals |
| `src/guest/runtime/init.ts` | Environment setup before any React code runs (guest markers, callbacks) |
| `src/guest/runtime/globals-setup.ts` | Console and runtime helpers (HostEvent, callbacks, etc.) |
| `src/guest/runtime/react-global.ts` | Exposes React/JSX runtimes on `globalThis` |
| `src/guest/runtime/reconciler/*` | react-reconciler config + instance management + op encoding |
| `src/sdk/*` | Guest SDK (`rill/sdk`): components, hooks, RemoteRef, ErrorBoundary |
| `src/guest/build/bundle.ts` | Auto-generated: injectable `GUEST_BUNDLE_CODE` |
| `src/host/Engine.ts` | Loads Guest bundle, injects APIs, manages sandbox lifecycle |

## Regenerating the Guest Bundle

After modifying any file in `src/guest/` or `src/shared/`:

```bash
bun scripts/build-guest-bundle.ts
```

This regenerates `src/guest/build/bundle.ts`. The file should be committed to version control.
