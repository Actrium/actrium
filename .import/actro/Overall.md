# Canvas + Rill 架构深度分析文档

**文档版本：** 2.1 (2025-12-16)
**最后更新：** 架构重构 - 每 Tab 独立 RCTRootView，删除截图系统

---

## 📊 项目状态总览

### 最新改进 (2025-12-12)

#### rill/core 升级亮点
- ✅ **删除线程池实现** - 简化架构，移除 PooledEngine/WorkerPool 等复杂模块
- ✅ **测试覆盖率提升** - 从 88.94% → 89.66% (+0.72%)
- ✅ **Engine 核心覆盖率** - 从 81.06% → 92.72% (+11.66%)
- ✅ **关键错误路径测试** - 新增 timeout/forceDestroy/async-errors 测试
- ✅ **Biome 代码质量工具** - 统一格式化和 linting（123 个文件）
- ✅ **零 lint 错误** - 严格的代码质量标准

**新增测试覆盖**：
- `engine.timeout.test.ts` - timeout 触发 fatalError、forceDestroy 异常处理、timer 清理
- `engine.async-errors.test.ts` - setInterval 错误捕获、Promise rejection、callback 调用失败
- `oxcAdapter.test.ts` - 依赖扫描和验证
- `ErrorBoundary.test.tsx` - React 错误边界全面测试

#### askit 升级亮点
- ✅ **Biome 工具链** - 与 rill 保持一致的代码质量标准
- ✅ **测试数量增加** - 从 171 → 187 个测试 (+16 个边界情况测试)
- ✅ **全面错误处理测试** - EventEmitter 消息队列、重试逻辑、错误降级
- ✅ **45 个文件格式化** - 一致的代码风格（2空格、单引号、分号）
- ✅ **CI/CD 就绪** - prepublishOnly 包含 biome check:ci

**新增测试覆盖**：
- `EventEmitter.guest.edge-cases.test.ts` - 16 个边界情况测试
  - setMaxListeners/getMaxListeners
  - Pattern listener 错误处理
  - 消息队列溢出和重试逻辑
  - emit 参数验证
  - 本地监听器通知

### 特性

#### 代码质量保证
```bash
# rill 和 askit 均支持
bun run format      # Biome 自动格式化
bun run check       # Lint 检查并自动修复
bun run check:ci    # CI 环境严格检查
bun run typecheck   # TypeScript 类型检查
bun run test        # 完整测试套件
```

#### 测试质量指标
- **rill Engine.ts**: 92.72% 覆盖率（包含所有关键错误路径）
- **askit EventEmitter**: 覆盖消息队列、重试、错误降级全流程
- **零容忍**: eval()、动态 import、非白名单模块
- **自动化**: prepublishOnly hooks 确保发布前质量检查

---

## 一、系统概览

这是一个基于 **Rill 动态 UI 引擎** 的 React Native 应用架构，实现了沙箱化的插件系统。核心思想是：

- **Host (actr-host)**: React Native macOS 应用，负责渲染和安全执行
- **Guest (counterapp)**: 独立打包的插件应用（.actr 格式），运行在沙箱中
- **Rill**: 连接 Host 和 Guest 的桥接层，提供安全的通信机制
- **askit**: 同构 UI 组件库和 API，为 Host 和 Guest 提供统一接口

---

## 二、模块架构关系

### 2.1 模块依赖图

```
┌─────────────────────────────────────────────────────────────┐
│                     Canvas actr-host                        │
│  (React Native macOS App - Host Runtime)                    │
│                                                              │
│  ┌──────────────┐         ┌──────────────────┐             │
│  │  index.ts    │────────►│  ActrApp.tsx     │             │
│  │  (Entry)     │         │  (3-Panel Layout)│             │
│  └──────────────┘         └────────┬─────────┘             │
│                                    │                         │
│                           ┌────────▼─────────┐              │
│                           │ GuestAppView.tsx │              │
│                           │ (Plugin Loader)  │              │
│                           └────────┬─────────┘              │
│                                    │                         │
│       ┌────────────────────────────┼────────────────┐       │
│       │                            │                 │       │
│  ┌────▼──────┐          ┌─────────▼──────┐   ┌─────▼────┐ │
│  │  Engine   │          │FunctionQuickJS │   │ Component│ │
│  │ (rill)    │          │   Provider     │   │ Registry │ │
│  └────┬──────┘          └────────────────┘   └──────────┘ │
│       │                                                     │
└───────┼─────────────────────────────────────────────────────┘
        │ uses
        │
┌───────▼─────────────────────────────────────────────────────┐
│                    Rill Core Library v0.3.0                 │
│  																			(rill/packages/core)  │
│  🟢 测试覆盖率 89.66% | Biome + ESLint             │
│                                                              │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────┐    │
│  │  Engine.ts  │───►│ Receiver.ts  │───►│ Registry   │    │
│  │  (Sandbox)  │    │ (Ops→React)  │    │ (Components)│   │
│  │  92.72%覆盖 │    └──────────────┘    └────────────┘    │
│  └──────┬──────┘                                            │
│         │ injects                                           │
│  ┌──────▼────────────────────┐                             │
│  │  Reconciler (index.ts)    │                             │
│  │  - React→Operations       │                             │
│  │  - CallbackRegistry       │                             │
│  │  - OperationCollector     │                             │
│  └───────────────────────────┘                             │
│                                                              │
│  SDK (Virtual Components):                                  │
│  ┌────────────────────────────────────────┐                │
│  │ View, Text, Image, ScrollView, etc.    │                │
│  │ useHostEvent, useConfig, useSendToHost │                │
│  │ ErrorBoundary (测试覆盖率 100%)        │                │
│  └────────────────────────────────────────┘                │
└──────────────────────────────────────────────────────────────┘
                    │ used by
                    │
┌───────────────────▼──────────────────────────────────────────┐
│                    askit v0.1.0                              │
│  																			 (askit)               │
│  🟢 测试覆盖率 93.56% 行 / 78.81% 函数             │
│                                                              │
│  Core APIs:                                                  │
│  ┌──────────────────────────────────────────────────┐      │
│  │ EventEmitter (Host ↔ Guest 事件总线)            │      │
│  │ - 消息队列 & 自动重试 (max 3 retries)           │      │
│  │ - Pattern matching ('user:*', 'analytics:**')   │      │
│  │ - Rate limiting (throttle/debounce)              │      │
│  │ - 完整错误处理 & 降级                            │      │
│  ├──────────────────────────────────────────────────┤      │
│  │ Toast (跨平台 toast 通知)                       │      │
│  │ Haptic (触觉反馈)                                │      │
│  │ Bridge (模块路由 & 消息处理)                    │      │
│  └──────────────────────────────────────────────────┘      │
│                                                              │
│  UI Components (同构):                                       │
│  ┌──────────────────────────────────────────────────┐      │
│  │ ChatBubble, StepList, ThemeView, UserAvatar      │      │
│  │ (每个组件都有 .host.tsx 和 .guest.tsx 实现)     │      │
│  └──────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────┘
                    │ used by
                    │
┌───────────────────▼──────────────────────────────────────────┐
│              Guest App (counterapp)                          │
│  (Canvas/Artifacts/actr-app-examples/     │
│   counterapp/)                                               │
│                                                              │
│  ┌──────────────────┐          ┌──────────────────┐        │
│  │ left-panel.tsx   │          │ right-panel.tsx  │        │
│  │ (React Component)│          │ (React Component)│        │
│  └────────┬─────────┘          └────────┬─────────┘        │
│           │                              │                   │
│           │ import { View, Text, ... }   │                   │
│           └──────────┬───────────────────┘                   │
│                      │                                       │
│               from 'react-native'                            │
│                  (SDK alias)                                 │
│                                                              │
│  Build Output (.actr package):                              │
│  ┌────────────────────────────────────────────────┐        │
│  │ manifest.json                                   │        │
│  │ left-panel.js   (IIFE bundle)                  │        │
│  │ right-panel.js  (IIFE bundle)                  │        │
│  └────────────────────────────────────────────────┘        │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 核心依赖关系

**actr-host package.json 依赖：**
```json
{
  "react": "19.0.0",
  "react-native": "0.79.6",
  "react-reconciler": "^0.31.0",
  "rill": "file:../../rill/packages/core",
  "askit": "github:GoAskAway/askit"
}
```

**Rill Core 对等依赖：**
```json
{
  "peerDependencies": {
    "react": ">=18.0.0",
    "react-native": ">=0.70.0",
    "react-reconciler": ">=0.29.0"
  }
}
```

**Rill Core 工具链：**
```json
{
  "devDependencies": {
    "@biomejs/biome": "^2.3.8",
    "@types/bun": "latest",
    "bun": "^1.0.0"
  },
  "scripts": {
    "format": "biome format --write .",
    "check": "biome check --write .",
    "check:ci": "biome ci .",
    "test": "bun test",
    "test:coverage": "bun test --coverage"
  }
}
```

**askit 工具链：**
```json
{
  "devDependencies": {
    "@biomejs/biome": "^2.3.8",
    "@eslint/js": "^9.39.1",
    "@typescript-eslint/eslint-plugin": "^8.48.0",
    "react": "19.0.0",
    "react-native": "0.79.6",
    "typescript": "^5.3.0"
  },
  "scripts": {
    "format": "biome format --write src",
    "check": "biome check --write src",
    "check:ci": "biome ci src",
    "lint": "eslint src",
    "test": "bun test",
    "prepublishOnly": "bun run typecheck && bun run check:ci && bun test"
  }
}
```

**counterapp devDependencies：**
```json
{
  "dependencies": {
    "react": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "typescript": "^5.7.2"
  }
}
```

---

## 三、关键类型定义

### 3.1 核心数据结构 (types.ts)

```typescript
// ============ Operation Types (Sandbox → Host) ============

export type OperationType =
  | 'CREATE'    // 创建节点
  | 'UPDATE'    // 更新节点属性
  | 'DELETE'    // 删除节点
  | 'APPEND'    // 追加子节点
  | 'INSERT'    // 插入子节点
  | 'REMOVE'    // 移除子节点
  | 'REORDER'   // 重排子节点
  | 'TEXT';     // 更新文本

// 操作批次
export interface OperationBatch {
  version: number;
  batchId: number;
  operations: Operation[];
}

// 序列化的属性
export interface SerializedFunction {
  __type: 'function';
  __fnId: string;  // 回调函数 ID
}

// ============ Host Message Types (Host → Sandbox) ============

export type HostMessageType =
  | 'CALL_FUNCTION'   // 调用 Guest 回调函数
  | 'HOST_EVENT'      // Host 发送事件到 Guest
  | 'CONFIG_UPDATE'   // 更新配置
  | 'DESTROY';        // 销毁沙箱

// ============ Virtual Node ============

export interface VNode {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: VNode[];
  parent: VNode | null;
  registeredFnIds?: Set<string>;  // 注册的回调函数 ID
}

// Host 侧的节点实例
export interface NodeInstance {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: number[];  // 子节点 ID 列表
}
```

### 3.2 Engine 接口 (已简化 - 删除线程池)

```typescript
export interface EngineOptions {
  provider?: JSEngineProvider;  // JS 引擎提供者
  timeout?: number;              // 执行超时（默认 5000ms）
  debug?: boolean;
  logger?: { log, warn, error };
  requireWhitelist?: readonly string[];  // 允许的模块白名单
  onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
  receiverMaxBatchSize?: number;  // 单批次最大操作数（默认 5000）
}

export interface JSEngineContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

export interface JSEngineProvider {
  createRuntime(): Promise<JSEngineRuntime> | JSEngineRuntime;
}
```

**架构简化说明**：
- ❌ 删除了 `PooledEngine` - 复杂的线程池管理
- ❌ 删除了 `WorkerPool` - Worker 池化机制
- ❌ 删除了 `PooledWorkerProvider` - Worker 提供者池化
- ✅ 保留了简单高效的单 Engine 实例模型
- ✅ 每个 GuestAppView 独立创建 Engine
- ✅ 通过 React lifecycle 自动管理生命周期

---

## 四、构建流程详解

### 4.1 Rill CLI 构建 Guest Bundle

**位置：** `rill/packages/cli/src/build.ts`

**核心流程：**

```typescript
// 1. 使用 Vite 构建
const viteConfig: InlineConfig = {
  build: {
    lib: {
      entry: entryPath,
      name: '__RillGuest',     // IIFE 全局变量名
      formats: ['iife'],
    },
    rollupOptions: {
      external: [
        'react',
        'react/jsx-runtime',
        'react-native',
        'rill/reconciler',
        'rill/sdk'
      ],
      output: {
        globals: {
          'react': 'React',
          'react/jsx-runtime': 'ReactJSXRuntime',
          'react-native': 'ReactNative',
          'rill/reconciler': 'RillReconciler',
          'rill/sdk': 'RillSDK'
        }
      }
    }
  }
};

// 2. 依赖分析和验证（安全保证）
await analyze(bundlePath, {
  whitelist: ['react', 'react-native', 'rill/reconciler', 'rill/sdk'],
  failOnViolation: true,
  treatEvalAsViolation: true,
  treatDynamicNonLiteralAsViolation: true
});
```

**Bundle 结构：**

```javascript
// 1. Runtime Injection (回调注册、事件系统)
(function() {
  var __callbacks = new Map();
  globalThis.__registerCallback = function(fn) { ... };
  globalThis.__invokeCallback = function(fnId, args) { ... };
  globalThis.__useHostEvent = function(eventName, callback) { ... };
  globalThis.__handleHostMessage = function(message) { ... };
})();

// 2. Guest 组件代码 (IIFE)
var __RillGuest = (function(ReactJSXRuntime, React, ReactNative) {
  "use strict";
  // 组件实现...
  return GuestComponent;
})(ReactJSXRuntime, React, ReactNative);

// 3. Auto-render Footer
(function() {
  var React = require('react');
  var reconciler = require('rill/reconciler');
  var Component = __RillGuest;
  var element = React.createElement(Component);
  reconciler.render(element, __sendToHost);
})();
```

**External Dependencies 处理：**
- `react`, `react/jsx-runtime` → 由 Engine 在沙箱中注入
- `rill/reconciler` → Engine 的 `require()` polyfill 提供
- `rill/sdk` → 虚拟模块，返回字符串组件名和 hooks

### 4.2 打包流程：counterapp → .actr

**脚本：** `Canvas/Artifacts/actr-app-examples/pack-actr.sh`

```bash
# 1. 进入源目录
cd counterapp

# 2. 构建左右面板（使用 rill CLI）
rill build src/left-panel.tsx --outfile dist/left-panel.js
rill build src/right-panel.tsx --outfile dist/right-panel.js

# 3. 打包成 .actr (zip 格式)
zip -r ../counterapp.actr manifest.json left-panel.js right-panel.js
```

**manifest.json 结构：**
```json
{
  "name": "counterapp",
  "version": "1.0.0",
  "description": "Interactive Counter Test App",
  "layout": {
    "leftPanel": "left-panel.js",
    "rightPanel": "right-panel.js",
    "rightPanelDefaultVisible": true
  }
}
```

**最终 .actr 内容：**
```
counterapp.actr (Zip Archive)
├── manifest.json
├── left-panel.js   (4160 bytes, minified IIFE)
└── right-panel.js  (5337 bytes, minified IIFE)
```

### 4.3 Vite 配置关键点

**Rollup External Modules:**
```javascript
external: [
  'react',
  'react/jsx-runtime',
  'react-native',
  'rill/reconciler',
  'rill/sdk'
]
```

**这些模块不会被打包进 bundle，而是：**
1. `react`, `react/jsx-runtime` → Engine 通过 `setGlobal()` 注入
2. `rill/reconciler` → Engine 的 `require()` polyfill 返回 reconciler 模块
3. `rill/sdk` → 虚拟模块，返回组件字符串名称

**代码验证（analyze.report.json）：**
```json
{
  "deps": [
    "react",
    "rill/reconciler"
  ],
  "dynamicNonLiteral": 0,
  "evalCount": 0
}
```

---

## 五、运行时流程详解

### 5.1 Canvas 加载 .actr 完整流程

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Host 启动 (index.ts)                                     │
├─────────────────────────────────────────────────────────────┤
│ AppRegistry.registerComponent('ActrApp', () => ActrApp)     │
│ - 注册 React Native 组件                                    │
│ - 配置日志系统（NativeLogger + log-server:9999）           │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 2. ActrApp 渲染三栏布局                                     │
├─────────────────────────────────────────────────────────────┤
│ <View flexDirection="row">                                  │
│   {/* Left Panel (240px) */}                               │
│   <GuestAppView script={leftPanelScript} />                │
│                                                              │
│   {/* Center Panel (flex) */}                              │
│   <ScrollView>对话内容</ScrollView>                         │
│                                                              │
│   {/* Right Panel (400px) */}                              │
│   <GuestAppView script={rightPanelScript} />               │
│ </View>                                                     │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 3. GuestAppView 初始化 Engine                               │
├─────────────────────────────────────────────────────────────┤
│ React.useEffect(() => {                                     │
│   // 3.1 创建 QuickJS Provider                             │
│   const provider = await createFunctionQuickJSProvider()   │
│                                                              │
│   // 3.2 创建 Engine (单实例，无池化)                      │
│   const engine = new Engine({ provider, debug: true })     │
│                                                              │
│   // 3.3 注册 Host 组件                                    │
│   engine.register({                                         │
│     View, Text, Image, ScrollView,                         │
│     TouchableOpacity, Button, ...                          │
│   })                                                        │
│                                                              │
│   // 3.4 创建 Receiver                                     │
│   engine.createReceiver(handleUpdate)                      │
│                                                              │
│   // 3.5 加载 Guest Bundle (带超时保护)                   │
│   await engine.loadBundle(script, { ... })                 │
│ }, [script])                                                │
│                                                              │
│ // 3.6 清理 (组件卸载时自动调用)                          │
│ return () => engine.destroy()                              │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 4. Engine 初始化沙箱 (engine.ts)                            │
├─────────────────────────────────────────────────────────────┤
│ // 4.1 创建 QuickJS Context                                │
│ this.runtime = await provider.createRuntime()              │
│ this.context = this.runtime.createContext()                │
│                                                              │
│ // 4.2 注入 Polyfills                                      │
│ this.context.setGlobal('React', React)                     │
│ this.context.setGlobal('ReactJSXRuntime', ReactJSXRuntime)│
│ this.context.setGlobal('console', { log, warn, error })   │
│                                                              │
│ // 4.3 注入 require() polyfill                             │
│ this.context.setGlobal('require', (moduleName) => {        │
│   switch (moduleName) {                                    │
│     case 'react': return React;                            │
│     case 'rill/reconciler': return RillReconciler;         │
│     case 'rill/sdk': return { View: 'View', ... };        │
│   }                                                         │
│ })                                                          │
│                                                              │
│ // 4.4 注入 __sendToHost (发送操作批次到 Host)             │
│ this.context.setGlobal('__sendToHost', (batch) => {        │
│   this.receiver.applyBatch(batch)                          │
│ })                                                          │
│                                                              │
│ // 4.5 超时保护 (新增特性)                           │
│ const timeoutPromise = new Promise((_, reject) =>          │
│   setTimeout(() => {                                        │
│     this.forceDestroy()  // 强制销毁并清理资源            │
│     reject(new TimeoutError('Execution timeout'))          │
│   }, this.timeout)                                          │
│ )                                                           │
│                                                              │
│ await Promise.race([executeBundle(code), timeoutPromise]) │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 5. 执行 Guest Bundle                                        │
├─────────────────────────────────────────────────────────────┤
│ // FunctionQuickJSProvider.eval()                          │
│ const fn = new Function(...globalNames, code)             │
│ fn(...globalValues)                                        │
│                                                              │
│ ↓ Bundle 执行流程：                                         │
│                                                              │
│ 5.1 Runtime Inject 初始化                                  │
│     - __registerCallback                                   │
│     - __useHostEvent                                       │
│     - __handleHostMessage                                  │
│                                                              │
│ 5.2 __RillGuest IIFE 执行                                  │
│     - 定义组件：LeftPanel / RightPanel                      │
│                                                              │
│ 5.3 Auto-render Footer                                     │
│     ├─ require('react')           → React                  │
│     ├─ require('rill/reconciler') → RillReconciler         │
│     ├─ React.createElement(Component)                      │
│     └─ reconciler.render(element, __sendToHost)            │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 6. Reconciler 渲染 (reconciler/index.ts)                    │
├─────────────────────────────────────────────────────────────┤
│ render(element, sendToHost) {                              │
│   // 6.1 创建 React Reconciler 容器                        │
│   const container = { children: [] }                       │
│   const root = reconciler.createContainer(container, ...)  │
│                                                              │
│   // 6.2 更新容器触发 React 渲染                           │
│   reconciler.updateContainer(element, root, ...)           │
│                                                              │
│   ↓ React 渲染过程：                                        │
│                                                              │
│   6.3 createInstance('View', { style: {...} })             │
│       ├─ 生成节点 ID                                       │
│       ├─ 序列化 props (函数 → fnId)                        │
│       └─ collector.add({ op: 'CREATE', id, type, props })  │
│                                                              │
│   6.4 appendInitialChild(parent, child)                    │
│       └─ collector.add({ op: 'APPEND', parentId, childId })│
│                                                              │
│   6.5 commitUpdate() / commitTextUpdate()                  │
│       └─ collector.add({ op: 'UPDATE', ... })              │
│                                                              │
│   6.6 finalizeInitialChildren()                            │
│       └─ collector.flush(sendToHost)                       │
│           发送 OperationBatch 到 Host                       │
│ }                                                           │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  │ OperationBatch
                  ▼
┌─────────────────────────────────────────────────────────────┐
│ 7. Receiver 应用操作 (receiver.ts)                          │
├─────────────────────────────────────────────────────────────┤
│ applyBatch(batch) {                                         │
│   batch.operations.forEach(op => {                         │
│     switch (op.op) {                                       │
│       case 'CREATE':                                       │
│         this.nodeMap.set(op.id, {                          │
│           id: op.id,                                       │
│           type: op.type,                                   │
│           props: this.deserializeProps(op.props),          │
│           children: []                                     │
│         })                                                  │
│         break                                              │
│                                                              │
│       case 'APPEND':                                       │
│         if (op.parentId === 0) {                           │
│           this.rootChildren.push(op.childId)               │
│         } else {                                           │
│           parent.children.push(op.childId)                 │
│         }                                                  │
│         break                                              │
│                                                              │
│       case 'UPDATE': ...                                   │
│     }                                                      │
│   })                                                        │
│   this.scheduleUpdate()  // 触发 onUpdate 回调             │
│ }                                                           │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│ 8. Receiver 构建 React 树                                   │
├─────────────────────────────────────────────────────────────┤
│ render(): React.ReactElement {                             │
│   return this.rootChildren.map(id => this.renderNode(id))  │
│ }                                                           │
│                                                              │
│ renderNode(id) {                                           │
│   const node = this.nodeMap.get(id)                        │
│   const Component = this.registry.get(node.type)           │
│   const children = node.children.map(id =>                 │
│     this.renderNode(id)                                    │
│   )                                                         │
│   return React.createElement(                              │
│     Component,                                             │
│     { ...node.props, key: `rill-${id}` },                 │
│     ...children                                            │
│   )                                                         │
│ }                                                           │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  │ React Tree
                  ▼
┌─────────────────────────────────────────────────────────────┐
│ 9. React Native 渲染到屏幕                                  │
├─────────────────────────────────────────────────────────────┤
│ <LocalEngineView>                                          │
│   {content}   ← receiver.render() 返回的 React 元素        │
│ </LocalEngineView>                                         │
│                                                              │
│ ↓ 最终渲染的组件树：                                        │
│                                                              │
│ <ScrollView style={{flex:1, backgroundColor:'#1a1a2e'}}>  │
│   <Text>🎮 计数器测试</Text>                               │
│   <View>总点击次数: 0</View>                               │
│   <TouchableOpacity onPress={fn_123}>                     │
│     <Text>🔴 红色 +1</Text>                                │
│   </TouchableOpacity>                                      │
│   ...                                                      │
│ </ScrollView>                                              │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Metro Bundler 的角色

**Metro 配置：** `Canvas/actr-host/metro.config.js`

```javascript
module.exports = {
  resolver: {
    unstable_enableSymlinks: true,
    extraNodeModules: {
      'react': path.resolve(projectRoot, 'node_modules/react'),
      'react-reconciler': path.resolve(projectRoot, 'node_modules/react-reconciler'),
      'rill': path.resolve(projectRoot, 'node_modules/rill'),
      'askit': path.resolve(projectRoot, 'node_modules/askit')
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs']
  }
};
```

**Metro 负责：**
1. **编译 Host 代码**（index.ts, ActrApp.tsx, GuestAppView.tsx）
2. **解析模块依赖**（rill, askit, react-native）
3. **支持 symlink**（`file:../../rill/packages/core`）
4. **生成 Host Bundle**（运行在 React Native macOS 环境）

**Metro 不处理 Guest 代码：**
- Guest 代码由 `rill CLI` 使用 Vite 独立打包
- Guest bundle 作为**字符串**在运行时加载
- 完全隔离，不经过 Metro 处理

### 5.3 模块提供机制详解

#### Guest 中的 require() 调用

```typescript
// Guest 代码 (left-panel.tsx)
import React, { useState } from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
```

**编译后（Vite）：**
```javascript
var React = require('react');
var ReactNative = require('react-native');
// 使用 ReactNative.View, ReactNative.Text, ...
```

**运行时拦截：**
```typescript
// Engine.injectPolyfills() 注入的 require()
this.context.setGlobal('require', (moduleName: string) => {
  switch (moduleName) {
    case 'react':
      return React;  // Host 的 React 实例

    case 'react-native':
      return ReactNative;  // 实际返回 RillSDK

    case 'rill/reconciler':
      return RillReconciler;  // Reconciler 模块

    case 'rill/sdk':
      // 虚拟模块，返回字符串组件名
      return {
        View: 'View',
        Text: 'Text',
        ScrollView: 'ScrollView',
        TouchableOpacity: 'TouchableOpacity',
        useHostEvent: this.context.getGlobal('__useHostEvent'),
        useConfig: this.context.getGlobal('__getConfig'),
        useSendToHost: () => this.context.getGlobal('__sendEventToHost')
      };

    default:
      throw new RequireError(`Unsupported require("${moduleName}")`);
  }
});
```

#### 组件名称的映射

**Guest 使用：**
```jsx
<View style={{flex: 1}}>
  <Text>Hello</Text>
</View>
```

**Reconciler 接收：**
```typescript
createInstance(type: 'View', props: { style: {flex: 1} })
// 生成操作：
{ op: 'CREATE', id: 1, type: 'View', props: { style: {flex: 1} } }
```

**Receiver 映射：**
```typescript
renderNode(id) {
  const node = this.nodeMap.get(id);  // { id: 1, type: 'View', ... }
  const Component = this.registry.get('View');  // → React Native View
  return React.createElement(Component, node.props, ...children);
}
```

**组件注册：**
```typescript
// GuestAppView.tsx
const hostComponents = {
  View: View as React.ComponentType,
  Text: Text as React.ComponentType,
  ScrollView: PatchedScrollView as React.ComponentType,
  TouchableOpacity: TouchableOpacity as React.ComponentType,
  // ...
};
engine.register(hostComponents);
```

---

## 六、日志系统

### 6.1 NativeLogger (Xcode Console)

**位置：** React Native Native Module

```typescript
// index.ts
const NativeLogger = NativeModules.NativeLogger;
NativeLogger?.log?.('Message');
NativeLogger?.warn?.('Warning');
NativeLogger?.error?.('Error');
```

**输出到：** Xcode Debug Console

### 6.2 远程日志服务器 (端口 9999)

**服务器：** `Canvas/actr-host/log-server.js`

```javascript
const http = require('http');
const PORT = 9999;

http.createServer((req, res) => {
  if (req.method === 'POST' && req.url === '/log') {
    const data = JSON.parse(body);
    console.log(`[${time}] ${prefix} ${data.msg}`);
  }
}).listen(9999);
```

**启动：**
```bash
node log-server.js
```

**客户端使用：**
```typescript
const sendLog = (level: string, msg: string) => {
  fetch('http://localhost:9999/log', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ level, msg })
  }).catch(() => {});

  // 同时输出到 Xcode
  NativeModules.NativeLogger?.[level]?.(msg);
};

const nlog = (msg: string) => sendLog('log', msg);
```

**特点：**
- **双输出**：同时发送到 HTTP 服务器和 Xcode
- **彩色输出**：终端显示带颜色的日志
- **非阻塞**：fetch 失败静默处理

### 6.3 Guest 日志

**Guest 中的 console.log：**
```typescript
// Engine.injectPolyfills()
this.context.setGlobal('console', {
  log: (...args) => {
    if (debug) logger.log('[Guest]', ...args);
  },
  warn: (...args) => logger.warn('[Guest]', ...args),
  error: (...args) => logger.error('[Guest]', ...args)
});
```

**日志流向：**
```
Guest console.log()
  → Engine logger
  → sendLog()
  → [HTTP Server 9999] + [NativeLogger]
```

---

## 七、关键流程图

### 7.1 事件处理流程

```
┌──────────────────────────────────────────────────────────────┐
│ 用户点击按钮                                                  │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Host: React Native TouchableOpacity.onPress                  │
│   props.onPress()  ← Receiver 反序列化的函数代理             │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Receiver.deserializeValue()                                  │
│   return (...args) => {                                      │
│     this.sendToSandbox({                                     │
│       type: 'CALL_FUNCTION',                                 │
│       fnId: 'fn_123_abc',                                   │
│       args: args                                             │
│     })                                                       │
│   }                                                          │
└────────────┬─────────────────────────────────────────────────┘
             │ HostMessage
┌────────────▼─────────────────────────────────────────────────┐
│ Engine.sendToSandbox()                                       │
│   this.context.getGlobal('__handleHostMessage')(message)    │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Guest: Runtime Inject __handleHostMessage                    │
│   case 'CALL_FUNCTION':                                      │
│     __invokeCallback(message.fnId, message.args)             │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Guest: CallbackRegistry.invoke('fn_123_abc', [])             │
│   const fn = this.callbacks.get('fn_123_abc')                │
│   fn()  ← 原始的 onPress 回调                                │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Guest: setState(count + 1)                                   │
│   → React re-render                                          │
│   → Reconciler 生成 UPDATE 操作                              │
│   → collector.flush(sendToHost)                              │
└────────────┬─────────────────────────────────────────────────┘
             │ OperationBatch
┌────────────▼─────────────────────────────────────────────────┐
│ Host: Receiver.applyBatch()                                  │
│   → handleUpdate() → receiver.render()                       │
│   → setState(newContent)                                     │
└────────────┬─────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────┐
│ Host: React Native re-render                                 │
│   显示更新后的 UI                                            │
└──────────────────────────────────────────────────────────────┘
```

### 7.2 数据流向图

```
┌───────────────────────────────────────────────────────────┐
│                    Guest Sandbox                          │
│                                                           │
│  React Component                                          │
│       ↓                                                   │
│  React.createElement(View, props)                         │
│       ↓                                                   │
│  Reconciler (react-reconciler)                            │
│       ├─ createInstance()                                 │
│       ├─ appendInitialChild()                             │
│       └─ commitUpdate()                                   │
│           ↓                                               │
│  OperationCollector                                       │
│       ↓                                                   │
│  OperationBatch {                                         │
│    batchId: 1,                                            │
│    operations: [                                          │
│      { op: 'CREATE', id: 1, type: 'View', props: {...} },│
│      { op: 'APPEND', parentId: 0, childId: 1 },          │
│      { op: 'CREATE', id: 2, type: 'Text', props: {...} },│
│      { op: 'APPEND', parentId: 1, childId: 2 }           │
│    ]                                                      │
│  }                                                        │
└───────────────────┬───────────────────────────────────────┘
                    │ sendToHost(batch)
                    │ (通过 __sendToHost 全局函数)
┌───────────────────▼───────────────────────────────────────┐
│                    Host Runtime                           │
│                                                           │
│  Receiver.applyBatch(batch)                               │
│       ├─ handleCreate()    → nodeMap.set(id, node)       │
│       ├─ handleAppend()    → rootChildren.push(childId)  │
│       └─ scheduleUpdate()  → queueMicrotask(onUpdate)    │
│           ↓                                               │
│  onUpdate() callback                                      │
│       ↓                                                   │
│  receiver.render()                                        │
│       ├─ renderNode(1)                                    │
│       │   ├─ registry.get('View') → <View />            │
│       │   └─ renderNode(2)                               │
│       │       └─ registry.get('Text') → <Text />        │
│       │                                                   │
│       └─ React.createElement(View, props,                │
│             React.createElement(Text, props))             │
│           ↓                                               │
│  React Element Tree                                       │
│       ↓                                                   │
│  React Native Renderer                                    │
│       ↓                                                   │
│  Native UI (macOS)                                        │
└───────────────────────────────────────────────────────────┘
```

---

## 八、性能优化机制

### 8.1 批量操作

**OperationCollector：**
- 收集 React 渲染阶段的所有操作
- 在 commit 阶段一次性发送
- 减少跨沙箱通信次数

### 8.2 操作限流

**Receiver.maxBatchSize：**
```typescript
receiverMaxBatchSize: 5000  // 单批次最大操作数
```

**超限处理：**
```typescript
if (applied >= limit) {
  skipped += (batch.operations.length - i);
  break;
}
// 发送 RECEIVER_BACKPRESSURE 事件通知 Guest
```

### 8.3 Microtask 调度

```typescript
scheduleUpdate() {
  if (this.updateScheduled) return;
  this.updateScheduled = true;

  queueMicrotask(() => {
    this.updateScheduled = false;
    this.onUpdate();
  });
}
```

**优势：**
- 批量更新完成后再触发 UI 刷新
- 避免同步渲染卡顿

---

## 九、安全机制

### 9.1 模块白名单

```typescript
requireWhitelist: new Set([
  'react',
  'react-native',
  'react/jsx-runtime',
  'rill/reconciler',
  'rill/sdk'
])
```

**任何其他 require() 调用都会抛出 RequireError**

### 9.2 严格依赖检查 (保证)

**Build 时分析 (oxcAdapter)：**
```typescript
await analyze(bundlePath, {
  whitelist: ['react', 'react-native', 'rill/reconciler', 'rill/sdk'],
  failOnViolation: true,
  treatEvalAsViolation: true,
  treatDynamicNonLiteralAsViolation: true
});
```

**检测内容：**
- 静态 import/require
- 动态 import()
- eval() 调用
- 动态非字面量 import
- Data URLs (自动忽略)
- HTTP/HTTPS imports (自动忽略)
- Bundler 注入的调用 (__webpack_require__, __vite_ssr_import__, 等)

**测试覆盖（新增）：**
- `build.analyze.test.ts` - 基础分析测试
- `build.analyze.ignore.test.ts` - 忽略模式测试
- `build.analyze.edges.test.ts` - 边界情况测试
- `oxcAdapter.test.ts` - oxc 解析器集成测试

### 9.3 Callback 清理

```typescript
// 节点删除时自动清理回调
cleanupNodeCallbacks(node, callbackRegistry) {
  if (node.registeredFnIds) {
    node.registeredFnIds.forEach(fnId => registry.remove(fnId));
  }
  node.children.forEach(child => cleanupNodeCallbacks(child, registry));
}
```

**防止内存泄漏**

### 9.4 执行超时 (保证)

```typescript
timeout: 5000  // 默认 5 秒

// Promise.race 硬超时
Promise.race([
  this.executeBundle(code),
  timeoutPromise  // 超时后强制 destroy Engine
]);

// forceDestroy() 错误处理（新增测试）
forceDestroy() {
  this.destroyed = true;

  // 1. 清理所有定时器
  this.timeoutMap.forEach((id) => clearTimeout(id));
  this.intervalMap.forEach((id) => clearInterval(id));
  this.timeoutMap.clear();
  this.intervalMap.clear();

  // 2. 销毁 context (即使抛出异常也继续)
  try {
    this.context?.dispose();
  } catch (error) {
    // 静默处理 dispose 错误
  }
  this.context = null;

  // 3. 销毁 runtime (即使抛出异常也继续)
  try {
    this.runtime?.dispose();
  } catch (error) {
    // 静默处理 dispose 错误
  }
  this.runtime = null;

  // 4. 触发 fatalError 事件
  this.emit('fatalError', error);
}
```

**新增测试覆盖**：
- Timeout 触发 fatalError 和 forceDestroy
- context.dispose() 抛出异常时的处理
- runtime.dispose() 抛出异常时的处理
- Timer 清理验证

---

## 十、错误处理机制 (特性)

### 10.1 React ErrorBoundary (rill/sdk)

```typescript
// SDK 提供的 ErrorBoundary 组件
<ErrorBoundary fallback={<ErrorView />} onError={handleError}>
  <GuestComponent />
</ErrorBoundary>
```

**测试覆盖 (17 个测试)**：
- 捕获 render 阶段错误
- 捕获 componentDidMount 错误
- 捕获 componentDidUpdate 错误
- 捕获 componentWillUnmount 错误
- 捕获 async useEffect 错误
- 捕获事件处理器错误
- Fallback UI 渲染
- onError callback 调用
- 错误恢复机制
- 多层嵌套 ErrorBoundary
- getDerivedStateFromError 生命周期
- componentDidCatch 生命周期

### 10.2 Engine 异步错误处理

```typescript
// setInterval 回调错误捕获
this.context.setGlobal('setInterval', (callback, delay) => {
  const id = setInterval(() => {
    try {
      callback();
    } catch (error) {
      this.errorCount++;
      this.lastErrorAt = Date.now();
      this.emit('error', error instanceof Error ? error : new Error(String(error)));
    }
  }, delay);
  return id;
});

// Unhandled Promise Rejection 处理
addEventListener('unhandledrejection', (event) => {
  const error = event.reason instanceof Error
    ? event.reason
    : new Error(String(event.reason));
  this.emit('error', error);
  event.preventDefault();
});
```

**测试覆盖 (9 个测试)**：
- setInterval 回调错误捕获
- setInterval 非 Error 异常处理
- Promise rejection Error 类型处理
- Promise rejection 非 Error 类型转换
- handleCallFunction 错误捕获
- handleCallFunction 缺少 context 处理
- handleCallFunction evalCode 失败处理
- errorCount 增量验证
- lastErrorAt 时间戳验证

### 10.3 askit EventEmitter 错误降级

```typescript
// 消息发送失败时自动重试
emit(event: string, payload?: unknown) {
  try {
    global.sendToHost(`askit:event:${event}`, payload);
  } catch (error) {
    logger.error('EventEmitter', `Failed to send event "${event}"`, { error });
    this.queueMessage(event, payload);  // 加入重试队列
  }
}

// 重试逻辑
private processQueue() {
  while (this.messageQueue.length > 0) {
    const msg = this.messageQueue.shift()!;

    if (msg.retries >= this.maxRetries) {
      logger.error('EventEmitter', `Dropped event "${msg.event}" after ${msg.retries} retries`);
      continue;  // 丢弃消息
    }

    try {
      global.sendToHost(`askit:event:${msg.event}`, msg.payload);
      // 成功，不重新入队
    } catch (error) {
      msg.retries++;
      this.messageQueue.push(msg);  // 重新入队
    }
  }

  if (this.messageQueue.length > 0) {
    this.scheduleRetry();  // 调度下次重试
  }
}
```

**测试覆盖 (16 个边界情况)**：
- setMaxListeners/getMaxListeners
- Pattern listener 错误捕获
- 消息队列溢出处理
- 重试耗尽后丢弃消息
- sendToHost 不可用时的降级
- emit 参数验证
- 本地监听器错误隔离
- 重试调度防抖

---

## 十一、开发工作流

### 11.1 代码质量检查

```bash
# rill 项目
cd rill
bun run format        # 格式化所有代码
bun run check         # Lint 检查并自动修复
bun run typecheck     # TypeScript 类型检查
bun run test:coverage # 运行测试并生成覆盖率报告

# askit 项目
cd askit
bun run format        # 格式化所有代码
bun run check         # Lint 检查并自动修复
bun run lint          # ESLint 检查
bun run typecheck     # TypeScript 类型检查
bun run test:coverage # 运行测试并生成覆盖率报告
```

### 11.2 CI/CD 集成

```bash
# rill prepublishOnly hook
bun run typecheck && bun run check:ci && bun test

# askit prepublishOnly hook
bun run typecheck && bun run check:ci && bun test
```

**检查项目**：
- ✅ TypeScript 编译无错误
- ✅ Biome lint 零错误
- ✅ 所有测试通过
- ✅ 覆盖率达标
- ✅ 无 eval()、动态 import 等不安全代码

### 11.3 Guest 应用开发

```bash
# 1. 启动日志服务器
cd Canvas/actr-host
node log-server.js

# 2. 开发 Guest 应用
cd Canvas/Artifacts/actr-app-examples/counterapp
# 编辑 src/left-panel.tsx
rill build src/left-panel.tsx --outfile left-panel.js

# 3. 打包 .actr
cd ..
./pack-actr.sh pack counterapp

# 4. Host 应用热更新
# Metro 自动检测到 .actr 变化，刷新 UI
```

### 11.4 调试技巧

**启用 Debug 日志：**
```typescript
const engine = new Engine({
  debug: true,
  logger: { log: nlog, warn: nwarn, error: nerror }
});
```

**查看操作批次：**
```typescript
// Receiver 中添加
console.log('[Receiver] Batch:', JSON.stringify(batch, null, 2));
```

**查看 Reconciler 调用：**
```typescript
// reconciler/index.ts
console.log('[Reconciler] createInstance:', type, props);
console.log('[Reconciler] appendInitialChild:', parent.id, child.id);
```

**检查 Bundle 依赖：**
```bash
cat counterapp/analyze.report.json
```

**监控 Engine 错误事件：**
```typescript
engine.on('error', (error) => {
  console.error('[Engine Error]', error);
});

engine.on('fatalError', (error) => {
  console.error('[Engine Fatal Error]', error);
  // Engine 已被销毁，需要重新创建
});
```

---

## 十二、总结与最佳实践

### 12.1 架构优势

1. **完全隔离**：Guest 代码运行在独立沙箱，无法访问 Host 环境
2. **类型安全**：TypeScript 端到端类型检查
3. **热更新友好**：Guest bundle 可动态加载，无需重新编译 Host
4. **高性能**：批量操作 + Microtask 调度，减少渲染开销
5. **可扩展**：通过 ComponentRegistry 注册自定义组件
6. **质量**：
   - ✅ 89.66% 测试覆盖率（rill）
   - ✅ 93.56% 行覆盖率（askit）
   - ✅ 全面的错误处理和降级机制
   - ✅ CI/CD 就绪的自动化检查
   - ✅ 零容忍的安全策略

### 12.2 关键文件路径汇总

**Rill Core:**
- `rill/packages/core/src/runtime/engine.ts` - Engine 核心 (92.72% 覆盖)
- `rill/packages/core/src/runtime/receiver.ts` - Receiver
- `rill/packages/core/src/reconciler/index.ts` - Reconciler
- `rill/packages/core/src/types.ts` - 类型定义
- `rill/packages/cli/src/build.ts` - CLI 构建工具
- `rill/packages/cli/src/oxcAdapter.ts` - 依赖分析器
- `rill/biome.json` - 代码质量配置

**Rill Tests ():**
- `rill/packages/core/src/runtime/engine.timeout.test.ts`
- `rill/packages/core/src/runtime/engine.async-errors.test.ts`
- `rill/packages/core/src/sdk/ErrorBoundary.test.tsx`
- `rill/packages/cli/src/oxcAdapter.test.ts`
- `rill/packages/cli/src/build.analyze.*.test.ts`

**askit:**
- `askit/src/api/EventEmitter.guest.ts` - Guest 事件总线
- `askit/src/api/EventEmitter.host.ts` - Host 事件总线
- `askit/src/core/bridge.ts` - 消息路由
- `askit/src/core/typed-bridge.ts` - 类型安全桥接
- `askit/biome.json` - 代码质量配置

**askit Tests ():**
- `askit/src/api/EventEmitter.guest.edge-cases.test.ts`
- `askit/src/api/EventEmitter.guest.test.ts`
- `askit/src/api/EventEmitter.host.test.ts`
- `askit/src/core/bridge.test.ts`

**Canvas Host:**
- `Canvas/actr-host/index.ts` - 应用入口
- `Canvas/actr-host/src/ActrApp.tsx` - 主应用
- `Canvas/actr-host/src/GuestAppView.tsx` - 插件加载器
- `Canvas/actr-host/src/FunctionQuickJSProvider.ts` - JS 引擎
- `Canvas/actr-host/metro.config.js` - Metro 配置
- `Canvas/actr-host/log-server.js` - 日志服务器

**Guest App:**
- `Canvas/Artifacts/actr-app-examples/counterapp/src/left-panel.tsx`
- `Canvas/Artifacts/actr-app-examples/counterapp/src/right-panel.tsx`
- `Canvas/Artifacts/actr-app-examples/counterapp/manifest.json`
- `Canvas/Artifacts/actr-app-examples/pack-actr.sh` - 打包脚本

### 12.3 质量指标

| 指标 | rill/core | askit | 说明 |
|------|-----------|-------|------|
| **测试覆盖率** | 89.66% | 93.56% (行) | 包含所有关键错误路径 |
| **Engine 核心覆盖** | 92.72% | - | timeout/forceDestroy/async |
| **错误处理测试** | ✅ | ✅ | 完整的降级和恢复机制 |
| **代码格式化** | Biome | Biome | 一致的代码风格 |
| **Lint 错误** | 0 | 0 | 严格的质量标准 |
| **CI/CD** | ✅ | ✅ | prepublishOnly hooks |
| **类型安全** | 100% | 100% | 端到端 TypeScript |
| **安全检查** | 零容忍 | 零容忍 | eval/动态 import 检测 |

### 12.4 架构演进路线图

**已完成（2025-12-12）**：
- ✅ 删除线程池实现，简化架构
- ✅ 添加 Biome 工具链
- ✅ 提升测试覆盖率
- ✅ 完整的错误处理和降级机制
- ✅ CI/CD 自动化检查

**未来规划**：
- 🔄 性能监控和指标收集
- 🔄 Guest 应用热重载机制
- 🔄 更多内置组件和 API
- 🔄 插件市场和版本管理
- 🔄 跨平台支持（iOS、Android）

