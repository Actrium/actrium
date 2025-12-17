# askit

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

[中文文档](./README.zh.md)

**askit** is an isomorphic UI kit for the AskAway platform, providing unified APIs and components that work seamlessly across both the Host App (React Native) and Guest environments (QuickJS Sandbox).

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
const avatar = UserAvatar({ uri: 'https://example.com/avatar.png', size: 48 });
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
├── index.host.ts     # Host entry (React Native)
├── index.guest.ts    # Guest entry (QuickJS sandbox)
├── api/              # EventEmitter, Toast, Haptic implementations
├── ui/               # UI components (StepList, ThemeView, etc.)
├── core/             # Host-only bridging and utilities
│   ├── bridge.ts     # Host-Guest message bridge
│   ├── registry.ts   # Component and module registration
│   ├── throttle.ts   # Rate limiting utilities
│   └── typed-bridge.ts # Type-safe bridge helpers
├── contracts/        # Type contracts for Host-Guest communication
└── types/            # Shared TypeScript types
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
