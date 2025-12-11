# Best Practices

## Preventing Memory Leaks

### Problem: Forgetting to Clean Up Listeners

```tsx
// ❌ Wrong: Forgot to clean up listener
useEffect(() => {
  EventEmitter.on('data:update', handleUpdate);
  // Listener persists after component unmounts → Memory leak
}, []);
```

### ✅ Solution 1: Use the Returned Cleanup Function

```tsx
// ✅ Correct: Use cleanup function
useEffect(() => {
  const cleanup = EventEmitter.on('data:update', handleUpdate);
  return cleanup; // React automatically calls this on unmount
}, []);
```

### ✅ Solution 2: Shorthand Form

```tsx
// ✅ More concise
useEffect(() => {
  return EventEmitter.on('data:update', handleUpdate);
}, []);
```

### Automatic Memory Leak Detection

askit automatically detects potential memory leaks:

```tsx
// If more than 10 listeners for the same event, you'll get a warning
useEffect(() => {
  return EventEmitter.on('scroll:position', handleScroll);
}, []); // If this component is created 10+ times, you'll see a warning
```

**Warning Example:**
```
[askit/EventEmitter] Possible memory leak detected: 11 listeners for "scroll:position"
Context: {
  event: "scroll:position",
  count: 11,
  maxListeners: 10,
  suggestion: "Use the cleanup function returned by on() in React useEffect"
}
```

### Adjusting Listener Limits

```tsx
import { EventEmitter } from 'askit';

// Some scenarios may need more listeners (e.g., list rendering)
(EventEmitter as any).setMaxListeners(20);

// Or disable warnings (not recommended)
(EventEmitter as any).setMaxListeners(0); // 0 = unlimited
```

## Performance Optimization: Throttle and Debounce

### High-Frequency Events Cause Performance Issues

```tsx
// ❌ Problem: Scroll events trigger too frequently
useEffect(() => {
  return EventEmitter.on('scroll:position', (position) => {
    // This function may be called 100+ times per second
    updateUI(position);
  });
}, []);
```

### ✅ Solution: Use Throttle

```tsx
// ✅ Throttle: Execute at most once per 100ms
useEffect(() => {
  return EventEmitter.on(
    'scroll:position',
    (position) => {
      updateUI(position);
    },
    { rateLimit: 'throttle', delay: 100 } // Limit frequency
  );
}, []);
```

### ✅ Solution: Use Debounce

```tsx
// ✅ Debounce: Execute only after user stops typing for 200ms
useEffect(() => {
  return EventEmitter.on(
    'input:change',
    (value) => {
      searchAPI(value);
    },
    { rateLimit: 'debounce', delay: 200 } // Delay execution
  );
}, []);
```

### Throttle vs Debounce

| Scenario | Use | Reason |
|----------|-----|--------|
| Scroll position updates | `throttle` (100ms) | Need continuous updates, but not too frequent |
| Window resize | `throttle` (150ms) | Need to track resizing, but limit frequency |
| Search input | `debounce` (200-300ms) | Wait for user to stop typing before searching |
| Auto-save | `debounce` (1000ms) | Wait for user to stop editing before saving |
| Button clicks | `throttle` (300ms) | Prevent duplicate clicks |

### Recommended Delay Values

- **Don't set too high**: 100-300ms is a reasonable range to maintain responsiveness
- **Adjust by scenario**:
  - Scroll/resize: 50-150ms
  - Input search: 200-300ms
  - Auto-save: 500-1000ms

```tsx
// ✅ Reasonable delay values
{ rateLimit: 'throttle', delay: 100 }  // Scroll
{ rateLimit: 'debounce', delay: 250 }  // Search

// ⚠️ Too high delays feel sluggish
{ rateLimit: 'throttle', delay: 1000 } // Too slow!
```

## Combining Best Practices

```tsx
// Combine all best practices
useEffect(() => {
  // Auto cleanup + throttle limiting
  return EventEmitter.on(
    'analytics:*', // Wildcard matches all analytics events
    (data) => {
      sendToAnalytics(data);
    },
    { rateLimit: 'throttle', delay: 100 } // Avoid sending too frequently
  );
}, []);
```

## Debugging Tips

### 1. Check Current Listener Count

```tsx
// Check for leaks during development
console.log('Max listeners:', (EventEmitter as any).getMaxListeners());
// Output: Max listeners: 10
```

### 2. Temporarily Disable Throttle for Debugging

```tsx
// Temporarily disable throttle to observe behavior
return EventEmitter.on(
  'debug:event',
  handler,
  { rateLimit: 'none' } // No limiting during debug
);
```

### 3. Use console for Debugging

```tsx
// View event flow during development
EventEmitter.on('*', (payload) => {
  console.log('[Debug] Event received:', payload);
});
```

## Summary

✅ **Must Do:**
1. Always use the cleanup function returned by `on()`
2. Return cleanup function in React useEffect
3. Use throttle/debounce for high-frequency events

⚠️ **Caution:**
1. Warning when more than 10 listeners per event
2. Don't set delays too high (100-300ms recommended)

🚫 **Don't:**
1. Forget to clean up listeners
2. Leave high-frequency events unlimited
3. Ignore memory leak warnings
