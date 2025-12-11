# Rill

Lightweight, headless, sandboxed React Native dynamic UI rendering engine. (Chinese README migrated to README.zh.md under docs as per new policy)

> **rill** /rɪl/ n. 小溪、细流 - 寓意轻量、流畅的数据流动

## 特性 (This file will be deprecated; please refer to docs/en/*.zh.md)

- **类 React 开发体验**：使用 JSX 和 Hooks 编写guest
- **完全沙箱隔离**：基于 QuickJS，guest崩溃不影响宿主
- **轻量高效**：无 WebView 开销，原生渲染性能
- **灵活扩展**：支持注册自定义业务组件

## 快速开始

### 安装

```bash
bun add rill
# 或
yarn add rill
```

### 宿主端集成

```tsx
import { Engine, EngineView } from '@rill/core';
import { NativeStepList } from './components/NativeStepList';

// 1. 创建引擎实例
const engine = new Engine();

// 2. 注册自定义组件
engine.register({
  StepList: NativeStepList,
});

// 3. 渲染guest
function App() {
  return (
    <EngineView
      engine={engine}
      bundleUrl="https://cdn.example.com/guest.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Guest loaded')}
      onError={(err) => console.error('Guest error:', err)}
    />
  );
}
```

### guest端开发

```tsx
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from '@rill/core/sdk';

export default function MyGuest() {
  const config = useConfig<{ theme: string }>();

  useHostEvent('REFRESH', () => {
    console.log('Host requested refresh');
  });

  return (
    <View style={{ flex: 1, padding: 20 }}>
      <Text style={{ fontSize: 24 }}>Hello from Guest!</Text>
      <Text>Theme: {config.theme}</Text>
      <TouchableOpacity onPress={() => console.log('Pressed!')}>
        <Text>Click Me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### 构建guest

```bash
# 安装 CLI
bun add -g rill

# 构建
rill build src/guest.tsx -o dist/bundle.js

# 开发模式
rill build src/guest.tsx --watch --no-minify --sourcemap
```

## 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                       宿主 App (React Native)                    │
├─────────────────────────────────────────────────────────────────┤
│  EngineView → Engine → QuickJS Context                          │
│                            │                                     │
│                            ▼                                     │
│                    ┌───────────────────┐                        │
│                    │  Guest Bundle.js  │                        │
│                    │  (React + SDK)     │                        │
│                    └───────────────────┘                        │
│                            │                                     │
│                            ▼                                     │
│                    Reconciler (JSON Ops)                        │
│                            │                                     │
│                            ▼                                     │
│                    Receiver → Registry → 原生组件树              │
└─────────────────────────────────────────────────────────────────┘
```

## 模块说明

| 模块 | 路径 | 说明 |
|------|------|------|
| SDK | `rill/sdk` | guest开发套件，虚组件和 Hooks |
| Runtime | `rill` | 宿主运行时，Engine 和 EngineView |
| CLI | `rill` (bin) | guest打包工具 |

## API

### Engine

```typescript
const engine = new Engine(options?: EngineOptions);

interface EngineOptions {
  timeout?: number;      // 执行超时 (默认 5000ms)
  debug?: boolean;       // 调试模式
  logger?: Logger;       // 自定义日志
}

// 注册组件
engine.register({ ComponentName: ReactComponent });

// 加载guest
await engine.loadBundle(bundleUrl, initialProps);

// 发送事件到guest
engine.sendEvent('EVENT_NAME', payload);

// 更新配置
engine.updateConfig({ key: value });

// 销毁
engine.destroy();
```

### SDK Hooks

```typescript
// 订阅宿主事件
useHostEvent('EVENT_NAME', (payload) => {
  // 处理事件
});

// 获取初始配置
const config = useConfig<ConfigType>();

// 发送消息到宿主
const send = useSendToHost();
send('EVENT_NAME', payload);
```

### 默认组件

- `View` - 容器组件
- `Text` - 文本组件
- `Image` - 图片组件
- `ScrollView` - 滚动容器
- `TouchableOpacity` - 可触摸组件

## 性能优化


## Host ↔ Guest 事件

- guest（guest）通过 SDK 的 `useHostEvent(event, callback)` 订阅宿主事件。
- 宿主通过 `engine.sendEvent(eventName, payload)` 发送事件。
- 取消订阅：`const off = useHostEvent('EVT', cb); off?.();` 保存返回值，在卸载时调用以移除监听。

guest示例：

```tsx
import * as React from 'react';
import { View, Text } from '@rill/core/sdk';
import { useHostEvent, useSendToHost } from '@rill/core/sdk';

export default function Guest() {
  const send = useSendToHost();
  React.useEffect(() => {
    const off = useHostEvent('PING', (payload: { ok: number }) => {
      // 处理宿主事件
      send('ACK', { got: payload.ok });
    });
    return () => { off && off(); };
  }, []);
  return <View><Text>Ready</Text></View>;
}
```

宿主示例：

```ts
import { Engine } from '@rill/core';
const engine = new Engine({ quickjs });
engine.on('message', (m) => { /* m.event, m.payload */ });
engine.sendEvent('PING', { ok: 1 });
```

说明：
- Engine 在运行时注入 `__useHostEvent`/`__handleHostEvent` 的兜底实现，即使打包产物没有 CLI banner 也能工作。
- 为了性能，请使用稳定回调，并在组件卸载时取消订阅。

## SDK 编译期内联与严格守卫

目标：guest产物在运行时不能 require/import `rill/sdk`，SDK 仅用于类型与编译期。

- 使用 rill CLI（Vite lib build, IIFE）构建，CLI 会设置 alias 使 `rill/sdk` 能被完全内联/摇树。
- 构建后 CLI 会运行严格守卫（analyze），若发现非白名单依赖（如 `rill/sdk`）将直接失败。

命令：

```bash
# 构建（默认开启严格守卫）
rill build src/guest.tsx -o dist/bundle.js

# 分析已构建 bundle
rill analyze dist/bundle.js \
  --fail-on-violation \
  --treat-eval-as-violation \
  --treat-dynamic-non-literal-as-violation
```

运行时白名单：`react`、`react-native`、`react/jsx-runtime`、`rill/reconciler`。

如 bundle 仍包含 `require('@rill/core/sdk')`，analyze 将 fail-fast 并给出提示。

## 宿主集成（正确 API）

```ts
import { Engine } from '@rill/core';
import { DefaultComponents } from 'rill/components';

const engine = new Engine({ quickjs });
engine.register(DefaultComponents);
const receiver = engine.createReceiver(() => {/* 触发宿主刷新 */});
await engine.loadBundle(codeOrUrl);
```

说明：
- 使用 `register(components)`（不是 `registerComponent`）。
- 使用 `loadBundle(source)`（不是 `loadGuest`）。
- 通过 `new Engine({ quickjs })` 提供 QuickJS provider。

## init 模板默认

`rill init` 将生成：
- vite.config.ts：IIFE lib build，external：react/react-native/react/jsx-runtime/rill-reconciler；alias `rill/sdk` 指向 ESM 方便内联。
- tsconfig.json：Bundler 解析、isolatedModules、strict、verbatimModuleSyntax，并为编辑器提供 `rill/sdk` path 类型映射。
- 示例guest使用 `import { View, Text } from '@rill/core/sdk'`。


Rill 内置多种性能优化机制：

```tsx
import {
  ThrottledScheduler,
  VirtualScrollCalculator,
  PerformanceMonitor
} from 'rill/runtime';

// 批量更新节流
const scheduler = new ThrottledScheduler(onBatch, {
  maxBatchSize: 100,
  throttleMs: 16,
  enableMerge: true,
});

// 虚拟滚动
const calculator = new VirtualScrollCalculator({
  estimatedItemHeight: 60,
  overscan: 5,
});

// 性能监控
const monitor = new PerformanceMonitor();
```

## 调试工具

```tsx
import { createDevTools } from 'rill/devtools';

const devtools = createDevTools();
devtools.enable();

// 查看组件树
console.log(devtools.getComponentTreeText(nodeMap, rootChildren));

// 导出调试数据
const data = devtools.exportAll();
```

## 文档 (Deprecated)

- [API 文档](./docs/API.zh.md) - 完整 API 参考
- [使用指南](./docs/GUIDE.zh.md) - 入门教程和最佳实践
- [架构设计](./docs/ARCHITECTURE.zh.md) - 系统架构详解
- [生产环境指南](./docs/PRODUCTION_GUIDE.zh.md) - 生产部署检查清单
- [guest示例](./examples/) - 完整源码示例

## 开发

```bash
# 安装依赖
bun install

# 构建
bun run build

# 开发模式
bun run build:watch

# 类型检查
bun run typecheck

# 测试
bun test

# 测试覆盖率
bun run test:coverage
```

## 测试

项目包含完整的测试套件：

- 单元测试：各模块功能测试
- 集成测试：端到端场景测试
- 覆盖率目标：80%+ 代码覆盖

```bash
bun test            # 运行所有测试
bun test --run  # 单次运行
bun test:coverage  # 生成覆盖率报告
```

## 许可证

Apache-2.0
