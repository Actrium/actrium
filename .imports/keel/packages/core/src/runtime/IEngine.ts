/**
 * IEngine - Common interface for Engine implementations
 *
 * Each Engine instance owns a dedicated JS runtime/thread.
 * Create a new Engine for each isolated execution context needed.
 */

import type { OperationBatch, SerializedValue } from '../types';
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
   * 统计窗口（毫秒），用于计算 ops/s 与 batch/s
   */
  windowMs: number;
  /**
   * 最近窗口内 ops/秒
   */
  opsPerSecond: number;
  /**
   * 最近窗口内 batch/秒
   */
  batchesPerSecond: number;
  /**
   * 累计接收的 batch 数量
   */
  totalBatches: number;
  /**
   * 累计接收的 ops 数量（按 batch.operations.length 计）
   */
  totalOps: number;
  /**
   * 最近一次 batch 信息
   */
  lastBatch: {
    batchId: number;
    at: number;
    totalOps: number;
    applyDurationMs: number | null;
  } | null;

  /**
   * 活动时间线（用于 Host 侧趋势图/归因提示）
   * - points 为等宽桶聚合（ops/batch/skip/apply 耗时）
   * - 适合直接用于 sparkline / bar chart
   */
  timeline?: EngineActivityTimeline;
}

export interface EngineActivityTimelinePoint {
  /**
   * 桶结束时间戳（ms）
   */
  at: number;
  /**
   * 桶内 ops 总数
   */
  ops: number;
  /**
   * 桶内 batch 数量
   */
  batches: number;
  /**
   * 桶内被 Receiver 跳过的 ops 数量（背压信号）
   */
  skippedOps: number;
  /**
   * 桶内 applyBatch 平均耗时（ms），无样本则为 null
   */
  applyDurationMsAvg: number | null;
  /**
   * 桶内 applyBatch 最大耗时（ms），无样本则为 null
   */
  applyDurationMsMax: number | null;
}

export interface EngineActivityTimeline {
  /**
   * 时间线覆盖窗口（ms）
   */
  windowMs: number;
  /**
   * 单个时间桶宽度（ms）
   */
  bucketMs: number;
  /**
   * 从旧到新排列的桶
   */
  points: EngineActivityTimelinePoint[];
}

/**
 * Engine 诊断快照（用于 Host 侧“任务管理器/资源监视器”）
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
