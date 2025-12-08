# Rill

Lightweight, headless, sandboxed React Native dynamic UI rendering engine.

> **rill** /rɪl/ n. a small stream - symbolizing lightweight, smooth data flow

## Features

- **React-like Development Experience**: Write plugins using JSX and Hooks
- **Complete Sandbox Isolation**: Based on QuickJS, plugin crashes don't affect the host
- **Lightweight and Efficient**: No WebView overhead, native rendering performance
- **Flexible Extension**: Supports registering custom business components

## Repository Structure

This is a monorepo containing multiple packages:

```
rill/
├── packages/
│   ├── rill/          # Core runtime library
│   │   ├── src/
│   │   │   ├── runtime/    # Engine and renderer
│   │   │   ├── sdk/        # Plugin development SDK
│   │   │   └── reconciler/ # React reconciler
│   │   └── package.json
│   └── cli/           # CLI tools for plugin development
│       ├── src/
│       └── package.json
├── docs/              # Documentation
├── examples/          # Example projects
└── package.json       # Workspace root
```

### Packages

- **rill** - Core runtime library for host applications
  - Published as `rill` on npm
  - Exports: `rill` (runtime), `rill/sdk` (plugin SDK)

- **rill-cli** - Command-line tools for plugin development
  - Published as `rill-cli` on npm
  - Commands: `rill build`, `rill init`

### Development

Build all packages:
```bash
npm run build
```

Build a specific package:
```bash
cd packages/rill && npm run build
cd packages/cli && npm run build
```

Run tests:
```bash
npm run test --workspaces
```

## Quick Start

### Installation

Compatibility and peer dependencies

- Keep React and react-reconciler in compatible pairs to avoid install/runtime issues.
- Choose only one platform peer: react-dom (Web) or react-native (RN). react-native-quickjs is optional for RN.

Recommended pairings

- React 18.2.x ↔ react-reconciler 0.29–0.31
- React 19.0.x ↔ react-reconciler 0.32.x
- React 19.2.x+ ↔ react-reconciler 0.33.x

Install examples

- React Native (RN 0.82 + React 19.2)
  - npm i rill react@^19.2.1 react-native@^0.82 react-reconciler@^0.33
- Web (React 19.2)
  - npm i rill react@^19.2.1 react-dom@^19.2.1 react-reconciler@^0.33

Notes

- If you see npm ERESOLVE complaining react-reconciler@0.33 requires react@^19.2: upgrade React to ^19.2.1 (or align reconciler to 0.32 if you must stay on React 19.0).
- Avoid using --legacy-peer-deps in the long term; fix the pairing instead.


```bash
npm install rill
# or
yarn add rill
```

### Host Integration

```tsx
import { Engine, EngineView } from 'rill';
import { NativeStepList } from './components/NativeStepList';

// 1. Create engine instance
const engine = new Engine();

// 2. Register custom components
engine.register({
  StepList: NativeStepList,
});

// 3. Render plugin
function App() {
  return (
    <EngineView
      engine={engine}
      bundleUrl="https://cdn.example.com/plugin.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Plugin loaded')}
      onError={(err) => console.error('Plugin error:', err)}
    />
  );
}
```

### Plugin Development

```tsx
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from 'rill/sdk';

export default function MyPlugin() {
  const config = useConfig<{ theme: string }>();

  useHostEvent('REFRESH', () => {
    console.log('Host requested refresh');
  });

  return (
    <View style={{ flex: 1, padding: 20 }}>
      <Text style={{ fontSize: 24 }}>Hello from Plugin!</Text>
      <Text>Theme: {config.theme}</Text>
      <TouchableOpacity onPress={() => console.log('Pressed!')}>
        <Text>Click Me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### Build Plugin

```bash
# Install CLI
npm install -g rill

# Build
rill build src/plugin.tsx -o dist/bundle.js

# Development mode
rill build src/plugin.tsx --watch --no-minify --sourcemap
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Host App (React Native)                    │
├─────────────────────────────────────────────────────────────────┤
│  EngineView → Engine → QuickJS Context                          │
│                            │                                     │
│                            ▼                                     │
│                    ┌───────────────────┐                        │
│                    │  Plugin Bundle.js  │                        │
│                    │  (React + SDK)     │                        │
│                    └───────────────────┘                        │
│                            │                                     │
│                            ▼                                     │
│                    Reconciler (JSON Ops)                        │
│                            │                                     │
│                            ▼                                     │
│                    Receiver → Registry → Native Component Tree  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Description

| Module | Path | Description |
|--------|------|-------------|
| SDK | `rill/sdk` | Plugin development kit, virtual components and Hooks |
| Runtime | `rill` | Host runtime, Engine and EngineView |
| CLI | `rill` (bin) | Plugin bundler tool |

## API

### Engine

```typescript
const engine = new Engine(options?: EngineOptions);

interface EngineOptions {
  timeout?: number;      // Execution timeout (default 5000ms)
  debug?: boolean;       // Debug mode
  logger?: Logger;       // Custom logger
}

// Register components
engine.register({ ComponentName: ReactComponent });

// Load plugin
await engine.loadBundle(bundleUrl, initialProps);

// Send event to plugin
engine.sendEvent('EVENT_NAME', payload);

// Update configuration
engine.updateConfig({ key: value });

// Destroy
engine.destroy();
```

### SDK Hooks

```typescript
// Subscribe to host events
useHostEvent('EVENT_NAME', (payload) => {
  // Handle event
});

// Get initial configuration
const config = useConfig<ConfigType>();

// Send message to host
const send = useSendToHost();
send('EVENT_NAME', payload);
```

### Default Components

- `View` - Container component
- `Text` - Text component
- `Image` - Image component
- `ScrollView` - Scroll container
- `TouchableOpacity` - Touchable component

## Performance Optimization

Rill has built-in performance optimization mechanisms:

```tsx
import {
  ThrottledScheduler,
  VirtualScrollCalculator,
  PerformanceMonitor
} from 'rill/runtime';

// Batch update throttling
const scheduler = new ThrottledScheduler(onBatch, {
  maxBatchSize: 100,
  throttleMs: 16,
  enableMerge: true,
});

// Virtual scrolling
const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 60,
  overscan: 5,
});

// Performance monitoring
const monitor = new PerformanceMonitor();
```

## Debugging Tools

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools();
devtools.enable();

// View component tree
console.log(devtools.getComponentTreeText(nodeMap, rootChildren));

// Export debug data
const data = devtools.exportAll();
```

## Documentation

- [API Documentation](./docs/en/API.md) - Complete API reference
- [User Guide](./docs/en/GUIDE.md) - Getting started tutorial and best practices
- [Architecture Design](./docs/en/ARCHITECTURE.md) - System architecture details

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Development mode
npm run build:watch

# Type check
npm run typecheck

# Test
npm run test

# Test coverage
npm run test:coverage
```

## Testing

The project includes a complete test suite:

- Unit tests: Module functionality tests
- Integration tests: End-to-end scenario tests
- Coverage target: 80%+ code coverage

```bash
npm test           # Run all tests
npm test -- --run  # Single run
npm test:coverage  # Generate coverage report
```

## License

Apache-2.0
