/**
 * Rill Engine
 *
 * Sandbox engine core, responsible for managing QuickJS execution environment and lifecycle.
 * Uses react-native-quickjs for sandboxed JavaScript execution.
 */

import * as React from 'react';
import * as ReactJSXRuntime from 'react/jsx-runtime';

// Lazy-load react-native to avoid bundler issues in non-RN environments (e.g., Bun tests)
let ReactNative: typeof import('react-native') | undefined;
function getReactNative(): typeof import('react-native') {
  if (!ReactNative) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      ReactNative = require('react-native');
    } catch {
      throw new Error('[rill] react-native is not available in this environment');
    }
  }
  return ReactNative!;
}

import * as RillReconciler from '../reconciler';

import type {
  OperationBatch,
  HostMessage,
  CallFunctionMessage,
  HostEventMessage,
  SerializedValue,
} from '../types';
import { ComponentRegistry } from './registry';
import type { ComponentMap } from './registry';
import { Receiver } from './receiver';

import type { IEngine, EngineEvents, GuestMessage, EngineHealth } from './IEngine';

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
}

// Re-export from IEngine for backward compatibility
export type { IEngine, EngineEvents, GuestMessage, EngineHealth } from './IEngine';

/**
 * Event listener
 */
type EventListener<T> = (data: T) => void;

/**
 * QuickJS interface (provided by react-native-quickjs)
 */
export interface JSEngineContext {
  eval(code: string): unknown;
  evalAsync?(code: string): Promise<unknown>;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;

  /**
   * Set an interrupt handler for execution budget control.
   * The handler is called periodically during execution.
   * Return `true` to interrupt execution, `false` to continue.
   * Not all providers support this - check capabilities before use.
   */
  setInterruptHandler?(handler: () => boolean): void;

  /**
   * Clear the interrupt handler.
   */
  clearInterruptHandler?(): void;
}

export interface JSEngineRuntime {
  createContext(): JSEngineContext;
  dispose(): void;
}

/**
 * QuickJS provider interface
 * Allows injection of QuickJS runtime implementation
 */
export interface JSEngineProvider {
  createRuntime(): Promise<JSEngineRuntime> | JSEngineRuntime;
}

/** Error types for better classification */
export class RequireError extends Error { constructor(message: string) { super(message); this.name = 'RequireError'; } }
export class ExecutionError extends Error { constructor(message: string) { super(message); this.name = 'ExecutionError'; } }
export class TimeoutError extends Error { constructor(message: string) { super(message); this.name = 'TimeoutError'; } }

/**
 * Rill Engine - Standalone JS sandbox engine
 *
 * For multi-tenant scenarios with shared worker pools, use PooledEngine instead.
 *
 * @example
 * ```typescript
 * const engine = new Engine({ debug: true });
 * engine.register({ StepList: NativeStepList });
 * await engine.loadBundle(bundleCode);
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
  private readonly engineId: number;
  private _timeoutTimer?: ReturnType<typeof setTimeout>;

  // Event listeners
  private listeners: Map<keyof EngineEvents, Set<EventListener<unknown>>> = new Map();

  // Memory leak detection for Engine events
  private maxListeners = 10;
  private warnedEvents = new Set<keyof EngineEvents>();

  constructor(options: EngineOptions = {}) {
    const defaultWhitelist = new Set(['react', 'react-native', 'react/jsx-runtime', 'rill/reconciler']);
    // Provide a safe fallback logger if console is not available
    const defaultLogger = typeof console !== 'undefined' ? console : {
      log: () => {},
      warn: () => {},
      error: () => {},
      info: () => {},
      debug: () => {},
    };
    this.options = {
      provider: options.provider,
      timeout: options.timeout ?? 5000,
      debug: options.debug ?? false,
      logger: options.logger ?? defaultLogger,
      requireWhitelist: new Set(options.requireWhitelist ?? Array.from(defaultWhitelist)),
      onMetric: options.onMetric,
      receiverMaxBatchSize: options.receiverMaxBatchSize ?? 5000,
    };

    this.registry = new ComponentRegistry();

    // Initialize JS engine provider
    // Priority: explicit provider > DefaultJSEngineProvider auto-detect
    if (!this.options.provider) {
      try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const { DefaultJSEngineProvider } = require('./DefaultJSEngineProvider');
        this.options.provider = DefaultJSEngineProvider.create({
          timeout: this.options.timeout,
          sandbox: options.sandbox,
        });
        if (this.options.debug) this.options.logger.log('[rill] Initialized DefaultJSEngineProvider');
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
              setGlobal: (name: string, value: unknown) => { isolatedScope[name] = value; },
              getGlobal: (name: string) => isolatedScope[name],
              dispose: () => {},
            }),
            dispose: () => {},
          }),
        };
      }
    }
    
    this.engineId = ++engineIdCounter;

    if (this.options.debug) {
      this.options.logger.log(`[rill] Engine #${this.engineId} created with QuickJS sandbox`);
    }
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
  async loadBundle(
    source: string,
    initialProps?: Record<string, unknown>
  ): Promise<void> {
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
        this.options.logger.log(`[rill] Engine #${this.engineId} Bundle loaded, length:`, code.length);
        this.options.logger.log(`[rill] Engine #${this.engineId} Bundle preview:`, code.substring(0, 200));
      }

      // Initialize sandbox and execute
      await this.initializeRuntime();

      // Execute with timeout protection
      // Note: Hard timeout enforcement depends on the provider:
      // - VMProvider: Uses vm.Script timeout (hard interrupt)
      // - WorkerJSEngineProvider: Uses QuickJS executionTimeout (hard interrupt)
      // - RNQuickJSProvider: Depends on native package implementation
      // - NoSandboxProvider: No timeout support (dev-only)
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
            this.options.logger.error(`[rill] Fatal: Bundle execution exceeded timeout ${timeout}ms, destroying engine`);
            const error = new TimeoutError(`[rill] Execution exceeded timeout ${timeout}ms (hard limit)`);

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
          await Promise.race([
            this.executeBundle(code),
            timeoutPromise,
          ]);
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
      this.emit('load');

      if (this.options.debug) {
        this.options.logger.log(`[rill] Engine #${this.engineId} ✅ Bundle executed successfully`);
      }
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      this.errorCount += 1;
      this.lastErrorAt = Date.now();
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
      this.options.onMetric?.('engine.fetchBundle', Date.now() - s1, { status: 200, size: text.length });
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
    if (!this.options.provider) {
      throw new Error('[rill] QuickJS provider not initialized');
    }
    this.runtime = await this.options.provider.createRuntime();
    this.context = this.runtime.createContext();

    this.injectPolyfills();
    await this.injectRuntimeAPI();

    const dur = Date.now() - start;
    this.options.onMetric?.('engine.initializeRuntime', dur);
  }

  /**
   * Inject polyfills into sandbox
   */
  private injectPolyfills(): void {
    if (!this.context) return;

    const logger = this.options.logger;
    const debug = this.options.debug;

    // Save native timer functions to avoid recursion issues (with fallbacks for test environments)
    const nativeSetTimeout = typeof globalThis.setTimeout === 'function'
      ? globalThis.setTimeout.bind(globalThis)
      : (fn: () => void, _ms?: number) => { Promise.resolve().then(fn); return 0; };
    const nativeClearTimeout = typeof globalThis.clearTimeout === 'function'
      ? globalThis.clearTimeout.bind(globalThis)
      : () => {};
    const nativeSetInterval = typeof globalThis.setInterval === 'function'
      ? globalThis.setInterval.bind(globalThis)
      : () => 0;
    const nativeClearInterval = typeof globalThis.clearInterval === 'function'
      ? globalThis.clearInterval.bind(globalThis)
      : () => {};
    const nativeQueueMicrotask = typeof globalThis.queueMicrotask === 'function'
      ? globalThis.queueMicrotask.bind(globalThis)
      : (fn: () => void) => Promise.resolve().then(fn);

    // Inject React and ReactJSXRuntime as global variables
    // This is required for bundles compiled with rill CLI
    this.context.setGlobal('React', React);
    this.context.setGlobal('ReactJSXRuntime', ReactJSXRuntime);

    // console
    this.context.setGlobal('console', {
      log: (...args: unknown[]) => {
        if (debug) logger.log('[Guest]', ...args);
      },
      warn: (...args: unknown[]) => logger.warn('[Guest]', ...args),
      error: (...args: unknown[]) => logger.error('[Guest]', ...args),
      debug: (...args: unknown[]) => {
        if (debug) logger.log('[Guest:debug]', ...args);
      },
      info: (...args: unknown[]) => {
        if (debug) logger.log('[Guest:info]', ...args);
      },
    });

    // require: module loader for Guest code
    this.context.setGlobal('require', (moduleName: string) => {
      if (debug) {
        logger.log('[rill:require]', moduleName);
      }

      if (!this.options.requireWhitelist.has(moduleName)) {
        throw new RequireError(`[rill] Unsupported require("${moduleName}")`);
      }

      switch (moduleName) {
        case 'react':
          return React;
        case 'react-native':
          return ReactNative;
        case 'react/jsx-runtime':
          return ReactJSXRuntime;
        case 'rill/reconciler':
          return RillReconciler;
        default:
          throw new Error(`[rill] Unsupported require("${moduleName}")`);
      }
    });

    // setTimeout / clearTimeout
    const timeoutMap = new Map<number, NodeJS.Timeout>();
    let timeoutId = 0;

    this.context.setGlobal('setTimeout', (fn: () => void, delay: number) => {
      const id = ++timeoutId;
      const handle = nativeSetTimeout(() => {
        timeoutMap.delete(id);
        try {
          fn();
        } catch (error) {
          logger.error('[Guest] setTimeout error:', error);
        }
      }, delay);
      timeoutMap.set(id, handle as NodeJS.Timeout);
      return id;
    });

    this.context.setGlobal('clearTimeout', (id: number) => {
      const handle = timeoutMap.get(id);
      if (handle) {
        nativeClearTimeout(handle as any);
        timeoutMap.delete(id);
      }
    });

    // setInterval / clearInterval
    const intervalMap = new Map<number, NodeJS.Timeout>();
    let intervalId = 0;

    this.context.setGlobal('setInterval', (fn: () => void, delay: number) => {
      const id = ++intervalId;
      const handle = nativeSetInterval(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Guest] setInterval error:', error);
        }
      }, delay);
      intervalMap.set(id, handle as NodeJS.Timeout);
      return id;
    });

    this.context.setGlobal('clearInterval', (id: number) => {
      const handle = intervalMap.get(id);
      if (handle) {
        nativeClearInterval(handle as any);
        intervalMap.delete(id);
      }
    });

    // queueMicrotask
    this.context.setGlobal('queueMicrotask', (fn: () => void) => {
      nativeQueueMicrotask(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Guest] queueMicrotask error:', error);
        }
      });
    });
  }

  /**
   * Inject runtime API into sandbox
   */
  private async injectRuntimeAPI(): Promise<void> {
    if (!this.context) return;

    const debug = this.options.debug;
    const logger = this.options.logger;

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
    this.context.setGlobal('__sendToHost', (batch: OperationBatch) => {
      if (debug) {
        logger.log(`[rill] Engine #${this.engineId} __sendToHost called, operations:`, batch.operations.length);
      }
      this.emit('operation', batch);
      if (this.receiver) {
        if (debug) {
          logger.log(`[rill] Engine #${this.engineId} applying batch to receiver`);
        }
        this.receiver.applyBatch(batch);
      } else {
        logger.warn(`[rill] Engine #${this.engineId} has no receiver to apply batch!`);
      }
    });

    // __getConfig: Get initial configuration
    this.context.setGlobal('__getConfig', () => this.config);

    // __sendEventToHost: Send event to host
    this.context.setGlobal(
      '__sendEventToHost',
      (eventName: string, payload?: unknown) => {
        if (debug) {
          logger.log('[rill] Guest event:', eventName, payload);
        }
        this.emit('message', { event: eventName, payload });
      }
    );

    // __handleHostMessage: Handle messages from host
    this.context.setGlobal(
      '__handleHostMessage',
      (message: HostMessage) => {
        this.handleHostMessage(message);
      }
    );
  }

  /**
   * Helper to evaluate code - uses evalAsync if available (for Worker providers),
   * otherwise falls back to sync eval
   */
  private async evalCode(code: string): Promise<void> {
    if (!this.context) return;
    if (this.context.evalAsync) { // Check evalAsync directly on context
      await this.context.evalAsync(code);
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

    try {
      await this.evalCode(
        `__invokeCallback("${message.fnId}", ${JSON.stringify(message.args)})`
      );
    } catch (error) {
      this.options.logger.error(`[rill] Failed to invoke callback ${message.fnId}:`, error);
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
    this.options.onMetric?.('engine.sendToSandbox', Date.now() - start, { size: JSON.stringify(message).length });
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
  sendEvent(eventName: string, payload?: SerializedValue): void {
    // Fire-and-forget for backward compatibility, but still awaits internally for Worker providers
    void this.sendToSandbox({
      type: 'HOST_EVENT',
      eventName,
      payload: payload ?? null,
    });
  }

  /**
   * Update configuration
   */
  updateConfig(config: Record<string, SerializedValue>): void {
    this.config = { ...this.config, ...config };
    // Fire-and-forget for backward compatibility, but still awaits internally for Worker providers
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
      this.options.logger.log(`[rill] Engine #${this.engineId} creating Receiver`);
    }
    this.receiver = new Receiver(
      this.registry,
      (message) => this.sendToSandbox(message),
      onUpdate,
      { onMetric: this.options.onMetric, maxBatchSize: this.options.receiverMaxBatchSize }
    );
    if (this.options.debug) {
      this.options.logger.log(`[rill] Engine #${this.engineId} Receiver created`);
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
   * Destroy engine
   */
  destroy(): void {
    if (this.destroyed) return;

    this.destroyed = true;
    this.loaded = false;

    this.emit('destroy');

    this.receiver?.clear();
    this.receiver = null;

    this.context?.dispose();
    this.context = null;
    this.runtime?.dispose();
    this.runtime = null;

    this.listeners.clear();
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
  }
}
