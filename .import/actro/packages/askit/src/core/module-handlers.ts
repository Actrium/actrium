/**
 * Module Handlers Registry
 *
 * Defines how ASKIT_* events are handled by the bridge.
 * New modules can be added here without modifying bridge.ts.
 */

import { logger } from './logger';
import { MODULE_PERMISSIONS, modules } from './registry.modules';

/**
 * Module handler definition
 */
export interface ModuleHandler {
  /** Required permission (from MODULE_PERMISSIONS) */
  permission?: string;
  /** Validate payload before executing */
  validate: (payload: unknown) => boolean;
  /** Execute the module action */
  execute: (payload: unknown) => unknown;
}

/**
 * Extract and validate Toast payload
 */
function validateToastPayload(payload: unknown): payload is { message: string; options?: unknown } {
  if (!payload || typeof payload !== 'object') return false;
  const { message } = payload as { message?: unknown };
  return typeof message === 'string';
}

/**
 * Extract and validate Haptic payload
 */
function validateHapticPayload(payload: unknown): payload is { type?: string } {
  // Haptic payload is optional, just needs to be object or undefined
  return payload === undefined || payload === null || typeof payload === 'object';
}

/**
 * Module handlers registry
 *
 * Maps ASKIT_* event names to their handlers.
 * To add a new module:
 * 1. Add handler here
 * 2. Add permission in registry.modules.ts
 */
export const moduleHandlers: Record<string, ModuleHandler> = {
  ASKIT_TOAST_SHOW: {
    permission: MODULE_PERMISSIONS['toast'],
    validate: validateToastPayload,
    execute: (payload) => {
      const { message, options } = payload as { message: string; options?: unknown };
      return modules.toast.show(message, options as Parameters<typeof modules.toast.show>[1]);
    },
  },

  ASKIT_HAPTIC_TRIGGER: {
    permission: MODULE_PERMISSIONS['haptic'],
    validate: validateHapticPayload,
    execute: (payload) => {
      const { type } = (payload as { type?: string }) || {};
      return modules.haptic.trigger(type as Parameters<typeof modules.haptic.trigger>[0]);
    },
  },
};

/**
 * Check if an event is a module event
 */
export function isModuleEvent(event: string): boolean {
  return event in moduleHandlers;
}

/**
 * Get handler for a module event
 */
export function getModuleHandler(event: string): ModuleHandler | undefined {
  return moduleHandlers[event];
}

/**
 * Execute a module handler with validation and logging
 */
export function executeModuleHandler(
  event: string,
  payload: unknown,
  handler: ModuleHandler
): unknown {
  if (!handler.validate(payload)) {
    logger.warn('Bridge', `Invalid payload for ${event}`, { event });
    return undefined;
  }

  return handler.execute(payload);
}
