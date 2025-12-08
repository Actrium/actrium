/**
 * Rill Reconciler
 *
 * Custom renderer based on react-reconciler
 * Transforms React render actions into JSON instructions
 */

// Static import for react-reconciler
import Reconciler from 'react-reconciler';
import type { HostConfig, Reconciler as ReconcilerType } from 'react-reconciler';
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
 * Serialize result with tracked function IDs
 */
interface SerializeResult {
  props: SerializedProps;
  fnIds: Set<string>;
}

/**
 * Serialize props, convert functions to fnId and track registered IDs
 */
function serializePropsWithTracking(
  props: Record<string, unknown>,
  callbackRegistry: CallbackRegistry
): SerializeResult {
  const result: SerializedProps = {};
  const fnIds = new Set<string>();

  for (const [key, value] of Object.entries(props)) {
    // Skip children and internal properties
    if (key === 'children' || key.startsWith('__')) continue;

    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      const { value: serialized, fnIds: arrayFnIds } = serializeArrayWithTracking(value, callbackRegistry);
      result[key] = serialized;
      arrayFnIds.forEach(id => fnIds.add(id));
    } else if (typeof value === 'object' && value !== null) {
      const { value: serialized, fnIds: objFnIds } = serializeObjectWithTracking(
        value as Record<string, unknown>,
        callbackRegistry
      );
      result[key] = serialized;
      objFnIds.forEach(id => fnIds.add(id));
    } else {
      result[key] = value as SerializedValue;
    }
  }

  return { props: result, fnIds };
}

/**
 * Serialize array with tracking
 */
function serializeArrayWithTracking(
  arr: unknown[],
  callbackRegistry: CallbackRegistry
): { value: SerializedValue[]; fnIds: Set<string> } {
  const fnIds = new Set<string>();
  const value = arr.map((item) => {
    if (typeof item === 'function') {
      const fnId = callbackRegistry.register(item as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      return { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(item)) {
      const { value: serialized, fnIds: nestedFnIds } = serializeArrayWithTracking(item, callbackRegistry);
      nestedFnIds.forEach(id => fnIds.add(id));
      return serialized;
    } else if (typeof item === 'object' && item !== null) {
      const { value: serialized, fnIds: nestedFnIds } = serializeObjectWithTracking(
        item as Record<string, unknown>,
        callbackRegistry
      );
      nestedFnIds.forEach(id => fnIds.add(id));
      return serialized;
    }
    return item as SerializedValue;
  });
  return { value, fnIds };
}

/**
 * Serialize object with tracking
 */
function serializeObjectWithTracking(
  obj: Record<string, unknown>,
  callbackRegistry: CallbackRegistry
): { value: Record<string, SerializedValue>; fnIds: Set<string> } {
  const result: Record<string, SerializedValue> = {};
  const fnIds = new Set<string>();

  for (const [key, value] of Object.entries(obj)) {
    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      const { value: serialized, fnIds: nestedFnIds } = serializeArrayWithTracking(value, callbackRegistry);
      result[key] = serialized;
      nestedFnIds.forEach(id => fnIds.add(id));
    } else if (typeof value === 'object' && value !== null) {
      const { value: serialized, fnIds: nestedFnIds } = serializeObjectWithTracking(
        value as Record<string, unknown>,
        callbackRegistry
      );
      result[key] = serialized;
      nestedFnIds.forEach(id => fnIds.add(id));
    } else {
      result[key] = value as SerializedValue;
    }
  }

  return { value: result, fnIds };
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

// ============ React Reconciler ============
// react-reconciler is statically imported at the top of the file for Metro compile-time resolution

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
type RillReconciler = ReconcilerType<
  RootContainer,
  VNode,
  VNode,
  unknown,
  VNode,
  object
>;

/**
 * Create custom renderer
 */
export function createReconciler(sendToHost: SendToHost): {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: OperationCollector;
} {
  // react-reconciler is already statically imported at the top of this file

  const callbackRegistry = new CallbackRegistry();
  const collector = new OperationCollector();
  let nodeIdCounter = 0;

  const hostConfig: HostConfig<
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
    unknown, // NoTimeout
    unknown // TransitionStatus
  > = {
    // ============ Core Methods ============

    createInstance(
      type: string,
      props: Record<string, unknown>
    ): VNode {
      const id = ++nodeIdCounter;
      const { props: serializedProps, fnIds } = serializePropsWithTracking(props, callbackRegistry);

      const node: VNode = {
        id,
        type,
        props,
        children: [],
        parent: null,
        registeredFnIds: fnIds.size > 0 ? fnIds : undefined,
      };

      const op: CreateOperation = {
        op: 'CREATE',
        id,
        type,
        props: serializedProps,
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

    commitUpdate(
      instance: VNode,
      _type: string,
      oldProps: Record<string, unknown>,
      newProps: Record<string, unknown>,
      _internalHandle: unknown
    ): void {
      // Find removed properties
      const removedProps = getRemovedProps(oldProps, newProps);

      // Clean up old registered callbacks
      if (instance.registeredFnIds) {
        instance.registeredFnIds.forEach(fnId => callbackRegistry.remove(fnId));
      }

      // Serialize new props and track new callbacks
      const { props: serializedProps, fnIds: newFnIds } = serializePropsWithTracking(newProps, callbackRegistry);

      instance.props = newProps;
      instance.registeredFnIds = newFnIds.size > 0 ? newFnIds : undefined;

      const op: UpdateOperation = {
        op: 'UPDATE',
        id: instance.id,
        props: serializedProps,
        removedProps: removedProps.length > 0 ? removedProps : undefined,
      };
      collector.add(op);
    },

    commitTextUpdate(textInstance: VNode, _oldText: string, newText: string): void {
      textInstance.props['text'] = newText;

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

    isPrimaryRenderer: false,
    warnsIfNotActing: true,

    // Scheduling
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    noTimeout: -1,

    getCurrentUpdatePriority(): number {
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

    // React 19 new features
    NotPendingTransition: null as never,
    HostTransitionContext: null as never,

    setCurrentUpdatePriority(_priority: number): void {},

    resolveUpdatePriority(): number {
      return DefaultEventPriority;
    },

    shouldAttemptEagerTransition(): boolean {
      return false;
    },

    requestPostPaintCallback(_callback: (time: number) => void): void {},

    maySuspendCommit(): boolean {
      return false;
    },

    preloadInstance(): boolean {
      return true;
    },

    startSuspendingCommit(): void {},

    suspendInstance(): void {},

    waitForCommitToBeReady(): null {
      return null;
    },

    resetFormInstance(): void {},

    trackSchedulerEvent(): void {},

    resolveEventType(): string {
      return 'unknown';
    },

    resolveEventTimeStamp(): number {
      return Date.now();
    },
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
  // Clean up current node's registered callbacks
  if (node.registeredFnIds) {
    node.registeredFnIds.forEach(fnId => registry.remove(fnId));
    node.registeredFnIds = undefined;
  }

  // Recursively clean up children
  for (const child of node.children) {
    cleanupNodeCallbacks(child, registry);
  }
}

// ============ Render Entry ============

/**
 * Reconciler instance data for each plugin
 */
interface ReconcilerInstance {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: OperationCollector;
  container: RootContainer;
  root: ReturnType<RillReconciler['createContainer']>;
}

/**
 * Map of sendToHost functions to their reconciler instances
 * This allows multiple plugins to each have their own isolated reconciler
 */
const reconcilerMap = new Map<SendToHost, ReconcilerInstance>();

/**
 * Render React element
 *
 * Each unique sendToHost function gets its own isolated reconciler instance.
 * This allows multiple plugins to run simultaneously without interfering with each other.
 */
export function render(
  element: React.ReactElement,
  sendToHost: SendToHost
): void {
  let instance = reconcilerMap.get(sendToHost);

  if (!instance) {
    // Create new reconciler instance for this plugin
    const reconcilerInstance = createReconciler(sendToHost);
    const container: RootContainer = { children: [] };
    const root = reconcilerInstance.reconciler.createContainer(
      container,
      0, // ConcurrentRoot
      null,
      false,
      null,
      'rill',
      (error: Error, _info: unknown) => console.error('[rill] Uncaught error:', error), // onUncaughtError
      (error: Error, _info: unknown) => console.error('[rill] Caught error:', error), // onCaughtError
      (error: Error, _info: unknown) => console.error('[rill] Recoverable error:', error), // onRecoverableError
      () => {}, // onTransitionStart
      null
    );

    instance = {
      ...reconcilerInstance,
      container,
      root,
    };

    reconcilerMap.set(sendToHost, instance);
  }

  instance.reconciler.updateContainer(element, instance.root, null, () => {});
}

/**
 * Unmount a specific plugin instance
 */
export function unmount(sendToHost: SendToHost): void {
  const instance = reconcilerMap.get(sendToHost);
  if (instance) {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    instance.callbackRegistry.clear();
    reconcilerMap.delete(sendToHost);
  }
}

/**
 * Unmount all plugin instances
 */
export function unmountAll(): void {
  reconcilerMap.forEach(instance => {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    instance.callbackRegistry.clear();
  });
  reconcilerMap.clear();
}

/**
 * Get callback registry for a specific plugin instance
 * @deprecated This function assumes a single plugin instance. Use the plugin's own callback management instead.
 */
export function getCallbackRegistry(sendToHost?: SendToHost): CallbackRegistry | null {
  if (sendToHost) {
    const instance = reconcilerMap.get(sendToHost);
    return instance?.callbackRegistry ?? null;
  }
  // Fallback: return the first available registry (for backward compatibility)
  const firstInstance = reconcilerMap.values().next().value;
  return firstInstance?.callbackRegistry ?? null;
}

export { type VNode, type Operation, type OperationBatch, type SendToHost };
