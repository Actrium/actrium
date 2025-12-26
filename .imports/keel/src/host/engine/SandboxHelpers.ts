/**
 * Sandbox Injection Helper Functions
 *
 * Utilities for formatting console output and handling sandbox globals
 */

/**
 * Format argument for console output (handles circular references)
 */
export function formatArg(arg: unknown, seen = new WeakSet()): unknown {
  if (arg === null || arg === undefined) return arg;
  if (typeof arg !== 'object') return arg;

  // Handle circular references
  if (seen.has(arg as object)) return '[Circular]';
  seen.add(arg as object);

  // Handle arrays
  if (Array.isArray(arg)) {
    return arg.map((item) => formatArg(item, seen));
  }

  // Handle plain objects
  try {
    const formatted: Record<string, unknown> = {};
    for (const key of Object.keys(arg as object)) {
      formatted[key] = formatArg((arg as Record<string, unknown>)[key], seen);
    }
    return formatted;
  } catch {
    return String(arg);
  }
}

/**
 * Format string template with placeholders (%s, %d, %i, %f, %o, %O)
 */
export function formatWithPlaceholders(template: string, params: unknown[]): string {
  let idx = 0;
  return template.replace(/%[sdifoO]/g, (token) => {
    const value = idx < params.length ? params[idx++] : '';
    switch (token) {
      case '%d':
      case '%i':
        return String(Number(value));
      case '%f':
        return String(Number(value));
      case '%o':
      case '%O': {
        try {
          return JSON.stringify(formatArg(value), null, 2);
        } catch {
          return String(value);
        }
      }
      default:
        return String(value);
    }
  });
}

/**
 * Format console arguments (handles template strings and object formatting)
 */
export function formatConsoleArgs(args: unknown[]): unknown[] {
  if (args.length > 1 && typeof args[0] === 'string' && /%[sdifoO]/.test(args[0])) {
    const [template, ...rest] = args;
    const formattedFirst = formatWithPlaceholders(template as string, rest);
    const remaining = rest
      .slice((template as string).match(/%[sdifoO]/g)?.length ?? 0)
      .map((arg) =>
        typeof arg === 'object' && arg !== null
          ? (() => {
              try {
                return JSON.stringify(formatArg(arg), null, 2);
              } catch {
                return formatArg(arg);
              }
            })()
          : arg
      );
    return [formattedFirst, ...remaining];
  }

  return args.map((arg) => {
    if (typeof arg === 'object' && arg !== null) {
      try {
        // Format objects nicely with JSON.stringify for readability
        return JSON.stringify(formatArg(arg), null, 2);
      } catch {
        return formatArg(arg);
      }
    }
    return arg;
  });
}

/**
 * Create RillSDK module object
 */
export function createRillSDKModule() {
  return {
    // React Native components (as string names)
    View: 'View',
    Text: 'Text',
    Image: 'Image',
    ScrollView: 'ScrollView',
    TouchableOpacity: 'TouchableOpacity',
    Button: 'Button',
    ActivityIndicator: 'ActivityIndicator',
    FlatList: 'FlatList',
    TextInput: 'TextInput',
    Switch: 'Switch',

    // Host communication hooks
    // These must exist for IIFE/externalized bundles that read from global RillSDK/RillLet.
    // They resolve their implementations lazily from runtime-injected globals.
    useHostEvent: (eventName: string, callback: (payload: unknown) => void) => {
      const fn = (globalThis as unknown as { __useHostEvent?: unknown }).__useHostEvent;
      if (typeof fn !== 'function') {
        throw new Error('[rill] __useHostEvent is not available yet');
      }
      return (fn as (name: string, cb: (payload: unknown) => void) => () => void)(
        eventName,
        callback
      );
    },
    useConfig: () => {
      const fn = (globalThis as unknown as { __getConfig?: unknown }).__getConfig;
      if (typeof fn !== 'function') {
        throw new Error('[rill] __getConfig is not available yet');
      }
      return (fn as () => unknown)();
    },
    useSendToHost: () => {
      return (eventName: string, payload?: unknown) => {
        const fn = (globalThis as unknown as { __sendEventToHost?: unknown }).__sendEventToHost;
        if (typeof fn !== 'function') {
          throw new Error('[rill] __sendEventToHost is not available yet');
        }
        return (fn as (eventName: string, payload?: unknown) => void)(eventName, payload);
      };
    },
  };
}

/**
 * Create minimal CommonJS globals
 */
export function createCommonJSGlobals() {
  const moduleObj = { exports: {} as Record<string, unknown> };
  return {
    module: moduleObj,
    exports: moduleObj.exports,
  };
}

/**
 * Create React Native shim module
 */
export function createReactNativeShim() {
  const Image = {
    type: 'Image',
    resolveAssetSource: (source: unknown) => source,
    prefetch: async (_uri?: string) => true,
    queryCache: async (_uris?: string[]) => ({}) as Record<string, 'disk' | 'memory'>,
    getSize: (_uri: string, success?: (w: number, h: number) => void) => {
      if (typeof success === 'function') success(0, 0);
    },
  };

  return {
    Platform: {
      OS: 'web',
      select: (o: Record<string, unknown>) => o.default ?? o.web,
    },
    StyleSheet: { create: (s: unknown) => s },
    View: 'View',
    Text: 'Text',
    Image,
    ScrollView: 'ScrollView',
    TouchableOpacity: 'TouchableOpacity',
    Button: 'Button',
    ActivityIndicator: 'ActivityIndicator',
    FlatList: 'FlatList',
    TextInput: 'TextInput',
    Switch: 'Switch',
  };
}

/**
 * Create console setup code for sandbox
 */
export const CONSOLE_SETUP_CODE = `
(function(){
  if (typeof globalThis.console === 'undefined') {
    globalThis.console = {
      log: function() { __console_log.apply(null, arguments); },
      warn: function() { __console_warn.apply(null, arguments); },
      error: function() { __console_error.apply(null, arguments); },
      debug: function() { __console_debug.apply(null, arguments); },
      info: function() { __console_info.apply(null, arguments); }
    };
  }
})();`;

/**
 * Create runtime helpers code for sandbox
 */
export const RUNTIME_HELPERS_CODE = `
(function(){
  if (typeof globalThis.__hostEventListeners === 'undefined') {
    var __hostEventListeners = new Map();
    globalThis.__useHostEvent = function(eventName, callback){
      if (!__hostEventListeners.has(eventName)) __hostEventListeners.set(eventName, new Set());
      var set = __hostEventListeners.get(eventName);
      set.add(callback);
      return function(){ try { set.delete(callback); } catch(_){} };
    };
    globalThis.__handleHostEvent = function(eventName, payload){
      var set = __hostEventListeners.get(eventName);
      if (set) {
        set.forEach(function(cb){ try { cb(payload); } catch(e) { console.error('[rill] Host event listener error:', e); } });
      }
    };
  }

  // Callback registry helpers (used by shims to serialize function props)
  // Note: Some bundles also inject these; keep it idempotent.
  if (!globalThis.__callbacks) {
    globalThis.__callbacks = new Map();
  }
  if (typeof globalThis.__callbackId !== 'number') {
    globalThis.__callbackId = 0;
  }
  if (typeof globalThis.__registerCallback !== 'function') {
    globalThis.__registerCallback = function(fn){
      var id = 'fn_' + (++globalThis.__callbackId);
      globalThis.__callbacks.set(id, fn);
      return id;
    };
  }
  if (typeof globalThis.__invokeCallback !== 'function') {
    globalThis.__invokeCallback = function(id, args){
      var fn = globalThis.__callbacks.get(id);
      if (fn) {
        if (typeof console !== 'undefined' && console.log) {
          console.log('[rill] Invoking callback:', id, 'fn type:', typeof fn, 'args:', args);
        }
        try {
          return fn.apply(null, args || []);
        } catch(e) {
          console.error('[rill] Callback execution error for', id);
          console.error('[rill] Error type:', typeof e);
          console.error('[rill] Error message:', e && e.message ? e.message : String(e));
          console.error('[rill] Error stack:', e && e.stack ? e.stack : 'no stack');
          console.error('[rill] Error name:', e && e.name ? e.name : 'no name');
          throw e;
        }
      } else {
        console.warn('[rill] Callback not found:', id);
      }
    };
  }
  if (typeof globalThis.__removeCallback !== 'function') {
    globalThis.__removeCallback = function(id){
      globalThis.__callbacks.delete(id);
    };
  }
})();`;
