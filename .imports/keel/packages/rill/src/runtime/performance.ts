/**
 * Performance Optimization Module
 *
 * Provides batch update throttling, operation merging, and other optimization mechanisms
 */

import type { Operation, OperationBatch } from '../types';

/**
 * Batch update configuration
 */
export interface BatchConfig {
  /**
   * Maximum batch size
   * @default 100
   */
  maxBatchSize?: number;

  /**
   * Throttle interval (milliseconds)
   * @default 16 (~60fps)
   */
  throttleMs?: number;

  /**
   * Enable operation merging
   * @default true
   */
  enableMerge?: boolean;
}

/**
 * Operation Merger
 *
 * Merges consecutive operations on the same node to reduce unnecessary updates
 */
export class OperationMerger {
  /**
   * Merge operation batch
   */
  merge(operations: Operation[]): Operation[] {
    if (operations.length <= 1) return operations;

    const result: Operation[] = [];
    const updateMap = new Map<number, Extract<Operation, { op: 'UPDATE' }>>();

    for (const op of operations) {
      switch (op.op) {
        case 'UPDATE': {
          // Merge multiple updates on the same node
          const existing = updateMap.get(op.id);
          if (existing) {
            // Merge props
            existing.props = { ...existing.props, ...op.props };
            // Merge removedProps
            if (op.removedProps) {
              existing.removedProps = [
                ...(existing.removedProps || []),
                ...op.removedProps,
              ].filter((key, index, arr) => arr.indexOf(key) === index);
            }
          } else {
            const newOp = { ...op };
            updateMap.set(op.id, newOp);
            result.push(newOp);
          }
          break;
        }

        case 'DELETE': {
          // When deleting a node, remove all previous updates for that node
          updateMap.delete(op.id);
          // Also remove UPDATE operations for this node from result
          const deleteIndex = result.findIndex(
            (o) => o.op === 'UPDATE' && o.id === op.id
          );
          if (deleteIndex !== -1) {
            result.splice(deleteIndex, 1);
          }
          result.push(op);
          break;
        }

        default:
          result.push(op);
      }
    }

    return result;
  }
}

/**
 * Throttled Update Scheduler
 *
 * Controls update frequency to avoid excessive re-renders
 */
export class ThrottledScheduler {
  private pendingOperations: Operation[] = [];
  private lastFlushTime = 0;
  private flushScheduled = false;
  private rafId: number | null = null;
  private timeoutId: ReturnType<typeof setTimeout> | null = null;

  private config: Required<BatchConfig>;
  private merger: OperationMerger;
  private onFlush: (batch: OperationBatch) => void;
  private batchId = 0;
  private version = 1;

  constructor(
    onFlush: (batch: OperationBatch) => void,
    config: BatchConfig = {}
  ) {
    this.config = {
      maxBatchSize: config.maxBatchSize ?? 100,
      throttleMs: config.throttleMs ?? 16,
      enableMerge: config.enableMerge ?? true,
    };
    this.merger = new OperationMerger();
    this.onFlush = onFlush;
  }

  /**
   * Add operation to queue
   */
  enqueue(operation: Operation): void {
    this.pendingOperations.push({
      ...operation,
      timestamp: Date.now(),
    });

    // Flush immediately if max batch size is reached
    if (this.pendingOperations.length >= this.config.maxBatchSize) {
      this.flush();
      return;
    }

    this.scheduleFlush();
  }

  /**
   * Add multiple operations
   */
  enqueueAll(operations: Operation[]): void {
    for (const op of operations) {
      this.pendingOperations.push({
        ...op,
        timestamp: Date.now(),
      });
    }

    if (this.pendingOperations.length >= this.config.maxBatchSize) {
      this.flush();
      return;
    }

    this.scheduleFlush();
  }

  /**
   * Schedule flush
   */
  private scheduleFlush(): void {
    if (this.flushScheduled) return;
    this.flushScheduled = true;

    const now = Date.now();
    const timeSinceLastFlush = now - this.lastFlushTime;

    if (timeSinceLastFlush >= this.config.throttleMs) {
      // Use microtask to ensure synchronous code completes
      queueMicrotask(() => this.flush());
    } else {
      // Delay until next throttle cycle
      const delay = this.config.throttleMs - timeSinceLastFlush;
      this.timeoutId = setTimeout(() => this.flush(), delay);
    }
  }

  /**
   * Flush all pending operations immediately
   */
  flush(): void {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    if (this.rafId && typeof cancelAnimationFrame !== 'undefined') {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }

    this.flushScheduled = false;

    if (this.pendingOperations.length === 0) return;

    let operations = [...this.pendingOperations];
    this.pendingOperations = [];
    this.lastFlushTime = Date.now();

    // Apply operation merging
    if (this.config.enableMerge) {
      operations = this.merger.merge(operations);
    }

    const batch: OperationBatch = {
      version: this.version,
      batchId: ++this.batchId,
      operations,
    };

    this.onFlush(batch);
  }

  /**
   * Get pending operation count
   */
  get pendingCount(): number {
    return this.pendingOperations.length;
  }

  /**
   * Clean up resources
   */
  dispose(): void {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    if (this.rafId && typeof cancelAnimationFrame !== 'undefined') {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    this.pendingOperations = [];
    this.flushScheduled = false;
  }
}

/**
 * Virtual scroll configuration
 */
export interface VirtualScrollConfig {
  /**
   * Estimated item height
   */
  estimatedItemHeight: number;

  /**
   * Buffer items outside visible area
   * @default 5
   */
  overscan?: number;

  /**
   * Scroll throttle interval (milliseconds)
   * @default 16
   */
  scrollThrottleMs?: number;
}

/**
 * Virtual scroll state
 */
export interface VirtualScrollState {
  /** Visible items start index */
  startIndex: number;
  /** Visible items end index */
  endIndex: number;
  /** Top placeholder height */
  offsetTop: number;
  /** Bottom placeholder height */
  offsetBottom: number;
  /** Visible items */
  visibleItems: number[];
}

/**
 * Virtual Scroll Calculator
 *
 * For virtualized rendering of FlatList and other long lists
 */
export class VirtualScrollCalculator {
  private config: Required<VirtualScrollConfig>;
  private itemHeights = new Map<number, number>();
  private totalItems = 0;

  constructor(config: VirtualScrollConfig) {
    this.config = {
      estimatedItemHeight: config.estimatedItemHeight,
      overscan: config.overscan ?? 5,
      scrollThrottleMs: config.scrollThrottleMs ?? 16,
    };
  }

  /**
   * Set total item count
   */
  setTotalItems(count: number): void {
    this.totalItems = count;
  }

  /**
   * Record actual item height
   */
  setItemHeight(index: number, height: number): void {
    this.itemHeights.set(index, height);
  }

  /**
   * Get item height (actual or estimated)
   */
  getItemHeight(index: number): number {
    return this.itemHeights.get(index) ?? this.config.estimatedItemHeight;
  }

  /**
   * Calculate item offset from top
   */
  getItemOffset(index: number): number {
    let offset = 0;
    for (let i = 0; i < index; i++) {
      offset += this.getItemHeight(i);
    }
    return offset;
  }

  /**
   * Calculate total height
   */
  getTotalHeight(): number {
    let height = 0;
    for (let i = 0; i < this.totalItems; i++) {
      height += this.getItemHeight(i);
    }
    return height;
  }

  /**
   * Calculate visible range
   */
  calculate(
    scrollTop: number,
    viewportHeight: number
  ): VirtualScrollState {
    if (this.totalItems === 0) {
      return {
        startIndex: 0,
        endIndex: 0,
        offsetTop: 0,
        offsetBottom: 0,
        visibleItems: [],
      };
    }

    // Find start index
    let startIndex = 0;
    let accumulatedHeight = 0;

    while (
      startIndex < this.totalItems &&
      accumulatedHeight + this.getItemHeight(startIndex) < scrollTop
    ) {
      accumulatedHeight += this.getItemHeight(startIndex);
      startIndex++;
    }

    // Apply overscan
    startIndex = Math.max(0, startIndex - this.config.overscan);

    // Find end index
    let endIndex = startIndex;
    let visibleHeight = 0;

    while (
      endIndex < this.totalItems &&
      visibleHeight < viewportHeight + this.config.estimatedItemHeight * this.config.overscan * 2
    ) {
      visibleHeight += this.getItemHeight(endIndex);
      endIndex++;
    }

    // Apply overscan
    endIndex = Math.min(this.totalItems, endIndex + this.config.overscan);

    // Calculate offsets
    let offsetTop = 0;
    for (let i = 0; i < startIndex; i++) {
      offsetTop += this.getItemHeight(i);
    }

    let offsetBottom = 0;
    for (let i = endIndex; i < this.totalItems; i++) {
      offsetBottom += this.getItemHeight(i);
    }

    // Generate visible items index list
    const visibleItems: number[] = [];
    for (let i = startIndex; i < endIndex; i++) {
      visibleItems.push(i);
    }

    return {
      startIndex,
      endIndex,
      offsetTop,
      offsetBottom,
      visibleItems,
    };
  }

  /**
   * Clear cache
   */
  clear(): void {
    this.itemHeights.clear();
    this.totalItems = 0;
  }
}

/**
 * Scroll Event Throttler
 */
export class ScrollThrottler {
  private lastScrollTop = 0;
  private lastCallTime = 0;
  private scheduledCallback: (() => void) | null = null;
  private timeoutId: ReturnType<typeof setTimeout> | null = null;
  private config: { throttleMs: number; minDelta: number };

  constructor(throttleMs = 16, minDelta = 1) {
    this.config = { throttleMs, minDelta };
  }

  /**
   * Handle scroll event
   */
  onScroll(scrollTop: number, callback: (scrollTop: number) => void): void {
    const now = Date.now();
    const timeDelta = now - this.lastCallTime;
    const scrollDelta = Math.abs(scrollTop - this.lastScrollTop);

    // Ignore if scroll distance is too small
    if (scrollDelta < this.config.minDelta) {
      return;
    }

    // Execute immediately if enough time has passed since last call
    if (timeDelta >= this.config.throttleMs) {
      this.execute(scrollTop, callback);
      return;
    }

    // Otherwise schedule delayed execution
    this.scheduledCallback = () => callback(scrollTop);

    if (!this.timeoutId) {
      this.timeoutId = setTimeout(() => {
        if (this.scheduledCallback) {
          this.execute(scrollTop, this.scheduledCallback as (scrollTop: number) => void);
          this.scheduledCallback = null;
        }
        this.timeoutId = null;
      }, this.config.throttleMs - timeDelta);
    }
  }

  private execute(scrollTop: number, callback: (scrollTop: number) => void): void {
    this.lastScrollTop = scrollTop;
    this.lastCallTime = Date.now();
    callback(scrollTop);
  }

  /**
   * Clean up resources
   */
  dispose(): void {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    this.scheduledCallback = null;
  }
}

/**
 * Performance Monitor
 *
 * Collects and reports performance metrics
 */
export interface PerformanceMetrics {
  /** Total operations */
  totalOperations: number;
  /** Total batches */
  totalBatches: number;
  /** Average batch size */
  avgBatchSize: number;
  /** Operations saved by merging */
  mergedOperations: number;
  /** Node create count */
  createCount: number;
  /** Node update count */
  updateCount: number;
  /** Node delete count */
  deleteCount: number;
  /** Last update time */
  lastUpdateTime: number;
}

export class PerformanceMonitor {
  private metrics: PerformanceMetrics = {
    totalOperations: 0,
    totalBatches: 0,
    avgBatchSize: 0,
    mergedOperations: 0,
    createCount: 0,
    updateCount: 0,
    deleteCount: 0,
    lastUpdateTime: 0,
  };

  /**
   * Record batch
   */
  recordBatch(batch: OperationBatch, originalCount?: number): void {
    this.metrics.totalBatches++;
    this.metrics.totalOperations += batch.operations.length;
    this.metrics.avgBatchSize =
      this.metrics.totalOperations / this.metrics.totalBatches;
    this.metrics.lastUpdateTime = Date.now();

    if (originalCount !== undefined) {
      this.metrics.mergedOperations += originalCount - batch.operations.length;
    }

    for (const op of batch.operations) {
      switch (op.op) {
        case 'CREATE':
          this.metrics.createCount++;
          break;
        case 'UPDATE':
          this.metrics.updateCount++;
          break;
        case 'DELETE':
          this.metrics.deleteCount++;
          break;
      }
    }
  }

  /**
   * Get performance metrics
   */
  getMetrics(): PerformanceMetrics {
    return { ...this.metrics };
  }

  /**
   * Reset metrics
   */
  reset(): void {
    this.metrics = {
      totalOperations: 0,
      totalBatches: 0,
      avgBatchSize: 0,
      mergedOperations: 0,
      createCount: 0,
      updateCount: 0,
      deleteCount: 0,
      lastUpdateTime: 0,
    };
  }
}
