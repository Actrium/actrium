# Rill Architecture Design Document

> See also: [Internal Implementation Details](./internals/) (Bridge layer, serialization, sandbox comparison)

## 1. Design Philosophy

### 1.1 Remote-UI vs Rill

| Dimension | Shopify Remote-UI | Rill |
|-----------|-------------------|------|
| Target Platform | Web (DOM) | React Native & Web |
| Sandbox Technology | iframe / Web Worker | Pluggable JSEngineProvider (JSC, QuickJS, NodeVM, WasmQuickJS) |
| Render Target | Real DOM Elements | RN Native Components |
| Communication | postMessage | JSEngineProvider Bridge |
| Change Detection | MutationObserver | react-reconciler |
| Component Model | Custom Elements | Virtual Components (string identifiers) |

### 1.2 Core Concepts

1. **Producer-Consumer Pattern**: Sandbox produces UI descriptions, host consumes and renders
2. **Whitelist Component Security Model**: Only registered component types can be rendered
3. **Function Serialization Mechanism**: Callbacks converted to ID references
4. **Batch Update Optimization**: Aggregate multiple operations into single transmission
5. **Unified Bridge Layer**: Type-safe serialization with automatic callback lifecycle management

---

## 2. Overall Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Host App (React Native)                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────────┐  │
│  │  EngineView │───▶│   Engine    │───▶│      JSEngineProvider Context   │  │
│  │  (React)    │    │  (Manager)  │    │  ┌─────────────────────────────┐│  │
│  └─────────────┘    └─────────────┘    │  │     Guest Bundle.js        ││  │
│         │                 │            │  │  ┌─────────────────────────┐││  │
│         │                 │            │  │  │  React + Reconciler     │││  │
│         ▼                 │            │  │  │  (Custom Renderer)      │││  │
│  ┌─────────────┐          │            │  │  └─────────────────────────┘││  │
│  │  Receiver   │◀─────────┘            │  │             │               ││  │
│  │  (Parser)   │    Operations         │  │             ▼               ││  │
│  └─────────────┘    (Serialized)       │  │  ┌─────────────────────────┐││  │
│         │                              │  │  │  sendToHost(ops)        │││  │
│         ▼                              │  │  └─────────────────────────┘││  │
│  ┌─────────────┐    ┌─────────────┐    │  └─────────────────────────────┘│  │
│  │  Registry   │    │   Bridge    │    └─────────────────────────────────┘  │
│  │  (Mapping)  │    │ (Serialize) │                                         │
│  └─────────────┘    └─────────────┘                                         │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    Native Component Tree (View, Text, Image...)      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Module Structure

```
rill/
├── src/
│   ├── host/              # Host runtime
│   │   ├── Engine.ts      # Engine main class
│   │   ├── receiver/      # Operation receiver
│   │   ├── registry.ts    # Component registry
│   │   ├── engine/        # Engine internal modules
│   │   └── preset/        # Default component presets
│   │
│   ├── guest/             # Guest-side code
│   │   ├── bundle.ts      # Guest runtime entry (sets global React/RillSDK/Reconciler)
│   │   ├── runtime/       # Guest runtime modules
│   │   │   └── reconciler/  # Custom reconciler
│   │   └── build/         # Built runtime bundle (GUEST_BUNDLE_CODE)
│   │
│   ├── sdk/               # Guest SDK (rill/sdk)
│   │   ├── index.ts       # SDK exports
│   │   ├── sdk.ts         # Hooks/components implementation
│   │   └── types.ts       # Type definitions
│   │
│   ├── shared/            # Shared utilities
│   │   ├── index.ts       # Exports
│   │   ├── types.ts       # Type definitions
│   │   ├── TypeRules.ts   # Serialization rules
│   │   ├── bridge/        # Bridge layer
│   │   └── CallbackRegistry.ts
│   │
│   ├── sandbox/           # Sandbox providers
│   │   ├── types/         # Provider interfaces
│   │   ├── providers/     # VM, JSC, QuickJS providers
│   │   ├── native/        # Native JSI bindings
│   │   ├── wasm/          # WASM QuickJS sandbox
│   │   └── web/           # Web Worker sandbox
│   │
│   ├── cli/               # CLI build tools
│   │   └── build.ts       # Bun-based bundler
│   │
│   └── devtools/          # Development tools
```

---

## 4. Data Flow Design

### 4.1 Render Flow (Guest → Host)

```
Guest JSX           Reconciler              Bridge              Receiver           Native Components
   │                   │                     │                    │                  │
   │  <View>           │                     │                    │                  │
   │──────────────────▶│                     │                    │                  │
   │                   │ createInstance()    │                    │                  │
   │                   │ ─────────────────┐  │                    │                  │
   │                   │                  │  │                    │                  │
   │                   │◀─────────────────┘  │                    │                  │
   │                   │                     │                    │                  │
   │                   │ appendChild()       │                    │                  │
   │                   │ ─────────────────┐  │                    │                  │
   │                   │                  │  │                    │                  │
   │                   │◀─────────────────┘  │                    │                  │
   │                   │                     │                    │                  │
   │                   │    Commit Phase     │                    │                  │
   │                   │────────────────────▶│                    │                  │
   │                   │   [Operation[]]     │                    │                  │
   │                   │                     │ serialize(ops)     │                  │
   │                   │                     │───────────────────▶│                  │
   │                   │                     │                    │ applyOperations  │
   │                   │                     │                    │─────────────────▶│
   │                   │                     │                    │                  │
```

### 4.2 Event Flow (Host → Guest)

```
User Click           Native Component        Receiver              Bridge             Guest
   │                 │                    │                    │                   │
   │  onPress        │                    │                    │                   │
   │────────────────▶│                    │                    │                   │
   │                 │ handlePress(fnId)  │                    │                   │
   │                 │───────────────────▶│                    │                   │
   │                 │                    │ callFunction       │                   │
   │                 │                    │───────────────────▶│                   │
   │                 │                    │                    │ invoke(fnId,args) │
   │                 │                    │                    │──────────────────▶│
   │                 │                    │                    │                   │
   │                 │                    │                    │   [new ops]       │
   │                 │                    │◀───────────────────│◀──────────────────│
   │                 │◀───────────────────│                    │                   │
   │◀────────────────│   Re-render        │                    │                   │
```

---

## 5. Communication Protocol

### 5.1 Operations (Guest → Host)

```typescript
type OperationType =
  | 'CREATE'      // Create node
  | 'UPDATE'      // Update properties
  | 'DELETE'      // Delete node
  | 'APPEND'      // Append child node
  | 'INSERT'      // Insert child at position
  | 'REMOVE'      // Remove child node
  | 'REORDER'     // Reorder children
  | 'TEXT'        // Update text content
  | 'REF_CALL';   // Remote method call (Remote Ref)

interface CreateOperation {
  op: 'CREATE';
  id: number;
  type: string;           // Component type: 'View', 'Text', etc.
  props: SerializedProps;
}

interface UpdateOperation {
  op: 'UPDATE';
  id: number;
  props: SerializedProps;
  removedProps?: string[];
}

// ... other operation types
```

### 5.2 Property Serialization

```typescript
type SerializedValue =
  | null
  | boolean
  | number
  | string
  | SerializedFunction
  | SerializedValue[]
  | { [key: string]: SerializedValue };

interface SerializedFunction {
  __type: 'function';
  __fnId: string;       // Function ID: 'fn_xxxxx'
}
```

### 5.3 Host Messages (Host → Guest)

```typescript
type HostMessageType =
  | 'CALL_FUNCTION'      // Call callback function
  | 'HOST_EVENT'         // Host event broadcast
  | 'CONFIG_UPDATE'      // Configuration update
  | 'DESTROY'            // Destroy signal
  | 'REF_METHOD_RESULT'; // Remote Ref method call result

interface CallFunctionMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: SerializedValue[];
}

interface HostEventMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: SerializedValue;
}
```

---

## 6. Core Modules

### 6.1 Engine

```typescript
import { Engine } from 'rill';

const engine = new Engine({
  timeout: 5000,        // Execution timeout (ms)
  debug: false,         // Debug mode
  provider: myProvider, // Custom JSEngineProvider
});

// Register components
engine.register({ StepList: NativeStepList });

// Load guest bundle
await engine.loadBundle(bundleCode, initialProps);

// Send events to guest
engine.sendEvent('REFRESH', { timestamp: Date.now() });

// Listen to guest messages
engine.on('message', (msg) => console.log(msg));

// Health check
const health = engine.getHealth();

// Destroy
engine.destroy();
```

### 6.2 Guest SDK

```typescript
// src/sdk/index.ts - Guest SDK exports
import { View, Text, TouchableOpacity, useHostEvent, useConfig, useSendToHost } from 'rill/sdk';

export default function MyGuest() {
  const config = useConfig<{ theme: string }>();
  const send = useSendToHost();

  useHostEvent('REFRESH', (payload) => {
    console.log('Refreshing...', payload);
  });

  return (
    <View>
      <Text>Theme: {config.theme}</Text>
      <TouchableOpacity onPress={() => send('ACTION', { type: 'click' })}>
        <Text>Press Me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### 6.3 Receiver

The Receiver parses operations from the sandbox and builds the React Native component tree:

```typescript
class Receiver {
  private nodeMap = new Map<number, NodeInstance>();
  private rootChildren: number[] = [];
  private refMap = new Map<number, React.RefObject<unknown>>();  // Remote Ref support

  applyBatch(batch: OperationBatch): ReceiverApplyStats {
    for (const op of batch.operations) {
      this.operationHandlers[op.op](op);
    }
    this.scheduleUpdate();
    return { applied, skipped, failed, total };
  }

  render(): React.ReactElement | string | null {
    // Render the root node tree
  }
}
```

---

## 7. CLI Design

### 7.1 Build Configuration

CLI uses **Bun.build** for guest bundle compilation:

```typescript
// src/cli/build.ts
export async function build(options: BuildOptions): Promise<void> {
	  const result = await Bun.build({
	    entrypoints: [entryPath],
	    target: 'browser',
	    format: 'cjs',
	    minify,
	    external: ['react', 'react/jsx-runtime', 'react-native', 'rill/sdk'],
	    define: {
	      'process.env.NODE_ENV': '"production"',
	      __DEV__: 'false',
	    },
	  });

  // Post-process: wrap with runtime inject
  // Strict dependency guard check
  // Syntax validation
}
```

### 7.2 CLI Commands

```bash
# Build bundle
bunx rill build src/guest.tsx -o dist/bundle.js

# Development mode
bunx rill build src/guest.tsx --watch --no-minify --sourcemap

# Analyze bundle
bunx rill analyze dist/bundle.js
```

---

## 8. Security Design

### 8.1 Sandbox Isolation

1. **JSEngineProvider Isolation**: Independent JS execution context
   - Supports multiple modes: `VM` (Node.js/Bun), `JSC` (Apple platforms), `QuickJS` (RN all platforms), `WasmQuickJS` (Web)
2. **Whitelist Components**: Only registered component types can render
3. **Require Whitelist**: Only allowed modules can be required
4. **API Restrictions**: No dangerous APIs (fetch, XMLHttpRequest, etc.)

### 8.2 Exception Handling

```typescript
// Sandbox exceptions don't affect host
engine.on('error', (error: Error) => {
  console.error('[Guest Error]', error.message);
});

engine.on('fatalError', (error: Error) => {
  console.error('[Guest Fatal]', error.message);
  // Sandbox corrupted, need to reload
});
```

### 8.3 Resource Limits

1. **Execution Timeout**: `timeout` option (default 5000ms)
2. **Memory Limit**: JSEngineProvider heap memory cap
3. **Batch Size Limit**: `receiverMaxBatchSize` (default 5000)

---

## 9. Performance Optimization

### 9.1 Batch Updates

- Reconciler sends operations uniformly during commit phase
- Use `queueMicrotask` to schedule updates, ensuring batch operations complete before triggering render

### 9.2 Incremental Updates

- Only send changed properties (diff)
- Use `removedProps` to mark deleted properties

### 9.3 List Optimization

- FlatList virtualization support
- Reuse created node instances

### 9.4 Remote Ref

Support for Guest code to call Host component instance methods (e.g., `focus()`, `scrollTo()`):

```typescript
// Guest side
const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();

const handleFocus = async () => {
  await remoteInput?.invoke('focus');
};

return <TextInput ref={inputRef} />;
```

Communication flow:
1. Guest sends `REF_CALL` operation (with refId, method, args, callId)
2. Host Receiver finds component instance via refMap, calls method
3. Host returns `REF_METHOD_RESULT` message (with result or error)
4. Guest Promise resolves/rejects

---

## 10. DevTools Support

### 10.1 Development Mode

```typescript
if (__DEV__) {
  engine.on('operation', (op) => {
    console.log('[Rill Op]', op);
  });
}
```

### 10.2 DevTools Features

- Visual component tree inspector
- Operation log viewer
- Performance panel
- Diagnostics collector
