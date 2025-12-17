# Getting Started

This guide will help you get started with askit in your project.

## Prerequisites

- Node.js 18+
- React 19+ / React Native 0.82+ (for Host App)
- rill engine (for Guest sandbox)

## Installation

```bash
bun add github:GoAskAway/askit
```

> **Note**: askit is published via GitHub and exports TypeScript source directly (no build step required). Your bundler (Metro, Vite, etc.) needs to support TypeScript compilation. Both React Native Metro and modern bundlers handle this automatically.

## Basic Setup

### For Guest Development

In your guest code, simply import and use the APIs:

```typescript
import { EventEmitter, Toast, Haptic } from 'askit';

// Initialize your guest
EventEmitter.emit('guest:init', { name: 'MyGuest', version: '1.0.0' });

// Listen for host events
EventEmitter.on('host:ready', () => {
  Toast.show('Guest connected!');
});
```

### For Host App Integration

In your React Native host app:

```typescript
import { createEngineAdapter, components } from 'askit/core';
import { Engine } from 'rill';

// Create and configure engine
const engine = new Engine();

// Connect askit bridge
const adapter = createEngineAdapter(engine);

// Register all askit components
engine.register(components);

// Load and run guest (supports URL or bundled code string)
await engine.loadBundle('https://example.com/guest.js');
```

## Project Structure

A typical AskAway project structure:

```
my-askaway-app/
├── host/                 # React Native host app
│   ├── src/
│   │   ├── App.tsx
│   │   └── engine/
│   │       └── setup.ts  # Engine configuration
│   └── package.json
├── guests/              # Guest projects
│   └── my-guest/
│       ├── src/
│       │   └── index.ts
│       └── package.json
└── package.json
```

## Next Steps

- [API Reference](./api-reference.md) - Learn about EventEmitter, Toast, and Haptic APIs
- [Components](./components.md) - Explore UI components
- [Architecture](./architecture.md) - Understand the isomorphic design
