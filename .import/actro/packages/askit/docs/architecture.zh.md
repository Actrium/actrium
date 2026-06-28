# 架构

## 概述

**askit** 是构建在 [rill](https://github.com/GoAskAway/rill) 之上的 UI 组件库与 API 层。通过 package.json 条件导出，同一份 `import { StepList, Toast } from 'askit'` 在 Host 端获得真实 React Native 组件，在 Guest 端获得字符串标识符（由 rill 传递给 Host 渲染）。

### 与 rill 的关系

| 层级 | 职责 |
|------|------|
| **rill** | 沙箱隔离的动态 UI 渲染引擎 — 在独立的 JS 沙箱（QuickJS/JSC）中运行 React 代码，将渲染操作序列化为指令发送给 Host，由 Host 端的真实 React Native 执行渲染 |
| **askit** | 构建在 rill 之上的 UI 组件与 API 层 — 提供业务组件（StepList、ChatBubble 等）和跨边界 API（EventEmitter、Toast、Haptic） |

### 核心机制

- **UI 组件**：Guest 端是字符串 `"StepList"`，Host 端是完整的 RN 实现
- **API**：Guest 端通过 `global.__sendEventToHost('ASKIT_TOAST_SHOW', {...})` 发命令，Host 端的 bridge 路由到真实的 ToastAndroid 等原生 API

## 设计原则

### 1. 相同 API，不同实现

开发者使用相同的 API 接口编写代码，无论执行环境如何：

```typescript
// 这段代码在 Host 和 Guest 中都能工作
import { EventEmitter, Toast } from 'askit';

EventEmitter.emit('action', { type: 'click' });
Toast.show('Hello!');
```

### 2. 条件导出

`package.json` 使用条件导出来提供不同的包：

```json
{
  "exports": {
    ".": {
      "react-native": "./src/index.host.ts",
      "default": "./src/index.guest.ts"
    }
  }
}
```

- **react-native**：带有真实 React Native 组件的 Host 实现
- **default**：带有消息传递的 Guest 实现

### 3. 90% 通用 + 10% 核心

| 类别 | 位置 | 用途 |
|------|------|------|
| 通用 (90%) | `askit` | 两个环境都可用的 API 和组件 |
| 核心 (10%) | `askit/core` | 仅 Host 端的桥接和注册 |

## 消息流

```
┌──────────────────────────────────────────────────────────────────┐
│                        HOST APP                                   │
│                                                                   │
│  ┌────────────────┐    ┌──────────────────┐    ┌──────────────┐ │
│  │ EventEmitter   │    │  createEngine    │    │    Rill      │ │
│  │ (host)         │◄───│  Adapter         │◄───│   Engine     │ │
│  │ - emit()       │    │  - [BROADCASTER] │    │              │ │
│  │ - on()/off()   │    │  - [NOTIFY]      │    │              │ │
│  └────────────────┘    └──────────────────┘    └──────┬───────┘ │
│         │                                               │         │
│         ▼                                               │         │
│  ┌────────────────┐                                    │         │
│  │ React Native   │                                    │         │
│  │     组件       │                                    │         │
│  │ Toast, Haptic  │                                    │         │
│  └────────────────┘                                    │         │
└────────────────────────────────────────────────────────┼─────────┘
                                                         │
                            消息协议 (askit:...)
                                                         │
┌────────────────────────────────────────────────────────┼─────────┐
│                   GUEST (QuickJS 沙箱)                 │         │
│                                                        ▼         │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    askit (guest)                          │  │
│  │                                                           │  │
│  │  EventEmitter.emit('event')                              │  │
│  │    └──► global.__sendEventToHost('event', payload)  │  │
│  │                                                           │  │
│  │  Toast.show(msg, opts)                                   │  │
│  │    └──► global.__sendEventToHost('ASKIT_TOAST_SHOW', { message: msg, options }) │  │
│  │                                                           │  │
│  │  Haptic.trigger(type)                                    │  │
│  │    └──► global.__sendEventToHost('ASKIT_HAPTIC_TRIGGER', { type })│  │
│  │                                                           │  │
│  │  Component() ──► 返回 DSL 对象                           │  │
│  └──────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

## 模块职责

### Host 实现 (`*.host.ts`)

直接在 Host App 中执行：

```typescript
// Toast.host.ts
import { ToastAndroid, Platform } from 'react-native';

export const Toast = {
  show(message: string, options?: ToastOptions) {
    if (Platform.OS === 'android') {
      ToastAndroid.show(message, ToastAndroid.SHORT);
    }
    // iOS: 使用自定义实现
  }
};
```

### Guest 实现 (`*.guest.ts`)

发送消息到 Host 执行：

```typescript
// Toast.guest.ts
class RemoteToast implements ToastAPI {
  show(message: string, options?: ToastOptions): void {
    if (typeof global.sendToHost === 'function') {
      global.__sendEventToHost('ASKIT_TOAST_SHOW', { message, options });
    }
  }
}

export const Toast: ToastAPI = new RemoteToast();
```

### 桥接 (`core/bridge.ts`)

将 Guest 消息路由到适当的处理器：

```typescript
import { NOTIFY_SYMBOL } from '../api/EventEmitter.host';

function handleGuestMessage(message: GuestMessage) {
  // 保留内部命令
  if (message.event === 'ASKIT_TOAST_SHOW') {
    const { message: text, options } = message.payload as any;
    return modules.toast.show(text, options);
  }

  if (message.event === 'ASKIT_HAPTIC_TRIGGER') {
    const { type } = message.payload as any;
    return modules.haptic.trigger(type);
  }

  // 业务事件：转发到 EventEmitter
  (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL](message.event, message.payload);
}
```

### Engine 适配器 (`core/bridge.ts`)

连接 Rill Engine 和 askit EventEmitter：

```typescript
import { BROADCASTER_SYMBOL } from '../api/EventEmitter.host';

function createEngineAdapter(engine: EngineInterface) {
  // 将 Host EventEmitter 事件转发到 Guest
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event, payload) => {
    engine.sendEvent(event, payload);
  });

  // 监听 Guest 消息并路由它们
  const unsubscribeMessage = engine.on('message', (message) => {
    handleGuestMessage({
      event: message.event,
      payload: message.payload
    });
  });

  return {
    dispose() {
      (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL](null);
      unsubscribeMessage();
    },
  };
}
```

### 注册表 (`core/registry.ts`)

管理组件和模块注册：

```typescript
export const components = {
  PanelMarker,
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble
} as const;

export const modules = {
  toast: Toast,    // Host Toast 实现
  haptic: Haptic   // Host Haptic 实现
};
```

## 包结构

askit 直接导出 TypeScript 源码（无需构建步骤）：

```
askit/
├── src/
│   ├── index.host.ts      # Host (React Native) 入口
│   ├── index.guest.ts     # Guest (QuickJS) 入口
│   ├── core/
│   │   └── index.ts       # 仅 Host 使用的核心模块
│   ├── api/               # API 实现
│   │   ├── *.host.ts      # Host 实现
│   │   └── *.guest.ts     # Guest 实现
│   └── ui/                # UI 组件
│       ├── */
│       │   ├── *.host.tsx  # 原生组件
│       │   └── *.guest.tsx # DSL 生成器
└── package.json           # 直接从 src/ 导出
```

你的打包工具（Metro、Vite 等）会自动处理 TypeScript 编译。

## 测试策略

| 层级 | 环境 | 工具 |
|------|------|------|
| API 逻辑 | Bun 运行时 | bun test |
| Android 特定 | Mock 注入 | bun test + DI |
| UI 组件 | React Native | E2E / 集成测试 |
| 完整集成 | 设备/模拟器 | Detox |

```typescript
// 示例：使用依赖注入测试 Android 特定代码
import { _injectMocks, HostToast, Toast } from './Toast.host';

beforeEach(() => {
  _injectMocks(
    { OS: 'android' },
    { showWithGravity: vi.fn(), SHORT: 0, BOTTOM: 80, TOP: 48, CENTER: 17, LONG: 1 }
  );
});

it('应该在 Android 上调用 ToastAndroid', () => {
  Toast.show('Hello');
  expect(mockShowWithGravity).toHaveBeenCalled();
});
```
