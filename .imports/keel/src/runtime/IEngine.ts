/**
 * IEngine - Common interface for Engine implementations
 *
 * Each Engine instance owns a dedicated JS runtime/thread.
 * Create a new Engine for each isolated execution context needed.
 */

import type { OperationBatch, SerializedValue } from './types';
import type { Receiver, ReceiverStats } from './receiver';
import type { ComponentMap, ComponentRegistry } from './registry';

/**
 * Message from guest to host
 */
export interface GuestMessage {
  event: string;
  payload: unknown;
}

/**
 * DevTools console entry (from Guest)
 */
export interface DevToolsConsoleEntry {
  level: 'log' | 'info' | 'warn' | 'error' | 'debug';
  args: unknown[];
  timestamp: number;
  stack?: string;
}

/**
 * DevTools error entry (from Guest)
 */
export interface DevToolsError {
  message: string;
  stack?: string;
  timestamp: number;
  fatal: boolean;
}

/**
 * Engine event types
 */
export interface EngineEvents {
  load: () => void;
  error: (error: Error) => void;
  destroy: () => void;
  operation: (batch: OperationBatch) => void;
  message: (message: GuestMessage) => void;
  /**
   * Fatal error event - emitted when the engine encounters an unrecoverable error
   * such as execution timeout. The engine will be automatically destroyed after this event.
   */
  fatalError: (error: Error) => void;
  /**
   * DevTools: Console log from Guest sandbox
   */
  devtoolsConsole: (entry: DevToolsConsoleEntry) => void;
  /**
   * DevTools: Error from Guest sandbox
   */
  devtoolsError: (error: DevToolsError) => void;
  /**
   * DevTools: Guest devtools is ready
   */
  devtoolsReady: (data: Record<string, unknown>) => void;
}

/**
 * Health snapshot for observability
 */
export interface EngineHealth {
  loaded: boolean;
  destroyed: boolean;
  errorCount: number;
  lastErrorAt: number | null;
  receiverNodes: number;
  batching: boolean;
}

/**
 * Resource statistics for monitoring
 */
export interface ResourceStats {
  timers: number;
  nodes: number;
  callbacks: number;
}

export interface EngineActivityStats {
  /**
   * Stats window (ms), used for calculating ops/s and batch/s
   */
  windowMs: number;
  /**
   * ops/s within recent window
   */
  opsPerSecond: number;
  /**
   * batch/s within recent window
   */
  batchesPerSecond: number;
  /**
   * Total batches received
   */
  totalBatches: number;
  /**
   * Total ops received (by batch.operations.length)
   */
  totalOps: number;
  /**
   * Last batch info
   */
  lastBatch: {
    batchId: number;
    at: number;
    totalOps: number;
    applyDurationMs: number | null;
  } | null;

  /**
   * Activity timeline (for Host-side trend charts/attribution hints)
   * - points are fixed-width bucket aggregations (ops/batch/skip/apply duration)
   * - suitable for sparkline / bar chart
   */
  timeline?: EngineActivityTimeline;
}

export interface EngineActivityTimelinePoint {
  /**
   * Bucket end timestamp (ms)
   */
  at: number;
  /**
   * Total ops in bucket
   */
  ops: number;
  /**
   * Batch count in bucket
   */
  batches: number;
  /**
   * Ops skipped by Receiver in bucket (backpressure signal)
   */
  skippedOps: number;
  /**
   * Average applyBatch duration in bucket (ms), null if no samples
   */
  applyDurationMsAvg: number | null;
  /**
   * Max applyBatch duration in bucket (ms), null if no samples
   */
  applyDurationMsMax: number | null;
}

export interface EngineActivityTimeline {
  /**
   * Timeline coverage window (ms)
   */
  windowMs: number;
  /**
   * Single time bucket width (ms)
   */
  bucketMs: number;
  /**
   * Buckets sorted from oldest to newest
   */
  points: EngineActivityTimelinePoint[];
}

/**
 * Engine diagnostics snapshot (for Host-side "Task Manager/Resource Monitor")
 */
export interface EngineDiagnostics {
  id: string;
  health: EngineHealth;
  resources: ResourceStats;
  activity: EngineActivityStats;
  receiver: ReceiverStats | null;
  host: {
    lastEventName: string | null;
    lastEventAt: number | null;
    lastPayloadBytes: number | null;
  };
  guest: {
    lastEventName: string | null;
    lastEventAt: number | null;
    lastPayloadBytes: number | null;
    sleeping: boolean | null;
    sleepingAt: number | null;
  };
}

/**
 * Common engine interface
 */
export interface IEngine {
  /**
   * Unique engine identifier
   */
  readonly id: string;

  /**
   * Register custom components
   */
  register(components: ComponentMap): void;

  /**
   * Load and execute Guest code
   */
  loadBundle(source: string, initialProps?: Record<string, unknown>): Promise<void>;

  /**
   * Subscribe to engine events
   * @returns Unsubscribe function
   */
  on<K extends keyof EngineEvents>(
    event: K,
    listener: EngineEvents[K] extends () => void
      ? () => void
      : (data: Parameters<EngineEvents[K]>[0]) => void
  ): () => void;

  /**
   * Send event to sandbox guest
   */
  sendEvent(eventName: string, payload?: unknown): void;

  /**
   * Update configuration
   */
  updateConfig(config: Record<string, SerializedValue>): void;

  /**
   * Create Receiver for rendering
   */
  createReceiver(onUpdate: () => void): Receiver;

  /**
   * Get current Receiver
   */
  getReceiver(): Receiver | null;

  /**
   * Get component registry
   */
  getRegistry(): ComponentRegistry;

  /**
   * Check if bundle is loaded
   */
  readonly isLoaded: boolean;

  /**
   * Check if engine is destroyed
   */
  readonly isDestroyed: boolean;

  /**
   * Get health snapshot for observability
   */
  getHealth(): EngineHealth;

  /**
   * Get resource statistics for monitoring
   */
  getResourceStats(): ResourceStats;

  /**
   * Get diagnostic snapshot for host-side monitoring UI
   */
  getDiagnostics(): EngineDiagnostics;

  /**
   * Set maximum number of listeners per event before warning
   * @param n - Maximum listener count (default: 10)
   */
  setMaxListeners(n: number): void;

  /**
   * Get current maximum listener threshold
   */
  getMaxListeners(): number;

  /**
   * Destroy engine and release resources
   */
  destroy(): void;
}
