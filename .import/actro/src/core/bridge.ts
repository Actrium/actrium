/**
 * AskIt Bridge - Message Handler for Host-Guest Communication
 *
 * This module handles messages from guests and routes them to
 * the appropriate native modules.
 */

import {
  BROADCASTER_SYMBOL,
  EventEmitter,
  type HostEventEmitter,
  NOTIFY_SYMBOL,
} from '../api/EventEmitter.host';
import { isGuestToHostEventName, validateGuestToHostPayload } from '../contracts/generated';
import type { ContractViolation } from '../contracts/runtime';
import { logger } from './logger';
import { MODULE_PERMISSIONS, modules } from './registry.modules';

/**
 * Message types from guests
 */
interface GuestMessage {
  event: string;
  payload?: unknown;
}

export type GuestMessageHandlerOptions = {
  onContractEvent?: (eventName: string, payload: unknown) => void;
  onContractViolation?: (violation: ContractViolation) => void;
  /**
   * Guest declared permissions (from `.actr/manifest.json`).
   */
  permissions?: readonly string[];
  /**
   * Permission policy:
   * - allow: no check
   * - warn: log violation but continue execution
   * - deny: log violation and refuse execution
   */
  permissionMode?: 'allow' | 'warn' | 'deny';
};

/**
 * Handle incoming message from guest
 *
 * Simplified implementation that inlines validation and parsing for better performance.
 */
export function handleGuestMessage(
  message: GuestMessage,
  options?: GuestMessageHandlerOptions
): unknown {
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
        const requiredPermission = MODULE_PERMISSIONS[moduleNameStr];
        const mode = options?.permissionMode ?? 'allow';
        if (requiredPermission && mode !== 'allow') {
          const declared = options?.permissions ?? [];
          const hasPermission = declared.includes(requiredPermission);
          if (!hasPermission) {
            const v: ContractViolation = {
              at: Date.now(),
              direction: 'guestToHost',
              kind: 'missing_permission',
              eventName: event,
              payload,
              reason: `missing permission: ${requiredPermission}`,
            };
            options?.onContractViolation?.(v);
            logger.warn(
              'Bridge',
              `Permission not declared: ${requiredPermission} (module ${moduleNameStr}.${methodNameStr})`,
              { permission: requiredPermission, module: moduleNameStr, method: methodNameStr }
            );
            if (mode === 'deny') return undefined;
          }
        }

        const module = modules[moduleNameStr as keyof typeof modules];
        if (module) {
          const method = module[methodNameStr as keyof typeof module];
          if (typeof method === 'function') {
            const args = Array.isArray(payload) ? payload : payload !== undefined ? [payload] : [];
            return (method as (...args: unknown[]) => unknown).apply(
              module as unknown as Record<string, unknown>,
              args
            );
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
  // Contract events (Guest -> Host)
  if (isGuestToHostEventName(event)) {
    if (validateGuestToHostPayload(event, payload)) {
      options?.onContractEvent?.(event, payload);
      return undefined;
    }

    const v: ContractViolation = {
      at: Date.now(),
      direction: 'guestToHost',
      kind: 'invalid_payload',
      eventName: event,
      payload,
      reason: 'payload does not satisfy contracts schema',
    };
    options?.onContractViolation?.(v);
    logger.error('Bridge', `Invalid contract payload: ${event}`, { event });
    return undefined;
  }

  options?.onContractViolation?.({
    at: Date.now(),
    direction: 'guestToHost',
    kind: 'unknown_event',
    eventName: event,
    payload,
    reason: 'event not declared in ask contracts',
  });
  logger.warn('Bridge', `Unknown event: ${event}`, { event });
  return undefined;
}

/**
 * Engine interface for adapter
 */
export type EngineInterface = {
  on: (
    event: 'message',
    callback: (message: { event: string; payload?: unknown }) => void
  ) => () => void;
  sendEvent: (event: string, payload?: unknown) => void;
};

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
};
export function createEngineAdapter(
  engine: EngineInterface,
  options?: GuestMessageHandlerOptions
): {
  dispose: () => void;
};
export function createEngineAdapter(
  engine: EngineInterface,
  options?: GuestMessageHandlerOptions
): {
  dispose: () => void;
} {
  // Forward EventEmitter events to engine with askit:event: prefix
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event: string, payload: unknown) => {
    engine.sendEvent(`askit:event:${event}`, payload);
  });

  // Listen for guest messages and route them
  const unsubscribeMessage = engine.on('message', (message) => {
    handleGuestMessage({ event: message.event, payload: message.payload }, options);
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
