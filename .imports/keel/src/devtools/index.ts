/**
 * Rill DevTools
 *
 * Provides component tree inspection, performance monitoring, and debugging tools
 */

import type { NodeInstance, Operation, OperationBatch } from '../types';

/**
 * Component tree node display info
 */
export interface TreeNode {
  /** Node ID */
  id: number;
  /** Component type */
  type: string;
  /** Props (filtered for sensitive info) */
  props: Record<string, unknown>;
  /** Children */
  children: TreeNode[];
  /** Expanded state */
  expanded?: boolean;
  /** Depth */
  depth: number;
  /** Highlighted state */
  highlighted?: boolean;
}

/**
 * Component tree inspector configuration
 */
export interface InspectorConfig {
  /**
   * Maximum display depth
   * @default 10
   */
  maxDepth?: number;

  /**
   * Filtered property names (hide sensitive info)
   * @default ['style']
   */
  filterProps?: string[];

  /**
   * Show function type properties
   * @default false
   */
  showFunctions?: boolean;

  /**
   * Enable highlighting changed nodes
   * @default true
   */
  highlightChanges?: boolean;
}

/**
 * Component Tree Inspector
 *
 * Converts node map to visualizable tree structure
 */
export class ComponentInspector {
  private config: Required<InspectorConfig>;
  private changedNodeIds = new Set<number>();
  private changeHighlightDuration = 1000;

  constructor(config: InspectorConfig = {}) {
    this.config = {
      maxDepth: config.maxDepth ?? 10,
      filterProps: config.filterProps ?? [],
      showFunctions: config.showFunctions ?? false,
      highlightChanges: config.highlightChanges ?? true,
    };
  }

  /**
   * Build component tree from node map
   */
  buildTree(
    nodeMap: Map<number, NodeInstance>,
    rootChildren: number[]
  ): TreeNode[] {
    return rootChildren
      .map((id) => this.buildNode(nodeMap, id, 0))
      .filter((node): node is TreeNode => node !== null);
  }

  /**
   * Build single node
   */
  private buildNode(
    nodeMap: Map<number, NodeInstance>,
    id: number,
    depth: number
  ): TreeNode | null {
    if (depth > this.config.maxDepth) {
      return null;
    }

    const node = nodeMap.get(id);
    if (!node) return null;

    const filteredProps = this.filterProps(node.props);

    return {
      id: node.id,
      type: node.type,
      props: filteredProps,
      children: node.children
        .map((childId) => this.buildNode(nodeMap, childId, depth + 1))
        .filter((child): child is TreeNode => child !== null),
      depth,
      highlighted: this.changedNodeIds.has(id),
    };
  }

  /**
   * Filter props
   */
  private filterProps(
    props: Record<string, unknown>
  ): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(props)) {
      // Skip filtered properties
      if (this.config.filterProps.includes(key)) {
        result[key] = '[filtered]';
        continue;
      }

      // Handle function types
      if (typeof value === 'function') {
        if (this.config.showFunctions) {
          result[key] = '[Function]';
        }
        continue;
      }

      // Handle serialized function markers
      if (
        typeof value === 'object' &&
        value !== null &&
        '__type' in value &&
        (value as { __type: string }).__type === 'function'
      ) {
        result[key] = '[Callback]';
        continue;
      }

      result[key] = value;
    }

    return result;
  }

  /**
   * Record changed node
   */
  recordChange(nodeId: number): void {
    if (!this.config.highlightChanges) return;

    this.changedNodeIds.add(nodeId);

    // Auto-clear highlight
    setTimeout(() => {
      this.changedNodeIds.delete(nodeId);
    }, this.changeHighlightDuration);
  }

  /**
   * Record changes from operation batch
   */
  recordBatchChanges(batch: OperationBatch): void {
    for (const op of batch.operations) {
      if ('id' in op) {
        this.recordChange(op.id);
      }
    }
  }

  /**
   * Clear all highlights
   */
  clearHighlights(): void {
    this.changedNodeIds.clear();
  }

  /**
   * Generate text representation of tree (for console output)
   */
  toText(tree: TreeNode[]): string {
    const lines: string[] = [];
    this.formatNode(tree, lines, '');
    return lines.join('\n');
  }

  /**
   * Format node as text
   */
  private formatNode(
    nodes: TreeNode[],
    lines: string[],
    prefix: string
  ): void {
    nodes.forEach((node, index) => {
      const isLast = index === nodes.length - 1;
      const connector = isLast ? '└─' : '├─';
      const highlight = node.highlighted ? ' *' : '';

      // Format props
      const propsStr = this.formatPropsString(node.props);

      lines.push(`${prefix}${connector} <${node.type}${propsStr}>${highlight}`);

      const childPrefix = prefix + (isLast ? '   ' : '│  ');
      this.formatNode(node.children, lines, childPrefix);
    });
  }

  /**
   * Format props string
   */
  private formatPropsString(props: Record<string, unknown>): string {
    const entries = Object.entries(props);
    if (entries.length === 0) return '';

    const parts = entries.slice(0, 3).map(([key, value]) => {
      if (typeof value === 'string') {
        return `${key}="${value}"`;
      }
      if (typeof value === 'number' || typeof value === 'boolean') {
        return `${key}={${value}}`;
      }
      return `${key}={...}`;
    });

    if (entries.length > 3) {
      parts.push('...');
    }

    return ' ' + parts.join(' ');
  }

  /**
   * Generate JSON representation of tree
   */
  toJSON(tree: TreeNode[]): string {
    return JSON.stringify(tree, null, 2);
  }
}

/**
 * Operation log entry
 */
export interface OperationLog {
  /** Batch ID */
  batchId: number;
  /** Timestamp */
  timestamp: number;
  /** Operation list */
  operations: Operation[];
  /** Duration (if available) */
  duration?: number;
}

/**
 * Operation Logger
 */
export class OperationLogger {
  private logs: OperationLog[] = [];
  private maxLogs: number;
  private enabled = true;

  constructor(maxLogs = 100) {
    this.maxLogs = maxLogs;
  }

  /**
   * Enable/disable logging
   */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }

  /**
   * Log batch
   */
  log(batch: OperationBatch, duration?: number): void {
    if (!this.enabled) return;

    this.logs.push({
      batchId: batch.batchId,
      timestamp: Date.now(),
      operations: [...batch.operations],
      duration,
    });

    // Keep log count limit
    while (this.logs.length > this.maxLogs) {
      this.logs.shift();
    }
  }

  /**
   * Get all logs
   */
  getLogs(): OperationLog[] {
    return [...this.logs];
  }

  /**
   * Get recent N logs
   */
  getRecentLogs(count: number): OperationLog[] {
    return this.logs.slice(-count);
  }

  /**
   * Filter logs by operation type
   */
  filterByType(
    type: Operation['op']
  ): Array<{ log: OperationLog; operation: Operation }> {
    const results: Array<{ log: OperationLog; operation: Operation }> = [];

    for (const log of this.logs) {
      for (const op of log.operations) {
        if (op.op === type) {
          results.push({ log, operation: op });
        }
      }
    }

    return results;
  }

  /**
   * Filter logs by node ID
   */
  filterByNodeId(
    nodeId: number
  ): Array<{ log: OperationLog; operation: Operation }> {
    const results: Array<{ log: OperationLog; operation: Operation }> = [];

    for (const log of this.logs) {
      for (const op of log.operations) {
        if ('id' in op && op.id === nodeId) {
          results.push({ log, operation: op });
        }
      }
    }

    return results;
  }

  /**
   * Get statistics
   */
  getStats(): {
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
      avgOperationsPerBatch:
        this.logs.length > 0 ? totalOperations / this.logs.length : 0,
    };
  }

  /**
   * Clear logs
   */
  clear(): void {
    this.logs = [];
  }

  /**
   * Export as JSON
   */
  export(): string {
    return JSON.stringify(
      {
        exportTime: new Date().toISOString(),
        stats: this.getStats(),
        logs: this.logs,
      },
      null,
      2
    );
  }
}

/**
 * Timeline event types
 */
export type TimelineEventType =
  | 'mount'
  | 'update'
  | 'unmount'
  | 'batch'
  | 'callback'
  | 'event';

/**
 * Timeline event
 */
export interface TimelineEvent {
  /** Event type */
  type: TimelineEventType;
  /** Timestamp */
  timestamp: number;
  /** Event data */
  data: Record<string, unknown>;
  /** Duration */
  duration?: number;
}

/**
 * Timeline Recorder
 *
 * Records component lifecycle and render events
 */
export class TimelineRecorder {
  private events: TimelineEvent[] = [];
  private maxEvents: number;
  private enabled = true;
  private startTime: number;

  constructor(maxEvents = 500) {
    this.maxEvents = maxEvents;
    this.startTime = Date.now();
  }

  /**
   * Enable/disable recording
   */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }

  /**
   * Record event
   */
  record(
    type: TimelineEventType,
    data: Record<string, unknown>,
    duration?: number
  ): void {
    if (!this.enabled) return;

    this.events.push({
      type,
      timestamp: Date.now() - this.startTime,
      data,
      duration,
    });

    while (this.events.length > this.maxEvents) {
      this.events.shift();
    }
  }

  /**
   * Record mount event
   */
  recordMount(nodeId: number, type: string): void {
    this.record('mount', { nodeId, type });
  }

  /**
   * Record update event
   */
  recordUpdate(nodeId: number, changedProps: string[]): void {
    this.record('update', { nodeId, changedProps });
  }

  /**
   * Record unmount event
   */
  recordUnmount(nodeId: number): void {
    this.record('unmount', { nodeId });
  }

  /**
   * Record batch processing
   */
  recordBatch(batchId: number, operationCount: number, duration: number): void {
    this.record('batch', { batchId, operationCount }, duration);
  }

  /**
   * Record callback invocation
   */
  recordCallback(fnId: string, args: unknown[]): void {
    this.record('callback', { fnId, argCount: args.length });
  }

  /**
   * Record host event
   */
  recordHostEvent(eventName: string, payload?: unknown): void {
    this.record('event', {
      eventName,
      hasPayload: payload !== undefined,
    });
  }

  /**
   * Get all events
   */
  getEvents(): TimelineEvent[] {
    return [...this.events];
  }

  /**
   * Get events in time range
   */
  getEventsInRange(startMs: number, endMs: number): TimelineEvent[] {
    return this.events.filter(
      (e) => e.timestamp >= startMs && e.timestamp <= endMs
    );
  }

  /**
   * Get events by type
   */
  getEventsByType(type: TimelineEventType): TimelineEvent[] {
    return this.events.filter((e) => e.type === type);
  }

  /**
   * Get total elapsed time
   */
  getElapsedTime(): number {
    return Date.now() - this.startTime;
  }

  /**
   * Reset timeline
   */
  reset(): void {
    this.events = [];
    this.startTime = Date.now();
  }

  /**
   * Export timeline data
   */
  export(): string {
    return JSON.stringify(
      {
        exportTime: new Date().toISOString(),
        startTime: new Date(this.startTime).toISOString(),
        elapsedTime: this.getElapsedTime(),
        eventCount: this.events.length,
        events: this.events,
      },
      null,
      2
    );
  }
}

/**
 * DevTools Main Class
 *
 * Integrates all debugging tools
 */
export class DevTools {
  readonly inspector: ComponentInspector;
  readonly logger: OperationLogger;
  readonly timeline: TimelineRecorder;

  private enabled = false;

  constructor(config?: {
    inspector?: InspectorConfig;
    maxLogs?: number;
    maxTimelineEvents?: number;
  }) {
    this.inspector = new ComponentInspector(config?.inspector);
    this.logger = new OperationLogger(config?.maxLogs);
    this.timeline = new TimelineRecorder(config?.maxTimelineEvents);
  }

  /**
   * Enable DevTools
   */
  enable(): void {
    this.enabled = true;
    this.logger.setEnabled(true);
    this.timeline.setEnabled(true);
  }

  /**
   * Disable DevTools
   */
  disable(): void {
    this.enabled = false;
    this.logger.setEnabled(false);
    this.timeline.setEnabled(false);
  }

  /**
   * Check if enabled
   */
  isEnabled(): boolean {
    return this.enabled;
  }

  /**
   * Handle operation batch
   */
  onBatch(batch: OperationBatch, duration?: number): void {
    if (!this.enabled) return;

    this.logger.log(batch, duration);
    this.inspector.recordBatchChanges(batch);
    this.timeline.recordBatch(batch.batchId, batch.operations.length, duration ?? 0);

    // Record individual operations
    for (const op of batch.operations) {
      switch (op.op) {
        case 'CREATE':
          this.timeline.recordMount(op.id, op.type);
          break;
        case 'UPDATE':
          this.timeline.recordUpdate(
            op.id,
            Object.keys(op.props)
          );
          break;
        case 'DELETE':
          this.timeline.recordUnmount(op.id);
          break;
      }
    }
  }

  /**
   * Handle callback invocation
   */
  onCallback(fnId: string, args: unknown[]): void {
    if (!this.enabled) return;
    this.timeline.recordCallback(fnId, args);
  }

  /**
   * Handle host event
   */
  onHostEvent(eventName: string, payload?: unknown): void {
    if (!this.enabled) return;
    this.timeline.recordHostEvent(eventName, payload);
  }

  /**
   * Get component tree
   */
  getComponentTree(
    nodeMap: Map<number, NodeInstance>,
    rootChildren: number[]
  ): TreeNode[] {
    return this.inspector.buildTree(nodeMap, rootChildren);
  }

  /**
   * Get component tree text representation
   */
  getComponentTreeText(
    nodeMap: Map<number, NodeInstance>,
    rootChildren: number[]
  ): string {
    const tree = this.getComponentTree(nodeMap, rootChildren);
    return this.inspector.toText(tree);
  }

  /**
   * Export all debug data
   */
  exportAll(): string {
    return JSON.stringify(
      {
        exportTime: new Date().toISOString(),
        logs: JSON.parse(this.logger.export()),
        timeline: JSON.parse(this.timeline.export()),
      },
      null,
      2
    );
  }

  /**
   * Reset all data
   */
  reset(): void {
    this.logger.clear();
    this.timeline.reset();
    this.inspector.clearHighlights();
  }
}

/**
 * Create DevTools instance
 */
export function createDevTools(config?: {
  inspector?: InspectorConfig;
  maxLogs?: number;
  maxTimelineEvents?: number;
}): DevTools {
  return new DevTools(config);
}
