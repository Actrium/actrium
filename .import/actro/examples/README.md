# Rill Plugin Examples

This directory contains Rill plugin development and host integration examples, covering all features from basic to advanced.

## Example List

### 1. simple-plugin - Basic Plugin Development

**Target Audience**: Developers new to Rill

**Features Demonstrated**:
- Basic components: View, Text, TouchableOpacity, ScrollView
- State management: useState, useEffect
- Host communication:
  - `useHostEvent` - Listen to host events
  - `useSendToHost` - Send messages to host
  - `useConfig` - Get plugin configuration
- Theme support: Light and dark theme switching

[View Example →](./simple-plugin)

---

### 2. host-integration - Complete Host Integration Example

**Target Audience**: Developers who need to integrate Rill into React Native applications

**Features Demonstrated**:
- ✅ Engine instance creation and configuration
- ✅ EngineView lifecycle management
- ✅ Bidirectional host-plugin communication
- ✅ Error handling and recovery
- ✅ Custom component registration
- ✅ QuickJS Provider configuration
- ✅ Dynamic plugin loading
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

### Building Plugins

All example plugins follow the same build process:

```bash
# Navigate to any example directory
cd examples/simple-plugin  # or host-integration, advanced-features

# Install dependencies
npm install

# Build the plugin
npm run build

# Development mode (no minification + sourcemap)
npm run build:dev

# Watch for file changes (if supported)
npm run watch
```

Build output: `dist/bundle.js` or `dist/plugin.js`

## Project Structure

### Plugin Example Structure

```
simple-plugin/
├── package.json      # Project configuration
├── README.md         # Detailed documentation
├── src/
│   └── index.tsx     # Plugin main entry
└── dist/
    └── bundle.js     # Build output
```

### Host Integration Example Structure

```
host-integration/
├── package.json
├── README.md         # Complete integration guide
├── src/
│   ├── plugin.tsx            # Plugin code
│   ├── HostApp.tsx           # Host application example
│   └── QuickJSProvider.tsx   # Provider configuration
└── dist/
    └── plugin.js
```

## Creating a New Plugin

1. Copy the `simple-plugin` directory
2. Modify the name in `package.json`
3. Edit `src/index.tsx` to implement plugin logic
4. Run `npm run build` to build

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
  useConfig,         // Get plugin configuration

  // Types
  StyleProp,
  LayoutEvent,
  ScrollEvent,
} from 'rill/sdk';
```

## Learning Path

Recommended learning order:

### 1️⃣ Getting Started

Start with **simple-plugin** to learn:
- How to write basic plugins
- Usage of SDK components and hooks
- Plugin build process

### 2️⃣ Host Integration

Read the **host-integration** example to understand:
- How to integrate Rill into your application
- Usage of Engine and EngineView
- Lifecycle management and error handling
- Host-plugin communication mechanisms

### 3️⃣ Production Optimization

Study the **advanced-features** example to master:
- Performance monitoring and optimization
- Security configuration (whitelist, timeout)
- Health checks and error recovery
- Production environment best practices

---

## Core Concepts

### Plugin-side API

```tsx
import {
  // Basic components
  View, Text, TouchableOpacity, ScrollView,
  Image, ActivityIndicator, Button,

  // Hooks
  useHostEvent,      // Listen to host events
  useSendToHost,     // Send messages to host
  useConfig,         // Get plugin configuration
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
  quickjs: provider,
  timeout: 5000,
  debug: true,
  onMetric: (name, value) => console.log(`${name}: ${value}ms`),
});

// Register custom components
engine.register({ MapView: NativeMapView });

// Send events to plugin
engine.sendEvent('THEME_CHANGE', { theme: 'dark' });

// Listen to plugin messages
engine.on('message', (msg) => console.log(msg));

// Render plugin UI
<EngineView
  engine={engine}
  bundleUrl="https://cdn.example.com/plugin.js"
  initialProps={{ userId: '123' }}
  onLoad={() => console.log('Loaded')}
  onError={(err) => console.error(err)}
/>
```

---

## FAQ

### Q: How to use third-party libraries in plugins?

A: You need to configure allowed modules through `requireWhitelist` on the host side. Refer to the [advanced-features](./advanced-features) example.

### Q: How to debug plugins?

A:
1. Use `npm run build:dev` to build with sourcemaps
2. Enable `debug: true` in Engine configuration
3. Use console.log for logging (visible on host side)

### Q: How to optimize plugin performance?

A: Refer to the performance monitoring and optimization guide in the [advanced-features](./advanced-features) example.

### Q: How to handle plugin errors?

A: Use the `onError` callback and `renderError` for custom error UI in EngineView. See the [host-integration](./host-integration) example for details.

---

## Related Resources

- [Rill Documentation](../README.md)
- [API Reference](../docs/API.md)
- [User Guide](../docs/GUIDE.md)
- [Architecture Design](../docs/ARCHITECTURE.md)
- [Production Guide](../docs/PRODUCTION_GUIDE.md)
