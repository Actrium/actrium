/**
 * EventEmitter - Guest Implementation
 *
 * In guest/sandbox environment, EventEmitter communicates with Host through the bridge.
 * Uses global.sendToHost and global.onHostEvent provided by Rill runtime.
 *
 * Features:
 * - Wildcard pattern matching ('user:*', 'analytics:**')
 * - Automatic retry on network failures
 * - Message queue for offline support
 */

import type { EventEmitterAPI, EventCallback, EventListenerOptions } from '../types';
import { logger } from '../core/logger';
import { rateLimit } from '../core/throttle';

type ListenerMap = Map<string, Set<EventCallback<unknown>>>;
type PatternListenerMap = Map<EventCallback<unknown>, { pattern: string; regex: RegExp }>;
// Map from original callback to rate-limited callback
type CallbackWrapperMap = Map<EventCallback<unknown>, EventCallback<unknown>>;

interface QueuedMessage {
  event: string;
  payload: unknown;
  retries: number;
  timestamp: number;
}

import type { TypedSendToHost } from '../types';

// Declare globals injected by Rill runtime
declare const global: {
  sendToHost?: TypedSendToHost;
  onHostEvent?: (callback: (event: string, payload: unknown) => void) => void;
};

class GuestEventEmitter implements EventEmitterAPI {
  private listeners: ListenerMap = new Map();
  private patternListeners: PatternListenerMap = new Map();
  private callbackWrappers: CallbackWrapperMap = new Map();
  private messageQueue: QueuedMessage[] = [];
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private initialized = false;

  // Configuration
  private maxRetries = 3;
  private retryDelay = 1000; // ms
  private maxQueueSize = 100;
  private maxListeners = 10; // Prevent memory leaks from forgotten cleanups
  private warnedEvents = new Set<string>(); // Track which events already warned

  constructor() {
    this.initHostEventListener();
  }

  /**
   * Initialize host event listener (once)
   */
  private initHostEventListener(): void {
    if (this.initialized) return;
    this.initialized = true;

    // Register callback to receive events from Host
    if (typeof global.onHostEvent === 'function') {
      global.onHostEvent((event: string, payload: unknown) => {
        this.handleHostEvent(event, payload);
      });
    }
  }

  /**
   * Handle incoming event from Host
   */
  private handleHostEvent(event: string, payload: unknown): void {
    // 1. Trigger exact match listeners
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

    // 2. Trigger pattern match listeners
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
   * Examples:
   *   'user:*'     -> matches 'user:login', 'user:logout'
   *   'user:**'    -> matches 'user:profile:update', 'user:settings:theme'
   *   'analytics:*:click' -> matches 'analytics:button:click', 'analytics:link:click'
   */
  private patternToRegex(pattern: string): RegExp {
    const escaped = pattern
      .replace(/[.+?^${}()|[\]\\]/g, '\\$&') // Escape regex special chars
      .replace(/\*\*/g, '___DOUBLE_STAR___')
      .replace(/\*/g, '[^:]+') // Single * matches one segment (no colons)
      .replace(/___DOUBLE_STAR___/g, '.+'); // ** matches multiple segments
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
   * Emit an event to Host
   */
  emit(event: string, payload?: unknown): void {
    // Validate event name
    if (!event || typeof event !== 'string') {
      logger.error('EventEmitter', 'Invalid event name - must be a non-empty string');
      return;
    }

    // Notify local listeners in sandbox
    this.notifyLocalListeners(event, payload);

    // Send to Host through bridge with retry logic
    this.sendToHostWithRetry(event, payload);
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
   * Send message to host with automatic retry
   */
  private sendToHostWithRetry(event: string, payload: unknown): void {
    if (typeof global.sendToHost !== 'function') {
      logger.warn('EventEmitter', 'sendToHost not available in this environment');
      return;
    }

    try {
      global.sendToHost(`askit:event:${event}`, payload);
    } catch (error) {
      logger.error('EventEmitter', `Failed to send event "${event}" to host`, {
        event,
        error: error instanceof Error ? error.message : String(error),
        hasSendToHost: typeof global.sendToHost === 'function',
      });

      // Add to retry queue
      this.queueMessage(event, payload);
    }
  }

  /**
   * Add message to retry queue
   */
  private queueMessage(event: string, payload: unknown): void {
    // Check queue size limit
    if (this.messageQueue.length >= this.maxQueueSize) {
      logger.warn(
        'EventEmitter',
        `Message queue full (${this.maxQueueSize}), dropping oldest message`
      );
      this.messageQueue.shift();
    }

    this.messageQueue.push({
      event,
      payload,
      retries: 0,
      timestamp: Date.now(),
    });

    this.scheduleRetry();
  }

  /**
   * Schedule retry attempt
   */
  private scheduleRetry(): void {
    if (this.retryTimer) return; // Already scheduled

    this.retryTimer = setTimeout(() => {
      this.processQueue();
    }, this.retryDelay);
  }

  /**
   * Process message queue and retry failed messages
   */
  private processQueue(): void {
    this.retryTimer = null;

    if (this.messageQueue.length === 0) return;

    const failed: QueuedMessage[] = [];

    while (this.messageQueue.length > 0) {
      const msg = this.messageQueue.shift()!;

      if (msg.retries >= this.maxRetries) {
        logger.error('EventEmitter', `Dropped event "${msg.event}" after ${msg.retries} retries`, {
          event: msg.event,
          retries: msg.retries,
          timestamp: msg.timestamp,
        });
        continue;
      }

      if (typeof global.sendToHost !== 'function') {
        // Host still not available, requeue
        msg.retries++;
        failed.push(msg);
        continue;
      }

      try {
        global.sendToHost(`askit:event:${msg.event}`, msg.payload);
        // Success! Don't requeue
      } catch (error) {
        logger.error(
          'EventEmitter',
          `Retry ${msg.retries + 1}/${this.maxRetries} failed for "${msg.event}"`,
          {
            event: msg.event,
            retries: msg.retries + 1,
            maxRetries: this.maxRetries,
            error: error instanceof Error ? error.message : String(error),
          }
        );
        msg.retries++;
        failed.push(msg);
      }
    }

    // Requeue failed messages
    this.messageQueue = failed;

    // Schedule next retry if needed
    if (failed.length > 0) {
      this.scheduleRetry();
    }
  }

  /**
   * Subscribe to an event (supports wildcards and rate limiting)
   *
   * Examples:
   *   on('user:login', cb)           // Exact match
   *   on('user:*', cb)               // Match user:login, user:logout, etc.
   *   on('analytics:**', cb)         // Match all nested analytics events
   *   on('api:*:success', cb)        // Match api:users:success, api:posts:success
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
      const limited = rateLimit(callback, {
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

  /**
   * Simulate receiving event from host (for testing)
   * @internal
   */
  _simulateHostEvent(event: string, payload: unknown): void {
    this.handleHostEvent(event, payload);
  }
}

// Export singleton instance
export const EventEmitter: EventEmitterAPI = new GuestEventEmitter();

// Export class for testing
export { GuestEventEmitter };
