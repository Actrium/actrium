# Guest API 参考

Guest 模块提供了用于在 Rill 沙箱内构建动态 UI 的组件、hooks 和平台 API。Guest 代码使用标准 React 模式（JSX、hooks）并通过定义良好的协议与 Host 通信。

导入路径：`rill/guest`

---

## 组件

下面列出的所有组件都可以在沙箱内通过 `require('rill/guest')` 或 JSX 使用。它们映射到通过 `ComponentRegistry` 注册的 Host 侧 React Native 组件实现。

### View

容器组件。映射到 React Native 的 `View`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `style` | `ViewStyle` | `undefined` | 用于布局和外观的样式对象。 |
| `testID` | `string` | `undefined` | 用于自动化的测试标识符。 |
| `onLayout` | `(event: LayoutEvent) => void` | `undefined` | 视图布局发生变化时调用。 |

### Text

文本显示组件。映射到 React Native 的 `Text`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `style` | `TextStyle` | `undefined` | 文本外观的样式对象。 |
| `numberOfLines` | `number` | `undefined` | 截断前的最大行数。 |
| `ellipsizeMode` | `'head' \| 'middle' \| 'tail' \| 'clip'` | `'tail'` | 文本超过 `numberOfLines` 时如何截断。 |
| `selectable` | `boolean` | `false` | 文本是否可被用户选择。 |
| `onPress` | `() => void` | `undefined` | 文本被按下时调用。 |

### Image

图像显示组件。映射到 React Native 的 `Image`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `source` | `{ uri: string } \| number` | （必需） | 图像源。远程图像使用 URI 对象，打包资源使用数字。 |
| `style` | `ImageStyle` | `undefined` | 图像外观的样式对象。 |
| `resizeMode` | `'cover' \| 'contain' \| 'stretch' \| 'repeat' \| 'center'` | `'cover'` | 图像如何调整大小以适应其容器。 |
| `onLoad` | `() => void` | `undefined` | 图像加载完成时调用。 |
| `onError` | `(error: { nativeEvent: { error: string } }) => void` | `undefined` | 图像加载失败时调用。 |

### TouchableOpacity

带不透明度反馈的可触摸包装器。映射到 React Native 的 `TouchableOpacity`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `onPress` | `() => void` | `undefined` | 组件被按下时调用。 |
| `onLongPress` | `() => void` | `undefined` | 组件被长按时调用。 |
| `activeOpacity` | `number` | `0.2` | 触摸活动时应用的不透明度。 |
| `disabled` | `boolean` | `false` | 触摸是否被禁用。 |

### ScrollView

可滚动容器。映射到 React Native 的 `ScrollView`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | 滚动视图是否水平滚动。 |
| `showsVerticalScrollIndicator` | `boolean` | `true` | 是否显示垂直滚动指示器。 |
| `showsHorizontalScrollIndicator` | `boolean` | `true` | 是否显示水平滚动指示器。 |
| `onScroll` | `(event: ScrollEvent) => void` | `undefined` | 滚动位置变化时调用。 |

### FlatList

高性能可滚动列表。映射到 React Native 的 `FlatList`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `data` | `T[]` | （必需） | 要渲染的项目数组。 |
| `renderItem` | `(info: { item: T, index: number }) => ReactElement` | （必需） | 渲染每个项目的函数。 |
| `keyExtractor` | `(item: T, index: number) => string` | `undefined` | 为每个项目提取唯一键的函数。 |
| `horizontal` | `boolean` | `false` | 列表是否水平滚动。 |
| `onEndReached` | `() => void` | `undefined` | 到达列表末尾时调用。 |
| `onEndReachedThreshold` | `number` | `undefined` | 距离末尾多远（以可见长度单位）触发 `onEndReached`。 |
| `ListHeaderComponent` | `ReactElement \| (() => ReactElement)` | `undefined` | 在列表顶部渲染。 |
| `ListEmptyComponent` | `ReactElement \| (() => ReactElement)` | `undefined` | 当 `data` 为空时渲染。 |

### TextInput

文本输入字段。映射到 React Native 的 `TextInput`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `value` | `string` | `undefined` | 当前文本值（受控）。 |
| `onChangeText` | `(text: string) => void` | `undefined` | 文本变化时调用。 |
| `placeholder` | `string` | `undefined` | 为空时显示的占位符文本。 |
| `secureTextEntry` | `boolean` | `false` | 是否遮蔽文本（密码字段）。 |
| `multiline` | `boolean` | `false` | 输入是否支持多行。 |
| `maxLength` | `number` | `undefined` | 允许的最大字符数。 |

### Button

基础按钮。映射到 React Native 的 `Button`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `title` | `string` | （必需） | 按钮标签文本。 |
| `onPress` | `() => void` | （必需） | 按钮被按下时调用。 |
| `disabled` | `boolean` | `false` | 按钮是否被禁用。 |
| `color` | `string` | `undefined` | 按钮颜色（平台特定行为）。 |

### Switch

开关切换。映射到 React Native 的 `Switch`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `value` | `boolean` | `false` | 当前开/关状态。 |
| `onValueChange` | `(value: boolean) => void` | `undefined` | 开关被切换时调用。 |
| `disabled` | `boolean` | `false` | 开关是否被禁用。 |

### ActivityIndicator

加载旋转器。映射到 React Native 的 `ActivityIndicator`。

| Prop | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `size` | `'small' \| 'large'` | `'small'` | 指示器的大小。 |
| `color` | `string` | `undefined` | 指示器的颜色。 |
| `animating` | `boolean` | `true` | 指示器是否正在动画。 |

---

## Hooks

### useConfig\<T\>()

返回当前 Guest 配置。配置最初通过 `loadBundle(source, initialProps)` 设置，可以通过 `engine.updateConfig()` 从 Host 更新。

```typescript
function useConfig<T = Record<string, unknown>>(): T
```

**示例：**

```tsx
const config = useConfig<{ theme: string; userName: string }>();
return <Text>{config.userName}</Text>;
```

### useHostEvent\<T\>(eventName, callback)

订阅通过 `engine.sendEvent()` 从 Host 发送的事件。当 Host 发送匹配名称的事件时，将调用回调。

```typescript
function useHostEvent<T = unknown>(
  eventName: string,
  callback: (payload: T) => void
): void
```

**示例：**

```tsx
useHostEvent<{ items: string[] }>('dataUpdate', (payload) => {
  setItems(payload.items);
});
```

### useSendToHost()

返回一个从 Guest 向 Host 发送事件的函数。Host 通过 `engine.on('message', handler)` 接收这些事件。

```typescript
function useSendToHost(): (eventName: string, payload?: unknown) => void
```

**示例：**

```tsx
const sendToHost = useSendToHost();

const handlePress = () => {
  sendToHost('buttonPressed', { id: 'submit' });
};
```

### useRemoteRef\<T\>()

创建对 Host 侧组件实例的远程引用，允许 Guest 在其上调用方法（例如 `focus()`、`scrollTo()`）。

```typescript
function useRemoteRef<T>(): [RefCallback, RemoteRef<T> | null]
```

返回一个元组：
1. 附加到目标组件的 ref 回调。
2. 一个 `RemoteRef` 对象（在 ref 附加之前为 `null`）。

#### RemoteRef 接口

| 属性 / 方法 | 类型 | 描述 |
|---|---|---|
| `nodeId` | `number` | 引用组件的节点 ID。 |
| `invoke(method, ...args)` | `(...args: unknown[]) => Promise<unknown>` | 在 Host 组件实例上调用方法。返回带结果的 Promise。 |
| `call` | `Proxy` | 用于直接方法调用的代理对象（语法糖）。 |

#### 预定义 Ref 类型

**TextInputRef：**

| 方法 | 签名 | 描述 |
|---|---|---|
| `focus()` | `() => Promise<void>` | 聚焦文本输入。 |
| `blur()` | `() => Promise<void>` | 失焦文本输入。 |
| `clear()` | `() => Promise<void>` | 清除文本输入值。 |

**ScrollViewRef：**

| 方法 | 签名 | 描述 |
|---|---|---|
| `scrollTo(options)` | `(options: { x?: number, y?: number, animated?: boolean }) => Promise<void>` | 滚动到特定位置。 |
| `scrollToEnd(options?)` | `(options?: { animated?: boolean }) => Promise<void>` | 滚动到内容末尾。 |

**FlatListRef：**

| 方法 | 签名 | 描述 |
|---|---|---|
| `scrollToIndex(params)` | `(params: { index: number, animated?: boolean }) => Promise<void>` | 滚动到特定项目索引。 |
| `scrollToOffset(params)` | `(params: { offset: number, animated?: boolean }) => Promise<void>` | 滚动到特定像素偏移。 |

**示例：**

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

## RillErrorBoundary

用于捕获 Guest 代码中渲染错误的 React 错误边界组件。防止单个组件错误导致整个沙箱 UI 崩溃。

```typescript
interface RillErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactElement | ((error: Error) => ReactElement);
  onError?: (error: Error, errorInfo: { componentStack: string }) => void;
}
```

| Prop | 类型 | 描述 |
|---|---|---|
| `children` | `ReactNode` | 要包装的组件树。 |
| `fallback` | `ReactElement \| (error: Error) => ReactElement` | 捕获错误时渲染的内容。 |
| `onError` | `(error: Error, errorInfo: object) => void` | 捕获错误时调用。 |

---

## 平台 API

这些 API 提供对沙箱内平台信息和实用工具的访问。

### Platform

提供有关当前平台的信息。

```typescript
const Platform: {
  OS: 'ios' | 'android' | 'web' | 'macos' | 'windows';
  Version: number | string;
  select: <T>(specifics: { ios?: T; android?: T; default?: T }) => T;
}
```

### Dimensions

提供屏幕和窗口尺寸。

```typescript
const Dimensions: {
  get: (dim: 'window' | 'screen') => { width: number; height: number };
}
```

### StyleSheet

用于创建优化样式对象的实用工具。

```typescript
const StyleSheet: {
  create: <T extends Record<string, ViewStyle | TextStyle | ImageStyle>>(styles: T) => T;
  flatten: (style: StyleProp) => object;
  absoluteFill: ViewStyle;
  hairlineWidth: number;
}
```

### Linking

提供 URL 打开功能。

```typescript
const Linking: {
  openURL: (url: string) => Promise<void>;
  canOpenURL: (url: string) => Promise<boolean>;
}
```

---

## 沙箱安全工具

guest 沙箱只提供**核心 ECMAScript**。Web 平台的全局对象不在其中，而处理字节时最想用的那几个恰好都不存在。下面是从一个跑在 QuickJS-WASM 里的真实 rill guest 内部读 `typeof globalThis[name]` 得到的结果：

| 不存在 | 存在 |
|---|---|
| `TextEncoder`、`TextDecoder`、`performance`、`btoa`、`atob`、`Buffer`、`crypto` | `Uint8Array`、`console`、`setTimeout` |

这不是"某些平台上可能没有"的兼容性提醒，而是整整一类能力都不在：编码和字节格式化的全局对象，**两个方向都没有**。

> **TypeScript 不会替你拦住这个坑。** 只要 guest 的 `tsconfig` 的 `lib` 里含 `DOM`，`TextEncoder`、`TextDecoder`、`performance` 就都被声明了，于是 `new TextEncoder()` 类型检查完全通过，然后在沙箱里运行时抛错。而 guest 的 `tsconfig` 往往会因为别的原因带上 `DOM`。更麻烦的是这个失败依赖运行环境：在浏览器里跑本地开发循环时一切正常，等上真机才炸。

`rill/guest` 把 rill 自己的 guest bundle 在用的那几个helper 出口了出来，你不必重新实现一遍。每个 helper 都在**每次调用时**重新检查真实全局是否存在（所以宿主晚一步注入全局也能被认到），检查不到就走纯 JS 实现。

```typescript
import {
  createUtf8Encoder,
  createUtf8Decoder,
  encodeBase64,
  decodeBase64,
  nowMs,
} from 'rill/guest';
```

### createUtf8Encoder()

```typescript
function createUtf8Encoder(): (str: string) => Uint8Array;
```

返回一个编码函数，输出与 `TextEncoder.encode` **逐字节相同**，包括 WHATWG 规定的"孤立代理项编码为 U+FFFD（`ef bf bd`）"这条规则。

逐字节相同是硬要求，不是讲究：宿主侧的 wire 解码器是**致命模式**的 —— 错一个字节，整条消息就整体失败关闭，不会只丢那个字符。

一次调用拿到函数后在循环里复用，不要每次都重新创建。

```typescript
const encode = createUtf8Encoder();
const bytes = encode('hello');  // Uint8Array
```

### createUtf8Decoder()

```typescript
function createUtf8Decoder(): (bytes: Uint8Array) => string;
```

反方向 —— 用于宿主能力**交回**给你的字节（你请宿主读的一个文件、一个响应体）。输出与**非致命**的 `TextDecoder('utf-8').decode` 一致：坏输入按 WHATWG 的替换规则产出 U+FFFD 而不是抛错，开头的 BOM 会被去掉。

选非致命是刻意的，而且和宿主侧 wire 解码器的选择正好相反。这两件事并不矛盾，各说各的：wire 帧格式错了是协议被违反，必须整条消息失败关闭；而你拿到的一段任意字节，应该解出浏览器会给你的那个"有损但有定义"的字符串。真需要严格判断时，把结果再编码回去比较一下即可。

### encodeBase64() / decodeBase64()

```typescript
function encodeBase64(bytes: Uint8Array): string;
function decodeBase64(text: string): Uint8Array;
```

标准 base64（RFC 4648 §4，带填充）—— 不是 base64url，因为后者那两个不同的字符会静默解出与发送方本意不同的字节。

`decodeBase64` 沿用 `atob` 的宽容度（忽略 ASCII 空白、填充可省），遇到不是合法 base64 的输入会**抛错**：一段损坏的字节负载解成乱码，比当场停下来更糟。

### nowMs()

```typescript
function nowMs(): number;
```

沙箱里有 `performance.now()` 就用它，没有就退到 `Date.now()`。退化后的精度约 1 毫秒 —— 做帧率控制和诊断足够，但不要当作精确时间戳放到 wire 上。
