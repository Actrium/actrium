/**
 * Instruction Receiver
 *
 * Parses operation instructions from sandbox and builds React Native component tree
 */

import React from 'react';

// Polyfill queueMicrotask if not available
const safeQueueMicrotask = typeof queueMicrotask !== 'undefined'
  ? queueMicrotask
  : (callback: () => void) => Promise.resolve().then(callback);
import type {
  Operation,
  OperationBatch,
  NodeInstance,
  SerializedProps,
  SerializedFunction,
  HostMessage,
} from '../types';
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
}

export class Receiver {
  private nodeMap = new Map<number, NodeInstance>();
  private rootChildren: number[] = [];
  private registry: ComponentRegistry;
  private sendToSandbox: SendToSandbox;
  private onUpdate: () => void;
  private updateScheduled = false;
  private opts: {
    onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;
    maxBatchSize: number;
    debug: boolean;
  };

  constructor(
    registry: ComponentRegistry,
    sendToSandbox: SendToSandbox,
    onUpdate: () => void,
    options?: ReceiverOptions,
  ) {
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
  applyBatch(batch: OperationBatch): void {
    const start = Date.now();
    const limit = this.opts.maxBatchSize;
    let applied = 0;
    let skipped = 0;
    let failed = 0;
    for (let i = 0; i < batch.operations.length; i++) {
      if (applied >= limit) { skipped += (batch.operations.length - i); break; }
      const op = batch.operations[i];
      if (!op) continue;
      try {
        this.applyOperation(op);
        applied++;
      } catch (e) {
        failed++;
        // continue applying remaining operations
      }
    }
    this.opts.onMetric?.('receiver.applyBatch', Date.now() - start, { applied, skipped, failed, total: batch.operations.length });

    if (skipped > 0) {
      // Inform guest about backpressure/skip to allow adaptive throttling upstream
      try {
        this.sendToSandbox({ type: 'HOST_EVENT', eventName: 'RECEIVER_BACKPRESSURE', payload: { batchId: batch.batchId, skipped, applied, total: batch.operations.length } });
      } catch {}
    }

    this.scheduleUpdate();
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
      this.log(`Applying INSERT: childId=${op.childId}, parentId=${op.parentId}, index=${op.index}`);
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
      node.props['text'] = op.text;
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
      // Create proxy function
      return (...args: unknown[]) => {
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
    this.log('render() called, rootChildren:', this.rootChildren.length, 'nodeMap:', this.nodeMap.size);
    if (this.rootChildren.length === 0) {
      this.log('render() returning null (no root children)');
      this.opts.onMetric?.('receiver.render', Date.now() - t0, { nodeCount: this.nodeMap.size });
      return null;
    }

    // If only one root node, render directly
    const firstChild = this.rootChildren[0];
    if (this.rootChildren.length === 1 && firstChild) {
      const el = this.renderNode(firstChild);
      this.log('render() returning single element, type:', (el as any)?.type?.name || typeof el);
      this.opts.onMetric?.('receiver.render', Date.now() - t0, { nodeCount: this.nodeMap.size });
      return el;
    }

    // Multiple root nodes, wrap in Fragment
    const el = React.createElement(
      React.Fragment,
      null,
      ...this.rootChildren.map((id) => this.renderNode(id))
    );
    this.log('render() returning Fragment with', this.rootChildren.length, 'children');
    this.opts.onMetric?.('receiver.render', Date.now() - t0, { nodeCount: this.nodeMap.size });
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
      return node.props['text'] as string;
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
    const props = {
      ...node.props,
      key: `rill-${id}`,
    };

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

  /**
   * Get debug info
   */
  getDebugInfo(): {
    nodeCount: number;
    rootChildren: number[];
    nodes: Array<{ id: number; type: string; childCount: number }>;
  } {
    return {
      nodeCount: this.nodeMap.size,
      rootChildren: [...this.rootChildren],
      nodes: Array.from(this.nodeMap.values()).map((node) => ({
        id: node.id,
        type: node.type,
        childCount: node.children.length,
      })),
    };
  }
}
