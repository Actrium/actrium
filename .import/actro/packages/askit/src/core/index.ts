/**
 * AskIt Core - Host Only Module
 *
 * This module is only available in Host App environment.
 * It provides the registry and bridge for connecting askit to Rill engine.
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { components, createEngineAdapter } from 'askit/core';
 *
 * const engine = new Engine();
 * engine.register(components);
 * const adapter = createEngineAdapter(engine);
 * ```
 */

// Bridge exports
export type { GuestMessageHandlerOptions } from './bridge';
export { Bridge, createEngineAdapter, handleGuestMessage } from './bridge';

// Registry exports
export { components, configureHaptic, configureToast, modules } from './registry';
export type { RateLimitConfig } from './throttle';

// Utilities
export { debounce, rateLimit, throttle } from './throttle';

// Protocol constants
export { ASKIT_HAPTIC_TRIGGER, ASKIT_TOAST_SHOW } from './protocol';
