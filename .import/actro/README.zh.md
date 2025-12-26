# askit

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

[English](./README.md)

**askit** 是构建在 [rill](https://github.com/GoAskAway/rill) 之上的 UI 组件库与 API 层。通过 package.json 条件导出，同一份 `import { StepList, Toast } from 'askit'` 在 Host 端获得真实 React Native 组件，在 Guest 端获得字符串标识符（由 rill 传递给 Host 渲染）。

## 与 rill 的关系

| 层级 | 职责 |
|------|------|
| **rill** | 沙箱隔离的动态 UI 渲染引擎 — 在独立的 JS 沙箱（QuickJS/JSC）中运行 React 代码，将渲染操作序列化为指令发送给 Host，由 Host 端的真实 React Native 执行渲染 |
| **askit** | 构建在 rill 之上的 UI 组件与 API 层 — 提供业务组件（StepList、ChatBubble 等）和跨边界 API（EventEmitter、Toast、Haptic） |

**核心机制：**
- **UI 组件**：Guest 端是字符串 `"StepList"`，Host 端是完整的 RN 实现
- **API**：Guest 端通过 `global.__sendEventToHost('ASKIT_TOAST_SHOW', {...})` 发命令，Host 端的 bridge 路由到真实的 ToastAndroid 等原生 API

## 架构

askit 采用 **90% 通用 + 10% 核心** 架构：

- **通用 (90%)**：Host 和 Guest 共享相同的 API 接口
- **核心 (10%)**：仅 Host 端使用的桥接和注册模块

```
┌─────────────────────────────────────────────────────────────┐
│                      Host App (RN)                          │
│  ┌─────────────────┐  ┌───────────────────────────────┐    │
│  │   askit/core    │  │         askit                 │    │
│  │  - registry     │  │  - EventEmitter, Toast, Haptic│    │
│  │  - bridge       │  │  - StepList, ThemeView...     │    │
│  └────────┬────────┘  └──────────────┬────────────────┘    │
│           │                          │                      │
│           └──────────┬───────────────┘                      │
│                      ▼                                      │
│              ┌───────────────┐                              │
│              │  rill Engine  │                              │
│              └───────┬───────┘                              │
└──────────────────────┼──────────────────────────────────────┘
                       │ 消息协议
┌──────────────────────┼──────────────────────────────────────┐
│                      ▼                                      │
│              ┌───────────────┐                              │
│              │ QuickJS Sandbox│                             │
│              └───────┬───────┘                              │
│                      │                                      │
│  ┌───────────────────▼─────────────────────────────────┐   │
│  │                    askit                            │   │
│  │  - EventEmitter, Toast, Haptic (Guest 实现)         │   │
│  │  - StepList, ThemeView... (DSL 生成器)              │   │
│  └─────────────────────────────────────────────────────┘   │
│                      Guest                                 │
└─────────────────────────────────────────────────────────────┘
```

## 安装

```bash
bun add github:GoAskAway/askit
```

> **注意**：askit 通过 GitHub 发布，直接导出 TypeScript 源码（无需构建步骤）。你的打包工具（Metro、Vite 等）需要支持 TypeScript 编译。React Native Metro 和现代打包工具都会自动处理。

## 使用

### 在 Guest 中 (QuickJS Sandbox)

```typescript
import { EventEmitter, Toast, Haptic } from 'askit';
import { StepList, UserAvatar } from 'askit';

// 事件通信
EventEmitter.emit('guest:ready', { version: '1.0.0' });
EventEmitter.on('host:config', (config) => {
  console.log('收到配置:', config);
});

// Toast 通知
Toast.show('来自插件的问候!', { duration: 'short', position: 'bottom' });

// 触觉反馈
Haptic.trigger('light');

// UI 组件（Guest 侧写 JSX；Host 侧注册同名组件实现）
export function App() {
  return <UserAvatar uri="https://example.com/avatar.png" size={48} />;
}
```

### 在 Host App 中 (React Native)

```typescript
// 导入核心模块用于桥接
import { createEngineAdapter, components } from 'askit/core';
import { Engine } from 'rill';

// 创建引擎并连接 askit
const engine = new Engine();
const adapter = createEngineAdapter(engine);

// 注册组件以渲染插件 UI
engine.register(components);

// 启动插件（支持 URL 或打包代码字符串）
await engine.loadBundle('https://example.com/guest.js');
```

## askit 是什么？

`askit` 是构建在 `rill` 之上的 **UI 组件库 + Host/Guest 接入层**。

- **Guest（沙箱）侧**：提供组件的 element 标识（string `ElementType`）与少量 API。你直接写标准 React JSX（例如 `<UserAvatar />`、`<StepList />`）。
- **Host（React Native）侧**：提供同名组件的真实 RN 实现，以及适配器 `createEngineAdapter`，用于把 rill `Engine` 的事件通道与 askit 的模块/事件打通。

### 分工与边界

| 层级 | 负责什么 | 典型代码 | 说明 |
|---|---|---|---|
| 业务应用（你的产品） | 业务逻辑、页面、状态、数据请求 | `unified-app.js` / app TSX | 发送/接收业务事件 |
| `askit` | 组件库 + 少量跨边界 API（Toast/Haptic/EventEmitter） | `askit/src/ui`, `askit/src/api` | 使用 rill HostEvent 模型 |
| `rill` | 沙箱运行时、reconciler、ops/receiver、devtools | `Engine`, `Receiver`, providers | 框架/运行时，不关心业务 UI |

### 消息模型（对齐 rill）

- **Guest → Host**：`global.__sendEventToHost(eventName, payload)`
  - `ASKIT_*` 为 askit 模块使用的保留内部命令事件：
    - `ASKIT_TOAST_SHOW` payload `{ message, options }`
    - `ASKIT_HAPTIC_TRIGGER` payload `{ type }`
  - 其他事件名均视为业务事件，会被转发到 Host `EventEmitter`。
- **Host → Guest**：Host 侧 `EventEmitter.emit(eventName, payload)` 会被转发到 `engine.sendEvent(eventName, payload)`。

## API 参考

### EventEmitter

Host 与 Guest 之间基于事件的通信。

```typescript
// 发送事件
EventEmitter.emit(event: string, payload?: unknown): void

// 监听事件（支持通配符：'user:*', 'analytics:**'）
EventEmitter.on(event: string, handler: (payload: unknown) => void, options?: {
  rateLimit?: 'throttle' | 'debounce' | 'none';
  delay?: number;
}): () => void

// 单次监听
EventEmitter.once(event: string, handler: (payload: unknown) => void): () => void

// 移除监听
EventEmitter.off(event: string, handler: (payload: unknown) => void): void
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
Haptic.trigger(type?: 'light' | 'medium' | 'heavy' | 'selection' | 'success' | 'warning' | 'error'): void
```

## 组件

| 组件 | 描述 |
|------|------|
| `StepList` | 带状态指示器的步骤列表 |
| `ThemeView` | 主题感知容器视图 |
| `UserAvatar` | 带降级支持的用户头像 |
| `ChatBubble` | 聊天消息气泡 |
| `PanelMarker` | 面板标记，用于识别左/右面板 (仅 Host) |
| `EngineMonitorOverlay` | rill 引擎监控调试覆盖层 (仅 Host) |

### 仅 Host 端工具

| 工具 | 描述 |
|------|------|
| `extractPanels` | 从 React 元素树中提取左/右面板 |

## 模块结构

```
askit/src
├── index.host.ts     # Host 入口 (React Native)
├── index.guest.ts    # Guest 入口 (QuickJS sandbox)
├── api/              # EventEmitter, Toast, Haptic 实现
├── ui/               # UI 组件 (StepList, ThemeView 等)
├── core/             # 仅 Host 端桥接和工具
│   ├── bridge.ts     # Host-Guest 消息桥接
│   ├── registry.ts   # 组件和模块注册
│   ├── throttle.ts   # 节流限速工具
│   └── protocol.ts # 保留事件名（ASKIT_*）
├── contracts/        # Host-Guest 通信类型契约
└── types/            # 共享 TypeScript 类型
```

### 条件导出

包使用条件导出来提供不同的实现：

```json
{
  "exports": {
    ".": {
      "react-native": "./src/index.host.ts",
      "default": "./src/index.guest.ts"
    },
    "./core": "./src/core/index.ts",
    "./contracts": "./src/contracts/index.ts"
  }
}
```

- **React Native**：获取原生实现，包含真实 UI 组件
- **Default** (QuickJS/Node)：获取远程实现，通过消息传递
- **contracts**：Host-Guest 通信的类型契约

## 开发

```bash
# 安装依赖
bun install

# 运行测试
bun test

# 运行测试并生成覆盖率报告
bun test --coverage

# 代码检查
bun run lint

# 代码格式化
bun run fmt

# 构建
bun run build
```

## 许可证

Apache-2.0
