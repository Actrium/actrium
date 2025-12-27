/**
 * Guest Globals Setup
 *
 * Sets up console and runtime helpers in the Guest sandbox.
 * This must run after Host has set up __console_* globals via setGlobal.
 */

// ============================================
// Console Setup
// ============================================
// Host sets __console_log, __console_warn, etc. via setGlobal
// We wrap them into a standard console object

declare const __console_log: (...args: unknown[]) => void;
declare const __console_warn: (...args: unknown[]) => void;
declare const __console_error: (...args: unknown[]) => void;
declare const __console_debug: (...args: unknown[]) => void;
declare const __console_info: (...args: unknown[]) => void;

if (typeof globalThis.console === 'undefined') {
  (globalThis as Record<string, unknown>).console = {
    log: (...args: unknown[]) => __console_log(...args),
    warn: (...args: unknown[]) => __console_warn(...args),
    error: (...args: unknown[]) => __console_error(...args),
    debug: (...args: unknown[]) => __console_debug(...args),
    info: (...args: unknown[]) => __console_info(...args),
  };
}

// ============================================
// Host Event System
// ============================================
// Allows Guest code to subscribe to Host events

type HostEventCallback = (payload: unknown) => void;

const __hostEventListeners = new Map<string, Set<HostEventCallback>>();

// Subscribe to host events (used by useHostEvent hook)
(globalThis as Record<string, unknown>).__useHostEvent = (
  eventName: string,
  callback: HostEventCallback
): (() => void) => {
  if (!__hostEventListeners.has(eventName)) {
    __hostEventListeners.set(eventName, new Set());
  }
  const set = __hostEventListeners.get(eventName)!;
  set.add(callback);
  return () => {
    try {
      set.delete(callback);
    } catch {
      // ignore cleanup errors
    }
  };
};

// Called by Host to dispatch events to Guest listeners
(globalThis as Record<string, unknown>).__handleHostEvent = (
  eventName: string,
  payload: unknown
): void => {
  const set = __hostEventListeners.get(eventName);
  if (set) {
    set.forEach((cb) => {
      try {
        cb(payload);
      } catch (e) {
        console.error('[rill] Host event listener error:', e);
      }
    });
  }
};

// ============================================
// Callback Registry Helpers
// ============================================
// Note: Basic __callbacks/__registerCallback/__invokeCallback are in init.ts
// Here we add __removeCallback for cleanup

if (typeof (globalThis as Record<string, unknown>).__removeCallback !== 'function') {
  (globalThis as Record<string, unknown>).__removeCallback = (id: string): void => {
    const callbacks = (globalThis as Record<string, unknown>).__callbacks as
      | Map<string, unknown>
      | undefined;
    callbacks?.delete(id);
  };
}

export const GLOBALS_SETUP_COMPLETE = true;
