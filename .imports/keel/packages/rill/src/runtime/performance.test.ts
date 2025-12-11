/**
 * Performance module unit tests
 */

import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import {
  OperationMerger,
  ThrottledScheduler,
  VirtualScrollCalculator,
  ScrollThrottler,
  PerformanceMonitor,
} from './performance';
import type { Operation, OperationBatch } from '../types';

// Helper to wait for real time
const sleep = (ms: number) => {
  if (typeof globalThis.setTimeout === 'function') {
    return new Promise(resolve => globalThis.setTimeout(resolve, ms));
  }
  // Fallback: use Bun.sleep if available
  try {
    // Bun provides Bun.sleep as a top-level function
    return (globalThis as any).Bun?.sleep?.(ms) ?? Promise.resolve();
  } catch {
    return Promise.resolve();
  }
};

// ============ OperationMerger 测试 ============

describe('OperationMerger', () => {
  let merger: OperationMerger;

  beforeEach(() => {
    merger = new OperationMerger();
  });

  describe('merge', () => {
    it('should return empty array for empty input', () => {
      expect(merger.merge([])).toEqual([]);
    });

    it('should return single operation unchanged', () => {
      const ops: Operation[] = [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
      ];
      expect(merger.merge(ops)).toEqual(ops);
    });

    it('should merge multiple UPDATE operations for same node', () => {
      const ops: Operation[] = [
        { op: 'UPDATE', id: 1, props: { style: { color: 'red' } } },
        { op: 'UPDATE', id: 1, props: { style: { backgroundColor: 'blue' } } },
        { op: 'UPDATE', id: 1, props: { testID: 'test' } },
      ];

      const merged = merger.merge(ops);

      expect(merged).toHaveLength(1);
      expect(merged[0]).toEqual({
        op: 'UPDATE',
        id: 1,
        props: {
          style: { backgroundColor: 'blue' },
          testID: 'test',
        },
      });
    });

    it('should merge removedProps correctly', () => {
      const ops: Operation[] = [
        { op: 'UPDATE', id: 1, props: {}, removedProps: ['a'] },
        { op: 'UPDATE', id: 1, props: {}, removedProps: ['b'] },
        { op: 'UPDATE', id: 1, props: {}, removedProps: ['a', 'c'] },
      ];

      const merged = merger.merge(ops);

      expect(merged).toHaveLength(1);
      expect(merged[0].op).toBe('UPDATE');
      if (merged[0].op === 'UPDATE') {
        expect(merged[0].removedProps).toEqual(['a', 'b', 'c']);
      }
    });

    it('should not merge UPDATE operations for different nodes', () => {
      const ops: Operation[] = [
        { op: 'UPDATE', id: 1, props: { a: 1 } },
        { op: 'UPDATE', id: 2, props: { b: 2 } },
      ];

      const merged = merger.merge(ops);

      expect(merged).toHaveLength(2);
    });

    it('should remove UPDATE when DELETE follows', () => {
      const ops: Operation[] = [
        { op: 'UPDATE', id: 1, props: { a: 1 } },
        { op: 'UPDATE', id: 1, props: { b: 2 } },
        { op: 'DELETE', id: 1 },
      ];

      const merged = merger.merge(ops);

      expect(merged).toHaveLength(1);
      expect(merged[0].op).toBe('DELETE');
    });

    it('should preserve non-UPDATE operations order', () => {
      const ops: Operation[] = [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        { op: 'CREATE', id: 2, type: 'Text', props: {} },
      ];

      const merged = merger.merge(ops);

      expect(merged).toEqual(ops);
    });

    it('should handle mixed operations correctly', () => {
      const ops: Operation[] = [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'UPDATE', id: 1, props: { style: { flex: 1 } } },
        { op: 'CREATE', id: 2, type: 'Text', props: {} },
        { op: 'UPDATE', id: 1, props: { style: { padding: 10 } } },
        { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
      ];

      const merged = merger.merge(ops);

      // CREATE, merged UPDATE, CREATE, APPEND
      expect(merged).toHaveLength(4);
      expect(merged[0].op).toBe('CREATE');
      expect(merged[1].op).toBe('UPDATE');
      expect((merged[1] as Extract<Operation, { op: 'UPDATE' }>).props).toEqual({
        style: { padding: 10 },
      });
    });
  });
});

// ============ ThrottledScheduler 测试 ============

describe('ThrottledScheduler', () => {
  let scheduler: ThrottledScheduler;
  let flushCallback: ReturnType<typeof mock>;

  beforeEach(() => {
    
    flushCallback = mock();
  });

  afterEach(() => {
    scheduler?.dispose();
    
  });

  describe('enqueue', () => {
    it('should schedule flush after enqueue', async () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });

      expect(scheduler.pendingCount).toBe(1);

      // 执行 microtask
      await Promise.resolve();

      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(scheduler.pendingCount).toBe(0);
    });

    it('should batch multiple enqueues', async () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      scheduler.enqueue({ op: 'CREATE', id: 2, type: 'Text', props: {} });
      scheduler.enqueue({ op: 'APPEND', id: 2, parentId: 1, childId: 2 });

      expect(scheduler.pendingCount).toBe(3);

      await Promise.resolve();

      expect(flushCallback).toHaveBeenCalledTimes(1);
      const batch: OperationBatch = flushCallback.mock.calls[0][0];
      expect(batch.operations).toHaveLength(3);
    });

    it('should flush immediately when maxBatchSize reached', async () => {
      scheduler = new ThrottledScheduler(flushCallback, { maxBatchSize: 2 });

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      scheduler.enqueue({ op: 'CREATE', id: 2, type: 'Text', props: {} });

      // maxBatchSize reached, should flush immediately
      expect(flushCallback).toHaveBeenCalledTimes(1);
    });

    it('should merge operations when enabled', async () => {
      scheduler = new ThrottledScheduler(flushCallback, { enableMerge: true });

      scheduler.enqueue({ op: 'UPDATE', id: 1, props: { a: 1 } });
      scheduler.enqueue({ op: 'UPDATE', id: 1, props: { b: 2 } });

      await Promise.resolve();

      const batch: OperationBatch = flushCallback.mock.calls[0][0];
      expect(batch.operations).toHaveLength(1);
      expect((batch.operations[0] as Extract<Operation, { op: 'UPDATE' }>).props).toEqual({
        a: 1,
        b: 2,
      });
    });

    it('should not merge when disabled', async () => {
      scheduler = new ThrottledScheduler(flushCallback, { enableMerge: false });

      scheduler.enqueue({ op: 'UPDATE', id: 1, props: { a: 1 } });
      scheduler.enqueue({ op: 'UPDATE', id: 1, props: { b: 2 } });

      await Promise.resolve();

      const batch: OperationBatch = flushCallback.mock.calls[0][0];
      expect(batch.operations).toHaveLength(2);
    });
  });

  describe('enqueueAll', () => {
    it('should enqueue multiple operations at once', async () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.enqueueAll([
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'CREATE', id: 2, type: 'Text', props: {} },
      ]);

      expect(scheduler.pendingCount).toBe(2);

      await Promise.resolve();

      expect(flushCallback).toHaveBeenCalledTimes(1);
    });
  });

  describe('flush', () => {
    it('should do nothing when no pending operations', () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.flush();

      expect(flushCallback).not.toHaveBeenCalled();
    });

    it('should flush all pending operations immediately', async () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      scheduler.flush();

      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(scheduler.pendingCount).toBe(0);
    });

    it('should increment batchId for each flush', async () => {
      scheduler = new ThrottledScheduler(flushCallback);

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      scheduler.flush();

      scheduler.enqueue({ op: 'CREATE', id: 2, type: 'Text', props: {} });
      scheduler.flush();

      expect(flushCallback.mock.calls[0][0].batchId).toBe(1);
      expect(flushCallback.mock.calls[1][0].batchId).toBe(2);
    });
  });

  describe('throttling', () => {
    it('should throttle flushes', async () => {
      // Use short throttle time for real-time testing
      scheduler = new ThrottledScheduler(flushCallback, { throttleMs: 20 });

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      await Promise.resolve();

      // First flush happened
      expect(flushCallback).toHaveBeenCalledTimes(1);

      // Immediately enqueue again - should be throttled
      scheduler.enqueue({ op: 'CREATE', id: 2, type: 'Text', props: {} });

      // Should still be 1 (throttled)
      expect(flushCallback).toHaveBeenCalledTimes(1);

      // Wait for throttle period
      await sleep(30);

      // Now second flush should have happened
      expect(flushCallback).toHaveBeenCalledTimes(2);
    });
  });

  describe('dispose', () => {
    it('should clear pending operations and timers', async () => {
      scheduler = new ThrottledScheduler(flushCallback, { throttleMs: 20 });

      scheduler.enqueue({ op: 'CREATE', id: 1, type: 'View', props: {} });
      await Promise.resolve();

      scheduler.enqueue({ op: 'CREATE', id: 2, type: 'Text', props: {} });
      scheduler.dispose();

      expect(scheduler.pendingCount).toBe(0);

      // Wait past throttle period - should NOT trigger another flush
      await sleep(30);
      expect(flushCallback).toHaveBeenCalledTimes(1); // Only the first one
    });
  });
});

// ============ VirtualScrollCalculator 测试 ============

describe('VirtualScrollCalculator', () => {
  let calculator: VirtualScrollCalculator;

  beforeEach(() => {
    calculator = new VirtualScrollCalculator({
      estimatedItemHeight: 50,
      overscan: 2,
    });
  });

  describe('setTotalItems', () => {
    it('should set total items count', () => {
      calculator.setTotalItems(100);
      const state = calculator.calculate(0, 500);
      expect(state.visibleItems.length).toBeGreaterThan(0);
    });
  });

  describe('setItemHeight', () => {
    it('should record actual item height', () => {
      calculator.setItemHeight(0, 100);
      expect(calculator.getItemHeight(0)).toBe(100);
    });

    it('should return estimated height for unknown items', () => {
      expect(calculator.getItemHeight(999)).toBe(50);
    });
  });

  describe('getItemOffset', () => {
    it('should calculate offset correctly', () => {
      calculator.setTotalItems(10);
      calculator.setItemHeight(0, 100);
      calculator.setItemHeight(1, 50);

      expect(calculator.getItemOffset(0)).toBe(0);
      expect(calculator.getItemOffset(1)).toBe(100);
      expect(calculator.getItemOffset(2)).toBe(150);
    });
  });

  describe('getTotalHeight', () => {
    it('should calculate total height', () => {
      calculator.setTotalItems(10);
      // 10 items * 50px estimated = 500px
      expect(calculator.getTotalHeight()).toBe(500);
    });

    it('should use actual heights when available', () => {
      calculator.setTotalItems(3);
      calculator.setItemHeight(0, 100);
      calculator.setItemHeight(1, 200);
      // 100 + 200 + 50 (estimated) = 350
      expect(calculator.getTotalHeight()).toBe(350);
    });
  });

  describe('calculate', () => {
    it('should return empty state for zero items', () => {
      calculator.setTotalItems(0);
      const state = calculator.calculate(0, 500);

      expect(state.startIndex).toBe(0);
      expect(state.endIndex).toBe(0);
      expect(state.visibleItems).toEqual([]);
    });

    it('should calculate visible range at top', () => {
      calculator.setTotalItems(100);
      const state = calculator.calculate(0, 300);

      expect(state.startIndex).toBe(0);
      // 300px / 50px = 6 items + overscan
      expect(state.endIndex).toBeGreaterThanOrEqual(6);
      expect(state.offsetTop).toBe(0);
    });

    it('should calculate visible range at middle', () => {
      calculator.setTotalItems(100);
      // 滚动到第 20 个项目 (20 * 50 = 1000px)
      const state = calculator.calculate(1000, 300);

      expect(state.startIndex).toBeLessThanOrEqual(20);
      expect(state.startIndex).toBeGreaterThanOrEqual(15); // with overscan (允许更大范围)
      expect(state.endIndex).toBeGreaterThan(20);
    });

    it('should calculate visible range at bottom', () => {
      calculator.setTotalItems(100);
      // 滚动到底部 (100 * 50 - 300 = 4700px)
      const state = calculator.calculate(4700, 300);

      expect(state.endIndex).toBe(100);
      expect(state.offsetBottom).toBe(0);
    });

    it('should apply overscan correctly', () => {
      calculator = new VirtualScrollCalculator({
        estimatedItemHeight: 50,
        overscan: 5,
      });
      calculator.setTotalItems(100);

      const state = calculator.calculate(500, 300);
      // 500px / 50px = 10, minus overscan = 5
      expect(state.startIndex).toBeLessThanOrEqual(5);
    });

    it('should calculate offsets correctly', () => {
      calculator.setTotalItems(100);
      const state = calculator.calculate(500, 300);

      expect(state.offsetTop).toBeGreaterThan(0);
      expect(state.offsetBottom).toBeGreaterThan(0);
      expect(state.offsetTop + state.offsetBottom).toBeLessThan(5000); // 总高度
    });

    it('should generate visibleItems array', () => {
      calculator.setTotalItems(100);
      const state = calculator.calculate(0, 300);

      expect(state.visibleItems.length).toBe(state.endIndex - state.startIndex);
      expect(state.visibleItems[0]).toBe(state.startIndex);
    });
  });

  describe('clear', () => {
    it('should reset all state', () => {
      calculator.setTotalItems(100);
      calculator.setItemHeight(0, 100);
      calculator.clear();

      expect(calculator.getItemHeight(0)).toBe(50); // back to estimated
      expect(calculator.calculate(0, 300).visibleItems).toEqual([]);
    });
  });
});

// ============ ScrollThrottler 测试 ============

describe('ScrollThrottler', () => {
  let throttler: ScrollThrottler;
  let callback: ReturnType<typeof mock>;

  beforeEach(() => {
    callback = mock();
    // Use short throttle time for real-time testing
    throttler = new ScrollThrottler(20, 5);
  });

  afterEach(() => {
    throttler.dispose();
  });

  describe('onScroll', () => {
    it('should execute callback immediately on first scroll', () => {
      throttler.onScroll(100, callback);

      expect(callback).toHaveBeenCalledWith(100);
    });

    it('should ignore small scroll deltas', () => {
      throttler.onScroll(100, callback);
      throttler.onScroll(102, callback); // delta = 2 < minDelta

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it('should throttle rapid scrolls', async () => {
      throttler.onScroll(100, callback);
      throttler.onScroll(200, callback);
      throttler.onScroll(300, callback);

      // Only first one executes immediately
      expect(callback).toHaveBeenCalledTimes(1);
      expect(callback).toHaveBeenCalledWith(100);

      // Wait for throttle period
      await sleep(30);

      // After throttle, last value should be executed
      expect(callback).toHaveBeenCalledTimes(2);
      expect(callback).toHaveBeenLastCalledWith(300);
    });

    it('should allow immediate execution after throttle period', async () => {
      throttler.onScroll(100, callback);

      // Wait for throttle period to pass
      await sleep(30);

      // Now should execute immediately again
      throttler.onScroll(200, callback);

      expect(callback).toHaveBeenCalledTimes(2);
      expect(callback).toHaveBeenLastCalledWith(200);
    });
  });

  describe('dispose', () => {
    it('should cancel pending callbacks', async () => {
      throttler.onScroll(100, callback);
      throttler.onScroll(200, callback);
      throttler.dispose();

      // Wait past throttle period
      await sleep(30);

      // Should still only have the first call
      expect(callback).toHaveBeenCalledTimes(1);
    });
  });
});

// ============ PerformanceMonitor 测试 ============

describe('PerformanceMonitor', () => {
  let monitor: PerformanceMonitor;

  beforeEach(() => {
    monitor = new PerformanceMonitor();
  });

  describe('recordBatch', () => {
    it('should track total operations', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
        ],
      });

      const metrics = monitor.getMetrics();
      expect(metrics.totalOperations).toBe(2);
    });

    it('should track batch count', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      });
      monitor.recordBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'CREATE', id: 2, type: 'Text', props: {} }],
      });

      const metrics = monitor.getMetrics();
      expect(metrics.totalBatches).toBe(2);
    });

    it('should calculate average batch size', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
        ],
      });
      monitor.recordBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'CREATE', id: 3, type: 'View', props: {} }],
      });

      const metrics = monitor.getMetrics();
      expect(metrics.avgBatchSize).toBe(1.5); // (2 + 1) / 2
    });

    it('should track merged operations', () => {
      monitor.recordBatch(
        {
          version: 1,
          batchId: 1,
          operations: [{ op: 'UPDATE', id: 1, props: { a: 1, b: 2 } }],
        },
        3 // original count before merge
      );

      const metrics = monitor.getMetrics();
      expect(metrics.mergedOperations).toBe(2); // 3 - 1
    });

    it('should count operation types', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'UPDATE', id: 1, props: { style: {} } },
          { op: 'DELETE', id: 2 },
        ],
      });

      const metrics = monitor.getMetrics();
      expect(metrics.createCount).toBe(2);
      expect(metrics.updateCount).toBe(1);
      expect(metrics.deleteCount).toBe(1);
    });

    it('should track last update time', () => {
      const before = Date.now();
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      });
      const after = Date.now();

      const metrics = monitor.getMetrics();
      expect(metrics.lastUpdateTime).toBeGreaterThanOrEqual(before);
      expect(metrics.lastUpdateTime).toBeLessThanOrEqual(after);
    });
  });

  describe('getMetrics', () => {
    it('should return a copy of metrics', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      });

      const metrics1 = monitor.getMetrics();
      const metrics2 = monitor.getMetrics();

      expect(metrics1).not.toBe(metrics2);
      expect(metrics1).toEqual(metrics2);
    });
  });

  describe('reset', () => {
    it('should reset all metrics', () => {
      monitor.recordBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'UPDATE', id: 1, props: {} },
          { op: 'DELETE', id: 1 },
        ],
      });

      monitor.reset();
      const metrics = monitor.getMetrics();

      expect(metrics.totalOperations).toBe(0);
      expect(metrics.totalBatches).toBe(0);
      expect(metrics.avgBatchSize).toBe(0);
      expect(metrics.mergedOperations).toBe(0);
      expect(metrics.createCount).toBe(0);
      expect(metrics.updateCount).toBe(0);
      expect(metrics.deleteCount).toBe(0);
      expect(metrics.lastUpdateTime).toBe(0);
    });
  });
});
