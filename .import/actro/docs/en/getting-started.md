# Getting Started

This guide will help you get started with askit in your project.

## Prerequisites

- Node.js 18+
- React Native 0.72+ (for Host App)
- rill engine (for Plugin sandbox)

## Installation

```bash
npm install askit
```

## Basic Setup

### For Plugin Development

In your plugin code, simply import and use the APIs:

```typescript
import { Bus, Toast, Haptic } from 'askit';

// Initialize your plugin
Bus.emit('plugin:init', { name: 'MyPlugin', version: '1.0.0' });

// Listen for host events
Bus.on('host:ready', () => {
  Toast.show('Plugin connected!');
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

// Load and run plugin
await engine.loadPlugin('https://example.com/plugin.js');
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
├── plugins/              # Plugin projects
│   └── my-plugin/
│       ├── src/
│       │   └── index.ts
│       └── package.json
└── package.json
```

## Next Steps

- [API Reference](./api-reference.md) - Learn about Bus, Toast, and Haptic APIs
- [Components](./components.md) - Explore UI components
- [Architecture](./architecture.md) - Understand the isomorphic design
