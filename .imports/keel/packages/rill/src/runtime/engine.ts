/**
 * Rill Engine
 *
 * Sandbox engine core, responsible for managing QuickJS execution environment and lifecycle.
 * Uses react-native-quickjs for sandboxed JavaScript execution.
 */

import * as React from 'react';
import * as ReactNative from 'react-native';
import * as ReactJSXRuntime from 'react/jsx-runtime';

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

/**
 * Engine configuration options
 */
export interface EngineOptions {
  /**
   * QuickJS provider for creating sandbox runtime
   * Required - must be provided by the host application
   */
  quickjs: QuickJSProvider;

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

/**
 * Engine event types
 */
/**
 * Message from plugin to host
 */
export interface PluginMessage {
  event: string;
  payload: unknown;
}

export interface EngineEvents {
  load: () => void;
  error: (error: Error) => void;
  destroy: () => void;
  operation: (batch: OperationBatch) => void;
  message: (message: PluginMessage) => void;
}

/**
 * Event listener
 */
type EventListener<T> = (data: T) => void;

/**
 * QuickJS interface (provided by react-native-quickjs)
 */
export interface QuickJSContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

export interface QuickJSRuntime {
  createContext(): QuickJSContext;
  dispose(): void;
}

/**
 * QuickJS provider interface
 * Allows injection of QuickJS runtime implementation
 */
export interface QuickJSProvider {
  createRuntime(): Promise<QuickJSRuntime> | QuickJSRuntime;
}

/** Error types for better classification */
export class RequireError extends Error { constructor(message: string) { super(message); this.name = 'RequireError'; } }
export class ExecutionError extends Error { constructor(message: string) { super(message); this.name = 'ExecutionError'; } }
export class TimeoutError extends Error { constructor(message: string) { super(message); this.name = 'TimeoutError'; } }

/**
 * Rill Engine
 *
 * @example
 * ```typescript
 * const engine = new Engine({ debug: true });
 * engine.register({ StepList: NativeStepList });
 * await engine.loadBundle(bundleCode);
 * ```
 */
export class Engine {
  private runtime: QuickJSRuntime | null = null;
  private context: QuickJSContext | null = null;
  private registry: ComponentRegistry;
  private receiver: Receiver | null = null;
  private config: Record<string, unknown> = {};
  private options: Required<Omit<EngineOptions, 'logger' | 'requireWhitelist' | 'onMetric' | 'receiverMaxBatchSize'>> & { logger: NonNullable<EngineOptions['logger']>, requireWhitelist: ReadonlySet<string>, onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void, receiverMaxBatchSize: number };
  private destroyed = false;
  private loaded = false;
  private errorCount = 0;
  private lastErrorAt: number | null = null;

  // Event listeners
  private listeners: Map<keyof EngineEvents, Set<EventListener<unknown>>> = new Map();

  constructor(options: EngineOptions) {
    if (!options.quickjs) {
      throw new Error('[rill] QuickJS provider is required');
    }

    const defaultWhitelist = new Set(['react', 'react-native', 'react/jsx-runtime', 'rill/reconciler']);
    this.options = {
      quickjs: options.quickjs,
      timeout: options.timeout ?? 5000,
      debug: options.debug ?? false,
      logger: options.logger ?? console,
      requireWhitelist: new Set(options.requireWhitelist ?? Array.from(defaultWhitelist)),
      onMetric: options.onMetric,
      receiverMaxBatchSize: options.receiverMaxBatchSize ?? 5000,
    };

    this.registry = new ComponentRegistry();

    if (this.options.debug) {
      this.options.logger.log('[rill] Engine created with QuickJS sandbox');
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
   * Load and execute plugin bundle
   */
  async loadBundle(
    source: string,
    initialProps?: Record<string, unknown>
  ): Promise<void> {
    if (this.destroyed) {
      throw new Error('[rill] Engine has been destroyed');
    }

    if (this.loaded) {
      throw new Error('[rill] Engine already loaded a bundle');
    }

    this.config = initialProps ?? {};

    try {
      // Get bundle code
      const code = await this.resolveSource(source);

      if (this.options.debug) {
        this.options.logger.log('[rill] Bundle loaded, length:', code.length);
        this.options.logger.log('[rill] Bundle preview:', code.substring(0, 200));
      }

      // Initialize sandbox and execute
      await this.initializeRuntime();

      // Best-effort timeout guard: only effective if eval yields to event loop
      if (this.options.timeout > 0) {
        let timedOut = false;
        const timer = setTimeout(() => {
          timedOut = true;
          this.options.logger.error('[rill] Bundle execution timeout');
        }, this.options.timeout);
        try {
          this.executeBundle(code);
          if (timedOut) {
            throw new TimeoutError(`[rill] Execution exceeded timeout ${this.options.timeout}ms`);
          }
        } finally {
          clearTimeout(timer);
        }
      } else {
        this.executeBundle(code);
      }

      this.loaded = true;
      this.emit('load');

      if (this.options.debug) {
        this.options.logger.log('[rill] ✅ Bundle executed successfully');
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
    this.runtime = await this.options.quickjs.createRuntime();
    this.context = this.runtime.createContext();

    this.injectPolyfills();
    this.injectRuntimeAPI();

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

    // console
    this.context.setGlobal('console', {
      log: (...args: unknown[]) => {
        if (debug) logger.log('[Plugin]', ...args);
      },
      warn: (...args: unknown[]) => logger.warn('[Plugin]', ...args),
      error: (...args: unknown[]) => logger.error('[Plugin]', ...args),
      debug: (...args: unknown[]) => {
        if (debug) logger.log('[Plugin:debug]', ...args);
      },
      info: (...args: unknown[]) => {
        if (debug) logger.log('[Plugin:info]', ...args);
      },
    });

    // require: module loader for plugin bundle
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
    const timeoutMap = new Map<number, ReturnType<typeof setTimeout>>();
    let timeoutId = 0;

    this.context.setGlobal('setTimeout', (fn: () => void, delay: number) => {
      const id = ++timeoutId;
      const handle = setTimeout(() => {
        timeoutMap.delete(id);
        try {
          fn();
        } catch (error) {
          logger.error('[Plugin] setTimeout error:', error);
        }
      }, delay);
      timeoutMap.set(id, handle);
      return id;
    });

    this.context.setGlobal('clearTimeout', (id: number) => {
      const handle = timeoutMap.get(id);
      if (handle) {
        clearTimeout(handle);
        timeoutMap.delete(id);
      }
    });

    // setInterval / clearInterval
    const intervalMap = new Map<number, ReturnType<typeof setInterval>>();
    let intervalId = 0;

    this.context.setGlobal('setInterval', (fn: () => void, delay: number) => {
      const id = ++intervalId;
      const handle = setInterval(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Plugin] setInterval error:', error);
        }
      }, delay);
      intervalMap.set(id, handle);
      return id;
    });

    this.context.setGlobal('clearInterval', (id: number) => {
      const handle = intervalMap.get(id);
      if (handle) {
        clearInterval(handle);
        intervalMap.delete(id);
      }
    });

    // queueMicrotask
    this.context.setGlobal('queueMicrotask', (fn: () => void) => {
      queueMicrotask(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Plugin] queueMicrotask error:', error);
        }
      });
    });
  }

  /**
   * Inject runtime API into sandbox
   */
  private injectRuntimeAPI(): void {
    if (!this.context) return;

    const debug = this.options.debug;
    const logger = this.options.logger;

    // __sendToHost: Send operations to host
    this.context.setGlobal('__sendToHost', (batch: OperationBatch) => {
      if (debug) {
        logger.log('[rill] __sendToHost called, operations:', batch.operations.length);
      }
      this.emit('operation', batch);
      this.receiver?.applyBatch(batch);
    });

    // __getConfig: Get initial configuration
    this.context.setGlobal('__getConfig', () => this.config);

    // __sendEventToHost: Send event to host
    this.context.setGlobal(
      '__sendEventToHost',
      (eventName: string, payload?: unknown) => {
        if (debug) {
          logger.log('[rill] Plugin event:', eventName, payload);
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
   * Execute bundle code in sandbox
   */
  private executeBundle(code: string): void {
    const start = Date.now();
    if (!this.context) {
      throw new Error('[rill] Context not initialized');
    }

    try {
      this.context.eval(code);
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
  private handleHostMessage(message: HostMessage): void {
    switch (message.type) {
      case 'CALL_FUNCTION':
        this.handleCallFunction(message);
        break;
      case 'HOST_EVENT':
        this.handleHostEvent(message);
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
  private handleCallFunction(message: CallFunctionMessage): void {
    if (!this.context) return;

    try {
      this.context.eval(
        `__invokeCallback("${message.fnId}", ${JSON.stringify(message.args)})`
      );
    } catch (error) {
      this.options.logger.error(`[rill] Failed to invoke callback ${message.fnId}:`, error);
    }
  }

  /**
   * Handle host event
   */
  private handleHostEvent(message: HostEventMessage): void {
    if (!this.context) return;

    try {
      this.context.eval(
        `__handleHostEvent("${message.eventName}", ${JSON.stringify(message.payload)})`
      );
    } catch (error) {
      this.options.logger.error(`[rill] Failed to handle host event ${message.eventName}:`, error);
    }
  }

  /**
   * Send message to sandbox
   */
  sendToSandbox(message: HostMessage): void {
    if (this.destroyed || !this.context) return;
    const start = Date.now();
    this.context.eval(`__handleHostMessage(${JSON.stringify(message)})`);
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
    listener: EventListener<EngineEvents[K] extends () => void ? void : Parameters<EngineEvents[K]>[0]>
  ): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(listener as EventListener<unknown>);

    return () => {
      this.listeners.get(event)?.delete(listener as EventListener<unknown>);
    };
  }

  /**
   * Send event to sandbox plugin
   */
  sendEvent(eventName: string, payload?: SerializedValue): void {
    this.sendToSandbox({
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
    this.sendToSandbox({
      type: 'CONFIG_UPDATE',
      config,
    });
  }

  /**
   * Create Receiver
   */
  createReceiver(onUpdate: () => void): Receiver {
    this.receiver = new Receiver(
      this.registry,
      (message) => this.sendToSandbox(message),
      onUpdate,
      { onMetric: this.options.onMetric, maxBatchSize: this.options.receiverMaxBatchSize }
    );
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
   * Health snapshot for observability
   */
  getHealth(): { loaded: boolean; destroyed: boolean; errorCount: number; lastErrorAt: number | null; receiverNodes: number } {
    return {
      loaded: this.loaded,
      destroyed: this.destroyed,
      errorCount: this.errorCount,
      lastErrorAt: this.lastErrorAt,
      receiverNodes: this.receiver?.nodeCount ?? 0,
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
}
