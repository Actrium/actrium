# askit 与 rill 集成指南

本指南详细说明 askit 如何与 rill 集成，实现沙箱化的 Guest 执行环境并提供统一的 API 接口。

## 概览

askit 作为 rill 沙箱引擎之上的**同构 UI/API 层**，提供：
- 在 Host 和 Guest 中表现一致的统一 API
- 为 AskAway 平台优化的预构建 UI 组件
- 自动消息路由和桥接

```
┌─────────────────────────────────────────────────────────┐
│                     你的 Host App                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │                                                  │   │
│  │    import { Engine } from 'rill'                │   │
│  │    import { createEngineAdapter,                │   │
│  │             components } from 'askit/core'      │   │
│  │                                                  │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
                           │
                           │ 集成层
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    askit/core                            │
│  ┌──────────────────┐         ┌──────────────────┐     │
│  │  桥接适配器      │◄────────┤   AskitRegistry  │     │
│  │                  │         │                  │     │
│  │  - 消息路由      │         │  - 组件          │     │
│  │  - 事件转发      │         │  - 模块          │     │
│  │                  │         │  - 处理器        │     │
│  └────────┬─────────┘         └──────────────────┘     │
└───────────┼──────────────────────────────────────────────┘
            │
            │ engine.on('message') / engine.sendEvent()
            ▼
┌─────────────────────────────────────────────────────────┐
│                    rill 引擎                             │
│  ┌──────────────────────────────────────────────────┐  │
│  │  QuickJS 沙箱运行时                              │  │
│  │                                                   │  │
│  │  - 执行 Guest 代码                               │  │
│  │  - 提供 global.sendToHost()                      │  │
│  │  - 提供 global.onHostEvent()                     │  │
│  │  - 组件注册                                       │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
            │
            │ Guest 导入 'askit'
            ▼
┌─────────────────────────────────────────────────────────┐
│                    Guest 代码                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │  import { EventEmitter, Toast, StepList } from 'askit'│
│  │                                                   │  │
│  │  EventEmitter.emit('ready')                      │  │
│  │  Toast.show('你好！')                            │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## 集成流程

### 1. 初始化设置

```typescript
// Host App 初始化
import { Engine } from 'rill';
import { createEngineAdapter, components } from 'askit/core';

// 步骤 1: 创建 rill 引擎
const engine = new Engine();

// 步骤 2: 创建 askit 桥接适配器
const adapter = createEngineAdapter(engine);

// 步骤 3: 注册 askit 组件
engine.register(components);

// 步骤 4: 加载 Guest bundle
await engine.loadBundle('https://cdn.example.com/guest.js');
```

**可视化流程：**

```
Host App 启动
     │
     ├─► new Engine()
     │        │
     │        └─► 创建 QuickJS 上下文
     │
     ├─► createEngineAdapter(engine)
     │        │
     │        ├─► 监听 engine.on('message')
     │        └─► 注册 EventEmitter 事件转发器
     │
     ├─► engine.register(components)
     │        │
     │        └─► 注册：PanelMarker, StepList, ThemeView, UserAvatar, ChatBubble
     │
     └─► engine.loadBundle(url)
              │
              ├─► 下载 Guest bundle
              ├─► 在 QuickJS 沙箱中执行
              └─► Guest 现在可以导入 'askit'
```

### 2. 消息路由

当 Guest 调用 askit API 时，消息通过多个层级路由：

```
┌─────────────────────────────────────────────────────────────────┐
│ GUEST (QuickJS 沙箱)                                            │
│                                                                  │
│  Toast.show('你好', { duration: 'short' })                      │
│     │                                                            │
│     └─► askit/Guest 实现                                        │
│           │                                                      │
│           └─► global.sendToHost({                               │
│                 event: 'askit:toast:show',                       │
│                 payload: ['你好', { duration: 'short' }]         │
│               })                                                 │
└────────────────────────────┼────────────────────────────────────┘
                             │
                消息：{ event, payload }
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL 引擎                  ▼                                     │
│                                                                  │
│  engine.emit('message', { event, payload })                     │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT 桥接                 ▼                                     │
│                                                                  │
│  handleGuestMessage({ event, payload })                         │
│     │                                                            │
│     ├─► 解析事件：'askit:toast:show'                           │
│     │     → module='toast', method='show'                        │
│     │                                                            │
│     └─► 调用 modules['toast']['show'](payload)                  │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT HOST 模块            ▼                                     │
│                                                                  │
│  Toast.show('你好', { duration: 'short' })                      │
│     │                                                            │
│     └─► ToastAndroid.show('你好', ToastAndroid.SHORT)           │
│              │                                                   │
│              └─► 显示原生 Android Toast                         │
└───────────────────────────────────────────────────────────────────┘
```

### 3. 事件广播（Host → Guest）

Host 可以通过 EventEmitter 向 Guest 发送事件：

```
┌─────────────────────────────────────────────────────────────────┐
│ HOST APP                                                         │
│                                                                  │
│  import { EventEmitter } from 'askit/core';                     │
│                                                                  │
│  EventEmitter.emit('config:update', { theme: 'dark' })          │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ ASKIT 桥接                 ▼                                     │
│                                                                  │
│  // 使用基于 Symbol 的内部 API                                  │
│  EventEmitter[BROADCASTER_SYMBOL]((event, payload) => {         │
│    engine.sendEvent(event, payload)                             │
│  })                                                              │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL 引擎                  ▼                                     │
│                                                                  │
│  调用 global.onHostEvent('config:update', { theme: 'dark' })    │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ GUEST (QuickJS)            ▼                                     │
│                                                                  │
│  import { EventEmitter } from 'askit';                          │
│                                                                  │
│  EventEmitter.on('config:update', (config) => {                 │
│    console.log('主题更改为：', config.theme);                   │
│  });                                                             │
└───────────────────────────────────────────────────────────────────┘
```

### 4. UI 组件渲染

askit 组件在 Guest 中返回 DSL 对象，rill 在 Host 中原生渲染：

```
┌─────────────────────────────────────────────────────────────────┐
│ GUEST 代码                                                       │
│                                                                  │
│  import { StepList } from 'askit';                              │
│                                                                  │
│  const ui = StepList({                                          │
│    steps: [                                                      │
│      { id: '1', label: '步骤 1', status: 'completed' },         │
│      { id: '2', label: '步骤 2', status: 'active' }             │
│    ]                                                             │
│  });                                                             │
│                                                                  │
│  // 返回：'StepList' (字符串标识符)                             │
└────────────────────────────┼────────────────────────────────────┘
                             │
                   DSL: { type: 'StepList', props }
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ RILL 引擎                  ▼                                     │
│                                                                  │
│  从 Guest 接收 DSL 对象                                         │
│  在已注册组件中查找 'StepList'                                  │
└────────────────────────────┼────────────────────────────────────┘
                             │
                             │
┌────────────────────────────┼────────────────────────────────────┐
│ HOST APP                   ▼                                     │
│                                                                  │
│  渲染：<StepList                                                │
│          steps={[...]}                                           │
│        />                                                        │
│                                                                  │
│  这是真实的 React Native 组件                                   │
└───────────────────────────────────────────────────────────────────┘
```

## 完整示例

### Host App 设置

```typescript
// App.tsx
import React, { useEffect, useState } from 'react';
import { View } from 'react-native';
import { Engine, EngineView } from 'rill';
import { createEngineAdapter, components } from 'askit/core';
import { EventEmitter } from 'askit';

export default function App() {
  const [engine] = useState(() => {
    // 1. 创建引擎
    const eng = new Engine();

    // 2. 设置 askit 桥接
    createEngineAdapter(eng);

    // 3. 注册组件
    eng.register(components);

    return eng;
  });

  useEffect(() => {
    // 4. 向 Guest 发送事件
    EventEmitter.emit('app:ready', { version: '1.0.0' });
  }, []);

  return (
    <View style={{ flex: 1 }}>
      {/* 5. 渲染 Guest UI */}
      <EngineView
        engine={engine}
        bundleUrl="https://cdn.example.com/guest.js"
        initialProps={{ userId: '123' }}
      />
    </View>
  );
}
```

### Guest 代码

```typescript
// guest.ts
import { EventEmitter, Toast, Haptic, StepList } from 'askit';

// 监听 Host 事件
EventEmitter.on('app:ready', (data) => {
  Toast.show(`App 版本：${data.version}`);
  Haptic.trigger('success');
});

// 向 Host 发送事件
EventEmitter.emit('guest:loaded', { timestamp: Date.now() });

// 渲染 UI
export default function GuestUI(props) {
  return StepList({
    steps: [
      { id: '1', label: '欢迎', status: 'completed' },
      { id: '2', label: '开始使用', status: 'active' },
    ],
  });
}
```

## 生命周期时序

```
┌──────────┐
│   HOST   │
└─────┬────┘
      │
      │ 1. new Engine()
      │ 2. createEngineAdapter(engine)
      │ 3. engine.register(components)
      │
      ▼
┌─────────────────┐
│  ASKIT 桥接     │ ← 监听消息
└─────┬───────────┘
      │
      │ 4. engine.loadBundle(url)
      │
      ▼
┌──────────────┐
│ RILL 引擎    │
└─────┬────────┘
      │
      │ 5. 下载并执行 Guest
      │
      ▼
┌──────────┐
│  GUEST   │
└─────┬────┘
      │
      │ 6. import 'askit'
      │ 7. EventEmitter.emit('guest:loaded')
      │
      ▼
┌─────────────────┐
│  ASKIT 桥接     │ ← 接收消息
└─────┬───────────┘
      │
      │ 8. 路由到 EventEmitter 处理器
      │
      ▼
┌──────────┐
│   HOST   │ ← 触发 EventEmitter.on('guest:loaded')
└──────────┘
```

## 核心概念

### 1. 条件导出
askit 使用 package.json 的 `exports` 提供不同的实现：
- Host 从 `./src/index.host.ts` 导入（React Native 环境）
- Guest 从 `./src/index.guest.ts` 导入（QuickJS 环境）

### 2. 消息协议
所有 Guest→Host 通信使用以下格式：
```typescript
{
  event: 'askit:module:method',  // 例如：'askit:toast:show'
  payload: any[]                  // 方法参数
}
```

### 3. 桥接适配器
桥接适配器连接 askit 到 rill 的消息系统：
- 监听 `engine.on('message')` 处理 Guest→Host
- 使用 `EventEmitter[BROADCASTER_SYMBOL]()` 处理 Host→Guest（基于 Symbol 的内部 API）
- 将消息路由到适当的处理器

### 4. 组件注册表
`components` 将字符串标识符映射到 React 组件：
```typescript
{
  PanelMarker,
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble,
}
```

## 调试

### 启用消息日志

```typescript
// 在 Host 中
import { Bridge } from 'askit/core';

const originalHandler = Bridge.handleMessage;
Bridge.handleMessage = (message) => {
  console.log('[askit] Guest→Host:', message);
  return originalHandler(message);
};
```

### 检查 Guest 环境

```typescript
// 在 Guest 中
console.log('可用的全局变量：', Object.keys(global));
console.log('可以发送到 host：', typeof global.sendToHost === 'function');
```

## 最佳实践

1. **始终先创建桥接适配器**，再加载 Guest 代码
2. **在调用 `engine.loadBundle()` 之前注册组件**
3. **使用 EventEmitter 进行双向事件通信**，而不是直接消息传递
4. **保持 Guest bundle 小巧** - askit 已通过 `rill/sdk` 包含
5. **优雅地处理错误** - Guest 崩溃不应导致 Host 崩溃

## 故障排除

| 问题 | 原因 | 解决方案 |
|------|------|----------|
| "sendToHost is not a function" | 引擎设置前加载了 Guest | 在 `loadBundle()` 前创建适配器 |
| 组件未渲染 | 组件未注册 | 添加到 `components` 并调用 `engine.register()` |
| 消息未收到 | 未创建桥接 | 调用 `createEngineAdapter(engine)` |
| EventEmitter 事件不工作 | loadBundle 后才创建适配器 | 在引擎设置期间创建适配器 |

## 下一步

- [API 参考](./api-reference.md) - 详细的 API 文档
- [组件](./components.md) - 可用的 UI 组件
- [架构](./architecture.md) - 内部设计细节
