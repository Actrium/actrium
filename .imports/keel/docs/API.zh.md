# Rill API 文档

## 概述

Rill 是一个轻量级的 React Native 动态 UI 渲染引擎，允许在沙箱环境中运行 React 组件并将渲染结果传递到宿主应用。

## 包导出

```
rill/
├── (default)       # 宿主运行时 (Engine, EngineView, Receiver)
├── /let            # Guest SDK (组件、Hooks)
├── /devtools       # 开发工具
├── /sandbox        # 沙箱提供者
├── /sandbox-native # 原生沙箱 (JSC/QuickJS)
├── /sandbox-web    # Web 沙箱 (Worker)
└── /cli            # CLI 构建工具
```

---

## Guest SDK (rill/let)

Guest 开发者使用的 SDK，在沙箱环境中运行。

### 虚组件

虚组件是字符串标识符，在打包时被 JSX 转换为操作指令。

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity, TextInput, FlatList, Button, Switch, ActivityIndicator } from 'rill/let';
```

#### View

容器组件。

```tsx
<View style={{ flex: 1, padding: 16 }}>
  {children}
</View>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| style | StyleProp | 样式对象 |
| testID | string | 测试标识 |
| onLayout | (event: LayoutEvent) => void | 布局回调 |

#### Text

文本组件。

```tsx
<Text numberOfLines={2} style={{ fontSize: 16 }}>
  Hello World
</Text>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| style | StyleProp | 样式对象 |
| numberOfLines | number | 最大行数 |
| ellipsizeMode | 'head' \| 'middle' \| 'tail' \| 'clip' | 截断模式 |
| selectable | boolean | 是否可选中 |
| onPress | () => void | 点击回调 |

#### Image

图片组件。

```tsx
<Image
  source={{ uri: 'https://example.com/image.png' }}
  style={{ width: 100, height: 100 }}
  resizeMode="cover"
/>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| source | ImageSource | 图片源 |
| style | StyleProp | 样式对象 |
| resizeMode | 'cover' \| 'contain' \| 'stretch' \| 'center' | 缩放模式 |
| onLoad | () => void | 加载完成回调 |
| onError | () => void | 加载失败回调 |

#### TouchableOpacity

可点击组件。

```tsx
<TouchableOpacity onPress={() => console.log('pressed')} activeOpacity={0.7}>
  <Text>Click me</Text>
</TouchableOpacity>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| onPress | () => void | 点击回调 |
| onLongPress | () => void | 长按回调 |
| activeOpacity | number | 按下时透明度 |
| disabled | boolean | 是否禁用 |

#### ScrollView

滚动容器。

```tsx
<ScrollView horizontal showsHorizontalScrollIndicator={false}>
  {items.map(item => <Item key={item.id} />)}
</ScrollView>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| horizontal | boolean | 是否水平滚动 |
| showsVerticalScrollIndicator | boolean | 显示垂直滚动条 |
| showsHorizontalScrollIndicator | boolean | 显示水平滚动条 |
| onScroll | (event: ScrollEvent) => void | 滚动回调 |

#### FlatList

高性能列表组件。

```tsx
<FlatList
  data={items}
  renderItem={({ item }) => <Item data={item} />}
  keyExtractor={item => item.id}
/>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| data | T[] | 数据数组 |
| renderItem | (info: { item: T; index: number }) => ReactElement | 渲染函数 |
| keyExtractor | (item: T, index: number) => string | 键提取函数 |
| horizontal | boolean | 是否水平布局 |
| onEndReached | () => void | 滚动到底部回调 |
| onEndReachedThreshold | number | 触发阈值 |

#### TextInput

文本输入组件。

```tsx
<TextInput
  value={text}
  onChangeText={setText}
  placeholder="Enter text"
  keyboardType="default"
/>
```

| 属性 | 类型 | 说明 |
|------|------|------|
| value | string | 文本值 |
| onChangeText | (text: string) => void | 文本变化回调 |
| placeholder | string | 占位文本 |
| secureTextEntry | boolean | 密码模式 |
| multiline | boolean | 多行输入 |
| maxLength | number | 最大长度 |

#### Button

按钮组件。

```tsx
<Button title="Submit" onPress={handleSubmit} disabled={loading} />
```

| 属性 | 类型 | 说明 |
|------|------|------|
| title | string | 按钮文字 |
| onPress | () => void | 点击回调 |
| disabled | boolean | 是否禁用 |
| color | string | 按钮颜色 |

#### Switch

开关组件。

```tsx
<Switch value={enabled} onValueChange={setEnabled} />
```

| 属性 | 类型 | 说明 |
|------|------|------|
| value | boolean | 当前值 |
| onValueChange | (value: boolean) => void | 值变化回调 |
| disabled | boolean | 是否禁用 |

#### ActivityIndicator

加载指示器。

```tsx
<ActivityIndicator size="large" color="#0066cc" />
```

| 属性 | 类型 | 说明 |
|------|------|------|
| size | 'small' \| 'large' | 大小 |
| color | string | 颜色 |
| animating | boolean | 是否动画 |

---

### Hooks

#### useHostEvent

监听宿主事件。

```tsx
import { useHostEvent } from 'rill/let';

function Guest() {
  useHostEvent<{ force: boolean }>('REFRESH', (payload) => {
    console.log('Refreshing...', payload.force);
  });

  return <View />;
}
```

| 参数 | 类型 | 说明 |
|------|------|------|
| eventName | string | 事件名称 |
| callback | (payload: T) => void | 事件回调 |

#### useConfig

获取初始配置。

```tsx
import { useConfig } from 'rill/let';

interface Config {
  theme: 'light' | 'dark';
  userId: string;
}

function Guest() {
  const config = useConfig<Config>();

  return <Text>Theme: {config.theme}</Text>;
}
```

| 返回值 | 类型 | 说明 |
|--------|------|------|
| config | T | 配置对象 |

#### useSendToHost

向宿主发送事件。

```tsx
import { useSendToHost } from 'rill/let';

function Guest() {
  const sendToHost = useSendToHost();

  const handleComplete = () => {
    sendToHost('TASK_COMPLETE', { taskId: '123', result: 'success' });
  };

  return <Button title="Complete" onPress={handleComplete} />;
}
```

| 返回值 | 类型 | 说明 |
|--------|------|------|
| sendToHost | (eventName: string, payload?: unknown) => void | 发送函数 |

#### useRemoteRef

创建远程引用，用于调用 Host 组件实例方法。

```tsx
import { useRemoteRef, TextInput, TextInputRef } from 'rill/let';

function Guest() {
  const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();

  const handleFocus = async () => {
    await remoteInput?.invoke('focus');
  };

  return (
    <View>
      <TextInput ref={inputRef} placeholder="输入文本" />
      <TouchableOpacity onPress={handleFocus}>
        <Text>聚焦输入框</Text>
      </TouchableOpacity>
    </View>
  );
}
```

| 返回值 | 类型 | 说明 |
|--------|------|------|
| refCallback | RemoteRefCallback | 传递给组件 ref 属性的回调 |
| remoteRef | RemoteRef\<T\> \| null | 远程引用对象（挂载后可用） |

**RemoteRef 接口：**

| 属性/方法 | 类型 | 说明 |
|-----------|------|------|
| nodeId | number | 节点 ID |
| invoke | (method: string, ...args: unknown[]) => Promise\<R\> | 调用远程方法 |
| call | Proxy | 类型安全的方法代理 |

**预定义 Ref 类型：**

- `TextInputRef`: `focus()`, `blur()`, `clear()`
- `ScrollViewRef`: `scrollTo()`, `scrollToEnd()`
- `FlatListRef`: `scrollToIndex()`, `scrollToOffset()`

---

### RillErrorBoundary

Guest 端错误边界组件，捕获渲染错误。

```tsx
import { RillErrorBoundary, View, Text } from 'rill/let';

function App() {
  return (
    <RillErrorBoundary
      fallback={<Text>发生错误</Text>}
      onError={(error, info) => {
        console.error('渲染错误:', error, info.componentStack);
      }}
    >
      <MyComponent />
    </RillErrorBoundary>
  );
}
```

| 属性 | 类型 | 说明 |
|------|------|------|
| children | ReactNode | 子组件 |
| fallback | ReactNode \| ((error, info) => ReactNode) | 错误时显示的内容 |
| onError | (error: Error, info: ErrorInfo) => void | 错误回调 |

---

## 宿主运行时 (rill)

宿主端运行时，负责执行沙箱和渲染 UI。

### Engine

沙箱引擎，管理 JS 执行环境。

```tsx
import { Engine } from 'rill';

const engine = new Engine({
  timeout: 5000,
  debug: true,
});

// 注册自定义组件
engine.register({
  StepList: NativeStepList,
  CustomButton: MyButton,
});

// 加载并执行 Guest
await engine.loadBundle('https://cdn.example.com/guest.js', {
  theme: 'dark',
  userId: '12345',
});

// 监听事件
engine.on('load', () => console.log('Guest loaded'));
engine.on('error', (error) => console.error('Guest error:', error));
engine.on('message', (msg) => console.log('Guest message:', msg));
engine.on('destroy', () => console.log('Guest destroyed'));

// 发送事件到沙箱
engine.sendEvent('REFRESH', { force: true });

// 健康检查
const health = engine.getHealth();

// 销毁引擎
engine.destroy();
```

#### 构造函数选项

| 选项 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| provider | JSEngineProvider | auto | JS 引擎 Provider |
| timeout | number | 5000 | 执行超时 (ms) |
| debug | boolean | false | 调试模式 |
| logger | { log, warn, error } | console | 自定义日志处理器 |
| requireWhitelist | string[] | ['react', ...] | 允许 require 的模块白名单 |
| onMetric | (name, value, extra?) => void | - | 性能指标回调 |
| receiverMaxBatchSize | number | 5000 | 每批次最大操作数 |

#### 方法

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| register | components: ComponentMap | void | 注册自定义组件 |
| loadBundle | source: string, props?: object | Promise\<void\> | 加载并执行 Guest Bundle |
| sendEvent | eventName: string, payload?: unknown | void | 发送事件到 Guest |
| updateConfig | config: object | void | 更新 Guest 配置 |
| on | event: keyof EngineEvents, handler: Function | () => void | 订阅引擎事件 |
| getHealth | - | EngineHealth | 获取引擎健康状态 |
| getReceiver | - | Receiver \| null | 获取当前接收器 |
| getRegistry | - | ComponentRegistry | 获取组件注册表 |
| destroy | - | void | 销毁引擎并释放所有资源 |

#### 属性

| 属性 | 类型 | 说明 |
|------|------|------|
| id | string (只读) | 引擎唯一标识符 |
| loaded | boolean | Bundle 是否已加载 |
| destroyed | boolean | 引擎是否已销毁 |

### EngineView

渲染引擎输出的 React Native 组件。

```tsx
import { Engine, EngineView } from 'rill';

function GuestContainer() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  useEffect(() => {
    engine.register({ StepList: NativeStepList });
  }, [engine]);

  return (
    <EngineView
      engine={engine}
      source="https://cdn.example.com/bundle.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Loaded')}
      onError={(error) => console.error(error)}
      fallback={<ActivityIndicator />}
      renderError={(error) => <Text>Error: {error.message}</Text>}
      style={{ flex: 1 }}
    />
  );
}
```

#### Props

| 属性 | 类型 | 必须 | 说明 |
|------|------|------|------|
| engine | Engine | 是 | Engine 实例 |
| source | string | 是 | Bundle URL 或代码 |
| initialProps | object | 否 | 初始属性 |
| onLoad | () => void | 否 | 加载完成回调 |
| onError | (error: Error) => void | 否 | 错误回调 |
| onDestroy | () => void | 否 | 销毁回调 |
| fallback | ReactNode | 否 | 加载中占位 |
| renderError | (error: Error) => ReactNode | 否 | 错误渲染函数 |
| style | object | 否 | 容器样式 |

### ComponentRegistry

组件注册表，管理组件白名单。

```tsx
import { ComponentRegistry } from 'rill';

const registry = new ComponentRegistry();
registry.register('CustomCard', MyCard);
registry.registerAll({ Header, Footer });

// 查询组件
const Component = registry.get('View');
const hasComponent = registry.has('CustomCard');
```

### Receiver

指令接收器，解析操作并构建组件树。

```tsx
import { Receiver } from 'rill';

const receiver = new Receiver(
  registry,
  (message) => engine.sendToSandbox(message),
  () => forceUpdate()
);

// 应用操作批次
receiver.applyBatch(batch);

// 渲染组件树
const tree = receiver.render();
```

---

## DevTools (rill/devtools)

Rill 应用的开发和调试工具集。

### createDevTools

创建 DevTools 实例。

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools({
  runtime: {
    maxLogs: 100,
    maxTimelineEvents: 500,
  },
});

// 启用/禁用
devtools.enable();
devtools.disable();

// 连接引擎
devtools.connectEngine(engine);

// 获取组件树
const tree = devtools.getHostTree();

// 导出调试数据
const data = devtools.export();

// 重置所有数据
devtools.reset();
```

---

## CLI (rill/cli)

命令行工具，用于构建 Guest Bundle。

### 构建命令

```bash
# 构建 Guest Bundle
bun run rill/cli build src/guest.tsx -o dist/bundle.js

# 监听模式
bun run rill/cli build src/guest.tsx --watch --no-minify --sourcemap

# 分析 Bundle
bun run rill/cli analyze dist/bundle.js
```

### 编程接口

```tsx
import { build, analyze } from 'rill/cli';

await build({
  entry: 'src/guest.tsx',
  outfile: 'dist/bundle.js',
  minify: true,
  sourcemap: false,
  watch: false,
  strict: true,  // 启用严格依赖检查（默认）
});

await analyze('dist/bundle.js', {
  whitelist: ['react', 'react-native', 'react/jsx-runtime', '@rill/let'],
  failOnViolation: true,
});
```

---

## 类型定义

### EngineOptions

```typescript
interface EngineOptions {
  provider?: JSEngineProvider;          // 自定义 Provider
  timeout?: number;                     // 执行超时（默认 5000ms）
  debug?: boolean;                      // 调试模式
  logger?: {
    log: (...args: unknown[]) => void;
    warn: (...args: unknown[]) => void;
    error: (...args: unknown[]) => void;
  };
  onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
  requireWhitelist?: string[];          // 允许的 require() 模块
  receiverMaxBatchSize?: number;        // 每批次最大操作数（默认 5000）
}
```

### EngineEvents

```typescript
interface EngineEvents {
  load: () => void;                      // Bundle 成功加载
  error: (error: Error) => void;         // Guest 运行时错误
  fatalError: (error: Error) => void;    // 致命错误 - 引擎自动销毁
  destroy: () => void;                   // 引擎已销毁
  operation: (batch: OperationBatch) => void;  // 收到操作批次
  message: (message: GuestMessage) => void;    // 收到 Guest 消息
}
```

### EngineHealth

```typescript
interface EngineHealth {
  loaded: boolean;         // Bundle 是否已加载
  destroyed: boolean;      // 引擎是否已销毁
  errorCount: number;      // 总错误数
  lastErrorAt: number | null;  // 上次错误时间戳
  receiverNodes: number;   // Receiver 中的节点数
  batching: boolean;       // 批处理是否激活
}
```

### OperationBatch

```typescript
interface OperationBatch {
  version: number;          // 协议版本
  batchId: number;          // 批次标识符
  operations: Operation[];  // 操作数组
}
```

### Operation 类型

```typescript
type OperationType =
  | 'CREATE'    // 创建新节点
  | 'UPDATE'    // 更新节点属性
  | 'DELETE'    // 删除节点
  | 'APPEND'    // 添加子节点
  | 'INSERT'    // 在索引处插入子节点
  | 'REMOVE'    // 移除子节点
  | 'REORDER'   // 重排子节点
  | 'TEXT'      // 更新文本内容
  | 'REF_CALL'; // 远程方法调用（Remote Ref）

interface CreateOperation {
  op: 'CREATE';
  id: number;
  type: string;
  props: SerializedProps;
}

interface UpdateOperation {
  op: 'UPDATE';
  id: number;
  props: SerializedProps;
  removedProps?: string[];
}

// ... 其他操作类型
```

### HostMessage 类型

```typescript
type HostMessageType =
  | 'CALL_FUNCTION'      // 调用回调函数
  | 'HOST_EVENT'         // 宿主事件广播
  | 'CONFIG_UPDATE'      // 配置更新
  | 'DESTROY'            // 销毁信号
  | 'REF_METHOD_RESULT'; // Remote Ref 方法调用结果
```

### JSEngineProvider

```typescript
interface JSEngineProvider {
  createRuntime(): Promise<JSEngineRuntime>;
}

interface JSEngineRuntime {
  createContext(): Promise<JSEngineContext>;
}

interface JSEngineContext {
  eval(code: string): Promise<unknown>;
  set(name: string, value: unknown): void;
  get(name: string): unknown;
  dispose(): void;
}
```

---

## 错误处理

### 沙箱错误隔离

Guest 中的错误不会影响宿主应用：

```tsx
// Guest 代码中的错误会被捕获
function BuggyGuest() {
  throw new Error('Guest crashed!');
}

// 宿主端处理
engine.on('error', (error) => {
  console.error('Guest error:', error);
  // 显示降级 UI
});
```

### 超时保护

```tsx
const engine = new Engine({ timeout: 5000 });

// 5 秒后自动终止无响应的脚本
```

---

## 最佳实践

### 1. 组件白名单

只注册必要的组件：

```tsx
engine.register({
  Card: MyCard,
  Badge: MyBadge,
});
```

### 2. 调试

```tsx
engine.on('error', (error) => {
  console.error('[Guest 错误]', error);
  reportError(error);
});

engine.on('operation', (batch) => {
  console.log(`操作数量: ${batch.operations.length}`);
});
```

### 3. 错误边界

```tsx
<EngineView
  engine={engine}
  source={bundleUrl}
  onError={(error) => {
    reportError(error);
  }}
  renderError={(error) => <ErrorFallback error={error} />}
/>
```
