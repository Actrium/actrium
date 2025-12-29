/**
 * @rill/bridge - Type Rules
 *
 * 类型规则系统：定义跨边界数据的编码/解码策略
 * Host ↔ Guest 共享
 */

import { createDecoder, createEncoder } from './serialization';
import type { SerializedFunction, SerializedPromise } from './types';

export { createDecoder, createEncoder };

/**
 * Helper type for checking serialized objects with __type property
 */
type SerializedObject = { __type: string; [key: string]: unknown };

/**
 * 数据传输策略
 */
export type TransportStrategy =
  | 'passthrough' // 直接传递（JSI 安全类型）
  | 'serialize' // 序列化传递（复杂类型）
  | 'proxy'; // 代理传递（函数）

/**
 * 类型处理规则
 */
export interface TypeRule {
  /**
   * 规则名称（用于调试）
   */
  name: string;

  /**
   * 类型检测
   */
  // Reason: Type rule must accept any value to check its type
  match: (value: unknown) => boolean;

  /**
   * 编码策略（发送前）
   * @param value - 原始值
   * @param context - Bridge 上下文（提供 registry 等）
   */
  // Reason: Type rule encode/decode must handle arbitrary values
  encode?: (value: unknown, context: TypeRuleContext) => unknown;

  /**
   * 解码策略（接收后）
   * @param value - 编码后的值
   * @param context - Bridge 上下文
   */
  // Reason: Type rule decode must handle arbitrary value types
  decode?: (value: unknown, context: TypeRuleContext) => unknown;

  /**
   * 传输策略
   */
  strategy: TransportStrategy;
}

/**
 * 类型规则上下文（Bridge 提供给规则的能力）
 */
export interface TypeRuleContext {
  /**
   * 递归编码（用于嵌套结构）
   */
  // Reason: Recursive encode/decode must handle arbitrary value types
  encode: (value: unknown) => unknown;

  /**
   * 递归解码（用于嵌套结构）
   */
  // Reason: Recursive decode must handle arbitrary value types
  decode: (value: unknown) => unknown;

  /**
   * 注册函数（返回 fnId）
   */
  // biome-ignore lint/complexity/noBannedTypes: Generic function registration requires Function type
  registerFunction: (fn: Function) => string;

  /**
   * 调用函数（通过 fnId）
   */
  // Reason: Callback functions have arbitrary signatures with unknown args/return
  invokeFunction: (fnId: string, args: unknown[]) => unknown;

  /**
   * Optional logger for error reporting
   * If not provided, errors will be logged to console
   */
  logger?: {
    // Reason: Logger methods accept arbitrary console arguments
    error: (...args: unknown[]) => void;
  };

  /**
   * 注册 Promise（用于跨边界 Promise 传递）
   * Bridge 会自动监听 Promise 结算并发送消息到对端
   * @param promise - 要注册的 Promise
   * @returns promiseId
   */
  registerPromise?: (promise: Promise<unknown>) => string;

  /**
   * 创建待解析的 Promise（用于解码时创建）
   * @param promiseId - Promise ID
   * @returns 一个 Promise，当收到对端结算消息时 resolve/reject
   */
  createPendingPromise?: (promiseId: string) => Promise<unknown>;
}

/**
 * 预定义的类型规则
 */
export const DEFAULT_TYPE_RULES: TypeRule[] = [
  // 1. Null/Undefined - 直接传递
  {
    name: 'null-undefined',
    match: (v) => v === null || v === undefined,
    strategy: 'passthrough',
  },

  // 2. Primitives - 直接传递
  {
    name: 'primitives',
    match: (v) => typeof v === 'boolean' || typeof v === 'number' || typeof v === 'string',
    strategy: 'passthrough',
  },

  // 2.5. Circular Reference - 解码为 undefined（数据已丢失）
  {
    name: 'circular',
    match: (v) =>
      typeof v === 'object' &&
      v !== null &&
      '__type' in v &&
      (v as { __type: string }).__type === 'circular',
    decode: () => undefined, // 循环引用无法恢复，返回 undefined
    strategy: 'serialize',
  },

  // 3. Serialized Function - 解码为 proxy
  {
    name: 'serialized-function',
    match: (v) =>
      typeof v === 'object' &&
      v !== null &&
      '__type' in v &&
      (v as SerializedFunction).__type === 'function' &&
      '__fnId' in v,
    decode: (v, ctx) => {
      const { __fnId, __source } = v as SerializedFunction;
      // Reason: Deserialized function proxy accepts arbitrary arguments
      const proxy = (...args: unknown[]) => {
        try {
          // IMPORTANT: Encode args before crossing JSI boundary to Guest sandbox
          // This handles complex objects like GestureResponderEvent which may contain:
          // - Functions (preventDefault, stopPropagation) → { __type: 'function', __fnId }
          // - Native object references → recursively encoded to plain objects
          // - Circular references → { __type: 'circular' }
          // Without encoding, JSI setGlobal would crash when passing these types
          const encodedArgs = args.map((arg) => ctx.encode(arg));
          const result = ctx.invokeFunction(__fnId, encodedArgs);
          // Async errors propagate naturally as Promise rejections
          return result;
        } catch (err) {
          // Always log to console for visibility (even if ctx.logger not provided)
          console.error(`[rill:TypeRules] ❌ Callback ${__fnId} threw sync error:`, err);
          // Also use provided logger if available
          if (ctx.logger?.error) {
            ctx.logger.error(`[TypeRules] Callback ${__fnId} threw sync error:`, err);
          }
          // In debug mode, re-throw to help identify the issue quickly
          // Check for global debug flag (set by Host or tests)
          if (
            typeof globalThis !== 'undefined' &&
            (globalThis as { __RILL_DEBUG__?: boolean }).__RILL_DEBUG__
          ) {
            throw err;
          }
          // In production, return undefined to prevent Host crashes
          // but the error is still visible in console
          return undefined;
        }
      };
      // Attach source for DevTools inspection
      if (__source) {
        (proxy as { __source?: string }).__source = __source;
      }
      return proxy;
    },
    strategy: 'proxy',
  },

  // 4. Functions - 编码为 fnId
  {
    name: 'function',
    match: (v) => typeof v === 'function',
    encode: (fn, ctx) => {
      // biome-ignore lint/complexity/noBannedTypes: fn is verified as function by match()
      const fnId = ctx.registerFunction(fn as Function);
      // Capture function source for DevTools (truncate if too long)
      let source: string | undefined;
      try {
        const fullSource = (fn as Function).toString();
        // Truncate long sources to avoid performance issues
        source = fullSource.length > 500 ? fullSource.slice(0, 500) + '...' : fullSource;
      } catch {
        // Some functions may not support toString()
      }
      return { __type: 'function', __fnId: fnId, __source: source } as SerializedFunction;
    },
    strategy: 'proxy',
  },

  // 4.5. Serialized Promise - 解码为 pending Promise
  {
    name: 'serialized-promise',
    match: (v) =>
      typeof v === 'object' &&
      v !== null &&
      '__type' in v &&
      (v as SerializedPromise).__type === 'promise' &&
      '__promiseId' in v,
    decode: (v, ctx) => {
      const { __promiseId } = v as SerializedPromise;
      // 如果 Bridge 提供了 createPendingPromise，使用它
      if (ctx.createPendingPromise) {
        return ctx.createPendingPromise(__promiseId);
      }
      // 否则返回一个永不 resolve 的 Promise（降级处理）
      console.warn('[TypeRules] Promise decoding not supported, createPendingPromise not provided');
      return new Promise(() => {});
    },
    strategy: 'proxy',
  },

  // 4.6. Promise - 编码为 promiseId
  {
    name: 'promise',
    match: (v) => v instanceof Promise,
    encode: (promise, ctx) => {
      // 如果 Bridge 提供了 registerPromise，使用它
      if (ctx.registerPromise) {
        const promiseId = ctx.registerPromise(promise as Promise<unknown>);
        return { __type: 'promise', __promiseId: promiseId } as SerializedPromise;
      }
      // 否则标记为 unsupported（降级处理）
      console.warn('[TypeRules] Promise encoding not supported, registerPromise not provided');
      return { __type: 'unsupported', __originalType: 'Promise' } as unknown;
    },
    strategy: 'proxy',
  },

  // 5. Date - 序列化传递
  {
    name: 'date',
    match: (v) =>
      v instanceof Date ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'date'),
    encode: (date) => {
      // Already serialized by Guest?
      if (
        typeof date === 'object' &&
        date !== null &&
        '__type' in date &&
        (date as SerializedObject).__type === 'date'
      ) {
        return date;
      }
      return {
        __type: 'date',
        __value: (date as Date).toISOString(),
      };
    },
    decode: (obj) => {
      if (obj instanceof Date) return obj;
      const { __value } = obj as { __type: 'date'; __value: string };
      return new Date(__value);
    },
    strategy: 'serialize',
  },

  // 6. RegExp - 序列化传递
  {
    name: 'regexp',
    match: (v) =>
      v instanceof RegExp ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'regexp'),
    encode: (regex) => {
      // Already serialized by Guest?
      if (
        typeof regex === 'object' &&
        regex !== null &&
        '__type' in regex &&
        (regex as SerializedObject).__type === 'regexp'
      ) {
        return regex;
      }
      return {
        __type: 'regexp',
        __source: (regex as RegExp).source,
        __flags: (regex as RegExp).flags,
      };
    },
    decode: (obj) => {
      if (obj instanceof RegExp) return obj;
      const { __source, __flags } = obj as {
        __type: 'regexp';
        __source: string;
        __flags: string;
      };
      return new RegExp(__source, __flags);
    },
    strategy: 'serialize',
  },

  // 7. Error - 序列化传递
  {
    name: 'error',
    match: (v) =>
      v instanceof Error ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'error'),
    encode: (error) => {
      // Already serialized by Guest?
      if (
        typeof error === 'object' &&
        error !== null &&
        '__type' in error &&
        (error as SerializedObject).__type === 'error'
      ) {
        return error;
      }
      return {
        __type: 'error',
        __name: (error as Error).name,
        __message: (error as Error).message,
        __stack: (error as Error).stack,
      };
    },
    decode: (obj) => {
      if (obj instanceof Error) return obj;
      const { __name, __message, __stack } = obj as {
        __type: 'error';
        __name: string;
        __message: string;
        __stack?: string;
      };
      const error = new Error(__message);
      error.name = __name;
      if (__stack) {
        error.stack = __stack;
      }
      return error;
    },
    strategy: 'serialize',
  },

  // 8. Map - 编码/解码
  {
    name: 'map',
    match: (v) =>
      v instanceof Map ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'map'),
    encode: (map, ctx) => {
      // Already serialized by Guest?
      if (!(map instanceof Map)) {
        return map;
      }
      const entries: [unknown, unknown][] = [];
      for (const [k, v] of map.entries()) {
        entries.push([ctx.encode(k), ctx.encode(v)]);
      }
      return { __type: 'map', __entries: entries };
    },
    decode: (obj, ctx) => {
      if (obj instanceof Map) return obj;
      const { __entries } = obj as { __type: 'map'; __entries: [unknown, unknown][] };
      const map = new Map();
      for (const [k, v] of __entries) {
        map.set(ctx.decode(k), ctx.decode(v));
      }
      return map;
    },
    strategy: 'serialize',
  },

  // 9. Set - 编码/解码
  {
    name: 'set',
    match: (v) =>
      v instanceof Set ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'set'),
    encode: (set, ctx) => {
      // Already serialized by Guest?
      if (!(set instanceof Set)) {
        return set;
      }
      const values: unknown[] = [];
      for (const v of set.values()) {
        values.push(ctx.encode(v));
      }
      return { __type: 'set', __values: values };
    },
    decode: (obj, ctx) => {
      if (obj instanceof Set) return obj;
      const { __values } = obj as { __type: 'set'; __values: unknown[] };
      const set = new Set();
      for (const v of __values) {
        set.add(ctx.decode(v));
      }
      return set;
    },
    strategy: 'serialize',
  },

  // 10. TypedArray - 序列化传递 (must be before ArrayBuffer)
  {
    name: 'typedarray',
    match: (v) =>
      (ArrayBuffer.isView(v) && !(v instanceof DataView)) ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'typedarray'),
    encode: (view) => {
      if (!ArrayBuffer.isView(view) || view instanceof DataView) {
        // Already serialized
        return view;
      }
      const typedArray = view as
        | Int8Array
        | Uint8Array
        | Uint8ClampedArray
        | Int16Array
        | Uint16Array
        | Int32Array
        | Uint32Array
        | Float32Array
        | Float64Array
        | BigInt64Array
        | BigUint64Array;

      // Get constructor name for reconstruction
      const ctorName = typedArray.constructor.name;

      // Handle BigInt arrays specially (can't use Array.from directly)
      if (typedArray instanceof BigInt64Array || typedArray instanceof BigUint64Array) {
        const data: string[] = [];
        for (let i = 0; i < typedArray.length; i++) {
          data.push(typedArray[i]!.toString());
        }
        return {
          __type: 'typedarray',
          __ctor: ctorName,
          __data: data,
          __bigint: true,
        };
      }

      return {
        __type: 'typedarray',
        __ctor: ctorName,
        __data: Array.from(typedArray as Uint8Array), // Works for all non-BigInt typed arrays
      };
    },
    decode: (obj) => {
      if (ArrayBuffer.isView(obj) && !(obj instanceof DataView)) {
        return obj;
      }
      const { __ctor, __data, __bigint } = obj as {
        __type: 'typedarray';
        __ctor: string;
        __data: number[] | string[];
        __bigint?: boolean;
      };

      // Map constructor name to actual constructor
      const constructors: Record<
        string,
        new (
          data: ArrayLike<number> | ArrayLike<bigint>
        ) => ArrayBufferView
      > = {
        Int8Array: Int8Array,
        Uint8Array: Uint8Array,
        Uint8ClampedArray: Uint8ClampedArray,
        Int16Array: Int16Array,
        Uint16Array: Uint16Array,
        Int32Array: Int32Array,
        Uint32Array: Uint32Array,
        Float32Array: Float32Array,
        Float64Array: Float64Array,
        BigInt64Array: BigInt64Array as unknown as new (
          data: ArrayLike<number> | ArrayLike<bigint>
        ) => ArrayBufferView,
        BigUint64Array: BigUint64Array as unknown as new (
          data: ArrayLike<number> | ArrayLike<bigint>
        ) => ArrayBufferView,
      };

      const Ctor = constructors[__ctor];
      if (!Ctor) {
        console.warn(`[TypeRules] Unknown TypedArray constructor: ${__ctor}`);
        return new Uint8Array(__data as number[]);
      }

      // Handle BigInt arrays
      if (__bigint) {
        const bigIntData = (__data as string[]).map((s) => BigInt(s));
        return new (Ctor as unknown as new (data: bigint[]) => BigInt64Array | BigUint64Array)(
          bigIntData
        );
      }

      return new (Ctor as new (data: number[]) => ArrayBufferView)(__data as number[]);
    },
    strategy: 'serialize',
  },

  // 11. ArrayBuffer - 序列化传递
  {
    name: 'arraybuffer',
    match: (v) =>
      v instanceof ArrayBuffer ||
      (typeof v === 'object' &&
        v !== null &&
        '__type' in v &&
        (v as SerializedObject).__type === 'arraybuffer'),
    encode: (buffer) => {
      if (!(buffer instanceof ArrayBuffer)) {
        // Already serialized
        return buffer;
      }
      const bytes = new Uint8Array(buffer);
      return {
        __type: 'arraybuffer',
        __data: Array.from(bytes),
      };
    },
    decode: (obj) => {
      if (obj instanceof ArrayBuffer) {
        return obj;
      }
      const { __data } = obj as { __type: 'arraybuffer'; __data: number[] };
      const bytes = new Uint8Array(__data);
      return bytes.buffer;
    },
    strategy: 'serialize',
  },

  // 12. Arrays - 递归处理
  {
    name: 'array',
    match: (v) => Array.isArray(v),
    encode: (arr, ctx) => (arr as unknown[]).map((item) => ctx.encode(item)),
    decode: (arr, ctx) => (arr as unknown[]).map((item) => ctx.decode(item)),
    strategy: 'serialize',
  },

  // 13. toJSON - 支持自定义序列化
  // 允许类实例通过 toJSON() 方法控制序列化行为
  // 例如: class User { toJSON() { return { __class: 'User', name: this.name }; } }
  {
    name: 'toJSON',
    match: (v) =>
      typeof v === 'object' &&
      v !== null &&
      !Array.isArray(v) &&
      typeof (v as { toJSON?: unknown }).toJSON === 'function' &&
      // 排除内置类型（Date, RegExp 等已有专门规则）
      !(v instanceof Date) &&
      !(v instanceof RegExp) &&
      !(v instanceof Error) &&
      !(v instanceof Map) &&
      !(v instanceof Set),
    encode: (obj, ctx) => {
      // 调用对象的 toJSON 方法，然后递归编码结果
      const serialized = (obj as { toJSON: () => unknown }).toJSON();
      return ctx.encode(serialized);
    },
    // decode 不需要特殊处理，toJSON 的结果会被正常解码
    strategy: 'serialize',
  },

  // 14. Objects - 递归处理
  {
    name: 'object',
    match: (v) => typeof v === 'object' && v !== null,
    encode: (obj, ctx) => {
      // 检测特殊序列化类型（已处理的，直接返回）
      if (typeof obj === 'object' && obj !== null && '__type' in obj) {
        const typed = obj as { __type: string };
        const specialTypes = [
          'date',
          'regexp',
          'error',
          'function',
          'circular',
          'arraybuffer',
          'typedarray',
        ];
        if (specialTypes.includes(typed.__type)) {
          return obj;
        }
      }

      // 普通对象递归处理
      const result: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
        result[key] = ctx.encode(value);
      }
      return result;
    },
    decode: (obj, ctx) => {
      // 检测特殊序列化类型（不处理，让其他规则处理）
      if (typeof obj === 'object' && obj !== null && '__type' in obj) {
        // 不处理，返回原值让其他规则匹配
        return obj;
      }

      // 普通对象递归解码
      const result: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
        result[key] = ctx.decode(value);
      }
      return result;
    },
    strategy: 'serialize',
  },
];
