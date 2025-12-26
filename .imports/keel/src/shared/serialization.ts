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
 */
export function createEncoder(
  typeRules: TypeRule[],
  context: TypeRuleContext
): (value: unknown) => unknown {
  // 每次顶层调用时的 seen 集合（用于检测循环引用）
  let seen: WeakSet<object> | null = null;
  let depth = 0;

  const encode = (value: unknown): unknown => {
    // 非对象类型直接处理
    if (value === null || typeof value !== 'object') {
      // 遍历类型规则，找到第一个匹配的规则
      for (const rule of typeRules) {
        if (rule.match(value)) {
          return rule.encode ? rule.encode(value, context) : value;
        }
      }
      return value;
    }

    // 对象类型：检测循环引用
    const obj = value as object;

    // 顶层调用时初始化 seen
    if (depth === 0) {
      seen = new WeakSet();
    }

    // 检测循环引用
    if (seen!.has(obj)) {
      return { __type: 'circular' } as CircularRef;
    }

    // 标记为已访问
    seen!.add(obj);
    depth++;

    try {
      // 遍历类型规则，找到第一个匹配的规则
      for (const rule of typeRules) {
        if (rule.match(value)) {
          return rule.encode ? rule.encode(value, context) : value;
        }
      }

      // 未匹配任何规则 - 如果是普通对象，递归编码其属性
      if (!Array.isArray(value)) {
        return encodeObject(value as Record<string, unknown>, encode);
      }

      return value;
    } finally {
      depth--;
      // 顶层调用结束时清理
      if (depth === 0) {
        seen = null;
      }
    }
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
