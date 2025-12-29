# Rill Guest Example

This directory contains a simple Rill guest example demonstrating the core SDK features.

## simple-guest

A complete guest example showing:

- **Components**: View, Text, ScrollView, TouchableOpacity
- **State**: useState, useEffect
- **Host Communication**:
  - `useHostEvent` - Listen to host events
  - `useSendToHost` - Send messages to host
  - `useConfig` - Get configuration from host
- **Theming**: Light/dark mode support

## Quick Start

```bash
cd examples/simple-guest

# Install dependencies
bun install

# Build guest bundle
bun run build

# Build with sourcemap (development)
bun run build:dev
```

Output: `dist/bundle.js`

## Guest SDK API

```tsx
import {
  // Components
  View,
  Text,
  ScrollView,
  TouchableOpacity,
  Image,
  ActivityIndicator,
  Button,
  TextInput,
  FlatList,
  Switch,

  // Hooks
  useHostEvent,      // Listen to host events
  useSendToHost,     // Send messages to host
  useConfig,         // Get configuration from host
} from 'rill/sdk';
```

### useHostEvent

```tsx
// Listen to host events
useHostEvent('REFRESH', () => {
  console.log('Host requested refresh');
});

// Typed payload
useHostEvent<{ theme: 'light' | 'dark' }>('THEME_CHANGE', (payload) => {
  console.log('Theme:', payload.theme);
});
```

### useSendToHost

```tsx
const sendToHost = useSendToHost();

// Send message to host
sendToHost('MY_EVENT', { data: 'value' });
```

### useConfig

```tsx
interface Config {
  title: string;
  userId: string;
}

const config = useConfig<Config>();
console.log(config.title);
```

## Verify Example

```bash
cd examples
bun run verify-examples.ts
```

## Host Integration

For host-side integration (Engine, EngineView), see:

- `rill/host/preset` - Host UI components (works for both React Native and Web)

Host integration example:

```tsx
import { Engine } from 'rill';
import { DefaultComponents, EngineView } from 'rill/host/preset';

const engine = new Engine({
  sandbox: createSandboxProvider(),
  timeout: 5000,
});

engine.register(DefaultComponents);

<EngineView
  engine={engine}
  source="https://cdn.example.com/guest.js"
  initialProps={{ userId: '123' }}
  onLoad={() => console.log('Loaded')}
  onError={(err) => console.error(err)}
/>
```
