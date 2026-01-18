# Rill 使用指南

## 简介

Rill 是一个轻量级的 React Native 动态 UI 渲染引擎，类似于 Shopify 的 remote-ui。它允许在安全的沙箱环境中运行 React 组件，并将渲染结果传递到宿主应用显示。

### 核心特性

- **安全沙箱** - 支持可插拔的 JSEngineProvider (QuickJS, VM, Worker)
- **React 开发体验** - 支持 JSX、Hooks 等现代 React 特性
- **高性能** - 批量更新、操作合并、虚拟滚动
- **类型安全** - 完整的 TypeScript 支持
- **零依赖** - SDK 不依赖 react-native

---

## 快速开始

### 1. 安装

```bash
# 使用 bun
bun add rill

# 使用 npm
npm install rill
```

**Peer 依赖：**
- React 18.2+ 或 19.x
- react-reconciler（匹配你的 React 版本）
- react-native（RN 应用）或 react-dom（Web 应用）

### 2. 创建 Guest

```tsx
// src/guest.tsx
import { View, Text, TouchableOpacity, useConfig, useSendToHost } from 'rill/sdk';

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

### 3. 构建 Guest

```bash
bunx rill build src/guest.tsx -o dist/bundle.js
```

### 4. 在宿主应用中使用

```tsx
// App.tsx
import React, { useMemo } from 'react';
import { SafeAreaView, Text, ActivityIndicator } from 'react-native';
import { Engine, EngineView } from 'rill';

export default function App() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  return (
    <SafeAreaView style={{ flex: 1 }}>
      <EngineView
        engine={engine}
        source="https://cdn.example.com/guest.js"
        initialProps={{
          title: 'Hello Rill',
          theme: 'light',
        }}
        onLoad={() => console.log('Guest loaded')}
        onError={(error) => console.error('Guest error:', error)}
        fallback={<ActivityIndicator />}
      />
    </SafeAreaView>
  );
}
```

---

## Guest 开发

### 项目结构

```
my-guest/
├── src/
│   └── guest.tsx    # Guest 入口
├── dist/
│   └── bundle.js    # 构建输出
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
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

### 使用虚组件

虚组件是字符串标识符，在构建时被转换为操作指令：

```tsx
import { View, Text, Image, ScrollView, TouchableOpacity } from 'rill/sdk';

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
import { useConfig } from 'rill/sdk';

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
import { useState } from 'react';
import { View, Text, useHostEvent } from 'rill/sdk';

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
import { TouchableOpacity, Text, useSendToHost } from 'rill/sdk';

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

#### useRemoteRef - 调用宿主组件方法

调用宿主组件实例的方法（如 `focus()`、`scrollTo()` 等）：

```tsx
import { useRemoteRef, View, TextInput, TouchableOpacity, Text, TextInputRef } from 'rill/sdk';

function Guest() {
  const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();

  const handleFocus = async () => {
    await remoteInput?.invoke('focus');
  };

  const handleClear = async () => {
    await remoteInput?.invoke('clear');
  };

  return (
    <View>
      <TextInput ref={inputRef} placeholder="输入文本" />
      <TouchableOpacity onPress={handleFocus}>
        <Text>聚焦</Text>
      </TouchableOpacity>
      <TouchableOpacity onPress={handleClear}>
        <Text>清空</Text>
      </TouchableOpacity>
    </View>
  );
}
```

可用的 Ref 类型：
- `TextInputRef`: `focus()`, `blur()`, `clear()`
- `ScrollViewRef`: `scrollTo({ x, y, animated })`, `scrollToEnd()`
- `FlatListRef`: `scrollToIndex()`, `scrollToOffset()`

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
import { useState } from 'react';
import { View, Text, FlatList } from 'rill/sdk';

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
import React, { useMemo } from 'react';
import { View } from 'react-native';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  return (
    <View style={{ flex: 1 }}>
      <EngineView
        engine={engine}
        source="https://cdn.example.com/guest.js"
        initialProps={{ theme: 'dark' }}
      />
    </View>
  );
}
```

### 自定义组件

注册宿主端的原生组件供 Guest 使用：

```tsx
import React, { useMemo, useEffect } from 'react';
import { Engine, EngineView } from 'rill';
import { NativeStepList } from './components/NativeStepList';
import { CustomButton } from './components/CustomButton';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  useEffect(() => {
    engine.register({
      StepList: NativeStepList,
      CustomButton: CustomButton,
    });
  }, [engine]);

  return (
    <EngineView
      engine={engine}
      source={bundleUrl}
    />
  );
}
```

Guest 中使用自定义组件：

```tsx
// 在 Guest 中声明自定义组件类型
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

#### 宿主 -> Guest

```tsx
import React, { useMemo } from 'react';
import { Button, View } from 'react-native';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  const handleRefresh = () => {
    engine.sendEvent('REFRESH', { force: true });
  };

  return (
    <View>
      <Button title="Refresh" onPress={handleRefresh} />
      <EngineView engine={engine} source={bundleUrl} />
    </View>
  );
}
```

#### Guest -> 宿主

监听 Engine 的 message 事件：

```tsx
import React, { useMemo, useEffect } from 'react';
import { Engine, EngineView } from 'rill';

function GuestHost() {
  const engine = useMemo(() => new Engine({ debug: __DEV__ }), []);

  useEffect(() => {
    const unsubscribe = engine.on('message', (message) => {
      switch (message.event) {
        case 'TASK_COMPLETE':
          console.log('Task completed:', message.payload);
          break;
        case 'NAVIGATION':
          navigation.navigate(message.payload.route);
          break;
      }
    });

    return unsubscribe;
  }, [engine]);

  return <EngineView engine={engine} source={bundleUrl} />;
}
```

---

## 调试

### 基本调试

使用引擎事件监控运行时行为：

```tsx
import { Engine } from 'rill';

const engine = new Engine({ debug: true });

// 监控错误
engine.on('error', (error) => {
  console.error('[Guest Error]', error);
});

// 监控操作
engine.on('operation', (batch) => {
  console.log(`Operations: ${batch.operations.length}`);
});

// 健康检查
const health = engine.getHealth();
console.log('Health:', health);
```

### DevTools（可选）

高级调试，使用 DevTools 包：

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools();

if (__DEV__) {
  devtools.enable();
  devtools.connectEngine(engine);
}

// 获取组件树
const tree = devtools.getHostTree();

// 导出调试数据
const data = devtools.export();
```

---

## 安全考虑

### 沙箱隔离

- Guest 代码在 JSEngineProvider 沙箱中运行（QuickJS, VM, Worker）
- 无法访问宿主的原生 API
- 无法执行网络请求（除非宿主提供）
- 无法访问文件系统

### 组件白名单

只有显式注册的组件才能被 Guest 使用：

```tsx
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

Guest 错误不会崩溃宿主应用：

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

---

## 常见问题

### 1. Guest 加载失败

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

**解决**: 检查函数是否正确传递：
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
