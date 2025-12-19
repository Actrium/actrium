/**
 * Instruction Receiver
 *
 * Parses operation instructions from sandbox and builds React Native component tree
 */

import React from 'react';

// Augment globalThis for debug tracking properties
declare global {
  // eslint-disable-next-line no-var
  var __RECEIVER_APPEND_CALLS: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_PARENT_IDS: number[] | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_DETAILS:
    | Array<{ parentId: number; childId: number; parentExists: boolean }>
    | undefined;
  // eslint-disable-next-line no-var
  var __TOUCHABLE_RENDER_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __LAST_TOUCHABLE_HAS_ONPRESS: boolean | undefined;
  // eslint-disable-next-line no-var
  var __RECEIVER_FUNCTION_COUNT: number | undefined;
  // eslint-disable-next-line no-var
  var __LAST_FUNCTION_FNID: string | undefined;
}

// Polyfill queueMicrotask if not available
const safeQueueMicrotask =
  typeof queueMicrotask !== 'undefined'
    ? queueMicrotask
    : (callback: () => void) => Promise.resolve().then(callback);

import { hasCallback, invokeCallback } from '../../let/reconciler/index';
import type {
  HostMessage,
  NodeInstance,
  Operation,
  OperationBatch,
  SerializedFunction,
  SerializedProps,
} from './types';
import type { ComponentRegistry } from './registry';

/**
 * Message send function type
 */
export type SendToSandbox = (message: HostMessage) => void | Promise<void>;

/**
 * Check if value is serialized function
 */
function isSerializedFunction(value: unknown): value is SerializedFunction {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__type' in value &&
    (value as SerializedFunction).__type === 'function'
  );
}

/**
 * Instruction Receiver
 */
export interface ReceiverOptions {
  onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
  maxBatchSize?: number;
  debug?: boolean;
  /**
   * Attribution window (aggregate recent N seconds) for trend attribution ("who/why consuming").
   * @default 5000
   */
  attributionWindowMs?: number;
  /**
   * Attribution history retention (ms), older entries are discarded.
   * @default 60000
   */
  attributionHistoryMs?: number;
  /**
   * Maximum attribution history samples (batch count), for memory limit fallback.
   * @default 200
   */
  attributionMaxSamples?: number;
}

export interface ReceiverApplyStats {
  batchId?: number;
  total: number;
  applied: number;
  skipped: number;
  failed: number;
  at: number;
  durationMs: number;

  /**
   * Node count change before/after batch (for "who is growing")
   */
  nodesBefore: number;
  nodesAfter: number;
  nodeDelta: number;

  /**
   * Operation distribution for this batch (applied only)
   */
  opCounts: Record<string, number>;

  /**
   * Skipped operation distribution (only when backpressure/limit triggered)
   */
  skippedOpCounts: Record<string, number>;

  /**
   * Top N node types touched in this batch (for attribution)
   */
  topNodeTypes: Array<{ type: string; ops: number }>;
  topNodeTypesSkipped: Array<{ type: string; ops: number }>;
}

export interface ReceiverStats {
  nodeCount: number;
  rootChildrenCount: number;
  renderCount: number;
  lastRenderAt: number | null;
  lastRenderDurationMs: number | null;
  totalBatches: number;
  totalOps: {
    received: number;
    applied: number;
    skipped: number;
    failed: number;
  };
  lastApply: ReceiverApplyStats | null;
  attribution: ReceiverAttributionWindow | null;
}

export type ReceiverAttributionWorstKind = 'largest' | 'slowest' | 'mostSkipped' | 'mostGrowth';

export interface ReceiverAttributionWorstBatch {
  kind: ReceiverAttributionWorstKind;
  batchId?: number;
  at: number;
  total: number;
  applied: number;
  skipped: number;
  failed: number;
  durationMs: number;
  nodeDelta: number;
}

export interface ReceiverAttributionWindow {
  /**
   * Time window covered by this aggregation (ms)
   */
  windowMs: number;
  /**
   * Number of batch samples within the window
   */
  sampleCount: number;
  /**
   * Total ops in window (received)
   */
  total: number;
  applied: number;
  skipped: number;
  failed: number;
  /**
   * Sum of applyBatch duration within window (ms)
   */
  durationMs: number;
  /**
   * Sum of node delta within window (rough indicator of tree growing/shrinking)
   */
  nodeDelta: number;
  /**
   * Operation distribution within window (applied aggregation)
   */
  opCounts: Record<string, number>;
  /**
   * Skipped operation distribution within window (skipped aggregation)
   */
  skippedOpCounts: Record<string, number>;
  /**
   * Top N node types touched within window (applied aggregation)
   */
  topNodeTypes: Array<{ type: string; ops: number }>;
  /**
   * Top N node types touched within window (skipped aggregation)
   */
  topNodeTypesSkipped: Array<{ type: string; ops: number }>;
  /**
   * Worst batch samples within window (for quick identification of "large/slow/skip surge/node growth")
   */
  worstBatches: ReceiverAttributionWorstBatch[];
}

export class Receiver {
  private attributionWindowMs: number;
  private attributionHistoryMs: number;
  private attributionMaxSamples: number;
  private applyHistory: ReceiverApplySample[] = [];
  private nodeMap = new Map<number, NodeInstance>();
  private rootChildren: number[] = [];
  private registry: ComponentRegistry;
  private sendToSandbox: SendToSandbox;
  private onUpdate: () => void;
  private updateScheduled = false;
  private renderCount = 0;
  private lastRenderAt: number | null = null;
  private lastRenderDurationMs: number | null = null;
  private lastApply: ReceiverApplyStats | null = null;
  private totalBatches = 0;
  private totalOpsReceived = 0;
  private totalOpsApplied = 0;
  private totalOpsSkipped = 0;
  private totalOpsFailed = 0;
  private opts: {
    onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
    maxBatchSize: number;
    debug: boolean;
  };

  constructor(
    registry: ComponentRegistry,
    sendToSandbox: SendToSandbox,
    onUpdate: () => void,
    options?: ReceiverOptions
  ) {
    this.attributionWindowMs = options?.attributionWindowMs ?? 5000;
    this.attributionHistoryMs = options?.attributionHistoryMs ?? 60_000;
    this.attributionMaxSamples = options?.attributionMaxSamples ?? 200;
    if (this.attributionHistoryMs < this.attributionWindowMs) {
      this.attributionHistoryMs = this.attributionWindowMs;
    }

    this.registry = registry;
    this.sendToSandbox = sendToSandbox;
    this.onUpdate = onUpdate;
    this.opts = {
      onMetric: options?.onMetric,
      maxBatchSize: options?.maxBatchSize ?? 5000,
      debug: options?.debug ?? false,
    };
  }

  private log(...args: unknown[]): void {
    if (this.opts.debug) {
      console.log('[rill:Receiver]', ...args);
    }
  }

  /**
   * Apply operation batch
   */
  applyBatch(batch: OperationBatch): ReceiverApplyStats {
    const start = Date.now();
    const nodesBefore = this.nodeMap.size;
    const limit = this.opts.maxBatchSize;
    let applied = 0;
    let skipped = 0;
    let failed = 0;

    const opCounts: Record<string, number> = {};
    const skippedOpCounts: Record<string, number> = {};
    const nodeTypeCounts = new Map<string, number>();
    const skippedNodeTypeCounts = new Map<string, number>();

    const incRecord = (rec: Record<string, number>, key: string, n = 1) => {
      rec[key] = (rec[key] ?? 0) + n;
    };
    const incMap = (m: Map<string, number>, key: string, n = 1) => {
      m.set(key, (m.get(key) ?? 0) + n);
    };
    const getTouchedTypes = (op: Operation): string[] => {
      switch (op.op) {
        case 'CREATE':
          return [op.type];
        case 'UPDATE':
        case 'DELETE':
        case 'TEXT': {
          const t = this.nodeMap.get(op.id)?.type;
          return t ? [t] : [];
        }
        case 'APPEND':
        case 'INSERT':
        case 'REMOVE': {
          const types: string[] = [];
          if (op.parentId !== 0) {
            const pt = this.nodeMap.get(op.parentId)?.type;
            if (pt) types.push(pt);
          }
          const ct = this.nodeMap.get(op.childId)?.type;
          if (ct) types.push(ct);
          return types;
        }
        case 'REORDER': {
          if (op.parentId === 0) return [];
          const pt = this.nodeMap.get(op.parentId)?.type;
          return pt ? [pt] : [];
        }
        default:
          return [];
      }
    };
    const topN = (m: Map<string, number>, n: number) =>
      Array.from(m.entries())
        .sort((a, b) => b[1] - a[1])
        .slice(0, n)
        .map(([type, ops]) => ({ type, ops }));

    for (let i = 0; i < batch.operations.length; i++) {
      if (applied >= limit) {
        // Record skipped op distribution and attribution (for backpressure diagnosis)
        for (let j = i; j < batch.operations.length; j++) {
          const op = batch.operations[j];
          if (!op) continue;
          incRecord(skippedOpCounts, op.op);
          for (const t of getTouchedTypes(op)) incMap(skippedNodeTypeCounts, t);
        }
        skipped += batch.operations.length - i;
        break;
      }
      const op = batch.operations[i];
      if (!op) continue;
      try {
        const touchedTypes = getTouchedTypes(op);
        this.applyOperation(op);
        applied++;
        incRecord(opCounts, op.op);
        for (const t of touchedTypes) incMap(nodeTypeCounts, t);
      } catch (_e) {
        failed++;
        // continue applying remaining operations
      }
    }
    this.opts.onMetric?.('receiver.applyBatch', Date.now() - start, {
      applied,
      skipped,
      failed,
      total: batch.operations.length,
    });

    const durationMs = Date.now() - start;
    this.totalBatches += 1;
    this.totalOpsReceived += batch.operations.length;
    this.totalOpsApplied += applied;
    this.totalOpsSkipped += skipped;
    this.totalOpsFailed += failed;
    const nodesAfter = this.nodeMap.size;
    const nodeDelta = nodesAfter - nodesBefore;
    const applyStats: ReceiverApplyStats = {
      batchId: batch.batchId,
      total: batch.operations.length,
      applied,
      skipped,
      failed,
      at: Date.now(),
      durationMs,
      nodesBefore,
      nodesAfter,
      nodeDelta,
      opCounts,
      skippedOpCounts,
      topNodeTypes: topN(nodeTypeCounts, 6),
      topNodeTypesSkipped: topN(skippedNodeTypeCounts, 6),
    };
    this.lastApply = applyStats;
    this.recordApplySample(
      applyStats,
      Object.fromEntries(nodeTypeCounts.entries()),
      Object.fromEntries(skippedNodeTypeCounts.entries())
    );

    if (skipped > 0) {
      // Inform guest about backpressure/skip to allow adaptive throttling upstream
      try {
        this.sendToSandbox({
          type: 'HOST_EVENT',
          eventName: 'RECEIVER_BACKPRESSURE',
          payload: { batchId: batch.batchId, skipped, applied, total: batch.operations.length },
        });
      } catch {
        // Silently ignore errors when sending backpressure notifications
      }
    }

    this.scheduleUpdate();
    return applyStats;
  }

  private recordApplySample(
    stats: ReceiverApplyStats,
    nodeTypeCountsAll: Record<string, number>,
    skippedNodeTypeCountsAll: Record<string, number>
  ): void {
    this.applyHistory.push({ ...stats, nodeTypeCountsAll, skippedNodeTypeCountsAll });
    this.trimApplyHistory(stats.at);
  }

  private trimApplyHistory(now: number): void {
    const cutoff = now - this.attributionHistoryMs;
    while (this.applyHistory.length > 0 && this.applyHistory[0]!.at < cutoff) {
      this.applyHistory.shift();
    }
    if (this.applyHistory.length > this.attributionMaxSamples) {
      this.applyHistory.splice(0, this.applyHistory.length - this.attributionMaxSamples);
    }
  }

  private computeAttributionWindow(now: number): ReceiverAttributionWindow | null {
    if (this.applyHistory.length === 0) return null;
    const cutoff = now - this.attributionWindowMs;
    const samples = this.applyHistory.filter((s) => s.at >= cutoff);
    if (samples.length === 0) return null;

    const incRecord = (rec: Record<string, number>, key: string, n = 1) => {
      rec[key] = (rec[key] ?? 0) + n;
    };
    const incMap = (m: Map<string, number>, key: string, n = 1) => {
      m.set(key, (m.get(key) ?? 0) + n);
    };
    const topN = (m: Map<string, number>, n: number) =>
      Array.from(m.entries())
        .sort((a, b) => b[1] - a[1])
        .slice(0, n)
        .map(([type, ops]) => ({ type, ops }));

    let total = 0;
    let applied = 0;
    let skipped = 0;
    let failed = 0;
    let durationMs = 0;
    let nodeDelta = 0;

    const opCounts: Record<string, number> = {};
    const skippedOpCounts: Record<string, number> = {};
    const nodeTypeCounts = new Map<string, number>();
    const skippedNodeTypeCounts = new Map<string, number>();

    for (const s of samples) {
      total += s.total;
      applied += s.applied;
      skipped += s.skipped;
      failed += s.failed;
      durationMs += s.durationMs;
      nodeDelta += s.nodeDelta;

      for (const [k, v] of Object.entries(s.opCounts)) incRecord(opCounts, k, v);
      for (const [k, v] of Object.entries(s.skippedOpCounts)) incRecord(skippedOpCounts, k, v);

      for (const [t, v] of Object.entries(s.nodeTypeCountsAll)) incMap(nodeTypeCounts, t, v);
      for (const [t, v] of Object.entries(s.skippedNodeTypeCountsAll))
        incMap(skippedNodeTypeCounts, t, v);
    }

    const pickMax = (metric: (s: ReceiverApplyStats) => number): ReceiverApplyStats | null => {
      let best: ReceiverApplyStats | null = null;
      let bestV = -Infinity;
      for (const s of samples) {
        const v = metric(s);
        if (v > bestV) {
          bestV = v;
          best = s;
        }
      }
      return best;
    };

    const worstBatches: ReceiverAttributionWorstBatch[] = [];
    const largest = pickMax((s) => s.total);
    if (largest) {
      worstBatches.push({
        kind: 'largest',
        batchId: largest.batchId,
        at: largest.at,
        total: largest.total,
        applied: largest.applied,
        skipped: largest.skipped,
        failed: largest.failed,
        durationMs: largest.durationMs,
        nodeDelta: largest.nodeDelta,
      });
    }

    const slowest = pickMax((s) => s.durationMs);
    if (slowest && slowest.durationMs > 0) {
      worstBatches.push({
        kind: 'slowest',
        batchId: slowest.batchId,
        at: slowest.at,
        total: slowest.total,
        applied: slowest.applied,
        skipped: slowest.skipped,
        failed: slowest.failed,
        durationMs: slowest.durationMs,
        nodeDelta: slowest.nodeDelta,
      });
    }

    const mostSkipped = pickMax((s) => s.skipped);
    if (mostSkipped && mostSkipped.skipped > 0) {
      worstBatches.push({
        kind: 'mostSkipped',
        batchId: mostSkipped.batchId,
        at: mostSkipped.at,
        total: mostSkipped.total,
        applied: mostSkipped.applied,
        skipped: mostSkipped.skipped,
        failed: mostSkipped.failed,
        durationMs: mostSkipped.durationMs,
        nodeDelta: mostSkipped.nodeDelta,
      });
    }

    const mostGrowth = pickMax((s) => s.nodeDelta);
    if (mostGrowth && mostGrowth.nodeDelta !== 0) {
      worstBatches.push({
        kind: 'mostGrowth',
        batchId: mostGrowth.batchId,
        at: mostGrowth.at,
        total: mostGrowth.total,
        applied: mostGrowth.applied,
        skipped: mostGrowth.skipped,
        failed: mostGrowth.failed,
        durationMs: mostGrowth.durationMs,
        nodeDelta: mostGrowth.nodeDelta,
      });
    }

    return {
      windowMs: this.attributionWindowMs,
      sampleCount: samples.length,
      total,
      applied,
      skipped,
      failed,
      durationMs,
      nodeDelta,
      opCounts,
      skippedOpCounts,
      topNodeTypes: topN(nodeTypeCounts, 6),
      topNodeTypesSkipped: topN(skippedNodeTypeCounts, 6),
      worstBatches,
    };
  }

  /**
   * Schedule update (debounced)
   */
  private scheduleUpdate(): void {
    if (this.updateScheduled) return;
    this.updateScheduled = true;

    // Use microtask to ensure batch operations complete before updating
    safeQueueMicrotask(() => {
      this.updateScheduled = false;
      this.log('calling onUpdate, rootChildren:', this.rootChildren.length);
      this.onUpdate();
      this.log('onUpdate completed');
    });
  }

  /**
   * Apply single operation
   */
  private applyOperation(op: Operation): void {
    if (this.opts.onMetric) {
      this.opts.onMetric('receiver.operation', 0, { op: op.op, id: op.id });
    }
    // Add detailed logging for operations
    if (op.op === 'APPEND') {
      this.log(`Applying APPEND: childId=${op.childId}, parentId=${op.parentId}`);
    } else if (op.op === 'INSERT') {
      this.log(
        `Applying INSERT: childId=${op.childId}, parentId=${op.parentId}, index=${op.index}`
      );
    } else if (op.op === 'CREATE') {
      this.log(`Applying CREATE: id=${op.id}, type=${op.type}`);
    }

    switch (op.op) {
      case 'CREATE':
        this.handleCreate(op);
        break;
      case 'UPDATE':
        this.handleUpdate(op);
        break;
      case 'APPEND':
        this.handleAppend(op);
        break;
      case 'INSERT':
        this.handleInsert(op);
        break;
      case 'REMOVE':
        this.handleRemove(op);
        break;
      case 'DELETE':
        this.handleDelete(op);
        break;
      case 'REORDER':
        this.handleReorder(op);
        break;
      case 'TEXT':
        this.handleText(op);
        break;
      default:
        console.warn('[rill] Unknown operation:', (op as Operation).op);
    }
  }

  /**
   * Handle create operation
   */
  private handleCreate(op: Extract<Operation, { op: 'CREATE' }>): void {
    const node: NodeInstance = {
      id: op.id,
      type: op.type,
      props: this.deserializeProps(op.props),
      children: [],
    };
    this.nodeMap.set(op.id, node);
  }

  /**
   * Handle update operation
   */
  private handleUpdate(op: Extract<Operation, { op: 'UPDATE' }>): void {
    const node = this.nodeMap.get(op.id);
    if (!node) {
      console.warn(`[rill] Node ${op.id} not found for update`);
      return;
    }

    // Merge new properties
    const newProps = this.deserializeProps(op.props);
    node.props = { ...node.props, ...newProps };

    // Remove deleted properties
    if (op.removedProps) {
      for (const key of op.removedProps) {
        delete node.props[key];
      }
    }
  }

  /**
   * Handle append operation
   */
  private handleAppend(op: Extract<Operation, { op: 'APPEND' }>): void {
    // 🔴 TRACK: Log APPEND operation details
    if (typeof globalThis !== 'undefined') {
      globalThis.__RECEIVER_APPEND_CALLS = (globalThis.__RECEIVER_APPEND_CALLS || 0) + 1;
      if (!globalThis.__APPEND_PARENT_IDS) {
        globalThis.__APPEND_PARENT_IDS = [];
      }
      globalThis.__APPEND_PARENT_IDS.push(op.parentId);

      if (!globalThis.__APPEND_DETAILS) {
        globalThis.__APPEND_DETAILS = [];
      }
      globalThis.__APPEND_DETAILS.push({
        parentId: op.parentId,
        childId: op.childId,
        parentExists: this.nodeMap.has(op.parentId),
      });
    }

    if (op.parentId === 0) {
      // Append to root container
      if (!this.rootChildren.includes(op.childId)) {
        this.rootChildren.push(op.childId);
      }
    } else {
      const parent = this.nodeMap.get(op.parentId);
      if (parent && !parent.children.includes(op.childId)) {
        parent.children.push(op.childId);
      }
    }
  }

  /**
   * Handle insert operation
   */
  private handleInsert(op: Extract<Operation, { op: 'INSERT' }>): void {
    if (op.parentId === 0) {
      // Insert into root container
      const existingIndex = this.rootChildren.indexOf(op.childId);
      if (existingIndex !== -1) {
        this.rootChildren.splice(existingIndex, 1);
      }
      this.rootChildren.splice(op.index, 0, op.childId);
    } else {
      const parent = this.nodeMap.get(op.parentId);
      if (parent) {
        const existingIndex = parent.children.indexOf(op.childId);
        if (existingIndex !== -1) {
          parent.children.splice(existingIndex, 1);
        }
        parent.children.splice(op.index, 0, op.childId);
      }
    }
  }

  /**
   * Handle remove operation
   */
  private handleRemove(op: Extract<Operation, { op: 'REMOVE' }>): void {
    if (op.parentId === 0) {
      const index = this.rootChildren.indexOf(op.childId);
      if (index !== -1) {
        this.rootChildren.splice(index, 1);
      }
    } else {
      const parent = this.nodeMap.get(op.parentId);
      if (parent) {
        const index = parent.children.indexOf(op.childId);
        if (index !== -1) {
          parent.children.splice(index, 1);
        }
      }
    }
  }

  /**
   * Handle delete operation
   */
  private handleDelete(op: Extract<Operation, { op: 'DELETE' }>): void {
    // Best-effort: detach from root/parents to avoid stale references.
    // Protocol-wise, callers SHOULD send REMOVE before DELETE, but we keep Receiver resilient.
    const rootIndex = this.rootChildren.indexOf(op.id);
    if (rootIndex !== -1) {
      this.rootChildren.splice(rootIndex, 1);
    }

    for (const node of this.nodeMap.values()) {
      const idx = node.children.indexOf(op.id);
      if (idx !== -1) node.children.splice(idx, 1);
    }

    this.deleteNodeRecursive(op.id);
  }

  /**
   * Recursively delete node
   */
  private deleteNodeRecursive(id: number): void {
    const node = this.nodeMap.get(id);
    if (!node) return;

    // Recursively delete children
    for (const childId of node.children) {
      this.deleteNodeRecursive(childId);
    }

    this.nodeMap.delete(id);
  }

  /**
   * Handle reorder operation
   */
  private handleReorder(op: Extract<Operation, { op: 'REORDER' }>): void {
    if (op.parentId === 0) {
      this.rootChildren = op.childIds;
    } else {
      const parent = this.nodeMap.get(op.parentId);
      if (parent) {
        parent.children = op.childIds;
      }
    }
  }

  /**
   * Handle text update operation
   */
  private handleText(op: Extract<Operation, { op: 'TEXT' }>): void {
    const node = this.nodeMap.get(op.id);
    if (node) {
      node.props.text = op.text;
    }
  }

  /**
   * Deserialize props
   */
  private deserializeProps(props: SerializedProps): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(props)) {
      result[key] = this.deserializeValue(value);
    }

    return result;
  }

  /**
   * Deserialize value
   */
  private deserializeValue(value: unknown, depth = 0): unknown {
    const MAX_DEPTH = 50;

    if (depth > MAX_DEPTH) {
      console.warn('[rill:Receiver] Maximum deserialization depth exceeded');
      return undefined;
    }

    if (isSerializedFunction(value)) {
      // 🔴 TRACK: Log function creation
      // Create proxy function
      return (...args: unknown[]) => {
        // Try to invoke locally first (if React is running in Host)
        if (hasCallback(value.__fnId)) {
          try {
            invokeCallback(value.__fnId, args);
            return;
          } catch (e) {
            console.error('[rill:Receiver] Local callback execution failed:', e);
          }
        }

        this.sendToSandbox({
          type: 'CALL_FUNCTION',
          fnId: value.__fnId,
          args: args as [],
        });
      };
    }

    if (Array.isArray(value)) {
      return value.map((item) => this.deserializeValue(item, depth + 1));
    }

    if (typeof value === 'object' && value !== null) {
      const result: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value)) {
        result[k] = this.deserializeValue(v, depth + 1);
      }
      return result;
    }

    return value;
  }

  /**
   * Render component tree
   */
  render(): React.ReactElement | string | null {
    const t0 = Date.now();
    this.renderCount += 1;
    this.log(
      'render() called, rootChildren:',
      this.rootChildren.length,
      'nodeMap:',
      this.nodeMap.size
    );
    if (this.rootChildren.length === 0) {
      this.log('render() returning null (no root children)');
      const durationMs = Date.now() - t0;
      this.lastRenderAt = Date.now();
      this.lastRenderDurationMs = durationMs;
      this.opts.onMetric?.('receiver.render', durationMs, { nodeCount: this.nodeMap.size });
      return null;
    }

    // If only one root node, render directly
    const firstChild = this.rootChildren[0];
    if (this.rootChildren.length === 1 && firstChild) {
      const el = this.renderNode(firstChild);
      let elType: string = typeof el;
      if (el && typeof el === 'object' && 'type' in el) {
        const elTypeProp = el.type as unknown;
        if (elTypeProp && typeof elTypeProp === 'object' && 'name' in elTypeProp) {
          const nameVal = (elTypeProp as { name: unknown }).name;
          if (typeof nameVal === 'string') {
            elType = nameVal;
          }
        }
      }
      this.log('render() returning single element, type:', elType);
      const durationMs = Date.now() - t0;
      this.lastRenderAt = Date.now();
      this.lastRenderDurationMs = durationMs;
      this.opts.onMetric?.('receiver.render', durationMs, { nodeCount: this.nodeMap.size });
      return el;
    }

    // Multiple root nodes, wrap in Fragment
    const el = React.createElement(
      React.Fragment,
      null,
      ...this.rootChildren.map((id) => this.renderNode(id))
    );
    this.log('render() returning Fragment with', this.rootChildren.length, 'children');
    const durationMs = Date.now() - t0;
    this.lastRenderAt = Date.now();
    this.lastRenderDurationMs = durationMs;
    this.opts.onMetric?.('receiver.render', durationMs, { nodeCount: this.nodeMap.size });
    return el;
  }

  /**
   * Render single node
   */
  private renderNode(id: number): React.ReactElement | string | null {
    const node = this.nodeMap.get(id);
    if (!node) {
      console.warn(`[rill] Node ${id} not found`);
      return null;
    }

    // Handle text node
    if (node.type === '__TEXT__') {
      return node.props.text as string;
    }

    // Get component implementation
    const Component = this.registry.get(node.type);
    if (!Component) {
      console.warn(`[rill] Component "${node.type}" not registered`);
      return null;
    }

    // Recursively render children
    const children = node.children
      .map((childId) => this.renderNode(childId))
      .filter((child): child is React.ReactElement | string => child !== null);

    // Build props
    const props: Record<string, unknown> = {
      ...node.props,
      key: `rill-${id}`,
    };

    // 🔴 TRACK: Log TouchableOpacity props
    if (node.type === 'TouchableOpacity') {
      if (typeof globalThis !== 'undefined') {
        globalThis.__TOUCHABLE_RENDER_COUNT = (globalThis.__TOUCHABLE_RENDER_COUNT || 0) + 1;
        globalThis.__LAST_TOUCHABLE_HAS_ONPRESS = typeof props.onPress === 'function';
      }
    }

    return React.createElement(Component, props, ...children);
  }

  /**
   * Get node count
   */
  get nodeCount(): number {
    return this.nodeMap.size;
  }

  /**
   * Clear all nodes
   */
  clear(): void {
    this.nodeMap.clear();
    this.rootChildren = [];
  }

  getStats(): ReceiverStats {
    return {
      nodeCount: this.nodeMap.size,
      rootChildrenCount: this.rootChildren.length,
      renderCount: this.renderCount,
      lastRenderAt: this.lastRenderAt,
      lastRenderDurationMs: this.lastRenderDurationMs,
      totalBatches: this.totalBatches,
      totalOps: {
        received: this.totalOpsReceived,
        applied: this.totalOpsApplied,
        skipped: this.totalOpsSkipped,
        failed: this.totalOpsFailed,
      },
      lastApply: this.lastApply,
      attribution: this.computeAttributionWindow(Date.now()),
    };
  }

  /**
   * Get debug info
   */
  getDebugInfo(): {
    nodeCount: number;
    rootChildren: number[];
    nodes: Array<{ id: number; type: string; childCount: number }>;
    appendCalls?: number;
    appendParentIds?: number[];
    appendSample?: Array<{ parentId: number; childId: number; parentExists: boolean }>;
    touchableRenderCount?: number;
    touchableHasOnPress?: boolean;
    functionCount?: number;
    lastFunctionFnId?: string;
  } {
    return {
      nodeCount: this.nodeMap.size,
      rootChildren: [...this.rootChildren],
      nodes: Array.from(this.nodeMap.values()).map((node) => ({
        id: node.id,
        type: node.type,
        childCount: node.children.length,
      })),
      // 🔴 TRACK: Include APPEND tracking data
      appendCalls:
        typeof globalThis !== 'undefined' ? globalThis.__RECEIVER_APPEND_CALLS : undefined,
      appendParentIds:
        typeof globalThis !== 'undefined' ? globalThis.__APPEND_PARENT_IDS : undefined,
      appendSample:
        typeof globalThis !== 'undefined' ? globalThis.__APPEND_DETAILS?.slice(0, 10) : undefined,
      // 🔴 TRACK: Include TouchableOpacity tracking
      touchableRenderCount:
        typeof globalThis !== 'undefined' ? globalThis.__TOUCHABLE_RENDER_COUNT : undefined,
      touchableHasOnPress:
        typeof globalThis !== 'undefined' ? globalThis.__LAST_TOUCHABLE_HAS_ONPRESS : undefined,
      // 🔴 TRACK: Include function deserialization tracking
      functionCount:
        typeof globalThis !== 'undefined' ? globalThis.__RECEIVER_FUNCTION_COUNT : undefined,
      lastFunctionFnId:
        typeof globalThis !== 'undefined' ? globalThis.__LAST_FUNCTION_FNID : undefined,
    };
  }
}

interface ReceiverApplySample extends ReceiverApplyStats {
  nodeTypeCountsAll: Record<string, number>;
  skippedNodeTypeCountsAll: Record<string, number>;
}
