# Quick Start

This guide walks you through setting up Keel in a React Native project, writing your first guest component, and rendering it on the host side.

## Prerequisites

- **React** 18.2+ or 19.x
- **Bun** runtime (for building guest bundles)
- **React Native** 0.72+ (or react-dom for web targets)

## Installation

```bash
bun add keel
```

Peer dependencies (install if not already present):

```bash
bun add react react-reconciler react-native
```

Keel supports React 18.2+ and 19.x. The `react-reconciler` package is required for the guest runtime's custom reconciler.

## Write a Guest Component

A guest component runs inside Keel's sandboxed environment. It uses the `keel/guest` module to access primitives, configuration, and host communication.

Create a file at `src/guest.tsx`:

```tsx
import { View, Text, TouchableOpacity, useConfig, useSendToHost } from 'keel/guest';

interface Config {
  title: string;
  theme: 'light' | 'dark';
}

export default function MyGuest() {
  const config = useConfig<Config>();
  const sendToHost = useSendToHost();

  const handlePress = () => {
    sendToHost('BUTTON_CLICKED', { timestamp: Date.now() });
  };

  return (
    <View style={{ padding: 16 }}>
      <Text style={{ fontSize: 24 }}>{config.title}</Text>
      <TouchableOpacity onPress={handlePress}>
        <Text>Click me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

Key points:

- `useConfig<T>()` reads the initial props provided by the host, typed with a generic parameter.
- `useSendToHost()` returns a function to dispatch events from the guest to the host application.
- All UI primitives (`View`, `Text`, `TouchableOpacity`, etc.) are imported from `keel/guest`, not from `react-native`.

## Build the Guest Bundle

Keel includes a build command that compiles your guest component into a self-contained JavaScript bundle:

```bash
bunx keel build src/guest.tsx -o dist/bundle.js
```

The output bundle can be served from a CDN, loaded from the filesystem, or embedded as a static asset.

## Host-Side Integration

On the host side, create an `Engine` instance and use the `EngineView` component to render the guest:

```tsx
import React, { useMemo } from 'react';
import { SafeAreaView, ActivityIndicator } from 'react-native';
import { Engine } from 'keel/host';
import { EngineView } from 'keel/host/preset';

export default function App() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  return (
    <SafeAreaView style={{ flex: 1 }}>
      <EngineView
        engine={engine}
        source="https://cdn.example.com/guest.js"
        initialProps={{ title: 'Hello Keel', theme: 'light' }}
        onLoad={() => console.log('Guest loaded')}
        onError={(error) => console.error('Guest error:', error)}
        fallback={<ActivityIndicator />}
      />
    </SafeAreaView>
  );
}
```

- `Engine` manages the sandboxed JavaScript execution environment.
- `EngineView` fetches the guest bundle from `source`, runs it inside the engine, and renders the resulting UI tree.
- `initialProps` are passed to the guest and accessible via `useConfig()`.
- `fallback` is displayed while the guest bundle loads.

## Next Steps

- [Host Integration](./host-integration.md) -- Engine configuration, component registration, event communication, lifecycle management, and headless mode.
- [Guest Development](./guest-development.md) -- Available components, hooks, styling, communication patterns, and platform APIs.
