# askit

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

[中文文档](./README.zh.md)

**askit** is a UI component library and API layer built on top of [rill](https://github.com/GoAskAway/rill). Through package.json conditional exports, the same `import { StepList, Toast } from 'askit'` delivers real React Native components on the Host side and string identifiers on the Guest side (which rill passes to Host for rendering).

## Relationship with rill

| Layer | Role |
|-------|------|
| **rill** | Sandbox-isolated dynamic UI rendering engine — runs React code in isolated JS sandbox (QuickJS/JSC), serializes render operations into instructions, sends them to Host for real React Native rendering |
| **askit** | UI components and API layer on top of rill — provides business components (StepList, ChatBubble, etc.) and cross-boundary APIs (EventEmitter, Toast, Haptic) |

**How it works:**
- **UI Components**: Guest side is string `"StepList"`, Host side is complete RN implementation
- **APIs**: Guest sends commands via `global.__sendEventToHost('ASKIT_TOAST_SHOW', {...})`, Host bridge routes to real native APIs like ToastAndroid

## Architecture

askit follows a **90% Universal + 10% Core** architecture:

- **Universal (90%)**: Same API surface for both Host and Guest
- **Core (10%)**: Host-only modules for bridging and registration

```
┌─────────────────────────────────────────────────────────────┐
│                      Host App (RN)                          │
│  ┌─────────────────┐  ┌───────────────────────────────┐    │
│  │   askit/core    │  │         askit                 │    │
│  │  - registry     │  │  - EventEmitter, Toast, Haptic│    │
│  │  - bridge       │  │  - StepList, ThemeView...     │    │
│  └────────┬────────┘  └──────────────┬────────────────┘    │
│           │                          │                      │
│           └──────────┬───────────────┘                      │
│                      ▼                                      │
│              ┌───────────────┐                              │
│              │  rill Engine  │                              │
│              └───────┬───────┘                              │
└──────────────────────┼──────────────────────────────────────┘
                       │ Message Protocol
┌──────────────────────┼──────────────────────────────────────┐
│                      ▼                                      │
│              ┌───────────────┐                              │
│              │ QuickJS Sandbox│                             │
│              └───────┬───────┘                              │
│                      │                                      │
│  ┌───────────────────▼─────────────────────────────────┐   │
│  │                    askit                            │   │
│  │  - EventEmitter, Toast, Haptic (Guest impl)         │   │
│  │  - StepList, ThemeView... (DSL generators)          │   │
│  └─────────────────────────────────────────────────────┘   │
│                      Guest                                 │
└─────────────────────────────────────────────────────────────┘
```

## Installation

```bash
bun add github:GoAskAway/askit
```

> **Note**: askit is published via GitHub and exports TypeScript source directly (no build step required). Your bundler (Metro, Vite, etc.) needs to support TypeScript compilation. Both React Native Metro and modern bundlers handle this automatically.

## Usage

### In Guest (QuickJS Sandbox)

```typescript
import { EventEmitter, Toast, Haptic } from 'askit';
import { StepList, UserAvatar } from 'askit';

// Event communication
EventEmitter.emit('guest:ready', { version: '1.0.0' });
EventEmitter.on('host:config', (config) => {
  console.log('Received config:', config);
});

// Toast notifications
Toast.show('Hello from guest!', { duration: 'short', position: 'bottom' });

// Haptic feedback
Haptic.trigger('light');

// UI Components (returns DSL for host rendering)
export function App() {
  return <UserAvatar uri="https://example.com/avatar.png" size={48} />;
}
```

### In Host App (React Native)

```typescript
// Import core module for bridging
import { createEngineAdapter, components } from 'askit/core';
import { Engine } from 'rill';

// Create engine and connect askit
const engine = new Engine();
const adapter = createEngineAdapter(engine);

// Register components for rendering guest UI
engine.register(components);

// Start guest (supports URL or bundled code string)
await engine.loadBundle('https://example.com/guest.js');
```

## What is askit?

`askit` is the **UI kit + host/guest integration layer** designed to run on top of `rill`.

- In **Guest** (sandbox): `askit` provides **element identifiers** (string `ElementType`) and lightweight APIs.
  You write standard React JSX (`<UserAvatar />`, `<StepList />`).
- In **Host** (React Native): `askit` provides the **real native implementations** of those components,
  plus a small adapter (`createEngineAdapter`) that wires rill's `Engine` event channel to askit modules/events.

### Responsibilities & boundaries

| Layer | Owns | Typical code | Notes |
|---|---|---|---|
| App (your product) | Business logic, screens, state, data fetching | `unified-app.js` / app TSX | Emits/receives domain events |
| `askit` | Component library + a few cross-boundary APIs (Toast/Haptic/EventEmitter) | `askit/src/ui`, `askit/src/api` | Uses rill HostEvent model |
| `rill` | Sandbox runtime, reconciler, ops/receiver, devtools | `Engine`, `Receiver`, providers | Framework/runtime; no app UI |

### Message model (aligned with rill)

- **Guest → Host**: `global.__sendEventToHost(eventName, payload)`
  - `ASKIT_*` are reserved internal command events used by askit modules:
    - `ASKIT_TOAST_SHOW` payload `{ message, options }`
    - `ASKIT_HAPTIC_TRIGGER` payload `{ type }`
  - All other event names are **app events** and will be forwarded to Host `EventEmitter`.
- **Host → Guest**: Host `EventEmitter.emit(eventName, payload)` is forwarded to `engine.sendEvent(eventName, payload)`.

## API Reference

### EventEmitter

Event-based communication between Host and Guest.

```typescript
// Emit event
EventEmitter.emit(event: string, payload?: unknown): void

// Listen to event (supports wildcards: 'user:*', 'analytics:**')
EventEmitter.on(event: string, handler: (payload: unknown) => void, options?: {
  rateLimit?: 'throttle' | 'debounce' | 'none';
  delay?: number;
}): () => void

// Listen once
EventEmitter.once(event: string, handler: (payload: unknown) => void): () => void

// Remove listener
EventEmitter.off(event: string, handler: (payload: unknown) => void): void
```

### Toast

Display toast notifications.

```typescript
Toast.show(message: string, options?: {
  duration?: 'short' | 'long' | number;  // Default: 'short'
  position?: 'top' | 'center' | 'bottom'; // Default: 'bottom'
}): void
```

### Haptic

Trigger haptic feedback.

```typescript
Haptic.trigger(type?: 'light' | 'medium' | 'heavy' | 'selection' | 'success' | 'warning' | 'error'): void
```

## Components

| Component | Description |
|-----------|-------------|
| `StepList` | Step-by-step list with status indicators |
| `ThemeView` | Theme-aware container view |
| `UserAvatar` | User avatar with fallback support |
| `ChatBubble` | Chat message bubble |
| `PanelMarker` | Panel marker for left/right panel identification (Host-only) |
| `EngineMonitorOverlay` | Debug overlay for rill engine monitoring (Host-only) |

### Host-only Utilities

| Utility | Description |
|---------|-------------|
| `extractPanels` | Extract left/right panels from React element tree |

## Module Structure

```
askit/src
├── index.host.ts         # Host entry (React Native)
├── index.guest.ts        # Guest entry (QuickJS sandbox)
├── api/                  # EventEmitter, Toast, Haptic implementations
│   ├── EventEmitterBase.ts   # Shared base class for Host/Guest
│   ├── EventEmitter.host.ts  # Host-side EventEmitter
│   ├── EventEmitter.guest.ts # Guest-side EventEmitter
│   ├── Toast.host.ts         # Native Toast (Android/iOS)
│   ├── Toast.guest.ts        # Bridge-based Toast
│   ├── Haptic.host.ts        # Native Haptic feedback
│   └── Haptic.guest.ts       # Bridge-based Haptic
├── ui/                   # UI components
│   ├── guest-factory.ts      # Unified Guest component factory
│   ├── StepList/             # Step-by-step list component
│   ├── ThemeView/            # Theme-aware container
│   ├── UserAvatar/           # User avatar with fallback
│   ├── ChatBubble/           # Chat message bubble
│   ├── Panel/                # Panel marker & extraction
│   └── EngineMonitor/        # Debug overlay (Host-only)
├── core/                 # Host-only bridging and utilities
│   ├── bridge.ts             # Host-Guest message routing
│   ├── module-handlers.ts    # Module handler registry
│   ├── registry.ts           # Component and module registration
│   ├── throttle.ts           # Rate limiting utilities
│   └── protocol.ts           # Reserved event names (ASKIT_*)
├── contracts/            # Type contracts for Host-Guest communication
│   ├── generated.ts          # Auto-generated from JSON schema
│   └── runtime.ts            # Runtime validation utilities
└── types/                # Shared TypeScript types
```

### Conditional Exports

The package uses conditional exports to serve different implementations:

```json
{
  "exports": {
    ".": {
      "react-native": "./src/index.host.ts",
      "default": "./src/index.guest.ts"
    },
    "./core": "./src/core/index.ts",
    "./contracts": "./src/contracts/index.ts"
  }
}
```

- **React Native**: Gets native implementations with real UI components
- **Default** (QuickJS/Node): Gets remote implementations with message passing

## Development

```bash
# Install dependencies
bun install

# Run tests
bun test

# Run tests with coverage
bun test --coverage

# Lint
bun run lint

# Format
bun run fmt

# Build
bun run build
```

## License

Apache-2.0
