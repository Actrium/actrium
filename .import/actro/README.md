# Rill

Lightweight, headless, sandboxed React Native dynamic UI rendering engine.

> **rill** /rɪl/ n. a small stream - symbolizing lightweight, smooth data flow

## Features

- **React Development Experience**: Write guests using JSX and Hooks
- **Complete Sandbox Isolation**: Pluggable JSEngineProvider (QuickJS, VM, Worker), guest crashes don't affect the host
- **Lightweight and Efficient**: No WebView overhead, native rendering performance
- **Unified Bridge Layer**: Type-safe serialization with automatic callback lifecycle management
- **Flexible Extension**: Supports registering custom business components

## Package Exports

```
rill/
├── (default)       # Host runtime (Engine, EngineView, Receiver)
├── /let            # Guest SDK (components, hooks)
├── /devtools       # Development tools
├── /sandbox        # Sandbox providers
├── /sandbox-native # Native sandbox (JSC/QuickJS)
├── /sandbox-web    # Web sandbox (Worker)
└── /cli            # CLI build tools
```

## Quick Start

### Installation

```bash
# Using bun
bun add rill

# Using npm
npm install rill
```

**Peer Dependencies:**
- React 18.2+ or 19.x
- react-reconciler (matching your React version)
- react-native (for RN apps) or react-dom (for web)

### Host Integration

```tsx
import React, { useMemo, useEffect } from 'react';
import { Engine, EngineView } from 'rill';
import { NativeStepList } from './components/NativeStepList';

function App() {
  // 1. Create engine instance
  const engine = useMemo(() => new Engine({
    debug: __DEV__,
    timeout: 5000,
  }), []);

  // 2. Register custom components
  useEffect(() => {
    engine.register({
      StepList: NativeStepList,
    });
  }, [engine]);

  // 3. Render guest
  return (
    <EngineView
      engine={engine}
      source="https://cdn.example.com/guest.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Guest loaded')}
      onError={(err) => console.error('Guest error:', err)}
      renderError={(error) => <Text>Error: {error.message}</Text>}
    />
  );
}
```

### Guest Development

```tsx
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from 'rill/let';

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
# Build bundle
bun run rill/cli build src/guest.tsx -o dist/bundle.js

# Development mode
bun run rill/cli build src/guest.tsx --watch --no-minify --sourcemap

# Analyze bundle
bun run rill/cli analyze dist/bundle.js
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Host App (React Native)                    │
├─────────────────────────────────────────────────────────────────┤
│  EngineView → Engine → Sandbox Provider                         │
│                            │                                     │
│                            ▼                                     │
│                    ┌───────────────────┐                        │
│                    │  Guest Bundle.js  │                        │
│                    │  (React + SDK)    │                        │
│                    └───────────────────┘                        │
│                            │                                     │
│                            ▼                                     │
│                    Reconciler (VNode Ops)                       │
│                            │                                     │
│                    ┌───────┴───────┐                            │
│                    │    Bridge     │  ← Unified serialization   │
│                    └───────┬───────┘                            │
│                            ▼                                     │
│                    Receiver → Registry → Native Component Tree  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Overview

| Module | Import Path | Description |
|--------|-------------|-------------|
| Runtime | `rill` | Host runtime: Engine, EngineView, Receiver |
| Guest SDK | `rill/let` | Guest development kit: components, hooks |
| DevTools | `rill/devtools` | Debug tools: operation logging, tree inspection |
| CLI | `rill/cli` | Guest bundler (Bun-based) |

## API

### Engine

```typescript
import { Engine } from 'rill';

const engine = new Engine({
  timeout: 5000,        // Execution timeout (ms)
  debug: false,         // Debug mode
});

// Register components
engine.register({ ComponentName: ReactComponent });

// Load guest
await engine.loadBundle(bundleUrl, initialProps);

// Send event to guest
engine.sendEvent('EVENT_NAME', payload);

// Listen to guest messages
engine.on('message', (msg) => console.log(msg.event, msg.payload));

// Health check
const health = engine.getHealth();

// Destroy
engine.destroy();
```

### SDK Hooks

```typescript
import { useHostEvent, useConfig, useSendToHost, useRemoteRef, TextInputRef } from 'rill/let';

// Subscribe to host events
useHostEvent('EVENT_NAME', (payload) => { /* handle */ });

// Get initial configuration
const config = useConfig<ConfigType>();

// Send message to host
const send = useSendToHost();
send('EVENT_NAME', payload);

// Call host component methods (Remote Ref)
const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();
await remoteInput?.invoke('focus');
```

### Default Components

- `View` - Container component
- `Text` - Text component
- `Image` - Image component
- `ScrollView` - Scroll container
- `TouchableOpacity` - Touchable component
- `TextInput` - Text input
- `FlatList` - Virtualized list
- `Button` - Button component
- `Switch` - Toggle switch
- `ActivityIndicator` - Loading indicator

## Host ↔ Guest Communication

Guest subscribes using SDK hook `useHostEvent(event, callback)`, Host sends via `engine.sendEvent(eventName, payload)`.

**Guest example:**
```tsx
import { View, Text, useHostEvent, useSendToHost } from 'rill/let';

export default function Guest() {
  const send = useSendToHost();

  useHostEvent('PING', (payload) => {
    send('PONG', { received: payload });
  });

  return <View><Text>Ready</Text></View>;
}
```

**Host example:**
```tsx
import { Engine } from 'rill';

const engine = new Engine();
engine.on('message', (m) => console.log(m.event, m.payload));
engine.sendEvent('PING', { timestamp: Date.now() });
```

## Documentation

- [API Reference](./docs/API.md) - Complete API documentation
- [User Guide](./docs/GUIDE.md) - Getting started and best practices
- [Architecture](./docs/ARCHITECTURE.md) - System architecture details
- [Production Guide](./docs/PRODUCTION_GUIDE.md) - Production deployment checklist
- [Internal Docs](./docs/internals/) - Bridge layer, serialization details

## Development

```bash
# Install dependencies
bun install

# Run tests
bun test

# Type check
bun run typecheck

# Lint
bun run lint
```

## Testing

```bash
npm run test:all      # Run all tests (unit + native + E2E)
npm test              # Unit tests only
npm run test:native   # Native C++/ObjC++ tests (QuickJS/JSC)
npm run test:e2e      # Web Worker E2E tests
npm run test:e2e:wasm # WASM sandbox E2E tests
npm run test:e2e:rn   # React Native macOS E2E tests
```

## License

Apache-2.0
