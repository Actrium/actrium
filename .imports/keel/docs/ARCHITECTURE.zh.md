# Rill 架构设计文档

## 1. 设计理念对比

### 1.1 Remote-UI vs Rill

| 维度 | Shopify Remote-UI | Rill |
|------|-------------------|------|
| 目标平台 | Web (DOM) | React Native |
| 沙箱技术 | iframe / Web Worker | QuickJS |
| 渲染目标 | 真实 DOM 元素 | RN 原生组件 |
| 通信方式 | postMessage | QuickJS Bridge |
| 变更检测 | MutationObserver | react-reconciler |
| 组件模型 | Custom Elements | 虚组件 (字符串标识符) |

### 1.2 核心借鉴点

1. **Producer-Consumer 模式**：沙箱产生 UI 描述，宿主消费并渲染
2. **白名单组件安全模型**：只允许渲染已注册的组件类型
3. **函数序列化机制**：回调函数转换为 ID 引用
4. **批量更新优化**：聚合多个操作为单次传输

---

## 2. 总体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              宿主 App (React Native)                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────────┐ │
│  │  EngineView │───▶│   Engine    │───▶│          QuickJS Context        │ │
│  │  (React)    │    │  (Manager)  │    │  ┌─────────────────────────────┐│ │
│  └─────────────┘    └─────────────┘    │  │     Guest Bundle.js        ││ │
│         │                 │            │  │  ┌─────────────────────────┐││ │
│         │                 │            │  │  │  React + Reconciler     │││ │
│         ▼                 │            │  │  │  (Custom Renderer)      │││ │
│  ┌─────────────┐          │            │  │  └─────────────────────────┘││ │
│  │  Receiver   │◀─────────┘            │  │             │               ││ │
│  │  (指令解析)  │    Operations         │  │             ▼               ││ │
│  └─────────────┘    (JSON Array)       │  │  ┌─────────────────────────┐││ │
│         │                              │  │  │  sendToHost(ops)        │││ │
│         ▼                              │  │  └─────────────────────────┘││ │
│  ┌─────────────┐                       │  └─────────────────────────────┘│ │
│  │  Registry   │                       └─────────────────────────────────┘ │
│  │  (组件映射)  │                                                           │
│  └─────────────┘                                                           │
│         │                                                                   │
│         ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    原生组件树 (View, Text, Image...)                  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 数据流设计

### 3.1 渲染流程 (Sandbox → Host)

```
guest JSX           Reconciler              Bridge              Receiver           原生组件
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
   │                   │                     │ sendToHost(ops)    │                  │
   │                   │                     │───────────────────▶│                  │
   │                   │                     │                    │ applyOperations  │
   │                   │                     │                    │─────────────────▶│
   │                   │                     │                    │                  │
```

### 3.2 事件流程 (Host → Sandbox)

```
用户点击           原生组件             Receiver              Bridge             Sandbox
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

## 4. 通信协议规范

### 4.1 操作指令 (Sandbox → Host)

```typescript
/**
 * 操作类型枚举
 */
type OperationType =
  | 'CREATE'      // 创建节点
  | 'UPDATE'      // 更新属性
  | 'DELETE'      // 删除节点
  | 'APPEND'      // 追加子节点
  | 'INSERT'      // 插入子节点到指定位置
  | 'REMOVE'      // 移除子节点
  | 'REORDER'     // 重排子节点
  | 'TEXT'        // 更新文本内容

/**
 * 基础操作接口
 */
interface BaseOperation {
  op: OperationType;
  id: number;           // 节点唯一标识
  timestamp?: number;   // 可选：用于调试和性能分析
}

/**
 * 创建节点操作
 */
interface CreateOperation extends BaseOperation {
  op: 'CREATE';
  type: string;         // 组件类型: 'View', 'Text', 'StepList'
  props: SerializedProps;
}

/**
 * 更新节点操作
 */
interface UpdateOperation extends BaseOperation {
  op: 'UPDATE';
  props: SerializedProps;
  removedProps?: string[];  // 被移除的属性 key 列表
}

/**
 * 追加子节点操作
 */
interface AppendOperation extends BaseOperation {
  op: 'APPEND';
  parentId: number;
  childId: number;
}

/**
 * 插入子节点操作
 */
interface InsertOperation extends BaseOperation {
  op: 'INSERT';
  parentId: number;
  childId: number;
  index: number;        // 插入位置
}

/**
 * 删除节点操作
 */
interface DeleteOperation extends BaseOperation {
  op: 'DELETE';
}

/**
 * 移除子节点操作
 */
interface RemoveOperation extends BaseOperation {
  op: 'REMOVE';
  parentId: number;
  childId: number;
}

/**
 * 重排子节点操作
 */
interface ReorderOperation extends BaseOperation {
  op: 'REORDER';
  parentId: number;
  childIds: number[];   // 新的子节点顺序
}

/**
 * 更新文本操作
 */
interface TextOperation extends BaseOperation {
  op: 'TEXT';
  text: string;
}

type Operation =
  | CreateOperation
  | UpdateOperation
  | AppendOperation
  | InsertOperation
  | DeleteOperation
  | RemoveOperation
  | ReorderOperation
  | TextOperation;
```

### 4.2 属性序列化规范

```typescript
/**
 * 序列化后的属性对象
 */
type SerializedProps = Record<string, SerializedValue>;

/**
 * 序列化值类型
 */
type SerializedValue =
  | null
  | boolean
  | number
  | string
  | SerializedFunction
  | SerializedValue[]
  | { [key: string]: SerializedValue };

/**
 * 序列化的函数引用
 * 用于在宿主端触发回调
 */
interface SerializedFunction {
  __type: 'function';
  __fnId: string;       // 函数唯一标识: 'fn_xxxxx'
}

/**
 * 样式对象 (Plain Object，非 StyleSheet)
 */
interface StyleObject {
  flex?: number;
  flexDirection?: 'row' | 'column';
  justifyContent?: 'flex-start' | 'flex-end' | 'center' | 'space-between' | 'space-around';
  alignItems?: 'flex-start' | 'flex-end' | 'center' | 'stretch';
  padding?: number;
  margin?: number;
  backgroundColor?: string;
  color?: string;
  fontSize?: number;
  fontWeight?: string;
  // ... 其他 RN 支持的样式属性
}
```

### 4.3 宿主消息 (Host → Sandbox)

```typescript
/**
 * 宿主发送给沙箱的消息类型
 */
type HostMessageType =
  | 'CALL_FUNCTION'     // 调用回调函数
  | 'HOST_EVENT'        // 宿主事件广播
  | 'CONFIG_UPDATE'     // 配置更新
  | 'DESTROY'           // 销毁信号

/**
 * 基础消息接口
 */
interface BaseHostMessage {
  type: HostMessageType;
  seq?: number;         // 消息序列号 (用于请求-响应配对)
}

/**
 * 调用回调函数消息
 */
interface CallFunctionMessage extends BaseHostMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: SerializedValue[];
}

/**
 * 宿主事件消息
 */
interface HostEventMessage extends BaseHostMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: SerializedValue;
}

/**
 * 配置更新消息
 */
interface ConfigUpdateMessage extends BaseHostMessage {
  type: 'CONFIG_UPDATE';
  config: Record<string, SerializedValue>;
}

/**
 * 销毁消息
 */
interface DestroyMessage extends BaseHostMessage {
  type: 'DESTROY';
}

type HostMessage =
  | CallFunctionMessage
  | HostEventMessage
  | ConfigUpdateMessage
  | DestroyMessage;
```

### 4.4 批量更新协议

```typescript
/**
 * 操作批次
 * 一次 commit 可能包含多个操作
 */
interface OperationBatch {
  version: number;      // 协议版本
  batchId: number;      // 批次 ID
  operations: Operation[];
}

/**
 * 发送函数签名
 */
type SendToHost = (batch: OperationBatch) => void;
```

---

## 5. 模块详细设计

### 5.1 Reconciler 模块

```typescript
// src/reconciler/index.ts

import Reconciler from 'react-reconciler';

/**
 * 虚拟节点 (内部表示)
 */
interface VNode {
  id: number;
  type: string;
  props: Record<string, any>;
  children: VNode[];
  parent: VNode | null;
}

/**
 * 回调函数注册表
 */
class CallbackRegistry {
  private callbacks = new Map<string, Function>();
  private counter = 0;

  register(fn: Function): string {
    const fnId = `fn_${++this.counter}`;
    this.callbacks.set(fnId, fn);
    return fnId;
  }

  invoke(fnId: string, args: any[]): any {
    const fn = this.callbacks.get(fnId);
    if (fn) {
      return fn(...args);
    }
    console.warn(`Callback ${fnId} not found`);
  }

  remove(fnId: string): void {
    this.callbacks.delete(fnId);
  }

  clear(): void {
    this.callbacks.clear();
  }
}

/**
 * 操作收集器
 * 收集 render phase 的操作，在 commit phase 统一发送
 */
class OperationCollector {
  private operations: Operation[] = [];
  private batchId = 0;

  add(op: Operation): void {
    this.operations.push(op);
  }

  flush(sendToHost: SendToHost): void {
    if (this.operations.length === 0) return;

    sendToHost({
      version: 1,
      batchId: ++this.batchId,
      operations: this.operations,
    });

    this.operations = [];
  }
}

/**
 * 创建自定义渲染器
 */
function createReconciler(sendToHost: SendToHost) {
  const callbackRegistry = new CallbackRegistry();
  const collector = new OperationCollector();
  let nodeIdCounter = 0;

  /**
   * 序列化 props，将函数转换为 fnId
   */
  function serializeProps(props: Record<string, any>): SerializedProps {
    const result: SerializedProps = {};

    for (const [key, value] of Object.entries(props)) {
      if (key === 'children') continue;

      if (typeof value === 'function') {
        const fnId = callbackRegistry.register(value);
        result[key] = { __type: 'function', __fnId: fnId };
      } else {
        result[key] = value;
      }
    }

    return result;
  }

  const hostConfig: Reconciler.HostConfig<...> = {
    // 创建实例
    createInstance(type, props) {
      const id = ++nodeIdCounter;
      const node: VNode = {
        id,
        type,
        props,
        children: [],
        parent: null,
      };

      collector.add({
        op: 'CREATE',
        id,
        type,
        props: serializeProps(props),
      });

      return node;
    },

    // 创建文本节点
    createTextInstance(text) {
      const id = ++nodeIdCounter;
      const node: VNode = {
        id,
        type: '__TEXT__',
        props: { text },
        children: [],
        parent: null,
      };

      collector.add({
        op: 'CREATE',
        id,
        type: '__TEXT__',
        props: { text },
      });

      return node;
    },

    // 追加子节点
    appendChild(parent, child) {
      parent.children.push(child);
      child.parent = parent;

      collector.add({
        op: 'APPEND',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      });
    },

    // 移除子节点
    removeChild(parent, child) {
      const index = parent.children.indexOf(child);
      if (index !== -1) {
        parent.children.splice(index, 1);
      }
      child.parent = null;

      collector.add({
        op: 'REMOVE',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      });
    },

    // 提交更新
    commitUpdate(node, updatePayload, type, oldProps, newProps) {
      node.props = newProps;

      collector.add({
        op: 'UPDATE',
        id: node.id,
        props: serializeProps(newProps),
      });
    },

    // 提交阶段结束时刷新操作
    resetAfterCommit() {
      collector.flush(sendToHost);
    },

    // ... 其他必需的 hostConfig 方法
  };

  return {
    reconciler: Reconciler(hostConfig),
    callbackRegistry,
  };
}
```

### 5.2 SDK 模块

```typescript
// src/sdk/index.ts

/**
 * 虚组件定义
 * 这些导出仅作为类型标识符，不包含实际实现
 */
export const View = 'View';
export const Text = 'Text';
export const Image = 'Image';
export const ScrollView = 'ScrollView';
export const TouchableOpacity = 'TouchableOpacity';
export const FlatList = 'FlatList';

/**
 * 组件 Props 类型定义
 */
export interface ViewProps {
  style?: StyleProp;
  children?: React.ReactNode;
  testID?: string;
}

export interface TextProps {
  style?: StyleProp;
  children?: React.ReactNode;
  numberOfLines?: number;
}

export interface ImageProps {
  source: { uri: string } | number;
  style?: StyleProp;
  resizeMode?: 'cover' | 'contain' | 'stretch' | 'center';
}

export interface TouchableOpacityProps {
  style?: StyleProp;
  onPress?: () => void;
  activeOpacity?: number;
  children?: React.ReactNode;
}

/**
 * 样式类型 (Plain Object)
 */
export type StyleProp = Record<string, string | number> | undefined;

// ============ Hooks ============

/**
 * 订阅宿主事件
 * @param eventName 事件名称
 * @param callback 回调函数
 */
export function useHostEvent(eventName: string, callback: (payload: any) => void): void {
  // 实际实现在 reconciler 运行时注入
  // SDK 中仅提供类型签名
}

/**
 * 获取宿主下发的初始配置
 * @returns 配置对象
 */
export function useConfig<T = Record<string, any>>(): T {
  // 实际实现在 reconciler 运行时注入
  return {} as T;
}

/**
 * 向宿主发送消息
 * @returns send 函数
 */
export function useSendToHost(): (eventName: string, payload?: any) => void {
  // 实际实现在 reconciler 运行时注入
  return () => {};
}
```

### 5.3 Runtime 模块

```typescript
// src/runtime/index.ts

import { QuickJS } from 'react-native-quickjs';

/**
 * 组件注册表
 */
class ComponentRegistry {
  private components = new Map<string, React.ComponentType<any>>();

  register(name: string, component: React.ComponentType<any>): void {
    this.components.set(name, component);
  }

  registerAll(map: Record<string, React.ComponentType<any>>): void {
    Object.entries(map).forEach(([name, component]) => {
      this.register(name, component);
    });
  }

  get(name: string): React.ComponentType<any> | undefined {
    return this.components.get(name);
  }

  has(name: string): boolean {
    return this.components.has(name);
  }
}

/**
 * 节点映射表
 * ID -> React Element 实例
 */
class NodeMap {
  private nodes = new Map<number, NodeInstance>();

  set(id: number, node: NodeInstance): void {
    this.nodes.set(id, node);
  }

  get(id: number): NodeInstance | undefined {
    return this.nodes.get(id);
  }

  delete(id: number): void {
    this.nodes.delete(id);
  }

  clear(): void {
    this.nodes.clear();
  }
}

interface NodeInstance {
  id: number;
  type: string;
  props: Record<string, any>;
  children: number[];  // 子节点 ID 列表
}

/**
 * 指令接收器
 */
class Receiver {
  private nodeMap = new NodeMap();
  private rootId: number | null = null;
  private onUpdate: () => void;

  constructor(private registry: ComponentRegistry, onUpdate: () => void) {
    this.onUpdate = onUpdate;
  }

  /**
   * 应用操作批次
   */
  applyBatch(batch: OperationBatch): void {
    for (const op of batch.operations) {
      this.applyOperation(op);
    }
    this.onUpdate();
  }

  private applyOperation(op: Operation): void {
    switch (op.op) {
      case 'CREATE':
        this.handleCreate(op);
        break;
      case 'UPDATE':
        this.handleUpdate(op);
        break;
      case 'APPEND':
        this.handleAppend(op);
        break;
      case 'REMOVE':
        this.handleRemove(op);
        break;
      case 'DELETE':
        this.handleDelete(op);
        break;
      // ... 其他操作类型
    }
  }

  private handleCreate(op: CreateOperation): void {
    const node: NodeInstance = {
      id: op.id,
      type: op.type,
      props: this.deserializeProps(op.props),
      children: [],
    };
    this.nodeMap.set(op.id, node);

    if (this.rootId === null) {
      this.rootId = op.id;
    }
  }

  private handleUpdate(op: UpdateOperation): void {
    const node = this.nodeMap.get(op.id);
    if (node) {
      node.props = { ...node.props, ...this.deserializeProps(op.props) };
    }
  }

  private handleAppend(op: AppendOperation): void {
    const parent = this.nodeMap.get(op.parentId);
    if (parent) {
      parent.children.push(op.childId);
    }
  }

  private handleRemove(op: RemoveOperation): void {
    const parent = this.nodeMap.get(op.parentId);
    if (parent) {
      const index = parent.children.indexOf(op.childId);
      if (index !== -1) {
        parent.children.splice(index, 1);
      }
    }
  }

  private handleDelete(op: DeleteOperation): void {
    this.nodeMap.delete(op.id);
  }

  /**
   * 反序列化 props，将 fnId 转换为实际可调用的函数
   */
  private deserializeProps(props: SerializedProps): Record<string, any> {
    const result: Record<string, any> = {};

    for (const [key, value] of Object.entries(props)) {
      if (value && typeof value === 'object' && '__type' in value) {
        if (value.__type === 'function') {
          // 创建一个代理函数，调用时发送消息给沙箱
          result[key] = (...args: any[]) => {
            this.callSandboxFunction(value.__fnId, args);
          };
        }
      } else {
        result[key] = value;
      }
    }

    return result;
  }

  private callSandboxFunction(fnId: string, args: any[]): void {
    // 由 Engine 实现具体的调用逻辑
    this.engine?.sendToSandbox({
      type: 'CALL_FUNCTION',
      fnId,
      args,
    });
  }

  /**
   * 渲染组件树
   */
  render(): React.ReactElement | null {
    if (this.rootId === null) return null;
    return this.renderNode(this.rootId);
  }

  private renderNode(id: number): React.ReactElement | null {
    const node = this.nodeMap.get(id);
    if (!node) return null;

    // 处理文本节点
    if (node.type === '__TEXT__') {
      return node.props.text;
    }

    // 获取组件实现
    const Component = this.registry.get(node.type);
    if (!Component) {
      console.warn(`Component "${node.type}" not registered`);
      return null;
    }

    // 递归渲染子节点
    const children = node.children.map(childId => this.renderNode(childId));

    return React.createElement(Component, { ...node.props, key: id }, ...children);
  }
}

/**
 * 引擎主类
 */
export class Engine {
  private quickjs: QuickJS | null = null;
  private registry = new ComponentRegistry();
  private receiver: Receiver | null = null;
  private config: Record<string, any> = {};

  constructor() {
    this.setupDefaultComponents();
  }

  /**
   * 注册默认组件
   */
  private setupDefaultComponents(): void {
    // 在 components/ 目录中实现
    this.registry.registerAll({
      View: require('../components/View').default,
      Text: require('../components/Text').default,
      Image: require('../components/Image').default,
      ScrollView: require('../components/ScrollView').default,
      TouchableOpacity: require('../components/TouchableOpacity').default,
    });
  }

  /**
   * 注册自定义组件
   */
  register(components: Record<string, React.ComponentType<any>>): void {
    this.registry.registerAll(components);
  }

  /**
   * 加载并运行guest Bundle
   */
  async loadBundle(bundleUrl: string, initialProps?: Record<string, any>): Promise<void> {
    // 1. 获取 Bundle 内容
    const bundleCode = await this.fetchBundle(bundleUrl);

    // 2. 创建 QuickJS Context
    this.quickjs = new QuickJS();

    // 3. 注入 Polyfill 和运行时
    this.injectPolyfills();
    this.injectRuntime(initialProps);

    // 4. 执行 Bundle
    this.quickjs.eval(bundleCode);
  }

  /**
   * 注入 Polyfill
   */
  private injectPolyfills(): void {
    // console
    this.quickjs?.setGlobal('console', {
      log: (...args: any[]) => console.log('[Guest]', ...args),
      warn: (...args: any[]) => console.warn('[Guest]', ...args),
      error: (...args: any[]) => console.error('[Guest]', ...args),
    });

    // setTimeout / setInterval
    this.quickjs?.setGlobal('setTimeout', (fn: Function, delay: number) => {
      return setTimeout(() => this.quickjs?.eval(`(${fn})()`), delay);
    });

    // ... 其他 Polyfill
  }

  /**
   * 注入运行时钩子
   */
  private injectRuntime(initialProps?: Record<string, any>): void {
    this.config = initialProps || {};

    // sendToHost: 沙箱向宿主发送操作
    this.quickjs?.setGlobal('__sendToHost', (batch: OperationBatch) => {
      this.receiver?.applyBatch(batch);
    });

    // __getConfig: 获取初始配置
    this.quickjs?.setGlobal('__getConfig', () => this.config);
  }

  /**
   * 向沙箱发送消息
   */
  sendToSandbox(message: HostMessage): void {
    this.quickjs?.eval(`__handleHostMessage(${JSON.stringify(message)})`);
  }

  /**
   * 发送宿主事件
   */
  emit(eventName: string, payload?: any): void {
    this.sendToSandbox({
      type: 'HOST_EVENT',
      eventName,
      payload,
    });
  }

  /**
   * 销毁引擎
   */
  destroy(): void {
    this.sendToSandbox({ type: 'DESTROY' });
    this.quickjs?.dispose();
    this.quickjs = null;
    this.receiver = null;
  }

  /**
   * 创建 Receiver 并返回
   */
  createReceiver(onUpdate: () => void): Receiver {
    this.receiver = new Receiver(this.registry, onUpdate);
    return this.receiver;
  }
}
```

### 5.4 EngineView 组件

```typescript
// src/runtime/EngineView.tsx

import React, { useEffect, useState, useCallback } from 'react';
import { View, ActivityIndicator, StyleSheet } from 'react-native';
import { Engine } from './Engine';

interface EngineViewProps {
  engine: Engine;
  bundleUrl: string;
  initialProps?: Record<string, any>;
  onError?: (error: Error) => void;
  onLoad?: () => void;
  fallback?: React.ReactNode;
}

export function EngineView({
  engine,
  bundleUrl,
  initialProps,
  onError,
  onLoad,
  fallback,
}: EngineViewProps) {
  const [content, setContent] = useState<React.ReactElement | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const handleUpdate = useCallback(() => {
    const receiver = engine.getReceiver();
    if (receiver) {
      setContent(receiver.render());
    }
  }, [engine]);

  useEffect(() => {
    let mounted = true;

    async function loadGuest() {
      try {
        // 创建 Receiver
        engine.createReceiver(handleUpdate);

        // 加载并执行 Bundle
        await engine.loadBundle(bundleUrl, initialProps);

        if (mounted) {
          setLoading(false);
          onLoad?.();
        }
      } catch (err) {
        if (mounted) {
          const error = err instanceof Error ? err : new Error(String(err));
          setError(error);
          setLoading(false);
          onError?.(error);
        }
      }
    }

    loadGuest();

    return () => {
      mounted = false;
      engine.destroy();
    };
  }, [engine, bundleUrl, initialProps]);

  if (loading) {
    return fallback ?? (
      <View style={styles.loading}>
        <ActivityIndicator />
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.error}>
        {/* 可自定义错误展示 */}
      </View>
    );
  }

  return <>{content}</>;
}

const styles = StyleSheet.create({
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  error: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
});
```

---

## 6. CLI 设计

### 6.1 打包配置

```typescript
// src/cli/build.ts

import { build } from 'esbuild';
import path from 'path';

interface BuildOptions {
  entry: string;
  outfile: string;
  minify?: boolean;
  sourcemap?: boolean;
}

export async function buildGuest(options: BuildOptions) {
  const { entry, outfile, minify = true, sourcemap = false } = options;

  await build({
    entryPoints: [entry],
    bundle: true,
    outfile,
    format: 'iife',
    globalName: '__RillGuest',
    minify,
    sourcemap,
    target: 'es2020',

    // 外部依赖 - 不打包进 bundle
    external: ['react-native'],

    // 注入运行时
    inject: [path.resolve(__dirname, '../reconciler/runtime-inject.js')],

    // 别名配置
    alias: {
      'react-native-mini-engine/sdk': path.resolve(__dirname, '../sdk'),
    },

    // 定义全局常量
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },

    // JSX 转换
    jsx: 'automatic',
    jsxImportSource: 'react',
  });

  console.log(`✓ Built: ${outfile}`);
}
```

### 6.2 CLI 入口

```typescript
// src/cli/index.ts

#!/usr/bin/env node

import { program } from 'commander';
import { buildGuest } from './build';

program
  .name('@rill/core')
  .description('Rill guest CLI')
  .version('0.1.0');

program
  .command('build')
  .description('Build a guest bundle')
  .argument('<entry>', 'Entry file path')
  .option('-o, --outfile <path>', 'Output file path', 'dist/bundle.js')
  .option('--no-minify', 'Disable minification')
  .option('--sourcemap', 'Generate sourcemap')
  .action(async (entry, options) => {
    await buildGuest({
      entry,
      outfile: options.outfile,
      minify: options.minify,
      sourcemap: options.sourcemap,
    });
  });

program.parse();
```

---

## 7. 安全设计

### 7.1 沙箱隔离

1. **QuickJS 隔离**：独立的 JS 执行上下文，无法访问宿主的全局对象
2. **白名单组件**：只能渲染 Registry 中注册的组件
3. **API 限制**：不注入危险 API (fetch, XMLHttpRequest 等)

### 7.2 异常处理

```typescript
// 沙箱内异常不影响宿主
engine.quickjs?.setGlobal('__errorHandler', (error: Error) => {
  console.error('[Guest Error]', error.message);
  // 可选：上报错误
});
```

### 7.3 资源限制

1. **执行超时**：单次渲染操作超时限制
2. **内存限制**：QuickJS 堆内存上限
3. **操作频率**：批次发送频率限制

---

## 8. 性能优化

### 8.1 批量更新

- Reconciler 在 commit phase 统一发送操作
- 使用 `requestAnimationFrame` 节流高频更新

### 8.2 增量更新

- 只发送变更的属性 (diff)
- 使用 `removedProps` 标记被删除的属性

### 8.3 列表优化

- FlatList 虚拟化支持
- 复用已创建的节点实例

---

## 9. 调试支持

### 9.1 开发模式

```typescript
if (__DEV__) {
  // 打印所有操作
  engine.on('operation', (op) => {
    console.log('[Rill Op]', op);
  });
}
```

### 9.2 DevTools

- 提供可视化的组件树检查器
- 操作日志查看
- 性能面板
