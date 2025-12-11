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

// Registry exports
export { components, modules, configureToast, configureHaptic } from './registry';

// Bridge exports
export { Bridge, handleGuestMessage, createEngineAdapter } from './bridge';
export { createTypedSender, TypedBridge, validatePayload } from './typed-bridge';

// Utilities
export { throttle, debounce, rateLimit } from './throttle';
export type { RateLimitConfig } from './throttle';
export { createGuestComponent, validateProps, validators } from './component-helpers';
export type { GuestComponent, ValidationRule } from './component-helpers';

// Type exports
export type { AskitMessageMap, TypedSendToHost, EventEmitterMessageMap } from '../types';
