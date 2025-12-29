/**
 * Guest Environment Initialization
 *
 * This file MUST be imported FIRST before any other imports.
 * It sets up the Guest environment markers that CallbackRegistry checks.
 *
 * Why this is needed:
 * - Bundlers execute imported modules before the entry point's own code
 * - CallbackRegistry constructor checks __RILL_GUEST_ENV__ at module init time
 * - This file ensures the env is set before any other module runs
 */

import type { SandboxGlobals } from '../../sandbox/globals';

// Type-safe access to globalThis with sandbox globals
const globals = globalThis as typeof globalThis & SandboxGlobals;

// Initialize __callbacks BEFORE CallbackRegistry constructor runs
if (!globals.__callbacks) {
  globals.__callbacks = new Map();
}

// Initialize callback counter
if (typeof globals.__callbackId === 'undefined') {
  globals.__callbackId = 0;
}

// Mark Guest environment - CallbackRegistry checks this in constructor
globals.__RILL_GUEST_ENV__ = true;

// Provide __registerCallback for CallbackRegistry to use
if (typeof globals.__registerCallback !== 'function') {
  globals.__registerCallback = (fn: (...args: unknown[]) => unknown): string => {
    const id = `fn_${++globals.__callbackId!}`;
    globals.__callbacks!.set(id, fn);
    return id;
  };
}

// Provide __invokeCallback for Host to call Guest functions
if (typeof globals.__invokeCallback !== 'function') {
  globals.__invokeCallback = (fnId: string, args: unknown[]): unknown => {
    const fn = globals.__callbacks?.get(fnId);
    if (fn) {
      return fn(...(args || []));
    }
    console.warn('[rill] Callback not found:', fnId);
    return undefined;
  };
}

// Export a marker to ensure this module is imported
export const GUEST_INIT_COMPLETE = true;
