# Rill 使用指南

## 简介

Rill 是一个轻量级的 React Native 动态 UI 渲染引擎，类似于 Shopify 的 remote-ui。它允许在安全的沙箱环境中运行 React 组件，并将渲染结果传递到宿主应用显示。

### 核心特性

- **安全沙箱** - 使用 QuickJS 隔离guest代码
- **React 开发体验** - 支持 JSX、Hooks 等现代 React 特性
- **高性能** - 批量更新、操作合并、虚拟滚动
- **类型安全** - 完整的 TypeScript 支持
- **零依赖** - SDK 不依赖 react-native

---

## 快速开始

### 1. 安装

```bash
# 在宿主应用中
bun add rill react-native-quickjs

# 在guest项目中 (仅开发依赖)
bun add -D rill
```

### 2. 创建guest

```tsx
// src/guest.tsx
import { View, Text, TouchableOpacity, useConfig, useSendToHost } from '@rill/core/sdk';

interface Config {
  title: string;
  theme: 'light' | 'dark';
}

export default function MyGuest() {
  const config = useConfig<Config>();
  const sendToHost = useSendToHost();

  const handlePress = () => {
    sendToHost('BUTTON_CLICKED', { timestamp: Date.now() });
  };

  return (
    <View style={{ padding: 16 }}>
      <Text style={{ fontSize: 24 }}>{config.title}</Text>
      <TouchableOpacity onPress={handlePress}>
        <Text>Click me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### 3. 构建guest

```bash
bunx rill build src/guest.tsx -o dist/bundle.js
```

### 4. 在宿主应用中使用

```tsx
// App.tsx
import React from 'react';
import { SafeAreaView, Text } from 'react-native';
import { EngineView } from 'rill/runtime';

export default function App() {
  return (
    <SafeAreaView style={{ flex: 1 }}>
      <EngineView
        source="https://cdn.example.com/guest.js"
        initialProps={{
          title: 'Hello Rill',
          theme: 'light',
        }}
        onLoad={() => console.log('Guest loaded')}
        onError={(error) => console.error('Guest error:', error)}
        fallback={<Text>Loading guest...</Text>}
      />
    </SafeAreaView>
  );
}
```

---

## guest开发

### 项目结构

```
my-guest/
├── src/
│   └── guest.tsx    # guest入口
├── dist/
│   └── bundle.js     # 构建输出
├── package.json
└── tsconfig.json
```

### package.json

```json
{
  "name": "my-guest",
  "version": "1.0.0",
  "scripts": {
    "build": "rill build src/guest.tsx -o dist/bundle.js",
    "watch": "rill build src/guest.tsx -o dist/bundle.js --watch"
  },
  "devDependencies": {
    "rill": "^1.0.0",
    "typescript": "^5.0.0"
  }
}
```

### tsconfig.json

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "paths": {
      "rill/sdk": ["./node_modules/rill/dist/sdk"]
    }
  },
  "include": ["src"]
}
```

### 使用虚组件

虚组件是字符串标识符，在构建时被转换为操作指令：

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity } from '@rill/core/sdk';

function MyComponent() {
  return (
    <View style={{ flex: 1 }}>
      <ScrollView>
        <Image
          source={{ uri: 'https://example.com/image.png' }}
          style={{ width: 100, height: 100 }}
        />
        <Text>Hello World</Text>
        <TouchableOpacity onPress={() => console.log('pressed')}>
          <Text>Click me</Text>
        </TouchableOpacity>
      </ScrollView>
    </View>
  );
}
```

### 使用 Hooks

#### useConfig - 获取配置

```tsx
interface Config {
  userId: string;
  theme: 'light' | 'dark';
  features: string[];
}

function Guest() {
  const config = useConfig<Config>();

  return (
    <View>
      <Text>User: {config.userId}</Text>
      <Text>Theme: {config.theme}</Text>
    </View>
  );
}
```

#### useHostEvent - 监听宿主事件

```tsx
function Guest() {
  const [refreshCount, setRefreshCount] = useState(0);

  useHostEvent<{ force: boolean }>('REFRESH', (payload) => {
    setRefreshCount((c) => c + 1);
    if (payload.force) {
      // 强制刷新逻辑
    }
  });

  return <Text>Refreshed {refreshCount} times</Text>;
}
```

#### useSendToHost - 发送事件到宿主

```tsx
function Guest() {
  const sendToHost = useSendToHost();

  const handleComplete = (result: string) => {
    sendToHost('TASK_COMPLETE', { result, timestamp: Date.now() });
  };

  return (
    <TouchableOpacity onPress={() => handleComplete('success')}>
      <Text>Complete Task</Text>
    </TouchableOpacity>
  );
}
```

### 样式

支持大部分 React Native 样式属性：

```tsx
<View
  style={{
    flex: 1,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    padding: 16,
    margin: 8,
    backgroundColor: '#f5f5f5',
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#ddd',
  }}
>
  <Text
    style={{
      fontSize: 16,
      fontWeight: 'bold',
      color: '#333',
      textAlign: 'center',
    }}
  >
    Styled Text
  </Text>
</View>
```

### 列表渲染

使用 FlatList 渲染长列表：

```tsx
interface Item {
  id: string;
  title: string;
}

function Guest() {
  const [items] = useState<Item[]>([
    { id: '1', title: 'Item 1' },
    { id: '2', title: 'Item 2' },
    { id: '3', title: 'Item 3' },
  ]);

  return (
    <FlatList
      data={items}
      keyExtractor={(item) => item.id}
      renderItem={({ item }) => (
        <View style={{ padding: 16, borderBottomWidth: 1 }}>
          <Text>{item.title}</Text>
        </View>
      )}
      ListHeaderComponent={<Text style={{ fontSize: 24 }}>My List</Text>}
      ListEmptyComponent={<Text>No items</Text>}
    />
  );
}
```

---

## 宿主集成

### 基本集成

```tsx
import React from 'react';
import { View } from 'react-native';
import { EngineView } from 'rill/runtime';

function GuestHost() {
  return (
    <View style={{ flex: 1 }}>
      <EngineView
        source="https://cdn.example.com/guest.js"
        initialProps={{ theme: 'dark' }}
      />
    </View>
  );
}
```

### 自定义组件

注册宿主端的原生组件供guest使用：

```tsx
import { NativeStepList } from './components/NativeStepList';
import { CustomButton } from './components/CustomButton';

function GuestHost() {
  return (
    <EngineView
      source={bundleUrl}
      components={{
        StepList: NativeStepList,
        CustomButton: CustomButton,
      }}
    />
  );
}
```

guest中使用自定义组件：

```tsx
// 在guest中声明自定义组件类型
declare const StepList: string;
declare const CustomButton: string;

function Guest() {
  return (
    <View>
      <StepList steps={['Step 1', 'Step 2', 'Step 3']} />
      <CustomButton title="Submit" variant="primary" />
    </View>
  );
}
```

### 事件通信

#### 宿主 -> guest

```tsx
import { useRef } from 'react';
import { Engine } from 'rill/runtime';

function GuestHost() {
  const engineRef = useRef<Engine>(null);

  const handleRefresh = () => {
    engineRef.current?.sendEvent('REFRESH', { force: true });
  };

  return (
    <View>
      <Button title="Refresh" onPress={handleRefresh} />
      <EngineView
        ref={engineRef}
        source={bundleUrl}
      />
    </View>
  );
}
```

#### guest -> 宿主

在 EngineView 中监听操作事件：

```tsx
import { Engine, EngineView } from 'rill/runtime';

function GuestHost() {
  const handleGuestEvent = (eventName: string, payload: unknown) => {
    switch (eventName) {
      case 'TASK_COMPLETE':
        console.log('Task completed:', payload);
        break;
      case 'NAVIGATION':
        navigation.navigate(payload.route);
        break;
    }
  };

  return (
    <EngineView
      source={bundleUrl}
      onGuestEvent={handleGuestEvent}
    />
  );
}
```

### 使用 Engine API

直接使用 Engine 类获得更多控制：

```tsx
import { Engine, Receiver, ComponentRegistry } from 'rill/runtime';

function useRillEngine(bundleUrl: string, initialProps: object) {
  const [tree, setTree] = useState<React.ReactElement | null>(null);
  const engineRef = useRef<Engine | null>(null);
  const receiverRef = useRef<Receiver | null>(null);

  useEffect(() => {
    const engine = new Engine({ debug: __DEV__ });
    const registry = new ComponentRegistry();

    // 注册默认组件
    registry.registerAll(DefaultComponents);

    // 创建 Receiver
    const receiver = engine.createReceiver(() => {
      setTree(receiver.render());
    });

    engineRef.current = engine;
    receiverRef.current = receiver;

    // 加载guest
    engine.loadBundle(bundleUrl, initialProps).catch(console.error);

    return () => {
      engine.destroy();
    };
  }, [bundleUrl]);

  return { tree, engine: engineRef.current };
}
```

---

## 性能优化

### 批量更新

Rill 自动批量处理更新以优化性能：

```tsx
import { ThrottledScheduler } from 'rill/runtime';

// 自定义节流配置
const scheduler = new ThrottledScheduler(
  (batch) => receiver.applyBatch(batch),
  {
    maxBatchSize: 100,    // 最大批次大小
    throttleMs: 16,       // 约 60fps
    enableMerge: true,    // 启用操作合并
  }
);
```

### 虚拟滚动

对于长列表，使用虚拟滚动优化：

```tsx
import { VirtualScrollCalculator } from 'rill/runtime';

const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 60,  // 预估项目高度
  overscan: 5,              // 可视区外缓冲
  scrollThrottleMs: 16,     // 滚动节流
});

// 计算可视范围
const state = calculator.calculate(scrollTop, viewportHeight);
// 只渲染 state.visibleItems 中的项目
```

### 性能监控

```tsx
import { PerformanceMonitor } from 'rill/runtime';

const monitor = new PerformanceMonitor();

// 记录批次
engine.on('operation', (batch) => {
  monitor.recordBatch(batch);
});

// 查看指标
const metrics = monitor.getMetrics();
console.log(`Total operations: ${metrics.totalOperations}`);
console.log(`Average batch size: ${metrics.avgBatchSize}`);
console.log(`Merged operations: ${metrics.mergedOperations}`);
```

---

## 调试

### 启用 DevTools

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools({
  inspector: { maxDepth: 10 },
  maxLogs: 100,
  maxTimelineEvents: 500,
});

// 在开发环境启用
if (__DEV__) {
  devtools.enable();
}

// 记录事件
engine.on('operation', (batch) => {
  devtools.onBatch(batch);
});
```

### 查看组件树

```tsx
const receiver = engine.getReceiver();
const treeText = devtools.getComponentTreeText(
  receiver.nodeMap,
  receiver.rootChildren
);

console.log(treeText);
// 输出:
// └─ <View testID="root">
//    ├─ <Text numberOfLines={2}>
//    └─ <TouchableOpacity>
//       └─ <Text>
```

### 导出调试数据

```tsx
const debugData = devtools.exportAll();
// 保存或发送到服务器
```

---

## 安全考虑

### 沙箱隔离

- guest代码在 QuickJS 沙箱中运行
- 无法访问宿主的原生 API
- 无法执行网络请求 (除非宿主提供)
- 无法访问文件系统

### 组件白名单

只有显式注册的组件才能被guest使用：

```tsx
// 只注册安全的组件
engine.register({
  View: SafeView,
  Text: SafeText,
  Image: SafeImage,
  // 不注册可能有安全风险的组件
});
```

### 超时保护

防止恶意脚本占用资源：

```tsx
const engine = new Engine({
  timeout: 5000,  // 5 秒超时
});
```

### 错误隔离

guest错误不会崩溃宿主应用：

```tsx
<EngineView
  source={bundleUrl}
  onError={(error) => {
    // 记录错误
    reportError(error);
    // 显示降级 UI
  }}
  fallback={<ErrorFallback />}
/>
```

---

## 常见问题

### 1. guest加载失败

**问题**: `Failed to fetch bundle: 404`

**解决**: 检查 bundle URL 是否正确，确保服务器已启动。

### 2. 组件未显示

**问题**: 控制台显示 `Component "X" not registered`

**解决**: 在宿主端注册该组件：
```tsx
engine.register({ X: MyXComponent });
```

### 3. 样式不生效

**问题**: 样式属性不起作用

**解决**: 确保使用驼峰命名，不是 CSS 格式：
```tsx
// 正确
style={{ backgroundColor: 'red', fontSize: 16 }}

// 错误
style={{ 'background-color': 'red', 'font-size': 16 }}
```

### 4. 回调函数不触发

**问题**: onPress 等事件不响应

**解决**: 检查函数是否正确传递，不要在 JSX 中使用箭头函数包装：
```tsx
// 推荐
<TouchableOpacity onPress={handlePress}>

// 也可以
<TouchableOpacity onPress={() => handlePress()}>
```

### 5. 内存泄漏

**问题**: 内存持续增长

**解决**: 确保在组件卸载时销毁引擎：
```tsx
useEffect(() => {
  return () => engine.destroy();
}, []);
```

---

## 示例项目

完整示例请参考 `examples/` 目录：

- `examples/basic-guest/` - 基础guest示例
- `examples/host-app/` - 宿主应用示例
- `examples/custom-components/` - 自定义组件示例
