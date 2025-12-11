# Rill

Lightweight, headless, sandboxed React Native dynamic UI rendering engine.

> **rill** /rɪl/ n. a small stream - symbolizing lightweight, smooth data flow

## Features

- **React-like Development Experience**: Write guests using JSX and Hooks
- **Complete Sandbox Isolation**: Based on QuickJS, guest crashes don't affect the host
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
│   │   │   ├── sdk/        # Guest development SDK
│   │   │   └── reconciler/ # React reconciler
│   │   ├── docs/      # Documentation and examples
│   │   └── package.json
│   └── cli/           # CLI tools for guest development
│       ├── src/
│       └── package.json
└── package.json       # Workspace root
```

### Packages

- **rill** - Core runtime library for host applications
  - Published as `rill` on npm
  - Exports: `rill` (runtime), `rill/sdk` (guest SDK)

- **rill-cli** - Command-line tools for guest development
  - Published as `rill-cli` on npm
  - Commands: `rill build`, `rill init`

### Development

Build all packages:
```bash
bun run build
```

Build a specific package:
```bash
cd packages/rill && bun run build
cd packages/cli && bun run build
```

Run tests:
```bash
bun test
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
  - bun add rill react@^19.2.1 react-native@^0.82 react-reconciler@^0.33
- Web (React 19.2)
  - bun add rill react@^19.2.1 react-dom@^19.2.1 react-reconciler@^0.33

Notes

- If you see npm ERESOLVE complaining react-reconciler@0.33 requires react@^19.2: upgrade React to ^19.2.1 (or align reconciler to 0.32 if you must stay on React 19.0).
- Avoid using --legacy-peer-deps in the long term; fix the pairing instead.


```bash
bun add rill
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

// 3. Render guest
function App() {
  return (
    <EngineView
      engine={engine}
      bundleUrl="https://cdn.example.com/guest.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Guest loaded')}
      onError={(err) => console.error('Guest error:', err)}
    />
  );
}
```

### Guest Development

```tsx
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from 'rill/sdk';

export default function MyGuest() {
  const config = useConfig<{ theme: string }>();

  useHostEvent('REFRESH', () => {
    console.log('Host requested refresh');
  });

  return (
    <View style={{ flex: 1, padding: 20 }}>
      <Text style={{ fontSize: 24 }}>Hello from Guest!</Text>
      <Text>Theme: {config.theme}</Text>
      <TouchableOpacity onPress={() => console.log('Pressed!')}>
        <Text>Click Me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### Build Guest

```bash
# Install CLI
bun add -g rill

# Build
rill build src/guest.tsx -o dist/bundle.js

# Development mode
rill build src/guest.tsx --watch --no-minify --sourcemap
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
│                    │  Guest Bundle.js  │                        │
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
| SDK | `rill/sdk` | Guest development kit, virtual components and Hooks |
| Runtime | `rill` | Host runtime, Engine and EngineView |
| CLI | `rill` (bin) | Guest bundler tool |

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

// Load guest
await engine.loadBundle(bundleUrl, initialProps);

// Send event to guest
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

- [API Documentation](./docs/API.md) - Complete API reference
- [User Guide](./docs/GUIDE.md) - Getting started tutorial and best practices
- [Architecture Design](./docs/ARCHITECTURE.md) - System architecture details
- [Production Guide](./docs/PRODUCTION_GUIDE.md) - Production deployment checklist
- [Guest Examples](./examples/) - Working examples with complete source code

## Development

```bash
# Install dependencies
bun install

# Build
bun run build

# Development mode
bun run build:watch

# Type check
bun run typecheck

# Test
bun test

# Test coverage
bun run test:coverage
```

## Testing


## Host ↔ Guest Events

- Guest subscribes using SDK hook `useHostEvent(event, callback)`.
- Host sends events via `engine.sendEvent(eventName, payload)`.
- Unsubscribe: `const off = useHostEvent('EVT', cb); off?.();` If you keep the return value from the hook call, call it to remove the listener.

Example (guest):

```tsx
import * as React from 'react';
import { View, Text } from 'rill/sdk';
import { useHostEvent, useSendToHost } from 'rill/sdk';

export default function Guest() {
  const send = useSendToHost();
  React.useEffect(() => {
    const off = useHostEvent('PING', (payload: { ok: number }) => {
      // handle host event
      send('ACK', { got: payload.ok });
    });
    return () => { off && off(); };
  }, []);
  return <View><Text>Ready</Text></View>;
}
```

Example (host):

```ts
import { Engine } from 'rill';
const engine = new Engine({ provider: yourJSEngineProvider });
engine.on('message', (m) => { /* m.event, m.payload */ });
engine.sendEvent('PING', { ok: 1 });
```

Notes:
- Engine injects `__useHostEvent`/`__handleHostEvent` at runtime as a safety polyfill, so the hook works even if your bundle doesn’t include the CLI banner.
- For performance, prefer stable callbacks and unsubscribe on unmount.

## SDK Compile-time Inlining and Strict Guard

Goal: guest bundles must not require/import `rill/sdk` at runtime. The SDK is type-level + compile-time only.

- Build with rill CLI (Vite lib build, IIFE output). The CLI sets a resolve alias so `rill/sdk` can be fully inlined/treeshaken.
- Post-build, the CLI runs a strict guard that analyzes the bundle and fails if non-whitelisted runtime deps are present (e.g., `rill/sdk`).

CLI:

```bash
# Build (strict guard on by default)
rill build src/guest.tsx -o dist/bundle.js

# Analyze an existing bundle
rill analyze dist/bundle.js \
  --fail-on-violation \
  --treat-eval-as-violation \
  --treat-dynamic-non-literal-as-violation
```

Whitelist (runtime): `react`, `react-native`, `react/jsx-runtime`, `rill/reconciler`.

If the bundle still contains `require('rill/sdk')`, analyze fails fast with guidance.

## Host Integration (correct API)

```ts
import { Engine } from 'rill';
import { View, Text } from 'react-native'; // or your custom components

const engine = new Engine({ provider: yourJSEngineProvider });
engine.register({ View, Text }); // Register the components your guest needs
const receiver = engine.createReceiver(() => {/* schedule host re-render */});
await engine.loadBundle(codeOrUrl);
```

Notes:
- Use `register(components)` (not `registerComponent`).
- Use `loadBundle(source)` (not `loadGuest`).
- Provide a JS engine provider via `new Engine({ provider })`.

## Init Template Defaults

`rill init` generates:
- vite.config.ts: IIFE lib build, external: react/react-native/react/jsx-runtime/rill-reconciler, alias `rill/sdk` to ESM for inlining.
- tsconfig.json: Bundler-resolve, isolatedModules, strict, verbatimModuleSyntax, and editor-only path typing for `rill/sdk`.
- Example guest using `import { View, Text } from 'rill/sdk'`.


The project includes a complete test suite:

- Unit tests: Module functionality tests
- Integration tests: End-to-end scenario tests
- Coverage target: 80%+ code coverage

```bash
bun test            # Run all tests
bun test --run  # Single run
bun test:coverage  # Generate coverage report
```

## License

Apache-2.0
