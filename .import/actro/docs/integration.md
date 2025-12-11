# askit & rill Integration Guide

This guide explains how askit integrates with rill to enable sandboxed Guest execution with a unified API surface.

## Overview

askit acts as an **isomorphic UI/API layer** on top of rill's sandboxing engine, providing:
- Unified APIs that work identically in both Host and Guest
- Pre-built UI components optimized for the AskAway platform
- Automatic message routing and bridging

```
┌─────────────────────────────────────────────────────────┐
│                     Your Host App                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │                                                  │   │
│  │    import { Engine } from 'rill'                │   │
│  │    import { createEngineAdapter,                │   │
│  │             components } from 'askit/core'      │   │
│  │                                                  │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
                           │
                           │ Integration Layer
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    askit/core                            │
│  ┌──────────────────┐         ┌──────────────────┐     │
│  │  Bridge Adapter  │◄────────┤   AskitRegistry  │     │
│  │                  │         │                  │     │
│  │  - Message       │         │  - Components    │     │
│  │    routing       │         │  - Modules       │     │
│  │  - Event         │         │  - Handlers      │     │
│  │    forwarding    │         │                  │     │
│  └────────┬─────────┘         └──────────────────┘     │
└───────────┼──────────────────────────────────────────────┘
            │
            │ engine.on('message') / engine.sendEvent()
            ▼
┌─────────────────────────────────────────────────────────┐
│                    rill Engine                           │
│  ┌──────────────────────────────────────────────────┐  │
│  │  QuickJS Sandbox Runtime                         │  │
│  │                                                   │  │
│  │  - Executes Guest code                           │  │
│  │  - Provides global.sendToHost()                  │  │
│  │  - Provides global.onHostEvent()                 │  │
│  │  - Component registration                        │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
            │
            │ Guest imports 'askit'
            ▼
┌─────────────────────────────────────────────────────────┐
│                    Guest Code                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │  import { EventEmitter, Toast, StepList } from 'askit'│
│  │                                                   │  │
│  │  EventEmitter.emit('ready')                      │  │
│  │  Toast.show('Hello!')                            │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Integration Flow

### 1. Initial Setup

```typescript
// Host App initialization
import { Engine } from 'rill';
import { createEngineAdapter, components } from 'askit/core';

// Step 1: Create rill engine
const engine = new Engine();

// Step 2: Create askit bridge adapter
const adapter = createEngineAdapter(engine);

// Step 3: Register askit components
engine.register(components);

// Step 4: Load Guest bundle
await engine.loadGuest('https://cdn.example.com/guest.js');
```

**Visual Flow:**

```
Host App Startup
     │
     ├─► new Engine()
     │        │
     │        └─► QuickJS context created
     │
     ├─► createEngineAdapter(engine)
     │        │
     │        ├─► Listen to engine.on('message')
     │        └─► Register EventEmitter forwarder
     │
     ├─► engine.register(components)
     │        │
     │        └─► Register: StepList, ThemeView, UserAvatar, ChatBubble
     │
     └─► engine.loadGuest(url)
              │
              ├─► Download Guest bundle
              ├─► Execute in QuickJS sandbox
              └─► Guest can now import 'askit'
```

### 2. Message Routing

When Guest calls askit APIs, messages are routed through multiple layers:

```
┌─────────────────────────────────────────────────────────────────┐
│ GUEST (QuickJS Sandbox)                                         │
│                                                                  │
│  Toast.show('Hello', { duration: 'short' })                     │
│     │                                                            │
│     └─► askit/Guest implementation                              │
│           │                                                      │
│           └─► global.sendToHost({                               │
│                 event: 'askit:toast:show',                       │
│                 payload: ['Hello', { duration: 'short' }]        │
│               })                                                 │
└────────────────────────────┼────────────────────────────────────┘
                             │
                Message: { event, payload }
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL ENGINE                ▼                                     │
│                                                                  │
│  engine.emit('message', { event, payload })                     │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT BRIDGE               ▼                                     │
│                                                                  │
│  handleGuestMessage({ event, payload })                         │
│     │                                                            │
│     ├─► Parse event: 'askit:toast:show'                         │
│     │     → module='toast', method='show'                        │
│     │                                                            │
│     └─► Call modules['toast']['show'](payload)                  │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT HOST MODULE          ▼                                     │
│                                                                  │
│  Toast.show('Hello', { duration: 'short' })                     │
│     │                                                            │
│     └─► ToastAndroid.show('Hello', ToastAndroid.SHORT)          │
│              │                                                   │
│              └─► Native Android Toast appears                   │
└───────────────────────────────────────────────────────────────────┘
```

### 3. Event Broadcasting (Host → Guest)

Host can send events to Guest through the EventEmitter:

```
┌─────────────────────────────────────────────────────────────────┐
│ HOST APP                                                         │
│                                                                  │
│  import { EventEmitter } from 'askit/core';                     │
│                                                                  │
│  EventEmitter.emit('config:update', { theme: 'dark' })          │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT BRIDGE               ▼                                     │
│                                                                  │
│  // Uses Symbol-based internal API                              │
│  EventEmitter[BROADCASTER_SYMBOL]((event, payload) => {         │
│    engine.sendEvent(event, payload)                             │
│  })                                                              │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL ENGINE                ▼                                     │
│                                                                  │
│  Calls global.onHostEvent('config:update', { theme: 'dark' })   │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ GUEST (QuickJS)            ▼                                     │
│                                                                  │
│  import { EventEmitter } from 'askit';                          │
│                                                                  │
│  EventEmitter.on('config:update', (config) => {                 │
│    console.log('Theme changed to:', config.theme);              │
│  });                                                             │
└───────────────────────────────────────────────────────────────────┘
```

### 4. UI Component Rendering

askit components return DSL objects in Guest, which rill renders natively in Host:

```
┌─────────────────────────────────────────────────────────────────┐
│ GUEST CODE                                                       │
│                                                                  │
│  import { StepList } from 'askit';                              │
│                                                                  │
│  const ui = StepList({                                          │
│    steps: [                                                      │
│      { id: '1', label: 'Step 1', status: 'completed' },         │
│      { id: '2', label: 'Step 2', status: 'active' }             │
│    ]                                                             │
│  });                                                             │
│                                                                  │
│  // Returns: 'StepList' (string identifier)                     │
└────────────────────────────┼────────────────────────────────────┘
                             │
                   DSL: { type: 'StepList', props }
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL ENGINE                ▼                                     │
│                                                                  │
│  Receives DSL object from Guest                                 │
│  Looks up 'StepList' in registered components                   │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ HOST APP                   ▼                                     │
│                                                                  │
│  Renders: <StepList                                             │
│             steps={[...]}                                        │
│           />                                                     │
│                                                                  │
│  This is the REAL React Native component                        │
└───────────────────────────────────────────────────────────────────┘
```

## Complete Example

### Host App Setup

```typescript
// App.tsx
import React, { useEffect, useState } from 'react';
import { View } from 'react-native';
import { Engine, EngineView } from 'rill';
import { createEngineAdapter, components } from 'askit/core';
import { EventEmitter } from 'askit';

export default function App() {
  const [engine] = useState(() => {
    // 1. Create engine
    const eng = new Engine();

    // 2. Setup askit bridge
    createEngineAdapter(eng);

    // 3. Register components
    eng.register(components);

    return eng;
  });

  useEffect(() => {
    // 4. Send events to Guest
    EventEmitter.emit('app:ready', { version: '1.0.0' });
  }, []);

  return (
    <View style={{ flex: 1 }}>
      {/* 5. Render Guest UI */}
      <EngineView
        engine={engine}
        bundleUrl="https://cdn.example.com/guest.js"
        initialProps={{ userId: '123' }}
      />
    </View>
  );
}
```

### Guest Code

```typescript
// guest.ts
import { EventEmitter, Toast, Haptic, StepList } from 'askit';

// Listen for Host events
EventEmitter.on('app:ready', (data) => {
  Toast.show(`App version: ${data.version}`);
  Haptic.trigger('success');
});

// Send events to Host
EventEmitter.emit('guest:loaded', { timestamp: Date.now() });

// Render UI
export default function GuestUI(props) {
  return StepList({
    steps: [
      { id: '1', label: 'Welcome', status: 'completed' },
      { id: '2', label: 'Get Started', status: 'active' },
    ],
  });
}
```

## Lifecycle Sequence

```
┌──────────┐
│   HOST   │
└─────┬────┘
      │
      │ 1. new Engine()
      │ 2. createEngineAdapter(engine)
      │ 3. engine.register(components)
      │
      ▼
┌─────────────────┐
│  ASKIT BRIDGE   │ ← Listening for messages
└─────┬───────────┘
      │
      │ 4. engine.loadGuest(url)
      │
      ▼
┌──────────────┐
│ RILL ENGINE  │
└─────┬────────┘
      │
      │ 5. Download & execute Guest
      │
      ▼
┌──────────┐
│  GUEST   │
└─────┬────┘
      │
      │ 6. import 'askit'
      │ 7. EventEmitter.emit('guest:loaded')
      │
      ▼
┌─────────────────┐
│  ASKIT BRIDGE   │ ← Receives message
└─────┬───────────┘
      │
      │ 8. Routes to EventEmitter handlers
      │
      ▼
┌──────────┐
│   HOST   │ ← EventEmitter.on('guest:loaded') triggered
└──────────┘
```

## Key Concepts

### 1. Conditional Exports
askit uses package.json `exports` to serve different implementations:
- Host imports from `./src/index.host.ts` (React Native environment)
- Guest imports from `./src/index.guest.ts` (QuickJS environment)

### 2. Message Protocol
All Guest→Host communication uses the format:
```typescript
{
  event: 'askit:module:method',  // e.g., 'askit:toast:show'
  payload: any[]                  // Method arguments
}
```

### 3. Bridge Adapter
The bridge adapter connects askit to rill's message system:
- Listens to `engine.on('message')` for Guest→Host
- Uses `EventEmitter[BROADCASTER_SYMBOL]()` for Host→Guest (Symbol-based internal API)
- Routes messages to appropriate handlers

### 4. Component Registry
`components` maps string identifiers to React components:
```typescript
{
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble,
}
```

## Debugging

### Enable Message Logging

```typescript
// In Host
import { Bridge } from 'askit/core';

const originalHandler = Bridge.handleMessage;
Bridge.handleMessage = (message) => {
  console.log('[askit] Guest→Host:', message);
  return originalHandler(message);
};
```

### Inspect Guest Environment

```typescript
// In Guest
console.log('Available globals:', Object.keys(global));
console.log('Can send to host:', typeof global.sendToHost === 'function');
```

## Best Practices

1. **Always create the bridge adapter** before loading Guest code
2. **Register components** before calling `engine.loadGuest()`
3. **Use EventEmitter for bidirectional events**, not direct message passing
4. **Keep Guest bundles small** - askit is already included via `rill/sdk`
5. **Handle errors gracefully** - Guest crashes shouldn't crash Host

## Troubleshooting

| Issue | Cause | Solution |
|-------|-------|----------|
| "sendToHost is not a function" | Guest loaded before engine setup | Create adapter before `loadGuest()` |
| Component not rendering | Component not registered | Add to `components` and call `engine.register()` |
| Messages not received | Bridge not created | Call `createEngineAdapter(engine)` |
| EventEmitter events not working | Adapter created after loadGuest | Create adapter during engine setup |

## Next Steps

- [API Reference](./api-reference.md) - Detailed API documentation
- [Components](./components.md) - Available UI components
- [Architecture](./architecture.md) - Internal design details
