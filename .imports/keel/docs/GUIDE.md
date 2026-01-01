# Rill User Guide

## Introduction

Rill is a lightweight React Native dynamic UI rendering engine, similar to Shopify's remote-ui. It allows running React components in a secure sandbox environment and passing the render results to the host application for display.

### Core Features

- **Secure Sandbox** - Pluggable JSEngineProvider (QuickJS, VM, Worker)
- **React Development Experience** - Supports JSX, Hooks, and other modern React features
- **High Performance** - Batch updates, operation merging, virtual scrolling
- **Type Safety** - Complete TypeScript support
- **Zero Dependencies** - SDK doesn't depend on react-native

---

## Quick Start

### 1. Installation

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

### 2. Create Guest

```tsx
// src/guest.tsx
import { View, Text, TouchableOpacity, useConfig, useSendToHost } from 'rill/let';

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
bun run rill/cli build src/guest.tsx -o dist/bundle.js
```

### 4. Use in Host Application

```tsx
// App.tsx
import React, { useMemo, useEffect } from 'react';
import { SafeAreaView, Text, ActivityIndicator } from 'react-native';
import { Engine, EngineView } from 'rill';

export default function App() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  return (
    <SafeAreaView style={{ flex: 1 }}>
      <EngineView
        engine={engine}
        source="https://cdn.example.com/guest.js"
        initialProps={{
          title: 'Hello Rill',
          theme: 'light',
        }}
        onLoad={() => console.log('Guest loaded')}
        onError={(error) => console.error('Guest error:', error)}
        fallback={<ActivityIndicator />}
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
│   └── bundle.js    # Build output
├── package.json
└── tsconfig.json
```

### package.json

```json
{
  "name": "my-guest",
  "version": "1.0.0",
  "scripts": {
    "build": "bun run rill/cli build src/guest.tsx -o dist/bundle.js",
    "watch": "bun run rill/cli build src/guest.tsx -o dist/bundle.js --watch"
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
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

### Using Virtual Components

Virtual components are string identifiers that are transformed into operation instructions during build:

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity } from 'rill/let';

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
import { useConfig } from 'rill/let';

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
import { useState } from 'react';
import { View, Text, useHostEvent } from 'rill/let';

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
import { TouchableOpacity, Text, useSendToHost } from 'rill/let';

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

#### useRemoteRef - Call Host Component Methods

Call methods on host component instances (like `focus()`, `scrollTo()`):

```tsx
import { useRemoteRef, View, TextInput, TouchableOpacity, Text, TextInputRef } from 'rill/let';

function Guest() {
  const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();

  const handleFocus = async () => {
    await remoteInput?.invoke('focus');
  };

  const handleClear = async () => {
    await remoteInput?.invoke('clear');
  };

  return (
    <View>
      <TextInput ref={inputRef} placeholder="Enter text" />
      <TouchableOpacity onPress={handleFocus}>
        <Text>Focus</Text>
      </TouchableOpacity>
      <TouchableOpacity onPress={handleClear}>
        <Text>Clear</Text>
      </TouchableOpacity>
    </View>
  );
}
```

Available ref types:
- `TextInputRef`: `focus()`, `blur()`, `clear()`
- `ScrollViewRef`: `scrollTo({ x, y, animated })`, `scrollToEnd()`
- `FlatListRef`: `scrollToIndex()`, `scrollToOffset()`

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
import { useState } from 'react';
import { View, Text, FlatList } from 'rill/let';

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
import React, { useMemo } from 'react';
import { View } from 'react-native';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  return (
    <View style={{ flex: 1 }}>
      <EngineView
        engine={engine}
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
import React, { useMemo, useEffect } from 'react';
import { Engine, EngineView } from 'rill';
import { NativeStepList } from './components/NativeStepList';
import { CustomButton } from './components/CustomButton';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  useEffect(() => {
    engine.register({
      StepList: NativeStepList,
      CustomButton: CustomButton,
    });
  }, [engine]);

  return (
    <EngineView
      engine={engine}
      source={bundleUrl}
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
import React, { useMemo } from 'react';
import { Button, View } from 'react-native';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  const handleRefresh = () => {
    engine.sendEvent('REFRESH', { force: true });
  };

  return (
    <View>
      <Button title="Refresh" onPress={handleRefresh} />
      <EngineView engine={engine} source={bundleUrl} />
    </View>
  );
}
```

#### Guest -> Host

Listen for messages from guest using engine events:

```tsx
import React, { useMemo, useEffect } from 'react';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  useEffect(() => {
    const unsubscribe = engine.on('message', (message) => {
      switch (message.event) {
        case 'TASK_COMPLETE':
          console.log('Task completed:', message.payload);
          break;
        case 'NAVIGATION':
          navigation.navigate(message.payload.route);
          break;
      }
    });

    return unsubscribe;
  }, [engine]);

  return <EngineView engine={engine} source={bundleUrl} />;
}
```

---

## Debugging

### Basic Debugging

Use engine events to monitor runtime behavior:

```tsx
import { Engine } from 'rill';

const engine = new Engine({ debug: true });

// Monitor errors
engine.on('error', (error) => {
  console.error('[Guest Error]', error);
});

// Monitor operations
engine.on('operation', (batch) => {
  console.log(`Operations: ${batch.operations.length}`);
});

// Health check
const health = engine.getHealth();
console.log('Health:', health);
```

### DevTools (Optional)

For advanced debugging, use the DevTools package:

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools();

if (__DEV__) {
  devtools.enable();
  devtools.connectEngine(engine);
}

// Get component tree
const tree = devtools.getHostTree();

// Export debug data
const data = devtools.export();
```

---

## Security Considerations

### Sandbox Isolation

- Guest code runs in JSEngineProvider sandbox (QuickJS, VM, Worker)
- Cannot access host's native APIs
- Cannot make network requests (unless host provides)
- Cannot access file system

### Component Whitelist

Only explicitly registered components can be used by guests:

```tsx
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
  engine={engine}
  source={bundleUrl}
  onError={(error) => {
    reportError(error);
  }}
  renderError={(error) => <ErrorFallback error={error} />}
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

**Solution**: Check if function is passed correctly:
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
