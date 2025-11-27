# Getting Started

This guide will help you get started with askit in your project.

## Prerequisites

- Node.js 18+
- React Native 0.72+ (for Host App)
- rill engine (for Guest sandbox)

## Installation

```bash
npm install askit
```

## Basic Setup

### For Guest Development

In your guest code, simply import and use the APIs:

```typescript
import { Bus, Toast, Haptic } from 'askit';

// Initialize your guest
Bus.emit('guest:init', { name: 'MyGuest', version: '1.0.0' });

// Listen for host events
Bus.on('host:ready', () => {
  Toast.show('Guest connected!');
});
```

### For Host App Integration

In your React Native host app:

```typescript
import { createEngineAdapter, AskitComponents, AskitModules } from 'askit/core';
import { Engine } from 'rill';

// Create and configure engine
const engine = new Engine();

// Connect askit bridge
const adapter = createEngineAdapter(engine);

// Register all askit components
Object.entries(AskitComponents).forEach(([name, Component]) => {
  engine.registerComponent(name, Component);
});

// Load and run guest
await engine.loadGuest('https://example.com/guest.js');
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

- [API Reference](./api-reference.md) - Learn about Bus, Toast, and Haptic APIs
- [Components](./components.md) - Explore UI components
- [Architecture](./architecture.md) - Understand the isomorphic design
