# askit

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

[中文文档](./README.zh.md)

**askit** is an isomorphic UI kit for the AskAway platform, providing unified APIs and components that work seamlessly across both the Host App (React Native) and Guest environments (QuickJS Sandbox).

## Architecture

askit follows a **90% Universal + 10% Core** architecture:

- **Universal (90%)**: Same API surface for both Host and Guest
- **Core (10%)**: Host-only modules for bridging and registration

```
┌─────────────────────────────────────────────────────────┐
│                      Host App (RN)                      │
│  ┌─────────────────┐  ┌─────────────────────────────┐  │
│  │   askit/core    │  │         askit               │  │
│  │  - registry     │  │  - Bus, Toast, Haptic       │  │
│  │  - bridge       │  │  - StepList, ThemeView...   │  │
│  └────────┬────────┘  └──────────────┬──────────────┘  │
│           │                          │                  │
│           └──────────┬───────────────┘                  │
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │  rill Engine  │                          │
│              └───────┬───────┘                          │
└──────────────────────┼──────────────────────────────────┘
                       │ Message Protocol
┌──────────────────────┼──────────────────────────────────┐
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │ QuickJS Sandbox│                         │
│              └───────┬───────┘                          │
│                      │                                  │
│  ┌───────────────────▼───────────────────────────────┐ │
│  │                    askit                          │ │
│  │  - Bus, Toast, Haptic (Remote implementations)    │ │
│  │  - StepList, ThemeView... (DSL generators)        │ │
│  └───────────────────────────────────────────────────┘ │
│                      Guest                             │
└─────────────────────────────────────────────────────────┘
```

## Installation

```bash
npm install askit
```

## Usage

### In Guest (QuickJS Sandbox)

```typescript
import { Bus, Toast, Haptic } from 'askit';
import { StepList, UserAvatar } from 'askit';

// Event communication
Bus.emit('guest:ready', { version: '1.0.0' });
Bus.on('host:config', (config) => {
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
import { createEngineAdapter, AskitComponents } from 'askit/core';
import { Engine } from 'rill';

// Create engine and connect askit
const engine = new Engine();
const adapter = createEngineAdapter(engine);

// Register components for rendering guest UI
Object.entries(AskitComponents).forEach(([name, Component]) => {
  engine.registerComponent(name, Component);
});

// Start guest
engine.loadGuest('./guest.js');
```

## API Reference

### Bus

Event-based communication between Host and Guest.

```typescript
// Emit event
Bus.emit(event: string, payload?: unknown): void

// Listen to event
Bus.on(event: string, handler: (payload: unknown) => void): () => void

// Listen once
Bus.once(event: string, handler: (payload: unknown) => void): () => void

// Remove listener
Bus.off(event: string, handler?: (payload: unknown) => void): void
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
Haptic.trigger(type?: 'light' | 'medium' | 'heavy' | 'success' | 'warning' | 'error'): void
```

## Components

| Component | Description |
|-----------|-------------|
| `StepList` | Step-by-step list with status indicators |
| `ThemeView` | Theme-aware container view |
| `UserAvatar` | User avatar with fallback support |
| `ChatBubble` | Chat message bubble |

## Module Structure

```
askit
├── index.ts          # Universal exports (Bus, Toast, Haptic, Components)
└── core/
    ├── registry.ts   # Component and module registration
    └── bridge.ts     # Host-Guest message bridge
```

### Conditional Exports

The package uses conditional exports to serve different implementations:

```json
{
  "exports": {
    ".": {
      "react-native": "./dist/native/index.js",
      "default": "./dist/remote/index.js"
    },
    "./core": "./dist/core/index.js"
  }
}
```

- **React Native**: Gets native implementations with real UI components
- **Default** (QuickJS/Node): Gets remote implementations with message passing

## Development

```bash
# Install dependencies
npm install

# Run tests
npm test

# Run tests with coverage
npm run test:coverage

# Lint
npm run lint

# Format
npm run fmt

# Build
npm run build
```

## License

Apache-2.0
