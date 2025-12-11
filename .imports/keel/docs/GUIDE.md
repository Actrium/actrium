# Rill User Guide

## Introduction

Rill is a lightweight React Native dynamic UI rendering engine, similar to Shopify's remote-ui. It allows running React components in a secure sandbox environment and passing the render results to the host application for display.

### Core Features

- **Secure Sandbox** - Uses QuickJS to isolate guest code
- **React Development Experience** - Supports JSX, Hooks, and other modern React features
- **High Performance** - Batch updates, operation merging, virtual scrolling
- **Type Safety** - Complete TypeScript support
- **Zero Dependencies** - SDK doesn't depend on react-native

---

## Quick Start

### 1. Installation

```bash
# In host application
bun add rill react-native-quickjs

# In guest project (dev dependency only)
bun add -D rill
```

### 2. Create Guest

```tsx
// src/guest.tsx
import { View, Text, TouchableOpacity, useConfig, useSendToHost } from 'rill/sdk';

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

### 3. Build Guest

```bash
bunx rill build src/guest.tsx -o dist/bundle.js
```

### 4. Use in Host Application

```tsx
// App.tsx
import React from 'react';
import { SafeAreaView, Text } from 'react-native';
import { EngineView } from 'rill/runtime';

export default function App() {
  return (
    <SafeAreaView style={{ flex: 1 }}>
      <EngineView
        source="https://cdn.example.com/guest.js"
        initialProps={{
          title: 'Hello Rill',
          theme: 'light',
        }}
        onLoad={() => console.log('Guest loaded')}
        onError={(error) => console.error('Guest error:', error)}
        fallback={<Text>Loading guest...</Text>}
      />
    </SafeAreaView>
  );
}
```

---

## Guest Development

### Project Structure

```
my-guest/
├── src/
│   └── guest.tsx    # Guest entry
├── dist/
│   └── bundle.js     # Build output
├── package.json
└── tsconfig.json
```

### package.json

```json
{
  "name": "my-guest",
  "version": "1.0.0",
  "scripts": {
    "build": "rill build src/guest.tsx -o dist/bundle.js",
    "watch": "rill build src/guest.tsx -o dist/bundle.js --watch"
  },
  "devDependencies": {
    "rill": "^1.0.0",
    "typescript": "^5.0.0"
  }
}
```

### tsconfig.json

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "paths": {
      "rill/sdk": ["./node_modules/rill/dist/sdk"]
    }
  },
  "include": ["src"]
}
```

### Using Virtual Components

Virtual components are string identifiers that are transformed into operation instructions during build:

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity } from 'rill/sdk';

function MyComponent() {
  return (
    <View style={{ flex: 1 }}>
      <ScrollView>
        <Image
          source={{ uri: 'https://example.com/image.png' }}
          style={{ width: 100, height: 100 }}
        />
        <Text>Hello World</Text>
        <TouchableOpacity onPress={() => console.log('pressed')}>
          <Text>Click me</Text>
        </TouchableOpacity>
      </ScrollView>
    </View>
  );
}
```

### Using Hooks

#### useConfig - Get Configuration

```tsx
interface Config {
  userId: string;
  theme: 'light' | 'dark';
  features: string[];
}

function Guest() {
  const config = useConfig<Config>();

  return (
    <View>
      <Text>User: {config.userId}</Text>
      <Text>Theme: {config.theme}</Text>
    </View>
  );
}
```

#### useHostEvent - Listen to Host Events

```tsx
function Guest() {
  const [refreshCount, setRefreshCount] = useState(0);

  useHostEvent<{ force: boolean }>('REFRESH', (payload) => {
    setRefreshCount((c) => c + 1);
    if (payload.force) {
      // Force refresh logic
    }
  });

  return <Text>Refreshed {refreshCount} times</Text>;
}
```

#### useSendToHost - Send Events to Host

```tsx
function Guest() {
  const sendToHost = useSendToHost();

  const handleComplete = (result: string) => {
    sendToHost('TASK_COMPLETE', { result, timestamp: Date.now() });
  };

  return (
    <TouchableOpacity onPress={() => handleComplete('success')}>
      <Text>Complete Task</Text>
    </TouchableOpacity>
  );
}
```

### Styling

Supports most React Native style properties:

```tsx
<View
  style={{
    flex: 1,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    padding: 16,
    margin: 8,
    backgroundColor: '#f5f5f5',
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#ddd',
  }}
>
  <Text
    style={{
      fontSize: 16,
      fontWeight: 'bold',
      color: '#333',
      textAlign: 'center',
    }}
  >
    Styled Text
  </Text>
</View>
```

### List Rendering

Use FlatList for rendering long lists:

```tsx
interface Item {
  id: string;
  title: string;
}

function Guest() {
  const [items] = useState<Item[]>([
    { id: '1', title: 'Item 1' },
    { id: '2', title: 'Item 2' },
    { id: '3', title: 'Item 3' },
  ]);

  return (
    <FlatList
      data={items}
      keyExtractor={(item) => item.id}
      renderItem={({ item }) => (
        <View style={{ padding: 16, borderBottomWidth: 1 }}>
          <Text>{item.title}</Text>
        </View>
      )}
      ListHeaderComponent={<Text style={{ fontSize: 24 }}>My List</Text>}
      ListEmptyComponent={<Text>No items</Text>}
    />
  );
}
```

---

## Host Integration

### Basic Integration

```tsx
import React from 'react';
import { View } from 'react-native';
import { EngineView } from 'rill/runtime';

function GuestHost() {
  return (
    <View style={{ flex: 1 }}>
      <EngineView
        source="https://cdn.example.com/guest.js"
        initialProps={{ theme: 'dark' }}
      />
    </View>
  );
}
```

### Custom Components

Register host-side native components for guest use:

```tsx
import { NativeStepList } from './components/NativeStepList';
import { CustomButton } from './components/CustomButton';

function GuestHost() {
  return (
    <EngineView
      source={bundleUrl}
      components={{
        StepList: NativeStepList,
        CustomButton: CustomButton,
      }}
    />
  );
}
```

Using custom components in guests:

```tsx
// Declare custom component types in guest
declare const StepList: string;
declare const CustomButton: string;

function Guest() {
  return (
    <View>
      <StepList steps={['Step 1', 'Step 2', 'Step 3']} />
      <CustomButton title="Submit" variant="primary" />
    </View>
  );
}
```

### Event Communication

#### Host -> Guest

```tsx
import { useRef } from 'react';
import { Engine } from 'rill/runtime';

function GuestHost() {
  const engineRef = useRef<Engine>(null);

  const handleRefresh = () => {
    engineRef.current?.sendEvent('REFRESH', { force: true });
  };

  return (
    <View>
      <Button title="Refresh" onPress={handleRefresh} />
      <EngineView
        ref={engineRef}
        source={bundleUrl}
      />
    </View>
  );
}
```

#### Guest -> Host

Listen for operation events in EngineView:

```tsx
import { Engine, EngineView } from 'rill/runtime';

function GuestHost() {
  const handleGuestEvent = (eventName: string, payload: unknown) => {
    switch (eventName) {
      case 'TASK_COMPLETE':
        console.log('Task completed:', payload);
        break;
      case 'NAVIGATION':
        navigation.navigate(payload.route);
        break;
    }
  };

  return (
    <EngineView
      source={bundleUrl}
      onGuestEvent={handleGuestEvent}
    />
  );
}
```

### Using Engine API

Use the Engine class directly for more control:

```tsx
import { Engine, Receiver, ComponentRegistry } from 'rill/runtime';

function useRillEngine(bundleUrl: string, initialProps: object) {
  const [tree, setTree] = useState<React.ReactElement | null>(null);
  const engineRef = useRef<Engine | null>(null);
  const receiverRef = useRef<Receiver | null>(null);

  useEffect(() => {
    const engine = new Engine({ debug: __DEV__ });
    const registry = new ComponentRegistry();

    // Register default components
    registry.registerAll(DefaultComponents);

    // Create Receiver
    const receiver = engine.createReceiver(() => {
      setTree(receiver.render());
    });

    engineRef.current = engine;
    receiverRef.current = receiver;

    // Load guest
    engine.loadBundle(bundleUrl, initialProps).catch(console.error);

    return () => {
      engine.destroy();
    };
  }, [bundleUrl]);

  return { tree, engine: engineRef.current };
}
```

---

## Performance Optimization

### Batch Updates

Rill automatically batches updates to optimize performance:

```tsx
import { ThrottledScheduler } from 'rill/runtime';

// Custom throttle configuration
const scheduler = new ThrottledScheduler(
  (batch) => receiver.applyBatch(batch),
  {
    maxBatchSize: 100,    // Maximum batch size
    throttleMs: 16,       // ~60fps
    enableMerge: true,    // Enable operation merging
  }
);
```

### Virtual Scrolling

Use virtual scrolling for long lists:

```tsx
import { VirtualScrollCalculator } from 'rill/runtime';

const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 60,  // Estimated item height
  overscan: 5,              // Buffer outside visible area
  scrollThrottleMs: 16,     // Scroll throttle
});

// Calculate visible range
const state = calculator.calculate(scrollTop, viewportHeight);
// Only render items in state.visibleItems
```

### Performance Monitoring

```tsx
import { PerformanceMonitor } from 'rill/runtime';

const monitor = new PerformanceMonitor();

// Record batches
engine.on('operation', (batch) => {
  monitor.recordBatch(batch);
});

// View metrics
const metrics = monitor.getMetrics();
console.log(`Total operations: ${metrics.totalOperations}`);
console.log(`Average batch size: ${metrics.avgBatchSize}`);
console.log(`Merged operations: ${metrics.mergedOperations}`);
```

---

## Debugging

### Enable DevTools

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools({
  inspector: { maxDepth: 10 },
  maxLogs: 100,
  maxTimelineEvents: 500,
});

// Enable in development
if (__DEV__) {
  devtools.enable();
}

// Record events
engine.on('operation', (batch) => {
  devtools.onBatch(batch);
});
```

### View Component Tree

```tsx
const receiver = engine.getReceiver();
const treeText = devtools.getComponentTreeText(
  receiver.nodeMap,
  receiver.rootChildren
);

console.log(treeText);
// Output:
// └─ <View testID="root">
//    ├─ <Text numberOfLines={2}>
//    └─ <TouchableOpacity>
//       └─ <Text>
```

### Export Debug Data

```tsx
const debugData = devtools.exportAll();
// Save or send to server
```

---

## Security Considerations

### Sandbox Isolation

- Guest code runs in QuickJS sandbox
- Cannot access host's native APIs
- Cannot make network requests (unless host provides)
- Cannot access file system

### Component Whitelist

Only explicitly registered components can be used by guests:

```tsx
// Only register safe components
engine.register({
  View: SafeView,
  Text: SafeText,
  Image: SafeImage,
  // Don't register potentially risky components
});
```

### Timeout Protection

Prevent malicious scripts from consuming resources:

```tsx
const engine = new Engine({
  timeout: 5000,  // 5 second timeout
});
```

### Error Isolation

Guest errors won't crash the host application:

```tsx
<EngineView
  source={bundleUrl}
  onError={(error) => {
    // Log error
    reportError(error);
    // Show fallback UI
  }}
  fallback={<ErrorFallback />}
/>
```

---

## FAQ

### 1. Guest Load Failed

**Problem**: `Failed to fetch bundle: 404`

**Solution**: Check if bundle URL is correct and server is running.

### 2. Component Not Showing

**Problem**: Console shows `Component "X" not registered`

**Solution**: Register the component on host side:
```tsx
engine.register({ X: MyXComponent });
```

### 3. Styles Not Working

**Problem**: Style properties don't take effect

**Solution**: Use camelCase naming, not CSS format:
```tsx
// Correct
style={{ backgroundColor: 'red', fontSize: 16 }}

// Wrong
style={{ 'background-color': 'red', 'font-size': 16 }}
```

### 4. Callbacks Not Firing

**Problem**: onPress and other events don't respond

**Solution**: Check if function is passed correctly, don't unnecessarily wrap with arrow functions:
```tsx
// Recommended
<TouchableOpacity onPress={handlePress}>

// Also works
<TouchableOpacity onPress={() => handlePress()}>
```

### 5. Memory Leaks

**Problem**: Memory keeps growing

**Solution**: Ensure engine is destroyed on component unmount:
```tsx
useEffect(() => {
  return () => engine.destroy();
}, []);
```

---

## Example Projects

For complete examples, see the `examples/` directory:

- `examples/basic-guest/` - Basic guest example
- `examples/host-app/` - Host application example
- `examples/custom-components/` - Custom components example
