/**
 * EventEmitter - Host Implementation
 *
 * Provides event-based communication between Host and Guest engines.
 * In host environment, EventEmitter manages multiple engine instances and broadcasts events.
 *
 * Features:
 * - Wildcard pattern matching ('user:*', 'analytics:**')
 * - Multi-engine broadcast support
 * - Error handling and isolation
 */

import type { EventEmitterAPI, EventCallback, EventListenerOptions } from '../types';
import { logger } from '../core/logger';
import { rateLimit } from '../core/throttle';

type ListenerMap = Map<string, Set<EventCallback<unknown>>>;
type PatternListenerMap = Map<EventCallback<unknown>, { pattern: string; regex: RegExp }>;
// Map from original callback to rate-limited callback
type CallbackWrapperMap = Map<EventCallback<unknown>, EventCallback<unknown>>;

/**
 * Event broadcast callback type
 * Used by createEngineAdapter to forward events to Rill Engine
 */
export type EventBroadcaster = (event: string, payload: unknown) => void;

/**
 * Internal symbols for accessing private APIs
 * These are used by core modules (bridge.ts) but hidden from external users
 */
const BROADCASTER_SYMBOL = Symbol.for('askit.eventEmitter.broadcaster');
const NOTIFY_SYMBOL = Symbol.for('askit.eventEmitter.notify');

class HostEventEmitter implements EventEmitterAPI {
  private listeners: ListenerMap = new Map();
  private patternListeners: PatternListenerMap = new Map();
  private callbackWrappers: CallbackWrapperMap = new Map();
  private broadcaster: EventBroadcaster | null = null;
  private maxListeners = 10; // Prevent memory leaks from forgotten cleanups
  private warnedEvents = new Set<string>(); // Track which events already warned

  /**
   * Emit an event to all listeners and broadcast to engine if connected
   */
  emit(event: string, payload?: unknown): void {
    // Notify local listeners (exact match + pattern match)
    this.notifyLocalListeners(event, payload);

    // Broadcast to engine if connected
    if (this.broadcaster) {
      try {
        this.broadcaster(event, payload);
      } catch (error) {
        logger.error('EventEmitter', 'Error broadcasting to engine', {
          event,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  /**
   * Notify local listeners (exact match + pattern match)
   */
  private notifyLocalListeners(event: string, payload: unknown): void {
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
   * Convert wildcard pattern to regex
   */
  private patternToRegex(pattern: string): RegExp {
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
  private isPattern(event: string): boolean {
    return event.includes('*');
  }

  /**
   * Get listener count for an event
   */
  private getListenerCount(event: string): number {
    const exactListeners = this.listeners.get(event);
    return exactListeners ? exactListeners.size : 0;
  }

  /**
   * Check and warn if listener count exceeds maximum
   */
  private checkMaxListeners(event: string): void {
    if (this.maxListeners === 0) return; // 0 means unlimited

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
   * Set maximum number of listeners per event (0 = unlimited)
   */
  setMaxListeners(max: number): void {
    this.maxListeners = Math.max(0, max);
    this.warnedEvents.clear(); // Reset warnings
  }

  /**
   * Get maximum listeners limit
   */
  getMaxListeners(): number {
    return this.maxListeners;
  }

  /**
   * Subscribe to an event (supports wildcards and rate limiting)
   *
   * Examples:
   *   on('user:login', cb)           // Exact match
   *   on('user:*', cb)               // Match user:login, user:logout, etc.
   *   on('analytics:**', cb)         // Match all nested analytics events
   *
   *   // With rate limiting (prevents performance issues from high-frequency events)
   *   on('scroll:position', cb, { rateLimit: 'throttle', delay: 100 })
   *   on('input:change', cb, { rateLimit: 'debounce', delay: 200 })
   *
   * @param event - Event name or pattern
   * @param callback - Event handler
   * @param options - Listener options (rate limiting, etc.)
   * @returns Cleanup function to unsubscribe
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
        delay: options.delay ?? 100, // Default 100ms
      });
      actualCallback = limited as EventCallback<T>;
      // Store mapping for cleanup
      this.callbackWrappers.set(
        callback as EventCallback<unknown>,
        actualCallback as EventCallback<unknown>
      );
    }

    // Check if pattern or exact match
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

      // Check for potential memory leaks
      this.checkMaxListeners(event);

      return () => {
        this.off(event, callback);
      };
    }
  }

  /**
   * Unsubscribe from an event (supports wildcards)
   */
  off<T = unknown>(event: string, callback: EventCallback<T>): void {
    // Get the actual callback (might be rate-limited wrapper)
    const actualCallback =
      (this.callbackWrappers.get(callback as EventCallback<unknown>) as EventCallback<T>) ||
      callback;

    // Try pattern listeners first
    if (this.isPattern(event)) {
      this.patternListeners.delete(actualCallback as EventCallback<unknown>);
      this.callbackWrappers.delete(callback as EventCallback<unknown>);
      return;
    }

    // Try exact match listeners
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
   * Subscribe to an event once (supports wildcards)
   * @returns Cleanup function to unsubscribe
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
   * @param event - Optional event name or pattern
   */
  removeAllListeners(event?: string): void {
    if (event === undefined) {
      // Remove all listeners
      this.listeners.clear();
      this.patternListeners.clear();
      this.callbackWrappers.clear();
      this.warnedEvents.clear();
    } else if (this.isPattern(event)) {
      // Remove pattern listeners matching this pattern
      this.patternListeners.forEach((data, callback) => {
        if (data.pattern === event) {
          this.patternListeners.delete(callback);
        }
      });
    } else {
      // Remove exact match listeners
      this.listeners.delete(event);
      this.warnedEvents.delete(event);
    }
  }

  // =========================================================================
  // Internal methods for createEngineAdapter (accessed via Symbols)
  // =========================================================================

  /**
   * Connect to engine broadcaster (called by createEngineAdapter)
   * @internal - Accessed via BROADCASTER_SYMBOL
   */
  [BROADCASTER_SYMBOL](broadcaster: EventBroadcaster | null): void {
    this.broadcaster = broadcaster;
  }

  /**
   * Notify listeners without broadcasting to engine (called by createEngineAdapter)
   * Used when receiving events from engine to avoid infinite loops
   * @internal - Accessed via NOTIFY_SYMBOL
   */
  [NOTIFY_SYMBOL](event: string, payload: unknown): void {
    this.notifyLocalListeners(event, payload);
  }
}

// Export singleton instance
export const EventEmitter: EventEmitterAPI = new HostEventEmitter();

// Export the class for internal use by core module
export { HostEventEmitter };

// Export symbols for core module access
export { BROADCASTER_SYMBOL, NOTIFY_SYMBOL };
