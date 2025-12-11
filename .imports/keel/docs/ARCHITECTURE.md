# Rill Architecture Design Document

## 1. Design Philosophy Comparison

### 1.1 Remote-UI vs Rill

| Dimension | Shopify Remote-UI | Rill |
|-----------|-------------------|------|
| Target Platform | Web (DOM) | React Native |
| Sandbox Technology | iframe / Web Worker | QuickJS |
| Render Target | Real DOM Elements | RN Native Components |
| Communication | postMessage | QuickJS Bridge |
| Change Detection | MutationObserver | react-reconciler |
| Component Model | Custom Elements | Virtual Components (string identifiers) |

### 1.2 Core Concepts Borrowed

1. **Producer-Consumer Pattern**: Sandbox produces UI descriptions, host consumes and renders
2. **Whitelist Component Security Model**: Only registered component types can be rendered
3. **Function Serialization Mechanism**: Callbacks converted to ID references
4. **Batch Update Optimization**: Aggregate multiple operations into single transmission

---

## 2. Overall Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Host App (React Native)                         │
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
│  │  (Parser)   │    Operations         │  │             ▼               ││ │
│  └─────────────┘    (JSON Array)       │  │  ┌─────────────────────────┐││ │
│         │                              │  │  │  sendToHost(ops)        │││ │
│         ▼                              │  │  └─────────────────────────┘││ │
│  ┌─────────────┐                       │  └─────────────────────────────┘│ │
│  │  Registry   │                       └─────────────────────────────────┘ │
│  │  (Mapping)  │                                                           │
│  └─────────────┘                                                           │
│         │                                                                   │
│         ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    Native Component Tree (View, Text, Image...)      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Data Flow Design

### 3.1 Render Flow (Sandbox → Host)

```
Guest JSX           Reconciler              Bridge              Receiver           Native Components
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

### 3.2 Event Flow (Host → Sandbox)

```
User Click           Native Component        Receiver              Bridge             Sandbox
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

## 4. Communication Protocol Specification

### 4.1 Operation Instructions (Sandbox → Host)

```typescript
/**
 * Operation type enum
 */
type OperationType =
  | 'CREATE'      // Create node
  | 'UPDATE'      // Update properties
  | 'DELETE'      // Delete node
  | 'APPEND'      // Append child node
  | 'INSERT'      // Insert child at position
  | 'REMOVE'      // Remove child node
  | 'REORDER'     // Reorder children
  | 'TEXT'        // Update text content

/**
 * Base operation interface
 */
interface BaseOperation {
  op: OperationType;
  id: number;           // Node unique identifier
  timestamp?: number;   // Optional: for debugging and performance analysis
}

/**
 * Create node operation
 */
interface CreateOperation extends BaseOperation {
  op: 'CREATE';
  type: string;         // Component type: 'View', 'Text', 'StepList'
  props: SerializedProps;
}

/**
 * Update node operation
 */
interface UpdateOperation extends BaseOperation {
  op: 'UPDATE';
  props: SerializedProps;
  removedProps?: string[];  // List of removed property keys
}

/**
 * Append child operation
 */
interface AppendOperation extends BaseOperation {
  op: 'APPEND';
  parentId: number;
  childId: number;
}

/**
 * Insert child operation
 */
interface InsertOperation extends BaseOperation {
  op: 'INSERT';
  parentId: number;
  childId: number;
  index: number;        // Insert position
}

/**
 * Delete node operation
 */
interface DeleteOperation extends BaseOperation {
  op: 'DELETE';
}

/**
 * Remove child operation
 */
interface RemoveOperation extends BaseOperation {
  op: 'REMOVE';
  parentId: number;
  childId: number;
}

/**
 * Reorder children operation
 */
interface ReorderOperation extends BaseOperation {
  op: 'REORDER';
  parentId: number;
  childIds: number[];   // New child order
}

/**
 * Update text operation
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

### 4.2 Property Serialization Specification

```typescript
/**
 * Serialized props object
 */
type SerializedProps = Record<string, SerializedValue>;

/**
 * Serialized value type
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
 * Serialized function reference
 * Used to trigger callbacks on host side
 */
interface SerializedFunction {
  __type: 'function';
  __fnId: string;       // Function unique identifier: 'fn_xxxxx'
}

/**
 * Style object (Plain Object, not StyleSheet)
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
  // ... other RN supported style properties
}
```

### 4.3 Host Messages (Host → Sandbox)

```typescript
/**
 * Host message type enum
 */
type HostMessageType =
  | 'CALL_FUNCTION'     // Call callback function
  | 'HOST_EVENT'        // Host event broadcast
  | 'CONFIG_UPDATE'     // Configuration update
  | 'DESTROY'           // Destroy signal

/**
 * Base message interface
 */
interface BaseHostMessage {
  type: HostMessageType;
  seq?: number;         // Message sequence number (for request-response pairing)
}

/**
 * Call callback function message
 */
interface CallFunctionMessage extends BaseHostMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: SerializedValue[];
}

/**
 * Host event message
 */
interface HostEventMessage extends BaseHostMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: SerializedValue;
}

/**
 * Configuration update message
 */
interface ConfigUpdateMessage extends BaseHostMessage {
  type: 'CONFIG_UPDATE';
  config: Record<string, SerializedValue>;
}

/**
 * Destroy message
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

### 4.4 Batch Update Protocol

```typescript
/**
 * Operation batch
 * A single commit may contain multiple operations
 */
interface OperationBatch {
  version: number;      // Protocol version
  batchId: number;      // Batch ID
  operations: Operation[];
}

/**
 * Send function signature
 */
type SendToHost = (batch: OperationBatch) => void;
```

---

## 5. Module Detailed Design

### 5.1 Reconciler Module

```typescript
// src/reconciler/index.ts

import Reconciler from 'react-reconciler';

/**
 * Virtual node (internal representation)
 */
interface VNode {
  id: number;
  type: string;
  props: Record<string, any>;
  children: VNode[];
  parent: VNode | null;
}

/**
 * Callback registry
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
 * Operation collector
 * Collects render phase operations, sends during commit phase
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
 * Create custom renderer
 */
function createReconciler(sendToHost: SendToHost) {
  const callbackRegistry = new CallbackRegistry();
  const collector = new OperationCollector();
  let nodeIdCounter = 0;

  /**
   * Serialize props, convert functions to fnId
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
    // Create instance
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

    // Create text node
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

    // Append child
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

    // Remove child
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

    // Commit update
    commitUpdate(node, updatePayload, type, oldProps, newProps) {
      node.props = newProps;

      collector.add({
        op: 'UPDATE',
        id: node.id,
        props: serializeProps(newProps),
      });
    },

    // Flush operations at end of commit phase
    resetAfterCommit() {
      collector.flush(sendToHost);
    },

    // ... other required hostConfig methods
  };

  return {
    reconciler: Reconciler(hostConfig),
    callbackRegistry,
  };
}
```

### 5.2 SDK Module

```typescript
// src/sdk/index.ts

/**
 * Virtual component definitions
 * These exports are only type identifiers, no actual implementation
 */
export const View = 'View';
export const Text = 'Text';
export const Image = 'Image';
export const ScrollView = 'ScrollView';
export const TouchableOpacity = 'TouchableOpacity';
export const FlatList = 'FlatList';

/**
 * Component Props type definitions
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
 * Style type (Plain Object)
 */
export type StyleProp = Record<string, string | number> | undefined;

// ============ Hooks ============

/**
 * Subscribe to host events
 * @param eventName Event name
 * @param callback Callback function
 */
export function useHostEvent(eventName: string, callback: (payload: any) => void): void {
  // Actual implementation injected at reconciler runtime
  // SDK only provides type signature
}

/**
 * Get initial configuration from host
 * @returns Configuration object
 */
export function useConfig<T = Record<string, any>>(): T {
  // Actual implementation injected at reconciler runtime
  return {} as T;
}

/**
 * Send messages to host
 * @returns send function
 */
export function useSendToHost(): (eventName: string, payload?: any) => void {
  // Actual implementation injected at reconciler runtime
  return () => {};
}
```

### 5.3 Runtime Module

```typescript
// src/runtime/index.ts

import { QuickJS } from 'react-native-quickjs';

/**
 * Component registry
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
 * Node mapping table
 * ID -> React Element instance
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
  children: number[];  // Child node ID list
}

/**
 * Operation receiver
 */
class Receiver {
  private nodeMap = new NodeMap();
  private rootId: number | null = null;
  private onUpdate: () => void;

  constructor(private registry: ComponentRegistry, onUpdate: () => void) {
    this.onUpdate = onUpdate;
  }

  /**
   * Apply operation batch
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
      // ... other operation types
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
   * Deserialize props, convert fnId to callable functions
   */
  private deserializeProps(props: SerializedProps): Record<string, any> {
    const result: Record<string, any> = {};

    for (const [key, value] of Object.entries(props)) {
      if (value && typeof value === 'object' && '__type' in value) {
        if (value.__type === 'function') {
          // Create proxy function that sends message to sandbox when called
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
    // Actual call logic implemented by Engine
    this.engine?.sendToSandbox({
      type: 'CALL_FUNCTION',
      fnId,
      args,
    });
  }

  /**
   * Render component tree
   */
  render(): React.ReactElement | null {
    if (this.rootId === null) return null;
    return this.renderNode(this.rootId);
  }

  private renderNode(id: number): React.ReactElement | null {
    const node = this.nodeMap.get(id);
    if (!node) return null;

    // Handle text nodes
    if (node.type === '__TEXT__') {
      return node.props.text;
    }

    // Get component implementation
    const Component = this.registry.get(node.type);
    if (!Component) {
      console.warn(`Component "${node.type}" not registered`);
      return null;
    }

    // Recursively render children
    const children = node.children.map(childId => this.renderNode(childId));

    return React.createElement(Component, { ...node.props, key: id }, ...children);
  }
}

/**
 * Engine main class
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
   * Register default components
   */
  private setupDefaultComponents(): void {
    // Implemented in components/ directory
    this.registry.registerAll({
      View: require('../components/View').default,
      Text: require('../components/Text').default,
      Image: require('../components/Image').default,
      ScrollView: require('../components/ScrollView').default,
      TouchableOpacity: require('../components/TouchableOpacity').default,
    });
  }

  /**
   * Register custom components
   */
  register(components: Record<string, React.ComponentType<any>>): void {
    this.registry.registerAll(components);
  }

  /**
   * Load and run guest bundle
   */
  async loadBundle(bundleUrl: string, initialProps?: Record<string, any>): Promise<void> {
    // 1. Fetch bundle content
    const bundleCode = await this.fetchBundle(bundleUrl);

    // 2. Create QuickJS Context
    this.quickjs = new QuickJS();

    // 3. Inject polyfills and runtime
    this.injectPolyfills();
    this.injectRuntime(initialProps);

    // 4. Execute bundle
    this.quickjs.eval(bundleCode);
  }

  /**
   * Inject polyfills
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

    // ... other polyfills
  }

  /**
   * Inject runtime hooks
   */
  private injectRuntime(initialProps?: Record<string, any>): void {
    this.config = initialProps || {};

    // sendToHost: sandbox sends operations to host
    this.quickjs?.setGlobal('__sendToHost', (batch: OperationBatch) => {
      this.receiver?.applyBatch(batch);
    });

    // __getConfig: get initial configuration
    this.quickjs?.setGlobal('__getConfig', () => this.config);
  }

  /**
   * Send message to sandbox
   */
  sendToSandbox(message: HostMessage): void {
    this.quickjs?.eval(`__handleHostMessage(${JSON.stringify(message)})`);
  }

  /**
   * Send host event
   */
  emit(eventName: string, payload?: any): void {
    this.sendToSandbox({
      type: 'HOST_EVENT',
      eventName,
      payload,
    });
  }

  /**
   * Destroy engine
   */
  destroy(): void {
    this.sendToSandbox({ type: 'DESTROY' });
    this.quickjs?.dispose();
    this.quickjs = null;
    this.receiver = null;
  }

  /**
   * Create and return Receiver
   */
  createReceiver(onUpdate: () => void): Receiver {
    this.receiver = new Receiver(this.registry, onUpdate);
    return this.receiver;
  }
}
```

### 5.4 EngineView Component

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
        // Create Receiver
        engine.createReceiver(handleUpdate);

        // Load and execute bundle
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
        {/* Custom error display */}
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

## 6. CLI Design

### 6.1 Build Configuration

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

    // External dependencies - not bundled
    external: ['react-native'],

    // Inject runtime
    inject: [path.resolve(__dirname, '../reconciler/runtime-inject.js')],

    // Alias configuration
    alias: {
      'react-native-mini-engine/sdk': path.resolve(__dirname, '../sdk'),
    },

    // Define global constants
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },

    // JSX transform
    jsx: 'automatic',
    jsxImportSource: 'react',
  });

  console.log(`✓ Built: ${outfile}`);
}
```

### 6.2 CLI Entry

```typescript
// src/cli/index.ts

#!/usr/bin/env node

import { program } from 'commander';
import { buildGuest } from './build';

program
  .name('rill')
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

## 7. Security Design

### 7.1 Sandbox Isolation

1. **QuickJS Isolation**: Independent JS execution context, cannot access host global objects
2. **Whitelist Components**: Can only render registered component types
3. **API Restrictions**: Don't inject dangerous APIs (fetch, XMLHttpRequest, etc.)

### 7.2 Exception Handling

```typescript
// Sandbox exceptions don't affect host
engine.quickjs?.setGlobal('__errorHandler', (error: Error) => {
  console.error('[Guest Error]', error.message);
  // Optional: report error
});
```

### 7.3 Resource Limits

1. **Execution Timeout**: Timeout limit for single render operations
2. **Memory Limit**: QuickJS heap memory cap
3. **Operation Rate**: Batch send rate limiting

---

## 8. Performance Optimization

### 8.1 Batch Updates

- Reconciler sends operations uniformly during commit phase
- Use `requestAnimationFrame` to throttle high-frequency updates

### 8.2 Incremental Updates

- Only send changed properties (diff)
- Use `removedProps` to mark deleted properties

### 8.3 List Optimization

- FlatList virtualization support
- Reuse created node instances

---

## 9. Debugging Support

### 9.1 Development Mode

```typescript
if (__DEV__) {
  // Print all operations
  engine.on('operation', (op) => {
    console.log('[Rill Op]', op);
  });
}
```

### 9.2 DevTools

- Provide visual component tree inspector
- Operation log viewer
- Performance panel
