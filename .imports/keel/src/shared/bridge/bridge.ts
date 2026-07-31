/**
 * @keel/shared/bridge - Communication Layer
 *
 * Bridge is the communication layer between Host and Guest
 * - exposes exactly two methods: sendToHost() / sendToGuest()
 * - handles all encoding/decoding internally, automatically
 * - leans on JSI as far as it goes: natively supported types pass straight through, zero overhead
 */

import type {
  BridgeValue,
  BridgeValueObject,
  CallbackRegistry,
  CodecCallbacks,
  HostMessage,
  Operation,
  OperationBatch,
  ReviewedUnknown,
  SerializedError,
  SerializedHostMessage,
  SerializedOperation,
  SerializedOperationBatch,
  SerializedValue,
  SerializedValueObject,
  TypeRule,
} from '..';

import {
  createDecoder,
  createEncoder,
  DEFAULT_TYPE_RULES,
  HostMsg,
  isSerializedFunction,
  operationHasProps,
  decodeObject as sharedDecodeObject,
  encodeObject as sharedEncodeObject,
} from '..';
// Imported from './types' directly (not '..') to avoid a circular-init TDZ:
// shared/index.ts re-exports this module before types constants initialize.
import { OPERATION_TYPES } from '../types';
import { PromiseManager, type PromiseSettleResult } from './promise-manager';

// Runtime membership set for the OperationType union (see shared/types.ts).
const VALID_OPERATION_TYPES: ReadonlySet<string> = new Set(OPERATION_TYPES);

/**
 * Bridge configuration options
 */
export interface BridgeOptions {
  /**
   * Called when Guest sends an operation batch (already decoded)
   */
  onGuestOperations: (batch: OperationBatch) => void;

  /**
   * Called with the SERIALIZED batch (callbacks as `{__fnId}` markers, not live functions)
   * before it is decoded. When set, the Bridge forwards the serialized batch here and SKIPS
   * local decode + {@link onGuestOperations} entirely.
   *
   * This is the seam the off-main-thread worker host uses: the worker holds the sandbox but
   * has no renderer, so it ships the structured-clone-safe serialized batch across the worker
   * boundary; the main thread decodes it (turning `{__fnId}` back into proxies that invoke the
   * guest callback over postMessage). Leave unset for in-thread engines.
   */
  onSerializedBatch?: (batch: SerializedOperationBatch) => void;

  /**
   * Called when Host sends a message to Guest (already decoded)
   */
  onHostMessage: (message: HostMessage) => void | Promise<void>;

  /**
   * Callback Registry
   */
  callbackRegistry: CallbackRegistry;

  /**
   * Guest callback invoker - invokes callbacks registered in the Sandbox
   * When provided, the bridge routes those calls to the guest automatically
   */
  guestInvoker?: (fnId: string, args: ReviewedUnknown[]) => ReviewedUnknown;

  /**
   * Guest callback releaser - releases callbacks registered in the Sandbox
   * When provided, the bridge routes release calls to the guest
   */
  guestReleaseCallback?: (fnId: string) => void;

  /**
   * Type rules (optional; DEFAULT_TYPE_RULES when omitted)
   */
  typeRules?: TypeRule[];

  /**
   * Promise timeout in milliseconds, default 30000 (30 seconds); 0 disables it
   */
  promiseTimeout?: number;

  /**
   * Debug mode
   */
  debug?: boolean;

  /**
   * Optional logger for error reporting
   * If not provided, errors will be logged to console
   */
  logger?: {
    // Reason: Logger methods accept arbitrary console arguments
    error: (...args: unknown[]) => void;
  };
}

/**
 * Result of encoding a batch with callback tracking
 */
export interface EncodeBatchResult {
  serialized: SerializedOperationBatch;
  fnIds: Set<string>;
}

/**
 * Bridge - unified communication layer
 *
 * Two methods only; all encoding and decoding happens inside
 */
export class Bridge {
  private onGuestOperations: (batch: OperationBatch) => void;
  private onSerializedBatch?: (batch: SerializedOperationBatch) => void;
  private onHostMessage: (message: HostMessage) => void | Promise<void>;
  private registry: CallbackRegistry;
  private guestInvoker?: (fnId: string, args: ReviewedUnknown[]) => ReviewedUnknown;
  private guestReleaseCallback?: (fnId: string) => void;
  private typeRules: TypeRule[];
  private debug: boolean;

  // Track function IDs during encoding (for cleanup)
  private currentEncodingFnIds: Set<string> | null = null;

  // Promise handling - delegated to PromiseManager
  private promiseManager: PromiseManager;

  // Type rule context (capabilities exposed to the rules)
  private readonly context: CodecCallbacks;

  // Encoder/Decoder functions (created using shared utilities)
  private readonly encoder: (value: ReviewedUnknown) => ReviewedUnknown;
  private readonly decoder: (value: ReviewedUnknown) => ReviewedUnknown;

  constructor(options: BridgeOptions) {
    this.onGuestOperations = options.onGuestOperations;
    this.onSerializedBatch = options.onSerializedBatch;
    this.onHostMessage = options.onHostMessage;
    this.registry = options.callbackRegistry;
    this.guestInvoker = options.guestInvoker;
    this.guestReleaseCallback = options.guestReleaseCallback;
    this.typeRules = options.typeRules ?? DEFAULT_TYPE_RULES;
    this.debug = options.debug ?? false;

    // Initialize PromiseManager
    this.promiseManager = new PromiseManager({
      timeout: options.promiseTimeout ?? 30000,
      onSendResult: (promiseId, result) => this.sendPromiseResult(promiseId, result),
      debug: this.debug,
    });

    // Declare the type rule context first: the encoder and decoder both close over it
    // Reason: Context must reference encoder/decoder which reference context (circular)
    this.context = {} as CodecCallbacks;

    // Create the encoder and decoder using shared utilities
    this.encoder = createEncoder(this.typeRules, this.context);
    this.decoder = createDecoder(this.typeRules, this.context);

    // Finish filling in the context now that the encoder and decoder exist
    this.context.encode = this.encoder;
    this.context.decode = this.decoder;
    this.context.logger = options.logger; // Pass logger to context for TypeRules error reporting
    this.context.registerFunction = (fn) => {
      const fnId = this.registry.register(fn as (...args: unknown[]) => unknown);
      // Track if encoding
      if (this.currentEncodingFnIds) {
        this.currentEncodingFnIds.add(fnId);
      }
      return fnId;
    };
    // invokeFunction: routes smartly to Guest or Host registry
    this.context.invokeFunction = (fnId, args) => {
      // Host registry first (callbacks registered on the host side)
      if (this.registry.has(fnId)) {
        return this.registry.invoke(fnId, args);
      }
      // Not in Host registry -> route to Guest sandbox
      // Covers both fn_N (sandbox __keel.registerCallback) and fn_xxx_N (Guest globalCallbackRegistry)
      // NOTE: args encoding is handled by TypeRules (serialized-function decode)
      if (this.guestInvoker) {
        return this.guestInvoker(fnId, args);
      }
      // Cannot route
      if (this.debug) {
        console.warn(
          `[Bridge] invokeFunction: fnId ${fnId} not found in Host registry and no guestInvoker`
        );
      }
      return undefined;
    };
    // Promise handling - delegated to PromiseManager
    this.context.registerPromise = (promise) => this.promiseManager.register(promise);
    this.context.createPendingPromise = (promiseId) => this.promiseManager.createPending(promiseId);
  }

  // ============================================
  // Public API — Guest → Host (3 explicit entry points)
  // ============================================

  /**
   * Send a raw (unencoded) operation batch from Guest to Host.
   * Used by VM providers where Guest and Host share the same runtime.
   * The batch will be encoded (Function → fnId) then decoded (fnId → proxy).
   */
  sendRawBatch(batch: OperationBatch): void {
    if (this.debug) {
      console.log('[Bridge] sendRawBatch input:', batch);
    }
    Bridge.assertRawBatchShape(batch);
    this.dispatchSerializedBatch(this.encodeBatch(batch).serialized);
  }

  /**
   * Guard sendRawBatch against structurally-broken input so failures surface
   * as an attributable error instead of a TypeError deep in encodeBatch. The
   * signature case is a guest-realm ArrayBuffer that a sandbox boundary
   * converter (JSI / QuickJS JSON bridge) degraded into a plain empty object —
   * the batch bytes are already lost by then, and only the boundary can be
   * fixed, so name it explicitly.
   */
  private static assertRawBatchShape(batch: OperationBatch): void {
    if (typeof batch !== 'object' || batch === null || Array.isArray(batch)) {
      throw new Error(
        `[Bridge] sendRawBatch: expected an OperationBatch object, got ${batch === null ? 'null' : Array.isArray(batch) ? 'array' : typeof batch}`
      );
    }
    // Reason: probing an untrusted object's field before it is proven to be an array
    if (!Array.isArray((batch as { operations?: unknown }).operations)) {
      const keyCount = Object.keys(batch).length;
      const hint =
        keyCount === 0
          ? ' — an empty object here usually means the guest sent a value the sandbox boundary converter cannot carry (e.g. a binary buffer degraded to {}); the guest must send JSON'
          : '';
      throw new Error(
        // Reason: reporting the same unproven field's typeof in the error message
        `[Bridge] sendRawBatch: batch.operations must be an array, got ${typeof (batch as { operations?: unknown }).operations}${hint}`
      );
    }
  }

  /**
   * Send a pre-serialized operation batch from Guest to Host.
   * Used by WASM/Worker providers where Guest already serialized (Function → fnId).
   * Skips encoding — only decodes (fnId → proxy).
   */
  sendSerializedBatch(batch: SerializedOperationBatch): void {
    if (this.debug) {
      console.log('[Bridge] sendSerializedBatch input:', batch.operations.length, 'operations');
    }
    this.dispatchSerializedBatch(batch);
  }

  /**
   * Send a JSON-stringified operation batch from Guest to Host.
   * Used on Android where JSI object conversion overflows native stack
   * for large batches — Guest JSON.stringify's before crossing the bridge.
   */
  sendJsonBatch(json: string): void {
    if (this.debug) {
      console.log('[Bridge] sendJsonBatch input:', `JSON string(${json.length})`);
    }
    const serializedBatch = Bridge.assertSerializedBatchShape(JSON.parse(json));
    if (this.debug) {
      console.log('[Bridge] Decoded JSON batch:', serializedBatch.operations.length, 'operations');
    }
    this.dispatchSerializedBatch(serializedBatch);
  }

  /**
   * Cheap structural validation for a JSON-decoded batch arriving across the
   * guest -> host trust boundary. Deliberately not a full schema check — it
   * only guards the invariants the dispatch path relies on (batch is an
   * object, `operations` is an array, every entry is an object with a known
   * `op` discriminant). Anything else throws with enough context to trace
   * the offending guest payload.
   */
  // Reason: JSON.parse output from the guest is untrusted until this check runs
  private static assertSerializedBatchShape(value: unknown): SerializedOperationBatch {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      throw new Error(
        `[Bridge] sendJsonBatch: expected a batch object, got ${value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value}`
      );
    }
    // Reason: probing an untrusted object's field before it is proven to be an array
    const operations = (value as { operations?: unknown }).operations;
    if (!Array.isArray(operations)) {
      throw new Error(
        `[Bridge] sendJsonBatch: batch.operations must be an array, got ${typeof operations}`
      );
    }
    for (let i = 0; i < operations.length; i++) {
      const op = operations[i];
      if (typeof op !== 'object' || op === null) {
        throw new Error(
          `[Bridge] sendJsonBatch: operations[${i}] must be an object, got ${op === null ? 'null' : typeof op}`
        );
      }
      // Reason: probing an untrusted operation's discriminant before validation
      const opType = (op as { op?: unknown }).op;
      if (typeof opType !== 'string' || !VALID_OPERATION_TYPES.has(opType)) {
        throw new Error(
          `[Bridge] sendJsonBatch: operations[${i}].op is not a known operation type (got ${JSON.stringify(opType)})`
        );
      }
    }
    return value as SerializedOperationBatch;
  }

  /**
   * Common dispatch path: decode serialized batch, extract fnIds, and deliver.
   */
  private dispatchSerializedBatch(serializedBatch: SerializedOperationBatch): void {
    // Off-main-thread worker host: forward the structured-clone-safe serialized batch and skip
    // local decode. The worker has no renderer, and its callback proxies could not cross the
    // worker boundary anyway — the main thread decodes and wires callbacks back over postMessage.
    if (this.onSerializedBatch) {
      this.onSerializedBatch(serializedBatch);
      return;
    }

    // Extract fnIds from each operation's props for cleanup tracking
    const operationFnIds = new Map<number, Set<string>>();
    for (const op of serializedBatch.operations) {
      if (operationHasProps(op)) {
        const fnIds = Bridge.extractFnIds(op.props);
        if (fnIds.size > 0) {
          operationFnIds.set(op.id, fnIds);
        }
      }
    }

    if (this.debug) {
      console.log('[Bridge] dispatch serialized:', serializedBatch);
      console.log('[Bridge] dispatch fnIds:', operationFnIds);
    }

    // Decode: fnId → Function proxy
    const decoded = this.decodeBatch(serializedBatch);

    // Attach fnIds metadata to operations for cleanup
    for (const op of decoded.operations) {
      const fnIds = operationFnIds.get(op.id);
      if (fnIds) {
        (op as Operation & { _fnIds?: Set<string> })._fnIds = fnIds;
      }
    }

    if (this.debug) {
      console.log('[Bridge] dispatch decoded:', decoded);
    }

    this.onGuestOperations(decoded);
  }

  /**
   * Host → Guest
   * Send a host message; encoding happens inside
   */
  async sendToGuest(message: HostMessage): Promise<void> {
    if (this.debug) {
      console.log('[Bridge] sendToGuest input:', message);
    }

    // 1. Encode
    const encoded = this.encodeHostMessage(message);

    if (this.debug) {
      console.log('[Bridge] sendToGuest encoded:', encoded);
    }

    // 2. Cross the JSI boundary

    // 3. Decode and hand off to the receiver
    const decoded = this.decodeHostMessage(encoded);

    if (this.debug) {
      console.log('[Bridge] sendToGuest decoded:', decoded);
    }

    await this.onHostMessage(decoded);
  }

  // ============================================
  // Internals - encode (BridgeValue -> SerializedValue)
  // ============================================

  /**
   * Encode an operation batch.
   * Tracks all function IDs registered during encoding and returns them
   * alongside the serialized batch (for callback cleanup).
   */
  encodeBatch(batch: OperationBatch): EncodeBatchResult {
    this.currentEncodingFnIds = new Set<string>();

    try {
      const serialized: SerializedOperationBatch = {
        version: batch.version,
        batchId: batch.batchId,
        operations: batch.operations.map((op): SerializedOperation => {
          if (operationHasProps(op)) {
            return {
              ...op,
              props: this.encodeObject(op.props),
            };
          }
          // Handle REF_CALL - encode args
          if (op.op === 'REF_CALL') {
            return {
              ...op,
              args: op.args.map((arg) => this.encode(arg)),
            };
          }
          return op;
        }),
      };

      return { serialized, fnIds: this.currentEncodingFnIds };
    } finally {
      this.currentEncodingFnIds = null;
    }
  }

  /**
   * Encode a Host message
   */
  private encodeHostMessage(message: HostMessage): SerializedHostMessage {
    switch (message.type) {
      case HostMsg.CALL_FUNCTION:
        return {
          type: HostMsg.CALL_FUNCTION,
          fnId: message.fnId,
          args: message.args.map((arg) => this.encode(arg)),
        };
      case HostMsg.HOST_EVENT:
        return {
          type: HostMsg.HOST_EVENT,
          eventName: message.eventName,
          payload: this.encode(message.payload),
        };
      case HostMsg.CONFIG_UPDATE:
        return {
          type: HostMsg.CONFIG_UPDATE,
          config: this.encodeObject(message.config),
        };
      case HostMsg.DESTROY:
        return { type: HostMsg.DESTROY };
      case HostMsg.PROMISE_RESOLVE:
        return {
          type: HostMsg.PROMISE_RESOLVE,
          promiseId: message.promiseId,
          value: this.encode(message.value),
        };
      case HostMsg.PROMISE_REJECT:
        return {
          type: HostMsg.PROMISE_REJECT,
          promiseId: message.promiseId,
          error: message.error,
        };
      case HostMsg.REF_METHOD_RESULT:
        return {
          type: HostMsg.REF_METHOD_RESULT,
          refId: message.refId,
          callId: message.callId,
          result: message.result !== undefined ? this.encode(message.result) : undefined,
          error: message.error,
        };
    }
  }

  /**
   * Encode an arbitrary BridgeValue - dispatches automatically via type rules
   */
  private encode(value: BridgeValue): SerializedValue {
    return this.encoder(value) as SerializedValue;
  }

  /**
   * Encode an object's properties, recursing through the shared encoder
   */
  private encodeObject(obj: BridgeValueObject): SerializedValueObject {
    return sharedEncodeObject(obj, this.encoder) as SerializedValueObject;
  }

  // ============================================
  // Internals - decode (SerializedValue -> BridgeValue)
  // ============================================

  /**
   * Decode an operation batch
   */
  private decodeBatch(batch: SerializedOperationBatch): OperationBatch {
    return {
      version: batch.version,
      batchId: batch.batchId,
      operations: batch.operations.map((op): Operation => {
        if (operationHasProps(op)) {
          return {
            ...op,
            props: this.decodeObject(op.props),
          };
        }
        // Handle REF_CALL - decode args
        if (op.op === 'REF_CALL') {
          return {
            ...op,
            args: op.args.map((arg) => this.decode(arg)),
          };
        }
        return op;
      }),
    };
  }

  /**
   * Decode a Host message
   */
  private decodeHostMessage(message: SerializedHostMessage): HostMessage {
    switch (message.type) {
      case HostMsg.CALL_FUNCTION:
        return {
          type: HostMsg.CALL_FUNCTION,
          fnId: message.fnId,
          args: message.args.map((arg) => this.decode(arg)),
        };
      case HostMsg.HOST_EVENT:
        return {
          type: HostMsg.HOST_EVENT,
          eventName: message.eventName,
          payload: this.decode(message.payload),
        };
      case HostMsg.CONFIG_UPDATE:
        return {
          type: HostMsg.CONFIG_UPDATE,
          config: this.decodeObject(message.config),
        };
      case HostMsg.DESTROY:
        return { type: HostMsg.DESTROY };
      case HostMsg.PROMISE_RESOLVE:
        // Settle the promise
        this.settlePromise(message.promiseId, {
          status: 'fulfilled',
          value: this.decode(message.value),
        });
        return {
          type: HostMsg.PROMISE_RESOLVE,
          promiseId: message.promiseId,
          value: this.decode(message.value),
        };
      case HostMsg.PROMISE_REJECT: {
        // Handle the promise rejection
        const error = new Error(message.error.__message);
        error.name = message.error.__name;
        if (message.error.__stack) {
          error.stack = message.error.__stack;
        }
        this.settlePromise(message.promiseId, { status: 'rejected', reason: error });
        return {
          type: HostMsg.PROMISE_REJECT,
          promiseId: message.promiseId,
          error: message.error,
        };
      }
      case HostMsg.REF_METHOD_RESULT:
        return {
          type: HostMsg.REF_METHOD_RESULT,
          refId: message.refId,
          callId: message.callId,
          result: message.result !== undefined ? this.decode(message.result) : undefined,
          error: message.error,
        };
    }
  }

  /**
   * Decode an arbitrary SerializedValue - dispatches automatically via type rules
   */
  private decode(value: SerializedValue): BridgeValue {
    return this.decoder(value) as BridgeValue;
  }

  /**
   * Decode an object's properties, recursing through the shared decoder
   */
  private decodeObject(obj: SerializedValueObject): BridgeValueObject {
    return sharedDecodeObject(obj, this.decoder) as BridgeValueObject;
  }

  // ============================================
  // Promise Methods
  // ============================================

  /**
   * Send the promise settlement result to the peer
   */
  private sendPromiseResult(promiseId: string, result: PromiseSettleResult): void {
    // Fire-and-forget, but MUST handle rejection to avoid unhandled Promise rejection.
    const safeSend = (p: Promise<void>) => {
      p.catch((e) => {
        if (this.debug) {
          console.warn('[Bridge] Failed to deliver promise result to guest:', e);
        }
      });
    };

    if (result.status === 'fulfilled') {
      // Send the resolve message
      safeSend(
        this.sendToGuest({
          type: HostMsg.PROMISE_RESOLVE,
          promiseId,
          value: result.value as BridgeValue,
        })
      );
    } else {
      // Send the reject message
      const error =
        result.reason instanceof Error ? result.reason : new Error(String(result.reason));
      const serializedError: SerializedError = {
        __type: 'error',
        __name: error.name,
        __message: error.message,
        __stack: error.stack,
      };
      safeSend(
        this.sendToGuest({
          type: HostMsg.PROMISE_REJECT,
          promiseId,
          error: serializedError,
        })
      );
    }
  }

  /**
   * Settle a pending promise - delegated to PromiseManager
   */
  private settlePromise(promiseId: string, result: PromiseSettleResult): void {
    this.promiseManager.settle(promiseId, result);
  }

  // ============================================
  // Callback Lifecycle Methods
  // ============================================

  /**
   * Release a callback - routes to Host registry or Guest as appropriate
   * Used by Receiver for cleanup
   */
  releaseCallback(fnId: string): void {
    // Check Host registry first
    if (this.registry.has(fnId)) {
      this.registry.release(fnId);
      return;
    }
    // Not in Host registry → route to Guest
    if (this.guestReleaseCallback) {
      this.guestReleaseCallback(fnId);
    }
  }

  // ============================================
  // Helper Methods
  // ============================================

  /**
   * Extract all function IDs from serialized props
   * Used by Receiver for callback cleanup
   */
  static extractFnIds(props: SerializedValueObject): Set<string> {
    const fnIds = new Set<string>();

    function traverse(value: SerializedValue): void {
      if (value === null || value === undefined) return;

      // Check if it's a serialized function (centralized type guard)
      if (
        typeof value === 'object' &&
        value !== null &&
        isSerializedFunction(value as SerializedValue)
      ) {
        fnIds.add((value as { __fnId: string }).__fnId);
        return;
      }

      // Recursively traverse arrays
      if (Array.isArray(value)) {
        for (const item of value) {
          traverse(item);
        }
        return;
      }

      // Recursively traverse objects
      if (typeof value === 'object') {
        for (const v of Object.values(value as Record<string, SerializedValue>)) {
          traverse(v);
        }
      }
    }

    traverse(props);
    return fnIds;
  }

  // ============================================
  // Lifecycle Methods
  // ============================================

  /**
   * Destroy the Bridge and clean up all resources.
   * Clears pending promises to prevent timeout errors during shutdown.
   */
  destroy(): void {
    // Clear pending promises to prevent timeout errors
    this.promiseManager.clear();

    // Clear callback registry
    this.registry.clear();
  }
}
