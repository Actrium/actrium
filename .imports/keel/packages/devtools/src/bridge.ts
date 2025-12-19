/**
 * DevTools Bridge
 *
 * Connects Host RuntimeCollector with Guest debug messages
 */

import type {
  DevToolsEvent,
  DevToolsEventType,
  EventHandler,
  GuestDebugMessage,
  GuestError,
  HostMetrics,
  HostTreeNode,
  ProfilingReport,
  ProfilingSession,
  SandboxStatus,
  Unsubscribe,
  ConsoleEntry,
  OperationLogEntry,
} from './types';
import type { RuntimeCollector, NodeInstance } from './runtime';

// ============ Bridge ============

export class DevToolsBridge {
  private enabled = false;
  private runtimeCollector: RuntimeCollector | null = null;
  private guestReady = false;

  // Guest data
  private guestConsoleLogs: ConsoleEntry[] = [];
  private guestErrors: GuestError[] = [];

  // Subscriptions
  private subscribers = new Map<DevToolsEventType, Set<EventHandler>>();

  // Profiling
  private profilingSession: ProfilingSession | null = null;

  // Node map reference (from runtime)
  private nodeMapRef: Map<number, NodeInstance> | null = null;
  private rootChildrenRef: number[] = [];

  // ============ Setup ============

  /**
   * Connect runtime collector
   */
  connectRuntime(
    collector: RuntimeCollector,
    nodeMap: Map<number, NodeInstance>,
    rootChildren: number[]
  ): void {
    this.runtimeCollector = collector;
    this.nodeMapRef = nodeMap;
    this.rootChildrenRef = rootChildren;
  }

  /**
   * Disconnect runtime collector
   */
  disconnectRuntime(): void {
    this.runtimeCollector = null;
    this.nodeMapRef = null;
    this.rootChildrenRef = [];
  }

  enable(): void {
    this.enabled = true;
    this.runtimeCollector?.enable();
  }

  disable(): void {
    this.enabled = false;
    this.runtimeCollector?.disable();
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  // ============ Guest Message Handling ============

  /**
   * Handle message from guest
   */
  handleGuestMessage(message: GuestDebugMessage): void {
    if (!this.enabled) return;

    switch (message.type) {
      case '__DEVTOOLS_READY__':
        this.guestReady = true;
        break;

      case '__DEVTOOLS_CONSOLE__':
        this.guestConsoleLogs.push(message.entry);
        if (this.guestConsoleLogs.length > 100) {
          this.guestConsoleLogs.shift();
        }
        this.emit('console', message.entry);
        break;

      case '__DEVTOOLS_ERROR__':
        this.guestErrors.push(message.error);
        if (this.guestErrors.length > 50) {
          this.guestErrors.shift();
        }
        this.emit('error', message.error);

        if (this.profilingSession) {
          this.profilingSession.errors.push(message.error);
        }
        break;

      case '__DEVTOOLS_RENDER__':
        // Record timings in profiling if active
        if (this.profilingSession && message.timings) {
          this.profilingSession.guestRenders.push(...message.timings);
        }

        this.emit('render', {
          timings: message.timings,
          commitDuration: message.commitDuration,
        });
        break;
    }
  }

  // ============ Subscription ============

  subscribe<T = unknown>(event: DevToolsEventType, handler: EventHandler<T>): Unsubscribe {
    if (!this.subscribers.has(event)) {
      this.subscribers.set(event, new Set());
    }
    this.subscribers.get(event)!.add(handler as EventHandler);

    return () => {
      this.subscribers.get(event)?.delete(handler as EventHandler);
    };
  }

  private emit(type: DevToolsEventType, data: unknown): void {
    const event: DevToolsEvent = {
      type,
      timestamp: Date.now(),
      data,
    };

    const handlers = this.subscribers.get(type);
    if (handlers) {
      for (const handler of handlers) {
        try {
          handler(event);
        } catch (e) {
          console.error('[DevTools] Event handler error:', e);
        }
      }
    }
  }

  // ============ Data Access ============

  /**
   * Get host component tree (built from nodeMap)
   */
  getHostTree(): HostTreeNode[] {
    if (!this.runtimeCollector || !this.nodeMapRef) return [];
    return this.runtimeCollector.buildTree(this.nodeMapRef, this.rootChildrenRef);
  }

  getHostMetrics(): HostMetrics | null {
    if (!this.runtimeCollector) return null;
    return this.runtimeCollector.getMetrics(this.nodeMapRef ?? undefined);
  }

  getSandboxStatus(): SandboxStatus | null {
    return this.runtimeCollector?.getSandboxStatus() ?? null;
  }

  getOperationLogs(): OperationLogEntry[] {
    return this.runtimeCollector?.getLogs() ?? [];
  }

  getConsoleLogs(): ConsoleEntry[] {
    return [...this.guestConsoleLogs];
  }

  getErrors(): GuestError[] {
    return [...this.guestErrors];
  }

  isGuestReady(): boolean {
    return this.guestReady;
  }

  // ============ Profiling ============

  startProfiling(): void {
    this.profilingSession = {
      startTime: Date.now(),
      hostOperations: [],
      guestRenders: [],
      errors: [],
    };
  }

  stopProfiling(): ProfilingReport | null {
    if (!this.profilingSession) return null;

    const session = this.profilingSession;
    session.endTime = Date.now();
    this.profilingSession = null;

    const duration = session.endTime - session.startTime;

    // Calculate node render stats
    const nodeStats = new Map<number, { type: string; totalTime: number; count: number }>();
    for (const render of session.guestRenders) {
      const existing = nodeStats.get(render.nodeId) ?? {
        type: render.type,
        totalTime: 0,
        count: 0,
      };
      existing.totalTime += render.duration;
      existing.count++;
      nodeStats.set(render.nodeId, existing);
    }

    const slowestNodes = Array.from(nodeStats.entries())
      .map(([nodeId, stats]) => ({
        nodeId,
        type: stats.type,
        avgTime: stats.totalTime / stats.count,
        count: stats.count,
      }))
      .sort((a, b) => b.avgTime - a.avgTime)
      .slice(0, 10);

    // Build timeline
    const timeline: ProfilingReport['timeline'] = [
      ...session.hostOperations.map((op) => ({
        timestamp: op.timestamp,
        type: 'operation' as const,
        data: op,
      })),
      ...session.guestRenders.map((r) => ({
        timestamp: r.timestamp,
        type: 'render' as const,
        data: r,
      })),
      ...session.errors.map((e) => ({
        timestamp: e.timestamp,
        type: 'error' as const,
        data: e,
      })),
    ].sort((a, b) => a.timestamp - b.timestamp);

    const totalOperations = session.hostOperations.reduce(
      (sum, log) => sum + log.operations.length,
      0
    );
    const avgOperationTime =
      session.hostOperations.length > 0
        ? session.hostOperations.reduce((sum, log) => sum + (log.duration ?? 0), 0) /
          session.hostOperations.length
        : 0;
    const avgRenderTime =
      session.guestRenders.length > 0
        ? session.guestRenders.reduce((sum, r) => sum + r.duration, 0) / session.guestRenders.length
        : 0;

    return {
      duration,
      summary: {
        totalOperations,
        totalRenders: session.guestRenders.length,
        avgOperationTime,
        avgRenderTime,
        slowestNodes,
        errorCount: session.errors.length,
      },
      timeline,
    };
  }

  isProfiling(): boolean {
    return this.profilingSession !== null;
  }

  /**
   * Record operation batch (called by host collector hook)
   */
  recordOperationBatch(batch: OperationLogEntry): void {
    if (this.profilingSession) {
      this.profilingSession.hostOperations.push(batch);
    }
    this.emit('operation', batch);
  }

  // ============ Export ============

  export(): string {
    return JSON.stringify(
      {
        exportTime: new Date().toISOString(),
        host: {
          tree: this.getHostTree(),
          metrics: this.getHostMetrics(),
          sandboxStatus: this.getSandboxStatus(),
          operationLogs: this.getOperationLogs(),
        },
        guest: {
          ready: this.guestReady,
          consoleLogs: this.guestConsoleLogs,
          errors: this.guestErrors,
        },
      },
      null,
      2
    );
  }

  // ============ Reset ============

  clear(): void {
    this.runtimeCollector?.clear();
    this.guestConsoleLogs = [];
    this.guestErrors = [];
    this.profilingSession = null;
  }

  reset(): void {
    this.clear();
    this.guestReady = false;
  }
}

/**
 * Create a new DevTools bridge
 */
export function createBridge(): DevToolsBridge {
  return new DevToolsBridge();
}
