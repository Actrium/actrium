# Architecture

## Overview

**askit** is a UI component library and API layer built on top of [rill](https://github.com/GoAskAway/rill). Through package.json conditional exports, the same `import { StepList, Toast } from 'askit'` delivers real React Native components on the Host side and string identifiers on the Guest side (which rill passes to Host for rendering).

### Relationship with rill

| Layer | Role |
|-------|------|
| **rill** | Sandbox-isolated dynamic UI rendering engine — runs React code in isolated JS sandbox (QuickJS/JSC), serializes render operations into instructions, sends them to Host for real React Native rendering |
| **askit** | UI components and API layer on top of rill — provides business components (StepList, ChatBubble, etc.) and cross-boundary APIs (EventEmitter, Toast, Haptic) |

### Core Mechanism

- **UI Components**: Guest side is string `"StepList"`, Host side is complete RN implementation
- **APIs**: Guest sends commands via `global.__sendEventToHost('ASKIT_TOAST_SHOW', {...})`, Host bridge routes to real native APIs like ToastAndroid

## Design Principles

### 1. Same API, Different Implementations

Developers write code using the same API surface regardless of the execution environment:

```typescript
// This code works in both Host and Guest
import { EventEmitter, Toast } from 'askit';

EventEmitter.emit('action', { type: 'click' });
Toast.show('Hello!');
```

### 2. Conditional Exports

The `package.json` uses conditional exports to serve different TypeScript sources:

```json
{
  "exports": {
    ".": {
      "react-native": "./src/index.host.ts",
      "default": "./src/index.guest.ts"
    }
  }
}
```

- **react-native**: Host implementations with real React Native components
- **default**: Guest implementations with message passing

### 3. 90% Universal + 10% Core

| Category | Location | Purpose |
|----------|----------|---------|
| Universal (90%) | `askit` | APIs and components usable in both environments |
| Core (10%) | `askit/core` | Host-only bridging and registration |

## Message Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                        HOST APP                                   │
│                                                                   │
│  ┌────────────────┐    ┌──────────────────┐    ┌──────────────┐ │
│  │ EventEmitter   │    │  createEngine    │    │    Rill      │ │
│  │ (host)         │◄───│  Adapter         │◄───│   Engine     │ │
│  │ - emit()       │    │  - _setBroadcast │    │              │ │
│  │ - on()/off()   │    │  - _notifyListen │    │              │ │
│  └────────────────┘    └──────────────────┘    └──────┬───────┘ │
│         │                                               │         │
│         ▼                                               │         │
│  ┌────────────────┐                                    │         │
│  │ React Native   │                                    │         │
│  │  Components    │                                    │         │
│  │ Toast, Haptic  │                                    │         │
│  └────────────────┘                                    │         │
└────────────────────────────────────────────────────────┼─────────┘
                                                         │
                            Message Protocol (rill HostEvent model)

- Guest 0 Host: `event: string`, `payload: unknown`
  - `ASKIT_*` are **reserved internal commands** used by askit modules.
  - All other events are treated as **app-level events** and are forwarded to Host `EventEmitter`.
- Host 0 Guest: Host `EventEmitter.emit(event, payload)` is forwarded to `engine.sendEvent(event, payload)`.
                                                         │
┌────────────────────────────────────────────────────────┼─────────┐
│                        GUEST (QuickJS Sandbox)         │         │
│                                                        ▼         │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    askit (guest)                          │  │
│  │                                                           │  │
│  │  EventEmitter.emit('event')                              │  │
│  │    └──► global.__sendEventToHost('event', payload)  │  │
│  │                                                           │  │
│  │  Toast.show(msg, opts)                                   │  │
│  │    └──► global.__sendEventToHost('ASKIT_TOAST_SHOW', { message: msg, options }) │  │
│  │                                                           │  │
│  │  Haptic.trigger(type)                                    │  │
│  │    └──► global.__sendEventToHost('ASKIT_HAPTIC_TRIGGER', { type })│  │
│  │                                                           │  │
│  │  Component() ──► Returns DSL object                      │  │
│  └──────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

## Module Responsibilities

### Host Implementations (`*.host.ts`)

Execute directly in the Host App:

```typescript
// Toast.host.ts
import { ToastAndroid, Platform } from 'react-native';

export const Toast = {
  show(message: string, options?: ToastOptions) {
    if (Platform.OS === 'android') {
      ToastAndroid.show(message, ToastAndroid.SHORT);
    }
    // iOS: use custom implementation
  }
};
```

### Guest Implementations (`*.guest.ts`)

Send messages to Host for execution:

```typescript
// Toast.guest.ts
class RemoteToast implements ToastAPI {
  show(message: string, options?: ToastOptions): void {
    if (typeof global.sendToHost === 'function') {
      global.__sendEventToHost('ASKIT_TOAST_SHOW', { message, options });
    }
  }
}

export const Toast: ToastAPI = new RemoteToast();
```

### Bridge (`core/bridge.ts`)

Routes messages from Guest to appropriate handlers:

```typescript
import { NOTIFY_SYMBOL } from '../api/EventEmitter.host';

function handleGuestMessage(message: GuestMessage) {
  // Reserved internal commands
  if (message.event === 'ASKIT_TOAST_SHOW') {
    const { message: text, options } = message.payload as any;
    return modules.toast.show(text, options);
  }

  if (message.event === 'ASKIT_HAPTIC_TRIGGER') {
    const { type } = message.payload as any;
    return modules.haptic.trigger(type);
  }

  // App-level event: forward to EventEmitter
  (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL](message.event, message.payload);
}
```

### Engine Adapter (`core/bridge.ts`)

Connects Rill Engine with askit EventEmitter:

```typescript
import { BROADCASTER_SYMBOL } from '../api/EventEmitter.host';

function createEngineAdapter(engine: EngineInterface) {
  // Forward Host EventEmitter events to Guest
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event, payload) => {
    engine.sendEvent(event, payload);
  });

  // Listen for guest messages and route them
  const unsubscribeMessage = engine.on('message', (message) => {
    handleGuestMessage({
      event: message.event,
      payload: message.payload
    });
  });

  return {
    dispose() {
      (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL](null);
      unsubscribeMessage();
    },
  };
}
```

### Registry (`core/registry.ts`)

Manages component and module registration:

```typescript
export const components = {
  PanelMarker,
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble
} as const;

export const modules = {
  toast: Toast,    // Host Toast implementation
  haptic: Haptic   // Host Haptic implementation
};
```

## Package Structure

askit exports TypeScript source directly (no build step required):

```
askit/
├── src/
│   ├── index.host.ts      # Host (React Native) entry
│   ├── index.guest.ts     # Guest (QuickJS) entry
│   ├── core/
│   │   └── index.ts       # Host-only core module
│   ├── api/               # API implementations
│   │   ├── *.host.ts      # Host implementations
│   │   └── *.guest.ts     # Guest implementations
│   └── ui/                # UI components
│       ├── */
│       │   ├── *.host.tsx  # Native components
│       │   └── *.guest.tsx # DSL generators
└── package.json           # Exports directly from src/
```

Your bundler (Metro, Vite, etc.) handles TypeScript compilation automatically.

## Testing Strategy

| Layer | Environment | Tools |
|-------|-------------|-------|
| API Logic | Node.js | Vitest |
| Android-specific | Mock injection | Vitest + DI |
| UI Components | React Native | E2E / Integration |
| Full Integration | Device/Simulator | Detox |

```typescript
// Example: Testing Android-specific code with dependency injection
import { _injectMocks, NativeToast } from './Toast.native';

beforeEach(() => {
  _injectMocks(
    { OS: 'android' },
    { showWithGravity: vi.fn(), SHORT: 0, BOTTOM: 80 }
  );
});

it('should call ToastAndroid on Android', () => {
  const toast = new NativeToast();
  toast.show('Hello');
  expect(mockShowWithGravity).toHaveBeenCalled();
});
```
