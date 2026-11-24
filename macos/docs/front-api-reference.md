# Actrium Front API reference for macOS

[简体中文](front-api-reference.zh.md) · [Front development guide](keel-askit-guest-guide.md)

This reference covers the current macOS Host for v1 JS Fronts built with `--v1-capability-manifest`. The Host checks the bundle against its signed capability manifest before loading, then exposes the components and methods linked by the Guest. A linked method also needs a matching package capability declaration and any session or user authorization required by the Host.

## Host capabilities

Import each method by name. RPC methods return promises. Subscriptions accept a handler and immediately return an unsubscribe function. Declare the capability ID in the package's `[host]` capabilities. For example, the import `writeText` from `host:clipboard` requires the ID `host:clipboard.write`.

| Module import | Method and result | Capability ID | Availability |
| --- | --- | --- | --- |
| `host:service/<route>` | `call({ payloadBase64: string, timeoutMs?: number }): Promise<{ payloadBase64: string }>` | `service.call` | Bound Service with the fixed RPC route |
| `host:service/<event>` | `on(handler): () => void`, event `{ event: string, payloadBase64: string }` | `service.on` | Bound Service and event source for the fixed event |
| `host:service` | `uploadAttachment({ filename: string, base64: string, type?: AttachmentUploadType, id?: string }): Promise<{ attachmentId: string }>` | `service.attachmentUpload` | Native Realm-bound Service with attachment upload |
| `host:clipboard` | `writeText({ text: string }): Promise<void>` | `host:clipboard.write` | User grant |
| `host:file-import` | `pick({ source?: 'files' \| 'photos', accept?: string[], multiple?: boolean }): Promise<{ files: { name: string, base64: string, thumbnailBase64?: string }[] }>` | `host:file-import.pick` | User grant and native picker |
| `host:file-import` | `openPreview({ fileId: string }): Promise<void>` | `host:file-import.openPreview` | An image selected into this document's attachment cache |
| `host:file-import` | `release({ fileId: string }): Promise<void>` | `host:file-import.release` | An attachment selected into this document's cache |
| `host:local-storage` | `getItem({ key: string }): Promise<string \| null>` | `host:local-storage.getItem` | Native storage for this Front |
| `host:local-storage` | `setItem({ key: string, value: string }): Promise<void>` | `host:local-storage.setItem` | Native storage for this Front |
| `host:local-storage` | `removeItem({ key: string }): Promise<void>` | `host:local-storage.removeItem` | Native storage for this Front |
| `host:speech` | `recognize({ locale?: string, continuous?: boolean }): Promise<{ text: string }>` | `host:speech.recognize` | User grant, native speech support, and required macOS permissions |
| `host:speech` | `stop(): Promise<void>` | `host:speech.stop` | Native speech support for this Guest's session |
| `host:speech` | `onInterimResult(handler): () => void`, event `{ text: string }` | `host:speech.onInterimResult` | Intermediate transcript from this Guest's session |
| `host:speech` | `onStateChange(handler): () => void`, event `{ state: 'recording' \| 'finalizing' }` | `host:speech.onStateChange` | State changes from this Guest's session |

The Host contract also reserves `service.tell`, `host:assets.read`, `host:notifications.show`, `host:camera.capturePhoto`, `host:microphone.record`, `host:location.getCurrentPosition`, `host:external-link.open`, `host:navigate.navigate`, `host:file-export.save`, `host:share.share`, and `host:net.request`. These methods are unimplemented in the current macOS Host. See the [capability model](../../packages/actrium/src/runtime-host/capability-model.ts), [method contracts](../../packages/actrium/src/runtime-host/contract.ts), and [v1 Service binding](../../packages/actrium/src/runtime-host/v1-guest.ts).

### Service RPC and events

Encode the Service contract's protobuf bytes as standard Base64. A call takes the route from the import path. `timeoutMs` defaults to 30,000 ms and accepts a positive value up to 300,000 ms. A subscription emits the exact event name in its import path. Use the Service's advertised events and clean up the subscription when your component unmounts. This example uses a sample event name and takes an already encoded RPC payload.

```ts
import { useEffect } from 'react';
import { call as echo } from 'host:service/echo.v1.EchoService.Echo';
import { on as onNoteChanged } from 'host:service/notes.v1.changed';

async function requestEcho(payloadBase64: string) {
  return echo({ payloadBase64, timeoutMs: 8_000 });
}

function useNoteChanges(onPayload: (payloadBase64: string) => void) {
  useEffect(() => onNoteChanged(({ payloadBase64 }) => onPayload(payloadBase64)), [onPayload]);
}
```

Use fixed imports for `call` and `on`. The root `host:service` module exposes the fixed attachment transport directly.

### Clipboard, file import, and attachment upload

For chat attachments, pass `cache: true` to `pick`. The result contains
`{ fileId, name, size, thumbnailBase64? }` for each item. Pass the same
`fileId` to `uploadAttachment({ fileId, type })` or `openPreview({ fileId })`.
The Host resolves the original from its cache. Release an attachment when no
draft or sent message needs it. Reloading or closing the document clears
its cache.

Clipboard writes accept a string, including an empty string. The file picker uses `source: 'files' | 'photos'`, defaults to `'files'`, and selects a single item by default. For files, `accept: string[]` accepts filename extensions such as `'.pdf'` and MIME filters such as `'image/*'`. The Photos picker selects images. Canceling a picker returns an empty `files` array. A selection is limited to 10 items and 7 MiB of combined file bytes.

```ts
import { writeText } from 'host:clipboard';
import { pick } from 'host:file-import';
import { uploadAttachment } from 'host:service';

async function copyLabel(label: string) {
  await writeText({ text: label });
}

async function uploadDocument() {
  const { files } = await pick({ source: 'files', accept: ['.pdf'] });
  const file = files[0];
  if (!file) return;
  return uploadAttachment({ filename: file.name, base64: file.base64, type: 'document' });
}
```

Upload accepts a nonempty `filename`, standard padded Base64 that decodes to at most 7 MiB, an optional nonempty `id`, and an optional `type` (`AttachmentUploadType`) of `'document' | 'image' | 'audio' | 'video' | 'other'`. The default type is `'document'`. The bound Service returns the `attachmentId`. Handle rejected promises in the UI, including a denied grant for either user-authorized method. See the [macOS picker](../Actrium/MultiProcess/RendererFileImport.swift) and [attachment transport](../../packages/actrium/src/runtime-host/attachment-upload.ts).

### Local storage

The native broker scopes storage to the Front's storage origin. Keys and values are strings. A missing key returns `null`, and removing a missing key succeeds. The UTF-8 limits are 1,024 bytes per key and 64 KiB per value, with at most 1,024 entries and 1 MiB per origin. Store complex values as JSON and handle rejected writes.

```ts
import { getItem, removeItem, setItem } from 'host:local-storage';

async function saveDraft(draft: string) {
  await setItem({ key: 'draft', value: draft });
}

async function loadDraft() {
  return (await getItem({ key: 'draft' })) ?? '';
}

async function clearDraft() {
  await removeItem({ key: 'draft' });
}
```

See the [native storage backend](../../crates/actrium-core/src/guest_local_storage.rs) and [macOS origin selection](../Actrium/Models/ActrPackageManifestReader.swift).

### Speech recognition

`locale` is an optional language tag such as `'zh-CN'`. Omit `continuous` to finish after one utterance, or pass `true` to collect dictation until the session ends. Start recognition from a user action in the active, authorized Tab. Each Guest has one recognition session at a time. `recognize` resolves with the final text, while `stop` ends the active recognition. Subscribe before starting if you need live transcription or recording status, and use both unsubscribe functions when the UI unmounts.

```tsx
import { useEffect } from 'react';
import { onInterimResult, onStateChange, recognize, stop } from 'host:speech';

function useDictationStatus(
  updateInterim: (text: string) => void,
  updateState: (state: 'recording' | 'finalizing') => void
) {
  useEffect(() => {
    const offInterim = onInterimResult(({ text }) => updateInterim(text));
    const offState = onStateChange(({ state }) => updateState(state));
    return () => {
      offState();
      offInterim();
    };
  }, [updateInterim, updateState]);
}

async function startDictation() {
  return (await recognize({ locale: 'zh-CN', continuous: true })).text;
}

async function finishDictation() {
  await stop();
}
```

See the [Host method implementation](../../packages/actrium/src/runtime-host/host-modules.ts) and [Askaway](https://github.com/Actrium/actr-hub/tree/main/askaway) for a full UI with dictation, clipboard, and uploads.

## Components

Import the following components from `keel/guest`. The prop lists describe the Guest SDK types and the explicit macOS wrappers. `key` is React's list key. Where listed, `style` takes a React Native-style object or array, and `testID` identifies an element for tests. Native event objects cross the Guest bridge. Use their data fields, such as `nativeEvent.layout` for layout and `nativeEvent.contentOffset` for scrolling.

| Component | Props | Events and macOS behavior |
| --- | --- | --- |
| `View` | `children`, `style`, `testID`, `pointerEvents`, `accessible`, `accessibilityLabel` | `onLayout(event)`, `onMouseEnter()`, `onMouseLeave()` |
| `Text` | `children`, `style`, `testID`, `numberOfLines`, `ellipsizeMode`, `selectable` | `onPress()`, `onLongPress()` |
| `ScrollView` | `View` props plus `horizontal`, `showsVerticalScrollIndicator`, `showsHorizontalScrollIndicator`, `pagingEnabled`, `bounces`, `scrollEnabled`, `scrollEventThrottle`, `contentContainerStyle`, `keyboardShouldPersistTaps`, `keyboardDismissMode` | `View` events plus `onScroll`, `onScrollBeginDrag`, `onScrollEndDrag`, `onMomentumScrollBegin`, `onMomentumScrollEnd`, each with a scroll event |
| `FlatList` | `ScrollView` props plus `data`, `renderItem({ item, index })`, `keyExtractor(item, index)`, `ItemSeparatorComponent`, `ListHeaderComponent`, `ListFooterComponent`, `ListEmptyComponent`, `numColumns`, `initialNumToRender`, `onEndReachedThreshold`, `refreshing`, `getItemLayout(data, index)` | `ScrollView` events plus `onEndReached()` and `onRefresh()`. The native list calls render and layout callbacks across the bridge. |
| `TextInput` | `value`, `defaultValue`, `placeholder`, `placeholderTextColor`, `style`, `testID`, `accessible`, `accessibilityLabel`, `multiline`, `numberOfLines`, `maxLength`, `editable`, `autoCapitalize`, `autoCorrect`, `autoFocus`, `secureTextEntry`, `keyboardType`, `returnKeyType`, `selectTextOnFocus`, `submitBehavior`, `submitOnEnter` | `onChangeText(text)`, `onChange({ nativeEvent: { text, eventCount } })`, `onFocus()`, `onBlur()`, `onSubmitEditing()`. With `multiline` and the macOS extension `submitOnEnter`, plain Enter submits and the wrapper uses the native multiline keyboard path. |
| `Button` | `title`, `color`, `disabled`, `style`, `testID`, `accessibilityLabel`, `children` | `onPress()`. Optional children replace the drawn title. The title remains the default accessibility label. |
| `Switch` | `value`, `disabled`, `trackColor`, `thumbColor`, `style`, `testID`, `ios_backgroundColor` | `onValueChange(value)`. `ios_backgroundColor` is an iOS-specific SDK prop. |
| `Select` | `options`, `value`, `placeholder`, `disabled`, `accessibilityLabel`, `style`, `testID` | Opens a native macOS menu anchored to the control. `onValueChange(value)` returns the selected option value. |
| `Menu` | `title`, `items`, `disabled`, `accessibilityLabel`, `style`, `testID`, `children` | Opens a native macOS action menu anchored to the button. Optional children replace the drawn title. `onSelect(value)` returns the selected action value. |
| `Svg` | `width`, `height`, `viewBox`, `paths`, `color`, `style`, `testID`, `accessibilityLabel` | Displays local SVG paths in macOS Host builds that include RNSVG. Each path takes `d` and optional fill, stroke, and opacity attributes. |
| `Image` | Numeric asset ID or bounded JPEG data URI `source`, `resizeMode`, `style`, `testID`, `accessible`, `accessibilityLabel` | `onLoad`, `onError`. Manifest name `ImageAsset`. File picker thumbnails support the `data:image/jpeg;base64,` prefix and up to 12 KiB of JPEG bytes. |
| `Supramark` | `markdown: string`, `theme?: 'light' \| 'dark'`, `paddingHorizontal?: number`, `variant?: 'error'` | Available in Host builds with the renderer enabled. Renders Markdown display content up to 65,536 JS string code units. Link labels and image alt text are rendered as content. The `error` variant renders the input verbatim as selectable text with the default paragraph style. |

For controlled input, update `value` with `onChangeText`. The macOS `submitOnEnter` extension is handled by the Host wrapper and is absent from the generic Keel `TextInputProps` type. The v1 SDK does not expose imperative component handles through `useRemoteRef`. For typed values, see the [Guest component declarations](../../packages/keel/src/sdk/sdk.ts). For actual restrictions, see the [macOS registry](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx), the [TextInput wrapper](../../packages/actrium/src/runtime-host/GuestTextInput.tsx), and the [image allowlist](../../packages/actrium/src/runtime-host/image-asset.ts).

Each `Select` option and `Menu` item is a `{ value, label, disabled? }` object. Values must be unique within a menu. Dismissing a menu leaves the Guest selection unchanged.

On macOS, set `style={{ cursor: 'pointer' }}` on a clickable component for the pointing hand. The visible window applies the nearest explicit cursor from the renderer's hit path. A `View` can use `onMouseEnter()` and `onMouseLeave()` to update hover state.

## Guest SDK and sandbox APIs

### Imports

The v1 runtime accepts `react`, `react/jsx-runtime`, `react/jsx-dev-runtime`, `keel/guest`, and linked named `host:*` exports. Use static imports. The following are all the v1 runtime exports from `keel/guest` in addition to the components above.

| Export | Use |
| --- | --- |
| `StyleSheet.create(styles)` | Returns the style object with its type preserved. |
| `StyleSheet.flatten(style)` | Returns the supplied value as-is in the Guest shim. |
| `useConfig<T>()` | Reads the Host's current config. The standalone Front passes `containerWidth` and `containerHeight` when available. |
| `createUtf8Encoder()` | Returns a function `(text: string) => Uint8Array`. |
| `createUtf8Decoder()` | Returns a function `(bytes: Uint8Array) => string`. |
| `encodeBase64(bytes)` | Encodes a `Uint8Array` as standard padded Base64. |
| `decodeBase64(text)` | Decodes standard Base64 to `Uint8Array`. |
| `nowMs()` | Returns `performance.now()` when available, with `Date.now()` as a fallback. Use it for local timing. |

The v1 import allowlist excludes React Native runtime imports and the additional exports documented for generic Keel, including `TouchableOpacity`, `ActivityIndicator`, `Platform`, `Linking`, `useHostEvent`, and `useRemoteRef`. Type-only imports are removed by the build. See the [v1 import rules](../../packages/keel/src/v1.ts) and [Guest byte utilities](../../packages/keel/src/guest/runtime/reconciler/sandbox-compat.ts).

### Globals

Use ECMAScript built-ins such as `Promise`, `JSON`, `Map`, `Date`, `ArrayBuffer`, and typed arrays. Keel adds the following application-facing globals.

| Global | Use |
| --- | --- |
| `console.log`, `console.info`, `console.warn`, `console.error`, `console.debug` | Log through the Host console. |
| `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` | Schedule and cancel timers managed by the Guest engine. |
| `setImmediate`, `clearImmediate`, `queueMicrotask` | Schedule deferred work in the Guest engine. |

The exact JavaScript built-ins depend on the Host's sandbox engine. For portable code, use the SDK byte and timing helpers where you would otherwise use `TextEncoder`, `TextDecoder`, `atob`, `btoa`, or `performance`. The Guest sandbox exposes no general-purpose DOM, Node.js filesystem, browser storage, or network API. Use the linked Host methods or a bound Service for external effects. See the [engine's global setup](../../packages/keel/src/host/engine/engine.ts) and the [Guest console setup](../../packages/keel/src/guest/runtime/globals-setup.ts).
