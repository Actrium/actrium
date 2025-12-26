/**
 * Callback Registry - Guest Side
 *
 * 管理 Guest 侧的函数引用
 * - 为函数生成唯一 ID (fnId)
 * - 使用引用计数防止内存泄漏
 * - 与 Host 注入的 __callbacks Map 共享（在 Guest 环境中）
 */

import type { CallbackRegistry as ICallbackRegistry } from './types';

// Extend globalThis to include Rill-specific properties
declare global {
  var __RILL_GUEST_ENV__: boolean | undefined;
  var __callbacks: Map<string, (...args: unknown[]) => unknown> | undefined;
  var __registerCallback: ((fn: (...args: unknown[]) => unknown) => string) | undefined;
}

export class CallbackRegistry implements ICallbackRegistry {
  private callbacks: Map<string, (...args: unknown[]) => unknown>;
  private refCounts = new Map<string, number>();
  private counter = 0;
  private instanceId = Math.random().toString(36).substring(2, 7);
  private useGuestCallbacks: boolean;

  constructor() {
    // Check if we're in Guest environment
    // Guest environment is marked by __RILL_GUEST_ENV__ flag set by Host injection
    const isGuestEnv = globalThis.__RILL_GUEST_ENV__ === true;
    const guestCallbacks = globalThis.__callbacks;

    if (isGuestEnv && guestCallbacks instanceof Map) {
      // Guest environment: share the same Map with Host-injected __callbacks
      this.callbacks = guestCallbacks;
      this.useGuestCallbacks = true;
    } else {
      // Host environment or test environment: use internal Map
      this.callbacks = new Map();
      this.useGuestCallbacks = false;
    }
  }

  /**
   * Get internal callbacks Map (for syncing with globalThis.__callbacks)
   */
  getMap(): Map<string, (...args: unknown[]) => unknown> {
    return this.callbacks;
  }

  /**
   * Register callback function with initial reference count of 1
   */
  register(fn: (...args: unknown[]) => unknown): string {
    // Use Guest's __registerCallback if available (for consistent fnId format)
    if (this.useGuestCallbacks && typeof globalThis.__registerCallback === 'function') {
      const fnId = globalThis.__registerCallback(fn);
      this.refCounts.set(fnId, 1);
      return fnId;
    }
    // Default: generate fnId with instanceId
    const fnId = `fn_${this.instanceId}_${++this.counter}`;
    this.callbacks.set(fnId, fn);
    this.refCounts.set(fnId, 1);
    return fnId;
  }

  /**
   * Increase reference count for a callback
   */
  retain(fnId: string): void {
    const count = this.refCounts.get(fnId) || 0;
    this.refCounts.set(fnId, count + 1);
  }

  /**
   * Decrease reference count and remove callback if count reaches 0
   */
  release(fnId: string): void {
    const count = (this.refCounts.get(fnId) || 1) - 1;
    if (count <= 0) {
      this.callbacks.delete(fnId);
      this.refCounts.delete(fnId);
    } else {
      this.refCounts.set(fnId, count);
    }
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
   * Check if callback function exists
   */
  has(fnId: string): boolean {
    return this.callbacks.has(fnId);
  }

  /**
   * Remove callback function (ignores reference count)
   * @deprecated Use release() instead for proper reference counting
   */
  remove(fnId: string): void {
    this.callbacks.delete(fnId);
    this.refCounts.delete(fnId);
  }

  /**
   * Remove multiple callback functions (ignores reference count)
   * @deprecated Use release() instead for proper reference counting
   */
  removeAll(fnIds: string[]): void {
    fnIds.forEach((fnId) => {
      this.callbacks.delete(fnId);
      this.refCounts.delete(fnId);
    });
  }

  /**
   * Clear all callbacks
   */
  clear(): void {
    this.callbacks.clear();
    this.refCounts.clear();
    this.counter = 0;
  }

  /**
   * Get registered callback count
   */
  get size(): number {
    return this.callbacks.size;
  }

  /**
   * Get reference count for a callback (for debugging)
   */
  getRefCount(fnId: string): number {
    return this.refCounts.get(fnId) || 0;
  }
}

/**
 * Global callback registry for Guest environment
 * Used by reconciler for function serialization
 */
export const globalCallbackRegistry = new CallbackRegistry();
