# Keel

Lightweight, headless, sandboxed React Native dynamic UI rendering engine.

> **keel** [/kiːl/](https://ipa-reader.com/?text=ki%CB%90l&voice=Emma) n. the backbone of a hull - the spine that keeps a vessel upright and holding its course

## Features

- **React Development Experience**: Write guests using JSX and Hooks
- **Complete Sandbox Isolation**: Multiple sandbox backends (VM/JSC/Hermes/QuickJS/WASM) with automatic selection; guest crashes don't affect the host
- **Lightweight and Efficient**: No WebView overhead, native rendering performance
- **Unified Bridge Layer**: Type-safe serialization with automatic callback lifecycle management
- **Flexible Extension**: Supports registering custom business components

## Package Exports

```
keel/
├── /host            # Host runtime (Engine, useEngineView)
├── /host/preset     # Host UI helpers (EngineView, DefaultComponents)
├── /guest           # Guest SDK (components, hooks, platform APIs)
├── /devtools        # Development tools
└── /cli             # CLI build tools
```

> `import ... from 'keel'` is intentionally unsupported — use the explicit entrypoints above.


## Quick Start

### Installation

```bash
# Using bun
bun add keel

# Using npm
npm install keel
```

**Peer Dependencies:**
- React 18.2+ or 19.x
- react-reconciler (matching your React version)
- react-native (for RN apps) or react-dom (for web)

### Host Integration

```tsx
import React, { useMemo, useEffect } from 'react';
import { Engine } from 'keel/host';
import { EngineView } from 'keel/host/preset';
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
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from 'keel/guest';

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
bunx keel build src/guest.tsx -o dist/bundle.js

# Development mode
bunx keel build src/guest.tsx --no-minify --sourcemap

# Analyze bundle
bunx keel analyze dist/bundle.js
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Host App (React Native)                    │
├─────────────────────────────────────────────────────────────────┤
│  EngineView → Engine → Sandbox Backend                          │
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
| Host | `keel/host` | Host runtime: Engine, useEngineView |
| Host Preset | `keel/host/preset` | Host UI helpers: EngineView, DefaultComponents |
| Guest | `keel/guest` | Guest module: components, hooks |
| DevTools | `keel/devtools` | Debug tools: operation logging, tree inspection |
| CLI | `keel` (bin) / `keel/cli` | Guest bundler (Bun-based) |

## API

### Engine

```typescript
import { Engine } from 'keel/host';

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
const { health } = engine.getDiagnostics();

// Destroy
engine.destroy();
```

### SDK Hooks

```typescript
import { useHostEvent, useConfig, useSendToHost, useRemoteRef, TextInputRef } from 'keel/guest';

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
import { View, Text, useHostEvent, useSendToHost } from 'keel/guest';

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
import { Engine } from 'keel/host';

const engine = new Engine();
engine.on('message', (m) => console.log(m.event, m.payload));
engine.sendEvent('PING', { timestamp: Date.now() });
```

## Documentation

Docs site (private GitHub Pages; repository access required):
https://glowing-adventure-wnyz3lm.pages.github.io/

The same pages on GitHub:

- [Getting Started](./docs/getting-started/)
- [Guides](./docs/guides/cli.md)
- [API](./docs/api/)
- [Architecture](./docs/architecture/)
- [Reference](./docs/reference/sandbox-comparison.md)

Chinese counterparts sit next to the English files as `*.zh.md`. The site default is English; a Chinese browser is sent to `/zh/` until you pick a language in the header.

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
