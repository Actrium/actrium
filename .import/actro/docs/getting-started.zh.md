# 快速开始

本指南将帮助你在项目中开始使用 askit。

## 前置要求

- Node.js 18+
- React 19+ / React Native 0.82+ (用于 Host App)
- rill 引擎 (用于 Guest 沙箱)

## 安装

```bash
bun add github:GoAskAway/askit
```

> **注意**：askit 通过 GitHub 发布，直接导出 TypeScript 源码（无需构建步骤）。你的打包工具（Metro、Vite 等）需要支持 TypeScript 编译。React Native Metro 和现代打包工具都会自动处理。

## 基本设置

### 插件开发

在插件代码中，直接导入并使用 API：

```typescript
import { EventEmitter, Toast, Haptic } from 'askit';

// 初始化插件
EventEmitter.emit('guest:init', { name: 'MyGuest', version: '1.0.0' });

// 监听 Host 事件
EventEmitter.on('host:ready', () => {
  Toast.show('插件已连接!');
});
```

### Host App 集成

在 React Native Host App 中：

```typescript
import { createEngineAdapter, components } from 'askit/core';
import { Engine } from 'rill';

// 创建并配置引擎
const engine = new Engine();

// 连接 askit 桥接
const adapter = createEngineAdapter(engine);

// 注册所有 askit 组件
engine.register(components);

// 加载并运行插件
await engine.loadGuest('https://example.com/guest.js');
```

## 项目结构

典型的 AskAway 项目结构：

```
my-askaway-app/
├── host/                 # React Native Host App
│   ├── src/
│   │   ├── App.tsx
│   │   └── engine/
│   │       └── setup.ts  # 引擎配置
│   └── package.json
├── guests/              # 插件项目
│   └── my-guest/
│       ├── src/
│       │   └── index.ts
│       └── package.json
└── package.json
```

## 下一步

- [API 参考](./api-reference.md) - 了解 EventEmitter、Toast 和 Haptic API
- [组件](./components.md) - 探索 UI 组件
- [架构](./architecture.md) - 理解同构设计
