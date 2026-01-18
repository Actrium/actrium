/**
 * @rill/bridge - Serialization Utilities
 *
 * 通用序列化工具，供 Host 和 Guest 共享使用
 * - Host: Bridge.ts 使用
 * - Guest: reconciler 使用（打包注入到沙箱）
 */

import type { TypeRule, TypeRuleContext } from './TypeRules';

/** 循环引用标记 */
export interface CircularRef {
  __type: 'circular';
}

/**
 * 创建编码函数
 * 使用 TypeRules 遍历匹配并编码值
 * 自动检测循环引用，避免无限递归
 *
 * 性能优化：
 * - 原始类型快速路径（跳过规则遍历）
 * - 复用 WeakSet 避免重复分配
 * - 按类型预分组规则减少匹配次数
 */
export function createEncoder(
  typeRules: TypeRule[],
  context: TypeRuleContext
): (value: unknown) => unknown {
  // 预分组规则：按类型快速分派
  const functionRules = typeRules.filter((r) => r.name === 'function');
  const promiseRules = typeRules.filter((r) => r.name === 'promise');
  const dateRules = typeRules.filter((r) => r.name === 'date');
  // 排除需要递归处理的规则（array、object）- 由本模块带循环引用检测的逻辑处理
  const objectRules = typeRules.filter(
    (r) =>
      r.name !== 'function' &&
      r.name !== 'promise' &&
      r.name !== 'date' &&
      r.name !== 'null-undefined' &&
      r.name !== 'primitives' &&
      r.name !== 'array' &&
      r.name !== 'object'
  );

  // 复用 WeakSet 检测循环引用
  const seenPool = new WeakSet<object>();

  const encode = (value: unknown): unknown => {
    // 快速路径：null/undefined 直接返回
    if (value === null || value === undefined) {
      return value;
    }

    // 快速路径：原始类型直接返回（跳过规则遍历）
    const type = typeof value;
    if (type === 'boolean' || type === 'number' || type === 'string') {
      return value;
    }

    // 函数类型：直接使用函数规则
    if (type === 'function') {
      for (const rule of functionRules) {
        if (rule.match(value)) {
          return rule.encode ? rule.encode(value, context) : value;
        }
      }
      return value;
    }

    // 对象类型
    if (type === 'object') {
      // Promise 检查（叶子对象，不需要循环引用检测）
      if (value instanceof Promise) {
        for (const rule of promiseRules) {
          if (rule.match(value)) {
            return rule.encode ? rule.encode(value, context) : value;
          }
        }
      }

      // Date 检查（叶子对象，不需要循环引用检测）
      if (value instanceof Date) {
        for (const rule of dateRules) {
          if (rule.match(value)) {
            return rule.encode ? rule.encode(value, context) : value;
          }
        }
      }

      // 其他对象规则（ArrayBuffer、TypedArray 等叶子对象）
      for (const rule of objectRules) {
        if (rule.match(value)) {
          return rule.encode ? rule.encode(value, context) : value;
        }
      }

      // 未匹配任何规则 - 需要递归遍历，进行循环引用检测
      const obj = value as object;

      if (seenPool.has(obj)) {
        return { __type: 'circular' } as CircularRef;
      }

      seenPool.add(obj);

      // 普通对象：递归编码其属性
      if (!Array.isArray(value)) {
        return encodeObject(value as Record<string, unknown>, encode);
      }

      // 数组：递归编码元素
      return (value as unknown[]).map(encode);
    }

    return value;
  };

  return encode;
}

/**
 * 编码对象的所有属性
 */
export function encodeObject(
  obj: Record<string, unknown>,
  encode: (value: unknown) => unknown
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    result[key] = encode(value);
  }
  return result;
}

/**
 * 创建解码函数
 * 使用 TypeRules 遍历匹配并解码值
 */
export function createDecoder(
  typeRules: TypeRule[],
  context: TypeRuleContext
): (value: unknown) => unknown {
  const decode = (value: unknown): unknown => {
    // 遍历类型规则，找到第一个匹配的规则
    for (const rule of typeRules) {
      if (rule.match(value)) {
        // 如果规则有 decode 方法，使用它；否则直接返回（passthrough）
        return rule.decode ? rule.decode(value, context) : value;
      }
    }

    // 未匹配任何规则 - 如果是对象，递归解码其属性
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      return decodeObject(value as Record<string, unknown>, decode);
    }

    // 其他情况直接返回
    return value;
  };

  return decode;
}

/**
 * 解码对象的所有属性
 */
export function decodeObject(
  obj: Record<string, unknown>,
  decode: (value: unknown) => unknown
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    result[key] = decode(value);
  }
  return result;
}
