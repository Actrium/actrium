/**
 * Rill Engine
 *
 * Sandbox engine core, responsible for managing QuickJS execution environment and lifecycle
 */

import type {
  OperationBatch,
  HostMessage,
  CallFunctionMessage,
  HostEventMessage,
  SerializedValue,
} from '../types';
import { ComponentRegistry, type ComponentMap } from './registry';
import { Receiver } from './receiver';

/**
 * Engine configuration options
 */
export interface EngineOptions {
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
}

/**
 * Engine event types
 */
export interface EngineEvents {
  load: () => void;
  error: (error: Error) => void;
  destroy: () => void;
  operation: (batch: OperationBatch) => void;
}

/**
 * Event listener
 */
type EventListener<T> = (data: T) => void;

/**
 * QuickJS interface (provided by react-native-quickjs)
 * Minimal interface definition; actual implementation depends on specific QuickJS binding library
 */
interface QuickJSContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface QuickJSRuntime {
  createContext(): QuickJSContext;
  dispose(): void;
}

/**
 * Rill Engine
 *
 * @example
 * ```typescript
 * const engine = new Engine();
 *
 * // Register custom components
 * engine.register({
 *   StepList: NativeStepList,
 *   CustomButton: MyButton,
 * });
 *
 * // Load plugin
 * await engine.loadBundle('https://cdn.example.com/plugin.js', {
 *   theme: 'dark',
 * });
 * ```
 */
export class Engine {
  private runtime: QuickJSRuntime | null = null;
  private context: QuickJSContext | null = null;
  private registry: ComponentRegistry;
  private receiver: Receiver | null = null;
  private config: Record<string, unknown> = {};
  private options: Required<EngineOptions>;
  private destroyed = false;
  private loaded = false;

  // Event listeners
  private listeners: Map<keyof EngineEvents, Set<EventListener<unknown>>> =
    new Map();

  constructor(options: EngineOptions = {}) {
    this.options = {
      timeout: options.timeout ?? 5000,
      debug: options.debug ?? false,
      logger: options.logger ?? console,
    };

    this.registry = new ComponentRegistry();
  }

  /**
   * Register custom components
   *
   * @param components Component map
   */
  register(components: ComponentMap): void {
    this.registry.registerAll(components);
  }

  /**
   * Load and execute plugin bundle
   *
   * @param source Bundle source (URL or code string)
   * @param initialProps Initial properties
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

      // Initialize QuickJS environment
      await this.initializeRuntime();

      // Execute bundle
      this.executeBundle(code);

      this.loaded = true;
      this.emit('load');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      this.emit('error', err);
      throw err;
    }
  }

  /**
   * Resolve bundle source
   */
  private async resolveSource(source: string): Promise<string> {
    // Fetch if URL
    if (source.startsWith('http://') || source.startsWith('https://')) {
      const response = await fetch(source);
      if (!response.ok) {
        throw new Error(`Failed to fetch bundle: ${response.status}`);
      }
      return response.text();
    }

    // Otherwise treat as code string
    return source;
  }

  /**
   * Initialize QuickJS runtime
   */
  private async initializeRuntime(): Promise<void> {
    // Note: Actual implementation requires react-native-quickjs
    // This provides mock implementation for type checking
    // Real usage requires:
    // import { QuickJS } from 'react-native-quickjs';
    // this.runtime = await QuickJS.createRuntime();

    // Mock implementation (for development/testing)
    this.runtime = this.createMockRuntime();
    this.context = this.runtime.createContext();

    // Inject polyfills
    this.injectPolyfills();

    // Inject runtime API
    this.injectRuntimeAPI();
  }

  /**
   * Create mock runtime (for development/testing)
   * Real environment should use react-native-quickjs
   */
  private createMockRuntime(): QuickJSRuntime {
    const globals: Record<string, unknown> = {};

    return {
      createContext: () => ({
        eval: (code: string) => {
          // In real environment, executed by QuickJS
          // Using eval here for testing only
          const fn = new Function(
            ...Object.keys(globals),
            `"use strict";\n${code}`
          );
          return fn(...Object.values(globals));
        },
        setGlobal: (name: string, value: unknown) => {
          globals[name] = value;
        },
        getGlobal: (name: string) => globals[name],
        dispose: () => {},
      }),
      dispose: () => {},
    };
  }

  /**
   * Inject polyfills
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
          logger.error('[Plugin] setTimeout callback error:', error);
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
          logger.error('[Plugin] setInterval callback error:', error);
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

    // Promise (natively supported by QuickJS)
    // No additional injection needed

    // queueMicrotask
    this.context.setGlobal('queueMicrotask', (fn: () => void) => {
      queueMicrotask(() => {
        try {
          fn();
        } catch (error) {
          logger.error('[Plugin] queueMicrotask callback error:', error);
        }
      });
    });
  }

  /**
   * Inject runtime API
   */
  private injectRuntimeAPI(): void {
    if (!this.context) return;

    // __sendToHost: Send operations to host
    this.context.setGlobal('__sendToHost', (batch: OperationBatch) => {
      if (this.options.debug) {
        this.options.logger.log('[rill] Operations:', batch);
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
        if (this.options.debug) {
          this.options.logger.log('[rill] Plugin event:', eventName, payload);
        }
        // Can be propagated externally via event system
      }
    );

    // __handleHostMessage: Handle messages from host
    // Called by sendToSandbox
    this.context.setGlobal(
      '__handleHostMessage',
      (message: HostMessage) => {
        this.handleHostMessage(message);
      }
    );
  }

  /**
   * Execute bundle code
   */
  private executeBundle(code: string): void {
    if (!this.context) {
      throw new Error('[rill] Context not initialized');
    }

    try {
      this.context.eval(code);
    } catch (error) {
      this.options.logger.error('[rill] Bundle execution error:', error);
      throw error;
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
      // Call callback handler in sandbox
      this.context.eval(
        `__invokeCallback("${message.fnId}", ${JSON.stringify(message.args)})`
      );
    } catch (error) {
      this.options.logger.error(
        `[rill] Failed to invoke callback ${message.fnId}:`,
        error
      );
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
      this.options.logger.error(
        `[rill] Failed to handle host event ${message.eventName}:`,
        error
      );
    }
  }

  /**
   * Send message to sandbox
   */
  sendToSandbox(message: HostMessage): void {
    if (this.destroyed || !this.context) return;

    this.context.eval(`__handleHostMessage(${JSON.stringify(message)})`);
  }

  /**
   * Emit event
   */
  emit<K extends keyof EngineEvents>(
    event: K,
    ...args: EngineEvents[K] extends () => void
      ? []
      : [Parameters<EngineEvents[K]>[0]]
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
    listener: EventListener<
      EngineEvents[K] extends () => void ? void : Parameters<EngineEvents[K]>[0]
    >
  ): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(listener as EventListener<unknown>);

    // Return unsubscribe function
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
      onUpdate
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
   * Destroy engine
   */
  destroy(): void {
    if (this.destroyed) return;

    this.destroyed = true;
    this.loaded = false;

    // Emit destroy event first
    this.emit('destroy');

    // Clean up Receiver
    this.receiver?.clear();
    this.receiver = null;

    // Clean up QuickJS
    this.context?.dispose();
    this.context = null;
    this.runtime?.dispose();
    this.runtime = null;

    // Clean up listeners last
    this.listeners.clear();
  }
}
