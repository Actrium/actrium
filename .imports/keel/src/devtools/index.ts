/**
 * @rill/devtools
 *
 * Debug tools for rill - operation logging, performance profiling, and error tracking
 *
 * Guest-side data is automatically collected by:
 * - DEVTOOLS_SHIM (console/error interception, injected by Engine)
 * - Reconciler (render timing, integrated in @rill/let)
 *
 * Usage:
 *
 * ```typescript
 * import { createDevTools } from '@rill/devtools';
 *
 * const devtools = createDevTools();
 * devtools.enable();
 *
 * // Connect to engine
 * devtools.connectEngine(engine);
 *
 * // Subscribe to events
 * devtools.subscribe('error', (event) => {
 *   console.log('Guest error:', event.data);
 * });
 *
 * // Profile performance
 * devtools.startProfiling();
 * // ... user interactions ...
 * const report = devtools.stopProfiling();
 * ```
 */

// ============ Types ============

export type {
  // Common
  Timestamp,
  NodeId,
  // Host
  HostTreeNode,
  OperationLogEntry,
  OperationRecord,
  HostMetrics,
  SandboxStatus,
  // Guest
  ConsoleEntry,
  GuestError,
  ReconcilerRenderTiming,
  // Bridge messages
  GuestDebugMessage,
  // Events
  DevToolsEventType,
  DevToolsEvent,
  EventHandler,
  Unsubscribe,
  // Profiling
  ProfilingSession,
  ProfilingReport,
} from './types';

// ============ Runtime Collector ============

export type {
  RuntimeCollectorConfig,
  TimelineEvent,
  TimelineEventType,
  NodeInstance,
} from './runtime';
export { RuntimeCollector, createRuntimeCollector } from './runtime';

// ============ Bridge ============

export { DevToolsBridge, createBridge } from './bridge';

// ============ Main API ============

import { createRuntimeCollector, type RuntimeCollectorConfig, type NodeInstance } from './runtime';
import { createBridge } from './bridge';
import type {
  DevToolsEventType,
  EventHandler,
  Unsubscribe,
  ProfilingReport,
  GuestDebugMessage,
  HostTreeNode,
  HostMetrics,
  SandboxStatus,
  ConsoleEntry,
  GuestError,
  OperationLogEntry,
} from './types';

export interface DevToolsConfig {
  runtime?: RuntimeCollectorConfig;
}

/**
 * Main DevTools API
 */
export class DevTools {
  private bridge = createBridge();
  private runtimeCollector = createRuntimeCollector();
  private enabled = false;

  constructor(config?: DevToolsConfig) {
    if (config?.runtime) {
      this.runtimeCollector = createRuntimeCollector(config.runtime);
    }
  }

  // ============ Lifecycle ============

  enable(): void {
    this.enabled = true;
    this.runtimeCollector.enable();
    this.bridge.enable();
  }

  disable(): void {
    this.enabled = false;
    this.runtimeCollector.disable();
    this.bridge.disable();
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  // ============ Engine Connection ============

  /**
   * Connect to rill Engine
   */
  connectEngine(engine: {
    getNodeMap(): Map<number, NodeInstance>;
    getRootChildren(): number[];
    on?(event: string, handler: (...args: unknown[]) => void): void;
  }): void {
    const nodeMap = engine.getNodeMap();
    const rootChildren = engine.getRootChildren();

    this.bridge.connectRuntime(this.runtimeCollector, nodeMap, rootChildren);

    // Hook into engine events if available
    if (engine.on) {
      engine.on('batch', (...args: unknown[]) => {
        const batch = args[0] as { batchId: number; operations: unknown[] };
        const duration = args[1] as number | undefined;
        this.runtimeCollector.logBatch(
          batch as Parameters<typeof this.runtimeCollector.logBatch>[0],
          duration
        );
        this.bridge.recordOperationBatch({
          batchId: batch.batchId,
          timestamp: Date.now(),
          operations: batch.operations as OperationLogEntry['operations'],
          duration,
        });
      });

      engine.on('callback', (...args: unknown[]) => {
        const fnId = args[0] as string;
        const callbackArgs = args[1] as unknown[];
        this.runtimeCollector.recordCallback(fnId, callbackArgs);
      });

      engine.on('hostEvent', (...args: unknown[]) => {
        const eventName = args[0] as string;
        const payload = args[1];
        this.runtimeCollector.recordHostEvent(eventName, payload);
      });

      engine.on('guestMessage', (...args: unknown[]) => {
        const message = args[0];
        if (
          typeof message === 'object' &&
          message !== null &&
          'type' in message &&
          typeof (message as { type: string }).type === 'string' &&
          (message as { type: string }).type.startsWith('__DEVTOOLS_')
        ) {
          this.bridge.handleGuestMessage(message as GuestDebugMessage);
        }
      });

      engine.on('sandboxStateChange', (...args: unknown[]) => {
        const state = args[0] as SandboxStatus['state'];
        this.runtimeCollector.updateSandboxStatus({ state });
      });

      engine.on('sandboxError', () => {
        this.runtimeCollector.recordSandboxError();
      });
    }
  }

  disconnectEngine(): void {
    this.bridge.disconnectRuntime();
  }

  handleGuestMessage(message: GuestDebugMessage): void {
    this.bridge.handleGuestMessage(message);
  }

  // ============ Subscription ============

  subscribe<T = unknown>(event: DevToolsEventType, handler: EventHandler<T>): Unsubscribe {
    return this.bridge.subscribe(event, handler);
  }

  // ============ Data Access ============

  getHostTree(): HostTreeNode[] {
    return this.bridge.getHostTree();
  }

  getHostMetrics(): HostMetrics | null {
    return this.bridge.getHostMetrics();
  }

  getSandboxStatus(): SandboxStatus | null {
    return this.bridge.getSandboxStatus();
  }

  getOperationLogs(): OperationLogEntry[] {
    return this.bridge.getOperationLogs();
  }

  getConsoleLogs(): ConsoleEntry[] {
    return this.bridge.getConsoleLogs();
  }

  getErrors(): GuestError[] {
    return this.bridge.getErrors();
  }

  isGuestReady(): boolean {
    return this.bridge.isGuestReady();
  }

  // ============ Profiling ============

  startProfiling(): void {
    this.bridge.startProfiling();
  }

  stopProfiling(): ProfilingReport | null {
    return this.bridge.stopProfiling();
  }

  isProfiling(): boolean {
    return this.bridge.isProfiling();
  }

  // ============ Export ============

  export(): string {
    return this.bridge.export();
  }

  getHostTreeText(): string {
    const tree = this.getHostTree();
    return this.runtimeCollector.treeToText(tree);
  }

  // ============ Reset ============

  clear(): void {
    this.runtimeCollector.clear();
    this.bridge.clear();
  }

  reset(): void {
    this.runtimeCollector.clear();
    this.bridge.reset();
  }
}

/**
 * Create a new DevTools instance
 */
export function createDevTools(config?: DevToolsConfig): DevTools {
  return new DevTools(config);
}

// ============ Global Instance ============

let globalDevTools: DevTools | null = null;

export function getDevTools(config?: DevToolsConfig): DevTools {
  if (!globalDevTools) {
    globalDevTools = createDevTools(config);
  }
  return globalDevTools;
}

export function resetDevTools(): void {
  if (globalDevTools) {
    globalDevTools.reset();
    globalDevTools = null;
  }
}
