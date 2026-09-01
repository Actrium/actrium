# Rill + Askit Guest Development Guide

## Overview

**Rill** is a sandbox-based React rendering engine:
- **Guest**: App code running in QuickJS sandbox, produces UI operation commands
- **Host**: React Native environment, receives commands and renders native components

**Askit** is an isomorphic UI component library with shared code between Guest and Host.

## Architecture

```
┌─────────────────────────────────────────┐
│            Guest Sandbox                │
│  (QuickJS)                              │
│                                         │
│  User Code (TSX/JSX)                    │
│         ↓                               │
│  React Virtual DOM                      │
│         ↓                               │
│  Rill Operation Serialization           │
│   CREATE / UPDATE / DELETE / CALL       │
└─────────────────────────────────────────┘
                    ↓ (JSON)
┌─────────────────────────────────────────┐
│            Host Environment             │
│  (React Native)                         │
│                                         │
│  Rill Engine receives operations        │
│         ↓                               │
│  Native component rendering             │
│   <View> <Text> <TouchableOpacity>      │
└─────────────────────────────────────────┘
```

## Guest App Structure

```
my-guest-app/
├── manifest.json       # App metadata
├── src/
│   ├── left-panel.tsx  # Left panel component
│   └── right-panel.tsx # Right panel component
└── dist/               # Built output
    ├── left-panel.js
    └── right-panel.js
```

### manifest.json

```json
{
  "name": "MyApp",
  "version": "1.0.0",
  "description": "App description",
  "layout": {
    "leftPanel": "left-panel.js",
    "rightPanel": "right-panel.js",
    "rightPanelDefaultVisible": true
  }
}
```

## Basic Component Example

```tsx
// src/left-panel.tsx
import React, { useState } from 'react';
import { View, Text, TouchableOpacity, ScrollView } from 'react-native';

export default function LeftPanel() {
  const [count, setCount] = useState(0);

  return (
    <ScrollView style={{ flex: 1, padding: 16 }}>
      <Text style={{ fontSize: 24, color: '#fff' }}>
        Count: {count}
      </Text>
      <TouchableOpacity
        onPress={() => setCount(count + 1)}
        style={{ padding: 12, backgroundColor: '#007AFF', borderRadius: 8 }}
      >
        <Text style={{ color: '#fff' }}>Increment</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}
```

## Using Askit Components

```tsx
import { ThemeView, StepList, ChatBubble, UserAvatar } from 'askit';

// Theme container
<ThemeView variant="surface" padding="medium">
  <Text>Auto-themed content</Text>
</ThemeView>

// Step list
<StepList
  steps={[
    { title: 'Step 1', status: 'completed' },
    { title: 'Step 2', status: 'active' },
    { title: 'Step 3', status: 'pending' },
  ]}
/>

// Chat bubble
<ChatBubble
  message="Hello!"
  sender="user"
  timestamp={Date.now()}
/>

// User avatar
<UserAvatar name="John" size="medium" />
```

## Guest ↔ Host Communication

### Send to Host

```tsx
import { useSendToHost } from 'rill/sdk';

function MyComponent() {
  const sendToHost = useSendToHost();

  const handleAction = () => {
    sendToHost('USER_ACTION', { type: 'button_click' });
  };

  return (
    <TouchableOpacity onPress={handleAction}>
      <Text>Send Event</Text>
    </TouchableOpacity>
  );
}
```

### Receive from Host

```tsx
import { useHostEvent } from 'rill/sdk';

function MyComponent() {
  useHostEvent('DATA_UPDATED', (payload) => {
    console.log('Received:', payload);
  });

  return <View>...</View>;
}
```

### Event Bus (Askit)

```tsx
import { EventEmitter } from 'askit';

// Emit event
EventEmitter.emit('REFRESH', { force: true });

// Listen to event
EventEmitter.on('REFRESH', (data) => {
  console.log('Refresh event:', data);
});
```

## Build Process

### Using Rill CLI

```bash
# Build single panel
rill build src/left-panel.tsx --outfile dist/left-panel.js

# Build with options
rill build src/app.tsx -o dist/bundle.js --watch --sourcemap --no-minify
```

### Package as .actr

```bash
# Create .actr package (zip format)
cd my-app
zip -r ../my-app.actr manifest.json dist/*.js
```

Or use the provided script:
```bash
cd packages/actro-runtime/actr-app-examples
./pack-actr.sh my-app
```

## Sandbox Limitations

| Allowed | Not Allowed |
|---------|-------------|
| React Hooks | Node.js APIs (fs, path) |
| JavaScript standard library | Browser APIs (window, document) |
| Askit APIs (Toast, Haptic) | Direct native module access |
| Host communication | External network requests |

## Style System

Use React Native StyleSheet format:

```tsx
// Inline styles
<View style={{ flex: 1, padding: 20 }}>

// Style objects
const styles = {
  container: { flex: 1, padding: 20 },
  text: { fontSize: 16, color: '#fff' }
};

// NOT supported: CSS class names
<View className="container">  // Invalid
```

## Debugging

Enable debug mode in Host:
```typescript
const engine = new Engine({ debug: true });
```

Guest console output appears in Host logs:
```javascript
console.log('Guest: current state', state);
console.error('Guest: error', error);
```

## Best Practices

1. **State Management**: Use React Hooks (`useState`, `useReducer`)
2. **Component Splitting**: Break into small, reusable components
3. **Error Handling**: Wrap risky operations in try-catch
4. **Performance**: Use `React.memo` to optimize re-renders
5. **Communication**: Batch updates to reduce cross-boundary overhead

## Available Components

**React Native (registered in Host):**
- View, Text, Image, ScrollView
- TouchableOpacity, TextInput, Button
- FlatList, SectionList

**Askit:**
- ThemeView, StepList, ChatBubble, UserAvatar
- Toast, Haptic, EventEmitter
