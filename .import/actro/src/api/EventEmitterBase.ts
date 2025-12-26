/**
 * EventEmitter - Shared Base Class
 *
 * Contains common logic for Host and Guest EventEmitter implementations:
 * - Pattern matching (wildcards)
 * - Rate limiting
 * - Memory leak detection
 * - Local listener management
 */

import { logger } from '../core/logger';
import { rateLimit } from '../core/throttle';
import type { EventCallback, EventEmitterAPI, EventListenerOptions } from '../types';

export type ListenerMap = Map<string, Set<EventCallback<unknown>>>;
export type PatternListenerMap = Map<EventCallback<unknown>, { pattern: string; regex: RegExp }>;
export type CallbackWrapperMap = Map<EventCallback<unknown>, EventCallback<unknown>>;

/**
 * Abstract base class for EventEmitter implementations
 *
 * Subclasses must implement:
 * - emit(): How to broadcast events (Host broadcasts to engine, Guest sends to host)
 */
export abstract class EventEmitterBase implements EventEmitterAPI {
  protected listeners: ListenerMap = new Map();
  protected patternListeners: PatternListenerMap = new Map();
  protected callbackWrappers: CallbackWrapperMap = new Map();
  protected maxListeners = 10;
  protected warnedEvents = new Set<string>();

  /**
   * Emit an event - subclasses define how events are broadcast
   */
  abstract emit(event: string, payload?: unknown): void;

  /**
   * Hook called when a new listener is added for a non-pattern event
   * Guest uses this to lazily subscribe to host events
   */
  protected onListenerAdded(_event: string): void {
    // Override in subclass if needed
  }

  /**
   * Convert wildcard pattern to regex
   * Examples:
   *   'user:*'     -> matches 'user:login', 'user:logout'
   *   'user:**'    -> matches 'user:profile:update', 'user:settings:theme'
   */
  protected patternToRegex(pattern: string): RegExp {
    const escaped = pattern
      .replace(/[.+?^${}()|[\]\\]/g, '\\$&')
      .replace(/\*\*/g, '___DOUBLE_STAR___')
      .replace(/\*/g, '[^:]+')
      .replace(/___DOUBLE_STAR___/g, '.+');
    return new RegExp(`^${escaped}$`);
  }

  /**
   * Check if event name contains wildcards
   */
  protected isPattern(event: string): boolean {
    return event.includes('*');
  }

  /**
   * Get listener count for an event
   */
  protected getListenerCount(event: string): number {
    const exactListeners = this.listeners.get(event);
    return exactListeners ? exactListeners.size : 0;
  }

  /**
   * Check and warn if listener count exceeds maximum
   */
  protected checkMaxListeners(event: string): void {
    if (this.maxListeners === 0) return;

    const count = this.getListenerCount(event);
    if (count > this.maxListeners && !this.warnedEvents.has(event)) {
      this.warnedEvents.add(event);
      logger.warn(
        'EventEmitter',
        `Possible memory leak detected: ${count} listeners for "${event}"`,
        {
          event,
          count,
          maxListeners: this.maxListeners,
          suggestion: 'Use the cleanup function returned by on() in React useEffect',
        }
      );
    }
  }

  /**
   * Notify local listeners (exact match + pattern match)
   */
  protected notifyLocalListeners(event: string, payload: unknown): void {
    // 1. Exact match
    const callbacks = this.listeners.get(event);
    if (callbacks) {
      callbacks.forEach((callback) => {
        try {
          callback(payload);
        } catch (error) {
          logger.error('EventEmitter', `Error in listener for "${event}"`, {
            event,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      });
    }

    // 2. Pattern match
    this.patternListeners.forEach((data, callback) => {
      if (data.regex.test(event)) {
        try {
          callback(payload);
        } catch (error) {
          logger.error('EventEmitter', `Error in pattern listener for "${event}"`, {
            event,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    });
  }

  /**
   * Set maximum number of listeners per event (0 = unlimited)
   */
  setMaxListeners(max: number): void {
    this.maxListeners = Math.max(0, max);
    this.warnedEvents.clear();
  }

  /**
   * Get maximum listeners limit
   */
  getMaxListeners(): number {
    return this.maxListeners;
  }

  /**
   * Subscribe to an event (supports wildcards and rate limiting)
   */
  on<T = unknown>(
    event: string,
    callback: EventCallback<T>,
    options?: EventListenerOptions
  ): () => void {
    // Apply rate limiting if specified
    let actualCallback: EventCallback<T> = callback;
    if (options?.rateLimit && options.rateLimit !== 'none') {
      const limited = rateLimit(callback as (...args: unknown[]) => unknown, {
        type: options.rateLimit,
        delay: options.delay ?? 100,
      });
      actualCallback = limited as EventCallback<T>;
      this.callbackWrappers.set(
        callback as EventCallback<unknown>,
        actualCallback as EventCallback<unknown>
      );
    }

    if (this.isPattern(event)) {
      // Pattern listener
      const regex = this.patternToRegex(event);
      this.patternListeners.set(actualCallback as EventCallback<unknown>, {
        pattern: event,
        regex,
      });

      return () => {
        this.patternListeners.delete(actualCallback as EventCallback<unknown>);
        this.callbackWrappers.delete(callback as EventCallback<unknown>);
      };
    } else {
      // Exact match listener
      let callbacks = this.listeners.get(event);
      if (!callbacks) {
        callbacks = new Set();
        this.listeners.set(event, callbacks);
      }
      callbacks.add(actualCallback as EventCallback<unknown>);

      // Hook for subclass (Guest uses this for lazy host subscription)
      this.onListenerAdded(event);

      // Check for potential memory leaks
      this.checkMaxListeners(event);

      return () => {
        this.off(event, callback);
      };
    }
  }

  /**
   * Unsubscribe from an event
   */
  off<T = unknown>(event: string, callback: EventCallback<T>): void {
    const actualCallback =
      (this.callbackWrappers.get(callback as EventCallback<unknown>) as EventCallback<T>) ||
      callback;

    if (this.isPattern(event)) {
      this.patternListeners.delete(actualCallback as EventCallback<unknown>);
      this.callbackWrappers.delete(callback as EventCallback<unknown>);
      return;
    }

    const callbacks = this.listeners.get(event);
    if (callbacks) {
      callbacks.delete(actualCallback as EventCallback<unknown>);
      if (callbacks.size === 0) {
        this.listeners.delete(event);
      }
      this.callbackWrappers.delete(callback as EventCallback<unknown>);
    }
  }

  /**
   * Subscribe to an event once
   */
  once<T = unknown>(event: string, callback: EventCallback<T>): () => void {
    const wrapper: EventCallback<T> = (payload) => {
      this.off(event, wrapper);
      callback(payload);
    };
    return this.on(event, wrapper);
  }

  /**
   * Remove all listeners for an event, or all listeners if no event specified
   */
  removeAllListeners(event?: string): void {
    if (event === undefined) {
      this.listeners.clear();
      this.patternListeners.clear();
      this.callbackWrappers.clear();
      this.warnedEvents.clear();
    } else if (this.isPattern(event)) {
      this.patternListeners.forEach((data, callback) => {
        if (data.pattern === event) {
          this.patternListeners.delete(callback);
        }
      });
    } else {
      this.listeners.delete(event);
      this.warnedEvents.delete(event);
    }
  }
}
