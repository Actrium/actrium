/**
 * Host-side DevTools Collector
 *
 * Collects debug data from the Host runtime (operation logs, performance)
 */

import { isSerializedFunction } from '../shared';
import type {
  HostMetrics,
  HostTreeNode,
  NodeId,
  OperationLogEntry,
  OperationRecord,
  SandboxStatus,
  Timestamp,
} from './types';

// ============ Configuration ============

export interface RuntimeCollectorConfig {
  /** Maximum operation logs to keep */
  maxLogs?: number;
  /** Maximum timeline events to keep */
  maxTimelineEvents?: number;
}

// ============ Timeline Event ============

export type TimelineEventType = 'batch' | 'callback' | 'event';

export interface TimelineEvent {
  type: TimelineEventType;
  timestamp: Timestamp;
  data: Record<string, unknown>;
  duration?: number | undefined;
}

// ============ Node Instance (from runtime) ============

export interface NodeInstance {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: number[];
}

// ============ Runtime Collector ============

export class RuntimeCollector {
  private config: Required<RuntimeCollectorConfig>;
  private logs: OperationLogEntry[] = [];
  private timeline: TimelineEvent[] = [];
  private startTime: Timestamp;
  private enabled = false;

  private sandboxStatus: SandboxStatus = {
    state: 'idle',
    errorCount: 0,
  };

  constructor(config: RuntimeCollectorConfig = {}) {
    this.config = {
      maxLogs: config.maxLogs ?? 100,
      maxTimelineEvents: config.maxTimelineEvents ?? 500,
    };
    this.startTime = Date.now();
  }

  // ============ Enable/Disable ============

  enable(): void {
    this.enabled = true;
  }

  disable(): void {
    this.enabled = false;
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  // ============ Operation Logging ============

  /**
   * Log an operation batch
   */
  logBatch(batch: { batchId: number; operations: OperationRecord[] }, duration?: number): void {
    if (!this.enabled) return;

    const entry: OperationLogEntry = {
      batchId: batch.batchId,
      timestamp: Date.now(),
      operations: [...batch.operations],
      duration,
    };

    this.logs.push(entry);
    while (this.logs.length > this.config.maxLogs) {
      this.logs.shift();
    }

    this.recordTimeline(
      'batch',
      { batchId: batch.batchId, operationCount: batch.operations.length },
      duration
    );
  }

  getLogs(): OperationLogEntry[] {
    return [...this.logs];
  }

  getRecentLogs(count: number): OperationLogEntry[] {
    return this.logs.slice(-count);
  }

  getOperationStats(): {
    totalLogs: number;
    totalOperations: number;
    operationCounts: Record<string, number>;
    avgOperationsPerBatch: number;
  } {
    const operationCounts: Record<string, number> = {};
    let totalOperations = 0;

    for (const log of this.logs) {
      totalOperations += log.operations.length;
      for (const op of log.operations) {
        operationCounts[op.op] = (operationCounts[op.op] || 0) + 1;
      }
    }

    return {
      totalLogs: this.logs.length,
      totalOperations,
      operationCounts,
      avgOperationsPerBatch: this.logs.length > 0 ? totalOperations / this.logs.length : 0,
    };
  }

  // ============ Timeline ============

  private recordTimeline(
    type: TimelineEventType,
    data: Record<string, unknown>,
    duration?: number
  ): void {
    if (!this.enabled) return;

    this.timeline.push({
      type,
      timestamp: Date.now() - this.startTime,
      data,
      duration,
    });

    while (this.timeline.length > this.config.maxTimelineEvents) {
      this.timeline.shift();
    }
  }

  recordCallback(fnId: string, args: unknown[]): void {
    if (!this.enabled) return;
    this.recordTimeline('callback', { fnId, argCount: args.length });
  }

  recordHostEvent(eventName: string, payload?: unknown): void {
    if (!this.enabled) return;
    this.recordTimeline('event', { eventName, hasPayload: payload !== undefined });
  }

  getTimeline(): TimelineEvent[] {
    return [...this.timeline];
  }

  getElapsedTime(): number {
    return Date.now() - this.startTime;
  }

  // ============ Component Tree ============

  /**
   * Build display tree from node map
   */
  buildTree(nodeMap: Map<NodeId, NodeInstance>, rootChildren: NodeId[]): HostTreeNode[] {
    return rootChildren
      .map((id) => this.buildNode(nodeMap, id, 0))
      .filter((node): node is HostTreeNode => node !== null);
  }

  private buildNode(
    nodeMap: Map<NodeId, NodeInstance>,
    id: NodeId,
    depth: number
  ): HostTreeNode | null {
    if (depth > 50) return null; // Safety limit

    const node = nodeMap.get(id);
    if (!node) return null;

    return {
      id: node.id,
      type: node.type,
      props: this.filterProps(node.props),
      children: node.children
        .map((childId) => this.buildNode(nodeMap, childId, depth + 1))
        .filter((child): child is HostTreeNode => child !== null),
      depth,
    };
  }

  private filterProps(props: Record<string, unknown>): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(props)) {
      if (typeof value === 'function') {
        continue; // Skip functions
      }

      // Use shared type guard to detect serialized functions
      // biome-ignore lint/suspicious/noExplicitAny: Type guard accepts any value for checking
      if (isSerializedFunction(value as any)) {
        result[key] = '[Callback]';
        continue;
      }

      result[key] = value;
    }

    return result;
  }

  /**
   * Format tree as text (for console output)
   */
  treeToText(tree: HostTreeNode[]): string {
    const lines: string[] = [];
    this.formatNodes(tree, lines, '');
    return lines.join('\n');
  }

  private formatNodes(nodes: HostTreeNode[], lines: string[], prefix: string): void {
    nodes.forEach((node, index) => {
      const isLast = index === nodes.length - 1;
      const connector = isLast ? '└─' : '├─';
      const propsStr = this.formatPropsString(node.props);

      lines.push(`${prefix}${connector} <${node.type}${propsStr}>`);

      const childPrefix = prefix + (isLast ? '   ' : '│  ');
      this.formatNodes(node.children, lines, childPrefix);
    });
  }

  private formatPropsString(props: Record<string, unknown>): string {
    const entries = Object.entries(props);
    if (entries.length === 0) return '';

    const parts = entries.slice(0, 3).map(([key, value]) => {
      if (typeof value === 'string') return `${key}="${value}"`;
      if (typeof value === 'number' || typeof value === 'boolean') return `${key}={${value}}`;
      return `${key}={...}`;
    });

    if (entries.length > 3) parts.push('...');
    return ` ${parts.join(' ')}`;
  }

  // ============ Sandbox Status ============

  updateSandboxStatus(status: Partial<SandboxStatus>): void {
    this.sandboxStatus = { ...this.sandboxStatus, ...status };
    if (status.state === 'running' && !this.sandboxStatus.startTime) {
      this.sandboxStatus.startTime = Date.now();
    }
    this.sandboxStatus.lastActivityTime = Date.now();
  }

  recordSandboxError(): void {
    this.sandboxStatus.errorCount++;
    this.sandboxStatus.lastActivityTime = Date.now();
  }

  getSandboxStatus(): SandboxStatus {
    return { ...this.sandboxStatus };
  }

  // ============ Metrics ============

  getMetrics(nodeMap?: Map<NodeId, NodeInstance>): HostMetrics {
    const recentLogs = this.getRecentLogs(10);
    const avgProcessingTime =
      recentLogs.length > 0
        ? recentLogs.reduce((sum, log) => sum + (log.duration || 0), 0) / recentLogs.length
        : 0;

    return {
      operationProcessingTime: avgProcessingTime,
      pendingOperations: 0,
      nodeCount: nodeMap?.size ?? 0,
    };
  }

  // ============ Reset ============

  clear(): void {
    this.logs = [];
    this.timeline = [];
    this.startTime = Date.now();
  }
}

/**
 * Create a new Runtime collector instance
 */
export function createRuntimeCollector(config?: RuntimeCollectorConfig): RuntimeCollector {
  return new RuntimeCollector(config);
}
