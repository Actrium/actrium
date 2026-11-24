# macOS Actrium Front API 参考

[English](front-api-reference.md) · [Front 开发指南](keel-askit-guest-guide.zh.md)

本文对应当前 macOS Host 上通过 `--v1-capability-manifest` 构建的 v1 JS Front。Host 加载前会核对 bundle 和已签名的能力清单，再向 Guest 提供已链接的组件和方法。调用方法还需要在包里声明对应的能力，以及满足 Host 要求的会话状态和用户授权。

## Host 能力

按名称导入方法。RPC 方法返回 Promise，订阅方法接收回调并立即返回一个取消订阅的函数。在包清单的 `[host]` 能力列表中声明能力 ID。例如从 `host:clipboard` 导入 `writeText` 时，要声明 `host:clipboard.write`。

| 导入模块 | 方法和结果 | 能力 ID | 使用条件 |
| --- | --- | --- | --- |
| `host:service/<route>` | `call({ payloadBase64: string, timeoutMs?: number }): Promise<{ payloadBase64: string }>` | `service.call` | 已绑定 Service，且提供该固定 RPC 路由 |
| `host:service/<event>` | `on(handler): () => void`，事件为 `{ event: string, payloadBase64: string }` | `service.on` | 已绑定 Service，且提供该固定事件的数据源 |
| `host:service` | `uploadAttachment({ filename: string, base64: string, type?: AttachmentUploadType, id?: string }): Promise<{ attachmentId: string }>` | `service.attachmentUpload` | 绑定了支持上传的原生 Realm Service |
| `host:clipboard` | `writeText({ text: string }): Promise<void>` | `host:clipboard.write` | 用户授权 |
| `host:file-import` | `pick({ source?: 'files' \| 'photos', accept?: string[], multiple?: boolean }): Promise<{ files: { name: string, base64: string, thumbnailBase64?: string }[] }>` | `host:file-import.pick` | 用户授权及原生选择器 |
| `host:file-import` | `openPreview({ fileId: string }): Promise<void>` | `host:file-import.openPreview` | 当前文档选择并缓存的图片 |
| `host:file-import` | `release({ fileId: string }): Promise<void>` | `host:file-import.release` | 当前文档选择并缓存的附件 |
| `host:local-storage` | `getItem({ key: string }): Promise<string \| null>` | `host:local-storage.getItem` | 当前 Front 的原生本地存储 |
| `host:local-storage` | `setItem({ key: string, value: string }): Promise<void>` | `host:local-storage.setItem` | 当前 Front 的原生本地存储 |
| `host:local-storage` | `removeItem({ key: string }): Promise<void>` | `host:local-storage.removeItem` | 当前 Front 的原生本地存储 |
| `host:speech` | `recognize({ locale?: string, continuous?: boolean }): Promise<{ text: string }>` | `host:speech.recognize` | 用户授权、原生语音支持及所需的 macOS 系统权限 |
| `host:speech` | `stop(): Promise<void>` | `host:speech.stop` | 当前 Guest 会话的原生语音支持 |
| `host:speech` | `onInterimResult(handler): () => void`，事件为 `{ text: string }` | `host:speech.onInterimResult` | 当前 Guest 会话的临时识别结果 |
| `host:speech` | `onStateChange(handler): () => void`，事件为 `{ state: 'recording' \| 'finalizing' }` | `host:speech.onStateChange` | 当前 Guest 会话的状态变化 |

契约还预留了 `service.tell`、`host:assets.read`、`host:notifications.show`、`host:camera.capturePhoto`、`host:microphone.record`、`host:location.getCurrentPosition`、`host:external-link.open`、`host:navigate.navigate`、`host:file-export.save`、`host:share.share` 和 `host:net.request`。这些方法在当前 macOS Host 中尚未实现。详见[能力模型](../../packages/actrium/src/runtime-host/capability-model.ts)、[方法契约](../../packages/actrium/src/runtime-host/contract.ts)和 [v1 Service 绑定](../../packages/actrium/src/runtime-host/v1-guest.ts)。

### Service RPC 与事件订阅

用标准 Base64 编码 Service 契约定义的 protobuf 字节。Host 从导入路径取 RPC 路由。`timeoutMs` 默认 30,000 毫秒，传入时须为大于 0 且不超过 300,000 的数。订阅只接收导入路径里指定的事件。使用 Service 已声明的事件，并在组件卸载时取消订阅。下面的事件名是示例，RPC 函数接收已经编码好的请求。

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

`call` 和 `on` 使用带固定路由或事件名的导入。附件上传直接从 `host:service` 导入。

### 剪贴板、文件导入和附件上传

选择聊天附件时可以向 `pick` 传入 `cache: true`。每个结果包含
`{ fileId, name, size, thumbnailBase64? }`。将同一个 `fileId` 传给
`uploadAttachment({ fileId, type })` 或 `openPreview({ fileId })`，Host 会从缓存读取原件。
草稿和已发送的消息都不再需要附件时，可以调用 `release`。刷新或关闭文档会清理其缓存。

剪贴板可以写入任意字符串，包括空字符串。文件选择器的 `source` 为 `'files' | 'photos'`，默认 `'files'`，默认单选。选择文件时，`accept: string[]` 可填 `'.pdf'` 等扩展名或 `'image/*'` 等 MIME 过滤值。照片选择器只选图片。用户取消选择时返回空的 `files` 数组。一次最多选择 10 项，文件总大小上限为 7 MiB。

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

上传时需要非空 `filename` 和标准带填充的 Base64，解码后最多 7 MiB。`id` 可省略，提供时需为非空字符串。`type` 的类型为 `AttachmentUploadType`，支持 `'document' | 'image' | 'audio' | 'video' | 'other'`，默认 `'document'`。绑定的 Service 返回 `attachmentId`。在 UI 层处理调用失败，包括用户拒绝剪贴板授权或文件选择授权。详见 [macOS 文件选择器](../Actrium/MultiProcess/RendererFileImport.swift)和[附件传输](../../packages/actrium/src/runtime-host/attachment-upload.ts)。

### 本地存储

原生存储按 Front 的存储来源隔离。键和值都是字符串。读取不存在的键返回 `null`，删除不存在的键也会成功。按 UTF-8 计算，每个键最多 1,024 字节，每个值最多 64 KiB。每个存储来源最多 1,024 项，总共 1 MiB。复杂数据可以序列化成 JSON，写入失败应由调用方处理。

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

详见[原生存储后端](../../crates/actrium-core/src/guest_local_storage.rs)和 [macOS 存储来源选择](../Actrium/Models/ActrPackageManifestReader.swift)。

### 语音识别

`locale` 是可选的语言标签，例如 `'zh-CN'`。省略 `continuous` 时识别一段话后结束，设为 `true` 时可持续听写到会话结束。在已授权的当前 Tab 中由用户操作触发识别。每个 Guest 同时只能运行一个识别会话。`recognize` 返回最终文本，`stop` 结束当前识别。需要实时文字或录制状态时，在启动前订阅，组件卸载时调用两个取消订阅函数。

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

完整的听写、复制和附件上传界面可参考 [Askaway](https://github.com/Actrium/actr-hub/tree/main/askaway)。[Host 方法实现](../../packages/actrium/src/runtime-host/host-modules.ts)

## 组件

以下组件均从 `keel/guest` 导入。表中的 props 来自 Guest SDK 类型和 macOS Host 的组件包装层。`key` 是 React 的列表键。表中列出的 `style` 接受 React Native 风格的对象或数组，`testID` 用于测试定位。原生事件经过 Guest bridge 传递，读取布局或滚动等数据时可用 `nativeEvent.layout` 或 `nativeEvent.contentOffset`。

| 组件 | Props | 事件与 macOS 行为 |
| --- | --- | --- |
| `View` | `children`、`style`、`testID`、`pointerEvents`、`accessible`、`accessibilityLabel` | `onLayout(event)`、`onMouseEnter()`、`onMouseLeave()` |
| `Text` | `children`、`style`、`testID`、`numberOfLines`、`ellipsizeMode`、`selectable` | `onPress()`、`onLongPress()` |
| `ScrollView` | `View` 的 props 加上 `horizontal`、`showsVerticalScrollIndicator`、`showsHorizontalScrollIndicator`、`pagingEnabled`、`bounces`、`scrollEnabled`、`scrollEventThrottle`、`contentContainerStyle`、`keyboardShouldPersistTaps`、`keyboardDismissMode` | `View` 的事件加上 `onScroll`、`onScrollBeginDrag`、`onScrollEndDrag`、`onMomentumScrollBegin`、`onMomentumScrollEnd`，后五项都接收滚动事件 |
| `FlatList` | `ScrollView` 的 props 加上 `data`、`renderItem({ item, index })`、`keyExtractor(item, index)`、`ItemSeparatorComponent`、`ListHeaderComponent`、`ListFooterComponent`、`ListEmptyComponent`、`numColumns`、`initialNumToRender`、`onEndReachedThreshold`、`refreshing`、`getItemLayout(data, index)` | `ScrollView` 的事件加上 `onEndReached()`、`onRefresh()`。原生列表通过 bridge 调用渲染和布局回调。 |
| `TextInput` | `value`、`defaultValue`、`placeholder`、`placeholderTextColor`、`style`、`testID`、`accessible`、`accessibilityLabel`、`multiline`、`numberOfLines`、`maxLength`、`editable`、`autoCapitalize`、`autoCorrect`、`autoFocus`、`secureTextEntry`、`keyboardType`、`returnKeyType`、`selectTextOnFocus`、`submitBehavior`、`submitOnEnter` | `onChangeText(text)`、`onChange({ nativeEvent: { text, eventCount } })`、`onFocus()`、`onBlur()`、`onSubmitEditing()`。同时启用 `multiline` 和 macOS 扩展 `submitOnEnter` 时，单按 Enter 提交，输入框走原生多行键盘处理。 |
| `Button` | `title`、`color`、`disabled`、`style`、`testID`、`accessibilityLabel`、`children` | `onPress()`。可用子元素自定义按钮外观，`title` 默认作为无障碍标签。 |
| `Switch` | `value`、`disabled`、`trackColor`、`thumbColor`、`style`、`testID`、`ios_backgroundColor` | `onValueChange(value)`。`ios_backgroundColor` 是 iOS 专用的 SDK prop。 |
| `Select` | `options`、`value`、`placeholder`、`disabled`、`accessibilityLabel`、`style`、`testID` | 在控件旁打开 macOS 原生下拉框。选中后调用 `onValueChange(value)`。 |
| `Menu` | `title`、`items`、`disabled`、`accessibilityLabel`、`style`、`testID`、`children` | 在按钮旁打开 macOS 原生操作菜单，可用子元素自定义按钮外观。选中后调用 `onSelect(value)`。 |
| `Svg` | `width`、`height`、`viewBox`、`paths`、`color`、`style`、`testID`、`accessibilityLabel` | 在链接 RNSVG 的 macOS Host 构建中显示本地 SVG 路径。每条路径可传 `d`、填充、描边和透明度。 |
| `Image` | 数字资源 ID 或受限 JPEG data URI 类型的 `source`、`resizeMode`、`style`、`testID`、`accessible`、`accessibilityLabel` | `onLoad`、`onError`。在清单中记为 `ImageAsset`。文件选择器缩略图可使用 `data:image/jpeg;base64,` 前缀，JPEG 数据最多 12 KiB。 |
| `Supramark` | `markdown: string`、`theme?: 'light' \| 'dark'`、`paddingHorizontal?: number`、`variant?: 'error'` | 需要 Host 构建启用对应渲染器。渲染最多 65,536 个 JS 字符串代码单元的 Markdown。链接文字和图片替代文字作为内容展示。`error` 用默认段落样式展示可选中的原文。 |

在 macOS 上，可通过 `style={{ cursor: 'pointer' }}` 为可点击组件使用手形指针。可见窗口按 Renderer 命中路径上最近的显式设置更新指针。`View` 可通过 `onMouseEnter()` 和 `onMouseLeave()` 更新悬停状态。

受控输入框用 `onChangeText` 更新 `value`。macOS 扩展 `submitOnEnter` 由 Host 包装层处理，通用 Keel 的 `TextInputProps` 未声明它。v1 Front 不开放 `useRemoteRef` 提供的命令式组件操作。详见 [Guest 组件类型](../../packages/keel/src/sdk/sdk.ts)、[macOS 注册表](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx)、[TextInput 包装层](../../packages/actrium/src/runtime-host/GuestTextInput.tsx)和[图片白名单](../../packages/actrium/src/runtime-host/image-asset.ts)。

## Guest SDK 和沙箱 API

### 模块导入

v1 运行时接受 `react`、`react/jsx-runtime`、`react/jsx-dev-runtime`、`keel/guest`，以及已链接的具名 `host:*` 导出。使用静态导入。除了上面的组件，`keel/guest` 在 v1 还开放以下运行时 API。

| 导出 | 用法 |
| --- | --- |
| `StyleSheet.create(styles)` | 原样返回样式对象，并保留其类型。 |
| `StyleSheet.flatten(style)` | Guest 实现原样返回传入值。 |
| `useConfig<T>()` | 读取 Host 当前配置。独立 Front 会在可用时收到 `containerWidth` 和 `containerHeight`。 |
| `createUtf8Encoder()` | 返回 `(text: string) => Uint8Array`。 |
| `createUtf8Decoder()` | 返回 `(bytes: Uint8Array) => string`。 |
| `encodeBase64(bytes)` | 将 `Uint8Array` 编码为标准带填充的 Base64。 |
| `decodeBase64(text)` | 将标准 Base64 解码为 `Uint8Array`。 |
| `nowMs()` | 可用时返回 `performance.now()`，否则回退到 `Date.now()`。用于本地计时。 |

v1 导入白名单排除了 React Native 运行时导入以及通用 Keel 的额外导出，例如 `TouchableOpacity`、`ActivityIndicator`、`Platform`、`Linking`、`useHostEvent` 和 `useRemoteRef`。构建时会移除纯类型导入。详见 [v1 导入规则](../../packages/keel/src/v1.ts)和 [Guest 字节工具](../../packages/keel/src/guest/runtime/reconciler/sandbox-compat.ts)。

### 全局 API

可以用 ECMAScript 内建 API，例如 `Promise`、`JSON`、`Map`、`Date`、`ArrayBuffer` 和类型化数组。Keel 还提供以下面向 Guest 应用的全局 API。

| 全局 API | 用法 |
| --- | --- |
| `console.log`、`console.info`、`console.warn`、`console.error`、`console.debug` | 将日志交给 Host。 |
| `setTimeout`、`clearTimeout`、`setInterval`、`clearInterval` | 创建和取消由 Guest Engine 管理的计时器。 |
| `setImmediate`、`clearImmediate`、`queueMicrotask` | 在 Guest Engine 中调度延后执行的任务。 |

JavaScript 内建 API 的实际范围取决于 Host 的沙箱引擎。需要跨引擎兼容时，用上表中的 SDK 字节和计时工具替代对 `TextEncoder`、`TextDecoder`、`atob`、`btoa` 或 `performance` 的依赖。沙箱没有通用 DOM、Node.js 文件系统、浏览器存储或网络 API。需要访问沙箱外的能力时，通过已链接的 Host 方法或绑定的 Service。详见 [Engine 全局 API 初始化](../../packages/keel/src/host/engine/engine.ts)和 [Guest console 初始化](../../packages/keel/src/guest/runtime/globals-setup.ts)。
