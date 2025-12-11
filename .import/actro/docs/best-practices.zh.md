# 最佳实践

## 避免内存泄漏

### 问题：忘记清理监听器

```tsx
// ❌ 错误：忘记清理监听器
useEffect(() => {
  EventEmitter.on('data:update', handleUpdate);
  // 组件卸载后，监听器仍然存在 → 内存泄漏
}, []);
```

### ✅ 解决方案 1：使用返回的清理函数

```tsx
// ✅ 正确：使用清理函数
useEffect(() => {
  const cleanup = EventEmitter.on('data:update', handleUpdate);
  return cleanup; // React 会在组件卸载时自动调用
}, []);
```

### ✅ 解决方案 2：简写形式

```tsx
// ✅ 更简洁的写法
useEffect(() => {
  return EventEmitter.on('data:update', handleUpdate);
}, []);
```

### 自动内存泄漏检测

askit 会自动检测潜在的内存泄漏：

```tsx
// 如果同一个事件有超过 10 个监听器，会发出警告
useEffect(() => {
  return EventEmitter.on('scroll:position', handleScroll);
}, []); // 如果这个组件被创建 10+ 次，会收到警告
```

**警告示例：**
```
[askit/EventEmitter] Possible memory leak detected: 11 listeners for "scroll:position"
Context: {
  event: "scroll:position",
  count: 11,
  maxListeners: 10,
  suggestion: "Use the cleanup function returned by on() in React useEffect"
}
```

### 调整监听器限制

```tsx
import { EventEmitter } from 'askit';

// 某些场景可能需要更多监听器（如列表渲染）
(EventEmitter as any).setMaxListeners(20);

// 或者禁用警告（不推荐）
(EventEmitter as any).setMaxListeners(0); // 0 = 无限制
```

## 性能优化：节流和防抖

### 高频事件导致性能问题

```tsx
// ❌ 问题：滚动事件触发太频繁
useEffect(() => {
  return EventEmitter.on('scroll:position', (position) => {
    // 这个函数可能每秒调用 100+ 次
    updateUI(position);
  });
}, []);
```

### ✅ 解决方案：使用节流（Throttle）

```tsx
// ✅ 节流：最多每 100ms 执行一次
useEffect(() => {
  return EventEmitter.on(
    'scroll:position',
    (position) => {
      updateUI(position);
    },
    { rateLimit: 'throttle', delay: 100 } // 限制频率
  );
}, []);
```

### ✅ 解决方案：使用防抖（Debounce）

```tsx
// ✅ 防抖：等用户停止输入 200ms 后才执行
useEffect(() => {
  return EventEmitter.on(
    'input:change',
    (value) => {
      searchAPI(value);
    },
    { rateLimit: 'debounce', delay: 200 } // 延迟执行
  );
}, []);
```

### 节流 vs 防抖的选择

| 场景 | 使用 | 原因 |
|------|------|------|
| 滚动位置更新 | `throttle` (100ms) | 需要持续更新，但不要太频繁 |
| 窗口大小调整 | `throttle` (150ms) | 需要跟随调整，但限制频率 |
| 搜索输入 | `debounce` (200-300ms) | 等用户停止输入后再搜索 |
| 自动保存 | `debounce` (1000ms) | 等用户停止编辑后再保存 |
| 按钮点击 | `throttle` (300ms) | 防止重复点击 |

### 延迟值建议

- **不要太高**：100-300ms 是合理范围，保持响应性
- **根据场景调整**：
  - 滚动/调整大小：50-150ms
  - 输入搜索：200-300ms
  - 自动保存：500-1000ms

```tsx
// ✅ 合理的延迟值
{ rateLimit: 'throttle', delay: 100 }  // 滚动
{ rateLimit: 'debounce', delay: 250 }  // 搜索

// ⚠️ 延迟过高会让用户感觉卡顿
{ rateLimit: 'throttle', delay: 1000 } // 太慢！
```

## 组合使用

```tsx
// 结合所有最佳实践
useEffect(() => {
  // 自动清理 + 节流限制
  return EventEmitter.on(
    'analytics:*', // 通配符匹配所有分析事件
    (data) => {
      sendToAnalytics(data);
    },
    { rateLimit: 'throttle', delay: 100 } // 避免发送过于频繁
  );
}, []);
```

## 调试技巧

### 1. 查看当前监听器数量

```tsx
// 开发时检查是否有泄漏
console.log('Max listeners:', (EventEmitter as any).getMaxListeners());
// 输出: Max listeners: 10
```

### 2. 临时禁用节流进行调试

```tsx
// 开发时可以临时禁用节流观察行为
return EventEmitter.on(
  'debug:event',
  handler,
  { rateLimit: 'none' } // 调试时不限制
);
```

### 3. 使用 console 调试

```tsx
// 开发环境查看事件流
EventEmitter.on('*', (payload) => {
  console.log('[Debug] Event received:', payload);
});
```

## 总结

✅ **必须做：**
1. 始终使用 `on()` 返回的清理函数
2. 在 React useEffect 中 return 清理函数
3. 高频事件使用节流/防抖

⚠️ **注意：**
1. 监听器超过 10 个会收到警告
2. 延迟值不要设太高（100-300ms）

🚫 **不要做：**
1. 忘记清理监听器
2. 高频事件不做限流
3. 忽略内存泄漏警告
