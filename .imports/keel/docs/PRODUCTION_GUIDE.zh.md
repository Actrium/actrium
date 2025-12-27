# Rill 生产部署指南

本文档总结了在生产环境部署 Rill 的建议配置与运维实践。

## 1. 运行时加固

- 模块白名单（requireWhitelist）
  - 在创建 Engine 时传入 `requireWhitelist`。仅允许guest bundle `require()` 这些模块。
  - 默认白名单：`react`、`react-native`、`react/jsx-runtime`、`rill/sdk`。
- 执行超时
  - 使用 `timeout` 选项（默认 5000ms）。QuickJS `eval` 为同步执行，无法被强制中断，该超时保护为“尽力而为”。若需严格 CPU 时间切片，请考虑 worker/线程隔离。
- 错误分类
  - Engine 会抛出 `RequireError`、`ExecutionError`、`TimeoutError`，便于监控与治理。
  - 订阅 `engine.on('error', handler)` 并上报。
- 指标上报
  - 通过 `onMetric(name, value, extra?)` 采集基础指标：
    - `engine.resolveSource`（ms）
    - `engine.fetchBundle`（ms, { status, size }）
    - `engine.initializeRuntime`（ms）
    - `engine.executeBundle`（ms, { size }）
    - `receiver.applyBatch`（ms, { applied, skipped, total }）
- 批量限制
  - `receiverMaxBatchSize`（默认 5000）限制单批操作应用数量，避免guest一次性下发巨量操作拖垮宿主。超出部分跳过，并通过指标上报。

## 2. 可观测性

- 日志
  - 通过 `logger` 注入结构化日志，预发阶段可开启 `debug`。
  - guest侧 `console` 会被注入并带前缀输出。
- 指标
  - 将 `onMetric` 事件转发到你的指标系统（Datadog/Prometheus/Logcat 等）。
- 健康检查
  - 检查 `engine.isLoaded`，可选做一次 `sendEvent` 往返。

## 3. 安全与隔离

- 沙箱
  - 生产环境推荐使用 `JSEngineProvider` 实现（如 QuickJS WASM），保障与宿主隔离。
  - 支持的沙箱模式：`'vm'` (Node.js VM)、`'worker'` (Web Worker)、`'none'` (无沙箱)
- 模块访问
  - 尽量精简白名单，避免暴露 Node 内置或动态加载能力。
- 回调
  - 对来自guest的 payload 做校验，建议定义强类型 props。

## 4. 性能建议

- 节流与合并
  - 高频更新场景可使用 `ThrottledScheduler`、`OperationMerger`。
- 虚拟列表
  - 长列表渲染组合 `VirtualScrollCalculator` 与 `ScrollThrottler` 并调优参数。
- 内存管理
  - guest不再使用时及时调用 `engine.destroy()`。

## 5. CI/CD 与打包

- 测试
  - 保持单元 + 集成测试为绿。设置覆盖率阈值。
- Lint
  - 启用 `@typescript-eslint/consistent-type-imports`，避免打包链路解析陷阱。
- 分析
  - 使用 CLI `analyze` 检查 bundle：体积、依赖来源、是否包含非白名单模块（可选启用 fail-on-violation）。

## 6. 宿主集成清单

- [ ] (可选) 提供 `JSEngineProvider` 实现，或使用内置沙箱模式
- [ ] 创建 `Engine` 并设置 `sandbox`、`requireWhitelist`、`timeout`、`logger`、`onMetric`、`receiverMaxBatchSize`
- [ ] 注册宿主组件：`engine.register()`
- [ ] 创建并使用 `receiver = engine.createReceiver(onUpdate)` 渲染树更新
- [ ] 打通事件桥：`engine.sendEvent` 与 `__handleHostMessage`
- [ ] 健康检查与指标上报
- [ ] 订阅 `engine.on('error', ...)` 并提供降级 UI

## 7. 可观测性细节

- 指标名称与负载
  - engine.resolveSource: { }（ms）
  - engine.fetchBundle: { status, size }（ms）
  - engine.initializeRuntime: { }（ms）
  - engine.executeBundle: { size }（ms）
  - engine.sendToSandbox: { size }（ms）
  - receiver.applyBatch: { applied, skipped, failed, total }（ms）
  - receiver.render: { nodeCount }（ms）

- onMetric 使用
```ts
const metrics: Array<{ name: string; value: number; extra?: Record<string, unknown> }> = [];
const engine = new Engine({
  provider: myJSEngineProvider, // 可选
  sandbox: 'vm', // 可选: 'vm' | 'worker' | 'none'
  onMetric: (n, v, e) => metrics.push({ name: n, value: v, extra: e })
});
```

- 健康检查 API
```ts
const health = engine.getHealth();
// { loaded, destroyed, errorCount, lastErrorAt, receiverNodes }
```

## 8. CLI 使用

- Analyze 白名单扫描
```bash
bun run rill/cli analyze dist/bundle.js # 默认只警告
```
代码方式（带选项）:
```ts
import { analyze } from 'rill/cli';
await analyze('dist/bundle.js', {
  whitelist: ['react', 'react-native', 'react/jsx-runtime', 'rill/sdk'],
  failOnViolation: true,
  treatEvalAsViolation: true,
  treatDynamicNonLiteralAsViolation: true,
});
```

- 构建 Guest Bundle
```bash
bun run rill/cli build src/guest.tsx -o dist/bundle.js
```

## 9. 常见问题

- 在 Node 测试环境引入 react-native 的问题
  - 通过 Vitest alias 映射到轻量 stub，不影响实际生产 bundle。
- 构建/测试报 "Expected 'from', got 'typeOf'"
  - 使用 type-only import，并启用 eslint 规则。
