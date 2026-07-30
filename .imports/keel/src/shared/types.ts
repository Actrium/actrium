/**
 * @rill/bridge - Protocol Type System
 *
 * A complete type system built on what JSI can support natively
 * - types JSI supports natively: passed straight through, zero overhead
 * - types the bridge has to handle: serialized on the way out, deserialized on the way in
 *
 * This is the protocol layer shared between host and guest
 */

// ============================================
// Type Safety
// ============================================

/**
 * Reviewed Unknown Type
 *
 * Use of `unknown` is allowed only when it's explicit and intentional.
 * Prefer specific types when possible; otherwise use ReviewedUnknown and validate at runtime.
 */
export type ReviewedUnknown = unknown;

// ============================================
// Natively supported by JSI (passed straight through, zero overhead)
// ============================================

/**
 * JSI primitive types
 */
export type JSIPrimitive = null | undefined | boolean | number | string;

/**
 * JSI-safe type - can be passed across the boundary directly
 */
export type JSISafe = JSIPrimitive | JSISafeArray | JSISafeObject;

/**
 * JSI-safe array
 */
export interface JSISafeArray extends Array<JSISafe> {}

/**
 * JSI-safe object
 */
export interface JSISafeObject {
  [key: string]: JSISafe;
}

// ============================================
// Needs bridge handling (JSI cannot carry these directly)
// ============================================

/**
 * Types that require serialization
 */
export type RequiresBridge =
  // biome-ignore lint/complexity/noBannedTypes: Generic function type for serialization
  Function | Date | RegExp | Error | Map<BridgeValue, BridgeValue> | Set<BridgeValue>;

// ============================================
// Complete Bridge type
// ============================================

/**
 * The complete value type Bridge supports
 */
export type BridgeValue = JSIPrimitive | RequiresBridge | BridgeValueArray | BridgeValueObject;

/**
 * Bridge value array
 */
export interface BridgeValueArray extends Array<BridgeValue> {}

/**
 * Bridge value object
 */
export interface BridgeValueObject {
  [key: string]: BridgeValue;
}

// ============================================
// Serialized types
// ============================================

/**
 * Serialized function
 */
export interface SerializedFunction {
  __type: 'function';
  __fnId: string;
  __name?: string; // Original function name for DevTools
  __sourceFile?: string; // Original source file path (from Babel plugin)
  __sourceLine?: number; // Original source line number (from Babel plugin)
}

/**
 * Serialized date
 */
export interface SerializedDate {
  __type: 'date';
  __value: string; // ISO 8601
}

/**
 * Serialized regexp
 */
export interface SerializedRegExp {
  __type: 'regexp';
  __source: string;
  __flags: string;
}

/**
 * Serialized error
 */
export interface SerializedError {
  __type: 'error';
  __name: string;
  __message: string;
  __stack?: string;
}

/**
 * Serialized Map
 */
export interface SerializedMap {
  __type: 'map';
  __entries: [SerializedValue, SerializedValue][];
}

/**
 * Serialized Set
 */
export interface SerializedSet {
  __type: 'set';
  __values: SerializedValue[];
}

/**
 * Serialized Promise
 * A promise is tracked by id; the result comes back asynchronously in a promise:settle message
 */
export interface SerializedPromise {
  __type: 'promise';
  __promiseId: string;
}

/**
 * All serialized special types
 */
export type SerializedSpecialType =
  | SerializedFunction
  | SerializedDate
  | SerializedRegExp
  | SerializedError
  | SerializedMap
  | SerializedSet
  | SerializedPromise;

/**
 * Serialized value - JSI safe
 */
export type SerializedValue =
  | JSIPrimitive
  | SerializedSpecialType
  | SerializedValueArray
  | SerializedValueObject;

/**
 * Serialized value array
 */
export interface SerializedValueArray extends Array<SerializedValue> {}

/**
 * Serialized value object
 */
export interface SerializedValueObject {
  [key: string]: SerializedValue;
}

// ============================================
// Operation types (Guest -> Host)
// ============================================

/**
 * Enumeration of operation types
 */
export const OPERATION_TYPES = [
  'CREATE',
  'UPDATE',
  'DELETE',
  'APPEND',
  'INSERT',
  'REMOVE',
  'REORDER',
  'TEXT',
  'REF_CALL',
] as const;

export type OperationType = (typeof OPERATION_TYPES)[number];

/**
 * Base operation interface
 */
export interface BaseOperation {
  op: OperationType;
  id: number;
  timestamp?: number;
}

export interface CreateOperation extends BaseOperation {
  op: 'CREATE';
  type: string;
  props: BridgeValueObject;
}

export interface UpdateOperation extends BaseOperation {
  op: 'UPDATE';
  props: BridgeValueObject;
  removedProps?: string[];
}

export interface DeleteOperation extends BaseOperation {
  op: 'DELETE';
}

export interface AppendOperation extends BaseOperation {
  op: 'APPEND';
  parentId: number;
  childId: number;
}

export interface InsertOperation extends BaseOperation {
  op: 'INSERT';
  parentId: number;
  childId: number;
  index: number;
}

export interface RemoveOperation extends BaseOperation {
  op: 'REMOVE';
  parentId: number;
  childId: number;
}

export interface ReorderOperation extends BaseOperation {
  op: 'REORDER';
  parentId: number;
  childIds: number[];
}

export interface TextOperation extends BaseOperation {
  op: 'TEXT';
  text: string;
}

/**
 * Remote Ref method-call operation
 * Guest calls a method on a host component instance (focus, blur, scrollTo, ...)
 */
export interface RefCallOperation extends BaseOperation {
  op: 'REF_CALL';
  refId: number; // node id (the same id used in nodeMap)
  method: string; // method name: focus, blur, measure, scrollTo, ...
  args: BridgeValue[]; // method arguments
  callId: string; // unique call id (used to match up the promise)
}

/**
 * Serialized REF_CALL operation
 */
export interface SerializedRefCallOperation extends BaseOperation {
  op: 'REF_CALL';
  refId: number;
  method: string;
  args: SerializedValue[];
  callId: string;
}

/**
 * Operation - Discriminated Union
 */
export type Operation =
  | CreateOperation
  | UpdateOperation
  | DeleteOperation
  | AppendOperation
  | InsertOperation
  | RemoveOperation
  | ReorderOperation
  | TextOperation
  | RefCallOperation;

/**
 * Operation batch
 */
export interface OperationBatch {
  version: number;
  batchId: number;
  operations: Operation[];
}

/**
 * Serialized operation batch
 */
export interface SerializedOperationBatch {
  version: number;
  batchId: number;
  operations: SerializedOperation[];
}

/**
 * Serialized CREATE operation
 */
export interface SerializedCreateOperation extends BaseOperation {
  op: 'CREATE';
  type: string;
  props: SerializedValueObject;
}

/**
 * Serialized UPDATE operation
 */
export interface SerializedUpdateOperation extends BaseOperation {
  op: 'UPDATE';
  props: SerializedValueObject;
  removedProps?: string[];
}

/**
 * Serialized operation - props uses SerializedValueObject
 */
export type SerializedOperation =
  | SerializedCreateOperation
  | SerializedUpdateOperation
  | DeleteOperation
  | AppendOperation
  | InsertOperation
  | RemoveOperation
  | ReorderOperation
  | TextOperation
  | SerializedRefCallOperation;

/**
 * Check whether an operation carries props
 */
export function operationHasProps(
  op: Operation | SerializedOperation
): op is CreateOperation | UpdateOperation | SerializedCreateOperation | SerializedUpdateOperation {
  return op.op === 'CREATE' || op.op === 'UPDATE';
}

// ============================================
// Message types (Host -> Guest)
// ============================================

export enum HostMsg {
  CALL_FUNCTION = 'CALL_FUNCTION',
  HOST_EVENT = 'HOST_EVENT',
  CONFIG_UPDATE = 'CONFIG_UPDATE',
  DESTROY = 'DESTROY',
  PROMISE_RESOLVE = 'PROMISE_RESOLVE',
  PROMISE_REJECT = 'PROMISE_REJECT',
  REF_METHOD_RESULT = 'REF_METHOD_RESULT',
}

export interface CallFunctionMessage {
  type: HostMsg.CALL_FUNCTION;
  fnId: string;
  args: BridgeValue[];
}

export interface HostEventMessage {
  type: HostMsg.HOST_EVENT;
  eventName: string;
  payload: BridgeValue;
}

export interface ConfigUpdateMessage {
  type: HostMsg.CONFIG_UPDATE;
  config: BridgeValueObject;
}

export interface DestroyMessage {
  type: HostMsg.DESTROY;
}

/**
 * Remote Ref method-call result message
 * The result the host returns to the guest for a ref method call
 */
export interface RefMethodResultMessage {
  type: HostMsg.REF_METHOD_RESULT;
  refId: number; // node id
  callId: string; // unique call id (pairs with the REF_CALL)
  result?: BridgeValue; // return value on success
  error?: SerializedError; // error info on failure
}

/**
 * Host -> Guest messages
 */
export type HostMessage =
  | CallFunctionMessage
  | HostEventMessage
  | ConfigUpdateMessage
  | DestroyMessage
  | PromiseSettleMessage
  | RefMethodResultMessage;

/**
 * Promise settlement message - used to asynchronously deliver a promise's result
 */
export type PromiseSettleMessage =
  | { type: HostMsg.PROMISE_RESOLVE; promiseId: string; value: BridgeValue }
  | { type: HostMsg.PROMISE_REJECT; promiseId: string; error: SerializedError };

/**
 * Serialized REF_METHOD_RESULT message
 */
export interface SerializedRefMethodResultMessage {
  type: HostMsg.REF_METHOD_RESULT;
  refId: number;
  callId: string;
  result?: SerializedValue;
  error?: SerializedError;
}

export type SerializedHostMessage =
  | { type: HostMsg.CALL_FUNCTION; fnId: string; args: SerializedValue[] }
  | { type: HostMsg.HOST_EVENT; eventName: string; payload: SerializedValue }
  | { type: HostMsg.CONFIG_UPDATE; config: SerializedValueObject }
  | { type: HostMsg.DESTROY }
  | SerializedPromiseSettleMessage
  | SerializedRefMethodResultMessage;

/**
 * Serialized promise settlement message
 */
export type SerializedPromiseSettleMessage =
  | { type: HostMsg.PROMISE_RESOLVE; promiseId: string; value: SerializedValue }
  | { type: HostMsg.PROMISE_REJECT; promiseId: string; error: SerializedError };

// ============================================
// Callback Registry interface
// ============================================

/**
 * Callback Registry - manages function calls across the boundary
 *
 * Note: Uses ReviewedUnknown for flexibility with existing implementations.
 * The actual serialization/deserialization is handled by Bridge.
 */
export interface CallbackRegistry {
  /**
   * Register a function and return its fnId
   */
  register(fn: (...args: ReviewedUnknown[]) => ReviewedUnknown): string;

  /**
   * Invoke the function identified by fnId
   */
  invoke(fnId: string, args: ReviewedUnknown[]): ReviewedUnknown;

  /**
   * Check whether fnId exists
   */
  has(fnId: string): boolean;

  /**
   * Remove the given fnId
   */
  remove(fnId: string): void;

  /**
   * Clear all registered functions
   */
  clear(): void;

  /**
   * Increment the reference count
   */
  retain(fnId: string): void;

  /**
   * Drop a reference; the entry is removed once the count reaches zero
   */
  release(fnId: string): void;

  /**
   * Get the reference count
   */
  getRefCount(fnId: string): number;

  /**
   * The backing Map, for syncing into globalThis.__rill.callbacks
   */
  getMap(): Map<string, (...args: ReviewedUnknown[]) => ReviewedUnknown>;

  /**
   * Number of currently registered functions
   */
  readonly size: number;
}

// ============================================
// Convenience type aliases
// ============================================

/**
 * Serialized props object
 */
export type SerializedProps = SerializedValueObject;

/**
 * Send operations from Guest to Host.
 * The batch format depends on the provider:
 * - VM: OperationBatch (raw, same runtime)
 * - WASM: SerializedOperationBatch (pre-serialized, Function → fnId)
 * - Native sandbox: string (JSON-serialized to avoid JSI stack overflow)
 *
 * NOTE: there is deliberately no ArrayBuffer arm. The JS guest has no
 * schema-conformant binary op-batch encoder (the legacy TS pair was retired;
 * see contracts/op-batch-wire.MATURITY.md §4a) — the only binary op-batch
 * route is the Rust WASM guest, whose frames are decoded by the host's
 * wire decoder (src/host/wire/wire-decoder.ts) before they ever reach a
 * SendToHost boundary.
 */
export type SendToHost = (batch: OperationBatch | SerializedOperationBatch | string) => void;

// ============================================
// Type Guards
// ============================================

/**
 * Check whether the value is a JSI primitive type
 */
export function isJSIPrimitive(value: BridgeValue): value is JSIPrimitive {
  return (
    value === null ||
    value === undefined ||
    typeof value === 'boolean' ||
    typeof value === 'number' ||
    typeof value === 'string'
  );
}

/**
 * Check whether the value is a serialized function
 */
export function isSerializedFunction(value: SerializedValue): value is SerializedFunction {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedFunction).__type === 'function' &&
    '__fnId' in value
  );
}

/**
 * Check whether the value is a serialized date
 */
export function isSerializedDate(value: SerializedValue): value is SerializedDate {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedDate).__type === 'date'
  );
}

/**
 * Check whether the value is a serialized regexp
 */
export function isSerializedRegExp(value: SerializedValue): value is SerializedRegExp {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedRegExp).__type === 'regexp'
  );
}

/**
 * Check whether the value is a serialized error
 */
export function isSerializedError(value: SerializedValue): value is SerializedError {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedError).__type === 'error'
  );
}

/**
 * Check whether the value is a serialized Map
 */
export function isSerializedMap(value: SerializedValue): value is SerializedMap {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedMap).__type === 'map'
  );
}

/**
 * Check whether the value is a serialized Set
 */
export function isSerializedSet(value: SerializedValue): value is SerializedSet {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedSet).__type === 'set'
  );
}

/**
 * Check whether the value is a serialized Promise
 */
export function isSerializedPromise(value: SerializedValue): value is SerializedPromise {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedPromise).__type === 'promise' &&
    '__promiseId' in value
  );
}

/**
 * Check whether the value is any serialized special type
 */
export function isSerializedSpecialType(value: SerializedValue): value is SerializedSpecialType {
  return (
    isSerializedFunction(value) ||
    isSerializedDate(value) ||
    isSerializedRegExp(value) ||
    isSerializedError(value) ||
    isSerializedMap(value) ||
    isSerializedSet(value) ||
    isSerializedPromise(value)
  );
}
