# Guest Runtime Architecture

This document describes the architecture of Rill's guest runtime - how user code is bundled, injected into the sandbox, and executed.

## Overview

Rill uses a **two-phase architecture** to run guest code in a sandboxed environment:

1. **Build Phase**: Compiles the guest runtime (React + reconciler + bridge protocol) into a single minified string
2. **Runtime Phase**: Injects code segments into the sandbox to build up a state machine

```
┌─────────────────────────────────────────────────────────────────┐
│                        BUILD PHASE                               │
│  src/guest-bundle/entry.ts  ──[bun build]──>  build/bundle.ts   │
│       (React + Reconciler + Bridge)              (270KB string)  │
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                       RUNTIME PHASE                              │
│  Engine.loadBundle()                                             │
│    1. evalCode(CONSOLE_SETUP_CODE)     → console.log/warn/error │
│    2. evalCode(RUNTIME_HELPERS_CODE)   → __callbacks, __useHostEvent │
│    3. evalCode(ALL_SHIMS)              → React, require() shim   │
│    4. evalCode(GUEST_BUNDLE_CODE)      → RillReconciler.render() │
│    5. evalCode(userBundle)             → User's guest component  │
└─────────────────────────────────────────────────────────────────┘
```

## Directory Structure

```
src/
├── guest-bundle/              # Guest runtime bundle
│   ├── entry.ts               # Bundle entry point (source)
│   ├── init.ts                # Environment initialization (source)
│   ├── reconciler/            # React reconciler implementation (source)
│   │   ├── index.ts           # Public API (render, unmount, etc.)
│   │   ├── host-config.ts     # react-reconciler host configuration
│   │   ├── reconciler-manager.ts  # Reconciler instance management
│   │   ├── operation-collector.ts # Operation batching
│   │   ├── element-transform.ts   # Guest element transformation
│   │   ├── guest-encoder.ts   # Props serialization
│   │   ├── devtools.ts        # DevTools integration
│   │   └── types.ts           # Reconciler types
│   └── build/
│       └── bundle.ts          # Auto-generated build output (DO NOT EDIT)
│
├── let/                       # User-facing SDK (rill/let)
│   ├── index.ts               # Public exports (View, Text, hooks)
│   ├── sdk.ts                 # Components and hooks implementation
│   └── types.ts               # User-facing types
│
├── bridge/                    # Shared protocol layer
│   ├── index.ts               # Protocol exports
│   ├── types.ts               # Operation and message types
│   ├── TypeRules.ts           # Serialization rules
│   ├── serialization.ts       # Encoder/decoder utilities
│   └── CallbackRegistry.ts    # Cross-boundary function management
│
└── runtime/                   # Host runtime
    ├── engine.ts              # Engine (loads and executes guest)
    ├── receiver.ts            # Receives operations, renders UI
    └── bridge/Bridge.ts       # Host-side serialization
```

## Build Phase

### Build Script: `scripts/build-guest-bundle.ts`

```bash
bun scripts/build-guest-bundle.ts
```

**Input**: `src/guest-bundle/entry.ts`
**Output**: `src/guest-bundle/build/bundle.ts`

The build process:
1. Bundles `entry.ts` with all dependencies (React, react-reconciler, bridge protocol)
2. Minifies to IIFE format (~270KB)
3. Prepends environment initialization code
4. Wraps in TypeScript export as `GUEST_BUNDLE_CODE`

### What Gets Bundled

```
entry.ts
├── init.ts                    # Sets up __RILL_GUEST_ENV__, __callbacks
├── reconciler/index.ts        # Reconciler public API
│   ├── reconciler-manager.ts  # render(), unmount(), unmountAll()
│   ├── host-config.ts         # react-reconciler configuration
│   └── operation-collector.ts # Operation batching
├── bridge/*                   # Shared protocol (CallbackRegistry, TypeRules)
├── react                      # React 19 runtime
└── react-reconciler           # Custom renderer foundation
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

### 5. User Bundle

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

### Why Separate `src/let/` and `src/guest-bundle/`?

- **`src/let/`**: User-facing API - what developers import in their guest code
- **`src/guest-bundle/`**: Runtime internals - bundled and injected by the engine

Users import from `rill/let`:
```tsx
import { View, Text, useHostEvent } from 'rill/let';
```

They never directly use `render()`, `CallbackRegistry`, etc. - those are runtime internals.

### Shared Bridge Protocol

`src/bridge/` contains the serialization protocol shared by both sides:
- **Guest side**: Bundled into `GUEST_BUNDLE_CODE`
- **Host side**: Imported directly by `src/runtime/bridge/Bridge.ts`

This ensures both sides use identical serialization logic.

## File Responsibilities

| File | Role |
|------|------|
| `guest-bundle/entry.ts` | Bundle entry point, exports to `globalThis.RillReconciler` |
| `guest-bundle/init.ts` | Environment setup before any React code runs |
| `guest-bundle/build/bundle.ts` | Auto-generated, contains bundled guest runtime |
| `guest-bundle/reconciler/host-config.ts` | react-reconciler configuration |
| `guest-bundle/reconciler/reconciler-manager.ts` | Manages reconciler instances, public API |
| `guest-bundle/reconciler/operation-collector.ts` | Batches operations before sending |
| `runtime/engine.ts` | Loads bundle, injects code, manages sandbox lifecycle |
| `runtime/engine/shims.ts` | React hooks and JSX runtime shims |
| `runtime/engine/SandboxHelpers.ts` | Console and runtime helper injection code |

## Regenerating the Guest Bundle

After modifying any file in `src/guest-bundle/` or `src/bridge/`:

```bash
bun scripts/build-guest-bundle.ts
```

This regenerates `src/guest-bundle/build/bundle.ts`. The file should be committed to version control.
