# Rill Guest Examples

This directory contains Rill guest development and host integration examples, covering all features from basic to advanced.

## Example List

### 1. simple-guest - Basic Guest Development

**Target Audience**: Developers new to Rill

**Features Demonstrated**:
- Basic components: View, Text, TouchableOpacity, ScrollView
- State management: useState, useEffect
- Host communication:
  - `useHostEvent` - Listen to host events
  - `useSendToHost` - Send messages to host
  - `useConfig` - Get guest configuration
- Theme support: Light and dark theme switching

[View Example →](./simple-guest)

---

### 2. host-integration - Complete Host Integration Example

**Target Audience**: Developers who need to integrate Rill into React Native applications

**Features Demonstrated**:
- ✅ Engine instance creation and configuration
- ✅ EngineView lifecycle management
- ✅ Bidirectional host-guest communication
- ✅ Error handling and recovery
- ✅ Custom component registration
- ✅ QuickJS Provider configuration
- ✅ Dynamic guest loading
- ✅ Resource cleanup and memory management

[View Example →](./host-integration)

---

### 3. advanced-features - Advanced Features Example

**Target Audience**: Developers who need to optimize performance and deploy to production

**Features Demonstrated**:
- ✅ Performance metrics tracking (onMetric)
- ✅ Execution timeout protection (timeout)
- ✅ Module whitelist (requireWhitelist)
- ✅ Health checks (getHealth)
- ✅ Batch processing optimization (receiverMaxBatchSize)
- ✅ Custom logging system
- ✅ Debug mode configuration
- ✅ Performance monitoring and alerting

[View Example →](./advanced-features)

---

## Quick Start

### Building Guests

All example guests follow the same build process:

```bash
# Navigate to any example directory
cd examples/simple-guest  # or host-integration, advanced-features

# Install dependencies
bun install

# Build the guest
bun run build

# Development mode (no minification + sourcemap)
bun run build:dev

# Watch for file changes (if supported)
bun run watch
```

Build output: `dist/bundle.js` or `dist/guest.js`

### Verifying Examples

Run the verification script to ensure all examples work correctly:

```bash
cd examples
bun run verify-examples.ts
```

This performs comprehensive checks including:
- **Bundle**: File size validation
- **Load**: Success and timing
- **Tree**: Root component, node count, required components
- **Operations**: CREATE/APPEND counts, component types
- **Events**: Re-render behavior on host events
- **State**: Render count and state history

Example output:
```
══════════════════════════════════════════════════════════════════════════════════
EXAMPLE VERIFICATION RESULTS
══════════════════════════════════════════════════════════════════════════════════

✅ PASS simple-guest
  [Bundle] Size (KB): 4.42 ✓
  [Load] Success: true ✓
  [Tree] Root component: ScrollView ✓
  [Ops] Initial count: 38 ✓
  [Events] REFRESH triggers re-render: true ✓
  ...

SUMMARY: 3/3 examples passed (38/38 checks)
```

## Project Structure

### Guest Example Structure

```
simple-guest/
├── package.json      # Project configuration
├── README.md         # Detailed documentation
├── src/
│   └── index.tsx     # Guest main entry
└── dist/
    └── bundle.js     # Build output
```

### Host Integration Example Structure

```
host-integration/
├── package.json
├── README.md         # Complete integration guide
├── src/
│   ├── guest.tsx            # Guest code
│   ├── HostApp.tsx           # Host application example
│   └── QuickJSProvider.tsx   # Provider configuration
└── dist/
    └── guest.js
```

## Creating a New Guest

1. Copy the `simple-guest` directory
2. Modify the name in `package.json`
3. Edit `src/index.tsx` to implement guest logic
4. Run `bun run build` to build

## SDK Import

```tsx
import {
  // Basic components
  View,
  Text,
  TouchableOpacity,
  ScrollView,
  Image,
  ActivityIndicator,
  Button,

  // Hooks
  useHostEvent,      // Listen to host events
  useSendToHost,     // Send messages to host
  useConfig,         // Get guest configuration

  // Types
  StyleProp,
  LayoutEvent,
  ScrollEvent,
} from 'rill/sdk';
```

## Learning Path

Recommended learning order:

### 1️⃣ Getting Started

Start with **simple-guest** to learn:
- How to write basic guests
- Usage of SDK components and hooks
- Guest build process

### 2️⃣ Host Integration

Read the **host-integration** example to understand:
- How to integrate Rill into your application
- Usage of Engine and EngineView
- Lifecycle management and error handling
- Host-guest communication mechanisms

### 3️⃣ Production Optimization

Study the **advanced-features** example to master:
- Performance monitoring and optimization
- Security configuration (whitelist, timeout)
- Health checks and error recovery
- Production environment best practices

---

## Core Concepts

### Guest-side API

```tsx
import {
  // Basic components
  View, Text, TouchableOpacity, ScrollView,
  Image, ActivityIndicator, Button,

  // Hooks
  useHostEvent,      // Listen to host events
  useSendToHost,     // Send messages to host
  useConfig,         // Get guest configuration
} from 'rill/sdk';

// Send message to host
const sendToHost = useSendToHost();
sendToHost('MY_EVENT', { data: 'value' });

// Listen to host events
useHostEvent('REFRESH', () => {
  console.log('Host requested refresh');
});

// Typed event handling
useHostEvent<{ theme: 'light' | 'dark' }>('THEME_CHANGE', (payload) => {
  console.log('Theme:', payload.theme);
});

// Get configuration
const config = useConfig<{ title: string }>();
console.log('Title:', config.title);
```

### Host-side API

```tsx
import { Engine, EngineView } from 'rill';

// Create Engine
const engine = new Engine({
  provider: provider,
  timeout: 5000,
  debug: true,
  onMetric: (name, value) => console.log(`${name}: ${value}ms`),
});

// Register custom components
engine.register({ MapView: NativeMapView });

// Send events to guest
engine.sendEvent('THEME_CHANGE', { theme: 'dark' });

// Listen to guest messages
engine.on('message', (msg) => console.log(msg));

// Render guest UI
<EngineView
  engine={engine}
  bundleUrl="https://cdn.example.com/guest.js"
  initialProps={{ userId: '123' }}
  onLoad={() => console.log('Loaded')}
  onError={(err) => console.error(err)}
/>
```

---

## FAQ

### Q: How to use third-party libraries in guests?

A: You need to configure allowed modules through `requireWhitelist` on the host side. Refer to the [advanced-features](./advanced-features) example.

### Q: How to debug guests?

A:
1. Use `bun run build:dev` to build with sourcemaps
2. Enable `debug: true` in Engine configuration
3. Use console.log for logging (visible on host side)

### Q: How to optimize guest performance?

A: Refer to the performance monitoring and optimization guide in the [advanced-features](./advanced-features) example.

### Q: How to handle guest errors?

A: Use the `onError` callback and `renderError` for custom error UI in EngineView. See the [host-integration](./host-integration) example for details.

---

## Related Resources

- [Rill Documentation](../README.md)
- [API Reference](../docs/API.md)
- [User Guide](../docs/GUIDE.md)
- [Architecture Design](../docs/ARCHITECTURE.md)
- [Production Guide](../docs/PRODUCTION_GUIDE.md)
