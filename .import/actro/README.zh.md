# askit

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

[English](./README.md)

**askit** 是 AskAway 平台的同构 UI 工具包，提供统一的 API 和组件，可在 Host App (React Native) 和 Guest 环境 (QuickJS Sandbox) 中无缝运行。

## 架构

askit 采用 **90% 通用 + 10% 核心** 架构：

- **通用 (90%)**：Host 和 Guest 共享相同的 API 接口
- **核心 (10%)**：仅 Host 端使用的桥接和注册模块

```
┌─────────────────────────────────────────────────────────┐
│                      Host App (RN)                      │
│  ┌─────────────────┐  ┌─────────────────────────────┐  │
│  │   askit/core    │  │         askit               │  │
│  │  - registry     │  │  - Bus, Toast, Haptic       │  │
│  │  - bridge       │  │  - StepList, ThemeView...   │  │
│  └────────┬────────┘  └──────────────┬──────────────┘  │
│           │                          │                  │
│           └──────────┬───────────────┘                  │
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │  rill Engine  │                          │
│              └───────┬───────┘                          │
└──────────────────────┼──────────────────────────────────┘
                       │ 消息协议
┌──────────────────────┼──────────────────────────────────┐
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │ QuickJS Sandbox│                         │
│              └───────┬───────┘                          │
│                      │                                  │
│  ┌───────────────────▼───────────────────────────────┐ │
│  │                    askit                          │ │
│  │  - Bus, Toast, Haptic (远程实现)                   │ │
│  │  - StepList, ThemeView... (DSL 生成器)            │ │
│  └───────────────────────────────────────────────────┘ │
│                      Guest                             │
└─────────────────────────────────────────────────────────┘
```

## 安装

```bash
npm install askit
```

## 使用

### 在 Guest 中 (QuickJS Sandbox)

```typescript
import { Bus, Toast, Haptic } from 'askit';
import { StepList, UserAvatar } from 'askit';

// 事件通信
Bus.emit('guest:ready', { version: '1.0.0' });
Bus.on('host:config', (config) => {
  console.log('收到配置:', config);
});

// Toast 通知
Toast.show('来自插件的问候!', { duration: 'short', position: 'bottom' });

// 触觉反馈
Haptic.trigger('light');

// UI 组件 (返回 DSL 供 Host 渲染)
const avatar = UserAvatar({ uri: 'https://example.com/avatar.png', size: 48 });
```

### 在 Host App 中 (React Native)

```typescript
// 导入核心模块用于桥接
import { createEngineAdapter, AskitComponents } from 'askit/core';
import { Engine } from 'rill';

// 创建引擎并连接 askit
const engine = new Engine();
const adapter = createEngineAdapter(engine);

// 注册组件以渲染插件 UI
Object.entries(AskitComponents).forEach(([name, Component]) => {
  engine.registerComponent(name, Component);
});

// 启动插件
engine.loadGuest('./guest.js');
```

## API 参考

### Bus

Host 与 Guest 之间基于事件的通信。

```typescript
// 发送事件
Bus.emit(event: string, payload?: unknown): void

// 监听事件
Bus.on(event: string, handler: (payload: unknown) => void): () => void

// 单次监听
Bus.once(event: string, handler: (payload: unknown) => void): () => void

// 移除监听
Bus.off(event: string, handler?: (payload: unknown) => void): void
```

### Toast

显示 Toast 通知。

```typescript
Toast.show(message: string, options?: {
  duration?: 'short' | 'long' | number;  // 默认: 'short'
  position?: 'top' | 'center' | 'bottom'; // 默认: 'bottom'
}): void
```

### Haptic

触发触觉反馈。

```typescript
Haptic.trigger(type?: 'light' | 'medium' | 'heavy' | 'success' | 'warning' | 'error'): void
```

## 组件

| 组件 | 描述 |
|------|------|
| `StepList` | 带状态指示器的步骤列表 |
| `ThemeView` | 主题感知容器视图 |
| `UserAvatar` | 带降级支持的用户头像 |
| `ChatBubble` | 聊天消息气泡 |

## 模块结构

```
askit
├── index.ts          # 通用导出 (Bus, Toast, Haptic, Components)
└── core/
    ├── registry.ts   # 组件和模块注册
    └── bridge.ts     # Host-Guest 消息桥接
```

### 条件导出

包使用条件导出来提供不同的实现：

```json
{
  "exports": {
    ".": {
      "react-native": "./dist/native/index.js",
      "default": "./dist/remote/index.js"
    },
    "./core": "./dist/core/index.js"
  }
}
```

- **React Native**：获取原生实现，包含真实 UI 组件
- **Default** (QuickJS/Node)：获取远程实现，通过消息传递

## 开发

```bash
# 安装依赖
npm install

# 运行测试
npm test

# 运行测试并生成覆盖率报告
npm run test:coverage

# 代码检查
npm run lint

# 代码格式化
npm run fmt

# 构建
npm run build
```

## 许可证

Apache-2.0
