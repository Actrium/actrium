# API 参考

## EventEmitter

EventEmitter API 提供 Host 与 Guest 之间基于事件的通信。

### 特性

- **通配符模式匹配**：使用 `*`（单层级）或 `**`（多层级）模式订阅多个事件
- **速率限制**：内置节流（throttle）和防抖（debounce）支持，适用于高频事件
- **类型安全**：扩展 `AskitEvents` 接口以获得类型化的事件负载
- **内存泄漏检测**：当监听器数量超过阈值时自动警告

### 方法

#### `EventEmitter.emit(event, payload?)`

发送带有可选负载的事件。

```typescript
// 基本用法
EventEmitter.emit('user:action', { type: 'click', target: 'button' });

// 类型安全用法（需要声明 AskitEvents）
declare module 'askit' {
  interface AskitEvents {
    'user:login': { userId: string; timestamp: number };
  }
}

EventEmitter.emit('user:login', { userId: '123', timestamp: Date.now() });
```

**参数：**
- `event: string | keyof AskitEvents` - 事件名称
- `payload?: T | AskitEvents[K]` - 可选的事件数据

#### `EventEmitter.on(event, handler, options?)`

订阅事件。支持通配符模式和速率限制。返回取消订阅函数。

```typescript
// 精确匹配
const unsubscribe = EventEmitter.on('data:update', (data) => {
  console.log('数据更新:', data);
});

// 通配符模式
EventEmitter.on('user:*', (payload) => {
  // 匹配 user:login、user:logout 等
});

EventEmitter.on('analytics:**', (payload) => {
  // 匹配 analytics:click、analytics:page:view 等
});

// 使用速率限制（节流）
EventEmitter.on('scroll:position', handleScroll, {
  rateLimit: 'throttle',
  delay: 100  // 每 100ms 最多执行一次
});

// 使用速率限制（防抖）
EventEmitter.on('input:change', handleInput, {
  rateLimit: 'debounce',
  delay: 200  // 事件停止后 200ms 才执行
});

// 稍后：取消订阅
unsubscribe();
```

**参数：**
- `event: string | keyof AskitEvents` - 事件名称或模式（支持 `*` 和 `**`）
- `handler: (payload: T) => void` - 回调函数
- `options?: EventListenerOptions` - 可选的速率限制配置

**EventListenerOptions：**
| 属性 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `rateLimit` | `'throttle' \| 'debounce' \| 'none'` | `'none'` | 速率限制类型 |
| `delay` | `number` | `100` | 速率限制的延迟毫秒数 |

**返回：** `() => void` - 取消订阅函数

#### `EventEmitter.once(event, handler)`

订阅事件，只执行一次。支持通配符模式。

```typescript
EventEmitter.once('init:complete', () => {
  console.log('初始化完成');
});

// 使用通配符
EventEmitter.once('error:*', (error) => {
  console.error('第一个错误:', error);
});
```

#### `EventEmitter.off(event, handler)`

移除特定的事件监听器。

```typescript
const handler = (data) => console.log(data);
EventEmitter.on('data:update', handler);

// 稍后：移除这个特定的处理器
EventEmitter.off('data:update', handler);
```

#### `EventEmitter.setMaxListeners(n)`

设置每个事件的最大监听器数量（0 = 无限制）。默认为 10。

```typescript
// 允许每个事件最多 20 个监听器
EventEmitter.setMaxListeners(20);

// 禁用警告
EventEmitter.setMaxListeners(0);
```

#### `EventEmitter.getMaxListeners()`

获取当前的最大监听器限制。

```typescript
const limit = EventEmitter.getMaxListeners();  // 默认返回 10
```

---

## Toast

向用户显示 Toast 通知。

### 方法

#### `Toast.show(message, options?)`

显示 Toast 消息。

```typescript
// 简单 Toast
Toast.show('Hello World!');

// 带选项
Toast.show('操作成功', {
  duration: 'long',
  position: 'top'
});

// 自定义时长（毫秒）
Toast.show('处理中...', {
  duration: 5000
});
```

**参数：**
- `message: string` - 要显示的文本
- `options?: ToastOptions` - 配置选项

**ToastOptions：**
| 属性 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `duration` | `'short' \| 'long' \| number` | `'short'` | 显示时长（short=2秒，long=3.5秒） |
| `position` | `'top' \| 'center' \| 'bottom'` | `'bottom'` | Toast 位置 |

---

## Haptic

在支持的设备上触发触觉反馈。

### 方法

#### `Haptic.trigger(type?)`

触发触觉反馈。

```typescript
// 轻度反馈（默认）
Haptic.trigger();
Haptic.trigger('light');

// 中度反馈
Haptic.trigger('medium');

// 重度反馈
Haptic.trigger('heavy');

// 成功反馈
Haptic.trigger('success');

// 警告反馈
Haptic.trigger('warning');

// 错误反馈
Haptic.trigger('error');
```

**参数：**
- `type?: HapticType` - 反馈强度/类型

**HapticType：**
| 值 | 描述 |
|----|------|
| `'light'` | 轻微反馈 |
| `'medium'` | 标准反馈 |
| `'heavy'` | 强烈反馈 |
| `'selection'` | 选择反馈 |
| `'success'` | 成功通知模式 |
| `'warning'` | 警告通知模式 |
| `'error'` | 错误通知模式 |

---

## Core 模块（仅 Host）

`askit/core` 模块仅在 Host App 中可用。

### createEngineAdapter(engine)

为 rill 引擎创建桥接适配器。

```typescript
import { createEngineAdapter } from 'askit/core';
import { Engine } from 'rill';

const engine = new Engine();
const adapter = createEngineAdapter(engine);
```

### components

所有已注册 UI 组件的映射。

```typescript
import { components } from 'askit/core';

// 注册到引擎
engine.register(components);
```

### modules

所有已注册 API 模块的映射（toast、haptic 处理器）。

### configureToast(handler)

覆盖默认的 Toast 实现。

```typescript
import { configureToast } from 'askit/core';

configureToast((message, options) => {
  // 自定义 Toast 实现
  MyCustomToast.show(message, options);
});
```

### configureHaptic(handler)

覆盖默认的 Haptic 实现。

```typescript
import { configureHaptic } from 'askit/core';

configureHaptic((type) => {
  // 自定义触觉反馈实现
  MyHapticLibrary.trigger(type);
});
```
