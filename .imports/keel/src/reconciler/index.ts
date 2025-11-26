/**
 * Rill Reconciler
 *
 * Custom renderer based on react-reconciler
 * Transforms React render actions into JSON instructions
 */

import Reconciler from 'react-reconciler';
import { DefaultEventPriority } from 'react-reconciler/constants';
import type {
  VNode,
  Operation,
  CreateOperation,
  UpdateOperation,
  AppendOperation,
  RemoveOperation,
  InsertOperation,
  SerializedProps,
  SerializedValue,
  SerializedFunction,
  OperationBatch,
  SendToHost,
} from '../types';

// ============ Callback Registry ============

/**
 * Callback Registry
 * Manages callback functions in sandbox, generates unique IDs for host invocation
 */
export class CallbackRegistry {
  private callbacks = new Map<string, (...args: unknown[]) => unknown>();
  private counter = 0;

  /**
   * Register callback function
   */
  register(fn: (...args: unknown[]) => unknown): string {
    const fnId = `fn_${++this.counter}_${Date.now().toString(36)}`;
    this.callbacks.set(fnId, fn);
    return fnId;
  }

  /**
   * Invoke callback function
   */
  invoke(fnId: string, args: unknown[]): unknown {
    const fn = this.callbacks.get(fnId);
    if (fn) {
      try {
        return fn(...args);
      } catch (error) {
        console.error(`[rill] Callback ${fnId} threw error:`, error);
        throw error;
      }
    }
    console.warn(`[rill] Callback ${fnId} not found`);
    return undefined;
  }

  /**
   * Remove callback function
   */
  remove(fnId: string): void {
    this.callbacks.delete(fnId);
  }

  /**
   * Remove multiple callback functions
   */
  removeAll(fnIds: string[]): void {
    fnIds.forEach((fnId) => this.callbacks.delete(fnId));
  }

  /**
   * Clear all callbacks
   */
  clear(): void {
    this.callbacks.clear();
    this.counter = 0;
  }

  /**
   * Get registered callback count
   */
  get size(): number {
    return this.callbacks.size;
  }
}

// ============ Operation Collector ============

/**
 * Operation Collector
 * Collects operations during render phase, sends all during commit phase
 */
export class OperationCollector {
  private operations: Operation[] = [];
  private batchId = 0;
  private version = 1;

  /**
   * Add operation
   */
  add(op: Operation): void {
    this.operations.push({
      ...op,
      timestamp: Date.now(),
    });
  }

  /**
   * Flush and send all operations
   */
  flush(sendToHost: SendToHost): void {
    if (this.operations.length === 0) return;

    const batch: OperationBatch = {
      version: this.version,
      batchId: ++this.batchId,
      operations: [...this.operations],
    };

    this.operations = [];
    sendToHost(batch);
  }

  /**
   * Get pending operation count
   */
  get pendingCount(): number {
    return this.operations.length;
  }
}

// ============ Helper Functions ============

/**
 * Check if value is serialized function
 */
function isSerializedFunction(value: unknown): value is SerializedFunction {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedFunction).__type === 'function'
  );
}

/**
 * Serialize props, convert functions to fnId
 */
function serializeProps(
  props: Record<string, unknown>,
  callbackRegistry: CallbackRegistry
): SerializedProps {
  const result: SerializedProps = {};

  for (const [key, value] of Object.entries(props)) {
    // Skip children and internal properties
    if (key === 'children' || key.startsWith('__')) continue;

    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      result[key] = serializeArray(value, callbackRegistry);
    } else if (typeof value === 'object' && value !== null) {
      result[key] = serializeObject(
        value as Record<string, unknown>,
        callbackRegistry
      );
    } else {
      result[key] = value as SerializedValue;
    }
  }

  return result;
}

/**
 * Serialize array
 */
function serializeArray(
  arr: unknown[],
  callbackRegistry: CallbackRegistry
): SerializedValue[] {
  return arr.map((item) => {
    if (typeof item === 'function') {
      const fnId = callbackRegistry.register(item);
      return { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(item)) {
      return serializeArray(item, callbackRegistry);
    } else if (typeof item === 'object' && item !== null) {
      return serializeObject(
        item as Record<string, unknown>,
        callbackRegistry
      );
    }
    return item as SerializedValue;
  });
}

/**
 * Serialize object
 */
function serializeObject(
  obj: Record<string, unknown>,
  callbackRegistry: CallbackRegistry
): Record<string, SerializedValue> {
  const result: Record<string, SerializedValue> = {};

  for (const [key, value] of Object.entries(obj)) {
    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      result[key] = serializeArray(value, callbackRegistry);
    } else if (typeof value === 'object' && value !== null) {
      result[key] = serializeObject(
        value as Record<string, unknown>,
        callbackRegistry
      );
    } else {
      result[key] = value as SerializedValue;
    }
  }

  return result;
}

/**
 * Compare props differences, find removed properties
 */
function getRemovedProps(
  oldProps: Record<string, unknown>,
  newProps: Record<string, unknown>
): string[] {
  const removed: string[] = [];
  for (const key of Object.keys(oldProps)) {
    if (key !== 'children' && !(key in newProps)) {
      removed.push(key);
    }
  }
  return removed;
}

// ============ Create Reconciler ============

/**
 * Root container type
 */
interface RootContainer {
  children: VNode[];
}

/**
 * Reconciler instance type
 */
type RillReconciler = Reconciler.Reconciler<
  RootContainer,
  VNode,
  VNode,
  VNode,
  unknown
>;

/**
 * Create custom renderer
 */
export function createReconciler(sendToHost: SendToHost): {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: OperationCollector;
} {
  const callbackRegistry = new CallbackRegistry();
  const collector = new OperationCollector();
  let nodeIdCounter = 0;

  const hostConfig: Reconciler.HostConfig<
    string, // Type
    Record<string, unknown>, // Props
    RootContainer, // Container
    VNode, // Instance
    VNode, // TextInstance
    unknown, // SuspenseInstance
    unknown, // HydratableInstance
    VNode, // PublicInstance
    object, // HostContext
    unknown, // UpdatePayload
    unknown, // ChildSet
    unknown, // TimeoutHandle
    unknown // NoTimeout
  > = {
    // ============ Core Methods ============

    createInstance(
      type: string,
      props: Record<string, unknown>
    ): VNode {
      const id = ++nodeIdCounter;
      const node: VNode = {
        id,
        type,
        props,
        children: [],
        parent: null,
      };

      const op: CreateOperation = {
        op: 'CREATE',
        id,
        type,
        props: serializeProps(props, callbackRegistry),
      };
      collector.add(op);

      return node;
    },

    createTextInstance(text: string): VNode {
      const id = ++nodeIdCounter;
      const node: VNode = {
        id,
        type: '__TEXT__',
        props: { text },
        children: [],
        parent: null,
      };

      const op: CreateOperation = {
        op: 'CREATE',
        id,
        type: '__TEXT__',
        props: { text },
      };
      collector.add(op);

      return node;
    },

    appendInitialChild(parent: VNode, child: VNode): void {
      parent.children.push(child);
      child.parent = parent;
    },

    appendChild(parent: VNode, child: VNode): void {
      parent.children.push(child);
      child.parent = parent;

      const op: AppendOperation = {
        op: 'APPEND',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      };
      collector.add(op);
    },

    appendChildToContainer(container: RootContainer, child: VNode): void {
      container.children.push(child);

      const op: AppendOperation = {
        op: 'APPEND',
        id: child.id,
        parentId: 0, // 0 represents root container
        childId: child.id,
      };
      collector.add(op);
    },

    insertBefore(
      parent: VNode,
      child: VNode,
      beforeChild: VNode
    ): void {
      const index = parent.children.indexOf(beforeChild);
      if (index !== -1) {
        parent.children.splice(index, 0, child);
      } else {
        parent.children.push(child);
      }
      child.parent = parent;

      const op: InsertOperation = {
        op: 'INSERT',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
        index: index !== -1 ? index : parent.children.length - 1,
      };
      collector.add(op);
    },

    insertInContainerBefore(
      container: RootContainer,
      child: VNode,
      beforeChild: VNode
    ): void {
      const index = container.children.indexOf(beforeChild);
      if (index !== -1) {
        container.children.splice(index, 0, child);
      } else {
        container.children.push(child);
      }

      const op: InsertOperation = {
        op: 'INSERT',
        id: child.id,
        parentId: 0,
        childId: child.id,
        index: index !== -1 ? index : container.children.length - 1,
      };
      collector.add(op);
    },

    removeChild(parent: VNode, child: VNode): void {
      const index = parent.children.indexOf(child);
      if (index !== -1) {
        parent.children.splice(index, 1);
      }
      child.parent = null;

      const op: RemoveOperation = {
        op: 'REMOVE',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      };
      collector.add(op);

      // Clean up callbacks for this node and its children
      cleanupNodeCallbacks(child, callbackRegistry);
    },

    removeChildFromContainer(container: RootContainer, child: VNode): void {
      const index = container.children.indexOf(child);
      if (index !== -1) {
        container.children.splice(index, 1);
      }

      const op: RemoveOperation = {
        op: 'REMOVE',
        id: child.id,
        parentId: 0,
        childId: child.id,
      };
      collector.add(op);

      cleanupNodeCallbacks(child, callbackRegistry);
    },

    prepareUpdate(
      _instance: VNode,
      _type: string,
      oldProps: Record<string, unknown>,
      newProps: Record<string, unknown>
    ): Record<string, unknown> | null {
      // Check for changes
      const keys = new Set([...Object.keys(oldProps), ...Object.keys(newProps)]);
      for (const key of keys) {
        if (key === 'children') continue;
        if (oldProps[key] !== newProps[key]) {
          return newProps;
        }
      }
      return null;
    },

    commitUpdate(
      instance: VNode,
      _updatePayload: Record<string, unknown> | null,
      _type: string,
      oldProps: Record<string, unknown>,
      newProps: Record<string, unknown>
    ): void {
      // Find removed properties
      const removedProps = getRemovedProps(oldProps, newProps);

      // Clean up old callbacks
      for (const key of Object.keys(oldProps)) {
        const value = oldProps[key];
        if (typeof value === 'function') {
          // Old callbacks need cleanup (will be re-registered if new props also have same-named function)
        }
      }

      instance.props = newProps;

      const op: UpdateOperation = {
        op: 'UPDATE',
        id: instance.id,
        props: serializeProps(newProps, callbackRegistry),
        removedProps: removedProps.length > 0 ? removedProps : undefined,
      };
      collector.add(op);
    },

    commitTextUpdate(textInstance: VNode, _oldText: string, newText: string): void {
      textInstance.props.text = newText;

      const op: UpdateOperation = {
        op: 'UPDATE',
        id: textInstance.id,
        props: { text: newText },
      };
      collector.add(op);
    },

    // ============ Container Methods ============

    getRootHostContext(): object {
      return {};
    },

    getChildHostContext(parentContext: object): object {
      return parentContext;
    },

    getPublicInstance(instance: VNode): VNode {
      return instance;
    },

    prepareForCommit(): Record<string, unknown> | null {
      return null;
    },

    resetAfterCommit(): void {
      collector.flush(sendToHost);
    },

    preparePortalMount(): void {
      // Portal support (not implemented)
    },

    // ============ Configuration ============

    supportsMutation: true,
    supportsPersistence: false,
    supportsHydration: false,

    isPrimaryRenderer: true,
    warnsIfNotActing: true,

    // Scheduling
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    noTimeout: -1,

    getCurrentEventPriority(): number {
      return DefaultEventPriority;
    },

    getInstanceFromNode(): null {
      return null;
    },

    beforeActiveInstanceBlur(): void {},
    afterActiveInstanceBlur(): void {},

    prepareScopeUpdate(): void {},
    getInstanceFromScope(): null {
      return null;
    },

    detachDeletedInstance(): void {},

    // ============ Unsupported Features ============

    finalizeInitialChildren(): boolean {
      return false;
    },

    shouldSetTextContent(): boolean {
      return false;
    },

    clearContainer(container: RootContainer): void {
      container.children = [];
    },

    hideInstance(): void {},
    hideTextInstance(): void {},
    unhideInstance(): void {},
    unhideTextInstance(): void {},

    // React 18 concurrent features
    supportsMicrotasks: true,
    scheduleMicrotask:
      typeof queueMicrotask === 'function' ? queueMicrotask : setTimeout,
  };

  const reconciler = Reconciler(hostConfig);

  return {
    reconciler,
    callbackRegistry,
    collector,
  };
}

/**
 * Recursively clean up callbacks for node and its children
 */
function cleanupNodeCallbacks(node: VNode, registry: CallbackRegistry): void {
  // Clean up current node's callbacks
  for (const value of Object.values(node.props)) {
    if (isSerializedFunction(value)) {
      registry.remove(value.__fnId);
    }
  }

  // Recursively clean up children
  for (const child of node.children) {
    cleanupNodeCallbacks(child, registry);
  }
}

// ============ Render Entry ============

let container: RootContainer | null = null;
let root: ReturnType<RillReconciler['createContainer']> | null = null;
let reconcilerInstance: {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: OperationCollector;
} | null = null;

/**
 * Render React element
 */
export function render(
  element: React.ReactElement,
  sendToHost: SendToHost
): void {
  if (!reconcilerInstance) {
    reconcilerInstance = createReconciler(sendToHost);
  }

  if (!container) {
    container = { children: [] };
    root = reconcilerInstance.reconciler.createContainer(
      container,
      0, // ConcurrentRoot
      null,
      false,
      null,
      'rill',
      (error: Error) => console.error('[rill] Recoverable error:', error),
      null
    );
  }

  reconcilerInstance.reconciler.updateContainer(element, root!, null, () => {});
}

/**
 * Unmount
 */
export function unmount(): void {
  if (root && reconcilerInstance) {
    reconcilerInstance.reconciler.updateContainer(null, root, null, () => {});
  }
  container = null;
  root = null;
  if (reconcilerInstance) {
    reconcilerInstance.callbackRegistry.clear();
  }
  reconcilerInstance = null;
}

/**
 * Get callback registry (for host to invoke callbacks)
 */
export function getCallbackRegistry(): CallbackRegistry | null {
  return reconcilerInstance?.callbackRegistry ?? null;
}

export { type VNode, type Operation, type OperationBatch, type SendToHost };
