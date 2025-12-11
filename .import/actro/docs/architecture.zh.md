# 架构

## 概述

askit 实现了**同构架构**，使相同的代码能够在不同环境中运行，并提供特定于环境的实现。

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
│  │    └──► global.sendToHost('askit:event:event', payload)  │  │
│  │                                                           │  │
│  │  Toast.show(msg, opts)                                   │  │
│  │    └──► global.sendToHost('askit:toast:show', [msg,...]) │  │
│  │                                                           │  │
│  │  Haptic.trigger(type)                                    │  │
│  │    └──► global.sendToHost('askit:haptic:trigger', [type])│  │
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
      // 发送为数组以避免序列化问题
      global.sendToHost('askit:toast:show', [message, options]);
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
  if (message.event.startsWith('askit:')) {
    const parts = message.event.slice(6).split(':');
    const [moduleName, methodName] = parts;

    // EventEmitter 事件（格式：askit:event:eventName）
    if (moduleName === 'event') {
      const eventName = parts.slice(1).join(':');
      // 使用 Symbol 访问内部 API（对公共 API 隐藏）
      (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL](eventName, message.payload);
      return;
    }

    // 模块调用（格式：askit:module:method）
    const module = modules[moduleName];
    if (module) {
      const method = module[methodName];
      if (typeof method === 'function') {
        const args = Array.isArray(message.payload)
          ? message.payload
          : [message.payload];
        return method.apply(module, args);
      }
    }
  }
}
```

### Engine 适配器 (`core/bridge.ts`)

连接 Rill Engine 和 askit EventEmitter：

```typescript
import { BROADCASTER_SYMBOL } from '../api/EventEmitter.host';

function createEngineAdapter(engine: EngineInterface) {
  // 将 EventEmitter 事件转发到 engine，带有 askit:event: 前缀
  // 使用 Symbol 访问内部 API（对公共 API 隐藏）
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event, payload) => {
    engine.sendEvent(`askit:event:${event}`, payload);
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
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble
};

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
