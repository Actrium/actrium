# Bridge Layer Architecture

The Bridge layer provides unified serialization and communication between Guest (sandbox) and Host (native) environments.

## Directory Structure

```
src/runtime/bridge/
├── types.ts          # Authoritative type definitions
├── serializer.ts     # Shared serialization utilities
├── TypeRules.ts      # Type validation rules
├── PromiseManager.ts # Promise lifecycle management
├── Bridge.ts         # Core communication layer
└── index.ts          # Public exports
```

## Type Flow

```
bridge/types.ts (authoritative source)
       ↓
runtime/types.ts (re-export)
       ↓
let/types.ts (re-export)
       ↓
reconciler/index.ts (consumer)
```

## Key Components

### serializer.ts

Shared utilities for cross-boundary data transfer:

- `serializeValue()` - Serialize any value for JSI transfer
- `serializeObject()` - Serialize object properties
- `deserializeValue()` - Restore serialized values
- `createSerializedFunction()` - Create function references
- `isSerializedFunction()` - Type guard for serialized functions

### types.ts

Core type definitions:

- `SerializedFunction` - `{ __type: 'function', __fnId: string }`
- `SerializedValue` - Union of all serializable types
- `BridgeValue` - Runtime values with callable proxies
- `CallbackRegistry` - Interface for function registration/invocation

### Bridge.ts

Communication layer managing:

- Callback registration and invocation
- Promise lifecycle (PromiseManager)
- Bidirectional data flow

## Data Flow

### Guest → Host (Rendering)

```
JSX → Reconciler.createInstance()
    → serializeObject(props)
    → CREATE operation
    → Bridge.sendToHost()
    → Host Receiver
```

### Host → Guest (Callbacks)

```
User Event → Host Component
          → Receiver.handleCallback(fnId, args)
          → Bridge.callGuestFunction()
          → Guest callback execution
```

### Remote Ref (useRemoteRef)

Bidirectional flow for calling host component methods from guest:

```
Guest (invoke)              Host (Receiver)
     │                           │
     │  REF_CALL {               │
     │    refId, method,         │
     │    args, callId           │
     │  }                        │
     │  ───────────────────────> │
     │                           │ refMap.get(refId).current[method]()
     │                           │
     │  REF_METHOD_RESULT {      │
     │    refId, callId,         │
     │    result | error         │
     │  }                        │
     │  <─────────────────────── │
     │                           │
     │  Promise.resolve(result)  │
```

**Operations:**
- `REF_CALL` - Guest sends method invocation request
- `REF_METHOD_RESULT` - Host returns result/error

**Types:**
- `RefCallOperation` - Operation with refId, method, args, callId
- `RefMethodResultMessage` - Message with result or SerializedError

## Design Principles

1. **Single Source of Truth**: All serialization logic in `bridge/`
2. **Type Safety**: Shared type guards prevent format mismatches
3. **Symmetric Design**: Same utilities work on both sides
4. **JSI Optimized**: Minimal overhead for React Native bridge
