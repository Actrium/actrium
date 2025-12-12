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

// Type exports
export type { AskitMessageMap, EventEmitterMessageMap, TypedSendToHost } from '../types';

// Bridge exports
export { Bridge, createEngineAdapter, handleGuestMessage } from './bridge';
export type { GuestComponent, ValidationRule } from './component-helpers';
export { createGuestComponent, validateProps, validators } from './component-helpers';
// Registry exports
export { components, configureHaptic, configureToast, modules } from './registry';
export type { RateLimitConfig } from './throttle';
// Utilities
export { debounce, rateLimit, throttle } from './throttle';
export { createTypedSender, TypedBridge, validatePayload } from './typed-bridge';
