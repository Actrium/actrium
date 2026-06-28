/**
 * AskIt Bridge - Message Handler for Host-Guest Communication
 *
 * This module handles messages from guests and routes them to
 * the appropriate native modules.
 */

import { EventEmitter, type HostEventEmitterInternal } from '../api/EventEmitter.host';
import { isGuestToHostEventName, validateGuestToHostPayload } from '../contracts/generated';
import type { ContractViolation } from '../contracts/runtime';
import { logger } from './logger';
import { executeModuleHandler, getModuleHandler, isModuleEvent } from './module-handlers';

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
 * Check permission and report violation if needed
 * @returns true if execution should proceed, false if denied
 */
function checkPermission(
  event: string,
  permission: string | undefined,
  payload: unknown,
  options?: GuestMessageHandlerOptions
): boolean {
  if (!permission) return true;

  const mode = options?.permissionMode ?? 'allow';
  if (mode === 'allow') return true;

  const declared = options?.permissions ?? [];
  const hasPermission = declared.includes(permission);

  if (hasPermission) return true;

  const violation: ContractViolation = {
    at: Date.now(),
    direction: 'guestToHost',
    kind: 'missing_permission',
    eventName: event,
    payload,
    reason: `missing permission: ${permission}`,
  };

  options?.onContractViolation?.(violation);
  logger.warn('Bridge', `Permission not declared: ${permission}`, {
    permission,
    event,
  });

  return mode !== 'deny';
}

/**
 * Handle incoming message from guest
 */
export function handleGuestMessage(
  message: GuestMessage,
  options?: GuestMessageHandlerOptions
): unknown {
  // Validate message format
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

  // 1. Check if it's a module event (ASKIT_*)
  if (isModuleEvent(event)) {
    const handler = getModuleHandler(event)!;

    // Permission check
    if (!checkPermission(event, handler.permission, payload, options)) {
      return undefined;
    }

    return executeModuleHandler(event, payload, handler);
  }

  // 2. Contract events (Guest -> Host)
  if (isGuestToHostEventName(event)) {
    if (validateGuestToHostPayload(event, payload)) {
      options?.onContractEvent?.(event, payload);
      return undefined;
    }

    const violation: ContractViolation = {
      at: Date.now(),
      direction: 'guestToHost',
      kind: 'invalid_payload',
      eventName: event,
      payload,
      reason: 'payload does not satisfy contracts schema',
    };
    options?.onContractViolation?.(violation);
    logger.error('Bridge', `Invalid contract payload: ${event}`, { event });
    return undefined;
  }

  // 3. App-level events - forward to Host EventEmitter
  (EventEmitter as HostEventEmitterInternal)._notifyLocal(event, payload);
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
export function createEngineAdapter(engine: EngineInterface): { dispose: () => void };
export function createEngineAdapter(
  engine: EngineInterface,
  options?: GuestMessageHandlerOptions
): { dispose: () => void };
export function createEngineAdapter(
  engine: EngineInterface,
  options?: GuestMessageHandlerOptions
): { dispose: () => void } {
  const emitter = EventEmitter as HostEventEmitterInternal;

  // Forward Host EventEmitter events to Guest via rill Engine.sendEvent
  emitter._setBroadcaster((event: string, payload: unknown) => {
    engine.sendEvent(event, payload);
  });

  // Listen for guest messages and route them
  const unsubscribeMessage = engine.on('message', (message) => {
    handleGuestMessage({ event: message.event, payload: message.payload }, options);
  });

  return {
    dispose() {
      emitter._setBroadcaster(null);
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
