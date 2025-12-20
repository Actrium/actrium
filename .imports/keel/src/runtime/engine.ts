/**
 * Rill Engine
 *
 * Sandbox engine core, responsible for managing QuickJS execution environment and lifecycle.
 * Uses react-native-quickjs for sandboxed JavaScript execution.
 */

// Augment globalThis for DevTools integration
declare global {
  // eslint-disable-next-line no-var
  var __sendEventToHost: ((eventName: string, payload?: unknown) => void) | undefined;
}

import * as RillReconciler from '../let/reconciler/index';
import { ALL_SHIMS, DEVTOOLS_SHIM } from './shims';

import type {
  CallFunctionMessage,
  HostEventMessage,
  HostMessage,
  OperationBatch,
  SerializedValue,
} from './types';
import type { EngineDiagnostics, EngineEvents, EngineHealth, IEngine } from './IEngine';
import { Receiver } from './receiver';
import type { ComponentMap } from './registry';
import { ComponentRegistry } from './registry';
import type { RuntimeCollectorConfig, RuntimeCollector } from '../devtools/index';
import { createRuntimeCollector } from '../devtools/index';

/**
 * Engine configuration options
 */
export interface EngineOptions {
  /**
   * JS Engine provider for creating sandbox runtime.
   * Optional - if not provided, a default will be selected based on the environment.
   */
  provider?: JSEngineProvider;

  /**
   * Legacy parameter: `quickjs` (equivalent to `provider`)
   * @deprecated Use `provider` instead
   */
  quickjs?: JSEngineProvider;

  /**
   * Explicitly select a sandbox mode.
   * - `vm`: (Default on Node/Bun) Uses Node's `vm` module for a secure, native sandbox.
   * - `worker`: Uses `@sebastianwessel/quickjs` in a Web Worker.
   * - `none`: Runs code directly in the host context via `eval`. Insecure, but fast and easy to debug.
   * If not set, the best available provider for the environment is chosen automatically.
   */
  sandbox?: 'vm' | 'worker' | 'none';

  /**
   * Execution timeout (milliseconds)
   * @default 5000
   */
  timeout?: number;

  /**
   * Enable debug mode
   * @default false
   */
  debug?: boolean;

  /**
   * Custom logger
   */
  logger?: {
    log: (...args: unknown[]) => void;
    warn: (...args: unknown[]) => void;
    error: (...args: unknown[]) => void;
  };

  /**
   * Allowed modules for sandbox require()
   * If not provided, a safe default whitelist will be used
   */
  requireWhitelist?: readonly string[];

  /**
   * Performance metrics reporter hook
   * Called with metric name and duration in ms
   */
  onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;

  /**
   * Maximum operations per batch applied by Receiver
   * Excess operations are skipped to protect host performance
   * @default 5000
   */
  receiverMaxBatchSize?: number;

  /**
   * Diagnostics parameters (for Host-side Task Manager/Resource Monitor)
   */
  diagnostics?: {
    /**
     * Stats window (ms) for calculating ops/s and batch/s
     * @default 5000
     */
    activityWindowMs?: number;
    /**
     * Activity sample retention duration (ms), for timeline aggregation
     * @default 60000
     */
    activityHistoryMs?: number;
    /**
     * Timeline bucket width (ms)
     * @default 2000
     */
    activityBucketMs?: number;
  };

  /**
   * DevTools configuration
   * - true: Enable with default settings
   * - false/undefined: Disable (default)
   * - RuntimeCollectorConfig: Enable with custom settings
   */
  devtools?: boolean | RuntimeCollectorConfig;
}

// Re-export types for convenience
export type {
  EngineActivityStats,
  EngineActivityTimeline,
  EngineActivityTimelinePoint,
  EngineDiagnostics,
  EngineEvents,
  EngineHealth,
  GuestMessage,
  IEngine,
} from './IEngine';

/**
 * Event listener
 */
type EventListener<T> = (data: T) => void;

// Re-export sandbox provider types
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from '../sandbox';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from '../sandbox';

/** Error types for better classification */
export class RequireError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'RequireError';
  }
}
export class ExecutionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ExecutionError';
  }
}
export class TimeoutError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TimeoutError';
  }
}

/**
 * Rill Engine - JS sandbox engine with dedicated runtime
 *
 * Each Engine instance owns an isolated JS runtime.
 * Create a new Engine for each isolated context (e.g., each tab/view).
 *
 * @example
 * ```typescript
 * const engine = new Engine({ debug: true });
 * engine.register({ StepList: NativeStepList });
 * await engine.loadBundle(bundleCode);
 * // When done:
 * engine.destroy();
 * ```
 */
// Global engine counter for debugging
let engineIdCounter = 0;

export class Engine implements IEngine {
  private runtime: JSEngineRuntime | null = null;
  private context: JSEngineContext | null = null;
  private registry: ComponentRegistry;
  private receiver: Receiver | null = null;
  private config: Record<string, unknown> = {};
  private options: {
    provider?: JSEngineProvider;
    timeout: number;
    debug: boolean;
    logger: NonNullable<EngineOptions['logger']>;
    requireWhitelist: ReadonlySet<string>;
    onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
    receiverMaxBatchSize: number;
  };
  private destroyed = false;
  private loaded = false;
  private errorCount = 0;
  private lastErrorAt: number | null = null;
  private _timeoutTimer?: ReturnType<typeof setTimeout>;

  // Unique engine ID (UUID-like format)
  public readonly id: string;

  // Timer tracking for resource cleanup
  private timeoutMap = new Map<number, ReturnType<typeof setTimeout>>();
  private intervalMap = new Map<number, ReturnType<typeof setInterval>>();
  private timeoutIdCounter = 0;
  private intervalIdCounter = 0;

  // Native timer references (captured during polyfill injection)
  private nativeClearTimeout?: (handle: ReturnType<typeof setTimeout>) => void;
  private nativeClearInterval?: (handle: ReturnType<typeof setInterval>) => void;

  // Event listeners
  private listeners: Map<keyof EngineEvents, Set<EventListener<unknown>>> = new Map();

  // Memory leak detection for Engine events
  private maxListeners = 10;
  private warnedEvents = new Set<keyof EngineEvents>();

  // sendToHost reference for callback registry access
  private sendToHostFn: ((batch: OperationBatch) => void) | null = null;

  // Host-side activity tracking (for Task Manager/Resource Monitor)
  private activityWindowMs = 5000;
  private activityHistoryMs = 60_000;
  private activityBucketMs = 2000;
  private activitySamples: Array<{
    at: number;
    ops: number;
    appliedOps: number;
    skippedOps: number;
    failedOps: number;
    applyDurationMs: number | null;
  }> = [];
  private totalBatches = 0;
  private totalOps = 0;
  private lastBatch: {
    batchId: number;
    at: number;
    totalOps: number;
    applyDurationMs: number | null;
  } | null = null;

  // Guest proactive event reporting observability (for Host-side monitoring)
  private lastGuestEventName: string | null = null;
  private lastGuestEventAt: number | null = null;
  private lastGuestPayloadBytes: number | null = null;
  private guestSleeping: boolean | null = null;
  private guestSleepingAt: number | null = null;

  // Host → Guest event observability (for Host-side Task Manager/Resource Monitor)
  private lastHostEventName: string | null = null;
  private lastHostEventAt: number | null = null;
  private lastHostPayloadBytes: number | null = null;

  // DevTools collector (optional)
  private _devtools: RuntimeCollector | null = null;

  constructor(options: EngineOptions = {}) {
    const defaultWhitelist = new Set(['react', 'react-native', 'react/jsx-runtime', '@rill/let']);
    // Provide a safe fallback logger if console is not available
    const defaultLogger =
      typeof console !== 'undefined'
        ? console
        : {
            log: () => {},
            warn: () => {},
            error: () => {},
            info: () => {},
            debug: () => {},
          };
    this.options = {
      provider: options.provider ?? options.quickjs,
      timeout: options.timeout ?? 5000,
      debug: options.debug ?? false,
      logger: options.logger ?? defaultLogger,
      requireWhitelist: new Set(options.requireWhitelist ?? Array.from(defaultWhitelist)),
      onMetric: options.onMetric,
      receiverMaxBatchSize: options.receiverMaxBatchSize ?? 5000,
    };

    this.activityWindowMs = options.diagnostics?.activityWindowMs ?? this.activityWindowMs;
    this.activityHistoryMs = options.diagnostics?.activityHistoryMs ?? this.activityHistoryMs;
    this.activityBucketMs = options.diagnostics?.activityBucketMs ?? this.activityBucketMs;
    if (this.activityHistoryMs < this.activityWindowMs) {
      this.activityHistoryMs = this.activityWindowMs;
    }
    if (!Number.isFinite(this.activityBucketMs) || this.activityBucketMs <= 0) {
      this.activityBucketMs = 2000;
    }

    this.registry = new ComponentRegistry();

    // Initialize JS engine provider
    // Priority: explicit provider > DefaultProvider auto-detect
    if (!this.options.provider) {
      try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const { DefaultProvider } = require('../sandbox/index');
        this.options.provider = DefaultProvider.create({
          timeout: this.options.timeout,
          sandbox: options.sandbox,
        });
        // Log provider type using logger (shows in Xcode console)
        if (this.options.debug) {
          const providerName = this.options.provider?.constructor?.name || 'unknown';
          this.options.logger.log(`[rill] Provider type: ${providerName}`);
          this.options.logger.log('[rill] Initialized DefaultProvider');
        }
      } catch (e) {
        this.options.logger.error('[rill] Failed to initialize DefaultJSEngineProvider:', e);
        // Provide a minimal fallback for tests - use isolated scope instead of globalThis
        const isolatedScope: Record<string, unknown> = {};
        this.options.provider = {
          createRuntime: () => ({
            createContext: () => ({
              eval: (code: string) => {
                // Create a function with the isolated scope
                const fn = new Function(...Object.keys(isolatedScope), code);
                return fn(...Object.values(isolatedScope));
              },
              setGlobal: (name: string, value: unknown) => {
                isolatedScope[name] = value;
              },
              getGlobal: (name: string) => isolatedScope[name],
              dispose: () => {},
            }),
            dispose: () => {},
          }),
        };
      }
    }

    // Generate unique engine ID
    const counter = ++engineIdCounter;
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2, 6);
    this.id = `engine-${counter}-${timestamp}-${random}`;

    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.id}] Engine created`);
    }

    // Initialize DevTools if enabled
    if (options.devtools) {
      const devtoolsConfig = typeof options.devtools === 'object' ? options.devtools : {};
      this._devtools = createRuntimeCollector(devtoolsConfig);
      this._devtools.enable();
      if (this.options.debug) {
        this.options.logger.log(`[rill:${this.id}] DevTools enabled`);
      }
    }

    // Emit metric for engine creation
    this.options.onMetric?.('engine.created', 1, { engineId: this.id });
  }

  /**
   * Register custom components
   */
  register(components: ComponentMap): void {
    this.registry.registerAll(components);
    if (this.options.debug) {
      this.options.logger.log('[rill] Registered components:', Object.keys(components).join(', '));
    }
  }

  /**
   * Load and execute Guest code
   */
  async loadBundle(source: string, initialProps?: Record<string, unknown>): Promise<void> {
    if (this.destroyed) {
      throw new Error('[rill] Engine has been destroyed');
    }

    if (this.loaded) {
      throw new Error('[rill] Engine already loaded a Guest');
    }

    this.config = initialProps ?? {};

    try {
      // Get bundle code
      const code = await this.resolveSource(source);

      if (this.options.debug) {
        this.options.logger.log(`[rill:${this.id}] Bundle loaded, length:`, code.length);
        this.options.logger.log(`[rill:${this.id}] Bundle preview:`, code.substring(0, 200));
        this.options.logger.log(
          `[rill:${this.id}] Bundle footer (last 500 chars):`,
          code.substring(code.length - 500)
        );
        this.options.logger.log(`[rill:${this.id}] Has Auto-render:`, code.includes('Auto-render'));
      }

      // Initialize sandbox and execute
      await this.initializeRuntime();

      // Update DevTools sandbox status
      this._devtools?.updateSandboxStatus({ state: 'running' });

      // Execute with timeout protection
      // Note: Hard timeout enforcement depends on the provider:
      // - VMProvider: Uses vm.Script timeout (hard interrupt)
      // - WorkerProvider: Web Worker sandbox (async-only)
      // - QuickJSProvider/JSCProvider: Native JSI implementation dependent
      //
      // The timer below serves as a fallback safety net for async providers
      // and will forcibly destroy the engine if execution hangs.

      const timeout = this.options.timeout;
      const hasTimeout = timeout > 0 && typeof globalThis.setTimeout === 'function';

      if (hasTimeout) {
        // Use Promise.race for hard timeout enforcement on async providers
        const timeoutPromise = new Promise<never>((_, reject) => {
          const timer = globalThis.setTimeout(() => {
            // Fatal: execution exceeded timeout, force destroy to prevent snowball
            this.options.logger.error(
              `[rill] Fatal: Bundle execution exceeded timeout ${timeout}ms, destroying engine`
            );
            const error = new TimeoutError(
              `[rill] Execution exceeded timeout ${timeout}ms (hard limit)`
            );

            // Emit fatal error before destroying
            this.emit('fatalError', error);

            // Force destroy the engine to prevent resource leak
            this.forceDestroy();

            reject(error);
          }, timeout);

          // Store timer reference for cleanup
          this._timeoutTimer = timer;
        });

        try {
          await Promise.race([this.executeBundle(code), timeoutPromise]);
        } finally {
          // Clean up timer if execution completed normally
          if (this._timeoutTimer) {
            globalThis.clearTimeout(this._timeoutTimer);
            this._timeoutTimer = undefined;
          }
        }
      } else {
        // No timeout protection available
        await this.executeBundle(code);
      }

      this.loaded = true;
      this._devtools?.updateSandboxStatus({ state: 'ready' });
      this.emit('load');

      if (this.options.debug) {
        this.options.logger.log(`[rill:${this.id}] Bundle executed successfully`);
      }
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      this.errorCount += 1;
      this.lastErrorAt = Date.now();
      this._devtools?.recordSandboxError();
      this._devtools?.updateSandboxStatus({ state: 'error' });
      this.emit('error', err);
      throw err;
    }
  }

  /**
   * Resolve bundle source (URL or code string)
   */
  private async resolveSource(source: string): Promise<string> {
    const start = Date.now();
    if (source.startsWith('http://') || source.startsWith('https://')) {
      const s1 = Date.now();
      const response = await fetch(source);
      if (!response.ok) {
        this.options.onMetric?.('engine.fetchBundle', Date.now() - s1, { status: response.status });
        throw new Error(`Failed to fetch bundle: ${response.status}`);
      }
      const text = await response.text();
      this.options.onMetric?.('engine.fetchBundle', Date.now() - s1, {
        status: 200,
        size: text.length,
      });
      this.options.onMetric?.('engine.resolveSource', Date.now() - start);
      return text;
    }
    this.options.onMetric?.('engine.resolveSource', Date.now() - start);
    return source;
  }

  /**
   * Initialize QuickJS runtime using the provided QuickJS provider
   */
  private async initializeRuntime(): Promise<void> {
    const start = Date.now();
    const debug = this.options.debug;
    const logger = this.options.logger;

    if (!this.options.provider) {
      throw new Error('[rill] QuickJS provider not initialized');
    }
    if (debug) logger.log(`[rill:${this.id}] initializeRuntime: creating runtime...`);
    this.runtime = await this.options.provider.createRuntime();
    if (debug) logger.log(`[rill:${this.id}] initializeRuntime: runtime created`);
    this.context = this.runtime.createContext();
    if (debug) {
      logger.log(`[rill:${this.id}] initializeRuntime: context created, injecting polyfills...`);
    }

    await this.injectPolyfills();
    if (debug) {
      logger.log(`[rill:${this.id}] initializeRuntime: polyfills done, injecting runtimeAPI...`);
    }
    await this.injectRuntimeAPI();
    if (debug) logger.log(`[rill:${this.id}] initializeRuntime: done`);

    const dur = Date.now() - start;
    this.options.onMetric?.('engine.initializeRuntime', dur);
  }

  /**
   * Inject polyfills into sandbox
   */
  private async injectPolyfills(): Promise<void> {
    if (!this.context) return;

    const logger = this.options.logger;
    const debug = this.options.debug;

    // Helper to await and log setGlobal calls for debugging
    const setGlobalWithLog = async (name: string, value: unknown): Promise<void> => {
      if (debug) logger.log(`[rill:${this.id}] setGlobal: ${name} starting...`);
      const start = Date.now();
      try {
        await this.context!.setGlobal(name, value);
        if (debug)
          logger.log(`[rill:${this.id}] setGlobal: ${name} done (${Date.now() - start}ms)`);
      } catch (e) {
        logger.error(`[rill:${this.id}] setGlobal: ${name} failed:`, e);
        throw e;
      }
    };

    // Save native timer functions to avoid recursion issues (with fallbacks for test environments)
    const nativeSetTimeout =
      typeof globalThis.setTimeout === 'function'
        ? globalThis.setTimeout.bind(globalThis)
        : (fn: () => void, _ms?: number) => {
            Promise.resolve().then(fn);
            return 0;
          };
    const nativeClearTimeout =
      typeof globalThis.clearTimeout === 'function'
        ? globalThis.clearTimeout.bind(globalThis)
        : () => {};
    const nativeSetInterval =
      typeof globalThis.setInterval === 'function'
        ? globalThis.setInterval.bind(globalThis)
        : () => 0;
    const nativeClearInterval =
      typeof globalThis.clearInterval === 'function'
        ? globalThis.clearInterval.bind(globalThis)
        : () => {};
    const nativeQueueMicrotask =
      typeof globalThis.queueMicrotask === 'function'
        ? globalThis.queueMicrotask.bind(globalThis)
        : (fn: () => void) => Promise.resolve().then(fn);

    // Inject React/JSX shims into Guest sandbox
    // React runs entirely within Guest - no cross-engine serialization needed
    // This is the correct architecture: Guest has its own lightweight React implementation
    const injectReactShims = async () => {
      try {
        // Check if shims are already injected (e.g., from previous load)
        const alreadyInjected = this.context?.getGlobal('__REACT_SHIM__');
        if (alreadyInjected === true) {
          if (debug) logger.log(`[rill:${this.id}] React shims already injected, skipping`);
          return;
        }

        // Inject all shims (console, React, JSX runtime)
        await this.evalCode(ALL_SHIMS);

        if (debug) logger.log(`[rill:${this.id}] React/JSX shims injected into Guest sandbox`);
      } catch (e) {
        logger.warn(`[rill:${this.id}] Failed to inject React shims:`, e);
      }
    };

    // Inject RillSDK as global variable for IIFE bundles
    // Bundle format: (function(ReactJSXRuntime, React, RillSDK) { ... })(ReactJSXRuntime, React, RillSDK)
    // Note: This object only contains strings and nulls, so it should serialize fine
    const RillSDKModule = {
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
      // Host communication hooks will be added later in injectRuntimeAPI
      useHostEvent: null,
      useConfig: null,
      useSendToHost: null,
    };
    await setGlobalWithLog('RillSDK', RillSDKModule);

    // Helper to format objects for logging (handles circular references)
    const formatArg = (arg: unknown, seen = new WeakSet()): unknown => {
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
    };

    const formatArgs = (args: unknown[]): unknown[] => {
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
    };

    const engineId = this.id;

    // console - Register each method separately for JSC sandbox compatibility
    // JSC sandbox can't handle objects with function properties via RN bridge
    await setGlobalWithLog('__console_log', (...args: unknown[]) => {
      if (debug) logger.log(`[rill:${engineId}][Guest]`, ...formatArgs(args));
    });
    await setGlobalWithLog('__console_warn', (...args: unknown[]) => {
      logger.warn(`[rill:${engineId}][Guest]`, ...formatArgs(args));
    });
    await setGlobalWithLog('__console_error', (...args: unknown[]) => {
      logger.error(`[rill:${engineId}][Guest]`, ...formatArgs(args));
    });
    await setGlobalWithLog('__console_debug', (...args: unknown[]) => {
      if (debug) logger.log(`[rill:${engineId}][Guest:debug]`, ...formatArgs(args));
    });
    await setGlobalWithLog('__console_info', (...args: unknown[]) => {
      if (debug) logger.log(`[rill:${engineId}][Guest:info]`, ...formatArgs(args));
    });

    // Inject React/JSX shims BEFORE require is set up
    // This allows require('react') to return the Guest's shim
    await injectReactShims();

    // require: module loader for Guest code
    // Note: Globals like __useHostEvent, __getConfig, __sendEventToHost are defined AFTER require.
    // They are accessed lazily when require('rill/sdk') is called (after injectPolyfills completes).
    await setGlobalWithLog('require', (moduleName: string) => {
      if (debug) {
        logger.log('[rill:require]', moduleName);
      }

      if (!this.options.requireWhitelist.has(moduleName)) {
        throw new RequireError(`[rill] Unsupported require("${moduleName}")`);
      }

      switch (moduleName) {
        case 'react':
          // Return Guest's React shim (injected via injectReactShims)
          // This avoids cross-engine serialization of complex Host objects
          return this.context?.getGlobal('React');
        case 'react-native':
          // Return a minimal RN shim - real RN module not available in sandbox
          // Guest code should use string component names via rill/sdk, not RN directly
          return {
            Platform: {
              OS: 'web',
              select: (o: Record<string, unknown>) => o['default'] ?? o['web'],
            },
            StyleSheet: { create: (s: unknown) => s },
            View: 'View',
            Text: 'Text',
            Image: 'Image',
          };
        case 'react/jsx-runtime':
          // Return Guest's JSX runtime shim (injected via injectReactShims)
          return this.context?.getGlobal('ReactJSXRuntime');
        case 'rill/reconciler':
          return RillReconciler;
        case '@rill/let':
          // Guest SDK module - provides component names (strings), host hooks, and reconciler
          // Component names are used by rill reconciler to look up registered components
          // Host communication hooks are accessed lazily from sandbox globals
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
            // Host communication hooks (lazily accessed from sandbox globals)
            useHostEvent: this.context?.getGlobal('__useHostEvent'),
            useConfig: this.context?.getGlobal('__getConfig'),
            useSendToHost: () => this.context?.getGlobal('__sendEventToHost'),
            // Reconciler functions (for auto-render)
            render: RillReconciler.render,
            unmount: RillReconciler.unmount,
            unmountAll: RillReconciler.unmountAll,
          };
        default:
          throw new Error(`[rill] Unsupported require("${moduleName}")`);
      }
    });

    // React/JSX shims are already injected before require was set up
    // No need for lazy getters - Guest has its own React implementation

    // Store native clear functions for cleanup
    this.nativeClearTimeout = nativeClearTimeout;
    this.nativeClearInterval = nativeClearInterval;

    // setTimeout / clearTimeout - use instance maps for cleanup
    await setGlobalWithLog('setTimeout', (fn: () => void, delay: number) => {
      const id = ++this.timeoutIdCounter;
      const handle = nativeSetTimeout(() => {
        this.timeoutMap.delete(id);
        try {
          fn();
        } catch (error) {
          // Enhanced error handling for timer callbacks
          const err = error instanceof Error ? error : new Error(String(error));
          logger.error(`[rill:${this.id}][Guest] setTimeout error:`, err);

          // Track error for monitoring
          this.errorCount++;
          this.lastErrorAt = Date.now();

          // Emit error event so Host can handle it
          this.emit('error', err);
        }
      }, delay);
      this.timeoutMap.set(id, handle as unknown as ReturnType<typeof setTimeout>);
      return id;
    });

    await setGlobalWithLog('clearTimeout', (id: number) => {
      const handle = this.timeoutMap.get(id);
      if (handle !== undefined) {
        nativeClearTimeout(handle);
        this.timeoutMap.delete(id);
      }
    });

    // setInterval / clearInterval - use instance maps for cleanup
    await setGlobalWithLog('setInterval', (fn: () => void, delay: number) => {
      const id = ++this.intervalIdCounter;
      const handle = nativeSetInterval(() => {
        try {
          fn();
        } catch (error) {
          // Enhanced error handling for interval callbacks
          const err = error instanceof Error ? error : new Error(String(error));
          logger.error(`[rill:${this.id}][Guest] setInterval error:`, err);

          // Track error for monitoring
          this.errorCount++;
          this.lastErrorAt = Date.now();

          // Emit error event so Host can handle it
          this.emit('error', err);
        }
      }, delay);
      this.intervalMap.set(id, handle as unknown as ReturnType<typeof setInterval>);
      return id;
    });

    await setGlobalWithLog('clearInterval', (id: number) => {
      const handle = this.intervalMap.get(id);
      if (handle !== undefined) {
        nativeClearInterval(handle);
        this.intervalMap.delete(id);
      }
    });

    // queueMicrotask
    await setGlobalWithLog('queueMicrotask', (fn: () => void) => {
      nativeQueueMicrotask(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Guest] queueMicrotask error:', error);
        }
      });
    });

    // Unhandled Promise Rejection monitoring
    // This catches Promise rejections that are not handled with .catch()
    // Note: Support varies by sandbox environment (vm/worker/none)
    try {
      const unhandledRejectionHandler = (event: {
        reason?: unknown;
        promise?: Promise<unknown>;
        preventDefault?: () => void;
      }) => {
        const error =
          event.reason instanceof Error ? event.reason : new Error(String(event.reason));
        logger.error(`[rill:${this.id}][Guest] Unhandled Promise Rejection:`, error);

        // Track error for monitoring
        this.errorCount++;
        this.lastErrorAt = Date.now();

        // Emit error event so Host can handle it
        this.emit('error', error);

        // Prevent error from bubbling to Host console
        if (event.preventDefault) {
          event.preventDefault();
        }
      };

      // Try to set unhandledrejection handler
      // Different sandbox environments have different support
      if (typeof globalThis !== 'undefined') {
        // Modern browsers and Node.js support
        if ('addEventListener' in globalThis) {
          globalThis.addEventListener('unhandledrejection', unhandledRejectionHandler);
        } else if ('onunhandledrejection' in globalThis) {
          globalThis.onunhandledrejection = unhandledRejectionHandler;
        }
      }

      // Inject into sandbox context
      await setGlobalWithLog('onunhandledrejection', unhandledRejectionHandler);
    } catch (_err) {
      // Silently fail if unhandledrejection is not supported
      if (debug) {
        logger.warn(`[rill:${this.id}] Unhandledrejection handler not supported in this sandbox`);
      }
    }

    // Construct console object in sandbox using the registered callbacks
    // This is necessary because JSC sandbox can't handle objects with function properties
    const CONSOLE_SETUP = `
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
    try {
      await this.evalCode(CONSOLE_SETUP);
      if (debug) {
        logger.log(`[rill:${this.id}] injectPolyfills: console object constructed in sandbox`);
      }
    } catch (e) {
      logger.warn(`[rill:${this.id}] Failed to construct console in sandbox:`, e);
    }
  }

  /**
   * Inject runtime API into sandbox
   */
  private async injectRuntimeAPI(): Promise<void> {
    if (!this.context) return;

    const debug = this.options.debug;
    const logger = this.options.logger;

    // Collect all setGlobal promises (may be sync or async depending on provider)
    const setGlobalPromises: (void | Promise<void>)[] = [];

    // Inject runtime helpers for host-guest event communication
    const RUNTIME_HELPERS = `
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
})();`;
    try {
      await this.evalCode(RUNTIME_HELPERS);
    } catch (e) {
      logger.warn('[rill] Failed to inject runtime helpers:', e);
    }

    // __sendToHost: Send operations to host
    const sendToHost = (batch: OperationBatch) => {
      if (debug) {
        logger.log(`[rill:${this.id}] __sendToHost called, operations:`, batch.operations.length);
      }

      // Send to DevTools
      if (typeof globalThis.__sendEventToHost === 'function') {
        globalThis.__sendEventToHost('DEVTOOLS_OPERATIONS', batch);
      }

      // Record batch activity (regardless of receiver presence)
      const at = Date.now();
      const totalOps = batch.operations.length;
      this.totalBatches += 1;
      this.totalOps += totalOps;
      this.lastBatch = {
        batchId: batch.batchId,
        at,
        totalOps,
        applyDurationMs: null,
      };
      const sample: (typeof this.activitySamples)[number] = {
        at,
        ops: totalOps,
        appliedOps: 0,
        skippedOps: 0,
        failedOps: 0,
        applyDurationMs: null,
      };
      this.activitySamples.push(sample);
      // Trim samples outside window to prevent unbounded growth
      const cutoff = at - this.activityHistoryMs;
      while (this.activitySamples.length > 0 && this.activitySamples[0]!.at < cutoff) {
        this.activitySamples.shift();
      }
      // Fallback limit (for extreme cases)
      if (this.activitySamples.length > 2000) {
        this.activitySamples = this.activitySamples.slice(-1000);
      }

      this.emit('operation', batch);
      if (this.receiver) {
        if (debug) {
          logger.log(`[rill:${this.id}] Applying batch to receiver`);
        }
        const applyStats = this.receiver.applyBatch(batch);
        sample.appliedOps = applyStats.applied;
        sample.skippedOps = applyStats.skipped;
        sample.failedOps = applyStats.failed;
        sample.applyDurationMs = applyStats.durationMs;
        if (this.lastBatch && this.lastBatch.batchId === batch.batchId) {
          this.lastBatch.applyDurationMs = applyStats.durationMs;
        }

        // Record to DevTools collector
        this._devtools?.logBatch(
          { batchId: batch.batchId, operations: batch.operations },
          applyStats.durationMs
        );
      } else {
        logger.warn(`[rill:${this.id}] No receiver to apply batch!`);
      }
    };

    // Save reference for callback registry access
    this.sendToHostFn = sendToHost;
    setGlobalPromises.push(this.context.setGlobal('__sendToHost', sendToHost));

    // __getConfig: Get initial configuration
    setGlobalPromises.push(this.context.setGlobal('__getConfig', () => this.config));

    // __sendEventToHost: Send event to host
    setGlobalPromises.push(
      this.context.setGlobal('__sendEventToHost', (eventName: string, payload?: unknown) => {
        if (debug) {
          logger.log('[rill] Guest event:', eventName, payload);
        }

        // Handle DevTools messages from Guest
        if (eventName.startsWith('__DEVTOOLS_') && this._devtools) {
          const p = payload as Record<string, unknown> | undefined;
          switch (eventName) {
            case '__DEVTOOLS_CONSOLE__':
              // Console logs are forwarded via emit for external processing
              if (p?.entry) {
                this.emit(
                  'devtoolsConsole',
                  p.entry as Parameters<EngineEvents['devtoolsConsole']>[0]
                );
              }
              break;
            case '__DEVTOOLS_ERROR__':
              // Errors are recorded in devtools and emitted
              this._devtools.recordSandboxError();
              if (p?.error) {
                this.emit('devtoolsError', p.error as Parameters<EngineEvents['devtoolsError']>[0]);
              }
              break;
            case '__DEVTOOLS_READY__':
              // Guest devtools is ready
              this.emit('devtoolsReady', {});
              break;
          }
          return; // Don't process as regular message
        }

        this.lastGuestEventName = eventName;
        this.lastGuestEventAt = Date.now();
        if (payload === undefined) {
          this.lastGuestPayloadBytes = 0;
        } else {
          try {
            this.lastGuestPayloadBytes = JSON.stringify(payload).length;
          } catch {
            this.lastGuestPayloadBytes = null;
          }
        }

        // Special convention: Guest reports its sleep state (used with HOST_VISIBILITY)
        if (eventName === 'GUEST_SLEEP_STATE' && payload && typeof payload === 'object') {
          const sleeping = (payload as { sleeping?: unknown }).sleeping;
          if (typeof sleeping === 'boolean') {
            this.guestSleeping = sleeping;
            this.guestSleepingAt = Date.now();
          }
        }
        this.emit('message', { event: eventName, payload });
      })
    );

    // __handleHostMessage: Handle messages from host
    setGlobalPromises.push(
      this.context.setGlobal('__handleHostMessage', (message: HostMessage) => {
        this.handleHostMessage(message);
      })
    );

    // Skip RillSDK/ReactNative hooks update for JSC sandbox
    // The hooks (__useHostEvent, __getConfig, __sendEventToHost) are already available as global functions
    // Bundles use require('rill/sdk') which returns these via the callback proxy mechanism
    // Trying to pass an object with getGlobal results (Promises in JSC) causes serialization issues
    if (debug) {
      logger.log(
        `[rill:${this.id}] injectRuntimeAPI: skipping RillSDK hooks update (available via require())`
      );
    }

    // Wait for all setGlobal operations to complete (important for async providers like JSC)
    await Promise.all(setGlobalPromises);
    if (debug) {
      logger.log(`[rill:${this.id}] injectRuntimeAPI: all setGlobal operations done`);
    }

    // Inject DevTools Guest shim if enabled
    if (this._devtools) {
      try {
        await this.evalCode(DEVTOOLS_SHIM);
        if (debug) {
          logger.log(`[rill:${this.id}] injectRuntimeAPI: DevTools shim injected`);
        }
      } catch (e) {
        logger.warn(`[rill:${this.id}] Failed to inject DevTools shim:`, e);
      }
    }
  }

  /**
   * Helper to evaluate code - uses evalAsync if available (for Worker providers),
   * otherwise falls back to sync eval.
   * Note: evalAsync is a non-standard extension for async-only providers.
   */
  private async evalCode(code: string): Promise<void> {
    if (!this.context) return;
    // Check for non-standard evalAsync (Worker providers)
    const ctx = this.context as JSEngineContext & {
      evalAsync?: (code: string) => Promise<unknown>;
    };
    if (ctx.evalAsync) {
      await ctx.evalAsync(code);
    } else {
      this.context.eval(code);
    }
  }

  /**
   * Execute bundle code in sandbox
   */
  private async executeBundle(code: string): Promise<void> {
    const start = Date.now();
    if (!this.context) {
      throw new Error('[rill] Context not initialized');
    }

    try {
      await this.evalCode(code);
      const dur = Date.now() - start;
      this.options.onMetric?.('engine.executeBundle', dur, { size: code.length });
    } catch (error) {
      this.options.logger.error('[rill] Bundle execution error:', error);
      const err = error instanceof Error ? error : new Error(String(error));
      throw new ExecutionError(err.message);
    }
  }

  /**
   * Handle messages from host
   */
  private async handleHostMessage(message: HostMessage): Promise<void> {
    switch (message.type) {
      case 'CALL_FUNCTION':
        await this.handleCallFunction(message);
        break;
      case 'HOST_EVENT':
        await this.handleHostEvent(message);
        break;
      case 'CONFIG_UPDATE':
        this.config = { ...this.config, ...message.config };
        break;
      case 'DESTROY':
        this.destroy();
        break;
    }
  }

  /**
   * Handle callback function invocation
   */
  private async handleCallFunction(message: CallFunctionMessage): Promise<void> {
    if (!this.context) return;

    if (this.options.debug) {
      this.options.logger.log(
        `[rill:${this.id}] handleCallFunction called, fnId:`,
        message.fnId,
        'args:',
        message.args
      );
    }
    try {
      // Record to DevTools
      this._devtools?.recordCallback(message.fnId, message.args);

      // ✅ Prefer Host-side CallbackRegistry:
      // - In RNQuickJS / VMProvider scenarios, reconciler runs on Host (Hermes/Node) side,
      //   serializePropsWithTracking() registers function handles from Guest into CallbackRegistry.
      // - When Receiver triggers events, it only carries fnId; we can invoke directly via registry.
      // - Fallback: if fnId not found in registry, fall back to Guest runtime's __invokeCallback (legacy path).
      if (
        typeof RillReconciler.hasCallback === 'function' &&
        RillReconciler.hasCallback(message.fnId)
      ) {
        RillReconciler.invokeCallback(message.fnId, message.args);
        if (this.options.debug) {
          this.options.logger.log(
            `[rill:${this.id}] Successfully invoked callback (host registry)`
          );
        }
        return;
      }

      await this.evalCode(`__invokeCallback("${message.fnId}", ${JSON.stringify(message.args)})`);
      if (this.options.debug) {
        this.options.logger.log(`[rill:${this.id}] Successfully invoked callback (sandbox eval)`);
      }
    } catch (error) {
      this.options.logger.error(
        `[rill:${this.id}] Failed to invoke callback ${message.fnId}:`,
        error
      );
    }
  }

  /**
   * Handle host event
   */
  private async handleHostEvent(message: HostEventMessage): Promise<void> {
    if (!this.context) return;

    try {
      await this.evalCode(
        `__handleHostEvent("${message.eventName}", ${JSON.stringify(message.payload)})`
      );
    } catch (error) {
      this.options.logger.error(`[rill] Failed to handle host event ${message.eventName}:`, error);
    }
  }

  /**
   * Send message to sandbox
   */
  async sendToSandbox(message: HostMessage): Promise<void> {
    if (this.destroyed || !this.context) return;
    const start = Date.now();
    await this.evalCode(`__handleHostMessage(${JSON.stringify(message)})`);
    this.options.onMetric?.('engine.sendToSandbox', Date.now() - start, {
      size: JSON.stringify(message).length,
    });
  }

  /**
   * Emit event
   */
  emit<K extends keyof EngineEvents>(
    event: K,
    ...args: EngineEvents[K] extends () => void ? [] : [Parameters<EngineEvents[K]>[0]]
  ): void {
    const listeners = this.listeners.get(event);
    if (listeners) {
      listeners.forEach((listener) => {
        try {
          listener(args[0]);
        } catch (error) {
          this.options.logger.error(`[rill] Event listener error:`, error);
        }
      });
    }
  }

  /**
   * Listen to engine events
   */
  on<K extends keyof EngineEvents>(
    event: K,
    listener: EngineEvents[K] extends () => void
      ? () => void
      : (data: Parameters<EngineEvents[K]>[0]) => void
  ): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(listener as EventListener<unknown>);

    // Memory leak detection - warn if listener count exceeds threshold
    if (this.options.debug) {
      const count = this.listeners.get(event)!.size;
      if (count > this.maxListeners && !this.warnedEvents.has(event)) {
        this.options.logger.warn(
          `[rill] Possible EventEmitter memory leak detected. ` +
            `${count} listeners added for event "${String(event)}". ` +
            `Use setMaxListeners() to increase limit.`
        );
        this.warnedEvents.add(event);
      }
    }

    return () => {
      this.listeners.get(event)?.delete(listener as EventListener<unknown>);
      // Clear warning if count drops below threshold
      if (this.warnedEvents.has(event)) {
        const count = this.listeners.get(event)?.size ?? 0;
        if (count <= this.maxListeners) {
          this.warnedEvents.delete(event);
        }
      }
    };
  }

  /**
   * Send event to sandbox guest
   */
  sendEvent(eventName: string, payload?: unknown): void {
    this.lastHostEventName = eventName;
    this.lastHostEventAt = Date.now();
    if (payload === undefined) {
      this.lastHostPayloadBytes = 0;
    } else {
      try {
        this.lastHostPayloadBytes = JSON.stringify(payload).length;
      } catch {
        this.lastHostPayloadBytes = null;
      }
    }

    // Record to DevTools
    this._devtools?.recordHostEvent(eventName, payload);

    void this.sendToSandbox({
      type: 'HOST_EVENT',
      eventName,
      payload: (payload ?? null) as SerializedValue,
    });
  }

  /**
   * Update configuration
   */
  updateConfig(config: Record<string, SerializedValue>): void {
    this.config = { ...this.config, ...config };
    void this.sendToSandbox({
      type: 'CONFIG_UPDATE',
      config,
    });
  }

  /**
   * Create Receiver
   */
  createReceiver(onUpdate: () => void): Receiver {
    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.id}] Creating Receiver`);
    }
    this.receiver = new Receiver(
      this.registry,
      (message) => this.sendToSandbox(message),
      onUpdate,
      { onMetric: this.options.onMetric, maxBatchSize: this.options.receiverMaxBatchSize }
    );
    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.id}] Receiver created`);
    }
    return this.receiver;
  }

  /**
   * Get Receiver
   */
  getReceiver(): Receiver | null {
    return this.receiver;
  }

  /**
   * Get component registry
   */
  getRegistry(): ComponentRegistry {
    return this.registry;
  }

  /**
   * Check if loaded
   */
  get isLoaded(): boolean {
    return this.loaded;
  }

  /**
   * Check if destroyed
   */
  get isDestroyed(): boolean {
    return this.destroyed;
  }

  /**
   * Get DevTools collector (if enabled)
   */
  get devtools(): RuntimeCollector | null {
    return this._devtools;
  }

  /**
   * Set maximum number of listeners per event before warning
   */
  setMaxListeners(n: number): void {
    this.maxListeners = n;
  }

  /**
   * Get current maximum listener threshold
   */
  getMaxListeners(): number {
    return this.maxListeners;
  }

  /**
   * Health snapshot for observability
   */
  getHealth(): EngineHealth {
    return {
      loaded: this.loaded,
      destroyed: this.destroyed,
      errorCount: this.errorCount,
      lastErrorAt: this.lastErrorAt,
      receiverNodes: this.receiver?.nodeCount ?? 0,
      batching: false,
    };
  }

  /**
   * Destroy engine and release all resources
   */
  destroy(): void {
    if (this.destroyed) return;

    this.destroyed = true;
    this.loaded = false;

    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.id}] Destroying engine`);
    }

    this.emit('destroy');

    // Clear all pending timers
    this.clearAllTimers();

    this.receiver?.clear();
    this.receiver = null;

    this.context?.dispose();
    this.context = null;
    this.runtime?.dispose();
    this.runtime = null;

    this.listeners.clear();

    // Clear DevTools
    if (this._devtools) {
      this._devtools.updateSandboxStatus({ state: 'destroyed' });
      this._devtools.disable();
      this._devtools.clear();
      this._devtools = null;
    }

    // Emit metric for engine destruction
    this.options.onMetric?.('engine.destroyed', 1, { engineId: this.id });
  }

  /**
   * Clear all pending timers (timeouts and intervals)
   */
  private clearAllTimers(): void {
    // Clear all timeouts
    for (const handle of this.timeoutMap.values()) {
      this.nativeClearTimeout?.(handle);
    }
    this.timeoutMap.clear();

    // Clear all intervals
    for (const handle of this.intervalMap.values()) {
      this.nativeClearInterval?.(handle);
    }
    this.intervalMap.clear();

    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.id}] Cleared all timers`);
    }
  }

  /**
   * Force destroy engine without emitting events.
   * Used for fatal error recovery (e.g., timeout) to prevent snowball effects.
   * This is more aggressive than destroy() - it doesn't emit 'destroy' event
   * to avoid potential callback execution during error recovery.
   */
  private forceDestroy(): void {
    if (this.destroyed) return;

    this.destroyed = true;
    this.loaded = false;

    // Don't emit 'destroy' event during force destroy to avoid callbacks

    // Clear all pending timers first
    this.clearAllTimers();

    this.receiver?.clear();
    this.receiver = null;

    try {
      this.context?.dispose();
    } catch {
      // Ignore errors during force dispose
    }
    this.context = null;

    try {
      this.runtime?.dispose();
    } catch {
      // Ignore errors during force dispose
    }
    this.runtime = null;

    // Keep listeners for fatalError handling, clear after
    // Give handlers a chance to process, then clear
    queueMicrotask(() => {
      this.listeners.clear();
    });

    // Clear DevTools
    if (this._devtools) {
      this._devtools.updateSandboxStatus({ state: 'destroyed' });
      this._devtools.disable();
      this._devtools.clear();
      this._devtools = null;
    }

    // Emit metric for engine destruction
    this.options.onMetric?.('engine.destroyed', 1, { engineId: this.id, forced: true });
  }

  /**
   * Get resource statistics for monitoring
   */
  getResourceStats(): { timers: number; nodes: number; callbacks: number } {
    // Get callback count from reconciler
    const callbackRegistry = this.sendToHostFn
      ? RillReconciler.getCallbackRegistry(this.sendToHostFn)
      : null;

    return {
      timers: this.timeoutMap.size + this.intervalMap.size,
      nodes: this.receiver?.nodeCount ?? 0,
      callbacks: callbackRegistry?.size ?? 0,
    };
  }

  getDiagnostics(): EngineDiagnostics {
    const now = Date.now();
    const cutoff = now - this.activityWindowMs;

    // Calculate ops and batches within stats window
    let windowOps = 0;
    let windowBatches = 0;
    for (let i = this.activitySamples.length - 1; i >= 0; i--) {
      const s = this.activitySamples[i]!;
      if (s.at < cutoff) break;
      windowOps += s.ops;
      windowBatches += 1;
    }

    const seconds = this.activityWindowMs / 1000;
    const opsPerSecond = seconds > 0 ? windowOps / seconds : 0;
    const batchesPerSecond = seconds > 0 ? windowBatches / seconds : 0;

    // Timeline (for trends/attribution): aggregate recent activityHistoryMs into fixed buckets
    const bucketMs = this.activityBucketMs;
    const bucketCount = Math.max(1, Math.ceil(this.activityHistoryMs / bucketMs));
    const timelineWindowMs = bucketCount * bucketMs;
    const timelineStart = now - timelineWindowMs;
    const timelineEnd = timelineStart + timelineWindowMs;

    const buckets = Array.from({ length: bucketCount }, () => ({
      ops: 0,
      batches: 0,
      skippedOps: 0,
      applyMsSum: 0,
      applyMsCount: 0,
      applyMsMax: 0,
    }));

    for (const s of this.activitySamples) {
      if (s.at < timelineStart) continue;
      const atForBucket = Math.min(s.at, timelineEnd - 1);
      if (atForBucket < timelineStart) continue;
      const idx = Math.floor((atForBucket - timelineStart) / bucketMs);
      if (idx < 0 || idx >= buckets.length) continue;
      const b = buckets[idx]!;
      b.ops += s.ops;
      b.batches += 1;
      b.skippedOps += s.skippedOps;
      if (s.applyDurationMs != null) {
        b.applyMsSum += s.applyDurationMs;
        b.applyMsCount += 1;
        b.applyMsMax = Math.max(b.applyMsMax, s.applyDurationMs);
      }
    }

    const timeline = {
      windowMs: timelineWindowMs,
      bucketMs,
      points: buckets.map((b, i) => ({
        at: timelineStart + (i + 1) * bucketMs,
        ops: b.ops,
        batches: b.batches,
        skippedOps: b.skippedOps,
        applyDurationMsAvg: b.applyMsCount > 0 ? b.applyMsSum / b.applyMsCount : null,
        applyDurationMsMax: b.applyMsCount > 0 ? b.applyMsMax : null,
      })),
    };

    return {
      id: this.id,
      health: this.getHealth(),
      resources: this.getResourceStats(),
      activity: {
        windowMs: this.activityWindowMs,
        opsPerSecond,
        batchesPerSecond,
        totalBatches: this.totalBatches,
        totalOps: this.totalOps,
        lastBatch: this.lastBatch,
        timeline,
      },
      receiver: this.receiver ? this.receiver.getStats() : null,
      host: {
        lastEventName: this.lastHostEventName,
        lastEventAt: this.lastHostEventAt,
        lastPayloadBytes: this.lastHostPayloadBytes,
      },
      guest: {
        lastEventName: this.lastGuestEventName,
        lastEventAt: this.lastGuestEventAt,
        lastPayloadBytes: this.lastGuestPayloadBytes,
        sleeping: this.guestSleeping,
        sleepingAt: this.guestSleepingAt,
      },
    };
  }
}
