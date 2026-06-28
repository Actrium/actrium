# API Reference

## EventEmitter

The EventEmitter API provides event-based communication between Host and Guest.

### Features

- **Wildcard Pattern Matching**: Subscribe to multiple events using `*` (single-level) or `**` (multi-level) patterns
- **Rate Limiting**: Built-in throttle and debounce support for high-frequency events
- **Type Safety**: Extend `AskitEvents` interface for typed event payloads
- **Memory Leak Detection**: Automatic warnings when listener count exceeds threshold

### Methods

#### `EventEmitter.emit(event, payload?)`

Emit an event with optional payload.

```typescript
// Basic usage
EventEmitter.emit('user:action', { type: 'click', target: 'button' });

// Type-safe usage (requires declaring AskitEvents)
declare module 'askit' {
  interface AskitEvents {
    'user:login': { userId: string; timestamp: number };
  }
}

EventEmitter.emit('user:login', { userId: '123', timestamp: Date.now() });
```

**Parameters:**
- `event: string | keyof AskitEvents` - Event name
- `payload?: T | AskitEvents[K]` - Optional event data

#### `EventEmitter.on(event, handler, options?)`

Subscribe to an event. Supports wildcard patterns and rate limiting. Returns an unsubscribe function.

```typescript
// Exact match
const unsubscribe = EventEmitter.on('data:update', (data) => {
  console.log('Data updated:', data);
});

// Wildcard patterns
EventEmitter.on('user:*', (payload) => {
  // Matches user:login, user:logout, etc.
});

EventEmitter.on('analytics:**', (payload) => {
  // Matches analytics:click, analytics:page:view, etc.
});

// With rate limiting (throttle)
EventEmitter.on('scroll:position', handleScroll, {
  rateLimit: 'throttle',
  delay: 100  // Execute at most once per 100ms
});

// With rate limiting (debounce)
EventEmitter.on('input:change', handleInput, {
  rateLimit: 'debounce',
  delay: 200  // Execute only after 200ms of no events
});

// Later: unsubscribe
unsubscribe();
```

**Parameters:**
- `event: string | keyof AskitEvents` - Event name or pattern (supports `*` and `**`)
- `handler: (payload: T) => void` - Callback function
- `options?: EventListenerOptions` - Optional rate limiting configuration

**EventListenerOptions:**
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `rateLimit` | `'throttle' \| 'debounce' \| 'none'` | `'none'` | Rate limiting type |
| `delay` | `number` | `100` | Delay in milliseconds for rate limiting |

**Returns:** `() => void` - Unsubscribe function

#### `EventEmitter.once(event, handler)`

Subscribe to an event for one-time execution. Supports wildcard patterns.

```typescript
EventEmitter.once('init:complete', () => {
  console.log('Initialization complete');
});

// With wildcard
EventEmitter.once('error:*', (error) => {
  console.error('First error:', error);
});
```

#### `EventEmitter.off(event, handler)`

Remove a specific event listener.

```typescript
const handler = (data) => console.log(data);
EventEmitter.on('data:update', handler);

// Later: remove this specific handler
EventEmitter.off('data:update', handler);
```

#### `EventEmitter.setMaxListeners(n)`

Set the maximum number of listeners per event (0 = unlimited). Default is 10.

```typescript
// Allow up to 20 listeners per event
EventEmitter.setMaxListeners(20);

// Disable warnings
EventEmitter.setMaxListeners(0);
```

#### `EventEmitter.getMaxListeners()`

Get the current maximum listeners limit.

```typescript
const limit = EventEmitter.getMaxListeners();  // Returns 10 by default
```

---

## Toast

Display toast notifications to the user.

### Methods

#### `Toast.show(message, options?)`

Show a toast message.

```typescript
// Simple toast
Toast.show('Hello World!');

// With options
Toast.show('Operation successful', {
  duration: 'long',
  position: 'top'
});

// Custom duration (milliseconds)
Toast.show('Processing...', {
  duration: 5000
});
```

**Parameters:**
- `message: string` - Text to display
- `options?: ToastOptions` - Configuration options

**ToastOptions:**
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `duration` | `'short' \| 'long' \| number` | `'short'` | Display duration (short=2s, long=3.5s) |
| `position` | `'top' \| 'center' \| 'bottom'` | `'bottom'` | Toast position |

---

## Haptic

Trigger haptic feedback on supported devices.

### Methods

#### `Haptic.trigger(type?)`

Trigger haptic feedback.

```typescript
// Light feedback (default)
Haptic.trigger();
Haptic.trigger('light');

// Medium feedback
Haptic.trigger('medium');

// Heavy feedback
Haptic.trigger('heavy');

// Success feedback
Haptic.trigger('success');

// Warning feedback
Haptic.trigger('warning');

// Error feedback
Haptic.trigger('error');
```

**Parameters:**
- `type?: HapticType` - Feedback intensity/type

**HapticType:**
| Value | Description |
|-------|-------------|
| `'light'` | Subtle feedback |
| `'medium'` | Standard feedback |
| `'heavy'` | Strong feedback |
| `'selection'` | Selection feedback |
| `'success'` | Success notification pattern |
| `'warning'` | Warning notification pattern |
| `'error'` | Error notification pattern |

---

## Core Module (Host Only)

The `askit/core` module is only available in the Host App.

### createEngineAdapter(engine)

Create a bridge adapter for the rill engine.

```typescript
import { createEngineAdapter } from 'askit/core';
import { Engine } from 'rill';

const engine = new Engine();
const adapter = createEngineAdapter(engine);
```

### components

Map of all registered UI components.

```typescript
import { components } from 'askit/core';

// Register with engine
engine.register(components);
```

### modules

Map of all registered API modules (toast, haptic handlers).

### configureToast(handler)

Override the default toast implementation.

```typescript
import { configureToast } from 'askit/core';

configureToast((message, options) => {
  // Custom toast implementation
  MyCustomToast.show(message, options);
});
```

### configureHaptic(handler)

Override the default haptic implementation.

```typescript
import { configureHaptic } from 'askit/core';

configureHaptic((type) => {
  // Custom haptic implementation
  MyHapticLibrary.trigger(type);
});
```
