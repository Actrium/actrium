# Rill 架构设计文档

> 延伸阅读：[内部实现细节](./internals/)（桥接层、序列化、沙箱对比）

## 1. 设计理念

### 1.1 Remote-UI vs Rill

| 维度 | Shopify Remote-UI | Rill |
|------|-------------------|------|
| 目标平台 | Web (DOM) | React Native & Web |
| 沙箱技术 | iframe / Web Worker | 可插拔 JSEngineProvider (JSC, QuickJS, NodeVM, WasmQuickJS) |
| 渲染目标 | 真实 DOM 元素 | RN 组件 |
| 通信方式 | postMessage | JSEngineProvider Bridge (JSI \| Serialize) |
| 变更检测 | MutationObserver | react-reconciler |
| 组件模型 | Custom Elements | 虚组件 |

### 1.2 核心概念

1. **Producer-Consumer 模式**：沙箱产生 UI 描述，宿主消费并渲染
2. **白名单组件安全模型**：只允许渲染已注册的组件类型
3. **函数序列化机制**：回调函数转换为 ID 引用
4. **批量更新优化**：聚合多个操作为单次传输
5. **统一桥接层**：类型安全的序列化，自动回调生命周期管理

---

## 2. 总体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              宿主 App (React Native)                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────────┐  │
│  │  EngineView │───▶│   Engine    │───▶│      JSEngineProvider Context   │  │
│  │  (React)    │    │  (Manager)  │    │  ┌─────────────────────────────┐│  │
│  └─────────────┘    └─────────────┘    │  │     Guest Bundle.js        ││  │
│         │                 │            │  │  ┌─────────────────────────┐││  │
│         │                 │            │  │  │  React + Reconciler     │││  │
│         ▼                 │            │  │  │  (Custom Renderer)      │││  │
│  ┌─────────────┐          │            │  │  └─────────────────────────┘││  │
│  │  Receiver   │◀─────────┘            │  │             │               ││  │
│  │  (指令解析)  │    Operations         │  │             ▼               ││  │
│  └─────────────┘    (序列化数据)        │  │  ┌─────────────────────────┐││  │
│         │                              │  │  │  sendToHost(ops)        │││  │
│         ▼                              │  │  └─────────────────────────┘││  │
│  ┌─────────────┐    ┌─────────────┐    │  └─────────────────────────────┘│  │
│  │  Registry   │    │   Bridge    │    └─────────────────────────────────┘  │
│  │  (组件映射)  │    │  (序列化)   │                                         │
│  └─────────────┘    └─────────────┘                                         │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    原生组件树 (View, Text, Image...)                  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 模块结构

```
rill/
├── src/
│   ├── host/              # 宿主运行时
│   │   ├── Engine.ts      # Engine 主类
│   │   ├── receiver/      # 指令接收器
│   │   ├── registry.ts    # 组件注册表
│   │   ├── engine/        # Engine 内部模块
│   │   └── preset/        # 默认组件预设
│   │
│   ├── guest/             # Guest 端代码
│   │   ├── let/           # Guest SDK (rill/let)
│   │   │   ├── index.ts   # SDK 导出
│   │   │   ├── sdk.ts     # Hooks 实现
│   │   │   └── types.ts   # 类型定义
│   │   ├── runtime/       # Guest 运行时
│   │   │   └── reconciler/  # 自定义渲染器
│   │   └── build/         # 构建后的运行时
│   │
│   ├── guest-bundle/      # Guest bundle 入口
│   │   ├── entry.ts       # Bundle 入口点
│   │   └── build/         # 自动生成的 bundle 输出
│   │
│   ├── shared/            # 共享工具
│   │   ├── index.ts       # 导出
│   │   ├── types.ts       # 类型定义
│   │   ├── TypeRules.ts   # 序列化规则
│   │   ├── bridge/        # Bridge 层
│   │   └── CallbackRegistry.ts
│   │
│   ├── sandbox/           # 沙箱提供者
│   │   ├── types/         # Provider 接口
│   │   ├── providers/     # VM, JSC, QuickJS 提供者
│   │   ├── native/        # 原生 JSI 绑定
│   │   ├── wasm/          # WASM QuickJS 沙箱
│   │   └── web/           # Web Worker 沙箱
│   │
│   ├── cli/               # CLI 构建工具
│   │   └── build.ts       # Bun 打包器
│   │
│   └── devtools/          # 开发者工具
```

---

## 4. 数据流设计

### 4.1 渲染流程 (Guest → Host)

```
Guest JSX           Reconciler              Bridge              Receiver           原生组件
   │                   │                     │                    │                  │
   │  <View>           │                     │                    │                  │
   │──────────────────▶│                     │                    │                  │
   │                   │ createInstance()    │                    │                  │
   │                   │ ─────────────────┐  │                    │                  │
   │                   │                  │  │                    │                  │
   │                   │◀─────────────────┘  │                    │                  │
   │                   │                     │                    │                  │
   │                   │ appendChild()       │                    │                  │
   │                   │ ─────────────────┐  │                    │                  │
   │                   │                  │  │                    │                  │
   │                   │◀─────────────────┘  │                    │                  │
   │                   │                     │                    │                  │
   │                   │    Commit Phase     │                    │                  │
   │                   │────────────────────▶│                    │                  │
   │                   │   [Operation[]]     │                    │                  │
   │                   │                     │ serialize(ops)     │                  │
   │                   │                     │───────────────────▶│                  │
   │                   │                     │                    │ applyOperations  │
   │                   │                     │                    │─────────────────▶│
   │                   │                     │                    │                  │
```

### 4.2 事件流程 (Host → Guest)

```
用户点击           原生组件             Receiver              Bridge             Guest
   │                 │                    │                    │                   │
   │  onPress        │                    │                    │                   │
   │────────────────▶│                    │                    │                   │
   │                 │ handlePress(fnId)  │                    │                   │
   │                 │───────────────────▶│                    │                   │
   │                 │                    │ callFunction       │                   │
   │                 │                    │───────────────────▶│                   │
   │                 │                    │                    │ invoke(fnId,args) │
   │                 │                    │                    │──────────────────▶│
   │                 │                    │                    │                   │
   │                 │                    │                    │   [new ops]       │
   │                 │                    │◀───────────────────│◀──────────────────│
   │                 │◀───────────────────│                    │                   │
   │◀────────────────│   Re-render        │                    │                   │
```

---

## 5. 通信协议

### 5.1 操作指令 (Guest → Host)

```typescript
type OperationType =
  | 'CREATE'      // 创建节点
  | 'UPDATE'      // 更新属性
  | 'DELETE'      // 删除节点
  | 'APPEND'      // 追加子节点
  | 'INSERT'      // 插入子节点到指定位置
  | 'REMOVE'      // 移除子节点
  | 'REORDER'     // 重排子节点
  | 'TEXT'        // 更新文本内容
  | 'REF_CALL';   // 远程方法调用（Remote Ref）

interface CreateOperation {
  op: 'CREATE';
  id: number;
  type: string;           // 组件类型: 'View', 'Text' 等
  props: SerializedProps;
}

interface UpdateOperation {
  op: 'UPDATE';
  id: number;
  props: SerializedProps;
  removedProps?: string[];
}

// ... 其他操作类型
```

### 5.2 属性序列化

```typescript
type SerializedValue =
  | null
  | boolean
  | number
  | string
  | SerializedFunction
  | SerializedValue[]
  | { [key: string]: SerializedValue };

interface SerializedFunction {
  __type: 'function';
  __fnId: string;       // 函数 ID: 'fn_xxxxx'
}
```

### 5.3 宿主消息 (Host → Guest)

```typescript
type HostMessageType =
  | 'CALL_FUNCTION'      // 调用回调函数
  | 'HOST_EVENT'         // 宿主事件广播
  | 'CONFIG_UPDATE'      // 配置更新
  | 'DESTROY'            // 销毁信号
  | 'REF_METHOD_RESULT'; // Remote Ref 方法调用结果

interface CallFunctionMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: SerializedValue[];
}

interface HostEventMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: SerializedValue;
}
```

---

## 6. 核心模块

### 6.1 Engine

```typescript
import { Engine } from 'rill';

const engine = new Engine({
  timeout: 5000,        // 执行超时（毫秒）
  debug: false,         // 调试模式
  provider: myProvider, // 自定义 JSEngineProvider
});

// 注册组件
engine.register({ StepList: NativeStepList });

// 加载 Guest Bundle
await engine.loadBundle(bundleCode, initialProps);

// 发送事件给 Guest
engine.sendEvent('REFRESH', { timestamp: Date.now() });

// 监听 Guest 消息
engine.on('message', (msg) => console.log(msg));

// 健康检查
const health = engine.getHealth();

// 销毁
engine.destroy();
```

### 6.2 Guest SDK

```typescript
// src/sdk/index.ts - Guest SDK 导出
import { View, Text, TouchableOpacity, useHostEvent, useConfig, useSendToHost } from 'rill/let';

export default function MyGuest() {
  const config = useConfig<{ theme: string }>();
  const send = useSendToHost();

  useHostEvent('REFRESH', (payload) => {
    console.log('刷新中...', payload);
  });

  return (
    <View>
      <Text>主题: {config.theme}</Text>
      <TouchableOpacity onPress={() => send('ACTION', { type: 'click' })}>
        <Text>点击我</Text>
      </TouchableOpacity>
    </View>
  );
}
```

### 6.3 Receiver

Receiver 解析沙箱操作指令，构建 React Native 组件树：

```typescript
class Receiver {
  private nodeMap = new Map<number, NodeInstance>();
  private rootChildren: number[] = [];
  private refMap = new Map<number, React.RefObject<unknown>>();  // Remote Ref 支持

  applyBatch(batch: OperationBatch): ReceiverApplyStats {
    for (const op of batch.operations) {
      this.operationHandlers[op.op](op);
    }
    this.scheduleUpdate();
    return { applied, skipped, failed, total };
  }

  render(): React.ReactElement | string | null {
    // 渲染根节点树
  }
}
```

---

## 7. CLI 设计

### 7.1 打包配置

CLI 使用 **Bun.build** 编译 Guest Bundle：

```typescript
// src/cli/build.ts
export async function build(options: BuildOptions): Promise<void> {
  const result = await Bun.build({
    entrypoints: [entryPath],
    target: 'browser',
    format: 'cjs',
    minify,
    external: ['react', 'react/jsx-runtime', 'react-native', 'rill/let'],
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },
  });

  // 后处理：包装运行时注入
  // 严格依赖检查
  // 语法验证
}
```

### 7.2 CLI 命令

```bash
# 构建 bundle
bun run rill/cli build src/guest.tsx -o dist/bundle.js

# 开发模式
bun run rill/cli build src/guest.tsx --watch --no-minify --sourcemap

# 分析 bundle
bun run rill/cli analyze dist/bundle.js
```

---

## 8. 安全设计

### 8.1 沙箱隔离

1. **JSEngineProvider 隔离**：独立的 JS 执行上下文
   - 支持多种模式：`VM`（Node.js/Bun）、`JSC`（Apple 平台）、`QuickJS`（RN 全平台）、`WasmQuickJS`（Web）
2. **白名单组件**：只能渲染 Registry 中注册的组件
3. **Require 白名单**：只允许 require 指定的模块
4. **API 限制**：不注入危险 API（fetch、XMLHttpRequest 等）

### 8.2 异常处理

```typescript
// 沙箱内异常不影响宿主
engine.on('error', (error: Error) => {
  console.error('[Guest Error]', error.message);
});

engine.on('fatalError', (error: Error) => {
  console.error('[Guest Fatal]', error.message);
  // 沙箱已损坏，需要重新加载
});
```

### 8.3 资源限制

1. **执行超时**：`timeout` 选项（默认 5000ms）
2. **内存限制**：JSEngineProvider 堆内存上限
3. **批次大小限制**：`receiverMaxBatchSize`（默认 5000）

---

## 9. 性能优化

### 9.1 批量更新

- Reconciler 在 commit phase 统一发送操作
- 使用 `queueMicrotask` 调度更新，确保批次操作完成后再触发渲染

### 9.2 增量更新

- 只发送变更的属性（diff）
- 使用 `removedProps` 标记被删除的属性

### 9.3 列表优化

- FlatList 虚拟化支持
- 复用已创建的节点实例

### 9.4 Remote Ref

支持 Guest 代码调用 Host 组件实例方法（如 `focus()`、`scrollTo()` 等）：

```typescript
// Guest 端
const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();

const handleFocus = async () => {
  await remoteInput?.invoke('focus');
};

return <TextInput ref={inputRef} />;
```

通信流程：
1. Guest 发送 `REF_CALL` 操作（包含 refId、method、args、callId）
2. Host Receiver 通过 refMap 查找组件实例，调用方法
3. Host 返回 `REF_METHOD_RESULT` 消息（包含 result 或 error）
4. Guest Promise resolve/reject

---

## 10. DevTools 支持

### 10.1 开发模式

```typescript
if (__DEV__) {
  engine.on('operation', (op) => {
    console.log('[Rill Op]', op);
  });
}
```

### 10.2 DevTools 功能

- 可视化组件树检查器
- 操作日志查看
- 性能面板
- 诊断数据收集器
