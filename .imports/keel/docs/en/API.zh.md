# Rill API 文档

## 概述

Rill 是一个轻量级的 React Native 动态 UI 渲染引擎，允许在沙箱环境中运行 React 组件并将渲染结果传递到宿主应用。

## 模块结构

```
rill/
├── sdk          # 插件端 SDK (在沙箱中运行)
├── runtime      # 宿主端运行时
├── reconciler   # React 协调器
└── devtools     # 调试工具
```

---

## SDK (rill/sdk)

插件开发者使用的 SDK，在 QuickJS 沙箱中运行。

### 虚组件

虚组件是字符串标识符，在打包时被 JSX 转换为操作指令。

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity, TextInput, FlatList, Button, Switch, ActivityIndicator } from 'rill/sdk';
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
| onScrollBeginDrag | () => void | 开始拖动回调 |
| onScrollEndDrag | () => void | 结束拖动回调 |

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
| ItemSeparatorComponent | ComponentType | 分隔组件 |
| ListHeaderComponent | ReactElement | 头部组件 |
| ListFooterComponent | ReactElement | 尾部组件 |
| ListEmptyComponent | ReactElement | 空列表组件 |
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
| keyboardType | 'default' \| 'numeric' \| 'email-address' \| 'phone-pad' | 键盘类型 |
| secureTextEntry | boolean | 密码模式 |
| multiline | boolean | 多行输入 |
| maxLength | number | 最大长度 |
| autoFocus | boolean | 自动聚焦 |
| editable | boolean | 是否可编辑 |
| onFocus | () => void | 聚焦回调 |
| onBlur | () => void | 失焦回调 |
| onSubmitEditing | () => void | 提交回调 |

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
| trackColor | { false: string; true: string } | 轨道颜色 |
| thumbColor | string | 滑块颜色 |

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
import { useHostEvent } from 'rill/sdk';

function Plugin() {
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
import { useConfig } from 'rill/sdk';

interface Config {
  theme: 'light' | 'dark';
  userId: string;
}

function Plugin() {
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
import { useSendToHost } from 'rill/sdk';

function Plugin() {
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

---

## Runtime (rill/runtime)

宿主端运行时，负责执行沙箱和渲染 UI。

### Engine

沙箱引擎，管理 QuickJS 执行环境。

```tsx
import { Engine } from 'rill/runtime';

const engine = new Engine({
  timeout: 5000,
  debug: true,
  logger: customLogger,
});

// 注册自定义组件
engine.register({
  StepList: NativeStepList,
  CustomButton: MyButton,
});

// 加载并执行插件
await engine.loadBundle('https://cdn.example.com/plugin.js', {
  theme: 'dark',
  userId: '12345',
});

// 监听事件
engine.on('load', () => console.log('Plugin loaded'));
engine.on('error', (error) => console.error('Plugin error:', error));
engine.on('operation', (batch) => console.log('Operations:', batch));
engine.on('destroy', () => console.log('Plugin destroyed'));

// 发送事件到沙箱
engine.sendEvent('REFRESH', { force: true });

// 更新配置
engine.updateConfig({ theme: 'light' });

// 销毁引擎
engine.destroy();
```

#### 构造函数选项

| 选项 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| timeout | number | 5000 | 执行超时 (ms) |
| debug | boolean | false | 调试模式 |
| logger | Logger | console | 日志处理器 |

#### 方法

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| register | components: ComponentMap | void | 注册组件 |
| loadBundle | source: string, props?: object | Promise\<void\> | 加载插件 |
| sendEvent | eventName: string, payload?: unknown | void | 发送事件 |
| updateConfig | config: object | void | 更新配置 |
| destroy | - | void | 销毁引擎 |

#### 属性

| 属性 | 类型 | 说明 |
|------|------|------|
| isLoaded | boolean | 是否已加载 |
| isDestroyed | boolean | 是否已销毁 |

### EngineView

渲染引擎输出的 React Native 组件。

```tsx
import { EngineView } from 'rill/runtime';

function PluginContainer() {
  return (
    <EngineView
      source="https://cdn.example.com/plugin.js"
      initialProps={{ theme: 'dark' }}
      components={{ CustomButton: MyButton }}
      onLoad={() => console.log('Loaded')}
      onError={(error) => console.error(error)}
      fallback={<Text>Loading...</Text>}
      debug={true}
    />
  );
}
```

#### Props

| 属性 | 类型 | 必须 | 说明 |
|------|------|------|------|
| source | string | 是 | Bundle URL 或代码 |
| initialProps | object | 否 | 初始属性 |
| components | ComponentMap | 否 | 自定义组件 |
| onLoad | () => void | 否 | 加载完成回调 |
| onError | (error: Error) => void | 否 | 错误回调 |
| fallback | ReactElement | 否 | 加载中占位 |
| debug | boolean | 否 | 调试模式 |

### ComponentRegistry

组件注册表，管理组件白名单。

```tsx
import { ComponentRegistry, createRegistry } from 'rill/runtime';

// 使用工厂函数
const registry = createRegistry({
  View: RNView,
  Text: RNText,
  Image: RNImage,
});

// 或使用类
const registry = new ComponentRegistry();
registry.register('CustomCard', MyCard);
registry.registerAll({ Header, Footer });

// 查询组件
const Component = registry.get('View');
const hasComponent = registry.has('CustomCard');
const allNames = registry.getAll();
```

### Receiver

指令接收器，解析操作并构建组件树。

```tsx
import { Receiver } from 'rill/runtime';

const receiver = new Receiver(
  registry,
  (message) => engine.sendToSandbox(message),
  () => forceUpdate()
);

// 应用操作批次
receiver.applyBatch(batch);

// 渲染组件树
const tree = receiver.render();

// 获取调试信息
const debugInfo = receiver.getDebugInfo();
```

---

## Performance (rill/runtime)

性能优化工具。

### ThrottledScheduler

节流调度器，控制更新频率。

```tsx
import { ThrottledScheduler } from 'rill/runtime';

const scheduler = new ThrottledScheduler(
  (batch) => receiver.applyBatch(batch),
  {
    maxBatchSize: 100,
    throttleMs: 16,
    enableMerge: true,
  }
);

// 添加操作
scheduler.enqueue(operation);
scheduler.enqueueAll(operations);

// 立即刷新
scheduler.flush();

// 清理
scheduler.dispose();
```

### VirtualScrollCalculator

虚拟滚动计算器，优化长列表渲染。

```tsx
import { VirtualScrollCalculator } from 'rill/runtime';

const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 50,
  overscan: 5,
  scrollThrottleMs: 16,
});

calculator.setTotalItems(1000);
calculator.setItemHeight(0, 100); // 记录实际高度

const state = calculator.calculate(scrollTop, viewportHeight);
// state: { startIndex, endIndex, offsetTop, offsetBottom, visibleItems }
```

### PerformanceMonitor

性能监控器。

```tsx
import { PerformanceMonitor } from 'rill/runtime';

const monitor = new PerformanceMonitor();

monitor.recordBatch(batch, originalCount);

const metrics = monitor.getMetrics();
// metrics: {
//   totalOperations, totalBatches, avgBatchSize,
//   mergedOperations, createCount, updateCount, deleteCount
// }

monitor.reset();
```

---

## DevTools (rill/devtools)

调试工具集。

### DevTools (主类)

整合所有调试功能。

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools({
  inspector: { maxDepth: 10, showFunctions: true },
  maxLogs: 100,
  maxTimelineEvents: 500,
});

// 启用/禁用
devtools.enable();
devtools.disable();

// 记录事件
devtools.onBatch(batch, duration);
devtools.onCallback(fnId, args);
devtools.onHostEvent(eventName, payload);

// 获取组件树
const tree = devtools.getComponentTree(nodeMap, rootChildren);
const treeText = devtools.getComponentTreeText(nodeMap, rootChildren);

// 导出调试数据
const data = devtools.exportAll();

// 重置
devtools.reset();
```

### ComponentInspector

组件树检查器。

```tsx
import { ComponentInspector } from 'rill/devtools';

const inspector = new ComponentInspector({
  maxDepth: 10,
  filterProps: ['style'],
  showFunctions: false,
  highlightChanges: true,
});

const tree = inspector.buildTree(nodeMap, rootChildren);
const text = inspector.toText(tree);
const json = inspector.toJSON(tree);

inspector.recordChange(nodeId);
inspector.clearHighlights();
```

### OperationLogger

操作日志记录器。

```tsx
import { OperationLogger } from 'rill/devtools';

const logger = new OperationLogger(100);

logger.log(batch, duration);

const logs = logger.getLogs();
const recent = logger.getRecentLogs(10);
const creates = logger.filterByType('CREATE');
const nodeOps = logger.filterByNodeId(1);
const stats = logger.getStats();

logger.clear();
const exported = logger.export();
```

### TimelineRecorder

时间线记录器。

```tsx
import { TimelineRecorder } from 'rill/devtools';

const timeline = new TimelineRecorder(500);

timeline.recordMount(nodeId, type);
timeline.recordUpdate(nodeId, changedProps);
timeline.recordUnmount(nodeId);
timeline.recordBatch(batchId, count, duration);
timeline.recordCallback(fnId, args);
timeline.recordHostEvent(eventName, payload);

const events = timeline.getEvents();
const rangeEvents = timeline.getEventsInRange(0, 1000);
const mounts = timeline.getEventsByType('mount');

timeline.reset();
const exported = timeline.export();
```

---

## CLI (rill/cli)

命令行工具，用于构建插件。

### 构建命令

```bash
# 构建插件
npx rill build src/plugin.tsx -o dist/bundle.js

# 监听模式
npx rill build src/plugin.tsx -o dist/bundle.js --watch

# 生成 sourcemap
npx rill build src/plugin.tsx -o dist/bundle.js --sourcemap

# 不压缩
npx rill build src/plugin.tsx -o dist/bundle.js --no-minify

# 生成 metafile
npx rill build src/plugin.tsx -o dist/bundle.js --metafile dist/meta.json
```

### 分析命令

```bash
# 分析 bundle
npx rill analyze dist/bundle.js
```

### 编程接口

```tsx
import { build, analyze } from 'rill/cli';

await build({
  entry: 'src/plugin.tsx',
  outfile: 'dist/bundle.js',
  minify: true,
  sourcemap: false,
  watch: false,
  metafile: 'dist/meta.json',
});

await analyze('dist/bundle.js');
```

---

## 类型定义

### Operation

操作指令类型。

```typescript
type OperationType =
  | 'CREATE'
  | 'UPDATE'
  | 'DELETE'
  | 'APPEND'
  | 'INSERT'
  | 'REMOVE'
  | 'REORDER'
  | 'TEXT';

interface CreateOperation {
  op: 'CREATE';
  id: number;
  type: string;
  props: SerializedProps;
  timestamp?: number;
}

interface UpdateOperation {
  op: 'UPDATE';
  id: number;
  props: SerializedProps;
  removedProps?: string[];
  timestamp?: number;
}

interface DeleteOperation {
  op: 'DELETE';
  id: number;
  timestamp?: number;
}

// ... 更多操作类型
```

### OperationBatch

操作批次。

```typescript
interface OperationBatch {
  version: number;
  batchId: number;
  operations: Operation[];
}
```

### HostMessage

宿主消息类型。

```typescript
type HostMessageType =
  | 'CALL_FUNCTION'
  | 'HOST_EVENT'
  | 'CONFIG_UPDATE'
  | 'DESTROY';

interface CallFunctionMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: unknown[];
}

interface HostEventMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: unknown;
}

// ... 更多消息类型
```

### StyleProp

样式类型。

```typescript
interface StyleObject {
  // 布局
  flex?: number;
  flexDirection?: 'row' | 'column' | 'row-reverse' | 'column-reverse';
  justifyContent?: 'flex-start' | 'flex-end' | 'center' | 'space-between' | 'space-around' | 'space-evenly';
  alignItems?: 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'baseline';

  // 尺寸
  width?: number | string;
  height?: number | string;
  minWidth?: number | string;
  maxWidth?: number | string;

  // 边距
  margin?: number;
  padding?: number;

  // 边框
  borderWidth?: number;
  borderColor?: string;
  borderRadius?: number;

  // 背景
  backgroundColor?: string;

  // 文字
  color?: string;
  fontSize?: number;
  fontWeight?: 'normal' | 'bold' | '100' | '200' | ... | '900';
  textAlign?: 'auto' | 'left' | 'right' | 'center' | 'justify';

  // ... 更多样式属性
}

type StyleProp = StyleObject | StyleObject[] | null | undefined;
```

---

## 错误处理

### 沙箱错误隔离

插件中的错误不会影响宿主应用：

```tsx
// 插件代码中的错误会被捕获
function BuggyPlugin() {
  throw new Error('Plugin crashed!');
}

// 宿主端处理
engine.on('error', (error) => {
  console.error('Plugin error:', error);
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
  // 只注册需要的组件
  Card: MyCard,
  Badge: MyBadge,
});
```

### 2. 性能优化

```tsx
// 使用节流调度器
const scheduler = new ThrottledScheduler(onBatch, {
  maxBatchSize: 50,
  throttleMs: 16,
  enableMerge: true,
});

// 长列表使用虚拟滚动
const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 60,
  overscan: 3,
});
```

### 3. 调试

```tsx
// 开发环境启用 DevTools
if (__DEV__) {
  const devtools = createDevTools();
  devtools.enable();
}
```

### 4. 错误边界

```tsx
<EngineView
  source={bundleUrl}
  onError={(error) => {
    reportError(error);
  }}
  fallback={<ErrorFallback />}
/>
```
