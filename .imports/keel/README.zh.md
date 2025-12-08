# Rill

Lightweight, headless, sandboxed React Native dynamic UI rendering engine. (Chinese README migrated to README.zh.md under docs as per new policy)

> **rill** /rɪl/ n. 小溪、细流 - 寓意轻量、流畅的数据流动

## 特性 (This file will be deprecated; please refer to docs/en/*.zh.md)

- **类 React 开发体验**：使用 JSX 和 Hooks 编写插件
- **完全沙箱隔离**：基于 QuickJS，插件崩溃不影响宿主
- **轻量高效**：无 WebView 开销，原生渲染性能
- **灵活扩展**：支持注册自定义业务组件

## 快速开始

### 安装

```bash
npm install rill
# 或
yarn add rill
```

### 宿主端集成

```tsx
import { Engine, EngineView } from 'rill';
import { NativeStepList } from './components/NativeStepList';

// 1. 创建引擎实例
const engine = new Engine();

// 2. 注册自定义组件
engine.register({
  StepList: NativeStepList,
});

// 3. 渲染插件
function App() {
  return (
    <EngineView
      engine={engine}
      bundleUrl="https://cdn.example.com/plugin.js"
      initialProps={{ theme: 'dark' }}
      onLoad={() => console.log('Plugin loaded')}
      onError={(err) => console.error('Plugin error:', err)}
    />
  );
}
```

### 插件端开发

```tsx
import { View, Text, TouchableOpacity, useHostEvent, useConfig } from 'rill/sdk';

export default function MyPlugin() {
  const config = useConfig<{ theme: string }>();

  useHostEvent('REFRESH', () => {
    console.log('Host requested refresh');
  });

  return (
    <View style={{ flex: 1, padding: 20 }}>
      <Text style={{ fontSize: 24 }}>Hello from Plugin!</Text>
      <Text>Theme: {config.theme}</Text>
      <TouchableOpacity onPress={() => console.log('Pressed!')}>
        <Text>Click Me</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### 构建插件

```bash
# 安装 CLI
npm install -g rill

# 构建
rill build src/plugin.tsx -o dist/bundle.js

# 开发模式
rill build src/plugin.tsx --watch --no-minify --sourcemap
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
│                    │  Plugin Bundle.js  │                        │
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
| SDK | `rill/sdk` | 插件开发套件，虚组件和 Hooks |
| Runtime | `rill` | 宿主运行时，Engine 和 EngineView |
| CLI | `rill` (bin) | 插件打包工具 |

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

// 加载插件
await engine.loadBundle(bundleUrl, initialProps);

// 发送事件到插件
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

- [API 文档](./docs/en/API.zh.md) - 完整 API 参考
- [使用指南](./docs/en/GUIDE.zh.md) - 入门教程和最佳实践
- [架构设计](./docs/en/ARCHITECTURE.zh.md) - 系统架构详解

## 开发

```bash
# 安装依赖
npm install

# 构建
npm run build

# 开发模式
npm run build:watch

# 类型检查
npm run typecheck

# 测试
npm run test

# 测试覆盖率
npm run test:coverage
```

## 测试

项目包含完整的测试套件：

- 单元测试：各模块功能测试
- 集成测试：端到端场景测试
- 覆盖率目标：80%+ 代码覆盖

```bash
npm test           # 运行所有测试
npm test -- --run  # 单次运行
npm test:coverage  # 生成覆盖率报告
```

## 许可证

Apache-2.0
