/**
 * AskIt Bridge - Message Handler for Host-Guest Communication
 *
 * This module handles messages from guests and routes them to
 * the appropriate native modules.
 */

import { AskitModules } from './registry';
import { Bus, NativeBus } from '../api/Bus.native';

/**
 * Message types from guests
 */
interface GuestMessage {
  event: string;
  payload?: unknown;
}

/**
 * Parse askit event name
 * Format: askit:module:method
 */
function parseAskitEvent(event: string): { module: string; method: string } | null {
  if (!event.startsWith('askit:')) {
    return null;
  }

  const parts = event.slice(6).split(':');
  if (parts.length !== 2) {
    return null;
  }

  const module = parts[0];
  const method = parts[1];
  if (!module || !method) {
    return null;
  }

  return {
    module,
    method,
  };
}

/**
 * Handle incoming message from guest
 */
export function handleGuestMessage(message: GuestMessage): unknown {
  const { event, payload } = message;

  // Handle askit module calls
  const parsed = parseAskitEvent(event);
  if (parsed) {
    const handler = AskitModules[parsed.module];
    if (handler) {
      const args = Array.isArray(payload) ? payload : payload !== undefined ? [payload] : [];
      return handler.handle(parsed.method, args);
    }
    console.warn(`[askit/bridge] Unknown module: ${parsed.module}`);
    return undefined;
  }

  // Handle Bus events (format: bus:eventName)
  if (event.startsWith('bus:')) {
    const busEvent = event.slice(4);
    (Bus as NativeBus)._handleEngineMessage(busEvent, payload);
    return undefined;
  }

  // Unknown event
  console.warn(`[askit/bridge] Unknown event: ${event}`);
  return undefined;
}

/**
 * Create engine adapter for Rill integration
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { createEngineAdapter, AskitRegistry } from 'askit/core';
 *
 * const engine = new Engine();
 * const adapter = createEngineAdapter(engine);
 * engine.register(AskitRegistry.components);
 * ```
 */
export function createEngineAdapter(engine: {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  on: (event: string, callback: (...args: any[]) => void) => void;
  sendEvent: (event: string, payload?: unknown) => void;
}): {
  dispose: () => void;
} {
  // Listen for host events and forward to engine
  const unregisterBus = (Bus as NativeBus)._registerEngine((event: string, payload: unknown) => {
    engine.sendEvent(event, payload);
  });

  // Listen for guest messages
  engine.on('message', (event: string, payload: unknown) => {
    handleGuestMessage({ event, payload });
  });

  return {
    dispose() {
      unregisterBus();
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
