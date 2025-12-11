/**
 * AskIt Bridge - Message Handler for Host-Guest Communication
 *
 * This module handles messages from guests and routes them to
 * the appropriate native modules.
 */

import { modules } from './registry';
import {
  EventEmitter,
  HostEventEmitter,
  BROADCASTER_SYMBOL,
  NOTIFY_SYMBOL,
} from '../api/EventEmitter.host';
import { logger } from './logger';

/**
 * Message types from guests
 */
interface GuestMessage {
  event: string;
  payload?: unknown;
}

/**
 * Handle incoming message from guest
 *
 * Simplified implementation that inlines validation and parsing for better performance.
 */
export function handleGuestMessage(message: GuestMessage): unknown {
  // Inline validation
  if (
    !message ||
    typeof message !== 'object' ||
    !message.event ||
    typeof message.event !== 'string'
  ) {
    logger.error('Bridge', 'Invalid message format', {
      message: typeof message === 'object' ? JSON.stringify(message) : String(message),
    });
    return undefined;
  }

  const { event, payload } = message;

  // Handle askit protocol messages (format: askit:...)
  if (event.startsWith('askit:')) {
    const parts = event.slice(6).split(':');

    if (parts.length < 2) {
      logger.warn('Bridge', `Invalid askit protocol format: ${event}`, { event });
      return undefined;
    }

    // EventEmitter events (format: askit:event:eventName)
    // Must check this FIRST because eventName can contain colons (e.g., user:login)
    if (parts[0] === 'event') {
      const eventName = parts.slice(1).join(':'); // Support nested events like 'user:login'
      (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL](eventName, payload);
      return undefined;
    }

    // Module calls (format: askit:module:method) - exactly 2 parts after 'askit:'
    if (parts.length === 2) {
      // Validate module and method names (alphanumeric and underscore only)
      const validName = /^[a-zA-Z0-9_]+$/;
      const moduleNameStr = parts[0]!;
      const methodNameStr = parts[1]!;
      if (validName.test(moduleNameStr) && validName.test(methodNameStr)) {
        const module = modules[moduleNameStr as keyof typeof modules];
        if (module) {
          const method = module[methodNameStr as keyof typeof module];
          if (typeof method === 'function') {
            const args = Array.isArray(payload) ? payload : payload !== undefined ? [payload] : [];
            return (method as (...args: unknown[]) => unknown).apply(module as any, args);
          }
        }

        logger.warn('Bridge', `Unknown module or method: ${moduleNameStr}.${methodNameStr}`, {
          module: moduleNameStr,
          method: methodNameStr,
          event,
        });
      }
    } else {
      logger.warn('Bridge', `Invalid askit protocol format: ${event}`, { event });
    }
    return undefined;
  }

  // Unknown event
  logger.warn('Bridge', `Unknown event: ${event}`, { event });
  return undefined;
}

/**
 * Engine interface for adapter
 */
interface EngineInterface {
  on: (
    event: 'message',
    callback: (message: { event: string; payload: unknown }) => void
  ) => () => void;
  sendEvent: (event: string, payload?: unknown) => void;
}

/**
 * Create engine adapter for Rill integration
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { createEngineAdapter, components } from 'askit/core';
 *
 * const engine = new Engine();
 * const adapter = createEngineAdapter(engine);
 * engine.register(components);
 * ```
 */
export function createEngineAdapter(engine: EngineInterface): {
  dispose: () => void;
} {
  // Forward EventEmitter events to engine with askit:event: prefix
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event: string, payload: unknown) => {
    engine.sendEvent(`askit:event:${event}`, payload);
  });

  // Listen for guest messages and route them
  const unsubscribeMessage = engine.on('message', (message) => {
    handleGuestMessage({ event: message.event, payload: message.payload });
  });

  return {
    dispose() {
      // Disconnect broadcaster
      (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL](null);
      // Unsubscribe from engine messages
      unsubscribeMessage();
    },
  };
}

/**
 * Bridge utilities
 */
export const Bridge = {
  handleMessage: handleGuestMessage,
  createAdapter: createEngineAdapter,
};

export default Bridge;
