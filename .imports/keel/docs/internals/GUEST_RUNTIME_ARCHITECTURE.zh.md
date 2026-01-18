# Guest 运行时架构

本文档描述 Rill Guest 运行时的架构 —— 用户代码如何打包、注入沙箱并执行。

## 概述

Rill 采用**两阶段架构**在沙箱环境中运行 Guest 代码：

1. **构建阶段**：将 Guest 运行时（React + Reconciler + Bridge 协议）编译为单个压缩字符串
2. **运行时阶段**：向沙箱注入代码片段，逐步构建状态机

```
┌─────────────────────────────────────────────────────────────────┐
│                          构建阶段                                 │
│  src/guest/bundle.ts  ──[bun build]──>  src/guest/build/bundle.ts│
│   (React + SDK + Reconciler + Bridge)          (可注入字符串)      │
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                         运行时阶段                                │
│  Engine.loadBundle()                                             │
│    1. injectPolyfills()                → timers/require/module/exports │
│    2. evalCode(GUEST_BUNDLE_CODE)      → React/RillSDK/RillReconciler  │
│    3. injectRuntimeAPI()               → __sendToHost/__getConfig 等    │
│    4. evalCode(userBundle)             → 用户的 Guest 组件              │
└─────────────────────────────────────────────────────────────────┘
```

## 目录结构

```
src/
├── sdk/                       # Guest SDK (rill/sdk)
│   ├── index.ts               # 公共导出（组件、Hooks、类型）
│   ├── sdk.ts                 # 组件和 Hooks 实现
│   └── types.ts               # Guest 侧类型
│
├── guest/                     # Guest 运行时包（自动构建）
│   ├── bundle.ts              # 打包入口（源码）
│   ├── runtime/               # Guest 运行时模块
│   │   ├── init.ts            # 环境初始化
│   │   ├── globals-setup.ts   # console + HostEvent helpers + callbacks
│   │   ├── react-global.ts    # 将 React/JSX runtime 暴露到 globalThis
│   │   └── reconciler/        # React Reconciler 实现
│   │       ├── index.ts       # 公共 API (render, unmount 等)
│   │       ├── host-config.ts # react-reconciler 主机配置
│   │       ├── reconciler-manager.ts  # Reconciler 实例管理
│   │       ├── operation-collector.ts # 操作批处理
│   │       ├── element-transform.ts   # Guest 元素转换
│   │       ├── guest-encoder.ts       # Props 序列化
│   │       ├── devtools.ts            # DevTools 集成
│   │       └── types.ts               # Reconciler 类型
│   └── build/
│       └── bundle.ts          # 自动生成的 `GUEST_BUNDLE_CODE`（勿手动编辑）
│
├── shared/                    # 共享协议层
│   ├── index.ts               # 协议导出
│   ├── types.ts               # 操作和消息类型
│   ├── TypeRules.ts           # 序列化规则
│   ├── serialization.ts       # 编解码工具
│   ├── bridge/                # Bridge 层
│   │   └── Bridge.ts          # 核心通信
│   └── CallbackRegistry.ts    # 跨边界函数管理
│
└── host/                      # Host 运行时
    ├── Engine.ts              # Engine（加载和执行 Guest）
    └── receiver/              # 接收操作，渲染 UI
```

## 构建阶段

### 构建脚本：`scripts/build-guest-bundle.ts`

```bash
bun scripts/build-guest-bundle.ts
```

**输入**: `src/guest/bundle.ts`
**输出**: `src/guest/build/bundle.ts`

构建过程：
1. 打包 `src/guest/bundle.ts` 及所有依赖（React、rill/sdk、Reconciler、Bridge 协议）
2. 压缩为 IIFE 格式
3. Babel 转译到 ES5（提升 Guest 引擎兼容性）
4. 包装为 TypeScript 导出

### 打包内容

```
bundle.ts
├── runtime/init.ts            # 设置 __RILL_GUEST_ENV__、__callbacks
├── runtime/globals-setup.ts   # console + HostEvent helpers
├── runtime/react-global.ts    # React/JSX 全局
├── ../sdk/*                   # rill/sdk（Guest SDK）
├── runtime/reconciler/*       # Reconciler 实现
├── ../shared/*                # 共享协议（CallbackRegistry、TypeRules）
└── react / react-reconciler   # React 运行时与渲染器基础
```

## 运行时阶段：状态机

每次 `evalCode()` 调用都会向沙箱的全局状态添加内容。顺序至关重要：

### 1. Console 设置 (`CONSOLE_SETUP_CODE`)

```javascript
globalThis.console = {
  log: function() { __console_log.apply(null, arguments); },
  warn: function() { __console_warn.apply(null, arguments); },
  // ...
};
```

### 2. 运行时助手 (`RUNTIME_HELPERS_CODE`)

```javascript
globalThis.__callbacks = new Map();
globalThis.__callbackId = 0;
globalThis.__registerCallback = function(fn) { /* ... */ };
globalThis.__invokeCallback = function(fnId, args) { /* ... */ };
globalThis.__useHostEvent = function(eventName, callback) { /* ... */ };
globalThis.__handleHostEvent = function(eventName, payload) { /* ... */ };
```

### 3. React/JSX Shims (`ALL_SHIMS`)

```javascript
globalThis.__rillHooks = { states: [], effects: [], /* ... */ };
globalThis.React = { createElement, useState, useEffect, /* ... */ };
globalThis.require = function(id) {
  if (id === 'react') return globalThis.React;
  if (id === 'react-native') return { View: 'View', Text: 'Text', /* ... */ };
  // ...
};
```

### 4. Guest Bundle (`GUEST_BUNDLE_CODE`)

```javascript
globalThis.RillReconciler = {
  render: function(element, sendToHost) { /* ... */ },
  unmount: function(sendToHost) { /* ... */ },
  unmountAll: function() { /* ... */ },
  invokeCallback: function(fnId, args) { /* ... */ },
  releaseCallback: function(fnId) { /* ... */ },
  getCallbackCount: function() { /* ... */ },
};
```

### 5. 组件名全局变量 (injectRuntimeAPI)

Engine 会将每个已注册的组件名注入为全局变量，值为组件名字符串本身：

```javascript
// 对于 ComponentRegistry 中注册的每个组件：
globalThis.View = 'View';
globalThis.Text = 'Text';
globalThis.Image = 'Image';
// ... 等等
```

这允许用户代码使用**变量模式**而非**字符串模式**：

```javascript
// 变量模式（推荐）- 编译时验证
h(View, { style: styles.container }, children);

// 字符串模式 - 仅运行时验证
h('View', { style: styles.container }, children);
```

`rill/cli build` 命令会自动将 JSX 转换为变量模式。

### 6. 用户 Bundle

```javascript
// 用户编译后的 Guest 代码
var MyComponent = function() {
  var config = useConfig();
  useHostEvent('REFRESH', function() { /* ... */ });
  return React.createElement(View, null, /* ... */);
};

// 自动渲染
RillReconciler.render(
  React.createElement(MyComponent),
  globalThis.__sendToHost
);
```

## 数据流

### Guest → Host（渲染操作）

```
用户组件
     │
     ▼
React Reconciler (host-config.ts)
     │ createInstance、appendChild、commitUpdate 等
     ▼
Operation Collector (operation-collector.ts)
     │ 批量收集操作
     ▼
sendToHost(batch)  ──[序列化]──>  Host Engine
                                       │
                                       ▼
                                   Receiver
                                       │
                                       ▼
                                原生组件树
```

### Host → Guest（事件）

```
Host App
     │ engine.sendEvent('REFRESH', payload)
     ▼
Engine
     │ evalCode("__handleHostEvent('REFRESH', payload)")
     ▼
Sandbox
     │ __hostEventListeners.get('REFRESH').forEach(cb => cb(payload))
     ▼
用户组件中的 useHostEvent 回调
```

### Host → Guest（回调调用）

```
原生 Button onPress
     │
     ▼
Receiver.handleCallback(fnId, args)
     │
     ▼
Engine.evalCode("RillReconciler.invokeCallback(fnId, args)")
     │
     ▼
CallbackRegistry.invoke(fnId, args)
     │
     ▼
用户组件中的原始函数
```

## 关键设计决策

### 为什么预打包 Reconciler？

1. **启动性能**：单次 eval vs 多次脚本加载
2. **版本一致性**：React 和 Reconciler 版本锁定
3. **体积优化**：构建时 tree-shaking 和压缩

### 为什么分离 `src/sdk/` 和 `src/guest/`？

- **`src/sdk/`**：用户 API —— 开发者在 Guest 代码中导入的内容
- **`src/guest/`**：运行时内部 —— 由 Engine 打包并注入

用户从 `rill/sdk` 导入：
```tsx
import { View, Text, useHostEvent } from 'rill/sdk';
```

他们不会直接使用 `render()`、`CallbackRegistry` 等 —— 那些是运行时内部实现。

### 共享 Bridge 协议

`src/shared/` 包含双方共享的序列化协议：
- **Guest 端**：打包进 `GUEST_BUNDLE_CODE`
- **Host 端**：由 `src/host/bridge/Bridge.ts` 直接导入

这确保双方使用相同的序列化逻辑。

## 文件职责

| 文件 | 职责 |
|------|------|
| `src/guest/bundle.ts` | Guest 运行时入口：设置 `React`/`RillSDK`/`RillReconciler` 全局 |
| `src/guest/runtime/init.ts` | 在任何 React 代码运行前的环境设置（Guest 标记、callbacks） |
| `src/guest/runtime/globals-setup.ts` | Console 和运行时助手设置（HostEvent、callbacks 等） |
| `src/guest/runtime/react-global.ts` | 将 React/JSX runtime 暴露到 `globalThis` |
| `src/guest/runtime/reconciler/*` | react-reconciler 配置与实例管理、操作收集与编码 |
| `src/sdk/*` | Guest SDK（`rill/sdk`）：组件、Hooks、RemoteRef、ErrorBoundary |
| `src/guest/build/bundle.ts` | 自动生成：可注入的 `GUEST_BUNDLE_CODE` |
| `src/host/Engine.ts` | 加载 Guest bundle、注入 API、管理沙箱生命周期 |

## 重新生成 Guest Bundle

修改 `src/guest/` 或 `src/shared/` 中的任何文件后：

```bash
bun scripts/build-guest-bundle.ts
```

这将重新生成 `src/guest/build/bundle.ts`。该文件应提交到版本控制。
