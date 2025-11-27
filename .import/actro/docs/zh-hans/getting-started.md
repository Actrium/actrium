# 快速开始

本指南将帮助你在项目中开始使用 askit。

## 前置要求

- Node.js 18+
- React Native 0.72+ (用于 Host App)
- rill 引擎 (用于 Plugin 沙箱)

## 安装

```bash
npm install askit
```

## 基本设置

### 插件开发

在插件代码中，直接导入并使用 API：

```typescript
import { Bus, Toast, Haptic } from 'askit';

// 初始化插件
Bus.emit('plugin:init', { name: 'MyPlugin', version: '1.0.0' });

// 监听 Host 事件
Bus.on('host:ready', () => {
  Toast.show('插件已连接!');
});
```

### Host App 集成

在 React Native Host App 中：

```typescript
import { createEngineAdapter, AskitComponents, AskitModules } from 'askit/core';
import { Engine } from 'rill';

// 创建并配置引擎
const engine = new Engine();

// 连接 askit 桥接
const adapter = createEngineAdapter(engine);

// 注册所有 askit 组件
Object.entries(AskitComponents).forEach(([name, Component]) => {
  engine.registerComponent(name, Component);
});

// 加载并运行插件
await engine.loadPlugin('https://example.com/plugin.js');
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
├── plugins/              # 插件项目
│   └── my-plugin/
│       ├── src/
│       │   └── index.ts
│       └── package.json
└── package.json
```

## 下一步

- [API 参考](./api-reference.md) - 了解 Bus、Toast 和 Haptic API
- [组件](./components.md) - 探索 UI 组件
- [架构](./architecture.md) - 理解同构设计
