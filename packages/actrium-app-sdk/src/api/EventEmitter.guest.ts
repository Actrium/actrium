/**
 * EventEmitter - Guest Implementation
 *
 * In guest/sandbox environment, EventEmitter communicates with Host through the bridge.
 * Uses the event hooks injected by the Keel runtime.
 *
 * Features:
 * - Automatic retry on network failures
 * - Message queue for offline support
 * - Lazy host event subscription
 */

import { logger } from '../core/logger';
import type { EventEmitterAPI } from '../types';
import { EventEmitterBase } from './EventEmitterBase';

interface QueuedMessage {
  event: string;
  payload: unknown;
  retries: number;
  timestamp: number;
}

// Globals injected into every Keel sandbox.
const runtimeGlobal = globalThis as typeof globalThis & {
  __keel_emitEvent?: (eventName: string, payload?: unknown) => void;
  __keel_onHostEvent?: (
    eventName: string,
    callback: (payload: unknown) => void
  ) => () => void;
  __sendEventToHost?: (eventName: string, payload?: unknown) => void;
  __useHostEvent?: (eventName: string, callback: (payload: unknown) => void) => () => void;
};

const getSendToHost = () =>
  runtimeGlobal.__keel_emitEvent ?? runtimeGlobal.__sendEventToHost;

const getUseHostEvent = () =>
  runtimeGlobal.__keel_onHostEvent ?? runtimeGlobal.__useHostEvent;

class GuestEventEmitter extends EventEmitterBase implements EventEmitterAPI {
  private messageQueue: QueuedMessage[] = [];
  private retryTimer: ReturnType<typeof setTimeout> | null = null;

  // Host event subscription tracking (for keel's per-event subscription model)
  private hostEventUnsubscribers: (() => void)[] = [];
  private subscribedHostEvents = new Set<string>();

  // Configuration
  private maxRetries = 3;
  private retryDelay = 1000;
  private maxQueueSize = 100;

  /**
   * Hook called when a new listener is added - subscribe to host event
   */
  protected override onListenerAdded(event: string): void {
    this.ensureHostEventSubscribed(event);
  }

  /**
   * Ensure a specific host event is subscribed (lazy subscription)
   */
  private ensureHostEventSubscribed(eventName: string): void {
    if (this.subscribedHostEvents.has(eventName)) return;
    const useHostEvent = getUseHostEvent();
    if (!useHostEvent) return;

    const unsubscribe = useHostEvent(eventName, (payload: unknown) => {
      this.notifyLocalListeners(eventName, payload);
    });

    this.hostEventUnsubscribers.push(unsubscribe);
    this.subscribedHostEvents.add(eventName);
  }

  /**
   * Emit an event to Host
   */
  emit(event: string, payload?: unknown): void {
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
   * Send message to host with automatic retry
   */
  private sendToHostWithRetry(event: string, payload: unknown): void {
    const sendToHost = getSendToHost();
    if (!sendToHost) {
      logger.warn('EventEmitter', '__sendEventToHost not available in this environment');
      return;
    }

    try {
      sendToHost(event, payload);
    } catch (error) {
      logger.error('EventEmitter', `Failed to send event "${event}" to host`, {
        event,
        error: error instanceof Error ? error.message : String(error),
      });
      this.queueMessage(event, payload);
    }
  }

  /**
   * Add message to retry queue
   */
  private queueMessage(event: string, payload: unknown): void {
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
    if (this.retryTimer) return;

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
        });
        continue;
      }

      const sendToHost = getSendToHost();
      if (!sendToHost) {
        msg.retries++;
        failed.push(msg);
        continue;
      }

      try {
        sendToHost(msg.event, msg.payload);
      } catch (error) {
        logger.error(
          'EventEmitter',
          `Retry ${msg.retries + 1}/${this.maxRetries} failed for "${msg.event}"`,
          {
            event: msg.event,
            error: error instanceof Error ? error.message : String(error),
          }
        );
        msg.retries++;
        failed.push(msg);
      }
    }

    this.messageQueue = failed;

    if (failed.length > 0) {
      this.scheduleRetry();
    }
  }

  /**
   * Simulate receiving event from host (for testing)
   * @internal
   */
  _simulateHostEvent(event: string, payload: unknown): void {
    this.notifyLocalListeners(event, payload);
  }

  /**
   * Clear retry timer (for testing cleanup)
   * @internal
   */
  _clearRetryTimer(): void {
    if (this.retryTimer) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
    this.messageQueue = [];
  }

  /**
   * Configure retry behavior (for testing)
   * @internal
   */
  _setRetryConfig(config: { maxRetries?: number; retryDelay?: number }): void {
    if (config.maxRetries !== undefined) this.maxRetries = config.maxRetries;
    if (config.retryDelay !== undefined) this.retryDelay = config.retryDelay;
  }
}

// Export singleton instance
export const EventEmitter: EventEmitterAPI = new GuestEventEmitter();

// Export class for testing
export { GuestEventEmitter };
