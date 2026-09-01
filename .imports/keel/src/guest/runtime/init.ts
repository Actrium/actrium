/**
 * Guest Environment Initialization
 *
 * This file MUST be imported FIRST before any other imports.
 * It sets up the Guest environment markers that CallbackRegistry checks.
 */

import type { SandboxGlobals } from '../../host/sandbox/globals';

const globals = globalThis as typeof globalThis & SandboxGlobals;

// Initialize __keel namespace object
if (!globals.__keel) {
  globals.__keel = {};
}
const __keel = globals.__keel;

// Initialize callbacks BEFORE CallbackRegistry constructor runs
if (!__keel.callbacks) {
  __keel.callbacks = new Map();
}

// Initialize callback counter
if (typeof __keel.callbackId === 'undefined') {
  __keel.callbackId = 0;
}

// Mark Guest environment - CallbackRegistry checks this in constructor
globals.__KEEL_GUEST_ENV__ = true;

// Provide registerCallback for CallbackRegistry to use
if (typeof __keel.registerCallback !== 'function') {
  __keel.registerCallback = (fn: (...args: unknown[]) => unknown): string => {
    const id = `fn_${++__keel.callbackId!}`;
    __keel.callbacks!.set(id, fn);
    return id;
  };
}

// Provide invokeCallback for Host to call Guest functions
if (typeof __keel.invokeCallback !== 'function') {
  __keel.invokeCallback = (fnId: string, args: unknown[]): unknown => {
    const fn = __keel.callbacks?.get(fnId);
    if (fn) {
      return fn(...(args || []));
    }
    console.warn('[keel] Callback not found:', fnId);
    return undefined;
  };
}

export const GUEST_INIT_COMPLETE = true;
