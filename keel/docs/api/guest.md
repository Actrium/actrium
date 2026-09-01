# Guest API Reference

The Guest module provides components, hooks, and platform APIs for building dynamic UI inside the Keel sandbox. Guest code uses standard React patterns (JSX, hooks) and communicates with the host through a well-defined protocol.

Import path: `keel/guest`

---

## Components

All components listed below are available inside the sandbox via `require('keel/guest')` or JSX. They map to host-side React Native component implementations registered through `ComponentRegistry`.

### View

A container component. Maps to React Native's `View`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `style` | `ViewStyle` | `undefined` | Style object for layout and appearance. |
| `testID` | `string` | `undefined` | Test identifier for automation. |
| `onLayout` | `(event: LayoutEvent) => void` | `undefined` | Called when the view's layout changes. |

### Text

A text display component. Maps to React Native's `Text`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `style` | `TextStyle` | `undefined` | Style object for text appearance. |
| `numberOfLines` | `number` | `undefined` | Maximum number of lines before truncation. |
| `ellipsizeMode` | `'head' \| 'middle' \| 'tail' \| 'clip'` | `'tail'` | How text is truncated when it exceeds `numberOfLines`. |
| `selectable` | `boolean` | `false` | Whether the text is selectable by the user. |
| `onPress` | `() => void` | `undefined` | Called when the text is pressed. |

### Image

An image display component. Maps to React Native's `Image`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `source` | `{ uri: string } \| number` | (required) | Image source. A URI object for remote images or a number for bundled assets. |
| `style` | `ImageStyle` | `undefined` | Style object for image appearance. |
| `resizeMode` | `'cover' \| 'contain' \| 'stretch' \| 'repeat' \| 'center'` | `'cover'` | How the image is resized to fit its container. |
| `onLoad` | `() => void` | `undefined` | Called when the image finishes loading. |
| `onError` | `(error: { nativeEvent: { error: string } }) => void` | `undefined` | Called when the image fails to load. |

### TouchableOpacity

A touchable wrapper with opacity feedback. Maps to React Native's `TouchableOpacity`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `onPress` | `() => void` | `undefined` | Called when the component is pressed. |
| `onLongPress` | `() => void` | `undefined` | Called when the component is long-pressed. |
| `activeOpacity` | `number` | `0.2` | Opacity applied when the touch is active. |
| `disabled` | `boolean` | `false` | Whether the touch is disabled. |

### ScrollView

A scrollable container. Maps to React Native's `ScrollView`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | Whether the scroll view scrolls horizontally. |
| `showsVerticalScrollIndicator` | `boolean` | `true` | Whether the vertical scroll indicator is visible. |
| `showsHorizontalScrollIndicator` | `boolean` | `true` | Whether the horizontal scroll indicator is visible. |
| `onScroll` | `(event: ScrollEvent) => void` | `undefined` | Called when the scroll position changes. |

### FlatList

A performant scrollable list. Maps to React Native's `FlatList`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `data` | `T[]` | (required) | The array of items to render. |
| `renderItem` | `(info: { item: T, index: number }) => ReactElement` | (required) | Function that renders each item. |
| `keyExtractor` | `(item: T, index: number) => string` | `undefined` | Function that extracts a unique key for each item. |
| `horizontal` | `boolean` | `false` | Whether the list scrolls horizontally. |
| `onEndReached` | `() => void` | `undefined` | Called when the end of the list is reached. |
| `onEndReachedThreshold` | `number` | `undefined` | How far from the end (in visible length units) to trigger `onEndReached`. |
| `ListHeaderComponent` | `ReactElement \| (() => ReactElement)` | `undefined` | Rendered at the top of the list. |
| `ListEmptyComponent` | `ReactElement \| (() => ReactElement)` | `undefined` | Rendered when `data` is empty. |

### TextInput

A text input field. Maps to React Native's `TextInput`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `string` | `undefined` | The current text value (controlled). |
| `onChangeText` | `(text: string) => void` | `undefined` | Called when the text changes. |
| `placeholder` | `string` | `undefined` | Placeholder text displayed when empty. |
| `secureTextEntry` | `boolean` | `false` | Whether to obscure text (password field). |
| `multiline` | `boolean` | `false` | Whether the input supports multiple lines. |
| `maxLength` | `number` | `undefined` | Maximum number of characters allowed. |

### Button

A basic button. Maps to React Native's `Button`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `title` | `string` | (required) | The button label text. |
| `onPress` | `() => void` | (required) | Called when the button is pressed. |
| `disabled` | `boolean` | `false` | Whether the button is disabled. |
| `color` | `string` | `undefined` | Button color (platform-specific behavior). |

### Switch

A toggle switch. Maps to React Native's `Switch`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `boolean` | `false` | The current on/off state. |
| `onValueChange` | `(value: boolean) => void` | `undefined` | Called when the switch is toggled. |
| `disabled` | `boolean` | `false` | Whether the switch is disabled. |

### ActivityIndicator

A loading spinner. Maps to React Native's `ActivityIndicator`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `size` | `'small' \| 'large'` | `'small'` | The size of the indicator. |
| `color` | `string` | `undefined` | The color of the indicator. |
| `animating` | `boolean` | `true` | Whether the indicator is animating. |

---

## Hooks

### useConfig\<T\>()

Returns the current Guest configuration. The configuration is initially set via `loadBundle(source, initialProps)` and can be updated from the host via `engine.updateConfig()`.

```typescript
function useConfig<T = Record<string, unknown>>(): T
```

**Example:**

```tsx
const config = useConfig<{ theme: string; userName: string }>();
return <Text>{config.userName}</Text>;
```

### useHostEvent\<T\>(eventName, callback)

Subscribe to events sent from the host via `engine.sendEvent()`. The callback is invoked whenever the host sends an event with the matching name.

```typescript
function useHostEvent<T = unknown>(
  eventName: string,
  callback: (payload: T) => void
): void
```

**Example:**

```tsx
useHostEvent<{ items: string[] }>('dataUpdate', (payload) => {
  setItems(payload.items);
});
```

### useSendToHost()

Returns a function to send events from the Guest to the host. The host receives these events via `engine.on('message', handler)`.

```typescript
function useSendToHost(): (eventName: string, payload?: unknown) => void
```

**Example:**

```tsx
const sendToHost = useSendToHost();

const handlePress = () => {
  sendToHost('buttonPressed', { id: 'submit' });
};
```

### useRemoteRef\<T\>()

Creates a remote reference to a host-side component instance, allowing the Guest to invoke methods on it (e.g., `focus()`, `scrollTo()`).

```typescript
function useRemoteRef<T>(): [RefCallback, RemoteRef<T> | null]
```

Returns a tuple:
1. A ref callback to attach to the target component.
2. A `RemoteRef` object (or `null` before the ref is attached).

#### RemoteRef Interface

| Property / Method | Type | Description |
|---|---|---|
| `nodeId` | `number` | The node ID of the referenced component. |
| `invoke(method, ...args)` | `(...args: unknown[]) => Promise<unknown>` | Invoke a method on the host component instance. Returns a Promise with the result. |
| `call` | `Proxy` | A proxy object for direct method calls (syntactic sugar). |

#### Predefined Ref Types

**TextInputRef:**

| Method | Signature | Description |
|---|---|---|
| `focus()` | `() => Promise<void>` | Focus the text input. |
| `blur()` | `() => Promise<void>` | Blur the text input. |
| `clear()` | `() => Promise<void>` | Clear the text input value. |

**ScrollViewRef:**

| Method | Signature | Description |
|---|---|---|
| `scrollTo(options)` | `(options: { x?: number, y?: number, animated?: boolean }) => Promise<void>` | Scroll to a specific position. |
| `scrollToEnd(options?)` | `(options?: { animated?: boolean }) => Promise<void>` | Scroll to the end of the content. |

**FlatListRef:**

| Method | Signature | Description |
|---|---|---|
| `scrollToIndex(params)` | `(params: { index: number, animated?: boolean }) => Promise<void>` | Scroll to a specific item index. |
| `scrollToOffset(params)` | `(params: { offset: number, animated?: boolean }) => Promise<void>` | Scroll to a specific pixel offset. |

**Example:**

```tsx
const [ref, remoteRef] = useRemoteRef<TextInputRef>();

const handleFocus = async () => {
  if (remoteRef) {
    await remoteRef.call.focus();
  }
};

return <TextInput ref={ref} placeholder="Enter text" />;
```

---

## KeelErrorBoundary

A React error boundary component for catching rendering errors in Guest code. Prevents a single component error from crashing the entire sandbox UI.

```typescript
interface KeelErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactElement | ((error: Error) => ReactElement);
  onError?: (error: Error, errorInfo: { componentStack: string }) => void;
}
```

| Prop | Type | Description |
|---|---|---|
| `children` | `ReactNode` | The component tree to wrap. |
| `fallback` | `ReactElement \| (error: Error) => ReactElement` | Rendered when an error is caught. |
| `onError` | `(error: Error, errorInfo: object) => void` | Called when an error is caught. |

---

## Platform APIs

These APIs provide access to platform information and utilities inside the sandbox.

### Platform

Provides information about the current platform.

```typescript
const Platform: {
  OS: 'ios' | 'android' | 'web' | 'macos' | 'windows';
  Version: number | string;
  select: <T>(specifics: { ios?: T; android?: T; default?: T }) => T;
}
```

### Dimensions

Provides screen and window dimensions.

```typescript
const Dimensions: {
  get: (dim: 'window' | 'screen') => { width: number; height: number };
}
```

### StyleSheet

A utility for creating optimized style objects.

```typescript
const StyleSheet: {
  create: <T extends Record<string, ViewStyle | TextStyle | ImageStyle>>(styles: T) => T;
  flatten: (style: StyleProp) => object;
  absoluteFill: ViewStyle;
  hairlineWidth: number;
}
```

### Linking

Provides URL opening capabilities.

```typescript
const Linking: {
  openURL: (url: string) => Promise<void>;
  canOpenURL: (url: string) => Promise<boolean>;
}
```

---

## Sandbox-safe utilities

A guest sandbox provides **only core ECMAScript**. Web platform globals are not
part of that, and the ones you would reach for to handle bytes are missing.
Measured from inside a keel guest running on QuickJS-WASM, reporting
`typeof globalThis[name]`:

| absent | present |
|---|---|
| `TextEncoder`, `TextDecoder`, `performance`, `btoa`, `atob`, `Buffer`, `crypto` | `Uint8Array`, `console`, `setTimeout` |

This is a category, not a portability caveat: every encoding and byte-formatting
global is simply not there, in **both** directions.

> **TypeScript will not catch this.** Any guest `tsconfig` whose `lib` includes
> `DOM` declares `TextEncoder`, `TextDecoder`, `performance` and friends, so
> `new TextEncoder()` type-checks cleanly and then throws inside the sandbox. And
> a guest `tsconfig` tends to end up with `DOM` for unrelated reasons. The failure
> is environment-dependent, so it survives local testing in a browser-hosted dev
> loop and shows up on the device.

`keel/guest` exports the helpers keel's own guest bundle uses, so you do not have
to re-implement them. Each checks for the real global on **every call** — a host
that injects globals late is still picked up — and falls back to pure JS.

```typescript
import {
  createUtf8Encoder,
  createUtf8Decoder,
  encodeBase64,
  decodeBase64,
  nowMs,
} from 'keel/guest';
```

### createUtf8Encoder()

```typescript
function createUtf8Encoder(): (str: string) => Uint8Array;
```

Returns an encode function **byte-identical to `TextEncoder.encode`**, including
the WHATWG rule that a lone surrogate encodes as U+FFFD (`ef bf bd`). Byte
exactness is a hard requirement, not a nicety: the host's wire decoders are
*fatal*, so one wrong byte fails the whole message closed.

Call once per batch or scope and reuse the returned function inside loops.

```typescript
const encode = createUtf8Encoder();
const bytes = encode('hello');  // Uint8Array
```

### createUtf8Decoder()

```typescript
function createUtf8Decoder(): (bytes: Uint8Array) => string;
```

The other direction — for bytes a host capability hands **back** (a file you asked
the host to read, a response body). Output-identical to a **non-fatal**
`TextDecoder('utf-8').decode`: malformed input yields U+FFFD by the WHATWG
replacement rules rather than throwing, and a leading BOM is removed.

Non-fatal is deliberate, and it is the opposite of what the host's wire decoders
do. Those two are not in tension: a malformed wire frame is a protocol violation
that must fail the whole message closed, while arbitrary bytes you were handed
should decode to the same lossy-but-defined string a browser would give. If you
need strictness, encode the result back and compare.

### encodeBase64() / decodeBase64()

```typescript
function encodeBase64(bytes: Uint8Array): string;
function decodeBase64(text: string): Uint8Array;
```

Standard base64 (RFC 4648 §4, padded) — not base64url, whose two extra characters
would silently decode to different bytes than the sender meant. `decodeBase64`
follows the same tolerances `atob` has (ASCII whitespace ignored, padding
optional) and **throws** on anything that is not valid base64, because a
corrupted byte payload that decodes to garbage is worse than one that stops.

### nowMs()

```typescript
function nowMs(): number;
```

`performance.now()` when the sandbox has it, `Date.now()` otherwise. Fallback
resolution is about 1 ms — fine for frame pacing and diagnostics; do not put it on
the wire as a precise timestamp.
