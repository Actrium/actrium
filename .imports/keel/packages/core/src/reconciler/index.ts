/**
 * Rill Reconciler
 *
 * Custom renderer based on react-reconciler
 * Transforms React render actions into JSON instructions
 */

import type { ReactElement } from 'react';
import type { HostConfig, Reconciler as ReconcilerType } from 'react-reconciler';
// Static import for react-reconciler
import Reconciler from 'react-reconciler';
import { DefaultEventPriority } from 'react-reconciler/constants';

// Augment globalThis for debug tracking properties
declare global {
  // eslint-disable-next-line no-var
  var __OP_COUNTS: Record<string, number> | undefined;
  // eslint-disable-next-line no-var
  var __TOTAL_OPS: number | undefined;
  // eslint-disable-next-line no-var
  var __RECONCILER_MOUNTED: number | undefined;
  // eslint-disable-next-line no-var
  var __RECONCILER_UPDATED: number | undefined;
  // eslint-disable-next-line no-var
  var __RECONCILER_REMOVED: number | undefined;
  // eslint-disable-next-line no-var
  var __TOUCHABLE_CREATE_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __LAST_TOUCHABLE_FNID: string | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_CREATE_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __INSERT_CREATE_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __REMOVE_CREATE_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __RILL_RENDER_CALLED: boolean | undefined;
  // eslint-disable-next-line no-var
  var __RILL_RENDER_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_INITIAL_CALLED: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_CHILD_CALLED: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_TO_CONTAINER_CALLED: number | undefined;
  // eslint-disable-next-line no-var
  var __callbacks: Map<string, (...args: unknown[]) => unknown> | undefined;
}

import type {
  AppendOperation,
  CreateOperation,
  DeleteOperation,
  InsertOperation,
  Operation,
  OperationBatch,
  RemoveOperation,
  SendToHost,
  SerializedFunction,
  SerializedProps,
  SerializedValue,
  UpdateOperation,
  VNode,
} from '../types';

// ============ Callback Registry ============

/**
 * Callback Registry
 * Manages callback functions in sandbox, generates unique IDs for host invocation
 */
export class CallbackRegistry {
  private callbacks = new Map<string, (...args: unknown[]) => unknown>();
  private counter = 0;
  private instanceId = Math.random().toString(36).substring(2, 7);

  /**
   * Get internal callbacks Map (for syncing with globalThis.__callbacks)
   */
  getMap(): Map<string, (...args: unknown[]) => unknown> {
    return this.callbacks;
  }

  /**
   * Register callback function
   */
  register(fn: (...args: unknown[]) => unknown): string {
    const fnId = `fn_${this.instanceId}_${++this.counter}`;
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
    if (this.operations.length === 0) {
      return;
    }

    // 🔴 TRACK: Count operation types using global variable
    if (typeof globalThis !== 'undefined') {
      const opCounts: Record<string, number> = {};
      this.operations.forEach((op) => {
        opCounts[op.op] = (opCounts[op.op] || 0) + 1;
      });
      globalThis.__OP_COUNTS = opCounts;
      globalThis.__TOTAL_OPS = this.operations.length;
    }

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
  const visited = new WeakSet<object>();

  for (const [key, value] of Object.entries(props)) {
    // Skip children and internal properties
    if (key === 'children' || key.startsWith('__')) continue;

    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      const { value: serialized, fnIds: arrayFnIds } = serializeArrayWithTracking(
        value,
        callbackRegistry,
        visited
      );
      result[key] = serialized;
      arrayFnIds.forEach((id) => fnIds.add(id));
    } else if (typeof value === 'object' && value !== null) {
      const { value: serialized, fnIds: objFnIds } = serializeObjectWithTracking(
        value as Record<string, unknown>,
        callbackRegistry,
        visited
      );
      result[key] = serialized;
      objFnIds.forEach((id) => fnIds.add(id));
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
  callbackRegistry: CallbackRegistry,
  visited: WeakSet<object>
): { value: SerializedValue[]; fnIds: Set<string> } {
  // Check for circular reference
  if (visited.has(arr)) {
    return { value: [], fnIds: new Set() };
  }
  visited.add(arr);

  const fnIds = new Set<string>();
  const value = arr.map((item) => {
    if (typeof item === 'function') {
      const fnId = callbackRegistry.register(item as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      return { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(item)) {
      const { value: serialized, fnIds: nestedFnIds } = serializeArrayWithTracking(
        item,
        callbackRegistry,
        visited
      );
      nestedFnIds.forEach((id) => fnIds.add(id));
      return serialized;
    } else if (typeof item === 'object' && item !== null) {
      const { value: serialized, fnIds: nestedFnIds } = serializeObjectWithTracking(
        item as Record<string, unknown>,
        callbackRegistry,
        visited
      );
      nestedFnIds.forEach((id) => fnIds.add(id));
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
  callbackRegistry: CallbackRegistry,
  visited: WeakSet<object>
): { value: Record<string, SerializedValue>; fnIds: Set<string> } {
  // Check for circular reference
  if (visited.has(obj)) {
    return { value: {}, fnIds: new Set() };
  }
  visited.add(obj);

  const result: Record<string, SerializedValue> = {};
  const fnIds = new Set<string>();

  for (const [key, value] of Object.entries(obj)) {
    if (typeof value === 'function') {
      const fnId = callbackRegistry.register(value as (...args: unknown[]) => unknown);
      fnIds.add(fnId);
      result[key] = { __type: 'function', __fnId: fnId } as SerializedFunction;
    } else if (Array.isArray(value)) {
      const { value: serialized, fnIds: nestedFnIds } = serializeArrayWithTracking(
        value,
        callbackRegistry,
        visited
      );
      result[key] = serialized;
      nestedFnIds.forEach((id) => fnIds.add(id));
    } else if (typeof value === 'object' && value !== null) {
      const { value: serialized, fnIds: nestedFnIds } = serializeObjectWithTracking(
        value as Record<string, unknown>,
        callbackRegistry,
        visited
      );
      result[key] = serialized;
      nestedFnIds.forEach((id) => fnIds.add(id));
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
  RootContainer, // Container
  VNode, // Instance
  VNode, // TextInstance
  unknown, // SuspenseInstance
  VNode // PublicInstance
>;

/**
 * Extended host config with React 19 internal methods
 * These methods are used internally by React 19 but not yet in official types
 */
interface ExtendedHostConfig
  extends HostConfig<
    string,
    Record<string, unknown>,
    RootContainer,
    VNode,
    VNode,
    unknown,
    unknown,
    VNode,
    object,
    unknown,
    unknown,
    number,
    -1
  > {
  // Priority and scheduling methods
  resolveUpdatePriority: () => number;
  setCurrentUpdatePriority: () => void;
  getCurrentUpdatePriority: () => number;

  // Suspense-related methods
  maySuspendCommit: () => boolean;
  preloadInstance: () => boolean;
  startSuspendingCommit: () => void;
  suspendInstance: () => void;
  waitForCommitToBeReady: () => null;

  // Transition context
  NotPendingTransition: null;
  HostTransitionContext: null;

  // Misc methods
  requestPostPaintCallback: () => void;
  shouldAttemptEagerTransition: () => boolean;
  resolveEventType: () => null;
  resolveEventTimeStamp: () => number;
  resetFormInstance: () => void;
  trackSchedulerEvent: () => void;
}

/**
 * Create custom renderer
 */
export function createReconciler(
  sendToHost: SendToHost,
  callbackRegistry: CallbackRegistry
): {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: OperationCollector;
} {
  // react-reconciler is already statically imported at the top of this file

  const collector = new OperationCollector();
  let nodeIdCounter = 0;
  const pendingDeleteRoots = new Map<number, VNode>();

  // 🔧 FIX: Sync callbackRegistry with globalThis.__callbacks
  // This allows Engine.handleCallFunction() to find callbacks via __invokeCallback()
  if (typeof globalThis !== 'undefined') {
    globalThis.__callbacks = callbackRegistry.getMap();
  }

  const hostConfig: ExtendedHostConfig = {
    // ============ Core Methods ============

    createInstance(type: string, props: Record<string, unknown>): VNode {
      const id = ++nodeIdCounter;
      const { props: serializedProps, fnIds } = serializePropsWithTracking(props, callbackRegistry);

      // 🔴 TRACK: Log if this is TouchableOpacity with onPress
      if (type === 'TouchableOpacity' && props.onPress) {
        if (typeof globalThis !== 'undefined') {
          globalThis.__TOUCHABLE_CREATE_COUNT = (globalThis.__TOUCHABLE_CREATE_COUNT || 0) + 1;
          globalThis.__LAST_TOUCHABLE_FNID = Array.from(fnIds)[0];
        }
      }

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
      // 🔴 TRACK: Use global variable since console.log is filtered
      if (typeof globalThis !== 'undefined') {
        globalThis.__APPEND_INITIAL_CALLED = (globalThis.__APPEND_INITIAL_CALLED || 0) + 1;
      }

      parent.children.push(child);
      child.parent = parent;
      pendingDeleteRoots.delete(child.id);

      // 🔴 FIX: Send APPEND operation during initial render to establish parent-child relationships
      const op: AppendOperation = {
        op: 'APPEND',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      };
      collector.add(op);
    },

    appendChild(parent: VNode, child: VNode): void {
      parent.children.push(child);
      child.parent = parent;
      pendingDeleteRoots.delete(child.id);

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
      pendingDeleteRoots.delete(child.id);

      const op: AppendOperation = {
        op: 'APPEND',
        id: child.id,
        parentId: 0, // 0 represents root container
        childId: child.id,
      };
      collector.add(op);
    },

    insertBefore(parent: VNode, child: VNode, beforeChild: VNode): void {
      const index = parent.children.indexOf(beforeChild);
      if (index !== -1) {
        parent.children.splice(index, 0, child);
      } else {
        parent.children.push(child);
      }
      child.parent = parent;
      pendingDeleteRoots.delete(child.id);

      const op: InsertOperation = {
        op: 'INSERT',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
        index: index !== -1 ? index : parent.children.length - 1,
      };
      collector.add(op);
    },

    insertInContainerBefore(container: RootContainer, child: VNode, beforeChild: VNode): void {
      const index = container.children.indexOf(beforeChild);
      if (index !== -1) {
        container.children.splice(index, 0, child);
      } else {
        container.children.push(child);
      }
      pendingDeleteRoots.delete(child.id);

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
      pendingDeleteRoots.set(child.id, child);

      const op: RemoveOperation = {
        op: 'REMOVE',
        id: child.id,
        parentId: parent.id,
        childId: child.id,
      };
      collector.add(op);
    },

    removeChildFromContainer(container: RootContainer, child: VNode): void {
      const index = container.children.indexOf(child);
      if (index !== -1) {
        container.children.splice(index, 1);
      }
      pendingDeleteRoots.set(child.id, child);

      const op: RemoveOperation = {
        op: 'REMOVE',
        id: child.id,
        parentId: 0,
        childId: child.id,
      };
      collector.add(op);
    },

    prepareUpdate(
      _instance: VNode,
      _type: string,
      oldProps: Record<string, unknown>,
      newProps: Record<string, unknown>,
      _rootContainer: RootContainer,
      _hostContext: object
    ): unknown | null {
      // Return a truthy value to indicate an update is needed
      // The actual diffing happens in commitUpdate
      return oldProps !== newProps ? {} : null;
    },

    // React 18/19: commitUpdate signature vs runtime call may differ:
    // - React 18 (common type): commitUpdate(instance, updatePayload, type, prevProps, nextProps, internalHandle)
    // - React 19 (runtime): commitUpdate(instance, type, prevProps, nextProps, internalHandle)
    // Use runtime type check to handle both, avoiding treating internalHandle(Fiber) as props.
    commitUpdate(
      instance: VNode,
      updatePayloadOrType: unknown,
      typeOrPrevProps: unknown,
      prevPropsOrNextProps: unknown,
      nextPropsOrInternalHandle: unknown,
      _internalHandleMaybe?: unknown
    ): void {
      const isReact19 = typeof updatePayloadOrType === 'string';
      const oldProps = (isReact19 ? typeOrPrevProps : prevPropsOrNextProps) as Record<
        string,
        unknown
      >;
      const newProps = (isReact19 ? prevPropsOrNextProps : nextPropsOrInternalHandle) as Record<
        string,
        unknown
      >;

      // Find removed properties
      const removedProps = getRemovedProps(oldProps, newProps);

      // Clean up old registered callbacks
      if (instance.registeredFnIds) {
        instance.registeredFnIds.forEach((fnId) => callbackRegistry.remove(fnId));
      }

      // Serialize new props and track new callbacks
      const { props: serializedProps, fnIds: newFnIds } = serializePropsWithTracking(
        newProps,
        callbackRegistry
      );

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
      for (const [id, root] of pendingDeleteRoots) {
        const op: DeleteOperation = { op: 'DELETE', id };
        collector.add(op);
        cleanupNodeCallbacks(root, callbackRegistry);
      }
      pendingDeleteRoots.clear();
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
      // React may call clearContainer() during unmount. We translate that into
      // explicit REMOVE ops + deferred DELETE (in resetAfterCommit) so that:
      // - Host Receiver detaches root children
      // - CallbackRegistry does not leak function IDs
      for (const child of container.children) {
        pendingDeleteRoots.set(child.id, child);
        const op: RemoveOperation = {
          op: 'REMOVE',
          id: child.id,
          parentId: 0,
          childId: child.id,
        };
        collector.add(op);
      }
      container.children = [];
    },

    hideInstance(): void {},
    hideTextInstance(): void {},
    unhideInstance(): void {},
    unhideTextInstance(): void {},

    // React 18 concurrent features
    supportsMicrotasks: true,
    scheduleMicrotask: typeof queueMicrotask === 'function' ? queueMicrotask : setTimeout,

    // React 19 internal methods (not in official HostConfig types yet)
    resolveUpdatePriority: () => DefaultEventPriority,
    setCurrentUpdatePriority: () => {},
    getCurrentUpdatePriority: () => DefaultEventPriority,
    maySuspendCommit: () => false,
    preloadInstance: () => false,
    startSuspendingCommit: () => {},
    suspendInstance: () => {},
    waitForCommitToBeReady: () => null,
    NotPendingTransition: null,
    HostTransitionContext: null,
    requestPostPaintCallback: () => {},
    shouldAttemptEagerTransition: () => false,
    resolveEventType: () => null,
    resolveEventTimeStamp: () => Date.now(),
    resetFormInstance: () => {},
    trackSchedulerEvent: () => {},
  };

  const reconciler = Reconciler(hostConfig) as RillReconciler;

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
    node.registeredFnIds.forEach((fnId) => registry.remove(fnId));
    node.registeredFnIds = undefined;
  }

  // Recursively clean up children
  for (const child of node.children) {
    cleanupNodeCallbacks(child, registry);
  }
}

// ============ Reconciler Entry ============

/**
 * Reconciler instance data for each guest
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
 * This allows multiple guests to each have their own isolated reconciler
 */
const reconcilerMap = new Map<SendToHost, ReconcilerInstance>();

// Create a single, global instance of the callback registry
// This ensures that callbacks are preserved across engine re-creations,
// fixing the "Callback not found" issue caused by component remounts.
const globalCallbackRegistry = new CallbackRegistry();

// Cache Host-side React element symbol for cross-engine compatibility
const REACT_ELEMENT_TYPE = Symbol.for('react.element');
const REACT_FRAGMENT_TYPE = Symbol.for('react.fragment');

// String markers used by Guest shims (survive JSI serialization)
const RILL_ELEMENT_MARKER = '__rill_react_element__';
const RILL_FRAGMENT_MARKER = '__rill_react_fragment__';

/**
 * Transform Guest element to use Host's Symbol registry
 *
 * When Guest (JSC sandbox) creates React elements, it uses its own Symbol registry.
 * Host (Hermes) has a different Symbol registry, so Symbol.for('react.element')
 * returns different Symbols in each engine. Additionally, JSI doesn't preserve
 * Symbols across the engine boundary at all - they become undefined.
 *
 * We use string markers (__rillTypeMarker, __rillFragmentType) that survive
 * JSI serialization to identify React elements from the Guest.
 *
 * This function recursively transforms Guest elements to use Host Symbols.
 */
function transformGuestElement(element: unknown): ReactElement | null {
  if (element === null || element === undefined) {
    return null;
  }

  // Handle primitive values (text nodes)
  if (typeof element !== 'object') {
    return element as unknown as ReactElement;
  }

  // Handle arrays (children)
  if (Array.isArray(element)) {
    return element.map(transformGuestElement) as unknown as ReactElement;
  }

  const el = element as Record<string, unknown>;

  // Check if this is a Rill Guest element using the string marker
  // This marker survives JSI serialization while Symbols don't
  const isRillElement = el.__rillTypeMarker === RILL_ELEMENT_MARKER;

  // Also check for $$typeof Symbol (works when not crossing JSI, e.g., NoSandbox provider)
  const hasSymbolType = typeof el.$$typeof === 'symbol';

  if (!isRillElement && !hasSymbolType) {
    // Not a React element, return as-is
    return element as ReactElement;
  }

  // Determine if this is a Fragment
  const isFragment =
    el.__rillFragmentType === RILL_FRAGMENT_MARKER ||
    (typeof el.type === 'symbol' && (el.type as symbol).description === 'react.fragment');

  // Transform the element type
  let transformedType = el.type;
  if (isFragment) {
    transformedType = REACT_FRAGMENT_TYPE;
  }

  // Transform children recursively
  const props = el.props as Record<string, unknown> | undefined;
  let transformedProps = props;
  if (props && props.children !== undefined) {
    const transformedChildren = Array.isArray(props.children)
      ? props.children.map(transformGuestElement)
      : transformGuestElement(props.children);
    transformedProps = { ...props, children: transformedChildren };
  }

  // Create a new element with Host's Symbol
  return {
    $$typeof: REACT_ELEMENT_TYPE,
    type: transformedType,
    key: el.key,
    ref: el.ref,
    props: transformedProps,
    _owner: el._owner,
    _store: el._store,
  } as unknown as ReactElement;
}

/**
 * Render React element
 *
 * Each unique sendToHost function gets its own isolated reconciler instance.
 * This allows multiple guests to run simultaneously without interfering with each other.
 */
export function render(element: ReactElement, sendToHost: SendToHost): void {
  // Transform Guest element to use Host's Symbol registry
  const transformedElement = transformGuestElement(element);

  let instance = reconcilerMap.get(sendToHost);

  if (!instance) {
    // Create new reconciler instance for this guest
    const reconcilerInstance = createReconciler(sendToHost, globalCallbackRegistry);
    const container: RootContainer = { children: [] };
    const root = reconcilerInstance.reconciler.createContainer(
      container,
      0, // ConcurrentRoot
      null,
      false,
      null,
      'rill',
      (error: Error) => console.error('[rill] Error:', error), // onUncaughtError
      null // formState
    );

    instance = {
      ...reconcilerInstance,
      container,
      root,
    };

    reconcilerMap.set(sendToHost, instance);
  }

  instance.reconciler.updateContainer(transformedElement, instance.root, null, () => {});
}

/**
 * Unmount a specific guest instance
 */
export function unmount(sendToHost?: SendToHost): void {
  if (!sendToHost) return;
  const instance = reconcilerMap.get(sendToHost);
  if (instance) {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    // DO NOT clear the global callback registry here, as it's shared
    // instance.callbackRegistry.clear();
    reconcilerMap.delete(sendToHost);
  }
}
/**
 * Unmount all guest instances
 */
export function unmountAll(): void {
  reconcilerMap.forEach((instance) => {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    instance.callbackRegistry.clear();
  });
  reconcilerMap.clear();
}

/**
 * Get callback registry for a specific guest instance
 * Used for resource monitoring and debugging
 */
export function getCallbackRegistry(sendToHost?: SendToHost): CallbackRegistry | null {
  if (!sendToHost) {
    return null;
  }
  const instance = reconcilerMap.get(sendToHost);
  return instance ? instance.callbackRegistry : null;
}

/**
 * Check if a callback exists (global registry)
 *
 * Note: In RN / VM runtimes, Guest-side functions are seen by Host-side reconciler
 * as callable handles and registered in CallbackRegistry.
 * No need to rely on Guest runtime injected globalThis.__callbacks / __invokeCallback.
 */
export function hasCallback(fnId: string): boolean {
  return globalCallbackRegistry.getMap().has(fnId);
}

/**
 * Invoke a callback (global registry)
 */
export function invokeCallback(fnId: string, args: unknown[] = []): unknown {
  return globalCallbackRegistry.invoke(fnId, args);
}

export type { VNode, Operation, OperationBatch, SendToHost };
